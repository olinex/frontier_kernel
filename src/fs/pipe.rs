// @author:    olinex
// @time:      2024/03/20

// self mods

// use other mods
use alloc::sync::{Arc, Weak};
use enum_group::EnumGroup;
use spin::mutex::Mutex;

// use self mods
use super::File;
use crate::lang::buffer::{ByteBuffers, RingBuffer};
use crate::prelude::*;
use crate::task::suspend_current_and_run_other_task;

/// A wrapper enumeration class for ringbuffer, only readable or writable.
/// For the same ringbuffer, it only makes sense to write data to it if it is read.
///
/// So the writable side of the pipe holds a strong reference to the ring'buffer.
/// This means that even if the writable side is turned off,
/// the data in the buffer that has not yet been read will still be read.
///
/// Conversely, if there are no readable ends,
/// the pipe will be automatically recycled,
/// and weak references on the writable side will no longer be able to be written.
#[derive(EnumGroup)]
pub(crate) enum Pipe {
    Read(Arc<Mutex<RingBuffer>>),
    Write(Weak<Mutex<RingBuffer>>),
}
impl Pipe {
    /// Create a new readable pipe
    pub(crate) fn new(capacity: usize) -> Self {
        Self::Read(Arc::new(Mutex::new(RingBuffer::new(capacity))))
    }

    /// Fork writable pipe, and if the current pipe is readable, it will inevitably return a writable copy of the pipe.
    /// If the current pipe is writable, None will be returned when all readable sides of the pipe have been closed.
    pub(crate) fn writable_fork(&self) -> Option<Self> {
        match self {
            Self::Read(tap) => Some(Self::Write(Arc::downgrade(tap))),
            Self::Write(tap) => tap
                .upgrade()
                .and_then(|upgrade| Some(Self::Write(Arc::downgrade(&upgrade)))),
        }
    }

    /// Fork readable pipe, and if the current pipe is readable, it will inevitably return a readable copy of the pipe.
    /// If the current pipe is writable, None will be returned when all readable sides of the pipe have been closed.
    #[allow(dead_code)]
    pub(crate) fn readable_fork(&self) -> Option<Self> {
        match self {
            Self::Write(tap) => tap.upgrade().and_then(|upgrade| Some(Self::Read(upgrade))),
            Self::Read(tap) => Some(Self::Read(Arc::clone(tap))),
        }
    }

    /// Just copy current pipe, no matter if the pipe is closed.
    #[allow(dead_code)]
    pub(crate) fn clone(&self) -> Self {
        match self {
            Self::Read(tap) => Self::Read(Arc::clone(tap)),
            Self::Write(tap) => Self::Write(Weak::clone(tap)),
        }
    }

    /// Check if the pipe is closed and cannot write anymore byte into it.
    pub(crate) fn all_write_end_closed(&self) -> bool {
        match self {
            Self::Write(_) => false,
            Self::Read(buffer) => Arc::weak_count(buffer) == 0,
        }
    }

    /// Check if the pipe is close and no any other readable tap.
    pub(crate) fn all_read_end_closed(&self) -> bool {
        match self {
            Self::Read(_) => false,
            Self::Write(buffer) => buffer.upgrade().is_none(),
        }
    }
}
impl File for Pipe {

    /// Read bytes from pipe and write them into buffers.
    /// Only readable pipe can call this method or it will panic.
    /// 
    /// Each time it tries to read some bytes from the pipe, 
    /// this method will try to acquire a read-write lock on the pipe, 
    /// and if the lock is held by another task, it will pause the current task. 
    /// 
    /// After successfully obtaining the read-write lock, 
    /// it will first check whether there are bytes in the pipe that have not yet been read, 
    /// if they exist, they will write all of them to the buffer as much as possible, 
    /// if they do not exist, they will check whether there are still writers on the side, 
    /// and if they do not, they will immediately end the current task.
    /// 
    /// See [`crate::fs::File`]
    /// 
    /// - Errors
    ///     - ProcessHaveNotTask
    ///     - EOB
    fn read(&self, buffers: ByteBuffers) -> Result<u64> {
        let tap = if let Self::Read(tap) = self {
            tap
        } else {
            panic!("reading write only pipe");
        };
        let to_read_size = buffers.len() as u64;
        if to_read_size == 0 {
            return Ok(0);
        }
        let mut iterator = buffers.into_iter();
        let mut already_readed_size = 0u64;
        while already_readed_size < to_read_size {
            if let Some(mut inner) = tap.try_lock() {
                let wait_read_size = to_read_size - already_readed_size;
                let wait_read_size: u64 = match inner.len() as u64 {
                    x if x < wait_read_size => x,
                    _ => wait_read_size,
                };
                if wait_read_size == 0 {
                    if self.all_write_end_closed() {
                        return Ok(already_readed_size);
                    }
                    drop(inner);
                    suspend_current_and_run_other_task()?;
                    continue;
                }
                for _ in 0..wait_read_size {
                    if let Some(byte) = inner.read_byte() {
                        iterator.next_mut(byte)?;
                        already_readed_size += 1;
                    } else {
                        panic!("cannot read byte from ring buffer")
                    }
                }
            };
            suspend_current_and_run_other_task()?;
        }
        Ok(already_readed_size)
    }

    /// Read bytes from buffers and write them into pipe.
    /// Only writable pipe can call this method or it will panic.
    /// 
    /// Each time it tries to write some bytes to the pipe, 
    /// this method will try to acquire a read-write lock on the pipe, 
    /// and if the lock is held by another task, it will pause the current task. 
    /// 
    /// After successfully obtaining the read-write lock, 
    /// it will first check whether there are bytes in the pipe that have not yet been write, 
    /// if they exist, they will write all of them to the pipe as much as possible, 
    /// if they do not exist, they will check whether there are still readers on the side, 
    /// and if they do not, they will immediately end the current task.
    /// 
    /// See [`crate::fs::File`]
    /// 
    /// - Errors
    ///     - ProcessHaveNotTask
    ///     - EOB
    fn write(&self, buffers: ByteBuffers) -> Result<u64> {
        let tap = if let Self::Write(tap) = self {
            tap
        } else {
            panic!("writing read only pipe");
        };
        let to_write_size = buffers.len() as u64;
        if to_write_size == 0 {
            return Ok(0);
        }
        let mut iterator = buffers.into_iter();
        let mut already_written_size = 0u64;
        while let Some(tap) = tap.upgrade() {
            if already_written_size >= to_write_size {
                break;
            }
            if let Some(mut inner) = tap.try_lock() {
                let wait_write_size = to_write_size - already_written_size;
                let wait_wirte_size = match (inner.capacity() - inner.len()) as u64 {
                    x if x < wait_write_size => x,
                    _ => wait_write_size,
                };
                if wait_wirte_size == 0 {
                    if self.all_read_end_closed() {
                        return Ok(already_written_size);
                    }
                    drop(inner);
                    suspend_current_and_run_other_task()?;
                    continue;
                }
                for _ in 0..wait_wirte_size {
                    if let Some(byte) = iterator.next() {
                        inner.write_byte(byte)?;
                        already_written_size += 1;
                    } else {
                        panic!("no more byte from byte buffers")
                    }
                }
            };
            suspend_current_and_run_other_task()?;
        }
        Ok(already_written_size)
    }
}

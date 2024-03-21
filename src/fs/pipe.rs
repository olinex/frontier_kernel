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

#[derive(EnumGroup)]
pub(crate) enum Pipe {
    Read(Arc<Mutex<RingBuffer>>),
    Write(Weak<Mutex<RingBuffer>>),
}
impl Pipe {
    pub(crate) fn new(capacity: usize) -> Self {
        Self::Read(Arc::new(Mutex::new(RingBuffer::new(capacity))))
    }

    pub(crate) fn drainage(&self) -> Option<Self> {
        match self {
            Self::Read(tap) => Some(Self::Write(Arc::downgrade(tap))),
            Self::Write(tap) => tap
                .upgrade()
                .and_then(|upgrade| Some(Self::Write(Arc::downgrade(&upgrade)))),
        }
    }

    pub(crate) fn clone(&self) -> Option<Self> {
        match self {
            Self::Read(tap) => Some(Self::Read(Arc::clone(tap))),
            Self::Write(tap) => tap
                .upgrade()
                .and_then(|upgrade| Some(Self::Write(Arc::downgrade(&upgrade)))),
        }
    }

    #[inline(always)]
    pub(crate) fn readable(&self) -> bool {
        self.is_read()
    }

    #[inline(always)]
    pub(crate) fn writable(&self) -> bool {
        self.is_write()
    }

    pub(crate) fn all_write_end_closed(&self) -> bool {
        match self {
            Self::Write(_) => false,
            Self::Read(buffer) => Arc::weak_count(buffer) == 0,
        }
    }

    pub(crate) fn all_read_end_closed(&self) -> bool {
        match self {
            Self::Read(_) => false,
            Self::Write(buffer) => buffer.upgrade().is_none(),
        }
    }
}
impl File for Pipe {
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

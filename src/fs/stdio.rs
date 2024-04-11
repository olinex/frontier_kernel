// @author:    olinex
// @time:      2024/01/14

// self mods

// use other mods
use alloc::sync::Arc;
use spin::Mutex;
use frontier_lib::constant::charater;

// use self mods
use super::File;
use crate::lang::buffer::ByteBuffers;
use crate::prelude::*;
use crate::sbi::{SBIApi, SBI};
use crate::task::suspend_current_and_run_other_task;

/// The standard input queue of the kernel system.
struct Stdin {
    inner: Mutex<()>,
}
impl File for Stdin {
    /// Read some bytes from console and write them into buffers.
    /// 
    /// This method will keep trying to obtain the read-write lock of stdin, 
    /// and when the lock is obtained, the method will not release the lock until the specified number of bytes are read; 
    /// When bytes can no longer be retrieved from the underlying driver layer of the console, 
    /// the current task will be paused and other tasks will be executed, 
    /// and the lock will not be released. Unless the byte read is NULL.
    /// 
    /// See [`crate::fs::File`]
    /// 
    /// - Errors
    ///     - ProcessHaveNotTask
    ///     - EOB
    fn read(&self, buffers: ByteBuffers) -> Result<u64> {
        loop {
            if let Some(lock) = self.inner.try_lock() {
                let length = buffers.len() as u64;
                let mut count: u64 = 0;
                let mut iterator = buffers.into_iter();
                while count < length {
                    if let Some(c) = SBI::console_getchar() {
                        if c == charater::NULL as u8 {
                            drop(lock);
                            return Ok(count);
                        }
                        iterator.next_mut(c)?;
                        count += 1;
                    } else {
                        suspend_current_and_run_other_task()?;
                    }
                }
                drop(lock);
                return Ok(count);
            } else {
                suspend_current_and_run_other_task()?;
                continue;
            }
        }
    }

    /// stdin is not writable
    fn write(&self, _: ByteBuffers) -> Result<u64> {
        panic!("Cannot write to stdin!");
    }
}

/// The standard ouput queue of the kernel system.
struct Stdout {
    inner: Mutex<()>,
}
impl File for Stdout {
    /// stdout is not readable
    fn read(&self, _: ByteBuffers) -> Result<u64> {
        panic!("Cannot read from stdout!")
    }

    /// Read some bytes from buffers and write them into console.
    /// 
    /// This method will keep trying to obtain the read-write lock of stdout, 
    /// and when the lock is obtained, the method will not release the lock until the specified number of bytes are writed; 
    /// 
    /// See [`crate::fs::File`]
    /// 
    /// - Errors
    ///     - ProcessHaveNotTask
    ///     - EOB
    fn write(&self, buffers: ByteBuffers) -> Result<u64> {
        loop {
            if let Some(lock) = self.inner.try_lock() {
                let length = buffers.len() as u64;
                let string = buffers.into_utf8_str()?;
                print!("{}", string);
                drop(lock);
                return Ok(length);
            } else {
                suspend_current_and_run_other_task()?;
                continue;
            }
        }
    }
}

lazy_static! {
    /// Singleton standard input queue
    pub(crate) static ref STDIN: Arc<dyn File> = Arc::new(Stdin {
        inner: Mutex::new(()),
    });
    // Singleton standard output queue
    pub(crate) static ref STDOUT: Arc<dyn File> = Arc::new(Stdout {
        inner: Mutex::new(()),
    });
}

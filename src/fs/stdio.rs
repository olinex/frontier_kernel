// @author:    olinex
// @time:      2024/01/14

// self mods

// use other mods
use alloc::sync::Arc;
use spin::Mutex;

// use self mods
use super::File;
use crate::constant::ascii;
use crate::lang::buffer::ByteBuffers;
use crate::prelude::*;
use crate::sbi::{SBIApi, SBI};
use crate::task::suspend_current_and_run_other_task;

/// The standard input queue of the kernel system.
struct Stdin {
    inner: Mutex<()>,
}
impl File for Stdin {
    fn read(&self, buffers: ByteBuffers) -> Result<u64> {
        let lock = self.inner.lock();
        let length = buffers.len() as u64;
        let mut count: u64 = 0;
        let mut iterator = buffers.into_iter();
        while count < length {
            if let Some(c) = SBI::console_getchar() {
                if c == ascii::NULL {
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
        Ok(count)
    }

    fn write(&self, _: ByteBuffers) -> Result<u64> {
        panic!("Cannot write to stdin!");
    }
}

/// The standard ouput queue of the kernel system.
struct Stdout {
    inner: Mutex<()>,
}
impl File for Stdout {
    fn read(&self, _: ByteBuffers) -> Result<u64> {
        panic!("Cannot read from stdout!")
    }

    fn write(&self, buffers: ByteBuffers) -> Result<u64> {
        let lock = self.inner.lock();
        let length = buffers.len() as u64;
        let string = buffers.into_utf8_str();
        print!("{}", string);
        drop(lock);
        Ok(length)
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

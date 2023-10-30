// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods

// use self mods
use crate::constant::*;
use crate::prelude::*;
use crate::sbi::*;
use crate::task::*;

pub mod descriptor {
    pub const STDIN: usize = 0;
    pub const STDOUT: usize = 1;
}

/// Write a &str to the IO device.
/// Different IO devices are distinguished by different unique description ID numbers
///
/// # Arguments
/// * fd: the discriptor for different IO devices
/// * buffer: the pointer to the buffer to write
/// * len: the length of the buffer
///
/// # Returns
/// * Ok(writed length)
/// * Err(KernelError::InvalidFileDescriptor(fd))
pub fn sys_write(fd: usize, buffer: *const u8, len: usize) -> Result<isize> {
    match fd {
        descriptor::STDOUT => {
            let buffers = {
                let task = PROCESSOR.current_task()?;
                let inner = task.inner_access();
                let current_space = inner.space();
                current_space.translated_byte_buffers(buffer, len)?
            };
            for buffer in buffers {
                let str = core::str::from_utf8(buffer).unwrap();
                print!("{}", str);
            }
            Ok(len as isize)
        }
        _ => Err(KernelError::InvalidFileDescriptor(fd)),
    }
}

/// Read a &str from the IO device and save it to the buffer.
/// Different IO devices are distinguished by different unique description ID numbers
///
/// # Arguments
/// * fd: the discriptor for different IO devices
/// * buffer: the pointer to the buffer to write
/// * len: the length of the buffer
///
/// # Returns
/// * Ok(writed length)
/// * Err(KernelError::InvalidFileDescriptor(fd))
pub fn sys_read(fd: usize, buffer: *mut u8, len: usize) -> Result<isize> {
    assert!(len > 0);
    match fd {
        descriptor::STDIN => {
            let buffers = {
                let task = PROCESSOR.current_task()?;
                let inner = task.inner_access();
                let current_space = inner.space();
                current_space.translated_byte_buffers(buffer, len)?
            };
            let mut count = 0;
            'outer: for buffer in buffers {
                let mut offset = 0;
                while offset < buffer.len() {
                    if let Some(c) = SBI::console_getchar() {
                        if c == ascii::NULL {
                            break 'outer;
                        }
                        buffer[offset] = c;
                        count += 1;
                        offset += 1;
                    } else {
                        suspend_current_and_run_other_task()?;
                    }
                }
            }
            Ok(count as isize)
        }
        _ => Err(KernelError::InvalidFileDescriptor(fd)),
    }
}

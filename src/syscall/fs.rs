// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods

// use self mods
use crate::prelude::*;
use crate::task::TASK_CONTROLLER;

pub mod descriptor {
    pub const STDOUT: usize = 1;
}

// write buf of length `len`  to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> Result<isize> {
    match fd {
        descriptor::STDOUT => {
            let controller = TASK_CONTROLLER.access();
            let current_space = controller.current_space()?;
            let buffers = current_space.translated_byte_buffers(buf, len)?;
            for buffer in buffers {
                let str = core::str::from_utf8(buffer).unwrap();
                print!("{}", str);
            }
            Ok(len as isize)
        }
        _ => Err(KernelError::InvalidFileDescriptor(fd)),
    }
}

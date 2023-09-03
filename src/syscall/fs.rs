// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods

// use self mods
use crate::prelude::*;

pub mod descriptor {
    pub const STDOUT: usize = 1;
}

// write buf of length `len`  to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> Result<isize> {
    match fd {
        descriptor::STDOUT => {
            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            let str = core::str::from_utf8(slice).unwrap();
            print!("{}", str);
            Ok(len as isize)
        }
        _ => Err(KernelError::InvalidFileDescriptor(fd)),
    }
}

// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods

// use self mods
use crate::print;

pub mod file_descriptor {
    pub const STDOUT: usize = 1;
}

// write buf of length `len`  to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    match fd {
        file_descriptor::STDOUT => {
            let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            let str = core::str::from_utf8(slice).unwrap();
            print!("{}", str);
            len as isize
        }
        _ => {
            panic!("Unsupported fd in sys_write!");
        }
    }
}

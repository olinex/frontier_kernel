// @author:    olinex
// @time:      2023/09/03

// self mods
mod fs;
mod process;
mod task;
mod time;

// use other mods

// use self mods
use crate::prelude::*;

mod ids {
    pub const WRITE: usize = 64;
    pub const EXIT: usize = 93;
    pub const YIELD: usize = 124;
    pub const GET_TIME: usize = 169;
}

// handle syscall exception with `syscall_id` and other arguments
#[inline(always)]
pub fn syscall(syscall_id: usize, arg1: usize, arg2: usize, arg3: usize) -> Result<isize> {
    match syscall_id {
        ids::WRITE => fs::sys_write(arg1, arg2 as *const u8, arg3),
        ids::EXIT => process::sys_exit(arg1 as i32),
        ids::YIELD => task::sys_yield(),
        ids::GET_TIME => time::sys_get_time(),
        _ => Err(KernelError::InvaidSyscallId(syscall_id)),
    }
}

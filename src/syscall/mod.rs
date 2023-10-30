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
    pub const READ: usize = 63;
    pub const WRITE: usize = 64;
    pub const EXIT: usize = 93;
    pub const YIELD: usize = 124;
    pub const GET_TIME: usize = 169;
    pub const GET_PID: usize = 172;
    pub const FORK: usize = 220;
    pub const EXEC: usize = 221;
    pub const WAIT_PID: usize = 260;
}

// handle syscall exception with `syscall_id` and other arguments
#[inline(always)]
pub fn syscall(syscall_id: usize, arg1: usize, arg2: usize, arg3: usize) -> Result<isize> {
    match syscall_id {
        ids::READ => fs::sys_read(arg1, arg2 as *mut u8, arg3),
        ids::WRITE => fs::sys_write(arg1, arg2 as *const u8, arg3),
        ids::EXIT => process::sys_exit(arg1 as i32),
        ids::YIELD => task::sys_yield(),
        ids::GET_TIME => time::sys_get_time(),
        ids::GET_PID => process::sys_get_pid(),
        ids::FORK => process::sys_fork(),
        ids::EXEC => process::sys_exec(arg1 as *const u8),
        ids::WAIT_PID => process::sys_wait_pid(arg1 as isize, arg2 as *mut i32),
        _ => Err(KernelError::InvaidSyscallId(syscall_id)),
    }
}

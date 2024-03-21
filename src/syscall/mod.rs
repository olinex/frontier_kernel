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
    pub(crate) const OPEN: usize = 56;
    pub(crate) const CLOSE: usize = 57;
    pub(crate) const PIPE: usize = 59;
    pub(crate) const READ: usize = 63;
    pub(crate) const WRITE: usize = 64;
    pub(crate) const EXIT: usize = 93;
    pub(crate) const YIELD: usize = 124;
    pub(crate) const GET_TIME: usize = 169;
    pub(crate) const GET_PID: usize = 172;
    pub(crate) const FORK: usize = 220;
    pub(crate) const EXEC: usize = 221;
    pub(crate) const WAIT_PID: usize = 260;
}

// handle syscall exception with `syscall_id` and other arguments
#[inline(always)]
pub(crate) fn syscall(syscall_id: usize, arg1: usize, arg2: usize, arg3: usize) -> Result<isize> {
    match syscall_id {
        ids::OPEN => fs::sys_open(arg1 as *const u8, arg2 as u32),
        ids::CLOSE => fs::sys_close(arg1),
        ids::PIPE => fs::sys_pipe(arg1 as *const [usize; 2]),
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

//! Implementation of syscalls
//!
//! The single entry point to all system calls, [`syscall()`], is called
//! whenever userspace wishes to perform a system call using the `ecall`
//! instruction. In this case, the processor raises an 'Environment call from
//! U-mode' exception, which is handled as one of the cases in
//! [`crate::trap::trap_handler`].
//!
//! For clarity, each single syscall is implemented as its own function, named
//! `sys_` then the name of the syscall. You can find functions like this in
//! submodules, and you should also implement syscalls this way.

mod fs;
mod process;
mod task;

mod ids {
    pub const WRITE: usize = 64;
    pub const EXIT: usize = 93;
    pub const YIELD: usize = 124;
}

// handle syscall exception with `syscall_id` and other arguments
#[inline(always)]
pub fn syscall(syscall_id: usize, arg1: usize, arg2: usize, arg3: usize) -> isize {
    match syscall_id {
        ids::WRITE => fs::sys_write(arg1, arg2 as *const u8, arg3),
        ids::EXIT => process::sys_exit(arg1 as i32),
        ids::YIELD => task::sys_yield(),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}

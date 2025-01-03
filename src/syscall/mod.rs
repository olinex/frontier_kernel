// @author:    olinex
// @time:      2023/09/03

// self mods
mod fs;
mod process;
mod signal;
mod sync;
mod task;
mod time;

// use other mods
use frontier_lib::{constant::sysid, model::signal::SignalAction};

// use self mods
use crate::prelude::*;

// handle syscall exception with `syscall_id` and other arguments
#[inline(always)]
pub(crate) fn syscall(syscall_id: usize, arg1: usize, arg2: usize, arg3: usize) -> Result<isize> {
    match syscall_id {
        sysid::DUP => fs::sys_dup(arg1 as usize),
        sysid::OPEN => fs::sys_open(arg1 as *const u8, arg2 as u32),
        sysid::CLOSE => fs::sys_close(arg1),
        sysid::PIPE => fs::sys_pipe(arg1 as *mut usize, arg2 as *mut usize),
        sysid::READ => fs::sys_read(arg1, arg2 as *mut u8, arg3),
        sysid::WRITE => fs::sys_write(arg1, arg2 as *const u8, arg3),
        sysid::EXIT => process::sys_exit(arg1 as i32),
        sysid::SLEEP => time::sys_sleep(arg1 as usize),
        sysid::YIELD => task::sys_yield(),
        sysid::KILL => signal::sys_kill(arg1 as isize, arg2 as usize),
        sysid::SIG_ACTION => signal::sys_sig_action(
            arg1 as usize,
            arg2 as *const SignalAction,
            arg3 as *mut SignalAction,
        ),
        sysid::SIG_PROC_MASK => signal::sys_sig_proc_mask(arg1 as u32),
        sysid::SIG_RETURN => signal::sys_sig_return(),
        sysid::GET_TIME => time::sys_get_time(),
        sysid::GET_PID => process::sys_get_pid(),
        sysid::FORK => process::sys_fork(),
        sysid::EXEC => process::sys_exec(arg1 as *const u8, arg2 as *const u8),
        sysid::WAIT_PID => process::sys_wait_pid(arg1 as isize, arg2 as *mut i32),
        sysid::THREAD_CREATE => task::sys_thread_create(arg1, arg2),
        sysid::GET_TID => task::sys_get_tid(),
        sysid::WAIT_TID => task::sys_wait_tid(arg1 as isize, arg2 as *mut i32),
        sysid::MUTEX_CREATE => sync::sys_create_mutex(arg1 != 0),
        sysid::MUTEX_LOCK => sync::sys_lock_mutex(arg1 as usize),
        sysid::MUTEX_UNLOCK => sync::sys_unlock_mutex(arg1 as usize),
        sysid::SEMAPHORE_CREATE => sync::sys_create_semaphore(arg1 != 0, arg2 as isize),
        sysid::SEMAPHORE_UP => sync::sys_up_semaphore(arg1 as usize),
        sysid::SEMAPHORE_DOWN => sync::sys_down_semaphore(arg1 as usize),
        sysid::CONDVAR_CREATE => sync::sys_create_condvar(),
        sysid::CONDVAR_SIGNAL => sync::sys_signal_condvar(arg1 as usize),
        sysid::CONDVAR_WAIT => sync::sys_wait_condvar(arg1 as usize, arg2 as usize),
        _ => Err(KernelError::InvaidSyscallId(syscall_id)),
    }
}

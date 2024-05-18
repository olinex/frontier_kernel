// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods

// use self mods
use crate::prelude::*;
use crate::task::{suspend_current_and_run_other_task, PROCESSOR, TASK_SCHEDULER};

/// Yield to other task, current task will be suspended
///
/// - Errors
///     - ProcessHaveNotTask
#[inline(always)]
pub(crate) fn sys_yield() -> Result<isize> {
    suspend_current_and_run_other_task()?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_thread_create(entry_point: usize, arg: usize) -> Result<isize> {
    let current_task = PROCESSOR.current_task()?;
    let new_task = current_task.fork_task(entry_point, arg)?;
    let tid = new_task.tid();
    TASK_SCHEDULER.add_task(new_task);
    debug!("create a new thread {} with entry point: {}", tid, entry_point);
    Ok(tid as isize)
}

#[inline(always)]
pub(crate) fn sys_get_tid() -> Result<isize> {
    Ok(PROCESSOR.current_task()?.tid() as isize)
}

#[inline(always)]
pub(crate) fn sys_wait_tid(tid: isize, exit_code_ptr: *mut i32) -> Result<isize> {
    let current_task = PROCESSOR.current_task()?;
    current_task.wait_tid(tid, exit_code_ptr)
}

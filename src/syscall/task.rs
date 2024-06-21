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

/// Create a new thread in the current task's process
/// 
/// - Arguments
///     - entry_point: the virtual address of entry point in user space 
///     - arg: argument pass from user mode which will be store in a10 register
/// 
/// - Errors
///     - ProcessHaveNotTask
///     - ForkWithNoRootTask(tid)
///     - AreaAllocFailed(start_vpn, end_vpn)
///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
///     - VPNAlreadyMapped(vpn)
///     - InvaidPageTablePerm(flags)
///     - FrameExhausted
///     - AllocFullPageMapper(ppn)
///     - PPNAlreadyMapped(ppn)
///     - PPNNotMapped(ppn)
#[inline(always)]
pub(crate) fn sys_thread_create(entry_point: usize, arg: usize) -> Result<isize> {
    let current_task = PROCESSOR.current_task()?;
    current_task.forkable()?;
    let process = current_task.process();
    let new_task = process.alloc_task(entry_point, Some(arg))?;
    let tid = new_task.tid();
    TASK_SCHEDULER.put_read_task(new_task);
    debug!(
        "create a new thread {} with entry point: {}",
        tid, entry_point
    );
    Ok(tid as isize)
}

/// Get current task's unique id
/// 
/// - Errors
///     - ProcessHaveNotTask
#[inline(always)]
pub(crate) fn sys_get_tid() -> Result<isize> {
    Ok(PROCESSOR.current_task()?.tid() as isize)
}

/// Wait child task becomes a zombie task, reclaim trap context and user stack, and collect its return value
/// 
/// - Arguments
///     - tid: the id of the task which we are waiting for
///     - exit_code_ptr: The pointer address that represents the return value of the child task,
///         the child task needs to write the return value by itself.
///         If this address is 0, it means that it does not need to be saved
/// 
/// - Returns
///     - -1: child task does not exist
///     - -2: child task is still alive
/// 
/// - Errors
///     - ProcessHaveNotTask
#[inline(always)]
pub(crate) fn sys_wait_tid(tid: isize, exit_code_ptr: *mut i32) -> Result<isize> {
    let current_task = PROCESSOR.current_task()?;
    current_task.wait_tid(tid, exit_code_ptr)
}

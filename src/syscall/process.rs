// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods

use frontier_fs::OpenFlags;

// use self mods
use crate::fs::inode::ROOT_INODE;
use crate::prelude::*;
use crate::task::{exit_current_and_run_other_task, PROCESSOR, TASK_CONTROLLER};

/// Task exits and submit an exit code
///
/// # Arguments
/// * exit_code
#[inline(always)]
pub(crate) fn sys_exit(exit_code: i32) -> ! {
    debug!("Application exited with code {}", exit_code);
    exit_current_and_run_other_task(exit_code).unwrap();
    unreachable!();
}

/// Get the current task's process unique id
#[inline(always)]
pub(crate) fn sys_get_pid() -> Result<isize> {
    let current_task = PROCESSOR.current_task()?;
    Ok(current_task.pid() as isize)
}

/// Fork a new children process from the parent process,
/// the children process will also at the very moment after have called the fork;
/// So it seems very likely the children process and the parent process are both fork a new process
/// but we can use the return value to distinguish then
///
/// If the return value is 0, it means that the process is the new process;
/// If the return value is other than 0, it means that the process is the parent process and the return value is the pid of the new process
///
/// # Returns
/// * Ok(0 or new process pid)
#[inline(always)]
pub(crate) fn sys_fork() -> Result<isize> {
    let current_task = PROCESSOR.current_task()?;
    let new_task = current_task.fork()?;
    let new_inner = new_task.inner_access();
    let trap_ctx = new_inner.trap_ctx()?;
    // for child process, fork returns 0
    trap_ctx.set_arg(0, 0);
    drop(new_inner);
    let pid = new_task.pid();
    TASK_CONTROLLER.add_task(new_task);
    debug!(
        "Task {} pid: {} created successfully by fork()",
        current_task.name(),
        pid
    );
    Ok(pid as isize)
}

/// Empty the memory page table of the current process and perform the task specified by the path parameter.
/// In some cases, we should not consider them errors,
/// but instead we should return an error code for the caller to decide how to proceed with it.
///
/// If the task isn't exists, it will return error code(-1)
/// or this function will not return back.
///
/// # Arguments
/// * path: the path of the task which should be run in the current process
///
/// # Returns
/// * Ok(0): task execute success
/// * Ok(-1): task does not exist
#[inline(always)]
pub(crate) fn sys_exec(path: *const u8) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let inner = task.inner_access();
    let current_space = inner.space();
    let path = current_space.translated_string(path)?;
    let file = ROOT_INODE.find(&path, OpenFlags::READ)?;
    let data = file.read_all()?;
    debug!(
        "Task {}({} bytes) was loaded successfully",
        path,
        data.len()
    );
    drop(file);
    drop(path);
    drop(inner);
    task.exec(&data)?;
    Ok(0)
}

/// Wait children process becomes a zombie process, reclaim all its resources, and collect its return value
///
/// # Arguments
/// * pid: the id of the process which we are waiting for
/// * exit_code_ptr: The pointer address that represents the return value of the child process,
///   the child process needs to write the return value by itself.
///   If this address is 0, it means that it does not need to be saved
///
/// # Returns
/// * Ok(-1): task does not exist
/// * Ok(-2): task is still alive
#[inline(always)]
pub(crate) fn sys_wait_pid(pid: isize, exit_code_ptr: *mut i32) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    task.wait(pid, exit_code_ptr)
}

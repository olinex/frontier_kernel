// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods

use frontier_fs::OpenFlags;

// use self mods
use crate::fs::inode::ROOT_INODE;
use crate::prelude::*;
use crate::task::{exit_current_and_run_other_task, PROCESSOR, TASK_SCHEDULER};

/// Task exits and submit an exit code
///
/// - Arguments
///     - exit_code
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
    Ok(current_task.process().pid() as isize)
}

/// Fork a new children process from the parent process,
/// the children process will also at the very moment after have called the fork;
/// So it seems very likely the children process and the parent process are both fork a new process
/// but we can use the return value to distinguish then
///
/// If the return value is 0, it means that the process is the new process;
/// If the return value is other than 0, it means that the process is the parent process and the return value is the pid of the new process
///
/// - Errors
///     - ProcessHaveNotTask
#[inline(always)]
pub(crate) fn sys_fork() -> Result<isize> {
    let current_task = PROCESSOR.current_task()?;
    let new_process = current_task.fork_process()?;
    let new_process_inner = new_process.inner_access();
    let new_root_task = new_process_inner.root_task();
    let new_task_inner = new_root_task.inner_access();
    new_task_inner.modify_trap_ctx(new_process_inner.space(), |trap_ctx| {
        // for child process, fork returns 0
        trap_ctx.set_arg(0, 0);
        Ok(())
    })?;
    drop(new_task_inner);
    drop(new_process_inner);
    let pid = new_process.pid();
    TASK_SCHEDULER.add_task(new_root_task);
    debug!("Process: {} created successfully by fork()", pid);
    Ok(pid as isize)
}

/// Empty the memory page table of the current process and perform the task specified by the path parameter.
/// In some cases, we should not consider them errors,
/// but instead we should return an error code for the caller to decide how to proceed with it.
///
/// If the task isn't exists, it will return error code(-1)
/// or this function will always return the count of return back string pointer in user stack(2)
///
/// - Arguments
///     - path_ptr: The pointer address that path of the task which should be run in the current process
///     - args_ptr: The pointer address that string of the command line arguments
///
/// - Errors
///     - ProcessHaveNotTask
///     - VPNNotMapped(vpn)
///     - FileSystemError
///         - InodeMustBeDirectory(bitmap index)
///         - DataOutOfBounds
///         - NoDroptableBlockCache
///         - RawDeviceError(error code)
///         - DuplicatedFname(name, inode bitmap index)
///         - BitmapExhausted(start_block_id)
///         - BitmapIndexDeallocated(bitmap_index)
///         - RawDeviceError(error code)
///     - FileMustBeReadable(bitmap index)
///     - FileDoesNotExists(name)
#[inline(always)]
pub(crate) fn sys_exec(path_ptr: *const u8, args_ptr: *const u8) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let process_inner = process.inner_access();
    let current_space = process_inner.space();
    let path = current_space.translated_string(path_ptr)?;
    if path.is_empty() {
        return Err(KernelError::FileDoesNotExists(path));
    }
    let args = current_space.translated_string(args_ptr)?;
    let file = ROOT_INODE.find(&path, OpenFlags::READ)?;
    let data = file.read_all()?;
    debug!(
        "Task {}({} bytes) was loaded successfully",
        path,
        data.len()
    );
    drop(file);
    drop(process_inner);
    drop(process);
    Ok(task.exec(path, &data, args)? as isize)
}

/// Wait children process becomes a zombie process, reclaim all its resources, and collect its return value
///
/// - Arguments
///     - pid: the id of the process which we are waiting for
///     - exit_code_ptr: The pointer address that represents the return value of the child process,
///         the child process needs to write the return value by itself.
///         If this address is 0, it means that it does not need to be saved
///
/// - Returns
///     - -1: task does not exist
///     - -2: task is still alive
///
/// - Errors
///     - ProcessHaveNotTask
#[inline(always)]
pub(crate) fn sys_wait_pid(pid: isize, exit_code_ptr: *mut i32) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    process.wait_pid(pid, exit_code_ptr)
}

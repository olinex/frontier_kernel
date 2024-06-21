// @author:    olinex
// @time:      2024/06/05

// self mods

// use other mods

use alloc::sync::Arc;

// use self mods
use crate::prelude::*;
use crate::task::PROCESSOR;

#[inline(always)]
pub(crate) fn sys_create_mutex(blocking: bool) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut inner = process.inner_exclusive_access();
    Ok(inner.alloc_mutex(blocking)? as isize)
}

#[allow(dead_code)]
#[inline(always)]
pub(crate) fn sys_release_mutex(id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut inner = process.inner_exclusive_access();
    inner.dealloc_mutex(id)?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_lock_mutex(id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let inner = process.inner_access();
    let mutex = Arc::clone(
        inner
            .get_mutex(id)
            .ok_or(KernelError::MutexDoesNotExist(id))?,
    );
    drop(inner);
    drop(process);
    drop(task);
    mutex.lock()?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_unlock_mutex(id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let inner = process.inner_access();
    let mutex = Arc::clone(
        inner
            .get_mutex(id)
            .ok_or(KernelError::MutexDoesNotExist(id))?,
    );
    drop(inner);
    drop(process);
    drop(task);
    mutex.unlock()?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_create_semaphore(blocking: bool, count: isize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut inner = process.inner_exclusive_access();
    Ok(inner.alloc_semaphore(blocking, count)? as isize)
}

#[inline(always)]
#[allow(dead_code)]
pub(crate) fn sys_release_semaphore(id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut inner = process.inner_exclusive_access();
    inner.dealloc_semaphore(id)?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_up_semaphore(id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let inner = process.inner_access();
    let sp = Arc::clone(
        inner
            .get_semaphore(id)
            .ok_or(KernelError::SemaphoreDoesNotExist(id))?,
    );
    drop(inner);
    drop(process);
    drop(task);
    sp.up()?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_down_semaphore(id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let inner = process.inner_access();
    let sp = Arc::clone(
        inner
            .get_semaphore(id)
            .ok_or(KernelError::SemaphoreDoesNotExist(id))?,
    );
    drop(inner);
    drop(process);
    drop(task);
    sp.down()?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_create_condvar() -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut inner = process.inner_exclusive_access();
    Ok(inner.alloc_condvar()? as isize)
}

#[inline(always)]
#[allow(dead_code)]
pub(crate) fn sys_release_condvar(id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut inner = process.inner_exclusive_access();
    inner.dealloc_condvar(id)?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_signal_condvar(id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let inner = process.inner_access();
    let condvar = Arc::clone(inner
        .get_condvar(id)
        .ok_or(KernelError::CondvarDoesNotExist(id))?);
    drop(inner);
    drop(process);
    drop(task);
    condvar.signal()?;
    Ok(0)
}

#[inline(always)]
pub(crate) fn sys_wait_condvar(id: usize, mutex_id: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let inner = process.inner_access();
    let mutex = Arc::clone(inner
        .get_mutex(mutex_id)
        .ok_or(KernelError::MutexDoesNotExist(mutex_id))?);
    let condvar = Arc::clone(inner
        .get_condvar(id)
        .ok_or(KernelError::CondvarDoesNotExist(id))?);
    drop(inner);
    drop(process);
    drop(task);
    condvar.wait(mutex)?;
    Ok(0)
}

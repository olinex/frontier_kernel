// @author:    olinex
// @time:      2024/06/11

// self mods

// use other mods
use alloc::sync::Arc;
use alloc::{collections::VecDeque, sync::Weak};

// use self mods
use crate::lang::container::UserPromiseRefCell;
use crate::prelude::*;
use crate::task::model::TaskControlBlock;
use crate::task::{
    block_current_and_run_other_task, suspend_current_and_run_other_task, PROCESSOR, TASK_SCHEDULER,
};

pub(crate) trait Semaphore: Sync + Send {
    fn up(&self) -> Result<isize>;
    fn down(&self) -> Result<isize>;
}

struct SemaphoreSpinInner {
    count: isize,
}

pub(crate) struct SemaphoreSpin {
    inner: UserPromiseRefCell<SemaphoreSpinInner>,
}
impl SemaphoreSpin {
    pub(crate) fn new(count: isize) -> Self {
        Self {
            inner: unsafe { UserPromiseRefCell::new(SemaphoreSpinInner { count }) },
        }
    }
}
impl Semaphore for SemaphoreSpin {
    fn down(&self) -> Result<isize> {
        loop {
            let mut inner = self.inner.exclusive_access();
            if inner.count <= 0 {
                drop(inner);
                suspend_current_and_run_other_task()?;
                continue;
            }
            inner.count -= 1;
            return Ok(inner.count);
        }
    }

    fn up(&self) -> Result<isize> {
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        return Ok(inner.count);
    }
}

struct SemaphoreBlockingInner {
    count: isize,
    waiting: VecDeque<Weak<TaskControlBlock>>,
}

pub(crate) struct SemaphoreBlocking {
    inner: UserPromiseRefCell<SemaphoreBlockingInner>,
}
impl SemaphoreBlocking {
    pub(crate) fn new(count: isize) -> Self {
        Self {
            inner: unsafe {
                UserPromiseRefCell::new(SemaphoreBlockingInner {
                    count,
                    waiting: VecDeque::new(),
                })
            },
        }
    }
}
impl Semaphore for SemaphoreBlocking {
    fn up(&self) -> Result<isize> {
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        if inner.count <= 0 {
            while let Some(other) = inner.waiting.pop_front() {
                if let Some(other) = other.upgrade() {
                    other.mark_suspended();
                    TASK_SCHEDULER.put_read_task(other);
                    break;
                }
            }
        }
        Ok(inner.count)
    }

    fn down(&self) -> Result<isize> {
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            let current_task = PROCESSOR.current_task()?;
            inner.waiting.push_back(Arc::downgrade(&current_task));
            drop(current_task);
            drop(inner);
            block_current_and_run_other_task()?;
            Ok(self.inner.access().count)
        } else {
            Ok(inner.count)
        }
    }
}

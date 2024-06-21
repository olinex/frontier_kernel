// @author:    olinex
// @time:      2024/06/04

// self mods

// use other mods
use alloc::collections::VecDeque;
use alloc::sync::{Arc, Weak};

// use self mods
use crate::lang::container::UserPromiseRefCell;
use crate::prelude::*;
use crate::task::model::TaskControlBlock;
use crate::task::{
    block_current_and_run_other_task, suspend_current_and_run_other_task, PROCESSOR, TASK_SCHEDULER,
};

pub(crate) trait Mutex: Sync + Send {
    fn lock(&self) -> Result<()>;
    fn unlock(&self) -> Result<()>;
}

struct MutexSpinInner {
    locked: Option<Weak<TaskControlBlock>>,
}

pub(crate) struct MutexSpin {
    inner: UserPromiseRefCell<MutexSpinInner>,
}
impl MutexSpin {
    pub(crate) fn new() -> Self {
        Self {
            inner: unsafe { UserPromiseRefCell::new(MutexSpinInner { locked: None }) },
        }
    }
}
impl Mutex for MutexSpin {
    fn lock(&self) -> Result<()> {
        loop {
            let current_task = PROCESSOR.current_task()?;
            let mut inner = self.inner.exclusive_access();
            if let Some(prev) = inner.locked.as_ref().and_then(|prev| prev.upgrade()) {
                if Arc::as_ptr(&prev) == Arc::as_ptr(&current_task) {
                    return Err(KernelError::DoubleLockMutex);
                }
                drop(prev);
                drop(inner);
                drop(current_task);
                suspend_current_and_run_other_task()?;
                continue;
            }
            inner.locked.replace(Arc::downgrade(&current_task));
            return Ok(());
        }
    }

    fn unlock(&self) -> Result<()> {
        let current_task = PROCESSOR.current_task()?;
        let mut inner = self.inner.exclusive_access();
        if let Some(prev) = inner.locked.as_ref().and_then(|prev| prev.upgrade()) {
            if Arc::as_ptr(&prev) != Arc::as_ptr(&current_task) {
                return Err(KernelError::DoubleUnlockMutex);
            }
        }
        inner.locked.take();
        Ok(())
    }
}

struct MutexBlockingInner {
    locked: Option<Weak<TaskControlBlock>>,
    next: Option<Weak<TaskControlBlock>>,
    waiting: VecDeque<Weak<TaskControlBlock>>,
}

pub(crate) struct MutexBlocking {
    inner: UserPromiseRefCell<MutexBlockingInner>,
}
impl MutexBlocking {
    pub(crate) fn new() -> Self {
        Self {
            inner: unsafe {
                UserPromiseRefCell::new(MutexBlockingInner {
                    locked: None,
                    next: None,
                    waiting: VecDeque::new(),
                })
            },
        }
    }
}
impl Mutex for MutexBlocking {
    fn lock(&self) -> Result<()> {
        loop {
            let current_task = PROCESSOR.current_task()?;
            let mut inner = self.inner.exclusive_access();
            if let Some(prev) = inner.locked.as_ref().and_then(|prev| prev.upgrade()) {
                if Arc::as_ptr(&prev) == Arc::as_ptr(&current_task) {
                    return Err(KernelError::DoubleLockMutex);
                } else if inner
                    .next
                    .as_ref()
                    .and_then(|next| next.upgrade())
                    .is_some_and(|next| Arc::as_ptr(&next) == Arc::as_ptr(&current_task))
                {
                    drop(prev);
                    drop(inner);
                    drop(current_task);
                    suspend_current_and_run_other_task()?;
                    continue;
                }
                inner.waiting.push_back(Arc::downgrade(&current_task));
                drop(prev);
                drop(inner);
                drop(current_task);
                block_current_and_run_other_task()?;
                continue;
            }
            inner.next.take();
            inner.locked.replace(Arc::downgrade(&current_task));
            return Ok(());
        }
    }

    fn unlock(&self) -> Result<()> {
        let current_task = PROCESSOR.current_task()?;
        let mut inner = self.inner.exclusive_access();
        if let Some(prev) = inner.locked.as_ref().and_then(|prev| prev.upgrade()) {
            if Arc::as_ptr(&prev) != Arc::as_ptr(&current_task) {
                return Err(KernelError::DoubleUnlockMutex);
            }
        }
        if inner
            .next
            .as_ref()
            .and_then(|prev| prev.upgrade())
            .is_none()
        {
            while let Some(other) = inner.waiting.pop_front() {
                if let Some(other) = other.upgrade() {
                    inner.next.replace(Arc::downgrade(&other));
                    other.mark_suspended();
                    TASK_SCHEDULER.put_read_task(other);
                    break;
                }
            }
        }
        inner.locked.take();
        Ok(())
    }
}

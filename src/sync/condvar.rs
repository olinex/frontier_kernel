// @author:    olinex
// @time:      2024/06/12

// self mods

// use other mods
use alloc::collections::VecDeque;
use alloc::sync::{Arc, Weak};

// use self mods
use crate::lang::container::UserPromiseRefCell;
use crate::prelude::*;
use crate::task::model::TaskControlBlock;
use crate::task::{block_current_and_run_other_task, PROCESSOR, TASK_SCHEDULER};

use super::mutex::Mutex;

pub(crate) trait Condvar: Sync + Send {
    fn signal(&self) -> Result<()>;
    fn wait(&self, mutex: Arc<dyn Mutex>) -> Result<()>;
}

struct CondvarBlockingInner {
    waiting: VecDeque<Weak<TaskControlBlock>>,
}

pub(crate) struct CondvarBlocking {
    inner: UserPromiseRefCell<CondvarBlockingInner>,
}
impl CondvarBlocking {
    pub(crate) fn new() -> Self {
        Self {
            inner: unsafe {
                UserPromiseRefCell::new(CondvarBlockingInner {
                    waiting: VecDeque::new(),
                })
            },
        }
    }
}
impl Condvar for CondvarBlocking {
    fn signal(&self) -> Result<()> {
        let mut inner = self.inner.exclusive_access();
        while let Some(prev) = inner.waiting.pop_front() {
            if let Some(prev) = prev.upgrade() {
                prev.mark_suspended();
                TASK_SCHEDULER.put_read_task(prev);
                break;
            }
        }
        Ok(())
    }

    fn wait(&self, mutex: Arc<dyn Mutex>) -> Result<()> {
        mutex.unlock()?;
        let task = PROCESSOR.current_task()?;
        let mut inner = self.inner.exclusive_access();
        inner.waiting.push_back(Arc::downgrade(&task));
        drop(inner);
        drop(task);
        block_current_and_run_other_task()?;
        mutex.lock()
    }
}

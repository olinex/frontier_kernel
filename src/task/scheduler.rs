// @author:    olinex
// @time:      2024/04/16

// self mods

// use other mods
use alloc::collections::VecDeque;
use alloc::sync::Arc;

// use self mods
use super::model::{ProcessControlBlock, TaskControlBlock, ROOT_PID};
use super::process::PROCESSOR;
use crate::lang::container::UserPromiseRefCell;

/// Task queue that contains all ready tasks which are waiting for running
pub(crate) struct TaskScheduler {
    dqueue: VecDeque<Arc<TaskControlBlock>>,
}

impl TaskScheduler {
    /// Get the length of the queue
    pub(crate) fn len(&self) -> usize {
        self.dqueue.len()
    }

    /// Put a task into the queue
    pub(crate) fn put(&mut self, task: Arc<TaskControlBlock>) {
        self.dqueue.push_back(task);
    }

    /// Fetch and pop the first task from the queue
    pub(crate) fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.dqueue.pop_front()
    }

    /// Get the process control block according to pid and return the root task control block
    /// 
    /// - Arguments
    ///     - pid: the process's unique id
    pub(crate) fn get_root(&self, pid: usize) -> Option<Arc<TaskControlBlock>> {
        for task in self.dqueue.iter() {
            if task.tid() == 0 && task.process().pid() == pid {
                return Some(Arc::clone(task));
            }
        }
        None
    }

    /// Create a new task controller, which will load the task code and create the virtual address space
    pub(crate) fn new() -> Self {
        Self {
            dqueue: VecDeque::new(),
        }
    }
}

lazy_static! {
    /// The global visibility task scheduler which will load all tasks's code and create virtual address space lazily.
    pub(crate) static ref TASK_SCHEDULER: Arc<UserPromiseRefCell<TaskScheduler>> = {
        Arc::new(unsafe {UserPromiseRefCell::new(TaskScheduler::new())})
    };
}
impl TASK_SCHEDULER {
    /// Get the total tasks count in dqueue
    pub(crate) fn task_count(&self) -> usize {
        self.access().len()
    }

    /// Apeend task into dqueue tail
    pub(crate) fn add_task(&self, task: Arc<TaskControlBlock>) {
        self.exclusive_access().put(task);
    }

    /// Get task from dqueue head
    pub(crate) fn fetch_task(&self) -> Option<Arc<TaskControlBlock>> {
        self.exclusive_access().fetch()
    }

    /// Get the process control block according to pid and return the root task control block
    /// 
    /// - Arguments
    ///     - pid: the process's unique id
    pub(crate) fn get_process(&self, pid: isize) -> Option<Arc<ProcessControlBlock>> {
        if let Ok(task) = PROCESSOR.current_task() {
            if pid < ROOT_PID as isize {
                return Some(task.process());
            }
            let process = task.process();
            if process.pid() == pid as usize {
                Some(process)
            } else {
                None
            }
        } else if let Some(task) = self.access().get_root(pid as usize) {
            Some(task.process())
        } else {
            None
        }
    }
}

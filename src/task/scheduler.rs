// @author:    olinex
// @time:      2024/04/16

// self mods

// use other mods
use alloc::collections::{BinaryHeap, VecDeque};
use alloc::sync::Arc;

// use self mods
use super::model::{ProcessControlBlock, TaskControlBlock, ROOT_PID};
use super::process::PROCESSOR;
use crate::lang::container::UserPromiseRefCell;
use crate::lang::timer::get_timer_us;

/// A wrapper class for organizing storage blocking tasks that are not actively scheduled until the timeout requirements are met.
pub(crate) struct TimerCondVar {
    expire_us: usize,
    task: Arc<TaskControlBlock>,
}
impl PartialEq for TimerCondVar {
    fn eq(&self, other: &Self) -> bool {
        self.expire_us == other.expire_us
    }
}
impl Eq for TimerCondVar {}
impl PartialOrd for TimerCondVar {
    /// Rust's binary heap pops up the largest item, 
    /// so we invert the timeout value so that each popping item is always the closest to the timeout.
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let a = -(self.expire_us as isize);
        let b = -(other.expire_us as isize);
        Some(a.cmp(&b))
    }
}
impl Ord for TimerCondVar {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

/// Task queue that contains all ready tasks which are waiting for running
pub(crate) struct TaskScheduler {
    ready: VecDeque<Arc<TaskControlBlock>>,
    timer: BinaryHeap<TimerCondVar>,
}

impl TaskScheduler {
    /// Put a task into the ready queue
    fn put_as_ready(&mut self, task: Arc<TaskControlBlock>) {
        self.ready.push_back(task);
    }

    /// Fetch and pop the first ready task from the queue
    fn pop_ready(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready.pop_front()
    }

    /// Put a tasks into the timer heap
    fn put_as_timer(&mut self, expire_us: usize, task: Arc<TaskControlBlock>) {
        self.timer.push(TimerCondVar {
            expire_us,
            task,
        })
    }

    /// pop the task with the smallest expire microseconds from the heap
    fn pop_timer(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.timer.pop().map(|cv| cv.task)
    }

    /// Get the task control block according to pid and return the root task control block
    ///
    /// - Arguments
    ///     - pid: the process's unique id
    fn get_root_ready(&self, pid: usize) -> Option<Arc<TaskControlBlock>> {
        for task in self.ready.iter() {
            if task.tid() == 0 && task.process().pid() == pid {
                return Some(Arc::clone(task));
            }
        }
        None
    }

    /// Create a new task controller, which will load the task code and create the virtual address space
    pub(crate) fn new() -> Self {
        Self {
            ready: VecDeque::new(),
            timer: BinaryHeap::new(),
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
    /// Put ready task into dqueue tail
    pub(crate) fn put_read_task(&self, task: Arc<TaskControlBlock>) {
        self.exclusive_access().put_as_ready(task);
    }

    /// Pop ready task from dqueue head
    pub(crate) fn pop_ready_task(&self) -> Option<Arc<TaskControlBlock>> {
        self.exclusive_access().pop_ready()
    }

    /// Put block task into binary heap
    pub(crate) fn put_sleep_task(&self, us: usize, task: Arc<TaskControlBlock>) {
        let expire_us = get_timer_us() + us;
        self.exclusive_access().put_as_timer(expire_us, task)
    }

    /// Check all timers if it was timeout.
    /// Any timer timeout will be pop out from heap. 
    pub(crate) fn check_timers(&self) {
        let current_us = get_timer_us();
        let mut inner = self.exclusive_access();
        while let Some(cv) = inner.timer.peek() {
            if cv.task.is_zombie() {
                inner.pop_timer().unwrap();
            } else if cv.expire_us <= current_us {
                let task: Arc<TaskControlBlock> = Arc::clone(&cv.task);
                task.mark_suspended();
                inner.put_as_ready(task);
                inner.pop_timer().unwrap();
            } else {
                break
            }
        }
    }

    /// Remove specified task's timer from heap.
    pub(crate) fn remove_timer(&self, task: &Arc<TaskControlBlock>) {
        let mut inner = self.exclusive_access();
        let mut temp = BinaryHeap::new();
        for cv in inner.timer.drain() {
            if Arc::as_ptr(task) != Arc::as_ptr(&cv.task) {
                temp.push(cv);
            }
        }
        inner.timer.append(&mut temp);
    }

    /// Get the process control block according to pid and return the root task control block
    ///
    /// - Arguments
    ///     - pid: the process's unique id
    pub(crate) fn get_process(&self, pid: isize) -> Option<Arc<ProcessControlBlock>> {
        if let Ok(task) = PROCESSOR.current_task() {
            let process = task.process();
            if pid < ROOT_PID as isize {
                return Some(process);
            }
            if process.pid() == pid as usize {
                Some(process)
            } else {
                None
            }
        } else if let Some(task) = self.access().get_root_ready(pid as usize) {
            Some(task.process())
        } else {
            None
        }
    }
}

// @author:    olinex
// @time:      2023/10/12

// self mods

// use other mods
use alloc::sync::Arc;

// use self mods
use super::context::TaskContext;
use super::model::{TaskControlBlock, INIT_PROC};
use super::scheduler::TASK_SCHEDULER;
use super::{switch, Signal};
use crate::lang::container::UserPromiseRefCell;
use crate::{configs, prelude::*};

/// Keep the current running task the processor structure
pub(crate) struct Processor {
    current: Option<Arc<TaskControlBlock>>,
    empty_task_ctx: TaskContext,
    idle_task_ctx: TaskContext,
}
impl Processor {
    /// Create a new empty processor
    fn new() -> Self {
        let empty_task_ctx = TaskContext::empty();
        Self {
            current: None,
            empty_task_ctx,
            idle_task_ctx: empty_task_ctx,
        }
    }

    /// Get the current task's clone
    fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(|block| Arc::clone(block))
    }

    /// Get the mutable pointer of the idle task context
    fn get_idle_task_ctx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_ctx as *mut _
    }
}

lazy_static! {
    pub(crate) static ref PROCESSOR: Arc<UserPromiseRefCell<Processor>> =
        Arc::new(unsafe { UserPromiseRefCell::new(Processor::new()) });
}
impl PROCESSOR {
    /// Get the task which was currently run.
    ///
    /// - Errors
    ///     - ProcessHaveNotTask
    pub(crate) fn current_task(&self) -> Result<Arc<TaskControlBlock>> {
        self.access()
            .current()
            .ok_or(KernelError::ProcessHaveNotTask)
    }

    /// switch current process to idle task context
    pub(crate) fn switch_from(&self, current_task_ctx_ptr: *mut TaskContext) {
        let mut processor = self.exclusive_access();
        let next_task_ctx_ptr = processor.get_idle_task_ctx_ptr();
        drop(processor);
        unsafe {
            switch::_fn_switch_task(current_task_ctx_ptr, next_task_ctx_ptr);
        }
    }

    /// Fetch a runnable task and switch current process to it
    #[inline(always)]
    pub(crate) fn schedule(&self) -> ! {
        loop {
            TASK_SCHEDULER.check_timers();
            if let Some(task) = TASK_SCHEDULER.pop_ready_task() {
                if task.is_zombie() {
                    continue;
                }
                let mut processor = self.exclusive_access();
                let current_task_ctx_ptr = processor.get_idle_task_ctx_ptr();
                let next_task_ctx_ptr = task.task_ctx_ptr();
                task.mark_running();
                processor.current.replace(task);
                drop(processor);
                unsafe {
                    switch::_fn_switch_task(current_task_ctx_ptr, next_task_ctx_ptr);
                }
            } else {
                panic!("There was no task available in the task queue")
            }
        }
    }

    /// Mark current task as suspended and run other runable task
    ///
    /// - Errors
    ///     - ProcessHaveNotTask
    pub(crate) fn suspend_current_and_run_other_task(&self) -> Result<()> {
        let mut processor = self.exclusive_access();
        if let Some(task) = processor.current.take() {
            task.mark_suspended();
            let current_task_ctx_ptr = task.task_ctx_ptr() as *mut TaskContext;
            TASK_SCHEDULER.put_read_task(task);
            drop(processor);
            self.switch_from(current_task_ctx_ptr);
            Ok(())
        } else {
            Err(KernelError::ProcessHaveNotTask)
        }
    }

    /// Mark current task as blocked and run other runable task.
    /// Blocked task will no input into runable task, untill it's status change to ready by system event.
    /// 
    /// - Errors
    ///     - ProcessHaveNotTask
    pub(crate) fn block_current_and_run_other_task(&self, f: impl FnOnce(Arc<TaskControlBlock>) -> Result<()>) -> Result<()> {
        let mut processor = self.exclusive_access();
        if let Some(task) = processor.current.take() {
            task.mark_blocked();
            let current_task_ctx_ptr = task.task_ctx_ptr() as *mut TaskContext;
            f(task)?;
            drop(processor);
            self.switch_from(current_task_ctx_ptr);
            Ok(())
        } else {
            Err(KernelError::ProcessHaveNotTask)
        }
    }

    /// Mark current task as exited, write the exit code into the current task context,
    /// and run other runable task.
    /// In the same time, we also do some other things:
    ///     - remove current task from timer block list
    ///
    /// - Arguments
    ///     - exit_code: the exit code passing from the user space
    ///
    /// - Errors
    ///     - ProcessHaveNotTask
    pub(crate) fn exit_current_and_run_other_task(&self, exit_code: i32) -> Result<()> {
        let mut processor = self.exclusive_access();
        if let Some(task) = processor.current.take() {
            TASK_SCHEDULER.remove_timer(&task);
            assert_eq!(Arc::strong_count(&task), 2);
            let current_task_ctx_ptr = &mut processor.empty_task_ctx as *mut _;
            task.mark_zombie(exit_code);
            drop(task);
            drop(processor);
            self.switch_from(current_task_ctx_ptr);
            Ok(())
        } else {
            Err(KernelError::ProcessHaveNotTask)
        }
    }

    /// Send signal to current task.
    ///
    /// - Arguments
    ///     - signal: which signal will be setted
    ///
    /// - Errors
    ///     - ProcessHaveNotTask
    ///     - DuplicateSignal(signal)
    pub(crate) fn send_current_task_signal(&self, signal: Signal) -> Result<()> {
        let processor = self.access();
        if let Some(task) = &processor.current {
            task.process().kill(signal)
        } else {
            Err(KernelError::ProcessHaveNotTask)
        }
    }

    /// All received signals are processed until all signals have been processed,
    /// or the current service needs to be terminated/suspended
    ///
    /// - Returns
    ///     - Ok(None): every signal was been handled and return back to user-mode
    ///     - Ok(Some(signal)): the signal is not completely silent
    ///     - Err(error): get something wrong when handling signal
    ///
    /// - Errors
    ///     - ProcessHaveNotTask
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    pub(crate) fn handle_current_task_signals(&self) -> Result<Option<Signal>> {
        loop {
            let processor = self.access();
            if let Some(task) = &processor.current {
                let process = task.process();
                let (killed, frozen) = process.handle_all_signals()?;
                if !frozen || killed {
                    break;
                };
                drop(process);
                drop(processor);
                self.suspend_current_and_run_other_task()?;
            } else {
                return Err(KernelError::ProcessHaveNotTask);
            }
        }
        let processor = self.access();
        if let Some(task) = &processor.current {
            Ok(task.process().check_bad_signals())
        } else {
            Err(KernelError::ProcessHaveNotTask)
        }
    }
}

/// Add a initial process to the task queue.
#[inline(always)]
pub(crate) fn add_init_proc() {
    debug!(
        "adding initial task {} to task queue",
        configs::INIT_PROCESS_PATH
    );
    TASK_SCHEDULER.put_read_task(INIT_PROC.inner_access().root_task());
}

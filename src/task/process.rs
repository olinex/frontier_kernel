// @author:    olinex
// @time:      2023/10/12

// self mods

// use other mods
use alloc::sync::Arc;

// use self mods
use super::context::TaskContext;
use super::control::{TaskController, TaskMeta};
use super::switch;
use crate::lang::container::UserPromiseRefCell;
use crate::{configs, prelude::*};

/// Keep the current running task the processor structure
pub(crate) struct Processor {
    current: Option<Arc<TaskMeta>>,
    idle_task_ctx: TaskContext,
}
impl Processor {
    /// Create a new empty processor
    fn new() -> Self {
        Self {
            current: None,
            idle_task_ctx: TaskContext::empty(),
        }
    }

    /// Get the current task's clone
    fn current(&self) -> Option<Arc<TaskMeta>> {
        self.current.as_ref().map(|meta| Arc::clone(meta))
    }

    /// Get the mutable pointer of the idle task context
    fn get_idle_task_ctx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_ctx as *mut _
    }
}

lazy_static! {
    /// The global visibility task controller which will load all tasks's code and create virtual address space lazily.
    pub(crate) static ref TASK_CONTROLLER: Arc<UserPromiseRefCell<TaskController>> = {
        Arc::new(unsafe {UserPromiseRefCell::new(TaskController::new())})
    };
}
impl TASK_CONTROLLER {
    pub(crate) fn task_count(&self) -> usize {
        self.access().len()
    }

    pub(crate) fn add_task(&self, task: Arc<TaskMeta>) {
        self.exclusive_access().put(task);
    }

    pub(crate) fn fetch_task(&self) -> Option<Arc<TaskMeta>> {
        self.exclusive_access().fetch()
    }
}

lazy_static! {
    pub(crate) static ref PROCESSOR: Arc<UserPromiseRefCell<Processor>> =
        Arc::new(unsafe { UserPromiseRefCell::new(Processor::new()) });
}
impl PROCESSOR {
    pub(crate) fn current_task(&self) -> Result<Arc<TaskMeta>> {
        self.exclusive_access()
            .current()
            .ok_or(KernelError::ProcessHaveNotTask)
    }

    /// switch current process to idle task context
    fn idle(&self, switched_task_cx_ptr: *mut TaskContext) {
        let mut processor = self.exclusive_access();
        let idle_task_cx_ptr = processor.get_idle_task_ctx_ptr();
        drop(processor);
        unsafe {
            switch::_fn_switch_task(switched_task_cx_ptr, idle_task_cx_ptr);
        }
    }

    /// Fetch a runnable task and switch current process to it
    #[inline(always)]
    pub(crate) fn schedule(&self) -> ! {
        loop {
            let mut processor = self.exclusive_access();
            if let Some(task) = TASK_CONTROLLER.fetch_task() {
                let idle_task_cx_ptr = processor.get_idle_task_ctx_ptr();
                let next_task_cx_ptr = task.task_ctx_ptr();
                task.mark_running();
                processor.current = Some(task);
                drop(processor);
                unsafe {
                    switch::_fn_switch_task(idle_task_cx_ptr, next_task_cx_ptr);
                }
            } else {
                panic!("There was no task available in the task queue")
            }
        }
    }

    /// Mark current task as suspended and run other runable task
    ///
    /// # Returns
    /// * Ok(())
    /// * Err(KernelError::ProcessHaveNotTask)
    pub(crate) fn suspend_current_and_run_other_task(&self) -> Result<()> {
        let processor = self.access();
        if let Some(meta) = &processor.current {
            meta.mark_suspended();
            TASK_CONTROLLER.add_task(Arc::clone(meta));
            let task_ctx_ptr = meta.task_ctx_ptr() as *mut TaskContext;
            drop(processor);
            self.idle(task_ctx_ptr);
            Ok(())
        } else {
            Err(KernelError::ProcessHaveNotTask)
        }
    }

    /// Mark current task as exited, write the exit code into the current task context,
    /// and run other runable task
    ///
    /// # Arguments
    /// * exit_code: the exit code passing from the user space
    ///
    /// # Returns
    /// * Ok(())
    /// * Err(KernelError::ProcessHaveNotTask)
    pub(crate) fn exit_current_and_run_other_task(&self, exit_code: i32) -> Result<()> {
        debug!(
            "exit current task, still {} tasks",
            TASK_CONTROLLER.task_count()
        );
        let processor = self.access();
        if let Some(meta) = &processor.current {
            meta.mark_zombie(exit_code);
            drop(processor);
            let mut unused = TaskContext::empty();
            self.idle(&mut unused as *mut _);
            Ok(())
        } else {
            Err(KernelError::ProcessHaveNotTask)
        }
    }
}

lazy_static! {
    pub(crate) static ref INITPROC: Arc<TaskMeta> = TaskMeta::new_init_proc().unwrap();
}

/// Add a initial process to the task queue.
#[inline(always)]
pub(crate) fn add_init_proc() {
    debug!(
        "adding initial task {} to task queue",
        configs::INIT_PROCESS_PATH
    );
    TASK_CONTROLLER.add_task(Arc::clone(&INITPROC));
}

// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods

// use self mods
use super::context;
use crate::configs;
use crate::memory::StackTr;
use crate::prelude::*;
use crate::trap::context as trap_context;

pub fn get_base_address(task_id: usize) -> usize {
    configs::APP_BASE_ADDRESS + task_id * configs::APP_SIZE_LIMIT
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TaskStatus {
    UnInit,
    Ready,
    Running,
    Suspended,
    Exited,
}

#[derive(Debug, Copy, Clone)]
pub struct TaskMeta {
    status: TaskStatus,
    ctx: context::TaskContext,
}

impl TaskMeta {
    fn new() -> Self {
        Self {
            status: TaskStatus::UnInit,
            ctx: context::TaskContext::new(),
        }
    }

    fn init(task_id: usize) -> Self {
        let trap_ctx = trap_context::TrapContext::create_app_init_context(
            get_base_address(task_id),
            trap_context::USER_STACK[task_id].get_top(),
        );
        let mut task_ctx = context::TaskContext::new();
        let stack_ctx_ptr = trap_context::KERNEL_STACK[task_id].push_context(trap_ctx);
        task_ctx.goto_restore(stack_ctx_ptr as *const _ as usize);
        Self {
            status: TaskStatus::Ready,
            ctx: task_ctx,
        }
    }

    #[inline]
    pub fn ctx(&self) -> &context::TaskContext {
        &self.ctx
    }

    #[inline]
    fn mark_suspended(&mut self) {
        self.status = TaskStatus::Suspended;
    }

    #[inline]
    pub fn mark_running(&mut self) {
        self.status = TaskStatus::Running;
    }

    #[inline]
    fn mark_exited(&mut self) {
        self.status = TaskStatus::Exited;
    }
}

pub struct TaskController {
    tasks: [TaskMeta; configs::MAX_TASK_NUM],
    current_task: usize,
}

impl TaskController {
    pub fn new(task_count: usize) -> Self {
        let mut tasks = [TaskMeta::new(); configs::MAX_TASK_NUM];
        for task_id in 0..task_count {
            tasks[task_id] = TaskMeta::init(task_id);
        }
        Self {
            tasks,
            current_task: 0,
        }
    }

    #[inline]
    pub fn first_task_meta(&mut self) -> &mut TaskMeta {
        &mut self.tasks[0]
    }

    #[inline]
    pub fn mark_current_task_suspended(&mut self) {
        self.tasks[self.current_task].mark_suspended();
    }

    #[inline]
    pub fn mark_current_task_exited(&mut self) {
        self.tasks[self.current_task].mark_exited();
    }

    fn find_other_runable_task(&self, task_count: usize) -> Option<usize> {
        let current = self.current_task;
        for offset in current + 1..current + task_count {
            let task_id = offset % task_count;
            if let TaskStatus::Ready | TaskStatus::Suspended = self.tasks[task_id].status {
                return Some(task_id);
            }
        }
        None
    }

    pub fn prepare_other_task(
        &mut self,
        task_count: usize,
    ) -> Option<(*mut context::TaskContext, *const context::TaskContext)> {
        if let Some(next_task_id) = self.find_other_runable_task(task_count) {
            let current_task_id = self.current_task;
            let next_task_meta = &mut self.tasks[next_task_id];
            next_task_meta.mark_running();
            let current_task_meta = &mut self.tasks[current_task_id];
            let current_task_ctx_ptr = &mut current_task_meta.ctx as *mut context::TaskContext;
            let next_task_ctx_ptr = &self.tasks[next_task_id].ctx as *const context::TaskContext;
            self.current_task = next_task_id;
            info!("switch task from {} to {}", current_task_id, next_task_id);
            Some((current_task_ctx_ptr, next_task_ctx_ptr))
        } else {
            None
        }
    }
}

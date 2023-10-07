// @author:    olinex
// @time:      2023/09/01

// self mods

use core::cell::RefMut;

// use other mods
use alloc::collections::BTreeMap;

// use self mods
use super::context::TaskContext;
use super::switch;
use crate::memory::space::{Space, KERNEL_SPACE};
use crate::prelude::*;
use crate::sbi::*;
use crate::trap::context::TrapContext;

/// The execution status of the task
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Suspended,
    Exited,
}

/// The meta information of the task
pub struct TaskMeta {
    status: TaskStatus,
    task_ctx: TaskContext,
    space: Space,
    /// The physical page number which saved the task's trap context
    #[allow(dead_code)]
    trap_ctx_ppn: usize,
    /// The size of the task's using virtual address from 0x00 to the top of the user stack
    #[allow(dead_code)]
    base_size: usize,
}
impl TaskMeta {
    /// Help function for helping the trap context from task's virtual address space
    /// # Arguments
    /// * space: the virtual address space
    fn get_trap_ctx(space: &Space) -> Result<&mut TrapContext> {
        let trap_ctx_area = space.get_trap_context_area()?;
        let (trap_ctx_vpn, _) = trap_ctx_area.range();
        let trap_ctx = unsafe { trap_ctx_area.as_kernel_mut(trap_ctx_vpn, 0)? };
        Ok(trap_ctx)
    }

    /// Create a new task meta
    ///
    /// # Arguments
    /// * task_id: the unique id for each task
    /// * data: the byte data of the task
    ///
    /// # Returns
    /// * Ok(TaskMeta)
    fn new(task_id: usize, data: &[u8]) -> Result<Self> {
        let (space, user_stack_top_va, kernel_stack_top_va, entry_point) =
            KERNEL_SPACE::new_task_from_elf(task_id, data)?;
        info!(
            "load task {} with user_stack_top_va: {:#x}, kernel_stack_top_va: {:#x}, entry_point: {:#x}",
            task_id, user_stack_top_va, kernel_stack_top_va, entry_point
        );
        let trap_ctx_ppn = space.trap_ctx_ppn()?;
        let trap_ctx = Self::get_trap_ctx(&space)?;
        *trap_ctx = TrapContext::create_app_init_context(
            entry_point,
            user_stack_top_va,
            kernel_stack_top_va,
        );
        let mut task_ctx = TaskContext::empty();
        task_ctx.goto_trap_return(kernel_stack_top_va);
        Ok(Self {
            status: TaskStatus::Ready,
            task_ctx,
            space,
            trap_ctx_ppn,
            base_size: user_stack_top_va,
        })
    }

    #[inline(always)]
    pub fn space(&self) -> &Space {
        &self.space
    }

    #[inline(always)]
    pub fn task_ctx(&self) -> &TaskContext {
        &self.task_ctx
    }

    #[inline(always)]
    pub fn trap_ctx(&self) -> Result<&mut TrapContext> {
        Self::get_trap_ctx(&self.space)
    }

    #[inline(always)]
    pub fn task_id(&self) -> usize {
        self.space.mmu_asid()
    }

    #[inline(always)]
    fn mark_suspended(&mut self) {
        self.status = TaskStatus::Suspended;
    }

    #[inline(always)]
    pub fn mark_running(&mut self) {
        self.status = TaskStatus::Running;
    }

    #[inline(always)]
    fn mark_exited(&mut self) {
        self.status = TaskStatus::Exited;
    }

    #[inline(always)]
    fn get_user_token(&self) -> usize {
        self.space.mmu_token()
    }
}
impl Drop for TaskMeta {
    fn drop(&mut self) {
        KERNEL_SPACE
            .unmap_kernel_task_stack(self.task_id())
            .unwrap();
    }
}

#[repr(C)]
pub struct TaskRange {
    pub start: usize,
    pub end: usize,
}

pub struct TaskController {
    tasks: BTreeMap<usize, TaskMeta>,
    current_task: usize,
}

impl TaskController {
    /// This function looks so strange that it accepts the RefMut of Self.
    /// Because we will change the controller and never return back.
    /// So we need a mutable reference for controller itself but which never dropped automatically
    /// That why we need a RefMut and drop it before switching to another task
    ///
    /// # Arguments
    /// * ref_mut: the mutable reference to the task conroller
    pub fn run_first_task(mut ref_mut: RefMut<'_, Self>) -> ! {
        if let Some((_, next_task)) = ref_mut.first_task_meta() {
            next_task.mark_running();
            let next_task_ctx_ptr = next_task.task_ctx() as *const TaskContext;
            drop(ref_mut);
            unsafe {
                switch::_fn_run_first_task(next_task_ctx_ptr);
            }
        }
        panic!("Unreachable code Cause by run_first_task");
    }

    pub fn run_other_task(mut ref_mut: RefMut<'_, Self>) {
        if let Ok((current_ptr, next_ptr)) = ref_mut.prepare_other_task() {
            drop(ref_mut);
            unsafe { switch::_fn_switch_task(next_ptr, current_ptr) };
        } else {
            info!("[kernel] All tasks completed!");
            SBI::shutdown();
        }
    }

    pub fn suspend_current_and_run_other_task(mut ref_mut: RefMut<'_, Self>) -> Result<()> {
        ref_mut.mark_current_task_suspended()?;
        Self::run_other_task(ref_mut);
        Ok(())
    }

    pub fn exit_current_and_run_other_task(mut ref_mut: RefMut<'_, Self>) -> Result<()> {
        ref_mut.mark_current_task_exited()?;
        Self::run_other_task(ref_mut);
        Ok(())
    }

    /// Create a new task controller, which will load the task code and create the virtual address space
    ///
    /// # Arguments
    /// * task_ranges: the slice of the task ranges which contain the task code's start and end physical addresses
    ///
    /// # Returns
    /// Ok(TaskController)
    pub fn new(task_ranges: &[TaskRange]) -> Result<Self> {
        let mut tasks = BTreeMap::new();

        // clear i-cache first
        unsafe { SBI::fence_i() };

        // load apps
        info!("task count = {}", task_ranges.len());
        for (i, range) in task_ranges.iter().enumerate() {
            // load app from data section to memory
            let length = range.end - range.start;
            info!(
                "app_{} memory range [{:#x}, {:#x}), length = {:#x}",
                i, range.start, range.end, length
            );
            // load task's code in byte slice
            let src = unsafe { core::slice::from_raw_parts(range.start as *const u8, length) };
            // create a new task meta
            let task = TaskMeta::new(i, src)?;
            tasks.insert(i, task);
        }

        Ok(Self {
            tasks,
            current_task: 0,
        })
    }

    #[inline(always)]
    fn current_task_meta(&self) -> Result<&TaskMeta> {
        self.tasks
            .get(&self.current_task)
            .ok_or(KernelError::TaskNotFound(self.current_task))
    }

    #[inline(always)]
    fn current_task_meta_mut(&mut self) -> Result<&mut TaskMeta> {
        self.tasks
            .get_mut(&self.current_task)
            .ok_or(KernelError::TaskNotFound(self.current_task))
    }

    #[inline(always)]
    pub fn current_space(&self) -> Result<&Space> {
        Ok(self.current_task_meta()?.space())
    }

    pub fn get_current_user_token(&self) -> Result<usize> {
        let meta = self.current_task_meta()?;
        Ok(meta.get_user_token())
    }

    pub fn get_current_trap_ctx(&self) -> Result<&mut TrapContext> {
        let meta = self.current_task_meta()?;
        meta.trap_ctx()
    }

    fn first_task_meta(&mut self) -> Option<(usize, &mut TaskMeta)> {
        for (task_id, meta) in self.tasks.iter_mut() {
            return Some((*task_id, meta));
        }
        None
    }

    fn mark_current_task_suspended(&mut self) -> Result<()> {
        let meta = self.current_task_meta_mut()?;
        meta.mark_suspended();
        Ok(())
    }

    fn mark_current_task_exited(&mut self) -> Result<()> {
        let meta = self.current_task_meta_mut()?;
        meta.mark_exited();
        Ok(())
    }

    fn find_other_runable_task(&mut self) -> Option<(usize, &mut TaskMeta)> {
        for (task_id, meta) in self.tasks.iter_mut() {
            if *task_id == self.current_task {
                continue;
            }
            if let TaskStatus::Ready | TaskStatus::Suspended = meta.status {
                return Some((*task_id, meta));
            }
        }
        None
    }

    fn prepare_other_task(&mut self) -> Result<(*mut TaskContext, *const TaskContext)> {
        let current_task_id = self.current_task;
        if let Some((next_task_id, next_task_meta)) = self.find_other_runable_task() {
            let next_task_ctx_ptr = &next_task_meta.task_ctx as *const TaskContext;
            next_task_meta.mark_running();
            let current_task_meta = self.current_task_meta_mut()?;
            let current_task_ctx_ptr = &mut current_task_meta.task_ctx as *mut TaskContext;
            self.current_task = next_task_id;
            info!("switch task from {} to {}", current_task_id, next_task_id);
            Ok((current_task_ctx_ptr, next_task_ctx_ptr))
        } else {
            Err(KernelError::NoRunableTasks)
        }
    }
}

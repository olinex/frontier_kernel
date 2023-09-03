// @author:    olinex
// @time:      2023/09/01

// self mods
mod context;
mod switch;

// use other mods
use core::arch::{asm, global_asm};
use lazy_static::lazy_static;
use sbi::legacy::shutdown;

// use self mods
use crate::configs;
use crate::lang::refs;
use crate::memory::StackTr;
use crate::prelude::*;
use crate::trap::context as trap_context;

global_asm!(include_str!("../assembly/riscv64/link_app.asm"));

fn get_base_address(task_id: usize) -> usize {
    configs::APP_BASE_ADDRESS + task_id * configs::APP_SIZE_LIMIT
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum TaskStatus {
    UnInit,
    Ready,
    Running,
    Suspended,
    Exited,
}

#[derive(Debug, Copy, Clone)]
struct TaskMeta {
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
    fn mark_suspended(&mut self) {
        self.status = TaskStatus::Suspended;
    }

    #[inline]
    fn mark_running(&mut self) {
        self.status = TaskStatus::Running;
    }

    #[inline]
    fn mark_exited(&mut self) {
        self.status = TaskStatus::Exited;
    }
}

struct TaskController {
    tasks: [TaskMeta; configs::MAX_APP_NUM],
    current_task: usize,
}

impl TaskController {
    fn new(task_count: usize) -> Self {
        let mut tasks = [TaskMeta::new(); configs::MAX_APP_NUM];
        for task_id in 0..task_count {
            tasks[task_id] = TaskMeta::init(task_id);
        }
        Self {
            tasks,
            current_task: 0,
        }
    }

    #[inline]
    fn mark_current_task_suspended(&mut self) {
        self.tasks[self.current_task].mark_suspended();
    }

    #[inline]
    fn mark_current_task_exited(&mut self) {
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

    fn prepare_other_task(
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

pub struct TaskManager {
    task_count: usize,
    start_address: usize,
    controller: refs::UPSafeCell<TaskController>,
}

impl TaskManager {
    fn new(app_count: usize, start_address: usize, controller: TaskController) -> Self {
        Self {
            task_count: app_count,
            start_address,
            controller: unsafe { refs::UPSafeCell::new(controller) },
        }
    }

    unsafe fn get_address_array(&self) -> &[usize] {
        core::slice::from_raw_parts(self.start_address as *const usize, self.task_count + 1)
    }

    // load appliation binary code to other memory locations
    fn load_apps(&self) {
        let app_start = unsafe {
            core::slice::from_raw_parts(self.start_address as *const usize, self.task_count + 1)
        };
        // clear i-cache first
        unsafe {
            asm!("fence.i");
        }
        // load apps
        for i in 0..self.task_count {
            let addr = get_base_address(i);
            // clear region
            (addr..addr + configs::APP_SIZE_LIMIT)
                .for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) });
            // load app from data section to memory
            let src = unsafe {
                core::slice::from_raw_parts(
                    app_start[i] as *const u8,
                    app_start[i + 1] - app_start[i],
                )
            };
            let dst = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, src.len()) };
            dst.copy_from_slice(src);
        }
    }

    // print all application address range like [start, end)
    fn print_app_infos(&self) {
        info!("task count = {}", self.task_count);
        let index_array = unsafe { self.get_address_array() };
        for i in 0..self.task_count {
            let (start_addr, end_addr) = (index_array[i], index_array[i + 1]);
            info!(
                "app_{} memory range [{:#x}, {:#x}), length = {:#x}",
                i,
                start_addr as usize,
                end_addr as usize,
                end_addr as usize - start_addr as usize
            );
        }
    }

    fn run_other_task(&self) {
        let mut controller = self.controller.exclusive_access();
        if let Some((current_ptr, next_ptr)) = controller.prepare_other_task(self.task_count) {
            drop(controller);
            unsafe { switch::_fn_switch_task(current_ptr, next_ptr) };
        } else {
            info!("[kernel] All tasks completed!");
            shutdown();
        }
    }

    fn run_first_task(&self) -> ! {
        let mut unused_current_task = context::TaskContext::new();
        let current_task_ctx_ptr = &mut unused_current_task as *mut context::TaskContext;
        let mut controller = self.controller.exclusive_access();
        let next_task = &mut controller.tasks[0];
        next_task.mark_running();
        let next_task_txt_ptr = &next_task.ctx as *const context::TaskContext;
        drop(controller);
        unsafe {
            switch::_fn_switch_task(current_task_ctx_ptr, next_task_txt_ptr);
        }
        panic!("Unreachable code Cause by run_first_task");
    }

    fn suspend_current_and_run_other_task(&self) {
        let mut controller = self.controller.exclusive_access();
        controller.mark_current_task_suspended();
        drop(controller);
        self.run_other_task();
    }

    fn exit_current_and_run_other_task(&self) {
        let mut controller = self.controller.exclusive_access();
        controller.mark_current_task_exited();
        drop(controller);
        self.run_other_task();
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: TaskManager = {
        // load _addr_app_count which defined in link_app.asm
        extern "C" { fn _addr_app_count(); }
        // convert _addr_app_count as const usize pointer
        let task_count_ptr = _addr_app_count as usize as *const usize;
        // read app_count value
        let task_count = unsafe {task_count_ptr.read_volatile()};
        // get start address which is after the app count pointer
        let start_address = unsafe {task_count_ptr.add(1)} as usize;
        let controller = TaskController::new(task_count);
        TaskManager::new(
            task_count,
            start_address,
            controller
        )
    };
}

// init co-operation system
pub fn init() {
    TASK_MANAGER.print_app_infos();
    TASK_MANAGER.load_apps()
}

// run tasks
pub fn run() -> ! {
    TASK_MANAGER.run_first_task()
}

// suspend and run other tasks
#[inline]
pub fn suspend_current_and_run_other_task() {
    TASK_MANAGER.suspend_current_and_run_other_task();
}

#[inline]
pub fn exit_current_and_run_other_task() {
    TASK_MANAGER.exit_current_and_run_other_task();
}

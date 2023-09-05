// @author:    olinex
// @time:      2023/09/01

// self mods
mod context;
mod control;
mod switch;

// use other mods
use cfg_if::cfg_if;
use core::arch::global_asm;
use lazy_static::lazy_static;

// use self mods
use crate::configs;
use crate::lang::refs;
use crate::prelude::*;
use crate::sbi::*;

cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        global_asm!(include_str!("../assembly/riscv64/link_app.asm"));
    } else {
        compile_error!("Unknown target_arch to include assembly ../assembly/*/link_app.asm");
    }
}

pub struct TaskManager {
    task_count: usize,
    start_address: usize,
    controller: refs::UPSafeCell<control::TaskController>,
}

impl TaskManager {
    fn new(app_count: usize, start_address: usize, controller: control::TaskController) -> Self {
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
        unsafe { SBI::fence_i() };

        // load apps
        for i in 0..self.task_count {
            let addr = control::get_base_address(i);
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
            SBI::shutdown();
        }
    }

    fn run_first_task(&self) -> ! {
        let mut unused_current_task = context::TaskContext::new();
        let current_task_ctx_ptr = &mut unused_current_task as *mut context::TaskContext;
        let mut controller = self.controller.exclusive_access();
        let next_task = controller.first_task_meta();
        next_task.mark_running();
        let next_task_txt_ptr = next_task.ctx() as *const context::TaskContext;
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
        let controller = control::TaskController::new(task_count);
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

// @author:    olinex
// @time:      2023/09/01

// self mods
mod context;
mod control;
mod switch;

// use other mods
use alloc::sync::Arc;
use core::arch::global_asm;

// use self mods
use crate::lang::container::UserPromiseRefCell;
use crate::prelude::*;

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        global_asm!(include_str!("../assembly/riscv64/link_app.asm"));
    } else {
        compile_error!("Unknown target_arch to include assembly ../assembly/*/link_app.asm");
    }
}

lazy_static! {
    pub static ref TASK_CONTROLLER: Arc<UserPromiseRefCell<control::TaskController>> = {
        // load _addr_app_count which defined in link_app.asm
        extern "C" { fn _addr_app_count(); }
        // convert _addr_app_count as const usize pointer
        let task_count_ptr = _addr_app_count as usize as *const usize;
        // read app_count value
        let task_count = unsafe {task_count_ptr.read_volatile()};
        // get start address which is after the app count pointer
        let start_address = unsafe {task_count_ptr.add(1)} as usize;
        let controller = control::TaskController::new(start_address, task_count).unwrap();
        Arc::new(unsafe {UserPromiseRefCell::new(controller)})
    };
}

// run tasks
#[inline(always)]
pub fn run() -> ! {
    let ref_mut = TASK_CONTROLLER.exclusive_access();
    control::TaskController::run_first_task(ref_mut);
}

// suspend and run other tasks
#[inline(always)]
pub fn suspend_current_and_run_other_task() -> Result<()> {
    TASK_CONTROLLER
        .exclusive_access()
        .suspend_current_and_run_other_task()
}

#[inline(always)]
pub fn exit_current_and_run_other_task() -> Result<()> {
    TASK_CONTROLLER
        .exclusive_access()
        .exit_current_and_run_other_task()
}

//! The time-sharing multitasking mechanism of the operating system is very complex,
//! among which the context switching of virtual page tables and tasks is the most complex.
//! We try to explain the process as briefly as possible with flowcharts and corresponding notes:
//!
//!    <-- (0) [`crate::memory::space::KERNEL_SPACE::activate`]
//!   |        * Make trap handler disabled
//!   |        * Make kernel virtual address space avtivate
//!   |
//!    --> (1) [`crate::task::control::TaskController::run_first_task`]
//!   |        * Find out the first runable task
//!   |        * Make the first task status is runnings
//!   |        * Jump to the assembly function and never return back
//!   |
//!    --> (2) [`crate::task::switch::_fn_first_task`]
//!   |        * rstore the ra/sp/callee saved registers
//!   |        * Swith to current task's kernel stack
//!   |        * Jump to `trap_return`,
//!   |
//!    --> (3) [`crate::trap::handler::trap_return`]
//!   |        * enable trampoline assembly code as trap handler
//!   |        * jump to `_restore` assemble function
//!              use a0 as trap context vritual address,
//!              use a1 as current user task's mmu token
//!   |
//!    --> (4) [`crate::assembly::trampoline::_fn_restore_all_registers_after_trap`]
//!   |        * switch page table to current task's
//!   |        * restore sstatus/sepc/other generated registers
//!   |        * swtich stack to current task's user stack
//!   |        * Jump to current task's entry point
//!   |
//!   |        *

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

// reexport
pub use control::TaskController;

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        global_asm!(include_str!("../assembly/riscv64/link_app.asm"));
    } else {
        compile_error!("Unknown target_arch to include assembly ../assembly/*/link_app.asm");
    }
}

lazy_static! {
    /// The global visibility task controller which will load all tasks's code and create virtual address space lazily.
    pub static ref TASK_CONTROLLER: Arc<UserPromiseRefCell<control::TaskController>> = {
        // load _addr_app_count which defined in link_app.asm
        extern "C" { fn _addr_app_count(); }
        // convert _addr_app_count as const usize pointer
        let task_count_ptr = _addr_app_count as usize as *const usize;
        // read app_count value
        let task_count = unsafe {task_count_ptr.read_volatile()};
        // get start address which is after the app count pointer
        let task_range_ptr = unsafe {task_count_ptr.add(1)} as usize;
        // load task range slice
        let task_ranges = unsafe {
            core::slice::from_raw_parts(task_range_ptr as *const control::TaskRange, task_count)
        };
        // create task controller and load all tasks's code
        let controller = control::TaskController::new(task_ranges).unwrap();
        Arc::new(unsafe {UserPromiseRefCell::new(controller)})
    };
}

/// This method allows the multitasking system to start really running,
/// which is the engine ignition switch
#[inline(always)]
#[allow(dead_code)]
pub fn run() -> ! {
    // Keep the ownership of the mutable reference of the task controller,
    // Because the `run_first_task` will never return, so we must and move it into the function
    let ref_mut = TASK_CONTROLLER.exclusive_access();
    control::TaskController::run_first_task(ref_mut);
}

/// Suspend current task and run other runable task
#[inline(always)]
pub fn suspend_current_and_run_other_task() -> Result<()> {
    let ref_mut = TASK_CONTROLLER.exclusive_access();
    TaskController::suspend_current_and_run_other_task(ref_mut)
}

/// Exit current task and run other runable task
#[inline(always)]
pub fn exit_current_and_run_other_task() -> Result<()> {
    let ref_mut = TASK_CONTROLLER.exclusive_access();
    TaskController::exit_current_and_run_other_task(ref_mut)
}

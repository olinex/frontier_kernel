//! The time-sharing multitasking mechanism of the operating system is very complex,
//! among which the context switching of virtual page tables and tasks is the most complex.
//! We try to explain the process as briefly as possible with flowcharts and corresponding notes:
//!
//!    <-- (0) [`crate::trap::init`]
//!   |        * Make trap handler disabled
//!   |        * Make kernel virtual address space activate
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

// @author:    olinex
// @time:      2023/09/01

// self mods
mod allocator;
mod context;
mod control;
mod process;
mod switch;

// use other mods

// use self mods
use crate::prelude::*;

// reexports
pub(crate) use process::PROCESSOR;
pub(crate) use process::TASK_CONTROLLER;

/// This method allows the multitasking system to start really running,
/// which is the engine ignition switch
#[allow(dead_code)]
#[inline(always)]
pub(crate) fn run() -> ! {
    process::PROCESSOR.schedule()
}

/// Suspend current task and run other runable task
pub(crate) fn suspend_current_and_run_other_task() -> Result<()> {
    process::PROCESSOR.suspend_current_and_run_other_task()
}

/// Exit current task and run other runable task
pub(crate) fn exit_current_and_run_other_task(exit_code: i32) -> Result<()> {
    process::PROCESSOR.exit_current_and_run_other_task(exit_code)
}

#[inline(always)]
pub(crate) fn init() {
    control::init_pid_allocator();
    process::add_init_proc();
}

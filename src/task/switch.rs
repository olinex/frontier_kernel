// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods
use core::arch::global_asm;

// use self mods
use super::context::TaskContext;

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        global_asm!(include_str!("../assembly/riscv64/switch.asm"));
    } else {
        compile_error!("Unknown target_arch to load switch.asm from ./assembly");
    }
}

extern "C" {
    /// Assembly function whitch will save current task's ra/sp/callee saved registers.
    /// For switching to other task, we must restore the registers by the task we want to run in the next time.
    pub fn _fn_switch_task(
        next_task_ctx_ptr: *const TaskContext,
        current_task_ctx_ptr: *mut TaskContext,
    );

    /// Assembly function whitch will just force restore the registers and run the next task
    pub fn _fn_run_first_task(
        next_task_ctx_ptr: *const TaskContext
    );
}

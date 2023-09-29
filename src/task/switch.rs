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
    pub fn _fn_switch_task(
        current_task_cx_ptr: *mut TaskContext,
        next_task_cx_ptr: *const TaskContext,
    );
}

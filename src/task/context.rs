// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods

// use self mods
use crate::trap::handler::trap_return;

/// The context of the task, which contains the important meta information.
/// This context will be used in every task's virtual address space.
/// Each time we switch current task to other runable task,
/// we must save the return address/user stack pointer and other callee saved registers
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TaskContext {
    // the return address which will be used after switching to the current task
    ra: usize,
    // the stack pointer which the task is currently using
    sp: usize,
    // callee save registers
    csrs: [usize; 12],
}

impl TaskContext {

    /// Create a new empty task context
    pub fn empty() -> Self {
        Self {
            ra: 0,
            sp: 0,
            csrs: [0; 12],
        }
    }

    /// Make the [crate::trap::handler::trap_return] as the return address after switching to the other
    pub fn goto_trap_return(&mut self, kernel_stack_ctx_va: usize) {
        self.ra = trap_return as usize;
        self.sp = kernel_stack_ctx_va;
    }
}

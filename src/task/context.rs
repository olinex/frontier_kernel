// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods

// use self mods
use crate::trap::handler::trap_return;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TaskContext {
    ra: usize,
    sp: usize,
    // callee save registers
    csrs: [usize; 12],
}

impl TaskContext {
    pub fn new() -> Self {
        Self {
            ra: 0,
            sp: 0,
            csrs: [0; 12],
        }
    }

    pub fn goto_trap_return(&mut self, kernel_stack_ctx_va: usize) {
        self.ra = trap_return as usize;
        self.sp = kernel_stack_ctx_va;
    }
}

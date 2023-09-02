// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods

// use self mods

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TaskContext {
    ra: usize,
    sp: usize,
    // callee save registers
    srs: [usize; 12],
}

impl TaskContext {
    pub fn new() -> Self {
        Self {
            ra: 0,
            sp: 0,
            srs: [0; 12],
        }
    }

    pub fn goto_restore(&mut self, kernel_stack_ctx_ptr: usize) {
        extern "C" {
            fn _fn_restore_all_registers_after_trap();
        }
        self.ra = _fn_restore_all_registers_after_trap as usize;
        self.sp = kernel_stack_ctx_ptr;
    }
}

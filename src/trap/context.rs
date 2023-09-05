// @author:    olinex
// @time:      2023/03/16

// self mods

// use other mods
use cfg_if::cfg_if;

// use self mods
use crate::configs::MAX_TASK_NUM;
use crate::memory::{stack, StackTr};

cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        use riscv::register::sstatus::{self, Sstatus, SPP};

        #[repr(C)]
        #[derive(Debug)]
        pub struct TrapContext {
            // WARNING: could not change the ordering of the fields in this structure,
            // because the context instance might be initialized by assembly code in the assembly/trap.asm

            // general purpose registers
            pub x: [usize; 32],
            // supervisor status register
            pub sstatus: Sstatus,
            // supervisor exception program counter
            pub sepc: usize,
        }

        impl TrapContext {
            // write value to x2 register (sp)
            // @sp: the stack pointer memory address
            pub fn set_sp(&mut self, sp: usize) {
                self.x[2] = sp;
            }

            // init app context
            // @entry: application code entry point memory address
            // @sp:  the stack pointer memory address
            pub fn create_app_init_context(entry: usize, sp: usize) -> Self {
                // read sstatus register value
                let mut sstatus = sstatus::read();
                // for app context, the supervisor previous privilege mode muse be user
                sstatus.set_spp(SPP::User);
                let mut ctx = Self {
                    x: [0; 32],
                    sstatus,
                    sepc: entry,
                };
                // app's user stack pointer
                ctx.set_sp(sp);
                ctx
            }
        }
    } else {
        compile_error!("Unknown target_arch to implying TrapContext");
    }
}

impl stack::KernelStack {
    pub fn push_context(&self, cx: TrapContext) -> &'static mut TrapContext {
        let cx_ptr = (self.get_top() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            *cx_ptr = cx;
            cx_ptr.as_mut().unwrap()
        }
    }
}

pub static KERNEL_STACK: [stack::KernelStack; MAX_TASK_NUM] =
    [stack::KernelStack::new(); MAX_TASK_NUM];

pub static USER_STACK: [stack::UserStack; MAX_TASK_NUM] = [stack::UserStack::new(); MAX_TASK_NUM];

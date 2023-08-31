// @author:    olinex
// @time:      2023/03/16

// self mods

// use other mods
use riscv::register::sstatus::{self, Sstatus, SPP};

// use self mods

#[repr(C)]
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
        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry,
        };
        // app's user stack pointer
        cx.set_sp(sp);
        cx
    }
}

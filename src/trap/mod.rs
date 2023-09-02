// @author:    olinex
// @time:      2023/03/17

// self mods
pub mod context;
pub mod handler;

// use other mods
use cfg_if::cfg_if;
use core::arch::global_asm;
use riscv::register::{mtvec::TrapMode, stvec};

// use self mods

// load asemble trap entry code
cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        global_asm!(include_str!("../assembly/riscv64/trap.asm"));
    } else {
        compile_error!("Unkown target_arch to load entry.asm from ./assembly");
    }
}

// init the supervisor trap vector base address register(stvec)'s value,
// which was the address of the symbol '_fn_save_all_registers_before_trap'
// this symbol was point to some assembly code that does two main things:
// 1 save all registers
// 2 call trap_handler and pass user stack
pub fn init() {
    extern "C" {
        fn _fn_save_all_registers_before_trap();
    }
    unsafe {
        stvec::write(
            _fn_save_all_registers_before_trap as usize,
            TrapMode::Direct,
        )
    }
}

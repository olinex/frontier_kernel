// @author:    olinex
// @time:      2023/03/17

// self mods
pub mod context;
pub mod handler;

// use other mods
use crate::sbi::*;
use cfg_if::cfg_if;
use core::arch::global_asm;

// use self mods

// load asemble trap entry code
cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        use crate::lang::timer;
        use riscv::register::{mtvec::TrapMode, stvec};
        global_asm!(include_str!("../assembly/riscv64/trap.asm"));
        // init the supervisor trap vector base address register(stvec)'s value,
        // which was the address of the symbol '_fn_save_all_registers_before_trap'
        // this symbol was point to some assembly code that does two main things:
        // 1 save all registers
        // 2 call trap_handler and pass user stack
        fn init_trap_vector() {
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

        // enable the time interrput and the first timer trigger
        // when system was trap with timer interrupt, it will set other trigger by itself
        fn init_timer_interrupt() {
            unsafe { SBI::set_stimer() };
            timer::set_next_trigger();
        }

        pub fn init() {
            init_trap_vector();
            init_timer_interrupt();
        }
    } else {
        compile_error!("Unknown target_arch to load entry.asm from ./assembly");
    }
}

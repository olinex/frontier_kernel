// @author:    olinex
// @time:      2023/03/09
#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(custom_test_frameworks)]
#![test_runner(lang::test::test_runner)]
#![reexport_test_harness_main = "test_main"]

// self mods

// use other mods
use cfg_if::cfg_if;
use core::arch::global_asm;

// use self mods
#[macro_use]
mod feature;
mod boards;
mod configs;
mod error;
mod lang;
mod memory;
mod syscall;
mod task;
mod trap;

// re export commonly used modules or functions
mod prelude {
    pub use log::*;
    pub use crate::error::*;
    pub use crate::{print, println};
}

// load assembly file and do init
cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        global_asm!(include_str!("./assembly/riscv64/entry.asm"));
    } else {
        compile_error!("Unkown target_arch to load entry.asm from ./assembly");
    }
}

// will be called in [`./assembly/riscv64/entry.asm`]
// for avoid rust main entrypoint symbol be confused by compiler
cfg_if! {
    if #[cfg(test)] {
        #[no_mangle]
        fn main() -> () {
            // for testing in qemu
            test_main()
        }
    } else {
        #[no_mangle]
        fn main() -> ! {
            run()
        }
    }
}

#[inline]
pub fn run() -> ! {
    memory::clear_bss();
    lang::logger::init();
    trap::init();
    task::init();
    task::run();
}

// @author:    olinex
// @time:      2023/03/09
#![no_std]
#![no_main]
#![feature(int_roundings)]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]
#![feature(custom_test_frameworks)]
#![feature(slice_from_ptr_range)]
#![test_runner(lang::test::test_runner)]
#![reexport_test_harness_main = "test_main"]

// extern other crate
#[macro_use]
extern crate log;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate cfg_if;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate alloc;

extern crate bit_field;
extern crate riscv;

// self mods

// use other mods
use core::arch::global_asm;

// use self mods
mod configs;
mod constant;
mod lang;
mod memory;
mod sbi;
mod syscall;
mod task;
mod trap;

// re export commonly used modules or functions
mod prelude {
    pub use crate::lang::error::*;
    pub use crate::{print, println};
}

// load assembly file and do init
cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        global_asm!(include_str!("./assembly/riscv64/entry.asm"));
    } else {
        compile_error!("Unknown target_arch to load entry.asm from ./assembly");
    }
}

// will be called in [`./assembly/riscv64/entry.asm`]
// for avoid rust main entrypoint symbol be confused by compiler
cfg_if! {
    if #[cfg(not(test))] {
        // for testing in qemu
        #[no_mangle]
        fn main() -> ! {
            init();
            task::run();
        }
    } else {
        #[no_mangle]
        fn main() -> () {
            init();
            test_main();
        }
    }
}

#[inline(always)]
fn init() {
    // clear bss must be the first thing to be done
    memory::clear_bss();
    // make logger marcos enable
    lang::logger::init();
    // make kernel memory heap and page table enable, initialize kernel space
    memory::init();
    // make trap handler enable
    trap::init();
    // make process enable
    task::init();
}

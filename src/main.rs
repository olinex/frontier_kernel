// @author:    olinex
// @time:      2023/03/09
#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::lang::test::test_runner)]
#![reexport_test_harness_main = "test_main"]

// self mods
#[macro_use]
mod feature;
mod boards;
mod configs;
mod lang;
mod loader;
mod memory;
mod syscall;
mod trap;

// use other mods
use core::arch::global_asm;

// use self mods

// load assembly file and do init
global_asm!(include_str!("./assembly/entry.asm"));

// for avoid rust main entrypoint symbol be confused by compiler
#[no_mangle]
fn main() -> ! {
    // for testing in qemu
    #[cfg(test)]
    test_main();
    binary_main()
}

#[inline]
fn binary_main() -> ! {
    memory::clear_bss();
    trap::init();
    loader::init();
    loader::run_next_app();
}

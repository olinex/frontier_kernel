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
mod memory;
mod syscall;
mod trap;
mod task;

// use other mods
use cfg_if::cfg_if;
use core::arch::global_asm;

// use self mods

// load assembly file and do init
cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        global_asm!(include_str!("./assembly/riscv64/entry.asm"));
    } else {
        compile_error!("Unkown target_arch to load entry.asm from ./assembly");
    }
}

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
    task::init();
    task::run();
}

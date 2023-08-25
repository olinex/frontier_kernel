// @author:    olinex
// @time:      2023/03/09
#![no_std]
#![no_main]
#![feature(panic_info_message)]

// self mods
#[macro_use]
mod feature;
mod memory;
mod language;
mod batch;
mod boards;
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
    memory::clear_bss();
    trap::init();
    batch::init();
    batch::run_next_app();
}

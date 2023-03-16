// @author:    olinex
// @time:      2023/03/09
#![no_std]
#![no_main]
#![feature(panic_info_message)]

// self mods
#[macro_use]
mod language;
mod sbi;

// use other mods
use core::arch::global_asm;

// use self mods

// load assembly file and do init
global_asm!(include_str!("./assemble/entry.asm"));

// for avoid rust main entrypoint symbol be confused by compiler
#[no_mangle]
fn main() -> ! {
    clear_bss();
    println!("Hello, world!");
    panic!("Shutdown machine!");
}

// init bss section to zero is very import when kernel was ready
fn clear_bss() {
    extern "C" {
        // load bss start address by symbol name
        fn start_bss();
        // load bss end address by symbol name
        fn end_bss();
    }
    // force set all byte to zero
    (start_bss as usize..end_bss as usize)
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

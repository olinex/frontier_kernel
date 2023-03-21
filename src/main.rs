// @author:    olinex
// @time:      2023/03/09
#![no_std]
#![no_main]
#![feature(panic_info_message)]

// self mods
#[macro_use]
mod language;
mod batch;
mod boards;
mod sbi;
mod syscall;
mod trap;

// use other mods
use core::arch::global_asm;

// use self mods

// load assembly file and do init
global_asm!(include_str!("./assemble/entry.asm"));

// for avoid rust main entrypoint symbol be confused by compiler
#[no_mangle]
fn main() -> ! {
    clear_bss();
    trap::init();
    batch::init();
    batch::run_next_app();
}

// init bss section to zero is very import when kernel was ready
#[inline]
fn clear_bss() {
    extern "C" {
        // load bss start address by symbol name
        fn _addr_start_bss();
        // load bss end address by symbol name
        fn _addr_end_bss();
    }
    // force set all byte to zero
    (_addr_start_bss as usize.._addr_end_bss as usize)
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

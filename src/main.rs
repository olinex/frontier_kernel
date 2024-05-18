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
mod configs;
mod drivers;
mod fs;
mod lang;
mod memory;
mod sbi;
mod syscall;
mod task;
mod trap;

// use other mods
use core::arch::global_asm;

// use self mods

// re export commonly used modules or functions
mod prelude {
    pub(crate) use crate::lang::error::*;
    pub(crate) use crate::{print, println};
}

// load assembly file and do init
cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        global_asm!(include_str!("./assembly/riscv64/entry.asm"));
    } else {
        compile_error!("Unknown target_arch to load entry.asm from ./assembly");
    }
}

/// Initially make bss section to zero is very import when kernel was initializing.
/// It must be called at the very first when kernel was booting.
/// This method must be compiling as inline code, 
/// because the boot stack was containing in the bss section
/// and all the boot stack will be set to zero.
#[inline(always)]
pub(crate) fn clear_bss() {
    // force set all byte to zero
    (configs::_addr_bss_start as usize..configs::_addr_bss_end as usize)
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

#[inline(always)]
fn init() {
    // make logger marcos enable
    lang::logger::init();
    // make kernel memory heap and page table enable, initialize kernel space
    memory::init();
    // make process enable
    task::init();
    // make trap handler enable
    trap::init();
}

// will be called in [`./assembly/riscv64/entry.asm`]
// for avoid rust main entrypoint symbol be confused by compiler
cfg_if! {
    if #[cfg(not(test))] {
        // for testing in qemu
        #[no_mangle]
        #[inline(always)]
        fn main() -> ! {
            // clear bss must be the first thing to be done
            clear_bss();
            init();
            task::run();
        }
    } else {
        #[no_mangle]
        #[inline(always)]
        fn main() -> () {
            // clear bss must be the first thing to be done
            clear_bss();
            init();
            test_main();
        }
    }
}

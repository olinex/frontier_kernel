// @author:    olinex
// @time:      2023/03/09
#![no_std]
#![no_main]
#![feature(thin_box)]
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
extern crate fdt;
extern crate riscv;

// self mods
mod configs;
mod drivers;
mod fs;
mod lang;
mod memory;
mod sbi;
mod sync;
mod syscall;
mod task;
mod trap;

// use other mods
use core::arch::global_asm;
use core::sync::atomic::{AtomicBool, Ordering};

// use self mods

// re export commonly used modules or functions
mod prelude {
    pub(crate) use crate::lang::error::*;

    #[allow(unused_imports)]
    pub(crate) use crate::{print, println};
}

// load assembly file and do init
cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
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
    // init heap/frames/kernel space
    memory::init();
    // make process enable
    task::init();
    // release initial lock
    release();
}

// will be called in [`./assembly/riscv64/entry.asm`]
// for avoid rust main entrypoint symbol be confused by compiler
#[no_mangle]
#[inline(always)]
fn main(hartid: usize, _: usize) -> () {
    if hartid == 0 {
        // clear bss must be the first thing to be done
        clear_bss();
        init();
        // release initial lock after initialize
        release();
    }
    // wait initial by hart zero was ready
    wait();
    // init trap handler
    trap::init();
    cfg_if! {
        if #[cfg(not(test))] {
            task::run();
        } else {
            test_main();
        }
    };
}

/// Make current hart to waitting kernel initialzation by hart zero.
#[inline(always)]
fn wait() {
    while INITIALIZED
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        // TODO: we should make hart sleep a bit instead of busy loop.
        continue;
    }
}

/// Release the global lock of kernel initialzation. Only can by call by hart zero.
#[inline(always)]
fn release() {
    INITIALIZED.store(false, Ordering::Relaxed);
}

/// The global lock of kernel initialization,
/// which will be released by hart zero.
static INITIALIZED: AtomicBool = AtomicBool::new(true);

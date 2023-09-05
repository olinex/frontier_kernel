// @author:    olinex
// @time:      2023/09/04

// self mods

// use other mods
use core::arch::asm;

// use self mods
use super::{SBIApi, SBI};

impl SBIApi for SBI {
    #[inline(always)]
    fn console_putchar(c: u8) {
        sbi::legacy::console_putchar(c)
    }

    #[inline(always)]
    fn shutdown() -> ! {
        sbi::legacy::shutdown()
    }

    #[inline(always)]
    fn set_timer(timer: usize) {
        sbi::legacy::set_timer(timer as u64);
    }

    #[inline(always)]
    fn get_time() -> usize {
        riscv::register::time::read()
    }

    #[inline(always)]
    unsafe fn fence_i() {
        asm!("fence.i");
    }

    #[inline(always)]
    unsafe fn set_stimer() {
        riscv::register::sie::set_stimer();
    }
}

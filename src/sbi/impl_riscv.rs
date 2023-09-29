// @author:    olinex
// @time:      2023/09/04

// self mods

// use other mods
use core::arch::asm;
use riscv::register::{satp, sie, stvec, time};
use sbi::legacy;

// use self mods
use super::{SBIApi, SBI};

impl SBIApi for SBI {
    #[inline(always)]
    fn console_putchar(c: u8) {
        legacy::console_putchar(c)
    }

    #[inline(always)]
    fn shutdown() -> ! {
        legacy::shutdown()
    }

    #[inline(always)]
    fn set_timer(timer: usize) {
        legacy::set_timer(timer as u64);
    }

    #[inline(always)]
    fn get_time() -> usize {
        time::read()
    }

    #[inline(always)]
    unsafe fn set_direct_trap_vector(addr: usize) {
        stvec::write(addr, stvec::TrapMode::Direct)
    }

    #[inline(always)]
    unsafe fn fence_i() {
        asm!("fence.i");
    }

    #[inline(always)]
    unsafe fn set_stimer() {
        sie::set_stimer();
    }

    #[inline(always)]
    unsafe fn sfence_vma() {
        asm!("sfence.vma");
    }

    #[inline(always)]
    unsafe fn write_mmu_token(token: usize) {
        satp::write(token);
        asm!("sfence.vma");
    }

    #[inline(always)]
    fn read_mmu_token() -> usize {
        satp::read().bits()
    }
}

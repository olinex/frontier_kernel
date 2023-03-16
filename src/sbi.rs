#![allow(unused)]
// @author:    olinex
// @time:      2023/03/15

// self mods

// use other mods

// use self mods
use core::arch::asm;

enum SBIWhich {
    SetTimer = 0,
    PutcharToConsole = 1,
    GetcharFromConsole = 2,
    ClearIpi = 3,
    SendIpi = 4,
    RemoteFenceI = 5,
    RemoteSfenceVma = 6,
    RemoteSfenceVmaAsid = 7,
    Shutdown = 8,
}

#[inline(always)]
fn sbi_call(which: SBIWhich, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let mut ret;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") arg0 => ret,
            in("x11") arg1,
            in("x12") arg2,
            in("x17") which as usize,
        );
    }
    ret
}

#[inline(always)]
pub fn set_timer(c: usize) {
    sbi_call(SBIWhich::SetTimer, c, 0, 0);
}

#[inline(always)]
pub fn put_char_to_console(c: usize) {
    sbi_call(SBIWhich::PutcharToConsole, c, 0, 0);
}

#[inline(always)]
pub fn get_char_from_console() -> usize {
    sbi_call(SBIWhich::GetcharFromConsole, 0, 0, 0)
}

#[inline(always)]
pub fn clear_ipi() {
    sbi_call(SBIWhich::ClearIpi, 0, 0, 0);
}

#[inline(always)]
pub fn send_ipi() {
    sbi_call(SBIWhich::SendIpi, 0, 0, 0);
}

#[inline(always)]
pub fn remote_fence_vma(vma: usize) {
    sbi_call(SBIWhich::RemoteFenceI, vma, 0, 0);
}

#[inline(always)]
pub fn remote_fence_vma_asid(vma: usize, asid: usize) {
    sbi_call(SBIWhich::RemoteFenceI, vma, asid, 0);
}

#[inline(always)]
pub fn shutdown() {
    sbi_call(SBIWhich::Shutdown, 0, 0, 0);
}

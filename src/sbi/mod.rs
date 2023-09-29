// @author:    olinex
// @time:      2023/09/04

// self mods

// use other mods

// use self mods

pub trait SBIApi {
    fn console_putchar(c: u8);
    fn shutdown() -> !;
    fn set_timer(timer: usize);
    fn get_time() -> usize;
    unsafe fn set_direct_trap_vector(addr: usize);
    unsafe fn fence_i();
    unsafe fn set_stimer();
    unsafe fn sfence_vma();
    unsafe fn write_mmu_token(bits: usize);
    fn read_mmu_token() -> usize;
}

pub struct SBI;

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        pub mod impl_riscv;
    } else {
        compile_error!("Unknown target_arch to implying sbi")
    }
}

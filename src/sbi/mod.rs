// @author:    olinex
// @time:      2023/09/04

// self mods

// use other mods
use cfg_if::cfg_if;

// use self mods

pub trait SBIApi {
    fn console_putchar(c: u8);
    fn shutdown() -> !;
    fn set_timer(timer: usize);
    fn get_time() -> usize;
    unsafe fn fence_i();
    unsafe fn set_stimer();
}

pub struct SBI;

cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        pub mod riscv_;
    } else {
        compile_error!("Unknown target_arch to implying sbi")
    }
}

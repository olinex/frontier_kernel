// @author:    olinex
// @time:      2023/03/17

// self mods

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        pub(crate) mod handler_riscv;
        pub(crate) use handler_riscv as handler;
        pub(crate) mod context_riscv;
        pub(crate) use context_riscv as context;
    }
}

// use other mods

// use self mods

#[inline(always)]
pub(crate) fn init() {
    handler::set_kernel_trap_entry();
    handler::init_timer_interrupt();
}

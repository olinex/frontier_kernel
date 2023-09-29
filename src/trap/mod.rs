// @author:    olinex
// @time:      2023/03/17

// self mods

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        pub mod handler_riscv;
        pub use handler_riscv as handler;
        pub mod context_riscv;
        pub use context_riscv as context;
    }
}

// use other mods

// use self mods

pub fn init() {
    handler::set_kernel_trap_entry();
    handler::init_timer_interrupt();
}

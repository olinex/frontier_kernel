// @author:    olinex
// @time:      2023/03/17

// self mods
cfg_if! {
    if #[cfg(feature = "board_qemu")] {
        pub mod board_qemu;
        pub use board_qemu as board;
    }
}

// use other mods

// use self mods


pub trait Exit {
    fn exit_success(&self) -> !;
    fn exit_failure(&self) -> !;
    fn exit_reset(&self) -> !;
    fn exit_other(&self, code: usize) -> !;
}



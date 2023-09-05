// @author:    olinex
// @time:      2023/08/26

// self mods

// use other mods
use cfg_if::cfg_if;
use log::Level;

// use self mods

pub const MAX_TASK_NUM: usize = 16;
pub const USER_STACK_SIZE: usize = 1024 * 8;
pub const KERNEL_STACK_SIZE: usize = 1024 * 4;
pub const APP_BASE_ADDRESS: usize = 0x80_400_000;
pub const APP_SIZE_LIMIT: usize = 0x20_000;
pub const TICKS_PER_SEC: usize = 100;

// the frequency of the board clock in Hz
cfg_if! {
    if #[cfg(feature = "board_qemu")] {
        pub const BOARD_CLOCK_FREQ: usize = 12_500_000;
    } else {
        compile_error!("Unknown feature for board");
    }

}


pub const LOG_LEVEL: Level = Level::Info;

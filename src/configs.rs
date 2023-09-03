// @author:    olinex
// @time:      2023/08/26

// self mods

// use other mods
use log::Level;

// use self mods

pub const MAX_APP_NUM: usize = 16;
pub const USER_STACK_SIZE: usize = 1024 * 8;
pub const KERNEL_STACK_SIZE: usize = 1024 * 4;
pub const APP_BASE_ADDRESS: usize = 0x80400000;
pub const APP_SIZE_LIMIT: usize = 0x20000;
pub const LOG_LEVEL: Level = Level::Info;

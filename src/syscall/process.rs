// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods

// use self mods
use crate::prelude::*;
use crate::task::exit_current_and_run_other_task;

// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    info!("Application exited with code {}", exit_code);
    exit_current_and_run_other_task();
    panic!("Unreachable in sys_exit!");
}

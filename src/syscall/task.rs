// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods

// use self mods
use crate::task;

// yield to other task
pub fn sys_yield() -> isize {
    task::suspend_current_and_run_other_task();
    0
}

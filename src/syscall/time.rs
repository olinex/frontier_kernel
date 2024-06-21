// @author:    olinex
// @time:      2023/09/05

// self mods

// use other mods

// use self mods
use crate::lang::timer;
use crate::prelude::*;
use crate::task::sleep_current_and_run_other_task;

/// Get the current timer value as microseconds,
/// which is the time duration from the moment when cpu reset to the current moment
#[inline(always)]
pub(crate) fn sys_get_time() -> Result<isize> {
    Ok(timer::get_timer_us() as isize)
}

#[inline(always)]
pub(crate) fn sys_sleep(us: usize) -> Result<isize> {
    sleep_current_and_run_other_task(us)?;
    Ok(0)
}
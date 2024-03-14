// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods

// use self mods
use crate::prelude::*;
use crate::task;

/// Yield to other task, current task will be suspended
/// 
/// # Returns
/// * Ok(0)
#[inline(always)]
pub(crate) fn sys_yield() -> Result<isize> {
    task::suspend_current_and_run_other_task()?;
    Ok(0)
}

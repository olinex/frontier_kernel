// @author:    olinex
// @time:      2023/09/05

// self mods

// use other mods

// use self mods
use crate::prelude::*;
use crate::lang::timer;

pub fn sys_get_time() -> Result<isize> {
    Ok(timer::get_time_us() as isize)
}

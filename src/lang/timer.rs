// @author:    olinex
// @time:      2023/09/04

// self mods

// use other mods

// use self mods
use crate::configs;
use crate::sbi::*;

const MICRO_PER_SEC: usize = 1_000_000;

#[inline(always)]
pub fn set_next_trigger() {
    SBI::set_timer(SBI::get_time() + (configs::BOARD_CLOCK_FREQ / configs::TICKS_PER_SEC) as usize);
}

#[inline(always)]
pub fn get_time_us() -> usize {
    SBI::get_time() / (configs::BOARD_CLOCK_FREQ / MICRO_PER_SEC) as usize
}

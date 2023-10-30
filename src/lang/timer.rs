// @author:    olinex
// @time:      2023/09/04

// self mods

// use other mods

// use self mods
use crate::configs;
use crate::sbi::*;

const MICRO_PER_SEC: usize = 1_000_000;

/// Set the timer to make cpu can be interrupted
#[inline(always)]
pub fn set_next_trigger() {
    SBI::set_timer(SBI::get_timer() + (configs::BOARD_CLOCK_FREQ / configs::TICKS_PER_SEC));
}

/// Get the current timer as microseconds.
/// Be careful, the timer microseconds isn't the timestamp from 1970-01-01T00:00:00,
/// it is the timestamp from the moment when cpu was reset:
/// * 1 seconds = 1000 milliseconds 
/// * 1 milliseconds = 1000 microseconds
#[inline(always)]
pub fn get_timer_us() -> usize {
    SBI::get_timer() * MICRO_PER_SEC / configs::BOARD_CLOCK_FREQ
}

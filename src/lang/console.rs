// @author:    olinex
// @time:      2023/03/15

// self mods

// use other mods
use core::fmt::{self, Write};

// use self mods
use crate::sbi::*;

struct Stdout;
impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            SBI::console_putchar(c as u8);
        }
        Ok(())
    }
}

// impl rust buildin print function
pub(crate) fn print(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

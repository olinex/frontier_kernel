// @author:    olinex
// @time:      2023/03/15

// self mods

// use other mods
use core::fmt::{self, Write};
use sbi;

// use self mods

struct Stdout;

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            sbi::legacy::console_putchar(c as u8);
        }
        Ok(())
    }
}

// impl rust buildin print function
pub fn print(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

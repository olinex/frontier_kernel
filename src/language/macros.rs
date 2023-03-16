// @author:    olinex
// @time:      2023/03/15

// self mods

// use other mods

// use self mods

#[macro_export]
macro_rules! print {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::language::console::print(format_args!($fmt $(, $($arg)+)?));
    }
}

#[macro_export]
macro_rules! println {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::language::console::print(format_args!(concat!($fmt, "\n") $(, $($arg)+)?));
    }
}

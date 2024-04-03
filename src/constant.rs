// @author:    olinex
// @time:      2023/10/11

// self mods

// use other mods

// use self mods

pub mod keycode {
    pub const LINE_FEED: u8 = 0x0a;
    pub const CARRIAGE_RETURN: u8 = 0x0d;
    pub const DELETE: u8 = 0x7f;
    pub const BACK_SPACE: u8 = 0x08;
    pub const CHANGE_MODE: u8 = 0xe0;
    pub const LEFT_ARROW: u8 = 0x25;
    pub const UP_ARROW: u8 = 0x26;
    pub const RIGHT_ARROW: u8 = 0x27;
    pub const DOWN_ARROW: u8 = 0x28;
}

pub mod charater {
    pub const SPACE: char = ' ';
    pub const NULL: char = '\0';
}

pub mod shortcut {
    pub const NULL: &'static str = "\0"; 
}

pub mod descriptor {
    pub const STDIN: usize = 0;
    pub const STDOUT: usize = 1;
}

// @author:    olinex
// @time:      2023/08/10

// self mods

// use other mods
use cfg_if::cfg_if;
use core::convert::From;

// use self mods

pub enum FeatureWord {
    Word32 = 32,
    Word64 = 64,
}

impl From<usize> for FeatureWord {
    fn from(value: usize) -> Self {
        match value {
            32 => FeatureWord::Word32,
            64 => FeatureWord::Word64,
            _ => panic!("Invalid feature word value: {}", value),
        }
    }
}

pub enum FeatureBoard {
    Qemu,
}

cfg_if! {
    if #[cfg(target_pointer_width = "32")] {
        pub const FEATURE_WORD: FeatureWord = FeatureWord::Word32;
    } else if #[cfg(target_pointer_width = "64")] {
        pub const FEATURE_WORD: FeatureWord = FeatureWord::Word64;
    } else {
        compile_error!("Unknown feature target_pointer_width")
    }
}

cfg_if! {
    if #[cfg(feature = "board_qemu")] {
        pub const FEATURE_BOARD: FeatureBoard = FeatureBoard::Qemu;
    } else {
        compile_error!("Unknown board feature")
    }
}



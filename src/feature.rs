// @author:    olinex
// @time:      2023/08/10

// self mods

// use other mods
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
            _ => panic!("invalid feature word value: {}", value),
        }
    }
}

// feature word choice
#[cfg(target_pointer_width = "32")]
pub const FEATURE_WORD: FeatureWord = FeatureWord::Word32;
#[cfg(target_pointer_width = "64")]
pub const FEATURE_WORD: FeatureWord = FeatureWord::Word64;

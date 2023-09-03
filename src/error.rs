// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods
use thiserror_no_std::Error;
use enum_group::EnumGroup;

// use self mods

#[derive(Error, EnumGroup)]
pub enum KernelError {
    #[groups(syscall)]
    #[error("[kernel] Invalid syscall id: {0}")]
    InvaidSyscallId(usize),

    #[groups(syscall)]
    #[error("[kernel] Invalid File descriptor label: {0}")]
    InvalidFileDescriptor(usize),

    #[error("[kernel] Unknown error catched")]
    Unknown,
}

pub type Result<T> = core::result::Result<T, KernelError>;

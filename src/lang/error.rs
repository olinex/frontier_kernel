// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods
use enum_group::EnumGroup;
use thiserror_no_std::Error;

// use self mods

#[derive(Error, EnumGroup, Debug)]
pub enum KernelError {
    // #[groups(language)]
    // #[error("[kernel] Index({index}) is out of range [{start}, {end})")]
    // IndexOutOfRange {
    //     index: usize,
    //     start: usize,
    //     end: usize,
    // },

    #[groups(syscall)]
    #[error("[kernel] Invalid syscall id: {0}")]
    InvaidSyscallId(usize),

    #[groups(syscall)]
    #[error("[kernel] Invalid file descriptor label: {0}")]
    InvalidFileDescriptor(usize),

    #[groups(memory)]
    #[error("[kernel] Frame exhausted")]
    FrameExhausted,

    #[groups(memory)]
    #[error("[kernel] Frame not deallocable")]
    FrameNotDeallocable(usize),

    #[groups(memory)]
    #[error("[kernel] Area [{0}, {1}) allocation failed")]
    AreaAllocationFailed(usize, usize),

    #[groups(memory)]
    #[error("[kernel] PPN {0} already mapped")]
    PPNAlreadyMapped(usize),

    #[groups(memory)]
    #[error("[kernel] PPN {0} was not mapped")]
    PPNNotMapped(usize),

    #[groups(memory)]
    #[error("[kernel] VPN {0} already mapped")]
    VPNAlreadyMapped(usize),

    #[groups(memory)]
    #[error("[kernel] VPN {0} was not mapped")]
    VPNNotMapped(usize),

    #[groups(memory)]
    #[error("[kernel] Page table get invalid permission flags: {0}")]
    InvaidPageTablePerm(usize),

    #[groups(task)]
    #[error("[kernel] Task {0} does not found")]
    TaskNotFound(usize),

    #[groups(task)]
    #[error("[kernel] Invalid headless task")]
    InvalidHeadlessTask,

    #[groups(task)]
    #[error("[kernel] Unloadable task")]
    UnloadableTask,

    #[groups(task)]
    #[error("[kernel] No runnable tasks found")]
    NoRunableTasks,

    #[groups(others)]
    #[error("[kernel] Parse elf error: {0}")]
    ParseElfError(#[from] elf::ParseError),
}

pub type Result<T> = core::result::Result<T, KernelError>;

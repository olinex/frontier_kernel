// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods
use alloc::string::String;
use enum_group::EnumGroup;
use frontier_fs::FFSError;
use thiserror_no_std::Error;

// use self mods

#[derive(Error, EnumGroup, Debug)]
pub(crate) enum KernelError {
    #[groups(base)]
    #[error("[kernel] End of Buffer")]
    EOB,

    #[groups(syscall)]
    #[error("[kernel] Invalid syscall id: {0}")]
    InvaidSyscallId(usize),

    #[groups(syscall)]
    #[error("[kernel] Invalid file descriptor label: {0}")]
    InvalidFileDescriptor(usize),

    #[groups(memory, frame)]
    #[error("[kernel] Frame exhausted")]
    FrameExhausted,

    #[groups(memory, frame)]
    #[error("[kernel] Frame not deallocable")]
    FrameNotDeallocable(usize),

    #[groups(memory, area)]
    #[error("[kernel] Area [{0}, {1}) alloc failed")]
    AreaAllocFailed(usize, usize),

    #[groups(memory, area)]
    #[error("[kernel] Area [{0}, {1}) dealloc failed")]
    AreaDeallocFailed(usize, usize),

    #[groups(memory, area)]
    #[error("[kernel] Area [{0}, {1}) does not exists")]
    AreaNotExists(usize, usize),

    #[groups(memory, area)]
    #[error("[kernel] Virtual page number {vpn} isn't in Area [{start}, {end})")]
    VPNOutOfArea {
        vpn: usize,
        start: usize,
        end: usize,
    },

    #[groups(memory, ppn)]
    #[error("[kernel] PPN {0} already mapped")]
    PPNAlreadyMapped(usize),

    #[groups(memory, ppn)]
    #[error("[kernel] PPN {0} was not mapped")]
    PPNNotMapped(usize),

    #[groups(memory, vpn)]
    #[error("[kernel] VPN {0} already mapped")]
    VPNAlreadyMapped(usize),

    #[groups(memory, vpn)]
    #[error("[kernel] VPN {0} was not mapped")]
    VPNNotMapped(usize),

    #[groups(memory, page_table)]
    #[error("[kernel] Try to allocate new page table entry from a full page mapper {0}")]
    AllocFullPageMapper(usize),

    #[groups(memory, page_table)]
    #[error("[kernel] Try to deallocate page table entry from a empty page mapper {0}")]
    DeallocEmptyPageMapper(usize),

    #[groups(memory, page_table)]
    #[error("[kernel] Page table get invalid permission flags: {0}")]
    InvaidPageTablePerm(usize),

    #[groups(task)]
    #[error("[kernel] Invalid headless task")]
    InvalidHeadlessTask,

    #[groups(task)]
    #[error("[kernel] Unloadable task")]
    UnloadableTask,

    #[groups(process)]
    #[error("[kernel] Process id exhausted")]
    PidExhausted,

    #[groups(process)]
    #[error("[kernel] Process id not deallocable")]
    PidNotDeallocable(usize),

    #[groups(process)]
    #[error("[kernel] Process have not task")]
    ProcessHaveNotTask,

    #[groups(fs)]
    #[error("[kernel] Invalid open flags {0:#x}")]
    InvalidOpenFlags(u32),

    #[groups(fs)]
    #[error("[kernel] File descriptor exhausted")]
    FileDescriptorExhausted,

    #[groups(fs)]
    #[error("[kernel] File descriptor does not exists")]
    FileDescriptorDoesNotExist(usize),

    #[groups(fs)]
    #[error("[kernel] File {0} does not exists")]
    FileDoesNotExists(String),

    #[groups(vfs)]
    #[error("Inode {0} must be readable")]
    FileMustBeReadable(u32),

    #[groups(vfs)]
    #[error("Inode {0} must be writable")]
    FileMustBeWritable(u32),

    #[groups(vfs)]
    #[error("Inode {0} must be executable")]
    FileMustBeExecutable(u32),

    #[groups(others, fs)]
    #[error("[kernel] File system error: {0}")]
    FileSystemError(#[from] FFSError),

    #[groups(others, parse, elf)]
    #[error("[kernel] Parse elf error: {0}")]
    ParseElfError(#[from] elf::ParseError),

    #[groups(others, parse, core)]
    #[error("[kernel] core error: {0}")]
    ParseStringError(#[from] alloc::string::ParseError),

    #[groups(others, parse, core)]
    #[error("[kernel] core error: {0}")]
    ParseUtf8Error(#[from] alloc::str::Utf8Error),
}

pub(crate) type Result<T> = core::result::Result<T, KernelError>;

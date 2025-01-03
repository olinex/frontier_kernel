// @author:    olinex
// @time:      2023/09/03

// self mods

// use other mods
use alloc::string::String;
use enum_group::EnumGroup;
use frontier_fs::FFSError;
use frontier_lib::{error::LibError, model::signal::Signal};
use thiserror_no_std::Error;

// use self mods

#[derive(Error, EnumGroup, Debug)]
pub(crate) enum KernelError {
    #[groups(base)]
    #[error("End of Buffer")]
    EOB,

    #[groups(base)]
    #[error("Id exhausted")]
    IdExhausted,

    #[groups(base)]
    #[error("Id not deallocable")]
    IdNotDeallocable(usize),

    #[groups(syscall)]
    #[error("Invalid syscall id: {0}")]
    InvaidSyscallId(usize),

    #[groups(syscall)]
    #[error("Command line arguments oversize")]
    OversizeArgs,

    #[groups(memory, frame)]
    #[error("Frame exhausted")]
    FrameExhausted,

    #[groups(memory, frame)]
    #[error("Frame not deallocable")]
    FrameNotDeallocable(usize),

    #[groups(memory, area)]
    #[error("Area [{0}, {1}) alloc failed")]
    AreaAllocFailed(usize, usize),

    #[groups(memory, area)]
    #[error("Area [{0}, {1}) dealloc failed")]
    AreaDeallocFailed(usize, usize),

    #[groups(memory, area)]
    #[error("Area [{0}, {1}) does not exists")]
    AreaNotExists(usize, usize),

    #[groups(memory, area)]
    #[error("Virtual page number {vpn} isn't in Area [{start}, {end})")]
    VPNOutOfArea {
        vpn: usize,
        start: usize,
        end: usize,
    },

    #[groups(memory, ppn)]
    #[error("PPN {0} already mapped")]
    PPNAlreadyMapped(usize),

    #[groups(memory, ppn)]
    #[error("PPN {0} was not mapped")]
    PPNNotMapped(usize),

    #[groups(memory, vpn)]
    #[error("VPN {0} already mapped")]
    VPNAlreadyMapped(usize),

    #[groups(memory, vpn)]
    #[error("VPN {0} was not mapped")]
    VPNNotMapped(usize),

    #[groups(memory, page_table)]
    #[error("Try to allocate new page table entry from a full page mapper {0}")]
    AllocFullPageMapper(usize),

    #[groups(memory, page_table)]
    #[error("Try to deallocate page table entry from a empty page mapper {0}")]
    DeallocEmptyPageMapper(usize),

    #[groups(memory, page_table)]
    #[error("Page table get invalid permission flags: {0}")]
    InvaidPageTablePerm(usize),

    #[groups(task)]
    #[error("Invalid headless task")]
    InvalidHeadlessTask,

    #[groups(task)]
    #[error("Unloadable task")]
    UnloadableTask,

    #[groups(process)]
    #[error("Process have not task")]
    ProcessHaveNotTask,

    #[groups(process)]
    #[error("Fork with no root task {0}")]
    ForkWithNoRootTask(usize),

    #[groups(process)]
    #[error("Exec other code in multi task process {0}")]
    ExecWithMultiTasks(usize),

    #[groups(fs)]
    #[error("Invalid open flags {0:#x}")]
    InvalidOpenFlags(u32),

    #[groups(fs)]
    #[error("File descriptor exhausted")]
    FileDescriptorExhausted,

    #[groups(fs)]
    #[error("File descriptor {0} does not exists")]
    FileDescriptorDoesNotExist(usize),

    #[groups(fs)]
    #[error("File {0} does not exists")]
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

    #[groups(signal)]
    #[error("Duplicate signal {0:?} as setting")]
    DuplicateSignal(Signal),

    #[groups(sync)]
    #[error("Double lock mutex")]
    DoubleLockMutex,

    #[groups(sync)]
    #[error("Double unlock mutex")]
    DoubleUnlockMutex,

    #[groups(sync)]
    #[error("Mutex exhausted")]
    MutexExhausted,

    #[groups(sync)]
    #[error("Mutex {0} does not exists")]
    MutexDoesNotExist(usize),

    #[groups(sync)]
    #[error("Semaphore exhausted")]
    SemaphoreExhausted,

    #[groups(sync)]
    #[error("Semaphore {0} does not exists")]
    SemaphoreDoesNotExist(usize),

    #[groups(sync)]
    #[error("Condvar exhausted")]
    CondvarExhausted,

    #[groups(sync)]
    #[error("Condvar {0} does not exists")]
    CondvarDoesNotExist(usize),

    #[groups(others, lib)]
    #[error("Lib error: {0}")]
    LibError(#[from] LibError),

    #[groups(others, driver, virtio)]
    #[error("Driver virtio error: {0}")]
    DriverVirtIOError(#[from] virtio_drivers::Error),

    #[groups(other, driver, mmio)]
    #[error("Driver mmio error: {0}")]
    DriverMMIOError(#[from] virtio_drivers::transport::mmio::MmioError),

    #[groups(others, fs)]
    #[error("File system error: {0}")]
    FileSystemError(#[from] FFSError),

    #[groups(others, parse, elf)]
    #[error("Parse elf error: {0}")]
    ParseElfError(#[from] elf::ParseError),

    #[groups(others, parse, core)]
    #[error("core error: {0}")]
    ParseStringError(#[from] alloc::string::ParseError),

    #[groups(others, parse, core)]
    #[error("core error: {0}")]
    ParseUtf8Error(#[from] alloc::str::Utf8Error),

    #[groups(others, parse, fdt)]
    #[error("fdt error: {0}")]
    ParseFdtError(#[from] fdt::FdtError)
}

pub(crate) type Result<T> = core::result::Result<T, KernelError>;

// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods
use alloc::sync::Arc;
use frontier_fs::OpenFlags;

// use self mods
use crate::configs::PIPE_RING_BUFFER_LENGTH;
use crate::fs::inode::ROOT_INODE;
use crate::fs::pipe::Pipe;
use crate::prelude::*;
use crate::task::*;

/// Open a file and return the file descriptor.
/// If the descriptor is less than zero, it means there was an error
///
/// - Arguments
///     - path_ptr: The pointer address that path to the file, it must end with \0 char
///     - flags: the unsigned value of the open flags
///
/// - Returns
///     -  > -1: file descriptor
///     - -1: file does not exists
///
/// - Errors
///     - InvalidOpenFlags(flags)
///     - ProcessHaveNotTask
///     - VPNNotMapped(vpn)
///     - FileSystemError
///         - InodeMustBeDirectory(bitmap index)
///         - DataOutOfBounds
///         - NoDroptableBlockCache
///         - RawDeviceError(error code)
///         - DuplicatedFname(name, inode bitmap index)
///         - BitmapExhausted(start_block_id)
///         - BitmapIndexDeallocated(bitmap_index)
///         - RawDeviceError(error code)
///         - FileMustBeReadable(bitmap index)
#[inline(always)]
pub(crate) fn sys_open(path_ptr: *const u8, flags: u32) -> Result<isize> {
    let flags = OpenFlags::from_bits(flags).ok_or(KernelError::InvalidOpenFlags(flags))?;
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut process_inner = process.inner_exclusive_access();
    let current_space = process_inner.space();
    let path = current_space.translated_string(path_ptr)?;
    match ROOT_INODE.find(&path, flags) {
        Ok(file) => Ok(process_inner.allc_fd(file)? as isize),
        Err(KernelError::FileDoesNotExists(_)) => Ok(-1),
        Err(other) => Err(other),
    }
}

/// Close a file and return the status code.
///
/// - Arguments
///     - fd: the file descriptor
///
/// - Return
///     - 0: success
///     - -1: file descriptor does not exists
/// 
/// - Errors
///     - ProcessHaveNotTask
#[inline(always)]
pub(crate) fn sys_close(fd: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut process_inner = process.inner_exclusive_access();
    if let Ok(_) = process_inner.dealloc_fd(fd) {
        Ok(0)
    } else {
        Ok(-1)
    }
}

/// Write a &str to the IO device.
/// Different IO devices are distinguished by different unique description ID numbers
///
/// - Arguments
///     - fd: the discriptor for different IO devices
///     - buffer_ptr: the pointer to the buffer to write
///     - len: the length of the buffer
///
/// - Returns
///     - writed length
/// 
/// - Errors
///     - ProcessHaveNotTask
///     - VPNNotMapped(vpn)
///     - InvalidFileDescriptor(fd)
///     - FileDescriptorDoesNotExist(fd)
#[inline(always)]
pub(crate) fn sys_write(fd: usize, buffer_ptr: *const u8, len: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let porcess_inner = process.inner_access();
    let current_space = porcess_inner.space();
    let buffers = current_space.translated_byte_buffers(buffer_ptr, len)?;
    let file = porcess_inner
        .get_file(fd)
        .ok_or(KernelError::FileDescriptorDoesNotExist(fd))?;
    let file = Arc::clone(file);
    drop(porcess_inner);
    Ok(file.write(buffers)? as isize)
}

/// Read a &str from the IO device and save it to the buffer.
/// Different IO devices are distinguished by different unique description ID numbers
///
/// - Arguments
///     - fd: the discriptor for different IO devices
///     - buffer_ptr: the pointer to the buffer to write
///     - len: the length of the buffer
/// 
/// - Returns
///     - readed length
///
/// - Errors
///     - ProcessHaveNotTask
///     - FileDescriptorDoesNotExist(fd)
#[inline(always)]
pub(crate) fn sys_read(fd: usize, buffer_ptr: *mut u8, len: usize) -> Result<isize> {
    assert!(len > 0);
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let process_inner = process.inner_access();
    let current_space = process_inner.space();
    let buffers = current_space.translated_byte_buffers(buffer_ptr, len)?;
    let file = process_inner
        .get_file(fd)
        .ok_or(KernelError::FileDescriptorDoesNotExist(fd))?;
    let file = Arc::clone(file);
    drop(process_inner);
    Ok(file.read(buffers)? as isize)
}

/// Create a pipe `file` in the current task, return readable file descriptor and writable file descriptor.
/// Both them are refer to the pipe file
///
/// - Arguments
///     - read_tap_fd_ptr: the pointer to the readable pipe file reference
///     - write_tap_fd_ptr: the pointer to the writable pipe file reference
///
/// - Returns
///     - 0: success
/// 
/// - Errors
///     - ProcessHaveNotTask
///     - FileDescriptorExhausted
///     - VPNNotMapped(vpn)
#[inline(always)]
pub(crate) fn sys_pipe(read_tap_fd_ptr: *mut usize, write_tap_fd_ptr: *mut usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut process_inner = process.inner_exclusive_access();
    let read_tap = Pipe::new(PIPE_RING_BUFFER_LENGTH);
    let write_tap = read_tap.writable_fork().unwrap();
    let read_fd = process_inner.allc_fd(Arc::new(read_tap))?;
    let write_fd = process_inner.allc_fd(Arc::new(write_tap))?;
    let current_space = process_inner.space();
    let read_tap_fd = current_space.translated_refmut(read_tap_fd_ptr)?;
    let write_tap_fd = current_space.translated_refmut(write_tap_fd_ptr)?;
    *read_tap_fd = read_fd;
    *write_tap_fd = write_fd;
    Ok(0)
}

/// Based on the incoming file descriptor, the specified file is copied and saved to the context of the current task.
/// Returns a new file descriptor, pointing to a copy of the file.
///
/// - Arguments
///     - fd: file descriptor
///
/// - Returns
///     - new file descriptor
/// 
/// - Errors
///     - ProcessHaveNotTask
///     - FileDescriptorDoesNotExist(fd)
///     - FileDescriptorExhausted
#[inline(always)]
pub(crate) fn sys_dup(fd: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let process = task.process();
    let mut process_inner = process.inner_exclusive_access();
    let file = process_inner
        .get_file(fd)
        .ok_or(KernelError::FileDescriptorDoesNotExist(fd))?;
    let file = Arc::clone(file);
    let fd = process_inner.allc_fd(file)?;
    Ok(fd as isize)
}

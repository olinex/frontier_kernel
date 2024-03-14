// @author:    olinex
// @time:      2023/03/17

// self mods

use alloc::sync::Arc;
// use other mods
use frontier_fs::OpenFlags;

// use self mods
use crate::fs::inode::ROOT_INODE;
use crate::prelude::*;
use crate::task::*;

/// Open a file and return the file descriptor.
/// If the descriptor is less than zero, it means there was an error
///
/// # Arguments
/// * path: the path to the file, it must end with \0 char
/// * flags: the unsigned value of the open flags
///
/// # Returns
/// * Ok(FileDescriptor)
/// * KernelError::InvalidOpenFlags(flags)
#[inline(always)]
pub(crate) fn sys_open(path: *const u8, flags: u32) -> Result<isize> {
    let flags = OpenFlags::from_bits(flags).ok_or(KernelError::InvalidOpenFlags(flags))?;
    let task = PROCESSOR.current_task()?;
    let mut inner = task.inner_exclusive_access();
    let current_space = inner.space();
    let path = current_space.translated_string(path)?;
    let file = ROOT_INODE.find(&path, flags)?;
    if let Ok(fd) = inner.allc_fd(file) {
        Ok(fd as isize)
    } else {
        Ok(-1)
    }
}

/// Close a file and return the status code.
///
/// # Arguments
/// * fd: the file descriptor
///
/// # Return
/// * Ok(0)
/// * Ok(-1)
#[inline(always)]
pub(crate) fn sys_close(fd: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let mut inner = task.inner_exclusive_access();
    if let Ok(_) = inner.dealloc_fd(fd) {
        Ok(0)
    } else {
        Ok(-1)
    }
}

/// Write a &str to the IO device.
/// Different IO devices are distinguished by different unique description ID numbers
///
/// # Arguments
/// * fd: the discriptor for different IO devices
/// * buffer: the pointer to the buffer to write
/// * len: the length of the buffer
///
/// # Returns
/// * Ok(writed length)
/// * Err(KernelError::InvalidFileDescriptor(fd))
#[inline(always)]
pub(crate) fn sys_write(fd: usize, buffer: *const u8, len: usize) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let inner = task.inner_access();
    let current_space = inner.space();
    let buffers = current_space.translated_byte_buffers(buffer, len)?;
    let file = inner
        .get_file(fd)
        .ok_or(KernelError::FileDescriptorDoesNotExist(fd))?;
    let file = Arc::clone(file);
    drop(inner);
    Ok(file.write(buffers)? as isize)
}

/// Read a &str from the IO device and save it to the buffer.
/// Different IO devices are distinguished by different unique description ID numbers
///
/// # Arguments
/// * fd: the discriptor for different IO devices
/// * buffer: the pointer to the buffer to write
/// * len: the length of the buffer
///
/// # Returns
/// * Ok(writed length)
/// * Err(KernelError::InvalidFileDescriptor(fd))
#[inline(always)]
pub(crate) fn sys_read(fd: usize, buffer: *mut u8, len: usize) -> Result<isize> {
    assert!(len > 0);
    let task = PROCESSOR.current_task()?;
    let inner = task.inner_access();
    let current_space = inner.space();
    let buffers = current_space.translated_byte_buffers(buffer, len)?;
    let file = inner
        .get_file(fd)
        .ok_or(KernelError::FileDescriptorDoesNotExist(fd))?;
    let file = Arc::clone(file);
    drop(inner);
    Ok(file.read(buffers)? as isize)
}

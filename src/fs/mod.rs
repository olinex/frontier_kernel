// @author:    olinex
// @time:      2024/01/10

// self mods
pub(crate) mod inode;
pub(crate) mod pipe;
pub(crate) mod stdio;

// use other mods
use alloc::boxed::Box;
use frontier_fs::block::BLOCK_DEVICE_REGISTER;
use frontier_fs::vfs::{FileSystem, FS};

// use self mods
use crate::drivers::blocks::BlockDeviceImpl;
use crate::lang::buffer::ByteBuffers;
use crate::prelude::*;

/// Core trait, all structs that implement this feature can be read and written as files.
pub(crate) trait File: Send + Sync {
    /// Read file to `UserBuffer`
    fn read(&self, buffers: ByteBuffers) -> Result<u64>;
    /// Write `UserBuffer` to file
    fn write(&self, buffers: ByteBuffers) -> Result<u64>;
}

lazy_static! {
    /// The root file system, through which all operations on files are invoked by the operating system
    pub(crate) static ref ROOT_FS: FS = {
        let device = Box::new(BlockDeviceImpl::new());
        let tracker = BLOCK_DEVICE_REGISTER.lock().mount(device).unwrap();
        *FS::open(&tracker).unwrap()
    };
}

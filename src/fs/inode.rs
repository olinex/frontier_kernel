// @author:    olinex
// @time:      2024/01/10

// self mods

// use other mods
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use frontier_fs::vfs::{FileSystem, Inode};
use frontier_fs::OpenFlags;
use spin::Mutex;

// use self mods
use super::{File, ROOT_FS};
use crate::lang::buffer::ByteBuffers;
use crate::prelude::*;

const PATH_SPLITER: &'static str = "/";

/// The inner struct for os inode, which contain the byte offsets have currently readed.
pub(crate) struct OSInodeInner {
    offset: u64,
    inode: Arc<Inode>,
}
impl OSInodeInner {
    #[inline(always)]
    fn must_be_readable(&self) -> Result<()> {
        if self.inode.flags().is_readable() {
            Ok(())
        } else {
            Err(KernelError::FileMustBeReadable(
                self.inode.inode_bitmap_index(),
            ))
        }
    }

    #[inline(always)]
    fn must_be_writable(&self) -> Result<()> {
        if self.inode.flags().is_writable() {
            Ok(())
        } else {
            Err(KernelError::FileMustBeWritable(
                self.inode.inode_bitmap_index(),
            ))
        }
    }

    #[inline(always)]
    fn must_be_executable(&self) -> Result<()> {
        if self.inode.flags().is_executable() {
            Ok(())
        } else {
            Err(KernelError::FileMustBeExecutable(
                self.inode.inode_bitmap_index(),
            ))
        }
    }
}

/// The Inode object for direct read/write by the operating system wraps the read-write inode and read-only permission identifiers
pub(crate) struct OSInode {
    flags: OpenFlags,
    inner: Mutex<OSInodeInner>,
}
impl OSInode {
    /// Create a new operation system inode obejct.
    ///
    /// # Arguments
    /// * flags: the permission mode for the operation of the inode
    /// * inode: the inode object return by file system
    pub(crate) fn new(flags: OpenFlags, inode: Arc<Inode>) -> Self {
        Self {
            flags,
            inner: Mutex::new(OSInodeInner { offset: 0, inode }),
        }
    }

    /// List all the child inode's name as String
    ///
    /// # Returns
    /// * Ok(Vec<name>)
    /// * Err(
    ///     FileSystemError(
    ///         InodeMustBeDirectory(bitmap index) |
    ///         DataOutOfBounds |
    ///         NoDroptableBlockCache |
    ///         RawDeviceError(error code)
    ///     )
    ///     FileMustBeReadable(bitmap index)
    /// )
    fn ls(&self) -> Result<Vec<String>> {
        let inner = self.inner.lock();
        inner.must_be_readable()?;
        Ok(inner.inode.list_child_names()?)
    }

    /// Get or create child os inode from this current os inode.
    ///
    /// # Arguments
    /// * name: the name of child os inode
    /// * flags: the permission mode for the operation of the inode
    ///
    /// # Returns
    /// * Ok(Arc<child os inode>)
    /// * Err(
    ///     FileSystemError(
    ///         InodeMustBeDirectory(bitmap index) |
    ///         DataOutOfBounds |
    ///         NoDroptableBlockCache |
    ///         RawDeviceError(error code)
    ///         DuplicatedFname(name, inode bitmap index) |
    ///         BitmapExhausted(start_block_id) |
    ///         BitmapIndexDeallocated(bitmap_index) |
    ///         RawDeviceError(error code)
    ///     ) |
    ///     FileMustBeReadable(bitmap index) |
    ///     FileDoesNotExists(name)
    /// )
    fn get_child(&self, name: &str, flags: OpenFlags) -> Result<Arc<OSInode>> {
        let inner = self.inner.lock();
        inner.must_be_readable()?;
        if let Some(child_inode) = inner.inode.get_child_inode(name)? {
            Ok(Arc::new(OSInode {
                flags,
                inner: Mutex::new(OSInodeInner {
                    offset: 0,
                    inode: Arc::new(child_inode),
                }),
            }))
        } else if flags.is_create() {
            let child_inode = inner.inode.create_child_inode(name, flags.into())?;
            Ok(Arc::new(OSInode {
                flags,
                inner: Mutex::new(OSInodeInner {
                    offset: 0,
                    inode: Arc::new(child_inode),
                }),
            }))
        } else {
            Err(KernelError::FileDoesNotExists(name.to_string()))
        }
    }

    /// Create a os inode as child into current inode
    ///
    /// # Arguments
    /// * name: the name of child os inode
    /// * flags: the permission mode for the operation of the inode
    ///
    /// # Returns
    /// * Ok(Arc<child os inode>)
    /// * Err(
    ///     FileSystemError(
    ///         InodeMustBeDirectory(bitmap index) |
    ///         DuplicatedFname(name, inode bitmap index) |
    ///         BitmapExhausted(start_block_id) |
    ///         BitmapIndexDeallocated(bitmap_index) |
    ///         DataOutOfBounds |
    ///         NoDroptableBlockCache |
    ///         RawDeviceError(error code)
    ///     ) |
    ///     FileMustBeWritable(bitmap index)
    /// )
    fn create_child(&self, name: &str, flags: OpenFlags) -> Result<Arc<OSInode>> {
        let inner = self.inner.lock();
        inner.must_be_writable()?;
        let child_inode = inner.inode.create_child_inode(name, flags.into())?;
        Ok(Arc::new(OSInode {
            flags,
            inner: Mutex::new(OSInodeInner {
                offset: 0,
                inode: Arc::new(child_inode),
            }),
        }))
    }

    /// Remove child os inode from current os inode
    ///
    /// # Arguments
    /// * name: the name of child os inode
    ///
    /// # Returns
    /// * Ok(())
    /// * Err(
    ///     FileSystemError(
    ///         InodeMustBeDirectory(bitmap index) |
    ///         FnameDoesNotExist(name, inode bitmap index) |
    ///         DataOutOfBounds |
    ///         BitmapIndexDeallocated(bitmap_index) |
    ///         NoDroptableBlockCache |
    ///         RawDeviceError(error code) |
    ///         DeleteNonEmptyDirectory(name, inode bitmap index)    
    ///     ) |
    ///     FileMustBeWritable(bitmap index)
    /// )
    fn remove_child(&self, name: &str) -> Result<()> {
        let inner = self.inner.lock();
        inner.must_be_writable()?;
        inner.inode.remove_child_inode(name)?;
        Ok(())
    }

    /// Read all bytes from current os inode
    ///
    /// # Returns
    /// * Ok(Vec<bytes>)
    /// * Err(
    ///     FileSystemError(
    ///         DataOutOfBounds |
    ///         NoDroptableBlockCache |
    ///         RawDeviceError(error code)
    ///     ) |
    ///     FileMustBeReadable(bitmap index)
    /// )
    pub(crate) fn read_all(&self) -> Result<Vec<u8>> {
        let inner = self.inner.lock();
        inner.must_be_readable()?;
        Ok(inner.inode.read_all()?)
    }
}
impl File for OSInode {
    fn read(&self, buffers: ByteBuffers) -> Result<u64> {
        let mut inner = self.inner.lock();
        inner.must_be_readable()?;
        let mut total_read_size = 0u64;
        for slice in buffers.into_slices() {
            let read_size = inner.inode.read_buffer(slice, inner.offset)?;
            if read_size == 0 {
                break;
            }
            inner.offset += read_size as u64;
            total_read_size += read_size as u64;
        }
        Ok(total_read_size)
    }

    fn write(&self, buffers: ByteBuffers) -> Result<u64> {
        let mut inner = self.inner.lock();
        inner.must_be_writable()?;
        let mut total_write_size = 0u64;
        for slice in buffers.into_slices() {
            let write_size = inner.inode.write_buffer(slice, inner.offset)?;
            assert_eq!(write_size, slice.len());
            inner.offset += write_size as u64;
            total_write_size += write_size as u64;
        }
        Ok(total_write_size)
    }
}

lazy_static! {
    /// The static root os inode read from file system
    pub(crate) static ref ROOT_INODE: Arc<OSInode> = {
        let root_inode = Arc::new(ROOT_FS.root_inode());
        Arc::new(OSInode::new(OpenFlags::RWDIR, root_inode))
    };
}
impl ROOT_INODE {
    /// Find the os inode in the file system by the path, and the path is split by "/".
    ///
    /// # Arguments
    /// * path: the path of the os inode, split by "/"
    /// * flags: once the os inode is found, the flags that affects subsequent behavior
    ///
    /// # Returns
    /// * Ok(Arc<OSInode>)
    /// * Err(
    ///     FileSystemError(
    ///         InodeMustBeDirectory(bitmap index) |
    ///         DataOutOfBounds |
    ///         NoDroptableBlockCache |
    ///         RawDeviceError(error code)
    ///         DuplicatedFname(name, inode bitmap index) |
    ///         BitmapExhausted(start_block_id) |
    ///         BitmapIndexDeallocated(bitmap_index) |
    ///         RawDeviceError(error code)
    ///     ) |
    ///     FileMustBeReadable(bitmap index) |
    ///     FileDoesNotExists(name)
    /// )
    pub(crate) fn find(&self, path: &str, flags: OpenFlags) -> Result<Arc<OSInode>> {
        let mut parent: Arc<OSInode> = Arc::clone(self);
        let names: Vec<&str> = path.split(PATH_SPLITER).collect();
        let last = names.len() - 1;
        let mut first = true;
        for (index, name) in names.iter().enumerate() {
            if first {
                first = false;
                if name.is_empty() {
                    continue;
                }
            }
            let flags = if index == last {
                flags
            } else {
                OpenFlags::RDIR
            };
            parent = parent.get_child(name, flags)?;
        }
        Ok(parent)
    }
}

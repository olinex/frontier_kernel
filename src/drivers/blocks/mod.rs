// @author:    olinex
// @time:      2024/01/09

// self mods
mod virtio_blk;

// use other mods
// use self mods

#[cfg(feature = "board_qemu")]
pub(crate) type BlockDeviceImpl = virtio_blk::VirtIOBlock;

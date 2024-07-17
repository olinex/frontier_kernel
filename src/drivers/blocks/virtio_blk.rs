// @author:    olinex
// @time:      2024/01/09

// self mods

// use other mods
use alloc::vec::Vec;
use core::ptr::NonNull;
use frontier_fs::block::BlockDevice;
use frontier_fs::configs::BLOCK_BYTE_SIZE;
use virtio_drivers::device::blk::VirtIOBlk;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::{BufferDirection, Hal, PhysAddr};

// use self mods
use crate::lang::container::UserPromiseRefCell;
use crate::memory::frame::{FrameTracker, FRAME_ALLOCATOR};
use crate::memory::space::KERNEL_SPACE;
use crate::prelude::*;

const VIRTIO_0: usize = 0x1000_1000;
const BLK_BYTE_SIZE: usize = 512;
const BLK_GROUP_COUNT: usize = BLOCK_BYTE_SIZE / BLK_BYTE_SIZE;

pub(crate) struct HalImpl;
unsafe impl Hal for HalImpl {
    /// Allocate memory frames for DMA to use.
    /// All frames are temporarily stored in a buffered queue
    /// and the physical addresses of these frames must be contiguous.
    /// - TODO: There is currently no mechanism in place to ensure frame continuity!
    ///
    /// - Arguments
    ///     - pages: the number of the frames will be allocated
    ///
    /// - Returns
    ///     - usize: the physical memory address DMA allocated
    fn dma_alloc(pages: usize, _direct: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        let mut frames = QUEUE_FRAMES.exclusive_access();
        let mut start_ppn = 0;
        let mut start_pa = 0;
        for i in 0..pages {
            let frame = FRAME_ALLOCATOR.alloc().unwrap();
            if i == 0 {
                start_ppn = frame.ppn();
                start_pa = frame.pa();
            }
            assert_eq!(frame.ppn(), start_ppn + i);
            frames.push(frame);
        }
        (start_pa, NonNull::new(start_pa as _).unwrap())
    }

    /// Dealloc memory frames by DMA.
    /// The release of frames by DMA is random,
    /// and DMA does not guarantee that it will be released from the start of a continuous frames
    ///
    /// - Arguments
    ///     - paddr: the physical memory address
    ///     - pages: the number of the frames will be deallocated
    unsafe fn dma_dealloc(paddr: PhysAddr, _vaddr: NonNull<u8>, pages: usize) -> i32 {
        trace!("dealloc DMA: paddr={:#018x}, pages={}", paddr, pages);
        let mut frames = QUEUE_FRAMES.exclusive_access();
        let mut offset = 0;
        let start_ppn = paddr.into();
        for (index, tracker) in frames.iter().enumerate() {
            if tracker.ppn() != start_ppn {
                continue;
            }
            offset = index;
            break;
        }
        for i in 0..pages {
            let mut tracker = frames.remove(offset + i);
            FRAME_ALLOCATOR.dealloc(&mut tracker).unwrap();
        }
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        NonNull::new(paddr as _).unwrap()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direct: BufferDirection) -> PhysAddr {
        let vaddr = buffer.as_ptr() as *mut u8 as usize;
        KERNEL_SPACE.access().translate_pa(vaddr).unwrap()
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direct: BufferDirection) {
        // Nothing to do, as the host already has access to all memory and we didn't copy the buffer
        // anywhere else.
    }
}

/// Virtual IO block devices via memory mapping
pub(crate) struct VirtIOBlock {
    blk: UserPromiseRefCell<VirtIOBlk<HalImpl, MmioTransport>>,
}
impl VirtIOBlock {
    pub(crate) fn new() -> Result<Self> {
        let header = VIRTIO_0 as *mut VirtIOHeader;
        let transport = unsafe { MmioTransport::new(NonNull::new(header).unwrap())? };
        let blk = VirtIOBlk::<HalImpl, _>::new(transport)?;
        Ok(Self {
            blk: unsafe { UserPromiseRefCell::new(blk) },
        })
    }
}
impl BlockDevice for VirtIOBlock {
    fn read_block(&self, id: usize, buffer: &mut [u8]) -> Option<isize> {
        let mut device = self.blk.exclusive_access();
        for i in 0..BLK_GROUP_COUNT {
            let start_offset = i * BLK_BYTE_SIZE;
            let end_offset = start_offset + BLK_BYTE_SIZE;
            if device
                .read_blocks(
                    id * BLK_GROUP_COUNT + i,
                    &mut buffer[start_offset..end_offset],
                )
                .is_err()
            {
                return Some(-1);
            }
        }
        None
    }

    fn write_block(&self, id: usize, buffer: &[u8]) -> Option<isize> {
        let mut device = self.blk.exclusive_access();
        for i in 0..BLK_GROUP_COUNT {
            let start_offset = i * BLK_BYTE_SIZE;
            let end_offset = start_offset + BLK_BYTE_SIZE;
            if device
                .write_blocks(id * BLK_GROUP_COUNT + i, &buffer[start_offset..end_offset])
                .is_err()
            {
                return Some(-1);
            }
        }
        None
    }
}

lazy_static! {
    pub(crate) static ref QUEUE_FRAMES: UserPromiseRefCell<Vec<FrameTracker>> =
        unsafe { UserPromiseRefCell::new(Vec::new()) };
}

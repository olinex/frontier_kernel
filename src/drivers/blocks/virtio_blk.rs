// @author:    olinex
// @time:      2024/01/09

// self mods
use crate::lang::container::UserPromiseRefCell;
use crate::memory::frame::{FrameTracker, FRAME_ALLOCATOR};
use crate::memory::space::KERNEL_SPACE;

// use other mods
use alloc::vec::Vec;
use frontier_fs::block::BlockDevice;
use frontier_fs::configs::BLOCK_BYTE_SIZE;
use virtio_drivers::{Hal, PhysAddr, VirtAddr, VirtIOBlk, VirtIOHeader};

// use self mods

const VIRTIO_0: usize = 0x1000_1000;
const BLK_BYTE_SIZE: usize = 512;
const BLK_GROUP_COUNT: usize = BLOCK_BYTE_SIZE / BLK_BYTE_SIZE;

lazy_static! {
    static ref QUEUE_FRAMES: UserPromiseRefCell<Vec<FrameTracker>> =
        unsafe { UserPromiseRefCell::new(Vec::new()) };
}

pub(crate) struct HalImpl;
impl Hal for HalImpl {
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
    fn dma_alloc(pages: usize) -> usize {
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
        start_pa
    }

    /// Dealloc memory frames by DMA.
    /// The release of frames by DMA is random, 
    /// and DMA does not guarantee that it will be released from the start of a continuous frames
    /// 
    /// - Arguments
    ///     - paddr: the physical memory address
    ///     - pages: the number of the frames will be deallocated
    fn dma_dealloc(paddr: PhysAddr, pages: usize) -> i32 {
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

    fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
        paddr
    }

    fn virt_to_phys(vaddr: VirtAddr) -> PhysAddr {
        KERNEL_SPACE.access().translate_pa(vaddr).unwrap()
    }
}

/// Virtual IO block devices via memory mapping
pub(crate) struct VirtIOBlock(UserPromiseRefCell<VirtIOBlk<'static, HalImpl>>);
impl VirtIOBlock {
    pub(crate) fn new() -> Self {
        let blk =
            VirtIOBlk::<HalImpl>::new(unsafe { &mut *(VIRTIO_0 as *mut VirtIOHeader) }).unwrap();
        Self(unsafe { UserPromiseRefCell::new(blk) })
    }
}
impl BlockDevice for VirtIOBlock {
    fn read_block(&self, id: usize, buffer: &mut [u8]) -> Option<isize> {
        let mut device = self.0.exclusive_access();
        for i in 0..BLK_GROUP_COUNT {
            let start_offset = i * BLK_BYTE_SIZE;
            let end_offset = start_offset + BLK_BYTE_SIZE;
            if device
                .read_block(
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
        let mut device = self.0.exclusive_access();
        for i in 0..BLK_GROUP_COUNT {
            let start_offset = i * BLK_BYTE_SIZE;
            let end_offset = start_offset + BLK_BYTE_SIZE;
            if device
                .write_block(id * BLK_GROUP_COUNT + i, &buffer[start_offset..end_offset])
                .is_err()
            {
                return Some(-1);
            }
        }
        None
    }
}

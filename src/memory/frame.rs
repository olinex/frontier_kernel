// @author:    olinex
// @time:      2023/09/16

// self mods

// use other mods
use alloc::sync::Arc;
use core::mem;

use super::PageBytes;
// use self mods
use super::allocator::BTreeSetFrameAllocator;
use super::{page_table::PageTable, PageTableTr};
use crate::configs;
use crate::lang::container;
use crate::prelude::*;

/// A tracker wrapper for physical memory frame
/// Which will automatically dealloc frame for others can reuse it
pub(crate) struct FrameTracker {
    ppn: usize,
}
impl FrameTracker {
    /// Create a new FrameTracker and clear all data in frame
    /// - Arguments
    ///     - frame: The physical page number
    pub(crate) fn new(ppn: usize) -> Self {
        let tracker = Self { ppn };
        tracker.clear();
        tracker
    }

    /// Get the physical page number
        pub(crate) fn ppn(&self) -> usize {
        self.ppn
    }

    /// Get the physical memory address of the beginning of the frame
    pub(crate) fn pa(&self) -> usize {
        self.ppn * configs::MEMORY_PAGE_BYTE_SIZE
    }

    /// [Unsafe] Returns data within a physical page according to the specified data structure
    /// - Arguments
    ///     - offset: The offset of the specified data structure, start from 0
    ///
    /// - Returns
    ///     - &mut U: return the mutable reference of U structure data cannot greater than the page size
        pub(crate) unsafe fn as_kernel_mut<'a, 'b, U>(&'a self, offset: usize) -> &'b mut U {
        let mem_size = mem::size_of::<U>();
        let end = offset + mem_size;
        assert!(end <= configs::MEMORY_PAGE_BYTE_SIZE);
        &mut *((self.pa() + offset) as *mut U)
    }

    /// Get the physical memory data from the frame as u8 array
    /// 
    /// - Returns
    ///     - &mut [u8; MEMORY_PAGE_SIZE]
        pub(crate) fn get_byte_array<'a, 'b>(&'a self) -> &'b mut PageBytes {
        unsafe { self.as_kernel_mut(0) }
    }

    /// Set all byte to zero in frame
    pub(crate) fn clear(&self) {
        let array = self.get_byte_array();
        for i in array {
            *i = 0;
        }
    }
}
impl Drop for FrameTracker {
    // Dealloc the physical memory frame when tracker is dropped
    fn drop(&mut self) {
        FRAME_ALLOCATOR.dealloc(self).unwrap()
    }
}

lazy_static! {
    /// Global physical memory frame allocator
    /// Because physical memory is unique throughout the system
    /// A globally unique allocator is required to manage it
    pub(crate) static ref FRAME_ALLOCATOR: Arc<container::UserPromiseRefCell<BTreeSetFrameAllocator>> =
        Arc::new(unsafe { container::UserPromiseRefCell::new(BTreeSetFrameAllocator::new()) });
}
impl FRAME_ALLOCATOR {

    /// Alloc a new frame an return the tracker.
    /// If the tracker is dropped, the frame will automatic dealloc.
    /// 
    /// - Errors
    ///     - FrameExhausted
    pub(crate) fn alloc(&self) -> Result<FrameTracker> {
        let ppn = self.exclusive_access().alloc()?;
        Ok(FrameTracker::new(ppn))
    }

    /// Dealloc a old frame.
    /// This method will be call by frame tracker when it was dropping,
    /// So they was not necessary to call by yourself.
    /// 
    /// In order to prevent this method from being abused, 
    /// we require that the incoming arguments of this method must be mutable frame tracker reference
    /// 
    /// - Arguments
    ///     - tracker: the mutable frame tracker reference
    /// 
    /// - Errors
    ///     FrameNotDeallocable(ppn)
    pub(crate) fn dealloc(&self, tracker: &mut FrameTracker) -> Result<()> {
        self.exclusive_access().dealloc(tracker.ppn())
    }
}

/// Initializes the global physical memory frame allocator
/// We must clear the bss section first
#[inline(always)]
pub(crate) fn init_frame_allocator() {
    let start = PageTable::get_ppn_with(configs::_addr_free_mem_start as usize);
    let end = PageTable::get_ppn_with(configs::_addr_free_mem_end as usize);
    debug!(
        "[{:>12}, {:>12}): Frame memory page initialized",
        start, end
    );
    FRAME_ALLOCATOR.exclusive_access().init(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_global_frame_allocator_alloc_and_dealloc() {
        assert_ne!(FRAME_ALLOCATOR.access().current_ppn(), 0);
        assert_ne!(FRAME_ALLOCATOR.access().end_ppn(), 0);
        let tracker = FRAME_ALLOCATOR.alloc().unwrap();
        let prev_ppn = tracker.ppn();
        assert!(prev_ppn < FRAME_ALLOCATOR.access().current_ppn());
        let tracker = FRAME_ALLOCATOR.alloc().unwrap();
        let next_ppn = tracker.ppn();
        assert!(next_ppn < FRAME_ALLOCATOR.access().current_ppn());
        assert!(prev_ppn + 1 == next_ppn);
    }

    #[test_case]
    fn test_global_frame_allocator_clear_when_drop() {
        let tracker = FRAME_ALLOCATOR.alloc().unwrap();
        let frame = tracker.ppn();
        let array = tracker.get_byte_array();
        assert_eq!(array[0], 0);
        array[0] = 1;
        drop(tracker);
        let tracker = FRAME_ALLOCATOR.alloc().unwrap();
        assert_eq!(frame, tracker.ppn());
        let array = tracker.get_byte_array();
        assert_eq!(array[0], 0);
        assert!((&array[0] as *const u8) as usize == tracker.pa());
    }
}

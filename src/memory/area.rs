// @author:    olinex
// @time:      2023/09/16

// self mods

// use other mods
use alloc::collections::BTreeMap;
use alloc::sync::Arc;

// use self mods
use super::allocator::LinkedListPageRangeAllocator;
use super::frame::{FrameTracker, FRAME_ALLOCATOR};
use super::page_table::PageTable;
use super::{PageTableFlags, PageTableTr};
use crate::lang::container::UserPromiseRefCell;
use crate::{configs, prelude::*};

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AreaType {
    Identical,
    Framed,
}

struct PageRangeTracker {
    start_vpn: usize,
    end_vpn: usize,
    allocator: Arc<LinkedListPageRangeAllocator>,
}
impl PageRangeTracker {
    fn new(
        start_vpn: usize,
        end_vpn: usize,
        allocator: &Arc<LinkedListPageRangeAllocator>,
    ) -> Self {
        Self {
            start_vpn,
            end_vpn,
            allocator: Arc::clone(allocator),
        }
    }

    #[inline(always)]
    pub fn byte_size(&self) -> usize {
        (self.end_vpn - self.start_vpn) * configs::MEMORY_PAGE_BYTE_SIZE
    }

    #[inline(always)]
    pub fn start_vpn(&self) -> usize {
        self.start_vpn
    }

    #[inline(always)]
    pub fn end_vpn(&self) -> usize {
        self.end_vpn
    }

    #[inline(always)]
    pub fn page_range(&self) -> core::ops::Range<usize> {
        self.start_vpn..self.end_vpn
    }
}
impl Drop for PageRangeTracker {
    fn drop(&mut self) {
        self.allocator
            .dealloc(self.start_vpn, self.end_vpn)
            .expect("Cannot dealloc a page range tracker")
    }
}

pub struct Area {
    flags: PageTableFlags,
    area_type: AreaType,
    frame_trackers: BTreeMap<usize, FrameTracker>,
    page_range_tracker: PageRangeTracker,
    page_table: Arc<UserPromiseRefCell<PageTable>>,
}
impl Area {
    pub fn new(
        start_vpn: usize,
        end_vpn: usize,
        flags: PageTableFlags,
        area_type: AreaType,
        allocator: &Arc<LinkedListPageRangeAllocator>,
        page_table: &Arc<UserPromiseRefCell<PageTable>>,
    ) -> Result<Self> {
        allocator
            .alloc(start_vpn, end_vpn)
            .ok_or(KernelError::AreaAllocationFailed(start_vpn, end_vpn))?;
        let page_range_tracker = PageRangeTracker::new(start_vpn, end_vpn, allocator);
        Ok(Self {
            area_type,
            flags,
            frame_trackers: BTreeMap::new(),
            page_range_tracker,
            page_table: Arc::clone(page_table),
        })
    }

    #[inline(always)]
    pub fn range(&self) -> (usize, usize) {
        (
            self.page_range_tracker.start_vpn(),
            self.page_range_tracker.end_vpn(),
        )
    }

    pub fn map_one(&mut self, vpn: usize) -> Result<usize> {
        let ppn = match self.area_type {
            AreaType::Identical => vpn,
            AreaType::Framed => {
                let tracker = FRAME_ALLOCATOR.alloc()?;
                let ppn = tracker.ppn();
                if let Some(_) = self.frame_trackers.insert(vpn, tracker) {
                    return Err(KernelError::VPNAlreadyMapped(vpn));
                }
                ppn
            }
        };
        // FIXME: frame alloc and page table alloc must be synchronized
        self.page_table
            .exclusive_access()
            .map_without_alloc(vpn, ppn, self.flags)?;
        Ok(ppn)
    }

    pub fn unmap_one(&mut self, vpn: usize) -> Result<usize> {
        let ppn = match self.area_type {
            AreaType::Identical => vpn,
            AreaType::Framed => {
                let tracker = self
                    .frame_trackers
                    .remove(&vpn)
                    .ok_or(KernelError::VPNNotMapped(vpn))?;
                tracker.ppn()
            }
        };
        // FIXME: frame dealloc and page table dealloc must be synchronized
        self.page_table
            .exclusive_access()
            .unmap_without_dealloc(vpn)?;
        Ok(ppn)
    }

    pub fn map(&mut self) -> Result<()> {
        for vpn in self.page_range_tracker.page_range() {
            self.map_one(vpn)?;
        }
        Ok(())
    }

    pub fn unmap(&mut self) -> Result<()> {
        for vpn in self.page_range_tracker.page_range() {
            if self.frame_trackers.contains_key(&vpn) {
                self.unmap_one(vpn)?;
            }
        }
        Ok(())
    }

    #[inline]
    fn write_page(&mut self, vpn: usize, offset: usize, data: &[u8]) -> Result<()> {
        if let Some(tracker) = self.frame_trackers.get(&vpn) {
            let dst = tracker.get_byte_array();
            dst[offset..data.len()].copy_from_slice(data);
            Ok(())
        } else {
            Err(KernelError::VPNNotMapped(vpn))
        }
    }

    pub unsafe fn as_kernel_mut<U>(&self, vpn: usize, linear_offset: usize) -> Result<&mut U> {
        if let Some(tracker) = self.frame_trackers.get(&vpn) {
            Ok(tracker.as_kernel_mut::<U>(linear_offset))
        } else {
            Err(KernelError::VPNNotMapped(vpn))
        }
    }

    pub fn write_multi_pages(&mut self, linear_offset: usize, data: &[u8]) -> Result<()> {
        assert_eq!(self.area_type, AreaType::Framed);
        let length = data.len();
        let linear_end = length + linear_offset;
        assert!(linear_end <= self.page_range_tracker.byte_size());
        let mut start = linear_offset % configs::MEMORY_PAGE_BYTE_SIZE;
        let start_vpn = linear_offset / configs::MEMORY_PAGE_BYTE_SIZE;
        let end_vpn = linear_end / configs::MEMORY_PAGE_BYTE_SIZE;
        let offset_vpn = self.page_range_tracker.start_vpn;
        for vpn in start_vpn..end_vpn {
            let data_offset = vpn * configs::MEMORY_PAGE_BYTE_SIZE;
            let src = &data[data_offset + start..data_offset + configs::MEMORY_PAGE_BYTE_SIZE];
            self.write_page(vpn + offset_vpn, start, src)?;
            start = 0;
        }
        let end = linear_end % configs::MEMORY_PAGE_BYTE_SIZE;
        if end != 0 {
            let data_offset = end_vpn * configs::MEMORY_PAGE_BYTE_SIZE;
            let src = &data[data_offset + start..data_offset + end];
            self.write_page(end_vpn + offset_vpn, start, src)?;
        }
        Ok(())
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        self.unmap().unwrap()
    }
}

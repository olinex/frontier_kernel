// @author:    olinex
// @time:      2023/09/16

// self mods

// use other mods
use alloc::sync::Arc;

// use self mods
use super::allocator::LinkedListPageRangeAllocator;
use super::page_table::PageTable;
use super::{PageBytes, PageTableFlags, PageTableTr};
use crate::lang::container::UserPromiseRefCell;
use crate::{configs, prelude::*};

/// The type of the area
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) enum AreaMapping {
    /// Area will only map vpn to ppn which equals vpn,
    /// and will not allocate frame
    Identical,
    /// Area will allocate frames and map vpn to it.
    /// every frame will be managed by area
    Framed,
}

/// The virtual page range tracker which will automatically dealloc when dropping,
/// Like the frame, virtual page cannot be allocated twice before it is dropped.
struct PageRangeTracker {
    /// The start virtual page number of the range which is contained in the range
    start_vpn: usize,
    /// The end virtual page number of the range which isn't contained in the range
    end_vpn: usize,
    /// The page range allocator which the tracker belongs to
    allocator: Arc<LinkedListPageRangeAllocator>,
}
impl PageRangeTracker {
    /// Create a new tracker, don't using it by yourself
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

    /// The byte size of current page range
    pub(crate) fn byte_size(&self) -> usize {
        (self.end_vpn - self.start_vpn) * configs::MEMORY_PAGE_BYTE_SIZE
    }

    /// Get the start page number of the page range
    pub(crate) fn start_vpn(&self) -> usize {
        self.start_vpn
    }

    /// Get the end page number of the page range
    pub(crate) fn end_vpn(&self) -> usize {
        self.end_vpn
    }

    /// Get the range of the virtual page number
    pub(crate) fn page_range(&self) -> core::ops::Range<usize> {
        self.start_vpn..self.end_vpn
    }

    /// Check the vpn is one of the page number in the page range
    pub(crate) fn contain(&self, vpn: usize) -> bool {
        self.start_vpn <= vpn && vpn < self.end_vpn
    }

    /// Check the is one of the page number in the page range
    ///
    /// - Errors
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    pub(crate) fn check(&self, vpn: usize) -> Result<()> {
        if self.contain(vpn) {
            Ok(())
        } else {
            Err(KernelError::VPNOutOfArea {
                vpn,
                start: self.start_vpn,
                end: self.end_vpn,
            })
        }
    }
}
impl Drop for PageRangeTracker {
    /// Dealloc the page range to allocator when dropping the tracker
    fn drop(&mut self) {
        self.allocator
            .dealloc(self.start_vpn, self.end_vpn)
            .unwrap();
    }
}

/// A area is a virtual abstraction class of contiguous virtual memory pages,
/// and the internal memory pages have the same permissions and mappings
pub(crate) struct Area {
    /// Area permission flags
    flags: PageTableFlags,
    /// Area mapping type
    area_mapping: AreaMapping,
    /// The tracker of the virtual page number range
    page_range_tracker: PageRangeTracker,
    /// The refrence of the page table which contains the area
    page_table: Arc<UserPromiseRefCell<PageTable>>,
}
impl Area {
    /// Create a new area
    ///
    /// - Arguments
    ///     - start_vpn: the start virtual page number of the area page range
    ///     - end_vpn: the end virutal page number ofthe area page range
    ///     - flags: the permission flags of the each frame
    ///     - area_mapping: area mapping type
    ///     - allocator: the virtual page range allocator
    ///     - page_table: the page table which will be used when alloc/delloc frame
    ///
    /// - Errors
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    pub(crate) fn new(
        start_vpn: usize,
        end_vpn: usize,
        flags: PageTableFlags,
        area_mapping: AreaMapping,
        allocator: &Arc<LinkedListPageRangeAllocator>,
        page_table: &Arc<UserPromiseRefCell<PageTable>>,
    ) -> Result<Self> {
        allocator
            .alloc(start_vpn, end_vpn)
            .ok_or(KernelError::AreaAllocFailed(start_vpn, end_vpn))?;
        let mut area = Self {
            area_mapping,
            flags,
            page_range_tracker: PageRangeTracker::new(start_vpn, end_vpn, allocator),
            page_table: Arc::clone(page_table),
        };
        area.map()?;
        Ok(area)
    }

    /// Create a new area copy by another area
    ///
    /// - Arguments
    ///     - another: another area which is to be copied
    ///     - allocator: the virtual page range allocator
    ///     - page_table: the page table which will be used when alloc/delloc frame
    ///
    /// - Errors
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    pub(crate) fn from_another(
        another: &Self,
        allocator: &Arc<LinkedListPageRangeAllocator>,
        page_table: &Arc<UserPromiseRefCell<PageTable>>,
    ) -> Result<Self> {
        let start_vpn = another.page_range_tracker.start_vpn();
        let end_vpn = another.page_range_tracker.end_vpn();
        allocator
            .alloc(start_vpn, end_vpn)
            .ok_or(KernelError::AreaAllocFailed(start_vpn, end_vpn))?;
        let mut area = Self {
            area_mapping: another.area_mapping,
            flags: another.flags,
            page_range_tracker: PageRangeTracker::new(start_vpn, end_vpn, allocator),
            page_table: Arc::clone(page_table),
        };
        area.map()?;
        area.copy_another(another)?;
        Ok(area)
    }

    /// Copy another area's bytes to current area
    ///
    /// - Arguments
    ///     - another: another area which is to be copied
    /// 
    /// - Returns
    ///     - Ok(copied virtual page count)
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn copy_another(&self, another: &Area) -> Result<usize> {
        let (src_start_vpn, src_end_vpn) = another.range();
        let (dst_start_vpn, dst_end_vpn) = self.range();
        let src_offset = src_end_vpn - src_start_vpn;
        let dst_offset = dst_end_vpn - dst_start_vpn;
        let count = if src_offset >= dst_offset {
            dst_offset
        } else {
            src_offset
        };
        for offset in 0..count {
            let src = another.get_byte_array(src_start_vpn + offset)?;
            let dst = self.get_byte_array(dst_start_vpn + offset)?;
            dst.copy_from_slice(src);
        }
        Ok(count)
    }

    /// In the same memory space, the tuple of the page range start and end virutal page number is unique,
    /// It can be used as a unique identifier for a area
    ///
    /// - Returns
    ///     - (start_vpn, end_vpn)
    pub(crate) fn range(&self) -> (usize, usize) {
        (
            self.page_range_tracker.start_vpn(),
            self.page_range_tracker.end_vpn(),
        )
    }

    /// Map a virtual page number to a physical page number.
    /// If the area mapping is Idential, the physical page number is equal to the virtual page number,
    /// and will not allocate memory frame
    /// If the area mapping is Framed, area will allocate memory frame, and make the virtual page number mapped to it,
    /// so the physical page number will be almost random
    ///
    /// - Arguments
    ///     - vpn: the virtual page number
    ///
    /// - Errors
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    fn map_one(&mut self, vpn: usize) -> Result<usize> {
        self.page_range_tracker.check(vpn)?;
        let ppn = match self.area_mapping {
            AreaMapping::Identical => {
                self.page_table
                    .exclusive_access()
                    .map_without_alloc(vpn, vpn, self.flags)?;
                vpn
            }
            AreaMapping::Framed => self.page_table.exclusive_access().map(vpn, self.flags)?,
        };
        Ok(ppn)
    }

    /// Unmap a virtal page number.
    /// If the area mapping is Idential, area will only unmap the virtual page number from page table,
    /// If the area mapping is Framed, area will deallocate memory frame and unmap the virtual page number from page table.
    ///
    /// - Arguments
    ///     - vpn: the virtual page number
    ///
    /// - Errors
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    ///     - PPNNotMapped(ppn)
    ///     - DeallocEmptyPageMapper(ppn)
    fn unmap_one(&mut self, vpn: usize) -> Result<usize> {
        self.page_range_tracker.check(vpn)?;
        let ppn = match self.area_mapping {
            AreaMapping::Identical => {
                self.page_table
                    .exclusive_access()
                    .unmap_without_dealloc(vpn)?;
                vpn
            }
            AreaMapping::Framed => self.page_table.exclusive_access().unmap(vpn)?,
        };
        Ok(ppn)
    }

    /// Map all virtual pages
    ///
    /// - Errors
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    fn map(&mut self) -> Result<()> {
        for vpn in self.page_range_tracker.page_range() {
            self.map_one(vpn)?;
        }
        Ok(())
    }

    /// Unallocate all virtual pages
    ///
    /// - Errors
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    ///     - PPNNotMapped(ppn)
    ///     - DeallocEmptyPageMapper(ppn)
    fn unmap(&mut self) -> Result<()> {
        for vpn in self.page_range_tracker.page_range() {
            self.unmap_one(vpn)?;
        }
        Ok(())
    }

    /// # Unsafe
    /// Force convert the vpn binary data into a struct
    /// - Arguments
    ///     - vpn: The virtual page number to convert
    ///     - offset: the first byte offset
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) unsafe fn as_kernel_mut<U>(&self, vpn: usize, offset: usize) -> Result<&mut U> {
        let page_table = self.page_table.access();
        let tracker = page_table.get_tracker_with(vpn)?;
        Ok(tracker.as_kernel_mut::<U>(offset))
    }

    /// Force convert the vpn binary data into a slice of bytes
    /// - Arguments
    ///     - vpn: The virtual page number to convert
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn get_byte_array<'a, 'b>(&'a self, vpn: usize) -> Result<&'b mut PageBytes> {
        let page_table = self.page_table.access();
        let tracker = page_table.get_tracker_with(vpn)?;
        Ok(tracker.get_byte_array())
    }

    /// Write byte data to the page
    /// - Arguments
    ///     - vpn: The virtual page number to write
    ///     - offset: the first byte offset will be written to the page
    ///     - data: The byte data to write
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    fn write_page(&self, vpn: usize, offset: usize, data: &[u8]) -> Result<()> {
        assert_eq!(self.area_mapping, AreaMapping::Framed);
        let dst = self.get_byte_array(vpn)?;
        dst[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    /// Write byte data to the multi continues pages.
    /// - Arguments
    ///     - offset: The byte offset from the beginning of the first virtual page
    ///     - data: The byte data to be written
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn write_multi_pages(&mut self, offset: usize, data: &[u8]) -> Result<()> {
        let length = data.len();
        let linear_end = length + offset;
        assert!(linear_end <= self.page_range_tracker.byte_size());
        let vpn_offset = self.page_range_tracker.start_vpn();
        let mut linear_start = offset;
        while linear_start < linear_end {
            let start = linear_start;
            let end = linear_end.min(linear_start + configs::MEMORY_PAGE_BYTE_SIZE);
            let src = &data[(start - offset)..(end - offset)];
            let vpn = start / configs::MEMORY_PAGE_BYTE_SIZE;
            let per_page_offset = start % configs::MEMORY_PAGE_BYTE_SIZE;
            self.write_page(vpn + vpn_offset, per_page_offset, src)?;
            linear_start = end;
        }
        Ok(())
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        self.unmap().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_area_from_another() {
        let page_table = Arc::new(unsafe { UserPromiseRefCell::new(*PageTable::new(0).unwrap()) });
        let allocator = Arc::new(LinkedListPageRangeAllocator::new(
            0,
            *super::super::MAX_VIRTUAL_PAGE_NUMBER + 1,
        ));
        let area = Area::new(
            0,
            1,
            PageTableFlags::R,
            AreaMapping::Framed,
            &allocator,
            &page_table,
        )
        .unwrap();
        assert!(Area::from_another(&area, &allocator, &page_table).is_err());
        let other_page_table =
            Arc::new(unsafe { UserPromiseRefCell::new(*PageTable::new(1).unwrap()) });
        let other_allocator = Arc::new(LinkedListPageRangeAllocator::new(
            0,
            *super::super::MAX_VIRTUAL_PAGE_NUMBER + 1,
        ));
        let result = Area::from_another(&area, &other_allocator, &other_page_table);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.flags.bits(), area.flags.bits());
        assert_eq!(result.area_mapping, area.area_mapping);
        assert_eq!(
            result.page_range_tracker.start_vpn(),
            area.page_range_tracker.start_vpn()
        );
        assert_eq!(
            result.page_range_tracker.end_vpn(),
            area.page_range_tracker.end_vpn()
        );
    }

    #[test_case]
    fn test_area_write_page() {
        let page_table = Arc::new(unsafe { UserPromiseRefCell::new(*PageTable::new(0).unwrap()) });
        let allocator = Arc::new(LinkedListPageRangeAllocator::new(
            0,
            *super::super::MAX_VIRTUAL_PAGE_NUMBER + 1,
        ));
        let area = Area::new(
            0,
            1,
            PageTableFlags::R,
            AreaMapping::Framed,
            &allocator,
            &page_table,
        )
        .unwrap();
        assert_eq!(
            area.get_byte_array(0).unwrap(),
            &[0; configs::MEMORY_PAGE_BYTE_SIZE]
        );
        assert!(area.write_page(0, 0, &[1u8; 2][0..2]).is_ok());
        assert_eq!(
            &area.get_byte_array(0).unwrap()[0..4],
            &[1u8, 1, 0, 0][0..4]
        );
        assert!(area.write_page(0, 2, &[2u8; 2][0..2]).is_ok());
        assert_eq!(
            &area.get_byte_array(0).unwrap()[0..6],
            &[1u8, 1, 2, 2, 0, 0][0..6]
        );
    }

    #[test_case]
    fn test_area_write_multi_pages() {
        let page_table = Arc::new(unsafe { UserPromiseRefCell::new(*PageTable::new(0).unwrap()) });
        let page_range_allocator = Arc::new(LinkedListPageRangeAllocator::new(
            0,
            *super::super::MAX_VIRTUAL_PAGE_NUMBER + 1,
        ));
        let mut area = Area::new(
            0,
            3,
            PageTableFlags::R,
            AreaMapping::Framed,
            &page_range_allocator,
            &page_table,
        )
        .unwrap();
        assert_eq!(
            area.get_byte_array(0).unwrap(),
            &[0; configs::MEMORY_PAGE_BYTE_SIZE]
        );
        assert_eq!(
            area.get_byte_array(1).unwrap(),
            &[0; configs::MEMORY_PAGE_BYTE_SIZE]
        );
        assert_eq!(
            area.get_byte_array(2).unwrap(),
            &[0; configs::MEMORY_PAGE_BYTE_SIZE]
        );
        assert!(area.write_multi_pages(0, &[1u8; 2][0..2]).is_ok());

        assert_eq!(&area.get_byte_array(0).unwrap()[0..2], &[1, 1][0..2]);
        assert_eq!(
            area.get_byte_array(1).unwrap(),
            &[0; configs::MEMORY_PAGE_BYTE_SIZE]
        );
        assert_eq!(
            area.get_byte_array(2).unwrap(),
            &[0; configs::MEMORY_PAGE_BYTE_SIZE]
        );

        assert!(area
            .write_multi_pages(configs::MEMORY_PAGE_BYTE_SIZE, &[1u8; 2][0..2])
            .is_ok());
        assert_eq!(&area.get_byte_array(0).unwrap()[0..2], &[1, 1][0..2]);
        assert_eq!(&area.get_byte_array(1).unwrap()[0..2], &[1, 1][0..2]);
        assert_eq!(
            area.get_byte_array(2).unwrap(),
            &[0; configs::MEMORY_PAGE_BYTE_SIZE]
        );

        assert!(area
            .write_multi_pages(configs::MEMORY_PAGE_BYTE_SIZE * 2 + 1, &[1u8; 2][0..2])
            .is_ok());
        assert_eq!(&area.get_byte_array(0).unwrap()[0..2], &[1, 1][0..2]);
        assert_eq!(&area.get_byte_array(1).unwrap()[0..2], &[1, 1][0..2]);
        assert_eq!(&area.get_byte_array(2).unwrap()[0..3], &[0, 1, 1][0..3]);
    }
}

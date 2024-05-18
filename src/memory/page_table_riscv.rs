// @author:    olinex
// @time:      2023/09/06

// self mods

// use other mods
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use bit_field::BitField;
use core::ops::{AddAssign, Range, SubAssign};
use riscv::register::satp::Mode;

// use self mods
use super::{frame, PageTableFlags, PageTableTr};
use crate::configs;
use crate::lang::container::UserPromiseRefCell;
use crate::prelude::*;

/// The offset of the vpn/ppn
const OFFSET_RANGE: Range<usize> = 0..12;

/// Page table entry(PTE)
/// Each page table have 512 PTE, each PTE have 64 bits
/// 
/// Page table entry binary composition structure
/// ```
/// ----------------------------------------------------------------
/// |   10   ||                   44                     |  |  08  |
/// |reversed||                  ppn                     |  | flag  |
/// ----------------------------------------------------------------
/// ```
const PTE_BYTE_SIZE: usize = 8;
const PTE_OFFSET_BIT_SIZE: usize = 9;
const PTE_COUNT: usize = configs::MEMORY_PAGE_BYTE_SIZE / PTE_BYTE_SIZE;
const PTE_FLAGS_RANGE: Range<usize> = 0..8;
const PTE_PPN_RANGE: Range<usize> = 10..54;

/// Memory manager unit binary composition structure
/// ```
/// ----------------------------------------------------------------
/// |04||      16      ||                  44                      |
/// |mo||     asid     ||                  ppn                     |
/// ----------------------------------------------------------------
/// ```
const MMU_PPN_RANGE: Range<usize> = 0..44;
const MMU_ASID_RANGE: Range<usize> = 44..60;
const MMU_MODE_RANGE: Range<usize> = 60..64;

pub(crate) const MAX_TASK_ID: usize = (1 << (MMU_ASID_RANGE.end - MMU_ASID_RANGE.start)) - 1;

cfg_if! {
    if #[cfg(all(feature = "mmu_sv39", target_arch = "riscv64"))] {

        const MMU_MODE: Mode = Mode::Sv39;
        const PAGE_LEVEL: usize = 3;
        /// Virtual address binary composition structure
        /// ```
        /// ----------------------------------------------------------------
        /// |                       ||   9   ||   9   ||   9   ||    12    |
        /// |       reversed        || ppn03 || ppn02 || ppn01 ||  offset  |
        /// ----------------------------------------------------------------
        /// ```
        const VA_PN_RANGE: Range<usize> = 12..39;
        const VA_RESERVED_RANGE: Range<usize> = 39..64;
        /// Physical address binary composition structure
        /// ```
        /// ----------------------------------------------------------------
        /// |      ||                   44                     ||    12    |
        /// | reve ||                  ppn                     ||  offset  |
        /// ----------------------------------------------------------------
        /// ```
        const PA_PN_RANGE: Range<usize> = 12..56;
    } else {
        compile_error!("Unsupported address mmu mode for riscv");
    }
}

bitflags! {
    /// The flags of the page table entry.
    /// This structure is only used in riscv
    #[derive(PartialEq, Eq)]
    pub(crate) struct PTEFlags: u8 {
        /// Is valid
        const V = 1 << 0;
        /// Is readable
        const R = 1 << 1;
        /// Is writeable
        const W = 1 << 2;
        /// Is executable
        const X = 1 << 3;
        /// Is user accessible
        const U = 1 << 4;
        /// Ignore
        const G = 1 << 5;
        /// Have accessed
        const A = 1 << 6;
        /// Is dirty(changed)
        const D = 1 << 7;
    }
}

/// The entry of the page table which records the relationship of the vpn and the ppn
#[derive(Copy, Clone)]
#[repr(C)]
pub(crate) struct PageTableEntry {
    bits: usize,
}
impl PageTableEntry {
    /// Create a new page table entry
    ///
    /// - Arguments
    ///     - ppn: the physical page number which the PTE refers to
    ///     - flags: permission and some other flag bits
    pub(crate) fn new(ppn: usize, flags: PTEFlags) -> Self {
        let mut bits = 0;
        bits.set_bits(PTE_PPN_RANGE, ppn)
            .set_bits(PTE_FLAGS_RANGE, flags.bits() as usize);
        Self { bits }
    }

    /// Create a new empty page table which is invalid
        pub(crate) fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }

    /// Get the physical page number as usize
        pub(crate) fn ppn(&self) -> usize {
        self.bits.get_bits(PTE_PPN_RANGE)
    }

    /// Get the PTE flags
        pub(crate) fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }

    /// Check current PTE validation
        pub(crate) fn is_valid(&self) -> bool {
        self.flags().contains(PTEFlags::V)
    }
}

/// The array of the PTE of the total page
type PTEArray = [PageTableEntry; PTE_COUNT];

/// The page mapper which contain the PTE
pub(crate) struct PageMapper {
    /// The physical page number of the parent page mapper,
    /// Which maybe None when page mapper is the root mapper
    parent: Option<usize>,
    /// The count of the PTE which are valid,
    /// When count is zero, mapper will be removed and the frame will be dealloc
    count: UserPromiseRefCell<usize>,
    /// The tracker of the frame which contains the page mapper,
    /// It will be dropped when page mapper is destroyed
    tracker: frame::FrameTracker,
}
impl PageMapper {
    /// Create a new page table by allocated frame
    ///
    /// - Arguments
    ///     - parent: the physical page number of the parent page mapper
    ///     - tracker: the page mapper which contains the page mapper
    fn new(parent: Option<usize>, tracker: frame::FrameTracker) -> Self {
        Self {
            parent,
            count: unsafe { UserPromiseRefCell::new(0) },
            tracker,
        }
    }

    /// Get the array of the PTE in the page mapper
        fn get_pte_array(&self) -> &mut PTEArray {
        unsafe { self.tracker.as_kernel_mut(0) }
    }

    /// Get the physcial page number of the page mapper
        fn ppn(&self) -> usize {
        self.tracker.ppn()
    }

    /// Check if the page mapper have no valid PTE
        fn is_empty(&self) -> bool {
        *self.count.access() == 0
    }

    /// Check if teh pge mapper's PTE are all valid
        fn is_full(&self) -> bool {
        *self.count.access() as usize == PTE_COUNT
    }

    /// Increase the valid PTE count and return increased count
    ///
    /// - Errors
    ///     - AllocFullPageMapper(ppn)
        fn incr(&self) -> Result<usize> {
        if self.is_full() {
            Err(KernelError::AllocFullPageMapper(self.ppn()))
        } else {
            self.count.exclusive_access().add_assign(1);
            Ok(*self.count.access())
        }
    }

    /// Decrease the valid PTE count and return the decreased count
    ///
    /// - Errors
    ///     - DeallocEmptyPageMapper(ppn)
        fn decr(&self) -> Result<usize> {
        if self.is_empty() {
            Err(KernelError::DeallocEmptyPageMapper(self.ppn()))
        } else {
            self.count.exclusive_access().sub_assign(1);
            Ok(*self.count.access())
        }
    }
}

/// The page table abstract struct
pub(crate) struct PageTable {
    /// The address space id of the page table
    asid: usize,
    /// The root page mapper of the current page table
    root: PageMapper,
    /// The other page mappers of the current page table, mapped ppn as key
    mappers: BTreeMap<usize, PageMapper>,
    /// The trackers of the frames, mapped vpn as key
    trackers: BTreeMap<usize, frame::FrameTracker>,
}
impl PageTable {
    /// Get the indexes of the PTE in the page mapper.
    ///
    /// - Arguments
    ///     - vpn: the virtual page number
    ///
    /// - Returns:
    /// Depending on the mechanism of multi-level page tables,
    /// The length of the returned in-page index is also different.
    ///     - Sv39:
    ///     return [usize; 3]: [ppn03(30..39), ppn02(21..30), ppn01(12..21)]
    fn page_indexes(vpn: usize) -> [usize; PAGE_LEVEL] {
        let mut indexes = [0; PAGE_LEVEL];
        let mut offset = 0;
        for i in (0..PAGE_LEVEL).rev() {
            let start = offset;
            offset += PTE_OFFSET_BIT_SIZE;
            indexes[i] = vpn.get_bits(start..offset);
        }
        indexes
    }
}
impl PageTableTr for PageTable {
    /// Help function to get physical page number with physical address
        fn get_ppn_with(pa: usize) -> usize {
        pa.get_bits(PA_PN_RANGE)
    }

    /// Help function to get virtual page number with virtual address
        fn get_vpn_with(va: usize) -> usize {
        va.get_bits(VA_PN_RANGE)
    }

    /// Help function to get address offset with physical address
        fn get_pa_offset(pa: usize) -> usize {
        pa.get_bits(OFFSET_RANGE)
    }

    /// Help function get get address offset with virtual address
        fn get_va_offset(va: usize) -> usize {
        Self::get_pa_offset(va)
    }

    /// Calculate base virtual address with virtual page number,
    /// The value range of the virtual address has certain constraints,
    /// and the value of the high bit must be the same as the first bit of the physical page number
    /// - Arguments
    ///     - vpn: the virtual page number
    fn cal_base_va_with(vpn: usize) -> usize {
        let mut va = vpn << configs::MEMORY_PAGE_BIT_SITE;
        let sign = va.get_bit(VA_PN_RANGE.end - 1);
        if sign {
            let reserved = (1 << (VA_RESERVED_RANGE.end - VA_RESERVED_RANGE.start)) - 1;
            va.set_bits(VA_RESERVED_RANGE, reserved);
        };
        va
    }

    /// Create a new page table, data will be wrapped by box and save to heap,
    /// because the method was declare in trait and return value must have certain size shen compile-time
    /// When creating the page table, the root page mapper's physical frame must be allocated
    ///
    /// - Arguments
    ///     - asid: address space id
    ///
    /// - Errors
    ///     - FrameExhausted
    fn new(asid: usize) -> Result<Box<Self>> {
        let tracker = frame::FRAME_ALLOCATOR.alloc()?;
        Ok(Box::new(Self {
            asid,
            root: PageMapper::new(None, tracker),
            mappers: BTreeMap::new(),
            trackers: BTreeMap::new(),
        }))
    }

    /// Get the asid of the page table
        fn asid(&self) -> usize {
        self.asid
    }

    /// Get the physical page number of the page table's root page mapper
        fn ppn(&self) -> usize {
        self.root.ppn()
    }

    /// Get the memory manger unit token value of the page table
        fn mmu_token(&self) -> usize {
        let mut token = 0;
        token
            .set_bits(MMU_MODE_RANGE, MMU_MODE as usize)
            .set_bits(MMU_ASID_RANGE, self.asid)
            .set_bits(MMU_PPN_RANGE, self.ppn());
        token
    }

    /// Establish mappings in virtual and physical page numbers.
    /// When creating a mapping, we may allocate frame as page mapper,
    /// and increase the counter of the used PTE.
    /// Please notice that this method will not alloc frame,
    /// the frame which is referenced by the ppn argument can be allocated by yourself
    ///
    /// - Arguments
    ///     - vpn: The virtual page number
    ///     - ppn: The physical page number
    ///     - flags: The flags of the PTE
    /// 
    /// - Errors
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    fn map_without_alloc(&mut self, vpn: usize, ppn: usize, flags: PageTableFlags) -> Result<()> {
        // force mark the flag if `valid` to be ture
        let bits = flags.bits() | PTEFlags::V.bits();
        // convert PagetableFlags to PTEFlags
        let flags =
            PTEFlags::from_bits(bits).ok_or(KernelError::InvaidPageTablePerm(bits as usize))?;
        // get the physical page number indexes
        let indexes = Self::page_indexes(vpn);
        let last = PAGE_LEVEL - 1;
        let mut mapper = &self.root;
        // find or create a next page mapper
        for i in 0..last {
            let entries = mapper.get_pte_array();
            let entry = &mut entries[indexes[i]];
            // check if the entry is valid
            // if entry is invalid, we should create a new page mapper
            // and write the entry to the parent page mapper
            // which entry is referenced to the currently created page mapper
            let child_ppn = if !entry.is_valid() {
                let tracker = frame::FRAME_ALLOCATOR.alloc()?;
                let child_ppn = tracker.ppn();
                *entry = PageTableEntry::new(child_ppn, PTEFlags::V);
                // increase the parent page mapper valid entries count
                let child_mapper = PageMapper::new(Some(mapper.ppn()), tracker);
                mapper.incr()?;
                if let Some(_) = self.mappers.insert(child_ppn, child_mapper) {
                    return Err(KernelError::PPNAlreadyMapped(child_ppn));
                }
                child_ppn
            } else {
                entry.ppn()
            };
            // when entry is valid, the next page mapper is created,
            // so we only need to find the next page mapper by index.
            mapper = self
                .mappers
                .get(&child_ppn)
                .ok_or(KernelError::PPNNotMapped(child_ppn))?;
        }
        let entries = mapper.get_pte_array();
        let entry = &mut entries[indexes[last]];
        // create a new entry to the page mapper which is referenced to the ppn
        if !entry.is_valid() {
            *entry = PageTableEntry::new(ppn, flags);
            // increase the parent page mapper valid entries count
            mapper.incr()?;
            Ok(())
        } else {
            Err(KernelError::VPNAlreadyMapped(vpn))
        }
    }

    /// Delete the page table entry which is referenced to the virtual page number argument.
    /// Each time deleteing a page table entry, we will check the parent page mapper's valid entries count.
    /// If all the entries in the page mapper is invalid, it means the frame of the page mapper can be deallocated.
    /// If the current page mapper is removed, we maybe need to loop up several times to find the empty one.
    ///
    /// - Arguments
    ///     - vpn: the virtual page number which will be dealloc
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    ///     - PPNNotMapped(ppn)
    ///     - DeallocEmptyPageMapper(ppn)
    fn unmap_without_dealloc(&mut self, vpn: usize) -> Result<usize> {
        // get the physical page number indexes
        let indexes = Self::page_indexes(vpn);
        let last = PAGE_LEVEL - 1;
        // the physical page number which vpn arugment is mapped
        let mut return_ppn = 0;
        let mut mapper = &self.root;
        // find the next page mapper
        for i in 0..last {
            let entries = mapper.get_pte_array();
            let entry = &entries[indexes[i]];
            // All related page mapper must be valid
            if !entry.is_valid() {
                return Err(KernelError::VPNNotMapped(vpn));
            }
            return_ppn = entry.ppn();
            mapper = self
                .mappers
                .get(&return_ppn)
                .ok_or(KernelError::PPNNotMapped(return_ppn))?;
        }
        // the physical page number which will be removed
        let mut remove_ppn = return_ppn;
        // Trace back the page mapper, check the currently valid PTE,
        // if the page mapper is empty remove the current page index table,
        // and delete its own PTE from the parent mapper.
        // Continue this operation until the page mapper is not empty or reaches the root page mapper
        for i in (0..=last).rev() {
            let entries = mapper.get_pte_array();
            let entry = &mut entries[indexes[i]];
            if !entry.is_valid() {
                return Err(KernelError::VPNNotMapped(vpn));
            }
            // find the ppn which vpn aregument referenced
            if i == last {
                return_ppn = entry.ppn();
            }
            mapper.decr()?;
            // make page table entry invalid as empty
            *entry = PageTableEntry::empty();
            // when i equals to 0, it means current page mapper is the root page mapper,
            // no need to continue or we will get an panic
            if !mapper.is_empty() || i == 0 {
                break;
            }
            // when we match a page mapper have no parent page mapper
            // it means we have got a bad mistaken
            let parent = match mapper.parent {
                Some(p) => p,
                None => unreachable!(),
            };
            self.mappers
                .remove(&remove_ppn)
                .ok_or(KernelError::PPNNotMapped(remove_ppn))?;
            // make mapper pointing to the parent mapper
            // when i equals to 1, the parent page mapper is root page mapper,
            // but root page mapper isn't in the mappers map
            if i != 1 {
                mapper = self
                    .mappers
                    .get(&parent)
                    .ok_or(KernelError::PPNNotMapped(parent))?;
            } else {
                mapper = &self.root;
            }
            remove_ppn = mapper.ppn();
        }
        Ok(return_ppn)
    }

    /// Establish mapping in virtual and physical page numbers and allocating memory frame.
    ///
    /// - Arguments
    ///     - vpn: the virtual page number which we want to map
    ///     - flags: the flags of the PTE
    ///
    /// - Errors
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags) 
    ///     - FrameExhausted 
    ///     - AllocFullPageMapper(ppn) 
    ///     - PPNAlreadyMapped(ppn) 
    ///     - PPNNotMapped(ppn)
    fn map(&mut self, vpn: usize, flags: PageTableFlags) -> Result<usize> {
        if !flags.is_empty() {
            let tracker = frame::FRAME_ALLOCATOR.alloc()?;
            let ppn = tracker.ppn();
            self.map_without_alloc(vpn, ppn, flags)?;
            match self.trackers.insert(vpn, tracker) {
                Some(_) => Err(KernelError::VPNAlreadyMapped(vpn)),
                None => Ok(ppn),
            }
        } else {
            Err(KernelError::InvaidPageTablePerm(flags.bits() as usize))
        }
    }

    /// Delete the PTE which references the vpn and deallocates the frames
    ///
    /// - Arguments
    ///     - vpn: the virtual page number which we want to unmap
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    ///     - PPNNotMapped(ppn)
    ///     - DeallocEmptyPageMapper(ppn)
    fn unmap(&mut self, vpn: usize) -> Result<usize> {
        let ppn = self.unmap_without_dealloc(vpn)?;
        match self.trackers.remove(&vpn) {
            None => Err(KernelError::VPNNotMapped(vpn)),
            Some(_) => Ok(ppn),
        }
    }

    /// Transalte the virtual page number to the physical page number according to the page table.
    /// If vpn is not specified then we will return None
    ///
    /// - Arguments
    ///     - vpn: the virtual page number
    ///
    /// - Returns
    ///     - Some(ppn)
    ///     - None
    fn translate_ppn_with(&self, vpn: usize) -> Option<usize> {
        let indexes = Self::page_indexes(vpn);
        let last = PAGE_LEVEL - 1;
        let mut entries = self.root.get_pte_array();
        for i in 0..last {
            let entry = &entries[indexes[i]];
            if !entry.is_valid() {
                return None;
            }
            let mapper = self.mappers.get(&entry.ppn())?;
            entries = mapper.get_pte_array();
        }
        let entry = &entries[indexes[last]];
        if entry.is_valid() {
            Some(entry.ppn())
        } else {
            None
        }
    }

    /// Get the frame tracker by virtual page number
    ///
    /// - Arguments
    ///     - vpn: the virtual page number
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    fn get_tracker_with(&self, vpn: usize) -> Result<&frame::FrameTracker> {
        self.trackers
            .get(&vpn)
            .ok_or(KernelError::VPNNotMapped(vpn))
    }

    /// Returns data within a physical page according to the specified data structure
    ///
    /// - Arguments
    ///     - vpn: the virtual page number
    ///     - offset: The offset of the specified data structure, start from 0
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    fn as_kernel_mut<'a, 'b, U>(&self, vpn: usize, offset: usize) -> Result<&'b mut U> {
        let tracker = self.get_tracker_with(vpn)?;
        Ok(unsafe { tracker.as_kernel_mut(offset) })
    }

    /// Get the physical memory data from the frame as u8 array
    ///
    /// - Arguments
    ///     - vpn: the virtual page number
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    fn get_byte_array<'a, 'b>(
        &'a self,
        vpn: usize,
    ) -> Result<&'b mut [u8; configs::MEMORY_PAGE_BYTE_SIZE]> {
        let tracker = self.get_tracker_with(vpn)?;
        Ok(tracker.get_byte_array())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    #[test_case]
    fn test_lazy_static() {
        assert_ne!(*MAX_VIRTUAL_PAGE_NUMBER, 0);
        assert_ne!(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER, 0);
        assert_ne!(*TRAMPOLINE_PHYSICAL_PAGE_NUMBER, 0);
        assert_ne!(*TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER, 0);
    }

    #[test_case]
    fn test_pte_is_valid() {
        assert!(!PageTableEntry::empty().is_valid());
        assert!(!PageTableEntry::new(0, PTEFlags::R).is_valid());
        assert!(PageTableEntry::new(0, PTEFlags::V).is_valid());
    }

    #[test_case]
    fn test_pte_ppn() {
        assert_eq!(PageTableEntry::empty().ppn(), 0);
        assert_eq!(PageTableEntry::new(0, PTEFlags::V).ppn(), 0);
        assert_eq!(PageTableEntry::new(1, PTEFlags::V).ppn(), 1);
        assert_ne!(PageTableEntry::new(1, PTEFlags::V).ppn(), 2);
    }

    #[test_case]
    fn test_pte_flags() {
        assert_eq!(PageTableEntry::empty().flags().bits(), 0);
        assert_eq!(
            PageTableEntry::new(0, PTEFlags::V).flags().bits(),
            PTEFlags::V.bits()
        );
        assert_eq!(
            PageTableEntry::new(0, PTEFlags::D).flags().bits(),
            PTEFlags::D.bits()
        );
        assert_ne!(
            PageTableEntry::new(0, PTEFlags::D).flags().bits(),
            PTEFlags::V.bits()
        );
    }

    #[test_case]
    fn test_pagetable_page_indexes() {
        assert_eq!(PageTable::page_indexes(0), [0; 3]);
        assert_eq!(PageTable::page_indexes(1), [0, 0, 1]);
        assert_eq!(PageTable::page_indexes(PTE_COUNT), [0, 1, 0]);
        assert_eq!(
            PageTable::page_indexes(PTE_COUNT - 1),
            [0, 0, PTE_COUNT - 1]
        );
        assert_eq!(PageTable::page_indexes(PTE_COUNT + 1), [0, 1, 1]);
        assert_eq!(PageTable::page_indexes(PTE_COUNT * PTE_COUNT), [1, 0, 0]);
        assert_eq!(
            PageTable::page_indexes(PTE_COUNT * PTE_COUNT - 1),
            [0, PTE_COUNT - 1, PTE_COUNT - 1]
        );
    }

    #[test_case]
    fn test_pagetable_asid() {
        assert!(PageTable::new(0).is_ok_and(|f| f.asid() == 0));
        assert!(PageTable::new(1).is_ok_and(|f| f.asid() == 1));
        assert!(PageTable::new(2).is_ok_and(|f| f.asid() == 2));
    }

    #[test_case]
    fn test_pagetable_cal_base_va_with() {
        assert_eq!(PageTable::cal_base_va_with(0), 0);
        assert_eq!(
            PageTable::cal_base_va_with(1),
            configs::MEMORY_PAGE_BYTE_SIZE
        );
        assert_eq!(
            PageTable::cal_base_va_with(PageTable::get_vpn_with(
                configs::TRAMPOLINE_VIRTUAL_BASE_ADDR
            )),
            configs::TRAMPOLINE_VIRTUAL_BASE_ADDR
        );
        assert_eq!(
            PageTable::cal_base_va_with(134217727),
            configs::TRAMPOLINE_VIRTUAL_BASE_ADDR
        )
    }

    #[test_case]
    fn test_pagetable_alloc_and_dealloc() {
        let mut page_table = PageTable::new(0).unwrap();
        assert!(page_table
            .map_without_alloc(0, 0, PageTableFlags::R)
            .is_ok());
        assert!(page_table
            .translate_ppn_with(0)
            .is_some_and(|ppn| ppn == 0));
        assert!(page_table
            .unmap_without_dealloc(0)
            .is_ok_and(|ppn| ppn == 0));

        assert!(page_table
            .map_without_alloc(1, 1, PageTableFlags::R)
            .is_ok());
        assert!(page_table
            .translate_ppn_with(1)
            .is_some_and(|ppn| ppn == 1));
        assert!(page_table
            .unmap_without_dealloc(1)
            .is_ok_and(|ppn| ppn == 1));

        assert!(page_table
            .map_without_alloc(
                *TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
                *TRAMPOLINE_PHYSICAL_PAGE_NUMBER,
                PageTableFlags::R
            )
            .is_ok());
        assert!(page_table
            .translate_ppn_with(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER)
            .is_some_and(|ppn| ppn == *TRAMPOLINE_PHYSICAL_PAGE_NUMBER));
        assert!(page_table
            .unmap_without_dealloc(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER)
            .is_ok_and(|ppn| ppn == *TRAMPOLINE_PHYSICAL_PAGE_NUMBER));
    }

    #[test_case]
    fn test_pagetable_map_and_unmap() {
        let mut page_table = PageTable::new(0).unwrap();
        assert!(page_table
            .get_tracker_with(0)
            .is_err_and(|e| e.is_vpnnotmapped()));
        assert!(page_table
            .map(0, PageTableFlags::EMPTY)
            .is_err_and(|e| e.is_invaidpagetableperm()));
        assert!(page_table.map(0, PageTableFlags::R).is_ok());
        assert!(page_table
            .get_tracker_with(0)
            .is_ok_and(|tracker| tracker.ppn() == page_table.translate_ppn_with(0).unwrap()));
        assert!(page_table.unmap(0).is_ok());
        assert!(page_table
            .get_tracker_with(0)
            .is_err_and(|e| e.is_vpnnotmapped()));
        assert!(page_table.map(0, PageTableFlags::R).is_ok());
    }

    #[test_case]
    fn test_pagetable_get_byte_array() {
        let mut page_table = PageTable::new(0).unwrap();
        assert!(page_table
            .get_byte_array(0)
            .is_err_and(|err| err.is_vpnnotmapped()));
        assert!(page_table.map(0, PageTableFlags::R).is_ok());
        let bytes = page_table.get_byte_array(0).unwrap();
        bytes[0] = 1u8;
        bytes[configs::MEMORY_PAGE_BYTE_SIZE - 1] = 1u8;
        let bytes = page_table.get_byte_array(0).unwrap();
        assert_eq!(bytes[0], 1u8);
        assert_eq!(bytes[configs::MEMORY_PAGE_BYTE_SIZE - 1], 1u8);
        assert!(page_table.unmap(0).is_ok());
        assert!(page_table
            .get_byte_array(0)
            .is_err_and(|err| err.is_vpnnotmapped()));
        assert!(page_table.map(0, PageTableFlags::R).is_ok());
        let bytes = page_table.get_byte_array(0).unwrap();
        assert_eq!(bytes[0], 0u8);
        assert_eq!(bytes[configs::MEMORY_PAGE_BYTE_SIZE - 1], 0u8);
    }
}

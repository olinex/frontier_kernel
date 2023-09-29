// @author:    olinex
// @time:      2023/09/06

// self mods

// use other mods
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use bit_field::BitField;
use core::ops::Range;
use riscv::register::satp::Mode;

// use self mods
use super::{frame, PageTableFlags, PageTableTr};
use crate::configs;
use crate::prelude::*;

const OFFSET_RAGE: Range<usize> = 0..12;
const PTE_BYTE_SIZE: usize = 8;
const PTE_OFFSET_BIT_SIZE: usize = 9;
const PTE_COUNT: usize = configs::MEMORY_PAGE_BYTE_SIZE / PTE_BYTE_SIZE;
const PTE_FLAGS_RANGE: Range<usize> = 0..8;
const PTE_PPN_RANGE: Range<usize> = 10..54;
const PTE_RESERVED_RANGE: Range<usize> = 54..64;
const MMU_PPN_RANGE: Range<usize> = 0..44;
const MMU_ASID_RANGE: Range<usize> = 44..60;
const MMU_MODE_RANGE: Range<usize> = 60..64;

pub const MAX_TASK_ID: usize = (1 << (MMU_ASID_RANGE.end - MMU_ASID_RANGE.start)) - 1;

cfg_if! {
    if #[cfg(all(feature = "mmu_sv39", target_arch = "riscv64"))] {
        const MMU_MODE: Mode = Mode::Sv39;
        const PAGE_LEVEL: usize = 3;
        const VIRTUAL_PAGE_BIT_SIZE: usize = 27;
        const VIRTUAL_PAGE_RANGE: Range<usize> = 12..39;
        const PHYSICAL_PAGE_RANGE: Range<usize> = 12..56;
    } else {
        compile_error!("Unsupported address mmu mode for riscv");
    }
}

bitflags! {
    #[derive(PartialEq, Eq)]
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct PageTableEntry {
    bits: usize,
}
impl PageTableEntry {
    #[inline(always)]
    pub fn new(ppn: usize, flags: PTEFlags) -> Self {
        let mut bits = 0;
        bits.set_bits(PTE_PPN_RANGE, ppn)
            .set_bits(PTE_FLAGS_RANGE, flags.bits() as usize);
        Self { bits }
    }

    #[inline(always)]
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }

    #[inline(always)]
    pub fn reserved(&self) -> usize {
        self.bits.get_bits(PTE_RESERVED_RANGE)
    }

    #[inline(always)]
    pub fn asid(&self) -> usize {
        self.bits.get_bits(PTE_RESERVED_RANGE)
    }

    #[inline(always)]
    pub fn ppn(&self) -> usize {
        self.bits.get_bits(PTE_PPN_RANGE)
    }

    #[inline(always)]
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }

    #[inline(always)]
    pub fn is_valid(&self) -> bool {
        self.flags() & PTEFlags::V == PTEFlags::V
    }
}

type PTESlice = [PageTableEntry; PTE_COUNT];

pub struct PageMapper {
    parent: Option<usize>,
    count: u16,
    tracker: frame::FrameTracker,
}
impl PageMapper {
    fn new(parent: Option<usize>, tracker: frame::FrameTracker) -> Self {
        Self {
            parent,
            count: 0,
            tracker,
        }
    }

    #[inline(always)]
    fn get_pte_array(&self) -> &mut PTESlice {
        unsafe { self.tracker.as_kernel_mut(0) }
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline(always)]
    fn ppn(&self) -> usize {
        self.tracker.ppn()
    }
}

pub struct PageTable {
    asid: usize,
    root: PageMapper,
    mappers: BTreeMap<usize, PageMapper>,
    trackers: BTreeMap<usize, frame::FrameTracker>,
}
impl PageTable {
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

    fn find_next_entries(&self, parent: &PTESlice, offset: usize) -> Option<&mut PTESlice> {
        let entry = &parent[offset];
        if entry.is_valid() {
            let mapper = self.mappers.get(&entry.ppn())?;
            Some(mapper.get_pte_array())
        } else {
            None
        }
    }

    fn print_mapper_as_entries(&self, mapper: &PageMapper, level: usize, print_header: bool) {
        if level >= PAGE_LEVEL {
            return;
        }
        if print_header {
            info!("offset|  reserved|          asid|           ppn|rswdaguxwrv");
        }
        let entries = mapper.get_pte_array();
        for (offset, entry) in entries.iter().enumerate() {
            if !entry.is_valid() {
                continue;
            }
            info!(
                "{:>6}|{:>10}|{:>14}|{:>14}| {:010b}",
                offset,
                entry.reserved(),
                entry.asid(),
                entry.ppn(),
                entry.flags(),
            );
            if let Some(child_mapper) = self.mappers.get(&entry.ppn()) {
                self.print_mapper_as_entries(child_mapper, level + 1, false);
            };
        }
    }
}
impl PageTableTr for PageTable {
    #[inline(always)]
    fn cal_ppn_with(pa: usize) -> usize {
        pa.get_bits(PHYSICAL_PAGE_RANGE)
    }

    #[inline(always)]
    fn cal_vpn_with(va: usize) -> usize {
        va.get_bits(VIRTUAL_PAGE_RANGE)
    }

    #[inline(always)]
    fn cal_pa_offset(pa: usize) -> usize {
        pa.get_bits(OFFSET_RAGE)
    }

    #[inline(always)]
    fn cal_va_offset(va: usize) -> usize {
        Self::cal_pa_offset(va)
    }

    fn cal_base_va_with(vpn: usize) -> usize {
        let vpn = vpn.get_bits(0..VIRTUAL_PAGE_BIT_SIZE);
        let sign = vpn.get_bit(VIRTUAL_PAGE_RANGE.end - configs::MEMORY_PAGE_BIT_SITE - 1);
        let vpn = if sign {
            let reserved = (1 << (configs::ARCH_WORD_SIZE - VIRTUAL_PAGE_RANGE.end)) - 1;
            (reserved << VIRTUAL_PAGE_BIT_SIZE) + vpn
        } else {
            vpn
        };
        vpn << configs::MEMORY_PAGE_BIT_SITE
    }

    fn new(asid: usize) -> Result<Box<Self>> {
        let tracker = frame::FRAME_ALLOCATOR.alloc()?;
        Ok(Box::new(Self {
            asid,
            root: PageMapper::new(None, tracker),
            mappers: BTreeMap::new(),
            trackers: BTreeMap::new(),
        }))
    }

    #[inline(always)]
    fn asid(&self) -> usize {
        self.asid
    }

    #[inline(always)]
    fn ppn(&self) -> usize {
        self.root.ppn()
    }

    #[inline(always)]
    fn mmu_token(&self) -> usize {
        let mut token = 0;
        token
            .set_bits(MMU_MODE_RANGE, MMU_MODE as usize)
            .set_bits(MMU_ASID_RANGE, self.asid)
            .set_bits(MMU_PPN_RANGE, self.ppn());
        token
    }

    fn map_without_alloc(&mut self, vpn: usize, ppn: usize, flags: PageTableFlags) -> Result<()> {
        let bits = flags.bits() | PTEFlags::V.bits();
        let flags =
            PTEFlags::from_bits(bits).ok_or(KernelError::InvaidPageTablePerm(bits as usize))?;
        let indexes = Self::page_indexes(vpn);
        let last = PAGE_LEVEL - 1;
        let mut mapper = &self.root;
        for i in 0..last {
            let entries = mapper.get_pte_array();
            let entry = &mut entries[indexes[i]];
            let child_ppn = if !entry.is_valid() {
                let tracker = frame::FRAME_ALLOCATOR.alloc()?;
                let child_ppn = tracker.ppn();
                *entry = PageTableEntry::new(child_ppn, PTEFlags::V);
                self.mappers
                    .insert(child_ppn, PageMapper::new(Some(mapper.ppn()), tracker));
                child_ppn
            } else {
                entry.ppn()
            };
            mapper = self
                .mappers
                .get(&child_ppn)
                .ok_or(KernelError::PPNNotMapped(child_ppn))?;
        }
        let entries = mapper.get_pte_array();
        let entry = &mut entries[indexes[last]];
        if !entry.is_valid() {
            *entry = PageTableEntry::new(ppn, flags);
            Ok(())
        } else {
            Err(KernelError::VPNAlreadyMapped(vpn))
        }
    }

    fn unmap_without_dealloc(&mut self, vpn: usize) -> Result<usize> {
        let indexes = Self::page_indexes(vpn);
        let last = PAGE_LEVEL - 1;
        let mut entries = self.root.get_pte_array();
        for i in 0..last {
            entries = self
                .find_next_entries(entries, indexes[i])
                .ok_or(KernelError::VPNNotMapped(vpn))?;
        }
        let entry = &mut entries[indexes[last]];
        if entry.is_valid() {
            let ppn = entry.ppn();
            *entry = PageTableEntry::empty();
            Ok(ppn)
        } else {
            Err(KernelError::VPNNotMapped(vpn))
        }
    }

    fn map(&mut self, vpn: usize, flags: PageTableFlags) -> Result<usize> {
        let tracker = frame::FRAME_ALLOCATOR.alloc()?;
        let ppn = tracker.ppn();
        self.map_without_alloc(vpn, ppn, flags)?;
        match self.trackers.insert(ppn, tracker) {
            Some(_) => Err(KernelError::PPNAlreadyMapped(ppn)),
            None => Ok(ppn),
        }
    }

    fn unmap(&mut self, vpn: usize) -> Result<usize> {
        let ppn = self.unmap_without_dealloc(vpn)?;
        match self.trackers.remove(&ppn) {
            Some(_) => Ok(ppn),
            None => Err(KernelError::PPNNotMapped(ppn)),
        }
    }

    fn translate_ppn_with(&self, vpn: usize) -> Option<usize> {
        let indexes = Self::page_indexes(vpn);
        let last = PAGE_LEVEL - 1;
        let mut entries = self.root.get_pte_array();
        for i in 0..last {
            entries = self.find_next_entries(entries, indexes[i])?;
        }
        let entry = &entries[indexes[last]];
        if entry.is_valid() {
            Some(entry.ppn())
        } else {
            None
        }
    }

    fn print_entries(&self, level: usize) {
        self.print_mapper_as_entries(&self.root, level, true);
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
    fn test_cal_base_va_with() {
        assert_eq!(PageTable::cal_base_va_with(0), 0);
        assert_eq!(
            PageTable::cal_base_va_with(1),
            configs::MEMORY_PAGE_BYTE_SIZE
        );
        assert_eq!(
            PageTable::cal_base_va_with(PageTable::cal_vpn_with(
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
    fn test_alloc_and_dealloc() {
        let mut page_table = PageTable::new(0).unwrap();
        assert!(page_table
            .map_without_alloc(0, 0, PageTableFlags::RV)
            .is_ok());
        assert!(page_table
            .translate_ppn_with(0)
            .is_some_and(|ppn| *ppn == 0));
        assert!(page_table
            .unmap_without_dealloc(0)
            .is_ok_and(|ppn| *ppn == 0));

        assert!(page_table
            .map_without_alloc(1, 1, PageTableFlags::RV)
            .is_ok());
        assert!(page_table
            .translate_ppn_with(1)
            .is_some_and(|ppn| *ppn == 1));
        assert!(page_table
            .unmap_without_dealloc(1)
            .is_ok_and(|ppn| *ppn == 1));

        assert!(page_table
            .map_without_alloc(
                *TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
                *TRAMPOLINE_PHYSICAL_PAGE_NUMBER,
                PageTableFlags::RV
            )
            .is_ok());
        assert!(page_table
            .translate_ppn_with(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER)
            .is_some_and(|ppn| *ppn == *TRAMPOLINE_PHYSICAL_PAGE_NUMBER));
        assert!(page_table
            .unmap_without_dealloc(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER)
            .is_ok_and(|ppn| *ppn == *TRAMPOLINE_PHYSICAL_PAGE_NUMBER));
    }
}

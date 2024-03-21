// @author:    olinex
// @time:      2023/08/14

// self mods
pub(crate) mod allocator;
pub(crate) mod area;
pub(crate) mod frame;
pub(crate) mod heap;
pub(crate) mod space;

cfg_if! {
    if #[cfg(all(any(target_arch = "riscv32", target_arch = "riscv64")))] {
        pub(crate) mod page_table_riscv;
        pub(crate) use page_table_riscv as page_table;
    }
}

// use other mods
use alloc::boxed::Box;

// use self mods
use crate::configs;
use crate::prelude::*;
use page_table::PageTable;

/// Alias of the page bytes array
pub(crate) type PageBytes = [u8; configs::MEMORY_PAGE_BYTE_SIZE];

lazy_static! {
    pub(crate) static ref MAX_VIRTUAL_PAGE_NUMBER: usize =
        PageTable::get_vpn_with(configs::MAX_VIRTUAL_ADDRESS);
    /// Trampoline will only have one page
    pub(crate) static ref TRAMPOLINE_VIRTUAL_PAGE_NUMBER: usize =
        PageTable::get_vpn_with(configs::TRAMPOLINE_VIRTUAL_BASE_ADDR);
    /// Trampoline's code was save in kernel's .text section
    pub(crate) static ref TRAMPOLINE_PHYSICAL_PAGE_NUMBER: usize =
        PageTable::get_ppn_with(configs::_fn_trampoline as usize);
    /// Trap context will only have one page
    pub(crate) static ref TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER: usize =
        PageTable::get_vpn_with(configs::TRAP_CTX_VIRTUAL_BASE_ADDR);
}

bitflags! {
    #[derive(Clone, Copy)]
    /// The abstract page table entry flags
    pub(crate) struct PageTableFlags: u8 {
        const EMPTY = 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const RX = Self::R.bits() | Self::X.bits();
        const RW = Self::R.bits() | Self::W.bits();
        const RXU = Self::RX.bits() | Self::U.bits();
        const RWU = Self::RW.bits() | Self::U.bits();
    }
}

pub(crate) trait PageTableTr {
    /// Calculate the physical page number by the physical address
    ///
    /// # Arguments
    /// * pa: physical address
    fn get_ppn_with(pa: usize) -> usize;

    /// Calculate the virtual page number by the virtual address
    ///
    /// # Arguments
    /// * va: virtual address
    fn get_vpn_with(va: usize) -> usize;

    /// Calculate the base virtual address of the page which is the first byte's by the virtual page number
    ///
    /// # Arguments
    /// * vpn: virtual page number
    fn cal_base_va_with(vpn: usize) -> usize;

    /// Calculate the offset of the physical address in the frame
    ///
    /// # Arguments
    /// * pa: physical address
    fn get_pa_offset(pa: usize) -> usize;

    /// Calculate the offset of the virtual address in the frame
    ///
    /// # Arguments
    /// * va: virtual address
    fn get_va_offset(va: usize) -> usize;

    /// Create a new page table, which will be store in the kernel's heap memory
    ///
    /// # Arguments
    /// * asid: the address space id which is helpful for page table cache refresh
    ///
    /// # Returns
    /// * Ok(Box(impl PageTableTr))
    fn new(asid: usize) -> Result<Box<Self>>;

    /// Get the asid of the page table
    fn asid(&self) -> usize;

    /// Get the physical page number of the page table's root page mapper
    fn ppn(&self) -> usize;

    /// Get the mmu token will is represented to the page table
    fn mmu_token(&self) -> usize;

    /// Make a page table entry to build relationship between virtual page and physical frame.
    /// This function will not alloc physical frame, so you must keep the frame tracker by yourself until you have removed the page table entry
    ///
    /// # Arguments
    /// * vpn: virtual page number
    /// * ppn: physical page number
    /// * flags: the permission flags of the page
    fn map_without_alloc(&mut self, vpn: usize, ppn: usize, flags: PageTableFlags) -> Result<()>;

    /// Make a page table entry to build relationship between virtual page and physical frame.
    /// This function will alloc physical frame randomly
    ///
    /// # Arguments
    /// * vpn: virtual page number
    /// * flags: the permission flags of the page
    ///
    /// # Returns
    /// * Ok(ppn)
    fn map(&mut self, vpn: usize, flags: PageTableFlags) -> Result<usize>;

    /// Remove the page table entry which the virtual page number is pointing to.
    /// This function will not dealloc physical frame, so you must drop the frame tracker by youself after you have removed the page table entry.
    ///
    /// # Arguments
    /// * vpn: virtual page number
    ///
    /// # Returns
    /// * Ok(ppn)
    fn unmap_without_dealloc(&mut self, vpn: usize) -> Result<usize>;

    /// Remove the page table entry which the virtual page number is pointing to.
    /// This function will dealloc physical frame at the same time
    ///
    /// # Arguments
    /// * vpn: virtual page number
    ///
    /// # Returns
    /// * Ok(ppn)
    fn unmap(&mut self, vpn: usize) -> Result<usize>;

    /// Translate the virtual page number to the physical page number according to the page table.
    /// If the virtual page number is not mapped, this function will return None
    ///
    /// # Arguments
    /// * vpn: virtual page number
    ///
    /// # Returns
    /// * Some(ppn)
    /// * None
    fn translate_ppn_with(&self, vpn: usize) -> Option<usize>;

    /// Get the reference of the frame tracker by the virtual page number
    ///
    /// # Arguments
    /// * vpn: virtual page number
    ///
    /// # Returns
    /// * Ok(&frame::FrameTracker)
    fn get_tracker_with(&self, vpn: usize) -> Result<&frame::FrameTracker>;

    /// Force convert the bytes to other type
    ///
    /// # Arguments
    /// * vpn: virtual page number
    /// * offset: the byte offset in the frame which points to the first byte
    fn as_kernel_mut<'a, 'b, U>(&self, vpn: usize, offset: usize) -> Result<&'b mut U>;

    /// Force convert all bytes in the frame to the array of the bytes
    ///
    /// # Arguments
    /// * vpn: virtual page number
    fn get_byte_array<'a, 'b>(&'a self, vpn: usize) -> Result<&'b mut PageBytes>;
}

pub(crate) fn print_memory_info() {
    debug!(
        "[{:#018x}, {:#018x}): Text section physical memory address range",
        configs::_addr_text_start as usize,
        configs::_addr_text_end as usize
    );
    debug!(
        "[{:#018x}, {:#018x}): Read only data section physical memory address range",
        configs::_addr_rodata_start as usize,
        configs::_addr_rodata_end as usize
    );
    debug!(
        "[{:#018x}, {:#018x}): Read write data section physical memory address range",
        configs::_addr_data_start as usize,
        configs::_addr_data_end as usize
    );
    debug!(
        "[{:#018x}, {:#018x}): Bootstack section physical memory address range",
        configs::_addr_bootstack_start as usize,
        configs::_addr_bootstack_end as usize
    );
    debug!(
        "[{:#018x}, {:#018x}): BSS section physical memory address range",
        configs::_addr_bss_start as usize,
        configs::_addr_bss_end as usize
    );
    debug!(
        "[{:#018x}, {:#018x}): Total physical memory address range",
        configs::_addr_mem_start as usize,
        configs::_addr_mem_end as usize
    );
    debug!(
        "[{:#018x}, {:#018x}): Kernel physical memory address range",
        configs::_addr_kernel_mem_start as usize,
        configs::_addr_kernel_mem_end as usize
    );
    debug!(
        "[{:#018x}, {:#018x}): Free physical memory address range",
        configs::_addr_free_mem_start as usize,
        configs::_addr_free_mem_end as usize
    );
    debug!(
        "{:>12}: As max virtual page number",
        *MAX_VIRTUAL_PAGE_NUMBER
    );
    debug!(
        "{:>12}: As trampoline virtual page number",
        *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
    );
    debug!(
        "{:>12}: As trampoline physical page number",
        *TRAMPOLINE_PHYSICAL_PAGE_NUMBER
    );
    debug!(
        "{:>12}: As trap context virtual page number",
        *TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER
    );
}

#[inline(always)]
pub(crate) fn init() {
    print_memory_info();
    heap::init_heap();
    frame::init_frame_allocator();
    space::init_kernel_space();
}

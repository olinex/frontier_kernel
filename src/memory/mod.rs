// @author:    olinex
// @time:      2023/08/14

// self mods
pub mod allocator;
pub mod area;
pub mod frame;
pub mod heap;
pub mod space;

cfg_if! {
    if #[cfg(all(any(target_arch = "riscv32", target_arch = "riscv64")))] {
        pub mod page_table_riscv;
        pub use page_table_riscv as page_table;
    }
}

// use other mods
use alloc::boxed::Box;

// use self mods
use crate::configs;
use crate::prelude::*;
use page_table::PageTable;

/// Alias of the page bytes array
pub type PageBytes = [u8; configs::MEMORY_PAGE_BYTE_SIZE];

lazy_static! {
    pub static ref MAX_VIRTUAL_PAGE_NUMBER: usize =
        PageTable::get_vpn_with(configs::MAX_VIRTUAL_ADDRESS);
    /// trampoline will only have one page
    pub static ref TRAMPOLINE_VIRTUAL_PAGE_NUMBER: usize =
        PageTable::get_vpn_with(configs::TRAMPOLINE_VIRTUAL_BASE_ADDR);
    /// trampoline's code was save in kernel's .text section
    pub static ref TRAMPOLINE_PHYSICAL_PAGE_NUMBER: usize =
        PageTable::get_ppn_with(configs::_fn_trampoline as usize);
    /// trap context will only have one page
    pub static ref TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER: usize =
        PageTable::get_vpn_with(configs::TRAP_CTX_VIRTUAL_BASE_ADDR);
}

bitflags! {
    #[derive(Clone, Copy)]
    /// The abstract page table entry flags
    pub struct PageTableFlags: u8 {
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

pub trait PageTableTr {
    /// Create a new page table and save it in physical frame menory
    fn get_ppn_with(pa: usize) -> usize;
    fn get_vpn_with(va: usize) -> usize;
    fn cal_base_va_with(vpn: usize) -> usize;
    fn get_pa_offset(pa: usize) -> usize;
    fn get_va_offset(va: usize) -> usize;
    fn new(asid: usize) -> Result<Box<Self>>;
    fn asid(&self) -> usize;
    fn ppn(&self) -> usize;
    fn mmu_token(&self) -> usize;
    fn map_without_alloc(&mut self, vpn: usize, ppn: usize, flags: PageTableFlags) -> Result<()>;
    fn map(&mut self, vpn: usize, flags: PageTableFlags) -> Result<usize>;
    fn unmap_without_dealloc(&mut self, vpn: usize) -> Result<usize>;
    fn unmap(&mut self, vpn: usize) -> Result<usize>;
    fn translate_ppn_with(&self, vpn: usize) -> Option<usize>;
    fn get_tracker_with(&self, vpn: usize) -> Result<&frame::FrameTracker>;
    fn as_kernel_mut<'a, 'b, U>(&self, vpn: usize, offset: usize) -> Result<&'b mut U>;
    fn get_byte_array<'a, 'b>(&'a self, vpn: usize) -> Result<&'b mut PageBytes>;
}

pub fn print_memory_info() {
    info!(
        "[{:#018x}, {:#018x}): Text section physical memory address range",
        configs::_addr_text_start as usize,
        configs::_addr_text_end as usize
    );
    info!(
        "[{:#018x}, {:#018x}): Read only data section physical memory address range",
        configs::_addr_rodata_start as usize,
        configs::_addr_rodata_end as usize
    );
    info!(
        "[{:#018x}, {:#018x}): Read write data section physical memory address range",
        configs::_addr_data_start as usize,
        configs::_addr_data_end as usize
    );
    info!(
        "[{:#018x}, {:#018x}): BSS section physical memory address range",
        configs::_addr_bss_start as usize,
        configs::_addr_bss_end as usize
    );
    info!(
        "[{:#018x}, {:#018x}): Total physical memory address range",
        configs::_addr_mem_start as usize,
        configs::_addr_mem_end as usize
    );
    info!(
        "[{:#018x}, {:#018x}): Kernel physical memory address range",
        configs::_addr_kernel_mem_start as usize,
        configs::_addr_kernel_mem_end as usize
    );
    info!(
        "[{:#018x}, {:#018x}): Free physical memory address range",
        configs::_addr_free_mem_start as usize,
        configs::_addr_free_mem_end as usize
    );
    info!(
        "{:>12}: As max virtual page number",
        *MAX_VIRTUAL_PAGE_NUMBER
    );
    info!(
        "{:>12}: As trampoline virtual page number",
        *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
    );
    info!(
        "{:>12}: As trampoline physical page number",
        *TRAMPOLINE_PHYSICAL_PAGE_NUMBER
    );
    info!(
        "{:>12}: As trap context virtual page number",
        *TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER
    );
}

// Initially make bss section to zero is very import when kernel was initializing
pub fn clear_bss() {
    // force set all byte to zero
    (configs::_addr_bss_start as usize..configs::_addr_bss_end as usize)
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

pub fn init() {
    print_memory_info();
    heap::init_heap();
    frame::init_frame_allocator();
    space::init_kernel_space();
}

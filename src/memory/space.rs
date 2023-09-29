// @author:    olinex
// @time:      2023/09/13

// self mods

// use other mods
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use elf::abi;
use elf::endian::AnyEndian;
use elf::segment::ProgramHeader;
use elf::ElfBytes;

// use self mods
use super::allocator::LinkedListPageRangeAllocator;
use super::area::{Area, AreaType};
use super::page_table::{PageTable, MAX_TASK_ID};
use super::{PageTableFlags, PageTableTr};
use crate::lang::container::UserPromiseRefCell;
use crate::sbi::{self, SBIApi};
use crate::{configs, prelude::*};

pub struct Space {
    area_set: BTreeMap<(usize, usize), Area>,
    page_table: Arc<UserPromiseRefCell<PageTable>>,
    page_range_allocator: Arc<LinkedListPageRangeAllocator>,
}
impl Space {
    pub fn cal_kernel_task_stack_vpn_range(task_id: usize) -> (usize, usize) {
        let max_vpn = *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER;
        let stack_page_count = Self::vpn_ceil(configs::KERNEL_TASK_STACK_BYTE_SIZE);
        let end_vpn = max_vpn - (stack_page_count + configs::KERNEL_GUARD_PAGE_COUNT) * task_id;
        let start_vpn = end_vpn - stack_page_count;
        (start_vpn, end_vpn)
    }

    pub fn cal_user_task_stack_vpn_range(end_va: usize) -> (usize, usize) {
        let start_va = end_va + configs::KERNEL_GUARD_PAGE_COUNT * configs::MEMORY_PAGE_BYTE_SIZE;
        let end_va = start_va + configs::USER_TASK_STACK_BYTE_SIZE;
        (Self::vpn_ceil(start_va), Self::vpn_ceil(end_va))
    }

    #[inline(always)]
    fn vpn_ceil(va: usize) -> usize {
        // TODO: pa may too big and overflow
        PageTable::cal_vpn_with(va + configs::MEMORY_PAGE_BYTE_SIZE - 1)
    }

    #[inline(always)]
    fn vpn_floor(va: usize) -> usize {
        PageTable::cal_vpn_with(va)
    }

    #[inline(always)]
    pub fn trap_ctx_ppn(&self) -> Result<usize> {
        self.page_table
            .exclusive_access()
            .translate_ppn_with(*super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER)
            .ok_or(KernelError::VPNNotMapped(
                *super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER,
            ))
    }

    #[inline(always)]
    pub fn mmu_asid(&self) -> usize {
        self.page_table.access().asid()
    }

    #[inline(always)]
    pub fn mmu_token(&self) -> usize {
        self.page_table.access().mmu_token()
    }

    pub fn activate(&self) {
        unsafe { sbi::SBI::write_mmu_token(self.mmu_token()) };
    }

    pub fn new_bare(asid: usize) -> Result<Self> {
        let page_table = PageTable::new(asid)?;
        let page_range_allocator =
            LinkedListPageRangeAllocator::new(0, *super::MAX_VIRTUAL_PAGE_NUMBER + 1);
        Ok(Self {
            page_table: Arc::new(unsafe { UserPromiseRefCell::new(*page_table) }),
            area_set: BTreeMap::new(),
            page_range_allocator: Arc::new(page_range_allocator),
        })
    }

    pub fn push(
        &mut self,
        mut area: Area,
        linear_offset: usize,
        data: Option<&[u8]>,
    ) -> Result<()> {
        // FIXME: Write page action and insert area action must be synchronized
        if let Some(data) = data {
            area.write_multi_pages(linear_offset, data)?;
        };
        let range = area.range();
        if let Some(_) = self.area_set.insert(range, area) {
            Err(KernelError::AreaAllocationFailed(range.0, range.1))
        } else {
            Ok(())
        }
    }

    pub fn pop(&mut self, range: (usize, usize)) -> Result<()> {
        if let Some(_) = self.area_set.remove(&range) {
            Ok(())
        } else {
            Err(KernelError::AreaAllocationFailed(range.0, range.1))
        }
    }

    pub fn get_area(&self, start_vpn: usize, end_vpn: usize) -> Result<&Area> {
        if let Some(area) = self.area_set.get(&(start_vpn, end_vpn)) {
            Ok(area)
        } else {
            Err(KernelError::AreaAllocationFailed(start_vpn, end_vpn))
        }
    }

    #[inline(always)]
    pub fn get_trap_context_area(&self) -> Result<&Area> {
        self.get_area(
            *super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER,
            *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
        )
    }

    pub fn map_trampoline(&self) -> Result<()> {
        let result = self.page_table.exclusive_access().map_without_alloc(
            *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
            *super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER,
            PageTableFlags::RXV,
        );
        info!(
            "[{:#018x}, {:#018x}] -> [{:#018x}, {:#018x}): mapped trampoline segment address range",
            PageTable::cal_base_va_with(*super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER),
            configs::MAX_VIRTUAL_ADDRESS,
            PageTable::cal_base_va_with(*super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER),
            PageTable::cal_base_va_with(*super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER + 1),
        );
        result
    }

    pub fn new_kernel() -> Result<Self> {
        let mut space = Self::new_bare(MAX_TASK_ID)?;
        let start_vpn = Self::vpn_floor(configs::_addr_text_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_text_end as usize);
        let mut area = Area::new(
            start_vpn,
            end_vpn,
            PageTableFlags::RXV,
            AreaType::Identical,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        area.map()?;
        space.push(area, 0, None)?;
        info!(
            "[{:#018x}, {:#018x}): mapped kernel .text segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
        );

        let start_vpn = Self::vpn_floor(configs::_addr_rodata_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_rodata_end as usize);
        let mut area = Area::new(
            start_vpn,
            end_vpn,
            PageTableFlags::RV,
            AreaType::Identical,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        area.map()?;
        space.push(area, 0, None)?;
        info!(
            "[{:#018x}, {:#018x}): mapped kernel .rodata segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
        );

        let start_vpn = Self::vpn_floor(configs::_addr_data_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_data_end as usize);
        if start_vpn != end_vpn {
            let mut area = Area::new(
                start_vpn,
                end_vpn,
                PageTableFlags::RWV,
                AreaType::Identical,
                &space.page_range_allocator,
                &space.page_table,
            )?;
            area.map()?;
            space.push(area, 0, None)?;
            info!(
                "[{:#018x}, {:#018x}): mapped kernel .data segment address range",
                PageTable::cal_base_va_with(start_vpn),
                PageTable::cal_base_va_with(end_vpn),
            );
        } else {
            warn!(
                "[{:#018x}, {:#018x}): empty kernel .data segment!",
                PageTable::cal_base_va_with(start_vpn),
                PageTable::cal_base_va_with(end_vpn),
            );
        }

        let start_vpn = Self::vpn_floor(configs::_addr_bss_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_bss_end as usize);
        if start_vpn != end_vpn {
            let mut area = Area::new(
                start_vpn,
                end_vpn,
                PageTableFlags::RWV,
                AreaType::Identical,
                &space.page_range_allocator,
                &space.page_table,
            )?;
            area.map()?;
            space.push(area, 0, None)?;
            info!(
                "[{:#018x}, {:#018x}): mapped kernel .bss segment address range",
                PageTable::cal_base_va_with(start_vpn),
                PageTable::cal_base_va_with(end_vpn),
            );
        } else {
            warn!(
                "[{:#018x}, {:#018x}): empty kernel .bss segment!",
                PageTable::cal_base_va_with(start_vpn),
                PageTable::cal_base_va_with(end_vpn),
            );
        }

        let start_vpn = Self::vpn_floor(configs::_addr_free_mem_start as usize);
        let end_vpn = Self::vpn_floor(configs::_addr_free_mem_end as usize);
        let mut area = Area::new(
            start_vpn,
            end_vpn,
            PageTableFlags::RWV,
            AreaType::Identical,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        area.map()?;
        space.push(area, 0, None)?;
        info!(
            "[{:#018x}, {:#018x}): mapped kernel free physical memory",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
        );
        space.map_trampoline()?;
        Ok(space)
    }
}

lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UserPromiseRefCell<Space>> =
        Arc::new(unsafe { UserPromiseRefCell::new(Space::new_kernel().unwrap()) });
}
impl KERNEL_SPACE {
    pub fn load_flags(bits: u32) -> PageTableFlags {
        let mut flags = PageTableFlags::V;
        if bits & abi::PF_R != 0 {
            flags |= PageTableFlags::R;
        }
        if bits & abi::PF_W != 0 {
            flags |= PageTableFlags::W;
        }
        if bits & abi::PF_X != 0 {
            flags |= PageTableFlags::X;
        }
        flags
    }

    /// Create a new task space by elf binary byte data
    /// Only the singleton KERNEL_SPACE have this function
    /// Each space is created of task and mapped a page entry of user kernel stack into kernel high half space
    /// # Arguments
    /// * task_id: the unique id of the task
    /// * data: the elf binary byte data sclice
    ///
    /// # Returns
    /// * Ok((Self, user_stack_top_va, kernel_stack_top_va, elf_entry_point))
    pub fn new_task_from_elf(task_id: usize, data: &[u8]) -> Result<(Space, usize, usize, usize)> {
        let elf_bytes = ElfBytes::<AnyEndian>::minimal_parse(data)?;
        let program_headers: Vec<ProgramHeader> = elf_bytes
            .segments()
            .ok_or(KernelError::InvalidHeadlessTask)?
            .iter()
            // This means we only accept elf loadable segments
            .filter(|phdr| phdr.p_type == abi::PT_LOAD)
            .collect();
        if program_headers.len() == 0 {
            return Err(KernelError::UnloadableTask);
        }
        let mut space = Space::new_bare(task_id)?;
        let mut max_end_va: usize = 0;
        for (index, phdr) in program_headers.iter().enumerate() {
            let start_va = phdr.p_vaddr;
            let end_va = phdr.p_vaddr + phdr.p_memsz;
            let start_vpn = Space::vpn_floor(start_va as usize);
            let end_vpn = Space::vpn_ceil(end_va as usize);
            // Task code and data was restricted as User Mode flags
            let flags = Self::load_flags(phdr.p_flags);
            max_end_va = end_va as usize;

            let mut area = Area::new(
                start_vpn,
                end_vpn,
                flags | PageTableFlags::U,
                AreaType::Framed,
                &space.page_range_allocator,
                &space.page_table,
            )?;
            area.map()?;
            let segment = elf_bytes.segment_data(&phdr)?;
            space.push(area, 0, Some(segment))?;
            info!(
                "[{:#018x}, {:#018x}): mapped {} segment address range",
                PageTable::cal_base_va_with(start_vpn),
                PageTable::cal_base_va_with(end_vpn),
                index
            );
        }
        // * Insert a guard page between segments and the user stack
        // Be careful, when `MEMORY_PAGE_BYTE_SIZE` is smaller than 4k
        // The `user_stack_bottom_va` and `user_stack_top_va` may be in the same page
        // * We place the user stack to lower half virtual memory space and near the code
        // Because it will have a better performance for locality
        let (user_stack_bottom_vpn, user_stack_top_vpn) =
            Space::cal_user_task_stack_vpn_range(max_end_va);
        // Map user stack with User Mode flag
        let mut area = Area::new(
            user_stack_bottom_vpn,
            user_stack_top_vpn,
            PageTableFlags::RWUV,
            AreaType::Framed,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        area.map()?;
        space.push(area, 0, None)?;
        info!(
            "[{:#018x}, {:#018x}): mapped user stack segment address range",
            PageTable::cal_base_va_with(user_stack_bottom_vpn),
            PageTable::cal_base_va_with(user_stack_top_vpn),
        );
        // Map TrapContext with No User Mode flag
        let mut area = Area::new(
            *super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER,
            *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
            PageTableFlags::RWV,
            AreaType::Framed,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        area.map()?;
        space.push(area, 0, None)?;
        info!(
            "[{:#018x}, {:#018x}): mapped trap context segment address range",
            PageTable::cal_base_va_with(*super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER),
            PageTable::cal_base_va_with(*super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER),
        );
        space.map_trampoline()?;
        let kernel_stack_top_vpn = KERNEL_SPACE.map_kernel_task_stack(task_id)?;
        Ok((
            space,
            PageTable::cal_base_va_with(user_stack_top_vpn),
            PageTable::cal_base_va_with(kernel_stack_top_vpn),
            elf_bytes.ehdr.e_entry as usize,
        ))
    }

    pub fn map_kernel_task_stack(&self, task_id: usize) -> Result<usize> {
        let (kernel_stack_bottom_vpn, kernel_stack_top_vpn) =
            Space::cal_kernel_task_stack_vpn_range(task_id);
        let kernel_space = self.access();
        // Map task's kernel stack area in space
        // It must be drop by task
        let mut area = Area::new(
            kernel_stack_bottom_vpn,
            kernel_stack_top_vpn,
            PageTableFlags::RWV,
            AreaType::Framed,
            &kernel_space.page_range_allocator,
            &kernel_space.page_table,
        )?;
        area.map()?;
        drop(kernel_space);
        self.exclusive_access().push(area, 0, None)?;
        info!(
            "[{:#018x}, {:#018x}): mapped kernel task stack segment address range",
            PageTable::cal_base_va_with(kernel_stack_bottom_vpn),
            PageTable::cal_base_va_with(kernel_stack_top_vpn),
        );
        Ok(kernel_stack_top_vpn)
    }

    pub fn unmap_kernel_task_stack(&self, task_id: usize) -> Result<()> {
        self.exclusive_access()
            .pop(Space::cal_kernel_task_stack_vpn_range(task_id))
    }
}

pub fn init_kernel_space() {
    KERNEL_SPACE.exclusive_access().activate();
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    #[test_case]
    fn test_cal_kernel_task_stack_va_range() {
        assert_eq!(
            Space::cal_kernel_task_stack_vpn_range(0).1,
            *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
        );
        assert_eq!(
            Space::cal_kernel_task_stack_vpn_range(0).0,
            *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE
        );
        assert_eq!(
            Space::cal_kernel_task_stack_vpn_range(1).1,
            *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE
                - configs::KERNEL_GUARD_PAGE_COUNT
        );
        assert_eq!(
            Space::cal_kernel_task_stack_vpn_range(1).0,
            *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE * 2
                - configs::KERNEL_GUARD_PAGE_COUNT
        );
    }

    #[test_case]
    fn test_map_and_unmap_kernel_task_stack() {
        // try create task 0's kernel stack
        assert!(KERNEL_SPACE
            .map_kernel_task_stack(0)
            .is_ok_and(|vpn| *vpn == *TRAMPOLINE_VIRTUAL_PAGE_NUMBER));
        assert!(KERNEL_SPACE
            .access()
            .page_table
            .access()
            .translate_ppn_with(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER)
            .is_some_and(|ppn| *ppn != 0));
        assert!(KERNEL_SPACE.unmap_kernel_task_stack(0).is_ok());
        // retry to create task 0's kernel stack
        assert!(KERNEL_SPACE
            .map_kernel_task_stack(0)
            .is_ok_and(|vpn| *vpn == *TRAMPOLINE_VIRTUAL_PAGE_NUMBER));
        assert!(KERNEL_SPACE
            .access()
            .page_table
            .access()
            .translate_ppn_with(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER)
            .is_some_and(|ppn| *ppn != 0));
        assert!(KERNEL_SPACE.unmap_kernel_task_stack(0).is_ok());
        // try to duplicate create task kernel stack
        assert!(KERNEL_SPACE.map_kernel_task_stack(3).is_ok_and(|vpn| *vpn
            == *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - 3 * (configs::KERNEL_GUARD_PAGE_COUNT
                    + (configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE))));
        assert!(KERNEL_SPACE
            .access()
            .page_table
            .access()
            .translate_ppn_with(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER)
            .is_some_and(|ppn| *ppn != 0));
        assert!(KERNEL_SPACE.map_kernel_task_stack(3).is_err());
        assert!(KERNEL_SPACE.unmap_kernel_task_stack(3).is_ok());
    }
}

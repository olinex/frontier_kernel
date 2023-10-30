// @author:    olinex
// @time:      2023/09/13

// self mods

// use other mods
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use elf::abi;
use elf::endian::AnyEndian;
use elf::segment::ProgramHeader;
use elf::ElfBytes;

// use self mods
use super::allocator::LinkedListPageRangeAllocator;
use super::area::{Area, AreaMapping};
use super::page_table::{PageTable, MAX_TASK_ID};
use super::{PageTableFlags, PageTableTr};
use crate::constant::ascii;
use crate::lang::container::UserPromiseRefCell;
use crate::sbi::{self, SBIApi};
use crate::{configs, prelude::*};

/// The abstract structure which represents the virtual memory address space
pub struct Space {
    /// Areas of the virtual page range, which keys are the the start and end virtual page number
    area_set: BTreeMap<(usize, usize), Area>,
    /// The table of pages represented by the virtual address space
    page_table: Arc<UserPromiseRefCell<PageTable>>,
    /// The allocator used to manage the areas page range,
    /// each area will alloc a range of the virtual page numbers and each range cannot have overlapping parts
    page_range_allocator: Arc<LinkedListPageRangeAllocator>,
}
impl Space {
    /// Get the range of the stack's virtual page number in the kernel address space,
    /// which stack is belong to the task according to the task's id.
    /// The kernel stack is allocated in the upper half space of the kernel address space.
    /// IN KERNEL SPACE:
    /// --------------------------------- <- MAX virtual address
    /// |       trampoline page         |
    /// ---------------------------------
    /// |   task0's kernel stack top    |
    /// |             ...               | <- task1
    /// |  task0's kernel stack bottom  |
    /// ---------------------------------
    /// |          guard page           |
    /// --------------------------------- <- stack top virtual address
    /// |   task1's kernel stack top    |
    /// |             ...               | <- task2
    /// |  task1's kernel stack bottom  |
    /// --------------------------------- <- stack bottom virtual address
    /// |          guard page           |
    /// ---------------------------------
    ///
    /// # Arguments
    /// * asid: the address space unique id
    ///
    /// # Returns
    /// * (start virtual page number, end virtual page number)
    pub fn get_kernel_task_stack_vpn_range(asid: usize) -> (usize, usize) {
        let max_vpn = *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER;
        let stack_page_count = Self::vpn_ceil(configs::KERNEL_TASK_STACK_BYTE_SIZE);
        let end_vpn = max_vpn - (stack_page_count + configs::KERNEL_GUARD_PAGE_COUNT) * asid;
        let start_vpn = end_vpn - stack_page_count;
        (start_vpn, end_vpn)
    }

    /// Get the range of the stack's virtual page number in the user address space,
    /// which stack is belong to the task.
    /// The user stack is allocated in the lower half of the user address space,
    /// separated from the code and data virtual page areas by guard page
    /// IN TASK SPACE:
    /// --------------------------------- <- stack top virtual address
    /// |    task0's user stack top     |
    /// |             ...               | <- task
    /// |   task0's uer stack bottom    |
    /// --------------------------------- <- stack bottom virtual address
    /// |          guard page           |
    /// ---------------------------------
    /// |           task data           |
    /// |              ...              |
    /// |           task code           |
    /// ---------------------------------
    /// |              ...              |
    /// --------------------------------- <- MIN virtual address
    /// # Arguments
    /// * end_va: the virtual address of the last code or data of task
    ///
    /// # Returns
    /// * (start virtual page number, end virtual page number)
    pub fn get_user_task_stack_vpn_range(end_va: usize) -> (usize, usize) {
        let start_va = end_va + configs::KERNEL_GUARD_PAGE_COUNT * configs::MEMORY_PAGE_BYTE_SIZE;
        let end_va = start_va + configs::USER_TASK_STACK_BYTE_SIZE;
        (Self::vpn_ceil(start_va), Self::vpn_ceil(end_va))
    }

    /// Get the virtual page number which is calculated by ceil divide the virtual address
    ///
    /// # Arguments
    /// * va: virtual address
    #[inline(always)]
    fn vpn_ceil(va: usize) -> usize {
        // TODO: pa may too big and overflow
        PageTable::get_vpn_with(va + configs::MEMORY_PAGE_BYTE_SIZE - 1)
    }

    /// Get the virtual page number which is calculated by floor divide the virtual address
    ///
    /// # Arguments
    /// * va: virtual address
    #[inline(always)]
    fn vpn_floor(va: usize) -> usize {
        PageTable::get_vpn_with(va)
    }

    /// Get the trap context's physical page number which is mapped into user address space.
    /// The trap context will be place to the upper half of the use address space.
    /// IN USER SPACE:
    /// --------------------------------- <- MAX virtual address
    /// |       trampoline page         |
    /// ---------------------------------
    /// |      trap context page        |
    /// ---------------------------------
    #[inline(always)]
    pub fn trap_ctx_ppn(&self) -> Result<usize> {
        self.page_table
            .access()
            .translate_ppn_with(*super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER)
            .ok_or(KernelError::VPNNotMapped(
                *super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER,
            ))
    }

    /// Get the memory manager unit token, which is pointed to the space's page table
    #[inline(always)]
    pub fn mmu_token(&self) -> usize {
        self.page_table.access().mmu_token()
    }

    /// Make current address space activate by wirtting the mmu token to the register
    #[inline(always)]
    fn activate(&self) {
        unsafe { sbi::SBI::write_mmu_token(self.mmu_token()) };
    }

    /// Create a new space without any area and frame except the root page mapper frame
    fn new_bare(asid: usize) -> Result<Self> {
        let page_table = PageTable::new(asid)?;
        let page_range_allocator =
            LinkedListPageRangeAllocator::new(0, *super::MAX_VIRTUAL_PAGE_NUMBER + 1);
        Ok(Self {
            page_table: Arc::new(unsafe { UserPromiseRefCell::new(*page_table) }),
            area_set: BTreeMap::new(),
            page_range_allocator: Arc::new(page_range_allocator),
        })
    }

    /// Push area into space and write data to the area,
    /// the area must belongs to the space
    ///
    /// # Arguments
    /// * area: the abstract structure belongs to the current space
    /// * offset: the first index of the data which will be writted into area
    /// * data: the binary data which will be writted into area
    ///
    /// # Returns
    /// Ok(())
    /// Err(KernelError::AreaAllocFailed(start vpn, end vpn))
    fn push(&mut self, mut area: Area, offset: usize, data: Option<&[u8]>) -> Result<()> {
        // FIXME: Write page action and insert area action must be synchronized
        if let Some(data) = data {
            area.write_multi_pages(offset, data)?;
        };
        let range = area.range();
        if let Some(_) = self.area_set.insert(range, area) {
            Err(KernelError::AreaAllocFailed(range.0, range.1))
        } else {
            Ok(())
        }
    }

    /// Pop area from space
    /// If area is removed, the frame and page range will be deallocated
    /// # Arguments
    /// * start_vpn: the start virtual page number of the area
    /// * end_vpn: the end virtual page number of the area which is not include in area
    ///
    /// # Returns
    /// * Ok(())
    /// * Err(KernelError::AreaDeallocFailed(start vpn, end vpn))
    fn pop(&mut self, start_vpn: usize, end_vpn: usize) -> Result<()> {
        if let Some(_) = self.area_set.remove(&(start_vpn, end_vpn)) {
            Ok(())
        } else {
            Err(KernelError::AreaDeallocFailed(start_vpn, end_vpn))
        }
    }

    /// Get the area from the space which have been allocated
    /// # Arguments
    /// * start_vpn: the start virtual page number of the area
    /// * end_vpn: the end virtual page number of the area which is not include in area
    ///
    /// # Returns
    /// * Ok(&Area)
    /// * Err(KernelError::AreaNotExists(start_vpn, end_vpn))
    fn get_area(&self, start_vpn: usize, end_vpn: usize) -> Result<&Area> {
        if let Some(area) = self.area_set.get(&(start_vpn, end_vpn)) {
            Ok(area)
        } else {
            Err(KernelError::AreaNotExists(start_vpn, end_vpn))
        }
    }

    /// Get the area which trap context was stored in it.
    ///
    /// # Returns
    /// * Ok(&Area)
    /// * Err(KernelError::AreaNotExists(start_vpn, end_vpn))
    #[inline(always)]
    pub fn get_trap_context_area(&self) -> Result<&Area> {
        self.get_area(
            *super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER,
            *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
        )
    }

    /// Translate byte buffers from current space to the current stack.
    /// Only kernel space allow to access all of the physical frame in memory.
    /// To reduce memory copies, each byte buffers in different frame will be load as bytes slice pointer.
    /// Please be carefully!!! This method does not guarantee the lifetime of the returned byte buffers.
    ///
    /// # Arguments
    /// * ptr: the pointer of the byte slice
    /// * len: the length of the byte slice
    ///
    /// # Returns
    /// * Ok(Vec[mut &'static [u8]])
    pub fn translated_byte_buffers(
        &self,
        ptr: *const u8,
        len: usize,
    ) -> Result<Vec<&'static mut [u8]>> {
        let mut start_va = ptr as usize;
        let end_va = start_va + len;
        let mut buffers = vec![];
        let page_table = self.page_table.access();
        while start_va < end_va {
            let tmp_start_va = start_va;
            let tmp_start_offset = PageTable::get_va_offset(tmp_start_va);
            let tmp_byte_length = configs::MEMORY_PAGE_BYTE_SIZE - tmp_start_offset;
            let tmp_end_va = end_va.min(tmp_start_va + tmp_byte_length);
            let tmp_end_offset = match PageTable::get_va_offset(tmp_end_va) {
                0 => configs::MEMORY_PAGE_BYTE_SIZE,
                a => a,
            };
            let buffer = page_table.get_byte_array(Self::vpn_floor(tmp_start_va))?;
            buffers.push(&mut buffer[tmp_start_offset..tmp_end_offset]);
            start_va = tmp_end_va;
        }
        Ok(buffers)
    }

    /// Translate a byte pointer into the String from current space to the current stack,
    /// it will extract each char until reach the NULL(\0) char.
    /// Only kernel sapce allow to access all of the physical frame in memory.
    /// To reduce memory copies, each byte buffers in different frame will be load as bytes slice pointer.
    /// Please be carefully!!! This method does not guarantee the lifetime of the returned byte buffers.
    ///
    /// # Arguments
    /// * ptr: the pointer of the string
    ///
    /// # Returns
    /// Ok(String)
    pub fn translated_string(&self, ptr: *const u8) -> Result<String> {
        let mut start_va = ptr as usize;
        let mut string = String::new();
        let page_table = self.page_table.access();
        'outer: loop {
            let tmp_start_va = start_va;
            let tmp_start_offset = PageTable::get_va_offset(tmp_start_va);
            let tmp_byte_length = configs::MEMORY_PAGE_BYTE_SIZE - tmp_start_offset;
            let tmp_end_va = tmp_start_va + tmp_byte_length;
            let buffer = page_table.get_byte_array(Self::vpn_floor(tmp_start_va))?;
            for offset in tmp_start_offset..configs::MEMORY_PAGE_BYTE_SIZE {
                let byte = buffer[offset];
                if byte == ascii::NULL {
                    break 'outer;
                }
                string.push(byte as char);
            }
            start_va = tmp_end_va;
        }
        Ok(string)
    }

    /// Translate a pointer into other type from current space to the current stack.
    /// Only kernel sapce allow to access all of the physical frame in memory.
    /// To reduce memory copies, each byte buffers in different frame will be load as bytes slice pointer.
    /// Please be carefully!!! This method does not guarantee the lifetime of the returned byte buffers.
    ///
    /// # Arguments
    /// * ptr: the pointer of generate type T
    ///
    /// # Returns
    /// Ok(&mut T)
    pub fn translated_refmut<T>(&self, ptr: *const T) -> Result<&mut T> {
        let vpn = Self::vpn_floor(ptr as usize);
        let offset = PageTable::get_va_offset(ptr as usize);
        self.page_table
            .exclusive_access()
            .as_kernel_mut(vpn, offset)
    }

    /// Map trampoline frame to the current address space's max page,
    /// By default, we assume that all code in trampoline page are addressed relative to registers,
    /// so all address spaces can share the same trampoline of kernel space by registering page table entry only
    ///
    /// IN TASK OR KERNEL SPACE:
    /// --------------------------------- <- MAX virtual address
    /// |       trampoline page         | <----
    /// ---------------------------------      |
    ///                                        |
    ///                                        |
    ///                                        | page table mappings
    /// IN KERNEL SPACE:                       |
    /// ---------------------------------      |
    /// |             data              |      |
    /// |   .text.trampoline segment    | -----
    /// |             code              |
    /// ---------------------------------
    /// |              ...              |
    /// --------------------------------- <- MIN virtual address
    ///
    /// # Returns
    /// * Ok(())
    fn map_trampoline(&self) -> Result<()> {
        self.page_table.exclusive_access().map_without_alloc(
            *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
            *super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER,
            PageTableFlags::RX,
        )?;
        debug!(
            "[{:#018x}, {:#018x}] -> [{:#018x}, {:#018x}): mapped trampoline segment address range",
            PageTable::cal_base_va_with(*super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER),
            configs::MAX_VIRTUAL_ADDRESS,
            PageTable::cal_base_va_with(*super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER),
            PageTable::cal_base_va_with(*super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER + 1),
        );
        Ok(())
    }

    /// When we enable the MMU virtual memory mechanism,
    /// the kernel-state code will also be addressed based on virtual memory,
    /// so we need to create a kernel address space,
    /// whose lower half virtual address are directly equal to the physical address
    ///
    ///                    --------------------------------- <- MAX virtual address
    ///                    |       trampoline page         | <----
    ///                    ---------------------------------      |
    ///              ----> |  task0's kernel stack area    |      |
    ///             |      ---------------------------------      |
    ///             |      |          guard page           |      |
    ///             |      ---------------------------------      |
    ///             |      |  task1's kernel stack area    |      |
    ///             |      ---------------------------------      |
    ///   free area |      |          guard page           |      |
    ///             |      ---------------------------------      | page table mappings
    ///             |      |             ...               |      |
    ///             |      |             ...               |      |
    ///             |      |             ...               |      |
    ///             |      ---------------------------------      |
    ///             -----> |      kernel boot stack        |      |
    ///                    ---------------------------------      |
    ///                    |             data              |      |
    ///                    |   .text.trampoline segment    | -----
    ///                    |             code              |
    ///                    ---------------------------------
    ///                    |              ...              |
    ///                    --------------------------------- <- MIN virtual address
    ///
    /// # Returns
    /// * Ok(kernel space)
    fn new_kernel() -> Result<Self> {
        // create a new bare space
        let mut space = Self::new_bare(MAX_TASK_ID)?;
        for (start_va, end_va) in configs::MMIO {
            let start_vpn = Self::vpn_floor(*start_va);
            let end_vpn = Self::vpn_ceil(*end_va);
            let area = Area::new(
                start_vpn,
                end_vpn,
                PageTableFlags::RW,
                AreaMapping::Identical,
                &space.page_range_allocator,
                &space.page_table,
            )?;
            space.push(area, 0, None)?;
            debug!(
                "[{:#018x}, {:#018x}): mapped kernel memory-mapped io registers virtual address range",
                PageTable::cal_base_va_with(start_vpn),
                PageTable::cal_base_va_with(end_vpn),
            );
        }
        let start_vpn = Self::vpn_floor(configs::_addr_text_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_text_end as usize);
        // map code segement as area
        let area = Area::new(
            start_vpn,
            end_vpn,
            PageTableFlags::RX,
            AreaMapping::Identical,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        space.push(area, 0, None)?;
        debug!(
            "[{:#018x}, {:#018x}): mapped kernel .text segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
        );
        // map read only data segement as area
        let start_vpn = Self::vpn_floor(configs::_addr_rodata_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_rodata_end as usize);
        let area = Area::new(
            start_vpn,
            end_vpn,
            PageTableFlags::R,
            AreaMapping::Identical,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        space.push(area, 0, None)?;
        debug!(
            "[{:#018x}, {:#018x}): mapped kernel .rodata segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
        );
        // map read write data segment as area
        let start_vpn = Self::vpn_floor(configs::_addr_data_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_data_end as usize);
        // A read/write data segment may be empty
        if start_vpn != end_vpn {
            let area = Area::new(
                start_vpn,
                end_vpn,
                PageTableFlags::RW,
                AreaMapping::Identical,
                &space.page_range_allocator,
                &space.page_table,
            )?;
            space.push(area, 0, None)?;
            debug!(
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
        // map block started by symbol segment as area
        let start_vpn = Self::vpn_floor(configs::_addr_bss_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_bss_end as usize);
        if start_vpn != end_vpn {
            let area = Area::new(
                start_vpn,
                end_vpn,
                PageTableFlags::RW,
                AreaMapping::Identical,
                &space.page_range_allocator,
                &space.page_table,
            )?;
            space.push(area, 0, None)?;
            debug!(
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
        // Treat the remaining physical pages as pages that the kernel can access directly
        let start_vpn = Self::vpn_floor(configs::_addr_free_mem_start as usize);
        // Be careful, the last page will be mapped as the trampoline page
        let end_vpn = Self::vpn_floor(configs::_addr_free_mem_end as usize);
        let area = Area::new(
            start_vpn,
            end_vpn,
            PageTableFlags::RW,
            AreaMapping::Identical,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        space.push(area, 0, None)?;
        debug!(
            "[{:#018x}, {:#018x}): mapped kernel free physical memory",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
        );
        // map trampoline page
        space.map_trampoline()?;
        Ok(space)
    }

    pub fn recycle_data_pages(&mut self) {
        self.area_set.clear();
    }
}

lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UserPromiseRefCell<Space>> =
        Arc::new(unsafe { UserPromiseRefCell::new(Space::new_kernel().unwrap()) });
}
impl KERNEL_SPACE {
    /// Convert ELF data flags to page table entry permission flags
    /// ELF data flags in bits:
    ///         | 4 | 3 | 2 | 1 |
    /// ELF:          R   W   X
    /// PTE:      X   W   R   V
    /// R = Readable
    /// W = Writeable
    /// X = eXecutable
    /// V = Valid
    ///
    /// # Arguments
    /// * bits: the elf data flags in bit format
    fn convert_flags(bits: u32) -> PageTableFlags {
        let mut flags = PageTableFlags::EMPTY;
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
    ///
    /// --------------------------------- <- MAX virtual address
    /// |       trampoline page         |
    /// ---------------------------------      
    /// |       trap context page       |      
    /// ---------------------------------      
    /// |             ...               |      
    /// |             ...               |      
    /// |             ...               |      
    /// ---------------------------------      
    /// |          user stack           |      
    /// ---------------------------------      
    /// |          guard page           |      
    /// ---------------------------------      
    /// |             data              |      
    /// |             code              |
    /// ---------------------------------
    /// |              ...              |
    /// --------------------------------- <- MIN virtual address
    ///
    /// # Arguments
    /// * asid: the unique id of the address space
    /// * data: the elf binary byte data sclice
    ///
    /// # Returns
    /// * Ok((Self, user_stack_top_va, kernel_stack_top_va, elf_entry_point))
    pub fn new_user_from_elf(asid: usize, data: &[u8]) -> Result<(Space, usize, usize)> {
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
        let mut space = Space::new_bare(asid)?;
        let mut max_end_va: usize = 0;
        for (index, phdr) in program_headers.iter().enumerate() {
            let start_va = phdr.p_vaddr;
            let end_va = phdr.p_vaddr + phdr.p_memsz;
            let start_vpn = Space::vpn_floor(start_va as usize);
            let end_vpn = Space::vpn_ceil(end_va as usize);
            // Task code and data was restricted as User Mode flags
            let flags = Self::convert_flags(phdr.p_flags);
            max_end_va = end_va as usize;
            let area = Area::new(
                start_vpn,
                end_vpn,
                flags | PageTableFlags::U,
                AreaMapping::Framed,
                &space.page_range_allocator,
                &space.page_table,
            )?;
            let segment = elf_bytes.segment_data(&phdr)?;
            space.push(area, 0, Some(segment))?;
            debug!(
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
            Space::get_user_task_stack_vpn_range(max_end_va);
        // Map user stack with User Mode flag
        let area = Area::new(
            user_stack_bottom_vpn,
            user_stack_top_vpn,
            PageTableFlags::RWU,
            AreaMapping::Framed,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        space.push(area, 0, None)?;
        debug!(
            "[{:#018x}, {:#018x}): mapped user stack segment address range",
            PageTable::cal_base_va_with(user_stack_bottom_vpn),
            PageTable::cal_base_va_with(user_stack_top_vpn),
        );
        // Map TrapContext with No User Mode flag
        let area = Area::new(
            *super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER,
            *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
            PageTableFlags::RW,
            AreaMapping::Framed,
            &space.page_range_allocator,
            &space.page_table,
        )?;
        space.push(area, 0, None)?;
        debug!(
            "[{:#018x}, {:#018x}): mapped trap context segment address range",
            PageTable::cal_base_va_with(*super::TRAP_CONTEXT_VIRTUAL_PAGE_NUMBER),
            PageTable::cal_base_va_with(*super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER),
        );
        space.map_trampoline()?;
        // Map a kernel task stack in to kernel's higher half virtual address part
        Ok((
            space,
            PageTable::cal_base_va_with(user_stack_top_vpn),
            elf_bytes.ehdr.e_entry as usize,
        ))
    }

    /// Create a new user space according to other user space,
    /// which will copy all of the bytes from other user space areas into new user space.
    /// Because if we want to fork a new task from origin, we need to copy all of the memory from it.
    ///
    /// # Arguments
    /// * asid: the address space unique id
    /// * another: the reference to the other user space
    ///
    /// # Returns
    /// * Ok(Space)
    pub fn new_user_from_another(asid: usize, another: &Space) -> Result<Space> {
        let mut space = Space::new_bare(asid)?;
        for ((start_vpn, end_vpn), other_area) in another.area_set.iter() {
            let area =
                Area::from_another(other_area, &space.page_range_allocator, &space.page_table)?;
            for vpn in *start_vpn..*end_vpn {
                let src = other_area.get_byte_array(vpn)?;
                let dst = area.get_byte_array(vpn)?;
                dst.copy_from_slice(src);
            }
            space.push(area, 0, None)?;
        }
        space.map_trampoline()?;
        Ok(space)
    }

    /// Each time creating a new task's space, we should map a task stack in kernel space at the same time.
    /// the range of the virtual address in kernel space is related to the task's unique id.
    ///
    /// # Arguments
    /// * asid: address space unique id
    ///
    /// # Returns
    /// * Ok(kernel stack top vpn)
    pub fn map_kernel_task_stack(&self, asid: usize) -> Result<usize> {
        let (kernel_stack_bottom_vpn, kernel_stack_top_vpn) =
            Space::get_kernel_task_stack_vpn_range(asid);
        let mut kernel_space = self.exclusive_access();
        // Map task's kernel stack area in space
        // It must be drop by task
        let area = Area::new(
            kernel_stack_bottom_vpn,
            kernel_stack_top_vpn,
            PageTableFlags::RW,
            AreaMapping::Framed,
            &kernel_space.page_range_allocator,
            &kernel_space.page_table,
        )?;
        kernel_space.push(area, 0, None)?;
        debug!(
            "[{:#018x}, {:#018x}): mapped kernel task stack segment address range",
            PageTable::cal_base_va_with(kernel_stack_bottom_vpn),
            PageTable::cal_base_va_with(kernel_stack_top_vpn),
        );
        Ok(kernel_stack_top_vpn)
    }

    /// Each time we destroy task, we should unmap the task stack in kernel space at the same time.
    ///
    /// # Arguments
    /// * asid: address space unique id
    ///
    /// # Returns
    /// * Ok(())
    pub fn unmap_kernel_task_stack(&self, asid: usize) -> Result<()> {
        let (start_vpn, end_vpn) = Space::get_kernel_task_stack_vpn_range(asid);
        self.exclusive_access().pop(start_vpn, end_vpn)?;
        debug!(
            "[{:#018x}, {:#018x}): unmapped kernel task stack segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
        );
        Ok(())
    }
}

/// Initially make the kernel space available.
/// Before calling this method, we must make sure that the mapping of the kernel address space is correct,
/// otherwise very complicated problems will occur
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
            Space::get_kernel_task_stack_vpn_range(0).1,
            *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
        );
        assert_eq!(
            Space::get_kernel_task_stack_vpn_range(0).0,
            *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE
        );
        assert_eq!(
            Space::get_kernel_task_stack_vpn_range(1).1,
            *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE
                - configs::KERNEL_GUARD_PAGE_COUNT
        );
        assert_eq!(
            Space::get_kernel_task_stack_vpn_range(1).0,
            *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE * 2
                - configs::KERNEL_GUARD_PAGE_COUNT
        );
    }

    #[test_case]
    fn test_space_vpn_ceil() {
        assert_eq!(Space::vpn_ceil(0), 0);
        assert_eq!(Space::vpn_ceil(1), 1);
        assert_eq!(Space::vpn_ceil(configs::MEMORY_PAGE_BYTE_SIZE - 1), 1);
        assert_eq!(Space::vpn_ceil(configs::MEMORY_PAGE_BYTE_SIZE), 1);
        assert_eq!(Space::vpn_ceil(configs::MEMORY_PAGE_BYTE_SIZE + 1), 2);
    }

    #[test_case]
    fn test_space_vpn_floor() {
        assert_eq!(Space::vpn_floor(0), 0);
        assert_eq!(Space::vpn_floor(1), 0);
        assert_eq!(Space::vpn_floor(configs::MEMORY_PAGE_BYTE_SIZE - 1), 0);
        assert_eq!(Space::vpn_floor(configs::MEMORY_PAGE_BYTE_SIZE), 1);
        assert_eq!(Space::vpn_floor(configs::MEMORY_PAGE_BYTE_SIZE + 1), 1);
    }

    #[test_case]
    fn test_kernel_space_area() {
        let kernel_space = KERNEL_SPACE.access();
        let page_table = kernel_space.page_table.access();
        let vpn = Space::vpn_floor((&KERNEL_SPACE as *const _) as usize);
        assert!(page_table
            .translate_ppn_with(vpn)
            .is_some_and(|ppn| *ppn == vpn));
    }

    #[test_case]
    fn test_kernel_space_code_map() {
        let kernel_space = KERNEL_SPACE.access();
        let page_table = kernel_space.page_table.access();
        let mid = (configs::_addr_text_start as usize + configs::_addr_text_end as usize) / 2;
        let vpn = PageTable::get_vpn_with(mid);
        assert_eq!(page_table.translate_ppn_with(vpn).unwrap(), vpn);

        let mid = (configs::_addr_data_start as usize + configs::_addr_data_end as usize) / 2;
        let vpn = PageTable::get_vpn_with(mid);
        assert_eq!(page_table.translate_ppn_with(vpn).unwrap(), vpn);

        let mid = (configs::_addr_rodata_start as usize + configs::_addr_rodata_start as usize) / 2;
        let vpn = PageTable::get_vpn_with(mid);
        assert_eq!(page_table.translate_ppn_with(vpn).unwrap(), vpn);

        let mid = (configs::_addr_bss_start as usize + configs::_addr_bss_start as usize) / 2;
        let vpn = PageTable::get_vpn_with(mid);
        assert_eq!(page_table.translate_ppn_with(vpn).unwrap(), vpn);
    }

    #[test_case]
    fn test_kernel_space_tempoline_map() {
        let kernel = KERNEL_SPACE.access();
        let page_table = kernel.page_table.access();
        assert_eq!(
            page_table
                .translate_ppn_with(*TRAMPOLINE_VIRTUAL_PAGE_NUMBER)
                .unwrap(),
            *TRAMPOLINE_PHYSICAL_PAGE_NUMBER
        )
    }

    #[test_case]
    fn test_kernel_space_map_and_unmap_kernel_task_stack() {
        // try create task 3's kernel stack
        assert!(KERNEL_SPACE.map_kernel_task_stack(3).is_ok());
        assert!(KERNEL_SPACE.unmap_kernel_task_stack(3).is_ok());
        // try to duplicate create task kernel stack
        assert!(KERNEL_SPACE.map_kernel_task_stack(3).is_ok_and(|vpn| *vpn
            == *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - 3 * (configs::KERNEL_GUARD_PAGE_COUNT
                    + (configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE))));
        assert!(KERNEL_SPACE.map_kernel_task_stack(3).is_err());
        assert!(KERNEL_SPACE.unmap_kernel_task_stack(3).is_ok());
    }
}

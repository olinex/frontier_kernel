// @author:    olinex
// @time:      2023/09/13

// self mods

// use other mods
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use elf::abi;
use elf::endian::AnyEndian;
use elf::segment::ProgramHeader;
use elf::ElfBytes;
use frontier_lib::constant::charater;

// use self mods
use super::allocator::LinkedListPageRangeAllocator;
use super::area::{Area, AreaMapping};
use super::page_table::{PageTable, MAX_TASK_ID};
use super::{PageTableFlags, PageTableTr};
use crate::lang::buffer::ByteBuffers;
use crate::lang::container::UserPromiseRefCell;
use crate::sbi::{self, SBIApi};
use crate::{configs, prelude::*};

/// The abstract structure which represents the virtual memory address space
pub(crate) struct Space {
    /// Areas of the virtual page range, which keys are the the start and end virtual page number
    area_set: BTreeMap<(usize, usize), Area>,
    /// The table of pages represented by the virtual address space
    page_table: Arc<UserPromiseRefCell<PageTable>>,
    /// The allocator used to manage the areas page range,
    /// each area will alloc a range of the virtual page numbers and each range cannot have overlapping parts
    page_range_allocator: Arc<LinkedListPageRangeAllocator>,
}
impl Space {
    /// Get the range of the kernel stack's virtual page number in the kernel address space,
    /// which kernel stack is belong to the task according to the kernel stack's id.
    /// The kernel stack is allocated in the upper half space of the kernel address space.
    /// ```
    /// IN KERNEL SPACE:
    /// --------------------------------- <- MAX virtual address
    /// |       trampoline page         |
    /// ---------------------------------
    /// |   task0's kernel stack top    |
    /// |             ...               | <- task0
    /// |  task0's kernel stack bottom  |
    /// ---------------------------------
    /// |          guard page           |
    /// --------------------------------- <- stack top virtual address
    /// |   task1's kernel stack top    |
    /// |             ...               | <- task1
    /// |  task1's kernel stack bottom  |
    /// --------------------------------- <- stack bottom virtual address
    /// |          guard page           |
    /// ---------------------------------
    /// ```
    ///
    /// - Arguments
    ///     - kid: the kernel stack unique id
    ///
    /// - Returns
    ///     - (start virtual page number, end virtual page number)
    pub(crate) fn get_kernel_task_stack_vpn_range(kid: usize) -> (usize, usize) {
        let max_vpn = *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER;
        let stack_page_count = Self::vpn_ceil(configs::KERNEL_TASK_STACK_BYTE_SIZE);
        let end_vpn = max_vpn - (stack_page_count + configs::KERNEL_GUARD_PAGE_COUNT) * kid;
        let start_vpn = end_vpn - stack_page_count;
        (start_vpn, end_vpn)
    }

    /// Get the top virtual address of the task's kernel stack
    ///
    /// - Arguments
    ///     - kid: the kernel stack unique id
    ///
    /// - Returns
    ///     - kernel stack's top virtual address
    pub(crate) fn get_kernel_task_stack_top_va(kid: usize) -> usize {
        let (_, end_vpn) = Self::get_kernel_task_stack_vpn_range(kid);
        PageTable::cal_base_va_with(end_vpn)
    }

    /// Get the range of the user stack's virtual page number in the user address space,
    /// which stack is belong to the task.
    /// The user stack is allocated in the lower half of the user address space,
    /// separated from the code and data virtual page areas by guard page
    /// ```
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
    /// ```
    ///
    /// - Arguments
    ///     - end_va: the virtual address of the last code or data of task
    ///     - tid: the unique id of the task
    ///
    /// - Returns
    ///     - (start virtual page number, end virtual page number)
    pub(crate) fn get_user_task_stack_vpn_range(end_va: usize, tid: usize) -> (usize, usize) {
        let guard_size =
            (tid + 1) * configs::KERNEL_GUARD_PAGE_COUNT * configs::MEMORY_PAGE_BYTE_SIZE;
        let stack_size = tid * configs::USER_TASK_STACK_BYTE_SIZE;
        let start_va = end_va + guard_size + stack_size;
        let end_va = start_va + configs::USER_TASK_STACK_BYTE_SIZE;
        (Self::vpn_ceil(start_va), Self::vpn_ceil(end_va))
    }

    /// Get the top virtual address of the task's user stack
    ///
    /// - Arguments
    ///     - end_va: the virtual address of the last code or data of task
    ///     - tid: the unique id of the task
    ///
    /// - Returns
    ///     - user stack's top virtual address
    pub(crate) fn get_user_task_stack_top_va(end_va: usize, tid: usize) -> usize {
        let (_, end_vpn) = Self::get_user_task_stack_vpn_range(end_va, tid);
        PageTable::cal_base_va_with(end_vpn)
    }

    /// Get the range of the task's trap context page number in the user address space,
    ///
    /// ```
    /// IN USER SPACE:
    /// --------------------------------- <- MAX virtual address
    /// |       trampoline page         |
    /// ---------------------------------
    /// |   task0's trap context top    |
    /// |             ...               | <- task0
    /// |  task0's trap context bottom  |
    /// --------------------------------- <- top virtual address
    /// |   task1's trap context top    |
    /// |             ...               | <- task1
    /// |  task1's trap context bottom  |
    /// --------------------------------- <- bottom virtual address
    /// ```
    ///
    /// - Arguments
    ///     - tid: the unique id of the task
    ///
    /// - Returns
    ///     - (start virtual page number, end virtual page number)
    pub(crate) fn get_task_trap_ctx_vpn_range(tid: usize) -> (usize, usize) {
        let offset = configs::MEMORY_PAGE_BYTE_SIZE * tid;
        let start_va = configs::TRAP_CTX_VIRTUAL_BASE_ADDR - offset;
        let end_va = start_va + configs::MEMORY_PAGE_BYTE_SIZE;
        (Self::vpn_ceil(start_va), Self::vpn_ceil(end_va))
    }

    /// Get the virtual page number which is calculated by ceil divide the virtual address
    ///
    /// - Arguments
    ///     - va: virtual address
    fn vpn_ceil(va: usize) -> usize {
        // TODO: pa may too big and overflow
        PageTable::get_vpn_with(va + configs::MEMORY_PAGE_BYTE_SIZE - 1)
    }

    /// Get the virtual page number which is calculated by floor divide the virtual address
    ///
    /// - Arguments
    ///     - va: virtual address
    fn vpn_floor(va: usize) -> usize {
        PageTable::get_vpn_with(va)
    }

    /// Get the memory manager unit token, which is pointed to the space's page table
    pub(crate) fn mmu_token(&self) -> usize {
        self.page_table.access().mmu_token()
    }

    /// Make current address space activate by wirtting the mmu token to the register
    fn activate(&self) {
        unsafe { sbi::SBI::write_mmu_token(self.mmu_token()) };
    }

    /// Create a new space without any area and frame except the root page mapper frame
    ///
    /// - Errors
    ///     - FrameExhausted
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
    /// - Arguments
    ///     - area: the abstract structure belongs to the current space
    ///     - offset: the first index of the data which will be writted into area
    ///     - data: the binary data which will be writted into area
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    ///     - AreaAllocFailed(start vpn, end vpn)
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
    /// - Arguments
    ///     - start_vpn: the start virtual page number of the area
    ///     - end_vpn: the end virtual page number of the area which is not include in area
    ///
    /// - Errors
    ///     - AreaDeallocFailed(start vpn, end vpn)
    fn pop(&mut self, start_vpn: usize, end_vpn: usize) -> Result<()> {
        if let Some(_) = self.area_set.remove(&(start_vpn, end_vpn)) {
            Ok(())
        } else {
            Err(KernelError::AreaDeallocFailed(start_vpn, end_vpn))
        }
    }

    /// Get the area from the space which have been allocated
    /// - Arguments
    ///     - start_vpn: the start virtual page number of the area
    ///     - end_vpn: the end virtual page number of the area which is not include in area
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    pub(crate) fn get_area(&self, start_vpn: usize, end_vpn: usize) -> Result<&Area> {
        self.area_set
            .get(&(start_vpn, end_vpn))
            .ok_or(KernelError::AreaNotExists(start_vpn, end_vpn))
    }

    /// Translate byte buffers from current space to the current stack.
    /// Only kernel space allow to access all of the physical frame in memory.
    /// To reduce memory copies, each byte buffers in different frame will be load as bytes slice pointer.
    /// Please be carefully!!! This method does not guarantee the lifetime of the returned byte buffers.
    ///
    /// - Arguments
    ///     - ptr: the pointer of the byte slice
    ///     - len: the length of the byte slice
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn translated_byte_buffers(
        &self,
        ptr: *const u8,
        len: usize,
    ) -> Result<ByteBuffers> {
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
        Ok(ByteBuffers::new(buffers, len))
    }

    /// Translate a byte pointer into the String from current space to the current stack,
    /// it will extract each char until reach the NULL(\0) char.
    /// Only kernel sapce allow to access all of the physical frame in memory.
    /// To reduce memory copies, each byte buffers in different frame will be load as bytes slice pointer.
    /// Please be carefully!!! This method does not guarantee the lifetime of the returned byte buffers.
    ///
    /// - Arguments
    ///     - ptr: the pointer of the string
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn translated_string(&self, ptr: *const u8) -> Result<String> {
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
                if byte == charater::NULL as u8 {
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
    /// - Arguments
    ///     - ptr: the pointer of generate type T
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn translated_refmut<T>(&self, ptr: *const T) -> Result<&mut T> {
        let vpn = Self::vpn_floor(ptr as usize);
        let offset = PageTable::get_va_offset(ptr as usize);
        self.page_table
            .exclusive_access()
            .as_kernel_mut(vpn, offset)
    }

    /// Translate virtual address to physcial address according to current space
    ///
    /// - Arguments
    ///     - va: virtual address
    ///
    /// - Returns
    ///     - Some(physical address)
    ///     - None
    pub(crate) fn translate_pa(&self, va: usize) -> Option<usize> {
        let vpn = Self::vpn_floor(va);
        let offset = PageTable::get_va_offset(va);
        let ppn = self.page_table.access().translate_ppn_with(vpn)?;
        Some(ppn * configs::MEMORY_PAGE_BYTE_SIZE + offset)
    }

    /// Map trampoline frame to the current address space's max page.
    /// By default, we assume that all code in trampoline page are addressed relative to registers,
    /// so all address spaces can share the same trampoline of kernel space by registering page table entry only.
    ///
    /// ```
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
    /// ```
    ///
    /// - Errors
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    fn map_trampoline(&self) -> Result<()> {
        debug!(
            "[{:#018x}, {:#018x}] -> [{:#018x}, {:#018x}): mapped trampoline segment address range",
            PageTable::cal_base_va_with(*super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER),
            configs::MAX_VIRTUAL_ADDRESS,
            PageTable::cal_base_va_with(*super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER),
            PageTable::cal_base_va_with(*super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER + 1),
        );
        self.page_table.exclusive_access().map_without_alloc(
            *super::TRAMPOLINE_VIRTUAL_PAGE_NUMBER,
            *super::TRAMPOLINE_PHYSICAL_PAGE_NUMBER,
            PageTableFlags::RX,
        )
    }

    /// Allocate user task stack frames to the current task's address space,
    /// each task have their own user stack in the space.
    /// We will insert a guard page between segments and the user stack.
    /// Be careful, when `MEMORY_PAGE_BYTE_SIZE` is smaller than 4k,
    /// the `user_stack_bottom_va` and `user_stack_top_va` may be in the same page.
    /// We place the user stack to lower half virtual memory space and near the code,
    /// because it will have a better performance for locality.
    ///
    /// ```
    /// IN TASK SPACE:
    /// ---------------------------------
    /// |    taskN's user stack top     |
    /// |             ...               |
    /// |   taskN's uer stack bottom    |
    /// ---------------------------------
    /// |             ...               |
    /// ---------------------------------
    /// |          guard page           |
    /// ---------------------------------
    /// |    task1's user stack top     |
    /// |             ...               |
    /// |   task1's uer stack bottom    |
    /// ---------------------------------
    /// |          guard page           |
    /// ---------------------------------
    /// |    task0's user stack top     |
    /// |             ...               |
    /// |   task0's uer stack bottom    |
    /// ---------------------------------
    /// |          guard page           |
    /// --------------------------------- <- end virtual address
    /// |           task data           |
    /// |              ...              |
    /// |           task code           |
    /// ---------------------------------
    /// |              ...              |
    /// --------------------------------- <- MIN virtual address
    /// ```
    ///
    /// - Arguments
    ///     - end_va: the virtual address of the last code or data of task
    ///     - tid: the unique id of the task
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
    pub(crate) fn alloc_user_task_stack(&mut self, end_va: usize, tid: usize) -> Result<()> {
        let (start_vpn, end_vpn) = Space::get_user_task_stack_vpn_range(end_va, tid);
        // Map user stack with User Mode flag
        let area = Area::new(
            start_vpn,
            end_vpn,
            PageTableFlags::RWU,
            AreaMapping::Framed,
            &self.page_range_allocator,
            &self.page_table,
        )?;
        self.push(area, 0, None)?;
        debug!(
            "[{:#018x}, {:#018x}): mapped task {}'s user stack segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
            tid,
        );
        Ok(())
    }

    /// Deallocate user task stack frames from the current task's address space.
    ///
    /// - Arguments
    ///     - end_va: the virtual address of the last code or data of task
    ///     - tid: the unique id of the task
    ///
    /// - Errors
    ///     - AreaDeallocFailed(start vpn, end vpn)
    pub(crate) fn dealloc_user_task_stack(&mut self, end_va: usize, tid: usize) -> Result<()> {
        let (start_vpn, end_vpn) = Space::get_user_task_stack_vpn_range(end_va, tid);
        self.pop(start_vpn, end_vpn)?;
        debug!(
            "[{:#018x}, {:#018x}): unmapped task {}'s user stack segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
            tid,
        );
        Ok(())
    }

    /// Allocate user trap context frame to the current task's address space,
    /// each task have their own trap context frame in the space.
    ///
    /// - Arguments
    ///     - tid: the unique id of the task
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
    pub(crate) fn alloc_task_trap_ctx(&mut self, tid: usize) -> Result<()> {
        let (start_vpn, end_vpn) = Space::get_task_trap_ctx_vpn_range(tid);
        // Map TrapContext with No User Mode flag
        let area = Area::new(
            start_vpn,
            end_vpn,
            PageTableFlags::RW,
            AreaMapping::Framed,
            &self.page_range_allocator,
            &self.page_table,
        )?;
        self.push(area, 0, None)?;
        debug!(
            "[{:#018x}, {:#018x}): mapped task {}'s trap context segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
            tid,
        );
        Ok(())
    }

    /// Deallocate user trap context frame from the current task's address space.
    ///
    /// - Arguments
    ///     - tid: the unique id of the task
    ///
    /// - Errors
    ///     - AreaDeallocFailed(start vpn, end vpn)
    pub(crate) fn dealloc_task_trap_ctx(&mut self, tid: usize) -> Result<()> {
        let (start_vpn, end_vpn) = Space::get_task_trap_ctx_vpn_range(tid);
        self.pop(start_vpn, end_vpn)?;
        debug!(
            "[{:#018x}, {:#018x}): unmapped task {}'s trap context segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
            tid,
        );
        Ok(())
    }

    /// Copy area from another space according to the vpn range
    ///
    /// - Arguments
    ///     - another: another space
    ///     - src_start_vpn: source start virtual page number of the range
    ///     - src_end_vpn: source end virtual page number of the range
    ///     - dst_start_vpn: destination start virtual page number of the range
    ///     - dst_end_vpn: destination end virtual page number of the range
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    pub(crate) fn copy_area_from_another(
        &mut self,
        another: &Self,
        src_start_vpn: usize,
        src_end_vpn: usize,
        dst_start_vpn: usize,
        dst_end_vpn: usize,
    ) -> Result<()> {
        let another_area = another.get_area(src_start_vpn, src_end_vpn)?;
        if let Ok(current_area) = self.get_area(dst_start_vpn, dst_end_vpn) {
            current_area.copy_another(another_area)?;
        } else {
            let current_area =
                Area::from_another(another_area, &self.page_range_allocator, &self.page_table)?;
            self.push(current_area, 0, None)?;
        }
        Ok(())
    }

    /// Copy area from self space according to the vpn range
    ///
    /// - Arguments
    ///     - src_start_vpn: source start virtual page number of the range
    ///     - src_end_vpn: source end virtual page number of the range
    ///     - dst_start_vpn: destination start virtual page number of the range
    ///     - dst_end_vpn: destination end virtual page number of the range
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    pub(crate) fn copy_area_from_self(
        &mut self,
        src_start_vpn: usize,
        src_end_vpn: usize,
        dst_start_vpn: usize,
        dst_end_vpn: usize,
    ) -> Result<()> {
        let another_area = self.get_area(src_start_vpn, src_end_vpn)?;
        if let Ok(current_area) = self.get_area(dst_start_vpn, dst_end_vpn) {
            current_area.copy_another(another_area)?;
        } else {
            let current_area =
                Area::from_another(another_area, &self.page_range_allocator, &self.page_table)?;
            self.push(current_area, 0, None)?;
        }
        Ok(())
    }

    /// When we enable the MMU virtual memory mechanism,
    /// the kernel-state code will also be addressed based on virtual memory,
    /// so we need to create a kernel address space,
    /// whose lower half virtual address are directly equal to the physical address.
    /// Be careful, task's kernel stack area will be created by PCB.
    ///
    /// ```
    /// IN KERNEL SPACE
    ///                    --------------------------------- <- MAX virtual address
    ///                    |       trampoline page         | <----
    ///                    ---------------------------------      |
    ///              ----> |  taskA's kernel stack area    |      |
    ///             |      ---------------------------------      |
    ///             |      |          guard page           |      |
    ///             |      ---------------------------------      |
    ///             |      |  taskB's kernel stack area    |      |
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
    ///                    |   .text.trampoline segment    |      |
    ///                    |             code              |      |
    ///                    --------------------------------- <----
    ///                    |              ...              |
    ///                    --------------------------------- <- MIN virtual address
    /// ```
    ///
    /// - Errors
    ///     - FrameExhausted
    ///     - VPNNotMapped(vpn)
    ///     - AreaAllocFailed(start vpn, end vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
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
        // map read write bootstack segment as area
        let start_vpn = Self::vpn_floor(configs::_addr_bootstack_start as usize);
        let end_vpn = Self::vpn_ceil(configs::_addr_bootstack_end as usize);
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
                "[{:#018x}, {:#018x}): mapped kernel bootstack segment address range",
                PageTable::cal_base_va_with(start_vpn),
                PageTable::cal_base_va_with(end_vpn),
            );
        } else {
            warn!(
                "[{:#018x}, {:#018x}): empty kernel bootstack segment!",
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

    pub(crate) fn recycle_data_pages(&mut self) {
        self.area_set.clear();
    }
}

lazy_static! {
    pub(crate) static ref KERNEL_SPACE: Arc<UserPromiseRefCell<Space>> =
        Arc::new(unsafe { UserPromiseRefCell::new(Space::new_kernel().unwrap()) });
}
impl KERNEL_SPACE {
    /// Convert ELF data flags to page table entry permission flags
    ///
    /// ```
    /// ELF data flags in bits:
    ///         | 4 | 3 | 2 | 1 |
    /// ELF:          R   W   X
    /// PTE:      X   W   R   V
    /// R = Readable
    /// W = Writeable
    /// X = eXecutable
    /// V = Valid
    /// ```
    ///
    /// - Arguments
    ///     - bits: the elf data flags in bit format
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

    /// Create a new task space by elf binary byte data.
    /// Only the singleton KERNEL_SPACE have this function.
    /// Each space is created of task and mapped a page entry of user kernel stack into kernel high half space.
    ///
    /// ```
    /// IN TASK SPACE
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
    /// ```
    ///
    /// - Arguments
    ///     - asid: the unique id of the address space
    ///     - data: the elf binary byte data sclice
    ///
    /// - Returns
    ///     - Self
    ///     - base_size
    ///     - elf_entry_point
    ///
    /// - Errors
    ///     - ParseElfError
    ///     - InvalidHeadlessTask
    ///     - UnloadableTask
    ///     - FrameExhausted
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    pub(crate) fn new_user_from_elf(asid: usize, data: &[u8]) -> Result<(Space, usize, usize)> {
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
        space.map_trampoline()?;
        Ok((space, max_end_va, elf_bytes.ehdr.e_entry as usize))
    }

    /// Create a new user space according to other user space,
    /// which will copy all of the bytes from other user space areas into new user space.
    /// Because if we want to fork a new task from origin, we need to copy all of the memory from it.
    ///
    /// - Arguments
    ///     - asid: the address space unique id
    ///     - another: the reference to the other user space
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
    ///     - VPNNotMapped(vpn)
    pub(crate) fn new_user_from_another(
        asid: usize,
        another: &Space,
        exclude_ranges: Option<BTreeSet<(usize, usize)>>,
    ) -> Result<Space> {
        let mut space = Space::new_bare(asid)?;
        for (range, another_area) in another.area_set.iter() {
            if exclude_ranges.as_ref().is_none()
                || exclude_ranges
                    .as_ref()
                    .is_some_and(|ranges| ranges.contains(range))
            {
                continue;
            }
            let area =
                Area::from_another(another_area, &space.page_range_allocator, &space.page_table)?;
            space.push(area, 0, None)?;
        }
        space.map_trampoline()?;
        Ok(space)
    }

    /// Each time creating a new task, we should map a new stack in kernel space at the same time.
    /// the range of the virtual address in kernel space is related to the kernel stack's unique id.
    /// Return the kernel stack top vpn.
    ///
    /// - Arguments
    ///     - kid: kernel stack's unique id
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
    ///     - VPNNotMapped(vpn)
    pub(crate) fn map_kernel_task_stack(&self, kid: usize) -> Result<usize> {
        let (kernel_stack_bottom_vpn, kernel_stack_top_vpn) =
            Space::get_kernel_task_stack_vpn_range(kid);
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
            "[{:#018x}, {:#018x}): mapped kernel stack {} segment address range",
            PageTable::cal_base_va_with(kernel_stack_bottom_vpn),
            PageTable::cal_base_va_with(kernel_stack_top_vpn),
            kid,
        );
        Ok(kernel_stack_top_vpn)
    }

    /// Each time we destroy task, we should unmap the task stack in kernel space at the same time.
    ///
    /// - Arguments
    ///     - kid: kernel stack unique id
    ///
    /// - Errors
    ///     - AreaDeallocFailed(start vpn, end vpn)
    pub(crate) fn unmap_kernel_task_stack(&self, kid: usize) -> Result<()> {
        let (start_vpn, end_vpn) = Space::get_kernel_task_stack_vpn_range(kid);
        self.exclusive_access().pop(start_vpn, end_vpn)?;
        debug!(
            "[{:#018x}, {:#018x}): unmapped kernel stack {} segment address range",
            PageTable::cal_base_va_with(start_vpn),
            PageTable::cal_base_va_with(end_vpn),
            kid,
        );
        Ok(())
    }
}

/// Initially make the kernel space available.
/// Before calling this method, we must make sure that the mapping of the kernel address space is correct,
/// otherwise very complicated problems will occur
#[inline(always)]
pub(crate) fn init_kernel_space() {
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
            .is_some_and(|ppn| ppn == vpn));
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
        assert!(KERNEL_SPACE.map_kernel_task_stack(3).is_ok_and(|vpn| vpn
            == *TRAMPOLINE_VIRTUAL_PAGE_NUMBER
                - 3 * (configs::KERNEL_GUARD_PAGE_COUNT
                    + (configs::KERNEL_TASK_STACK_BYTE_SIZE / configs::MEMORY_PAGE_BYTE_SIZE))));
        assert!(KERNEL_SPACE.map_kernel_task_stack(3).is_err());
        assert!(KERNEL_SPACE.unmap_kernel_task_stack(3).is_ok());
    }
}

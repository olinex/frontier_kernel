// @author:    olinex
// @time:      2023/08/26

// self mods

// use other mods
use log::Level;

// use self mods
pub(crate) const MEMORY_PAGE_BYTE_SIZE: usize = 4096;
pub(crate) const MEMORY_PAGE_BIT_SITE: usize = 12;

/// Stack byte size must be greater than 4k
/// Because for the safety reasons
/// We inject some guard page between stack area and other area
pub(crate) const USER_TASK_STACK_BYTE_SIZE: usize = MEMORY_PAGE_BYTE_SIZE * 8;
pub(crate) const KERNEL_TASK_STACK_BYTE_SIZE: usize = MEMORY_PAGE_BYTE_SIZE * 2;
pub(crate) const KERNEL_HEAP_BYTE_SIZE: usize = MEMORY_PAGE_BYTE_SIZE * 1024;
pub(crate) const KERNEL_GUARD_PAGE_COUNT: usize = 1;
pub(crate) const MAX_VIRTUAL_ADDRESS: usize = usize::MAX;
pub(crate) const MAX_PID_COUNT: usize = 65536;
pub(crate) const MAX_TID_COUNT: usize = 10240;
pub(crate) const INIT_PROCESS_PATH: &'static str = "/initproc";
pub(crate) const TRAMPOLINE_VIRTUAL_BASE_ADDR: usize = MAX_VIRTUAL_ADDRESS - MEMORY_PAGE_BYTE_SIZE + 1;
pub(crate) const TRAP_CTX_VIRTUAL_BASE_ADDR: usize = TRAMPOLINE_VIRTUAL_BASE_ADDR - MEMORY_PAGE_BYTE_SIZE;
pub(crate) const TICKS_PER_SEC: usize = 100;
pub(crate) const LOG_LEVEL: Level = Level::Info;
pub(crate) const MAX_FD_COUNT: usize = 65536;
pub(crate) const MAX_MUTEX_COUNT: usize = 1024;
pub(crate) const MAX_SEMAPHORE_COUNT: usize = MAX_MUTEX_COUNT;
pub(crate) const MAX_CONDVAR_COUNT: usize = MAX_MUTEX_COUNT;
pub(crate) const PIPE_RING_BUFFER_LENGTH: usize = 32;
pub(crate) const COMMAND_LINE_ARGUMENTS_BYTE_SIZE: usize = 512;

// the frequency of the board clock in Hz
cfg_if! {
    if #[cfg(all(feature = "board_qemu", any(target_arch = "riscv32", target_arch = "riscv64")))] {
        pub(crate) const BOARD_CLOCK_FREQ: usize = 12_500_000;
        // the memory-mapped io registers virtual address range
        pub(crate) const MMIO: &[(usize, usize)] = &[
            // VIRT_TEST/RTC  in virt machine
            (0x0010_0000, 0x0010_2000),
            // Virtio Block in virt machine
            (0x1000_1000, 0x1000_2000),
        ];
    } else {
        compile_error!("Unknown feature for board");
    }
}

// the range of the code sections
extern "C" {
    pub(crate) fn _addr_text_start();
    pub(crate) fn _addr_text_end();

    pub(crate) fn _addr_rodata_start();
    pub(crate) fn _addr_rodata_end();

    pub(crate) fn _addr_data_start();
    pub(crate) fn _addr_data_end();

    pub(crate) fn _addr_bootstack_start();
    pub(crate) fn _addr_bootstack_end();

    pub(crate) fn _addr_bss_start();
    pub(crate) fn _addr_bss_end();

    pub(crate) fn _addr_mem_start();
    pub(crate) fn _addr_mem_end();

    pub(crate) fn _addr_kernel_mem_start();
    pub(crate) fn _addr_kernel_mem_end();

    pub(crate) fn _addr_free_mem_start();
    pub(crate) fn _addr_free_mem_end();

    pub(crate) fn _fn_trampoline();
}

// @author:    olinex
// @time:      2023/08/26

// self mods

// use other mods
use log::Level;

// use self mods
pub const MEMORY_PAGE_BYTE_SIZE: usize = 4096;
pub const MEMORY_PAGE_BIT_SITE: usize = 12;

/// Stack byte size must be greater than 4k
/// Because for the safety reasons
/// We inject some guard page between stack area and other area
pub const USER_TASK_STACK_BYTE_SIZE: usize = MEMORY_PAGE_BYTE_SIZE * 8;
pub const KERNEL_TASK_STACK_BYTE_SIZE: usize = MEMORY_PAGE_BYTE_SIZE * 2;
pub const KERNEL_HEAP_BYTE_SIZE: usize = MEMORY_PAGE_BYTE_SIZE * 256;
pub const KERNEL_GUARD_PAGE_COUNT: usize = 1;
pub const MAX_VIRTUAL_ADDRESS: usize = usize::MAX;
pub const TRAMPOLINE_VIRTUAL_BASE_ADDR: usize = MAX_VIRTUAL_ADDRESS - MEMORY_PAGE_BYTE_SIZE + 1;
pub const TRAP_CTX_VIRTUAL_BASE_ADDR: usize = TRAMPOLINE_VIRTUAL_BASE_ADDR - MEMORY_PAGE_BYTE_SIZE;
pub const TICKS_PER_SEC: usize = 100;
pub const LOG_LEVEL: Level = Level::Info;

cfg_if! {
    if #[cfg(feature = "board_qemu")] {
        // the frequency of the board clock in Hz
        pub const BOARD_CLOCK_FREQ: usize = 12_500_000;
        // the memory-mapped io registers virtual address range
        pub const MMIO: &[(usize, usize)] = &[
            (0x0010_0000, 0x0010_2000)
        ];
    } else {
        compile_error!("Unknown feature for board");
    }
}

// the word size of the arch
cfg_if! {
    if #[cfg(target_arch = "riscv64")] {
        pub const ARCH_WORD_SIZE: usize = 64;
    } else if #[cfg(target_arch = "riscv32")] {
        pub const ARCH_WORD_SIZE: usize = 32;
    } else {
        compile_error!("Unknown target arch");
    }
}

// the range of the code sections
extern "C" {
    pub fn _addr_text_start();
    pub fn _addr_text_end();

    pub fn _addr_rodata_start();
    pub fn _addr_rodata_end();

    pub fn _addr_data_start();
    pub fn _addr_data_end();

    pub fn _addr_bss_start();
    pub fn _addr_bss_end();

    pub fn _addr_mem_start();
    pub fn _addr_mem_end();

    pub fn _addr_kernel_mem_start();
    pub fn _addr_kernel_mem_end();

    pub fn _addr_free_mem_start();
    pub fn _addr_free_mem_end();

    pub fn _fn_trampoline();
}

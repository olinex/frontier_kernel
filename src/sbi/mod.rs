// @author:    olinex
// @time:      2023/09/04

// self mods

// use other mods

// use self mods

pub trait SBIApi {
    /// Shutdown the kernel
    fn shutdown() -> !;

    /// This function is used to ensure that a subsequent instruction fetch will see any previous data stores already visible in the same hart
    unsafe fn sync_icache();

    /// Set the trap handler's entry point address to cpu in direct mode
    ///
    /// # Arguments
    /// * addr: the physical address of the trap handler
    unsafe fn set_direct_trap_vector(addr: usize);

    /// Put a single character into console and print it
    ///
    /// # Arguments
    /// * c: the byte of the character
    fn console_putchar(c: u8);

    /// Get a single character from console and return
    fn console_getchar() -> Option<u8>;

    /// Get the current time counter since the cpu have been reset previously.
    /// The counter value will increase in a fix frequency, so the frequence of the time counter is relative to the board of the SoC.
    /// If the function return 1, it don't means it return 1 second or 1 millisecond.
    fn get_timer() -> usize;

    /// Set the time counter for cpu to interrupt in the next time
    ///
    /// # Arguments
    /// * timer: the counter of the
    fn set_timer(timer: usize);

    /// Set cpu timer interrupt enabled
    unsafe fn enable_timer_interrupt();

    /// Read the memory manager unit's token which is represent to the page table
    fn read_mmu_token() -> usize;

    /// Write the mmu token to cpu
    ///
    /// # Arguments
    /// * bits: mmu token value
    unsafe fn write_mmu_token(bits: usize);

    /// This function is used to make sure that translation lookup buffer is synchronized with the page table forcefully
    unsafe fn sync_tlb();
}

pub struct SBI;

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        pub mod impl_riscv;
    } else {
        compile_error!("Unknown target_arch to implying sbi")
    }
}

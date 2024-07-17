// @author:    olinex
// @time:      2024/06/22

// self mods

// use other mods

// use self mods


// the memory-mapped io registers virtual address range
cfg_if! {
    if #[cfg(all(feature = "board_qemu", any(target_arch = "riscv32", target_arch = "riscv64")))] {
        // See `qemu/include/hw/riscv/virt.h` and `qemu/hw/riscv/virt.c`
        pub(crate) const MMIO: &[(usize, usize)] = &[
            // VIRT_TEST/RTC
            (0x0010_0000, 0x0010_2000),
            // Virtio Block
            (0x1000_1000, 0x1000_2000),
        ];
    } else {
        compile_error!("Unknown feature for board");
    }
}
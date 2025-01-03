// @author:    olinex
// @time:      2023/03/16

// self mods

// use other mods
use bit_field::BitField;
use core::ops::Range;

// use self mods
use super::handler;
use crate::memory::space;

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        use riscv::register::sstatus;

        const SSTATUS_SIE_POSITION: usize = 1;
        const SSTATUS_SPIE_POSITION: usize = 5;
        const SSTATUS_SPP_POSITION: usize = 8;
        const SSTATUS_SUM_POSITION: usize = 18;
        const SSTATUS_MXR_POSITION: usize = 19;
        const SSTATUS_FS_RANGE: Range<usize> = 13..15;
        const SSTATUS_XS_RANGE: Range<usize> = 15..17;

        #[repr(C)]
        #[derive(Debug, Clone, Copy, Default)]
        pub(crate) struct TrapContext {
            /// WARNING: could not change the ordering of the fields in this structure,
            /// because the context instance might be initialized by assembly code in the assembly/trampoline.asm

            /// general purpose registers
            pub(crate) x: [usize; 32],
            /// supervisor status register
            pub(crate) sstatus: usize,
            /// supervisor exception program counter
            pub(crate) sepc: usize,
            /// the value of the kernel mmu token, which contain the page number of the root page table
            pub(crate) kernel_mmu_token: usize,
            /// the virtual address of the kernel trap handler in the kernel space,
            /// because the trap handler will be injected as the trampoline space,
            /// which into all the space(including kernel space) at the max virutal page.
            /// it looks like unnecessary in the trap context,
            /// but we cannot remove it because task cannot load this value which is in the kernel space.
            /// we must copy it into trap context when the task is creating
            pub(crate) trap_handler_va: usize,
            /// the virtual address of the kernel task stack in the kernel space
            pub(crate) kernel_sp_va: usize,
        }

        impl TrapContext {
            /// Write value to x2 register (sp)
            ///
            /// - Arguments
            ///     - sp: the stack pointer memory address
            pub(crate) fn set_sp(&mut self, sp: usize) {
                self.x[2] = sp;
            }

            /// Write value to argument register(a0 ~ a7[x10 ~ x17])
            ///
            /// - Arguments
            ///     - index: the index of the argument register
            ///     - value: the value which will be written
            pub(crate) fn set_arg(&mut self, index: usize, value: usize) {
                assert!(index <= 7);
                self.x[10 + index] = value;
            }

            /// Read value from argument register
            /// 
            /// - Arguments
            ///     - index: the index of the argument register
            pub(crate) fn get_arg(&self, index: usize) -> usize {
                assert!(index <= 7);
                self.x[10 + index]
            }

            /// Make supervisor exception program counter to next instruction
            pub(crate) fn sepc_to_next_instruction(&mut self) -> usize {
                self.sepc += 4;
                self.sepc
            }

            /// Unfortunately, riscv crate's Sstatus structure doesn't support any method to set sstatus's bits
            /// so we have to read every bits out and change it by ourselves :(
            fn read_sstatus_bits() -> usize {
                let sts = sstatus::read();
                
                let mut bits = 0;
                bits.set_bit(SSTATUS_SIE_POSITION, sts.sie());
                bits.set_bit(SSTATUS_SPIE_POSITION, sts.spie());
                bits.set_bit(SSTATUS_SPP_POSITION, (sts.spp() as usize) != 0);
                bits.set_bit(SSTATUS_SUM_POSITION, sts.sum());
                bits.set_bit(SSTATUS_MXR_POSITION, sts.mxr());
                bits.set_bits(SSTATUS_FS_RANGE, sts.fs() as usize);
                bits.set_bits(SSTATUS_XS_RANGE, sts.xs() as usize);
                bits
            }

            /// init app context
            /// 
            /// - Arguments
            ///     - entry_point: application code entry point memory address
            ///     - user_stack_top_va:  the virtual address of the user stack in the user space
            ///     - kernel_stack_top_va: the virtual address of the kernel task stack in the kernel space
            pub(crate) fn create_app_init_context(entry_point: usize, user_stack_top_va: usize, kernel_stack_top_va: usize) -> Self {
                // for app context, the supervisor previous privilege mode must be user
                let mut sts = Self::read_sstatus_bits();
                sts.set_bit(SSTATUS_SPP_POSITION, false);
                let mut ctx = Self {
                    x: [0; 32],
                    sstatus: sts,
                    sepc: entry_point,
                    kernel_mmu_token: space::KERNEL_SPACE.access().mmu_token(),
                    trap_handler_va: handler::trap_handler as usize,
                    kernel_sp_va: kernel_stack_top_va,
                };
                // app's user stack pointer
                ctx.set_sp(user_stack_top_va);
                ctx
            }
        }
    } else {
        compile_error!("Unknown target_arch to implying TrapContext");
    }
}

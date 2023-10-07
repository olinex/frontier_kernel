// @author:    olinex
// @time:      2023/03/21

// self mods

// use other mods
//ref:: https://github.com/andre-richter/qemu-exit
use core::arch::asm;

// use self mods
use super::Exit;

const VIRT_TEST: u64 = 0x100000;
const EXIT_FAILURE_FLAG: isize = 0x3333;

// Encode the exit code using EXIT_FAILURE_FLAG.
const fn exit_code_encode(code: u32) -> u32 {
    (code << 16) | (EXIT_FAILURE_FLAG as u32)
}

pub enum QEMUExitStatus {
    // Equals `exit(0)`. qemu successful exit
    Success = 0x5555,
    // qemu reset
    Reset = 0x7777,
    // Equals `exit(1)`. qemu failed exit
    Failure = exit_code_encode(1) as isize,
}

pub enum ExitStatus {
    QEMU(QEMUExitStatus),
    Other(u32),
}

// RISCV64 configuration
pub struct RISCV64 {
    // Address of the sifive_test mapped device.
    addr: u64,
}

impl RISCV64 {
    // Create an instance.
    pub const fn new(addr: u64) -> Self {
        RISCV64 { addr }
    }

    // Exit qemu with specified exit code.
    fn exit(&self, status: ExitStatus) -> ! {
        // If code is not a special value, we need to encode it with EXIT_FAILURE_FLAG.
        let code = match status {
            ExitStatus::QEMU(q_status) => q_status as u32,
            ExitStatus::Other(o_status) => exit_code_encode(o_status),
        };

        unsafe {
            asm!(
                "sw {0}, 0({1})",
                in(reg)code,
                in(reg)self.addr
            );

            // For the case that the QEMU exit attempt did not work, transition into an infinite
            // loop. Calling `panic!()` here is unfeasible, since there is a good chance
            // this function here is the last expression in the `panic!()` handler
            // itself. This prevents a possible infinite loop.
            loop {
                asm!("wfi", options(nomem, nostack));
            }
        }
    }
}

impl Exit for RISCV64 {
    // Exit QEMU using `Success`, aka `0`, if possible.
    //
    // Note: Not possible for `X86`.
    #[inline]
    fn exit_success(&self) -> ! {
        self.exit(ExitStatus::QEMU(QEMUExitStatus::Success));
    }

    // Exit QEMU using `Failure`, aka `1`.
    #[inline]
    fn exit_failure(&self) -> ! {
        self.exit(ExitStatus::QEMU(QEMUExitStatus::Failure));
    }

    // Exit QEMU using `Reset`, aka `2`.
    #[inline]
    fn exit_reset(&self) -> ! {
        self.exit(ExitStatus::QEMU(QEMUExitStatus::Reset));
    }

    // Exit QEMU using `Other`, aka `3`.
    #[inline]
    fn exit_other(&self, code: usize) -> ! {
        self.exit(ExitStatus::Other(code as u32));
    }
}

pub const QEMU_EXIT_HANDLE: RISCV64 = RISCV64::new(VIRT_TEST);

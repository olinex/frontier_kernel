// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods
use core::arch::{asm, global_asm};
use riscv::register::{
    scause::{self, Exception, Trap, Interrupt},
    stval
};

// use self mods
use super::context;
use crate::lang::timer;
use crate::sbi::*;
use crate::syscall::syscall;
use crate::{configs, task};

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {

        global_asm!(include_str!("../assembly/riscv64/trampoline.asm"));
        // init the supervisor trap vector base address register(stvec)'s value,
        // which was the address of the symbol '_fn_save_all_registers_before_trap'
        // this symbol was point to some assembly code that does two main things:
        // 1 save all registers
        // 2 call trap_handler and pass user stack

        // enable the time interrput and the first timer trigger
        // when system was trap with timer interrupt, it will set other trigger by itself
        pub fn init_timer_interrupt() {
            unsafe { SBI::set_stimer() };
            timer::set_next_trigger();
        }

        pub fn set_kernel_trap_entry() {
            unsafe {SBI::set_direct_trap_vector(trap_from_kernel as usize)}
        }

        fn set_user_trap_entry() {
            unsafe {SBI::set_direct_trap_vector(configs::TRAMPOLINE_VIRTUAL_BASE_ADDR)}
        }

        #[no_mangle]
        pub fn trap_from_kernel() -> ! {
            panic!("a trap from kernel!");
        }

        #[no_mangle]
        pub fn trap_return() -> ! {
            set_user_trap_entry();
            let trap_ctx_va = configs::TRAP_CTX_VIRTUAL_BASE_ADDR;
            let user_mmu_token = task::TASK_CONTROLLER
                .access()
                .get_current_user_token()
                .unwrap();
            extern "C" {
                fn _fn_save_all_registers_before_trap();
                fn _fn_restore_all_registers_after_trap();
            }
            let restore_va = _fn_restore_all_registers_after_trap as usize
                - _fn_save_all_registers_before_trap as usize
                + configs::TRAMPOLINE_VIRTUAL_BASE_ADDR;
            unsafe {
                asm!(
                    "fence.i",
                    "jr {restore_va}",
                    restore_va = in(reg) restore_va,
                    in("a0") trap_ctx_va,
                    in("a1") user_mmu_token,
                    options(noreturn)
                );
            }
        }

        // the handler function of the kernel, there were three types of cause here
        // 1. application make ecalls to the kernel, handler will dispatch to the syscall
        // 2. some exceptions were thrown, handler will kill the application and continue
        // 3. other exceptions were thrown and the kernel was panic
        #[inline(always)]
        fn exception_trap_handler(ctx: &mut context::TrapContext, exception: Exception, stval: usize) {
            match exception {
                // UEE make ecalls
                Exception::UserEnvCall => {
                    // trap by exception will make hart to save the pc which caused the exception
                    // so supervisor exeption pc must point to the next instruction
                    ctx.sepc += 4;
                    match syscall(ctx.x[17], ctx.x[10], ctx.x[11], ctx.x[12]) {
                        Ok(code) => ctx.x[10] = code as usize,
                        Err(error) => {
                            error!("Syscall Fault cause: {}", error);
                            task::exit_current_and_run_other_task().unwrap();
                        }
                    }
                }
                // exception about memory fault
                Exception::StoreFault
                | Exception::StorePageFault
                | Exception::LoadFault
                | Exception::LoadPageFault => {
                    error!("PageFault in application, kernel killed it.");
                    task::exit_current_and_run_other_task().unwrap();
                }
                // apllcation run some illegal instruction
                Exception::IllegalInstruction => {
                    error!("IllegalInstruction in application, kernel killed it.");
                    task::exit_current_and_run_other_task().unwrap();
                }
                _ => {
                    panic!(
                        "Unsupported exception trap {:?}, stval = {:#x}!",
                        exception, stval
                    );
                }
            }
        }

        #[inline(always)]
        fn interrupt_trap_handler(_: &mut context::TrapContext, interrupt: Interrupt) {
            match interrupt {
                Interrupt::SupervisorTimer => {
                    timer::set_next_trigger();
                    task::suspend_current_and_run_other_task().unwrap();
                },
                _ => {
                    unimplemented!("Unimplemented interrupt handler, which was only implemented supervisor timer");
                }
            }
        }

        #[no_mangle]
        pub fn trap_handler() -> ! {
            // now we cannot handle trap from S mode to S mode
            // so we just make it panic here
            set_kernel_trap_entry();
            // load trap context from user space
            let controller = task::TASK_CONTROLLER.exclusive_access();
            let ctx = controller.get_current_trap_ctx().unwrap();
            // read the trap cause from register
            let scause = scause::read();
            // read the trap specific info value from register
            let stval = stval::read();
            // check the cause type
            match scause.cause() {
                // exception trap cause
                Trap::Exception(exception) => exception_trap_handler(ctx, exception, stval),
                // interrupt trap cause
                Trap::Interrupt(interrupt) => interrupt_trap_handler(ctx, interrupt),
            };
            trap_return();
        }
    } else {
        compile_error!("Unknown target_arch to implying trap_handler");
    }
}

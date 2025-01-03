// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods
use alloc::sync::Arc;
use core::arch::{asm, global_asm};
use frontier_lib::model::signal::SignalFlags;
use riscv::register::{
    scause::{self, Exception, Interrupt, Trap},
    stval,
};

// use self mods
use crate::{lang::timer, println};
use crate::syscall::syscall;
use crate::task::TASK_SCHEDULER;
use crate::{configs, task};
use crate::{memory::space::Space, sbi::*};

// enable the time interrput and the first timer trigger
// when system was trap with timer interrupt, it will set other trigger by itself
#[inline(always)]
pub(crate) fn init_timer_interrupt() {
    unsafe { SBI::enable_timer_interrupt() };
    timer::set_next_trigger();
}

/// Set `trap_from_kernel` function as the trap handler entry point
/// This function just panic so that we force disable the ability of the trap
#[inline(always)]
pub(crate) fn set_kernel_trap_entry() {
    unsafe { SBI::set_direct_trap_vector(trap_from_kernel as usize) }
}

/// Set trampoline code as the trap handler entry point which code is written in the file [assembly/trampoline.asm].
#[inline(always)]
pub(crate) fn set_user_trap_entry() {
    unsafe { SBI::set_direct_trap_vector(configs::TRAMPOLINE_VIRTUAL_BASE_ADDR) }
}

/// A help function for trap handler which force disable the trap.
#[no_mangle]
#[inline(always)]
pub(crate) fn trap_from_kernel() -> ! {
    println!("a trap from kernel");
    let scause = scause::read();
    let stval = stval::read();
    panic!("cause with {}: {}", scause.bits(), stval);
}

cfg_if! {
    if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {

        global_asm!(include_str!("../assembly/riscv64/trampoline.asm"));
        // init the supervisor trap vector base address register(stvec)'s value,
        // which was the address of the symbol '_fn_save_all_registers_before_trap'
        // this symbol was point to some assembly code that does two main things:
        // 1 save all registers
        // 2 call trap_handler and pass user stack
        #[no_mangle]
        #[inline(always)]
        pub(crate) fn trap_return() -> ! {
            set_user_trap_entry();
            let task = task::PROCESSOR.current_task().unwrap();
            let trap_ctx_va = Space::get_task_trap_ctx_bottom_va(task.tid());
            let process = task.process();
            let user_mmu_token = process.user_token();
            drop(process);
            drop(task);
            extern "C" {
                fn _fn_save_all_registers_before_trap();
                fn _fn_restore_all_registers_after_trap();
            }
            let restore_va = _fn_restore_all_registers_after_trap as usize
                - _fn_save_all_registers_before_trap as usize
                + configs::TRAMPOLINE_VIRTUAL_BASE_ADDR;
            unsafe {SBI::sync_icache()};
            unsafe {
                asm!(
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
        fn exception_trap_handler(exception: Exception, stval: usize) {
            match exception {
                // UEE make ecalls
                Exception::UserEnvCall => {
                    // extract the trap context's register value
                    // we must drop the controller and trap context's reference immediately,
                    // because in syscall function will borrow it too soon
                    let task = task::PROCESSOR.current_task().unwrap();
                    assert_eq!(Arc::strong_count(&task), 3);
                    let task_inner = task.inner_access();
                    let process = task.process();
                    let process_inner = process.inner_access();
                    let (syscall_id, arg1, arg2, arg3) = task_inner.modify_trap_ctx(process_inner.space(), |trap_ctx| {
                        let syscall_id = trap_ctx.get_arg(7);
                        let arg1 = trap_ctx.get_arg(0);
                        let arg2 = trap_ctx.get_arg(1);
                        let arg3 = trap_ctx.get_arg(2);
                        trap_ctx.sepc_to_next_instruction();
                        Ok((syscall_id, arg1, arg2, arg3))
                    }).unwrap();
                    drop(process_inner);
                    drop(process);
                    drop(task_inner);
                    drop(task);
                    match syscall(syscall_id, arg1, arg2, arg3) {
                        Ok(return_back) => {
                            let task = task::PROCESSOR.current_task().unwrap();
                            let task_inner = task.inner_access();
                            let process = task.process();
                            let process_inner = process.inner_access();
                            task_inner.modify_trap_ctx(process_inner.space(), |trap_ctx| {
                                trap_ctx.set_arg(0, return_back as usize);
                                Ok(())
                            }).unwrap();
                            drop(process_inner);
                            drop(process);
                            drop(task_inner);
                            drop(task);
                        },
                        Err(error) => {
                            let task = task::PROCESSOR.current_task().unwrap();
                            let tid = task.tid();
                            let process = task.process();
                            let pid = process.pid();
                            drop(process);
                            drop(task);
                            error!(
                                "process {}'s task {} syscall {} fault cause: {}",
                                pid,
                                tid,
                                syscall_id,
                                error
                            );
                            task::exit_current_and_run_other_task(-1).unwrap();
                        }
                    }
                }
                // exception about memory fault
                Exception::StoreFault
                | Exception::StorePageFault
                | Exception::InstructionFault
                | Exception::InstructionPageFault
                | Exception::LoadFault
                | Exception::LoadPageFault => {
                    error!("Fault {:?} in application, kernel send signal.", exception);
                    task::send_current_task_signal(SignalFlags::SEGV.trunc()).unwrap()
                }
                // apllcation run some illegal instruction
                Exception::IllegalInstruction => {
                    error!("IllegalInstruction in application, kernel send signal.");
                    task::send_current_task_signal(SignalFlags::ILL.trunc()).unwrap()
                }
                _ => {
                    panic!(
                        "Unsupported exception trap {:?}, stval = {:#x}!",
                        exception, stval
                    );
                }
            }
        }

        fn interrupt_trap_handler(interrupt: Interrupt) {
            match interrupt {
                Interrupt::SupervisorTimer => {
                    TASK_SCHEDULER.check_timers();
                    timer::set_next_trigger();
                    task::suspend_current_and_run_other_task().unwrap();
                },
                _ => {
                    unimplemented!("Unimplemented interrupt handler, which was only implemented supervisor timer");
                }
            }
        }

        #[no_mangle]
        #[inline(always)]
        pub(crate) fn trap_handler() -> ! {
            // now we cannot handle trap from S mode to S mode
            // so we just make it panic here
            set_kernel_trap_entry();
            // read the trap cause from register
            let scause = scause::read();
            // read the trap specific info value from register
            let stval = stval::read();
            // check the cause type
            match scause.cause() {
                // exception trap cause
                Trap::Exception(exception) => exception_trap_handler(exception, stval),
                // interrupt trap cause
                Trap::Interrupt(interrupt) => interrupt_trap_handler(interrupt),
            };
            // each time before return back to user-mode execution,
            // we try to check all pending signals and do some other action.
            match task::handle_current_task_signals() {
                // do noting when no signal is pending
                Ok(None) => (),
                // receive bad signal and exit current task
                Ok(Some(signal)) => {
                    error!("Syscall receive bad signal {}={}", signal.variant_name(), signal as usize);
                    task::exit_current_and_run_other_task(-(signal as i32)).unwrap();
                }
                // receive error when handling signal
                Err(error) => {
                    error!("Syscall handle signal cause: {}", error);
                    task::exit_current_and_run_other_task(-1).unwrap();
                }
            }
            trap_return();
        }
    } else {
        compile_error!("Unknown target_arch to implying trap_handler");
    }
}

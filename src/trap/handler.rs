// @author:    olinex
// @time:      2023/03/17

// self mods

use log::error;
// use other mods
use riscv::register::{
    scause::{self, Exception, Trap},
    stval,
};

// use self mods
use super::context;
use crate::prelude::*;
use crate::syscall::syscall;
use crate::task;

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
            // so wil must point to the next instruction
            ctx.sepc += 4;
            match syscall(ctx.x[17], ctx.x[10], ctx.x[11], ctx.x[12]) {
                Ok(code) => ctx.x[10] = code as usize,
                Err(error) => {
                    error!("{}", error);
                    task::exit_current_and_run_other_task();
                }
                _ => panic!("Unreachable expression"),
            }
        }
        // exception about memory fault
        Exception::StoreFault | Exception::StorePageFault => {
            error!("PageFault in application, kernel killed it.");
            task::exit_current_and_run_other_task();
        }
        // apllcation run some illegal instruction
        Exception::IllegalInstruction => {
            error!("IllegalInstruction in application, kernel killed it.");
            task::exit_current_and_run_other_task();
        }
        _ => {
            panic!(
                "Unsupported exception trap {:?}, stval = {:#x}!",
                exception, stval
            );
        }
    }
}

#[no_mangle]
pub fn trap_handler(ctx: &mut context::TrapContext) -> &mut context::TrapContext {
    // read the trap cause from register
    let scause = scause::read();
    // read the trap specific info value from register
    let stval = stval::read();
    // check the cause type
    match scause.cause() {
        // exception trap cause
        Trap::Exception(exception) => exception_trap_handler(ctx, exception, stval),
        // other trap we can not handle here
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    };
    ctx
}

// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods
use riscv::register::{
    scause::{self, Exception, Trap},
    stval,
};

// use self mods
use crate::println;
use crate::batch::run_next_app;
use crate::syscall::syscall;
use super::context::TrapContext;

/// the handler function of the kernel, there were three types of cause here
/// 1. application make ecalls to the kernel, handler will dispatch to the syscall
/// 2. some exceptions were thrown, handler will kill the application and continue
/// 3. other exceptions were thrown and the kernel was panic
#[no_mangle]
pub(crate) fn trap_handler(ctx: &mut TrapContext) -> &mut TrapContext {
    // read the trap cause from register
    let scause = scause::read();
    // read the trap specific info value from register
    let stval = stval::read();
    // check the cause type
    match scause.cause() {
        // application make ecalls
        Trap::Exception(Exception::UserEnvCall) => {
            ctx.sepc += 4;
            ctx.x[10] = syscall(
                ctx.x[17],
                [ctx.x[10], ctx.x[11], ctx.x[12]],
            ) as usize;
        }
        // exception about memory fault
        Trap::Exception(Exception::StoreFault) | Trap::Exception(Exception::StorePageFault) => {
            println!("[kernel] PageFault in application, kernel killed it.");
            run_next_app();
        }
        // apllcation run some illegal instruction
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            run_next_app();
        }
        // other exceptions we can not handle here
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    ctx
}

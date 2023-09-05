// @author:    olinex
// @time:      2023/03/09

// self mods

// use other mods
use core::panic::PanicInfo;

// use self mods
use crate::println;
use crate::sbi::*;

// panic handler must end the process and return noting
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    match (info.location(), info.message()) {
        (Some(loc), Some(msg)) => {
            println!(
                "[kernel] PANIC AT {}:{}, cause by {}",
                loc.file(),
                loc.line(),
                msg
            );
        }
        (Some(loc), None) => {
            println!(
                "[kernel] PANIC AT {}:{}, cause by unknown message",
                loc.file(),
                loc.line()
            );
        }
        (None, Some(msg)) => {
            println!("[kernel] PANIC AT unknown location, cause by {}", msg);
        }
        _ => {
            println!("[kernel] PANIC AT unknown location, cause by unknown message");
        }
    };
    SBI::shutdown()
}

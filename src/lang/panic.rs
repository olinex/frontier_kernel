// @author:    olinex
// @time:      2023/03/09

// self mods

// use other mods
use core::panic::PanicInfo;
use sbi;

// use self mods
use crate::println;

// panic handler must end the process and return noting
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    match (info.location(), info.message()) {
        (Some(loc), Some(msg)) => {
            println!("Panicked at {}:{}, cause by {}", loc.file(), loc.line(), msg);
        }
        (Some(loc), None) => {
            println!(
                "Panicked at {}:{}, cause by unknown message",
                loc.file(),
                loc.line()
            );
        }
        (None, Some(msg)) => {
            println!("Panicked at unknown location, cause by {}", msg);
        }
        _ => {
            println!("Panicked at unknown location, cause by unknown message");
        }
    };

    sbi::legacy::shutdown()
}

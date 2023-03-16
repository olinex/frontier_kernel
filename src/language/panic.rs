// @author:    olinex
// @time:      2023/03/09

// self mods

// use other mods
use core::panic::PanicInfo;

// use self mods
use crate::println;
use crate::sbi::shutdown;

// panic handler must end the process and return noting
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        println!(
            "Panicked at {}:{} {}",
            location.file(),
            location.line(),
            info.message().unwrap()
        );
    } else {
        println!("Panicked: {}", info.message().unwrap());
    }
    loop {
        // TODO: shutdown may be failed
        shutdown();
    }
}

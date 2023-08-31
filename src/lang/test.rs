// @author:    olinex
// @time:      2023/08/31

// self mods

// use other mods

// use self mods

#[cfg(test)]
pub fn test_runner(tests: &[&dyn Fn()]) -> ! {
    use crate::{boards::qemu, boards::qemu::QEMUExit, println};
    println!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
    qemu::QEMU_EXIT_HANDLE.exit_success()
}

// @author:    olinex
// @time:      2023/08/31

// self mods

// use other mods

// use self mods

#[cfg(test)]
pub fn test_runner(tests: &[&dyn Fn()]) -> ! {
    use crate::sbi::*;
    info!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
    info!("Successfully Finished {} tests", tests.len());
    SBI::shutdown()
}

#[cfg(test)]
mod tests {
    use crate::println;

    #[test_case]
    fn test() {
        println!("hello, unittest case");
    }
}

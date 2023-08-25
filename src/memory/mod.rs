// @author:    olinex
// @time:      2023/08/14

// self mods

// use other mods

// use self mods

// init bss section to zero is very import when kernel was ready
#[inline]
pub fn clear_bss() {
    extern "C" {
        // load bss start address by symbol name
        fn _addr_bss_start();
        // load bss end address by symbol name
        fn _addr_bss_end();
    }
    // force set all byte to zero
    (_addr_bss_start as usize.._addr_bss_end as usize)
        .for_each(|a| unsafe { (a as *mut u8).write_volatile(0) });
}

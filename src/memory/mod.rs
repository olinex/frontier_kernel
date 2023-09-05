// @author:    olinex
// @time:      2023/08/14

// self mods
pub mod heap;
pub mod stack;

// use other mods

// use self mods

pub trait StackTr {
    fn get_size(&self) -> usize;
    fn get_bottom(&self) -> usize;

    #[inline]
    fn get_top(&self) -> usize {
        self.get_bottom() + self.get_size()
    }
}

// init bss section to zero is very import when kernel was initializing
fn clear_bss() {
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

pub fn init() {
    clear_bss();
    heap::init_heap();
}

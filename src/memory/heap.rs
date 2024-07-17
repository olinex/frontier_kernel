// @author:    olinex
// @time:      2023/09/05

// self mods

// use other mods
use buddy_system_allocator as allocator;
use core::alloc::Layout;

// use self mods
use crate::configs;

// malloc memory in bss section, which will be used as kernel heap space
static mut KERNEL_HEAP_SPACE: [u8; configs::KERNEL_HEAP_BYTE_SIZE] =
    [0; configs::KERNEL_HEAP_BYTE_SIZE];

#[global_allocator]
static HEAP_ALLOCATOR: allocator::LockedHeap<32> = allocator::LockedHeap::<32>::empty();

#[alloc_error_handler]
pub(crate) fn handle_alloc_error(layout: Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

// Initialize the heap memory allocator
// The space of the heap was allocated in the bss section
#[inline(always)]
pub(crate) fn init_heap() {
    let start_addr = unsafe { KERNEL_HEAP_SPACE.as_ptr() as usize };
    let end_addr = start_addr + configs::KERNEL_HEAP_BYTE_SIZE;
    debug!("[{:#018x}, {:#018x}): Heap physical memory address initialized", start_addr, end_addr);
    unsafe {
        HEAP_ALLOCATOR
            .lock()
            .init(start_addr, configs::KERNEL_HEAP_BYTE_SIZE);
    }
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn test_bss_position() {
        use alloc::boxed::Box;
        extern "C" {
            fn _addr_bss_start();
            fn _addr_bss_end();
        }
        let bss_range = _addr_bss_start as usize.._addr_bss_end as usize;
        let a = Box::new(5);
        assert_eq!(*a, 5);
        assert!(bss_range.contains(&(a.as_ref() as *const _ as usize)));
    }

    #[test_case]
    fn test_vector() {
        let mut v = vec![];
        for i in 0..500 {
            v.push(i);
        }
        for i in 0..500 {
            assert_eq!(v[i], i);
        }
    }
}

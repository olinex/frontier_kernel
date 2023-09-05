// @author:    olinex
// @time:      2023/09/05

// self mods

// use other mods
use buddy_system_allocator as allocator;
use core::alloc;

// use self mods
use crate::configs;

static mut HEAP_SPACE: [u8; configs::KERNEL_HEAP_SIZE] = [0; configs::KERNEL_HEAP_SIZE];

#[global_allocator]
static HEAP_ALLOCATOR: allocator::LockedHeap<32> = allocator::LockedHeap::<32>::empty();

#[alloc_error_handler]
pub fn handle_alloc_error(layout: alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

pub fn init_heap() {
    unsafe {
        HEAP_ALLOCATOR
            .lock()
            .init(HEAP_SPACE.as_ptr() as usize, configs::KERNEL_HEAP_SIZE);
    }
}

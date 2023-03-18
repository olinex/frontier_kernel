// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods

// use self mods
use super::context::TrapContext;

const USER_STACK_SIZE: usize = 1024 * 8;
const KERNEL_STACK_SIZE: usize = 1024 * 8;

pub(crate) trait Stack {
    fn get_size(&self) -> usize;
    fn get_bottom(&self) -> usize;

    #[inline]
    fn get_top(&self) -> usize {
        self.get_bottom() + self.get_size()
    }
}

#[repr(align(4096))]
pub(crate) struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}

impl Stack for KernelStack {
    #[inline]
    fn get_size(&self) -> usize {
        self.data.len()
    }

    #[inline]
    fn get_bottom(&self) -> usize {
        self.data.as_ptr() as usize
    }
}

impl KernelStack {
    pub(crate) fn push_context(&self, cx: TrapContext) -> &'static mut TrapContext {
        let cx_ptr =
            (self.get_top() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            *cx_ptr = cx;
        }
        unsafe { cx_ptr.as_mut().unwrap() }
    }
}

#[repr(align(4096))]
pub(crate) struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

impl Stack for UserStack {
    #[inline]
    fn get_size(&self) -> usize {
        self.data.len()
    }

    #[inline]
    fn get_bottom(&self) -> usize {
        self.data.as_ptr() as usize
    }
}

pub(crate) static KERNEL_STACK: KernelStack = KernelStack {
    data: [0; KERNEL_STACK_SIZE],
};

pub(crate) static USER_STACK: UserStack = UserStack {
    data: [0; USER_STACK_SIZE],
};

// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods

// use self mods
use super::context::TrapContext;
use crate::configs::{KERNEL_STACK_SIZE, MAX_APP_NUM, USER_STACK_SIZE};

pub trait Stack {
    fn get_size(&self) -> usize;
    fn get_bottom(&self) -> usize;

    #[inline]
    fn get_top(&self) -> usize {
        self.get_bottom() + self.get_size()
    }
}

#[repr(align(4096))]
#[derive(Copy, Clone)]
pub struct KernelStack {
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
    pub fn push_context(&self, cx: TrapContext) -> &'static mut TrapContext {
        let cx_ptr = (self.get_top() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
        unsafe {
            *cx_ptr = cx;
            cx_ptr.as_mut().unwrap()
        }
    }
}

#[repr(align(4096))]
#[derive(Copy, Clone)]
pub struct UserStack {
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

pub static KERNEL_STACK: [KernelStack; MAX_APP_NUM] = [
    KernelStack {
        data: [0; KERNEL_STACK_SIZE],
    };
    MAX_APP_NUM
];

pub static USER_STACK: [UserStack; MAX_APP_NUM] = [
    UserStack {
        data: [0; USER_STACK_SIZE],
    };
    MAX_APP_NUM
];

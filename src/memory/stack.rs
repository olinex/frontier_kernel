// @author:    olinex
// @time:      2023/03/17

// self mods

// use other mods

// use self mods
use super::StackTr;
use crate::configs::{KERNEL_STACK_SIZE, USER_STACK_SIZE};

#[repr(align(4096))]
#[derive(Copy, Clone)]
pub struct KernelStack {
    data: [u8; KERNEL_STACK_SIZE],
}

impl KernelStack {
    pub const fn new() -> Self {
        Self {
            data: [0; KERNEL_STACK_SIZE],
        }
    }
}

impl StackTr for KernelStack {
    #[inline]
    fn get_size(&self) -> usize {
        self.data.len()
    }

    #[inline]
    fn get_bottom(&self) -> usize {
        self.data.as_ptr() as usize
    }
}

#[repr(align(4096))]
#[derive(Copy, Clone)]
pub struct UserStack {
    data: [u8; USER_STACK_SIZE],
}

impl UserStack {
    pub const fn new() -> UserStack {
        Self {
            data: [0; USER_STACK_SIZE],
        }
    }
}

impl StackTr for UserStack {
    #[inline]
    fn get_size(&self) -> usize {
        self.data.len()
    }

    #[inline]
    fn get_bottom(&self) -> usize {
        self.data.as_ptr() as usize
    }
}

// @author:    olinex
// @time:      2023/03/16

// self mods

// use other mods
use core::cell::{RefCell, RefMut};

// use self mods

pub struct UPSafeCell<T> {
    // inner data
    inner: RefCell<T>,
}

// force mark UPSafeCell as a Sync safe struct
unsafe impl<T> Sync for UPSafeCell<T> {}

impl<T> UPSafeCell<T> {
    // User is responsible to guarantee that inner struct is only used in
    // uniprocessor.
    pub unsafe fn new(value: T) -> Self {
        Self {
            inner: RefCell::new(value),
        }
    }
    // Panic if the data has been borrowed.
    pub fn exclusive_access(&self) -> RefMut<'_, T> {
        self.inner.borrow_mut()
    }
}

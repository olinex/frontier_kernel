// @author:    olinex
// @time:      2023/03/16

// self mods

// use other mods
use core::cell::{Ref, RefCell, RefMut};

// use self mods

pub struct UserPromiseRefCell<T> {
    // inner data
    inner: RefCell<T>,
}

// force mark UPSafeCell as a Sync safe struct
unsafe impl<T> Sync for UserPromiseRefCell<T> {}

impl<T> UserPromiseRefCell<T> {
    // User is responsible to guarantee that inner struct is only used in uniprocessor.
    #[inline(always)]
    pub unsafe fn new(value: T) -> Self {
        Self {
            inner: RefCell::new(value),
        }
    }

    // Panic if the data has been borrowed.
    #[inline(always)]
    pub fn exclusive_access(&self) -> RefMut<'_, T> {
        self.inner.borrow_mut()
    }

    // Only read borrowed
    #[inline(always)]
    pub fn access(&self) -> Ref<'_, T> {
        self.inner.borrow()
    }
}
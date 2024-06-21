// @author:    olinex
// @time:      2023/10/12

// self mods

// use other mods
use alloc::collections::BTreeSet;
use alloc::sync::{Arc, Weak};
use spin::mutex::Mutex;

// use self mods
use crate::lang::error::KernelError;
use crate::prelude::*;

//// An allocation manager.
/// which will keep all id in control.
pub(crate) struct BTreeIdAllocator {
    /// not yet allocated id
    current_id: usize,
    /// max id, which can not be allocated
    max_id: usize,
    /// a set of ids those have been release but not yet allocated
    recycled: BTreeSet<usize>,
}
impl BTreeIdAllocator {
    /// Create a new id allocator.
    /// The first id is 0.
    /// 
    /// - Arguments
    ///     - max_id: the max id which can not be allocated 
    pub(crate) fn new(max_id: usize) -> Self {
        Self {
            current_id: 0,
            max_id,
            recycled: BTreeSet::new(),
        }
    }

    /// Alloc a new id
    ///
    /// - Errors
    ///     - IdExhausted
    pub(crate) fn alloc(&mut self) -> Result<usize> {
        if let Some(id) = self.recycled.pop_first() {
            Ok(id)
        } else {
            let id = self.current_id;
            if id >= self.max_id {
                Err(KernelError::IdExhausted)
            } else {
                self.current_id += 1;
                Ok(id)
            }
        }
    }

    /// Dealloc a id
    /// 
    /// - Arguments
    ///     - pid: the unique id which will be dealloc
    ///
    /// - Errors
    ///     - IdNotDeallocable(id)
    pub(crate) fn dealloc(&mut self, id: usize) -> Result<()> {
        if id >= self.current_id || !self.recycled.insert(id) {
            Err(KernelError::IdNotDeallocable(id))
        } else {
            Ok(())
        }
    }
}

/// Id tracker maintains the life cycle of the id and returns the id to the id allocator when the tracker is released.
pub(crate) struct IdTracker {
    id: usize,
    allocator: Weak<Mutex<BTreeIdAllocator>>,
}
impl IdTracker {
    #[inline(always)]
    pub(crate) fn id(&self) -> usize {
        self.id
    }
}
impl Drop for IdTracker {
    fn drop(&mut self) {
        if let Some(allocator) = self.allocator.upgrade() {
            allocator.lock().dealloc(self.id).unwrap()
        }
    }
}

/// The id allocator which returning id tracker instand of returning raw id.
/// So using the allocator can make you have the ability to recycle id automatically.
pub(crate) struct AutoRecycledIdAllocator(Arc<Mutex<BTreeIdAllocator>>);
impl AutoRecycledIdAllocator {
    /// Create a new allocator.
    /// The id of the first tracker is 0.
    /// 
    /// - Arguments
    ///     - max_id: the max id which can not be allocated 
    pub(crate) fn new(max_id: usize) -> Self {
        Self(Arc::new(Mutex::new(BTreeIdAllocator::new(max_id))))
    }

    /// Alloc a new id tracker
    /// 
    /// - Returns
    ///     - Ok(id tracker)
    /// 
    /// - Errors
    ///     - IdExhausted
    pub(crate) fn alloc(&self) -> Result<IdTracker> {
        let id = self.0.lock().alloc()?;
        Ok( IdTracker { id, allocator: Arc::downgrade(&self.0)})
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test_case]
    fn test_frame_allocator_alloc_and_dealloc() {
        let mut allocator = BTreeIdAllocator::new(3);
        for i in 0..3 {
            assert_eq!(allocator.current_id, i);
            assert_eq!(allocator.max_id, 3);
            assert!(allocator.alloc().is_ok_and(|t| t == i));
        }
        assert!(allocator.alloc().is_err_and(|t| t.is_idexhausted()));
        for i in 0..3 {
            assert_eq!(allocator.current_id, 3);
            assert_eq!(allocator.max_id, 3);
            assert!(allocator.dealloc(i).is_ok());
        }
        for i in 0..3 {
            assert_eq!(allocator.current_id, 3);
            assert_eq!(allocator.max_id, 3);
            assert!(allocator.alloc().is_ok_and(|t| t == i));
        }
        assert!(allocator.alloc().is_err_and(|t| t.is_idexhausted()));
    }

    #[test_case]
    fn test_auto_recycled_allocator_alloc_and_dealloc() {
        let allocator = AutoRecycledIdAllocator::new(3);
        for _ in 0..3 {
            assert!(allocator.alloc().is_ok_and(|t| t.id() == 0));
        }
        let mut temp = Vec::new();
        for i in 0..3 {
            let tracker = allocator.alloc();
            assert!(tracker.as_ref().is_ok_and(|t| t.id() == i));
            temp.push(tracker);
        }
        assert!(allocator.alloc().is_err_and(|t| t.is_idexhausted()));
        drop(temp);
        let mut temp = Vec::new();
        for i in 0..3 {
            let tracker = allocator.alloc();
            assert!(tracker.as_ref().is_ok_and(|t| t.id() == i));
            temp.push(tracker);
        }
    }
}

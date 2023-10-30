// @author:    olinex
// @time:      2023/10/12

// self mods

// use other mods
use alloc::collections::BTreeSet;

// use self mods
use crate::lang::error::KernelError;
use crate::prelude::*;

//// A pid allocation manager.
/// which will keep all pid in control.
pub struct BTreePidAllocator {
    /// not yet allocated proccess id
    current_pid: usize,
    /// end process id, which will not be allocated
    end_pid: usize,
    /// a set of pids those have been release but not yet allocated
    recycled: BTreeSet<usize>,
}
impl BTreePidAllocator {
    /// Create a new allocator
    pub fn new() -> Self {
        Self {
            current_pid: 0,
            end_pid: usize::MAX,
            recycled: BTreeSet::new(),
        }
    }

    /// Get current allocable proccess id
    #[inline(always)]
    pub fn current_pid(&self) -> usize {
        self.current_pid
    }

    /// Get the end of process id
    #[inline(always)]
    pub fn end_pid(&self) -> usize {
        self.end_pid
    }

    /// Initialize a new BTreePidAllocator
    ///
    /// # Arguments
    /// * current_pid: the current process id which will be used in next time allocating
    /// * end_pid: the end of process id which will not be used
    pub fn init(&mut self, current_pid: usize, end_pid: usize) {
        assert!(current_pid < end_pid);
        self.current_pid = current_pid;
        self.end_pid = end_pid;
    }

    /// Alloc a new process id
    ///
    /// # Returns
    /// * Ok(usize): the new process id
    /// * Err(KernelError::PidExhausted): if no other process id can be allocated
    pub fn alloc(&mut self) -> Result<usize> {
        if let Some(pid) = self.recycled.pop_first() {
            Ok(pid)
        } else {
            let pid = self.current_pid;
            if pid >= self.end_pid {
                Err(KernelError::PidExhausted)
            } else {
                self.current_pid += 1;
                Ok(pid)
            }
        }
    }

    /// Dealloc a process id
    ///
    /// # Returns
    /// * Ok(())
    /// * Err(KernelError::PidNotDeallocable(pid))
    pub fn dealloc(&mut self, pid: usize) -> Result<()> {
        if pid >= self.current_pid || !self.recycled.insert(pid) {
            Err(KernelError::PidNotDeallocable(pid))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_frame_allocator_alloc_and_dealloc() {
        let mut allocator = BTreePidAllocator::new();
        allocator.init(0, 1);
        assert_eq!(allocator.current_pid, 0);
        assert_eq!(allocator.end_pid, 1);
        assert!(allocator.alloc().is_ok_and(|t| *t == 0));
        assert_eq!(allocator.current_pid, 1);
        assert_eq!(allocator.end_pid, 1);
        assert!(allocator.alloc().is_err_and(|t| t.is_pidexhausted()));
        assert!(allocator.dealloc(0).is_ok());
        assert!(allocator.alloc().is_ok_and(|t| *t == 0));
        assert_eq!(allocator.current_pid, 1);
        assert_eq!(allocator.end_pid, 1);
        assert!(allocator.alloc().is_err_and(|t| t.is_pidexhausted()));
    }
}

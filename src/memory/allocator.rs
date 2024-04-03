//! Contain the base structs and traits for memory
//! Diffrent ISA must impl those traits

// @author:    olinex
// @time:      2023/09/14

// self mods

// use other mods
use alloc::collections::BTreeSet;
use alloc::sync::{Arc, Weak};

// use self mods
use crate::lang::container::UserPromiseRefCell;
use crate::lang::error::*;

/// This structure is the smallest unit of the linked list of virtual memory intervals.
/// Each structure and its possible subsequent structs form a interval.
/// Each interval indicates that it has been occupied or can be applied for.
/// Therefore, two consecutive intervals must not be occupied or available at the same time.
/// For example: [1, 16)
/// | 1| 2| 3| 4| 5| 6| 7| 8| 9| 10| 11| 12| 13| 14| 15| 16|
/// |                all have been occupied                | <- this is ok
/// |                   all is available                   | <- this is ok
/// |     occupied       |            available            | <- this is ok
/// |     occupied       |    available    |    occupied   | <- this is ok
/// |     occupied       |    occupied    |    available   | <- this is bad!!!
pub(crate) struct PageNode {
    /// When used is true, it mean that the interval [current page, next page) is occupied
    used: bool,
    /// The virtual memory page number
    vpn: usize,
    /// A weak reference to the the current node's parent node
    prev: Weak<UserPromiseRefCell<PageNode>>,
    /// An optianal strong reference to the current node's next node
    next: Option<Arc<UserPromiseRefCell<PageNode>>>,
}
impl PageNode {
    /// Create a Create a separate node
    /// - Arguments
    ///     - used: mark the interval as occupied, default is false
    ///     - vpn: the virtual memory page number
    fn new(used: bool, vpn: usize) -> Self {
        Self {
            used,
            vpn,
            prev: Weak::new(),
            next: None,
        }
    }

    /// Link two nodes together.
    /// The linking process is little complicated, we want to link node4 to node1 as next node.
    /// The steps are as follows:
    ///       prev node                                                                           next node
    ///       node1(prev: ., next: 2)     node2(prev: 1, next: .)     node3(prev: ., next: 4)     node4(prev: 3, next: .)
    /// step1     |>>>>>>>>>>>>>>>break>>>>>>>>>>>>>>>|
    ///       node1(prev: ., next: 2)     node2(prev: ., next: .)     node3(prev: ., next: 4)     node4(prev: 3, next: .)
    /// step2                      |<------------------------------link-------------------------------|
    ///       node1(prev: ., next: 4)     node2(prev: ., next: .)     node3(prev: ., next: 4)     node4(prev: 3, next: .)
    /// step3                                                                              |<<<<<<break<<<<<<<|
    ///       node1(prev: ., next: 4)     node2(prev: ., next: .)     node3(prev: ., next: .)     node4(prev: 3, next: .)
    /// step4     |------------------------------------------link-------------------------------------------->|
    ///       node1(prev: ., next: 4)     node2(prev: ., next: .)     node3(prev: ., next: .)     node4(prev: 1, next: .)
    /// - Arguments
    ///     - prev_page_node: the page node which will ahead of the next node
    ///     - next_page_node: the page node which will behind the previous node
    fn link(
        prev_page_node: &Arc<UserPromiseRefCell<PageNode>>,
        next_page_node: &Arc<UserPromiseRefCell<PageNode>>,
    ) {
        if let Some(old_next_page_node) = &prev_page_node.access().next {
            old_next_page_node.exclusive_access().prev = Weak::new();
        }
        prev_page_node.exclusive_access().next = Some(Arc::clone(next_page_node));
        if let Some(old_prev) = &next_page_node.access().prev.upgrade() {
            old_prev.exclusive_access().next = None;
        }
        next_page_node.exclusive_access().prev = Arc::downgrade(prev_page_node);
    }

    /// Check and merge the two same interval of the status of available.
    /// If current not has previous node and next node
    /// This function is called after some page nodes insert into the linked list
    /// - Arguments
    ///     - node: the current page node which will be merged into the previous node
    fn merge_range(page_node: Arc<UserPromiseRefCell<PageNode>>) {
        let page_node_borrow = page_node.access();
        if let (Some(prev_page_node), Some(next_page_node)) =
            (page_node_borrow.prev.upgrade(), &page_node_borrow.next)
        {
            if page_node_borrow.used == prev_page_node.access().used {
                let prev = &Arc::clone(&prev_page_node);
                let next = &Arc::clone(next_page_node);
                drop(page_node_borrow);
                PageNode::link(prev, next);
            }
        };
    }
}

/// The allocation manager which contain the root node of page node linked list
pub(crate) struct LinkedListPageRangeAllocator {
    root: Arc<UserPromiseRefCell<PageNode>>,
}
impl LinkedListPageRangeAllocator {
    /// Create a new page range allocation manager
    /// - Arguments
    ///     - start_vpn: the first virtual memory page number
    ///     - end_vpn: the last virtual memory page number, which will not be allocated
    pub(crate) fn new(start_vpn: usize, end_vpn: usize) -> Self {
        assert!(start_vpn < end_vpn);
        let start_page_node =
            Arc::new(unsafe { UserPromiseRefCell::new(PageNode::new(false, start_vpn)) });
        let end_page_node =
            Arc::new(unsafe { UserPromiseRefCell::new(PageNode::new(false, end_vpn)) });
        PageNode::link(&start_page_node, &end_page_node);
        Self {
            root: start_page_node,
        }
    }

    /// Find the interval which is contain the given page interval.
    /// - Arguments
    ///     - start_vpn: the first virtual memory page number
    ///     - end_vpn: the last virtual memory page number, which will not be allocated
    ///
    /// - Returns
    ///     - None: if there was no contiguous interval that contains the given inerval
    ///     - Some((start_page_node, end_page_node))
    fn find_range_nodes(
        &self,
        start_vpn: usize,
        end_vpn: usize,
    ) -> Option<(
        Arc<UserPromiseRefCell<PageNode>>,
        Arc<UserPromiseRefCell<PageNode>>,
    )> {
        let mut current_page_node = Arc::clone(&self.root);
        loop {
            let current_borrow = current_page_node.access();
            if let Some(next_page_node) = &current_borrow.next {
                let next_page_node = Arc::clone(next_page_node);
                let next_page_node_borrow = next_page_node.access();
                if current_borrow.vpn <= start_vpn && next_page_node_borrow.vpn >= end_vpn {
                    return Some((Arc::clone(&current_page_node), Arc::clone(&next_page_node)));
                } else if next_page_node_borrow.vpn <= start_vpn {
                    drop(current_borrow);
                    drop(next_page_node_borrow);
                    current_page_node = next_page_node;
                } else {
                    return None;
                }
            } else {
                return None;
            };
        }
    }

    /// Change specified interval's available status.
    /// - Arguments
    ///     - start_vpn: the first virtual memory page number
    ///     - end_vpn: the last virtual memory page number, which will not be allocated
    ///
    /// - Returns
    ///     - Some(()): change succeeded
    ///     - None: change failed
    fn change(&self, start_vpn: usize, end_vpn: usize, used: bool) -> Option<()> {
        if start_vpn >= end_vpn {
            return None;
        }
        let (left_page_node, right_page_node) = self.find_range_nodes(start_vpn, end_vpn)?;
        let left_page_node_borrow = left_page_node.access();
        let right_page_node_borrow = right_page_node.access();
        let left_page_node_vpn = left_page_node_borrow.vpn;
        let right_page_node_vpn = right_page_node_borrow.vpn;
        let left_used = left_page_node_borrow.used;
        drop(left_page_node_borrow);
        drop(right_page_node_borrow);
        match (
            left_page_node_vpn == start_vpn,
            right_page_node_vpn == end_vpn,
            left_used == used,
        ) {
            (_, _, true) => None,
            (false, false, _) => {
                let start_page_node =
                    Arc::new(unsafe { UserPromiseRefCell::new(PageNode::new(used, start_vpn)) });
                let end_page_node =
                    Arc::new(unsafe { UserPromiseRefCell::new(PageNode::new(!used, end_vpn)) });
                PageNode::link(&left_page_node, &start_page_node);
                PageNode::link(&start_page_node, &end_page_node);
                PageNode::link(&end_page_node, &right_page_node);
                Some(())
            }
            (false, true, _) => {
                let start_page_node =
                    Arc::new(unsafe { UserPromiseRefCell::new(PageNode::new(!used, start_vpn)) });
                start_page_node.exclusive_access().used = used;
                PageNode::link(&left_page_node, &start_page_node);
                PageNode::link(&start_page_node, &right_page_node);
                PageNode::merge_range(right_page_node);
                Some(())
            }
            (true, false, _) => {
                let end_page_node =
                    Arc::new(unsafe { UserPromiseRefCell::new(PageNode::new(!used, end_vpn)) });
                left_page_node.exclusive_access().used = used;
                PageNode::link(&left_page_node, &end_page_node);
                PageNode::link(&end_page_node, &right_page_node);
                PageNode::merge_range(left_page_node);
                Some(())
            }
            (true, true, _) => {
                left_page_node.exclusive_access().used = used;
                PageNode::merge_range(left_page_node);
                PageNode::merge_range(right_page_node);
                Some(())
            }
        }
    }

    /// Alloc virtual page interval.
    /// - Arguments
    ///     - start_vpn: the first virtual memory page number
    ///     - end_vpn: the last virtual memory page number, which will not be allocated
    ///
    /// - Returns
    ///     - Some(()): change succeeded
    ///     - None: change failed
        pub(crate) fn alloc(&self, start_vpn: usize, end_vpn: usize) -> Option<()> {
        self.change(start_vpn, end_vpn, true)
    }

    /// Dealloc virtual page interval.
    /// - Arguments
    ///     - start_vpn: the first virtual memory page number
    ///     - end_vpn: the last virtual memory page number, which will not be allocated
    ///
    /// - Returns
    ///     - Some(()): change succeeded
    ///     - None: change failed
        pub(crate) fn dealloc(&self, start_vpn: usize, end_vpn: usize) -> Option<()> {
        self.change(start_vpn, end_vpn, false)
    }
}

/// A physical memory frame allocation manager
/// which will keep all frames in control.
pub(crate) struct BTreeSetFrameAllocator {
    /// not yet allocated physical page number
    /// which will be allocated in next time allocating when no more recycled frames
    current_ppn: usize,
    /// end frame number, which will not be allocated,
    /// unless the page offset is 4K - 1;
    end_ppn: usize,
    /// a set of frames those have been released but not yet allocated
    recycled: BTreeSet<usize>,
}
impl BTreeSetFrameAllocator {
    /// Create a new BTreeSetFrameAllocator
    pub(crate) fn new() -> Self {
        Self {
            current_ppn: 0,
            end_ppn: 0,
            recycled: BTreeSet::new(),
        }
    }

    /// Get current physical page number
        pub(crate) fn current_ppn(&self) -> usize {
        self.current_ppn
    }

    /// Get the end of physical page number
        pub(crate) fn end_ppn(&self) -> usize {
        self.end_ppn
    }

    /// Initialize a new BTreeSetFrameAllocator
    /// 
    /// - Arguments
    ///     - current_ppn: the current physical page number which will be used in next time allocating
    ///     - end_ppn: the end physical page number which will not be used, it must greater than current
    pub(crate) fn init(&mut self, current_ppn: usize, end_ppn: usize) {
        assert!(current_ppn < end_ppn);
        self.current_ppn = current_ppn;
        self.end_ppn = end_ppn;
    }

    /// Alloc a new frame and return new physical page number
    /// 
    /// - Errors
    ///     - FrameExhausted
    pub(crate) fn alloc(&mut self) -> Result<usize> {
        if let Some(ppn) = self.recycled.pop_first() {
            Ok(ppn)
        } else {
            let ppn = self.current_ppn;
            if ppn >= self.end_ppn {
                Err(KernelError::FrameExhausted)
            } else {
                self.current_ppn += 1;
                Ok(ppn)
            }
        }
    }

    /// Dealloc a frame
    /// 
    /// - Errors
    ///     - FrameNotDeallocable(ppn)
    pub(crate) fn dealloc(&mut self, ppn: usize) -> Result<()> {
        if ppn >= self.current_ppn || !self.recycled.insert(ppn) {
            Err(KernelError::FrameNotDeallocable(ppn))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_page_range_alloctor_alloc_and_dealloc() {
        let alloctor = LinkedListPageRangeAllocator::new(0, 1);
        assert!(alloctor.alloc(0, 1).is_some());
        assert!(alloctor.alloc(0, 1).is_none());
        assert!(alloctor.dealloc(0, 1).is_some());
        assert!(alloctor.dealloc(0, 1).is_none());
        assert!(alloctor.alloc(0, 1).is_some());
        assert!(alloctor.alloc(0, 1).is_none());
        assert!(alloctor.dealloc(0, 1).is_some());
        assert!(alloctor.dealloc(0, 1).is_none());

        assert!(alloctor.alloc(0, 0).is_none());
        assert!(alloctor.dealloc(0, 0).is_none());
        assert!(alloctor.alloc(0, 2).is_none());
        assert!(alloctor.dealloc(0, 2).is_none());
        assert!(alloctor.alloc(1, 1).is_none());
        assert!(alloctor.dealloc(1, 1).is_none());
        assert!(alloctor.alloc(1, 2).is_none());
        assert!(alloctor.dealloc(1, 2).is_none());
        assert!(alloctor.alloc(2, 2).is_none());
        assert!(alloctor.dealloc(2, 2).is_none());
        assert!(alloctor.alloc(2, 3).is_none());
        assert!(alloctor.dealloc(2, 3).is_none());
        assert!(alloctor.alloc(3, 2).is_none());
        assert!(alloctor.dealloc(3, 2).is_none());

        let alloctor = LinkedListPageRangeAllocator::new(0, 6);
        assert!(alloctor.alloc(0, 1).is_some());
        assert!(alloctor.alloc(2, 3).is_some());
        assert!(alloctor.alloc(1, 2).is_some());
        assert!(alloctor.dealloc(1, 2).is_some());
        assert!(alloctor.dealloc(0, 1).is_some());
        assert!(alloctor.dealloc(2, 3).is_some());

        assert!(alloctor.alloc(0, 1).is_some());
        assert!(alloctor.alloc(0, 2).is_none());
        assert!(alloctor.dealloc(0, 2).is_none());
        assert!(alloctor.dealloc(0, 1).is_some());
        assert!(alloctor.alloc(2, 5).is_some());
        assert!(alloctor.alloc(3, 4).is_none());
        assert!(alloctor.alloc(2, 3).is_none());
        assert!(alloctor.alloc(4, 5).is_none());
        assert!(alloctor.alloc(1, 3).is_none());
        assert!(alloctor.alloc(0, 3).is_none());
        assert!(alloctor.alloc(4, 6).is_none());
    }

    #[test_case]
    fn test_frame_allocator_alloc_and_dealloc() {
        let mut allocator = BTreeSetFrameAllocator::new();
        allocator.init(0, 1);
        assert_eq!(allocator.current_ppn, 0);
        assert_eq!(allocator.end_ppn, 1);
        assert!(allocator.alloc().is_ok_and(|t| t == 0));
        assert_eq!(allocator.current_ppn, 1);
        assert_eq!(allocator.end_ppn, 1);
        assert!(allocator.alloc().is_err_and(|t| t.is_frameexhausted()));
        assert!(allocator.dealloc(0).is_ok());
        assert!(allocator.alloc().is_ok_and(|t| t == 0));
        assert_eq!(allocator.current_ppn, 1);
        assert_eq!(allocator.end_ppn, 1);
        assert!(allocator.alloc().is_err_and(|t| t.is_frameexhausted()));
    }
}

// @author:    olinex
// @time:      2024/04/16

// self mods

// use other mods
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::cell::{Ref, RefMut};
use core::str::FromStr as _;
use enum_group::EnumGroup;
use frontier_fs::OpenFlags;
use frontier_lib::model::signal::{Signal, SignalAction, SignalFlags};

// use self mods
use super::allocator::{AutoRecycledIdAllocator, IdTracker};
use super::context::TaskContext;
use super::signal::SignalControlBlock;
use crate::configs;
use crate::fs::inode::ROOT_INODE;
use crate::fs::stdio::{STDIN, STDOUT};
use crate::fs::File;
use crate::lang::container::UserPromiseRefCell;
use crate::memory::space::{Space, KERNEL_SPACE};
use crate::prelude::*;
use crate::sync::condvar::{Condvar, CondvarBlocking};
use crate::sync::mutex::{Mutex, MutexBlocking, MutexSpin};
use crate::sync::semaphore::{Semaphore, SemaphoreBlocking, SemaphoreSpin};
use crate::trap::context::TrapContext;

pub(crate) const ROOT_TID: usize = 0;
pub(crate) const ROOT_PID: usize = 0;

/// The tracker of kernel stack,
/// each time the tracker is creating, we will map kernel stack to the kernel space.
/// When the tracker is dropping, the kernel stack will be unmaped from kernel space.
pub(crate) struct KernelStack(IdTracker);
impl KernelStack {
    /// Create a new kernel stack id tracker and map a new kernel stack pages
    pub(crate) fn new() -> Result<Self> {
        let tracker = KERNEL_STACK_ALLOCATOR.alloc()?;
        KERNEL_SPACE.map_kernel_task_stack(tracker.id())?;
        Ok(Self(tracker))
    }

    /// Get the id of the kernel stack 
    pub(crate) fn id(&self) -> usize {
        self.0.id()
    }
}
impl Drop for KernelStack {
    /// Drop kernel stack id tracker and unmap kernel stack pages
    fn drop(&mut self) {
        KERNEL_SPACE.unmap_kernel_task_stack(self.id()).unwrap();
    }
}

/// The execution status of the task
#[derive(EnumGroup, Debug, Copy, Clone, PartialEq)]
pub(crate) enum TaskStatus {
    Ready,
    Running,
    Blocked,
    Zombie,
}

/// The task resource in user space, It manages the lifecycle of the task stack and trap context,
/// and when it is created, the task stack and trap context are created at the same time,
/// and when it is released, both are released at the same time.
pub(crate) struct TaskUserResource {
    /// Thread id tracker
    tracker: IdTracker,
    /// The weak reference to the process for resource recycling
    process: Weak<ProcessControlBlock>,
}
impl TaskUserResource {
    /// Create a task resource manager, alloc trap context and task stack in user space
    ///
    /// - Arguments
    ///     - tracker: task id tracker created by process control block
    ///     - process: the process control block reference
    ///
    /// - Errors
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    fn new(tracker: IdTracker, process: &Arc<ProcessControlBlock>) -> Result<Self> {
        let resource = Self {
            tracker,
            process: Arc::downgrade(process),
        };
        let mut process_inner = process.inner_exclusive_access();
        let base_size = process_inner.base_size;
        resource.alloc(&mut process_inner.space, base_size)?;
        Ok(resource)
    }

    /// Alloc trap context and task stack when resource was creating.
    ///
    /// - Arguments
    ///     - space: the mutable space reference which belongs to the process
    ///     - base_size: the byte size of the executable code and data
    ///
    /// - Errors
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    fn alloc(&self, space: &mut Space, base_size: usize) -> Result<()> {
        let tid = self.tracker.id();
        space.alloc_user_task_stack(base_size, tid)?;
        space.alloc_task_trap_ctx(tid)?;
        Ok(())
    }

    /// Dealloc trap context and task stack when resource was dropping.
    ///
    /// - Arguments
    ///     - space: the mutable space reference which belongs to the process
    ///     - base_size: the byte size of the executable code and data
    ///
    /// - Errors
    ///     - AreaDeallocFailed(start vpn, end vpn)
    fn dealloc(&self, space: &mut Space, base_size: usize) -> Result<()> {
        let tid = self.tracker.id();
        space.dealloc_task_trap_ctx(tid)?;
        space.dealloc_user_task_stack(base_size, tid)?;
        Ok(())
    }

    /// Help function for getting the trap context from task's virtual address space
    ///
    /// - Arguments
    ///     - space: the virtual address space
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    fn get_trap_ctx<'a>(&'a self, space: &'a Space) -> Result<&'a mut TrapContext> {
        let (start_vpn, end_vpn) = Space::get_task_trap_ctx_vpn_range(self.tracker.id());
        let trap_ctx_area = space.get_area(start_vpn, end_vpn)?;
        let trap_ctx = unsafe { trap_ctx_area.as_kernel_mut(start_vpn, 0)? };
        Ok(trap_ctx)
    }
}
impl Drop for TaskUserResource {
    fn drop(&mut self) {
        if let Some(process) = self.process.upgrade() {
            let mut process_inner = process.inner_exclusive_access();
            let base_size = process_inner.base_size;
            self.dealloc(&mut process_inner.space, base_size).unwrap();
        }
    }
}

/// The inner task control block contains all mutable task data.
pub(crate) struct TaskControlBlockInner {
    /// The running status of the task
    status: TaskStatus,
    /// Task context which contain the register value of the task
    task_ctx: TaskContext,
    /// Store the exit code which define when task exiting
    exit_code: Option<usize>,
    /// user mode resource
    user_resource: Option<TaskUserResource>,
}
impl TaskControlBlockInner {
    /// Create a new inner task control block and alloc task user resource
    ///
    /// - Arguments
    ///     - tracker: task id tracker created by process control block
    ///     - process: the process control block reference
    ///
    /// - Errors
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    fn new(tracker: IdTracker, process: &Arc<ProcessControlBlock>) -> Result<Self> {
        let resource = TaskUserResource::new(tracker, process)?;
        Ok(Self {
            status: TaskStatus::Ready,
            task_ctx: TaskContext::empty(),
            exit_code: None,
            user_resource: Some(resource),
        })
    }

    /// Release user resource and mark current task as zombie task,
    /// keep task exit code, cause other task in the same process may wait for it.
    ///
    /// - Arguments
    ///     - exit_code: the exit code of current task
    fn release_user_resource(&mut self, exit_code: usize) {
        let resource = self.user_resource.take();
        self.status = TaskStatus::Zombie;
        self.exit_code.replace(exit_code);
        self.user_resource = None;
        resource.unwrap();
    }

    /// Modify the trap context through closures to avoid complex borrowing lifecycles.
    ///
    /// - Arguments
    ///     - space: the user space which contains trap context
    ///     - f: closure function accept mutable trap context reference and return the modify result
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    ///     - Errors from `f` closure
    pub(crate) fn modify_trap_ctx<T>(
        &self,
        space: &Space,
        f: impl FnOnce(&mut TrapContext) -> Result<T>,
    ) -> Result<T> {
        let trap_ctx = self.user_resource.as_ref().unwrap().get_trap_ctx(space)?;
        f(trap_ctx)
    }

    /// Modify the task context through closures to avoid complex borrowing lifecycles.
    ///
    /// - Arguments
    ///     - f: closure function accept mutable task context reference and return the modify result
    ///
    /// - Errors
    ///     - Errors from `f` closure
    pub(crate) fn modify_task_ctx(
        &mut self,
        f: impl FnOnce(&mut TaskContext) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.task_ctx)
    }
}

/// The task control block contains all task data
pub(crate) struct TaskControlBlock {
    /// The kernel stack of task
    kernel_stack: KernelStack,
    /// The reference to process
    process: Weak<ProcessControlBlock>,
    /// Mutable inner block
    inner: UserPromiseRefCell<TaskControlBlockInner>,
}
impl TaskControlBlock {
    /// Create a new task control block and alloc kernel stack in kernel space.
    ///
    /// - Arguments
    ///     - tracker: task id tracker created by process control block
    ///     - process: the process control block reference
    ///
    /// - Errors
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    pub(crate) fn new(tracker: IdTracker, process: &Arc<ProcessControlBlock>) -> Result<Self> {
        let kernel_stack = KernelStack::new()?;
        let inner = TaskControlBlockInner::new(tracker, process)?;
        Ok(Self {
            kernel_stack,
            process: Arc::downgrade(process),
            inner: unsafe { UserPromiseRefCell::new(inner) },
        })
    }

    /// Fork a new process control block by current task control block.
    /// Only the root task is allow to call this method.
    ///
    /// - Errors
    ///     - IdExhausted
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    pub(crate) fn fork_process(&self) -> Result<Arc<ProcessControlBlock>> {
        let process = self.process();
        let new_process = process.fork()?;
        let tracker = new_process.tid_allocator.alloc()?;
        let new_tid = tracker.id();
        assert_eq!(new_tid, ROOT_TID);
        let new_task = Arc::new(Self::new(tracker, &new_process)?);
        let mut process_inner = process.inner_exclusive_access();
        let mut new_process_inner = new_process.inner_exclusive_access();
        // Copy user stack's bytes data from current task's space to new task's space
        let (user_stack_start_vpn, user_stack_end_vpn) =
            Space::get_user_task_stack_vpn_range(process_inner.base_size, ROOT_TID);
        new_process_inner.space.copy_area_from_another(
            &process_inner.space,
            user_stack_start_vpn,
            user_stack_end_vpn,
            user_stack_start_vpn,
            user_stack_end_vpn,
        )?;
        // Insert current created task control block into new process control block
        new_process_inner
            .tasks
            .insert(new_tid, Arc::clone(&new_task));
        // Each task control block have their own kernel stack,
        // so we must modify the virtual address of the kernel task stack in the kernel space.
        let kernel_task_stack_top_va =
            Space::get_kernel_task_stack_top_va(new_task.kernel_stack.id());
        let mut inner = self.inner_exclusive_access();
        let mut new_inner = new_task.inner_exclusive_access();
        inner.modify_trap_ctx(&process_inner.space, |pre_trap_ctx| {
            new_inner.modify_trap_ctx(&new_process_inner.space, |trap_ctx| {
                *trap_ctx = *pre_trap_ctx;
                trap_ctx.kernel_sp_va = kernel_task_stack_top_va;
                Ok(())
            })
        })?;
        inner.modify_task_ctx(|pre_task_ctx| {
            new_inner.modify_task_ctx(|task_ctx| {
                *task_ctx = *pre_task_ctx;
                task_ctx.goto_trap_return(kernel_task_stack_top_va);
                Ok(())
            })
        })?;
        process_inner
            .childrens
            .insert(new_process.pid(), Arc::clone(&new_process));
        drop(inner);
        drop(new_inner);
        drop(process_inner);
        drop(new_process_inner);
        Ok(new_process)
    }

    /// Rebulid user space and execute other program by inject code data to new space.
    /// Only the processes which are have one task can call the function,
    /// We can't define what actions need to be performed for other tasks.
    ///
    /// - Arguments
    ///     - path: the path of the process code data in file system
    ///     - data: the executable byte data readed from file
    ///     - args: the string contains all arguments seperated by blank whitespace
    /// - Errors
    ///     - ParseElfError
    ///     - InvalidHeadlessTask
    ///     - UnloadableTask
    ///     - FrameExhausted
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    ///     - VPNNotMapped(vpn)
    ///     - AreaNotExists(start_vpn, end_vpn)
    pub(crate) fn exec(&self, path: String, data: &[u8], args: String) -> Result<usize> {
        let process = self.process();
        let mut process_inner = process.inner_exclusive_access();
        let pid = process.tracker.id();
        if process_inner.have_multi_tasks() {
            return Err(KernelError::ExecWithMultiTasks(pid));
        }
        // check args length limit
        let path_slice = path.as_bytes();
        let args_slice = args.as_bytes();
        // both path and args strings need to be stored in bytes in the user-mode stack and start with the byte length
        if configs::COMMAND_LINE_ARGUMENTS_BYTE_SIZE < path_slice.len() + args_slice.len() + 2 {
            return Err(KernelError::OversizeArgs);
        }
        let (mut space, base_size, entry_point) = KERNEL_SPACE::new_user_from_elf(pid, data)?;
        let inner = self.inner_exclusive_access();
        let resource = inner.user_resource.as_ref().unwrap();
        let prev_base_size = process_inner.base_size;
        resource.dealloc(&mut process_inner.space, prev_base_size)?;
        resource.alloc(&mut space, base_size)?;
        let kid = self.kernel_stack.id();
        let tid = resource.tracker.id();
        let kernel_stack_top_va = Space::get_kernel_task_stack_top_va(kid);
        let mut user_stack_top_va = Space::get_user_task_stack_top_va(base_size, tid);
        // push arguments into user stack as byte slice
        for value in args_slice.iter().rev() {
            user_stack_top_va -= 1;
            let byte = space.translated_refmut(user_stack_top_va as *const u8)?;
            *byte = *value;
        }
        user_stack_top_va -= core::mem::size_of::<usize>();
        let length = space.translated_refmut(user_stack_top_va as *const usize)?;
        *length = args.len();
        // push path into user stack as byte slice
        for value in path_slice.iter().rev() {
            user_stack_top_va -= 1;
            let byte = space.translated_refmut(user_stack_top_va as *const u8)?;
            *byte = *value;
        }
        user_stack_top_va -= core::mem::size_of::<usize>();
        let length = space.translated_refmut(user_stack_top_va as *const usize)?;
        *length = path.len();
        process_inner.path = path;
        process_inner.space = space;
        process_inner.entry_point = entry_point;
        process_inner.base_size = base_size;
        inner.modify_trap_ctx(&process_inner.space, |trap_ctx| {
            *trap_ctx = TrapContext::create_app_init_context(
                entry_point,
                user_stack_top_va,
                kernel_stack_top_va,
            );
            trap_ctx.set_arg(0, 2);
            trap_ctx.set_arg(1, user_stack_top_va);
            Ok(())
        })?;
        Ok(2)
    }

    /// Try to wait a task according to task id and return the status code of task
    /// 
    /// - Arguments
    ///     - tid: task id for waiting
    ///     - exit_code_ptr: the pointer of mutable i32 variable
    /// 
    /// - Returns
    ///     - Ok(-1): task does not exist
    ///     - Ok(-2): task is still running
    ///     - Ok(task id) 
    /// 
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn wait_tid(&self, tid: isize, exit_code_ptr: *mut i32) -> Result<isize> {
        let process = self.process.upgrade().unwrap();
        let mut process_inner = process.inner_exclusive_access();
        let child_tids: Vec<usize> = process_inner.tasks.keys().map(|v| *v).collect();
        for child_tid in child_tids {
            let task = process_inner.tasks.get(&child_tid).unwrap();
            match (task.is_zombie(), tid as usize == child_tid, tid) {
                (true, _, -1) | (true, true, _) => {
                    let task = process_inner.tasks.remove(&child_tid).unwrap();
                    assert_eq!(Arc::strong_count(&task), 1);
                    let exit_code = task.inner_access().exit_code.unwrap();
                    let real_exit_code = process_inner.space.translated_refmut(exit_code_ptr)?;
                    *real_exit_code = exit_code as i32;
                    return Ok(child_tid as isize);
                }
                (false, true, _) => return Ok(-2),
                _ => continue,
            }
        }
        if tid == -1 && process_inner.tasks.len() != 0 {
            Ok(-2)
        } else {
            Ok(-1)
        }
    }

    /// See [`TaskControlBlockInner::release_user_resource`]
    fn release_user_resource(&self, exit_code: usize) {
        self.inner_exclusive_access()
            .release_user_resource(exit_code);
    }

    /// Get the inmutable inner structure
    #[inline(always)]
    pub(crate) fn inner_access(&self) -> Ref<'_, TaskControlBlockInner> {
        self.inner.access()
    }

    /// Get the mutable inner structure
    #[inline(always)]
    pub(crate) fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    /// Get the current task's context pointer
    pub(crate) fn task_ctx_ptr(&self) -> *const TaskContext {
        &self.inner_access().task_ctx as *const TaskContext
    }

    /// Get the process of the current task.
    /// Be careful, this method will panic when process was dropped.
    pub(crate) fn process(&self) -> Arc<ProcessControlBlock> {
        self.process.upgrade().unwrap()
    }

    /// Get the current task's unique id
    pub(crate) fn tid(&self) -> usize {
        self.inner_access()
            .user_resource
            .as_ref()
            .unwrap()
            .tracker
            .id()
    }

    /// Check if the task could fork new process and new task
    ///
    /// - Errors
    ///     - ForkWithNoRootTask(tid)
    pub(crate) fn forkable(&self) -> Result<()> {
        let tid = self.tid();
        if tid != ROOT_TID {
            Err(KernelError::ForkWithNoRootTask(tid))
        } else {
            Ok(())
        }
    }

    /// Check current task if is zombie status
    pub(crate) fn is_zombie(&self) -> bool {
        let inner = self.inner_access();
        inner.user_resource.is_none() && inner.status.is_zombie()
    }

    /// Mark current task as suspended task
    pub(crate) fn mark_suspended(&self) {
        let mut inner = self.inner_exclusive_access();
        inner.status = TaskStatus::Ready;
    }

    /// Mark current task as running task
    pub(crate) fn mark_running(&self) {
        let mut inner = self.inner_exclusive_access();
        inner.status = TaskStatus::Running;
    }

    /// Mark current task as bloced task
    pub(crate) fn mark_blocked(&self) {
        let mut inner = self.inner_exclusive_access();
        inner.status = TaskStatus::Blocked;
    }

    /// Mark current task as zombie task.
    /// If current task is the root task of the process,
    /// all other tasks in the same process will be change to zombie status as well.
    ///
    /// - Arguments
    ///     - exit_code: the exit code of current task
    pub(crate) fn mark_zombie(&self, exit_code: i32) {
        if self.tid() != ROOT_TID {
            self.release_user_resource(exit_code as usize);
        } else {
            let process = self.process();
            process.mark_zombie(exit_code);
            assert!(process.is_zombie());
        }
    }
}

/// The inner process control block which contains all process's mutable data
pub(crate) struct ProcessControlBlockInner {
    /// The path of the process code data in file system
    path: String,
    /// The virtual memory address of the entry point
    entry_point: usize,
    /// The size of the process's using virtual address from 0x00 to the top of the data
    base_size: usize,
    /// The virtual memory address space of the process
    space: Space,
    /// The parent process of the current process, if the parent is None, the current process will be the `initproc`
    parent: Option<Weak<ProcessControlBlock>>,
    /// Child process of the current process
    childrens: BTreeMap<usize, Arc<ProcessControlBlock>>,
    /// Store the exit code which define when task exiting
    exit_code: Option<usize>,
    /// The table of the files which is using by process
    fd_table: Vec<Option<Arc<dyn File>>>,
    /// The lock resource of which is using by process
    mutex_table: Vec<Option<Arc<dyn Mutex>>>,
    /// The semaphore resource of which is using by process
    semaphore_table: Vec<Option<Arc<dyn Semaphore>>>,
    /// The condition variable resource of which is using by process
    condvar_table: Vec<Option<Arc<dyn Condvar>>>,
    /// The block information all about signal
    signal: SignalControlBlock,
    /// All the tasks belongs to the current process
    tasks: BTreeMap<usize, Arc<TaskControlBlock>>,
}
impl ProcessControlBlockInner {
    /// Create a new inner process control block.
    ///
    /// - Arguments
    ///     - path: the path of the executable file in file system
    ///     - space: the virtual memory address space of the process
    ///     - entry_point: the virtual address to the first instruction will be run in the memory space
    ///     - base_size: the size of the process's using virtual address from 0x00 to the top of the data
    ///     - fd_table: the table of the files which is using by process
    fn new(
        path: String,
        space: Space,
        entry_point: usize,
        base_size: usize,
        fd_table: Vec<Option<Arc<dyn File>>>,
    ) -> Self {
        Self {
            path,
            entry_point,
            base_size,
            space,
            parent: None,
            childrens: BTreeMap::new(),
            exit_code: None,
            fd_table,
            mutex_table: Vec::new(),
            semaphore_table: Vec::new(),
            condvar_table: Vec::new(),
            signal: SignalControlBlock::new(),
            tasks: BTreeMap::new(),
        }
    }

    /// Check if the current process have multi tasks,
    fn have_multi_tasks(&self) -> bool {
        self.tasks.len() > 1
    }

    /// Check if the current process is zombie status
    fn is_zombie(&self) -> bool {
        self.tasks.len() == 0 || self.tasks.iter().all(|(_, v)| v.is_zombie())
    }

    /// Get the exit code of current process.
    fn get_exit_code(&self) -> Option<usize> {
        self.exit_code
    }

    /// Set the exit code of current process
    fn set_exit_code(&mut self, exit_code: usize) {
        self.exit_code.replace(exit_code);
    }

    /// Get the root task of process
    pub(crate) fn root_task(&self) -> Arc<TaskControlBlock> {
        Arc::clone(self.tasks.get(&ROOT_TID).unwrap())
    }

    /// Get the space of process
    pub(crate) fn space(&self) -> &Space {
        &self.space
    }

    /// Set the signal masking and return previous version masking
    pub(crate) fn exchange_singal_mask(&mut self, mask: SignalFlags) -> SignalFlags {
        self.signal.mask(mask)
    }

    /// Get the action according to signal
    pub(crate) fn get_signal_action(&self, signal: Signal) -> SignalAction {
        self.signal.get_action(signal)
    }

    /// Set a new action by signal
    pub(crate) fn set_signal_action(&mut self, signal: Signal, action: SignalAction) {
        self.signal.set_action(signal, action)
    }

    /// Clear the signal being processed, and resume the normal trap context.
    /// We use the value of a0 in trap_ctx as the return value of the system call instead of using a specific value like 0,
    /// otherwise when the user-mode recovery trap context is returned,
    /// the a0 register in the original process context will be overwritten by these specific values,
    /// making it impossible for the process to resume normal execution after the signal processing is complete.
    ///
    /// Be careful, we will use the root task to handler signal,
    /// so the trap context will also be resumed to root task's trap context.
    ///
    /// - Returns
    ///     - isize: the signal value
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    pub(crate) fn signal_return(&mut self) -> Result<isize> {
        let root_task = self.root_task();
        if let Some((_, trap_ctx_backup)) = self.signal.rollback() {
            let task_inner = root_task.inner_access();
            task_inner.modify_trap_ctx(&self.space, |trap_ctx| {
                *trap_ctx = trap_ctx_backup;
                Ok(trap_ctx.get_arg(0) as isize)
            })
        } else {
            Ok(-1)
        }
    }

    /// Allocate a condition variable.
    ///
    /// - Errors
    ///     - CondvarExhausted
    pub(crate) fn alloc_condvar(&mut self) -> Result<usize> {
        let condvar: Arc<dyn Condvar> = Arc::new(CondvarBlocking::new());
        for (id, wrapper) in self.condvar_table.iter_mut().enumerate() {
            if wrapper.is_none() {
                (*wrapper).replace(Arc::clone(&condvar));
                return Ok(id);
            }
        }
        let id = self.condvar_table.len();
        if id >= configs::MAX_CONDVAR_COUNT {
            Err(KernelError::CondvarExhausted)
        } else {
            self.condvar_table.push(Some(Arc::clone(&condvar)));
            Ok(id)
        }
    }

    /// Deallocate a condition variable and try to dealloc heap resource in task control context.
    /// 
    /// - Arguments
    ///     - id: the id of the condition variable
    /// 
    /// - Errors
    ///     - CondvarDoesNotExist
    pub(crate) fn dealloc_condvar(&mut self, id: usize) -> Result<()> {
        let wrapper = self
            .condvar_table
            .get_mut(id)
            .ok_or(KernelError::CondvarDoesNotExist(id))?;
        if wrapper.is_some() {
            wrapper.take();
        }
        if id == self.condvar_table.len() - 1 {
            loop {
                if self
                    .condvar_table
                    .get(self.condvar_table.len() - 1)
                    .is_none()
                {
                    self.condvar_table.pop();
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Get the condition variable immutable reference from task control context
    /// 
    /// - Arguments
    ///     - id: the id the condition variable
    pub(crate) fn get_condvar(&self, id: usize) -> Option<&Arc<dyn Condvar>> {
        self.condvar_table
            .get(id)
            .and_then(|wrapper| wrapper.as_ref())
    }

    /// Allocate a semaphore and set initial source count by count argument.
    /// 
    /// - Arguemnts
    ///     - blocking: if the semaphore is blocking type
    ///     - count: the count of initial source
    ///
    /// - Errors
    ///     - SemaphoreExhausted
    pub(crate) fn alloc_semaphore(&mut self, blocking: bool, count: isize) -> Result<usize> {
        let semaphore: Arc<dyn Semaphore> = if blocking {
            Arc::new(SemaphoreBlocking::new(count))
        } else {
            Arc::new(SemaphoreSpin::new(count))
        };
        for (id, wrapper) in self.semaphore_table.iter_mut().enumerate() {
            if wrapper.is_none() {
                (*wrapper).replace(Arc::clone(&semaphore));
                return Ok(id);
            }
        }
        let id = self.semaphore_table.len();
        if id >= configs::MAX_SEMAPHORE_COUNT {
            Err(KernelError::SemaphoreExhausted)
        } else {
            self.semaphore_table.push(Some(Arc::clone(&semaphore)));
            return Ok(id);
        }
    }

    /// Deallocate a semaphore and try to dealloc heap resource in task control context.
    /// 
    /// - Arguments
    ///     - id: the id of the semaphore
    /// 
    /// - Errors
    ///     - SemaphoreDoesNotExist
    pub(crate) fn dealloc_semaphore(&mut self, id: usize) -> Result<()> {
        let wrapper = self
            .semaphore_table
            .get_mut(id)
            .ok_or(KernelError::SemaphoreDoesNotExist(id))?;
        if wrapper.is_some() {
            wrapper.take();
        }
        if id == self.semaphore_table.len() - 1 {
            loop {
                if self
                    .semaphore_table
                    .get(self.semaphore_table.len() - 1)
                    .is_none()
                {
                    self.semaphore_table.pop();
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Get the semaphore immutable reference from task control context
    /// 
    /// - Arguments
    ///     - id: the id the semaphore
    pub(crate) fn get_semaphore(&self, id: usize) -> Option<&Arc<dyn Semaphore>> {
        self.semaphore_table
            .get(id)
            .and_then(|wrapper| wrapper.as_ref())
    }


    /// Allocate a mutex.
    /// 
    /// - Arguemnts
    ///     - blocking: if the mutex is blocking type
    ///
    /// - Errors
    ///     - MutexExhausted
    pub(crate) fn alloc_mutex(&mut self, blocking: bool) -> Result<usize> {
        let mutex: Arc<dyn Mutex> = if blocking {
            Arc::new(MutexBlocking::new())
        } else {
            Arc::new(MutexSpin::new())
        };
        for (id, wrapper) in self.mutex_table.iter_mut().enumerate() {
            if wrapper.is_none() {
                (*wrapper).replace(Arc::clone(&mutex));
                return Ok(id);
            }
        }
        let id = self.mutex_table.len();
        if id >= configs::MAX_MUTEX_COUNT {
            Err(KernelError::MutexExhausted)
        } else {
            self.mutex_table.push(Some(Arc::clone(&mutex)));
            return Ok(id);
        }
    }

    /// Deallocate a mutex and try to dealloc heap resource in task control context.
    /// 
    /// - Arguments
    ///     - id: the id of the mutex
    /// 
    /// - Errors
    ///     - MutexDoesNotExist
    pub(crate) fn dealloc_mutex(&mut self, id: usize) -> Result<()> {
        let wrapper = self
            .mutex_table
            .get_mut(id)
            .ok_or(KernelError::MutexDoesNotExist(id))?;
        if wrapper.is_some() {
            wrapper.take();
        }
        if id == self.mutex_table.len() - 1 {
            loop {
                if self.mutex_table.get(self.mutex_table.len() - 1).is_none() {
                    self.mutex_table.pop();
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Get the mutex immutable reference from task control context
    /// 
    /// - Arguments
    ///     - id: the id the mutex
    pub(crate) fn get_mutex(&self, id: usize) -> Option<&Arc<dyn Mutex>> {
        self.mutex_table
            .get(id)
            .and_then(|wrapper| wrapper.as_ref())
    }

    /// Allocate a file descriptor and set the file object into task control block context.
    ///
    /// - Arguments
    ///     - file: the object which impl File trait
    ///
    /// - Errors
    ///     - FileDescriptorExhausted
    pub(crate) fn alloc_fd(&mut self, file: Arc<dyn File>) -> Result<usize> {
        for (fd, wrapper) in self.fd_table.iter_mut().enumerate() {
            if wrapper.is_none() {
                (*wrapper).replace(file);
                return Ok(fd);
            }
        }
        let fd = self.fd_table.len();
        if fd >= configs::MAX_FD_COUNT {
            Err(KernelError::FileDescriptorExhausted)
        } else {
            self.fd_table.push(Some(file));
            Ok(fd)
        }
    }

    /// Deallocate a file by file descriptor and remove the file object from task control block context.
    ///
    /// - Arguments
    ///     - fd: file descriptor
    ///
    /// - Errors
    ///     - FileDescriptorDoesNotExist(file descriptor)
    pub(crate) fn dealloc_fd(&mut self, fd: usize) -> Result<()> {
        let wrapper = self
            .fd_table
            .get_mut(fd)
            .ok_or(KernelError::FileDescriptorDoesNotExist(fd))?;
        if wrapper.is_some() {
            wrapper.take();
        }
        if fd == self.fd_table.len() - 1 {
            loop {
                if self.fd_table.get(self.fd_table.len() - 1).is_none() {
                    self.fd_table.pop();
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Get the reference of the file object by file descriptor
    ///
    /// - Arguments
    ///     - fd: file descriptor
    pub(crate) fn get_file(&self, fd: usize) -> Option<&Arc<dyn File>> {
        self.fd_table.get(fd).and_then(|wrapper| wrapper.as_ref())
    }
}

pub(crate) struct ProcessControlBlock {
    /// Process id which the task is belongs to
    tracker: IdTracker,
    /// Thread id allocator
    tid_allocator: AutoRecycledIdAllocator,
    /// Mutable inner block
    inner: UserPromiseRefCell<ProcessControlBlockInner>,
}
impl ProcessControlBlock {
    /// Create a new process control block.
    /// Except [`self::INIT_PROC`], all other PCB must relate to the parent PCB.
    ///
    /// - Arguments
    ///     - path: the path of the process code data in file system
    ///     - data: the executable byte data readed from file
    ///     - parent: optional parent PCB
    ///
    /// - Returns
    ///     - Ok(Arc<Self>)
    ///
    /// - Errors
    ///     - IdExhausted
    ///     - ParseElfError
    ///     - InvalidHeadlessTask
    ///     - UnloadableTask
    ///     - FrameExhausted
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    fn new(path: String, data: &[u8], parent: Option<Arc<Self>>) -> Result<Arc<Self>> {
        let tracker = PID_ALLOCATOR.alloc()?;
        let pid = tracker.id();
        let (space, base_size, entry_point) = KERNEL_SPACE::new_user_from_elf(pid, data)?;
        let fd_table = vec![
            Some(Arc::clone(&STDIN)),
            Some(Arc::clone(&STDOUT)),
            Some(Arc::clone(&STDOUT)),
        ];
        let inner =
            ProcessControlBlockInner::new(path.clone(), space, entry_point, base_size, fd_table);
        debug!(
            "load process {} with pid: {}, base_size: {:#x}, entry_point: {:#x}",
            path, pid, base_size, entry_point
        );
        let child = Arc::new(Self {
            tracker,
            tid_allocator: AutoRecycledIdAllocator::new(configs::MAX_TID_COUNT),
            inner: unsafe { UserPromiseRefCell::new(inner) },
        });
        if let Some(parent) = parent {
            child
                .inner
                .exclusive_access()
                .parent
                .replace(Arc::downgrade(&parent));
            parent
                .inner_exclusive_access()
                .childrens
                .insert(pid, Arc::clone(&child));
        };
        child.alloc_task(entry_point, None)?;
        Ok(child)
    }

    /// Fork a new process control block and copy all of data from source space,
    /// except tasks's trap context and user stack.
    /// New process must be the child process of the source process.
    ///
    /// - Errors
    ///     - IdExhausted
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    ///     - VPNNotMapped(vpn)
    fn fork(self: &Arc<Self>) -> Result<Arc<Self>> {
        let mut parent_inner = self.inner_exclusive_access();
        let path = parent_inner.path.clone();
        let tracker = PID_ALLOCATOR.alloc()?;
        let pid = tracker.id();
        let mut exclude_ranges = BTreeSet::new();
        // exclude all of the tasks trap context and user stack
        for prev_tid in parent_inner.tasks.keys() {
            exclude_ranges.insert(Space::get_user_task_stack_vpn_range(
                parent_inner.base_size,
                *prev_tid,
            ));
            exclude_ranges.insert(Space::get_task_trap_ctx_vpn_range(*prev_tid));
        }
        let space =
            KERNEL_SPACE::new_user_from_another(pid, &parent_inner.space, Some(exclude_ranges))?;
        // copy all file descriptors to new process
        let mut fd_table = Vec::new();
        for wrapper in parent_inner.fd_table.iter() {
            if let Some(fd) = wrapper {
                fd_table.push(Some(Arc::clone(fd)))
            } else {
                fd_table.push(None)
            }
        }
        let inner = ProcessControlBlockInner::new(
            path.clone(),
            space,
            parent_inner.entry_point,
            parent_inner.base_size,
            fd_table,
        );
        debug!(
            "fork process {} with pid: {}, base size: {:#x}",
            path, pid, parent_inner.base_size,
        );
        let child = Arc::new(Self {
            tracker,
            tid_allocator: AutoRecycledIdAllocator::new(configs::MAX_TID_COUNT),
            inner: unsafe { UserPromiseRefCell::new(inner) },
        });
        let mut child_inner = child.inner_exclusive_access();
        child_inner.parent.replace(Arc::downgrade(self));
        parent_inner.childrens.insert(pid, Arc::clone(&child));
        drop(child_inner);
        drop(parent_inner);
        Ok(child)
    }

    /// Try to wait a process according to process id and return the status code of task
    /// 
    /// - Arguments
    ///     - pid: process id for waiting
    ///     - exit_code_ptr: the pointer of mutable i32 variable
    /// 
    /// - Returns
    ///     - Ok(-1): process does not exist
    ///     - Ok(-2): process is still running
    ///     - Ok(process id) 
    /// 
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn wait_pid(&self, pid: isize, exit_code_ptr: *mut i32) -> Result<isize> {
        let parent_id = self.pid();
        let mut inner = self.inner_exclusive_access();
        if inner.childrens.len() == 0 {
            return Ok(-1);
        }
        let child_pids: Vec<usize> = inner.childrens.keys().map(|v| *v).collect();
        for child_pid in child_pids {
            let child = inner.childrens.get(&child_pid).unwrap();
            match (child.is_zombie(), pid as usize == child_pid, pid) {
                (true, _, -1) | (true, true, _) => {
                    debug!("drop child process {} from parent {}", child_pid, parent_id);
                    let child = inner.childrens.remove(&child_pid).unwrap();
                    assert_eq!(Arc::strong_count(&child), 1);
                    let exit_code = child.inner_access().get_exit_code().unwrap();
                    let real_exit_code = inner.space.translated_refmut(exit_code_ptr)?;
                    *real_exit_code = exit_code as i32;
                    return Ok(child_pid as isize);
                }
                (false, true, _) => return Ok(-2),
                _ => continue,
            }
        }
        if pid == -1 {
            Ok(-2)
        } else {
            Ok(-1)
        }
    }

    /// Kill the current process.
    ///
    /// - Arguments
    ///     - signal: the value of the signal send from user mode
    ///
    /// - Errors
    ///     - DuplicateSignal(signal)
    pub(crate) fn kill(&self, signal: Signal) -> Result<()> {
        self.inner_exclusive_access().signal.try_kill(signal)
    }

    /// Create the initial process control block
    ///
    /// - Returns
    ///     - Ok(Arc<Self>)
    ///
    /// - Errors
    ///     - FileSystemError
    ///         - InodeMustBeDirectory(bitmap index)
    ///         - DataOutOfBounds
    ///         - NoDroptableBlockCache
    ///         - RawDeviceError(error code)
    ///         - DuplicatedFname(name, inode bitmap index)
    ///         - BitmapExhausted(start_block_id)
    ///         - BitmapIndexDeallocated(bitmap_index)
    ///         - RawDeviceError(error code)
    ///     - FileMustBeReadable(bitmap index)
    ///     - FileDoesNotExists(name)
    ///     - ParseStringError
    ///     - IdExhausted
    ///     - ParseElfError
    ///     - InvalidHeadlessTask
    ///     - UnloadableTask
    ///     - FrameExhausted
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    pub(crate) fn new_init_proc() -> Result<Arc<Self>> {
        let file = ROOT_INODE.find(&configs::INIT_PROCESS_PATH, OpenFlags::READ)?;
        let data = file.read_all()?;
        let name = String::from_str(configs::INIT_PROCESS_PATH)?;
        Ok(Self::new(name, &data, None)?)
    }

    /// Get the inmutable inner structure
    pub(crate) fn inner_access(&self) -> Ref<'_, ProcessControlBlockInner> {
        self.inner.access()
    }

    /// Get the mutable inner structure
    pub(crate) fn inner_exclusive_access(&self) -> RefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }

    /// Get the process unique id
    pub(crate) fn pid(&self) -> usize {
        self.tracker.id()
    }

    /// Get the mmu token from space
    pub(crate) fn user_token(&self) -> usize {
        self.inner_access().space.mmu_token()
    }

    /// Allocate new task control block.
    /// This method could be called by Arc<Self>.
    ///
    /// - Arguments
    ///     - entry_point: the virtual address to the first instruction will be run in the memory space
    ///
    /// - Errors
    ///     - AreaAllocFailed(start_vpn, end_vpn)
    ///     - VPNOutOfArea(vpn, start_vpn, end_vpn)
    ///     - VPNAlreadyMapped(vpn)
    ///     - InvaidPageTablePerm(flags)
    ///     - FrameExhausted
    ///     - AllocFullPageMapper(ppn)
    ///     - PPNAlreadyMapped(ppn)
    ///     - PPNNotMapped(ppn)
    pub(crate) fn alloc_task(
        self: &Arc<ProcessControlBlock>,
        entry_point: usize,
        arg: Option<usize>,
    ) -> Result<Arc<TaskControlBlock>> {
        let tracker = self.tid_allocator.alloc()?;
        let tid = tracker.id();
        let task = Arc::new(TaskControlBlock::new(tracker, self)?);
        let mut process_inner = self.inner_exclusive_access();
        let user_stack_top_va = Space::get_user_task_stack_top_va(process_inner.base_size, tid);
        let kernel_stack_top_va = Space::get_kernel_task_stack_top_va(task.kernel_stack.id());
        let mut task_inner = task.inner_exclusive_access();
        task_inner.modify_trap_ctx(&process_inner.space, |trap_ctx| {
            *trap_ctx = TrapContext::create_app_init_context(
                entry_point,
                user_stack_top_va,
                kernel_stack_top_va,
            );
            if let Some(arg) = arg {
                trap_ctx.set_arg(0, arg);
            }
            Ok(())
        })?;
        task_inner.modify_task_ctx(|task_ctx| {
            task_ctx.goto_trap_return(kernel_stack_top_va);
            Ok(())
        })?;
        process_inner.tasks.insert(tid, Arc::clone(&task));
        drop(process_inner);
        drop(task_inner);
        Ok(task)
    }

    /// Check if there are any bad signal is setted.
    pub(crate) fn check_bad_signals(&self) -> Option<Signal> {
        let inner = self.inner_access();
        inner.signal.get_bad_signal()
    }

    /// Make current process to handle all signals.
    ///
    /// - Returns
    ///     - Ok(killed, frozen)
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    pub(crate) fn handle_all_signals(&self) -> Result<(bool, bool)> {
        let mut inner = self.inner_exclusive_access();
        for signal in Signal::iter() {
            if !inner.signal.is_pending_signal(signal) {
                continue;
            }
            match signal {
                // STOP and CONT are a pair of semaphores that affect each other
                Signal::STOP => {
                    inner.signal.freeze();
                }
                Signal::CONT => {
                    inner.signal.cont();
                }
                Signal::KILL | Signal::DEF => {
                    inner.signal.kill();
                }
                other => {
                    // Get the signal handle action, all of the action handle fuction is pointed to 0
                    let action = inner.signal.get_action(signal);
                    let handler = action.handler();
                    // If handler is default action, just ignore it and continue
                    if handler.is_null() {
                        debug!(
                            "Handle signal {:?} with default action: ignore it or kill process",
                            signal
                        );
                        continue;
                    }
                    debug!(
                        "Handle signal {:?} with custom action: {}",
                        signal, handler as usize,
                    );
                    // Only root task is able to accept signal
                    let root_task = inner.root_task();
                    let task_inner = root_task.inner_access();
                    // Copy root task's trap context
                    let trap_ctx_backup = task_inner.modify_trap_ctx(&inner.space, |trap_ctx| {
                        let trap_ctx_backup = trap_ctx.clone();
                        trap_ctx.sepc = handler as usize;
                        trap_ctx.set_arg(0, signal as usize);
                        Ok(trap_ctx_backup)
                    })?;
                    // Backup trap context to the process control block
                    inner.signal.backup(other, trap_ctx_backup);
                    return Ok((inner.signal.is_killed(), inner.signal.is_frozen()));
                }
            }
        }
        return Ok((inner.signal.is_killed(), inner.signal.is_frozen()));
    }

    /// Check if the current process is zombie status
    pub(crate) fn is_zombie(&self) -> bool {
        self.inner_access().is_zombie()
    }

    /// Mark current process as zombie process.
    /// Only Arc<Self> is able to call this function.
    /// All of the tasks in the current process will be clear immediately.
    ///
    /// - Arguments
    ///     - exit_code: the exit code of current process
    pub(crate) fn mark_zombie(self: &Arc<Self>, exit_code: i32) {
        let inner = self.inner_access();
        let tasks: Vec<Arc<TaskControlBlock>> =
            inner.tasks.values().map(|task| Arc::clone(task)).collect();
        // because we will also use inner process control block when releasing task's user resource
        drop(inner);
        for task in tasks {
            if task.is_zombie() {
                continue;
            }
            task.release_user_resource(exit_code as usize);
            assert_eq!(Arc::strong_count(&task), 3);
        }
        let mut inner = self.inner_exclusive_access();
        for child in inner.childrens.values() {
            child.mark_zombie(exit_code);
            let mut child_inner = child.inner_exclusive_access();
            child_inner.parent.replace(Arc::downgrade(&INIT_PROC));
            INIT_PROC
                .inner_exclusive_access()
                .childrens
                .insert(child.pid(), Arc::clone(child));
        }
        inner.tasks.clear();
        inner.childrens.clear();
        inner.space.recycle_data_pages();
        inner.set_exit_code(exit_code as usize);
    }
}

lazy_static! {
    /// The global singleton allocator of process id
    static ref PID_ALLOCATOR: AutoRecycledIdAllocator =
        AutoRecycledIdAllocator::new(configs::MAX_PID_COUNT);

    /// The global singleton allocator of kernel stack
    static ref KERNEL_STACK_ALLOCATOR: AutoRecycledIdAllocator =
        AutoRecycledIdAllocator::new(configs::MAX_PID_COUNT * configs::MAX_TID_COUNT);

    /// The initial process which will be created when operation system is started
    pub(crate) static ref INIT_PROC: Arc<ProcessControlBlock> =
        ProcessControlBlock::new_init_proc().unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_kernel_stack() {
        let stack = KernelStack::new();
        assert!(stack.is_ok());
        let stack = stack.unwrap();
        let id = stack.id();
        drop(stack);
        let stack = KernelStack::new();
        assert!(stack.is_ok());
        let stack = stack.unwrap();
        assert_eq!(stack.id(), id);
    }
}

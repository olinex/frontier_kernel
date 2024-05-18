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
use frontier_lib::model::signal::{Signal, SignalAction, SignalFlags, SingalTable, SIG_COUNT};

// use self mods
use super::allocator::{AutoRecycledIdAllocator, IdTracker};
use super::context::TaskContext;
use crate::configs;
use crate::fs::inode::ROOT_INODE;
use crate::fs::stdio::{STDIN, STDOUT};
use crate::fs::File;
use crate::lang::container::UserPromiseRefCell;
use crate::memory::space::{Space, KERNEL_SPACE};
use crate::prelude::*;
use crate::trap::context::TrapContext;

pub(crate) const ROOT_TID: usize = 0;
pub(crate) const ROOT_PID: usize = 0;

/// The tracker of kernel stack,
/// each time the tracker is creating, we will map kernel stack to the kernel space.
/// When the tracker is dropping, the kernel stack will be unmaped from kernel space.
pub(crate) struct KernelStack(IdTracker);
impl KernelStack {
    pub(crate) fn new() -> Result<Self> {
        let tracker = KERNEL_STACK_ALLOCATOR.alloc()?;
        let kid = tracker.id();
        KERNEL_SPACE.map_kernel_task_stack(kid)?;
        Ok(Self(tracker))
    }

    pub(crate) fn id(&self) -> usize {
        self.0.id()
    }
}
impl Drop for KernelStack {
    fn drop(&mut self) {
        let kid = self.id();
        KERNEL_SPACE.unmap_kernel_task_stack(kid).unwrap();
    }
}

/// The execution status of the task
#[derive(EnumGroup, Debug, Copy, Clone, PartialEq)]
pub(crate) enum TaskStatus {
    Ready,
    Running,
    Suspended,
    Zombie,
}

/// The control block for signal mechanism, each process have only one signal control block.
#[derive(Debug)]
pub(crate) struct SignalControlBlock {
    handling: Option<Signal>,
    /// The signals which was set by kill syscall
    setted: SignalFlags,
    /// The mask of the signals which should not be active
    masked: SignalFlags,
    /// The functions for handler signals, the index of the action is the signal value
    actions: SingalTable,
    /// The backup value of normal trap context saved when handing signal
    trap_ctx_backup: Option<TrapContext>,
    /// If killed is true, the current task will be exit in the after
    /// see [`crate::task::process::PROCESSOR::handle_current_task_signals`]
    killed: bool,
    /// If frozen is true, the current task will stop running until receive CONT signal
    /// see [`crate::task::process::PROCESSOR::handle_current_task_signals`]
    frozen: bool,
}
impl SignalControlBlock {
    /// Create a new task empty signal control block
    fn new() -> Self {
        Self {
            handling: None,
            setted: SignalFlags::empty(),
            masked: SignalFlags::empty(),
            actions: SingalTable::new(),
            trap_ctx_backup: None,
            killed: false,
            frozen: false,
        }
    }

    /// Check whether the signal is pending or not based on the block.
    /// Any pending signal will be handle by custom action or default action.
    /// The signal must meet the following conditions:
    ///     - have been setted by kill syscall
    ///     - not blocked by global masking setting
    ///     - no other handling signal or the handing signal not block it
    ///
    /// - Arguments:
    ///     - signal: The signal being detected
    fn is_pending_signal(&self, signal: Signal) -> bool {
        let flag = signal.into();
        self.setted.contains(flag)
            && !self.masked.contains(flag)
            && match self.handling {
                Some(handing_signal) => !self
                    .actions
                    .get(handing_signal as usize)
                    .mask()
                    .contains(flag),
                None => true,
            }
    }
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
        debug!("alloc task {}'s trap context and user stack", tid);
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
        debug!("dealloc task {}'s trap context and user stack", tid);
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
        self.exit_code = Some(exit_code);
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
    fn new(tracker: IdTracker, process: &Arc<ProcessControlBlock>) -> Result<Self> {
        let kernel_stack = KernelStack::new()?;
        let inner = TaskControlBlockInner::new(tracker, process)?;
        Ok(Self {
            kernel_stack,
            process: Arc::downgrade(process),
            inner: unsafe { UserPromiseRefCell::new(inner) },
        })
    }

    pub(crate) fn fork_task(&self, entry_point: usize, arg: usize) -> Result<Arc<Self>> {
        self.forkable()?;
        let process = self.process();
        let tracker = process.tid_allocator.alloc()?;
        let new_task = Arc::new(Self::new(tracker, &process)?);
        let new_tid = new_task.tid();
        let mut new_inner = new_task.inner_exclusive_access();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.insert(new_tid, Arc::clone(&new_task));
        let kernel_stack_top_va = Space::get_kernel_task_stack_top_va(new_task.kernel_stack.id());
        let user_stack_top_va = Space::get_user_task_stack_top_va(process_inner.base_size, new_tid);
        new_inner.modify_trap_ctx(&process_inner.space, |trap_ctx| {
            *trap_ctx = TrapContext::create_app_init_context(
                entry_point,
                user_stack_top_va,
                kernel_stack_top_va,
            );
            trap_ctx.set_arg(0, arg);
            Ok(())
        })?;
        new_inner.modify_task_ctx(|task_ctx| {
            task_ctx.goto_trap_return(kernel_stack_top_va);
            Ok(())
        })?;
        drop(new_inner);
        drop(process_inner);
        Ok(new_task)
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
        self.forkable()?;
        let process = self.process();
        let new_process = process.fork()?;
        let tracker = new_process.tid_allocator.alloc()?;
        let new_tid = tracker.id();
        assert_eq!(new_tid, ROOT_TID);
        let new_task = Arc::new(Self::new(tracker, &new_process)?);
        let process_inner = process.inner_access();
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
        inner.status = TaskStatus::Suspended;
    }

    /// Mark current task as running task
    pub(crate) fn mark_running(&self) {
        let mut inner = self.inner_exclusive_access();
        inner.status = TaskStatus::Running;
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
            self.process().mark_zombie(exit_code);
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
        self.tasks.iter().all(|(_, v)| v.is_zombie())
    }

    /// Get the exit code of current process.
    fn get_exit_code(&self) -> Option<usize> {
        self.exit_code
    }

    /// Set the exit code of current process
    fn set_exit_code(&mut self, exit_code: usize) {
        self.exit_code = Some(exit_code);
    }

    /// Get the root task of process
    pub(crate) fn root_task(&self) -> Arc<TaskControlBlock> {
        Arc::clone(self.tasks.get(&0).unwrap())
    }

    /// Get the space of process
    pub(crate) fn space(&self) -> &Space {
        &self.space
    }

    /// Get the signal masking
    pub(crate) fn get_singal_mask(&self) -> SignalFlags {
        self.signal.masked
    }

    /// Set the signal masking
    pub(crate) fn set_singal_mask(&mut self, mask: SignalFlags) {
        self.signal.masked = mask;
    }

    pub(crate) fn get_signal_action(&self, signal: Signal) -> SignalAction {
        self.signal.actions.get(signal as usize)
    }

    pub(crate) fn set_signal_action(&mut self, signal: Signal, action: SignalAction) {
        self.signal.actions.set(signal as usize, action)
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
        let signal = match self.signal.handling {
            Some(signal) => signal,
            None => return Ok(-1),
        };
        let trap_ctx_backup = match self.signal.trap_ctx_backup {
            Some(ctx) => ctx,
            None => return Ok(-1),
        };
        self.signal.handling = None;
        self.signal.setted.remove(signal.into());
        self.signal.trap_ctx_backup = None;
        let task_inner = root_task.inner_access();
        task_inner.modify_trap_ctx(&self.space, |trap_ctx| {
            *trap_ctx = trap_ctx_backup;
            Ok(trap_ctx.get_arg(0) as isize)
        })
    }

    /// Allocate a file descriptor and set the file object into task control block context.
    ///
    /// - Arguments
    ///     - file: the object which impl File trait
    ///
    /// - Errors
    ///     - FileDescriptorExhausted
    pub(crate) fn allc_fd(&mut self, file: Arc<dyn File>) -> Result<usize> {
        for (fd, wrapper) in self.fd_table.iter_mut().enumerate() {
            if wrapper.is_none() {
                *wrapper = Some(file);
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
            child.inner.exclusive_access().parent = Some(Arc::downgrade(&parent));
            parent
                .inner_exclusive_access()
                .childrens
                .insert(pid, Arc::clone(&child));
        };
        child.alloc_task(entry_point)?;
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
        child_inner.parent = Some(Arc::downgrade(self));
        parent_inner.childrens.insert(pid, Arc::clone(&child));
        drop(child_inner);
        drop(parent_inner);
        Ok(child)
    }

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
                    debug!("drop child process {} from parent {}", child_pid, parent_id,);
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
        let mut inner = self.inner_exclusive_access();
        if inner.signal.setted.contains(signal.into()) {
            Err(KernelError::DuplicateSignal(signal))
        } else {
            inner.signal.setted.insert(signal.into());
            Ok(())
        }
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
            Ok(())
        })?;
        task_inner.modify_task_ctx(|task_ctx| {
            task_ctx.goto_trap_return(kernel_stack_top_va);
            Ok(())
        })?;
        process_inner.tasks.insert(tid, Arc::clone(&task));
        Ok(Arc::clone(&task))
    }

    /// Check if there are any bad signal is setted.
    pub(crate) fn check_bad_signals(&self) -> Option<Signal> {
        let inner = self.inner_access();
        if inner.signal.setted.contains(SignalFlags::INT) {
            Some(Signal::INT)
        } else if inner.signal.setted.contains(SignalFlags::ILL) {
            Some(Signal::ILL)
        } else if inner.signal.setted.contains(SignalFlags::ABRT) {
            Some(Signal::ABRT)
        } else if inner.signal.setted.contains(SignalFlags::FPE) {
            Some(Signal::FPE)
        } else if inner.signal.setted.contains(SignalFlags::SEGV) {
            Some(Signal::SEGV)
        } else {
            None
        }
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
        for signum in 0..SIG_COUNT {
            let signal: Signal = signum.try_into().unwrap();
            if !inner.signal.is_pending_signal(signal) {
                continue;
            }
            match signal {
                // STOP and CONT are a pair of semaphores that affect each other
                Signal::STOP => {
                    inner.signal.setted ^= SignalFlags::STOP;
                    inner.signal.frozen = true;
                }
                Signal::CONT => {
                    inner.signal.setted ^= SignalFlags::CONT;
                    inner.signal.frozen = false;
                }
                Signal::KILL | Signal::DEF => {
                    inner.signal.killed = true;
                }
                other => {
                    // Get the signal handle action, all of the action handle fuction is pointed to 0
                    let action = inner.signal.actions.get(signal as usize);
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
                    inner.signal.setted ^= other.into();
                    inner.signal.handling = Some(other);
                    inner.signal.trap_ctx_backup = Some(trap_ctx_backup);
                    return Ok((inner.signal.killed, inner.signal.frozen));
                }
            }
        }
        return Ok((inner.signal.killed, inner.signal.frozen));
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
        let tasks = inner.tasks.clone();
        // because we will also use inner process control block when releasing task's user resource
        drop(inner);
        for task in tasks.values() {
            if task.is_zombie() {
                continue;
            }
            task.release_user_resource(exit_code as usize);
        }
        drop(tasks);
        let mut inner = self.inner_exclusive_access();
        inner.tasks.clear();
        inner.set_exit_code(exit_code as usize);
        for child in inner.childrens.values() {
            child.mark_zombie(exit_code);
            let mut child_inner = child.inner_exclusive_access();
            child_inner.parent = Some(Arc::downgrade(&INIT_PROC));
            INIT_PROC
                .inner_exclusive_access()
                .childrens
                .insert(child.pid(), Arc::clone(child));
        }
        inner.childrens.clear();
        inner.space.recycle_data_pages();
    }
}

lazy_static! {
    /// The global singleton allocator of process id
    static ref PID_ALLOCATOR: AutoRecycledIdAllocator =
        AutoRecycledIdAllocator::new(configs::MAX_PID_COUNT);
}

lazy_static! {
    /// The global singleton allocator of kernel stack
    static ref KERNEL_STACK_ALLOCATOR: AutoRecycledIdAllocator =
        AutoRecycledIdAllocator::new(configs::MAX_PID_COUNT * configs::MAX_TID_COUNT);
}

lazy_static! {
    /// The initial process which will be created when operation system is started
    pub(crate) static ref INIT_PROC: Arc<ProcessControlBlock> =
        ProcessControlBlock::new_init_proc().unwrap();
}

lazy_static! {
    /// The initial task which is belongs to the initial process
    pub(crate) static ref INIT_TASK: Arc<TaskControlBlock> = INIT_PROC.inner_access().root_task();
}

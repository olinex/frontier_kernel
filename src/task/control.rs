// @author:    olinex
// @time:      2023/09/01

// self mods

// use other mods
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::cell::{Ref, RefMut};
use core::str::FromStr;
use enum_group::EnumGroup;
use frontier_fs::OpenFlags;
use frontier_lib::model::signal::{Signal, SignalAction, SignalFlags, SingalTable, SIG_COUNT};

// use self mods
use super::allocator::BTreePidAllocator;
use super::context::TaskContext;
use super::process::INITPROC;
use crate::configs;
use crate::fs::inode::ROOT_INODE;
use crate::fs::stdio::{STDIN, STDOUT};
use crate::fs::File;
use crate::lang::container::UserPromiseRefCell;
use crate::memory::page_table::PageTable;
use crate::memory::space::{Space, KERNEL_SPACE};
use crate::memory::PageTableTr;
use crate::prelude::*;
use crate::trap::context::TrapContext;

/// A tracker for process id
pub(crate) struct PidTracker {
    pid: usize,
}
impl PidTracker {
    /// Create a new process id tracker and allocate task kernel stack in kernel space.
    ///
    /// - Arguments
    ///     - pid: the process id which must be allocated from PID_ALLOATOR
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
    ///     - VPNNotMapped(vpn)
    fn new(pid: usize) -> Result<Self> {
        debug!("alloc new pid {}", pid);
        KERNEL_SPACE.map_kernel_task_stack(pid)?;
        Ok(Self { pid })
    }

    /// Get the vritual address of the kernel stack
    fn kernel_stack_top_va(&self) -> usize {
        let (_, kernel_stack_top_vpn) = Space::get_kernel_task_stack_vpn_range(self.pid());
        PageTable::cal_base_va_with(kernel_stack_top_vpn)
    }

    /// Get the tracker's process id
    fn pid(&self) -> usize {
        self.pid
    }
}
impl Drop for PidTracker {
    fn drop(&mut self) {
        debug!("dealloc new pid {}", self.pid());
        PID_ALLOCATOR.dealloc(self).unwrap();
        KERNEL_SPACE.unmap_kernel_task_stack(self.pid()).unwrap();
    }
}

lazy_static! {
    static ref PID_ALLOCATOR: Arc<UserPromiseRefCell<BTreePidAllocator>> =
        Arc::new(unsafe { UserPromiseRefCell::new(BTreePidAllocator::new()) });
}
impl PID_ALLOCATOR {
    /// Allocate the process id, each pid will be unique in each every hart
    ///
    /// - Errors
    ///     - PidExhausted
    fn alloc(&self) -> Result<PidTracker> {
        let pid = self.exclusive_access().alloc()?;
        PidTracker::new(pid)
    }

    /// Deallocates the given tracker's pid
    ///
    /// - Arguments
    ///     - tracker: the pid tracker to deallocate
    ///
    /// - Errors
    ///     - PidNotDeallocable(pid)
    fn dealloc(&self, tracker: &mut PidTracker) -> Result<()> {
        self.exclusive_access().dealloc(tracker.pid())
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

#[derive(Debug)]
pub(crate) struct TaskSignalMeta {
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
impl TaskSignalMeta {
    /// Create a new task empty signal meta
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

    /// Check whether the signal is pending or not based on the meta.
    /// Any pending signal will be handle by custom action of default action.
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

/// The meta information of the task when in supervisor mode context
pub(crate) struct TaskMetaInner {
    /// The running status of the task
    status: TaskStatus,
    /// The virtual memory address space of the task
    space: Space,
    /// The physical page number which saved the task's trap context
    trap_ctx_ppn: usize,
    /// The parent process of the current process, if the parent is None, the current process will be the `initproc`
    parent: Option<Weak<TaskMeta>>,
    /// Child process of the current process
    childrens: Vec<Arc<TaskMeta>>,
    /// Store the exit code which define when task exiting
    exit_code: usize,
    /// The file table of tasks which is using by task
    fd_table: Vec<Option<Arc<dyn File>>>,
    /// The meta information all about signal
    signal: TaskSignalMeta,
}
impl TaskMetaInner {
    #[inline(always)]
    fn get_exit_code(&self) -> usize {
        self.exit_code
    }

    #[inline(always)]
    fn set_exit_code(&mut self, exit_code: usize) {
        self.exit_code = exit_code;
    }

    #[inline(always)]
    pub(crate) fn get_singal_mask(&self) -> SignalFlags {
        self.signal.masked
    }

    #[inline(always)]
    pub(crate) fn set_singal_mask(&mut self, mask: SignalFlags) {
        self.signal.masked = mask;
    }

    #[inline(always)]
    pub(crate) fn get_signal_action(&self, signal: Signal) -> SignalAction {
        self.signal.actions.get(signal as usize)
    }

    #[inline(always)]
    pub(crate) fn set_signal_action(&mut self, signal: Signal, action: SignalAction) {
        self.signal.actions.set(signal as usize, action)
    }

    /// Clear the signal being processed, and resume the normal trap context.
    /// We use the value of a0 in trap_ctx as the return value of the system call instead of using a specific value like 0,
    /// otherwise when the user-mode recovery trap context is returned,
    /// the a0 register in the original process context will be overwritten by these specific values,
    /// making it impossible for the process to resume normal execution after the signal processing is complete.
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    #[inline(always)]
    pub(crate) fn signal_return(&mut self) -> Result<isize> {
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
        let trap_ctx_current = self.trap_ctx()?;
        *trap_ctx_current = trap_ctx_backup;
        Ok(trap_ctx_current.get_arg(0) as isize)
    }

    /// Help function for helping the trap context from task's virtual address space
    ///
    /// - Arguments
    ///     - space: the virtual address space
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    fn get_trap_ctx(space: &Space) -> Result<&mut TrapContext> {
        let trap_ctx_area = space.get_trap_context_area()?;
        let (trap_ctx_vpn, _) = trap_ctx_area.range();
        let trap_ctx = unsafe { trap_ctx_area.as_kernel_mut(trap_ctx_vpn, 0)? };
        Ok(trap_ctx)
    }

    fn status(&self) -> TaskStatus {
        self.status
    }

    pub(crate) fn space(&self) -> &Space {
        &self.space
    }

    /// Get the current space's trap context
    ///
    /// - Errors
    ///     - AreaNotExists(start_vpn, end_vpn)
    ///     - VPNNotMapped(vpn)
    pub(crate) fn trap_ctx(&self) -> Result<&mut TrapContext> {
        Self::get_trap_ctx(&self.space)
    }

    /// Allocate a file descriptor and set the file object into task meta context.
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

    /// Deallocate a file by file descriptor and remove the file object from task meta context.
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

pub(crate) struct TaskMeta {
    /// The name of the task
    name: String,
    /// Process id which the task is belongs to
    tracker: PidTracker,
    /// Task context which contain the register value of the task
    task_ctx: TaskContext,
    /// The size of the task's using virtual address from 0x00 to the top of the user stack
    base_size: usize,
    /// Mutable inner meta
    inner: UserPromiseRefCell<TaskMetaInner>,
}
impl TaskMeta {
    /// Create a new task meta and make the relationship between parent task and child task
    ///
    /// - Arguments
    ///     - tracker: the unique id for each task
    ///     - data: the byte data of the task
    ///     - parent: the optional parent task
    ///
    /// - Errors
    ///     - PidExhausted
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
    fn new(name: String, data: &[u8], parent: Option<Arc<Self>>) -> Result<Arc<Self>> {
        let tracker = PID_ALLOCATOR.alloc()?;
        let pid = tracker.pid();
        let kernel_stack_top_va = tracker.kernel_stack_top_va();
        let (space, user_stack_top_va, entry_point) = KERNEL_SPACE::new_user_from_elf(pid, data)?;
        let trap_ctx_ppn = space.trap_ctx_ppn()?;
        let trap_ctx = TaskMetaInner::get_trap_ctx(&space)?;
        *trap_ctx = TrapContext::create_app_init_context(
            entry_point,
            user_stack_top_va,
            kernel_stack_top_va,
        );
        let mut task_ctx = TaskContext::empty();
        task_ctx.goto_trap_return(kernel_stack_top_va);
        let inner = TaskMetaInner {
            status: TaskStatus::Ready,
            space,
            trap_ctx_ppn,
            parent: None,
            childrens: vec![],
            exit_code: 0,
            fd_table: vec![
                Some(Arc::clone(&STDIN)),
                Some(Arc::clone(&STDOUT)),
                Some(Arc::clone(&STDOUT)),
            ],
            signal: TaskSignalMeta::new(),
        };
        debug!(
            "load task {} with pid: {}, user_stack_top_va: {:#x}, kernel_stack_top_va: {:#x}, entry_point: {:#x}",
            name, pid, user_stack_top_va, kernel_stack_top_va, entry_point
        );
        let child = Arc::new(Self {
            name,
            tracker,
            task_ctx,
            base_size: user_stack_top_va,
            inner: unsafe { UserPromiseRefCell::new(inner) },
        });
        if let Some(parent) = parent {
            child.inner.exclusive_access().parent = Some(Arc::downgrade(&parent));
            parent
                .inner
                .exclusive_access()
                .childrens
                .push(Arc::clone(&child))
        };
        Ok(child)
    }

    /// Fork a new task meta by another task meta which wrapped by Arc.
    ///
    /// - Arguments
    ///     - self: another task meta which wrapped by Arc.
    ///
    /// - Errors
    ///     - PidExhausted
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
    pub(crate) fn fork(self: &Arc<Self>) -> Result<Arc<Self>> {
        let inner = self.inner_access();
        let name = self.name().clone();
        let tracker = PID_ALLOCATOR.alloc()?;
        let pid = tracker.pid();
        let kernel_stack_top_va = tracker.kernel_stack_top_va();
        let space = KERNEL_SPACE::new_user_from_another(pid, self.inner_access().space())?;
        let user_stack_top_va = self.base_size();
        let trap_ctx_ppn = space.trap_ctx_ppn()?;
        let mut task_ctx = TaskContext::empty();
        task_ctx.goto_trap_return(kernel_stack_top_va);
        let mut new_fd_table = Vec::new();
        for wrapper in inner.fd_table.iter() {
            if let Some(fd) = wrapper {
                new_fd_table.push(Some(Arc::clone(fd)));
            } else {
                new_fd_table.push(None);
            }
        }
        drop(inner);
        let inner = TaskMetaInner {
            status: TaskStatus::Ready,
            space,
            trap_ctx_ppn,
            parent: None,
            childrens: vec![],
            exit_code: 0,
            fd_table: new_fd_table,
            signal: TaskSignalMeta::new(),
        };
        debug!(
            "fork task {} with pid: {}, user_stack_top_va: {:#x}, kernel_stack_top_va: {:#x}",
            name, pid, user_stack_top_va, kernel_stack_top_va
        );
        let child = Arc::new(Self {
            name,
            tracker,
            task_ctx,
            base_size: user_stack_top_va,
            inner: unsafe { UserPromiseRefCell::new(inner) },
        });
        {
            let mut child_inner = child.inner_exclusive_access();
            let trap_ctx = child_inner.trap_ctx()?;
            trap_ctx.kernel_sp_va = kernel_stack_top_va;
            child_inner.parent = Some(Arc::downgrade(self));
            self.inner_exclusive_access()
                .childrens
                .push(Arc::clone(&child));
        }
        Ok(child)
    }

    /// Execute the given code in current task and trap context,
    /// so we must create a new address space by code data.
    ///
    /// - Arguments
    ///     - data: the code to execute
    ///     - path: the path of executable file in file system
    ///     - args: command line arguments in string format
    ///
    /// - Errors
    ///     - OversizeArgs
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
    pub(crate) fn exec(&self, data: &[u8], path: String, args: String) -> Result<usize> {
        // check args length limit
        let path_slice = path.as_bytes();
        let args_slice = args.as_bytes();
        // both path and args strings need to be stored in bytes in the user-mode stack and start with the byte length
        if configs::COMMAND_LINE_ARGUMENTS_BYTE_SIZE < path_slice.len() + args_slice.len() + 2 {
            return Err(KernelError::OversizeArgs);
        }
        // memory_set with elf program headers/trampoline/trap context/user stack
        let pid = self.pid();
        let kernel_stack_top_va = self.tracker.kernel_stack_top_va();
        let (space, mut user_stack_top_va, entry_point) =
            KERNEL_SPACE::new_user_from_elf(pid, data)?;
        let trap_ctx_ppn = space.trap_ctx_ppn()?;
        let mut inner = self.inner_exclusive_access();
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
        // substitute memory_set
        inner.space = space;
        // update trap_cx ppn
        inner.trap_ctx_ppn = trap_ctx_ppn;
        // initialize trap_cx
        let trap_ctx = inner.trap_ctx()?;
        *trap_ctx = TrapContext::create_app_init_context(
            entry_point,
            user_stack_top_va,
            kernel_stack_top_va,
        );
        trap_ctx.set_arg(0, 2);
        trap_ctx.set_arg(1, user_stack_top_va);
        Ok(2)
    }

    /// Wait for other process to finish and return the child process's exit code.
    /// If pid is -1, it means that any exited child process will be returned,
    /// or it will find the child process which's pid is equal to the pid argument and check the status.
    /// Only the child process was exited and been zombie status, this function will return the exit code.
    ///
    /// - Arguments
    ///     - pid: the pid of the child process we are waiting for
    ///     - exit_code_ptr: the mutable pointer which will hold the exit code
    ///
    /// - Returns
    ///     - -1: child process not exists
    ///     - -2: child process have not yet exited
    ///     - other positive integer: the exit code of the child process
    ///
    /// - Errors
    ///     - VPNNotMapped(vpn)
    pub(crate) fn wait(&self, pid: isize, exit_code_ptr: *mut i32) -> Result<isize> {
        let mut inner = self.inner_exclusive_access();
        for (index, child) in inner.childrens.iter().enumerate() {
            match (child.is_zombie(), pid as usize == child.pid(), pid) {
                (true, _, -1) | (true, true, _) => {
                    let child = inner.childrens.remove(index);
                    assert_eq!(Arc::strong_count(&child), 1);
                    let exit_code = child.inner_access().get_exit_code();
                    let real_exit_code = inner.space().translated_refmut(exit_code_ptr)?;
                    *real_exit_code = exit_code as i32;
                    return Ok(child.pid() as isize);
                }
                (false, true, _) => return Ok(-2),
                _ => continue,
            }
        }
        if pid == -1 && inner.childrens.len() != 0 {
            Ok(-2)
        } else {
            Ok(-1)
        }
    }

    /// Send signal to current task.
    ///
    /// - Arguments
    ///     - signal: which signal will be setted
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

    /// Check if each signal bit is turned on and perform different processing functions depending on the signal.
    /// The following signals are forcibly processed and taken over by the kernel in once:
    ///     - STOP
    ///     - CONT
    ///     - KILL
    ///     - DEF
    /// 
    /// And only one of the remaining signals will be processed.
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
                    let action = inner.signal.actions.get(signal as usize);
                    let handler = action.handler();
                    if handler.is_null() {
                        debug!(
                            "Handle signal {:?} with default action: ignore it or kill process",
                            signal
                        );
                        break;
                    }
                    debug!(
                        "Handle signal {:?} with custom action: {}",
                        signal, handler as usize,
                    );
                    let trap_ctx = inner.trap_ctx()?;
                    let trap_ctx_backup = trap_ctx.clone();
                    trap_ctx.sepc = handler as usize;
                    trap_ctx.set_arg(0, signal as usize);
                    inner.signal.setted ^= other.into();
                    inner.signal.handling = Some(other);
                    inner.signal.trap_ctx_backup = Some(trap_ctx_backup);
                    break;
                }
            }
        }
        return Ok((inner.signal.killed, inner.signal.frozen));
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

    /// Create a new initial process
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
    ///     - ParseUtf8Error
    ///     - PidExhausted
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
    pub(crate) fn new_init_proc() -> Result<Arc<Self>> {
        let file = ROOT_INODE.find(configs::INIT_PROCESS_PATH, OpenFlags::READ)?;
        let data = file.read_all()?;
        let name = String::from_str(configs::INIT_PROCESS_PATH)?;
        Ok(Self::new(name, &data, None)?)
    }

    /// Get the name of the process
    pub(crate) fn name(&self) -> &String {
        &self.name
    }

    /// Get the inmutable inner structure
    pub(crate) fn inner_access(&self) -> Ref<'_, TaskMetaInner> {
        self.inner.access()
    }

    /// Get the mutable inner structure
    pub(crate) fn inner_exclusive_access(&self) -> RefMut<'_, TaskMetaInner> {
        self.inner.exclusive_access()
    }

    /// Get the current task's context pointer
    pub(crate) fn task_ctx_ptr(&self) -> *const TaskContext {
        &self.task_ctx as *const TaskContext
    }

    pub(crate) fn pid(&self) -> usize {
        self.tracker.pid()
    }

    pub(crate) fn user_token(&self) -> usize {
        self.inner_access().space().mmu_token()
    }

    /// Check the state of task if was zombie
    pub(crate) fn is_zombie(&self) -> bool {
        self.inner_access().status().is_zombie()
    }

    /// The byte size of the task code and user stack
    pub(crate) fn base_size(&self) -> usize {
        self.base_size
    }

    pub(crate) fn mark_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        inner.status = TaskStatus::Suspended;
    }

    pub(crate) fn mark_running(&self) {
        let mut inner = self.inner.exclusive_access();
        inner.status = TaskStatus::Running;
    }

    /// Mark current task as zombie task and save exit code into the stack context.
    /// If there were any child task, all of there must be changed to bind init process as the parent process.
    ///
    /// - Arguments
    ///     - exit_code: The exit code passing from user space
    pub(crate) fn mark_zombie(&self, exit_code: i32) {
        let mut inner = self.inner.exclusive_access();
        inner.status = TaskStatus::Zombie;
        inner.set_exit_code(exit_code as usize);
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.childrens.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.childrens.push(Arc::clone(child));
        }
        inner.childrens.clear();
        inner.space.recycle_data_pages();
    }
}

/// Task queue that contains all ready tasks which are waiting for running
pub(crate) struct TaskController {
    dqueue: VecDeque<Arc<TaskMeta>>,
}

impl TaskController {
    /// Get the length of the queue
    pub(crate) fn len(&self) -> usize {
        self.dqueue.len()
    }

    /// Put a task into the queue
    pub(crate) fn put(&mut self, task: Arc<TaskMeta>) {
        self.dqueue.push_back(task);
    }

    /// Fetch and pop the first task from the queue
    pub(crate) fn fetch(&mut self) -> Option<Arc<TaskMeta>> {
        self.dqueue.pop_front()
    }

    pub(crate) fn get(&self, pid: usize) -> Option<Arc<TaskMeta>> {
        for task in self.dqueue.iter() {
            if task.pid() == pid {
                return Some(Arc::clone(task));
            }
        }
        None
    }

    /// Create a new task controller, which will load the task code and create the virtual address space
    pub(crate) fn new() -> Self {
        Self {
            dqueue: VecDeque::new(),
        }
    }
}

#[inline(always)]
pub(crate) fn init_pid_allocator() {
    let mut allocator = PID_ALLOCATOR.exclusive_access();
    allocator.init(0, configs::MAX_PID_COUNT);
    debug!(
        "initialized pid allocator with pid range [{}, {})",
        allocator.current_pid(),
        allocator.end_pid()
    );
}

// @author:    olinex
// @time:      2024/04/06

// self mods

// use other mods
use frontier_lib::model::signal::{Signal, SignalAction, SignalFlags};

// use self mods
use crate::prelude::*;
use crate::task::{PROCESSOR, TASK_CONTROLLER};

/// Send a signal to other(but also self) process.
///
/// - Arguments
///     - pid: the id of the process which we want to send signal
///     - signum: the value of the signal
///
/// - Errors
///     - ProcessHaveNotTask
///     - UnknownSignum(isize)
#[inline(always)]
pub(crate) fn sys_kill(pid: isize, signum: usize) -> Result<isize> {
    let signal: Signal = signum.try_into()?;
    if let Some(task) = TASK_CONTROLLER.get(pid) {
        debug!("Try to kill {} with signal {:?}", task.pid(), signal);
        if let Err(_) = task.kill(signal) {
            Ok(-1)
        } else {
            Ok(0)
        }
    } else {
        Err(KernelError::ProcessHaveNotTask)
    }
}

/// Registers a user-mode function as a handler for a signal
/// 
/// - Arguments
///     - signum: the number of the signal
///     - new_action: 
///         the pointer of the [`frontier_lib::model::signal::SignalAction`] immutable reference,
///         which contains the function user-mode virtual memory address and signal mask settings.
///     - old_action: 
///         the pointer of the [`frontier_lib::model::signal::SignalAction`] mutable reference,
///         which will be writen the value of previous action.
/// 
/// - Errors
///     - LibError::InvalidSignalNumber(signum)
///     - ProcessHaveNotTask
///     - VPNNotMapped(vpn)
#[inline(always)]
pub(crate) fn sys_sig_action(
    signum: usize,
    new_action: *const SignalAction,
    old_action: *mut SignalAction,
) -> Result<isize> {
    let signal: Signal = signum.try_into()?;
    let flag: SignalFlags = signal.into();
    if flag.is_empty()
        || new_action.is_null()
        || old_action.is_null()
        || flag == SignalFlags::KILL
        || flag == SignalFlags::STOP
    {
        return Ok(-1);
    };
    let task = PROCESSOR.current_task()?;
    let mut inner = task.inner_exclusive_access();
    let space = inner.space();
    let new_action = space.translated_refmut(new_action)?.clone();
    let old_action = space.translated_refmut(old_action)?;
    *old_action = inner.get_signal_action(signal);
    inner.set_signal_action(signal, new_action);
    debug!("Set action {:?} in process {} with signal {:?}", new_action, task.pid(), signal);
    Ok(0)
}

/// Set up signal masking for the current task.
/// 
/// - Arguments
///     - mask: the bitmap of signal masking
/// 
/// - Errors
///     - ProcessHaveNotTask
#[inline(always)]
pub(crate) fn sys_sig_proc_mask(mask: u32) -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let mut inner = task.inner_exclusive_access();
    let old_mask = inner.get_singal_mask();
    if let Some(mask) = SignalFlags::from_bits(mask) {
        inner.set_singal_mask(mask);
        Ok(old_mask.bits() as isize)
    } else {
        Ok(-1)
    }
}

/// Make current task return to normal trap context after handling signal
/// 
/// - Errors
///     - ProcessHaveNotTask
#[inline(always)]
pub(crate) fn sys_sig_return() -> Result<isize> {
    let task = PROCESSOR.current_task()?;
    let mut inner = task.inner_exclusive_access();
    inner.signal_return()
}

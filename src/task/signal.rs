// @author:    olinex
// @time:      2024/05/22

// self mods

// use other mods
use frontier_lib::model::signal::{Signal, SignalAction, SignalFlags, SingalTable};

// use self mods
use crate::prelude::*;
use crate::trap::context::TrapContext;

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
    pub(crate) fn new() -> Self {
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
    pub(crate) fn is_pending_signal(&self, signal: Signal) -> bool {
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

    /// Change current signal control block to frozen status
    pub(crate) fn freeze(&mut self) {
        self.setted ^= SignalFlags::STOP;
        self.frozen = true;
    }

    /// Change current signal control block continue to run
    pub(crate) fn cont(&mut self) {
        self.setted ^= SignalFlags::CONT;
        self.frozen = false;
    }

    /// Change current signal control block to killed status
    pub(crate) fn kill(&mut self) {
        self.killed = true;
    }

    /// Try to change current signal control block to killable status
    ///
    /// - Arguments
    ///     - signal: the signal which will be killed
    ///
    /// - Errors
    ///     - DuplicateSignal(signal)
    pub(crate) fn try_kill(&mut self, signal: Signal) -> Result<()> {
        if self.setted.contains(signal.into()) {
            Err(KernelError::DuplicateSignal(signal))
        } else {
            self.setted.insert(signal.into());
            Ok(())
        }
    }

    /// Change current signal control block's mask flags settings,
    /// and return the previous mask flags
    pub(crate) fn mask(&mut self, masking: SignalFlags) -> SignalFlags {
        let old_mask = self.masked;
        self.masked = masking;
        return old_mask;
    }

    /// Back up the current trap context and use the specified signal as the currently processing
    ///
    /// - Arguments
    ///     - signal: currently processing signal
    ///     - trap_ctx: the trap context wait to backup
    pub(crate) fn backup(&mut self, signal: Signal, trap_ctx: TrapContext) {
        self.setted ^= signal.into();
        self.handling.replace(signal);
        self.trap_ctx_backup.replace(trap_ctx);
    }

    /// Roll back to a no-signal state or return the backup trap context and signal
    /// 
    /// - Arguments
    ///     - Some(backup signal, backup trap context)
    pub(crate) fn rollback(&mut self) -> Option<(Signal, TrapContext)> {
        if let (Some(signal), Some(trap_ctx)) = (self.handling.take(), self.trap_ctx_backup.take())
        {
            self.setted.remove(signal.into());
            Some((signal, trap_ctx))
        } else {
            None
        }
    }

    /// Get the bad signal which was setted into signal control block
    pub(crate) fn get_bad_signal(&self) -> Option<Signal> {
        if self.setted.contains(SignalFlags::INT) {
            Some(Signal::INT)
        } else if self.setted.contains(SignalFlags::ILL) {
            Some(Signal::ILL)
        } else if self.setted.contains(SignalFlags::ABRT) {
            Some(Signal::ABRT)
        } else if self.setted.contains(SignalFlags::FPE) {
            Some(Signal::FPE)
        } else if self.setted.contains(SignalFlags::SEGV) {
            Some(Signal::SEGV)
        } else {
            None
        }
    }

    /// Get the action according to signal
    pub(crate) fn get_action(&self, signal: Signal) -> SignalAction {
        self.actions.get(signal as usize)
    }

    /// Set the action by signal
    pub(crate) fn set_action(&mut self, signal: Signal, action: SignalAction) {
        self.actions.set(signal as usize, action)
    }

    /// Check if killed
    pub(crate) fn is_killed(&self) -> bool {
        self.killed
    }

    /// check if fronzen
    pub(crate) fn is_frozen(&self) -> bool {
        self.frozen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test_case]
    fn test_freeze_and_continue() {
        let mut scb = SignalControlBlock::new();
        assert!(!scb.is_frozen());
        assert!(!scb.is_killed());
        scb.freeze();
        assert!(scb.is_frozen());
        assert!(!scb.is_killed());
        scb.cont();
        assert!(!scb.is_frozen());
        assert!(!scb.is_killed());
    }

    #[test_case]
    fn test_kill() {
        let mut scb = SignalControlBlock::new();
        assert!(!scb.is_killed());
        assert!(!scb.is_frozen());
        scb.kill();
        assert!(scb.is_killed());
        assert!(!scb.is_frozen());
    }

    #[test_case]
    fn test_try_kill_and_is_pending_signal() {
        let mut scb = SignalControlBlock::new();
        for signal in Signal::iter() {
            assert!(!scb.is_pending_signal(signal));
            assert!(scb.try_kill(signal).is_ok());
            assert!(scb.is_pending_signal(signal));
            assert!(scb.try_kill(signal).is_err_and(|err| err.is_duplicatesignal()));
            assert!(scb.is_pending_signal(signal));
        }
    }

    #[test_case]
    fn test_mask_and_is_pending_signal() {
        let mut scb = SignalControlBlock::new();
        for signal in Signal::iter() {
            assert!(!scb.is_pending_signal(signal));
            assert!(scb.try_kill(signal).is_ok());
            assert!(scb.is_pending_signal(signal));
            scb.mask(signal.into());
            assert!(!scb.is_pending_signal(signal));
        }
    }

    #[test_case]
    fn test_handle_and_is_pending_signal() {
        let mut scb = SignalControlBlock::new();
        let trap_ctx = TrapContext::default();
        for signal in Signal::iter() {
            assert!(!scb.is_pending_signal(signal));
            scb.backup(signal, trap_ctx);
            assert!(scb.is_pending_signal(signal));
            scb.rollback();
            assert!(!scb.is_pending_signal(signal));
            scb.backup(signal, trap_ctx);
            assert!(scb.is_pending_signal(signal));
            scb.mask(signal.into());
            assert!(!scb.is_pending_signal(signal));
        }
    }
}

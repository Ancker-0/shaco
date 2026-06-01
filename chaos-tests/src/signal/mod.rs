// HUMAN

mod action;
use action::*;
use num_enum::TryFromPrimitive;

pub const NSIG: u32 = 64;

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u32)]
pub enum Signal {
    SIGKILL = 9,
    SIGSTOP = 19,
    SIGCHLD = 17,
    SIGUSR1 = 10,
    SIGUSR2 = 12,
    SIGALRM = 14,
}
pub use Signal::*;

impl Signal {
    pub fn mask(self) -> u64 {
        1u64 << (self as u32)
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct Sigset(u64);

impl Sigset {
    pub fn empty() -> Self {
        Sigset(0)
    }
    pub fn contains(&self, sig: Signal) -> bool {
        (self.0 & sig.mask()) != 0
    }
    pub fn add(&mut self, sig: Signal) {
        self.0 |= sig.mask();
    }
    pub fn add_set(&mut self, sigset: Sigset) {
        self.0 |= sigset.0;
    }
    pub fn remove(&mut self, sig: Signal) {
        self.0 &= !sig.mask();
    }
    pub fn remove_set(&mut self, sigset: Sigset) {
        self.0 &= !sigset.0;
    }
    pub fn one(&self) -> Option<Signal> {
        if self.0 == 0 {
            None
        } else {
            let sig_num = self.0.trailing_zeros();
            Signal::try_from(sig_num).ok()
        }
    }
}

pub struct SigConfig {
    pub pending: Sigset,
    pub blocked: Sigset,
    pub actions: [SigAction; NSIG as usize],
}

impl SigConfig {
    pub fn new() -> Self {
        let actions = [SigAction {
            handler: SigHandler::SIG_DFL,
            flags: 0,
            mask: 0,
        }; NSIG as usize];
        Self {
            pending: Sigset(0),
            blocked: Sigset(0),
            actions,
        }
    }

    pub fn sig_pending(&self, signo: u32) -> bool {
        let sig: Result<Signal, _> = signo.try_into();
        signo
            .try_into()
            .map_or(false, |sig| self.pending.contains(sig))
    }

    pub fn sig_raise(&mut self, signo: u32) {
        signo.try_into().map(|sig| self.pending.add(sig));
    }

    pub fn coalesce_pending(&self) -> Sigset {
        let mut active = self.pending;
        active.remove_set(self.blocked);
        active
    }

    pub fn sig_clear(&mut self, signo: u32) {
        signo.try_into().map(|sig| self.pending.remove(sig));
    }

    pub fn sig_block(&mut self, mask: u64) {
        self.blocked.add_set(Sigset(mask));
        self.blocked.remove(SIGKILL);
        self.blocked.remove(SIGSTOP);
    }

    pub fn sig_unblock(&mut self, mask: u64) {
        self.blocked.remove_set(Sigset(mask));
    }

    pub fn sig_setmask(&mut self, mask: u64) {
        self.blocked = Sigset(mask);
        self.blocked.remove(SIGKILL);
        self.blocked.remove(SIGSTOP);
    }

    pub fn deliverable(&self) -> Option<u32> {
        self.coalesce_pending().one().map(|sig| sig as u32)
    }

    pub fn set_action(&mut self, signo: u32, action: SigAction) {
        if signo < NSIG as u32 && signo != SIGKILL as u32 && signo != SIGSTOP as u32 {
            self.actions[signo as usize] = action;
        }
    }

    pub fn get_action(&self, signo: u32) -> &SigAction {
        if (signo as usize) < self.actions.len() {
            &self.actions[signo as usize]
        } else {
            &self.actions[0]
        }
    }

    pub fn is_ignored(&self, signo: u32) -> bool {
        if (signo as usize) < self.actions.len() {
            self.actions[signo as usize].handler == SIG_IGN
        } else {
            false
        }
    }

    pub fn clear_non_caught(&mut self) {
        for i in 1..self.actions.len() {
            if self.actions[i].handler != SIG_DFL && self.actions[i].handler != SIG_IGN {
                self.actions[i].handler = SIG_DFL;
            }
        }
    }
}

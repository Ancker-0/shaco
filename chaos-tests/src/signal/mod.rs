pub const NSIG: u32 = 64;
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

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum SigHandler {
  SIG_DFL,
  SIG_IGN,
  Handler(usize),
}
pub use SigHandler::{SIG_DFL, SIG_IGN};

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct SigAction {
    pub handler: SigHandler,
    pub flags: u32,
    pub mask: u64,
}

pub struct SigSet {
    pub pending: u64,
    pub blocked: u64,
    pub actions: [SigAction; NSIG as usize],
}

impl SigSet {
    pub fn new() -> Self {
        let actions = [SigAction { handler: SigHandler::SIG_DFL, flags: 0, mask: 0 }; NSIG as usize];
        Self { pending: 0, blocked: 0, actions }
    }

    pub fn sig_pending(&self, signo: u32) -> bool {
        (self.pending & (1u64 << signo)) != 0
    }

    pub fn sig_raise(&mut self, signo: u32) {
        if signo < NSIG {
            self.pending |= 1u64 << signo;
        }
    }

    pub fn coalesce_pending(&mut self) -> u64 {
        let active = self.pending & !self.blocked;
        let mut result: u64 = 0;
        for i in 1..NSIG {
            if (active & (1u64 << i)) != 0 {
                result |= 1 << i;
            }
        }
        result
    }

    pub fn sig_clear(&mut self, signo: u32) {
        if signo < NSIG {
            self.pending &= !(1u64 << signo);
        }
    }

    pub fn sig_block(&mut self, mask: u64) {
        self.blocked |= mask;
        self.blocked &= !(SIGKILL.mask() | SIGSTOP.mask());
    }

    pub fn sig_unblock(&mut self, mask: u64) {
        self.blocked &= !mask;
    }

    pub fn sig_setmask(&mut self, mask: u64) {
        self.blocked = mask & !(SIGKILL.mask() | SIGSTOP.mask());
    }

    pub fn deliverable(&self) -> Option<u32> {
        let actionable = self.pending & !self.blocked;
        if actionable == 0 { return None; }
        for i in 1..NSIG {
            if (actionable & (1u64 << i)) != 0 {
                return Some(i);
            }
        }
        None
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

use bitflags::*;

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

bitflags! {
    pub struct SignalActionFlags : usize {
        const NOCLDSTOP = 1;
        const NOCLDWAIT = 2;
        const SIGINFO = 4;
        const ONSTACK = 0x08000000;
        const RESTART = 0x10000000;
        const NODEFER = 0x40000000;
        const RESETHAND = 0x80000000;
        const RESTORER = 0x04000000;
    }
}

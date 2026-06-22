use bitflags::*;
use alloc::sync::Arc;
use log::info;
// use crate::process::{Process, Thread};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum SigHandler {
    SIG_DFL,
    SIG_IGN,
    SIG_ERR,
    Handler(usize),
}
pub use SigHandler::{SIG_DFL, SIG_IGN, SIG_ERR};

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct SignalAction {
    pub handler: SigHandler,
    pub flags: u32,
    pub mask: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Siginfo {
    pub signo: i32,
    pub errno: i32,
    pub code: i32,
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

pub const SI_ASYNCNL: i32 = -60;
pub const SI_TKILL: i32 = -6;
pub const SI_SIGIO: i32 = -5;
pub const SI_ASYNCIO: i32 = -4;
pub const SI_MESGQ: i32 = -3;
pub const SI_TIMER: i32 = -2;
pub const SI_QUEUE: i32 = -1;
pub const SI_USER: i32 = 0;
pub const SI_KERNEL: i32 = 128;

// /// return whether this thread exits
// pub fn handle_signal(thread: &Arc<Thread>, tf: &mut UserContext) -> bool {
//     let mut process = thread.proc.lock();
//     while let Some((idx, info)) =
//         process
//             .sig_queue
//             .iter()
//             .enumerate()
//             .find_map(|(idx, &(info, tid))| {
//                 if (tid == -1 || tid as usize == thread.tid)
//                     && !thread
//                         .inner
//                         .lock()
//                         .sig_mask
//                         .contains(FromPrimitive::from_i32(info.signo).unwrap())
//                 {
//                     Some((idx, info))
//                 } else {
//                     None
//                 }
//             })
//     {
//         use crate::signal::SignalActionFlags;

//         let signal: Signal = <Signal as FromPrimitive>::from_i32(info.signo).unwrap();
//         info!(
//             "process {} thread {} received signal: {:?}",
//             process.pid, thread.tid, signal
//         );

//         process.sig_queue.remove(idx);
//         process.pending_sigset.remove(signal);

//         let action = process.dispositions[info.signo as usize];
//         let action_flags = SignalActionFlags::from_bits_truncate(action.flags);

//         // enter signal handler
//         match action.handler {
//         }
//     }
//     return false;
// }

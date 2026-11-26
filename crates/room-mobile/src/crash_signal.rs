//! Log native crashes (SIGSEGV, SIGABRT, …) with the last start-up step
//! before Android's own crash handler runs (docs/41).
//!
//! Android's `debuggerd` already writes a tombstone and a backtrace to the
//! crash log (`adb logcat -b crash`), but it cannot say what the game was
//! doing. This handler adds one `F room` line — signal, fault address and
//! the last `room: step:` line — to logcat and the log file, then hands the
//! signal to the handler that was there before, so the tombstone is still
//! written. It allocates nothing.

#![allow(unsafe_code)] // sigaction and a signal handler are C FFI.

use std::fmt::Write as _;
use std::sync::OnceLock;

const SIGNALS: [(libc::c_int, &str); 7] = [
    (libc::SIGSEGV, "SIGSEGV (bad memory access)"),
    (libc::SIGBUS, "SIGBUS (bad memory access)"),
    (libc::SIGABRT, "SIGABRT (abort)"),
    (libc::SIGILL, "SIGILL (illegal instruction)"),
    (libc::SIGFPE, "SIGFPE (arithmetic error)"),
    (libc::SIGTRAP, "SIGTRAP (trap)"),
    (libc::SIGSYS, "SIGSYS (bad system call)"),
];

/// The handlers that were installed before ours, by signal number.
static PREVIOUS: OnceLock<[(libc::c_int, libc::sigaction); 7]> = OnceLock::new();

/// A fixed buffer `fmt::Write` fills without allocating; text past the end
/// is dropped.
struct Buf {
    bytes: [u8; 512],
    len: usize,
}

impl std::fmt::Write for Buf {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        // Keep one byte for the NUL.
        let room = self.bytes.len() - 1 - self.len;
        let n = s.len().min(room);
        self.bytes[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

extern "C" fn on_signal(sig: libc::c_int, info: *mut libc::siginfo_t, _ctx: *mut libc::c_void) {
    let name = SIGNALS
        .iter()
        .find(|(s, _)| *s == sig)
        .map_or("signal", |(_, n)| *n);
    let mut buf = Buf {
        bytes: [0; 512],
        len: 0,
    };
    // SAFETY: the kernel passes a valid siginfo with SA_SIGINFO.
    let addr = if info.is_null() {
        std::ptr::null_mut()
    } else {
        unsafe { (*info).si_addr() }
    };
    // SAFETY: gettid has no preconditions.
    let tid = unsafe { libc::gettid() };
    let _ = write!(
        buf,
        "room: FATAL {name} at address {addr:p} in thread {tid}; last "
    );
    let mut step = [0u8; 300];
    let n = room_app::startlog::last_step_into(&mut step);
    if n == 0 {
        let _ = buf.write_str("step unknown");
    } else {
        let _ = buf.write_str(std::str::from_utf8(&step[..n]).unwrap_or("step (not text)"));
    }
    let end = buf.len;
    buf.bytes[end] = 0;
    crate::logcat::log_fatal_raw(&buf.bytes[..=end]);

    // Put the old handler back and let it run (debuggerd's tombstone).
    if let Some(previous) = PREVIOUS.get() {
        for (s, action) in previous {
            if *s == sig {
                // SAFETY: restoring an action read by sigaction earlier.
                unsafe {
                    libc::sigaction(sig, action, std::ptr::null_mut());
                }
            }
        }
    }
    // A fault re-runs the faulting instruction on return and reaches the old
    // handler with its real fault address. abort() and other sent signals
    // must be sent again (they are blocked until this handler returns).
    let sent = !info.is_null() && unsafe { (*info).si_code } <= 0;
    if sig == libc::SIGABRT || sent {
        // SAFETY: raise has no preconditions.
        unsafe {
            libc::raise(sig);
        }
    }
}

/// Install the handler for the crash signals. Call once, early.
pub(crate) fn install() {
    let mut previous = [(0, unsafe_zeroed_action()); 7];
    for (i, (sig, _)) in SIGNALS.iter().enumerate() {
        let mut action = unsafe_zeroed_action();
        let handler: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) =
            on_signal;
        action.sa_sigaction = handler as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        let mut old = unsafe_zeroed_action();
        // SAFETY: valid action structs; the handler has the SA_SIGINFO
        // signature.
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(*sig, &action, &mut old);
        }
        previous[i] = (*sig, old);
    }
    let _ = PREVIOUS.set(previous);
}

fn unsafe_zeroed_action() -> libc::sigaction {
    // SAFETY: `sigaction` is plain data; all-zero is a valid "default" value.
    unsafe { std::mem::zeroed() }
}

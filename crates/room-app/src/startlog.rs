//! Start-up step log (docs/41): one line per step, so the last line shows
//! where a start that dies stopped.
//!
//! On phones the shell installs a sink ([`set_sink`]) that writes each line
//! to logcat and to a file at once (not through the stderr pipe, which can
//! lose its last lines when the process dies). On desktops the lines are
//! printed only with `ROOM_TRACE_START=1`, so normal logs do not change.

use std::sync::{Mutex, OnceLock};

static SINK: OnceLock<fn(&str)> = OnceLock::new();
/// When the first step was logged: lines carry the time since (where the
/// seconds of a slow phone start go).
static START: OnceLock<std::time::Instant> = OnceLock::new();
static LAST: Mutex<String> = Mutex::new(String::new());

/// Send step lines to `sink` (phones). Call once, before [`crate::run_with_event_loop`].
pub fn set_sink(sink: fn(&str)) {
    let _ = SINK.set(sink);
}

/// Log one start-up step as `room: step: +1.234s <what>` (seconds since the
/// first step) and remember it as the last step reached.
pub fn step(what: impl std::fmt::Display) {
    let t = START.get_or_init(std::time::Instant::now).elapsed();
    let line = format!("room: step: +{:.3}s {what}", t.as_secs_f64());
    if let Ok(mut last) = LAST.lock() {
        last.clear();
        last.push_str(&line);
    }
    match SINK.get() {
        Some(sink) => sink(&line),
        None if verbose() => eprintln!("{line}"),
        None => {}
    }
}

/// The last step logged, for crash handlers. `try_lock`: never blocks, so it
/// is usable from a signal handler (returns `None` if the lock is held).
#[must_use]
pub fn last_step() -> Option<String> {
    LAST.try_lock()
        .ok()
        .map(|s| s.clone())
        .filter(|s| !s.is_empty())
}

/// Copy the last step into `buf` without allocating (for a signal handler);
/// returns the bytes written, 0 if there is none or the lock is held.
pub fn last_step_into(buf: &mut [u8]) -> usize {
    let Ok(last) = LAST.try_lock() else {
        return 0;
    };
    let n = last.len().min(buf.len());
    buf[..n].copy_from_slice(&last.as_bytes()[..n]);
    n
}

fn verbose() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ROOM_TRACE_START").is_some())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn the_last_step_is_remembered() {
        step("one");
        step(format_args!("two {}", 2));
        let last = last_step().unwrap();
        assert!(last.starts_with("room: step: +"), "{last}");
        assert!(last.ends_with("s two 2"), "{last}");
        let mut buf = [0u8; 8];
        assert_eq!(last_step_into(&mut buf), 8);
        assert_eq!(&buf, b"room: st");
    }
}

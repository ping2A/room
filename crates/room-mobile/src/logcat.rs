//! Send stdout and stderr to the Android system log (`adb logcat -s room`),
//! and keep a copy in a file on the phone.
//!
//! The engine reports everything with `eprintln!("room: …")`. An Android app
//! has no terminal, so its stdout/stderr go nowhere unless redirected: this
//! points both at a pipe and copies each line to logcat from a thread.
//!
//! Every line also goes to `room-log.txt` (see [`open_log_file`]); the run
//! before is kept as `room-log-previous.txt`. Each line is written with one
//! `write` call, so it survives the process dying right after.

#![allow(unsafe_code)] // libc pipe/dup2/write and the liblog call are C FFI.

use std::ffi::{CString, c_char, c_int};
use std::io::{BufRead, BufReader};
use std::os::fd::{FromRawFd, IntoRawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};

/// `ANDROID_LOG_INFO` from `<android/log.h>`.
const ANDROID_LOG_INFO: c_int = 4;
/// `ANDROID_LOG_ERROR`.
const ANDROID_LOG_ERROR: c_int = 6;
/// `ANDROID_LOG_FATAL`.
const ANDROID_LOG_FATAL: c_int = 7;

/// The log file's descriptor (-1: none yet). A raw descriptor so the signal
/// handler can write to it.
static LOG_FD: AtomicI32 = AtomicI32::new(-1);
/// A second copy in a folder the Files app shows (-1: none).
static SHARED_FD: AtomicI32 = AtomicI32::new(-1);

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

/// Write one line to logcat under the tag `room`, at once (not through the
/// stderr pipe, so it is not lost if the process dies right after), and to
/// the log file.
pub(crate) fn log_line(line: &str) {
    write(ANDROID_LOG_INFO, line);
    file_line(line);
}

/// Same, as an error (`E/room`), for crashes and failed starts.
pub(crate) fn log_error(line: &str) {
    write(ANDROID_LOG_ERROR, line);
    file_line(line);
}

/// Sink for `room_app::startlog` step lines.
pub(crate) fn log_step(line: &str) {
    log_line(line);
}

fn write(prio: c_int, line: &str) {
    let Ok(text) = CString::new(line.replace('\0', " ")) else {
        return;
    };
    // SAFETY: `tag` and `text` are NUL-terminated and outlive the call;
    // liblog copies the bytes before returning.
    unsafe {
        __android_log_write(prio, c"room".as_ptr(), text.as_ptr());
    }
}

/// Write raw bytes to logcat as a fatal line and to the log file, without
/// allocating (for the signal handler). `text` must end with a NUL byte.
pub(crate) fn log_fatal_raw(text: &[u8]) {
    if text.last() != Some(&0) {
        return;
    }
    // SAFETY: NUL-terminated (checked above); liblog copies the bytes.
    unsafe {
        __android_log_write(ANDROID_LOG_FATAL, c"room".as_ptr(), text.as_ptr().cast());
    }
    let body = &text[..text.len() - 1];
    raw_file_write(body);
    raw_file_write(b"\n");
}

fn raw_file_write(bytes: &[u8]) {
    for fd in [
        LOG_FD.load(Ordering::Relaxed),
        SHARED_FD.load(Ordering::Relaxed),
    ] {
        if fd >= 0 {
            // SAFETY: `fd` is a log file, open for the life of the process;
            // `bytes` is valid for its length. A short write only loses log
            // text.
            unsafe {
                libc::write(fd, bytes.as_ptr().cast(), bytes.len());
            }
        }
    }
}

fn file_line(line: &str) {
    if LOG_FD.load(Ordering::Relaxed) < 0 && SHARED_FD.load(Ordering::Relaxed) < 0 {
        return;
    }
    let mut out = Vec::with_capacity(line.len() + 32);
    out.extend_from_slice(timestamp().as_bytes());
    out.extend_from_slice(line.as_bytes());
    out.push(b'\n');
    raw_file_write(&out);
}

/// `ssss.mmm ` since the log file was opened.
fn timestamp() -> String {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t = START.get_or_init(std::time::Instant::now).elapsed();
    format!("{:4}.{:03} ", t.as_secs(), t.subsec_millis())
}

/// Start `dir/room-log.txt` for this run, keeping the last run's as
/// `room-log-previous.txt`. Returns the previous log's last `tail` lines
/// (to show in logcat: they say how the last run ended).
pub(crate) fn open_log_file(dir: &Path, tail: usize) -> (PathBuf, Vec<String>) {
    let path = dir.join("room-log.txt");
    let previous = dir.join("room-log-previous.txt");
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::rename(&path, &previous);
    let last_lines = std::fs::read_to_string(&previous)
        .map(|text| {
            let lines: Vec<String> = text.lines().map(str::to_owned).collect();
            lines[lines.len().saturating_sub(tail)..].to_vec()
        })
        .unwrap_or_default();
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
    {
        LOG_FD.store(file.into_raw_fd(), Ordering::Relaxed);
    }
    (path, last_lines)
}

/// Also copy every line to `dir/room-log.txt` (a folder the phone's Files
/// app shows, `Documents/Room`). The app may not own a file a previous
/// install left there (Android then refuses to open it), so a dated name is
/// used in that case. Returns the file, or `None` when nothing could be
/// written there.
pub(crate) fn open_shared_log(dir: &Path) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    for name in ["room-log.txt".to_owned(), format!("room-log-{stamp}.txt")] {
        let path = dir.join(name);
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
        {
            SHARED_FD.store(file.into_raw_fd(), Ordering::Relaxed);
            return Some(path);
        }
    }
    None
}

/// An Android system property (`ro.product.model`, …), empty if unset.
pub(crate) fn system_property(name: &str) -> String {
    let Ok(name) = CString::new(name) else {
        return String::new();
    };
    // PROP_VALUE_MAX is 92 bytes including the NUL.
    let mut value = [0 as c_char; 92];
    // SAFETY: `name` is NUL-terminated; `value` has PROP_VALUE_MAX bytes,
    // the most the call writes.
    let n = unsafe { libc::__system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    if n <= 0 {
        return String::new();
    }
    // SAFETY: the call NUL-terminated `value`.
    unsafe { std::ffi::CStr::from_ptr(value.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// Log every panic straight to logcat and the log file (message, place,
/// thread, last start-up step, backtrace), then run the hook that was
/// installed before. The default hook prints to stderr, which reaches logcat
/// only through the pipe thread and can be lost when the app closes at once.
pub(crate) fn install_panic_logger() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let place = info
            .location()
            .map_or_else(String::new, |l| format!(" at {}:{}", l.file(), l.line()));
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_owned());
        log_error(&format!(
            "room: PANIC in thread '{}'{place}: {message}",
            thread.name().unwrap_or("?")
        ));
        if let Some(step) = room_app::startlog::last_step() {
            log_error(&format!("room: PANIC after {step}"));
        }
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        for line in trace.lines().take(80) {
            log_error(&format!("room:   {line}"));
        }
        previous(info);
    }));
}

/// Redirect fd 1 and fd 2 into logcat (and the log file). Call once,
/// before anything prints.
pub(crate) fn redirect_stdio() -> std::io::Result<()> {
    let mut fds: [c_int; 2] = [-1, -1];
    // SAFETY: `pipe` writes two descriptors into the two-element array.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let [read_fd, write_fd] = fds;
    // SAFETY: both descriptors were just returned by `pipe`; `dup2` replaces
    // stdout/stderr with the write end. The original write end is closed
    // afterwards, leaving fds 1 and 2 as its only users.
    unsafe {
        if libc::dup2(write_fd, 1) < 0 || libc::dup2(write_fd, 2) < 0 {
            let err = std::io::Error::last_os_error();
            libc::close(read_fd);
            libc::close(write_fd);
            return Err(err);
        }
        libc::close(write_fd);
    }
    // SAFETY: `read_fd` is an open descriptor owned by nothing else.
    let reader = unsafe { std::fs::File::from_raw_fd(read_fd) };
    std::thread::Builder::new()
        .name("room-logcat".into())
        .spawn(move || {
            for line in BufReader::new(reader).lines() {
                match line {
                    Ok(line) => log_line(&line),
                    Err(_) => break,
                }
            }
        })?;
    Ok(())
}

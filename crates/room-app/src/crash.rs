//! Minimal crash reporter (M13): panic hook → log file + stderr.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static CRASH_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Install a panic hook that writes a report under the config directory.
pub fn install() {
    let dir = crash_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = CRASH_DIR.set(dir);
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = write_report(info);
        prev(info);
    }));
}

fn crash_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Logs")
                .join("room");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join("room").join("logs");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("room")
            .join("logs");
    }
    PathBuf::from("room-logs")
}

fn write_report(info: &std::panic::PanicHookInfo<'_>) -> std::io::Result<()> {
    let dir = CRASH_DIR.get().cloned().unwrap_or_else(crash_dir);
    let _ = std::fs::create_dir_all(&dir);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("crash-{ts}.log"));
    let mut f = std::fs::File::create(&path)?;
    writeln!(f, "room crash report")?;
    writeln!(f, "version: {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(f, "os: {}", std::env::consts::OS)?;
    writeln!(f, "arch: {}", std::env::consts::ARCH)?;
    writeln!(f, "payload: {info}")?;
    if let Some(loc) = info.location() {
        writeln!(
            f,
            "location: {}:{}:{}",
            loc.file(),
            loc.line(),
            loc.column()
        )?;
    }
    eprintln!("room: crash report written to {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_dir_nonempty() {
        assert!(!crash_dir().as_os_str().is_empty());
    }
}

//! In-game PNG screenshot and GIF recording helpers.

use room_render::{encode_gif, encode_png, scale_rgba_nn};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Max GIF edge length (keeps files small / encode fast).
pub const GIF_MAX_EDGE: u32 = 640;
/// Target GIF frame interval.
pub const GIF_FRAME_INTERVAL: Duration = Duration::from_millis(100);
/// Cap recording length (~30s at 10 fps).
pub const GIF_MAX_FRAMES: usize = 300;
/// GIF frame delay in centiseconds (matches [`GIF_FRAME_INTERVAL`]).
pub const GIF_DELAY_CS: u16 = 10;

/// Active GIF capture session (RGBA frames at scaled size).
#[derive(Debug, Default)]
pub struct GifRecorder {
    /// Scaled frame width.
    pub width: u16,
    /// Scaled frame height.
    pub height: u16,
    /// RGBA frames.
    pub frames: Vec<Vec<u8>>,
    /// Last time a frame was kept.
    pub last_keep: Option<Instant>,
}

impl GifRecorder {
    /// Start empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a full-resolution RGBA frame (scaled + rate-limited). Returns true if kept.
    pub fn push_rgba(&mut self, rgba: &[u8], src_w: u32, src_h: u32) -> bool {
        if self.frames.len() >= GIF_MAX_FRAMES {
            return false;
        }
        let now = Instant::now();
        if let Some(prev) = self.last_keep {
            if now.duration_since(prev) < GIF_FRAME_INTERVAL {
                return false;
            }
        }
        let (w, h, scaled) = scale_rgba_nn(rgba, src_w, src_h, GIF_MAX_EDGE, GIF_MAX_EDGE);
        if w == 0 || h == 0 || w > u32::from(u16::MAX) || h > u32::from(u16::MAX) {
            return false;
        }
        let w = w as u16;
        let h = h as u16;
        if self.frames.is_empty() {
            self.width = w;
            self.height = h;
        } else if w != self.width || h != self.height {
            return false;
        }
        self.frames.push(scaled);
        self.last_keep = Some(now);
        true
    }

    /// Encode and clear. Returns PNG-like bytes of a GIF, or `None` if empty.
    pub fn finish(&mut self) -> Option<Vec<u8>> {
        if self.frames.is_empty() {
            return None;
        }
        let w = self.width;
        let h = self.height;
        let mut frames = std::mem::take(&mut self.frames);
        self.width = 0;
        self.height = 0;
        self.last_keep = None;
        let mut out = Vec::new();
        encode_gif(w, h, &mut frames, GIF_DELAY_CS, &mut out).ok()?;
        Some(out)
    }
}

/// `…/room/screenshots` under the platform data dir.
#[must_use]
pub fn screenshots_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("room")
                .join("screenshots");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(app) = std::env::var_os("APPDATA") {
            return PathBuf::from(app).join("room").join("screenshots");
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("room").join("screenshots");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("room")
            .join("screenshots");
    }
    PathBuf::from("screenshots")
}

/// Ensure the screenshots directory exists.
pub fn ensure_screenshots_dir() -> Result<PathBuf, String> {
    let dir = screenshots_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    Ok(dir)
}

fn stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

/// Write an RGBA buffer as PNG; returns the path.
pub fn write_png(dir: &Path, rgba: &[u8], width: u32, height: u32) -> Result<PathBuf, String> {
    let mut png = Vec::new();
    encode_png(width, height, rgba, &mut png).map_err(|e| e.to_string())?;
    let path = dir.join(format!("room-{}.png", stamp()));
    std::fs::write(&path, &png).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// Write GIF bytes; returns the path.
pub fn write_gif_bytes(dir: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let path = dir.join(format!("room-{}.gif", stamp()));
    std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_recorder_keeps_scaled_frame() {
        let mut rec = GifRecorder::new();
        let rgba = vec![255u8; 8 * 8 * 4];
        assert!(rec.push_rgba(&rgba, 8, 8));
        assert_eq!(rec.frames.len(), 1);
        let bytes = rec.finish().expect("gif");
        assert!(bytes.starts_with(b"GIF"));
    }
}

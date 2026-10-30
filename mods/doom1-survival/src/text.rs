//! Fixed HUD message buffer and print helpers.

use core::fmt::Write;

/// Fixed-size text buffer for HUD messages (no allocator).
pub struct Msg {
    buf: [u8; 128],
    len: usize,
}

impl Msg {
    /// Empty message.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buf: [0; 128],
            len: 0,
        }
    }

    /// UTF-8 view (lossy on invalid bytes → empty).
    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl Default for Msg {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for Msg {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len().saturating_sub(self.len));
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

//! Fixed HUD message buffer and print helpers.

use core::fmt::Write;
use room_script_sdk::game;

/// Fixed-size text buffer for HUD messages (no allocator in this module).
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

    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
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

/// Append the session display name for `seat` (BE3 fallback `Player N`).
pub fn write_player_name(buf: &mut Msg, seat: usize) {
    let mut tmp = [0u8; 96];
    let n = game::player_name(seat as i32, &mut tmp).max(0) as usize;
    let n = n.min(tmp.len());
    if let Ok(s) = core::str::from_utf8(&tmp[..n]) {
        let _ = buf.write_str(s);
    }
}

/// HUD message: `tell!(host, None, ..)` for everyone, `tell!(host, Some(slot), ..)` for one player.
#[macro_export]
macro_rules! tell {
    ($host:expr, $to:expr, $($arg:tt)*) => {{
        let mut m = $crate::text::Msg::new();
        let _ = core::fmt::Write::write_fmt(&mut m, format_args!($($arg)*));
        $host.print($to, m.as_str());
    }};
}

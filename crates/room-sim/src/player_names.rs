//! Session player display names (`docs/28` §9 BE3).

use crate::player::MAX_PLAYERS;
use room_script_abi::player_name::{MAX_BYTES, MAX_CHARS};

/// Fixed storage for deterministic session names (set before tic 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerNames {
    bytes: [[u8; MAX_BYTES]; MAX_PLAYERS],
    len: [u8; MAX_PLAYERS],
}

impl Default for PlayerNames {
    fn default() -> Self {
        Self {
            bytes: [[0; MAX_BYTES]; MAX_PLAYERS],
            len: [0; MAX_PLAYERS],
        }
    }
}

/// Sanitize a player-chosen display name (same rules as `room-server` lobby).
#[must_use]
pub fn sanitize_display_name(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len().min(MAX_CHARS * 4));
    for c in raw.chars() {
        if out.chars().count() >= MAX_CHARS {
            break;
        }
        if c.is_control() || is_dangerous_char(c) {
            continue;
        }
        if c.is_whitespace() {
            if out.ends_with(' ') || out.is_empty() {
                continue;
            }
            out.push(' ');
            continue;
        }
        out.push(c);
    }
    let out = out.trim().to_owned();
    if out.is_empty() { None } else { Some(out) }
}

fn is_dangerous_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'
            | '\u{200C}'
            | '\u{200D}'
            | '\u{2060}'
            | '\u{FEFF}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

impl PlayerNames {
    /// Store `raw` for `seat` (empty / invalid → clear → fallback at read time).
    pub fn set(&mut self, seat: usize, raw: &str) {
        if seat >= MAX_PLAYERS {
            return;
        }
        let Some(s) = sanitize_display_name(raw) else {
            self.len[seat] = 0;
            return;
        };
        let b = s.as_bytes();
        let n = b.len().min(MAX_BYTES);
        self.bytes[seat][..n].copy_from_slice(&b[..n]);
        self.len[seat] = u8::try_from(n).unwrap_or(u8::try_from(MAX_BYTES).unwrap_or(255));
    }

    /// UTF-8 name bytes when set; empty slice → use [`Self::write_display`].
    #[must_use]
    pub fn stored(&self, seat: usize) -> &[u8] {
        if seat >= MAX_PLAYERS {
            return &[];
        }
        let n = usize::from(self.len[seat]);
        &self.bytes[seat][..n.min(MAX_BYTES)]
    }

    /// Full display name length (stored or fallback `Player N`).
    #[must_use]
    pub fn display_len(&self, seat: usize) -> usize {
        if !self.stored(seat).is_empty() {
            return self.stored(seat).len();
        }
        fallback_len(seat)
    }

    /// Write display name into `buf` (truncates); returns full UTF-8 length.
    pub fn write_display(&self, seat: usize, buf: &mut [u8]) -> i32 {
        let full = if !self.stored(seat).is_empty() {
            let src = self.stored(seat);
            let n = src.len().min(buf.len());
            buf[..n].copy_from_slice(&src[..n]);
            src.len()
        } else {
            write_fallback(seat, buf)
        };
        i32::try_from(full).unwrap_or(i32::MAX)
    }

    /// Hash bytes for [`crate::World::checksum`].
    pub fn checksum_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(MAX_PLAYERS * (1 + MAX_BYTES));
        for seat in 0..MAX_PLAYERS {
            // Seats past the first 8 only count once named (UZ12 hash compat).
            if seat >= crate::player::LEGACY_SEATS && self.len[seat] == 0 {
                continue;
            }
            out.push(self.len[seat]);
            let n = usize::from(self.len[seat]).min(MAX_BYTES);
            out.extend_from_slice(&self.bytes[seat][..n]);
        }
        out
    }
}

fn fallback_len(seat: usize) -> usize {
    7 + decimal_len(seat.saturating_add(1))
}

fn decimal_len(mut n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut d = 0;
    while n > 0 {
        d += 1;
        n /= 10;
    }
    d
}

fn write_fallback(seat: usize, buf: &mut [u8]) -> usize {
    const PREFIX: &[u8] = b"Player ";
    let mut i = 0;
    let pl = PREFIX.len().min(buf.len());
    buf[..pl].copy_from_slice(&PREFIX[..pl]);
    i += pl;
    if i >= buf.len() {
        return fallback_len(seat);
    }
    i += write_decimal(seat.saturating_add(1), &mut buf[i..]);
    i.min(fallback_len(seat))
}

fn write_decimal(mut n: usize, buf: &mut [u8]) -> usize {
    if n == 0 {
        if !buf.is_empty() {
            buf[0] = b'0';
            return 1;
        }
        return 0;
    }
    let mut tmp = [0u8; 20];
    let mut len = 0;
    while n > 0 {
        tmp[len] = b'0' + u8::try_from(n % 10).unwrap_or(b'0');
        len += 1;
        n /= 10;
    }
    let out = len.min(buf.len());
    for j in 0..out {
        buf[j] = tmp[len - 1 - j];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_player_n() {
        let names = PlayerNames::default();
        let mut buf = [0u8; 32];
        let n = names.write_display(0, &mut buf);
        assert_eq!(n, 8);
        assert_eq!(core::str::from_utf8(&buf[..8]).unwrap(), "Player 1");
    }

    #[test]
    fn set_and_read() {
        let mut names = PlayerNames::default();
        names.set(1, "  Alice  ");
        assert_eq!(names.stored(1), b"Alice");
        let mut buf = [0u8; 32];
        assert_eq!(names.write_display(1, &mut buf), 5);
        assert_eq!(core::str::from_utf8(&buf[..5]).unwrap(), "Alice");
    }
}

//! Rate-limited broadcast lines for phase moments (`docs/28` §6.7 / BR7).

use crate::host::Host;
use crate::tables::TICRATE;

/// Minimum gap between team broadcasts (1.5 s).
pub const TEAM_GAP_TICS: i32 = (3 * TICRATE) / 2;

/// Fixed ring of pending team lines.
const CAP: usize = 8;

#[derive(Clone, Copy, Debug)]
struct Pending {
    text: [u8; 96],
    len: u8,
    enqueued_at: i32,
}

impl Pending {
    const fn empty() -> Self {
        Self {
            text: [0; 96],
            len: 0,
            enqueued_at: 0,
        }
    }

    fn set(&mut self, msg: &str, t: i32) {
        let n = msg.len().min(self.text.len());
        self.text[..n].copy_from_slice(&msg.as_bytes()[..n]);
        self.len = n as u8;
        self.enqueued_at = t;
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.text[..usize::from(self.len)]).unwrap_or("")
    }
}

/// Team broadcast queue (deterministic, fixed capacity).
#[derive(Clone, Copy, Debug)]
pub struct Announcer {
    q: [Pending; CAP],
    n: usize,
    last_team: i32,
}

impl Default for Announcer {
    fn default() -> Self {
        Self {
            q: [Pending::empty(); CAP],
            n: 0,
            last_team: -TEAM_GAP_TICS,
        }
    }
}

impl Announcer {
    /// Print a team line immediately or enqueue when inside the rate gate.
    pub fn team<H: Host>(&mut self, host: &mut H, msg: &str, t: i32) {
        if msg.is_empty() {
            return;
        }
        if t - self.last_team >= TEAM_GAP_TICS {
            host.print(None, msg);
            self.last_team = t;
            return;
        }
        if self.n < CAP {
            self.q[self.n].set(msg, t);
            self.n += 1;
        }
    }

    /// Flush queued team lines when the gate opens.
    pub fn tick<H: Host>(&mut self, host: &mut H, t: i32) {
        if self.n == 0 || t - self.last_team < TEAM_GAP_TICS {
            return;
        }
        let msg = self.q[0].as_str();
        if !msg.is_empty() {
            host.print(None, msg);
            self.last_team = t;
        }
        for i in 1..self.n {
            self.q[i - 1] = self.q[i];
        }
        self.n -= 1;
        self.q[self.n] = Pending::empty();
    }
}

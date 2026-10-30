//! Priority message / sound queue (`docs/27` §6.8).
//!
//! At most one **team** (broadcast) message every 1.5 s. Per-seat lines are not
//! rate-limited against the team gate. Lower-priority pending lines expire after
//! 4 s if never flushed.

use crate::host::Host;
use crate::tables::TICRATE;
use crate::text::Msg;
use core::fmt::Write;

/// Minimum gap between team broadcasts (1.5 s).
pub const TEAM_GAP_TICS: i32 = (3 * TICRATE) / 2;
/// Drop queued info/low lines older than this.
pub const EXPIRE_TICS: i32 = 4 * TICRATE;
/// Ring capacity.
const CAP: usize = 16;

/// Message priority (`docs/27` §6.8): 0 info … 3 critical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Priority {
    Info = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

#[derive(Clone, Copy)]
struct Pending {
    seat: Option<i32>,
    prio: Priority,
    enqueued_at: i32,
    text: [u8; 128],
    len: u8,
}

impl Pending {
    const fn empty() -> Self {
        Self {
            seat: None,
            prio: Priority::Info,
            enqueued_at: 0,
            text: [0; 128],
            len: 0,
        }
    }

}

/// Fixed ring of HUD announcements + sound cue helper.
#[derive(Clone, Copy)]
pub struct Announcer {
    q: [Pending; CAP],
    n: usize,
    last_team_print: i32,
}

impl Default for Announcer {
    fn default() -> Self {
        Self {
            q: [Pending::empty(); CAP],
            n: 0,
            last_team_print: -TEAM_GAP_TICS,
        }
    }
}

impl Announcer {
    /// Enqueue or print immediately. `seat` `None` = everyone (team).
    pub fn push(
        &mut self,
        host: &mut impl Host,
        seat: Option<i32>,
        prio: Priority,
        msg: &str,
    ) {
        let now = host.maptime();
        self.expire(now);
        if seat.is_none() && now - self.last_team_print < TEAM_GAP_TICS {
            self.enqueue(seat, prio, now, msg);
            return;
        }
        host.print(seat, msg);
        if seat.is_none() {
            self.last_team_print = now;
        }
    }

    /// Format into a [`Msg`] and [`Self::push`].
    pub fn say(
        &mut self,
        host: &mut impl Host,
        seat: Option<i32>,
        prio: Priority,
        args: core::fmt::Arguments<'_>,
    ) {
        let mut m = Msg::new();
        let _ = m.write_fmt(args);
        self.push(host, seat, prio, m.as_str());
    }

    /// Flush one pending team/seat line when the team gate allows.
    pub fn tick(&mut self, host: &mut impl Host) {
        let now = host.maptime();
        self.expire(now);
        if self.n == 0 {
            return;
        }
        // Prefer highest priority; stable by enqueue order within same prio.
        let mut best = 0usize;
        for i in 1..self.n {
            let a = &self.q[i];
            let b = &self.q[best];
            if a.prio > b.prio || (a.prio == b.prio && a.enqueued_at < b.enqueued_at) {
                best = i;
            }
        }
        let seat = self.q[best].seat;
        if seat.is_none() && now - self.last_team_print < TEAM_GAP_TICS {
            return;
        }
        let text = {
            let mut buf = [0u8; 128];
            let len = self.q[best].len as usize;
            buf[..len].copy_from_slice(&self.q[best].text[..len]);
            (buf, len, seat)
        };
        self.remove_at(best);
        let s = core::str::from_utf8(&text.0[..text.1]).unwrap_or("");
        host.print(text.2, s);
        if text.2.is_none() {
            self.last_team_print = now;
        }
    }

    /// Play a non-positional cue from `actor` when present (SNDINFO name).
    pub fn cue(host: &mut impl Host, actor: Option<room_script_sdk::game::Actor>, name: &str) {
        if let Some(a) = actor {
            host.actor_sound(a, name, 1.0);
        }
    }

    fn enqueue(&mut self, seat: Option<i32>, prio: Priority, now: i32, msg: &str) {
        if self.n == CAP {
            // Drop lowest-priority oldest.
            let mut worst = 0usize;
            for i in 1..self.n {
                let a = &self.q[i];
                let b = &self.q[worst];
                if a.prio < b.prio || (a.prio == b.prio && a.enqueued_at < b.enqueued_at) {
                    worst = i;
                }
            }
            self.remove_at(worst);
        }
        let mut p = Pending::empty();
        p.seat = seat;
        p.prio = prio;
        p.enqueued_at = now;
        let n = msg.len().min(128);
        p.text[..n].copy_from_slice(&msg.as_bytes()[..n]);
        p.len = n as u8;
        self.q[self.n] = p;
        self.n += 1;
    }

    fn expire(&mut self, now: i32) {
        let mut i = 0;
        while i < self.n {
            let p = &self.q[i];
            let stale = p.prio <= Priority::Normal && now - p.enqueued_at >= EXPIRE_TICS;
            if stale {
                self.remove_at(i);
            } else {
                i += 1;
            }
        }
    }

    fn remove_at(&mut self, i: usize) {
        if i >= self.n {
            return;
        }
        self.n -= 1;
        self.q[i] = self.q[self.n];
        self.q[self.n] = Pending::empty();
    }

    /// Test helper: queued lines.
    #[cfg(test)]
    #[must_use]
    pub fn queued(&self) -> usize {
        self.n
    }
}

/// Format + push at [`Priority::Normal`] (team).
#[macro_export]
macro_rules! say {
    ($ann:expr, $host:expr, $($arg:tt)*) => {{
        $ann.say(
            $host,
            None,
            $crate::announcer::Priority::Normal,
            format_args!($($arg)*),
        );
    }};
}

/// Format + push at a chosen priority (team).
#[macro_export]
macro_rules! say_prio {
    ($ann:expr, $host:expr, $prio:expr, $($arg:tt)*) => {{
        $ann.say($host, None, $prio, format_args!($($arg)*));
    }};
}

/// Format + push to one seat (`0`-based).
#[macro_export]
macro_rules! say_to {
    ($ann:expr, $host:expr, $seat:expr, $($arg:tt)*) => {{
        $ann.say(
            $host,
            Some($seat),
            $crate::announcer::Priority::Normal,
            format_args!($($arg)*),
        );
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::FakeHost;

    #[test]
    fn team_messages_rate_limited_to_one_per_gap() {
        let mut host = FakeHost::new(3, 1);
        let mut ann = Announcer::default();
        for i in 0..5 {
            ann.push(&mut host, None, Priority::Normal, &format!("msg{i}"));
        }
        assert_eq!(host.prints.len(), 1, "first prints immediately");
        assert_eq!(ann.queued(), 4);
        // Advance just under the gap — still blocked.
        host.maptime = TEAM_GAP_TICS - 1;
        ann.tick(&mut host);
        assert_eq!(host.prints.len(), 1);
        host.maptime = TEAM_GAP_TICS;
        ann.tick(&mut host);
        assert_eq!(host.prints.len(), 2);
        // Spacing between consecutive team prints ≥ gap.
        let mut last = -TEAM_GAP_TICS;
        for _ in 0..10 {
            host.maptime += 1;
            let before = host.prints.len();
            ann.tick(&mut host);
            if host.prints.len() > before {
                assert!(host.maptime - last >= TEAM_GAP_TICS);
                last = host.maptime;
            }
        }
    }

    #[test]
    fn higher_priority_flushes_first() {
        let mut host = FakeHost::new(3, 1);
        let mut ann = Announcer::default();
        ann.push(&mut host, None, Priority::Normal, "low");
        host.maptime = 0;
        // Gate closed — both queue after saturating the immediate slot.
        ann.push(&mut host, None, Priority::Info, "info");
        ann.push(&mut host, None, Priority::Critical, "crit");
        host.maptime = TEAM_GAP_TICS;
        ann.tick(&mut host);
        assert_eq!(host.prints.last().map(|p| p.1.as_str()), Some("crit"));
    }
}

//! Server-authoritative tic ring for protocol v2.
//!
//! Frames are released on a 35 Hz clock: tic `N` is never emitted before
//! `start + N·T`, and is emitted as soon as every seat has a command for it.
//! A seat that misses the slot by more than `grace` gets its last command
//! repeated (for a few tics, to hide jitter) and then neutral input, so one
//! slow client never freezes the others.

use room_proto::{NetTicCmd, TicCmdMsg, TicFrameMsg};
use std::time::{Duration, Instant};
use thiserror::Error;

const RING: usize = 128;
const MAX_SEATS: usize = 8;
/// One Doom tic (1/35 s).
pub const TIC: Duration = Duration::from_nanos(28_571_429);
/// Late tics a seat's last command is repeated before it goes neutral.
const REPEAT_TICS: u8 = 3;
/// Max frames released per clock call (catch-up after a long pause).
const MAX_BURST: usize = 35;

/// Tic ring errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum V2Error {
    /// Tic too far ahead.
    #[error("tic too far ahead")]
    TooFar,
    /// Bad seat index.
    #[error("bad seat")]
    BadSeat,
}

/// Per-room v2 lockstep state.
pub struct V2State {
    next_tic: u32,
    /// `cmds[tic % RING][seat]`
    ring: [[Option<NetTicCmd>; MAX_SEATS]; RING],
    /// When the first cmd for `tic % RING` arrived (relay latency start).
    first_cmd_at: [Option<Instant>; RING],
    active_mask: u8,
    max_players: u8,
    /// Clock origin for tic 0.
    start_at: Instant,
    /// Late-seat tolerance past a tic's slot.
    grace: Duration,
    /// Last command released per seat (repeated while a seat is briefly late).
    last_cmd: [NetTicCmd; MAX_SEATS],
    /// Consecutive tics each seat was filled by the server.
    miss_streak: [u8; MAX_SEATS],
    /// Micros from first cmd → frame for the last completed tic.
    pub last_relay_us: Option<u64>,
}

impl V2State {
    /// Empty state (active mask filled on start).
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_tic: 0,
            ring: [[None; MAX_SEATS]; RING],
            first_cmd_at: [None; RING],
            active_mask: 0,
            max_players: 2,
            start_at: Instant::now(),
            grace: Duration::from_millis(30),
            last_cmd: [NetTicCmd::default(); MAX_SEATS],
            miss_streak: [0; MAX_SEATS],
            last_relay_us: None,
        }
    }

    /// Reset when the match starts; tic 0 is due now.
    pub fn reset_for_start(&mut self) {
        self.reset_for_start_at(Instant::now());
    }

    /// Reset with an explicit clock origin (tests).
    pub fn reset_for_start_at(&mut self, start_at: Instant) {
        self.next_tic = 0;
        self.ring = [[None; MAX_SEATS]; RING];
        self.first_cmd_at = [None; RING];
        self.start_at = start_at;
        self.last_cmd = [NetTicCmd::default(); MAX_SEATS];
        self.miss_streak = [0; MAX_SEATS];
        self.last_relay_us = None;
    }

    /// Late-seat tolerance (`--stall-fill-ms`).
    pub fn set_grace(&mut self, grace: Duration) {
        self.grace = grace;
    }

    /// Set which seats are active (bitmask).
    pub fn set_active_seats(&mut self, max_players: u8) {
        self.max_players = max_players.clamp(2, 8);
        self.active_mask = (1u8 << self.max_players).wrapping_sub(1);
    }

    fn slot_at(&self, tic: u32) -> Instant {
        self.start_at + TIC * tic
    }

    fn seat_active(&self, s: u8) -> bool {
        (self.active_mask & (1 << s)) != 0
    }

    fn all_present(&self, tic: u32) -> bool {
        let slot = (tic as usize) % RING;
        (0..self.max_players)
            .filter(|&s| self.seat_active(s))
            .all(|s| self.ring[slot][usize::from(s)].is_some())
    }

    /// Push a command; returns a frame when `next_tic` is complete and due.
    ///
    /// # Errors
    ///
    /// Tic window / seat validation.
    pub fn push_cmd(&mut self, seat: u8, msg: TicCmdMsg) -> Result<Option<TicFrameMsg>, V2Error> {
        self.push_cmd_at(seat, msg, Instant::now())
    }

    /// [`Self::push_cmd`] with an explicit `now` (tests).
    ///
    /// # Errors
    ///
    /// Tic window / seat validation.
    pub fn push_cmd_at(
        &mut self,
        seat: u8,
        msg: TicCmdMsg,
        now: Instant,
    ) -> Result<Option<TicFrameMsg>, V2Error> {
        if seat >= self.max_players {
            return Err(V2Error::BadSeat);
        }
        if self.active_mask == 0 {
            self.set_active_seats(self.max_players);
        }
        let tic = msg.tic;
        if tic < self.next_tic {
            // Late (already released) or stale resend — ignore.
            return Ok(None);
        }
        if tic > self.next_tic.saturating_add(70) {
            return Err(V2Error::TooFar);
        }
        let slot = (tic as usize) % RING;
        let cell = &mut self.ring[slot][usize::from(seat)];
        if cell.is_some() {
            // First-cmd-wins.
            return Ok(None);
        }
        if self.first_cmd_at[slot].is_none() {
            self.first_cmd_at[slot] = Some(now);
        }
        *cell = Some(msg.cmd);
        Ok(self.try_complete_at(now))
    }

    /// Release `next_tic` if every seat has it and its slot is due.
    pub fn try_complete(&mut self) -> Option<TicFrameMsg> {
        self.try_complete_at(Instant::now())
    }

    /// [`Self::try_complete`] with an explicit `now`.
    pub fn try_complete_at(&mut self, now: Instant) -> Option<TicFrameMsg> {
        if self.active_mask == 0 || now < self.slot_at(self.next_tic) {
            return None;
        }
        if !self.all_present(self.next_tic) {
            return None;
        }
        Some(self.release(now))
    }

    /// Pop `next_tic`, filling missing seats (repeat last briefly, then neutral).
    fn release(&mut self, now: Instant) -> TicFrameMsg {
        let tic = self.next_tic;
        let slot = (tic as usize) % RING;
        let mut cmds = Vec::new();
        for s in 0..self.max_players {
            if !self.seat_active(s) {
                continue;
            }
            let i = usize::from(s);
            let cmd = if let Some(c) = self.ring[slot][i].take() {
                self.miss_streak[i] = 0;
                c
            } else {
                self.miss_streak[i] = self.miss_streak[i].saturating_add(1);
                if self.miss_streak[i] <= REPEAT_TICS {
                    NetTicCmd {
                        weapon_select: 0,
                        ..self.last_cmd[i]
                    }
                } else {
                    NetTicCmd::default()
                }
            };
            self.last_cmd[i] = cmd;
            cmds.push(cmd);
        }
        self.last_relay_us = self.first_cmd_at[slot]
            .map(|t| u64::try_from(now.duration_since(t).as_micros()).unwrap_or(u64::MAX));
        self.first_cmd_at[slot] = None;
        self.ring[slot] = [None; MAX_SEATS];
        self.next_tic = self.next_tic.saturating_add(1);
        TicFrameMsg {
            tic,
            seat_mask: self.active_mask,
            cmds,
        }
    }

    /// Release every frame that is due at `now` (complete, or past grace).
    pub fn clock_emit(&mut self, now: Instant) -> Vec<TicFrameMsg> {
        let mut out = Vec::new();
        if self.active_mask == 0 {
            return out;
        }
        while out.len() < MAX_BURST {
            let due = self.slot_at(self.next_tic);
            if now < due {
                break;
            }
            if self.all_present(self.next_tic) || now >= due + self.grace {
                out.push(self.release(now));
            } else {
                break;
            }
        }
        out
    }

    /// When the room actor should next call [`Self::clock_emit`].
    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        if self.active_mask == 0 {
            return None;
        }
        let due = self.slot_at(self.next_tic);
        Some(if self.all_present(self.next_tic) {
            due
        } else {
            due + self.grace
        })
    }

    /// Next tic awaiting release.
    #[must_use]
    #[allow(dead_code)]
    pub fn next_tic(&self) -> u32 {
        self.next_tic
    }
}

impl Default for V2State {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn cmd(tic: u32, fwd: i16) -> TicCmdMsg {
        TicCmdMsg {
            tic,
            cmd: NetTicCmd {
                forward_move: fwd,
                ..NetTicCmd::default()
            },
        }
    }

    #[test]
    fn two_seats_advance() {
        let t0 = Instant::now();
        let mut s = V2State::new();
        s.set_active_seats(2);
        s.reset_for_start_at(t0);
        assert!(s.push_cmd_at(0, cmd(0, 1), t0).unwrap().is_none());
        let frame = s.push_cmd_at(1, cmd(0, 0), t0).unwrap().unwrap();
        assert_eq!(frame.tic, 0);
        assert_eq!(frame.cmds[0].forward_move, 1);
        assert_eq!(s.next_tic(), 1);
        assert!(s.last_relay_us.is_some());
    }

    #[test]
    fn complete_tic_waits_for_its_slot() {
        let t0 = Instant::now();
        let mut s = V2State::new();
        s.set_active_seats(2);
        s.reset_for_start_at(t0);
        // Tic 3 arrives early from both seats — must not jump the 35 Hz clock.
        for t in 0..4 {
            let _ = s.push_cmd_at(0, cmd(t, 0), t0);
            let _ = s.push_cmd_at(1, cmd(t, 0), t0);
        }
        assert_eq!(s.next_tic(), 1, "only tic 0 is due at t0");
        let frames = s.clock_emit(t0 + TIC * 3);
        assert_eq!(
            frames.iter().map(|f| f.tic).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn late_seat_repeats_then_goes_neutral() {
        let t0 = Instant::now();
        let mut s = V2State::new();
        s.set_active_seats(2);
        s.set_grace(Duration::from_millis(30));
        s.reset_for_start_at(t0);
        let _ = s.push_cmd_at(0, cmd(0, 50), t0);
        let _ = s.push_cmd_at(1, cmd(0, 25), t0);
        // Seat 1 goes silent; seat 0 keeps sending.
        let mut seat1 = Vec::new();
        for t in 1..=5u32 {
            let _ = s.push_cmd_at(0, cmd(t, 50), t0);
            let now = t0 + TIC * t + Duration::from_millis(31);
            for f in s.clock_emit(now) {
                seat1.push(f.cmds[1].forward_move);
            }
        }
        assert_eq!(seat1, vec![25, 25, 25, 0, 0]);
    }

    #[test]
    fn missing_seat_waits_only_grace() {
        let t0 = Instant::now();
        let mut s = V2State::new();
        s.set_active_seats(2);
        s.set_grace(Duration::from_millis(30));
        s.reset_for_start_at(t0);
        let _ = s.push_cmd_at(0, cmd(0, 1), t0);
        assert!(s.clock_emit(t0 + Duration::from_millis(29)).is_empty());
        assert_eq!(s.clock_emit(t0 + Duration::from_millis(30)).len(), 1);
    }
}

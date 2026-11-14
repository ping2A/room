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
/// Seats a room starts with storage for (grown when a room allows more).
const BASE_SEATS: usize = 8;
/// Most seats a room may have (protocol v3, `room_sim::MAX_PLAYERS`).
pub const MAX_SEATS: usize = 64;
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
    /// `cmds[(tic % RING) * seats + seat]`.
    ring: Vec<Option<NetTicCmd>>,
    /// Seats [`Self::ring`] has room for.
    seats: usize,
    /// When the first cmd for `tic % RING` arrived (relay latency start).
    first_cmd_at: [Option<Instant>; RING],
    active_mask: u64,
    /// Bot seats (`fill_bots`): never waited for, relayed as neutral cmds.
    bot_mask: u64,
    /// Seats whose player left mid-match: not waited for either (each tic
    /// used to wait the full grace for them, adding lag for everyone).
    absent_mask: u64,
    max_players: u8,
    /// Clock origin for tic 0.
    start_at: Instant,
    /// Late-seat tolerance past a tic's slot.
    grace: Duration,
    /// Last command released per seat (repeated while a seat is briefly late).
    last_cmd: Vec<NetTicCmd>,
    /// Consecutive tics each seat was filled by the server.
    miss_streak: Vec<u8>,
    /// Micros from first cmd → frame for the last completed tic.
    pub last_relay_us: Option<u64>,
}

impl V2State {
    /// Empty state (active mask filled on start).
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_tic: 0,
            ring: vec![None; RING * BASE_SEATS],
            seats: BASE_SEATS,
            first_cmd_at: [None; RING],
            active_mask: 0,
            bot_mask: 0,
            absent_mask: 0,
            max_players: 2,
            start_at: Instant::now(),
            grace: Duration::from_millis(30),
            last_cmd: vec![NetTicCmd::default(); BASE_SEATS],
            miss_streak: vec![0; BASE_SEATS],
            last_relay_us: None,
        }
    }

    fn cell(&mut self, slot: usize, seat: usize) -> &mut Option<NetTicCmd> {
        &mut self.ring[slot * self.seats + seat]
    }

    fn cell_ref(&self, slot: usize, seat: usize) -> Option<&NetTicCmd> {
        self.ring[slot * self.seats + seat].as_ref()
    }

    /// Reset when the match starts; tic 0 is due now.
    pub fn reset_for_start(&mut self) {
        self.reset_for_start_at(Instant::now());
    }

    /// Reset with an explicit clock origin (tests).
    pub fn reset_for_start_at(&mut self, start_at: Instant) {
        self.next_tic = 0;
        self.ring = vec![None; RING * self.seats];
        self.first_cmd_at = [None; RING];
        self.start_at = start_at;
        self.last_cmd = vec![NetTicCmd::default(); self.seats];
        self.miss_streak = vec![0; self.seats];
        self.last_relay_us = None;
        self.absent_mask = 0;
    }

    /// Late-seat tolerance (`--stall-fill-ms`).
    pub fn set_grace(&mut self, grace: Duration) {
        self.grace = grace;
    }

    /// Set which seats are active (bitmask).
    pub fn set_active_seats(&mut self, max_players: u8) {
        self.max_players = max_players.clamp(2, MAX_SEATS as u8);
        self.active_mask = if usize::from(self.max_players) >= 64 {
            u64::MAX
        } else {
            (1u64 << self.max_players) - 1
        };
        let want = usize::from(self.max_players).max(BASE_SEATS);
        if want != self.seats {
            // Re-lay the ring for the new seat count (only before play).
            let old = std::mem::take(&mut self.ring);
            let old_seats = self.seats;
            self.ring = vec![None; RING * want];
            for slot in 0..RING {
                for seat in 0..old_seats.min(want) {
                    self.ring[slot * want + seat] = old[slot * old_seats + seat];
                }
            }
            self.seats = want;
            self.last_cmd.resize(want, NetTicCmd::default());
            self.miss_streak.resize(want, 0);
        }
    }

    fn slot_at(&self, tic: u32) -> Instant {
        self.start_at + TIC * tic
    }

    fn seat_active(&self, s: u8) -> bool {
        u32::from(s) < 64 && (self.active_mask & (1u64 << s)) != 0
    }

    fn all_present(&self, tic: u32) -> bool {
        let slot = (tic as usize) % RING;
        (0..self.max_players)
            .filter(|&s| self.seat_active(s) && !self.seat_bot(s))
            .all(|s| self.cell_ref(slot, usize::from(s)).is_some())
    }

    fn seat_bot(&self, s: u8) -> bool {
        u32::from(s) < 64 && ((self.bot_mask | self.absent_mask) & (1u64 << s)) != 0
    }

    /// A seat's player left (`true`) or a player took the seat again.
    pub fn set_seat_absent(&mut self, seat: u8, absent: bool) {
        if u32::from(seat) >= 64 {
            return;
        }
        if absent {
            self.absent_mask |= 1u64 << seat;
        } else {
            self.absent_mask &= !(1u64 << seat);
        }
    }

    /// Seats `first..max_players` are sim bots (`fill_bots`): nobody sends
    /// their commands, so a tic must not wait the stall grace for them. They
    /// relay as neutral commands; every client's sim drives them.
    pub fn set_bot_seats(&mut self, first: u8) {
        self.bot_mask = 0;
        for s in first..self.max_players.min(64) {
            self.bot_mask |= 1u64 << s;
        }
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
        if self.cell_ref(slot, usize::from(seat)).is_some() {
            // First-cmd-wins.
            return Ok(None);
        }
        if self.first_cmd_at[slot].is_none() {
            self.first_cmd_at[slot] = Some(now);
        }
        *self.cell(slot, usize::from(seat)) = Some(msg.cmd);
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
            let cmd = if self.seat_bot(s) {
                NetTicCmd::default()
            } else if let Some(c) = self.cell(slot, i).take() {
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
        for seat in 0..self.seats {
            *self.cell(slot, seat) = None;
        }
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
    fn sixty_four_seats_release_a_full_wide_frame() {
        let t0 = Instant::now();
        let mut s = V2State::new();
        s.set_active_seats(64);
        s.reset_for_start_at(t0);
        let mut frame = None;
        for seat in 0..64u8 {
            frame = s.push_cmd_at(seat, cmd(0, i16::from(seat)), t0).unwrap();
            if seat < 63 {
                assert!(frame.is_none(), "waits for seat 63");
            }
        }
        let frame = frame.unwrap();
        assert_eq!(frame.seat_mask, u64::MAX);
        assert_eq!(frame.cmds.len(), 64);
        assert_eq!(frame.cmds[63].forward_move, 63);
        assert!(room_proto::tic_frame_is_wide(&frame));
        assert_eq!(s.push_cmd_at(64, cmd(1, 0), t0), Err(V2Error::BadSeat));
        // Eight seats still make v2 frames.
        let mut s8 = V2State::new();
        s8.set_active_seats(8);
        s8.reset_for_start_at(t0);
        let mut f8 = None;
        for seat in 0..8u8 {
            f8 = s8.push_cmd_at(seat, cmd(0, 0), t0).unwrap();
        }
        assert!(!room_proto::tic_frame_is_wide(&f8.unwrap()));
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
    fn bot_seats_do_not_hold_the_tic() {
        let t0 = Instant::now();
        let mut s = V2State::new();
        s.set_active_seats(8);
        s.set_grace(Duration::from_millis(30));
        s.reset_for_start_at(t0);
        s.set_bot_seats(1);
        // Only the human seat 0 sends: the tic releases at once, not after the grace.
        let frame = s.push_cmd_at(0, cmd(0, 5), t0).unwrap().unwrap();
        assert_eq!(frame.cmds.len(), 8);
        assert_eq!(frame.cmds[0].forward_move, 5);
        assert!(frame.cmds[1..].iter().all(|c| c.forward_move == 0));
    }

    #[test]
    fn a_seat_that_left_is_not_waited_for() {
        let t0 = Instant::now();
        let mut s = V2State::new();
        s.set_active_seats(3);
        s.set_grace(Duration::from_millis(30));
        s.reset_for_start_at(t0);
        s.set_seat_absent(2, true);
        let _ = s.push_cmd_at(0, cmd(0, 1), t0).unwrap();
        let frame = s.push_cmd_at(1, cmd(0, 2), t0).unwrap().unwrap();
        assert_eq!(frame.cmds.len(), 3);
        assert_eq!(frame.cmds[2].forward_move, 0);
        // Back in the seat: waited for again.
        s.set_seat_absent(2, false);
        let _ = s.push_cmd_at(0, cmd(1, 1), t0 + TIC).unwrap();
        assert!(s.push_cmd_at(1, cmd(1, 2), t0 + TIC).unwrap().is_none());
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

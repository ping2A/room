//! Native Duke 30 Hz lockstep input relay for protocol v2.

use room_proto::{DukeSyncCmdMsg, DukeSyncFrameMsg, DukeSyncInput};
use std::time::{Duration, Instant};
use thiserror::Error;

const RING: usize = 128;
const MAX_SEATS: usize = 8;
const REPEAT_TICS: u8 = 3;
/// One native Duke tic (30 Hz).
pub const TIC: Duration = Duration::from_nanos(33_333_333);
const MAX_BURST: usize = 30;

/// Duke input-ring validation errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DukeV2Error {
    /// Input seat is not part of the current room roster.
    #[error("bad Duke seat")]
    BadSeat,
    /// Input is outside the bounded tic window.
    #[error("Duke tic too far ahead")]
    TooFar,
    /// Input is a duplicate for its seat and tic.
    #[error("duplicate Duke command")]
    Duplicate,
}

/// Server-authoritative 30 Hz ring for native Duke sync inputs.
pub struct DukeV2State {
    next_tic: u32,
    ring: [[Option<DukeSyncInput>; MAX_SEATS]; RING],
    active_mask: u8,
    max_players: u8,
    last_cmd: [DukeSyncInput; MAX_SEATS],
    miss_streak: [u8; MAX_SEATS],
    start_at: Instant,
    grace: Duration,
}

impl DukeV2State {
    /// Empty ring, configured when the room starts.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_tic: 0,
            ring: [[None; MAX_SEATS]; RING],
            active_mask: 0,
            max_players: 2,
            last_cmd: [DukeSyncInput::default(); MAX_SEATS],
            miss_streak: [0; MAX_SEATS],
            start_at: Instant::now(),
            grace: Duration::from_millis(30),
        }
    }

    /// Configure contiguous active seats 0..max_players.
    pub fn set_active_seats(&mut self, max_players: u8) {
        self.max_players = max_players.clamp(2, MAX_SEATS as u8);
        self.active_mask = (1u8 << self.max_players).wrapping_sub(1);
    }

    /// Configure the late-input grace period.
    pub fn set_grace(&mut self, grace: Duration) {
        self.grace = grace;
    }

    /// Reset the ring when a Duke match starts.
    pub fn reset_for_start(&mut self) {
        self.next_tic = 0;
        self.ring = [[None; MAX_SEATS]; RING];
        self.last_cmd = [DukeSyncInput::default(); MAX_SEATS];
        self.miss_streak = [0; MAX_SEATS];
        self.start_at = Instant::now();
    }

    fn slot_at(&self, tic: u32) -> Instant {
        self.start_at + TIC * tic
    }

    fn all_present(&self, tic: u32) -> bool {
        let slot = tic as usize % RING;
        (0..self.max_players).all(|seat| {
            self.active_mask & (1 << seat) == 0 || self.ring[slot][usize::from(seat)].is_some()
        })
    }

    /// Submit one native command; emits a frame only when it is due and complete.
    pub fn push_cmd_at(
        &mut self,
        seat: u8,
        msg: DukeSyncCmdMsg,
        now: Instant,
    ) -> Result<Option<DukeSyncFrameMsg>, DukeV2Error> {
        if seat >= self.max_players || self.active_mask & (1 << seat) == 0 {
            return Err(DukeV2Error::BadSeat);
        }
        if msg.tic < self.next_tic {
            return Ok(None);
        }
        if msg.tic > self.next_tic.saturating_add(70) {
            return Err(DukeV2Error::TooFar);
        }
        let slot = msg.tic as usize % RING;
        let cell = &mut self.ring[slot][usize::from(seat)];
        if cell.is_some() {
            return Err(DukeV2Error::Duplicate);
        }
        *cell = Some(msg.cmd);
        if msg.tic == self.next_tic && now >= self.slot_at(msg.tic) && self.all_present(msg.tic) {
            Ok(Some(self.release()))
        } else {
            Ok(None)
        }
    }

    /// Release due frames, filling missing seats with neutral commands after grace.
    pub fn clock_emit(&mut self, now: Instant) -> Vec<DukeSyncFrameMsg> {
        let mut frames = Vec::new();
        while frames.len() < MAX_BURST && self.active_mask != 0 {
            let due = self.slot_at(self.next_tic);
            if now < due || (!self.all_present(self.next_tic) && now < due + self.grace) {
                break;
            }
            frames.push(self.release());
        }
        frames
    }

    /// Next deadline for the room actor.
    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        (self.active_mask != 0).then(|| {
            let due = self.slot_at(self.next_tic);
            if self.all_present(self.next_tic) {
                due
            } else {
                due + self.grace
            }
        })
    }

    fn release(&mut self) -> DukeSyncFrameMsg {
        let tic = self.next_tic;
        let slot = tic as usize % RING;
        let mut cmds = Vec::with_capacity(self.active_mask.count_ones() as usize);
        for seat in 0..self.max_players {
            if self.active_mask & (1 << seat) == 0 {
                continue;
            }
            let seat_index = usize::from(seat);
            let cmd = if let Some(cmd) = self.ring[slot][seat_index].take() {
                self.miss_streak[seat_index] = 0;
                cmd
            } else {
                self.miss_streak[seat_index] = self.miss_streak[seat_index].saturating_add(1);
                if self.miss_streak[seat_index] <= REPEAT_TICS {
                    let mut repeated = self.last_cmd[seat_index];
                    repeated.bits &= !(0x0f << 8); // clear one-shot weapon selection
                    repeated
                } else {
                    DukeSyncInput::default()
                }
            };
            self.last_cmd[seat_index] = cmd;
            cmds.push(cmd);
        }
        self.ring[slot] = [None; MAX_SEATS];
        self.next_tic = self.next_tic.saturating_add(1);
        DukeSyncFrameMsg {
            tic,
            seat_mask: self.active_mask,
            cmds,
        }
    }
}

impl Default for DukeV2State {
    fn default() -> Self {
        Self::new()
    }
}

//! Spectator support: compact log of every released TIC_FRAME so a watcher
//! joining mid-game can replay the match from tic 0.

use room_proto::{FTR2_MAGIC, MAX_TIC_BATCH_FRAMES, MAX_TIC_BATCH_PAYLOAD, MSG_TIC_BATCH};

/// Hard cap on logged frame bytes per room (~5 h of 2-player play).
pub const MAX_FRAME_LOG_BYTES: usize = 48 << 20;

/// Max concurrent spectators per room.
pub const MAX_SPECTATORS: usize = 16;

/// Encoded TIC_FRAME bodies (no FTR2 header), contiguous.
#[derive(Default)]
pub struct FrameLog {
    bodies: Vec<u8>,
    offsets: Vec<u32>,
    full: bool,
    cap: usize,
}

impl FrameLog {
    /// Empty log with the default byte cap.
    #[must_use]
    pub fn new() -> Self {
        Self::with_cap(MAX_FRAME_LOG_BYTES)
    }

    /// Empty log with a custom byte cap.
    #[must_use]
    pub fn with_cap(cap: usize) -> Self {
        Self {
            cap,
            ..Self::default()
        }
    }

    /// Append one frame body; once full the log freezes (returns `false`).
    pub fn push(&mut self, body: &[u8]) -> bool {
        if self.full {
            return false;
        }
        if self.bodies.len() + body.len() > self.cap {
            self.full = true;
            return false;
        }
        let Ok(off) = u32::try_from(self.bodies.len()) else {
            self.full = true;
            return false;
        };
        self.offsets.push(off);
        self.bodies.extend_from_slice(body);
        true
    }

    /// Frames logged.
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Log stopped recording (spectators can no longer catch up).
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.full
    }

    fn frame(&self, i: usize) -> &[u8] {
        let start = self.offsets[i] as usize;
        let end = self
            .offsets
            .get(i + 1)
            .map_or(self.bodies.len(), |&o| o as usize);
        &self.bodies[start..end]
    }

    /// Encode a full TIC_BATCH packet starting at frame `from`.
    /// Returns `None` when `from` is already at the end.
    #[must_use]
    pub fn batch(&self, from: usize) -> Option<(Vec<u8>, usize)> {
        if from >= self.len() {
            return None;
        }
        let mut out = Vec::with_capacity(4096);
        out.extend_from_slice(FTR2_MAGIC);
        out.push(MSG_TIC_BATCH);
        out.extend_from_slice(&[0, 0]);
        let mut i = from;
        let mut n: u16 = 0;
        while i < self.len() && usize::from(n) < MAX_TIC_BATCH_FRAMES {
            let f = self.frame(i);
            // Payload = everything after the 5-byte FTR2 header.
            if out.len() - 5 + f.len() > MAX_TIC_BATCH_PAYLOAD {
                break;
            }
            out.extend_from_slice(f);
            n += 1;
            i += 1;
        }
        out[5..7].copy_from_slice(&n.to_le_bytes());
        Some((out, i))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use room_proto::{BinaryMsg, NetTicCmd, TicFrameMsg, encode_tic_frame, parse_binary};

    fn frame(tic: u32) -> TicFrameMsg {
        TicFrameMsg {
            tic,
            seat_mask: 0b11,
            cmds: vec![
                NetTicCmd {
                    forward_move: tic as i16,
                    ..NetTicCmd::default()
                },
                NetTicCmd::default(),
            ],
        }
    }

    #[test]
    fn batches_replay_every_frame_in_order() {
        let mut log = FrameLog::new();
        for t in 0..1000 {
            assert!(log.push(&encode_tic_frame(&frame(t))));
        }
        let mut cursor = 0;
        let mut seen = Vec::new();
        while let Some((pkt, next)) = log.batch(cursor) {
            let BinaryMsg::TicBatch(frames) = parse_binary(&pkt).unwrap() else {
                panic!("not a batch");
            };
            assert!(frames.len() <= MAX_TIC_BATCH_FRAMES);
            seen.extend(frames.into_iter().map(|f| f.tic));
            cursor = next;
        }
        assert_eq!(seen, (0..1000).collect::<Vec<_>>());
        assert!(log.batch(cursor).is_none());
    }

    #[test]
    fn freezes_at_cap() {
        let body = encode_tic_frame(&frame(0));
        let mut log = FrameLog::with_cap(body.len() * 3);
        assert!(log.push(&body));
        assert!(log.push(&body));
        assert!(log.push(&body));
        assert!(!log.push(&body));
        assert!(log.is_full());
        assert_eq!(log.len(), 3);
    }
}

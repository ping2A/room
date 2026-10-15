//! WebRTC SFU library for Room (`docs/22` §8).
//!
//! Signalling + mute roster always on; enable feature `rtp` for the str0m UDP
//! demux skeleton (ADR 0003). Feature `builtin-turn` enables the Lightcall
//! STUN/TURN servers for browser ICE.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod budget;
mod stun;
mod turn;

#[cfg(feature = "builtin-turn")]
mod turn_relay;

#[cfg(feature = "rtp")]
mod rtp;

pub use budget::{
    CAMERA_MIDDLE_KBPS, EIGHT_SEAT_MIDDLE_BUDGET_KBPS, VOICE_CAMERA_MIDDLE_KBPS, VOICE_KBPS,
    room_sfu_egress_kbps, subscriber_egress_kbps,
};
pub use stun::run as run_stun;
pub use turn::{IceServer, ice_servers, ice_servers_long_term, mint_turn_credential};

#[cfg(feature = "builtin-turn")]
pub use turn_relay::{TurnRelayConfig, issue_turn_user, revoke_turn_user, run_turn_relay};

#[cfg(feature = "rtp")]
pub use rtp::{UdpSfu, should_forward};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// SFU errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SfuError {
    /// Unknown seat.
    #[error("unknown seat {0}")]
    UnknownSeat(u8),
    /// Room at publisher capacity.
    #[error("too many publishers")]
    Full,
}

/// Inbound signalling from a game WebSocket (`rtc` control).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignalIn {
    /// Client SDP offer.
    Offer {
        /// Seat.
        seat: u8,
        /// SDP body.
        sdp: String,
    },
    /// Client SDP answer (polite peer responding to SFU offer).
    Answer {
        /// Seat.
        seat: u8,
        /// SDP body.
        sdp: String,
    },
    /// Trickle ICE candidate.
    Ice {
        /// Seat.
        seat: u8,
        /// Candidate string.
        candidate: String,
        /// Optional mid.
        #[serde(default)]
        mid: Option<String>,
    },
    /// Client reports a remote mid → seat mapping request.
    Track {
        /// Local seat publishing.
        seat: u8,
        /// Mid.
        mid: String,
    },
}

/// Outbound signalling to a seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignalOut {
    /// SFU answer to a client offer.
    Answer {
        /// Target seat.
        seat: u8,
        /// SDP.
        sdp: String,
    },
    /// SFU-initiated offer (new remote track).
    Offer {
        /// Target seat.
        seat: u8,
        /// SDP.
        sdp: String,
    },
    /// Trickle ICE.
    Ice {
        /// Target seat.
        seat: u8,
        /// Candidate.
        candidate: String,
        /// Mid.
        #[serde(default)]
        mid: Option<String>,
    },
    /// Map mid → publishing seat for UI tiles.
    Track {
        /// Viewing seat (recipient).
        seat: u8,
        /// Mid.
        mid: String,
        /// Publisher seat.
        from: u8,
    },
}

/// Per-seat media flags.
#[derive(Clone, Debug, Default)]
pub struct SeatMedia {
    /// Publishing audio.
    pub audio: bool,
    /// Publishing video.
    pub video: bool,
    /// Server mute audio (do not forward).
    pub muted_audio: bool,
    /// Server mute video.
    pub muted_video: bool,
    /// Last offer SDP from this seat (replayed when peers join later).
    pub last_offer: Option<String>,
    /// Last answer/offer SDP seen (debug / inspect).
    pub last_sdp: Option<String>,
}

/// One room's media session (in-process with `room-server`).
pub struct MediaSession {
    room_id: String,
    seats: BTreeMap<u8, SeatMedia>,
    max_publishers: u8,
}

impl MediaSession {
    /// Empty session for `room_id`.
    #[must_use]
    pub fn new(room_id: impl Into<String>) -> Self {
        Self {
            room_id: room_id.into(),
            seats: BTreeMap::new(),
            max_publishers: 8,
        }
    }

    /// Room id.
    #[must_use]
    pub fn room_id(&self) -> &str {
        &self.room_id
    }

    /// Ensure a seat row exists.
    pub fn ensure_seat(&mut self, seat: u8) {
        self.seats.entry(seat).or_default();
    }

    /// Seat just welcomed: deliver any offers already published by peers.
    ///
    /// Clients often create an offer immediately on welcome; late joiners would
    /// otherwise never see it (mesh relay until real SFU SDPs exist).
    #[must_use]
    pub fn seat_joined(&mut self, seat: u8) -> Vec<SignalOut> {
        self.ensure_seat(seat);
        let mut out = Vec::new();
        for (&other, row) in &self.seats {
            if other == seat {
                continue;
            }
            if let Some(sdp) = row.last_offer.clone() {
                out.push(SignalOut::Offer { seat, sdp });
            }
        }
        out
    }

    /// Apply server mute (SFU stops forwarding when RTP is live).
    pub fn set_mute(&mut self, seat: u8, audio: bool, video: bool) -> Result<(), SfuError> {
        let row = self
            .seats
            .get_mut(&seat)
            .ok_or(SfuError::UnknownSeat(seat))?;
        row.muted_audio = audio;
        row.muted_video = video;
        Ok(())
    }

    /// Handle inbound signalling; returns messages to send (possibly to other seats).
    ///
    /// # Errors
    ///
    /// Capacity / unknown seat.
    pub fn handle(&mut self, msg: SignalIn) -> Result<Vec<SignalOut>, SfuError> {
        match msg {
            SignalIn::Offer { seat, sdp } => {
                self.ensure_seat(seat);
                let pubs = self.seats.values().filter(|s| s.audio || s.video).count();
                if pubs >= usize::from(self.max_publishers)
                    && !self.seats.get(&seat).is_some_and(|s| s.audio || s.video)
                {
                    return Err(SfuError::Full);
                }
                if let Some(row) = self.seats.get_mut(&seat) {
                    row.audio = true;
                    row.last_offer = Some(sdp.clone());
                    row.last_sdp = Some(sdp.clone());
                }
                // Mesh relay until str0m produces real SFU SDPs: forward the
                // offer to every other seat (browser perfect-negotiation).
                let mut out = Vec::new();
                for &other in self.seats.keys() {
                    if other == seat {
                        continue;
                    }
                    out.push(SignalOut::Offer {
                        seat: other,
                        sdp: sdp.clone(),
                    });
                }
                Ok(out)
            }
            SignalIn::Answer { seat, sdp } => {
                self.ensure_seat(seat);
                if let Some(row) = self.seats.get_mut(&seat) {
                    row.last_sdp = Some(sdp.clone());
                }
                let mut out = Vec::new();
                for &other in self.seats.keys() {
                    if other == seat {
                        continue;
                    }
                    out.push(SignalOut::Answer {
                        seat: other,
                        sdp: sdp.clone(),
                    });
                }
                Ok(out)
            }
            SignalIn::Ice {
                seat,
                candidate,
                mid,
            } => {
                self.ensure_seat(seat);
                // Fan-out ICE to other seats (mesh-like until selective forwarding).
                let mut out = Vec::new();
                for &other in self.seats.keys() {
                    if other == seat {
                        continue;
                    }
                    out.push(SignalOut::Ice {
                        seat: other,
                        candidate: candidate.clone(),
                        mid: mid.clone(),
                    });
                }
                Ok(out)
            }
            SignalIn::Track { seat, mid } => {
                self.ensure_seat(seat);
                let mut out = Vec::new();
                for &other in self.seats.keys() {
                    if other == seat {
                        continue;
                    }
                    out.push(SignalOut::Track {
                        seat: other,
                        mid: mid.clone(),
                        from: seat,
                    });
                }
                Ok(out)
            }
        }
    }

    /// Whether audio from `seat` should be forwarded.
    #[must_use]
    pub fn forward_audio(&self, seat: u8) -> bool {
        self.seats
            .get(&seat)
            .is_some_and(|s| s.audio && !s.muted_audio)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn offer_forwards_to_peers_not_stub() {
        let mut s = MediaSession::new("r1");
        s.ensure_seat(1);
        let out = s
            .handle(SignalIn::Offer {
                seat: 0,
                sdp: "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n".into(),
            })
            .unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            SignalOut::Offer { seat: 1, sdp } => {
                assert!(sdp.contains("t=0 0"), "must forward real SDP, got {sdp:?}");
                assert!(!sdp.contains("room-sfu-stub"));
            }
            other => panic!("expected Offer to peer, got {other:?}"),
        }
        assert!(s.forward_audio(0));
        s.set_mute(0, true, false).unwrap();
        assert!(!s.forward_audio(0));
    }

    #[test]
    fn answer_forwards_to_peers() {
        let mut s = MediaSession::new("r1");
        s.ensure_seat(0);
        s.ensure_seat(1);
        let out = s
            .handle(SignalIn::Answer {
                seat: 1,
                sdp: "v=0\r\nt=0 0\r\n".into(),
            })
            .unwrap();
        assert!(matches!(
            &out[0],
            SignalOut::Answer {
                seat: 0,
                sdp
            } if sdp.contains("t=0 0")
        ));
    }

    #[test]
    fn late_joiner_gets_pending_offer() {
        let mut s = MediaSession::new("r1");
        let _ = s
            .handle(SignalIn::Offer {
                seat: 0,
                sdp: "v=0\r\nt=0 0\r\noffer-from-0\r\n".into(),
            })
            .unwrap();
        let out = s.seat_joined(1);
        assert_eq!(out.len(), 1);
        match &out[0] {
            SignalOut::Offer { seat: 1, sdp } => assert!(sdp.contains("offer-from-0")),
            other => panic!("expected replayed offer, got {other:?}"),
        }
    }
}

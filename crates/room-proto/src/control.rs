//! JSON control messages (`docs/22` §4).

use crate::ProtoError;
use serde::{Deserialize, Serialize};

/// Max JSON control frame size (WebRTC SDP offers/answers need headroom).
pub const MAX_CONTROL_BYTES: usize = 65_536;

/// Seat lifecycle in the roster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatState {
    /// In lobby.
    Lobby,
    /// Loading map.
    Loading,
    /// Sent ready.
    Ready,
    /// In game.
    Playing,
    /// Silent / disconnected (stall fill).
    Away,
}

/// One seat in a roster push.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosterSeat {
    /// Seat index.
    pub seat: u8,
    /// Display name.
    pub name: String,
    /// Lifecycle state.
    pub state: SeatState,
    /// Server mute flag.
    #[serde(default)]
    pub muted: bool,
}

/// Room settings inside `welcome`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WelcomeRoom {
    /// Start map.
    pub map: String,
    /// Skill 1..=5.
    pub skill: u8,
    /// Max seats.
    pub max_players: u8,
    /// RNG seed.
    pub rng_seed: u32,
    /// Input delay tics.
    pub delay: u32,
}

/// One ICE server entry (STUN/TURN) for `welcome.media`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IceServerEntry {
    /// One or more `stun:` / `turn:` / `turns:` URLs.
    pub urls: Vec<String>,
    /// TURN username (`expiry:user`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// TURN credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

/// Media block in `welcome`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaInfo {
    /// Whether WebRTC media is enabled for this room.
    pub enabled: bool,
    /// ICE servers (STUN + optional short-lived TURN).
    #[serde(default)]
    pub ice_servers: Vec<IceServerEntry>,
}

/// Client → server hello.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloMsg {
    /// Join ticket (opaque string; M1 may use room id).
    pub ticket: String,
    /// Client engine version.
    pub client_version: String,
    /// Script ABI major.
    pub abi_major: i32,
    /// IWAD fingerprint as hex string.
    pub iwad_fingerprint: String,
    /// Display name (M1; later taken from ticket claims).
    #[serde(default)]
    pub name: Option<String>,
    /// Optional resume token.
    #[serde(default)]
    pub resume_token: Option<String>,
}

/// Server → client welcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WelcomeMsg {
    /// Assigned seat.
    pub seat: u8,
    /// Room settings.
    pub room: WelcomeRoom,
    /// Resume token for this seat.
    pub resume_token: String,
    /// Current roster.
    pub roster: Vec<RosterSeat>,
    /// Media / ICE info.
    pub media: MediaInfo,
}

/// Server → client reject.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RejectMsg {
    /// Machine-readable code.
    pub code: String,
    /// Human reason.
    pub reason: String,
}

/// Tagged control message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ControlMsg {
    /// Client hello.
    Hello(HelloMsg),
    /// Server welcome.
    Welcome(WelcomeMsg),
    /// Reject.
    Reject(RejectMsg),
    /// Roster update.
    Roster {
        /// Seats.
        seats: Vec<RosterSeat>,
    },
    /// Client ready after map load.
    Ready {},
    /// Server start.
    Start {
        /// First tic.
        tic0: u32,
        /// Confirmed RNG seed.
        rng_seed: u32,
    },
    /// Chat.
    Chat {
        /// Text body.
        text: String,
        /// Filled by server.
        #[serde(default)]
        from: Option<u8>,
        /// Server timestamp.
        #[serde(default)]
        ts: Option<u64>,
    },
    /// Desync notice.
    Desync {
        /// Tic.
        tic: u32,
        /// Seats that disagreed.
        seats: Vec<u8>,
    },
    /// WebRTC signalling (opaque for M1).
    Rtc {
        /// offer / answer / ice / track.
        kind: String,
        /// SDP.
        #[serde(default)]
        sdp: Option<String>,
        /// ICE candidate.
        #[serde(default)]
        candidate: Option<String>,
        /// Mid.
        #[serde(default)]
        mid: Option<String>,
    },
    /// Admin / system notice.
    Notice {
        /// Text.
        text: String,
    },
    /// Bye before close.
    Bye {
        /// Code.
        code: String,
        /// Reason.
        reason: String,
    },
    /// Client pose telemetry for the admin minimap (map units / degrees).
    Pose {
        /// World X.
        x: i32,
        /// World Y.
        y: i32,
        /// Yaw degrees.
        angle: i16,
    },
    /// Sparse linedef outline for the admin minimap (`[x1,y1,x2,y2,…]`, i16 map units).
    MapOutline {
        /// Flat segment list.
        segs: Vec<i16>,
    },
    /// Client → server: join as a seatless spectator (instead of `hello`).
    Watch {
        /// IWAD fingerprint as hex string (must match the room).
        iwad_fingerprint: String,
        /// Spectator display name.
        #[serde(default)]
        name: Option<String>,
        /// Client engine version.
        #[serde(default)]
        client_version: String,
    },
    /// Server → spectator: room settings + roster. Frames from tic 0 follow
    /// as `TIC_BATCH` / `TIC_FRAME` once the room has started.
    Watching {
        /// Room settings.
        room: WelcomeRoom,
        /// Current roster.
        roster: Vec<RosterSeat>,
    },
}

/// Parse a JSON control message from UTF-8 text.
///
/// # Errors
///
/// Oversized, invalid JSON, or unknown fields (`deny_unknown_fields` via serde).
pub fn parse_control(text: &str) -> Result<ControlMsg, ProtoError> {
    if text.len() > MAX_CONTROL_BYTES {
        return Err(ProtoError::TooLarge);
    }
    serde_json::from_str(text).map_err(|e| ProtoError::BadJson(e.to_string()))
}

/// Parse control from raw UTF-8 bytes.
///
/// # Errors
///
/// Same as [`parse_control`].
pub fn parse_control_bytes(bytes: &[u8]) -> Result<ControlMsg, ProtoError> {
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ProtoError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|e| ProtoError::BadJson(e.to_string()))?;
    parse_control(text)
}

/// Encode a control message to JSON text.
///
/// # Errors
///
/// Serialization failure (should not happen for well-formed values).
pub fn encode_control(msg: &ControlMsg) -> Result<String, ProtoError> {
    serde_json::to_string(msg).map_err(|e| ProtoError::BadJson(e.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hello_ready_roundtrip() {
        let hello = ControlMsg::Hello(HelloMsg {
            ticket: "abc".into(),
            client_version: "0.1.0".into(),
            abi_major: 1,
            iwad_fingerprint: "deadbeef".into(),
            name: Some("p1".into()),
            resume_token: None,
        });
        let s = encode_control(&hello).unwrap();
        assert!(s.contains("\"t\":\"hello\""));
        assert_eq!(parse_control(&s).unwrap(), hello);

        let ready = ControlMsg::Ready {};
        let s = encode_control(&ready).unwrap();
        assert_eq!(parse_control(&s).unwrap(), ready);

        let pose = ControlMsg::Pose {
            x: 1,
            y: -2,
            angle: 90,
        };
        let s = encode_control(&pose).unwrap();
        assert_eq!(parse_control(&s).unwrap(), pose);
    }

    #[test]
    fn watch_roundtrip() {
        let watch = ControlMsg::Watch {
            iwad_fingerprint: "ab".into(),
            name: Some("eye".into()),
            client_version: "0.1".into(),
        };
        let s = encode_control(&watch).unwrap();
        assert!(s.contains("\"t\":\"watch\""));
        assert_eq!(parse_control(&s).unwrap(), watch);
        let minimal = r#"{"t":"watch","iwad_fingerprint":"ab"}"#;
        assert!(matches!(
            parse_control(minimal).unwrap(),
            ControlMsg::Watch { name: None, .. }
        ));
    }

    #[test]
    fn rejects_unknown_fields() {
        let bad = r#"{"t":"ready","extra":1}"#;
        assert!(matches!(parse_control(bad), Err(ProtoError::BadJson(_))));
    }
}

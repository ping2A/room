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
    /// Atomic Duke `ud.coop` mode (0 = DukeMatch, 1 = co-op, 2 = no spawn).
    #[serde(
        default = "default_duke_game_mode",
        skip_serializing_if = "is_default_duke_game_mode"
    )]
    pub duke_game_mode: u8,
    /// Atomic Duke `ud.ffire`; ignored for Doom rooms.
    #[serde(default, skip_serializing_if = "is_false")]
    pub duke_friendly_fire: bool,
    /// RNG seed.
    pub rng_seed: u32,
    /// Input delay tics.
    pub delay: u32,
    /// Gameplay mod every seat mounts after the IWAD (absent = vanilla).
    #[serde(default, rename = "mod", skip_serializing_if = "Option::is_none")]
    pub game_mod: Option<RoomModRef>,
    /// Extra content packs every seat mounts after the IWAD / gameplay mod
    /// (e.g. `voxel`). Served from `/play/mods/{name}.pk3`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<String>,
    /// Match time limit in seconds (0 = none). Every seat's sim ends the match
    /// when it runs out; the server closes the room shortly after.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub time_limit: u32,
    /// Empty seats are sim bots on every client (`GameOptions.fill_bots`, BE8).
    #[serde(default, skip_serializing_if = "is_false")]
    pub fill_bots: bool,
    /// Server content files (PK3 / WAD) every seat mounts after the IWAD, in
    /// order, before the gameplay mod and packs. From the room's preset;
    /// downloaded from `GET /api/content/{file}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<RoomContentRef>,
    /// Base game (IWAD) every seat uses instead of its default one (from the
    /// preset; absent = the client's IWAD).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iwad: Option<RoomContentRef>,
    /// Title of the server preset the room was created from (display only).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preset: String,
}

/// A content file from the server library (`GET /api/content/{file}`).
/// Clients must reject bytes whose [`content_fingerprint`] differs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomContentRef {
    /// File name in the library (`brutal22.pk3`, `freedoom2.wad`).
    pub file: String,
    /// [`content_fingerprint`] of the file, 16 hex digits.
    pub fingerprint: String,
    /// File size in bytes.
    pub size: u64,
}

/// Valid content library file name: `[A-Za-z0-9._-]`, 1..=96 chars, a known
/// extension (`wad`, `pk3`, `pk7`, `zip`, `ipk3`, `ipk7`), no leading dot.
#[must_use]
pub fn is_valid_content_file(name: &str) -> bool {
    let ok_chars = !name.is_empty()
        && name.len() <= 96
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'));
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    ok_chars
        && matches!(
            ext.as_deref(),
            Some("wad" | "pk3" | "pk7" | "zip" | "ipk3" | "ipk7")
        )
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(v: &u32) -> bool {
    *v == 0
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn default_duke_game_mode() -> u8 {
    1
}

fn is_default_duke_game_mode(v: &u8) -> bool {
    *v == 1
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

/// One seat in a `result` report (final standings, best first).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultRow {
    /// Seat index.
    pub seat: u8,
    /// 1-based place.
    pub place: u8,
    /// Frags.
    pub frags: i32,
    /// Deaths.
    pub deaths: i32,
    /// Damage dealt (BE11; 0 when omitted).
    #[serde(default)]
    pub damage: i32,
}

/// Mod PWAD a room plays with; clients download it from `GET /api/mods/{name}.wad`
/// and must reject bytes whose [`content_fingerprint`] differs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomModRef {
    /// Catalog name (`[a-z0-9_-]`, 1..=32).
    pub name: String,
    /// Display title (manifest `name`).
    pub title: String,
    /// [`content_fingerprint`] of the file, 16 hex digits.
    pub fingerprint: String,
    /// File size in bytes.
    pub size: u64,
}

/// FNV-1a over the bytes, then xor the length (same hash as the IWAD fingerprint).
#[must_use]
pub fn content_fingerprint(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h ^ bytes.len() as u64
}

/// Valid mod catalog name: 1..=32 of `[a-z0-9_-]`.
#[must_use]
pub fn is_valid_mod_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// Content packs clients can stack with a gameplay mod (`welcome.room.packs`).
pub const KNOWN_PACKS: &[&str] = &["voxel"];

/// `true` when `name` is a known content pack (e.g. `voxel`).
#[must_use]
pub fn is_known_pack(name: &str) -> bool {
    KNOWN_PACKS.iter().any(|p| *p == name)
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
        /// Bot fill for seats `bot_first_seat`..`max_players` (BE8).
        #[serde(default, skip_serializing_if = "is_false")]
        fill_bots: bool,
        /// First seat index driven by bot brains (humans occupy lower seats).
        #[serde(default, skip_serializing_if = "is_zero_u8")]
        bot_first_seat: u8,
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
    /// Client → server: the match ended in this seat's sim (identical on
    /// every honest seat). The server records it once all seats agree and
    /// closes the room after a grace period.
    Result {
        /// Winning seat (absent = no winner).
        #[serde(default)]
        winner: Option<u8>,
        /// Ended by the time limit.
        #[serde(default)]
        time_up: bool,
        /// Standings, best first.
        standings: Vec<ResultRow>,
    },
    /// Server → spectator: room settings + roster. Frames from tic 0 follow
    /// as `TIC_BATCH` / `TIC_FRAME` once the room has started.
    Watching {
        /// Room settings.
        room: WelcomeRoom,
        /// Current roster.
        roster: Vec<RosterSeat>,
    },
    /// Opt-in face-share blob (`docs/26` §8). Wire RGBA is 64×64 (fits control
    /// size after base64); local gallery may keep 128×128. Server relays when
    /// `face_share_enabled`; never persists.
    FaceShare {
        /// Sender seat (filled by server on relay).
        #[serde(default)]
        from: Option<u8>,
        /// Gallery label.
        name: String,
        /// Face id bits as decimal string (JS-safe).
        face_id: String,
        /// Base64 of raw 64×64×4 RGBA.
        rgba_b64: String,
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
    fn welcome_room_mod_is_optional_on_the_wire() {
        let mut room = WelcomeRoom {
            map: "E1M1".into(),
            skill: 3,
            max_players: 2,
            duke_game_mode: 1,
            duke_friendly_fire: false,
            rng_seed: 7,
            delay: 0,
            game_mod: None,
            packs: Vec::new(),
            time_limit: 0,
            fill_bots: false,
            content: Vec::new(),
            iwad: None,
            preset: String::new(),
        };
        let plain = serde_json::to_string(&room).unwrap();
        assert!(!plain.contains("content") && !plain.contains("iwad") && !plain.contains("preset"));
        assert!(
            !plain.contains("mod") && !plain.contains("time_limit") && !plain.contains("packs")
        );
        assert_eq!(serde_json::from_str::<WelcomeRoom>(&plain).unwrap(), room);
        room.game_mod = Some(RoomModRef {
            name: "bounty".into(),
            title: "Bounty Hunter".into(),
            fingerprint: format!("{:016x}", content_fingerprint(b"PWAD")),
            size: 4,
        });
        room.packs = vec!["voxel".into()];
        let modded = serde_json::to_string(&room).unwrap();
        assert!(modded.contains("\"mod\":{\"name\":\"bounty\""));
        assert!(modded.contains("\"packs\":[\"voxel\"]"));
        assert_eq!(serde_json::from_str::<WelcomeRoom>(&modded).unwrap(), room);
    }

    #[test]
    fn known_packs() {
        assert!(is_known_pack("voxel"));
        assert!(!is_known_pack("survival"));
    }

    #[test]
    fn mod_names_are_restricted() {
        assert!(is_valid_mod_name("doom1-bounty_2"));
        for bad in ["", "Bounty", "../x", "a.wad", &"x".repeat(33)] {
            assert!(!is_valid_mod_name(bad), "{bad}");
        }
    }

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
    fn result_roundtrip() {
        let msg = ControlMsg::Result {
            winner: Some(1),
            time_up: false,
            standings: vec![ResultRow {
                seat: 1,
                place: 1,
                frags: 2,
                deaths: 0,
                damage: 0,
            }],
        };
        let s = encode_control(&msg).unwrap();
        assert!(s.contains("\"t\":\"result\""));
        assert_eq!(parse_control(&s).unwrap(), msg);
        let draw = r#"{"t":"result","standings":[]}"#;
        assert!(matches!(
            parse_control(draw).unwrap(),
            ControlMsg::Result {
                winner: None,
                time_up: false,
                ..
            }
        ));
    }

    #[test]
    fn rejects_unknown_fields() {
        let bad = r#"{"t":"ready","extra":1}"#;
        assert!(matches!(parse_control(bad), Err(ProtoError::BadJson(_))));
    }
}

//! Lobby / handshake messages (`docs/17` §1.2).

use crate::NetError;
use crate::PeerId;

/// Engine version string exchanged in handshake (bootstrap constant).
pub const ENGINE_VERSION: &str = "0.1.0";
/// Script ABI major expected by peers.
pub const ABI_MAJOR: i32 = 1;

const MAGIC: &[u8; 4] = b"FNHS";

/// Hello from a joining peer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    /// Engine version.
    pub engine_version: String,
    /// ABI major.
    pub abi_major: i32,
    /// Load-order / defs fingerprint.
    pub fingerprint: u64,
    /// Player display name.
    pub name: String,
}

/// Welcome from the host (arbitrator).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Welcome {
    /// Assigned seat.
    pub peer_id: PeerId,
    /// Total peer count for the session.
    pub peer_count: u8,
    /// Input delay tics.
    pub delay: u32,
    /// RNG seed for the match.
    pub rng_seed: u32,
    /// Start map lump name.
    pub map: String,
    /// Skill 1..=5.
    pub skill: u8,
    /// Match rules: `0` co-op, `1` deathmatch. Trailing byte; `0` from older
    /// hosts.
    pub deathmatch: u8,
    /// Bot seats after the player seats (everyone simulates them).
    pub bots: u8,
}

/// Host rejected the join.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reject {
    /// Human-readable reason.
    pub reason: String,
}

/// Match start signal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    /// Confirmed RNG seed.
    pub rng_seed: u32,
    /// Display name per seat (`0..names.len()`), broadcast for lockstep (BE3).
    pub names: Vec<String>,
    /// Input delay the host picked once everyone joined (from measured
    /// ping); overrides [`Welcome::delay`]. `None` from older hosts.
    pub delay: Option<u32>,
}

/// Host → peer round-trip probe while the lobby fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ping {
    /// Host clock (ms since the probe began), echoed back.
    pub nonce: u32,
}

/// Peer → host answer to a [`Ping`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pong {
    /// Answering seat.
    pub peer_id: PeerId,
    /// [`Ping::nonce`].
    pub nonce: u32,
}

/// Peer finished local load and is ready for tic 0 (web coop gate).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ready {
    /// Seat that is ready.
    pub peer_id: PeerId,
}

/// Lobby packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbyMsg {
    /// Client → host.
    Hello(Hello),
    /// Host → client.
    Welcome(Welcome),
    /// Host → client.
    Reject(Reject),
    /// Host → all.
    Start(Start),
    /// Peer → all: map/sim loaded; lockstep may begin when every seat has Ready.
    Ready(Ready),
    /// Host → peer.
    Ping(Ping),
    /// Peer → host.
    Pong(Pong),
}

impl LobbyMsg {
    /// Encode to wire bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(MAGIC);
        match self {
            Self::Hello(h) => {
                v.push(1);
                push_str(&mut v, &h.engine_version);
                v.extend_from_slice(&h.abi_major.to_le_bytes());
                v.extend_from_slice(&h.fingerprint.to_le_bytes());
                push_str(&mut v, &h.name);
            }
            Self::Welcome(w) => {
                v.push(2);
                v.push(w.peer_id.0);
                v.push(w.peer_count);
                v.extend_from_slice(&w.delay.to_le_bytes());
                v.extend_from_slice(&w.rng_seed.to_le_bytes());
                push_str(&mut v, &w.map);
                v.push(w.skill);
                v.push(w.deathmatch);
                v.push(w.bots);
            }
            Self::Reject(r) => {
                v.push(3);
                push_str(&mut v, &r.reason);
            }
            Self::Start(s) => {
                // Type 6 carries the negotiated delay; 4 is the old layout.
                v.push(if s.delay.is_some() { 6 } else { 4 });
                v.extend_from_slice(&s.rng_seed.to_le_bytes());
                if let Some(delay) = s.delay {
                    v.extend_from_slice(&delay.to_le_bytes());
                }
                for name in &s.names {
                    push_str(&mut v, name);
                }
            }
            Self::Ready(r) => {
                v.push(5);
                v.push(r.peer_id.0);
            }
            Self::Ping(p) => {
                v.push(7);
                v.extend_from_slice(&p.nonce.to_le_bytes());
            }
            Self::Pong(p) => {
                v.push(8);
                v.push(p.peer_id.0);
                v.extend_from_slice(&p.nonce.to_le_bytes());
            }
        }
        v
    }

    /// Decode wire bytes.
    ///
    /// # Errors
    ///
    /// Truncated / unknown type.
    pub fn decode(bytes: &[u8]) -> Result<Self, NetError> {
        if bytes.len() < 5 || &bytes[0..4] != MAGIC {
            return Err(NetError::BadPacket);
        }
        let mut o = 5;
        match bytes[4] {
            1 => {
                let engine_version = take_str(bytes, &mut o)?;
                let abi_major = take_i32(bytes, &mut o)?;
                let fingerprint = take_u64(bytes, &mut o)?;
                let name = take_str(bytes, &mut o)?;
                Ok(Self::Hello(Hello {
                    engine_version,
                    abi_major,
                    fingerprint,
                    name,
                }))
            }
            2 => {
                let peer_id = PeerId(take_u8(bytes, &mut o)?);
                let peer_count = take_u8(bytes, &mut o)?;
                let delay = take_u32(bytes, &mut o)?;
                let rng_seed = take_u32(bytes, &mut o)?;
                let map = take_str(bytes, &mut o)?;
                let skill = take_u8(bytes, &mut o)?;
                // Rules and bots came later: absent from older hosts.
                let deathmatch = take_u8(bytes, &mut o).unwrap_or(0);
                let bots = take_u8(bytes, &mut o).unwrap_or(0);
                Ok(Self::Welcome(Welcome {
                    peer_id,
                    peer_count,
                    delay,
                    rng_seed,
                    map,
                    skill,
                    deathmatch,
                    bots,
                }))
            }
            3 => Ok(Self::Reject(Reject {
                reason: take_str(bytes, &mut o)?,
            })),
            kind @ (4 | 6) => {
                let rng_seed = take_u32(bytes, &mut o)?;
                let delay = if kind == 6 {
                    Some(take_u32(bytes, &mut o)?)
                } else {
                    None
                };
                let mut names = Vec::new();
                while o < bytes.len() {
                    names.push(take_str(bytes, &mut o)?);
                }
                Ok(Self::Start(Start {
                    rng_seed,
                    names,
                    delay,
                }))
            }
            7 => Ok(Self::Ping(Ping {
                nonce: take_u32(bytes, &mut o)?,
            })),
            8 => Ok(Self::Pong(Pong {
                peer_id: PeerId(take_u8(bytes, &mut o)?),
                nonce: take_u32(bytes, &mut o)?,
            })),
            5 => Ok(Self::Ready(Ready {
                peer_id: PeerId(take_u8(bytes, &mut o)?),
            })),
            _ => Err(NetError::BadPacket),
        }
    }
}

/// Validate a [`Hello`] against host expectations.
///
/// # Errors
///
/// [`NetError::Reject`] on mismatch.
pub fn validate_hello(hello: &Hello, fingerprint: u64) -> Result<(), NetError> {
    if hello.engine_version != ENGINE_VERSION {
        return Err(NetError::Reject(format!(
            "engine {} != {ENGINE_VERSION}",
            hello.engine_version
        )));
    }
    if hello.abi_major != ABI_MAJOR {
        return Err(NetError::Reject(format!(
            "abi {} != {ABI_MAJOR}",
            hello.abi_major
        )));
    }
    if hello.fingerprint != fingerprint {
        return Err(NetError::Reject(format!(
            "fingerprint {:016x} != {fingerprint:016x}",
            hello.fingerprint
        )));
    }
    Ok(())
}

/// Simple host-side lobby state (assigns seats 1..N-1; host is 0).
#[derive(Debug)]
pub struct LobbyHost {
    fingerprint: u64,
    max_players: u8,
    delay: u32,
    rng_seed: u32,
    map: String,
    skill: u8,
    deathmatch: u8,
    bots: u8,
    /// Joined names by peer id.
    seats: Vec<(PeerId, String)>,
    next_id: u8,
}

impl LobbyHost {
    /// Host sits in seat 0.
    #[must_use]
    pub fn new(
        fingerprint: u64,
        max_players: u8,
        delay: u32,
        rng_seed: u32,
        map: impl Into<String>,
        skill: u8,
    ) -> Self {
        Self {
            fingerprint,
            max_players: max_players.max(2),
            delay: delay.max(1),
            rng_seed,
            map: map.into(),
            skill: skill.clamp(1, 5),
            deathmatch: 0,
            bots: 0,
            seats: vec![(PeerId(0), "host".into())],
            next_id: 1,
        }
    }

    /// Match rules sent to every joiner: deathmatch (`true`) or co-op, and
    /// how many bot seats follow the player seats.
    pub fn set_rules(&mut self, deathmatch: bool, bots: u8) {
        self.deathmatch = u8::from(deathmatch);
        self.bots = bots;
    }

    /// Handle a hello; returns welcome or reject message.
    pub fn on_hello(&mut self, hello: Hello) -> LobbyMsg {
        if let Err(NetError::Reject(reason)) = validate_hello(&hello, self.fingerprint) {
            return LobbyMsg::Reject(Reject { reason });
        }
        if self.seats.len() as u8 >= self.max_players {
            return LobbyMsg::Reject(Reject {
                reason: "lobby full".into(),
            });
        }
        let id = PeerId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.seats.push((id, hello.name));
        self.welcome_for(id)
    }

    /// Like [`Self::on_hello`], but binds the joiner to an already-assigned `seat`
    /// (TCP accept / relay connection id).
    ///
    /// Duplicate Hellos for an occupied seat re-send [`Welcome`] (joiners retry
    /// until they see Welcome; a Reject would kill the match).
    pub fn on_hello_at(&mut self, seat: PeerId, hello: Hello) -> LobbyMsg {
        if let Err(NetError::Reject(reason)) = validate_hello(&hello, self.fingerprint) {
            return LobbyMsg::Reject(Reject { reason });
        }
        if seat.0 == 0 {
            return LobbyMsg::Reject(Reject {
                reason: "seat 0 is host".into(),
            });
        }
        if let Some((_, name)) = self.seats.iter_mut().find(|(id, _)| *id == seat) {
            *name = hello.name;
            return self.welcome_for(seat);
        }
        if self.seats.len() as u8 >= self.max_players {
            return LobbyMsg::Reject(Reject {
                reason: "lobby full".into(),
            });
        }
        self.seats.push((seat, hello.name));
        self.seats.sort_by_key(|(id, _)| id.0);
        if seat.0 >= self.next_id {
            self.next_id = seat.0.saturating_add(1);
        }
        self.welcome_for(seat)
    }

    fn welcome_for(&self, peer_id: PeerId) -> LobbyMsg {
        LobbyMsg::Welcome(Welcome {
            peer_id,
            peer_count: self.max_players,
            delay: self.delay,
            rng_seed: self.rng_seed,
            map: self.map.clone(),
            skill: self.skill,
            deathmatch: self.deathmatch,
            bots: self.bots,
        })
    }

    /// Broadcast start when lobby is ready (caller decides timing).
    #[must_use]
    pub fn start_msg(&self) -> LobbyMsg {
        let mut names = vec![String::new(); usize::from(self.max_players)];
        for (id, n) in &self.seats {
            let i = usize::from(id.0);
            if i < names.len() {
                names[i] = n.clone();
            }
        }
        LobbyMsg::Start(Start {
            rng_seed: self.rng_seed,
            names,
            delay: None,
        })
    }

    /// [`Self::start_msg`] carrying the input delay every seat must use.
    #[must_use]
    pub fn start_msg_with_delay(&self, delay: u32) -> LobbyMsg {
        match self.start_msg() {
            LobbyMsg::Start(mut s) => {
                s.delay = Some(delay);
                LobbyMsg::Start(s)
            }
            other => other,
        }
    }

    /// Occupied seats.
    #[must_use]
    pub fn seats(&self) -> &[(PeerId, String)] {
        &self.seats
    }
}

/// Lockstep input delay (tics, `1..=8`) for the host's measured round trips
/// to each joined seat (ms). A command from seat A reaches seat B in about
/// half A's round trip plus half B's (through the host on a relay; a mesh is
/// usually no slower), so the two slowest seats decide. A peer stalls only
/// when that one-way time (plus jitter margin) is longer than the delay.
#[must_use]
pub fn delay_for_rtts(rtts_ms: &[u32]) -> u32 {
    const TIC_MS: f64 = 1000.0 / 35.0;
    const JITTER_MS: f64 = 10.0;
    let mut sorted = rtts_ms.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let worst = f64::from(sorted.first().copied().unwrap_or(0));
    let second = f64::from(sorted.get(1).copied().unwrap_or(0));
    let one_way = (worst + second) / 2.0 + JITTER_MS;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let tics = (one_way / TIC_MS).ceil() as u32;
    tics.clamp(1, 8)
}

fn push_str(v: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let len = u16::try_from(b.len().min(usize::from(u16::MAX))).unwrap_or(u16::MAX);
    v.extend_from_slice(&len.to_le_bytes());
    v.extend_from_slice(&b[..usize::from(len)]);
}

fn take_u8(bytes: &[u8], o: &mut usize) -> Result<u8, NetError> {
    let v = *bytes.get(*o).ok_or(NetError::BadPacket)?;
    *o += 1;
    Ok(v)
}

fn take_i32(bytes: &[u8], o: &mut usize) -> Result<i32, NetError> {
    let s = bytes.get(*o..*o + 4).ok_or(NetError::BadPacket)?;
    *o += 4;
    Ok(i32::from_le_bytes(
        s.try_into().map_err(|_| NetError::BadPacket)?,
    ))
}

fn take_u32(bytes: &[u8], o: &mut usize) -> Result<u32, NetError> {
    let s = bytes.get(*o..*o + 4).ok_or(NetError::BadPacket)?;
    *o += 4;
    Ok(u32::from_le_bytes(
        s.try_into().map_err(|_| NetError::BadPacket)?,
    ))
}

fn take_u64(bytes: &[u8], o: &mut usize) -> Result<u64, NetError> {
    let s = bytes.get(*o..*o + 8).ok_or(NetError::BadPacket)?;
    *o += 8;
    Ok(u64::from_le_bytes(
        s.try_into().map_err(|_| NetError::BadPacket)?,
    ))
}

fn take_str(bytes: &[u8], o: &mut usize) -> Result<String, NetError> {
    let s = bytes.get(*o..*o + 2).ok_or(NetError::BadPacket)?;
    let len = u16::from_le_bytes(s.try_into().map_err(|_| NetError::BadPacket)?) as usize;
    *o += 2;
    let raw = bytes.get(*o..*o + len).ok_or(NetError::BadPacket)?;
    *o += len;
    String::from_utf8(raw.to_vec()).map_err(|_| NetError::BadPacket)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn lobby_codec_and_accept() {
        let hello = LobbyMsg::Hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint: 0xdead_beef,
            name: "p1".into(),
        });
        let bytes = hello.encode();
        assert_eq!(LobbyMsg::decode(&bytes).unwrap(), hello);

        let ready = LobbyMsg::Ready(Ready { peer_id: PeerId(1) });
        assert_eq!(LobbyMsg::decode(&ready.encode()).unwrap(), ready);

        let mut host = LobbyHost::new(0xdead_beef, 4, 2, 99, "MAP01", 3);
        let LobbyMsg::Hello(h) = hello else {
            panic!("hello");
        };
        let welcome = host.on_hello(h);
        match welcome {
            LobbyMsg::Welcome(w) => {
                assert_eq!(w.peer_id, PeerId(1));
                assert_eq!(w.peer_count, 4);
                assert_eq!(w.map, "MAP01");
                assert_eq!(w.rng_seed, 99);
            }
            other => panic!("expected welcome, got {other:?}"),
        }
    }

    #[test]
    fn lobby_rejects_bad_fingerprint() {
        let mut host = LobbyHost::new(1, 4, 1, 0, "MAP01", 3);
        let msg = host.on_hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint: 2,
            name: "x".into(),
        });
        assert!(matches!(msg, LobbyMsg::Reject(_)));
    }

    #[test]
    fn lobby_fills_to_max_then_start() {
        let mut host = LobbyHost::new(0xabc, 2, 1, 1234, "E1M1", 3);
        assert_eq!(host.seats().len(), 1);
        let w = host.on_hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint: 0xabc,
            name: "joiner".into(),
        });
        assert!(matches!(w, LobbyMsg::Welcome(_)));
        assert_eq!(host.seats().len(), 2);
        let full = host.on_hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint: 0xabc,
            name: "late".into(),
        });
        assert!(matches!(full, LobbyMsg::Reject(_)));
        match host.start_msg() {
            LobbyMsg::Start(s) => assert_eq!(s.rng_seed, 1234),
            other => panic!("expected Start, got {other:?}"),
        }
    }

    #[test]
    fn lobby_rejects_abi_mismatch() {
        let mut host = LobbyHost::new(9, 4, 1, 0, "MAP01", 3);
        let msg = host.on_hello(Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR + 1,
            fingerprint: 9,
            name: "x".into(),
        });
        assert!(matches!(msg, LobbyMsg::Reject(_)));
    }

    #[test]
    fn on_hello_at_idempotent_for_same_seat() {
        let mut host = LobbyHost::new(0xabc, 2, 1, 7, "E1M1", 3);
        let hello = Hello {
            engine_version: ENGINE_VERSION.into(),
            abi_major: ABI_MAJOR,
            fingerprint: 0xabc,
            name: "p2".into(),
        };
        let first = host.on_hello_at(PeerId(1), hello.clone());
        let second = host.on_hello_at(
            PeerId(1),
            Hello {
                name: "p2-retry".into(),
                ..hello
            },
        );
        assert!(matches!(first, LobbyMsg::Welcome(_)));
        match second {
            LobbyMsg::Welcome(w) => {
                assert_eq!(w.peer_id, PeerId(1));
                assert_eq!(host.seats().len(), 2);
                assert_eq!(host.seats()[1].1, "p2-retry");
            }
            other => panic!("expected Welcome on retry, got {other:?}"),
        }
    }

    #[test]
    fn start_with_delay_and_ping_pong_round_trip() {
        for delay in [None, Some(3)] {
            let msg = LobbyMsg::Start(Start {
                rng_seed: 77,
                names: vec!["Ann".into(), "Bob".into()],
                delay,
            });
            assert_eq!(LobbyMsg::decode(&msg.encode()).unwrap(), msg);
        }
        let ping = LobbyMsg::Ping(Ping { nonce: 1234 });
        assert_eq!(LobbyMsg::decode(&ping.encode()).unwrap(), ping);
        let pong = LobbyMsg::Pong(Pong {
            peer_id: PeerId(2),
            nonce: 1234,
        });
        assert_eq!(LobbyMsg::decode(&pong.encode()).unwrap(), pong);
    }

    #[test]
    fn delay_grows_with_the_two_slowest_round_trips() {
        assert_eq!(delay_for_rtts(&[]), 1);
        assert_eq!(delay_for_rtts(&[2, 1, 3]), 1, "LAN");
        // One joiner 60 ms away: 30 ms + 10 ms margin = 2 tics.
        assert_eq!(delay_for_rtts(&[60]), 2);
        // Two joiners 100 ms away talk to each other in ~100 ms: 4 tics.
        assert_eq!(delay_for_rtts(&[100, 100, 5]), 4);
        assert_eq!(delay_for_rtts(&[2000, 2000]), 8, "capped");
    }

    #[test]
    fn welcome_carries_rules_and_reads_old_hosts() {
        let w = Welcome {
            peer_id: PeerId(1),
            peer_count: 2,
            delay: 2,
            rng_seed: 9,
            map: "MAP01".into(),
            skill: 3,
            deathmatch: 1,
            bots: 3,
        };
        let msg = LobbyMsg::Welcome(w.clone());
        let bytes = msg.encode();
        assert_eq!(LobbyMsg::decode(&bytes).unwrap(), msg);
        // An older host stops after the skill byte: co-op, no bots.
        match LobbyMsg::decode(&bytes[..bytes.len() - 2]).unwrap() {
            LobbyMsg::Welcome(old) => {
                assert_eq!((old.deathmatch, old.bots), (0, 0));
                assert_eq!(old.map, "MAP01");
            }
            other => panic!("{other:?}"),
        }
    }
}

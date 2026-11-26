//! Find co-op games on the local network (docs/41 M7).
//!
//! A UDP host answers a small probe on its game port: a phone (or desktop)
//! broadcasts [`PROBE`] to the default port and lists every reply, so
//! nobody has to type an address on the same Wi-Fi. The reply carries the
//! lobby fingerprint, so the list can tell which games match the joiner's
//! own game and mods.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// Port `--host` binds by default (and the one probes go to).
pub const DEFAULT_PORT: u16 = 5029;

/// "Who is hosting?" sent by a joiner.
pub const PROBE: &[u8] = b"RMLANQ1";
/// Start of a host's answer.
const REPLY: &[u8] = b"RMLANA1";

/// One game found by [`discover`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanGame {
    /// Where to join.
    pub addr: SocketAddr,
    /// Players already in (host included).
    pub players: u8,
    /// Players the host waits for.
    pub max_players: u8,
    /// The host's lobby fingerprint (game, mods, map, skill).
    pub fingerprint: u64,
    /// Start map.
    pub map: String,
}

/// Whether `bytes` is a discovery probe.
#[must_use]
pub fn is_probe(bytes: &[u8]) -> bool {
    bytes == PROBE
}

/// A host's answer to a probe.
#[must_use]
pub fn reply(players: u8, max_players: u8, fingerprint: u64, map: &str) -> Vec<u8> {
    let mut out = REPLY.to_vec();
    out.push(players);
    out.push(max_players);
    out.extend_from_slice(&fingerprint.to_le_bytes());
    out.extend(map.bytes().take(32));
    out
}

/// Parse a reply from `addr`.
#[must_use]
pub fn parse_reply(addr: SocketAddr, bytes: &[u8]) -> Option<LanGame> {
    let rest = bytes.strip_prefix(REPLY)?;
    if rest.len() < 10 {
        return None;
    }
    let fingerprint = u64::from_le_bytes(rest[2..10].try_into().ok()?);
    Some(LanGame {
        addr,
        players: rest[0],
        max_players: rest[1],
        fingerprint,
        map: String::from_utf8_lossy(&rest[10..]).into_owned(),
    })
}

/// Probe `targets` (the broadcast address, or known hosts) and collect the
/// answers for `wait`. One entry per host, in the order they answered.
///
/// # Errors
/// No UDP socket could be opened.
pub fn discover(targets: &[SocketAddr], wait: Duration) -> std::io::Result<Vec<LanGame>> {
    let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))?;
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(Duration::from_millis(50)))?;
    let deadline = Instant::now() + wait;
    let mut found: Vec<LanGame> = Vec::new();
    let mut next_probe = Instant::now();
    let mut buf = [0u8; 128];
    while Instant::now() < deadline {
        // Probe a few times: UDP on Wi-Fi drops packets.
        if Instant::now() >= next_probe {
            for target in targets {
                let _ = socket.send_to(PROBE, target);
            }
            next_probe = Instant::now() + Duration::from_millis(400);
        }
        if let Ok((n, from)) = socket.recv_from(&mut buf) {
            if let Some(game) = parse_reply(from, &buf[..n]) {
                match found.iter_mut().find(|g| g.addr == from) {
                    Some(known) => *known = game,
                    None => found.push(game),
                }
            }
        }
    }
    Ok(found)
}

/// Where probes go on a normal LAN: the limited broadcast address.
#[must_use]
pub fn broadcast_target() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DEFAULT_PORT)
}

/// This device's address on the local network, to show players where to
/// join. Opening a UDP socket "towards" an address sends nothing; it only
/// makes the OS pick the outgoing interface.
#[must_use]
pub fn local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).ok()?;
    // TEST-NET-1 (RFC 5737): never routed anywhere, nothing is sent.
    socket
        .connect(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 9))
        .ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn replies_round_trip_and_junk_is_ignored() {
        let addr: SocketAddr = "192.168.1.20:5029".parse().unwrap();
        let bytes = reply(1, 2, 0x1234_5678_9abc_def0, "MAP01");
        assert_eq!(
            parse_reply(addr, &bytes),
            Some(LanGame {
                addr,
                players: 1,
                max_players: 2,
                fingerprint: 0x1234_5678_9abc_def0,
                map: "MAP01".into(),
            })
        );
        assert_eq!(parse_reply(addr, b"RMLANA1\x01"), None, "too short");
        assert_eq!(parse_reply(addr, b"hello"), None);
        assert!(is_probe(PROBE) && !is_probe(b"RMLANQ2"));
        // Long map names are cut, so a reply always fits one packet.
        assert_eq!(
            reply(1, 2, 0, &"X".repeat(100)).len(),
            REPLY.len() + 10 + 32
        );
    }

    #[test]
    fn a_host_answers_probes() {
        // A stand-in host: answers probes the way the UDP lobby does.
        let host = UdpSocket::bind("127.0.0.1:0").unwrap();
        let host_addr = host.local_addr().unwrap();
        let t = std::thread::spawn(move || {
            host.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut buf = [0u8; 64];
            if let Ok((n, from)) = host.recv_from(&mut buf) {
                if is_probe(&buf[..n]) {
                    host.send_to(&reply(1, 4, 7, "E1M1"), from).unwrap();
                }
            }
        });
        let found = discover(&[host_addr], Duration::from_millis(600)).unwrap();
        t.join().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].addr, host_addr);
        assert_eq!((found[0].players, found[0].max_players), (1, 4));
        assert_eq!(found[0].map, "E1M1");
    }
}

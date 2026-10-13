//! Noise Protocol Framework encryption over byte transports (`snow`).
//!
//! Pattern: `Noise_XX_25519_ChaChaPoly_BLAKE2s` — mutual ephemeral handshake,
//! then AEAD on every payload. Wraps any [`Transport`] (UDP / TCP / relay).

use crate::{NetError, PeerId, Transport};
use snow::{Builder, HandshakeState, TransportState};
use std::collections::BTreeMap;

/// Noise pattern used by Ferrite netplay.
pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

const TAG_HANDSHAKE: u8 = 1;
const TAG_DATA: u8 = 2;

/// Static/ephemeral private key (32 bytes).
#[derive(Clone)]
pub struct NoiseKeys {
    /// 32-byte private key.
    pub private: [u8; 32],
}

impl NoiseKeys {
    /// Fresh keypair via snow's default resolver (pure Rust).
    pub fn generate() -> Result<Self, NetError> {
        let pattern = NOISE_PATTERN
            .parse()
            .map_err(|e| NetError::Reject(format!("noise pattern: {e}")))?;
        let kp = Builder::new(pattern)
            .generate_keypair()
            .map_err(|e| NetError::Reject(format!("noise keygen: {e}")))?;
        let mut private = [0u8; 32];
        private.copy_from_slice(&kp.private);
        Ok(Self { private })
    }
}

enum PeerNoise {
    Handshaking {
        state: Box<HandshakeState>,
        outbox: Vec<Vec<u8>>,
    },
    Ready {
        transport: Box<TransportState>,
    },
}

/// Encrypts payloads after a Noise XX handshake with each peer.
pub struct NoiseTransport<T: Transport> {
    inner: T,
    self_id: PeerId,
    keys: NoiseKeys,
    sessions: BTreeMap<PeerId, PeerNoise>,
    plaintext_inbox: Vec<(PeerId, Vec<u8>)>,
}

impl<T: Transport> NoiseTransport<T> {
    /// Wrap `inner` for local seat `self_id`.
    pub fn new(inner: T, self_id: PeerId, keys: NoiseKeys) -> Self {
        Self {
            inner,
            self_id,
            keys,
            sessions: BTreeMap::new(),
            plaintext_inbox: Vec::new(),
        }
    }

    /// Underlying transport (UDP accept / TCP pump / etc.).
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Whether the Noise session with `peer` is in AEAD transport mode.
    #[must_use]
    pub fn is_peer_ready(&self, peer: PeerId) -> bool {
        matches!(self.sessions.get(&peer), Some(PeerNoise::Ready { .. }))
    }

    /// True when every remote in `peers` has finished the XX handshake.
    #[must_use]
    pub fn handshake_complete(&self, peers: &[PeerId]) -> bool {
        peers
            .iter()
            .filter(|p| **p != self.self_id)
            .all(|p| self.is_peer_ready(*p))
    }

    /// Drive XX handshake with all remotes until ready or `deadline`.
    ///
    /// On lossy transports (UDP) the initiator retries by rebuilding the session
    /// when no progress is made for ~200 ms.
    pub fn complete_handshake(
        &mut self,
        peers: &[PeerId],
        deadline: std::time::Instant,
    ) -> Result<(), NetError> {
        for &p in peers {
            if p != self.self_id {
                self.ensure_session(p)?;
            }
        }
        self.flush_outbox();
        let mut sink = Vec::new();
        let mut last_retry = std::time::Instant::now();
        while !self.handshake_complete(peers) {
            if std::time::Instant::now() > deadline {
                return Err(NetError::Reject("noise handshake timeout".into()));
            }
            if last_retry.elapsed() > std::time::Duration::from_millis(200) {
                let remotes: Vec<PeerId> = peers
                    .iter()
                    .copied()
                    .filter(|p| *p != self.self_id)
                    .collect();
                for peer in remotes {
                    if self.is_peer_ready(peer) {
                        continue;
                    }
                    // Only the lower id initiates; rebuild so the first XX message
                    // is re-sent after UDP loss.
                    if self.self_id < peer {
                        self.sessions.remove(&peer);
                        self.ensure_session(peer)?;
                    }
                }
                last_retry = std::time::Instant::now();
            }
            self.flush_outbox();
            self.poll(&mut sink);
            sink.clear();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(())
    }

    fn builder(&self) -> Result<Builder<'_>, NetError> {
        let pattern = NOISE_PATTERN
            .parse()
            .map_err(|e| NetError::Reject(format!("noise pattern: {e}")))?;
        Builder::new(pattern)
            .local_private_key(&self.keys.private)
            .map_err(|e| NetError::Reject(format!("noise key: {e}")))
    }

    /// Start handshake with `peer` if needed. Lower [`PeerId`] initiates.
    pub fn ensure_session(&mut self, peer: PeerId) -> Result<(), NetError> {
        if peer == self.self_id || self.sessions.contains_key(&peer) {
            return Ok(());
        }
        let initiator = self.self_id < peer;
        let state = if initiator {
            self.builder()?
                .build_initiator()
                .map_err(|e| NetError::Reject(format!("noise init: {e}")))?
        } else {
            self.builder()?
                .build_responder()
                .map_err(|e| NetError::Reject(format!("noise resp: {e}")))?
        };
        let mut outbox = Vec::new();
        if initiator {
            let mut buf = vec![0u8; 1024];
            // Temporary state for first message — rebuild via ensure after insert.
            let mut state = state;
            let n = state
                .write_message(&[], &mut buf)
                .map_err(|e| NetError::Reject(format!("noise hs write: {e}")))?;
            outbox.push(wrap_hs(&buf[..n]));
            self.sessions.insert(
                peer,
                PeerNoise::Handshaking {
                    state: Box::new(state),
                    outbox,
                },
            );
        } else {
            self.sessions.insert(
                peer,
                PeerNoise::Handshaking {
                    state: Box::new(state),
                    outbox,
                },
            );
        }
        Ok(())
    }

    fn flush_outbox(&mut self) {
        let peers: Vec<PeerId> = self.sessions.keys().copied().collect();
        for peer in peers {
            let msgs = match self.sessions.get_mut(&peer) {
                Some(PeerNoise::Handshaking { outbox, .. }) => std::mem::take(outbox),
                _ => continue,
            };
            for m in msgs {
                self.inner.send(peer, &m);
            }
        }
    }

    fn on_handshake(&mut self, peer: PeerId, payload: &[u8]) -> Result<(), NetError> {
        self.ensure_session(peer)?;
        let Some(entry) = self.sessions.remove(&peer) else {
            return Ok(());
        };
        let PeerNoise::Handshaking {
            mut state,
            mut outbox,
        } = entry
        else {
            self.sessions.insert(peer, entry);
            return Ok(());
        };
        let mut buf = vec![0u8; 1024];
        state
            .read_message(payload, &mut buf)
            .map_err(|e| NetError::Reject(format!("noise hs read: {e}")))?;
        if !state.is_handshake_finished() {
            let mut out = vec![0u8; 1024];
            if let Ok(n) = state.write_message(&[], &mut out) {
                outbox.push(wrap_hs(&out[..n]));
            }
        }
        if state.is_handshake_finished() {
            for m in outbox {
                self.inner.send(peer, &m);
            }
            let transport = (*state)
                .into_transport_mode()
                .map_err(|e| NetError::Reject(format!("noise transport: {e}")))?;
            self.sessions.insert(
                peer,
                PeerNoise::Ready {
                    transport: Box::new(transport),
                },
            );
            return Ok(());
        }
        self.sessions
            .insert(peer, PeerNoise::Handshaking { state, outbox });
        Ok(())
    }

    fn on_data(&mut self, peer: PeerId, payload: &[u8]) -> Result<(), NetError> {
        let Some(PeerNoise::Ready { transport }) = self.sessions.get_mut(&peer) else {
            return Ok(());
        };
        let mut buf = vec![0u8; payload.len().saturating_add(16)];
        let n = transport
            .read_message(payload, &mut buf)
            .map_err(|e| NetError::Reject(format!("noise decrypt: {e}")))?;
        self.plaintext_inbox.push((peer, buf[..n].to_vec()));
        Ok(())
    }
}

fn wrap_hs(msg: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + msg.len());
    v.push(TAG_HANDSHAKE);
    v.extend_from_slice(msg);
    v
}

fn wrap_data(msg: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + msg.len());
    v.push(TAG_DATA);
    v.extend_from_slice(msg);
    v
}

impl<T: Transport> Transport for NoiseTransport<T> {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        let _ = self.ensure_session(peer);
        self.flush_outbox();
        let enc = match self.sessions.get_mut(&peer) {
            Some(PeerNoise::Ready { transport }) => {
                let mut buf = vec![0u8; bytes.len().saturating_add(32)];
                match transport.write_message(bytes, &mut buf) {
                    Ok(n) => Some(wrap_data(&buf[..n])),
                    Err(_) => None,
                }
            }
            _ => None,
        };
        if let Some(msg) = enc {
            self.inner.send(peer, &msg);
        }
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        let mut raw = Vec::new();
        self.inner.poll(&mut raw);
        for (peer, bytes) in raw {
            if bytes.is_empty() {
                continue;
            }
            match bytes[0] {
                TAG_HANDSHAKE => {
                    let _ = self.on_handshake(peer, &bytes[1..]);
                }
                TAG_DATA => {
                    let _ = self.on_data(peer, &bytes[1..]);
                }
                _ => {}
            }
        }
        self.flush_outbox();
        out.append(&mut self.plaintext_inbox);
    }

    fn peers(&self) -> &[PeerId] {
        self.inner.peers()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn noise_xx_handshake_and_aead_roundtrip() {
        let k0 = NoiseKeys::generate().unwrap();
        let k1 = NoiseKeys::generate().unwrap();
        let pattern = NOISE_PATTERN.parse().unwrap();
        let mut init = Builder::new(pattern)
            .local_private_key(&k0.private)
            .unwrap()
            .build_initiator()
            .unwrap();
        let pattern = NOISE_PATTERN.parse().unwrap();
        let mut resp = Builder::new(pattern)
            .local_private_key(&k1.private)
            .unwrap()
            .build_responder()
            .unwrap();
        let mut buf_i = [0u8; 1024];
        let mut buf_r = [0u8; 1024];
        let n = init.write_message(&[], &mut buf_i).unwrap();
        let _ = resp.read_message(&buf_i[..n], &mut buf_r).unwrap();
        let n = resp.write_message(&[], &mut buf_r).unwrap();
        let _ = init.read_message(&buf_r[..n], &mut buf_i).unwrap();
        let n = init.write_message(&[], &mut buf_i).unwrap();
        let _ = resp.read_message(&buf_i[..n], &mut buf_r).unwrap();
        assert!(init.is_handshake_finished());
        assert!(resp.is_handshake_finished());
        let mut t0 = init.into_transport_mode().unwrap();
        let mut t1 = resp.into_transport_mode().unwrap();
        let mut ct = [0u8; 256];
        let n = t0.write_message(b"ferrite", &mut ct).unwrap();
        let mut pt = [0u8; 256];
        let m = t1.read_message(&ct[..n], &mut pt).unwrap();
        assert_eq!(&pt[..m], b"ferrite");
    }
}

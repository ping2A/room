//! str0m UDP SFU loop (`feature = "rtp"`): demux by `Rtc::accepts`, forward
//! `MediaData` to other seats unless server-muted.

use crate::SfuError;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use str0m::media::MediaKind;
use str0m::net::{Protocol, Receive};
use str0m::{Event, Input, Output, Rtc};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// One seat's WebRTC endpoint inside the process SFU.
struct SeatPeer {
    room_id: String,
    seat: u8,
    rtc: Rtc,
    muted_audio: bool,
    muted_video: bool,
    /// First audio mid seen (for Writer::write fan-out).
    audio_mid: Option<str0m::media::Mid>,
}

/// Shared UDP SFU for many rooms / seats.
pub struct UdpSfu {
    sock: Arc<UdpSocket>,
    peers: Mutex<BTreeMap<(String, u8), SeatPeer>>,
}

impl UdpSfu {
    /// Bind a single UDP port for all rooms.
    ///
    /// # Errors
    ///
    /// Bind failure.
    pub async fn bind(addr: SocketAddr) -> Result<Self, std::io::Error> {
        let sock = UdpSocket::bind(addr).await?;
        info!(%addr, "room-sfu UDP listening");
        Ok(Self {
            sock: Arc::new(sock),
            peers: Mutex::new(BTreeMap::new()),
        })
    }

    /// Local socket address.
    ///
    /// # Errors
    ///
    /// OS error.
    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.sock.local_addr()
    }

    /// Create a default `Rtc` for a new publisher (DTLS-SRTP).
    #[must_use]
    pub fn new_rtc() -> Rtc {
        Rtc::new(Instant::now())
    }

    /// Register (or replace) a seat peer.
    pub async fn register(&self, room_id: impl Into<String>, seat: u8, rtc: Rtc) {
        let room_id = room_id.into();
        let mut peers = self.peers.lock().await;
        peers.insert(
            (room_id.clone(), seat),
            SeatPeer {
                room_id,
                seat,
                rtc,
                muted_audio: false,
                muted_video: false,
                audio_mid: None,
            },
        );
    }

    /// Apply server mute for a seat (stops media fan-out).
    pub async fn set_mute(
        &self,
        room_id: &str,
        seat: u8,
        audio: bool,
        video: bool,
    ) -> Result<(), SfuError> {
        let mut peers = self.peers.lock().await;
        let peer = peers
            .get_mut(&(room_id.to_owned(), seat))
            .ok_or(SfuError::UnknownSeat(seat))?;
        peer.muted_audio = audio;
        peer.muted_video = video;
        Ok(())
    }

    /// Whether audio from this seat is currently forwardable.
    pub async fn forward_audio(&self, room_id: &str, seat: u8) -> bool {
        let peers = self.peers.lock().await;
        peers
            .get(&(room_id.to_owned(), seat))
            .is_some_and(|p| !p.muted_audio)
    }

    /// Drive the SFU forever: receive UDP → `handle_input` → drain `poll_output`
    /// (transmit + media fan-out with mute).
    pub async fn run(self: Arc<Self>) {
        let mut buf = vec![0u8; 2048];
        let dest = match self.sock.local_addr() {
            Ok(a) => a,
            Err(e) => {
                warn!(?e, "sfu local_addr");
                return;
            }
        };
        loop {
            let recv =
                tokio::time::timeout(Duration::from_millis(50), self.sock.recv_from(&mut buf))
                    .await;
            let now = Instant::now();
            match recv {
                Ok(Ok((n, source))) => {
                    let pkt = buf[..n].to_vec();
                    let input = match (&pkt[..]).try_into() {
                        Ok(contents) => Input::Receive(
                            now,
                            Receive {
                                proto: Protocol::Udp,
                                source,
                                destination: dest,
                                contents,
                            },
                        ),
                        Err(_) => continue,
                    };
                    self.drive_input(input).await;
                }
                Ok(Err(e)) => {
                    warn!(?e, "sfu udp recv");
                    break;
                }
                Err(_) => {
                    // Periodic timeout tick for all peers.
                    self.drive_timeout(now).await;
                }
            }
        }
    }

    async fn drive_timeout(&self, now: Instant) {
        let keys: Vec<(String, u8)> = {
            let peers = self.peers.lock().await;
            peers.keys().cloned().collect()
        };
        for key in keys {
            let input = Input::Timeout(now);
            self.drive_one(&key, input).await;
        }
    }

    async fn drive_input(&self, input: Input<'_>) {
        // Find accepting peer (may need owned packet for lifetime — clone contents).
        let key = {
            let peers = self.peers.lock().await;
            peers
                .iter()
                .find(|(_, p)| p.rtc.accepts(&input))
                .map(|(k, _)| k.clone())
        };
        let Some(key) = key else {
            debug!("sfu: no peer accepts packet");
            return;
        };
        self.drive_one(&key, input).await;
    }

    async fn drive_one(&self, key: &(String, u8), input: Input<'_>) {
        let mut peers = self.peers.lock().await;
        let Some(peer) = peers.get_mut(key) else {
            return;
        };
        if let Err(e) = peer.rtc.handle_input(input) {
            warn!(?e, seat = peer.seat, "handle_input");
            return;
        }
        // Drain outputs; collect media to forward after releasing exclusive rtc borrow.
        let mut media_out: Vec<ForwardMedia> = Vec::new();
        let room = peer.room_id.clone();
        let from_seat = peer.seat;
        let muted_audio = peer.muted_audio;
        let muted_video = peer.muted_video;
        loop {
            match peer.rtc.poll_output() {
                Ok(Output::Timeout(_)) => break,
                Ok(Output::Transmit(t)) => {
                    let _ = self.sock.try_send_to(&t.contents, t.destination);
                }
                Ok(Output::Event(Event::MediaAdded(m))) => {
                    if m.kind == MediaKind::Audio {
                        peer.audio_mid = Some(m.mid);
                    }
                }
                Ok(Output::Event(Event::MediaData(d))) => {
                    let is_audio = d.params.spec().codec.is_audio();
                    if should_forward(muted_audio, muted_video, is_audio) {
                        media_out.push(ForwardMedia {
                            room: room.clone(),
                            from_seat,
                            mid_hint: d.mid,
                            pt: d.pt,
                            time: d.time,
                            network_time: d.network_time,
                            data: Arc::clone(&d.data),
                            is_audio,
                        });
                    }
                }
                Ok(Output::Event(_)) => {}
                Err(e) => {
                    warn!(?e, "poll_output");
                    break;
                }
            }
        }
        drop(peers);
        for m in media_out {
            self.fanout_media(m).await;
        }
    }

    async fn fanout_media(&self, m: ForwardMedia) {
        let mut peers = self.peers.lock().await;
        let targets: Vec<(String, u8)> = peers
            .keys()
            .filter(|(r, s)| r == &m.room && *s != m.from_seat)
            .cloned()
            .collect();
        for key in targets {
            let Some(peer) = peers.get_mut(&key) else {
                continue;
            };
            // Prefer the publisher mid; fall back to the peer's known audio mid.
            let mid = if m.is_audio {
                peer.audio_mid.unwrap_or(m.mid_hint)
            } else {
                m.mid_hint
            };
            if let Some(w) = peer.rtc.writer(mid) {
                if let Err(e) = w.write(m.pt, m.network_time, m.time, Arc::clone(&m.data)) {
                    debug!(?e, seat = peer.seat, "forward write");
                }
            }
            // Drain transmit after write mutation.
            loop {
                match peer.rtc.poll_output() {
                    Ok(Output::Timeout(_)) => break,
                    Ok(Output::Transmit(t)) => {
                        let _ = self.sock.try_send_to(&t.contents, t.destination);
                    }
                    Ok(Output::Event(_)) => {}
                    Err(_) => break,
                }
            }
        }
    }
}

/// Pure mute gate used by the UDP loop (and unit-tested without RTP).
#[must_use]
pub fn should_forward(muted_audio: bool, muted_video: bool, is_audio: bool) -> bool {
    if is_audio { !muted_audio } else { !muted_video }
}

struct ForwardMedia {
    room: String,
    from_seat: u8,
    mid_hint: str0m::media::Mid,
    pt: str0m::media::Pt,
    time: str0m::media::MediaTime,
    network_time: Instant,
    data: Arc<[u8]>,
    is_audio: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn new_rtc_ok() {
        let _ = UdpSfu::new_rtc();
    }

    #[tokio::test]
    async fn mute_blocks_forward_flag() {
        let sfu = Arc::new(UdpSfu::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
        let _ = sfu.local_addr().unwrap();
        sfu.register("r", 0, UdpSfu::new_rtc()).await;
        sfu.register("r", 1, UdpSfu::new_rtc()).await;
        assert!(sfu.forward_audio("r", 0).await);
        sfu.set_mute("r", 0, true, false).await.unwrap();
        assert!(!sfu.forward_audio("r", 0).await);
        // Packet-capture stand-in: mute gate must drop audio before fan-out.
        assert!(!should_forward(true, false, true));
        assert!(should_forward(true, false, false));
        assert!(!should_forward(false, true, false));
    }
}

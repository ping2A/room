//! Protocol v2 WebSocket client (`FTR2` + JSON control; docs/22 §4).

use js_sys::{ArrayBuffer, Uint8Array};
use room_net::{NetTicCmd as NetNetTicCmd, PeerId, TimedCmd, Transport};
use room_proto::{
    BinaryMsg, ControlMsg, HelloMsg, NetTicCmd, RosterSeat, TicCmdMsg, TicFrameMsg, WelcomeMsg,
    WelcomeRoom, encode_binary, encode_control, parse_binary, parse_control,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, MessageEvent, WebSocket};

use crate::ws_net::PresenceEvent;

type Outbox = Rc<RefCell<VecDeque<OutMsg>>>;

enum OutMsg {
    Text(String),
    Binary(Vec<u8>),
}

struct Shared {
    self_id: PeerId,
    peer_count: u8,
    reject: Option<String>,
    /// Synthetic TimedCmds from TIC_FRAME (for LockstepSession).
    inbox: VecDeque<(PeerId, Vec<u8>)>,
    presence: VecDeque<PresenceEvent>,
    welcome: Option<WelcomeMsg>,
    started: Option<(u32, u32)>,
    resume_token: Option<String>,
    opened: bool,
    closed: bool,
    frames_rx: u32,
    cmds_tx: u32,
    /// Inbound WebRTC signalling (`ControlMsg::Rtc`).
    rtc_inbox: VecDeque<ControlMsg>,
    /// Highest TIC_FRAME tic received.
    last_frame_tic: Option<u32>,
    /// Smoothed round-trip time (ms); `None` until the first PONG.
    rtt_ms: Option<f64>,
    /// Spectator: room info from `watching`.
    watching: Option<(WelcomeRoom, Vec<RosterSeat>)>,
    /// Seats in the last roster (diffed to synthesize leaves).
    roster: std::collections::BTreeSet<u8>,
}

/// v2 game gateway transport.
pub struct WsV2Transport {
    ws: WebSocket,
    shared: Rc<RefCell<Shared>>,
    outbox: Outbox,
    peers: Vec<PeerId>,
    last_ping_ms: f64,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_open: Closure<dyn FnMut(web_sys::Event)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
    _on_error: Closure<dyn FnMut(web_sys::Event)>,
}

impl WsV2Transport {
    /// Connect; send `hello` on open.
    pub fn start(
        url: &str,
        ticket: &str,
        name: &str,
        fingerprint: u64,
        max_players: u8,
        resume_token: Option<String>,
        watch: bool,
    ) -> Result<Self, String> {
        web_sys::console::log_1(&format!("room-web: v2 connecting url={url} name={name:?}").into());
        let ws = WebSocket::new(url).map_err(|e| format!("WebSocket: {e:?}"))?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let shared = Rc::new(RefCell::new(Shared {
            self_id: PeerId(255),
            peer_count: max_players.max(2),
            reject: None,
            inbox: VecDeque::new(),
            presence: VecDeque::new(),
            welcome: None,
            started: None,
            resume_token: None,
            opened: false,
            closed: false,
            frames_rx: 0,
            cmds_tx: 0,
            rtc_inbox: VecDeque::new(),
            last_frame_tic: None,
            rtt_ms: None,
            watching: None,
            roster: std::collections::BTreeSet::new(),
        }));
        let outbox: Outbox = Rc::new(RefCell::new(VecDeque::new()));

        let shared_msg = Rc::clone(&shared);
        let on_message = Closure::wrap(Box::new(move |ev: MessageEvent| {
            if let Ok(buf) = ev.data().dyn_into::<ArrayBuffer>() {
                let bytes = Uint8Array::new(&buf).to_vec();
                on_binary(&shared_msg, &bytes);
            } else if let Some(text) = ev.data().as_string() {
                on_text(&shared_msg, &text);
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let shared_open = Rc::clone(&shared);
        let outbox_open = Rc::clone(&outbox);
        let ws_open = ws.clone();
        let ticket_owned = ticket.to_owned();
        let name_owned = name.to_owned();
        let fp_hex = format!("{fingerprint:016x}");
        let resume = resume_token.clone();
        let on_open = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            shared_open.borrow_mut().opened = true;
            push_presence(
                &mut shared_open.borrow_mut().presence,
                PresenceEvent::SocketOpen,
            );
            let hello = if watch {
                ControlMsg::Watch {
                    iwad_fingerprint: fp_hex.clone(),
                    name: Some(name_owned.clone()),
                    client_version: env!("CARGO_PKG_VERSION").into(),
                }
            } else {
                ControlMsg::Hello(HelloMsg {
                    ticket: ticket_owned.clone(),
                    client_version: env!("CARGO_PKG_VERSION").into(),
                    abi_major: 1,
                    iwad_fingerprint: fp_hex.clone(),
                    name: Some(name_owned.clone()),
                    resume_token: resume.clone(),
                })
            };
            if let Ok(s) = encode_control(&hello) {
                let _ = ws_open.send_with_str(&s);
            }
            let mut q = outbox_open.borrow_mut();
            while let Some(m) = q.pop_front() {
                match m {
                    OutMsg::Text(t) => {
                        let _ = ws_open.send_with_str(&t);
                    }
                    OutMsg::Binary(b) => {
                        let _ = ws_open.send_with_u8_array(&b);
                    }
                }
            }
            web_sys::console::log_1(&"room-web: v2 hello sent".into());
        }) as Box<dyn FnMut(web_sys::Event)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let shared_close = Rc::clone(&shared);
        let on_close = Closure::wrap(Box::new(move |ev: CloseEvent| {
            shared_close.borrow_mut().closed = true;
            push_presence(
                &mut shared_close.borrow_mut().presence,
                PresenceEvent::SocketClose {
                    code: ev.code(),
                    reason: ev.reason(),
                },
            );
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        let shared_err = Rc::clone(&shared);
        let on_error = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            push_presence(
                &mut shared_err.borrow_mut().presence,
                PresenceEvent::SocketError,
            );
        }) as Box<dyn FnMut(web_sys::Event)>);
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        let _ = (ticket, name, resume_token, watch);
        Ok(Self {
            ws,
            shared,
            outbox,
            peers: (0..max_players.max(2)).map(PeerId).collect(),
            last_ping_ms: f64::NEG_INFINITY,
            _on_message: on_message,
            _on_open: on_open,
            _on_close: on_close,
            _on_error: on_error,
        })
    }

    /// `Some(seat)` once welcome arrives; `Err` on reject.
    pub fn try_seat(&self) -> Result<Option<PeerId>, String> {
        let s = self.shared.borrow();
        if let Some(r) = &s.reject {
            return Err(r.clone());
        }
        if s.self_id.0 != 255 {
            Ok(Some(s.self_id))
        } else {
            Ok(None)
        }
    }

    /// True after the socket closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.borrow().closed
    }

    /// Debug counters (frames rx, cmds tx).
    #[must_use]
    pub fn data_stats(&self) -> (u32, u32) {
        let s = self.shared.borrow();
        (s.frames_rx, s.cmds_tx)
    }

    /// Highest server frame tic received so far.
    #[must_use]
    pub fn last_frame_tic(&self) -> Option<u32> {
        self.shared.borrow().last_frame_tic
    }

    /// Smoothed RTT to the server in milliseconds.
    #[must_use]
    pub fn rtt_ms(&self) -> Option<f64> {
        self.shared.borrow().rtt_ms
    }

    /// Ping every few seconds so lobby / spectator sockets beat the server
    /// idle timeout (30 s by default) while nothing else is sent.
    pub fn keepalive(&mut self) {
        if now_ms() - self.last_ping_ms >= 5_000.0 {
            self.send_ping();
        }
    }

    /// Send a PING; the nonce carries the send time so PONG needs no state.
    pub fn send_ping(&mut self) {
        self.last_ping_ms = now_ms();
        let nonce = (self.last_ping_ms * 1000.0) as u64;
        self.send_bin(encode_binary(&BinaryMsg::Ping {
            nonce,
            server_time_us: 0,
        }));
    }

    /// Update local peer list size.
    pub fn set_peer_count(&mut self, peer_count: u8) {
        self.peers = (0..peer_count.max(2)).map(PeerId).collect();
        self.shared.borrow_mut().peer_count = peer_count.max(2);
    }

    /// Take welcome once.
    pub fn take_welcome(&mut self) -> Option<WelcomeMsg> {
        let mut s = self.shared.borrow_mut();
        if let Some(w) = s.welcome.take() {
            s.self_id = PeerId(w.seat);
            s.peer_count = w.room.max_players;
            s.resume_token = Some(w.resume_token.clone());
            self.peers = (0..w.room.max_players).map(PeerId).collect();
            Some(w)
        } else {
            None
        }
    }

    /// Spectator room info once `watching` arrives.
    pub fn take_watching(&mut self) -> Option<(WelcomeRoom, Vec<RosterSeat>)> {
        self.shared.borrow_mut().watching.take()
    }

    /// `(tic0, rng_seed)` when server Start arrives.
    pub fn take_start(&mut self) -> Option<(u32, u32)> {
        self.shared.borrow_mut().started.take()
    }

    /// Fatal reject reason.
    pub fn take_reject(&mut self) -> Option<String> {
        self.shared.borrow_mut().reject.take()
    }

    /// Drain presence events.
    pub fn poll_presence(&mut self, out: &mut Vec<PresenceEvent>) {
        let mut s = self.shared.borrow_mut();
        while let Some(p) = s.presence.pop_front() {
            out.push(p);
        }
    }

    /// Send JSON `ready`.
    pub fn send_ready(&mut self) {
        self.send_control(&ControlMsg::Ready {});
    }

    /// Send admin telemetry pose.
    pub fn send_pose(&mut self, x: i32, y: i32, angle: i16) {
        self.send_control(&ControlMsg::Pose { x, y, angle });
    }

    /// Send sparse map outline for the admin minimap.
    pub fn send_map_outline(&mut self, segs: Vec<i16>) {
        if segs.is_empty() || segs.len() > 800 || segs.len() % 4 != 0 {
            return;
        }
        self.send_control(&ControlMsg::MapOutline { segs });
    }

    /// Send a WebRTC signalling control message.
    pub fn send_rtc(&mut self, msg: &ControlMsg) {
        if matches!(msg, ControlMsg::Rtc { .. }) {
            self.send_control(msg);
        }
    }

    /// Drain inbound `rtc` control messages.
    pub fn poll_rtc(&mut self, out: &mut Vec<ControlMsg>) {
        let mut s = self.shared.borrow_mut();
        while let Some(m) = s.rtc_inbox.pop_front() {
            out.push(m);
        }
    }

    /// Send a TIC_CMD for `tic` (seat stamped server-side).
    pub fn send_tic_cmd(&mut self, tic: u32, cmd: &NetNetTicCmd) {
        let msg = BinaryMsg::TicCmd(TicCmdMsg {
            tic,
            cmd: NetTicCmd {
                buttons: cmd.buttons,
                pitch: cmd.pitch,
                yaw: cmd.yaw,
                roll: cmd.roll,
                forward_move: cmd.forward_move,
                side_move: cmd.side_move,
                up_move: cmd.up_move,
                weapon_select: cmd.weapon_select,
            },
        });
        let bytes = encode_binary(&msg);
        {
            let mut s = self.shared.borrow_mut();
            s.cmds_tx = s.cmds_tx.saturating_add(1);
        }
        self.send_bin(bytes);
    }

    fn send_control(&mut self, msg: &ControlMsg) {
        let Ok(s) = encode_control(msg) else {
            return;
        };
        if self.shared.borrow().opened {
            let _ = self.ws.send_with_str(&s);
        } else {
            self.outbox.borrow_mut().push_back(OutMsg::Text(s));
        }
    }

    fn send_bin(&mut self, bytes: Vec<u8>) {
        if self.shared.borrow().opened {
            let _ = self.ws.send_with_u8_array(&bytes);
        } else {
            self.outbox.borrow_mut().push_back(OutMsg::Binary(bytes));
        }
    }
}

impl Transport for WsV2Transport {
    fn send(&mut self, _peer: PeerId, bytes: &[u8]) {
        // Lockstep uplink: TimedCmd bytes → TIC_CMD (ignore peer byte).
        if let Ok(t) = room_net::decode_timed_cmd(bytes) {
            self.send_tic_cmd(t.tic, &t.cmd);
        }
    }

    fn poll(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        let mut s = self.shared.borrow_mut();
        while let Some(p) = s.inbox.pop_front() {
            out.push(p);
        }
    }

    fn peers(&self) -> &[PeerId] {
        &self.peers
    }
}

fn on_text(shared: &Rc<RefCell<Shared>>, text: &str) {
    let Ok(msg) = parse_control(text) else {
        return;
    };
    match msg {
        ControlMsg::Welcome(w) => {
            let seat = w.seat;
            let peers = w.room.max_players;
            {
                let mut s = shared.borrow_mut();
                s.self_id = PeerId(seat);
                s.peer_count = peers;
                s.resume_token = Some(w.resume_token.clone());
                s.roster = w.roster.iter().map(|r| r.seat).collect();
                s.welcome = Some(w);
            }
            push_presence(
                &mut shared.borrow_mut().presence,
                PresenceEvent::Welcome {
                    seat: PeerId(seat),
                    peer_count: peers,
                },
            );
        }
        ControlMsg::Watching { room, roster } => {
            let mut s = shared.borrow_mut();
            s.peer_count = room.max_players.max(2);
            s.roster = roster.iter().map(|r| r.seat).collect();
            for seat in &roster {
                push_presence(
                    &mut s.presence,
                    PresenceEvent::PeerJoin {
                        seat: PeerId(seat.seat),
                        name: seat.name.clone(),
                    },
                );
            }
            s.watching = Some((room, roster));
        }
        ControlMsg::Start { tic0, rng_seed } => {
            shared.borrow_mut().started = Some((tic0, rng_seed));
        }
        ControlMsg::Reject(r) => {
            shared.borrow_mut().reject = Some(format!("{}: {}", r.code, r.reason));
        }
        ControlMsg::Roster { seats } => {
            let mut s = shared.borrow_mut();
            let now: std::collections::BTreeSet<u8> = seats.iter().map(|r| r.seat).collect();
            let gone: Vec<u8> = s.roster.difference(&now).copied().collect();
            for seat in gone {
                push_presence(
                    &mut s.presence,
                    PresenceEvent::PeerLeave { seat: PeerId(seat) },
                );
            }
            s.roster = now;
            for seat in seats {
                push_presence(
                    &mut s.presence,
                    PresenceEvent::PeerJoin {
                        seat: PeerId(seat.seat),
                        name: seat.name,
                    },
                );
            }
        }
        ControlMsg::Bye { reason, .. } => {
            shared.borrow_mut().reject = Some(reason);
        }
        ControlMsg::Rtc { .. } => {
            let mut s = shared.borrow_mut();
            if s.rtc_inbox.len() < 64 {
                s.rtc_inbox.push_back(msg);
            }
        }
        _ => {}
    }
}

fn on_binary(shared: &Rc<RefCell<Shared>>, bytes: &[u8]) {
    let Ok(msg) = parse_binary(bytes) else {
        return;
    };
    match msg {
        BinaryMsg::TicFrame(frame) => ingest_frame(shared, frame),
        BinaryMsg::TicBatch(frames) => {
            for frame in frames {
                ingest_frame(shared, frame);
            }
        }
        BinaryMsg::Pong { nonce, .. } => {
            let sample = now_ms() - nonce as f64 / 1000.0;
            if (0.0..10_000.0).contains(&sample) {
                let mut s = shared.borrow_mut();
                s.rtt_ms = Some(match s.rtt_ms {
                    Some(prev) => prev * 0.8 + sample * 0.2,
                    None => sample,
                });
            }
        }
        _ => {}
    }
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

fn ingest_frame(shared: &Rc<RefCell<Shared>>, frame: TicFrameMsg) {
    let mut s = shared.borrow_mut();
    s.frames_rx = s.frames_rx.saturating_add(1);
    s.last_frame_tic = Some(s.last_frame_tic.map_or(frame.tic, |t| t.max(frame.tic)));
    let mut idx = 0usize;
    for seat in 0..8u8 {
        if (frame.seat_mask & (1 << seat)) == 0 {
            continue;
        }
        let Some(cmd) = frame.cmds.get(idx) else {
            break;
        };
        idx += 1;
        // The server's copy of our own cmd is authoritative (it may be a
        // repeat-fill if ours arrived late), so every seat goes through here.
        let timed = TimedCmd {
            tic: frame.tic,
            peer: PeerId(seat),
            cmd: NetNetTicCmd {
                buttons: cmd.buttons,
                pitch: cmd.pitch,
                yaw: cmd.yaw,
                roll: cmd.roll,
                forward_move: cmd.forward_move,
                side_move: cmd.side_move,
                up_move: cmd.up_move,
                weapon_select: cmd.weapon_select,
            },
        };
        let enc = room_net::encode_timed_cmd(&timed);
        s.inbox.push_back((PeerId(seat), enc));
    }
}

fn push_presence(q: &mut VecDeque<PresenceEvent>, ev: PresenceEvent) {
    if q.len() >= 64 {
        let _ = q.pop_front();
    }
    q.push_back(ev);
}

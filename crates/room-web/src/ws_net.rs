//! Browser WebSocket client for the `room-ws` packet-server relay.

use js_sys::{ArrayBuffer, Uint8Array};
use room_net::{
    MSG_DATA, MSG_PEER_JOIN, MSG_PEER_LEAVE, MSG_REJECT, MSG_VOICE, MSG_WELCOME, PeerId, Transport,
    pack_client_data, pack_client_voice, pack_hello, parse_peer_join, parse_peer_leave,
    parse_relay,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, MessageEvent, WebSocket};

type Outbox = Rc<RefCell<VecDeque<Vec<u8>>>>;

/// Relay presence / socket lifecycle event for console + netplay.
#[derive(Clone, Debug)]
pub enum PresenceEvent {
    /// Local WebSocket opened; HELLO sent.
    SocketOpen,
    /// Relay assigned our seat.
    Welcome { seat: PeerId, peer_count: u8 },
    /// Another peer joined (or roster snapshot).
    PeerJoin { seat: PeerId, name: String },
    /// Peer disconnected from the relay.
    PeerLeave { seat: PeerId },
    /// Local WebSocket closed.
    SocketClose { code: u16, reason: String },
    /// Local WebSocket error.
    SocketError,
}

struct WsShared {
    self_id: PeerId,
    peer_count: u8,
    reject: Option<String>,
    inbox: VecDeque<(PeerId, Vec<u8>)>,
    voice_inbox: VecDeque<(PeerId, Vec<u8>)>,
    presence: VecDeque<PresenceEvent>,
    opened: bool,
    closed: bool,
    data_rx: u32,
    data_tx: u32,
}

/// WebSocket relay transport used by web lockstep coop.
pub struct WsTransport {
    ws: WebSocket,
    shared: Rc<RefCell<WsShared>>,
    outbox: Outbox,
    peers: Vec<PeerId>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_open: Closure<dyn FnMut(web_sys::Event)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
    _on_error: Closure<dyn FnMut(web_sys::Event)>,
}

impl WsTransport {
    /// Begin connecting; HELLO is sent on `onopen`.
    pub fn start(url: &str, name: &str, max_players: u8) -> Result<Self, String> {
        web_sys::console::log_1(
            &format!("room-web: ws connecting url={url} name={name:?} players={max_players}")
                .into(),
        );
        let ws = WebSocket::new(url).map_err(|e| format!("WebSocket: {e:?}"))?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let shared = Rc::new(RefCell::new(WsShared {
            self_id: PeerId(255),
            peer_count: max_players.max(2),
            reject: None,
            inbox: VecDeque::new(),
            voice_inbox: VecDeque::new(),
            presence: VecDeque::new(),
            opened: false,
            closed: false,
            data_rx: 0,
            data_tx: 0,
        }));
        let outbox: Outbox = Rc::new(RefCell::new(VecDeque::new()));

        let shared_msg = shared.clone();
        let on_message = Closure::wrap(Box::new(move |ev: MessageEvent| {
            let Ok(buf) = ev.data().dyn_into::<ArrayBuffer>() else {
                return;
            };
            let bytes = Uint8Array::new(&buf).to_vec();
            let mut s = shared_msg.borrow_mut();
            match parse_relay(&bytes) {
                Ok((MSG_WELCOME, rest)) if rest.len() >= 2 => {
                    s.self_id = PeerId(rest[0]);
                    s.peer_count = rest[1].max(2);
                    web_sys::console::log_1(
                        &format!(
                            "room-web: ws WELCOME seat={} peer_count={}",
                            rest[0], rest[1]
                        )
                        .into(),
                    );
                    push_presence(
                        &mut s.presence,
                        PresenceEvent::Welcome {
                            seat: PeerId(rest[0]),
                            peer_count: rest[1],
                        },
                    );
                }
                Ok((MSG_REJECT, rest)) => {
                    let reason = if rest.len() >= 2 {
                        let n = u16::from_le_bytes([rest[0], rest[1]]) as usize;
                        String::from_utf8_lossy(&rest[2..2 + n.min(rest.len().saturating_sub(2))])
                            .into_owned()
                    } else {
                        "rejected".into()
                    };
                    web_sys::console::error_1(&format!("room-web: ws REJECT {reason}").into());
                    s.reject = Some(reason);
                }
                Ok((MSG_PEER_JOIN, rest)) => match parse_peer_join(rest) {
                    Ok((seat, name)) => {
                        web_sys::console::log_1(
                            &format!("room-web: peer JOIN seat={} name={name:?}", seat.0).into(),
                        );
                        push_presence(&mut s.presence, PresenceEvent::PeerJoin { seat, name });
                    }
                    Err(_) => {
                        web_sys::console::warn_1(&"room-web: ws bad PEER_JOIN payload".into());
                    }
                },
                Ok((MSG_PEER_LEAVE, rest)) => match parse_peer_leave(rest) {
                    Ok(seat) => {
                        web_sys::console::warn_1(
                            &format!("room-web: peer LEAVE seat={}", seat.0).into(),
                        );
                        push_presence(&mut s.presence, PresenceEvent::PeerLeave { seat });
                    }
                    Err(_) => {
                        web_sys::console::warn_1(&"room-web: ws bad PEER_LEAVE payload".into());
                    }
                },
                Ok((MSG_DATA, rest)) if !rest.is_empty() => {
                    s.data_rx = s.data_rx.saturating_add(1);
                    s.inbox.push_back((PeerId(rest[0]), rest[1..].to_vec()));
                }
                Ok((MSG_VOICE, rest)) if !rest.is_empty() => {
                    // Bound backlog so a stalled JS poller cannot grow forever.
                    if s.voice_inbox.len() < 64 {
                        s.voice_inbox
                            .push_back((PeerId(rest[0]), rest[1..].to_vec()));
                    }
                }
                Ok((ty, _)) => {
                    web_sys::console::warn_1(
                        &format!("room-web: ws ignore msg type={ty} len={}", bytes.len()).into(),
                    );
                }
                Err(_) => {
                    web_sys::console::warn_1(
                        &format!("room-web: ws bad FTRL frame len={}", bytes.len()).into(),
                    );
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let shared_open = shared.clone();
        let outbox_open = outbox.clone();
        let ws_open = ws.clone();
        let hello = pack_hello(name);
        let name_log = name.to_owned();
        let on_open = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            {
                let mut s = shared_open.borrow_mut();
                s.opened = true;
                push_presence(&mut s.presence, PresenceEvent::SocketOpen);
            }
            web_sys::console::log_1(
                &format!("room-web: ws OPEN — sending HELLO name={name_log:?}").into(),
            );
            let _ = ws_open.send_with_u8_array(&hello);
            while let Some(pkt) = outbox_open.borrow_mut().pop_front() {
                let _ = ws_open.send_with_u8_array(&pkt);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let shared_close = shared.clone();
        let on_close = Closure::wrap(Box::new(move |ev: CloseEvent| {
            let code = ev.code();
            let reason = ev.reason();
            web_sys::console::warn_1(
                &format!(
                    "room-web: ws CLOSE code={code} reason={reason:?} clean={}",
                    ev.was_clean()
                )
                .into(),
            );
            let mut s = shared_close.borrow_mut();
            s.closed = true;
            s.opened = false;
            push_presence(&mut s.presence, PresenceEvent::SocketClose { code, reason });
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        let shared_err = shared.clone();
        let on_error = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            web_sys::console::error_1(&"room-web: ws ERROR".into());
            push_presence(
                &mut shared_err.borrow_mut().presence,
                PresenceEvent::SocketError,
            );
        }) as Box<dyn FnMut(web_sys::Event)>);
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        Ok(Self {
            ws,
            shared,
            outbox,
            peers: (0..max_players.max(2)).map(PeerId).collect(),
            _on_message: on_message,
            _on_open: on_open,
            _on_close: on_close,
            _on_error: on_error,
        })
    }

    /// `Some(seat)` once WELCOME arrives; `Err` on reject.
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

    /// Whether the local socket has closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.borrow().closed
    }

    /// Drain presence / socket lifecycle events.
    pub fn poll_presence(&mut self, out: &mut Vec<PresenceEvent>) {
        let mut s = self.shared.borrow_mut();
        while let Some(p) = s.presence.pop_front() {
            out.push(p);
        }
    }

    /// Update peer list after lobby Welcome (actual `peer_count`).
    pub fn set_peer_count(&mut self, peer_count: u8) {
        self.peers = (0..peer_count.max(2)).map(PeerId).collect();
        self.shared.borrow_mut().peer_count = peer_count.max(2);
    }

    /// Send a live-voice payload (not lockstep). Fan-out is handled by `room-ws`.
    pub fn send_voice(&mut self, payload: &[u8]) {
        if payload.is_empty() || payload.len() > 8_192 {
            return;
        }
        let packet = pack_client_voice(payload);
        if self.shared.borrow().opened {
            let _ = self.ws.send_with_u8_array(&packet);
        } else {
            self.outbox.borrow_mut().push_back(packet);
        }
    }

    /// Drain inbound voice frames `(from_seat, payload)`.
    pub fn poll_voice(&mut self, out: &mut Vec<(PeerId, Vec<u8>)>) {
        let mut s = self.shared.borrow_mut();
        while let Some(p) = s.voice_inbox.pop_front() {
            out.push(p);
        }
    }

    /// Debug counters for DATA rx/tx.
    #[must_use]
    pub fn data_stats(&self) -> (u32, u32) {
        let s = self.shared.borrow();
        (s.data_rx, s.data_tx)
    }
}

impl Transport for WsTransport {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        let self_id = self.shared.borrow().self_id;
        let Some(first) = self.peers.iter().copied().filter(|p| *p != self_id).min() else {
            return;
        };
        if peer != first {
            return;
        }
        let packet = pack_client_data(bytes);
        {
            let mut s = self.shared.borrow_mut();
            s.data_tx = s.data_tx.saturating_add(1);
        }
        if self.shared.borrow().opened {
            let _ = self.ws.send_with_u8_array(&packet);
        } else {
            self.outbox.borrow_mut().push_back(packet);
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

fn push_presence(q: &mut VecDeque<PresenceEvent>, ev: PresenceEvent) {
    if q.len() >= 64 {
        let _ = q.pop_front();
    }
    q.push_back(ev);
}

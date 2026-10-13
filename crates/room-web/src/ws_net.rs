//! Browser WebSocket client for the `room-ws` packet-server relay.

use room_net::{
    MSG_DATA, MSG_REJECT, MSG_WELCOME, PeerId, Transport, pack_client_data, pack_hello,
    parse_relay,
};
use js_sys::{ArrayBuffer, Uint8Array};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, MessageEvent, WebSocket};

type Outbox = Rc<RefCell<VecDeque<Vec<u8>>>>;

struct WsShared {
    self_id: PeerId,
    reject: Option<String>,
    inbox: VecDeque<(PeerId, Vec<u8>)>,
    opened: bool,
}

/// WebSocket relay transport used by web lockstep coop.
pub struct WsTransport {
    ws: WebSocket,
    shared: Rc<RefCell<WsShared>>,
    outbox: Outbox,
    peers: Vec<PeerId>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_open: Closure<dyn FnMut(web_sys::Event)>,
    _on_close: Closure<dyn FnMut(web_sys::Event)>,
    _on_error: Closure<dyn FnMut(web_sys::Event)>,
}

impl WsTransport {
    /// Begin connecting; HELLO is sent on `onopen`.
    pub fn start(url: &str, name: &str, max_players: u8) -> Result<Self, String> {
        let ws = WebSocket::new(url).map_err(|e| format!("WebSocket: {e:?}"))?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let shared = Rc::new(RefCell::new(WsShared {
            self_id: PeerId(255),
            reject: None,
            inbox: VecDeque::new(),
            opened: false,
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
                }
                Ok((MSG_REJECT, rest)) => {
                    s.reject = Some(if rest.len() >= 2 {
                        let n = u16::from_le_bytes([rest[0], rest[1]]) as usize;
                        String::from_utf8_lossy(&rest[2..2 + n.min(rest.len().saturating_sub(2))])
                            .into_owned()
                    } else {
                        "rejected".into()
                    });
                }
                Ok((MSG_DATA, rest)) if !rest.is_empty() => {
                    s.inbox
                        .push_back((PeerId(rest[0]), rest[1..].to_vec()));
                }
                _ => {}
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let shared_open = shared.clone();
        let outbox_open = outbox.clone();
        let ws_open = ws.clone();
        let hello = pack_hello(name);
        let on_open = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            shared_open.borrow_mut().opened = true;
            let _ = ws_open.send_with_u8_array(&hello);
            while let Some(pkt) = outbox_open.borrow_mut().pop_front() {
                let _ = ws_open.send_with_u8_array(&pkt);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let on_close = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            web_sys::console::warn_1(&"room-web: WebSocket closed".into());
        }) as Box<dyn FnMut(web_sys::Event)>);
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        let on_error = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
            web_sys::console::error_1(&"room-web: WebSocket error".into());
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

    /// Update peer list after lobby Welcome (actual `peer_count`).
    pub fn set_peer_count(&mut self, peer_count: u8) {
        self.peers = (0..peer_count.max(2)).map(PeerId).collect();
    }
}

impl Transport for WsTransport {
    fn send(&mut self, peer: PeerId, bytes: &[u8]) {
        let self_id = self.shared.borrow().self_id;
        let Some(first) = self
            .peers
            .iter()
            .copied()
            .filter(|p| *p != self_id)
            .min()
        else {
            return;
        };
        if peer != first {
            return;
        }
        let packet = pack_client_data(bytes);
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

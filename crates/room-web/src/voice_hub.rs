//! JS ↔ WebSocket voice bridge (PCM payloads multiplexed as `MSG_VOICE`).
//!
//! Capture/playback lives in `voice.js` (adapted from Lightcall). Rust only
//! queues bytes onto the active coop socket and exposes a poll API to JS.

use crate::ws_net::WsTransport;
use std::cell::RefCell;
use std::collections::VecDeque;

const MAX_OUTBOX: usize = 32;
const MAX_INBOX: usize = 64;
const MAX_PAYLOAD: usize = 8_192;

struct Hub {
    outbox: VecDeque<Vec<u8>>,
    inbox: VecDeque<(u8, Vec<u8>)>,
    /// True while a coop WebSocket is pumping voice.
    active: bool,
}

thread_local! {
    static HUB: RefCell<Hub> = const {
        RefCell::new(Hub {
            outbox: VecDeque::new(),
            inbox: VecDeque::new(),
            active: false,
        })
    };
}

/// Queue a voice frame from JS (`roomVoiceSend`).
pub fn enqueue_out(payload: &[u8]) {
    if payload.is_empty() || payload.len() > MAX_PAYLOAD {
        return;
    }
    HUB.with(|h| {
        let mut hub = h.borrow_mut();
        if hub.outbox.len() >= MAX_OUTBOX {
            let _ = hub.outbox.pop_front();
        }
        hub.outbox.push_back(payload.to_vec());
    });
}

/// Drain inbound voice for JS (`roomVoicePoll`).
pub fn take_inbox() -> Vec<(u8, Vec<u8>)> {
    HUB.with(|h| {
        let mut hub = h.borrow_mut();
        hub.inbox.drain(..).collect()
    })
}

/// Whether coop voice I/O is attached to a live socket.
#[must_use]
pub fn is_active() -> bool {
    HUB.with(|h| h.borrow().active)
}

/// Flush JS→WS outbox and WS→JS inbox on the active transport.
pub fn pump(transport: &mut WsTransport) {
    HUB.with(|h| {
        let mut hub = h.borrow_mut();
        hub.active = true;
        while let Some(payload) = hub.outbox.pop_front() {
            transport.send_voice(&payload);
        }
        let mut raw = Vec::new();
        transport.poll_voice(&mut raw);
        for (from, payload) in raw {
            if hub.inbox.len() >= MAX_INBOX {
                let _ = hub.inbox.pop_front();
            }
            hub.inbox.push_back((from.0, payload));
        }
    });
}

/// Mark voice inactive (socket dropped / single-player).
#[allow(dead_code)]
pub fn deactivate() {
    HUB.with(|h| {
        let mut hub = h.borrow_mut();
        hub.active = false;
        hub.outbox.clear();
        hub.inbox.clear();
    });
}

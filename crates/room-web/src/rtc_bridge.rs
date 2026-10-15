//! Bridge to `window.roomRtc` (Lightcall-style WebRTC client).

use js_sys::{Function, Reflect};
use room_proto::{ControlMsg, IceServerEntry, MediaInfo};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

fn room_rtc() -> Option<JsValue> {
    let win = web_sys::window()?;
    Reflect::get(&win, &JsValue::from_str("roomRtc")).ok()
}

/// `true` when `rtc.js` loaded and exposes `startFromMedia`.
#[must_use]
pub fn available() -> bool {
    room_rtc().is_some_and(|rtc| {
        Reflect::has(&rtc, &JsValue::from_str("startFromMedia")).unwrap_or(false)
    })
}

/// Start WebRTC from `welcome.media`.
///
/// `seat` selects perfect-negotiation role (seat 0 impolite, others polite).
pub fn start_from_media(media: &MediaInfo, seat: u8) {
    let Some(rtc) = room_rtc() else {
        return;
    };
    let Some(func) = Reflect::get(&rtc, &JsValue::from_str("startFromMedia"))
        .ok()
        .and_then(|v| v.dyn_into::<Function>().ok())
    else {
        return;
    };
    let media_js = media_to_js(media, seat);
    let _ = func.call1(&rtc, &media_js);
    web_sys::console::log_1(
        &format!(
            "room-web: roomRtc start seat={seat} ice_servers={}",
            media.ice_servers.len()
        )
        .into(),
    );
}

fn media_to_js(media: &MediaInfo, seat: u8) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = Reflect::set(
        &obj,
        &JsValue::from_str("enabled"),
        &JsValue::from_bool(media.enabled),
    );
    let _ = Reflect::set(
        &obj,
        &JsValue::from_str("seat"),
        &JsValue::from_f64(f64::from(seat)),
    );
    let _ = Reflect::set(
        &obj,
        &JsValue::from_str("polite"),
        &JsValue::from_bool(seat != 0),
    );
    let arr = js_sys::Array::new();
    for s in &media.ice_servers {
        let entry = js_sys::Object::new();
        let urls = js_sys::Array::new();
        for u in &s.urls {
            urls.push(&JsValue::from_str(u));
        }
        let _ = Reflect::set(&entry, &JsValue::from_str("urls"), &urls);
        if let Some(u) = &s.username {
            let _ = Reflect::set(
                &entry,
                &JsValue::from_str("username"),
                &JsValue::from_str(u),
            );
        }
        if let Some(c) = &s.credential {
            let _ = Reflect::set(
                &entry,
                &JsValue::from_str("credential"),
                &JsValue::from_str(c),
            );
        }
        arr.push(&entry);
    }
    let _ = Reflect::set(&obj, &JsValue::from_str("ice_servers"), &arr);
    obj.into()
}

/// Drain outbound signalling messages produced by `rtc.js`.
#[must_use]
pub fn take_outbound() -> Vec<ControlMsg> {
    let Some(rtc) = room_rtc() else {
        return Vec::new();
    };
    let Some(func) = Reflect::get(&rtc, &JsValue::from_str("takeOutboundSignals"))
        .ok()
        .and_then(|v| v.dyn_into::<Function>().ok())
    else {
        return Vec::new();
    };
    let Ok(ret) = func.call0(&rtc) else {
        return Vec::new();
    };
    let Ok(arr) = ret.dyn_into::<js_sys::Array>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..arr.length() {
        let v = arr.get(i);
        let kind = Reflect::get(&v, &JsValue::from_str("kind"))
            .ok()
            .and_then(|x| x.as_string())
            .unwrap_or_default();
        if kind.is_empty() {
            continue;
        }
        let sdp = Reflect::get(&v, &JsValue::from_str("sdp"))
            .ok()
            .and_then(|x| x.as_string());
        let candidate = Reflect::get(&v, &JsValue::from_str("candidate"))
            .ok()
            .and_then(|x| x.as_string());
        let mid = Reflect::get(&v, &JsValue::from_str("mid"))
            .ok()
            .and_then(|x| x.as_string());
        out.push(ControlMsg::Rtc {
            kind,
            sdp,
            candidate,
            mid,
        });
    }
    out
}

/// Deliver an inbound `rtc` control message to `rtc.js`.
pub fn handle_inbound(msg: &ControlMsg) {
    let ControlMsg::Rtc {
        kind,
        sdp,
        candidate,
        mid,
    } = msg
    else {
        return;
    };
    let Some(rtc) = room_rtc() else {
        return;
    };
    let Some(func) = Reflect::get(&rtc, &JsValue::from_str("handleSignal"))
        .ok()
        .and_then(|v| v.dyn_into::<Function>().ok())
    else {
        return;
    };
    let obj = js_sys::Object::new();
    let _ = Reflect::set(&obj, &JsValue::from_str("kind"), &JsValue::from_str(kind));
    if let Some(s) = sdp {
        let _ = Reflect::set(&obj, &JsValue::from_str("sdp"), &JsValue::from_str(s));
    }
    if let Some(c) = candidate {
        let _ = Reflect::set(&obj, &JsValue::from_str("candidate"), &JsValue::from_str(c));
    }
    if let Some(m) = mid {
        let _ = Reflect::set(&obj, &JsValue::from_str("mid"), &JsValue::from_str(m));
    }
    let _ = func.call1(&rtc, &obj);
}

/// Debug helper: describe ICE list.
#[must_use]
pub fn ice_summary(servers: &[IceServerEntry]) -> String {
    servers
        .iter()
        .map(|s| {
            let u = s.urls.first().map(String::as_str).unwrap_or("?");
            if u.starts_with("turn") {
                "TURN"
            } else if u.starts_with("stun") {
                "STUN"
            } else {
                "?"
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

//! F6: FaceShare relay gated by `face_share_enabled` (default off).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{ControlMsg, HelloMsg, encode_control, parse_control};
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use tokio_tungstenite::tungstenite::Message;

async fn drain_welcome(ws: &mut tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>) {
    for _ in 0..30 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if matches!(parse_control(&t), Ok(ControlMsg::Welcome(_))) {
                return;
            }
        }
    }
    panic!("no welcome");
}

fn hello(ticket: &str, name: &str) -> ControlMsg {
    ControlMsg::Hello(HelloMsg {
        ticket: ticket.into(),
        client_version: "t".into(),
        abi_major: 1,
        iwad_fingerprint: "f".into(),
        name: Some(name.into()),
        resume_token: None,
    })
}

fn share_msg() -> ControlMsg {
    // 64×64 RGBA all zeros → base64 fits control limit.
    let rgba = vec![0u8; 64 * 64 * 4];
    let rgba_b64 = base64_encode(&rgba);
    ControlMsg::FaceShare {
        from: None,
        name: "me".into(),
        face_id: "1".into(),
        rgba_b64,
    }
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let mut n = (u32::from(chunk[0])) << 16;
        if chunk.len() > 1 {
            n |= u32::from(chunk[1]) << 8;
        }
        if chunk.len() > 2 {
            n |= u32::from(chunk[2]);
        }
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[tokio::test]
async fn face_share_dropped_when_disabled() {
    let registry = RoomRegistry::new(ServerConfig {
        face_share_enabled: false,
        max_conn_per_ip: 0,
        ..ServerConfig::default()
    });
    let room = registry
        .create_room(CreateRoomRequest {
            max_players: 2,
            ..CreateRoomRequest::default()
        })
        .unwrap();
    let (addr, _h) = bind_ephemeral(registry).await.unwrap();
    let url = format!("ws://{addr}/v2/rooms/{}/ws", room.id);

    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    a.send(Message::Text(encode_control(&hello(&room.id, "a")).unwrap().into()))
        .await
        .unwrap();
    drain_welcome(&mut a).await;

    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    b.send(Message::Text(encode_control(&hello(&room.id, "b")).unwrap().into()))
        .await
        .unwrap();
    drain_welcome(&mut b).await;

    a.send(Message::Text(encode_control(&share_msg()).unwrap().into()))
        .await
        .unwrap();

    // Peer must not receive FaceShare while disabled.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(200);
    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            msg = b.next() => {
                if let Some(Ok(Message::Text(t))) = msg {
                    if let Ok(ControlMsg::FaceShare { .. }) = parse_control(&t) {
                        panic!("FaceShare must not relay when disabled");
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => break,
        }
    }
}

#[tokio::test]
async fn face_share_relays_when_enabled() {
    let registry = RoomRegistry::new(ServerConfig {
        face_share_enabled: true,
        max_conn_per_ip: 0,
        // FaceShare 64×64 RGBA≈22 KiB base64; default byte_burst is 16 KiB.
        byte_burst: 65_536.0,
        byte_refill_per_sec: 65_536.0,
        ..ServerConfig::default()
    });
    let room = registry
        .create_room(CreateRoomRequest {
            max_players: 2,
            ..CreateRoomRequest::default()
        })
        .unwrap();
    let (addr, _h) = bind_ephemeral(registry).await.unwrap();
    let url = format!("ws://{addr}/v2/rooms/{}/ws", room.id);

    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    a.send(Message::Text(encode_control(&hello(&room.id, "a")).unwrap().into()))
        .await
        .unwrap();
    drain_welcome(&mut a).await;

    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    b.send(Message::Text(encode_control(&hello(&room.id, "b")).unwrap().into()))
        .await
        .unwrap();
    drain_welcome(&mut b).await;

    a.send(Message::Text(encode_control(&share_msg()).unwrap().into()))
        .await
        .unwrap();

    let mut got = false;
    for _ in 0..40 {
        if let Some(Ok(Message::Text(t))) = b.next().await {
            if let Ok(ControlMsg::FaceShare {
                from,
                name,
                rgba_b64,
                ..
            }) = parse_control(&t)
            {
                assert_eq!(name, "me");
                assert!(from.is_some());
                assert!(!rgba_b64.is_empty());
                got = true;
                break;
            }
        }
    }
    assert!(got, "peer should receive FaceShare when enabled");
}

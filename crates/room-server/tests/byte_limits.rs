//! Byte / chat rate limits (docs/22 §5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{ControlMsg, HelloMsg, encode_control, parse_control};
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn oversized_byte_burst_counts_violation_path() {
    let registry = RoomRegistry::new(ServerConfig {
        max_conn_per_ip: 0,
        byte_burst: 300.0,
        byte_refill_per_sec: 0.0,
        msg_burst: 1_000.0,
        ..ServerConfig::default()
    });
    let room = registry
        .create_room(CreateRoomRequest {
            max_players: 2,
            ..CreateRoomRequest::default()
        })
        .unwrap();
    let (addr, _h) = bind_ephemeral(registry.clone()).await.unwrap();
    let url = format!("ws://{addr}/v2/rooms/{}/ws", room.id);
    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    a.send(Message::Text(
        encode_control(&ControlMsg::Hello(HelloMsg {
            ticket: room.id.clone(),
            client_version: "t".into(),
            abi_major: 1,
            iwad_fingerprint: "f".into(),
            name: Some("a".into()),
            resume_token: None,
        }))
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    // Drain welcome.
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = a.next().await {
            if matches!(parse_control(&t), Ok(ControlMsg::Welcome(_))) {
                break;
            }
        }
    }
    // Flood with large text frames until byte bucket trips (dropped, not fatal yet).
    let big = "x".repeat(80);
    for _ in 0..5 {
        let chat = encode_control(&ControlMsg::Chat {
            text: big.clone(),
            from: None,
            ts: None,
        })
        .unwrap();
        let _ = a.send(Message::Text(chat.into())).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let (_msgs, _bytes, violations, _frames) = registry.metrics_snapshot();
    assert!(violations > 0, "expected byte-limit violations");
}

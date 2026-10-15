//! Handshake / idle timeouts (docs/22 §5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::StreamExt;
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};

#[tokio::test]
async fn handshake_timeout_closes_silent_socket() {
    let registry = RoomRegistry::new(ServerConfig {
        max_conn_per_ip: 0,
        handshake_timeout_secs: 1,
        idle_timeout_secs: 60,
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
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    // No hello — expect close within handshake timeout.
    let closed = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(msg) = ws.next().await {
            if msg.is_err() {
                return true;
            }
            if let Ok(tokio_tungstenite::tungstenite::Message::Close(_)) = msg {
                return true;
            }
        }
        true
    })
    .await;
    assert!(closed.is_ok(), "expected handshake timeout close");
}

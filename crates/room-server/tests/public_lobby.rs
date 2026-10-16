//! Public lobby directory (GET /api/rooms + SPA assets).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn public_list_hides_private_and_serves_lobby() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    });
    let pub_room = registry
        .create_room(CreateRoomRequest {
            map: "E1M1".into(),
            skill: 3,
            max_players: 2,
            visibility: "public".into(),
            enable_verifier: false,
            game_mod: None,
            time_limit: None,
        })
        .unwrap();
    let _priv = registry
        .create_room(CreateRoomRequest {
            map: "E1M2".into(),
            skill: 3,
            max_players: 2,
            visibility: "private".into(),
            enable_verifier: false,
            game_mod: None,
            time_limit: None,
        })
        .unwrap();
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();

    let list = http_get(addr, "/api/rooms").await;
    assert!(list.contains(&pub_room.id), "{list}");
    assert!(list.contains("E1M1"), "{list}");
    assert!(
        !list.contains("E1M2"),
        "private rooms must not list: {list}"
    );

    let lobby = http_get(addr, "/").await;
    assert!(
        lobby.contains("Find a game") || lobby.contains("lobby"),
        "{lobby}"
    );
    let css = http_get(addr, "/lobby/assets/app.css").await;
    assert!(css.contains("--accent"), "{css}");
    let cfg = http_get(addr, "/api/lobby").await;
    assert!(cfg.contains("play_base"), "{cfg}");
}

async fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf).into_owned()
}

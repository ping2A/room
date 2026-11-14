//! S5: graceful drain, room_location, verifier flag.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn drain_rejects_creates_and_readyz_503() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        node_id: "node-a".into(),
        max_conn_per_ip: 0,
        drain_grace_secs: 3600,
        ..ServerConfig::default()
    });
    let created = registry
        .create_room(CreateRoomRequest {
            map: "E1M1".into(),
            skill: 3,
            max_players: 2,
            enable_verifier: false,
            visibility: "public".into(),
            game_mod: None,
            packs: Vec::new(),
            time_limit: None,
            game: None,
            fill_bots: false,
            quick_play: false,
            preset: None,
            ..CreateRoomRequest::default()
        })
        .unwrap();
    assert_eq!(registry.locate_room(&created.id).as_deref(), Some("node-a"));

    let (addr, _h) = bind_ephemeral(registry.clone()).await.unwrap();
    registry.begin_drain("maintenance");
    assert!(registry.is_draining());
    assert!(matches!(
        registry.create_room(CreateRoomRequest::default()),
        Err(room_server::RegistryError::Draining)
    ));

    let mut sock = tokio::net::TcpStream::connect(addr).await.unwrap();
    sock.write_all(b"GET /readyz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut buf = vec![0u8; 512];
    let n = sock.read(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(resp.contains("503"), "expected 503 draining, got {resp:?}");
}

#[tokio::test]
async fn verifier_flag_requires_node_enable() {
    let off = RoomRegistry::new(ServerConfig {
        enable_verifier: false,
        max_conn_per_ip: 0,
        ..ServerConfig::default()
    });
    let r = off
        .create_room(CreateRoomRequest {
            enable_verifier: true,
            ..CreateRoomRequest::default()
        })
        .unwrap();
    assert!(!r.settings.enable_verifier);

    let on = RoomRegistry::new(ServerConfig {
        enable_verifier: true,
        max_verified_rooms: 1,
        max_conn_per_ip: 0,
        ..ServerConfig::default()
    });
    let a = on
        .create_room(CreateRoomRequest {
            enable_verifier: true,
            ..CreateRoomRequest::default()
        })
        .unwrap();
    assert!(a.settings.enable_verifier);
    assert_eq!(on.verified_room_count(), 1);
    let err = on.create_room(CreateRoomRequest {
        enable_verifier: true,
        ..CreateRoomRequest::default()
    });
    assert!(matches!(err, Err(room_server::RegistryError::VerifierFull)));
}

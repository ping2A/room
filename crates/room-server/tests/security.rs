//! Security acceptance tests (docs/22 §9 Testing item 4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{
    BinaryMsg, ControlMsg, HelloMsg, NetTicCmd, TicCmdMsg, encode_binary, encode_control,
    parse_control,
};
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn security_headers_on_healthz() {
    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf);
    assert!(resp.contains("x-content-type-options: nosniff"));
    assert!(resp.contains("referrer-policy: no-referrer"));
    assert!(resp.contains("content-security-policy:"));
}

#[tokio::test]
async fn require_tickets_rejects_bare_room_id() {
    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        require_tickets: true,
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
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
        })
        .unwrap();
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();
    let url = format!("ws://{addr}/v2/rooms/{}/ws", created.id);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let hello = ControlMsg::Hello(HelloMsg {
        ticket: created.id.clone(), // bare room id — not a minted ticket
        client_version: "t".into(),
        abi_major: 1,
        iwad_fingerprint: "fp".into(),
        name: Some("eve".into()),
        resume_token: None,
    });
    ws.send(Message::Text(encode_control(&hello).unwrap().into()))
        .await
        .unwrap();
    let mut rejected = false;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(ControlMsg::Reject(r)) = parse_control(&t) {
                assert_eq!(r.code, "ticket");
                rejected = true;
                break;
            }
        }
    }
    assert!(rejected, "expected ticket reject");
}

#[tokio::test]
async fn seat_hijack_without_resume_token_rejected() {
    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
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
        })
        .unwrap();
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();
    let url = format!("ws://{addr}/v2/rooms/{}/ws", created.id);

    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let hello = ControlMsg::Hello(HelloMsg {
        ticket: created.id.clone(),
        client_version: "t".into(),
        abi_major: 1,
        iwad_fingerprint: "fp".into(),
        name: Some("alice".into()),
        resume_token: None,
    });
    a.send(Message::Text(encode_control(&hello).unwrap().into()))
        .await
        .unwrap();
    let mut resume = String::new();
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = a.next().await {
            if let Ok(ControlMsg::Welcome(w)) = parse_control(&t) {
                resume = w.resume_token;
                break;
            }
        }
    }
    assert!(!resume.is_empty());
    // Drop alice — seat reserved.
    drop(a);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Attacker joins without resume token → new seat if available, or lobby_full.
    // With max_players=2 and one reserved, one free seat remains for bob;
    // try reclaim alice's seat with wrong token.
    let (mut evil, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let bad = ControlMsg::Hello(HelloMsg {
        ticket: created.id.clone(),
        client_version: "t".into(),
        abi_major: 1,
        iwad_fingerprint: "fp".into(),
        name: Some("evil".into()),
        resume_token: Some("0".repeat(64)),
    });
    evil.send(Message::Text(encode_control(&bad).unwrap().into()))
        .await
        .unwrap();
    let mut denied = false;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = evil.next().await {
            if let Ok(ControlMsg::Reject(r)) = parse_control(&t) {
                assert_eq!(r.code, "resume_denied");
                denied = true;
                break;
            }
        }
    }
    assert!(denied, "wrong resume token must be denied");

    // Correct resume reclaims.
    let (mut good, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let ok = ControlMsg::Hello(HelloMsg {
        ticket: created.id.clone(),
        client_version: "t".into(),
        abi_major: 1,
        iwad_fingerprint: "fp".into(),
        name: Some("alice".into()),
        resume_token: Some(resume),
    });
    good.send(Message::Text(encode_control(&ok).unwrap().into()))
        .await
        .unwrap();
    let mut welcomed = false;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = good.next().await {
            if matches!(parse_control(&t), Ok(ControlMsg::Welcome(_))) {
                welcomed = true;
                break;
            }
        }
    }
    assert!(welcomed, "valid resume token reclaims seat");
}

#[tokio::test]
async fn message_flood_disconnects() {
    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        msg_burst: 5.0,
        msg_refill_per_sec: 0.0,
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
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
        })
        .unwrap();
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();
    let url = format!("ws://{addr}/v2/rooms/{}/ws", created.id);
    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let hello = ControlMsg::Hello(HelloMsg {
        ticket: created.id.clone(),
        client_version: "t".into(),
        abi_major: 1,
        iwad_fingerprint: "fp".into(),
        name: Some("flood".into()),
        resume_token: None,
    });
    a.send(Message::Text(encode_control(&hello).unwrap().into()))
        .await
        .unwrap();
    // Drain welcome.
    for _ in 0..10 {
        if let Some(Ok(Message::Text(t))) = a.next().await {
            if matches!(parse_control(&t), Ok(ControlMsg::Welcome(_))) {
                break;
            }
        }
    }
    // Spam until violations kick (burst 5, then 50 violation hits).
    let junk = encode_binary(&BinaryMsg::TicCmd(TicCmdMsg {
        tic: 0,
        cmd: NetTicCmd::default(),
    }));
    let mut closed = false;
    for _ in 0..200 {
        if a.send(Message::Binary(junk.clone().into())).await.is_err() {
            closed = true;
            break;
        }
        // Also try reading for close.
        match tokio::time::timeout(std::time::Duration::from_millis(5), a.next()).await {
            Ok(None) | Ok(Some(Err(_))) => {
                closed = true;
                break;
            }
            Ok(Some(Ok(Message::Close(_)))) => {
                closed = true;
                break;
            }
            _ => {}
        }
    }
    assert!(closed, "flood should disconnect the socket");
}

#[tokio::test]
async fn v2_tic_cmd_has_no_forgeable_peer_byte() {
    // TIC_CMD payload is tic + NetTicCmd only; seat comes from the connection.
    let encoded = encode_binary(&BinaryMsg::TicCmd(TicCmdMsg {
        tic: 7,
        cmd: NetTicCmd {
            forward_move: 40,
            ..NetTicCmd::default()
        },
    }));
    // FTR2 + type + u32 tic + 17-byte cmd = 5 + 4 + 17 = 26
    assert!(encoded.len() >= 26);
    // No peer index field after the type byte (tic is u32 at offset 5).
    let tic = u32::from_le_bytes([encoded[5], encoded[6], encoded[7], encoded[8]]);
    assert_eq!(tic, 7);
}

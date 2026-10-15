//! Multi-room legacy FTRL + v2 TIC_FRAME smoke tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_net::{
    MSG_DATA, MSG_WELCOME, NetTicCmd as NetNetTicCmd, PeerId, TimedCmd, decode_timed_cmd,
    encode_timed_cmd, pack_client_data, pack_hello, parse_relay,
};
use room_proto::{
    BinaryMsg, ControlMsg, HelloMsg, NetTicCmd, TicCmdMsg, encode_binary, encode_control,
    parse_binary, parse_control,
};
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn legacy_two_peers_stamped_data() {
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
        })
        .expect("create");
    let (addr, _server) = bind_ephemeral(registry).await.expect("bind");

    let url0 = format!("ws://{addr}/ws/{}", created.id);
    let url1 = format!("ws://{addr}/ws/{}", created.id);
    let (mut ws0, _) = tokio_tungstenite::connect_async(&url0).await.expect("ws0");
    let (mut ws1, _) = tokio_tungstenite::connect_async(&url1).await.expect("ws1");

    ws0.send(Message::Binary(pack_hello("p0").into()))
        .await
        .unwrap();
    ws1.send(Message::Binary(pack_hello("p1").into()))
        .await
        .unwrap();

    let mut seat0 = None;
    let mut seat1 = None;
    // Drain welcomes / joins.
    for _ in 0..8 {
        tokio::select! {
            m = ws0.next() => {
                if let Some(Ok(Message::Binary(b))) = m {
                    if let Ok((MSG_WELCOME, rest)) = parse_relay(&b) {
                        if !rest.is_empty() { seat0 = Some(rest[0]); }
                    }
                }
            }
            m = ws1.next() => {
                if let Some(Ok(Message::Binary(b))) = m {
                    if let Ok((MSG_WELCOME, rest)) = parse_relay(&b) {
                        if !rest.is_empty() { seat1 = Some(rest[0]); }
                    }
                }
            }
        }
        if seat0.is_some() && seat1.is_some() {
            break;
        }
    }
    assert_ne!(seat0, seat1);

    // p0 sends a TimedCmd claiming to be peer 99 — server must stamp real seat.
    let forged = TimedCmd {
        tic: 0,
        peer: PeerId(99),
        cmd: NetNetTicCmd {
            forward_move: 40,
            ..NetNetTicCmd::default()
        },
    };
    ws0.send(Message::Binary(
        pack_client_data(&encode_timed_cmd(&forged)).into(),
    ))
    .await
    .unwrap();

    let mut got = None;
    for _ in 0..20 {
        if let Some(Ok(Message::Binary(b))) = ws1.next().await {
            if let Ok((MSG_DATA, rest)) = parse_relay(&b) {
                if rest.len() > 1 {
                    if let Ok(t) = decode_timed_cmd(&rest[1..]) {
                        got = Some((rest[0], t));
                        break;
                    }
                }
            }
        }
    }
    let (from, t) = got.expect("DATA to p1");
    assert_eq!(from, seat0.unwrap());
    assert_eq!(t.peer.0, seat0.unwrap());
    assert_eq!(t.cmd.forward_move, 40);
}

#[tokio::test]
async fn v2_tic_frame_two_seats() {
    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
    let created = registry
        .create_room(CreateRoomRequest {
            map: "MAP01".into(),
            skill: 3,
            max_players: 2,
            enable_verifier: false,
            visibility: "public".into(),
        })
        .expect("create");
    let (addr, _server) = bind_ephemeral(registry).await.expect("bind");
    let url = format!("ws://{addr}/v2/rooms/{}/ws", created.id);

    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    for (ws, name) in [(&mut a, "alice"), (&mut b, "bob")] {
        let hello = ControlMsg::Hello(HelloMsg {
            ticket: created.id.clone(),
            client_version: "0.1.0".into(),
            abi_major: 1,
            iwad_fingerprint: "abc".into(),
            name: Some(name.into()),
            resume_token: None,
        });
        ws.send(Message::Text(encode_control(&hello).unwrap().into()))
            .await
            .unwrap();
    }

    // Wait for welcome then send ready.
    for ws in [&mut a, &mut b] {
        let mut welcomed = false;
        for _ in 0..10 {
            if let Some(Ok(Message::Text(t))) = ws.next().await {
                if let Ok(ControlMsg::Welcome(_)) = parse_control(&t) {
                    welcomed = true;
                    break;
                }
            }
        }
        assert!(welcomed);
        ws.send(Message::Text(
            encode_control(&ControlMsg::Ready {}).unwrap().into(),
        ))
        .await
        .unwrap();
    }

    // Drain until Start.
    let mut started = 0;
    for ws in [&mut a, &mut b] {
        for _ in 0..10 {
            if let Some(Ok(Message::Text(t))) = ws.next().await {
                if matches!(parse_control(&t), Ok(ControlMsg::Start { .. })) {
                    started += 1;
                    break;
                }
            }
        }
    }
    assert_eq!(started, 2);

    let cmd = |fwd| {
        encode_binary(&BinaryMsg::TicCmd(TicCmdMsg {
            tic: 0,
            cmd: NetTicCmd {
                forward_move: fwd,
                ..NetTicCmd::default()
            },
        }))
    };
    a.send(Message::Binary(cmd(5).into())).await.unwrap();
    b.send(Message::Binary(cmd(0).into())).await.unwrap();

    let mut frame = None;
    for _ in 0..20 {
        if let Some(Ok(Message::Binary(bin))) = a.next().await {
            if let Ok(BinaryMsg::TicFrame(f)) = parse_binary(&bin) {
                frame = Some(f);
                break;
            }
        }
    }
    let f = frame.expect("tic frame");
    assert_eq!(f.tic, 0);
    assert_eq!(f.seat_mask, 0b11);
    assert_eq!(f.cmds[0].forward_move, 5);
}

#[tokio::test]
async fn many_rooms_create_and_pair() {
    let n = 64usize;
    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        max_rooms: n + 10,
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
    let (addr, _server) = bind_ephemeral(registry.clone()).await.expect("bind");

    let mut rooms = Vec::with_capacity(n);
    for _ in 0..n {
        rooms.push(
            registry
                .create_room(CreateRoomRequest {
                    map: "E1M1".into(),
                    skill: 3,
                    max_players: 2,
                    enable_verifier: false,
                    visibility: "public".into(),
                })
                .expect("create"),
        );
    }
    assert_eq!(registry.len(), n);

    // One HELLO per room seat 0 — proves routing isolation.
    for room in &rooms {
        let url = format!("ws://{addr}/ws/{}", room.id);
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        ws.send(Message::Binary(pack_hello("solo").into()))
            .await
            .unwrap();
        let mut ok = false;
        for _ in 0..5 {
            if let Some(Ok(Message::Binary(b))) = ws.next().await {
                if let Ok((MSG_WELCOME, rest)) = parse_relay(&b) {
                    assert_eq!(rest[0], 0);
                    ok = true;
                    break;
                }
            }
        }
        assert!(ok, "welcome in room {}", room.id);
        let _ = ws.close(None).await;
    }
}

/// M1 acceptance: 1,000 rooms with paired v2 clients and matching TIC_FRAMEs.
#[tokio::test]
async fn soak_1000_rooms_v2_tic_frames() {
    const N: usize = 1_000;
    const TICS: u32 = 8;
    const BATCH: usize = 50;

    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        max_rooms: N + 10,
        max_conn_per_ip: 0, // soak opens thousands from one loopback IP
        ..ServerConfig::default()
    };
    let registry = RoomRegistry::new(cfg);
    let (addr, _server) = bind_ephemeral(registry.clone()).await.expect("bind");
    let base = format!("ws://{addr}");

    let mut ids = Vec::with_capacity(N);
    for _ in 0..N {
        ids.push(
            registry
                .create_room(CreateRoomRequest {
                    map: "E1M1".into(),
                    skill: 3,
                    max_players: 2,
                    enable_verifier: false,
                    visibility: "public".into(),
                })
                .expect("create")
                .id,
        );
    }
    assert_eq!(registry.len(), N);

    for chunk in ids.chunks(BATCH) {
        let mut tasks = Vec::with_capacity(chunk.len());
        for id in chunk {
            let base = base.clone();
            let id = id.clone();
            tasks.push(tokio::spawn(async move {
                drive_v2_room(&base, &id, TICS).await
            }));
        }
        for t in tasks {
            t.await.expect("join").expect("room");
        }
    }
}

async fn drive_v2_room(base: &str, room_id: &str, tics: u32) -> Result<(), String> {
    let url = format!("{base}/v2/rooms/{room_id}/ws");
    let (mut a, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| e.to_string())?;
    let (mut b, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| e.to_string())?;

    for (ws, name) in [(&mut a, "a"), (&mut b, "b")] {
        let hello = ControlMsg::Hello(HelloMsg {
            ticket: room_id.to_owned(),
            client_version: "test".into(),
            abi_major: 1,
            iwad_fingerprint: "fp".into(),
            name: Some(name.into()),
            resume_token: None,
        });
        ws.send(Message::Text(encode_control(&hello).unwrap().into()))
            .await
            .map_err(|e| e.to_string())?;
    }
    for ws in [&mut a, &mut b] {
        let mut ok = false;
        for _ in 0..20 {
            if let Some(Ok(Message::Text(t))) = ws.next().await {
                if matches!(parse_control(&t), Ok(ControlMsg::Welcome(_))) {
                    ok = true;
                    break;
                }
            }
        }
        if !ok {
            return Err("welcome".into());
        }
        ws.send(Message::Text(
            encode_control(&ControlMsg::Ready {}).unwrap().into(),
        ))
        .await
        .map_err(|e| e.to_string())?;
    }
    for ws in [&mut a, &mut b] {
        let mut ok = false;
        for _ in 0..20 {
            if let Some(Ok(Message::Text(t))) = ws.next().await {
                if matches!(parse_control(&t), Ok(ControlMsg::Start { .. })) {
                    ok = true;
                    break;
                }
            }
        }
        if !ok {
            return Err("start".into());
        }
    }

    for tic in 0..tics {
        let enc = |fwd| {
            encode_binary(&BinaryMsg::TicCmd(TicCmdMsg {
                tic,
                cmd: NetTicCmd {
                    forward_move: fwd,
                    ..NetTicCmd::default()
                },
            }))
        };
        a.send(Message::Binary(enc(1).into()))
            .await
            .map_err(|e| e.to_string())?;
        b.send(Message::Binary(enc(0).into()))
            .await
            .map_err(|e| e.to_string())?;
        let mut got = None;
        for _ in 0..40 {
            if let Some(Ok(Message::Binary(bin))) = a.next().await {
                if let Ok(BinaryMsg::TicFrame(f)) = parse_binary(&bin) {
                    got = Some(f);
                    break;
                }
            }
        }
        let f = got.ok_or("frame")?;
        if f.tic != tic || f.seat_mask != 0b11 {
            return Err(format!("bad frame {f:?}"));
        }
        // Drain peer frame.
        for _ in 0..40 {
            if let Some(Ok(Message::Binary(bin))) = b.next().await {
                if matches!(parse_binary(&bin), Ok(BinaryMsg::TicFrame(_))) {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn origin_allowlist_rejects_unknown() {
    let cfg = ServerConfig {
        bind: "127.0.0.1:0".into(),
        allowed_origins: vec!["http://allowed.example".into()],
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
        })
        .unwrap();
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();
    let url = format!("http://{addr}/ws/{}", created.id);
    // Hand-rolled upgrade with bad Origin → 403.
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "GET /ws/{} HTTP/1.1\r\nHost: {addr}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nOrigin: http://evil.example\r\n\r\n",
        created.id
    );
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = [0u8; 128];
    let n = stream.read(&mut buf).await.unwrap();
    let resp = std::str::from_utf8(&buf[..n]).unwrap_or("");
    assert!(
        resp.contains("403"),
        "expected 403 for bad Origin, got: {resp:?} (url={url})"
    );
}

#[tokio::test]
async fn join_ticket_round_trip_and_replay_rejected() {
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
        })
        .unwrap();
    let (ticket, _) = registry.issue_join_ticket(&created.id, "alice").unwrap();
    let claims = registry.consume_ticket(&ticket, &created.id).unwrap();
    assert_eq!(claims.name, "alice");
    assert!(registry.consume_ticket(&ticket, &created.id).is_err());
}

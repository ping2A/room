//! Seatless spectators replay the match from tic 0, then follow live.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{BinaryMsg, ControlMsg, HelloMsg, encode_control, parse_binary, parse_control};
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(ws: &mut Ws, msg: &ControlMsg) {
    ws.send(Message::Text(encode_control(msg).unwrap().into()))
        .await
        .unwrap();
}

async fn next_control(ws: &mut Ws) -> ControlMsg {
    loop {
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timeout")
            .expect("closed")
            .unwrap();
        if let Message::Text(t) = m {
            if let Ok(c) = parse_control(&t) {
                return c;
            }
        }
    }
}

fn hello(room: &str, name: &str) -> ControlMsg {
    ControlMsg::Hello(HelloMsg {
        ticket: room.into(),
        client_version: "t".into(),
        abi_major: 1,
        iwad_fingerprint: "f".into(),
        name: Some(name.into()),
        resume_token: None,
    })
}

#[tokio::test]
async fn late_spectator_gets_every_frame_from_tic0() {
    let registry = RoomRegistry::new(ServerConfig {
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
    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    // Same callsign twice: the second seat gets a suffix.
    send(&mut a, &hello(&room.id, "dup")).await;
    send(&mut b, &hello(&room.id, "dup")).await;
    for ws in [&mut a, &mut b] {
        loop {
            if matches!(next_control(ws).await, ControlMsg::Welcome(_)) {
                break;
            }
        }
        send(ws, &ControlMsg::Ready {}).await;
    }
    let mut names = Vec::new();
    loop {
        match next_control(&mut a).await {
            ControlMsg::Roster { seats } if seats.len() == 2 => {
                names = seats.into_iter().map(|s| s.name).collect();
            }
            ControlMsg::Start { .. } => break,
            _ => {}
        }
    }
    assert_eq!(names, vec!["dup".to_owned(), "dup 2".to_owned()]);

    // Players stay silent: the server clock still releases (neutral) frames.
    tokio::time::sleep(Duration::from_millis(400)).await;

    let (mut bad, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(
        &mut bad,
        &ControlMsg::Watch {
            iwad_fingerprint: "other".into(),
            name: Some("x".into()),
            client_version: "t".into(),
        },
    )
    .await;
    assert!(matches!(
        next_control(&mut bad).await,
        ControlMsg::Reject(r) if r.code == "fingerprint"
    ));

    let (mut s, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(
        &mut s,
        &ControlMsg::Watch {
            iwad_fingerprint: "f".into(),
            name: Some("eye".into()),
            client_version: "t".into(),
        },
    )
    .await;
    let ControlMsg::Watching { room: info, roster } = next_control(&mut s).await else {
        panic!("expected watching");
    };
    assert_eq!(info.max_players, 2);
    assert_eq!(roster.len(), 2);
    assert!(matches!(
        next_control(&mut s).await,
        ControlMsg::Start { tic0: 0, .. }
    ));

    let mut tics = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tics.len() < 30 && tokio::time::Instant::now() < deadline {
        let Ok(Some(Ok(m))) = tokio::time::timeout(Duration::from_secs(2), s.next()).await else {
            break;
        };
        let Message::Binary(b) = m else {
            continue;
        };
        match parse_binary(&b).unwrap() {
            BinaryMsg::TicBatch(frames) => tics.extend(frames.into_iter().map(|f| f.tic)),
            BinaryMsg::TicFrame(f) => tics.push(f.tic),
            _ => {}
        }
    }
    assert!(tics.len() >= 30, "spectator got {} frames", tics.len());
    let expected: Vec<u32> = (0..tics.len() as u32).collect();
    assert_eq!(tics, expected, "frames must be contiguous from tic 0");
}

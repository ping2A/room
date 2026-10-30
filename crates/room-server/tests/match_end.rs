//! Match end: seats report the result, the room closes after the grace
//! period, agreed results reach the leaderboard; room time limits.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{ControlMsg, HelloMsg, ResultRow, encode_control, parse_control};
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(ws: &mut Ws, msg: &ControlMsg) {
    ws.send(Message::Text(encode_control(msg).unwrap().into()))
        .await
        .unwrap();
}

/// Next control message matching `pick` (binary frames skipped).
async fn wait_for<T>(ws: &mut Ws, mut pick: impl FnMut(ControlMsg) -> Option<T>) -> T {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let next = tokio::time::timeout_at(deadline, ws.next())
            .await
            .expect("timed out");
        if let Some(Ok(Message::Text(t))) = next {
            if let Some(v) = parse_control(&t).ok().and_then(&mut pick) {
                return v;
            }
        }
    }
}

async fn join(addr: std::net::SocketAddr, room: &str, name: &str) -> (Ws, u8, u32) {
    let url = format!("ws://{addr}/v2/rooms/{room}/ws");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    send(
        &mut ws,
        &ControlMsg::Hello(HelloMsg {
            ticket: room.into(),
            client_version: "t".into(),
            abi_major: 1,
            iwad_fingerprint: "fp".into(),
            name: Some(name.into()),
            resume_token: None,
        }),
    )
    .await;
    let w = wait_for(&mut ws, |m| match m {
        ControlMsg::Welcome(w) => Some(w),
        _ => None,
    })
    .await;
    send(&mut ws, &ControlMsg::Ready {}).await;
    (ws, w.seat, w.room.time_limit)
}

fn result(winner: Option<u8>) -> ControlMsg {
    ControlMsg::Result {
        winner,
        time_up: false,
        standings: vec![
            ResultRow {
                seat: 0,
                place: 1,
                frags: 1,
                deaths: 0,
                damage: 0,
            },
            ResultRow {
                seat: 1,
                place: 2,
                frags: 0,
                deaths: 1,
                damage: 0,
            },
        ],
    }
}

fn registry(grace: u64) -> RoomRegistry {
    RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        match_end_grace_secs: grace,
        room_time_limit_mins: 15,
        ..ServerConfig::default()
    })
}

#[tokio::test]
async fn agreed_results_close_the_room_and_reach_the_leaderboard() {
    let registry = registry(1);
    let room = registry
        .create_room(CreateRoomRequest {
            time_limit: Some(7),
            ..CreateRoomRequest::default()
        })
        .unwrap();
    assert_eq!(room.settings.time_limit, 7);
    let (addr, _server) = bind_ephemeral(registry.clone()).await.unwrap();
    let (mut a, seat_a, limit) = join(addr, &room.id, "Ann").await;
    let (mut b, _, _) = join(addr, &room.id, "Bob").await;
    assert_eq!((seat_a, limit), (0, 7 * 60), "welcome carries seconds");
    for ws in [&mut a, &mut b] {
        wait_for(ws, |m| matches!(m, ControlMsg::Start { .. }).then_some(())).await;
    }

    send(&mut a, &result(Some(0))).await;
    send(&mut b, &result(Some(0))).await;
    let notice = wait_for(&mut a, |m| match m {
        ControlMsg::Notice { text } => Some(text),
        _ => None,
    })
    .await;
    assert!(notice.starts_with("Match over"), "{notice}");
    let bye = wait_for(&mut b, |m| match m {
        ControlMsg::Bye { code, .. } => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(bye, "over");
    for _ in 0..50 {
        if registry.get(&room.id).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        registry.get(&room.id).is_none(),
        "closed rooms leave the list"
    );

    let top = http_body(addr, "GET /api/leaderboard").await;
    let v: serde_json::Value = serde_json::from_str(&top).unwrap();
    assert_eq!(v[0]["name"], "Ann");
    assert_eq!(
        (v[0]["wins"].as_u64(), v[0]["frags"].as_i64()),
        (Some(1), Some(1))
    );
    assert_eq!(
        (v[1]["name"].as_str(), v[1]["deaths"].as_i64()),
        (Some("Bob"), Some(1))
    );
}

#[tokio::test]
async fn disagreeing_seats_neither_close_nor_score() {
    let registry = registry(0);
    let room = registry.create_room(CreateRoomRequest::default()).unwrap();
    assert_eq!(room.settings.time_limit, 15, "node default");
    let (addr, _server) = bind_ephemeral(registry.clone()).await.unwrap();
    let (mut a, _, _) = join(addr, &room.id, "Ann").await;
    let (mut b, _, _) = join(addr, &room.id, "Bob").await;
    for ws in [&mut a, &mut b] {
        wait_for(ws, |m| matches!(m, ControlMsg::Start { .. }).then_some(())).await;
    }
    send(&mut a, &result(Some(0))).await;
    send(&mut b, &result(Some(1))).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        registry.get(&room.id).is_some(),
        "one seat alone cannot end a room"
    );
    assert_eq!(http_body(addr, "GET /api/leaderboard").await, "[]");
}

#[test]
fn time_limits_are_clamped() {
    let registry = registry(0);
    let make = |m: Option<u16>| {
        registry
            .create_room(CreateRoomRequest {
                time_limit: m,
                ..CreateRoomRequest::default()
            })
            .unwrap()
            .settings
            .time_limit
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _g = rt.enter();
    assert_eq!(make(Some(999)), 180);
    assert_eq!(make(Some(0)), 15);
    assert_eq!(make(None), 15);
    assert_eq!(make(Some(1)), 1);
}

async fn http_body(addr: std::net::SocketAddr, line: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!("{line} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf).into_owned();
    resp.split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or(resp)
}

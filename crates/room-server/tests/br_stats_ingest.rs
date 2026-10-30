//! S-BR1: royale match results reach `br_stats.json`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{ControlMsg, HelloMsg, ResultRow, encode_control, parse_control};
use room_server::{
    BrStats, CreateRoomRequest, ModCatalog, MODE_BR, RoomRegistry, ServerConfig, bind_ephemeral,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn royale_registry(data_dir: &str) -> RoomRegistry {
    let wad = include_bytes!("../../../mods/doom1-royale/royale.wad");
    let mut mods = ModCatalog::default();
    mods.insert("royale", wad.to_vec(), None).unwrap();
    RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        data_dir: data_dir.into(),
        match_end_grace_secs: 1,
        mods: Arc::new(mods),
        ..ServerConfig::default()
    })
}

async fn send(ws: &mut Ws, msg: &ControlMsg) {
    ws.send(Message::Text(encode_control(msg).unwrap().into()))
        .await
        .unwrap();
}

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

async fn join(addr: std::net::SocketAddr, room: &str, name: &str) -> Ws {
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
    let _ = wait_for(&mut ws, |m| match m {
        ControlMsg::Welcome(_) => Some(()),
        _ => None,
    })
    .await;
    send(&mut ws, &ControlMsg::Ready {}).await;
    ws
}

fn rich_result(winner: Option<u8>) -> ControlMsg {
    ControlMsg::Result {
        winner,
        time_up: false,
        standings: vec![
            ResultRow {
                seat: 0,
                place: 1,
                frags: 4,
                deaths: 0,
                damage: 180,
            },
            ResultRow {
                seat: 1,
                place: 2,
                frags: 1,
                deaths: 1,
                damage: 55,
            },
        ],
    }
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

#[tokio::test]
async fn royale_results_update_br_leaderboard() {
    let dir = std::env::temp_dir().join(format!("room-br-ingest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_str().unwrap();
    let registry = royale_registry(d);
    let room = registry
        .create_room(CreateRoomRequest {
            game_mod: Some("royale".into()),
            max_players: 2,
            ..CreateRoomRequest::default()
        })
        .unwrap();
    let (addr, _server) = bind_ephemeral(registry.clone()).await.unwrap();
    let mut a = join(addr, &room.id, "Ann").await;
    let mut b = join(addr, &room.id, "Bob").await;
    for ws in [&mut a, &mut b] {
        wait_for(ws, |m| matches!(m, ControlMsg::Start { .. }).then_some(())).await;
    }
    send(&mut a, &rich_result(Some(0))).await;
    send(&mut b, &rich_result(Some(0))).await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    let body = http_body(addr, "GET /api/br/leaderboard").await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v[0]["name"], "Ann");
    assert_eq!(v[0]["damage"].as_i64(), Some(180));
    assert_eq!(v[0]["kills"].as_i64(), Some(4));

    let st = BrStats::open(d);
    assert_eq!(st.top(MODE_BR, 1)[0].top5s, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

//! CHECKSUM mismatch → `desync` control (docs/22 S5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{
    BinaryMsg, ControlMsg, HelloMsg, TicChecksum, encode_binary, encode_control, parse_control,
};
use room_server::{CreateRoomRequest, RoomRegistry, ServerConfig, bind_ephemeral};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn checksum_mismatch_broadcasts_desync() {
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
    for (ws, name) in [(&mut a, "a"), (&mut b, "b")] {
        ws.send(Message::Text(
            encode_control(&ControlMsg::Hello(HelloMsg {
                ticket: room.id.clone(),
                client_version: "t".into(),
                abi_major: 1,
                iwad_fingerprint: "f".into(),
                name: Some(name.into()),
                resume_token: None,
            }))
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    }
    for ws in [&mut a, &mut b] {
        wait_welcome(ws).await;
        ws.send(Message::Text(
            encode_control(&ControlMsg::Ready {}).unwrap().into(),
        ))
        .await
        .unwrap();
    }
    for ws in [&mut a, &mut b] {
        wait_start(ws).await;
    }

    a.send(Message::Binary(
        encode_binary(&BinaryMsg::Checksum(TicChecksum {
            tic: 35,
            checksum: 1,
        }))
        .into(),
    ))
    .await
    .unwrap();
    b.send(Message::Binary(
        encode_binary(&BinaryMsg::Checksum(TicChecksum {
            tic: 35,
            checksum: 2,
        }))
        .into(),
    ))
    .await
    .unwrap();

    let mut saw = false;
    for _ in 0..40 {
        tokio::select! {
            m = a.next() => {
                if let Some(Ok(Message::Text(t))) = m {
                    if let Ok(ControlMsg::Desync { tic, seats }) = parse_control(&t) {
                        assert_eq!(tic, 35);
                        assert!(!seats.is_empty());
                        saw = true;
                        break;
                    }
                }
            }
            m = b.next() => {
                if let Some(Ok(Message::Text(t))) = m {
                    if let Ok(ControlMsg::Desync { .. }) = parse_control(&t) {
                        saw = true;
                        break;
                    }
                }
            }
        }
    }
    assert!(saw, "expected desync notice");
}

async fn wait_welcome(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    for _ in 0..40 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if matches!(parse_control(&t), Ok(ControlMsg::Welcome(_))) {
                return;
            }
        }
    }
    panic!("no welcome");
}

async fn wait_start(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    for _ in 0..40 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if matches!(parse_control(&t), Ok(ControlMsg::Start { .. })) {
                return;
            }
        }
    }
    panic!("no start");
}

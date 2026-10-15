//! Admin API smoke (docs/22 §7 M3 bootstrap).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{ControlMsg, HelloMsg, encode_control, parse_control};
use room_server::{
    AppState, CreateRoomRequest, RoomRegistry, ServerConfig, admin_router, bind_ephemeral,
    public_router,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn admin_summary_and_close_audited() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    });
    let created = registry
        .create_room(CreateRoomRequest {
            map: "E1M1".into(),
            skill: 3,
            max_players: 2,
            enable_verifier: false,
            visibility: "public".into(),
        })
        .unwrap();
    let (addr, _server) = bind_ephemeral(registry.clone()).await.unwrap();

    let spa = http_get(addr, "/admin").await;
    assert!(
        spa.contains("Ops pit") || spa.contains("operator") || spa.contains("ROOM"),
        "{spa}"
    );
    let css = http_get(addr, "/admin/assets/app.css").await;
    assert!(css.contains("--accent"), "{css}");
    let js = http_get(addr, "/admin/assets/app.js").await;
    assert!(js.contains("/admin/api/v1/summary"), "{js}");

    let summary = http_get(addr, "/admin/api/v1/summary").await;
    assert!(summary.contains("\"rooms\":1"), "{summary}");

    let metrics = http_get(addr, "/metrics").await;
    assert!(metrics.contains("room_server_rooms 1"), "{metrics}");

    let close = http_post(
        addr,
        &format!("/admin/api/v1/rooms/{}/close", created.id),
        "",
    )
    .await;
    assert!(
        close.contains("204") || close.starts_with("HTTP/1.1 204"),
        "{close}"
    );

    let audit = http_get(addr, "/admin/api/v1/audit").await;
    assert!(audit.contains("close_room"), "{audit}");
    assert_eq!(registry.len(), 0);
}

#[tokio::test]
async fn admin_bind_separate_listener() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        admin_bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    });
    // `serve` is used for dual-bind; for the test, spin public+admin like production.
    let public_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let admin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let public_addr = public_listener.local_addr().unwrap();
    let admin_addr = admin_listener.local_addr().unwrap();
    let admin = registry.admin();
    let state = AppState {
        registry: registry.clone(),
        admin,
    };
    let public = public_router(state.clone());
    let admin = admin_router(state);
    tokio::spawn(async move {
        let _ = axum::serve(
            public_listener,
            public.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });
    tokio::spawn(async move {
        let _ = axum::serve(
            admin_listener,
            admin.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    let spa = http_get(admin_addr, "/admin").await;
    assert!(
        spa.contains("Ops pit") || spa.contains("operator") || spa.contains("ROOM"),
        "{spa}"
    );
    let missing = http_get(public_addr, "/admin").await;
    assert!(
        missing.contains("404"),
        "admin must not be on game bind: {missing}"
    );
    let health = http_get(public_addr, "/healthz").await;
    assert!(health.contains("ok"), "{health}");
}

#[tokio::test]
async fn admin_token_required_when_set() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        admin_token: "secret".into(),
        ..ServerConfig::default()
    });
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();
    let denied = http_get(addr, "/admin/api/v1/summary").await;
    assert!(denied.contains("401"), "{denied}");
    let ok = http_get_auth(addr, "/admin/api/v1/summary", "secret").await;
    assert!(ok.contains("\"rooms\":0"), "{ok}");
}

#[tokio::test]
async fn admin_notice_reaches_v2_client() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    });
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
    let url = format!("ws://{addr}/v2/rooms/{}/ws", created.id);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.send(Message::Text(
        encode_control(&ControlMsg::Hello(HelloMsg {
            ticket: created.id.clone(),
            client_version: "t".into(),
            abi_major: 1,
            iwad_fingerprint: "fp".into(),
            name: Some("p".into()),
            resume_token: None,
        }))
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if matches!(parse_control(&t), Ok(ControlMsg::Welcome(_))) {
                break;
            }
        }
    }
    let body = r#"{"text":"hello operators"}"#;
    let resp = http_post(
        addr,
        &format!("/admin/api/v1/rooms/{}/notice", created.id),
        body,
    )
    .await;
    assert!(resp.contains("204"), "{resp}");
    let mut noticed = false;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(ControlMsg::Notice { text }) = parse_control(&t) {
                assert_eq!(text, "hello operators");
                noticed = true;
                break;
            }
        }
    }
    assert!(noticed);
}

async fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
    http_raw(addr, "GET", path, "", None).await
}

async fn http_get_auth(addr: std::net::SocketAddr, path: &str, token: &str) -> String {
    http_raw(addr, "GET", path, "", Some(token)).await
}

async fn http_post(addr: std::net::SocketAddr, path: &str, body: &str) -> String {
    http_raw(addr, "POST", path, body, None).await
}

async fn http_delete(addr: std::net::SocketAddr, path: &str) -> String {
    http_raw(addr, "DELETE", path, "", None).await
}

#[tokio::test]
async fn ban_by_name_blocks_join_and_hello() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    });
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

    let add = http_post(
        addr,
        "/admin/api/v1/bans",
        r#"{"scope":"name","value":"Griefer","reason":"test","exp":0}"#,
    )
    .await;
    assert!(add.contains("200") || add.contains("\"scope\""), "{add}");

    let join = http_post(
        addr,
        &format!("/api/rooms/{}/join", created.id),
        r#"{"name":"griefer"}"#,
    )
    .await;
    assert!(
        join.contains("403"),
        "join must refuse banned pseudo: {join}"
    );

    let url = format!("ws://{addr}/v2/rooms/{}/ws", created.id);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.send(Message::Text(
        encode_control(&ControlMsg::Hello(HelloMsg {
            ticket: created.id.clone(),
            client_version: "t".into(),
            abi_major: 1,
            iwad_fingerprint: "fp".into(),
            name: Some("griefer".into()),
            resume_token: None,
        }))
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    let mut banned = false;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(ControlMsg::Reject(r)) = parse_control(&t) {
                assert_eq!(r.code, "banned");
                banned = true;
                break;
            }
        }
    }
    assert!(banned, "hello must reject banned name");

    let list = http_get(addr, "/admin/api/v1/bans").await;
    assert!(
        list.contains("Griefer") || list.contains("griefer"),
        "{list}"
    );
    // Remove via DELETE
    if let Some(id) = list
        .split("\"id\":\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
    {
        let del = http_delete(addr, &format!("/admin/api/v1/bans/{id}")).await;
        assert!(del.contains("204") || del.contains("200"), "{del}");
    }
}

#[tokio::test]
async fn ban_name_regex_and_auth_key_scopes() {
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        ..ServerConfig::default()
    });
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

    let add = http_post(
        addr,
        "/admin/api/v1/bans",
        r#"{"scope":"name_regex","value":"bot*","reason":"bots","exp":0}"#,
    )
    .await;
    assert!(add.contains("name_regex") || add.contains("200"), "{add}");

    let join = http_post(
        addr,
        &format!("/api/rooms/{}/join", created.id),
        r#"{"name":"Bot9000"}"#,
    )
    .await;
    assert!(join.contains("403"), "{join}");

    let add_key = http_post(
        addr,
        "/admin/api/v1/bans",
        r#"{"scope":"auth_key","value":"deadbeefcafe","reason":"stolen","exp":0}"#,
    )
    .await;
    assert!(
        add_key.contains("auth_key") || add_key.contains("200"),
        "{add_key}"
    );

    let url = format!("ws://{addr}/v2/rooms/{}/ws", created.id);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws.send(Message::Text(
        encode_control(&ControlMsg::Hello(HelloMsg {
            ticket: created.id.clone(),
            client_version: "t".into(),
            abi_major: 1,
            iwad_fingerprint: "deadbeefcafe".into(),
            name: Some("alice".into()),
            resume_token: None,
        }))
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    let mut banned = false;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(ControlMsg::Reject(r)) = parse_control(&t) {
                assert_eq!(r.code, "banned");
                banned = true;
                break;
            }
        }
    }
    assert!(banned, "fingerprint must match auth_key ban");
}

#[tokio::test]
async fn admin_store_persists_bans_and_audit() {
    let dir = std::env::temp_dir().join(format!("room-admin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = room_server::AdminStore::open(&dir).unwrap();
    store.audit("admin", "test", "t1", "detail");
    store.add_ban(room_server::Ban {
        id: "b1".into(),
        scope: "name".into(),
        value: "grief".into(),
        reason: "test".into(),
        exp: 0,
    });
    drop(store);
    let reloaded = room_server::AdminStore::open(&dir).unwrap();
    assert_eq!(reloaded.bans().len(), 1);
    assert_eq!(reloaded.bans()[0].value, "grief");
    assert!(
        reloaded
            .audit_list()
            .iter()
            .any(|e| e.action == "test" && e.target == "t1")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

async fn http_raw(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    token: Option<&str>,
) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    if let Some(t) = token {
        req.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    if method == "POST" {
        req.push_str("Content-Type: application/json\r\n");
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    req.push_str("\r\n");
    req.push_str(body);
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf).into_owned()
}

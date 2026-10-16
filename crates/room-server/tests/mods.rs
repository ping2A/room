//! Gameplay mods: catalog listing, file download, room creation and `welcome.room.mod`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::{SinkExt, StreamExt};
use room_proto::{ControlMsg, HelloMsg, content_fingerprint, encode_control, parse_control};
use room_server::{
    CreateRoomRequest, ModCatalog, RegistryError, RoomRegistry, ServerConfig, bind_ephemeral,
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

fn pwad(lumps: &[(&str, &[u8])]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut dir = Vec::new();
    let mut off = 12u32;
    for (name, body) in lumps {
        let mut n = [0u8; 8];
        n[..name.len()].copy_from_slice(name.as_bytes());
        dir.extend_from_slice(&off.to_le_bytes());
        dir.extend_from_slice(&(body.len() as u32).to_le_bytes());
        dir.extend_from_slice(&n);
        data.extend_from_slice(body);
        off += body.len() as u32;
    }
    let mut out = b"PWAD".to_vec();
    out.extend_from_slice(&(lumps.len() as u32).to_le_bytes());
    out.extend_from_slice(&off.to_le_bytes());
    out.extend_from_slice(&data);
    out.extend_from_slice(&dir);
    out
}

fn registry_with_mod() -> (RoomRegistry, Vec<u8>) {
    let wad = pwad(&[
        ("ROOMMOD", b"[mod]\nid = \"t\"\nname = \"Test Mod\"\n"),
        ("TESTMOD", b"\0asm\x01\0\0\0"),
    ]);
    let mut mods = ModCatalog::default();
    mods.insert("testmod", wad.clone(), None).unwrap();
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        mods: Arc::new(mods),
        ..ServerConfig::default()
    });
    (registry, wad)
}

fn request(game_mod: Option<&str>) -> CreateRoomRequest {
    CreateRoomRequest {
        game_mod: game_mod.map(str::to_owned),
        ..CreateRoomRequest::default()
    }
}

#[tokio::test]
async fn rooms_carry_the_mod_to_every_client() {
    let (registry, wad) = registry_with_mod();
    let fp = format!("{:016x}", content_fingerprint(&wad));

    assert!(matches!(
        registry.create_room(request(Some("missing"))),
        Err(RegistryError::UnknownMod)
    ));
    let vanilla = registry.create_room(request(Some(""))).unwrap();
    assert!(vanilla.settings.game_mod.is_none());
    let created = registry.create_room(request(Some("testmod"))).unwrap();
    let m = created.settings.game_mod.clone().unwrap();
    assert_eq!((m.name.as_str(), m.title.as_str()), ("testmod", "Test Mod"));
    assert_eq!(m.fingerprint, fp);

    let (addr, _server) = bind_ephemeral(registry).await.unwrap();

    let list = http(addr, "GET /api/mods", "").await;
    assert!(list.contains("\"name\":\"testmod\""), "{list}");
    assert!(list.contains("/api/mods/testmod.wad"), "{list}");

    let file = http_raw(addr, "GET /api/mods/testmod.wad", "").await;
    let split = file.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&file[..split]).to_ascii_lowercase();
    assert!(head.contains("200 ok"), "{head}");
    assert!(head.contains("access-control-allow-origin: *"), "{head}");
    assert!(head.contains(&fp), "etag carries the fingerprint: {head}");
    assert_eq!(&file[split + 4..], wad.as_slice());

    assert!(
        http(addr, "GET /api/mods/missing.wad", "")
            .await
            .contains("404")
    );
    assert!(
        http(addr, "GET /api/mods/testmod", "")
            .await
            .contains("404")
    );
    let bad = http(addr, "POST /api/rooms", r#"{"mod":"missing"}"#).await;
    assert!(bad.contains("400"), "{bad}");
    let listed = http(addr, "GET /api/rooms", "").await;
    assert!(listed.contains("\"mod\":{\"name\":\"testmod\""), "{listed}");

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
    let mut welcomed = None;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(ControlMsg::Welcome(w)) = parse_control(&t) {
                welcomed = Some(w);
                break;
            }
        }
    }
    let w = welcomed.expect("welcome");
    assert_eq!(w.room.game_mod, Some(m));
}

#[tokio::test]
async fn admins_allow_and_disallow_mods() {
    let dir = std::env::temp_dir().join(format!("room-mods-allow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wad = pwad(&[("ROOMMOD", b"[mod]\nname = \"Arena\"\n")]);
    std::fs::write(dir.join("arena.wad"), wad).unwrap();
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        mods: Arc::new(ModCatalog::load_dir(dir.to_str().unwrap())),
        ..ServerConfig::default()
    });
    let (addr, _server) = bind_ephemeral(registry.clone()).await.unwrap();

    assert_eq!(http_body(addr, "GET /api/mods").await, "[]");
    assert!(matches!(
        registry.create_room(request(Some("arena"))),
        Err(RegistryError::ModNotAllowed)
    ));
    let rows = http_body(addr, "GET /admin/api/v1/mods").await;
    assert!(
        rows.contains("\"name\":\"arena\"") && rows.contains("\"allowed\":false"),
        "{rows}"
    );

    let allow = http(addr, "POST /admin/api/v1/mods/arena", r#"{"allowed":true}"#).await;
    assert!(allow.starts_with("HTTP/1.1 204"), "{allow}");
    assert!(
        http_body(addr, "GET /api/mods")
            .await
            .contains("\"title\":\"Arena\"")
    );
    let room = registry.create_room(request(Some("arena"))).unwrap();
    assert_eq!(room.settings.game_mod.unwrap().name, "arena");

    let missing = http(addr, "POST /admin/api/v1/mods/ghost", r#"{"allowed":true}"#).await;
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    http(
        addr,
        "POST /admin/api/v1/mods/arena",
        r#"{"allowed":false}"#,
    )
    .await;
    assert_eq!(http_body(addr, "GET /api/mods").await, "[]");
    let file = http(addr, "GET /api/mods/arena.wad", "").await;
    assert!(
        file.starts_with("HTTP/1.1 200"),
        "running rooms still download it"
    );
    let audit = http_body(addr, "GET /admin/api/v1/audit").await;
    assert!(
        audit.contains("allow_mod") && audit.contains("disallow_mod"),
        "{audit}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

async fn http_body(addr: std::net::SocketAddr, line: &str) -> String {
    let resp = http(addr, line, "").await;
    resp.split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or(resp)
}

#[test]
fn catalog_loads_built_example_mods() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../mods/doom1-bounty");
    if !dir.join("bounty.wad").is_file() {
        eprintln!("skip: run ./scripts/room-server.sh mods");
        return;
    }
    let cat = ModCatalog::load_dir(dir.to_str().unwrap());
    let m = cat.get("bounty").expect("bounty.wad in catalog");
    assert_eq!(m.info.title, "Bounty Hunter");
    assert_eq!(
        m.info.fingerprint,
        format!("{:016x}", content_fingerprint(&m.bytes))
    );
    let royale = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../mods/doom1-royale");
    if royale.join("royale.wad").is_file() {
        let cat = ModCatalog::load_dir(royale.to_str().unwrap());
        assert_eq!(
            cat.get("royale").expect("royale.wad").info.title,
            "Battle Royale"
        );
    }
}

async fn http(addr: std::net::SocketAddr, line: &str, body: &str) -> String {
    String::from_utf8_lossy(&http_raw(addr, line, body).await).into_owned()
}

async fn http_raw(addr: std::net::SocketAddr, line: &str, body: &str) -> Vec<u8> {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "{line} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    buf
}

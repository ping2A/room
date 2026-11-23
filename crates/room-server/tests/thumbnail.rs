//! `GET /api/presets/{id}/thumbnail.png` (`thumbnails` feature, `docs/40` SR10).
#![cfg(feature = "thumbnails")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_server::{ContentStore, Preset, PresetConfig, RoomRegistry, ServerConfig, bind_ephemeral};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn get(addr: std::net::SocketAddr, path: &str) -> (String, Vec<u8>) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    (
        String::from_utf8_lossy(&buf[..split]).into_owned(),
        buf[split + 4..].to_vec(),
    )
}

#[tokio::test]
async fn preset_thumbnail_is_a_png() {
    let wad =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    if !wad.is_file() {
        eprintln!("skip: testdata/freedoom/freedoom2.wad missing (scripts/fetch-freedoom.sh)");
        return;
    }
    let dir = std::env::temp_dir().join(format!("room-thumb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&wad, dir.join("freedoom2.wad")).unwrap();
    let content = ContentStore::open(Some(dir.clone()), "", 64);
    content
        .set_presets(
            PresetConfig {
                presets: vec![Preset {
                    id: "fd2".into(),
                    title: "Freedoom".into(),
                    description: String::new(),
                    game: "doom".into(),
                    iwad: Some("freedoom2.wad".into()),
                    content: Vec::new(),
                    maps: vec!["MAP01".into(), "MAP02".into()],
                    default_map: "MAP01".into(),
                    game_mod: None,
                    skill: 0,
                    duke_game_mode: 1,
                    duke_friendly_fire: false,
                    doom_mode: None,
                }],
                ..PresetConfig::default()
            },
            &|_| true,
        )
        .unwrap();
    let registry = RoomRegistry::new(ServerConfig {
        bind: "127.0.0.1:0".into(),
        content: Arc::new(content),
        ..ServerConfig::default()
    });
    let (addr, _server) = bind_ephemeral(registry).await.unwrap();

    let (head, body) = get(addr, "/api/presets/fd2/thumbnail.png?w=160&h=120").await;
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: image/png"),
        "{head}"
    );
    assert!(body.starts_with(&[0x89, b'P', b'N', b'G']));
    // IHDR: width and height.
    assert_eq!(&body[16..24], &[0, 0, 0, 160, 0, 0, 0, 120]);
    // Served from the cache the second time: same bytes.
    let (_, again) = get(addr, "/api/presets/fd2/thumbnail.png?w=160&h=120").await;
    assert_eq!(body, again);

    let (head, _) = get(addr, "/api/presets/fd2/thumbnail.png?map=MAP02").await;
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    // Outside the preset's maps, malformed, unknown preset.
    let (head, _) = get(addr, "/api/presets/fd2/thumbnail.png?map=MAP09").await;
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    let (head, _) = get(addr, "/api/presets/fd2/thumbnail.png?map=..%2Fx").await;
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    let (head, _) = get(addr, "/api/presets/nope/thumbnail.png").await;
    assert!(head.starts_with("HTTP/1.1 404"), "{head}");
    let _ = std::fs::remove_dir_all(&dir);
}

//! Headless verifier smoke (feature `verifier`).
#![cfg(feature = "verifier")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_proto::{NetTicCmd, TicFrameMsg};
use room_server::ServerConfig;
use std::path::PathBuf;

#[tokio::test]
async fn headless_verifier_produces_checksum() {
    let wad =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    if !wad.is_file() {
        eprintln!("skip: freedoom2.wad missing");
        return;
    }
    let _ = ServerConfig::default();
    let (tx, mut rx) =
        room_server::spawn_verifier_for_test(wad, "MAP01", 3, 2026, 2).expect("spawn verifier");
    for tic in 0..36u32 {
        tx.send(TicFrameMsg {
            tic,
            seat_mask: 0b11,
            cmds: vec![NetTicCmd::default(), NetTicCmd::default()],
        })
        .await
        .unwrap();
    }
    drop(tx);
    let mut got = None;
    for _ in 0..400 {
        while let Ok(c) = rx.try_recv() {
            if c.tic == 35 {
                got = Some(c);
            }
        }
        if got.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let c = got.expect("expected verifier checksum at tic 35");
    assert_eq!(c.tic, 35);
    assert_ne!(c.checksum, 0);
}

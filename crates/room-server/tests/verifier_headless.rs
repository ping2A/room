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
    let (tx, mut rx) = room_server::spawn_verifier_for_test(wad, None, "MAP01", 3, 2026, 2, 0, false)
        .expect("spawn verifier");
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

/// Checksums for `tics` idle tics on E1M1, optionally with a mod mounted.
async fn verifier_checksums(
    iwad: PathBuf,
    game_mod: Option<(String, bytes::Bytes)>,
    tics: u32,
) -> Vec<(u32, u64)> {
    let (tx, mut rx) = room_server::spawn_verifier_for_test(iwad, game_mod, "E1M1", 4, 99, 2, 0, false)
        .expect("verifier");
    for tic in 0..tics {
        tx.send(TicFrameMsg {
            tic,
            seat_mask: 0b11,
            cmds: vec![NetTicCmd::default(), NetTicCmd::default()],
        })
        .await
        .unwrap();
    }
    drop(tx);
    let want = (tics.saturating_sub(1) / 35) as usize;
    let mut out = Vec::new();
    for _ in 0..1000 {
        while let Ok(c) = rx.try_recv() {
            out.push((c.tic, c.checksum));
        }
        if out.len() >= want {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    out
}

#[tokio::test]
async fn verifier_runs_the_room_mod() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let iwad = root.join("testdata/doom/doom1.wad");
    let wad = root.join("mods/doom1-bounty/bounty.wad");
    if !iwad.is_file() || !wad.is_file() {
        eprintln!("skip: needs doom1.wad and ./scripts/room-server.sh mods");
        return;
    }
    let bytes = bytes::Bytes::from(std::fs::read(&wad).unwrap());
    let vanilla = verifier_checksums(iwad.clone(), None, 281).await;
    let modded =
        verifier_checksums(iwad.clone(), Some(("bounty".into(), bytes.clone())), 281).await;
    let again = verifier_checksums(iwad, Some(("bounty".into(), bytes)), 281).await;
    assert_eq!(vanilla.len(), 8, "{vanilla:?}");
    assert_eq!(modded, again, "modded verifier must be deterministic");
    // The first bounty (5 s = tic 175) buffs a monster, so the worlds diverge after it.
    let diverged: Vec<u32> = vanilla
        .iter()
        .zip(&modded)
        .filter(|(a, b)| a.1 != b.1)
        .map(|(a, _)| a.0)
        .collect();
    assert!(
        diverged.first().is_some_and(|&t| t >= 175),
        "mod must change the world only once it acts: {diverged:?}"
    );
}

#[tokio::test]
async fn verifier_runs_a_battle_royale_room() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let iwad = root.join("testdata/doom/doom1.wad");
    let wad = root.join("mods/doom1-royale/royale.wad");
    if !iwad.is_file() || !wad.is_file() {
        eprintln!("skip: needs doom1.wad and ./scripts/room-server.sh mods");
        return;
    }
    let bytes = bytes::Bytes::from(std::fs::read(&wad).unwrap());
    let vanilla = verifier_checksums(iwad.clone(), None, 106).await;
    let royale =
        verifier_checksums(iwad.clone(), Some(("royale".into(), bytes.clone())), 106).await;
    let again = verifier_checksums(iwad, Some(("royale".into(), bytes)), 106).await;
    assert_eq!(royale.len(), 3, "{royale:?}");
    assert_eq!(
        royale, again,
        "battle royale verifier must be deterministic"
    );
    assert_ne!(
        vanilla[0], royale[0],
        "monsters removed and loot dropped at map start"
    );
}

//! Rollback building blocks: a speculative copy of the world, run through
//! the same commands, must land exactly where the real game does, and making
//! it must not touch the real game. Skips without Doom II.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
use room_game::{GameOptions, Headless, Skill};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, ClassRegistry, StateTable, TicCmd};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

fn deathmatch() -> Option<Headless> {
    let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom2.wad");
    if !wad.is_file() {
        eprintln!("skip: no doom2.wad");
        return None;
    }
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);
    let mut g = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 5,
        coop_players: 4,
        deathmatch: true,
        fill_bots: true,
        bot_first_seat: 2,
        ..Default::default()
    });
    g.set_defs(classes, states);
    g.start_custom_map(map, Skill::new(3)).unwrap();
    Some(g)
}

fn script(n: usize, seed: u32) -> Vec<Vec<TicCmd>> {
    let mut r = seed;
    let mut next = move || {
        r = r.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        r >> 8
    };
    (0..n)
        .map(|_| {
            let mut human = || TicCmd {
                forward_move: 40 * 256,
                side_move: (i16::try_from(next() % 3).unwrap() - 1) * 20 * 256,
                yaw: i16::try_from(next() % 800).unwrap() - 400,
                buttons: Buttons(if next() % 3 == 0 { Buttons::ATTACK } else { 0 }),
                ..TicCmd::default()
            };
            vec![human(), human(), TicCmd::default(), TicCmd::default()]
        })
        .collect()
}

#[test]
fn a_speculative_copy_lands_exactly_where_the_game_does() {
    let Some(mut g) = deathmatch() else { return };
    for cmds in script(300, 1) {
        g.tick(&cmds);
    }
    assert!(g.can_speculate());
    let ahead = script(200, 2);
    let before = g.checksum();
    let t = Instant::now();
    let copies: Vec<_> = [1, 8, 200]
        .iter()
        .map(|&n| g.speculate(&ahead[..n]).unwrap().checksum())
        .collect();
    let spec_time = t.elapsed();
    assert_eq!(g.checksum(), before, "speculating changed the real game");
    let mut real = Vec::new();
    for cmds in &ahead {
        g.tick(cmds);
        real.push(g.checksum().unwrap());
    }
    assert_eq!(copies, vec![real[0], real[7], real[199]]);
    // Rough cost, for the record (a copy plus 209 tics).
    let t = Instant::now();
    let w = g.speculate(&[]).unwrap();
    eprintln!(
        "copy {:?}, three speculations {:?}, {} actors",
        t.elapsed(),
        spec_time,
        w.actors.len()
    );
}

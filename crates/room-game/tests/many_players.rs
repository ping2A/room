//! UZ12: more than 8 players. A 16-seat headless soak (every seat moving,
//! turning and firing for a minute of game time, twice, same checksums) and
//! a 64-seat spawn check.
//!
//! Skips when `testdata/doom/doom1.wad` is absent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_decorate::parse_decorate;
use room_game::{GameOptions, Headless, Skill};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::{Buttons, ClassRegistry, MAX_PLAYERS, PlayerState, StateTable, TicCmd};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

fn start(wad: &PathBuf, seats: u8, seed: u32) -> Headless {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = parse_decorate(src).expect("decorate");
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).expect("classes"));
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let vfs = VfsBuilder::new().mount_path(wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: seed,
        record_demo: false,
        coop_players: seats,
        ..Default::default()
    });
    game.set_defs(classes, states);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    game
}

/// Seat `s` at tic `t`: walk in loops, turn, fire now and then (no RNG).
fn cmd_for(seat: usize, tic: u32) -> TicCmd {
    let phase = (tic / 35 + seat as u32) % 6;
    let mut buttons = 0;
    if (tic + seat as u32 * 7) % 50 < 6 {
        buttons |= Buttons::ATTACK;
    }
    if tic % 140 == seat as u32 {
        buttons |= Buttons::USE;
    }
    TicCmd {
        buttons: Buttons(buttons),
        forward_move: if phase < 4 { 25 } else { -15 },
        side_move: if phase == 2 { 24 } else { 0 },
        yaw: if phase == 5 { 400 } else { (seat as i16 % 5 - 2) * 40 },
        ..TicCmd::default()
    }
}

fn soak(wad: &PathBuf, seats: usize, tics: u32) -> (Vec<u64>, Headless) {
    let mut game = start(wad, seats as u8, 1234);
    let mut sums = Vec::new();
    for tic in 0..tics {
        let cmds: Vec<TicCmd> = (0..seats).map(|s| cmd_for(s, tic)).collect();
        game.tick(&cmds);
        if tic % 175 == 0 {
            sums.push(game.world().unwrap().checksum());
        }
    }
    (sums, game)
}

#[test]
fn sixteen_seat_soak_is_deterministic_and_every_seat_plays() {
    let Some(wad) = doom1() else {
        eprintln!("skip: doom1.wad absent");
        return;
    };
    let (a, game) = soak(&wad, 16, 35 * 60);
    let (b, _) = soak(&wad, 16, 35 * 60);
    assert_eq!(a, b, "same inputs, same checksums");
    let w = game.world().unwrap();
    let mut moved = 0;
    for seat in 0..16 {
        let p = w.players[seat].as_ref().unwrap_or_else(|| panic!("seat {seat} in game"));
        let mo = p.mo.expect("pawn");
        let a = w.actors.get(mo).expect("actor");
        if p.player_state != PlayerState::Dead && a.pos != a.spawn_point {
            moved += 1;
        }
    }
    assert!(moved >= 12, "most seats moved ({moved}/16)");
}

#[test]
fn sixty_four_seats_spawn_apart_and_up_to_eight_hash_as_before() {
    let Some(wad) = doom1() else {
        eprintln!("skip: doom1.wad absent");
        return;
    };
    let game = start(&wad, MAX_PLAYERS as u8, 1);
    let w = game.world().unwrap();
    let pawns: Vec<_> = (0..MAX_PLAYERS)
        .map(|s| {
            let mo = w.players[s].as_ref().unwrap_or_else(|| panic!("seat {s}")).mo.unwrap();
            w.actors.get(mo).unwrap().pos
        })
        .collect();
    // Seats past the map's starts stand apart rather than in one spot.
    let mut stacked = 0;
    for i in 8..pawns.len() {
        for j in 0..i {
            let (dx, dy) = (pawns[i].x() - pawns[j].x(), pawns[i].y() - pawns[j].y());
            // Room collides pawns as discs (radius 16 each).
            if dx * dx + dy * dy < 32.0 * 32.0 {
                eprintln!("overlap {i} {j} {:?} {:?}", pawns[i], pawns[j]);
                stacked += 1;
            }
        }
    }
    assert_eq!(stacked, 0, "overlapping pawns");
    // A seat past 8 changes the hash only while it is in the game.
    let mut four = start(&wad, 4, 9);
    let before = four.world().unwrap().checksum();
    four.world_mut().unwrap().set_player_display_name(40, "");
    assert_eq!(four.world().unwrap().checksum(), before);
}

//! The AssemblyScript example mod (`mods/doom1-bounty/bounty.wad`, built by
//! `scripts/room-server.sh mods`) end to end: bounty pick, payout,
//! final Warden spawn, and the editable source lump in the PWAD.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use room_game::{GameOptions, Headless, Skill, discover_mods, load_embedded_engine_data};
use room_map::{MapGeometry, load_map};
use room_nodebuild::ensure_nodes;
use room_sim::script_mods::ModSource;
use room_sim::{ActorId, TicCmd};
use room_testkit::{MapBuilder, rect};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

const BOUNTY_WAD: &[u8] = include_bytes!("../../../mods/doom1-bounty/bounty.wad");

fn bounty_mods() -> Vec<ModSource> {
    let vfs = VfsBuilder::new()
        .mount_memory("bounty.wad", Bytes::from_static(BOUNTY_WAD))
        .unwrap()
        .build()
        .unwrap();
    let (mods, errors) = discover_mods(&vfs);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(mods.len(), 1);
    mods
}

fn arena() -> MapGeometry {
    let mut b = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(128.0, 128.0, 0);
    for (x, y) in [(1500.0, 1500.0), (1700.0, 400.0), (400.0, 1700.0)] {
        b = b.thing(3001, x, y, 0);
    }
    let mut map = b.build_udmf("MAP01").unwrap();
    room_nodebuild::apply_nodes(&mut map);
    map
}

struct Run {
    game: Headless,
    msgs: Vec<String>,
}

impl Run {
    fn new(map: MapGeometry, skill: u8) -> Self {
        let data = load_embedded_engine_data().unwrap();
        let skill = Skill::new(skill);
        let mut game = Headless::new(GameOptions {
            skill,
            ..Default::default()
        });
        game.set_defs(data.classes, data.states);
        assert_eq!(game.set_script_mods(bounty_mods()).unwrap(), ["bounty"]);
        game.start_custom_map(map, skill).unwrap();
        Self {
            game,
            msgs: Vec::new(),
        }
    }

    fn tics(&mut self, n: u32) {
        for _ in 0..n {
            self.game.tick(&[TicCmd::default()]);
            self.msgs
                .extend(self.game.world_mut().unwrap().drain_messages());
            self.msgs.extend(self.game.drain_script_log());
        }
        assert!(
            !self.msgs.iter().any(|m| m.contains("disabled")),
            "{:?}",
            self.msgs
        );
    }

    fn saw(&self, prefix: &str) -> bool {
        self.msgs.iter().any(|m| m.starts_with(prefix))
    }

    fn player(&self) -> ActorId {
        self.game.world().unwrap().players[0]
            .as_ref()
            .unwrap()
            .mo
            .unwrap()
    }

    fn monsters(&self) -> Vec<ActorId> {
        let w = self.game.world().unwrap();
        w.thinkers
            .default
            .iter()
            .copied()
            .filter(|&a| {
                w.actors
                    .get(a)
                    .is_some_and(|a| a.health > 0 && a.flags & room_sim::FLAG_COUNTKILL != 0)
            })
            .collect()
    }

    fn kill_as_player(&mut self, id: ActorId) {
        let p = self.player();
        room_sim::damage_mobj(
            self.game.world_mut().unwrap(),
            id,
            Some(p),
            Some(p),
            100_000,
        );
    }
}

#[test]
fn bounty_mod_picks_pays_out_and_spawns_the_warden() {
    let mut r = Run::new(arena(), 4);
    assert!(r.game.classes().find_name("Warden").is_some());
    r.tics(4 * 35);
    assert!(
        r.saw("Bounty Hunter: skill 4 - first bounty in 5s"),
        "{:?}",
        r.msgs
    );
    assert!(!r.saw("New bounty"), "no bounty before 5 s");
    r.tics(2 * 35);
    // UV: 300% of the imp's 60 HP.
    assert!(r.saw("New bounty: DoomImp (180 HP)"), "{:?}", r.msgs);

    // Find the marked imp (the one with 180 HP) and claim it.
    let bounty = r
        .monsters()
        .into_iter()
        .find(|&a| r.game.world().unwrap().actors.get(a).unwrap().health == 180)
        .expect("bounty imp");
    let hp = r
        .game
        .world()
        .unwrap()
        .actors
        .get(r.player())
        .unwrap()
        .health;
    r.kill_as_player(bounty);
    r.tics(2);
    assert!(
        r.saw("Bounty claimed: DoomImp! +150 score, Shotgun"),
        "{:?}",
        r.msgs
    );
    let w = r.game.world().unwrap();
    assert_eq!(w.actors.get(r.player()).unwrap().health, hp + 25);
    let shotgun = w.classes.find_name("Shotgun").unwrap();
    assert_eq!(room_sim::count_inventory(w, r.player(), shotgun), 1);

    // Clear the map: the Warden spawns as the final bounty.
    for id in r.monsters() {
        r.kill_as_player(id);
    }
    r.tics(6 * 35);
    assert!(
        r.saw("Map cleared! Final bounty: THE WARDEN"),
        "{:?}",
        r.msgs
    );
    let w = r.game.world().unwrap();
    let warden = w.classes.find_name("Warden").unwrap();
    let wardens: Vec<_> = r
        .monsters()
        .into_iter()
        .filter(|&a| w.actors.get(a).unwrap().class == warden)
        .collect();
    assert_eq!(wardens.len(), 1);
    assert_eq!(w.actors.get(wardens[0]).unwrap().health, 1500);
    r.kill_as_player(wardens[0]);
    r.tics(2);
    assert!(r.saw("THE WARDEN FALLS - final score "), "{:?}", r.msgs);
}

#[test]
fn bounty_wad_ships_its_assemblyscript_source() {
    let vfs = VfsBuilder::new()
        .mount_memory("bounty.wad", Bytes::from_static(BOUNTY_WAD))
        .unwrap()
        .build()
        .unwrap();
    let src = vfs
        .read(vfs.find("S_BOUNTY", room_vfs::Namespace::Global).unwrap())
        .unwrap();
    let src = String::from_utf8_lossy(&src);
    assert!(src.contains("class BountyHunter extends RoomMod"));
    assert!(src.contains("from \"./room\""));
}

#[test]
fn bounty_mod_on_doom1_e1m1() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !p.is_file() {
        eprintln!("skip: testdata/doom/doom1.wad missing");
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let mut r = Run::new(map, 3);
    r.tics(8 * 35);
    assert!(r.saw("New bounty: "), "{:?}", r.msgs);
    assert!(r.saw("Bounty "), "hint printed: {:?}", r.msgs);
}

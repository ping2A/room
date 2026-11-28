//! The Echoes mod (`mods/echoes/echoes.wad`, AssemblyScript, built by
//! `scripts/room-server.sh mods`) end to end: a life is recorded, death turns
//! it into an echo, the echo replays the life in step with the next one,
//! shoots what is in front of it now, falls where the player fell and fades.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use room_game::{GameOptions, Headless, Skill, discover_mods, load_embedded_engine_data};
use room_map::{MapGeometry, load_map};
use room_nodebuild::ensure_nodes;
use room_sim::script_mods::ModSource;
use room_sim::{ActorId, Buttons, TicCmd, World};
use room_testkit::{MapBuilder, rect};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

const ECHOES_WAD: &[u8] = include_bytes!("../../../mods/echoes/echoes.wad");

fn echoes_mods() -> Vec<ModSource> {
    let vfs = VfsBuilder::new()
        .mount_memory("echoes.wad", Bytes::from_static(ECHOES_WAD))
        .unwrap()
        .build()
        .unwrap();
    let (mods, errors) = discover_mods(&vfs);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(mods.len(), 1);
    mods
}

/// A big room; the player starts at (128, 1024) facing east. `imp` adds an
/// imp straight ahead.
fn arena(imp: bool) -> MapGeometry {
    arena_named(imp, "MAP01")
}

fn arena_named(imp: bool, name: &str) -> MapGeometry {
    let mut b = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 2048.0, 2048.0), 0.0, 256.0)
        .player_start(128.0, 1024.0, 0);
    if imp {
        b = b.thing(3001, 640.0, 1024.0, 180);
    }
    let mut map = b.build_udmf(name).unwrap();
    room_nodebuild::apply_nodes(&mut map);
    map
}

struct Run {
    game: Headless,
    msgs: Vec<String>,
}

const IDLE: TicCmd = TicCmd {
    buttons: Buttons(0),
    pitch: 0,
    yaw: 0,
    roll: 0,
    forward_move: 0,
    side_move: 0,
    up_move: 0,
    weapon_select: 0,
};

impl Run {
    fn new(map: MapGeometry) -> Self {
        let data = load_embedded_engine_data().unwrap();
        let skill = Skill::new(3);
        let mut game = Headless::new(GameOptions {
            skill,
            ..Default::default()
        });
        game.set_defs(data.classes, data.states);
        assert_eq!(game.set_script_mods(echoes_mods()).unwrap(), ["echoes"]);
        game.start_custom_map(map, skill).unwrap();
        Self {
            game,
            msgs: Vec::new(),
        }
    }

    fn tic(&mut self, cmd: TicCmd) {
        self.game.tick(&[cmd]);
        self.msgs
            .extend(self.game.world_mut().unwrap().drain_messages());
        self.msgs.extend(self.game.drain_script_log());
        assert!(
            !self
                .msgs
                .iter()
                .any(|m| m.contains("disabled") || m.contains("abort")),
            "{:?}",
            self.msgs
        );
    }

    fn idle(&mut self, n: u32) {
        for _ in 0..n {
            self.tic(IDLE);
        }
    }

    fn saw(&self, text: &str) -> bool {
        self.msgs.iter().any(|m| m.contains(text))
    }

    fn world(&self) -> &World {
        self.game.world().unwrap()
    }

    fn player(&self) -> ActorId {
        self.world().players[0].as_ref().unwrap().mo.unwrap()
    }

    fn player_pos(&self) -> [f64; 3] {
        let p = self.world().actors.get(self.player()).unwrap().pos;
        [p.x(), p.y(), p.z()]
    }

    fn dead(&self) -> bool {
        self.world().players[0].as_ref().unwrap().player_state == room_sim::PlayerState::Dead
    }

    /// Die now and wait for the body to drop.
    fn die(&mut self) {
        assert!(self.game.cheat_kill_self(0));
        self.idle(20);
        assert!(self.dead());
    }

    /// Press Use until the player is back.
    fn respawn(&mut self) {
        for _ in 0..10 {
            self.tic(TicCmd {
                buttons: Buttons(Buttons::USE),
                ..IDLE
            });
            if !self.dead() {
                return;
            }
            self.tic(IDLE);
        }
        panic!("the player did not respawn");
    }

    fn echoes(&self) -> Vec<ActorId> {
        let w = self.world();
        let cls = w.classes.find_name("EchoMarine").expect("EchoMarine class");
        w.thinkers
            .default
            .iter()
            .copied()
            .filter(|&a| {
                w.actors
                    .get(a)
                    .is_some_and(|a| a.class == cls && !a.destroyed)
            })
            .collect()
    }

    fn pos(&self, id: ActorId) -> [f64; 3] {
        let p = self.world().actors.get(id).unwrap().pos;
        [p.x(), p.y(), p.z()]
    }

    fn monsters(&self) -> Vec<ActorId> {
        let w = self.world();
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

    /// The monster stays put, never attacks and survives the first life.
    fn freeze(&mut self, id: ActorId) {
        let a = self.game.world_mut().unwrap().actors.get_mut(id).unwrap();
        a.speed = 0.0;
        a.reaction_time = 1_000_000;
        a.health = 100_000;
    }

    fn pin(&mut self, id: ActorId, at: [f64; 3]) {
        let a = self.game.world_mut().unwrap().actors.get_mut(id).unwrap();
        a.pos = room_core::DVec3::new(at[0], at[1], at[2]);
        a.vel = room_core::DVec3::new(0.0, 0.0, 0.0);
    }

    fn health(&self, id: ActorId) -> i32 {
        self.world().actors.get(id).map_or(0, |a| a.health)
    }
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01 && (a[2] - b[2]).abs() < 0.01
}

#[test]
fn an_echo_replays_the_last_life_in_step_with_the_next() {
    let mut r = Run::new(arena(false));
    r.idle(5);
    // Life 1: walk east, then strafe north, recording where we were.
    let mut path = Vec::new();
    for t in 0..80 {
        let cmd = if t < 50 {
            TicCmd {
                forward_move: 25 * 256,
                ..IDLE
            }
        } else {
            TicCmd {
                side_move: -24 * 256,
                ..IDLE
            }
        };
        r.tic(cmd);
        path.push(r.player_pos());
    }
    assert!(
        path[79][0] > path[0][0] + 100.0,
        "walked east: {:?}",
        path[79]
    );
    assert!(r.echoes().is_empty(), "no echo during the first life");
    r.die();
    assert!(r.saw("Your echo remembers"), "{:?}", r.msgs);
    assert!(r.echoes().is_empty(), "the echo waits for the respawn");

    r.respawn();
    assert!(r.saw("1 echo walks with you"), "{:?}", r.msgs);
    let echo = r.echoes();
    assert_eq!(echo.len(), 1);
    let echo = echo[0];
    // Life 2 stands still; the echo walks life 1's path. The respawn tic
    // records the start, so the echo is one tic behind the recording index
    // of the first life's first moving tic.
    let start = r.pos(echo);
    let a = r.world().actors.get(echo).unwrap();
    assert!(a.flags & room_sim::FLAG_SOLID == 0, "echoes don't block");
    assert!(
        a.flags & room_sim::FLAG_SHOOTABLE == 0,
        "echoes can't be hurt"
    );
    assert!((a.alpha - 0.6).abs() < 1e-6, "alpha {}", a.alpha);
    r.idle(60);
    let mid = r.pos(echo);
    assert!(
        mid[0] > start[0] + 100.0,
        "the echo walked east: {start:?} -> {mid:?}"
    );
    assert!(
        path.iter().any(|p| close(*p, mid)),
        "the echo is on the recorded path: {mid:?}"
    );
    // The player did not move: the echo is not following them.
    assert!((r.player_pos()[0] - 128.0).abs() < 1.0);
}

#[test]
fn the_echo_falls_where_the_player_fell_then_fades() {
    let mut r = Run::new(arena(false));
    r.idle(5);
    for _ in 0..60 {
        r.tic(TicCmd {
            forward_move: 25 * 256,
            ..IDLE
        });
    }
    let fell = r.player_pos();
    r.die();
    r.respawn();
    let echo = r.echoes()[0];
    // Past the end of the tape the echo plays its death where we died.
    r.idle(70);
    let at = r.pos(echo);
    assert!(
        (at[0] - fell[0]).abs() < 1.0 && (at[1] - fell[1]).abs() < 1.0,
        "{at:?} vs {fell:?}"
    );
    // PLAY frames H.. are the death sequence (frame is 0-based: H = 7).
    let w = r.world();
    let st = w.actors.get(echo).unwrap().state.expect("state");
    let st = w.states.get(st).unwrap();
    assert_eq!(st.sprite, "PLAY");
    assert!(
        st.frame >= 7,
        "the echo plays its death, frame {}",
        st.frame
    );
    // Three seconds as a corpse, one to fade, then gone.
    r.idle(4 * 35 + 5);
    assert!(r.echoes().is_empty(), "the fallen echo faded away");
}

#[test]
fn an_echo_shoots_what_is_in_front_of_it_now() {
    let mut r = Run::new(arena(true));
    let imp = r.monsters()[0];
    r.freeze(imp);
    r.idle(5);
    // Life 1: stand and fire the pistol east for two seconds.
    for _ in 0..70 {
        r.tic(TicCmd {
            buttons: Buttons(Buttons::ATTACK),
            ..IDLE
        });
    }
    r.die();
    r.respawn();
    let echo = r.echoes()[0];
    let before = r.health(imp);
    // Life 2: the player never fires.
    r.idle(70);
    let after = r.health(imp);
    assert!(after < before, "the echo hit the imp: {before} -> {after}");
    // Pistol, 75%: 5 shots at most two seconds in (14-tic refire), 3..11 each.
    let shots = before - after;
    assert!(shots <= 5 * 11, "{shots} damage");
    assert_eq!(
        r.world().actors.get(imp).unwrap().target,
        Some(echo),
        "the imp blames the echo"
    );
}

#[test]
fn echoes_fire_rockets_too() {
    let mut r = Run::new(arena(true));
    let imp = r.monsters()[0];
    r.freeze(imp);
    assert!(r.game.cheat_give(0, "all") > 0);
    r.tic(TicCmd {
        weapon_select: 5,
        ..IDLE
    });
    r.idle(40);
    let rl = r.world().classes.find_name("RocketLauncher").unwrap();
    assert_eq!(
        r.world().players[0].as_ref().unwrap().ready_weapon,
        Some(rl)
    );
    for _ in 0..40 {
        r.tic(TicCmd {
            buttons: Buttons(Buttons::ATTACK),
            ..IDLE
        });
    }
    r.die();
    r.respawn();
    let before = r.health(imp);
    let player_hp = r.health(r.player());
    // Life 2: step out of the echo's line of fire (it stands where we
    // stood), then wait for its first rocket.
    for _ in 0..20 {
        r.pin(imp, [900.0, 1024.0, 0.0]);
        r.tic(TicCmd {
            side_move: -40 * 256,
            ..IDLE
        });
    }
    assert!(r.player_pos()[1] > 1024.0 + 64.0, "{:?}", r.player_pos());
    let rocket = r.world().classes.find_name("Rocket").unwrap();
    let mut seen = 0;
    for _ in 0..40 {
        r.pin(imp, [900.0, 1024.0, 0.0]);
        r.tic(IDLE);
        let w = r.world();
        let echo = r.echoes();
        seen += w
            .thinkers
            .default
            .iter()
            .filter(|&&a| {
                w.actors.get(a).is_some_and(|a| {
                    a.class == rocket && !a.destroyed && a.target == echo.first().copied()
                })
            })
            .count();
    }
    assert!(seen > 0, "the echo fired rockets");
    // Rockets fly straight at where the imp was; life 1's blasts left it
    // sliding, so it stays pinned through life 2.
    for _ in 0..60 {
        r.pin(imp, [900.0, 1024.0, 0.0]);
        r.tic(IDLE);
    }
    assert!(
        r.health(imp) < before,
        "the rocket hit the imp: {:?}",
        r.msgs
    );
    assert_eq!(r.health(r.player()), player_hp, "out of the line of fire");
}

#[test]
fn deaths_stack_up_to_max_echoes_and_the_oldest_fades() {
    let mut r = Run::new(arena(false));
    r.idle(5);
    for life in 0..6 {
        for _ in 0..(40 + life * 5) {
            r.tic(TicCmd {
                forward_move: 10 * 256,
                ..IDLE
            });
        }
        r.die();
        r.respawn();
    }
    assert!(r.saw("The oldest echo fades"), "{:?}", r.msgs);
    assert!(r.saw("4 echoes walk with you"), "{:?}", r.msgs);
    assert_eq!(r.echoes().len(), 4, "max_echoes = 4");
    // Older echoes are fainter.
    let mut alphas: Vec<f64> = r
        .echoes()
        .iter()
        .map(|&e| r.world().actors.get(e).unwrap().alpha)
        .collect();
    alphas.sort_by(f64::total_cmp);
    assert!(alphas[0] < alphas[3], "{alphas:?}");
}

#[test]
fn a_life_shorter_than_a_second_leaves_no_echo() {
    let mut r = Run::new(arena(false));
    r.idle(5);
    r.die();
    r.respawn();
    r.idle(10);
    assert!(r.echoes().is_empty());
    assert!(!r.saw("Your echo remembers"), "{:?}", r.msgs);
}

#[test]
fn echoes_wad_ships_its_assemblyscript_source() {
    let vfs = VfsBuilder::new()
        .mount_memory("echoes.wad", Bytes::from_static(ECHOES_WAD))
        .unwrap()
        .build()
        .unwrap();
    let src = vfs
        .read(vfs.find("S_ECHOES", room_vfs::Namespace::Global).unwrap())
        .unwrap();
    assert!(String::from_utf8_lossy(&src).contains("class Echoes extends RoomMod"));
}

#[test]
fn a_new_map_forgets_the_echoes() {
    let mut r = Run::new(arena(false));
    r.idle(50);
    r.die();
    r.respawn();
    assert_eq!(r.echoes().len(), 1);
    r.game
        .start_custom_map(arena_named(false, "MAP02"), Skill::new(3))
        .unwrap();
    r.idle(50);
    r.die();
    r.respawn();
    assert!(r.saw("1 echo walks with you"));
    assert_eq!(r.echoes().len(), 1, "only MAP02's own life comes back");
}

#[test]
fn echoes_on_doom1_e1m1() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !p.is_file() {
        eprintln!("skip: testdata/doom/doom1.wad missing");
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&p).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let mut r = Run::new(map);
    // Run up the start corridor shooting, three lives in a row.
    for _ in 0..3 {
        for t in 0..140 {
            r.tic(TicCmd {
                forward_move: 25 * 256,
                yaw: if t % 40 < 10 { 300 } else { 0 },
                buttons: Buttons(if t % 20 < 10 { Buttons::ATTACK } else { 0 }),
                ..IDLE
            });
            if r.dead() {
                break;
            }
        }
        if !r.dead() {
            r.die();
        }
        r.respawn();
    }
    assert!(r.saw("3 echoes walk with you"), "{:?}", r.msgs);
    assert_eq!(r.echoes().len(), 3);
    // Lives of up to four seconds, then three as a corpse and one to fade.
    r.idle(9 * 35);
    assert!(r.echoes().is_empty(), "every echo played out and faded");
}

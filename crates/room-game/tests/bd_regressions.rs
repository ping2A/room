//! Brutal Doom gameplay regressions, on the real mod (Freedoom 2 MAP01 +
//! Brutal Doom pk3 + Room's Rust ZScript port, the web's "Play solo" stack).
//!
//! Each test guards a behaviour that broke once:
//! - footsteps (`FootStep6` / `FootstepStrong`) must never hurt the player
//!   who walks or lands (spawn ownership + `A_Explode` flags);
//! - an exploding barrel must kill a monster next to it and hurt a player
//!   next to it (`BarrelExplosionDamage`: a resting Speed 0 missile);
//! - a rocket must kill the monster it hits.
//!
//! The pk3 comes from `ROOM_BRUTALDOOM_PK3`, else `testdata/brutal*.pk3`.
//! Without it (or without Freedoom) the tests print `skip:` and pass, so set
//! `ROOM_REQUIRE_BRUTALDOOM=1` to make a missing pk3 a failure.

use bytes::Bytes;
use room_core::DVec3;
use room_game::{GameOptions, Headless, Skill};
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::{ActorId, Buttons, TicCmd, World};
use room_vfs::{Namespace, VfsBuilder};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

fn repo() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn brutal_pk3() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ROOM_BRUTALDOOM_PK3") {
        return Some(PathBuf::from(p)).filter(|p| p.is_file());
    }
    let dir = repo().join("testdata");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pk3"))
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase().starts_with("brutal"))
        })
        .collect();
    found.sort();
    found.pop()
}

/// File bytes shared by every test in this binary (the pk3 is ~160 MB).
struct Files {
    iwad: Bytes,
    pk3: Bytes,
    zscript_port: Option<Bytes>,
}

fn files() -> Option<&'static Files> {
    static FILES: OnceLock<Option<Files>> = OnceLock::new();
    FILES
        .get_or_init(|| {
            let iwad = std::fs::read(repo().join("testdata/freedoom/freedoom2.wad")).ok()?;
            let pk3 = std::fs::read(brutal_pk3()?).ok()?;
            let zscript_port =
                std::fs::read(repo().join("mods/brutal-zscript-rs/brutal-zscript.wad")).ok();
            Some(Files {
                iwad: Bytes::from(iwad),
                pk3: Bytes::from(pk3),
                zscript_port: zscript_port.map(Bytes::from),
            })
        })
        .as_ref()
}

fn skip(why: &str) {
    assert!(
        std::env::var_os("ROOM_REQUIRE_BRUTALDOOM").is_none(),
        "ROOM_REQUIRE_BRUTALDOOM is set but {why}"
    );
    eprintln!("skip: {why}");
}

/// Freedoom 2 MAP01 under Brutal Doom, loaded like the web client does, a
/// second into the level.
fn start() -> Option<Headless> {
    let Some(f) = files() else {
        skip("no Brutal Doom pk3 (ROOM_BRUTALDOOM_PK3 / testdata/brutal*.pk3) or freedoom2.wad");
        return None;
    };
    let mut b = room_game::mount_embedded_engine(VfsBuilder::new()).unwrap();
    b = b.mount_memory("freedoom2.wad", f.iwad.clone()).unwrap();
    b = b.mount_memory("brutal.pk3", f.pk3.clone()).unwrap();
    if let Some(port) = &f.zscript_port {
        b = b.mount_memory("brutal-zscript.wad", port.clone()).unwrap();
    }
    let mut vfs = b.build().unwrap();
    let mut map = load_map(&vfs, "MAP01").unwrap();
    ensure_nodes(&mut map);
    let data = room_game::load_layered_engine_data(&vfs).unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        rng_seed: 1,
        ..GameOptions::default()
    });
    let defs = room_defs::DefinitionsBuilder::new(&vfs).build().unwrap();
    let opts = room_defs::mod_options::mod_options(&defs.cvars, &defs.menudef, &defs.language);
    for item in opts.iter().flat_map(|g| &g.items) {
        game.set_mod_cvar(&item.cvar, &item.default);
    }
    game.set_complevel(defs.complevel);
    let handlers = defs.zscript_event_handlers();
    game.set_mapinfo(Arc::new(defs.mapinfo.clone()));
    game.set_terrain(&defs.terrain);
    game.set_decaldef(&defs.decaldef);
    game.set_keyconf(&defs.keyconf);
    let mut libs = Vec::new();
    for name in &defs.loadacs.libraries {
        if let Some(id) = vfs
            .find(name, Namespace::Acs)
            .or_else(|| vfs.find_any_ns(name))
        {
            libs.push(vfs.read(id).unwrap().to_vec());
        }
    }
    if !libs.is_empty() {
        game.set_acs_libraries(libs);
    }
    game.set_acs_library_pool(
        room_game::acs_library_pool(&vfs),
        defs.loadacs.libraries.clone(),
    );
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript(zs.program, handlers);
    game.set_defs(data.classes, data.states);
    game.drop_weapons_without_sprites(&vfs);
    let (mods, _errors) = room_game::discover_mods(&vfs);
    if !mods.is_empty() {
        game.set_script_mods_with_vfs(mods, Some(&mut vfs)).unwrap();
    }
    if let Some(db) = data.compat_db {
        game.set_compat_db(db);
    }
    if let Some(f) = data.level_fixes {
        game.set_level_fixes(f);
    }
    game.start_custom_map(map, skill).unwrap();
    idle(&mut game, 35);
    Some(game)
}

fn idle(game: &mut Headless, tics: u32) {
    for _ in 0..tics {
        game.tick(&[TicCmd::default()]);
    }
}

fn player_mo(w: &World) -> ActorId {
    w.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .expect("player pawn")
}

fn health(w: &World, id: ActorId) -> i32 {
    w.actors.get(id).map_or(0, |a| a.health)
}

/// `class` at `offset` from the player (no rotation).
fn spawn_near_player(game: &mut Headless, class: &str, offset: (f64, f64)) -> ActorId {
    let w = game.world_mut().unwrap();
    let cid = w
        .classes
        .find_name(class)
        .unwrap_or_else(|| panic!("Brutal Doom class {class}"));
    let p = w.actors.get(player_mo(w)).unwrap().pos;
    let pos = DVec3::new(p.x() + offset.0, p.y() + offset.1, p.z());
    w.spawn_at(cid, pos, room_core::Angle::ZERO).unwrap()
}

/// `(yaw, pitch)` deltas (`TicCmd`, 1/65536 turn) that aim the player at
/// the middle of `id`: Brutal Doom plays without autoaim, and the target
/// moves.
fn aim_at(game: &Headless, id: ActorId) -> (i16, i16) {
    let w = game.world().unwrap();
    let (Some(t), Some(p)) = (w.actors.get(id), w.actors.get(player_mo(w))) else {
        return (0, 0);
    };
    let d = t.pos - p.pos;
    let want_yaw = d.y().atan2(d.x()).to_degrees();
    let yaw = (want_yaw - p.angle.degrees() + 540.0).rem_euclid(360.0) - 180.0;
    // Eye height is about `height * 0.75`; aim at the target's middle.
    let dz = (t.pos.z() + t.height * 0.5) - (p.pos.z() + p.height * 0.75);
    let flat = d.x().hypot(d.y()).max(1.0);
    // Room's pitch is positive looking up.
    let want_pitch = dz.atan2(flat).to_degrees();
    let pitch = want_pitch - p.pitch.degrees();
    #[allow(clippy::cast_possible_truncation)]
    let turn = |deg: f64| (deg * 65536.0 / 360.0).round().clamp(-32768.0, 32767.0) as i16;
    (turn(yaw), turn(pitch))
}

/// Kill the barrel the way a shot does: damage credited to the player.
fn shoot_barrel(game: &mut Headless, barrel: ActorId) {
    let w = game.world_mut().unwrap();
    let mo = player_mo(w);
    let dealt = w.damage_actor(barrel, Some(mo), Some(mo), 100);
    assert!(dealt > 0, "the barrel took no damage");
}

#[test]
fn bd_barrel_blast_kills_the_monster_next_to_it() {
    let Some(mut game) = start() else { return };
    game.cheat_god(0);
    let barrel = spawn_near_player(&mut game, "ExplosiveBarrel1", (0.0, 96.0));
    let zombie = spawn_near_player(&mut game, "RifleZombie", (40.0, 96.0));
    idle(&mut game, 40);
    let w = game.world().unwrap();
    let before = health(w, zombie);
    assert!(before > 0, "zombie alive before the blast");
    shoot_barrel(&mut game, barrel);
    idle(&mut game, 105);
    let w = game.world().unwrap();
    assert!(
        health(w, zombie) <= 0,
        "a RifleZombie 40 units from an exploding barrel survived ({before} -> {})",
        health(w, zombie)
    );
}

#[test]
fn bd_barrel_blast_hurts_the_player_next_to_it() {
    let Some(mut game) = start() else { return };
    game.cheat_kill_monsters();
    let barrel = spawn_near_player(&mut game, "ExplosiveBarrel1", (0.0, 64.0));
    idle(&mut game, 40);
    let mo = player_mo(game.world().unwrap());
    let before = health(game.world().unwrap(), mo);
    shoot_barrel(&mut game, barrel);
    idle(&mut game, 105);
    let after = health(game.world().unwrap(), mo);
    assert!(
        after < before,
        "the player 64 units from an exploding barrel took no damage ({before} -> {after})"
    );
}

/// Walk, then jump and land again and again, with a monster set as the
/// player's target: every step spawns `FootStep6` and every landing
/// `FootstepStrong` (a 60-damage `A_Explode(…, flags 0)`). None may hurt
/// the player (they once killed the player at the first landing).
#[test]
fn bd_footsteps_never_hurt_the_player() {
    let Some(mut game) = start() else { return };
    game.cheat_kill_monsters();
    // Any live actor will do as the player's target; Brutal Doom footsteps
    // used to inherit it and then hit the player.
    let decoy = spawn_near_player(&mut game, "ExplosiveBarrel1", (-512.0, -512.0));
    let mo = player_mo(game.world().unwrap());
    // Being hurt by it makes it the player's target, as in a real fight.
    let w = game.world_mut().unwrap();
    w.damage_actor(mo, Some(decoy), Some(decoy), 1);
    w.actors.get_mut(mo).unwrap().target = Some(decoy);
    idle(&mut game, 10);
    let start_health = health(game.world().unwrap(), mo);
    let start_pos = game.world().unwrap().actors.get(mo).unwrap().pos;
    let mut lowest = start_health;
    for t in 0..420u32 {
        let mut cmd = TicCmd::default();
        // Walk back and forth (`FootStep6` at every step), then jump on
        // the spot so each landing's `FootstepStrong` blast is centred on
        // the player.
        if t < 120 {
            cmd.forward_move = if (t / 30) % 2 == 0 { 12800 } else { -12800 };
        } else if t % 40 == 0 {
            cmd.buttons = Buttons(Buttons::JUMP);
        }
        game.tick(&[cmd]);
        let w = game.world().unwrap();
        lowest = lowest.min(health(w, mo));
        if std::env::var_os("BD_DBG").is_some() {
            for &id in &w.thinkers.default {
                if let Some(a) = w.actors.get(id) {
                    let n = w
                        .classes
                        .get(a.class)
                        .map(|c| c.name.as_str())
                        .unwrap_or("");
                    if n.eq_ignore_ascii_case("FootstepStrong")
                        || n.eq_ignore_ascii_case("FootStep6")
                    {
                        if a.destroyed {
                            continue;
                        }
                        eprintln!(
                            "  t{t} {n} {:?} target={:?} z={:.1} velz={:.1}",
                            id.index(),
                            a.target.map(|x| x.index()),
                            a.pos.z(),
                            a.vel.z()
                        );
                    }
                }
            }
        }
        if std::env::var_os("BD_DBG").is_some() && t % 15 == 0 {
            let a = w.actors.get(mo).unwrap();
            eprintln!(
                "t{t} hp {} pos {:?} velz {:.1} target {:?}",
                a.health,
                a.pos,
                a.vel.z(),
                a.target
            );
        }
    }
    let w = game.world().unwrap();
    let moved = (w.actors.get(mo).unwrap().pos - start_pos).length();
    assert!(lowest > 0, "the player died to their own footsteps");
    assert_eq!(
        lowest, start_health,
        "footsteps hurt the player ({start_health} -> {lowest}); moved {moved:.0} units"
    );
}

#[test]
fn bd_pistol_kills_the_monster_in_front() {
    let Some(mut game) = start() else { return };
    // (no god)
    game.cheat_kill_monsters();
    let zombie = {
        let w = game.world_mut().unwrap();
        w.summon_in_front(0, "RifleZombie")
            .expect("summon RifleZombie")
    };
    let before = health(game.world().unwrap(), zombie);
    assert!(before > 0);
    for t in 0..350u32 {
        let mut cmd = TicCmd::default();
        if t % 2 == 0 {
            cmd.buttons = Buttons(Buttons::ATTACK);
        }
        (cmd.yaw, cmd.pitch) = aim_at(&game, zombie);
        game.tick(&[cmd]);
        if std::env::var_os("BD_DBG").is_some() && t % 5 == 0 {
            let w = game.world().unwrap();
            let p = w.players[0].as_ref().unwrap();
            let st = p
                .pspr_state
                .and_then(|s| w.states.get(s))
                .map(|s| (s.sprite.clone(), s.frame, s.action.clone()));
            let pm = w.actors.get(player_mo(w)).unwrap();
            eprintln!(
                "   mo {:?} class {:?} pos {:?} angle {:?}",
                player_mo(w).index(),
                w.classes.get(pm.class).map(|c| c.name.clone()),
                pm.pos,
                pm.angle
            );
            eprintln!(
                "t{t} {:?} {st:?} zhp {} zdist {:.0}",
                p.ready_weapon
                    .and_then(|c| w.classes.get(c))
                    .map(|c| c.name.clone()),
                health(w, zombie),
                w.actors
                    .get(zombie)
                    .zip(w.actors.get(player_mo(w)))
                    .map_or(0.0, |(a, b)| (a.pos - b.pos).length())
            );
        }
        if health(game.world().unwrap(), zombie) <= 0 {
            break;
        }
    }
    let w = game.world().unwrap();
    assert!(
        health(w, zombie) <= 0,
        "the starting pistol never killed a RifleZombie in front of the player ({before} -> {})",
        health(w, zombie)
    );
}

/// One barrel's blast sets off the next one, whose blast then kills the
/// monster beside it (blast ownership down a chain of spawned actors).
#[test]
fn bd_barrel_chain_reaction_kills_the_far_monster() {
    let Some(mut game) = start() else { return };
    game.cheat_god(0);
    game.cheat_kill_monsters();
    let first = spawn_near_player(&mut game, "ExplosiveBarrel1", (0.0, 96.0));
    let second = spawn_near_player(&mut game, "ExplosiveBarrel1", (48.0, 96.0));
    let zombie = spawn_near_player(&mut game, "RifleZombie", (120.0, 96.0));
    idle(&mut game, 40);
    let before = health(game.world().unwrap(), zombie);
    assert!(before > 0);
    shoot_barrel(&mut game, first);
    idle(&mut game, 210);
    let w = game.world().unwrap();
    assert!(
        health(w, second) <= 0,
        "the second barrel survived the first one's blast"
    );
    assert!(
        health(w, zombie) <= 0,
        "a RifleZombie beside the second barrel survived the chain ({before} -> {})",
        health(w, zombie)
    );
}

/// The slot 5 launcher's projectile and its explosion kill the monster
/// (ownership of missiles spawned by missiles, blast damage).
#[test]
fn bd_launcher_kills_the_monster_it_hits() {
    let Some(mut game) = start() else { return };
    game.cheat_god(0);
    game.cheat_kill_monsters();
    assert!(game.cheat_give(0, "all") > 0, "give all");
    let zombie = spawn_near_player(&mut game, "RifleZombie", (256.0, 0.0));
    let before = health(game.world().unwrap(), zombie);
    assert!(before > 0);
    let mut fired = false;
    for t in 0..210u32 {
        let mut cmd = TicCmd::default();
        if t == 0 {
            cmd.weapon_select = 5;
        } else if t > 50 && t % 30 == 0 {
            cmd.buttons = Buttons(Buttons::ATTACK);
            fired = true;
        }
        (cmd.yaw, cmd.pitch) = aim_at(&game, zombie);
        game.tick(&[cmd]);
        if health(game.world().unwrap(), zombie) <= 0 {
            break;
        }
    }
    assert!(fired);
    let w = game.world().unwrap();
    let weapon = w.players[0]
        .as_ref()
        .and_then(|p| p.ready_weapon)
        .and_then(|c| w.classes.get(c))
        .map(|c| c.name.clone());
    assert!(
        health(w, zombie) <= 0,
        "a RifleZombie shot with {weapon:?} survived ({before} -> {})",
        health(w, zombie)
    );
}

/// After `give all`, every weapon slot brings up a weapon with a psprite.
/// Brutal Doom's M202 (`IncendiaryLauncher`) calls `A_Lower` inside its
/// `Select` state; Room once dropped the psprite there and the player was
/// left holding nothing.
#[test]
fn bd_every_weapon_slot_raises_a_weapon() {
    let Some(mut game) = start() else { return };
    game.cheat_god(0);
    game.cheat_kill_monsters();
    assert!(game.cheat_give(0, "all") > 0, "give all");
    let mut seen = Vec::new();
    for slot in 1..=9u8 {
        game.tick(&[TicCmd {
            weapon_select: slot,
            ..TicCmd::default()
        }]);
        idle(&mut game, 70);
        let w = game.world().unwrap();
        let p = w.players[0].as_ref().unwrap();
        let weapon = p
            .ready_weapon
            .and_then(|c| w.classes.get(c))
            .map_or_else(|| "none".to_owned(), |c| c.name.clone());
        assert!(
            p.pspr_state.is_some(),
            "slot {slot}: {weapon} has no psprite (the player holds nothing)"
        );
        seen.push(weapon);
    }
    seen.sort();
    seen.dedup();
    assert!(seen.len() >= 7, "slots 1-9 only reached {seen:?}");
}

/// Walk forward for `tics` tics.
fn walk(game: &mut Headless, tics: u32) {
    for _ in 0..tics {
        game.tick(&[TicCmd {
            forward_move: 6400,
            ..TicCmd::default()
        }]);
    }
}

/// The player's facing as a unit XY vector.
fn facing(game: &Headless) -> (f64, f64) {
    let w = game.world().unwrap();
    let a = w
        .actors
        .get(player_mo(w))
        .unwrap()
        .angle
        .degrees()
        .to_radians();
    (a.cos(), a.sin())
}

/// Brutal Doom's `Medikit2` / `Stimpack2` (`: Health`, no `MaxAmount`) heal
/// up to the player's max health: 25 and 10 points.
#[test]
fn bd_medikit_and_stimpack_heal_the_player() {
    let Some(mut game) = start() else {
        return;
    };
    let mo = player_mo(game.world().unwrap());
    game.world_mut().unwrap().actors.get_mut(mo).unwrap().health = 50;
    let (fx, fy) = facing(&game);
    let kit = spawn_near_player(&mut game, "Medikit", (fx * 24.0, fy * 24.0));
    {
        let w = game.world().unwrap();
        let class = w.classes.get(w.actors.get(kit).unwrap().class).unwrap();
        assert_eq!(class.name, "Medikit2", "Brutal Doom replaces the medikit");
    }
    walk(&mut game, 10);
    let w = game.world().unwrap();
    assert!(
        w.actors.get(kit).is_none_or(|a| a.destroyed),
        "the medikit is taken"
    );
    assert_eq!(health(w, mo), 75, "a medikit heals 25");

    let (fx, fy) = facing(&game);
    spawn_near_player(&mut game, "Stimpack", (fx * 24.0, fy * 24.0));
    walk(&mut game, 10);
    assert_eq!(health(game.world().unwrap(), mo), 85, "a stimpack heals 10");

    // Never past the max (100).
    game.world_mut().unwrap().actors.get_mut(mo).unwrap().health = 95;
    let (fx, fy) = facing(&game);
    spawn_near_player(&mut game, "Medikit", (fx * 24.0, fy * 24.0));
    walk(&mut game, 10);
    assert_eq!(
        health(game.world().unwrap(), mo),
        100,
        "capped at max health"
    );
}

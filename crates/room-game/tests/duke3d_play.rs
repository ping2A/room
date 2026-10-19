//! Converted Duke Nukem 3D (`testdata/duke/duke3d.grp`, not committed; the
//! test passes trivially without it): the generated DECORATE layers over the
//! engine, E1M1 spawns Duke enemies, items and decorations on their floors,
//! the player is `DukePlayer` with the pistol up, and a trooper fights.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use room_defs::DefinitionsBuilder;
use room_game::{
    DemoPlayer, DemoRecorder, GameOptions, Headless, Skill, load_layered_engine_data,
    mount_embedded_engine,
};
use room_map::load_map;
use room_sim::{
    Buttons, FLAG_COUNTKILL, FLAG_NOGRAVITY, FLAG_SOLID, FLAG_USESPECIAL, TicCmd, check_position,
    floor_at_xy, try_move,
};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn class_name(w: &room_sim::World, c: room_core::ClassId) -> &str {
    w.classes.get(c).map_or("?", |d| d.name.as_str())
}

fn grp() -> Option<Vec<u8>> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke/duke3d.grp");
    std::fs::read(p).ok()
}

#[test]
fn e1m1_spawns_duke_actors_and_plays() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let data = load_layered_engine_data(&vfs).unwrap();
    let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
    assert!(defs.warnings.is_empty(), "{:?}", defs.warnings);
    assert_eq!(
        room_game::resolve_map_music("E1M1", Some(&defs.mapinfo)).as_deref(),
        Some("STALKER")
    );
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_mapinfo(Arc::new(defs.mapinfo));
    game.set_keyconf(&defs.keyconf);
    game.set_defs(data.classes, data.states);
    game.drop_weapons_without_sprites(&vfs);
    let mut map = load_map(&vfs, "E1M1").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();
    for _ in 0..300 {
        game.tick(&[TicCmd::default()]);
    }

    let w = game.world().unwrap();
    let name = |c| class_name(w, c);
    let player = w.players[0].as_ref().unwrap();
    let mo = player.mo.unwrap();
    let pa = w.actors.get(mo).unwrap();
    assert_eq!(name(pa.class), "DukePlayer");
    assert_eq!(player.ready_weapon.map(name), Some("DukePistol"));
    assert!(pa.health > 0);
    let fits = |a: &room_sim::Actor, id, x: f64, y: f64| {
        let z = floor_at_xy(w, x, y).unwrap_or(f64::MAX);
        (z - a.pos.z()).abs() < 1.0
            && check_position(w, a, room_core::DVec3::new(x, y, z), Some(id)).ok
    };
    let (mut monsters, mut grounded, mut items, mut decor) = (0, 0, 0, 0);
    for &id in &w.thinkers.default {
        let Some(a) = w.actors.get(id) else { continue };
        let n = name(a.class);
        if a.flags & FLAG_COUNTKILL != 0 {
            assert!(n.starts_with("Duke"), "{n}");
            monsters += 1;
            if a.flags & FLAG_NOGRAVITY != 0 || fits(a, id, a.pos.x(), a.pos.y()) {
                grounded += 1;
            }
        } else if n.starts_with("DukeDecor") {
            decor += 1;
        } else if n.starts_with("Duke") {
            items += 1;
        }
    }
    assert!(monsters >= 10, "monsters {monsters}");
    // The converter and a post-spawn nudge move monsters off walls. Overlapping
    // spiral stairs can still leave one trooper floating.
    assert!(
        grounded * 20 >= monsters * 19,
        "{grounded}/{monsters} placed"
    );
    assert!(items >= 10, "items {items}");
    assert!(decor >= 50, "decorations {decor}");
    // Roof-edge fences belong to the street sector below. Finding the roof
    // instead lifts them by the drop and pins them under the sky.
    for &id in &w.thinkers.default {
        let Some(a) = w.actors.get(id) else { continue };
        if name(a.class) != "DukeDecor27030" {
            continue;
        }
        let ceil = room_sim::ceiling_at_xy(w, a.pos.x(), a.pos.y()).unwrap();
        assert!(
            a.pos.z() + a.height < ceil - 1.0,
            "fence {:?} under the sky",
            a.pos
        );
    }

    // Stand next to a trooper: it wakes, fires lasers and hurts the player.
    let hp = pa.health;
    let (trooper, (x, y)) = w
        .thinkers
        .default
        .iter()
        .copied()
        .find_map(|id| {
            let a = w.actors.get(id)?;
            if name(a.class) != "DukeTrooper" || !fits(a, id, a.pos.x(), a.pos.y()) {
                return None;
            }
            [(64.0, 0.0), (-64.0, 0.0), (0.0, 64.0), (0.0, -64.0)]
                .into_iter()
                .map(|(dx, dy)| (a.pos.x() + dx, a.pos.y() + dy))
                .find(|&(x, y)| {
                    let mut p = pa.clone();
                    p.pos = room_core::DVec3::new(x, y, a.pos.z());
                    fits(&p, mo, x, y)
                })
                .map(|spot| (id, spot))
        })
        .expect("a reachable trooper");
    let z = floor_at_xy(w, x, y).unwrap();
    if let Some(p) = game.world_mut().unwrap().actors.get_mut(mo) {
        p.pos = room_core::DVec3::new(x, y, z);
    }
    let mut woke = false;
    let mut hurt = false;
    for _ in 0..35 * 20 {
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        woke |= w.actors.get(trooper).is_some_and(|a| a.target == Some(mo));
        if w.actors.get(mo).is_some_and(|a| a.health < hp) {
            hurt = true;
            break;
        }
    }
    assert!(woke, "trooper never targeted the player");
    assert!(hurt, "trooper never hurt the player");

    // `give all` hands out the Duke arsenal, not Doom's slot defaults.
    game.cheat_give(0, "all");
    let w = game.world().unwrap();
    let owns = |n: &str| {
        w.classes
            .find_name(n)
            .is_some_and(|c| w.inventory.count(mo, c) > 0)
    };
    for n in ["DukeShotgun", "DukeRipper", "DukeRPG", "DukeFreezer"] {
        assert!(owns(n), "{n}");
    }
    assert!(!owns("PlasmaRifle") && !owns("BFG9000"));
    assert_eq!(w.weapon_slots.weapons_for(6)[0], "DukePipebombWeapon");
    assert_eq!(w.weapon_slots.weapons_for(9), ["DukeTripbombWeapon"]);
    assert_eq!(w.weapon_slots.weapons_for(0), ["DukeFreezer"]);
}

#[test]
fn e1m1_start_can_walk() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let mut game = start(&vfs, "E1M1");
    let w = game.world().unwrap();
    let p = w.players[0].as_ref().unwrap();
    let mo = p.mo.unwrap();
    let a = w.actors.get(mo).unwrap();
    assert!(
        a.pitch.degrees().abs() < 5.0,
        "start pitch {}",
        a.pitch.degrees()
    );
    let floor = floor_at_xy(w, a.pos.x(), a.pos.y()).unwrap_or(a.pos.z());
    assert!(
        a.pos.z() > floor + 40.0,
        "start feet {} should sit at the map eye, floor {floor}",
        a.pos.z()
    );
    let here = check_position(w, a, a.pos, Some(mo));
    assert!(
        here.ceiling_z - here.floor_z >= a.height,
        "start headroom {} < height {}",
        here.ceiling_z - here.floor_z,
        a.height
    );
    for _ in 0..90 {
        game.tick(&[TicCmd::default()]);
        if game.world().unwrap().players[0].as_ref().unwrap().on_ground {
            break;
        }
    }
    let w = game.world_mut().unwrap();
    let a = w.actors.get(mo).unwrap();
    assert!(
        (a.max_step_height - 48.0).abs() < 1.0,
        "duke step {}",
        a.max_step_height
    );
    let feet = a.pos.z();
    let sec = room_sim::sector_at_xy(w, a.pos.x(), a.pos.y()).unwrap();
    let radius = a.radius;
    let mut spots = Vec::new();
    for line in &w.map.lines {
        let (Some(front), Some(back)) = (line.front_sector, line.back_sector) else {
            continue;
        };
        let other = if front.raw() as usize == sec {
            back
        } else if back.raw() as usize == sec {
            front
        } else {
            continue;
        };
        let v1 = w.map.vertices[line.v1.raw() as usize].pos;
        let v2 = w.map.vertices[line.v2.raw() as usize].pos;
        let mid = (v1 + v2) * 0.5;
        let n = (v2 - v1).normalized();
        if n.length() == 0.0 {
            continue;
        }
        let side = room_core::DVec2::new(-n.y(), n.x()) * (radius + 4.0);
        let mut spot = mid + side;
        if room_sim::sector_at_xy(w, spot.x(), spot.y()) != Some(other.raw() as usize) {
            spot = mid - side;
        }
        if room_sim::sector_at_xy(w, spot.x(), spot.y()) != Some(other.raw() as usize) {
            continue;
        }
        let oz = floor_at_xy(w, spot.x(), spot.y()).unwrap_or(feet);
        if (oz - feet - 32.0).abs() > 3.0 {
            continue;
        }
        spots.push((spot.x(), spot.y()));
    }
    let climbed = spots
        .iter()
        .any(|&(x, y)| try_move(w, mo, room_core::DVec3::new(x, y, feet)));
    assert!(climbed, "32-unit roof curb should be a step");
    let z0 = w.actors.get(mo).unwrap().pos;
    let mut cmd = TicCmd::default();
    cmd.forward_move = 50 * 256;
    for _ in 0..35 {
        game.tick(&[cmd]);
    }
    let w = game.world().unwrap();
    let a = w.actors.get(mo).unwrap();
    let moved = (a.pos.x() - z0.x()).hypot(a.pos.y() - z0.y());
    assert!(
        moved > 16.0,
        "stuck under the start overhang, moved {moved}"
    );
}

#[test]
fn trash_fire_animates() {
    let Some(bytes) = grp() else { return };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let mut game = start(&vfs, "E1M1");
    let w = game.world_mut().unwrap();
    let fire: Vec<_> = w
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            let Some(a) = w.actors.get(id) else {
                return false;
            };
            let dx = a.pos.x() - 560.0;
            let dy = a.pos.y() + 3590.0;
            dx * dx + dy * dy < 80.0 * 80.0
                && w.classes.get(a.class).is_some_and(|c| c.spawn_tics > 0)
        })
        .collect();
    assert!(!fire.is_empty(), "dumpster fire has no frames");
    let before: Vec<_> = fire
        .iter()
        .map(|&id| w.actors.get(id).unwrap().frame)
        .collect();
    for _ in 0..40 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let changed = fire
        .iter()
        .zip(before)
        .any(|(&id, frame)| w.actors.get(id).is_some_and(|a| a.frame != frame));
    assert!(changed, "dumpster fire stayed on its first frame");
}

#[test]
fn street_walks_into_the_cinema() {
    let Some(bytes) = grp() else { return };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let mut game = start(&vfs, "E1M1");
    let (mo, street) = {
        let w = game.world_mut().unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let z = floor_at_xy(w, 1045.0, -3965.0).unwrap();
        let street = room_sim::sector_at_xy(w, 1045.0, -3965.0);
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.pos = room_core::DVec3::new(1045.0, -3965.0, z);
        }
        (mo, street)
    };
    let mut entered = false;
    {
        let w = game.world_mut().unwrap();
        w.actors.get_mut(mo).unwrap().angle = room_core::Angle::from_degrees(90.0);
    }
    let cmd = TicCmd {
        forward_move: 25 * 256,
        ..TicCmd::default()
    };
    for _ in 0..250 {
        game.tick(&[cmd]);
        let w = game.world().unwrap();
        let a = w.actors.get(mo).unwrap();
        let sec = room_sim::sector_at_xy(w, a.pos.x(), a.pos.y());
        if a.pos.y() > -3600.0 && sec != street && sec != Some(5) && sec != Some(319) {
            let here = check_position(w, a, a.pos, Some(mo));
            assert!(
                here.ok && here.ceiling_z - here.floor_z >= a.height,
                "inside but crushed"
            );
            entered = true;
            break;
        }
    }
    let w = game.world().unwrap();
    let a = w.actors.get(mo).unwrap();
    assert!(
        entered,
        "still outside at ({:.0},{:.0}) sec {:?}",
        a.pos.x(),
        a.pos.y(),
        room_sim::sector_at_xy(w, a.pos.x(), a.pos.y())
    );
}

/// The street mouth under the marquee (floor −32, ceiling 32) has to admit
/// Duke, and the swing door beyond it has to open.
#[test]
fn cinema_mouth_admits_the_player_and_the_door_swings() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let mut game = start(&vfs, "E1M1");
    let w = game.world_mut().unwrap();
    let mouth = w
        .map
        .sectors
        .iter()
        .position(|s| (s.floor_height + 32.0).abs() < 1.0 && (s.ceiling_height - 32.0).abs() < 1.0)
        .expect("cinema mouth");
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut n = 0.0;
    for &lid in &w.map.sectors[mouth].lines {
        let line = &w.map.lines[lid.raw() as usize];
        let v = w.map.vertices[line.v1.raw() as usize].pos;
        sx += v.x();
        sy += v.y();
        n += 1.0;
    }
    let (x, y) = (sx / n, sy / n);
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let floor = floor_at_xy(w, x, y).unwrap();
    {
        let a = w.actors.get_mut(mo).unwrap();
        a.pos = room_core::DVec3::new(x, y, floor);
    }
    let a = w.actors.get(mo).unwrap();
    let here = check_position(w, a, a.pos, Some(mo));
    assert!(
        here.ok && here.ceiling_z - here.floor_z >= a.height,
        "cinema mouth blocks: ok={} gap {}",
        here.ok,
        here.ceiling_z - here.floor_z
    );
    let mut entered = false;
    let lines: Vec<_> = w.map.sectors[mouth].lines.clone();
    for lid in lines {
        let line = &w.map.lines[lid.raw() as usize];
        let (Some(front), Some(back)) = (line.front_sector, line.back_sector) else {
            continue;
        };
        let other = if front.raw() as usize == mouth {
            back
        } else if back.raw() as usize == mouth {
            front
        } else {
            continue;
        };
        let outside = &w.map.sectors[other.raw() as usize];
        if outside.ceiling_height < 200.0 {
            continue;
        }
        let v1 = w.map.vertices[line.v1.raw() as usize].pos;
        let v2 = w.map.vertices[line.v2.raw() as usize].pos;
        let mid = (v1 + v2) * 0.5;
        let n = (v2 - v1).normalized();
        let side = room_core::DVec2::new(-n.y(), n.x()) * 24.0;
        let mut spot = mid + side;
        if room_sim::sector_at_xy(w, spot.x(), spot.y()) == Some(mouth) {
            spot = mid - side;
        }
        if room_sim::sector_at_xy(w, spot.x(), spot.y()) == Some(mouth) {
            continue;
        }
        let z = floor_at_xy(w, spot.x(), spot.y()).unwrap_or(floor);
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.pos = room_core::DVec3::new(spot.x(), spot.y(), z);
        }
        entered = try_move(w, mo, room_core::DVec3::new(x, y, z));
        break;
    }
    assert!(entered, "cannot walk in off the street");
    let before: Vec<f64> = w.polyobjs.iter().map(|p| p.angle).collect();
    let mut used = false;
    let polys: Vec<_> = w
        .polyobjs
        .iter()
        .map(|p| (p.lines.clone(), p.center))
        .collect();
    for (lines, center) in polys {
        if (center.y() - y).abs() > 800.0 {
            continue;
        }
        let Some(lid) = lines.iter().copied().find(|l| w.line_special(*l) == 80) else {
            continue;
        };
        let line = &w.map.lines[lid.raw() as usize];
        let v1 = w.map.vertices[line.v1.raw() as usize].pos;
        let v2 = w.map.vertices[line.v2.raw() as usize].pos;
        let mid = (v1 + v2) * 0.5;
        let n = (v2 - v1).normalized();
        let left = room_core::DVec2::new(-n.y(), n.x());
        let spot = mid + left * 32.0;
        let z = floor_at_xy(w, spot.x(), spot.y()).unwrap_or(floor);
        let face = mid - spot;
        let yaw = face.y().atan2(face.x()).to_degrees();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.pos = room_core::DVec3::new(spot.x(), spot.y(), z);
            a.angle = room_core::Angle::from_degrees(yaw);
        }
        if room_sim::use_lines(w, mo) {
            used = true;
            break;
        }
        let spot = mid - left * 32.0;
        let z = floor_at_xy(w, spot.x(), spot.y()).unwrap_or(floor);
        let face = mid - spot;
        let yaw = face.y().atan2(face.x()).to_degrees();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.pos = room_core::DVec3::new(spot.x(), spot.y(), z);
            a.angle = room_core::Angle::from_degrees(yaw);
        }
        if room_sim::use_lines(w, mo) {
            used = true;
            break;
        }
    }
    assert!(used, "use did not hit a cinema door");
    for _ in 0..35 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let turned = w
        .polyobjs
        .iter()
        .zip(before)
        .any(|(p, a)| (p.angle - a).abs() > 0.2);
    assert!(turned, "cinema swing door did not open");
}

/// The door's first linedef is only the polyobject anchor. Walking into that
/// edge still has to run the swing script on the other edges.
#[test]
fn walking_into_the_cinema_door_opens_it() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let mut game = start(&vfs, "E1M1");
    let w = game.world_mut().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let start = w.polyobjs.iter().map(|p| p.angle).collect::<Vec<_>>();
    let spot = w.polyobjs.iter().find_map(|p| {
        if (p.center.y() + 3659.0).abs() > 200.0 {
            return None;
        }
        let lid = p
            .lines
            .iter()
            .copied()
            .find(|l| w.map.lines[l.raw() as usize].special == 1)?;
        let line = &w.map.lines[lid.raw() as usize];
        let v1 = w.map.vertices[line.v1.raw() as usize].pos;
        let v2 = w.map.vertices[line.v2.raw() as usize].pos;
        let mid = (v1 + v2) * 0.5;
        // Street is south of the marquee doors.
        Some(room_core::DVec2::new(mid.x(), mid.y() - 48.0))
    });
    let Some(spot) = spot else {
        panic!("no cinema start line");
    };
    let z = floor_at_xy(w, spot.x(), spot.y()).unwrap();
    {
        let a = w.actors.get_mut(mo).unwrap();
        a.pos = room_core::DVec3::new(spot.x(), spot.y(), z);
        a.angle = room_core::Angle::from_degrees(90.0);
    }
    let cmd = TicCmd {
        forward_move: 25 * 256,
        ..TicCmd::default()
    };
    for _ in 0..45 {
        game.tick(&[cmd]);
    }
    let w = game.world().unwrap();
    let turned = w
        .polyobjs
        .iter()
        .zip(start)
        .any(|(p, a)| (p.angle - a).abs() > 0.2);
    let a = w.actors.get(mo).unwrap();
    assert!(
        turned,
        "door stayed shut, player at ({:.0},{:.0})",
        a.pos.x(),
        a.pos.y()
    );
}

fn start(vfs: &room_vfs::Vfs, map_name: &str) -> Headless {
    let data = load_layered_engine_data(vfs).unwrap();
    let defs = DefinitionsBuilder::new(vfs).build().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        ..Default::default()
    });
    game.set_mapinfo(Arc::new(defs.mapinfo));
    if let Some(id) = vfs.find_any_ns("CONEVT") {
        if let Ok(bytes) = vfs.read(id) {
            if let Ok(text) = std::str::from_utf8(&bytes) {
                game.set_conevt(text);
            }
        }
    }
    game.set_keyconf(&defs.keyconf);
    game.set_decaldef(&defs.decaldef);
    game.set_sndseq(&defs.sndseq);
    game.set_defs(data.classes, data.states);
    let mut map = load_map(vfs, map_name).unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();
    game
}

/// Blowing the charges by the rooftop vent opens the shaft; dropping in, the
/// floating `SE 7` fall link carries the player down into the building instead
/// of bouncing them between the two transporters.
#[test]
fn rooftop_vent_drops_into_the_building() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let mut game = start(&vfs, "E1M1");
    for _ in 0..5 {
        game.tick(&[TicCmd::default()]);
    }
    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let w = game.world().unwrap();
    let trigger = w.thinkers.default.iter().copied().find(|&id| {
        w.actors
            .get(id)
            .is_some_and(|a| (a.pos.x() + 1515.0).abs() < 2.0 && (a.pos.y() + 879.0).abs() < 2.0)
    });
    let trigger = trigger.expect("vent trigger charge");
    room_sim::damage_mobj(game.world_mut().unwrap(), trigger, Some(mo), Some(mo), 100);
    for _ in 0..350 {
        game.tick(&[TicCmd::default()]);
    }
    {
        let p = game.world_mut().unwrap().actors.get_mut(mo).unwrap();
        p.pos = room_core::DVec3::new(-1600.0, -870.0, 580.0);
        p.angle = room_core::Angle::from_degrees(250.0);
    }
    let mut lowest = f64::MAX;
    for _ in 0..80 {
        game.tick(&[TicCmd {
            forward_move: 25 * 256,
            ..TicCmd::default()
        }]);
        let p = game.world().unwrap().actors.get(mo).unwrap();
        lowest = lowest.min(p.pos.z());
    }
    let w = game.world().unwrap();
    let p = w.actors.get(mo).unwrap();
    let sector = room_nodebuild::point_in_sector(&w.map, p.pos.x(), p.pos.y()).map(|s| s.raw());
    // The shaft (sector 269) bottoms out at -132 below the transporter.
    assert!(
        sector != Some(269) && lowest < 300.0,
        "stuck at ({:.0},{:.0},{:.0}) sector {sector:?}",
        p.pos.x(),
        p.pos.y(),
        p.pos.z()
    );
}

/// Every live actor's frame has a sprite lump: no grey placeholder quads
/// from Doom-only puffs or blood while the hitscan guns fire.
#[test]
fn hitscan_effects_use_duke_sprites() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
    let frames: std::collections::BTreeSet<String> = vfs
        .iter()
        .filter(|(_, l)| l.namespace == room_vfs::Namespace::Sprites)
        .map(|(_, l)| l.short_name.as_str().get(..5).unwrap_or("").to_owned())
        .collect();
    assert!(
        defs.decaldef.place_texture("BulletChip", 7).as_deref() == Some("DKBHOLE"),
        "Duke bullet holes"
    );
    let mut game = start(&vfs, "E1M1");
    game.cheat_give(0, "all");
    // Level shots from the rooftop start fly over sky-hack edges (no puff, no
    // decal, as in Duke); tilt down onto the roof-edge lower walls.
    game.tick(&[TicCmd {
        pitch: -600,
        ..TicCmd::default()
    }]);
    let mut puffs = 0;
    let mut decals = Vec::new();
    for slot in [2, 3, 4] {
        game.tick(&[TicCmd {
            weapon_select: slot,
            ..TicCmd::default()
        }]);
        for _ in 0..50 {
            game.tick(&[TicCmd::default()]);
        }
        for t in 0..60 {
            game.tick(&[TicCmd {
                buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
                yaw: if t % 20 == 0 { 8192 } else { 0 },
                ..TicCmd::default()
            }]);
            let w = game.world().unwrap();
            decals.extend(w.decal_events.iter().map(|d| d.name.clone()));
            // A HUD patch without a lump used to draw as a grey placeholder square.
            let hud = w.player_hud(0).unwrap();
            for p in [&hud.weapon_patch, &hud.flash_patch] {
                assert!(
                    p.is_empty() || vfs.find_any_ns(p).is_some(),
                    "slot {slot} HUD patch {p} has no lump"
                );
            }
            for &id in &w.thinkers.default {
                let Some(a) = w.actors.get(id).filter(|a| !a.destroyed) else {
                    continue;
                };
                let Some(st) = a.state.and_then(|s| w.states.get(s)) else {
                    continue;
                };
                if st.sprite.eq_ignore_ascii_case("TNT1") {
                    continue;
                }
                let n = class_name(w, a.class);
                puffs += usize::from(n == "DukePuff");
                let key = format!("{}{}", st.sprite, char::from(b'A' + st.frame));
                assert!(
                    frames.contains(&key.to_ascii_uppercase()),
                    "{n} shows missing sprite {key}"
                );
            }
        }
    }
    assert!(puffs > 0, "no Duke shot sparks");
    assert!(decals.iter().any(|d| d == "BulletChip"), "{decals:?}");
}

#[test]
fn opening_car_flies_down_and_blows_up() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let car = |w: &room_sim::World| {
        w.thinkers.default.iter().copied().find(|&id| {
            w.actors.get(id).is_some_and(|a| {
                let n = class_name(w, a.class);
                !a.destroyed && n.starts_with("DukeCar") && n != "DukeCarFire"
            })
        })
    };
    let id = car(w).expect("E1M1 opening car");
    let start = w.actors.get(id).unwrap().pos;
    for _ in 0..70 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let now = w.actors.get(id).expect("car still flying").pos;
    assert!(now.z() < start.z() - 20.0, "car sinks {start:?} → {now:?}");
    for _ in 0..35 * 8 {
        game.tick(&[TicCmd::default()]);
    }
    assert!(car(game.world().unwrap()).is_none(), "car never blew up");
}

#[test]
fn sliding_doors_open_and_rotators_spin() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    for map_name in ["E1M2", "E2M2", "E2M3"] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let w = game.world_mut().unwrap();
        assert!(!w.polyobjs.is_empty(), "{map_name}");
        let script_of = |w: &room_sim::World, p: &room_sim::PolyobjDynamic| {
            p.lines
                .iter()
                .map(|l| &w.map.lines[l.raw() as usize])
                .find(|l| l.special == 80)
                .map(|l| l.args[0])
        };
        let Some(script) = w.polyobjs.iter().find_map(|p| script_of(w, p)) else {
            continue;
        };
        let doors: Vec<(i32, room_core::DVec2, f64)> = w
            .polyobjs
            .iter()
            .filter(|p| script_of(w, p) == Some(script))
            .map(|p| (p.id, p.start_spot, p.angle))
            .collect();
        let rotators: Vec<(i32, f64)> = w
            .polyobjs
            .iter()
            .filter(|p| script_of(w, p).is_none())
            .map(|p| (p.id, p.angle))
            .collect();
        room_sim::execute_special(w, 80, &[script, 0, 0, 0, 0], None, None);
        for _ in 0..70 {
            game.tick(&[TicCmd::default()]);
        }
        let w = game.world().unwrap();
        for (id, spot, angle) in doors {
            let p = w.polyobjs.iter().find(|p| p.id == id).unwrap();
            let moved = (p.start_spot.x() - spot.x()).hypot(p.start_spot.y() - spot.y());
            let turned = (p.angle - angle).abs();
            eprintln!("{map_name} door poly {id}: moved {moved:.1}, turned {turned:.2}");
            assert!(
                moved > 8.0 || turned > 1.0,
                "{map_name} poly {id} did not open"
            );
        }
        for (id, angle) in rotators {
            let p = w.polyobjs.iter().find(|p| p.id == id).unwrap();
            eprintln!("{map_name} rotator {id}: {:.2} → {:.2}", angle, p.angle);
            assert!(p.motion.is_some(), "{map_name} rotator {id} spins");
        }
    }
}

#[test]
fn pipebombs_tripbombs_and_jetpack_work() {
    let Some(bytes) = grp() else {
        return;
    };
    let wad = room_build::grp_to_wad(&bytes).unwrap();
    let vfs = mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap();
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world_mut().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let cid = |w: &room_sim::World, n: &str| w.classes.find_name(n).unwrap();
    for (item, n) in [
        ("DukePipebombWeapon", 1),
        ("DukePipebombAmmo", 3),
        ("DukeTripbombWeapon", 1),
        ("DukeTripbombAmmo", 2),
    ] {
        let c = cid(w, item);
        assert!(w.inventory.give(mo, c, n, 50), "{item}");
    }
    let count = |w: &room_sim::World, n: &str| w.inventory.count(mo, cid(w, n));
    let find = |w: &room_sim::World, n: &str| {
        let c = cid(w, n);
        w.thinkers.default.iter().copied().find(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| a.class == c && !a.destroyed)
        })
    };
    let select = |game: &mut Headless, slot: u8, class: &str| {
        game.tick(&[TicCmd {
            weapon_select: slot,
            ..TicCmd::default()
        }]);
        for _ in 0..70 {
            game.tick(&[TicCmd::default()]);
        }
        let w = game.world().unwrap();
        let ready = w.players[0].as_ref().unwrap().ready_weapon;
        assert_eq!(ready, Some(cid(w, class)), "slot {slot}");
    };
    let fire = |game: &mut Headless| {
        for _ in 0..3 {
            game.tick(&[TicCmd {
                buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
                ..TicCmd::default()
            }]);
        }
        for _ in 0..30 {
            game.tick(&[TicCmd::default()]);
        }
    };

    // Throw a pipebomb, then the remote sets it off.
    select(&mut game, 6, "DukePipebombWeapon");
    fire(&mut game);
    let w = game.world().unwrap();
    let bomb = find(w, "DukeThrownPipebomb").expect("a thrown pipebomb");
    let thrown = w.classes.get(w.actors.get(bomb).unwrap().class).unwrap();
    assert_ne!(
        thrown.flags & room_sim::FLAG_BOUNCEONFLOORS,
        0,
        "pipebombs bounce"
    );
    assert_eq!(w.actors.get(bomb).unwrap().master, Some(mo));
    assert_eq!(count(w, "DukePipebombAmmo"), 2);
    assert_eq!(count(w, "DukeBombsOut"), 1);
    fire(&mut game);
    let w = game.world().unwrap();
    assert!(
        w.actors
            .get(bomb)
            .is_none_or(|a| a.destroyed || a.health <= 0),
        "the remote detonates the pipebomb"
    );
    assert_eq!(count(w, "DukeBombsOut"), 0);
    assert_eq!(count(w, "DukePipebombAmmo"), 2);

    // A placed tripbomb arms, then the laser trips when the player walks
    // through the beam (the placer stands behind the mine).
    game.world_mut().unwrap().players[0].as_mut().unwrap().god = true;
    select(&mut game, 9, "DukeTripbombWeapon");
    fire(&mut game);
    let w = game.world().unwrap();
    let mine = find(w, "DukeTripbomb").expect("a placed tripbomb");
    assert_eq!(w.actors.get(mine).unwrap().master, None);
    assert_eq!(count(w, "DukeTripbombAmmo"), 1);
    assert!(w.classes.find_name("DukeLaserLine").is_some());
    let mut blew = false;
    for i in 0..180 {
        game.tick(&[TicCmd {
            forward_move: if i < 80 { 0 } else { 25 * 256 },
            ..TicCmd::default()
        }]);
        let w = game.world().unwrap();
        if w.actors
            .get(mine)
            .is_none_or(|a| a.destroyed || a.health <= 0)
        {
            blew = true;
            break;
        }
    }
    assert!(blew, "armed tripbomb laser never tripped");

    // The jetpack flies on pickup: `up_move` climbs.
    let w = game.world_mut().unwrap();
    let pos = w.actors.get(mo).unwrap().pos;
    let floor = pos.z();
    let jet = cid(w, "DukeJetpack");
    w.spawn_at(jet, pos, room_core::Angle::ZERO).unwrap();
    game.tick(&[TicCmd::default()]);
    assert!(game.world().unwrap().players[0].as_ref().unwrap().fly_tics > 0);
    for _ in 0..20 {
        game.tick(&[TicCmd {
            up_move: 768,
            ..TicCmd::default()
        }]);
    }
    let z = game.world().unwrap().actors.get(mo).unwrap().pos.z();
    assert!(z > floor + 30.0, "jetpack climb {floor} → {z}");
}

fn duke_vfs(bytes: &[u8]) -> room_vfs::Vfs {
    let wad = room_build::grp_to_wad(bytes).unwrap();
    mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_memory("duke3d.wad", Bytes::from(wad))
        .unwrap()
        .build()
        .unwrap()
}

/// Sound names heard over `tics` tics of `cmd`.
fn run(game: &mut Headless, tics: usize, cmd: TicCmd, heard: &mut Vec<String>) {
    for _ in 0..tics {
        game.tick(&[cmd.clone()]);
        let w = game.world().unwrap();
        heard.extend(w.sound_events.iter().map(|e| e.name.clone()));
    }
}

/// The fan at the bottom of the rooftop vent breaks when shot: broken
/// picture, glass sound, scrap, and it stops blocking the duct.
#[test]
fn vent_fan_breaks_when_shot() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let fan = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors.get(id).is_some_and(|a| {
                (a.pos.x() + 1606.0).abs() < 2.0 && (a.pos.y() + 896.0).abs() < 2.0
            })
        })
        .expect("vent fan");
    for class in [
        "DukeScrap",
        "DukeGlassPiece",
        "DukeSmallSmoke",
        "DukeShell",
        "DukeShotgunShell",
        "DukeCarFire",
        "DukePistolShots",
    ] {
        assert!(w.classes.find_name(class).is_some(), "{class}");
    }
    let a = w.actors.get(fan).unwrap();
    assert!(class_name(w, a.class).starts_with("DukeBreak"));
    assert_ne!(a.flags & room_sim::FLAG_SOLID, 0, "the fan blocks the duct");
    let spin = a.state;
    let (pos, facing) = (a.pos, a.angle);
    // Stand in front of the fan, along its facing, and shoot it.
    {
        let p = game.world_mut().unwrap().actors.get_mut(mo).unwrap();
        let r = facing.radians();
        p.pos = room_core::DVec3::new(pos.x() + 48.0 * r.cos(), pos.y() + 48.0 * r.sin(), pos.z());
        p.angle = room_core::Angle::from_degrees(facing.degrees() + 180.0);
        p.vel = room_core::DVec3::ZERO;
    }
    let mut heard = Vec::new();
    run(
        &mut game,
        6,
        TicCmd {
            buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
            ..TicCmd::default()
        },
        &mut heard,
    );
    run(&mut game, 4, TicCmd::default(), &mut heard);
    let w = game.world().unwrap();
    let a = w.actors.get(fan).expect("the broken fan stays");
    assert!(a.health <= 0, "fan not hit");
    assert_eq!(a.flags & room_sim::FLAG_SOLID, 0, "broken fan still blocks");
    assert_ne!(a.state, spin);
    assert!(
        heard.iter().any(|s| s == "duke/glass_heavybreak"),
        "{heard:?}"
    );
    let scrap = w.classes.find_name("DukeScrap").unwrap();
    assert!(
        w.thinkers
            .default
            .iter()
            .any(|&id| w.actors.get(id).is_some_and(|a| a.class == scrap)),
        "no scrap"
    );
}

/// One shotgun blast: Duke's fire sound, the pump (cock) after it, an ejected
/// shell, a muzzle flash, and about 31 Duke tics (36 Room tics) per shot.
#[test]
fn shotgun_fires_pumps_and_ejects_a_shell() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.cheat_give(0, "all");
    let mut heard = Vec::new();
    run(
        &mut game,
        1,
        TicCmd {
            weapon_select: 3,
            ..TicCmd::default()
        },
        &mut heard,
    );
    run(&mut game, 60, TicCmd::default(), &mut heard);
    heard.clear();
    let w = game.world().unwrap();
    let shells = w.classes.find_name("Shell").unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let before = w.inventory.count(mo, shells);
    let mut flash = false;
    let fire = TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
        ..TicCmd::default()
    };
    game.tick(&[fire]);
    for _ in 0..45 {
        let w = game.world().unwrap();
        heard.extend(w.sound_events.iter().map(|e| e.name.clone()));
        flash |= !w.player_hud(0).unwrap().flash_patch.is_empty();
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    assert_eq!(
        w.inventory.count(mo, shells),
        before - 1,
        "one shell per blast"
    );
    let fired = heard.iter().position(|s| s == "weapons/shotgf");
    let cocked = heard.iter().position(|s| s == "duke/shotgun_cock");
    assert!(
        fired.is_some() && cocked > fired,
        "fire then pump: {heard:?}"
    );
    assert!(flash, "no muzzle flash");
    let ejected = w.classes.find_name("DukeShotgunShell").unwrap();
    assert!(
        w.thinkers
            .default
            .iter()
            .any(|&id| w.actors.get(id).is_some_and(|a| a.class == ejected)),
        "no shotgun shell ejected"
    );
}

/// Duke doors toggle and sound like their `MUSICANDSFX` sprite (or not at all).
#[test]
fn doors_use_their_sector_sounds() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let seq: Vec<usize> = w
        .map
        .sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.sound_sequence.is_empty())
        .map(|(i, _)| i)
        .collect();
    assert!(!seq.is_empty(), "no door sound sequences");
    for &i in &seq {
        let name = &w.map.sectors[i].sound_sequence;
        assert!(
            w.sndseq.choose(name, 0).is_some() && w.sndseq.choose(name, 1).is_some(),
            "sector {i}: {name} not in SNDSEQ"
        );
    }
    // Open one, then close it with a second use.
    let sector = seq[0];
    let tag = w.map.sectors[sector].tags[0];
    let w = game.world_mut().unwrap();
    let mut heard = Vec::new();
    room_sim::execute_special(w, 12, &[tag, 16, 63000, 0, 0], None, None);
    run(&mut game, 70, TicCmd::default(), &mut heard);
    let w = game.world_mut().unwrap();
    let open = w.sectors[sector].ceiling_z;
    assert!(open > w.sectors[sector].floor_z + 8.0, "door did not open");
    room_sim::execute_special(w, 12, &[tag, 16, 63000, 0, 0], None, None);
    run(&mut game, 70, TicCmd::default(), &mut heard);
    let w = game.world().unwrap();
    assert!(w.sectors[sector].ceiling_z < open, "door did not close");
    assert!(heard.iter().any(|s| s.starts_with("duke/")), "{heard:?}");
}

/// The pistol ejects a shell per shot and reloads (clip out, clip in) after 12.
#[test]
fn pistol_reloads_every_twelve_shots() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    let mut heard = Vec::new();
    run(&mut game, 40, TicCmd::default(), &mut heard);
    heard.clear();
    run(
        &mut game,
        140,
        TicCmd {
            buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
            ..TicCmd::default()
        },
        &mut heard,
    );
    let shots = heard.iter().filter(|s| *s == "weapons/pistol").count();
    let reload = heard.iter().position(|s| s == "duke/eject_clip");
    let twelfth = heard
        .iter()
        .enumerate()
        .filter(|(_, s)| *s == "weapons/pistol")
        .nth(11)
        .map(|(i, _)| i);
    assert!(shots > 12, "{shots} shots");
    assert!(reload.is_some() && reload > twelfth, "{heard:?}");
    assert!(heard.iter().any(|s| s == "duke/insert_clip"));
    let w = game.world().unwrap();
    let shell = w.classes.find_name("DukeShell").unwrap();
    assert!(
        w.thinkers
            .default
            .iter()
            .any(|&id| w.actors.get(id).is_some_and(|a| a.class == shell)),
        "no pistol shells"
    );
}

/// The vent on the E1L1 roof stays shut (an `SE 13` floor) until one of its
/// C-9 charges blows; that charge sets off the rest of its hitag, hidden
/// ones included, and the effectors drop the cover.
#[test]
fn a_blown_charge_sets_off_its_group_and_opens_the_vent() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    for _ in 0..100 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let shut = [272usize, 273].map(|s| w.sectors[s].floor_z);
    let alive = |w: &room_sim::World, class: &str| {
        let c = w.classes.find_name(class).unwrap();
        w.thinkers
            .default
            .iter()
            .filter(|&&id| {
                w.actors
                    .get(id)
                    .is_some_and(|a| a.class == c && !a.destroyed && a.tid != 0)
            })
            .count()
    };
    let hidden = alive(w, "DukeHiddenCharge");
    assert!(hidden >= 3, "hidden charges: {hidden}");
    let trigger = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors.get(id).is_some_and(|a| {
                (a.pos.x() + 1515.0).abs() < 2.0 && (a.pos.y() + 879.0).abs() < 2.0
            })
        })
        .expect("vent charge");
    let group = w.actors.get(trigger).unwrap().tid;
    assert!(group >= 20_000, "charge tid {group}");
    room_sim::damage_mobj(game.world_mut().unwrap(), trigger, Some(mo), Some(mo), 100);
    for _ in 0..150 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let open = [272usize, 273].map(|s| w.sectors[s].floor_z);
    assert!(
        open[0] < shut[0] - 8.0 && open[1] < shut[1] - 8.0,
        "vent {shut:?} → {open:?}"
    );
    let left = w
        .thinkers
        .default
        .iter()
        .filter(|&&id| {
            w.actors
                .get(id)
                .is_some_and(|a| a.tid == group && !a.destroyed)
        })
        .count();
    assert_eq!(left, 0, "charges of the group still unlit");
    assert!(
        alive(w, "DukeHiddenCharge") < hidden,
        "hidden charges never blew"
    );
}

/// Shooting a glass wall shatters it: the masked picture goes, it stops
/// blocking, glass flies and breaks audibly.
#[test]
fn glass_walls_break_when_shot() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let glass: Vec<usize> = w
        .map
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.health > 0 && l.sides[1].is_some() && l.block_hitscan)
        .map(|(i, _)| i)
        .collect();
    assert!(!glass.is_empty(), "no breakable glass");
    game.world_mut().unwrap().players[0].as_mut().unwrap().god = true;
    for _ in 0..40 {
        game.tick(&[TicCmd::default()]);
    }
    let mut heard = Vec::new();
    let mut broke = None;
    for &li in &glass {
        let w = game.world_mut().unwrap();
        let l = &w.map.lines[li];
        let (a, b) = (
            w.map.vertices[l.v1.raw() as usize].pos,
            w.map.vertices[l.v2.raw() as usize].pos,
        );
        let (dx, dy) = (b.x() - a.x(), b.y() - a.y());
        let len = dx.hypot(dy);
        if len < 24.0 {
            continue;
        }
        let (nx, ny) = (dy / len, -dx / len);
        let at = (a.x() + dx / 2.0 + nx * 40.0, a.y() + dy / 2.0 + ny * 40.0);
        let Some(s) = room_nodebuild::point_in_sector(&w.map, at.0, at.1) else {
            continue;
        };
        let floor = w.sectors[s.raw() as usize].floor_z;
        // Aim at the middle of the opening: windows sit above a sill.
        let side_z = |side: Option<room_core::SectorId>| {
            side.map(|s| {
                (
                    w.sectors[s.raw() as usize].floor_z,
                    w.sectors[s.raw() as usize].ceiling_z,
                )
            })
        };
        let (Some((f0, c0)), Some((f1, c1))) = (side_z(l.front_sector), side_z(l.back_sector))
        else {
            continue;
        };
        let mid = (f0.max(f1) + c0.min(c1)) / 2.0;
        let pitch = ((mid - floor - 36.0) / 40.0).atan().to_degrees();
        let p = w.actors.get_mut(mo).unwrap();
        p.pos = room_core::DVec3::new(at.0, at.1, floor);
        p.angle = room_core::Angle::from_degrees((-ny).atan2(-nx).to_degrees());
        p.pitch = room_core::Angle::from_degrees(pitch);
        p.vel = room_core::DVec3::ZERO;
        run(
            &mut game,
            8,
            TicCmd {
                buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
                ..TicCmd::default()
            },
            &mut heard,
        );
        run(&mut game, 30, TicCmd::default(), &mut heard);
        let w = game.world().unwrap();
        broke = glass.iter().copied().find(|&g| {
            let front = w.map.lines[g].sides[0].unwrap().raw();
            w.switch_mids.get(&front).is_some_and(|t| t == "-")
        });
        if broke.is_some() {
            break;
        }
    }
    let li = broke.expect("no glass wall broke");
    let w = game.world().unwrap();
    let flags = w.line_flags(room_core::LineId::from_raw(li as u32));
    assert_eq!(flags & 1, 0, "broken glass still blocks");
    assert!(
        heard.iter().any(|s| s == "duke/glass_breaking"),
        "{heard:?}"
    );
}

/// Floor doors (sector lotag 21) open and shut again with each use.
#[test]
fn floor_doors_close_again() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M2");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let door = 153usize;
    let line = w
        .map
        .lines
        .iter()
        .find(|l| {
            l.special == 80
                && [l.front_sector, l.back_sector]
                    .iter()
                    .any(|s| s.is_some_and(|s| s.raw() as usize == door))
        })
        .expect("floor door use line");
    let args = line.args;
    let shut = w.sectors[door].floor_z;
    assert!(
        (w.sectors[door].ceiling_z - shut).abs() < 1.0,
        "starts shut"
    );
    let w = game.world_mut().unwrap();
    room_sim::execute_special(w, 80, &args, None, None);
    for _ in 0..200 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world_mut().unwrap();
    let open = w.sectors[door].floor_z;
    assert!(open < shut - 16.0, "did not open: {shut} → {open}");
    room_sim::execute_special(w, 80, &args, None, None);
    for _ in 0..200 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    assert!(
        (w.sectors[door].floor_z - shut).abs() < 1.0,
        "did not close: {}",
        w.sectors[door].floor_z
    );
}

/// Holding fire winds the pipebomb up (Duke's throw strength) and the throw
/// follows the view pitch.
#[test]
fn pipebomb_throw_strength_and_pitch() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world_mut().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    w.players[0].as_mut().unwrap().god = true;
    for (item, n) in [("DukePipebombWeapon", 1), ("DukePipebombAmmo", 10)] {
        let c = w.classes.find_name(item).unwrap();
        assert!(w.inventory.give(mo, c, n, 50));
    }
    game.tick(&[TicCmd {
        weapon_select: 6,
        ..TicCmd::default()
    }]);
    for _ in 0..70 {
        game.tick(&[TicCmd::default()]);
    }
    let fire = TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
        ..TicCmd::default()
    };
    // Throw after holding `hold` tics at `pitch`; the thrown bomb's velocity.
    let throw = |game: &mut Headless, hold: usize, pitch: f64| -> room_core::DVec3 {
        let w = game.world_mut().unwrap();
        w.actors.get_mut(mo).unwrap().pitch = room_core::Angle::from_degrees(pitch);
        let class = w.classes.find_name("DukeThrownPipebomb").unwrap();
        let before: Vec<_> = w.thinkers.default.clone();
        let mut vel = None;
        for t in 0..hold + 20 {
            let cmd = if t < hold {
                fire.clone()
            } else {
                TicCmd::default()
            };
            game.tick(&[cmd]);
            let w = game.world().unwrap();
            if vel.is_none() {
                vel = w.thinkers.default.iter().find_map(|&id| {
                    w.actors
                        .get(id)
                        .filter(|a| a.class == class && !before.contains(&id))
                        .map(|a| a.vel)
                });
            }
        }
        for _ in 0..30 {
            game.tick(&[TicCmd::default()]);
        }
        // The remote sets it off before the next throw.
        for t in 0..34 {
            let cmd = if t < 3 {
                fire.clone()
            } else {
                TicCmd::default()
            };
            game.tick(&[cmd]);
        }
        vel.expect("no pipebomb thrown")
    };
    let tap = throw(&mut game, 1, 0.0);
    let held = throw(&mut game, 12, 0.0);
    let up = throw(&mut game, 12, 30.0);
    let xy = |v: room_core::DVec3| v.x().hypot(v.y());
    assert!(xy(held) > xy(tap) + 5.0, "tap {tap:?} held {held:?}");
    assert!(up.z() > held.z() + 1.5, "level {held:?} up {up:?}");
}

/// A C-9 of hitag 297 lights the same-hitag `SE 8` sectors (the hole by the
/// E1L1 cinema roof).
#[test]
fn c9_blast_brightens_se8_lights() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    for _ in 0..40 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let before = [221usize, 224].map(|s| w.sectors[s].light);
    let trigger = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors.get(id).is_some_and(|a| {
                a.tid >= 20_000
                    && (a.pos.x() - 976.0).abs() < 4.0
                    && (a.pos.y() + 3580.0).abs() < 4.0
            })
        })
        .expect("C-9 of hitag 297");
    let script = w.actors.get(trigger).unwrap().args[0];
    assert!(script > 0, "charge script {script}");
    let _ = (mo, trigger);
    room_sim::execute_special(
        game.world_mut().unwrap(),
        80,
        &[script, 0, 0, 0, 0],
        None,
        None,
    );
    for _ in 0..20 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let after = [221usize, 224].map(|s| w.sectors[s].light);
    assert!(
        after[0] > before[0] || after[1] > before[1],
        "SE8 lights {before:?} → {after:?}"
    );
}

/// A touchplate `MASTERSWITCH` runs the other effectors in its sector (`SE 32`
/// on E1L4 lowers the ceiling of sector 85).
#[test]
fn masterswitch_runs_its_sector_effectors() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M4");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let line = w
        .map
        .lines
        .iter()
        .find(|l| {
            if l.special != 80 {
                return false;
            }
            let (a, b) = (
                w.map.vertices[l.v1.raw() as usize].pos,
                w.map.vertices[l.v2.raw() as usize].pos,
            );
            let (mx, my) = ((a.x() + b.x()) / 2.0, (a.y() + b.y()) / 2.0);
            (mx - 1008.0).abs() < 80.0 && (my + 1500.0).abs() < 80.0
        })
        .expect("touchplate 180");
    let args = line.args;
    let before = w.sectors[85].ceiling_z;
    room_sim::execute_special(game.world_mut().unwrap(), 80, &args, None, None);
    for _ in 0..80 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    assert!(
        w.sectors[85].ceiling_z < before - 4.0,
        "SE32 ceiling {before} → {}",
        w.sectors[85].ceiling_z
    );
}

/// Glass walls that share a lotag (`SE 128` / tagged panes) break together.
#[test]
fn broken_glass_spreads_to_the_next_pane() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E2M2");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let mut by_group: std::collections::BTreeMap<i32, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, l) in w.map.lines.iter().enumerate() {
        if l.health > 0 && l.health_group != 0 {
            by_group.entry(l.health_group).or_default().push(i);
        }
    }
    let (group, panes) = by_group
        .into_iter()
        .find(|(_, ids)| ids.len() >= 2)
        .expect("grouped glass");
    let first = panes[0];
    assert_eq!(
        room_sim::damage_linedef(
            game.world_mut().unwrap(),
            room_core::LineId::from_raw(first as u32),
            None,
            10
        ),
        0
    );
    for _ in 0..20 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    for &i in &panes {
        assert_eq!(w.map.lines[i].health, 0, "pane {i} of group {group}");
        let front = w.map.lines[i].sides[0].unwrap().raw();
        assert_eq!(
            w.switch_mids.get(&front).map(String::as_str),
            Some("-"),
            "pane {i} mid"
        );
    }
}

/// Shooting an ATM throws `DukeMoney`, which the player can pick up.
#[test]
fn atms_drop_money() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E3M2");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let atm = w
        .map
        .lines
        .iter()
        .enumerate()
        .find(|(_, l)| {
            if l.health <= 0 {
                return false;
            }
            let (a, b) = (
                w.map.vertices[l.v1.raw() as usize].pos,
                w.map.vertices[l.v2.raw() as usize].pos,
            );
            let (mx, my) = ((a.x() + b.x()) / 2.0, (a.y() + b.y()) / 2.0);
            (my + 2004.0).abs() < 8.0 && mx.abs() < 64.0
        })
        .map(|(i, _)| i)
        .expect("ATM wall");
    room_sim::damage_linedef(
        game.world_mut().unwrap(),
        room_core::LineId::from_raw(atm as u32),
        None,
        10,
    );
    for _ in 0..20 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let money = w.classes.find_name("DukeMoney").unwrap();
    let bill = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| w.actors.get(id).is_some_and(|a| a.class == money))
        .expect("no DukeMoney");
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let pos = w.actors.get(bill).unwrap().pos;
    if let Some(p) = game.world_mut().unwrap().actors.get_mut(mo) {
        p.pos = pos;
    }
    for _ in 0..8 {
        if let Some(p) = game.world_mut().unwrap().actors.get_mut(mo) {
            p.pos = pos;
        }
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    assert!(
        room_sim::count_inventory(w, mo, money) >= 1,
        "ATM money was not picked up"
    );
    assert!(
        w.actors.get(bill).is_none() || w.actors.get(bill).is_some_and(|a| a.destroyed),
        "picked-up bill still in the world"
    );
}

/// Using a babe with `DukeMoney` takes one bill (Duke's tip).
#[test]
fn using_a_babe_takes_a_tip() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = None;
    for map in ["E1M1", "E1M2", "E1M3", "E2M1", "E3M1", "E4M1"] {
        let mut g = start(&vfs, map);
        g.tick(&[TicCmd::default()]);
        let w = g.world().unwrap();
        if w.thinkers.default.iter().any(|&id| {
            w.actors.get(id).is_some_and(|a| {
                a.flags & FLAG_USESPECIAL != 0
                    && !a.destroyed
                    && class_name(w, a.class).starts_with("DukeBabe")
            })
        }) {
            game = Some(g);
            break;
        }
    }
    let mut game = game.expect("no babe (DukeBabe) on the tried maps");
    let w = game.world().unwrap();
    let babe = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors.get(id).is_some_and(|a| {
                a.flags & FLAG_USESPECIAL != 0
                    && !a.destroyed
                    && class_name(w, a.class).starts_with("DukeBabe")
            })
        })
        .expect("babe");
    let money = w.classes.find_name("DukeMoney").expect("DukeMoney");
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let (bx, by, bz) = {
        let a = w.actors.get(babe).unwrap();
        (a.pos.x(), a.pos.y(), a.pos.z())
    };
    let _ = game.world_mut().unwrap().drain_sound_events();
    room_sim::give_inventory(game.world_mut().unwrap(), mo, money, 3);
    {
        let w = game.world_mut().unwrap();
        if let Some(p) = w.actors.get_mut(mo) {
            p.pos = room_core::DVec3::new(bx, by - 24.0, bz);
            p.angle = room_core::Angle::from_degrees(90.0);
        }
    }
    let before = room_sim::count_inventory(game.world().unwrap(), mo, money);
    game.tick(&[TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::USE),
        ..TicCmd::default()
    }]);
    let after = room_sim::count_inventory(game.world().unwrap(), mo, money);
    assert!(
        after < before,
        "tip did not take money ({before} → {after})"
    );
    assert!(
        game.world()
            .unwrap()
            .messages
            .iter()
            .any(|m| m.text.contains("You tip her.")),
        "no tip message"
    );
    let sounds: Vec<_> = game
        .world_mut()
        .unwrap()
        .drain_sound_events()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        sounds
            .iter()
            .any(|n| n == "duke/duke_tip1" || n == "duke/duke_tip2"),
        "no tip line: {sounds:?}"
    );
}

/// Shooting a babe plays a CON hit line and drops money or a leftover picture.
#[test]
fn shooting_a_babe_plays_a_line_and_spawns() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = None;
    for map in ["E1M1", "E1M2", "E1M3", "E2M1", "E3M1", "E4M1"] {
        let mut g = start(&vfs, map);
        g.tick(&[TicCmd::default()]);
        let w = g.world().unwrap();
        if w.thinkers.default.iter().any(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| !a.destroyed && class_name(w, a.class).starts_with("DukeBabe"))
        }) {
            game = Some(g);
            break;
        }
    }
    let mut game = game.expect("no babe on the tried maps");
    let w = game.world().unwrap();
    let money = w.classes.find_name("DukeMoney").expect("DukeMoney");
    let bills_before = w
        .thinkers
        .default
        .iter()
        .filter(|&&id| {
            w.actors
                .get(id)
                .is_some_and(|a| a.class == money && !a.destroyed)
        })
        .count();
    let babe = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| !a.destroyed && class_name(w, a.class).starts_with("DukeBabe"))
        })
        .expect("babe");
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let _ = game.world_mut().unwrap().drain_sound_events();
    room_sim::damage_mobj(game.world_mut().unwrap(), babe, Some(mo), Some(mo), 10_000);
    let sounds: Vec<_> = game
        .world_mut()
        .unwrap()
        .drain_sound_events()
        .into_iter()
        .map(|e| e.name)
        .collect();
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let bills = w
        .thinkers
        .default
        .iter()
        .filter(|&&id| {
            w.actors
                .get(id)
                .is_some_and(|a| a.class == money && !a.destroyed)
        })
        .count();
    let line = sounds.iter().any(|n| {
        n == "duke/duke_hit_stripper1" || n == "duke/duke_hit_stripper2" || n == "duke/lady_scream"
    });
    assert!(
        line || bills > bills_before,
        "no CON hit line or money (sounds {sounds:?}, bills {bills_before} → {bills})"
    );
}

/// Using a water fountain heals (Duke's drink, cap `MAXWATERFOUNTAINHEALTH`).
#[test]
fn using_a_fountain_heals() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = None;
    for map in ["E1M1", "E1M2", "E1M3", "E2M1", "E3M1", "E3M2", "E4M1"] {
        let mut g = start(&vfs, map);
        g.tick(&[TicCmd::default()]);
        let w = g.world().unwrap();
        if w.thinkers.default.iter().any(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| !a.destroyed && class_name(w, a.class).starts_with("DukeFountain"))
        }) {
            game = Some(g);
            break;
        }
    }
    let mut game = game.expect("no water fountain on the tried maps");
    let w = game.world().unwrap();
    let fountain = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| !a.destroyed && class_name(w, a.class).starts_with("DukeFountain"))
        })
        .expect("fountain");
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let (fx, fy, fz) = {
        let a = w.actors.get(fountain).unwrap();
        (a.pos.x(), a.pos.y(), a.pos.z())
    };
    {
        let w = game.world_mut().unwrap();
        if let Some(p) = w.actors.get_mut(mo) {
            p.pos = room_core::DVec3::new(fx, fy - 24.0, fz);
            p.angle = room_core::Angle::from_degrees(90.0);
            p.health = 20;
        }
    }
    game.tick(&[TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::USE),
        ..TicCmd::default()
    }]);
    let hp = game
        .world()
        .unwrap()
        .actors
        .get(mo)
        .map(|a| a.health)
        .unwrap_or(0);
    assert!(hp > 20, "fountain did not heal (hp {hp})");
}

/// Shooting a fountain / toilet / stall swaps in the broken picture (Raze
/// `checkhitsprite`) and stops Use from healing.
#[test]
fn shooting_a_fountain_swaps_its_picture() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = None;
    for map in ["E1M1", "E1M2", "E1M3", "E2M1", "E3M1", "E3M2", "E4M1"] {
        let mut g = start(&vfs, map);
        g.tick(&[TicCmd::default()]);
        let w = g.world().unwrap();
        if w.thinkers.default.iter().any(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| !a.destroyed && class_name(w, a.class).starts_with("DukeFountain"))
        }) {
            game = Some(g);
            break;
        }
    }
    let mut game = game.expect("no water fountain on the tried maps");
    let w = game.world().unwrap();
    let fountain = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| !a.destroyed && class_name(w, a.class).starts_with("DukeFountain"))
        })
        .expect("fountain");
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let (sprite, frame, state) = {
        let a = w.actors.get(fountain).unwrap();
        assert!(a.health > 0);
        (a.sprite.clone(), a.frame, a.state)
    };
    room_sim::damage_mobj(game.world_mut().unwrap(), fountain, Some(mo), Some(mo), 1);
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let a = w.actors.get(fountain).expect("broken fountain stays");
    assert!(a.health <= 0, "fountain not broken");
    assert!(
        a.sprite != sprite || a.frame != frame || a.state != state,
        "fountain picture did not change"
    );
    let (fx, fy, fz) = (a.pos.x(), a.pos.y(), a.pos.z());
    {
        let w = game.world_mut().unwrap();
        if let Some(p) = w.actors.get_mut(mo) {
            p.pos = room_core::DVec3::new(fx, fy - 24.0, fz);
            p.angle = room_core::Angle::from_degrees(90.0);
            p.health = 20;
        }
    }
    game.tick(&[TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::USE),
        ..TicCmd::default()
    }]);
    let hp = game
        .world()
        .unwrap()
        .actors
        .get(mo)
        .map(|a| a.health)
        .unwrap_or(0);
    assert_eq!(hp, 20, "broken fountain still healed");
}

/// Killing a monster plays one of Duke's `KillTaunt` lines.
#[test]
fn killing_a_trooper_plays_a_taunt() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let taunts: Vec<String> = w
        .actors
        .get(mo)
        .and_then(|a| w.classes.get(a.class))
        .map(|c| c.kill_taunts.clone())
        .unwrap_or_default();
    assert!(
        !taunts.is_empty(),
        "DukePlayer should have KillTaunt sounds"
    );
    let trooper = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| a.flags & FLAG_COUNTKILL != 0 && !a.destroyed)
        })
        .expect("a COUNTKILL on E1M1");
    let _ = game.world_mut().unwrap().drain_sound_events();
    room_sim::damage_mobj(
        game.world_mut().unwrap(),
        trooper,
        Some(mo),
        Some(mo),
        10_000,
    );
    let sounds: Vec<_> = game
        .world_mut()
        .unwrap()
        .drain_sound_events()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        sounds.iter().any(|n| taunts.iter().any(|t| t == n)),
        "expected one of {taunts:?}, got {sounds:?}"
    );
}

/// Standing still plays a Duke knuckle-crack / wait line.
#[test]
fn standing_still_plays_an_idle_quip() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let taunts: Vec<String> = w
        .actors
        .get(mo)
        .and_then(|a| w.classes.get(a.class))
        .map(|c| c.idle_taunts.clone())
        .unwrap_or_default();
    assert!(
        !taunts.is_empty(),
        "DukePlayer should have IdleTaunt sounds"
    );
    let _ = game.world_mut().unwrap().drain_sound_events();
    {
        let p = game.world_mut().unwrap().players[0].as_mut().unwrap();
        p.god = true;
        p.idle_tics = 0;
        p.on_ground = true;
    }
    let mut heard = false;
    for _ in 0..160 {
        game.tick(&[TicCmd::default()]);
        if game.world().unwrap().players[0]
            .as_ref()
            .is_some_and(|p| p.on_ground)
        {
            // keep waiting
        }
        heard = game
            .world()
            .unwrap()
            .sound_events
            .iter()
            .any(|e| taunts.iter().any(|t| t == &e.name));
        if heard {
            break;
        }
    }
    assert!(heard, "no idle quip after standing still");
}

/// `SE 33` drops `DukeScrap` while a quake is felt.
#[test]
fn se33_drops_scrap_during_a_quake() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let debris = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors
                .get(id)
                .is_some_and(|a| class_name(w, a.class) == "DukeQuakeDebris" && !a.destroyed)
        })
        .expect("SE 33 DukeQuakeDebris on E1M6");
    let scrap = w.classes.find_name("DukeScrap").expect("DukeScrap");
    let before = w
        .thinkers
        .default
        .iter()
        .filter(|&&id| {
            w.actors
                .get(id)
                .is_some_and(|a| a.class == scrap && !a.destroyed)
        })
        .count();
    assert_eq!(
        room_sim::call_action_args(
            game.world_mut().unwrap(),
            debris,
            "A_Quake",
            &[
                "5".into(),
                "48".into(),
                "0".into(),
                "2048".into(),
                "world/quake".into(),
            ],
        ),
        room_sim::ActionResult::Continue
    );
    for _ in 0..16 {
        game.tick(&[TicCmd::default()]);
    }
    let after = game
        .world()
        .unwrap()
        .thinkers
        .default
        .iter()
        .filter(|&&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == scrap && !a.destroyed)
        })
        .count();
    assert!(after > before, "SE 33 scrap {before} → {after}");
}

/// A crouched (move-down on the ground) pipebomb throw has no standing lift.
#[test]
fn pipebomb_crouch_roll() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world_mut().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    w.players[0].as_mut().unwrap().god = true;
    for (item, n) in [("DukePipebombWeapon", 1), ("DukePipebombAmmo", 10)] {
        let c = w.classes.find_name(item).unwrap();
        assert!(w.inventory.give(mo, c, n, 50));
    }
    game.tick(&[TicCmd {
        weapon_select: 6,
        ..TicCmd::default()
    }]);
    for _ in 0..70 {
        game.tick(&[TicCmd::default()]);
    }
    let fire = |up: i16| TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::ATTACK),
        up_move: up,
        ..TicCmd::default()
    };
    let throw = |game: &mut Headless, crouch: bool| -> room_core::DVec3 {
        let w = game.world_mut().unwrap();
        w.actors.get_mut(mo).unwrap().pitch = room_core::Angle::ZERO;
        assert!(w.players[0].as_ref().unwrap().on_ground);
        let class = w.classes.find_name("DukeThrownPipebomb").unwrap();
        let before: Vec<_> = w.thinkers.default.clone();
        let mut vel = None;
        let up = if crouch { -64 } else { 0 };
        for t in 0..16 {
            game.tick(&[if t < 1 {
                fire(up)
            } else {
                TicCmd {
                    up_move: up,
                    ..TicCmd::default()
                }
            }]);
            let w = game.world().unwrap();
            if vel.is_none() {
                vel = w.thinkers.default.iter().find_map(|&id| {
                    w.actors
                        .get(id)
                        .filter(|a| a.class == class && !before.contains(&id))
                        .map(|a| a.vel)
                });
            }
        }
        for _ in 0..20 {
            game.tick(&[TicCmd::default()]);
        }
        for t in 0..20 {
            game.tick(&[if t < 3 { fire(0) } else { TicCmd::default() }]);
        }
        vel.expect("no pipebomb thrown")
    };
    let stand = throw(&mut game, false);
    let crouch = throw(&mut game, true);
    assert!(
        crouch.z() < stand.z() - 0.4,
        "stand {stand:?} crouch {crouch:?}"
    );
}

/// A ceiling door with `SE 8` lights the same-hitag rooms when it opens.
#[test]
fn opening_a_door_brightens_se8_lights() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in ["E1M1", "E1M2", "E1M3", "E1M4", "E2M1"] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        let Some((door, light_tag, bright)) =
            w.map
                .udmf_user
                .sectors
                .iter()
                .enumerate()
                .find_map(|(i, bag)| {
                    let tag = *bag.ints.get("user_doorlight")?;
                    let bright = *bag.ints.get("user_doorbright")?;
                    if bag.ints.get("user_doorlightopen").copied().unwrap_or(1) == 0 {
                        return None;
                    }
                    Some((i, tag, bright as i16))
                })
        else {
            continue;
        };
        let Some(&door_tag) = w.map.sectors[door].tags.first() else {
            continue;
        };
        let rooms: Vec<usize> = w
            .map
            .sectors
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tags.contains(&light_tag))
            .map(|(i, _)| i)
            .collect();
        assert!(
            !rooms.is_empty(),
            "{map_name} door {door} light tag {light_tag} has no rooms"
        );
        let before: Vec<i16> = rooms.iter().map(|&i| w.sectors[i].light).collect();
        assert_eq!(
            room_sim::execute_special(
                game.world_mut().unwrap(),
                12,
                &[door_tag, 16, 150, 0, 0],
                None,
                None,
            ),
            1
        );
        let w = game.world().unwrap();
        let after: Vec<i16> = rooms.iter().map(|&i| w.sectors[i].light).collect();
        assert!(
            after.iter().any(|&l| l == bright) || after.iter().zip(&before).any(|(&a, &b)| a > b),
            "{map_name} door {door} lights {before:?} → {after:?} want {bright}"
        );
        return;
    }
    panic!("no ceiling-door SE 8 on E1M1–E2M1");
}

/// A force-field sprite hurts the player on contact (`ContactDamage`).
#[test]
fn force_fields_hurt_on_touch() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let hp0 = w.actors.get(mo).unwrap().health;
    let pos = if let Some(field) = w.thinkers.default.iter().copied().find(|&id| {
        w.actors
            .get(id)
            .is_some_and(|a| w.classes.get(a.class).is_some_and(|c| c.contact_damage > 0))
    }) {
        w.actors.get(field).unwrap().pos
    } else {
        let (i, line) = w
            .map
            .udmf_user
            .lines
            .iter()
            .enumerate()
            .find(|(_, b)| b.ints.get("user_contactdamage").is_some_and(|&d| d > 0))
            .map(|(i, _)| (i, &w.map.lines[i]))
            .expect("an E1M1 force field");
        let a = w.map.vertices[line.v1.raw() as usize].pos;
        let b = w.map.vertices[line.v2.raw() as usize].pos;
        let _ = i;
        room_core::DVec3::new((a.x() + b.x()) / 2.0, (a.y() + b.y()) / 2.0, 0.0)
    };
    {
        let p = game.world_mut().unwrap().players[0].as_mut().unwrap();
        p.god = false;
        p.invulnerable = false;
    }
    for _ in 0..16 {
        if let Some(p) = game.world_mut().unwrap().actors.get_mut(mo) {
            p.pos = pos;
        }
        game.tick(&[TicCmd::default()]);
    }
    let hp = game
        .world()
        .unwrap()
        .actors
        .get(mo)
        .map(|a| a.health)
        .unwrap_or(0);
    assert!(hp < hp0, "force field {hp0} → {hp}");
}

/// Shooting a `BIGFORCE` wall removes its contact damage and blocking.
#[test]
fn force_field_walls_break_when_shot() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let li = w
        .map
        .lines
        .iter()
        .enumerate()
        .find(|(i, l)| {
            l.health > 0
                && w.map
                    .udmf_user
                    .lines
                    .get(*i)
                    .is_some_and(|b| b.ints.get("user_contactdamage").is_some_and(|&d| d > 0))
        })
        .map(|(i, _)| i)
        .expect("a breakable force-field wall");
    assert_eq!(
        room_sim::damage_linedef(
            game.world_mut().unwrap(),
            room_core::LineId::from_raw(li as u32),
            None,
            1
        ),
        0
    );
    for _ in 0..8 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    assert_eq!(w.map.lines[li].health, 0);
    assert!(!w.map.lines[li].block_projectiles);
    assert!(
        w.map
            .udmf_user
            .lines
            .get(li)
            .is_none_or(|b| !b.ints.get("user_contactdamage").is_some_and(|&d| d > 0)),
        "broken force wall still hurts"
    );
}

/// A C-9 blast breaks nearby health walls (force fields / glass).
#[test]
fn c9_blast_deletes_nearby_wall_geometry() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in ["E2M1", "E1M1", "E2M6"] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        let mut pair = None;
        for &cid in &w.thinkers.default {
            let Some(c) = w.actors.get(cid) else { continue };
            if c.destroyed || !(20_000..21_000).contains(&c.tid) {
                continue;
            }
            if !class_name(w, c.class).contains("Prop") {
                continue;
            }
            let (cx, cy) = (c.pos.x(), c.pos.y());
            for (i, line) in w.map.lines.iter().enumerate() {
                if line.health <= 0 {
                    continue;
                }
                let a = w.map.vertices[line.v1.raw() as usize].pos;
                let b = w.map.vertices[line.v2.raw() as usize].pos;
                let (ex, ey) = (b.x() - a.x(), b.y() - a.y());
                let len2 = ex * ex + ey * ey;
                let t = if len2 > 0.0 {
                    (((cx - a.x()) * ex + (cy - a.y()) * ey) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let d = (cx - (a.x() + ex * t)).hypot(cy - (a.y() + ey * t));
                if d < 96.0 {
                    pair = Some((cid, i, d));
                    break;
                }
            }
            if pair.is_some() {
                break;
            }
        }
        let Some((cid, li, _)) = pair else {
            continue;
        };
        room_sim::damage_mobj(game.world_mut().unwrap(), cid, None, None, 10_000);
        for _ in 0..80 {
            game.tick(&[TicCmd::default()]);
        }
        let w = game.world().unwrap();
        assert_eq!(
            w.map.lines[li].health, 0,
            "{map_name} C-9 did not break wall {li}"
        );
        return;
    }
    panic!("no C-9 next to a health wall on E2M1/E1M1/E2M6");
}

/// `/` quick-kicks without leaving the pistol: knee Fire, then the pistol Ready.
#[test]
fn quick_kick_returns_to_the_pistol() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let pistol = w.classes.find_name("DukePistol").expect("DukePistol");
    let kick = w.classes.find_name("DukeKick").expect("DukeKick");
    let ready = w.players[0].as_ref().unwrap().ready_weapon;
    assert_eq!(ready, Some(pistol));
    game.tick(&[TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::QUICKKICK),
        ..TicCmd::default()
    }]);
    let mid = game.world().unwrap().players[0].as_ref().unwrap();
    assert_eq!(mid.ready_weapon, Some(kick));
    assert_eq!(mid.quick_kick_restore, Some(pistol));
    for _ in 0..40 {
        game.tick(&[TicCmd::default()]);
    }
    let after = game.world().unwrap().players[0].as_ref().unwrap();
    assert_eq!(after.ready_weapon, Some(pistol));
    assert!(after.quick_kick_restore.is_none());
}

/// `'` (`+invuse`) heals from a stored medkit, then starts steroids when at full health.
#[test]
fn using_inventory_heals_then_starts_steroids() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let kit = w.classes.find_name("DukeFirstAid").expect("DukeFirstAid");
    let st = w.classes.find_name("DukeSteroids").expect("DukeSteroids");
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    room_sim::give_inventory(game.world_mut().unwrap(), mo, kit, 20);
    room_sim::give_inventory(game.world_mut().unwrap(), mo, st, 1);
    game.world_mut().unwrap().actors.get_mut(mo).unwrap().health = 50;
    let inv = TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::INVUSE),
        ..TicCmd::default()
    };
    for _ in 0..10 {
        game.tick(&[inv]);
    }
    let w = game.world().unwrap();
    assert_eq!(w.actors.get(mo).unwrap().health, 60);
    assert_eq!(room_sim::count_inventory(w, mo, kit), 10);
    assert_eq!(room_sim::count_inventory(w, mo, st), 1);
    game.world_mut().unwrap().actors.get_mut(mo).unwrap().health = 100;
    game.tick(&[TicCmd::default()]);
    game.tick(&[inv]);
    let after = game.world().unwrap();
    assert_eq!(after.players[0].as_ref().unwrap().steroid_tics, 420);
    assert_eq!(room_sim::count_inventory(after, mo, kit), 10);
    assert_eq!(room_sim::count_inventory(after, mo, st), 0);
}

/// `'` also starts night vision, scuba (air + radsuit), and a holoduke decoy.
#[test]
fn using_inventory_starts_night_vision_and_a_holoduke() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let nv = w
        .classes
        .find_name("DukeNightVision")
        .expect("DukeNightVision");
    let scuba = w.classes.find_name("DukeScubaGear").expect("DukeScubaGear");
    let holo = w.classes.find_name("DukeHoloduke").expect("DukeHoloduke");
    let decoy = w
        .classes
        .find_name("DukeHolodukeDecoy")
        .expect("DukeHolodukeDecoy");
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    game.world_mut().unwrap().actors.get_mut(mo).unwrap().health = 100;
    room_sim::give_inventory(game.world_mut().unwrap(), mo, nv, 1);
    room_sim::give_inventory(game.world_mut().unwrap(), mo, scuba, 1);
    room_sim::give_inventory(game.world_mut().unwrap(), mo, holo, 1);
    let inv = TicCmd {
        buttons: room_sim::Buttons(room_sim::Buttons::INVUSE),
        ..TicCmd::default()
    };
    game.tick(&[inv]);
    {
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert!(p.infrared);
        assert!(p.infrared_tics > 1000);
        assert_eq!(
            game.hud_state(0, false).map(|h| h.palette),
            Some(13),
            "night-vision green palette"
        );
    }
    game.tick(&[TicCmd::default()]);
    game.tick(&[inv]);
    {
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert!(p.radsuit);
        assert!(p.air_finished > 0);
    }
    game.tick(&[TicCmd::default()]);
    game.tick(&[inv]);
    let after = game.world().unwrap();
    assert_eq!(room_sim::count_inventory(after, mo, holo), 0);
    let decoy_id = after
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| after.actors.get(id).is_some_and(|a| a.class == decoy))
        .expect("holoduke decoy");
    let trooper = after
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            after.actors.get(id).is_some_and(|a| {
                after
                    .classes
                    .get(a.class)
                    .is_some_and(|c| c.name == "DukeTrooper")
                    && a.health > 0
            })
        })
        .expect("a trooper");
    let at = after.actors.get(trooper).unwrap().pos;
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(decoy_id) {
        a.pos = room_core::DVec3::new(at.x() + 32.0, at.y(), at.z());
    }
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
        a.flags |= room_sim::FLAG_NOTARGET;
    }
    room_sim::call_action(game.world_mut().unwrap(), trooper, "A_Look");
    assert_eq!(
        game.world().unwrap().actors.get(trooper).unwrap().target,
        Some(decoy_id),
        "trooper hunts the holoduke"
    );
}

/// Shrinker scales a trooper; freezer holds it, then a second hit shatters.
#[test]
fn shrinker_and_freezer_change_a_trooper() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let spark = w
        .classes
        .find_name("DukeShrinkSpark")
        .expect("DukeShrinkSpark");
    let blast = w
        .classes
        .find_name("DukeFreezeBlast")
        .expect("DukeFreezeBlast");
    let trooper = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors.get(id).is_some_and(|a| {
                w.classes
                    .get(a.class)
                    .is_some_and(|c| c.name == "DukeTrooper")
                    && a.health > 0
            })
        })
        .expect("a trooper");
    let pos = w.actors.get(trooper).unwrap().pos;
    let bolt = game
        .world_mut()
        .unwrap()
        .spawn_at(spark, pos, Default::default())
        .unwrap();
    assert_eq!(
        room_sim::damage_mobj(
            game.world_mut().unwrap(),
            trooper,
            Some(bolt),
            Some(bolt),
            8
        ),
        0
    );
    let a = game.world().unwrap().actors.get(trooper).unwrap();
    assert!((a.scale_x - a.scale_y).abs() < 1e-9);
    assert!(a.scale_x < 0.5, "shrunk scale {}", a.scale_x);
    assert!(a.health > 0);
    let shrink = game
        .world()
        .unwrap()
        .states
        .find_label(a.class, "Shrink")
        .expect("Shrink pose");
    assert_eq!(a.state, Some(shrink));
    let want = a.scale_x * 4.0;
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(trooper) {
        a.user_vars.insert("user_dukeshink".into(), 1);
    }
    game.tick(&[TicCmd::default()]);
    let grown = game.world().unwrap().actors.get(trooper).unwrap();
    assert!(
        (grown.scale_x - want).abs() < 0.05,
        "grow-back scale {} want {want}",
        grown.scale_x
    );
    let grow = game
        .world()
        .unwrap()
        .classes
        .find_name("DukeGrowSpark")
        .expect("DukeGrowSpark");
    let pos = game.world().unwrap().actors.get(trooper).unwrap().pos;
    let before = game.world().unwrap().actors.get(trooper).unwrap().scale_x;
    let swell = game
        .world_mut()
        .unwrap()
        .spawn_at(grow, pos, Default::default())
        .unwrap();
    assert_eq!(
        room_sim::damage_mobj(
            game.world_mut().unwrap(),
            trooper,
            Some(swell),
            Some(swell),
            8
        ),
        0
    );
    let after = game.world().unwrap().actors.get(trooper).unwrap().scale_x;
    assert!(after > before * 1.5, "expander swell {before} → {after}");
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(trooper) {
        a.user_vars.insert("user_dukegrow".into(), 1);
    }
    game.tick(&[TicCmd::default()]);
    assert!(
        game.world()
            .unwrap()
            .actors
            .get(trooper)
            .is_none_or(|a| a.destroyed || a.health <= 0),
        "expander pop"
    );

    let other = game
        .world()
        .unwrap()
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            id != trooper
                && game.world().unwrap().actors.get(id).is_some_and(|a| {
                    game.world()
                        .unwrap()
                        .classes
                        .get(a.class)
                        .is_some_and(|c| c.name == "DukeTrooper")
                        && a.health > 0
                })
        })
        .expect("another trooper");
    let pos = game.world().unwrap().actors.get(other).unwrap().pos;
    let ice = game
        .world_mut()
        .unwrap()
        .spawn_at(blast, pos, Default::default())
        .unwrap();
    assert_eq!(
        room_sim::damage_mobj(game.world_mut().unwrap(), other, Some(ice), Some(ice), 6),
        0
    );
    let hp = game.world().unwrap().actors.get(other).unwrap().health;
    assert!(hp > 0);
    game.tick(&[TicCmd::default()]);
    let still = game.world().unwrap().actors.get(other).unwrap();
    assert_eq!(still.vel.x(), 0.0);
    assert_eq!(still.vel.y(), 0.0);
    let ice_st = game
        .world()
        .unwrap()
        .states
        .find_label(still.class, "Ice")
        .expect("Ice pose");
    assert_eq!(still.state, Some(ice_st));
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(other) {
        a.user_vars.insert("user_dukefreeze".into(), 1);
    }
    game.tick(&[TicCmd::default()]);
    let statue = game.world().unwrap().actors.get(other).unwrap();
    assert!(statue.health > 0);
    assert_eq!(statue.user_vars.get("user_dukeicecorpse").copied(), Some(1));
    assert_eq!(
        room_sim::damage_mobj(game.world_mut().unwrap(), other, Some(ice), Some(ice), 6),
        6
    );
    assert!(
        game.world()
            .unwrap()
            .actors
            .get(other)
            .is_none_or(|a| a.destroyed || a.health <= 0)
    );
}

/// Palette-21 troopers are captains that warp to a free nearby spot.
#[test]
fn trooper_captains_teleport() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let cap = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors.get(id).is_some_and(|a| {
                w.classes
                    .get(a.class)
                    .is_some_and(|c| c.name == "DukeTrooperCaptain")
            })
        })
        .expect("a pal-21 captain");
    let at = w
        .actors
        .get(w.players[0].as_ref().unwrap().mo.unwrap())
        .unwrap()
        .pos;
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(cap) {
        a.pos = at;
    }
    let start = game.world().unwrap().actors.get(cap).unwrap().pos;
    let mut moved = false;
    for _ in 0..24 {
        room_sim::call_action_args(
            game.world_mut().unwrap(),
            cap,
            "A_RandomTeleport",
            &["192".into()],
        );
        let p = game.world().unwrap().actors.get(cap).unwrap().pos;
        if (p.x() - start.x()).hypot(p.y() - start.y()) > 8.0 {
            moved = true;
            break;
        }
    }
    assert!(moved, "captain never found a free warp");
}

/// Palette-21 enforcers are mini-bosses with double health.
#[test]
fn palette_mini_bosses_have_more_health() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E2M3");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let hp = |name: &str| {
        w.thinkers.default.iter().copied().find_map(|id| {
            let a = w.actors.get(id)?;
            (w.classes.get(a.class)?.name == name && a.health > 0).then_some(a.health)
        })
    };
    let mini = hp("DukeEnforcerMini").expect("pal-21 enforcer");
    let base = hp("DukeEnforcer").unwrap_or(mini / 2);
    assert!(mini >= base * 2, "mini {mini} vs enforcer {base}");
}

/// Fully submerged in a waterzone uses PLAYPAL-style index 14 (blue).
#[test]
fn underwater_uses_a_blue_palette() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M3");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let zones: Vec<usize> = w
        .map
        .sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| s.waterzone)
        .map(|(i, _)| i)
        .collect();
    assert!(!zones.is_empty(), "E1M3 waterzone sectors");
    let (x, y, z) = w
        .map
        .lines
        .iter()
        .find_map(|l| {
            let sid = l.front_sector?.raw() as usize;
            if !zones.contains(&sid) {
                return None;
            }
            let a = w.map.vertices[l.v1.raw() as usize].pos;
            let b = w.map.vertices[l.v2.raw() as usize].pos;
            let dx = b.y() - a.y();
            let dy = a.x() - b.x();
            let len = dx.hypot(dy).max(1.0);
            let ceil = w.sectors.get(sid)?.ceiling_z;
            let floor = w.sectors.get(sid)?.floor_z;
            Some((
                (a.x() + b.x()) * 0.5 + dx / len * 8.0,
                (a.y() + b.y()) * 0.5 + dy / len * 8.0,
                floor + (ceil - floor) * 0.25,
            ))
        })
        .expect("E1M3 waterzone line");
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
        a.pos = room_core::DVec3::new(x, y, z);
    }
    if let Some(p) = game.world_mut().unwrap().players[0].as_mut() {
        p.damage_count = 0;
        p.bonus_count = 0;
    }
    assert_eq!(
        game.hud_state(0, false).map(|h| h.palette),
        Some(14),
        "underwater blue palette"
    );
}

/// Using a viewscreen looks through the nearest security camera.
#[test]
fn using_a_viewscreen_looks_through_a_camera() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let screen = w
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            w.actors.get(id).is_some_and(|a| {
                w.classes
                    .get(a.class)
                    .is_some_and(|c| c.name == "DukeViewScreen")
            })
        })
        .expect("a viewscreen");
    assert!(
        w.thinkers.default.iter().any(|&id| {
            w.actors.get(id).is_some_and(|a| {
                w.classes
                    .get(a.class)
                    .is_some_and(|c| c.name == "DukeCamera")
            })
        }),
        "a camera"
    );
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(screen) {
        a.target = Some(mo);
    }
    room_sim::call_action_args(
        game.world_mut().unwrap(),
        screen,
        "A_ChangeCameraToNearest",
        &["DukeCamera".into(), "1".into()],
    );
    let w = game.world().unwrap();
    let p = w.players[0].as_ref().unwrap();
    let cam = p.camera.expect("camera view");
    assert!(p.camera_revert);
    assert_eq!(
        w.classes
            .get(w.actors.get(cam).unwrap().class)
            .unwrap()
            .name,
        "DukeCamera"
    );
    let bind = w
        .camera_textures
        .get("VCAMA0")
        .expect("viewscreen camera canvas");
    assert_eq!(
        bind.camera, cam,
        "canvas follows the camera you look through"
    );
}

/// Lotag-27 stretch bridges start walkable (always extended).
#[test]
fn e1m1_bridges_are_walkable() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let bridges: Vec<(usize, f64)> = w
        .map
        .udmf_user
        .sectors
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            (b.ints.get("user_bridge") == Some(&1))
                .then(|| w.sectors.get(i).map(|s| (i, s.ceiling_z - s.floor_z)))
                .flatten()
        })
        .collect();
    assert!(
        bridges.iter().any(|&(_, h)| h > 16.0),
        "a collapsed E1M1 bridge opened ({bridges:?})"
    );
}

/// Lotag-30 gears carry the player around their SE1 pivot.
#[test]
fn walking_a_lotag30_gear_turns_you() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M2");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    assert!(!w.spinners.is_empty(), "E1M2 walkable gears");
    let spin = w.spinners[0].clone();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    let floor = w
        .sectors
        .get(spin.sector.raw() as usize)
        .map(|s| s.floor_z)
        .unwrap_or(0.0);
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
        a.pos = room_core::DVec3::new(spin.cx, spin.cy + 16.0, floor);
        a.vel = room_core::DVec3::ZERO;
        a.radius = 1.0;
    }
    let (x0, y0) = {
        let a = game.world().unwrap().actors.get(mo).unwrap();
        (a.pos.x(), a.pos.y())
    };
    for _ in 0..35 {
        room_sim::tick_spinners(game.world_mut().unwrap());
    }
    let a = game.world().unwrap().actors.get(mo).unwrap();
    let (x1, y1) = (a.pos.x(), a.pos.y());
    let d0 = (x0 - spin.cx).hypot(y0 - spin.cy);
    let d1 = (x1 - spin.cx).hypot(y1 - spin.cy);
    assert!((d0 - d1).abs() < 12.0, "stayed on the gear {d0} -> {d1}");
    assert!(
        (x1 - x0).hypot(y1 - y0) > 4.0,
        "gear turned ({x0},{y0}) -> ({x1},{y1})"
    );
}

/// E2M1 subway carries the player toward the next locator.
#[test]
fn riding_e2m1_subway_follows_locators() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E2M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    assert!(!w.subways.is_empty(), "E2M1 subway cars");
    let car = w.subways[0].clone();
    let dest = car.points[car.i % car.points.len()];
    let po = car.poly.expect("E2M1 subway car walls");
    let spot0 = w
        .polyobjs
        .iter()
        .find(|p| p.id == po)
        .map(|p| (p.start_spot.x(), p.start_spot.y()))
        .expect("subway poly");
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
        a.pos = room_core::DVec3::new(car.x, car.y, a.pos.z());
        a.vel = room_core::DVec3::ZERO;
        a.radius = 1.0;
    }
    let (x0, y0) = {
        let a = game.world().unwrap().actors.get(mo).unwrap();
        (a.pos.x(), a.pos.y())
    };
    for _ in 0..35 {
        room_sim::tick_subways(game.world_mut().unwrap());
        room_sim::tick_polyobjs(game.world_mut().unwrap());
    }
    let a = game.world().unwrap().actors.get(mo).unwrap();
    let (x1, y1) = (a.pos.x(), a.pos.y());
    let d0 = (x0 - dest.0).hypot(y0 - dest.1);
    let d1 = (x1 - dest.0).hypot(y1 - dest.1);
    assert!(d1 < d0 - 8.0, "closer to the next locator {d0} -> {d1}");
    assert!(
        (x1 - x0).hypot(y1 - y0) > 4.0,
        "subway moved ({x0},{y0}) -> ({x1},{y1})"
    );
    let spot1 = game
        .world()
        .unwrap()
        .polyobjs
        .iter()
        .find(|p| p.id == po)
        .map(|p| (p.start_spot.x(), p.start_spot.y()))
        .unwrap();
    assert!(
        (spot1.0 - spot0.0).hypot(spot1.1 - spot0.1) > 4.0,
        "car walls moved {spot0:?} -> {spot1:?}"
    );
}

/// E1M1 `SE 20` stretch slides the bridge vertices when toggled.
#[test]
fn stretching_an_e1m1_bridge_moves_its_walls() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    assert!(!w.stretches.is_empty(), "E1M1 stretch bridges");
    let st = w.stretches[0].clone();
    let vid = st.verts[0];
    let (x0, y0) = {
        let v = &w.map.vertices[vid.raw() as usize];
        (v.pos.x(), v.pos.y())
    };
    game.world_mut().unwrap().stretches[0].sign = 1;
    for _ in 0..20 {
        room_sim::tick_stretches(game.world_mut().unwrap());
    }
    let w = game.world().unwrap();
    let v = &w.map.vertices[vid.raw() as usize];
    assert!(
        (v.pos.x() - x0).hypot(v.pos.y() - y0) > 4.0,
        "bridge walls moved ({x0},{y0}) -> ({},{})",
        v.pos.x(),
        v.pos.y()
    );
}

/// A `CYCLER` pulses its sector light (`Light_Glow`).
#[test]
fn cycler_pulses_sector_light() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in ["E1M1", "E1M2", "E1M3", "E2M1", "E3M1", "E4M1"] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        if w.glows.is_empty() {
            continue;
        }
        let glow = w.glows[0].clone();
        let light0 = w
            .sectors
            .get(glow.sector.raw() as usize)
            .map(|s| s.light)
            .unwrap_or(0);
        for _ in 0..glow.tics_max.max(8) {
            game.tick(&[TicCmd::default()]);
        }
        let light1 = game
            .world()
            .unwrap()
            .sectors
            .get(glow.sector.raw() as usize)
            .map(|s| s.light)
            .unwrap_or(0);
        assert_ne!(light0, light1, "{map_name} cycler light stayed {light0}");
        return;
    }
    panic!("no CYCLER glow on E1M1–E4M1");
}

/// A `DukeRespawn` marker is non-solid and can spawn its doomednum.
#[test]
fn respawn_marker_is_non_solid_and_spawns() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in ["E1M1", "E1M2", "E1M3", "E2M1", "E3M1", "E4M1"] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        let Some(id) = w.thinkers.default.iter().copied().find(|&id| {
            w.actors
                .get(id)
                .and_then(|a| w.classes.get(a.class))
                .is_some_and(|c| c.name == "DukeRespawn")
        }) else {
            continue;
        };
        let a = w.actors.get(id).unwrap();
        assert_eq!(a.flags & FLAG_SOLID, 0, "{map_name} RESPAWN does not block");
        assert_ne!(a.args[0], 0, "{map_name} respawn doomednum");
        return;
    }
    panic!("no DukeRespawn on E1M1–E4M1");
}

/// A water-bubble maker pops `DukeWaterBubble` actors.
#[test]
fn water_bubble_maker_spawns_bubbles() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in ["E1M1", "E1M2", "E1M3", "E2M1", "E3M1", "E4M1"] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        if !w.thinkers.default.iter().any(|&id| {
            w.actors
                .get(id)
                .and_then(|a| w.classes.get(a.class))
                .is_some_and(|c| c.name == "DukeWaterBubbleMaker")
        }) {
            continue;
        }
        for _ in 0..20 {
            game.tick(&[TicCmd::default()]);
        }
        let w = game.world().unwrap();
        let bubbles = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                w.actors
                    .get(id)
                    .and_then(|a| w.classes.get(a.class))
                    .is_some_and(|c| c.name == "DukeWaterBubble")
            })
            .count();
        assert!(bubbles >= 1, "{map_name} water bubbles ({bubbles})");
        return;
    }
    panic!("no WATERBUBBLEMAKER on E1M1–E4M1");
}

/// `DukeNaturalLightning` flashes the map (`Light_ForceLightning`).
#[test]
fn natural_lightning_flashes_the_map() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in [
        "E1M1", "E1M2", "E1M3", "E2M1", "E2M6", "E3M1", "E3M6", "E4M1", "E4M5",
    ] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        let Some(id) = w.thinkers.default.iter().copied().find(|&id| {
            w.actors
                .get(id)
                .and_then(|a| w.classes.get(a.class))
                .is_some_and(|c| c.name == "DukeNaturalLightning")
        }) else {
            continue;
        };
        let light0 = w.sectors.first().map(|s| s.light).unwrap_or(0);
        assert_eq!(
            room_sim::execute_special(
                game.world_mut().unwrap(),
                109,
                &[0, 0, 0, 0, 0],
                Some(id),
                None
            ),
            1
        );
        let w = game.world().unwrap();
        assert!(!w.fades.is_empty(), "{map_name} lightning fade");
        let light1 = w.sectors.first().map(|s| s.light).unwrap_or(0);
        assert!(light1 >= light0, "{map_name} flash {light0} -> {light1}");
        return;
    }
    panic!("no NATURALLIGHTNING on sample maps");
}

/// A `FORCESPHERE` pulses its scale (Atomic maps rarely place the tile).
#[test]
fn force_sphere_pulses_scale() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let (cls, pos) = {
        let w = game.world().unwrap();
        let cls = w.classes.find_name("DukeForceSphere").expect("class");
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        (cls, w.actors.get(mo).unwrap().pos)
    };
    let id = game
        .world_mut()
        .unwrap()
        .spawn_at(cls, pos, Default::default())
        .unwrap();
    for _ in 0..8 {
        game.tick(&[TicCmd::default()]);
    }
    let s1 = game.world().unwrap().actors.get(id).unwrap().scale_x;
    assert!(
        (s1 - 4.0).abs() < 1e-6,
        "force sphere should pulse to 4 after 8 tics, got {s1}"
    );
}

/// `SE 22` teeth retract when the linked lotag-29 door ceiling opens.
#[test]
fn teeth_retract_when_a_lotag_29_door_opens() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in ["E1M1", "E1M2", "E2M1", "E2M6", "E3M1", "E4M1"] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let Some((ti, door)) = game
            .world()
            .unwrap()
            .teeth
            .first()
            .map(|t| (t.verts.first().copied(), t.door))
        else {
            continue;
        };
        let Some(vid) = ti else {
            continue;
        };
        let x0 = game.world().unwrap().map.vertices[vid.raw() as usize]
            .pos
            .x();
        game.world_mut().unwrap().sectors[door.raw() as usize].ceiling_z += 64.0;
        for _ in 0..20 {
            room_sim::tick_teeth(game.world_mut().unwrap());
        }
        let x1 = game.world().unwrap().map.vertices[vid.raw() as usize]
            .pos
            .x();
        assert!(
            (x1 - x0).abs() > 4.0,
            "{map_name} teeth should retract {x0} -> {x1}"
        );
        return;
    }
}

/// `SE 16` reactor floors oscillate (Atomic has none).
#[test]
fn reactor_skew_oscillates_when_present() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in ["E1M1", "E1M2", "E2M1", "E3M1", "E4M1"] {
        let mut game = start(&vfs, map_name);
        game.tick(&[TicCmd::default()]);
        let Some(vid) = game
            .world()
            .unwrap()
            .skews
            .first()
            .and_then(|s| s.verts.first().copied())
        else {
            continue;
        };
        let x0 = game.world().unwrap().map.vertices[vid.raw() as usize]
            .pos
            .x();
        for _ in 0..20 {
            room_sim::tick_skews(game.world_mut().unwrap());
        }
        let x1 = game.world().unwrap().map.vertices[vid.raw() as usize]
            .pos
            .x();
        assert!(
            (x1 - x0).abs() > 4.0,
            "{map_name} reactor should skew {x0} -> {x1}"
        );
        return;
    }
}

/// Unfinished `SE 40`–`45` become a 3D floor when present.
#[test]
fn fof_becomes_a_3d_floor_when_present() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    for map_name in ["E1M1", "E1M2", "E2M1", "E3M1", "E4M1"] {
        let game = start(&vfs, map_name);
        if !game.world().unwrap().map.setup.ffloors.is_empty() {
            return;
        }
    }
}

/// A `FRAMEEFFECT1` copies the nearest monster's sprite and sits on it.
#[test]
fn frame_effect_copies_a_trooper() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let (cls, pos) = {
        let w = game.world().unwrap();
        let cls = w.classes.find_name("DukeFrameEffect").expect("class");
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        (cls, w.actors.get(mo).unwrap().pos)
    };
    let id = game
        .world_mut()
        .unwrap()
        .spawn_at(cls, pos, Default::default())
        .unwrap();
    for _ in 0..2 {
        game.tick(&[TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let fx = w.actors.get(id).unwrap();
    let tid = fx.target.expect("latched a monster");
    let src = w.actors.get(tid).unwrap();
    assert!(
        (src.flags & FLAG_COUNTKILL) != 0 && src.health > 0,
        "afterimage follows a living monster"
    );
    assert_eq!(fx.sprite, src.sprite, "copied sprite");
    assert!((fx.pos.x() - src.pos.x()).abs() < 1.0);
    assert!((fx.pos.y() - src.pos.y()).abs() < 1.0);
}

/// Room `.fsave` restores Duke's health and map time on E1M1.
#[test]
fn e1m1_save_and_load_restores_duke() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    for _ in 0..8 {
        game.tick(&[TicCmd::default()]);
    }
    {
        let w = game.world_mut().unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.health = 42;
        }
        w.level.time = 77;
    }
    let blob = game.save_game().unwrap();
    let info = room_game::read_save_info(&blob).unwrap();
    assert_eq!(info.map, "E1M1");
    {
        let w = game.world_mut().unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.health = 10;
        }
        w.level.time = 1;
    }
    game.load_game(&blob).unwrap();
    let w = game.world().unwrap();
    let mo = w.players[0].as_ref().unwrap().mo.unwrap();
    assert_eq!(w.actors.get(mo).unwrap().health, 42);
    assert_eq!(w.level.time, 77);
}

/// Room `.fdem` records E1M1 and verifies on a fresh world.
#[test]
fn e1m1_demo_records_and_verifies() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let data = load_layered_engine_data(&vfs).unwrap();
    let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        record_demo: true,
        ..Default::default()
    });
    game.set_mapinfo(Arc::new(defs.mapinfo));
    game.set_keyconf(&defs.keyconf);
    game.set_decaldef(&defs.decaldef);
    game.set_sndseq(&defs.sndseq);
    game.set_defs(data.classes, data.states);
    let mut map = load_map(&vfs, "E1M1").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, skill).unwrap();
    for _ in 0..40 {
        game.tick(&[TicCmd::default()]);
    }
    let rec = game.take_demo().expect("recorded");
    assert_eq!(rec.map, "E1M1");
    assert_eq!(rec.tics.len(), 40);
    let blob = rec.to_fdem();
    let rec = DemoRecorder::from_fdem(&blob).unwrap();
    let mut replay = start(&vfs, "E1M1");
    let result = DemoPlayer { demo: &rec }
        .verify(replay.world_mut().unwrap())
        .unwrap();
    assert!(
        result.first_mismatch.is_none(),
        "E1M1 demo desync {result:?}"
    );
    assert_eq!(result.tics_ok, 40);
}

fn dmo_to_cmd(i: room_build::DmoInput) -> TicCmd {
    let mut bits = 0u32;
    if i.bits & 1 != 0 {
        bits |= Buttons::JUMP;
    }
    if i.bits & 4 != 0 {
        bits |= Buttons::ATTACK;
    }
    if i.bits & (1 << 5) != 0 {
        bits |= Buttons::SPEED;
    }
    if i.bits & (1 << 22) != 0 {
        bits |= Buttons::QUICKKICK;
    }
    if i.bits & (1 << 29) != 0 {
        bits |= Buttons::USE;
    }
    if i.bits & (1 << 30) != 0 {
        bits |= Buttons::INVUSE;
    }
    TicCmd {
        forward_move: i.forward.saturating_mul(5),
        side_move: i.strafe.saturating_mul(5),
        yaw: i16::from(i.turn).saturating_mul(256),
        pitch: i16::from(i.horizon).saturating_mul(256),
        buttons: Buttons(bits),
        weapon_select: ((i.bits >> 8) & 0xf) as u8,
        ..TicCmd::default()
    }
}

/// An uncompressed 1.3D `.DMO` walk stream moves Duke on E1M1.
#[test]
fn uncompressed_dmo_walks_e1m1() {
    let Some(bytes) = grp() else {
        return;
    };
    let mut raw = vec![0u8; 24];
    raw[8] = 1;
    let mut tic = [0u8; 10];
    tic[2] = 0x00;
    tic[3] = 0x05;
    raw.extend_from_slice(&tic);
    let dmo = room_build::DmoDemo::parse(&raw).unwrap();
    assert_eq!(dmo.map_name(), "E1M1");
    assert_eq!(dmo.tics.len(), 1);
    let cmd = dmo_to_cmd(dmo.tics[0]);
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let p0 = game.world().unwrap().actors.get(mo).unwrap().pos;
    for _ in 0..8 {
        game.tick(&[cmd]);
    }
    let p1 = game.world().unwrap().actors.get(mo).unwrap().pos;
    assert!(
        (p1.x() - p0.x()).hypot(p1.y() - p0.y()) > 8.0,
        "DMO walk {p0:?} -> {p1:?}"
    );
}

/// Playing a `.DMO` looks through an `SE 27` / `CAMERA1` (`DukeCamera`).
#[test]
fn dmo_playback_looks_through_a_camera() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.world_mut().unwrap().set_demo_cameras(true);
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    let cam = w.players[0]
        .as_ref()
        .and_then(|p| p.camera)
        .expect("DMO camera view");
    assert!(
        !w.players[0].as_ref().unwrap().camera_revert,
        "demo move must keep the camera"
    );
    assert_eq!(
        w.classes
            .get(w.actors.get(cam).unwrap().class)
            .unwrap()
            .name,
        "DukeCamera"
    );
    assert_eq!(w.camera_textures.get("VCAMA0").map(|b| b.camera), Some(cam));
}

/// DukeMatch: no living troopers, Duke sits on a deathmatch start.
#[test]
fn e1m1_dukematch_has_no_monsters() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let data = load_layered_engine_data(&vfs).unwrap();
    let defs = DefinitionsBuilder::new(&vfs).build().unwrap();
    let skill = Skill::new(3);
    let mut game = Headless::new(GameOptions {
        skill,
        deathmatch: true,
        ..Default::default()
    });
    game.set_mapinfo(Arc::new(defs.mapinfo));
    game.set_keyconf(&defs.keyconf);
    game.set_defs(data.classes, data.states);
    let mut map = load_map(&vfs, "E1M1").unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    let dm = map
        .things
        .iter()
        .find(|t| t.type_id == 11)
        .map(|t| (f64::from(t.x), f64::from(t.y)));
    game.start_custom_map(map, skill).unwrap();
    let w = game.world().unwrap();
    assert!(w.deathmatch);
    let live = w
        .thinkers
        .default
        .iter()
        .filter(|&&id| {
            w.actors.get(id).is_some_and(|a| {
                !a.destroyed
                    && a.health > 0
                    && (a.flags & FLAG_COUNTKILL) != 0
                    && w.players.iter().flatten().all(|p| p.mo != Some(id))
            })
        })
        .count();
    assert_eq!(live, 0, "DukeMatch monsters {live}");
    if let Some((x, y)) = dm {
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let p = w.actors.get(mo).unwrap().pos;
        assert!(
            (p.x() - x).abs() < 4.0 && (p.y() - y).abs() < 4.0,
            "DM start ({x},{y}) got {p:?}"
        );
    }
}

/// Holding move-down on the ground crouches Duke (half height / view).
#[test]
fn e1m1_crouch_lowers_duke() {
    let Some(bytes) = grp() else {
        return;
    };
    let vfs = duke_vfs(&bytes);
    let mut game = start(&vfs, "E1M1");
    game.tick(&[TicCmd::default()]);
    let mo = game.world().unwrap().players[0]
        .as_ref()
        .unwrap()
        .mo
        .unwrap();
    let stand = game.world().unwrap().actors.get(mo).unwrap().height;
    let mut cmd = TicCmd::default();
    cmd.up_move = -1;
    for _ in 0..3 {
        game.tick(&[cmd]);
    }
    let w = game.world().unwrap();
    let p = w.players[0].as_ref().unwrap();
    assert!(p.crouching, "holding move-down crouches");
    let crouch = w.actors.get(mo).unwrap().height;
    assert!(crouch < stand * 0.6, "crouch {crouch} from stand {stand}");
    game.tick(&[TicCmd::default()]);
    let w = game.world().unwrap();
    assert!(!w.players[0].as_ref().unwrap().crouching, "release stands");
    assert!(
        (w.actors.get(mo).unwrap().height - stand).abs() < 0.1,
        "stood back to {stand}"
    );
}

//! Solo E1M1: reproduce death after the first (tag-4 computer) door and dump
//! everything about the attacker that a renderer would need to show it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_sim::{
    ActivationType, ActorId, ClassRegistry, FLAG_COUNTKILL, FLAG_DONTDRAW, StateTable, TicCmd,
    World, activate_line, check_sight, floor_at_xy,
};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

fn set_pos(world: &mut World, mo: ActorId, x: f64, y: f64, ang_deg: f64) {
    let z = floor_at_xy(world, x, y).unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = room_core::DVec3::new(x, y, z);
        a.angle = room_core::Angle::from_degrees(ang_deg);
        a.vel = room_core::DVec3::ZERO;
    }
}

#[test]
fn e1m1_after_computer_door_attacker_is_drawable_and_in_sight() {
    let Some(wad) = doom1() else {
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = room_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut world = World::from_map_with_classes(map, classes, states).unwrap();
    let mo = world.players[0].as_ref().unwrap().mo.unwrap();

    // Open the tag-4 computer door (WR line 476 / Doom special 109).
    let _ = activate_line(
        &mut world,
        room_core::LineId::from_raw(476),
        Some(mo),
        ActivationType::Cross,
    );
    for _ in 0..80 {
        world.set_ticcmds(&[Default::default(); 4]);
        world.tick();
    }
    eprintln!(
        "door_sec2 ceil={}",
        world.sectors.get(2).map(|s| s.ceiling_z).unwrap_or(-1.0)
    );

    // Dump every COUNTKILL near the computer-room approach.
    let mut monsters = Vec::new();
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        let flags = a.flags | world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        if (flags & FLAG_COUNTKILL) == 0 {
            continue;
        }
        let name = world
            .classes
            .get(a.class)
            .map(|c| c.name.as_str())
            .unwrap_or("?");
        monsters.push((id, name.to_string(), a.pos.x(), a.pos.y(), a.pos.z()));
    }
    monsters.sort_by(|a, b| {
        let da = (a.2 - 1888.0).hypot(a.3 - (-2560.0));
        let db = (b.2 - 1888.0).hypot(b.3 - (-2560.0));
        da.partial_cmp(&db).unwrap()
    });
    eprintln!("monsters nearest computer room:");
    for (id, name, x, y, z) in monsters.iter().take(12) {
        eprintln!("  {name} {id:?} ({x:.0},{y:.0},{z:.0})");
    }

    // Stand in the open doorway looking into the room (same approach a player walks).
    set_pos(&mut world, mo, 1888.0, -2848.0, 90.0);
    // Stamp soundtarget on nearby sectors by firing a shot so A_Look can wake.
    {
        let mut cmd = TicCmd::default();
        cmd.buttons = room_sim::Buttons(room_sim::Buttons::ATTACK);
        world.set_ticcmds(&[cmd, Default::default(), Default::default(), Default::default()]);
        world.tick();
    }

    let start_hp = world.actors.get(mo).unwrap().health;
    let mut hit_at = None;
    for tic in 0..35 * 30 {
        world.set_ticcmds(&[Default::default(); 4]);
        world.tick();
        let hp = world.actors.get(mo).map(|a| a.health).unwrap_or(0);
        if hp < start_hp {
            hit_at = Some((tic, hp));
            break;
        }
    }

    let p = world.actors.get(mo).unwrap();
    let px = p.pos.x();
    let py = p.pos.y();
    eprintln!(
        "player at ({px:.0},{py:.0}) hp {} door_ceil={}",
        p.health,
        world.sectors.get(2).map(|s| s.ceiling_z).unwrap_or(-1.0)
    );

    let Some((tic, hp)) = hit_at else {
        panic!("expected damage after entering computer door area (pos={px:.0},{py:.0})");
    };
    eprintln!("first damage tic={tic} hp {start_hp}->{hp}");

    let mut patches = room_ui::PatchCache::from_vfs(&vfs);
    let mut attackers = Vec::new();
    let mut missiles = Vec::new();
    let mut any_targeting = Vec::new();

    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || id == mo {
            continue;
        }
        let flags = a.flags | world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        let name = world
            .classes
            .get(a.class)
            .map(|c| c.name.as_str())
            .unwrap_or("?");
        let letter = char::from(b'A' + a.frame.min(25));
        let cands = [
            format!("{}{}1", a.sprite, letter),
            format!("{}{}0", a.sprite, letter),
            format!("{}A1", a.sprite),
        ];
        let drawable =
            !a.sprite.is_empty() && cands.iter().any(|c| patches.ensure(&vfs, c).is_some());
        let dist = ((a.pos.x() - px).powi(2) + (a.pos.y() - py).powi(2)).sqrt();
        let sees = check_sight(&world, mo, id);
        let sees_rev = check_sight(&world, id, mo);
        let dontdraw = (flags & FLAG_DONTDRAW) != 0;

        if a.target == Some(mo) {
            any_targeting.push(format!(
                "{name} COUNTKILL={} sprite={}{letter} dist={dist:.0} drawable={drawable} \
                 dontdraw={dontdraw} sees_p→m={sees} sees_m→p={sees_rev} pos=({:.0},{:.0})",
                (flags & FLAG_COUNTKILL) != 0,
                a.sprite,
                a.pos.x(),
                a.pos.y()
            ));
        }
        if a.target == Some(mo) && (flags & FLAG_COUNTKILL) != 0 {
            attackers.push(format!(
                "{name} id={id:?} sprite={}{letter} frame={} dist={dist:.0} drawable={drawable} \
                 dontdraw={dontdraw} sees={sees}/{sees_rev} pos=({:.0},{:.0},{:.0}) alpha={:.2} style={:?}",
                a.sprite,
                a.frame,
                a.pos.x(),
                a.pos.y(),
                a.pos.z(),
                a.alpha,
                a.render_style
            ));
        }
        if (name.contains("Ball") || name.contains("Missile") || name.contains("Puff"))
            && dist < 512.0
        {
            missiles.push(format!(
                "{name} sprite={}{letter} dist={dist:.0} drawable={drawable} dontdraw={dontdraw} \
                 pos=({:.0},{:.0},{:.0})",
                a.sprite,
                a.pos.x(),
                a.pos.y(),
                a.pos.z()
            ));
        }
    }

    eprintln!("all actors targeting player:");
    for l in &any_targeting {
        eprintln!("  {l}");
    }
    eprintln!("attackers (COUNTKILL):");
    for l in &attackers {
        eprintln!("  {l}");
    }
    eprintln!("nearby missiles:");
    for l in &missiles {
        eprintln!("  {l}");
    }

    if let Some(sid) = room_nodebuild::point_in_sector(&world.map, px, py)
        .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, px, py))
    {
        let s = &world.sectors[sid.raw() as usize];
        eprintln!(
            "player sector {} special={} light={} floor={} ceil={}",
            sid.raw(),
            world.map.sectors[sid.raw() as usize].special,
            s.light,
            s.floor_z,
            s.ceiling_z
        );
    }

    assert!(
        !attackers.is_empty() || !missiles.is_empty(),
        "took damage but no monster targeting player and no nearby missiles — \
         damage source unknown (sector? crush?). targeting={any_targeting:?}"
    );

    if !attackers.is_empty() {
        assert!(
            attackers.iter().all(|l| l.contains("drawable=true")),
            "invisible attacker sprite: {attackers:?}"
        );
        assert!(
            attackers.iter().all(|l| l.contains("dontdraw=false")),
            "DONTDRAW attacker: {attackers:?}"
        );
        assert!(
            attackers.iter().any(|l| l.contains("sees=")),
            "damaged with no attacker in LOS (shoot-through?): {attackers:?}"
        );
        // Monster→player sight must be true when they land a shot.
        assert!(
            attackers
                .iter()
                .any(|l| l.contains("sees=") && l.contains("/true")),
            "damaged with no monster→player LOS (shoot-through?): {attackers:?}"
        );
    }
}

#[test]
fn e1m1_embedded_decorate_zombies_drawable() {
    // Match the app path: room.pk3 DECORATE, not doom_basic.txt.
    let Some(wad) = doom1() else {
        return;
    };
    let Ok(data) = room_game::load_embedded_engine_data() else {
        eprintln!("skip: no embedded engine data");
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let mut world =
        World::from_map_with_classes(map, Arc::clone(&data.classes), Arc::clone(&data.states))
            .unwrap();
    let mo = world.players[0].as_ref().unwrap().mo.unwrap();

    let _ = activate_line(
        &mut world,
        room_core::LineId::from_raw(476),
        Some(mo),
        ActivationType::Cross,
    );
    for _ in 0..80 {
        world.set_ticcmds(&[Default::default(); 4]);
        world.tick();
    }
    set_pos(&mut world, mo, 1888.0, -2848.0, 90.0);
    {
        let mut cmd = TicCmd::default();
        cmd.buttons = room_sim::Buttons(room_sim::Buttons::ATTACK);
        world.set_ticcmds(&[cmd, Default::default(), Default::default(), Default::default()]);
        world.tick();
    }
    let start_hp = world.actors.get(mo).unwrap().health;
    let mut hit = false;
    for _ in 0..35 * 30 {
        world.set_ticcmds(&[Default::default(); 4]);
        world.tick();
        if world.actors.get(mo).map(|a| a.health).unwrap_or(0) < start_hp {
            hit = true;
            break;
        }
    }
    assert!(hit, "expected damage with embedded DECORATE");

    let mut patches = room_ui::PatchCache::from_vfs(&vfs);
    let mut bad = Vec::new();
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.target != Some(mo) {
            continue;
        }
        let flags = a.flags | world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        if (flags & FLAG_COUNTKILL) == 0 {
            continue;
        }
        let name = world
            .classes
            .get(a.class)
            .map(|c| c.name.as_str())
            .unwrap_or("?");
        let letter = char::from(b'A' + a.frame.min(25));
        let cands = [
            format!("{}{}1", a.sprite, letter),
            format!("{}{}0", a.sprite, letter),
            format!("{}A1", a.sprite),
        ];
        let drawable =
            !a.sprite.is_empty() && cands.iter().any(|c| patches.ensure(&vfs, c).is_some());
        let dontdraw = (flags & FLAG_DONTDRAW) != 0;
        eprintln!(
            "embedded attacker {name} sprite={}{letter} drawable={drawable} dontdraw={dontdraw} \
             h={} r={} alpha={} style={:?}",
            a.sprite, a.height, a.radius, a.alpha, a.render_style
        );
        if !drawable || dontdraw || a.height < 8.0 || a.radius < 4.0 || a.alpha < 0.1 {
            bad.push(name.to_string());
        }
    }
    assert!(bad.is_empty(), "bad attackers with embedded DECORATE: {bad:?}");
}

#[test]
fn e1m1_outdoor_zombie_cannot_shoot_through_start_hallway_walls() {
    // Regression: zombies on the outdoor ledge must not snipe the player through
    // solid geometry while the player is still in the start zigzag (feels like an
    // invisible killer "after the first door"/WR trigger).
    let Some(wad) = doom1() else {
        return;
    };
    let Ok(data) = room_game::load_embedded_engine_data() else {
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let mut world =
        World::from_map_with_classes(map, Arc::clone(&data.classes), Arc::clone(&data.states))
            .unwrap();
    let mo = world.players[0].as_ref().unwrap().mo.unwrap();

    // Player just past the WR trigger (line 476), still west of the outdoor ledge.
    set_pos(&mut world, mo, 1500.0, -3456.0, 0.0);

    let zid = world
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            let a = world.actors.get(id).unwrap();
            world
                .classes
                .get(a.class)
                .is_some_and(|c| c.name == "ZombieMan")
                && (a.pos.x() - 2912.0).abs() < 8.0
                && (a.pos.y() - (-2816.0)).abs() < 8.0
        })
        .expect("outdoor zigzag zombie");

    let sees = check_sight(&world, zid, mo);
    eprintln!(
        "outdoor zombie→player sight={sees} (player @1500,-3456, zombie @2912,-2816)"
    );
    assert!(
        !sees,
        "outdoor zombie must not see player through start-hallway walls"
    );

    // Force wake + attack anyway; hitscan must still be blocked.
    if let Some(z) = world.actors.get_mut(zid) {
        z.target = Some(mo);
    }
    let start_hp = world.actors.get(mo).unwrap().health;
    let ang = {
        let z = world.actors.get(zid).unwrap();
        let p = world.actors.get(mo).unwrap();
        let dx = p.pos.x() - z.pos.x();
        let dy = p.pos.y() - z.pos.y();
        room_core::Angle::from_radians(dy.atan2(dx))
    };
    let hit = room_sim::line_attack(&mut world, zid, ang, 4096.0, 15);
    eprintln!(
        "hitscan through walls: actor={:?} dist={:.0} hp {}→{}",
        hit.actor,
        hit.distance,
        start_hp,
        world.actors.get(mo).map(|a| a.health).unwrap_or(0)
    );
    assert_ne!(
        hit.actor,
        Some(mo),
        "hitscan must not hit player through solid walls"
    );
    assert_eq!(
        world.actors.get(mo).map(|a| a.health).unwrap_or(0),
        start_hp,
        "player must not take damage through walls"
    );
}

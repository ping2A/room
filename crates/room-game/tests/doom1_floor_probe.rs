//! Probe: doom1 E1M1 floor snap + sprite draw bottoms + step heights.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_map::{MapGeometry, load_map};
use room_nodebuild::ensure_nodes;
use room_sim::{ClassRegistry, StateTable, World, floor_at_xy};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

fn load_e1m1() -> (
    MapGeometry,
    Arc<ClassRegistry>,
    Arc<StateTable>,
    room_vfs::Vfs,
) {
    let wad = doom1().expect("doom1.wad");
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = room_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    (map, classes, states, vfs)
}

#[test]
fn e1m1_things_sit_on_sector_floor() {
    let Some(_) = doom1() else {
        return;
    };
    let (map, classes, states, _) = load_e1m1();
    let world = World::from_map_with_classes(map, classes, states).unwrap();

    let mut bad = Vec::new();
    let mut checked = 0u32;
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed {
            continue;
        }
        let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        let flags = a.flags | class_flags;
        // SPAWNCEILING = 1<<21, NOGRAVITY = 1<<5 (room-sim class flags).
        const FLAG_NOGRAVITY: u64 = 1 << 5;
        const FLAG_SPAWNCEILING: u64 = 1 << 21;
        if (flags & FLAG_SPAWNCEILING) != 0 {
            continue;
        }
        // Floating hangy / projectile classes.
        if (flags & FLAG_NOGRAVITY) != 0 {
            continue;
        }
        let Some(floor) = floor_at_xy(&world, a.pos.x(), a.pos.y()) else {
            bad.push(format!(
                "{} at ({:.1},{:.1}) no sector z={:.1}",
                world
                    .classes
                    .get(a.class)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?"),
                a.pos.x(),
                a.pos.y(),
                a.pos.z()
            ));
            continue;
        };
        checked += 1;
        let dz = a.pos.z() - floor;
        if dz.abs() > 1.0 {
            bad.push(format!(
                "{} at ({:.1},{:.1}) z={:.1} floor={:.1} dz={:+.1}",
                world
                    .classes
                    .get(a.class)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?"),
                a.pos.x(),
                a.pos.y(),
                a.pos.z(),
                floor,
                dz
            ));
        }
    }
    eprintln!("checked {checked} grounded actors, bad {}", bad.len());
    for b in bad.iter().take(40) {
        eprintln!("  BAD {b}");
    }
    assert!(
        bad.is_empty(),
        "{} actors off their sector floor",
        bad.len()
    );
}

#[test]
fn e1m1_sprite_draw_bottom_near_actor_feet() {
    let Some(_) = doom1() else {
        return;
    };
    let (map, classes, states, vfs) = load_e1m1();
    let world = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
    let mut patches = room_ui::PatchCache::from_vfs(&vfs);
    let mut report = Vec::new();
    let mut sunk = 0u32;
    let mut floated = 0u32;
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        let name = world
            .classes
            .get(a.class)
            .map(|c| c.name.as_str())
            .unwrap_or("?");
        if a.sprite.is_empty() {
            continue;
        }
        let letter = char::from(b'A' + a.frame.min(25));
        let candidates = [
            format!("{}{}0", a.sprite, letter),
            format!("{}{}1", a.sprite, letter),
        ];
        let mut found = None;
        for c in &candidates {
            if let Some(p) = patches.ensure(&vfs, c) {
                found = Some((c.clone(), p.width, p.height, p.left_offset, p.top_offset));
                break;
            }
        }
        let Some((lump, w, h, lo, to)) = found else {
            report.push(format!("{name} MISSING {:?}", candidates));
            continue;
        };
        let top = f64::from(to);
        let ph = f64::from(h);
        let z_bottom = a.pos.z() + top - ph;
        let sink = a.pos.z() - z_bottom; // how far sprite extends below feet
        if sink > 8.0 {
            sunk += 1;
            report.push(format!(
                "SUNK {name} {lump} actor_z={:.1} bottom={:.1} sink={sink:.1} {w}x{h} lo={lo} to={to}",
                a.pos.z(),
                z_bottom
            ));
        } else if sink < -2.0 {
            floated += 1;
            report.push(format!(
                "FLOAT {name} {lump} actor_z={:.1} bottom={:.1} sink={sink:.1} {w}x{h} lo={lo} to={to}",
                a.pos.z(),
                z_bottom
            ));
        }
    }
    eprintln!(
        "sprite bottoms: sunk_gt8={sunk} float_gt2={floated} notes={}",
        report.len()
    );
    for r in report.iter().take(40) {
        eprintln!("  {r}");
    }
}

#[test]
fn e1m1_step_sectors_and_lower_textures() {
    let Some(_) = doom1() else {
        return;
    };
    let (map, _, _, _) = load_e1m1();
    let mut steps = 0u32;
    let mut blank_lower_front = 0u32;
    let mut blank_lower_back = 0u32;
    let mut height_pairs = std::collections::BTreeMap::<(i32, i32), u32>::new();
    for line in &map.lines {
        let (Some(fs), Some(bs)) = (line.front_sector, line.back_sector) else {
            continue;
        };
        let front = &map.sectors[fs.raw() as usize];
        let back = &map.sectors[bs.raw() as usize];
        let df = (front.floor_height - back.floor_height).round() as i32;
        if df == 0 {
            continue;
        }
        steps += 1;
        let lo = front.floor_height.min(back.floor_height).round() as i32;
        let hi = front.floor_height.max(back.floor_height).round() as i32;
        *height_pairs.entry((lo, hi)).or_default() += 1;

        // Lower texture is on the side that faces the lower floor.
        if front.floor_height < back.floor_height {
            let side = line.sides[0].and_then(|id| map.sides.get(id.raw() as usize));
            let blank = side.is_none_or(|s| {
                let t = s.lower.trim();
                t.is_empty() || t == "-"
            });
            if blank {
                blank_lower_front += 1;
            }
        }
        if back.floor_height < front.floor_height {
            let side = line.sides[1].and_then(|id| map.sides.get(id.raw() as usize));
            let blank = side.is_none_or(|s| {
                let t = s.lower.trim();
                t.is_empty() || t == "-"
            });
            if blank {
                blank_lower_back += 1;
            }
        }
    }
    eprintln!("E1M1 step linedefs={steps}");
    eprintln!("blank lower (front-side-when-front-lower)={blank_lower_front}");
    eprintln!("blank lower (back-side-when-back-lower)={blank_lower_back}");
    eprintln!("unique step heights (top 15):");
    let mut pairs: Vec<_> = height_pairs.into_iter().collect();
    pairs.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for ((lo, hi), n) in pairs.into_iter().take(15) {
        eprintln!("  {lo} -> {hi} (delta {}) x{n}", hi - lo);
    }
}

#[test]
fn e1m1_base_floor_matches_sector_height() {
    let Some(_) = doom1() else {
        return;
    };
    let (map, _, _, _) = load_e1m1();
    let mut bad = 0u32;
    for (i, sec) in map.sectors.iter().enumerate() {
        let Some(p) = map.setup.planes.get(i) else {
            bad += 1;
            continue;
        };
        if (p.base_floor - sec.floor_height).abs() > 1e-6 {
            bad += 1;
            eprintln!(
                "sec {i}: base_floor={} floor_height={}",
                p.base_floor, sec.floor_height
            );
        }
        // Flat plane z_at should equal floor_height everywhere for doom1.
        if p.floor.is_slope() {
            eprintln!("sec {i}: unexpected slope");
            bad += 1;
        } else {
            let z = p.floor.z_at(0.0, 0.0);
            if (z - sec.floor_height).abs() > 1e-6 {
                bad += 1;
                eprintln!("sec {i}: plane z={z} != floor {}", sec.floor_height);
            }
        }
    }
    assert_eq!(bad, 0, "{bad} sector plane mismatches");
}

#[test]
fn e1m1_gl_nodes_cover_visible_floors() {
    let Some(_) = doom1() else {
        return;
    };
    let (map, _, _, _) = load_e1m1();
    let gl = map
        .gl_nodes
        .as_ref()
        .expect("apply_nodes should build GL nodes");
    assert!(!gl.subsectors.is_empty());
    let mut rings_ok = 0u32;
    let mut rings_thin = 0u32;
    for (ssi, sub) in gl.subsectors.iter().enumerate() {
        if sub.num_segs < 3 {
            rings_thin += 1;
            continue;
        }
        let start = sub.first_seg as usize;
        let end = start + sub.num_segs as usize;
        let segs = &gl.segs[start..end];
        // Closed: each seg's implied v2 is next v1 (except last → first via minisegs).
        assert!(
            segs.len() >= 3,
            "subsector {ssi} ring too small: {}",
            segs.len()
        );
        rings_ok += 1;
    }
    eprintln!("GL flat rings: ok={rings_ok} thin={rings_thin}");
    assert!(rings_ok > 100, "expected many GL subsector rings for E1M1");
}

#[test]
fn e1m1_all_thing_types_resolve() {
    let Some(_) = doom1() else {
        return;
    };
    let (map, classes, _, _) = load_e1m1();
    let mut missing = std::collections::BTreeMap::<i16, u32>::new();
    let mut ok = 0u32;
    for t in &map.things {
        if t.type_id == 1 {
            ok += 1;
            continue;
        }
        if classes.find_doomednum(i32::from(t.type_id)).is_some() {
            ok += 1;
        } else {
            *missing.entry(t.type_id).or_default() += 1;
        }
    }
    eprintln!("E1M1 things resolved={ok} missing_types={}", missing.len());
    for (id, n) in &missing {
        eprintln!("  missing DoomEd {id} x{n}");
    }
    assert!(
        missing.is_empty(),
        "{} DoomEd types on E1M1 have no actor class",
        missing.len()
    );
}

#[test]
fn zombie_missile_state_has_e_f_frames() {
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = room_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let z = classes.find_name("ZombieMan").expect("ZombieMan");
    let mid = states.find_label(z, "Missile").expect("Missile");
    let mut sid = Some(mid);
    let mut frames = Vec::new();
    for _ in 0..6 {
        let st = states.get(sid.unwrap()).unwrap();
        frames.push((st.sprite.clone(), st.frame, st.tics, st.action.clone()));
        sid = st.next;
        if sid.is_none() {
            break;
        }
        // stop if looped to See
        if frames.len() > 1 && st.sprite == "POSS" && st.frame < 4 {
            break;
        }
    }
    eprintln!("Missile chain: {frames:?}");
    assert_eq!(frames[0].1, 4, "first Missile frame should be E"); // E = 4
    assert!(frames.iter().any(|f| f.1 == 5), "should include F"); // F = 5
}

#[test]
fn e1m1_solo_first_damage_attacker_is_drawable() {
    let Some(_) = doom1() else {
        return;
    };
    let (map, classes, states, vfs) = load_e1m1();
    let mut world =
        World::from_map_with_classes(map, Arc::clone(&classes), Arc::clone(&states)).unwrap();
    let mo = world.players[0].as_ref().unwrap().mo.unwrap();
    // Place player facing a skill-3 ZombieMan near the zig-zag approach.
    let zombie = world.thinkers.default.iter().copied().find(|&id| {
        world
            .classes
            .get(world.actors.get(id).map(|a| a.class).unwrap_or_default())
            .is_some_and(|c| c.name == "ZombieMan")
    });
    let Some(zid) = zombie else {
        eprintln!("no ZombieMan on map");
        return;
    };
    let (zx, zy, zz) = {
        let z = world.actors.get(zid).unwrap();
        (z.pos.x(), z.pos.y(), z.pos.z())
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = room_core::DVec3::new(zx - 128.0, zy, zz);
        a.angle = room_core::Angle::from_degrees(0.0);
    }
    // Point zombie at player and wake.
    if let Some(z) = world.actors.get_mut(zid) {
        z.target = Some(mo);
        z.angle = room_core::Angle::from_degrees(180.0);
    }
    let start_hp = world.actors.get(mo).unwrap().health;

    use room_sim::{FLAG_COUNTKILL, TicCmd};
    let mut first_hit: Option<(u32, f64, f64, i32)> = None;
    for tic in 0..35 * 10 {
        let cmd = TicCmd::default();
        world.set_ticcmds(&[
            cmd,
            Default::default(),
            Default::default(),
            Default::default(),
        ]);
        world.tick();
        let hp = world.actors.get(mo).map(|a| a.health).unwrap_or(0);
        if hp < start_hp {
            let a = world.actors.get(mo).unwrap();
            first_hit = Some((tic, a.pos.x(), a.pos.y(), hp));
            break;
        }
    }
    let Some((tic, px, py, hp)) = first_hit else {
        eprintln!("no damage after waking zombie at ({zx:.0},{zy:.0})");
        return;
    };
    eprintln!("first damage tic={tic} hp {start_hp}->{hp} at ({px:.0},{py:.0})");

    let mut patches = room_ui::PatchCache::from_vfs(&vfs);
    let mut attackers = Vec::new();
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || id == mo || a.target != Some(mo) {
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
        let drawable = cands.iter().any(|c| patches.ensure(&vfs, c).is_some());
        let dist = ((a.pos.x() - px).powi(2) + (a.pos.y() - py).powi(2)).sqrt();
        attackers.push(format!(
            "{name} sprite='{}' frame={} dist={dist:.0} drawable={drawable} dont_draw={}",
            a.sprite,
            a.frame,
            (flags & room_sim::FLAG_DONTDRAW) != 0
        ));
    }
    for line in &attackers {
        eprintln!("  attacker {line}");
    }
    assert!(
        !attackers.is_empty(),
        "took damage but no monster has player as target"
    );
    assert!(
        attackers.iter().all(|l| l.contains("drawable=true")),
        "invisible attacker(s): {attackers:?}"
    );
    assert!(
        attackers.iter().all(|l| l.contains("dont_draw=false")),
        "DONTDRAW attacker(s): {attackers:?}"
    );
}

#[test]
fn e1m1_zombies_have_no_dontdraw_and_nonzero_size() {
    let Some(_) = doom1() else {
        return;
    };
    let (map, classes, states, _) = load_e1m1();
    let world = World::from_map_with_classes(map, classes, states).unwrap();
    let mut n = 0;
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        let Some(c) = world.classes.get(a.class) else {
            continue;
        };
        if c.name != "ZombieMan" && c.name != "ShotgunGuy" {
            continue;
        }
        n += 1;
        let flags = a.flags | c.flags;
        assert_eq!(
            flags & room_sim::FLAG_DONTDRAW,
            0,
            "{} has DONTDRAW flags={flags:#x}",
            c.name
        );
        assert!(a.height > 8.0, "{} height={}", c.name, a.height);
        assert!(a.radius > 4.0, "{} radius={}", c.name, a.radius);
        assert!(!a.sprite.is_empty(), "{} empty sprite", c.name);
    }
    assert!(n > 0, "no zombies/shotgunners spawned");
}

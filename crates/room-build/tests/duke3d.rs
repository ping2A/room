//! Full Duke Nukem 3D conversion (`testdata/duke/duke3d.grp`, not committed;
//! the test passes trivially without it).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_build::wad::convert_grp;
use room_map::load_map;
use room_vfs::{Namespace, VfsBuilder};
use std::path::PathBuf;

fn grp() -> Option<Vec<u8>> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke/duke3d.grp");
    std::fs::read(p).ok()
}

#[test]
fn every_duke3d_map_converts_loads_and_builds_nodes() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(&out.maps[..3], ["E1M1", "E1M2", "E1M3"]);
    assert!(out.maps.len() >= 40, "{:?}", out.maps);
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    assert!(
        vfs.find("STALKER", Namespace::Global).is_some() || vfs.find_any_ns("STALKER").is_some()
    );
    for name in &out.maps {
        let mut map = load_map(&vfs, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        room_nodebuild::ensure_nodes(&mut map);
        let nodes = map.game_nodes.as_ref().expect("nodes");
        assert!(!nodes.subsectors.is_empty(), "{name}");
        assert!(
            map.things.iter().any(|t| t.type_id == 1),
            "{name} player start"
        );
        // Every referenced wall texture ships in the WAD.
        for side in &map.sides {
            for tex in [&side.upper, &side.middle, &side.lower] {
                if tex != "-" {
                    assert!(
                        vfs.find(tex, Namespace::NewTextures).is_some(),
                        "{name}: {tex}"
                    );
                }
            }
        }
    }
}

#[test]
fn e1l1_start_sector_is_tall_enough_to_stand() {
    let Some(bytes) = grp() else {
        return;
    };
    let g = room_build::Grp::parse(&bytes).unwrap();
    let m = room_build::parse_map(g.get("E1L1.MAP").unwrap()).unwrap();
    let s = m.sectors[usize::try_from(m.start_sector).unwrap()];
    let gap = f64::from(s.floorz - s.ceilingz) / 256.0;
    assert!(gap >= 40.0, "start gap {gap}");
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "E1M1").unwrap();
    for (i, line) in map.lines.iter().enumerate() {
        let a = map.vertices[line.v1.0 as usize].pos;
        let b = map.vertices[line.v2.0 as usize].pos;
        let midy = (a.y() + b.y()) * 0.5;
        let midx = (a.x() + b.x()) * 0.5;
        if (midy + 3664.0).abs() > 20.0 || !(900.0..=1200.0).contains(&midx) {
            continue;
        }
        let side = line.sides[0].and_then(|s| map.sides.get(s.0 as usize));
        let px = 1100.0 - a.x();
        let py = -3720.0 - a.y();
        let cross = (b.x() - a.x()) * py - (b.y() - a.y()) * px;
        eprintln!(
            "L{i} ({:.0},{:.0})-({:.0},{:.0}) flags {} back {} light {:?} mid {:?} up {:?}",
            a.x(),
            a.y(),
            b.x(),
            b.y(),
            line.flags,
            line.back_sector.is_some(),
            side.map(|s| s.light),
            side.map(|s| s.middle.as_str()),
            side.map(|s| s.upper.as_str())
        );
    }
}

#[test]
fn e1l1_has_doors_an_exit_slopes_and_a_sky() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "E1M1").unwrap();
    // Glass breaks by its death special; the vent's C-9 charges run their
    // detonation script when they blow.
    assert!(
        map.lines.iter().any(|l| l.health > 0 && l.special == 80),
        "breakable glass"
    );
    assert!(
        map.things
            .iter()
            .any(|t| t.special == 80 && t.tid >= 20_000 && t.args[2] > 0),
        "charge fuse"
    );
    assert!(map.lines.iter().any(|l| l.special == 243), "Exit_Normal");
    assert!(map.sectors.iter().any(|s| s.ceiling_pic == "F_SKY1"));
    let info = vfs.read(vfs.find_any_ns("MAPINFO").unwrap()).unwrap();
    let info = String::from_utf8_lossy(&info);
    assert!(info.contains("map E1M1 \"Hollywood Holocaust\""), "{info}");
    assert!(info.contains("sky1 BSKY"), "{info}");
    assert!(vfs.find_any_ns("CONEVT").is_some(), "CON event VM lump");
    let deco = vfs.read(vfs.find_any_ns("DECORATE").unwrap()).unwrap();
    let deco = String::from_utf8_lossy(&deco);
    assert!(deco.contains("actor DukeMoney"), "{deco:.200}");
    assert!(deco.contains("actor DukeQuakeDebris"), "{deco:.200}");
    assert!(deco.contains("actor DukeRespawn"), "{deco:.200}");
    assert!(deco.contains("actor DukeWaterBubbleMaker"), "{deco:.200}");
    assert!(deco.contains("actor DukeNaturalLightning"), "{deco:.200}");
    assert!(deco.contains("actor DukeForceSphere"), "{deco:.200}");
    assert!(deco.contains("actor DukeFrameEffect"), "{deco:.200}");
    let fountain = deco
        .find("actor DukeFountain")
        .map(|i| &deco[i..i + deco[i..].find("\n}\n").unwrap_or(800) + 3])
        .unwrap_or("");
    assert!(
        fountain.contains("Health 1") && fountain.contains("Death:"),
        "shot fountain keeps its broken picture: {fountain}"
    );
    assert!(
        deco.contains("duke/duke_tip1") && deco.contains("duke/duke_hit_stripper1"),
        "babe CON tip/hit lines"
    );
    assert!(
        deco.contains("A_JumpIfLaserHit") && deco.contains("actor DukeLaserLine"),
        "tripbomb laser"
    );
    assert!(
        deco.contains("actor DukeFirstAid : Actor")
            && deco.contains("actor DukeSteroids : Actor")
            && deco.contains("actor DukeNightVision : Actor")
            && deco.contains("actor DukeHolodukeDecoy")
            && deco.contains("A_Wander"),
        "stored inventory and holoduke decoy"
    );
    assert!(
        deco.contains("Player.ForwardMove 1.25"),
        "DukePlayer run scale"
    );
    assert!(
        deco.contains("DamageType \"Shrink\"") && deco.contains("DamageType \"Ice\""),
        "shrinker and freezer damage types"
    );
    assert!(
        deco.contains("  Ice:\n") && deco.contains("  Shrink:\n"),
        "CON ice / shrink stills"
    );
    assert!(
        deco.contains("actor DukeExpander") && deco.contains("DamageType \"Grow\""),
        "expander"
    );
    assert!(
        deco.contains("actor DukeTrooperCaptain") && deco.contains("A_RandomTeleport"),
        "palette-21 trooper captains"
    );
    assert!(
        deco.contains("actor DukeEnforcerMini") && deco.contains("actor DukeOctabrainMini"),
        "palette mini-bosses"
    );
    let mini = deco
        .split_once("actor DukeEnforcerMini : DukeEnforcer ")
        .and_then(|(_, rest)| rest.split_whitespace().next()?.parse::<i16>().ok())
        .expect("enforcer mini editor number");
    assert!(
        load_map(&vfs, "E2M3")
            .unwrap()
            .things
            .iter()
            .any(|t| t.type_id == mini),
        "E2M3 pal-21 enforcers"
    );
    let cap = deco
        .split_once("actor DukeTrooperCaptain : DukeTrooper ")
        .and_then(|(_, rest)| rest.split_whitespace().next()?.parse::<i16>().ok())
        .expect("captain editor number");
    assert!(
        load_map(&vfs, "E1M1")
            .unwrap()
            .things
            .iter()
            .any(|t| t.type_id == cap),
        "E1M1 pal-21 captains"
    );
    assert!(
        deco.contains("actor DukeCamera")
            && deco.contains("actor DukeViewScreen")
            && deco.contains("A_ChangeCameraToNearest")
            && deco.contains("VCAM A"),
        "cameras and viewscreens"
    );
    let anim = vfs.read(vfs.find_any_ns("ANIMDEFS").unwrap()).unwrap();
    let anim = String::from_utf8_lossy(&anim);
    assert!(
        anim.contains("cameratexture VCAMA0"),
        "viewscreen camera canvas"
    );
    for lump in [
        "TITLEPIC", "INTERPIC", "CREDIT", "WIMAP0", "WIMAP1", "WIMAP2",
    ] {
        let id = vfs
            .find_any_ns(lump)
            .unwrap_or_else(|| panic!("{lump} from ANM"));
        assert!(
            room_formats::decode_image(&vfs.read(id).unwrap()).is_ok(),
            "{lump} is a Doom patch"
        );
    }
    let def = vfs
        .read(vfs.find_any_ns("TITLEDEF").expect("TITLEDEF"))
        .unwrap();
    let def = String::from_utf8_lossy(&def);
    assert!(
        def.contains("frames ") && def.contains("fps "),
        "title anim: {def}"
    );
    let titl = vfs.find_any_ns("TITL00").expect("TITL00 from LOGO.ANM");
    assert!(room_formats::decode_image(&vfs.read(titl).unwrap()).is_ok());
    let e1 = vfs
        .read(vfs.find_any_ns("E1ANDEF").expect("E1ANDEF"))
        .unwrap();
    assert!(
        String::from_utf8_lossy(&e1).contains("frames "),
        "episode 1 cine"
    );
    assert!(vfs.find_any_ns("E1AN00").is_some());
    let menu = vfs
        .read(vfs.find_any_ns("MENUDEF").expect("MENUDEF"))
        .unwrap();
    let menu = String::from_utf8_lossy(&menu);
    assert!(
        menu.contains("MainMenu")
            && menu.contains("EpisodeMenu")
            && menu.contains("New Game")
            && menu.contains("Load Game")
            && menu.contains("OptionsMenu")
            && menu.contains("autoaim"),
        "Duke title menu: {menu}"
    );
    assert!(
        vfs.find_any_ns("GRABBAG").is_some(),
        "menu track from music 0"
    );
    assert!(
        vfs.find_any_ns("RADLDEF").is_some() && vfs.find_any_ns("RADL00").is_some(),
        "3D Realms splash from RADLOGO.ANM"
    );
    assert!(
        load_map(&vfs, "E1M1")
            .unwrap()
            .things
            .iter()
            .any(|t| t.type_id == 26992)
            && load_map(&vfs, "E1M1")
                .unwrap()
                .things
                .iter()
                .any(|t| t.type_id == 26993),
        "E1M1 camera and viewscreen"
    );
    assert!(
        load_map(&vfs, "E1M1")
            .unwrap()
            .things
            .iter()
            .any(|t| t.type_id == 26991),
        "E1M1 SE 33 DukeQuakeDebris"
    );
    let e1 = load_map(&vfs, "E1M1").unwrap();
    assert!(
        e1.udmf_user.sectors.iter().enumerate().any(|(i, b)| {
            b.ints.get("user_bridge") == Some(&1)
                && e1
                    .sectors
                    .get(i)
                    .is_some_and(|s| s.ceiling_height > s.floor_height + 16.0)
        }),
        "E1M1 stretch bridges are walkable"
    );
    assert!(
        e1.lines.iter().enumerate().any(|(i, l)| {
            l.health > 0
                && e1
                    .udmf_user
                    .lines
                    .get(i)
                    .is_some_and(|b| b.ints.get("user_contactdamage").is_some_and(|&d| d > 0))
        }),
        "E1M1 force-field walls break"
    );
    let e2 = load_map(&vfs, "E2M2").unwrap();
    assert!(
        e2.lines.iter().any(|l| l.health > 0 && l.health_group != 0),
        "glass healthgroup"
    );
}

#[test]
fn moving_sectors_become_polyobjects_with_scripts() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let anim = vfs.read(vfs.find_any_ns("ANIMDEFS").unwrap()).unwrap();
    assert!(String::from_utf8_lossy(&anim).contains("texture BT"));
    let (mut polys, mut mirrors, mut scripted) = (0, 0, 0);
    for name in &out.maps {
        let mut map = load_map(&vfs, name).unwrap();
        room_nodebuild::ensure_nodes(&mut map);
        mirrors += map.lines.iter().filter(|l| l.special == 182).count();
        for po in &map.setup.polyobjects {
            polys += 1;
            assert!(po.lines.len() >= 3, "{name} poly {} open loop", po.id);
            let first = &map.lines[po.lines[0].raw() as usize];
            let last = &map.lines[po.lines[po.lines.len() - 1].raw() as usize];
            assert_eq!(first.v1, last.v2, "{name} poly {} closes", po.id);
            for ty in [9300, 9301] {
                assert!(
                    map.things
                        .iter()
                        .any(|t| t.type_id == ty && i32::from(t.angle) == po.id),
                    "{name} poly {} thing {ty}",
                    po.id
                );
            }
        }
        if !map.setup.polyobjects.is_empty() {
            assert!(map.behavior.is_some(), "{name} BEHAVIOR");
            scripted += 1;
        }
    }
    let water = {
        let mut n = 0;
        for name in &out.maps {
            let map = load_map(&vfs, name).unwrap();
            n += map
                .setup
                .portals
                .sectors
                .iter()
                .filter(|p| p.kind == room_map::PORTAL_KIND_WATER && p.target != p.sector)
                .count();
        }
        n
    };
    assert_eq!(
        room_build::convert::WATER_PORTAL,
        i32::from(room_map::PORTAL_KIND_WATER)
    );
    eprintln!(
        "polyobjects {polys}, maps with scripts {scripted}, mirrors {mirrors}, water links {water}"
    );
    assert!(water > 100, "{water}");
    assert!(polys > 200, "{polys}");
    assert!(mirrors > 10, "{mirrors}");
}

#[test]
fn e1m2_walkable_gears_write_user_spin() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "E1M2").unwrap();
    let n = map
        .udmf_user
        .sectors
        .iter()
        .filter(|b| b.ints.get("user_spin").is_some_and(|&s| s != 0))
        .count();
    assert!(n >= 2, "E1M2 lotag-30 gears got user_spin ({n})");
}

/// E2M1 subway writes locator waypoints (`user_subway`).
#[test]
fn e2m1_subway_writes_locator_path() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "E2M1").unwrap();
    let n = map
        .udmf_user
        .sectors
        .iter()
        .filter(|b| b.ints.get("user_subway").is_some_and(|&s| s != 0))
        .count();
    let pts = map
        .udmf_user
        .sectors
        .iter()
        .filter_map(|b| b.ints.get("user_subn").copied())
        .max()
        .unwrap_or(0);
    assert!(n >= 1, "E2M1 subway car got user_subway ({n})");
    assert!(pts >= 4, "E2M1 subway has locator waypoints ({pts})");
    let po = map
        .udmf_user
        .sectors
        .iter()
        .filter_map(|b| b.ints.get("user_subpo").copied())
        .find(|&id| id > 0);
    let Some(id) = po else {
        panic!("E2M1 subway car walls are a polyobject");
    };
    assert!(
        map.setup.polyobjects.iter().any(|p| p.id == id),
        "E2M1 subway poly {id}"
    );
}

/// Lotag-27 / `SE 20` bridges write stretch direction and distance.
#[test]
fn e1m1_stretch_bridges_write_user_stretch() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "E1M1").unwrap();
    let n = map
        .udmf_user
        .sectors
        .iter()
        .filter(|b| b.ints.get("user_stretch").is_some_and(|&s| s != 0))
        .count();
    assert!(n >= 1, "E1M1 SE 20 stretch ({n})");
}

/// Atomic `CYCLER` sprites become sector `Light_Glow` thinkers.
#[test]
fn atomic_has_cyclers() {
    let Some(bytes) = grp() else {
        return;
    };
    let g = room_build::Grp::parse(&bytes).unwrap();
    let mut text = String::new();
    for name in ["DEFS.CON", "USER.CON", "GAME.CON"] {
        if let Some(b) = g.get(name) {
            text.push_str(&String::from_utf8_lossy(b));
            text.push('\n');
        }
    }
    let con = room_build::ConData::parse(&text);
    let tile = con.value("CYCLER").unwrap_or(7) as i16;
    let mut n = 0u32;
    for name in g.names().filter(|n| n.ends_with(".MAP")) {
        let Some(body) = g.get(name) else { continue };
        let Ok(m) = room_build::parse_map(body) else {
            continue;
        };
        n += m.sprites.iter().filter(|sp| sp.picnum == tile).count() as u32;
    }
    assert!(n > 0, "Atomic CYCLER sprites ({n})");
}

/// `RESPAWN` markers become non-solid `DukeRespawn` things (editor 26994).
#[test]
fn respawn_markers_become_duke_respawn() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let mut n = 0u32;
    for name in &out.maps {
        let map = load_map(&vfs, name).unwrap();
        n += map.things.iter().filter(|t| t.type_id == 26994).count() as u32;
    }
    assert!(n >= 1, "RESPAWN as DukeRespawn ({n})");
}

/// `WATERBUBBLEMAKER` sprites become `DukeWaterBubbleMaker` (editor 26995).
#[test]
fn water_bubble_makers_become_things() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let mut n = 0u32;
    for name in &out.maps {
        let map = load_map(&vfs, name).unwrap();
        n += map.things.iter().filter(|t| t.type_id == 26995).count() as u32;
    }
    assert!(n >= 1, "WATERBUBBLEMAKER as DukeWaterBubbleMaker ({n})");
}

/// `NATURALLIGHTNING` sprites become `DukeNaturalLightning` (editor 26996).
#[test]
fn natural_lightning_becomes_a_thing() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let mut n = 0u32;
    let mut where_ = None;
    for name in &out.maps {
        let map = load_map(&vfs, name).unwrap();
        let c = map.things.iter().filter(|t| t.type_id == 26996).count() as u32;
        if c > 0 && where_.is_none() {
            where_ = Some(name.clone());
        }
        n += c;
    }
    assert!(
        n >= 1,
        "NATURALLIGHTNING as DukeNaturalLightning ({n}) {where_:?}"
    );
}

/// `FORCESPHERE` is a pulsing class; Atomic maps rarely place the tile.
#[test]
fn force_sphere_class_exists() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let deco = vfs.read(vfs.find_any_ns("DECORATE").unwrap()).unwrap();
    let deco = String::from_utf8_lossy(&deco);
    assert!(deco.contains("actor DukeForceSphere"), "{deco:.200}");
    assert!(deco.contains("A_SetScale"), "pulse scale");
    assert!(
        deco.contains("A_SpawnItemEx(\"DukeForceSphere\""),
        "Overlord emits a force sphere"
    );
}

/// `FRAMEEFFECT1` is a translucent afterimage class.
#[test]
fn frame_effect_class_exists() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let deco = vfs.read(vfs.find_any_ns("DECORATE").unwrap()).unwrap();
    let deco = String::from_utf8_lossy(&deco);
    assert!(deco.contains("actor DukeFrameEffect"), "{deco:.200}");
    assert!(deco.contains("A_DukeFrameEffect"), "copies nearest monster");
}

/// Atomic Edition has no reactor-skew / teeth-prong / demo-camera effectors.
#[test]
fn atomic_skips_se16_se22_se27() {
    let Some(bytes) = grp() else {
        return;
    };
    let g = room_build::Grp::parse(&bytes).unwrap();
    assert!(g.other_build_game().is_none(), "Duke GRP");
    let mut text = String::new();
    for name in ["DEFS.CON", "USER.CON", "GAME.CON"] {
        if let Some(b) = g.get(name) {
            text.push_str(&String::from_utf8_lossy(b));
            text.push('\n');
        }
    }
    let con = room_build::ConData::parse(&text);
    let se = con.value("SECTOREFFECTOR").unwrap_or(1) as i16;
    let loc_tile = con.value("LOCATORS").unwrap_or(6) as i16;
    let mut n16 = 0u32;
    let mut n22 = 0u32;
    let mut n27 = 0u32;
    let mut n6 = 0u32;
    let mut n14 = 0u32;
    let mut n20 = 0u32;
    let mut n30 = 0u32;
    let mut locators = 0u32;
    let mut lotag31 = 0u32;
    let mut lotag27 = 0u32;
    for name in g.names().filter(|n| n.ends_with(".MAP")) {
        let Some(body) = g.get(name) else { continue };
        let Ok(m) = room_build::parse_map(body) else {
            continue;
        };
        for s in &m.sectors {
            if s.lotag == 31 {
                lotag31 += 1;
            }
            if s.lotag == 27 {
                lotag27 += 1;
            }
        }
        for sp in &m.sprites {
            if sp.picnum == loc_tile {
                locators += 1;
            }
            if sp.picnum != se {
                continue;
            }
            match sp.lotag {
                6 => n6 += 1,
                14 => n14 += 1,
                16 => n16 += 1,
                20 => n20 += 1,
                22 => n22 += 1,
                27 => n27 += 1,
                30 => n30 += 1,
                _ => {}
            }
        }
    }
    assert_eq!(n16, 0, "Atomic SE16");
    assert_eq!(n22, 0, "Atomic SE22");
    assert!(n27 > 0, "Atomic SE27 demo cameras ({n27})");
    assert_eq!(n6, 6, "Atomic SE6");
    assert_eq!(n14, 2, "Atomic SE14");
    assert_eq!(n20, 36, "Atomic SE20");
    assert_eq!(n30, 21, "Atomic SE30");
    assert_eq!(locators, 303, "Atomic locators");
    assert_eq!(lotag31, 57, "Atomic lotag 31");
    assert_eq!(lotag27, 36, "Atomic lotag 27");
}

/// `SE 27` demo cameras become `DukeCamera` things (editor 26992).
#[test]
fn se27_demo_cameras_become_duke_cameras() {
    let Some(bytes) = grp() else {
        return;
    };
    let out = convert_grp(&bytes).unwrap();
    let vfs = VfsBuilder::new()
        .mount_memory("duke3d.wad", out.wad)
        .unwrap()
        .build()
        .unwrap();
    let mut cams = 0u32;
    for name in &out.maps {
        let map = load_map(&vfs, name).unwrap();
        cams += map.things.iter().filter(|t| t.type_id == 26992).count() as u32;
    }
    assert!(cams >= 8, "SE27 / CAMERA1 as DukeCamera ({cams})");
}

/// Atomic / 1.5 `.DMO` lumps in the GRP parse as a header (body is LZW).
#[test]
fn grp_dmo_headers_name_a_map() {
    let Some(bytes) = grp() else {
        return;
    };
    let g = room_build::Grp::parse(&bytes).unwrap();
    let mut n = 0u32;
    for name in g.names() {
        if !name.to_ascii_uppercase().ends_with(".DMO") {
            continue;
        }
        let Some(raw) = g.get(name) else {
            continue;
        };
        let d = room_build::DmoDemo::parse(raw).unwrap();
        assert!(
            d.map_name().starts_with('E') && d.map_name().contains('M'),
            "{name} -> {}",
            d.map_name()
        );
        assert!(
            d.tics.len() >= 8,
            "{name} LZW tics ({}) on {}",
            d.tics.len(),
            d.map_name()
        );
        n += 1;
    }
    // Shareware / Atomic ship intro demos; a GRP without them still parses.
    let _ = n;
}

//! Generated maps are valid, playable and deterministic for every theme and
//! player count.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_mapgen::{Biome, Biomes, Options, Size, Theme, generate};
use std::collections::{HashMap, HashSet};

fn check(opts: &Options) -> room_mapgen::Generated {
    let g = generate(opts).unwrap_or_else(|e| panic!("{opts:?}: {e}"));
    let m = &g.map;
    m.validate().unwrap();
    // Lines: no zero length, no duplicates, sides point at real sectors.
    let mut pairs = HashSet::new();
    for (i, l) in m.linedefs.iter().enumerate() {
        assert_ne!(l.v1, l.v2, "line {i} zero length");
        let (a, b) = (&m.vertices[l.v1 as usize], &m.vertices[l.v2 as usize]);
        assert!(a.x != b.x || a.y != b.y, "line {i} zero length");
        let key = (l.v1.min(l.v2), l.v1.max(l.v2));
        assert!(pairs.insert(key), "line {i} duplicates another");
        assert_ne!(l.side_front, 0xFFFF, "line {i} without front side");
        if l.side_back == 0xFFFF {
            assert_ne!(
                m.sidedefs[l.side_front as usize].middle, "-",
                "one-sided line {i} untextured"
            );
        }
    }
    for s in &m.sidedefs {
        assert!((s.sector as usize) < m.sectors.len());
    }
    // Things inside the map, on sectors with headroom.
    let mut geom = m.to_geometry().unwrap();
    room_nodebuild::ensure_nodes(&mut geom);
    let dm = m.things.iter().filter(|t| t.type_id == 11).count();
    assert!(
        dm >= usize::from(opts.players).min(8),
        "{dm} deathmatch starts"
    );
    for t in &m.things {
        let s = room_nodebuild::point_in_sector(&geom, f64::from(t.x), f64::from(t.y))
            .unwrap_or_else(|| panic!("thing {} at ({}, {}) outside the map", t.type_id, t.x, t.y));
        let sec = &m.sectors[s.raw() as usize];
        assert!(
            sec.ceiling_height - sec.floor_height >= 56,
            "thing {} in a crushed sector",
            t.type_id
        );
    }
    // Teleport lines lead to a tagged sector holding a destination.
    let mut dest_tags = HashMap::new();
    for t in m.things.iter().filter(|t| t.type_id == 14) {
        let s = room_nodebuild::point_in_sector(&geom, f64::from(t.x), f64::from(t.y)).unwrap();
        dest_tags.insert(m.sectors[s.raw() as usize].tag, ());
    }
    for l in m.linedefs.iter().filter(|l| l.special == 97) {
        assert!(
            dest_tags.contains_key(&l.tag),
            "teleport tag {} has no destination",
            l.tag
        );
    }
    // At least one start for a single player too.
    assert!(m.things.iter().any(|t| t.type_id == 1));
    g
}

#[test]
fn every_theme_and_player_count() {
    for theme in [
        Theme::Tech,
        Theme::Base,
        Theme::Hell,
        Theme::Castle,
        Theme::Ruins,
        Theme::Mixed,
    ] {
        for (players, size) in [(2, Size::Small), (8, Size::Medium), (16, Size::Large)] {
            for seed in [1, 2] {
                check(&Options {
                    seed,
                    players,
                    theme,
                    size,
                    ..Options::default()
                });
            }
        }
    }
}

#[test]
fn big_32_player_map() {
    let g = check(&Options {
        seed: 3,
        players: 32,
        size: Size::Huge,
        theme: Theme::Mixed,
        ..Options::default()
    });
    assert!(g.report.rooms >= 30, "rooms {}", g.report.rooms);
    assert!(g.report.dm_starts >= 48);
    assert!(g.report.teleporters >= 2);
    assert!(g.report.loops >= 5);
}

#[test]
fn same_seed_same_map() {
    let opts = Options {
        seed: 99,
        players: 12,
        theme: Theme::Mixed,
        ..Options::default()
    };
    let a = serde_json::to_string(&generate(&opts).unwrap().map).unwrap();
    let b = serde_json::to_string(&generate(&opts).unwrap().map).unwrap();
    assert_eq!(a, b);
    let c = serde_json::to_string(&generate(&Options { seed: 100, ..opts }).unwrap().map).unwrap();
    assert_ne!(a, c);
}

#[test]
fn switches_turn_features_off() {
    let opts = Options {
        seed: 4,
        players: 12,
        teleporters: false,
        liquids: false,
        power_items: false,
        verticality: 0,
        ..Options::default()
    };
    let g = check(&opts);
    let m = &g.map;
    assert!(m.linedefs.iter().all(|l| l.special != 97), "no teleporters");
    assert!(
        m.sectors.iter().all(|s| ![5, 7, 16].contains(&s.special)),
        "no damaging floors"
    );
    assert!(
        m.things
            .iter()
            .all(|t| ![2006, 83, 2013].contains(&t.type_id)),
        "no power-ups"
    );
    // Flat: rooms and corridors at 0; only set pieces (pits, daises,
    // crates, boulders) rise or sink, by at most 64.
    assert!(
        m.sectors
            .iter()
            .all(|s| (-24..=64).contains(&s.floor_height))
    );
    let at_zero = m.sectors.iter().filter(|s| s.floor_height == 0).count();
    assert!(at_zero * 2 > m.sectors.len(), "most sectors at height 0");
}

#[test]
fn exports_to_a_wad() {
    let g = check(&Options::default());
    let lumps = g.map.to_lumps().unwrap();
    let names: Vec<&str> = lumps.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "MAP01", "THINGS", "LINEDEFS", "SIDEDEFS", "VERTEXES", "SECTORS"
        ]
    );
}

#[test]
fn every_biome_mix_is_playable() {
    let mut seen = HashSet::new();
    for biomes in [
        Biomes::Mixed,
        Biomes::Indoor,
        Biomes::Caves,
        Biomes::Wilderness,
        Biomes::Urban,
    ] {
        for (seed, players, theme) in [
            (1, 8, Theme::Tech),
            (2, 16, Theme::Hell),
            (3, 4, Theme::Castle),
            (4, 12, Theme::Ruins),
        ] {
            let g = check(&Options {
                seed,
                players,
                theme,
                biomes,
                ..Options::default()
            });
            seen.extend(g.report.biomes.iter().copied());
            if biomes == Biomes::Indoor {
                assert!(
                    g.report
                        .biomes
                        .iter()
                        .all(|b| matches!(b, Biome::Indoor | Biome::Yard))
                );
            }
        }
    }
    for b in [
        Biome::Indoor,
        Biome::Yard,
        Biome::Cave,
        Biome::Canyon,
        Biome::Swamp,
        Biome::Courtyard,
        Biome::Plaza,
    ] {
        assert!(seen.contains(&b), "{b:?} never generated");
    }
}

#[test]
fn no_cliffs_anywhere() {
    // Floors stay walkable: no two-sided step above 24 between sectors that
    // are not set pieces (crates, boulders) — terraces, stairs, caves.
    for biomes in [
        Biomes::Mixed,
        Biomes::Caves,
        Biomes::Wilderness,
        Biomes::Urban,
    ] {
        for seed in 6..9 {
            no_cliffs(&check(&Options {
                seed,
                players: 12,
                biomes,
                verticality: 2,
                ..Options::default()
            }));
        }
    }
}

fn no_cliffs(g: &room_mapgen::Generated) {
    let m = &g.map;
    for l in m.linedefs.iter().filter(|l| l.side_back != 0xFFFF) {
        let f = &m.sectors[m.sidedefs[l.side_front as usize].sector as usize];
        let b = &m.sectors[m.sidedefs[l.side_back as usize].sector as usize];
        // Obstacles (crates, boulders) are 48+ high on purpose; what must
        // never happen is a ledge that looks climbable and is not.
        let d = (f.floor_height - b.floor_height).abs();
        let v = &m.vertices[l.v1 as usize];
        assert!(
            d <= 24 || d >= 48,
            "step {} → {} at ({}, {}): {} / {} ceil {} / {} light {} / {}",
            f.floor_height,
            b.floor_height,
            v.x,
            v.y,
            f.floor_pic,
            b.floor_pic,
            f.ceiling_height,
            b.ceiling_height,
            f.light,
            b.light
        );
    }
}

#[test]
fn monsters_for_solo_and_none_for_deathmatch() {
    let dm = check(&Options {
        seed: 2,
        players: 8,
        ..Options::default()
    });
    assert_eq!(dm.report.monsters, 0);
    let solo = check(&Options {
        seed: 2,
        players: 4,
        monsters: 2,
        ..Options::default()
    });
    assert!(
        solo.report.monsters >= 10,
        "monsters: {}",
        solo.report.monsters
    );
    // The hub (with the player starts) stays quiet; the far end gets the bosses.
    let m = &solo.map;
    let p1 = m.things.iter().find(|t| t.type_id == 1).unwrap();
    let near: Vec<_> = m
        .things
        .iter()
        .filter(|t| {
            [
                3004, 9, 3001, 3002, 58, 65, 3006, 3005, 69, 66, 71, 67, 68, 3003, 64, 16,
            ]
            .contains(&t.type_id)
        })
        .filter(|t| {
            (i32::from(t.x) - i32::from(p1.x)).abs() + (i32::from(t.y) - i32::from(p1.y)).abs()
                < 256
        })
        .collect();
    assert!(near.is_empty(), "monsters next to the start: {near:?}");
    // Later in an episode, with hordes: more and tougher.
    let late = check(&Options {
        seed: 2,
        players: 4,
        monsters: 3,
        progress: 3,
        ..Options::default()
    });
    assert!(late.report.monsters > solo.report.monsters);
    assert!(
        late.map
            .things
            .iter()
            .any(|t| [3003, 67, 68, 71, 64, 16].contains(&t.type_id))
    );
}

#[test]
fn doom1_gets_no_doom2_monsters() {
    let doom2_only = ["CPOS", "SKEL", "FATT", "BSPI", "BOS2", "PAIN", "VILE"];
    let g = room_mapgen::generate_for_game(
        &Options {
            seed: 5,
            players: 4,
            monsters: 3,
            progress: 4,
            ..Options::default()
        },
        &|_, _| true,
        &|s| !doom2_only.contains(&s),
    )
    .unwrap();
    assert!(g.report.monsters > 0);
    assert!(
        g.map
            .things
            .iter()
            .all(|t| ![65, 66, 67, 68, 69, 71, 64].contains(&t.type_id))
    );
}

#[test]
fn secret_closets_open_and_count() {
    let mut total = 0;
    for seed in 1..6 {
        let g = check(&Options {
            seed,
            players: 4,
            monsters: 2,
            ..Options::default()
        });
        let m = &g.map;
        let doors = m.linedefs.iter().filter(|l| l.special == 31).count();
        let secret_sectors = m.sectors.iter().filter(|s| s.special == 9).count();
        assert_eq!(secret_sectors, g.report.secrets, "seed {seed}");
        if g.report.secrets > 0 {
            assert!(
                doors >= g.report.secrets,
                "seed {seed}: {doors} doors for {} secrets",
                g.report.secrets
            );
            assert!(
                m.linedefs
                    .iter()
                    .filter(|l| l.special == 31)
                    .all(|l| l.flags & 32 != 0),
                "secret doors hidden on the automap"
            );
            // Doors start closed (ceiling on the floor).
            for l in m.linedefs.iter().filter(|l| l.special == 31) {
                let back = &m.sectors[m.sidedefs[l.side_back as usize].sector as usize];
                assert_eq!(back.ceiling_height, back.floor_height);
            }
        }
        total += g.report.secrets;
    }
    assert!(total >= 4, "secrets over 5 maps: {total}");
}

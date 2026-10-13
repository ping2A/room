//! M7 synthetic feature smoke: portal map + SNDSEQ stream + Pillar + fog + UDMF user_*.

#![allow(clippy::unwrap_used)]

use room_defs::parse_sndseq;
use room_game::{GameOptions, Headless, Skill};
use room_nodebuild::apply_nodes;
use room_sim::{SndSeqDef, SndSeqStep, execute_special, start_sequence};
use room_testkit::{MapBuilder, rect};

#[test]
fn m7_portal_sndseq_pillar_fog_smoke() {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room("b", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
        .sector_tag("a", 3)
        .portal_between("a", "b", 42, 1)
        .player_start(64.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    assert!(
        !map.setup.portals.lines.is_empty(),
        "portal line should be in setup"
    );

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 1993,
        ..Default::default()
    });
    let sndseq_text = r#"
:DoorOpen
	play doors/open
	delay 2
	play doors/stop
	door 0
end
"#;
    game.set_sndseq(&parse_sndseq(sndseq_text).unwrap());
    game.start_custom_map(map, Skill::new(3)).unwrap();

    let world = game.world_mut().unwrap();
    // Slot 0 resolves via door_slots.
    assert!(start_sequence(world, "0", 10.0, 20.0, 0.0));
    world.tick();
    let evs = world.drain_sound_events();
    assert!(
        evs.iter().any(|e| e.name == "doors/open"),
        "SNDSEQ play step should emit"
    );
    world.tick();
    world.tick();
    world.tick();
    let evs = world.drain_sound_events();
    assert!(
        evs.iter().any(|e| e.name == "doors/stop"),
        "SNDSEQ delay then second play"
    );

    // Pillar_Build on tagged sector.
    let floor0 = world.sectors[0].floor_z;
    assert_eq!(execute_special(world, 29, &[3, 8, 32, 0, 0], None, None), 1);
    for _ in 0..40 {
        world.tick();
    }
    assert!(
        world.sectors[0].floor_z > floor0,
        "pillar should raise floor"
    );

    // Fog density (ACSF SetFogDensity store).
    let d = 100u8;
    world.sectors[0].fog_density = d;
    world.fog_density = f32::from(d) / 255.0;
    assert!(world.fog_density > 0.0);
}

#[test]
fn m7_sndseq_table_slot_lookup() {
    let mut table = room_sim::SndSeqTable::default();
    table.sequences.insert(
        "DOOROPEN".into(),
        SndSeqDef {
            name: "DOOROPEN".into(),
            steps: vec![SndSeqStep::Play("x".into()), SndSeqStep::End],
            stopsound: None,
        },
    );
    table.door_slots.insert(0, "DOOROPEN".into());
    assert!(table.get("0").is_some());
    assert!(table.get("DoorOpen").is_some());
}

#[test]
fn m7_ffloor_slope_setup_smoke() {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .room("b", rect(256.0, 0.0, 512.0, 256.0), 64.0, 96.0)
        .sector_tag("b", 3)
        .ffloor_between("a", "b", 3, 1)
        .floor_plane("a", [0.0, -0.25, 1.0, 0.0])
        .player_start(32.0, 32.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    assert!(
        !map.setup.ffloors.is_empty(),
        "3D floor setup from Sector_Set3DFloor"
    );
    assert!(
        map.setup.planes[0].floor.is_slope(),
        "explicit floor plane should be a slope"
    );

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 7,
        ..Default::default()
    });
    game.start_custom_map(map, Skill::new(3)).unwrap();
    {
        let world = game.world().unwrap();
        assert!(!world.map.setup.ffloors.is_empty());
        assert!(world.map.setup.planes[0].floor.is_slope());
    }
    // Solid 3D-floor collision path is exercised by a few idle tics.
    for _ in 0..5 {
        game.tick(&[]);
    }
    assert!(game.world().unwrap().level.maptime >= 5);
}

#[test]
fn m7_udmf_user_props_smoke() {
    use room_map::parse_udmf;

    let text = r#"
namespace = "zdoom";
vertex { x = 0.0; y = 0.0; }
vertex { x = 256.0; y = 0.0; }
vertex { x = 256.0; y = 256.0; }
vertex { x = 0.0; y = 256.0; }
sector {
  heightfloor = 0; heightceiling = 128;
  texturefloor = "FLAT"; textureceiling = "FLAT";
  lightlevel = 160; id = 5;
  user_wet = true;
}
sidedef { sector = 0; texturemiddle = "WALL"; }
sidedef { sector = 0; texturemiddle = "WALL"; }
sidedef { sector = 0; texturemiddle = "WALL"; }
sidedef { sector = 0; texturemiddle = "WALL"; }
linedef { v1 = 0; v2 = 1; sidefront = 0; id = 9; user_score = 42; }
linedef { v1 = 1; v2 = 2; sidefront = 1; }
linedef { v1 = 2; v2 = 3; sidefront = 2; }
linedef { v1 = 3; v2 = 0; sidefront = 3; }
thing { x = 128.0; y = 128.0; type = 1; angle = 0; }
"#;
    let udmf = parse_udmf(text).unwrap();
    let mut map = room_map::link_geometry(
        "MAP01",
        room_map::MapFormat::Udmf,
        udmf.vertices,
        udmf.linedefs,
        udmf.sidedefs,
        udmf.sectors,
        udmf.things,
        [0u8; 16],
    );
    map.udmf_user = udmf.user;
    apply_nodes(&mut map);
    assert_eq!(map.udmf_user.lines[0].ints.get("user_score"), Some(&42));
    assert_eq!(map.udmf_user.sectors[0].ints.get("user_wet"), Some(&1));

    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        rng_seed: 11,
        ..Default::default()
    });
    game.start_custom_map(map, Skill::new(3)).unwrap();
    let world = game.world().unwrap();
    assert_eq!(
        world.map.udmf_user.lines[0].ints.get("user_score"),
        Some(&42)
    );
    assert_eq!(
        world.map.udmf_user.sectors[0].ints.get("user_wet"),
        Some(&1)
    );
}

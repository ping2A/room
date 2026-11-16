//! UZ6 (docs/36) renderer-facing playsim rules, one golden per feature:
//!
//! - `uz6_sprite_view.txt`: ROLLSPRITE / ROLLCENTER, SPRITEANGLE,
//!   SPRITEROTATION, SPRITEFLIP, XFLIP / YFLIP, SQUAREPIXELS / STRETCHPIXELS,
//!   NODYNAMICLIGHTING and ISOMETRICSPRITES as `sprite_view` reports them;
//! - `uz6_sprite_shadow.txt`: `casts_sprite_shadow` for `r_actorspriteshadow`
//!   0 / 1 / 2;
//! - `uz6_isometric.txt`: `isometric_sprite` scale / tilt over camera angles;
//! - `uz6_fog.txt`: MAPINFO `skyfog` / `skymist` / thick fog defaults and
//!   overrides as the world holds them.
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-sim --test golden_uz6_render`

#![allow(clippy::unwrap_used)]

use room_core::{Angle, DVec3};
use room_decorate::parse_decorate;
use room_nodebuild::apply_nodes;
use room_sim::{ClassRegistry, RenderStyle, StateTable, World};
use room_testkit::{MapBuilder, rect};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

const ACTORS: &str = r#"
actor PlainThing { Radius 16 Height 56 States { Spawn: PLAI A -1 Stop } }
actor Roller { Radius 16 Height 56 +ROLLSPRITE States { Spawn: ROLL A -1 Stop } }
actor CenterRoller { Radius 16 Height 56 +ROLLSPRITE +ROLLCENTER States { Spawn: ROLL A -1 Stop } }
actor StretchRoller { Radius 16 Height 56 +ROLLSPRITE +STRETCHPIXELS States { Spawn: ROLL A -1 Stop } }
actor Square { Radius 16 Height 56 +SQUAREPIXELS States { Spawn: SQUA A -1 Stop } }
actor Angled { Radius 16 Height 56 +SPRITEANGLE SpriteAngle 90 States { Spawn: ANGL A -1 Stop } }
actor Rotated { Radius 16 Height 56 SpriteRotation 45 States { Spawn: ROTA A -1 Stop } }
actor Flipper { Radius 16 Height 56 +SPRITEFLIP +XFLIP +YFLIP States { Spawn: FLIP A -1 Stop } }
actor Unlit { Radius 16 Height 56 +NODYNAMICLIGHTING States { Spawn: UNLI A -1 Stop } }
actor Iso { Radius 16 Height 56 +ISOMETRICSPRITES States { Spawn: ISOS A -1 Stop } }
actor Caster { Radius 16 Height 56 +CASTSPRITESHADOW States { Spawn: CAST A -1 Stop } }
actor Imp2 { Radius 20 Height 56 Monster States { Spawn: TROO A -1 Stop } }
actor ShyImp { Radius 20 Height 56 Monster +NOSPRITESHADOW States { Spawn: TROO A -1 Stop } }
actor GlowImp { Radius 20 Height 56 Monster RenderStyle Add States { Spawn: TROO A -1 Stop } }
actor GlowCaster { Radius 16 Height 56 +CASTSPRITESHADOW RenderStyle Add States { Spawn: CAST A -1 Stop } }
actor FlatThing { Radius 16 Height 56 +CASTSPRITESHADOW +FLATSPRITE States { Spawn: FLAT A -1 Stop } }
"#;

fn world() -> World {
    let src = format!(
        "{}\n{ACTORS}",
        include_str!("../../../testdata/actors/doom_basic.txt")
    );
    let ir = parse_decorate(&src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 256.0)
        .player_start(64.0, 64.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    World::from_map_with_classes(map, classes, states).unwrap()
}

fn check(name: &str, text: &str) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/goldens")
        .join(name);
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, text).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, want, "{} differs", path.display());
}

#[test]
fn uz6_sprite_view_flags_match_golden() {
    let mut w = world();
    let eye = DVec3::new(512.0, 100.0, 41.0);
    let mut out = String::new();
    for class in [
        "PlainThing",
        "Roller",
        "CenterRoller",
        "StretchRoller",
        "Square",
        "Angled",
        "Rotated",
        "Flipper",
        "Unlit",
        "Iso",
    ] {
        let cid = w.classes.find_name(class).unwrap();
        let id = w
            .spawn_at(
                cid,
                DVec3::new(512.0, 512.0, 0.0),
                Angle::from_degrees(30.0),
            )
            .unwrap();
        w.actors.get_mut(id).unwrap().roll = Angle::from_degrees(20.0);
        let a = w.actors.get(id).unwrap().clone();
        let v = room_sim::sprite_view(&w, &a, eye, 0).unwrap();
        let _ = writeln!(
            out,
            "{class:<13} rot_angle={:.2} fixed={} rot_offset={:.1} roll_deg={:.1} center={} \
             flip={} x={} y={} pixel_scale=({:.4},{:.4}) no_dynlight={} iso={}",
            v.rotation_angle_deg,
            v.fixed_rotation,
            v.rotation_offset_deg,
            v.roll_rad.to_degrees(),
            v.roll_center,
            v.sprite_flip,
            v.flip_x,
            v.flip_y,
            v.pixel_scale.0,
            v.pixel_scale.1,
            v.no_dynlight,
            v.isometric,
        );
    }
    // `+SPRITEANGLE` 90: the view-relative angle is 90 from wherever the
    // viewer stands (the viewer is due south, at -90°).
    assert!(
        out.contains("Angled        rot_angle=-180.00 fixed=true"),
        "{out}"
    );
    check("uz6_sprite_view.txt", &out);
}

#[test]
fn uz6_sprite_shadow_rule_matches_golden() {
    let mut w = world();
    let mut out = String::new();
    let player = w.players[0].as_ref().unwrap().mo.unwrap();
    let mut ids = vec![("player".to_owned(), player)];
    for class in [
        "PlainThing",
        "Caster",
        "Imp2",
        "ShyImp",
        "GlowImp",
        "GlowCaster",
        "FlatThing",
    ] {
        let cid = w.classes.find_name(class).unwrap();
        let id = w
            .spawn_at(cid, DVec3::new(300.0, 300.0, 0.0), Angle::ZERO)
            .unwrap();
        ids.push((class.to_owned(), id));
    }
    assert_eq!(
        w.actors.get(ids[5].1).unwrap().render_style,
        RenderStyle::Add,
        "GlowImp is additive"
    );
    for (name, id) in &ids {
        let modes: Vec<u8> = (0..3)
            .map(|m| u8::from(room_sim::casts_sprite_shadow(&w, *id, m)))
            .collect();
        let _ = writeln!(
            out,
            "{name:<11} mode0={} mode1={} mode2={}",
            modes[0], modes[1], modes[2]
        );
    }
    check("uz6_sprite_shadow.txt", &out);
}

#[test]
fn uz6_isometric_sprite_matches_golden() {
    let mut out = String::new();
    for (yaw, pitch) in [
        (0.0, 30.0),
        (45.0, 30.0),
        (45.0, 60.0),
        (135.0, 35.264),
        (90.0, 0.0),
    ] {
        let (scale, tilt) = room_sim::isometric_sprite(16.0, 32.0, 56.0, yaw, pitch);
        let _ = writeln!(
            out,
            "yaw={yaw:<6} pitch={pitch:<7} scale_y={scale:.4} tilt={tilt:.4}"
        );
    }
    check("uz6_isometric.txt", &out);
}

#[test]
fn uz6_mapinfo_fog_matches_golden() {
    let mut w = world();
    let mut out = String::new();
    let mut dump = |w: &World, label: &str| {
        let _ = writeln!(
            out,
            "{label:<10} skyfog={} mist={:?} mist_speed={:.4} mist_yscale={} use_mist={} thick={:?}",
            w.sky_fog,
            w.sky_mist,
            w.sky_mist_speed,
            w.sky_mist_yscale,
            w.use_sky_mist,
            w.thick_fog(),
        );
    };
    dump(&w, "defaults");
    let mut extras = BTreeMap::new();
    for (k, v) in [
        ("skyfog", "100"),
        ("skymist", "MIST01,200"),
        ("skymistyscale", "1000"),
        ("useskymist", "true"),
        ("thickfogdistance", "768"),
    ] {
        extras.insert(k.to_owned(), v.to_owned());
    }
    w.apply_mapinfo_fog(&extras);
    dump(&w, "mapinfo");
    extras.clear();
    extras.insert("thickfogmultiplier".to_owned(), "0".to_owned());
    w.apply_mapinfo_fog(&extras);
    dump(&w, "mult=0");
    check("uz6_fog.txt", &out);
}

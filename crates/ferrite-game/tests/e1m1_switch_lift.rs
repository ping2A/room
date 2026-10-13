//! E1M1: SR door switch (type 63) and WR lift (type 88).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_core::{Angle, DVec3, LineId};
use ferrite_map::load_map;
use ferrite_nodebuild::ensure_nodes;
use ferrite_sim::{
    ActivationType, ClassRegistry, StateTable, World, activate_line, tick_doors, use_lines,
};
use ferrite_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

fn load_e1m1() -> Option<World> {
    let wad = doom1()?;
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = ferrite_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    Some(World::from_map_with_classes(map, classes, states).unwrap())
}

#[test]
fn e1m1_sr_switch_opens_tagged_door() {
    let Some(mut world) = load_e1m1() else {
        return;
    };
    // Line 136: Doom type 63 → Door_Raise tag 3 (SW1COMP).
    let lid = LineId::from_raw(136);
    let line = world.map.lines.get(136).expect("line 136");
    eprintln!(
        "line136 special={} act={:#x} flags={:#x} args={:?}",
        line.special, line.activation, line.flags, line.args
    );
    assert_ne!(line.special, 0, "switch special must survive XLAT");
    assert_ne!(
        line.activation & ferrite_map::spac::USE,
        0,
        "SR switch must be USE"
    );
    assert_ne!(
        i32::from(line.flags) & 0x0200,
        0,
        "SR switch must be REP (ML_REPEAT_SPECIAL)"
    );

    let door_sec = world
        .map
        .sectors
        .iter()
        .enumerate()
        .find(|(_, s)| s.tags.contains(&3))
        .map(|(i, _)| i)
        .expect("tag 3 door sector");
    let z0 = world.sectors[door_sec].ceiling_z;
    let mo = world.players[0].as_ref().and_then(|p| p.mo);
    assert!(
        activate_line(&mut world, lid, mo, ActivationType::Use),
        "switch use must succeed"
    );
    assert!(!world.doors.is_empty(), "door thinker must start");
    for _ in 0..80 {
        tick_doors(&mut world);
    }
    let z1 = world.sectors[door_sec].ceiling_z;
    eprintln!("door sec {door_sec} ceil {z0} → {z1}");
    assert!(z1 > z0 + 1.0, "tagged door must rise after switch");
}

#[test]
fn e1m1_player_use_switch_from_in_front() {
    let Some(mut world) = load_e1m1() else {
        return;
    };
    let mo = world.players[0].as_ref().unwrap().mo.unwrap();
    // Face north toward SW1COMP at y=-3072, x≈288.
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = DVec3::new(288.0, -3100.0, 0.0);
        a.angle = Angle::from_degrees(90.0); // +Y
    }
    // SW1COMP is on the *lower* tier (front floor -8 → back floor 40).
    let side = world.map.sides.get(184).expect("switch sidedef");
    assert_eq!(side.lower.to_ascii_uppercase(), "SW1COMP");
    assert_eq!(side.y_offset_bottom, side.y_offset, "Doom y_offset applies to lower");
    assert!(use_lines(&mut world, mo), "use ray must find the switch");
    assert!(
        !world.doors.is_empty(),
        "use should start the door thinker"
    );
    let side = world.map.sides.get(184).expect("switch sidedef");
    assert_eq!(
        side.lower.to_ascii_uppercase(),
        "SW2COMP",
        "use must flip SW1COMP → SW2COMP on lower"
    );
}

#[test]
fn e1m1_wr_lift_uses_lowest_neighbor_and_no_stack() {
    let Some(mut world) = load_e1m1() else {
        return;
    };
    let lid = LineId::from_raw(191);
    let lift_sec = world
        .map
        .sectors
        .iter()
        .enumerate()
        .find(|(_, s)| s.tags.contains(&2))
        .map(|(i, _)| i)
        .expect("tag 2 lift");
    let high = world.sectors[lift_sec].floor_z;
    let mo = world.players[0].as_ref().unwrap().mo.unwrap();
    assert!(activate_line(
        &mut world,
        lid,
        Some(mo),
        ActivationType::Cross
    ));
    assert_eq!(world.plats.len(), 1);
    let low = world.plats[0].low;
    eprintln!("lift sec {lift_sec} high={high} low={low}");
    assert!(
        high - low > 16.0,
        "lift travel must be more than a lip stub (got {})",
        high - low
    );
    // Re-cross while busy: must not stack thinkers (activate may return false).
    let _ = activate_line(&mut world, lid, Some(mo), ActivationType::Cross);
    assert_eq!(world.plats.len(), 1, "busy lift must not stack plats");
}

#[test]
fn e1m1_exit_switch_flips_sw1strtn() {
    let Some(mut world) = load_e1m1() else {
        return;
    };
    let line = world.map.lines.get(326).expect("line 326");
    assert_eq!(line.special, 243, "Doom type 11 → Exit_Normal");
    assert_ne!(line.activation & ferrite_map::spac::USE, 0);
    assert_eq!(
        world.map.sides[449].middle.to_ascii_uppercase(),
        "SW1STRTN"
    );

    let mo = world.players[0].as_ref().unwrap().mo.unwrap();
    let v1 = world.map.vertices[line.v1.raw() as usize].pos;
    let v2 = world.map.vertices[line.v2.raw() as usize].pos;
    let mx = (v1.x() + v2.x()) * 0.5;
    let my = (v1.y() + v2.y()) * 0.5;
    let dx = v2.x() - v1.x();
    let dy = v2.y() - v1.y();
    let len = (dx * dx + dy * dy).sqrt();
    // Doom front normal = right of v1→v2.
    let nx = dy / len;
    let ny = -dx / len;
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = DVec3::new(mx + nx * 32.0, my + ny * 32.0, 0.0);
        a.angle = Angle::from_radians((-ny).atan2(-nx));
    }

    // Real tic path: `World::tick` takes the player slot out during
    // `player_think` — Exit must still activate and flip SW1STRTN→SW2STRTN.
    let mut cmd = ferrite_sim::TicCmd::default();
    cmd.buttons = ferrite_sim::Buttons(ferrite_sim::Buttons::USE);
    world.set_ticcmds(&[cmd]);
    world.tick();

    assert!(world.level.exiting, "Exit_Normal must run from player Use");
    assert_eq!(
        world.map.sides[449].middle.to_ascii_uppercase(),
        "SW2STRTN",
        "exit switch must flip red→green"
    );
}

//! Solo: closed door must block sight; open path must not shoot through walls.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_map::load_map;
use ferrite_nodebuild::ensure_nodes;
use ferrite_sim::{ClassRegistry, StateTable, World, check_sight, line_attack};
use ferrite_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

#[test]
fn e1m1_closed_door_blocks_sight_to_computer_room_zombie() {
    let Some(wad) = doom1() else {
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = ferrite_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut world = World::from_map_with_classes(map, classes, states).unwrap();

    let mo = world.players[0].as_ref().unwrap().mo.unwrap();
    // First skill-3 ZombieMan (computer / zig-zag area).
    let zid = world
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            world
                .classes
                .get(world.actors.get(id).unwrap().class)
                .is_some_and(|c| c.name == "ZombieMan")
        })
        .expect("zombie");

    let sees = check_sight(&world, zid, mo);
    let sees_rev = check_sight(&world, mo, zid);
    eprintln!("sight zombie→player={sees} player→zombie={sees_rev}");
    // At map start with closed doors, they should NOT see each other.
    assert!(
        !sees && !sees_rev,
        "zombie should not see player through closed E1M1 doors (got z→p={sees} p→z={sees_rev})"
    );

    // Hitscan from zombie toward player must not hit through walls.
    let ang = {
        let z = world.actors.get(zid).unwrap();
        let p = world.actors.get(mo).unwrap();
        let dx = p.pos.x() - z.pos.x();
        let dy = p.pos.y() - z.pos.y();
        ferrite_core::Angle::from_radians(dy.atan2(dx))
    };
    let hit = line_attack(&mut world, zid, ang, 4096.0, 10);
    eprintln!("hitscan through map: actor={:?} dist={:.0}", hit.actor, hit.distance);
    if let Some(id) = hit.actor {
        let name = world
            .classes
            .get(world.actors.get(id).unwrap().class)
            .map(|c| c.name.as_str())
            .unwrap_or("?");
        assert_ne!(id, mo, "hitscan must not hit player through walls (hit {name})");
    }
}

//! Node validation + point-in-sector sanity on Freedoom / testkit maps.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_map::load_map;
use ferrite_nodebuild::{apply_nodes, point_in_sector, validate_subsectors};
use ferrite_testkit::{MapBuilder, rect};
use ferrite_vfs::VfsBuilder;
use std::path::PathBuf;

#[test]
fn fan_builder_room_is_convex_closed() {
    let mut map = MapBuilder::new()
        .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
        .player_start(64.0, 128.0, 0)
        .build_udmf("MAP01")
        .unwrap();
    apply_nodes(&mut map);
    let nodes = map.game_nodes.as_ref().unwrap();
    let report = validate_subsectors(&map, nodes);
    assert!(
        report.ok(),
        "fan subsectors should be closed+convex: {:?}",
        report.failures
    );
    let sec = point_in_sector(&map, 64.0, 128.0);
    assert!(sec.is_some());
}

#[test]
fn freedoom_map01_player_start_in_sector() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
    if !path.is_file() {
        eprintln!("skip: freedoom2.wad missing");
        return;
    }
    let vfs = VfsBuilder::new()
        .mount_path(&path)
        .unwrap()
        .build()
        .unwrap();
    let map = load_map(&vfs, "MAP01").unwrap();
    let start = map
        .things
        .iter()
        .find(|t| t.type_id == 1)
        .expect("player start");
    let sec = point_in_sector(&map, f64::from(start.x), f64::from(start.y));
    assert!(
        sec.is_some(),
        "player start ({}, {}) not in any sector",
        start.x,
        start.y
    );
    // Classic Doom SEGS omit partition minisegs — they are not closed polygons.
    // Only index integrity is required here.
    let nodes = map.game_nodes.as_ref().unwrap();
    for (si, sub) in nodes.subsectors.iter().enumerate() {
        let end = sub.first_seg as usize + sub.num_segs as usize;
        assert!(end <= nodes.segs.len(), "subsector {si} seg range OOB");
    }
}

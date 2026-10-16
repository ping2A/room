//! Editor documents (`RawMap` JSON) rebuild the same geometry the loader makes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_map::{RawMap, load_map};
use room_vfs::VfsBuilder;
use std::path::PathBuf;

#[test]
fn doom1_maps_survive_json_roundtrip() {
    let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    if !wad.is_file() {
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    for name in ["E1M1", "E1M2", "E1M5"] {
        let loaded = load_map(&vfs, name).unwrap();
        let raw = RawMap::load(&vfs, name).unwrap();
        let json = serde_json::to_string(&raw).unwrap();
        let back: RawMap = serde_json::from_str(&json).unwrap();
        let geom = back.to_geometry().unwrap();
        assert_eq!(geom.vertices, loaded.vertices, "{name} vertices");
        for (i, (a, b)) in geom.sectors.iter().zip(&loaded.sectors).enumerate() {
            assert_eq!(format!("{a:?}"), format!("{b:?}"), "{name} sectors[{i}]");
        }
        assert_eq!(geom.sectors.len(), loaded.sectors.len(), "{name} sectors");
        for (i, (a, b)) in geom.sides.iter().zip(&loaded.sides).enumerate() {
            assert_eq!(format!("{a:?}"), format!("{b:?}"), "{name} sides[{i}]");
        }
        assert_eq!(geom.sides.len(), loaded.sides.len(), "{name} sides");
        for (i, (a, b)) in geom.lines.iter().zip(&loaded.lines).enumerate() {
            assert_eq!(format!("{a:?}"), format!("{b:?}"), "{name} lines[{i}]");
        }
        assert_eq!(geom.lines.len(), loaded.lines.len(), "{name} lines");
        assert_eq!(geom.things, loaded.things, "{name} things");
        let mut geom = geom;
        geom.game_nodes.clone_from(&loaded.game_nodes);
        geom.gl_nodes.clone_from(&loaded.gl_nodes);
        for (field, a, b) in [
            (
                "setup",
                format!("{:?}", geom.setup),
                format!("{:?}", loaded.setup),
            ),
            (
                "reject",
                format!("{:?}", geom.reject),
                format!("{:?}", loaded.reject),
            ),
            (
                "format",
                format!("{:?}", geom.format),
                format!("{:?}", loaded.format),
            ),
            (
                "udmf",
                format!("{:?}", geom.udmf_user),
                format!("{:?}", loaded.udmf_user),
            ),
        ] {
            assert!(a == b, "{name} {field} differs");
        }
    }
}

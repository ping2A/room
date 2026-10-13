//! Load every map in Freedoom Phase 1/2 when IWADs are present.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ferrite_map::load_map;
use ferrite_vfs::VfsBuilder;
use std::path::PathBuf;

fn freedoom_dir() -> PathBuf {
    if let Ok(p) = std::env::var("FERRITE_FREEDOOM_DIR") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom")
}

fn load_all(wad_name: &str, maps: &[&str]) {
    let path = freedoom_dir().join(wad_name);
    if !path.is_file() {
        eprintln!(
            "skip: {} missing (run scripts/fetch-freedoom.sh)",
            path.display()
        );
        return;
    }
    let vfs = VfsBuilder::new()
        .mount_path(&path)
        .expect("mount")
        .build()
        .expect("build vfs");
    for &name in maps {
        let map = load_map(&vfs, name).unwrap_or_else(|e| panic!("{wad_name} {name}: {e}"));
        assert!(
            !map.vertices.is_empty() && !map.lines.is_empty() && !map.sectors.is_empty(),
            "{wad_name} {name} empty geometry"
        );
        assert!(map.game_nodes.is_some(), "{wad_name} {name} missing nodes");
        assert!(
            !map.blockmap.blocks.is_empty(),
            "{wad_name} {name} empty blockmap"
        );
    }
}

#[test]
fn freedoom1_all_maps() {
    let maps: Vec<String> = (1..=4)
        .flat_map(|e| (1..=9).map(move |m| format!("E{e}M{m}")))
        .collect();
    let refs: Vec<&str> = maps.iter().map(String::as_str).collect();
    load_all("freedoom1.wad", &refs);
}

#[test]
fn freedoom2_all_maps() {
    let maps: Vec<String> = (1..=32).map(|i| format!("MAP{i:02}")).collect();
    let refs: Vec<&str> = maps.iter().map(String::as_str).collect();
    load_all("freedoom2.wad", &refs);
}

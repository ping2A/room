//! Optional Freedoom Phase 1/2 definition goldens.
//!
//! Run `scripts/fetch-freedoom.sh` first. Skipped when IWADs are absent so CI
//! without the cache still passes; enable with `ROOM_FREEDOOM_DIR` or the
//! default `testdata/freedoom` path.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use room_defs::{Definitions, DefinitionsBuilder};
use room_vfs::VfsBuilder;
use std::path::{Path, PathBuf};

fn freedoom_dir() -> PathBuf {
    if let Ok(p) = std::env::var("ROOM_FREEDOOM_DIR") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom")
}

fn try_build(path: &Path) -> Option<Definitions> {
    if !path.is_file() {
        return None;
    }
    let vfs = VfsBuilder::new().mount_path(path).ok()?.build().ok()?;
    DefinitionsBuilder::new(&vfs).build().ok()
}

fn summarise(defs: &Definitions) -> String {
    format!(
        "maps={}\ntextures={}\npnames={}\nanims={}\nswitches={}\nhas_playpal={}\nhas_colormap={}\nwarnings={}\n",
        defs.mapinfo.maps.len(),
        defs.textures.by_name.len(),
        defs.pnames.len(),
        defs.animdefs.anims.len(),
        defs.switches.len(),
        defs.playpal.is_some(),
        defs.colormap.is_some(),
        defs.warnings.len(),
    )
}

#[test]
fn freedoom1_definitions_snapshot() {
    let path = freedoom_dir().join("freedoom1.wad");
    let Some(defs) = try_build(&path) else {
        eprintln!(
            "skip: {} not found (run scripts/fetch-freedoom.sh)",
            path.display()
        );
        return;
    };
    insta::assert_snapshot!(summarise(&defs));
}

#[test]
fn freedoom2_definitions_snapshot() {
    let path = freedoom_dir().join("freedoom2.wad");
    let Some(defs) = try_build(&path) else {
        eprintln!(
            "skip: {} not found (run scripts/fetch-freedoom.sh)",
            path.display()
        );
        return;
    };
    insta::assert_snapshot!(summarise(&defs));
}

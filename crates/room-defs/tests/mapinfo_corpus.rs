//! MAPINFO corpus: every fixture under `testdata/mapinfo/` must parse with zero errors.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use std::fs;
use std::path::PathBuf;

#[test]
fn all_mapinfo_fixtures_parse() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/mapinfo");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {dir:?}: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext == "mapinfo" || ext == "txt")
        })
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "expected at least one MAPINFO fixture in {dir:?}"
    );

    for path in files {
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let info = room_defs::parse_mapinfo(&text)
            .unwrap_or_else(|e| panic!("parse {} failed: {e}", path.display()));
        assert!(!info.maps.is_empty(), "{} produced no maps", path.display());
    }
}

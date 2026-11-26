//! Fixture-driven primitive goldens (`testdata/primitives/`).
//! See `docs/build-engine-primitives/10-goldens.md`.
//!
//! Cases may embed geometry or set `"map": "E1L1.MAP"` to load from
//! `testdata/duke/duke3d.grp` (map cases skipped if GRP missing).

// Fixture helpers run only under `cargo test`; a malformed fixture should panic.
#![allow(clippy::unwrap_used)]

use room_build::grp::Grp;
use room_build_engine::{
    BuildWorld, SectorWalls, SectorZ, WallXY, clipmove_xy, getzsofslope, inside,
};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/primitives")
}

fn grp_bytes() -> Option<&'static [u8]> {
    static BYTES: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    BYTES
        .get_or_init(|| {
            let p =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke/duke3d.grp");
            fs::read(p).ok()
        })
        .as_deref()
}

#[derive(Debug, Deserialize)]
struct WallJson {
    x: i32,
    y: i32,
    point2: i16,
    nextsector: i16,
}

#[derive(Debug, Deserialize)]
struct SectorJson {
    wallptr: i16,
    wallnum: i16,
}

#[derive(Debug, Deserialize)]
struct SectorZJson {
    wallptr: i16,
    wallnum: i16,
    ceilingz: i32,
    floorz: i32,
    ceilingstat: u16,
    floorstat: u16,
    ceilingheinum: i16,
    floorheinum: i16,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    op: String,
    /// When set, load geometry from this GRP map instead of embedded walls.
    #[serde(default)]
    map: Option<String>,
    #[serde(default)]
    walls: Vec<WallJson>,
    #[serde(default)]
    sectors: Vec<SectorJson>,
    #[serde(default)]
    sector_z: Vec<SectorZJson>,
    /// Max |Δ| allowed on clipmove x/y vs expect (Chocolate slide ±1).
    #[serde(default)]
    pos_atol: i32,
    input: serde_json::Value,
    expect: serde_json::Value,
}

fn walls(v: &[WallJson]) -> Vec<WallXY> {
    v.iter()
        .map(|w| WallXY {
            x: w.x,
            y: w.y,
            point2: w.point2,
            nextsector: w.nextsector,
            cstat: 0,
        })
        .collect()
}

fn sectors(v: &[SectorJson]) -> Vec<SectorWalls> {
    v.iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect()
}

fn sector_z(v: &[SectorZJson]) -> Vec<SectorZ> {
    v.iter()
        .map(|s| SectorZ {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
            ceilingz: s.ceilingz,
            floorz: s.floorz,
            ceilingstat: s.ceilingstat,
            floorstat: s.floorstat,
            ceilingheinum: s.ceilingheinum,
            floorheinum: s.floorheinum,
        })
        .collect()
}

fn world_from_map(name: &str) -> Option<BuildWorld> {
    let bytes = grp_bytes()?;
    let grp = Grp::parse(bytes).ok()?;
    let data = grp.get(name)?;
    BuildWorld::load_map_bytes(data).ok()
}

fn geom_from_world(w: &BuildWorld) -> (Vec<WallXY>, Vec<SectorWalls>, Vec<SectorZ>) {
    let walls = w
        .walls
        .iter()
        .take(w.numwalls as usize)
        .map(|ww| WallXY {
            x: ww.x,
            y: ww.y,
            point2: ww.point2,
            nextsector: ww.nextsector,
            cstat: 0,
        })
        .collect();
    let secs = w
        .sectors
        .iter()
        .take(w.numsectors as usize)
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sz = w
        .sectors
        .iter()
        .take(w.numsectors as usize)
        .map(|s| SectorZ {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
            ceilingz: s.ceilingz,
            floorz: s.floorz,
            ceilingstat: s.ceilingstat,
            floorstat: s.floorstat,
            ceilingheinum: s.ceilingheinum,
            floorheinum: s.floorheinum,
        })
        .collect();
    (walls, secs, sz)
}

#[test]
fn primitives_manifest_goldens() {
    let manifest = fs::read_to_string(root().join("manifest.toml")).expect("manifest");
    let mut cases: Vec<String> = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('"') {
            if let Some(path) = rest.strip_suffix("\",") {
                cases.push(path.to_string());
            } else if let Some(path) = rest.strip_suffix('"') {
                cases.push(path.to_string());
            }
        }
    }
    assert!(!cases.is_empty(), "no cases in manifest");
    let mut ran = 0_u32;
    for rel in cases {
        let path = root().join(&rel);
        let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let case: Case = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{rel}: {e}"));
        if case.map.is_some() && grp_bytes().is_none() {
            continue;
        }
        run_case(&case);
        ran += 1;
    }
    assert!(ran > 0, "no cases ran");
}

fn run_case(case: &Case) {
    let (w, sec, sz) = if let Some(ref map) = case.map {
        let world =
            world_from_map(map).unwrap_or_else(|| panic!("{}: missing map {map}", case.name));
        geom_from_world(&world)
    } else {
        (
            walls(&case.walls),
            sectors(&case.sectors),
            sector_z(&case.sector_z),
        )
    };
    match case.op.as_str() {
        "inside" => {
            let x = case.input["x"].as_i64().unwrap() as i32;
            let y = case.input["y"].as_i64().unwrap() as i32;
            let sect = case.input["sect"].as_i64().unwrap() as i16;
            let got = inside(x, y, sect, &sec, &w);
            let exp = case.expect["inside"].as_bool().unwrap();
            assert_eq!(got, exp, "{} inside", case.name);
        }
        "getzsofslope" => {
            let x = case.input["x"].as_i64().unwrap() as i32;
            let y = case.input["y"].as_i64().unwrap() as i32;
            let sect = case.input["sect"].as_i64().unwrap() as i16;
            let (c, f) = getzsofslope(sect, x, y, &sz, &w);
            assert_eq!(
                c,
                case.expect["ceilz"].as_i64().unwrap() as i32,
                "{}",
                case.name
            );
            assert_eq!(
                f,
                case.expect["florz"].as_i64().unwrap() as i32,
                "{}",
                case.name
            );
        }
        "clipmove_xy" => {
            let mut x = case.input["x"].as_i64().unwrap() as i32;
            let mut y = case.input["y"].as_i64().unwrap() as i32;
            let mut sect = case.input["sect"].as_i64().unwrap() as i16;
            let xvect = case.input["xvect"].as_i64().unwrap() as i32;
            let yvect = case.input["yvect"].as_i64().unwrap() as i32;
            let walldist = case.input["walldist"].as_i64().unwrap() as i32;
            let hit = clipmove_xy(&mut x, &mut y, &mut sect, xvect, yvect, walldist, &sec, &w);
            let ex = case.expect["x"].as_i64().unwrap() as i32;
            let ey = case.expect["y"].as_i64().unwrap() as i32;
            let atol = case.pos_atol;
            assert!(
                (x - ex).abs() <= atol,
                "{} x: got {x} expect {ex} (atol {atol})",
                case.name
            );
            assert!(
                (y - ey).abs() <= atol,
                "{} y: got {y} expect {ey} (atol {atol})",
                case.name
            );
            assert_eq!(
                hit,
                case.expect["hit"].as_i64().unwrap() as i32,
                "{}",
                case.name
            );
        }
        other => panic!("{}: unsupported op {other}", case.name),
    }
}

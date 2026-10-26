//! Multi-map primitive soak: every Build `.MAP` in `DUKE3D.GRP` (when present).
//! Not Chocolate black-box — locks no-panic + sensible start-sector answers.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_build::grp::Grp;
use room_build_engine::{
    BuildWorld, SectorWalls, SectorZ, WallXY, clipmove_xy, getzsofslope, inside, updatesector,
};
use std::path::PathBuf;

fn grp_bytes() -> Option<Vec<u8>> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke/duke3d.grp");
    std::fs::read(p).ok()
}

fn wall_xy(w: &BuildWorld) -> Vec<WallXY> {
    w.walls
        .iter()
        .take(w.numwalls as usize)
        .map(|w| WallXY {
            x: w.x,
            y: w.y,
            point2: w.point2,
            nextsector: w.nextsector,
            cstat: 0,
        })
        .collect()
}

fn sector_walls(w: &BuildWorld) -> Vec<SectorWalls> {
    w.sectors
        .iter()
        .take(w.numsectors as usize)
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect()
}

fn sector_z(w: &BuildWorld) -> Vec<SectorZ> {
    w.sectors
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
        .collect()
}

#[test]
fn primitives_soak_all_grp_maps() {
    let Some(bytes) = grp_bytes() else {
        return;
    };
    let grp = Grp::parse(&bytes).unwrap();
    let mut n = 0_u32;
    for name in grp.names() {
        if !(name.starts_with('E') && name.ends_with(".MAP")) {
            continue;
        }
        let Some(map) = grp.get(name) else {
            continue;
        };
        let world = BuildWorld::load_map_bytes(map).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(world.numsectors > 0, "{name}");
        let walls = wall_xy(&world);
        let secs = sector_walls(&world);
        let sz = sector_z(&world);
        let (x, y, z) = (world.start[0], world.start[1], world.start[2]);
        let mut sect = world.start_sector;
        if !inside(x, y, sect, &secs, &walls) {
            updatesector(x, y, &mut sect, &secs, &walls);
        }
        assert!(
            sect >= 0 && inside(x, y, sect, &secs, &walls),
            "{name}: start ({x},{y}) not in any sector (hdr sect {})",
            world.start_sector
        );
        let (ceilz, florz) = getzsofslope(sect, x, y, &sz, &walls);
        assert!(ceilz <= florz, "{name}: ceilz={ceilz} florz={florz}");
        let _ = z; // start Z can sit outside flat getz on odd maps / slopes
        let mut cx = x;
        let mut cy = y;
        let mut cs = sect;
        let _ = clipmove_xy(&mut cx, &mut cy, &mut cs, 8, 0, 4, &secs, &walls);
        assert!(cx.abs() < 1_000_000 && cy.abs() < 1_000_000, "{name}");
        n += 1;
    }
    assert!(n >= 40, "expected ≥40 episode maps, got {n}");
}

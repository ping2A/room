//! Property tests for bootstrap primitives (no Chocolate required).

use proptest::prelude::*;
use room_build_engine::{
    SectorWalls, SectorZ, WallXY, clipmove_xy, getzsofslope, hitscan, inside, updatesector,
};

fn unit_square() -> (Vec<SectorWalls>, Vec<WallXY>) {
    let walls = vec![
        WallXY {
            x: 0,
            y: 0,
            point2: 1,
            nextsector: -1,
            cstat: 0,
        },
        WallXY {
            x: 100,
            y: 0,
            point2: 2,
            nextsector: -1,
            cstat: 0,
        },
        WallXY {
            x: 100,
            y: 100,
            point2: 3,
            nextsector: -1,
            cstat: 0,
        },
        WallXY {
            x: 0,
            y: 100,
            point2: 0,
            nextsector: -1,
            cstat: 0,
        },
    ];
    (
        vec![SectorWalls {
            wallptr: 0,
            wallnum: 4,
        }],
        walls,
    )
}

fn unit_square_z() -> (Vec<SectorZ>, Vec<WallXY>) {
    let (_, walls) = unit_square();
    (
        vec![SectorZ {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            ceilingstat: 0,
            floorstat: 0,
            ceilingheinum: 0,
            floorheinum: 0,
        }],
        walls,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn clipmove_stays_finite(xvect in -200_i32..200, yvect in -200_i32..200, walldist in 0_i32..20) {
        let (sec, walls) = unit_square();
        let mut x = 50;
        let mut y = 50;
        let mut s = 0_i16;
        let _hit = clipmove_xy(&mut x, &mut y, &mut s, xvect, yvect, walldist, &sec, &walls);
        prop_assert!(x > -10_000 && x < 10_000);
        prop_assert!(y > -10_000 && y < 10_000);
        prop_assert!(s == 0 || s == -1);
    }

    #[test]
    fn inside_center_band_true(x in 1_i32..99, y in 1_i32..99) {
        let (sec, walls) = unit_square();
        prop_assert!(inside(x, y, 0, &sec, &walls));
    }

    #[test]
    fn inside_outside_false(x in 101_i32..1000, y in -1000_i32..2000) {
        let (sec, walls) = unit_square();
        prop_assert!(!inside(x, y, 0, &sec, &walls));
    }

    #[test]
    fn getz_flat_constant(x in -500_i32..500, y in -500_i32..500) {
        let (sz, walls) = unit_square_z();
        let (c, f) = getzsofslope(0, x, y, &sz, &walls);
        prop_assert_eq!(c, -8192);
        prop_assert_eq!(f, 0);
    }

    #[test]
    fn hitscan_no_panic(
        vx in -500_i32..500,
        vy in -500_i32..500,
        x in 1_i32..99,
        y in 1_i32..99,
    ) {
        let (sec, walls) = unit_square();
        let h = hitscan(x, y, 0, 0, vx, vy, 0, 0, &sec, &walls);
        prop_assert!(h.hitwall >= -1);
    }

    #[test]
    fn updatesector_finds_square(x in 10_i32..90, y in 10_i32..90) {
        let (sec, walls) = unit_square();
        let mut s = -1_i16;
        updatesector(x, y, &mut s, &sec, &walls);
        prop_assert_eq!(s, 0);
    }

    #[test]
    fn updatesector_outside_clears(x in 200_i32..500, y in 200_i32..500) {
        let (sec, walls) = unit_square();
        let mut s = 0_i16;
        updatesector(x, y, &mut s, &sec, &walls);
        prop_assert!(s == -1 || s == 0);
    }
}

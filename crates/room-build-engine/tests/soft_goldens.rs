//! Deterministic soft FP goldens (synthetic maps — no commercial GRP).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_build_engine::{
    BuildWorld, SectorT, SoftFrame, SoftTiles, WallT, fill_fp_flats_sky, render_fp_solid,
    render_fp_textured,
};
/// Stable across Rust versions (unlike `DefaultHasher`).
fn frame_hash(pixels: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64; // FNV-1a offset
    for &b in pixels {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn square(floorpic: i16, ceilpic: i16, ceilstat: u16, wallpic: i16) -> (Vec<SectorT>, Vec<WallT>) {
    let sectors = vec![SectorT {
        wallptr: 0,
        wallnum: 4,
        ceilingz: -8192,
        floorz: 0,
        floorpicnum: floorpic,
        ceilingpicnum: ceilpic,
        ceilingstat: ceilstat,
        floorshade: 4,
        ceilingshade: 8,
        ..SectorT::default()
    }];
    let walls = vec![
        WallT {
            x: 0,
            y: 0,
            point2: 1,
            nextsector: -1,
            picnum: wallpic,
            shade: 2,
            xrepeat: 8,
            yrepeat: 8,
            ..WallT::default()
        },
        WallT {
            x: 512,
            y: 0,
            point2: 2,
            nextsector: -1,
            picnum: wallpic,
            shade: 2,
            xrepeat: 8,
            yrepeat: 8,
            ..WallT::default()
        },
        WallT {
            x: 512,
            y: 512,
            point2: 3,
            nextsector: -1,
            picnum: wallpic,
            shade: 2,
            xrepeat: 8,
            yrepeat: 8,
            ..WallT::default()
        },
        WallT {
            x: 0,
            y: 512,
            point2: 0,
            nextsector: -1,
            picnum: wallpic,
            shade: 2,
            xrepeat: 8,
            yrepeat: 8,
            ..WallT::default()
        },
    ];
    (sectors, walls)
}

#[test]
fn golden_solid_square_room() {
    let (sectors, walls) = square(10, 20, 0, 30);
    let w = BuildWorld::load_arrays(sectors, walls, &[], [256, 200, -2048], 512, 100, 0).unwrap();
    let v = w.build_view(256, 200, -2048, 512, 100, 0);
    let mut f = SoftFrame::new(96, 64);
    render_fp_solid(&w, &v, &mut f);
    // Locked hash — bump deliberately when soft path changes intentionally.
    assert_eq!(
        frame_hash(&f.pixels),
        0xafdd_71aa_31dc_8325,
        "solid soft golden drifted (update hash if intentional)"
    );
}

#[test]
fn golden_parallax_sky_band() {
    let (sectors, walls) = square(10, 89, 1, 30);
    let w = BuildWorld::load_arrays(sectors, walls, &[], [256, 256, -2048], 0, 100, 0).unwrap();
    let v = w.build_view(256, 256, -2048, 0, 100, 0);
    let mut f = SoftFrame::new(64, 48);
    fill_fp_flats_sky(&w, &v, &mut f, None);
    assert_eq!(
        frame_hash(&f.pixels),
        0x7a20_db51_5abb_31a5,
        "sky soft golden drifted (update hash if intentional)"
    );
}

/// Two rooms linked by a portal — view floods both; soft draw is stable.
#[test]
fn golden_portal_two_sectors() {
    let sectors = vec![
        SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            floorpicnum: 1,
            ceilingpicnum: 2,
            ..SectorT::default()
        },
        SectorT {
            wallptr: 4,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            floorpicnum: 3,
            ceilingpicnum: 4,
            ..SectorT::default()
        },
    ];
    let walls = vec![
        WallT {
            x: 0,
            y: 0,
            point2: 1,
            nextsector: -1,
            picnum: 10,
            ..WallT::default()
        },
        WallT {
            x: 256,
            y: 0,
            point2: 2,
            nextsector: 1,
            picnum: 11,
            ..WallT::default()
        },
        WallT {
            x: 256,
            y: 256,
            point2: 3,
            nextsector: -1,
            picnum: 12,
            ..WallT::default()
        },
        WallT {
            x: 0,
            y: 256,
            point2: 0,
            nextsector: -1,
            picnum: 13,
            ..WallT::default()
        },
        WallT {
            x: 256,
            y: 0,
            point2: 5,
            nextsector: -1,
            picnum: 20,
            ..WallT::default()
        },
        WallT {
            x: 512,
            y: 0,
            point2: 6,
            nextsector: -1,
            picnum: 21,
            ..WallT::default()
        },
        WallT {
            x: 512,
            y: 256,
            point2: 7,
            nextsector: -1,
            picnum: 22,
            ..WallT::default()
        },
        WallT {
            x: 256,
            y: 256,
            point2: 4,
            nextsector: 0,
            picnum: 23,
            ..WallT::default()
        },
    ];
    let w = BuildWorld::load_arrays(sectors, walls, &[], [128, 128, -2048], 0, 100, 0).unwrap();
    let v = w.build_view(128, 128, -2048, 0, 100, 0);
    assert!(v.visible_sectors.contains(&0) && v.visible_sectors.contains(&1));
    let mut f = SoftFrame::new(96, 64);
    render_fp_solid(&w, &v, &mut f);
    let lit = f.pixels.iter().filter(|&&p| p != 0).count();
    assert!(lit > 500, "portal view should paint flats/walls lit={lit}");
    assert_eq!(
        frame_hash(&f.pixels),
        0xc60d_5465_bd62_9b25,
        "portal soft golden drifted (update hash if intentional)"
    );
}

#[test]
fn golden_textured_walls() {
    let (sectors, walls) = square(3, 4, 0, 7);
    let mut tiles = SoftTiles::new();
    // 4x4 checker
    let mut pix = Vec::new();
    for x in 0..4 {
        for y in 0..4 {
            pix.push(if (x + y) % 2 == 0 { 50 } else { 200 });
        }
    }
    tiles.insert(7, 4, 4, pix);
    tiles.insert(3, 2, 2, vec![11, 12, 13, 14]);
    tiles.insert(4, 2, 2, vec![21, 22, 23, 24]);
    let w = BuildWorld::load_arrays(sectors, walls, &[], [256, 180, -2048], 512, 100, 0).unwrap();
    let v = w.build_view(256, 180, -2048, 512, 100, 0);
    let mut f = SoftFrame::new(96, 64);
    render_fp_textured(&w, &v, &mut f, &tiles);
    assert_eq!(
        frame_hash(&f.pixels),
        0x76a2_ed4b_7564_2b25,
        "textured soft golden drifted (update hash if intentional)"
    );
}

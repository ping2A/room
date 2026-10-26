#![no_main]
//! D2: no-panic fuzz for Build clean-room primitives.

use libfuzzer_sys::fuzz_target;
use room_build_engine::{
    clipmove_xy, getzsofslope, hitscan, inside, updatesector, SectorWalls, SectorZ, WallXY,
};

fn read_i32(data: &[u8], i: &mut usize) -> i32 {
    if *i + 4 > data.len() {
        return 0;
    }
    let v = i32::from_le_bytes(data[*i..*i + 4].try_into().unwrap_or([0; 4]));
    *i += 4;
    v
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }
    let mut i = 0_usize;
    let side = (read_i32(data, &mut i).unsigned_abs() % 2000 + 16) as i32;
    let walls = vec![
        WallXY {
            x: 0,
            y: 0,
            point2: 1,
            nextsector: -1,
            cstat: 0,
        },
        WallXY {
            x: side,
            y: 0,
            point2: 2,
            nextsector: -1,
            cstat: 0,
        },
        WallXY {
            x: side,
            y: side,
            point2: 3,
            nextsector: -1,
            cstat: 0,
        },
        WallXY {
            x: 0,
            y: side,
            point2: 0,
            nextsector: -1,
            cstat: 0,
        },
    ];
    let secs = vec![SectorWalls {
        wallptr: 0,
        wallnum: 4,
    }];
    let sz = vec![SectorZ {
        wallptr: 0,
        wallnum: 4,
        ceilingz: read_i32(data, &mut i),
        floorz: read_i32(data, &mut i),
        ceilingstat: 0,
        floorstat: 0,
        ceilingheinum: 0,
        floorheinum: 0,
    }];
    let x = read_i32(data, &mut i);
    let y = read_i32(data, &mut i);
    let vx = read_i32(data, &mut i);
    let vy = read_i32(data, &mut i);
    let _ = inside(x, y, 0, &secs, &walls);
    let _ = getzsofslope(0, x, y, &sz, &walls);
    let _ = hitscan(x, y, 0, 0, vx, vy, 0, 0, &secs, &walls);
    let mut sx = x;
    let mut sy = y;
    let mut sect = 0_i16;
    updatesector(sx, sy, &mut sect, &secs, &walls);
    let _ = clipmove_xy(&mut sx, &mut sy, &mut sect, vx, vy, 4, &secs, &walls);
});

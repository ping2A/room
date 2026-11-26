//! Small Build helpers — `docs/build-engine-primitives/09-drag-helpers.md`.

use crate::geom::{SectorWalls, WallXY};
use crate::numeric::ksqrt;

/// Sector that owns wall index `w`, or `-1`.
///
/// Scan is `0..numsectors` in order (behaviour for ties — first match).
pub fn sectorofwall(w: i16, sectors: &[SectorWalls]) -> i16 {
    if w < 0 {
        return -1;
    }
    let wu = w as usize;
    for (i, s) in sectors.iter().enumerate() {
        let start = s.wallptr as usize;
        let end = start.saturating_add(s.wallnum as usize);
        if wu >= start && wu < end {
            return i as i16;
        }
    }
    -1
}

/// Previous wall in the point2 loop whose `point2 == w`, or `-1`.
pub fn lastwall(w: i16, walls: &[WallXY]) -> i16 {
    if w < 0 || w as usize >= walls.len() {
        return -1;
    }
    for (i, wall) in walls.iter().enumerate() {
        if wall.point2 == w {
            return i as i16;
        }
    }
    -1
}

/// Rotate `(x, y)` around `(xpivot, ypivot)` by Build angle `da` (0..2047).
///
/// Uses a coarse integer sin/cos table (256 entries × 8) until GRP
/// `TABLES.DAT` sintable is loaded. **Not demo-safe** — replace before D5.
pub fn rotatepoint(xpivot: i32, ypivot: i32, x: i32, y: i32, da: i16) -> (i32, i32) {
    let ang = (i32::from(da) & 2047) as usize;
    let (sin, cos) = approx_sin_cos(ang); // Q14 i64
    let dx = i64::from(x) - i64::from(xpivot);
    let dy = i64::from(y) - i64::from(ypivot);
    // sintable scale is 16384 in Build; we use the same.
    let rx = (dx * cos - dy * sin) >> 14;
    let ry = (dx * sin + dy * cos) >> 14;
    (
        xpivot.wrapping_add(rx as i32),
        ypivot.wrapping_add(ry as i32),
    )
}

/// Move wall vertex `w` to `(newx, newy)` and every wall that shares that
/// point (`point2` chain / coincident endpoints).
///
/// Bootstrap: updates `walls[w]` and any wall whose `(x,y)` matches the old
/// position, plus the wall that points at `w` via `point2` (its endpoint is
/// wall `w`'s start — already the same vertex). Returns how many walls moved.
pub fn dragpoint(walls: &mut [WallXY], w: i16, newx: i32, newy: i32) -> i32 {
    if w < 0 || w as usize >= walls.len() {
        return 0;
    }
    let wi = w as usize;
    let oldx = walls[wi].x;
    let oldy = walls[wi].y;
    if oldx == newx && oldy == newy {
        return 0;
    }
    let mut n = 0_i32;
    for wall in walls.iter_mut() {
        if wall.x == oldx && wall.y == oldy {
            wall.x = newx;
            wall.y = newy;
            n += 1;
        }
    }
    n
}

/// Integer length of `(x, y)`.
pub fn length2d(x: i32, y: i32) -> i32 {
    let s = i64::from(x) * i64::from(x) + i64::from(y) * i64::from(y);
    if s <= 0 {
        return 0;
    }
    if s > i64::from(i32::MAX) {
        ksqrt(i32::MAX)
    } else {
        ksqrt(s as i32)
    }
}

/// Install Chocolate `TABLES.DAT` `sintable[2048]` (little-endian i16).
///
/// After this, [`approx_sincos_q14`] / [`sincos_q14`] use the real table.
pub fn install_sintable(table: [i16; 2048]) {
    let _ = SINTABLE.set(table);
}

/// Parse the leading `sintable[2048]` from a `TABLES.DAT` blob (4096 bytes min).
#[must_use]
pub fn sintable_from_tables_dat(data: &[u8]) -> Option<[i16; 2048]> {
    if data.len() < 4096 {
        return None;
    }
    let mut t = [0i16; 2048];
    for (i, slot) in t.iter_mut().enumerate() {
        let o = i * 2;
        *slot = i16::from_le_bytes([data[o], data[o + 1]]);
    }
    Some(t)
}

/// Parse `radarang[640]` after `sintable` and mirror into `[1280]` (Chocolate `loadtables`).
#[must_use]
pub fn radarang_from_tables_dat(data: &[u8]) -> Option<[i16; 1280]> {
    const SIN_BYTES: usize = 4096;
    const RADAR_BYTES: usize = 640 * 2;
    if data.len() < SIN_BYTES + RADAR_BYTES {
        return None;
    }
    let mut t = [0i16; 1280];
    for (i, slot) in t.iter_mut().take(640).enumerate() {
        let o = SIN_BYTES + i * 2;
        *slot = i16::from_le_bytes([data[o], data[o + 1]]);
    }
    for i in 0..640 {
        t[1279 - i] = -t[i];
    }
    Some(t)
}

/// Parse Chocolate `TABLES.DAT`'s `britable[16][64]` after its sine, angle,
/// and two font tables (offset 7424, 1024 bytes).
#[must_use]
pub fn britable_from_tables_dat(data: &[u8]) -> Option<[[u8; 64]; 16]> {
    const BRITABLE_OFFSET: usize = 4096 + 1280 + 1024 + 1024;
    let bytes = data.get(BRITABLE_OFFSET..BRITABLE_OFFSET + 16 * 64)?;
    let mut table = [[0; 64]; 16];
    for (row, values) in table.iter_mut().enumerate() {
        values.copy_from_slice(&bytes[row * 64..(row + 1) * 64]);
    }
    Some(table)
}

/// Install Chocolate `radarang[1280]` for [`getangle`].
pub fn install_radarang(table: [i16; 1280]) {
    let _ = RADARANG.set(table);
}

static RADARANG: std::sync::OnceLock<[i16; 1280]> = std::sync::OnceLock::new();

/// Chocolate `getangle(xvect, yvect)` — Build angle in `0..2047`.
///
/// Uses installed [`radarang_from_tables_dat`] when present; otherwise a
/// coarse fallback (axis / 45° cases still exact).
#[must_use]
pub fn getangle(xvect: i32, yvect: i32) -> i32 {
    use crate::numeric::{klabs, scale};
    if (xvect | yvect) == 0 {
        return 0;
    }
    if xvect == 0 {
        return 512 + (i32::from(yvect < 0) << 10);
    }
    if yvect == 0 {
        return i32::from(xvect < 0) << 10;
    }
    if xvect == yvect {
        return 256 + (i32::from(xvect < 0) << 10);
    }
    if xvect == -yvect {
        return 768 + (i32::from(xvect > 0) << 10);
    }
    if let Some(rad) = RADARANG.get() {
        if klabs(xvect) > klabs(yvect) {
            let idx = (640 + scale(160, yvect, xvect)) as usize;
            let a = if idx < rad.len() {
                i32::from(rad[idx])
            } else {
                0
            };
            return ((a >> 6) + (i32::from(xvect < 0) << 10)) & 2047;
        }
        let idx = (640 - scale(160, xvect, yvect)) as usize;
        let a = if idx < rad.len() {
            i32::from(rad[idx])
        } else {
            0
        };
        return ((a >> 6) + 512 + (i32::from(yvect < 0) << 10)) & 2047;
    }
    // Fallback without TABLES.DAT: octant-ish atan via ratio (not DEMO-safe).
    let ax = klabs(xvect);
    let ay = klabs(yvect);
    let ang = if ax > ay {
        scale(256, ay, ax)
    } else {
        512 - scale(256, ax, ay)
    };
    if xvect >= 0 {
        if yvect >= 0 { ang } else { (2048 - ang) & 2047 }
    } else if yvect >= 0 {
        (1024 - ang) & 2047
    } else {
        (1024 + ang) & 2047
    }
}

/// Q14 sin/cos for Build angle `ang & 2047`. Uses [`install_sintable`] when set.
#[must_use]
pub fn sincos_q14(ang: i32) -> (i32, i32) {
    let a = (ang as usize) & 2047;
    if let Some(t) = SINTABLE.get() {
        let sin = i32::from(t[a]);
        let cos = i32::from(t[(a + 512) & 2047]);
        return (sin, cos);
    }
    approx_sincos_q14(ang)
}

/// Linear Q14 sin/cos over 2048 Build angles (bootstrap fallback without `TABLES.DAT`).
pub fn approx_sincos_q14(ang: i32) -> (i32, i32) {
    if let Some(t) = SINTABLE.get() {
        let a = (ang as usize) & 2047;
        return (i32::from(t[a]), i32::from(t[(a + 512) & 2047]));
    }
    let (s, c) = approx_sin_cos((ang as usize) & 2047);
    (s as i32, c as i32)
}

/// Q14 sine/cosine for a 16.16 Build angle. Adjacent integer table entries are
/// interpolated with integer arithmetic so EDuke32 camera yaw keeps its
/// fractional part without introducing renderer platform differences.
#[must_use]
pub fn sincos_q14_16(ang_q16: i32) -> (i32, i32) {
    let wrapped = ang_q16.rem_euclid(2048_i32 << 16);
    let base = wrapped >> 16;
    let fraction = i64::from(wrapped & 0xFFFF);
    if fraction == 0 {
        return sincos_q14(base);
    }
    let (sin0, cos0) = sincos_q14(base);
    let (sin1, cos1) = sincos_q14(base + 1);
    let inverse = 65_536 - fraction;
    let interpolate =
        |a: i32, b: i32| ((i64::from(a) * inverse + i64::from(b) * fraction) >> 16) as i32;
    (interpolate(sin0, sin1), interpolate(cos0, cos1))
}

static SINTABLE: std::sync::OnceLock<[i16; 2048]> = std::sync::OnceLock::new();

fn approx_sin_cos(ang: usize) -> (i64, i64) {
    let a = ang & 2047;
    let q = a / 512;
    let r = (a % 512) as i64;
    let s = (r * 16384) / 512;
    let c = 16384 - s;
    match q {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::WallXY;

    #[test]
    fn sectorofwall_ranges() {
        let sectors = [
            SectorWalls {
                wallptr: 0,
                wallnum: 4,
            },
            SectorWalls {
                wallptr: 4,
                wallnum: 4,
            },
        ];
        assert_eq!(sectorofwall(0, &sectors), 0);
        assert_eq!(sectorofwall(3, &sectors), 0);
        assert_eq!(sectorofwall(4, &sectors), 1);
        assert_eq!(sectorofwall(99, &sectors), -1);
    }

    #[test]
    fn lastwall_loop() {
        let walls = [
            WallXY {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 1,
                y: 0,
                point2: 2,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 1,
                y: 1,
                point2: 3,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 0,
                y: 1,
                point2: 0,
                nextsector: -1,
                cstat: 0,
            },
        ];
        assert_eq!(lastwall(0, &walls), 3);
        assert_eq!(lastwall(2, &walls), 1);
    }

    #[test]
    fn rotatepoint_90_deg() {
        // ang 512 = 90°; (1,0) around origin → ~(0,1)
        let (x, y) = rotatepoint(0, 0, 1000, 0, 512);
        assert!(x.abs() < 30, "x={x}");
        assert!((y - 1000).abs() < 30, "y={y}");
    }

    #[test]
    fn length2d_3_4_5() {
        assert_eq!(length2d(3, 4), 5);
    }

    #[test]
    fn dragpoint_moves_shared_vertex() {
        // Square: wall0 start = wall3's point2 target conceptually share (0,0).
        let mut walls = [
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
        let n = dragpoint(&mut walls, 0, -10, -10);
        assert_eq!(n, 1);
        assert_eq!((walls[0].x, walls[0].y), (-10, -10));
        // Only wall 0 had that start; point2 links don't duplicate coords.
        assert_eq!((walls[1].x, walls[1].y), (100, 0));
    }

    #[test]
    fn dragpoint_moves_all_coincident_starts() {
        let mut walls = [
            WallXY {
                x: 5,
                y: 5,
                point2: 1,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 5,
                y: 5,
                point2: 0,
                nextsector: -1,
                cstat: 0,
            },
        ];
        assert_eq!(dragpoint(&mut walls, 0, 9, 9), 2);
        assert_eq!((walls[0].x, walls[0].y), (9, 9));
        assert_eq!((walls[1].x, walls[1].y), (9, 9));
    }
}

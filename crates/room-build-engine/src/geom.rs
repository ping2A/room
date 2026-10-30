//! Geometry primitives: `inside`, `updatesector`, `getzsofslope`.
//! Specs: `docs/build-engine-primitives/02-inside.md`, `03-updatesector.md`,
//! `04-getzsofslope.md`.

use crate::helpers::{approx_sincos_q14, getangle, sincos_q14};
use crate::numeric::{divscale, dmulscale, klabs, mulscale, nsqrtasm, scale};
use crate::world::{BuildWorld, MAXSECTORS};

fn ksgn(x: i32) -> i32 {
    if x > 0 {
        1
    } else if x < 0 {
        -1
    } else {
        0
    }
}

/// Minimal wall vertex for point-in-sector (x/y + `point2` index).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallXY {
    pub x: i32,
    pub y: i32,
    pub point2: i16,
    pub nextsector: i16,
    /// Build wall `cstat` (bit 0 = blocking) — used by [`getzrange`] portals.
    pub cstat: u16,
}

/// Sector wall range into a shared wall array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectorWalls {
    pub wallptr: i16,
    pub wallnum: i16,
}

/// Sector heights + slope bits for [`getzsofslope`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectorZ {
    pub wallptr: i16,
    pub wallnum: i16,
    pub ceilingz: i32,
    pub floorz: i32,
    /// Build `ceilingstat`; bit 2 = sloped.
    pub ceilingstat: u16,
    /// Build `floorstat`; bit 2 = sloped.
    pub floorstat: u16,
    pub ceilingheinum: i16,
    pub floorheinum: i16,
}

impl SectorZ {
    pub fn walls(self) -> SectorWalls {
        SectorWalls {
            wallptr: self.wallptr,
            wallnum: self.wallnum,
        }
    }
}

/// Even-odd point-in-sector.
///
/// Edge rule (draft, lock via goldens): a ray to +∞ on X counts a crossing
/// only when the edge straddles `y` with the upper endpoint exclusive
/// (`y1 <= y < y2` or `y2 <= y < y1`), matching common Build-style tests.
/// Points exactly on a horizontal edge are not treated as crossings.
/// Point-in-sector using Build's XOR / sign-bit test (not classical even-odd).
///
/// Matches Chocolate / Ken `inside`: for each wall whose Y-span straddles `y`,
/// XOR a side discriminator into an accumulator; the high bit is the result.
/// Spec: `docs/build-engine-primitives/02-inside-updatesector.md`.
pub fn inside(x: i32, y: i32, sectnum: i16, sectors: &[SectorWalls], walls: &[WallXY]) -> bool {
    if sectnum < 0 || sectnum as usize >= sectors.len() {
        return false;
    }
    let sec = sectors[sectnum as usize];
    let start = sec.wallptr as usize;
    let end = start.saturating_add(sec.wallnum as usize);
    if sec.wallnum <= 0 || end > walls.len() || start >= walls.len() {
        return false;
    }

    let mut wall_crossed: u32 = 0;
    for w in start..end {
        let a = walls[w];
        let p2 = a.point2 as usize;
        if p2 >= walls.len() {
            break;
        }
        let b = walls[p2];
        let y1 = a.y.wrapping_sub(y);
        let y2 = b.y.wrapping_sub(y);
        // Different sign bits ⇒ horizontal ray at `y` straddles the edge.
        if (y1 ^ y2) < 0 {
            let x1 = a.x.wrapping_sub(x);
            let x2 = b.x.wrapping_sub(x);
            if (x1 ^ x2) >= 0 {
                wall_crossed ^= x1 as u32;
            } else {
                // Cross product vs y2 picks the side when x straddles the edge.
                let cross = x1.wrapping_mul(y2).wrapping_sub(x2.wrapping_mul(y1));
                wall_crossed ^= (cross ^ y2) as u32;
            }
        }
    }
    (wall_crossed >> 31) != 0
}

/// Ray to +X from `(x,y)` crosses segment `(x1,y1)-(x2,y2)`?
///
/// Used by unit tests / helpers; sector membership uses [`inside`].
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn edge_crosses_ray(x: i32, y: i32, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
    let straddles = (y1 <= y && y < y2) || (y2 <= y && y < y1);
    if !straddles {
        return false;
    }
    let dy = i64::from(y2) - i64::from(y1);
    if dy == 0 {
        return false;
    }
    let x_int =
        i64::from(x1) + ((i64::from(y) - i64::from(y1)) * (i64::from(x2) - i64::from(x1))) / dy;
    x_int > i64::from(x)
}

/// Find sector containing `(x,y)`, updating `sect` hint in place.
///
/// Search: hint → neighbours of hint (by `nextsector` on its walls) → all.
/// Sets `*sect = -1` if none contain the point.
pub fn updatesector(
    x: i32,
    y: i32,
    sect: &mut i16,
    sectors: &[SectorWalls],
    walls: &[WallXY],
) -> i16 {
    let n = sectors.len() as i16;
    if n <= 0 {
        *sect = -1;
        return -1;
    }

    if *sect >= 0 && (*sect as usize) < sectors.len() && inside(x, y, *sect, sectors, walls) {
        return *sect;
    }

    // Neighbours of the (possibly invalid) hint: scan its walls' nextsector.
    if *sect >= 0 && (*sect as usize) < sectors.len() {
        let sec = sectors[*sect as usize];
        let start = sec.wallptr as usize;
        let end = start.saturating_add(sec.wallnum as usize).min(walls.len());
        for w in walls.iter().take(end).skip(start) {
            let ns = w.nextsector;
            if ns >= 0
                && (ns as usize) < sectors.len()
                && ns != *sect
                && inside(x, y, ns, sectors, walls)
            {
                *sect = ns;
                return ns;
            }
        }
    }

    for i in 0..n {
        if inside(x, y, i, sectors, walls) {
            *sect = i;
            return i;
        }
    }
    *sect = -1;
    -1
}

/// Ceiling and floor Build Z at `(x, y)` in sector `sectnum`.
///
/// Flat when the slope bit is clear or `heinum == 0`. Sloped planes match
/// Chocolate `getzsofslope`: hinge = first wall; `i = nsqrtasm(dx²+dy²)<<5`;
/// `j = dmulscale3(dx,y−wy,−dy,x−wx)`; `Δz = scale(heinum, j, i)`.
pub fn getzsofslope(
    sectnum: i16,
    x: i32,
    y: i32,
    sectors: &[SectorZ],
    walls: &[WallXY],
) -> (i32, i32) {
    if sectnum < 0 || sectnum as usize >= sectors.len() {
        return (0, 0);
    }
    let sec = sectors[sectnum as usize];
    let ceilz = plane_z(
        x,
        y,
        sec.ceilingz,
        sec.ceilingstat,
        sec.ceilingheinum,
        sec.wallptr,
        walls,
    );
    let florz = plane_z(
        x,
        y,
        sec.floorz,
        sec.floorstat,
        sec.floorheinum,
        sec.wallptr,
        walls,
    );
    (ceilz, florz)
}

fn plane_z(
    x: i32,
    y: i32,
    basez: i32,
    stat: u16,
    heinum: i16,
    wallptr: i16,
    walls: &[WallXY],
) -> i32 {
    if stat & 2 == 0 || heinum == 0 {
        return basez;
    }
    let w0 = wallptr as usize;
    if w0 >= walls.len() {
        return basez;
    }
    let a = walls[w0];
    let w1 = a.point2 as usize;
    if w1 >= walls.len() {
        return basez;
    }
    let b = walls[w1];
    let dx = b.x.wrapping_sub(a.x);
    let dy = b.y.wrapping_sub(a.y);
    let len_sq = (i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)).max(0) as u32;
    if len_sq == 0 {
        return basez;
    }
    // Chocolate: i = nsqrtasm(dx*dx+dy*dy)<<5; j = dmulscale3(...); scale(heinum,j,i).
    let i = nsqrtasm(len_sq).wrapping_shl(5);
    if i == 0 {
        return basez;
    }
    let j = dmulscale(dx, y.wrapping_sub(a.y), -dy, x.wrapping_sub(a.x), 3);
    let delta = scale(i32::from(heinum), j, i);
    basez.wrapping_add(delta)
}

/// Result of [`getzrange`].
///
/// Hit codes match Chocolate Build: sector → `sectnum + 16384`, sprite →
/// `sprite_index + 49152`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZRange {
    pub ceilz: i32,
    pub ceilhit: i32,
    pub florz: i32,
    pub florhit: i32,
}

/// Blocking sprite inputs for [`getzrange`] (face + floor; wall-aligned later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteZClip {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub cstat: u16,
    pub clipdist: u8,
    pub xrepeat: u8,
    pub yrepeat: u8,
    pub xoffset: i8,
    pub yoffset: i8,
    pub ang: i16,
    pub sectnum: i16,
    /// ART `xsize` / `ysize` (0 height skips the sprite).
    pub tile_width: i16,
    pub tile_height: i16,
    /// ART `picanm` offsets (bits 8–15 / 16–23).
    pub picanm_xoff: i8,
    pub picanm_yoff: i8,
}

/// Result of [`hitscan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HitScan {
    pub hitsect: i16,
    pub hitwall: i16,
    pub hitsprite: i16,
    pub hitx: i32,
    pub hity: i32,
    pub hitz: i32,
}

/// Sprite stand-in for hitscan.
///
/// - **Cylinder** (`face_span == 0`): XY radius [`clipdist`] (bootstrap / clipmove).
/// - **Face** (`face_span != 0`): Chocolate `hitscan` `cstat&48==0` — vertical
///   billboard; reject if perp-dist² > `(face_span)²>>7`; hit at closest point
///   on the ray; Z in `[z - face_height, z]` (bit 128 centers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteHit {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub sectnum: i16,
    /// Cylinder radius in XY (from `clipdist<<2`). Ignored when [`face_span`] ≠ 0.
    pub clipdist: i32,
    /// `cstat` bit 8 (256) = hitscan-sensitive; bootstrap requires it set to hit.
    pub cstat: u16,
    /// Build face-sprite: `tiles[].width * xrepeat`. `0` → cylinder mode.
    pub face_span: i32,
    /// Build face-sprite: `tiles[].height * yrepeat << 2`.
    pub face_height: i32,
}

impl SpriteHit {
    /// Cylinder-only hit (existing callers / goldens).
    #[must_use]
    pub const fn cylinder(x: i32, y: i32, z: i32, sectnum: i16, clipdist: i32, cstat: u16) -> Self {
        Self {
            x,
            y,
            z,
            sectnum,
            clipdist,
            cstat,
            face_span: 0,
            face_height: 0,
        }
    }
}

impl HitScan {
    pub const MISS: Self = Self {
        hitsect: -1,
        hitwall: -1,
        hitsprite: -1,
        hitx: 0,
        hity: 0,
        hitz: 0,
    };
}

/// Ray vs solid walls (portal BFS) and face sprites (clipdist cylinders).
///
/// Crossing a portal (`nextsector >= 0`) continues the search in that sector.
/// Sprites need `cstat & 256` (hitscan bit). Slope planes still omitted.
/// See `docs/build-engine-primitives/07-hitscan.md`.
pub fn hitscan(
    x: i32,
    y: i32,
    z: i32,
    sectnum: i16,
    vx: i32,
    vy: i32,
    vz: i32,
    _clipmask: u32,
    sectors: &[SectorWalls],
    walls: &[WallXY],
) -> HitScan {
    hitscan_with_sprites(x, y, z, sectnum, vx, vy, vz, _clipmask, sectors, walls, &[])
}

/// Like [`hitscan`] but also tests `sprites` in visited sectors.
pub fn hitscan_with_sprites(
    x: i32,
    y: i32,
    z: i32,
    sectnum: i16,
    vx: i32,
    vy: i32,
    vz: i32,
    clipmask: u32,
    sectors: &[SectorWalls],
    walls: &[WallXY],
    sprites: &[SpriteHit],
) -> HitScan {
    hitscan_full(
        x, y, z, sectnum, vx, vy, vz, clipmask, sectors, walls, sprites, None,
    )
}

/// Full hitscan: walls, portals, face sprites, and optional sector Z planes.
pub fn hitscan_full(
    x: i32,
    y: i32,
    z: i32,
    sectnum: i16,
    vx: i32,
    vy: i32,
    vz: i32,
    clipmask: u32,
    sectors: &[SectorWalls],
    walls: &[WallXY],
    sprites: &[SpriteHit],
    sector_z: Option<&[SectorZ]>,
) -> HitScan {
    if sectnum < 0 || sectnum as usize >= sectors.len() || (vx == 0 && vy == 0 && vz == 0) {
        return HitScan::MISS;
    }
    // Need XY motion for wall/sprite; pure vertical still hits floors/ceilings.
    let has_xy = vx != 0 || vy != 0;
    // Chocolate: `dawalclipmask = cliptype&65535`, `dasprclipmask = cliptype>>16`.
    let dawalclipmask = (clipmask & 0xffff) as u16;
    let dasprclipmask = ((clipmask >> 16) & 0xffff) as u16;

    // Chocolate compares hits by XY manhattan distance (not ray parameter `t`).
    // Walls/planes: reject if `manh >= best`; sprites: reject if `manh > best`
    // (equal manh prefers sprite when tested after walls).
    let mut best_manh = i32::MAX;
    let mut got = false;
    let mut best_w: i16 = -1;
    let mut best_sp: i16 = -1;
    let mut best_sect = sectnum;
    let mut best_xy = (x, y);
    let mut best_z = z;

    let mut visited = [false; crate::world::MAXSECTORS];
    let mut queue = [0_i16; crate::world::MAXSECTORS];
    let mut qh = 0_usize;
    let mut qt = 0_usize;
    queue[qt] = sectnum;
    qt += 1;
    visited[sectnum as usize] = true;

    while qh < qt {
        let s = queue[qh];
        qh += 1;
        let sec = sectors[s as usize];
        let start = sec.wallptr as usize;
        let end = start.saturating_add(sec.wallnum as usize).min(walls.len());

        // Chocolate order per sector: ceiling/floor, then walls, then sprites.
        if let Some(sz) = sector_z {
            if (s as usize) < sz.len() {
                if let Some((_t, hx, hy, hz)) =
                    ray_sector_planes(x, y, z, vx, vy, vz, s, sz, walls, sectors)
                {
                    let manh = klabs(hx.wrapping_sub(x)) + klabs(hy.wrapping_sub(y));
                    if manh < best_manh {
                        best_manh = manh;
                        got = true;
                        best_w = -1;
                        best_sp = -1;
                        best_sect = s;
                        best_xy = (hx, hy);
                        best_z = hz;
                    }
                }
            }
        }

        if has_xy {
            for wi in start..end {
                let a = walls[wi];
                let p2 = a.point2 as usize;
                if p2 >= walls.len() {
                    continue;
                }
                let b = walls[p2];
                // Chocolate back-face reject before `rintersect`.
                let x1 = a.x;
                let y1 = a.y;
                let x2 = b.x;
                let y2 = b.y;
                if (i64::from(x1) - i64::from(x)) * (i64::from(y2) - i64::from(y))
                    < (i64::from(x2) - i64::from(x)) * (i64::from(y1) - i64::from(y))
                {
                    continue;
                }
                let Some((t, hx, hy)) = ray_seg_intersect(x, y, vx, vy, x1, y1, x2, y2) else {
                    continue;
                };
                if t <= 0 {
                    continue;
                }
                let manh = klabs(hx.wrapping_sub(x)) + klabs(hy.wrapping_sub(y));
                if manh >= best_manh {
                    continue;
                }
                let hz = z.wrapping_add(((i64::from(vz) * t) >> 16) as i32);
                // Chocolate: solid wall, or `cstat&dawalclipmask`, or portal Z doesn't fit.
                let mut hit_wall = a.nextsector < 0 || (a.cstat & dawalclipmask) != 0;
                if !hit_wall {
                    if let Some(sz) = sector_z {
                        if (a.nextsector as usize) < sz.len() {
                            let (cz, fz) = getzsofslope(a.nextsector, hx, hy, sz, walls);
                            if hz <= cz || hz >= fz {
                                hit_wall = true;
                            }
                        }
                    }
                }
                if hit_wall {
                    best_manh = manh;
                    got = true;
                    best_w = wi as i16;
                    best_sp = -1;
                    best_sect = s;
                    best_xy = (hx, hy);
                    best_z = hz;
                    continue;
                }
                let ns = a.nextsector as usize;
                if ns < sectors.len() && !visited[ns] {
                    visited[ns] = true;
                    if qt < queue.len() {
                        queue[qt] = a.nextsector;
                        qt += 1;
                    }
                }
            }
            for (si, sp) in sprites.iter().enumerate() {
                // Chocolate: `(cstat&dasprclipmask)==0` → skip. Bootstrap tests pass
                // `clipmask=0` but still expect face-sprite bit 256 hits.
                let spr_clip_ok = if dasprclipmask == 0 {
                    sp.cstat & 256 != 0
                } else {
                    sp.cstat & dasprclipmask != 0
                };
                if sp.sectnum != s || !spr_clip_ok {
                    continue;
                }
                // Face sprites only when `cstat&48==0` (Chocolate hitscan case 0).
                let cstat_type = sp.cstat & 48;
                if sp.face_span != 0 && cstat_type == 0 {
                    if let Some((_t, hx, hy, hz)) = ray_face_sprite(x, y, z, vx, vy, vz, sp) {
                        let manh = klabs(hx.wrapping_sub(x)) + klabs(hy.wrapping_sub(y));
                        if manh <= best_manh {
                            best_manh = manh;
                            got = true;
                            best_w = -1;
                            best_sp = si as i16;
                            best_sect = s;
                            best_xy = (hx, hy);
                            best_z = hz;
                        }
                    }
                } else if sp.clipdist > 0 && cstat_type == 0 {
                    if let Some((t, hx, hy)) = ray_circle_xy(x, y, vx, vy, sp.x, sp.y, sp.clipdist)
                    {
                        let manh = klabs(hx.wrapping_sub(x)) + klabs(hy.wrapping_sub(y));
                        if manh <= best_manh {
                            best_manh = manh;
                            got = true;
                            best_w = -1;
                            best_sp = si as i16;
                            best_sect = s;
                            best_xy = (hx, hy);
                            best_z = z.wrapping_add(((i64::from(vz) * t) >> 16) as i32);
                        }
                    }
                }
            }
        }
    }

    if !got {
        return HitScan::MISS;
    }
    HitScan {
        hitsect: best_sect,
        hitwall: best_w,
        hitsprite: best_sp,
        hitx: best_xy.0,
        hity: best_xy.1,
        hitz: best_z,
    }
}

/// Intersect ray with floor and ceiling of sector `s`; return nearer hit in sector.
///
/// Matches Chocolate `hitscan` plane tests: sloped `*stat&2` via heinum (works with
/// `vz==0`); flat floors only when `vz>0` / ceilings when `vz<0`.
fn ray_sector_planes(
    xs: i32,
    ys: i32,
    zs: i32,
    vx: i32,
    vy: i32,
    vz: i32,
    s: i16,
    sectors: &[SectorZ],
    walls: &[WallXY],
    wall_secs: &[SectorWalls],
) -> Option<(i64, i32, i32, i32)> {
    let su = s as usize;
    if su >= sectors.len() {
        return None;
    }
    let sec = sectors[su];
    let sw = wall_secs.get(su).copied().unwrap_or(sec.walls());
    let wp = sw.wallptr as usize;
    if wp >= walls.len() {
        return None;
    }
    let wal = walls[wp];
    let p2 = wal.point2 as usize;
    if p2 >= walls.len() {
        return None;
    }
    let wal2 = walls[p2];
    let dax0 = wal2.x.wrapping_sub(wal.x);
    let day0 = wal2.y.wrapping_sub(wal.y);
    let wall_len =
        nsqrtasm((i64::from(dax0) * i64::from(dax0) + i64::from(day0) * i64::from(day0)) as u32);
    let mut best_manh = i32::MAX;
    let mut best: Option<(i64, i32, i32, i32)> = None;

    let mut consider = |hx: i32, hy: i32, hz: i32| {
        if inside(hx, hy, s, wall_secs, walls) == false {
            return;
        }
        let manh = klabs(hx.wrapping_sub(xs)) + klabs(hy.wrapping_sub(ys));
        if manh >= best_manh {
            return;
        }
        // Parametric t in Q16 along (vx,vy,vz); prefer manhattan order like Chocolate.
        let t_q16 = if vx != 0 {
            ((i64::from(hx) - i64::from(xs)) << 16) / i64::from(vx)
        } else if vy != 0 {
            ((i64::from(hy) - i64::from(ys)) << 16) / i64::from(vy)
        } else if vz != 0 {
            ((i64::from(hz) - i64::from(zs)) << 16) / i64::from(vz)
        } else {
            return;
        };
        if t_q16 <= 0 {
            return;
        }
        best_manh = manh;
        best = Some((t_q16, hx, hy, hz));
    };

    // Ceiling
    if sec.ceilingstat & 2 != 0 && wall_len != 0 {
        let mut dax = dax0;
        let mut day = day0;
        let i = divscale(i32::from(sec.ceilingheinum), wall_len, 15);
        dax = dax.wrapping_mul(i);
        day = day.wrapping_mul(i);
        let j = (vz << 8).wrapping_sub(dmulscale(dax, vy, -day, vx, 15));
        if j != 0 {
            let i = ((sec.ceilingz.wrapping_sub(zs)) << 8).wrapping_add(dmulscale(
                dax,
                ys.wrapping_sub(wal.y),
                -day,
                xs.wrapping_sub(wal.x),
                15,
            ));
            if (i ^ j) >= 0 && (klabs(i) >> 1) < klabs(j) {
                let t = divscale(i, j, 30);
                consider(
                    xs.wrapping_add(mulscale(vx, t, 30)),
                    ys.wrapping_add(mulscale(vy, t, 30)),
                    zs.wrapping_add(mulscale(vz, t, 30)),
                );
            }
        }
    } else if vz < 0 && zs >= sec.ceilingz {
        let i = sec.ceilingz.wrapping_sub(zs);
        if (klabs(i) >> 1) < -vz {
            let t = divscale(i, vz, 30);
            consider(
                xs.wrapping_add(mulscale(vx, t, 30)),
                ys.wrapping_add(mulscale(vy, t, 30)),
                sec.ceilingz,
            );
        }
    }

    // Floor
    if sec.floorstat & 2 != 0 && wall_len != 0 {
        let mut dax = dax0;
        let mut day = day0;
        let i = divscale(i32::from(sec.floorheinum), wall_len, 15);
        dax = dax.wrapping_mul(i);
        day = day.wrapping_mul(i);
        let j = (vz << 8).wrapping_sub(dmulscale(dax, vy, -day, vx, 15));
        if j != 0 {
            let i = ((sec.floorz.wrapping_sub(zs)) << 8).wrapping_add(dmulscale(
                dax,
                ys.wrapping_sub(wal.y),
                -day,
                xs.wrapping_sub(wal.x),
                15,
            ));
            if (i ^ j) >= 0 && (klabs(i) >> 1) < klabs(j) {
                let t = divscale(i, j, 30);
                consider(
                    xs.wrapping_add(mulscale(vx, t, 30)),
                    ys.wrapping_add(mulscale(vy, t, 30)),
                    zs.wrapping_add(mulscale(vz, t, 30)),
                );
            }
        }
    } else if vz > 0 && zs <= sec.floorz {
        let i = sec.floorz.wrapping_sub(zs);
        if (klabs(i) >> 1) < vz {
            let t = divscale(i, vz, 30);
            consider(
                xs.wrapping_add(mulscale(vx, t, 30)),
                ys.wrapping_add(mulscale(vy, t, 30)),
                sec.floorz,
            );
        }
    }

    best
}

/// Chocolate `hitscan` face sprite (`cstat&48==0`).
///
/// Intermediate `topt`/`bot`/`topu` use **i32 wrapping** like Chocolate/Build
/// (not i64) — overflow matters for distant rays.
fn ray_face_sprite(
    xs: i32,
    ys: i32,
    zs: i32,
    vx: i32,
    vy: i32,
    vz: i32,
    sp: &SpriteHit,
) -> Option<(i64, i32, i32, i32)> {
    use crate::numeric::scale;
    let x1 = sp.x;
    let y1 = sp.y;
    let mut z1 = sp.z;
    // Chocolate: `topt = vx*(x1-xs) + vy*(y1-ys)` in 32-bit.
    let topt = vx
        .wrapping_mul(x1.wrapping_sub(xs))
        .wrapping_add(vy.wrapping_mul(y1.wrapping_sub(ys)));
    if topt <= 0 {
        return None;
    }
    let bot = vx.wrapping_mul(vx).wrapping_add(vy.wrapping_mul(vy));
    if bot == 0 {
        return None;
    }
    let intz = zs.wrapping_add(scale(vz, topt, bot));
    let h = sp.face_height;
    if sp.cstat & 128 != 0 {
        z1 = z1.wrapping_add(h >> 1);
    }
    if intz > z1 || intz < z1.wrapping_sub(h) {
        return None;
    }
    let topu = vx
        .wrapping_mul(y1.wrapping_sub(ys))
        .wrapping_sub(vy.wrapping_mul(x1.wrapping_sub(xs)));
    let offx = scale(vx, topu, bot);
    let offy = scale(vy, topu, bot);
    let dist = i64::from(offx) * i64::from(offx) + i64::from(offy) * i64::from(offy);
    let span2 = i64::from(sp.face_span) * i64::from(sp.face_span);
    if dist > (span2 >> 7) {
        return None;
    }
    let intx = xs.wrapping_add(scale(vx, topt, bot));
    let inty = ys.wrapping_add(scale(vy, topt, bot));
    // Parametric t for callers; Chocolate compares manhattan, not t.
    let t_q16 = (i64::from(topt) << 16) / i64::from(bot);
    if t_q16 <= 0 {
        return None;
    }
    Some((t_q16, intx, inty, intz))
}

fn ray_circle_xy(
    ox: i32,
    oy: i32,
    vx: i32,
    vy: i32,
    cx: i32,
    cy: i32,
    r: i32,
) -> Option<(i64, i32, i32)> {
    let fx = i64::from(ox) - i64::from(cx);
    let fy = i64::from(oy) - i64::from(cy);
    let a = i64::from(vx) * i64::from(vx) + i64::from(vy) * i64::from(vy);
    if a == 0 {
        return None;
    }
    let b = 2 * (fx * i64::from(vx) + fy * i64::from(vy));
    let c = fx * fx + fy * fy - i64::from(r) * i64::from(r);
    let disc = b * b - 4 * a * c;
    if disc < 0 {
        return None;
    }
    let sqrt_disc = {
        let d = disc;
        // integer sqrt of disc
        let mut x = d;
        let mut y = (x + 1) >> 1;
        while y < x {
            x = y;
            y = (x + d / x) >> 1;
        }
        x
    };
    // t = (-b - sqrt) / (2a)  (nearer hit), as Q16
    let t_num = -b - sqrt_disc;
    if t_num <= 0 {
        // try far root if origin inside
        let t_num2 = -b + sqrt_disc;
        if t_num2 <= 0 {
            return None;
        }
        let t_q16 = (t_num2 << 16) / (2 * a);
        let hx = ox.wrapping_add(((i64::from(vx) * t_q16) >> 16) as i32);
        let hy = oy.wrapping_add(((i64::from(vy) * t_q16) >> 16) as i32);
        return Some((t_q16, hx, hy));
    }
    let t_q16 = (t_num << 16) / (2 * a);
    if t_q16 <= 0 {
        return None;
    }
    let hx = ox.wrapping_add(((i64::from(vx) * t_q16) >> 16) as i32);
    let hy = oy.wrapping_add(((i64::from(vy) * t_q16) >> 16) as i32);
    Some((t_q16, hx, hy))
}

/// Hit code: 0 = clear; `0x8000 | wall` = solid wall (Chocolate `clipobjectval`).
pub const HIT_WALL_BOOTSTRAP: i32 = 0x8000;

/// Minimal `clipmove`: move in XY against solid walls in the start sector.
///
/// Up to **3** passes: on a hit within the remaining vector, stop
/// `walldist + 1` short (Chocolate `keepaway` clearance) and project the
/// residual onto the wall tangent (slide). Ignores Z / sprites / portals /
/// `cliptype`. See `05-clipmove.md`. Hit codes use `0x8000 | wall`.
pub fn clipmove_xy(
    x: &mut i32,
    y: &mut i32,
    sect: &mut i16,
    xvect: i32,
    yvect: i32,
    walldist: i32,
    sectors: &[SectorWalls],
    walls: &[WallXY],
) -> i32 {
    clipmove_xy_at_z(
        x, y, 0, sect, xvect, yvect, walldist, 0, 0, sectors, walls, None,
    )
}

/// Chocolate `clipmove` (walls / portal Z-gates; sprites deferred).
///
/// `xvect`/`yvect` are **world-unit** deltas (caller shifts `posxv>>14`). Builds
/// walldist-thickened clip lines, `raytrace`s, slides, `keepaway`s — 3 passes.
pub fn clipmove_xy_at_z(
    x: &mut i32,
    y: &mut i32,
    z: i32,
    sect: &mut i16,
    xvect: i32,
    yvect: i32,
    walldist: i32,
    ceildist: i32,
    flordist: i32,
    sectors: &[SectorWalls],
    walls: &[WallXY],
    sector_z: Option<&[SectorZ]>,
) -> i32 {
    clipmove_xy_at_z_sprites(
        x,
        y,
        z,
        sect,
        xvect,
        yvect,
        walldist,
        ceildist,
        flordist,
        sectors,
        walls,
        sector_z,
        &[],
        0,
    )
}

/// Chocolate `clipmove` with face-sprite L-clips (`CLIPMASK0` / `dasprclipmask`).
///
/// `xvect`/`yvect` are **world-unit** deltas (caller shifts `posxv>>14` /
/// movesprite `(xchange*TIC)>>3`). Face sprites (`cstat&48==0`) add the Build L:
/// vertical at `x±(clipdist<<2+walldist)` (sign from `gx`) and horizontal at
/// `y±…` (sign from `gy`). Hit codes `49152+i`.
pub fn clipmove_xy_at_z_sprites(
    x: &mut i32,
    y: &mut i32,
    z: i32,
    sect: &mut i16,
    xvect: i32,
    yvect: i32,
    walldist: i32,
    ceildist: i32,
    flordist: i32,
    sectors: &[SectorWalls],
    walls: &[WallXY],
    sector_z: Option<&[SectorZ]>,
    sprites: &[SpriteZClip],
    dasprclipmask: u16,
) -> i32 {
    if (xvect | yvect) == 0 || *sect < 0 {
        return 0;
    }
    // World-unit API: scale to Chocolate <<14 for `dmulscale6` corner tests only.
    let oxvect = xvect << 14;
    let oyvect = yvect << 14;
    let mut goalx = x.wrapping_add(xvect);
    let mut goaly = y.wrapping_add(yvect);
    let gx = goalx.wrapping_sub(*x);
    let gy = goaly.wrapping_sub(*y);
    // Chocolate `MAXCLIPDIST` is 1024.
    let rad = nsqrtasm((gx.wrapping_mul(gx).wrapping_add(gy.wrapping_mul(gy))) as u32)
        + 1024
        + walldist
        + 8;
    let cx = (x.wrapping_add(goalx)) >> 1;
    let cy = (y.wrapping_add(goaly)) >> 1;
    let xmin = cx.wrapping_sub(rad);
    let ymin = cy.wrapping_sub(rad);
    let xmax = cx.wrapping_add(rad);
    let ymax = cy.wrapping_add(rad);
    let dawalclipmask: u16 = 1;

    let mut clipit: Vec<(i32, i32, i32, i32)> = Vec::with_capacity(256);
    let mut clipobjectval: Vec<i32> = Vec::with_capacity(256);
    let mut clipsectorlist = vec![0i16; MAXSECTORS];
    clipsectorlist[0] = *sect;
    let mut clipsectcnt = 0_usize;
    let mut clipsectnum = 1_usize;

    while clipsectcnt < clipsectnum {
        let dasect = clipsectorlist[clipsectcnt];
        clipsectcnt += 1;
        if dasect < 0 || dasect as usize >= sectors.len() {
            continue;
        }
        let sec = sectors[dasect as usize];
        let startwall = sec.wallptr as i32;
        let endwall = startwall + i32::from(sec.wallnum);
        let mut j = startwall;
        while j < endwall {
            if j < 0 || j as usize >= walls.len() {
                j += 1;
                continue;
            }
            let wal = walls[j as usize];
            let p2 = wal.point2 as usize;
            if p2 >= walls.len() {
                j += 1;
                continue;
            }
            let wal2 = walls[p2];
            let x1 = wal.x;
            let y1 = wal.y;
            let x2 = wal2.x;
            let y2 = wal2.y;
            if (x1 < xmin && x2 < xmin)
                || (x1 > xmax && x2 > xmax)
                || (y1 < ymin && y2 < ymin)
                || (y1 > ymax && y2 > ymax)
            {
                j += 1;
                continue;
            }
            let dx = x2.wrapping_sub(x1);
            let dy = y2.wrapping_sub(y1);
            if dx.wrapping_mul(y.wrapping_sub(y1)) < (x.wrapping_sub(x1)).wrapping_mul(dy) {
                j += 1;
                continue;
            }
            let dax = if dx > 0 {
                dx.wrapping_mul(ymin.wrapping_sub(y1))
            } else {
                dx.wrapping_mul(ymax.wrapping_sub(y1))
            };
            let day = if dy > 0 {
                dy.wrapping_mul(xmax.wrapping_sub(x1))
            } else {
                dy.wrapping_mul(xmin.wrapping_sub(x1))
            };
            if dax >= day {
                j += 1;
                continue;
            }

            let mut clipyou = wal.nextsector < 0 || (wal.cstat & dawalclipmask) != 0;
            if !clipyou && wal.nextsector >= 0 {
                if let Some(sz) = sector_z {
                    let ns = wal.nextsector;
                    if (ns as usize) < sz.len() && (dasect as usize) < sz.len() {
                        // Chocolate rintersect fallback to (*x,*y) when miss.
                        let (daxp, dayp) = if let Some((_, hx, hy)) =
                            ray_seg_intersect(*x, *y, gx, gy, x1, y1, x2, y2)
                        {
                            (hx, hy)
                        } else {
                            (*x, *y)
                        };
                        let (_, daz_f) = getzsofslope(dasect, daxp, dayp, sz, walls);
                        let (_, daz2_f) = getzsofslope(ns, daxp, dayp, sz, walls);
                        let sec2 = sz[ns as usize];
                        if daz2_f < daz_f.wrapping_sub(1 << 8)
                            && (sec2.floorstat & 1) == 0
                            && z >= daz2_f.wrapping_sub(flordist.wrapping_sub(1))
                        {
                            clipyou = true;
                        }
                        if !clipyou {
                            let (daz_c, _) = getzsofslope(dasect, daxp, dayp, sz, walls);
                            let (daz2_c, _) = getzsofslope(ns, daxp, dayp, sz, walls);
                            if daz2_c > daz_c.wrapping_add(1 << 8)
                                && (sec2.ceilingstat & 1) == 0
                                && z <= daz2_c.wrapping_add(ceildist.wrapping_sub(1))
                            {
                                clipyou = true;
                            }
                        }
                    }
                }
            }

            if clipyou {
                let oval = j + 32768;
                let mut bsz = walldist;
                if gx < 0 {
                    bsz = -bsz;
                }
                clipit.push((
                    x1.wrapping_sub(bsz),
                    y1.wrapping_sub(bsz),
                    x1.wrapping_sub(bsz),
                    y1.wrapping_add(bsz),
                ));
                clipobjectval.push(oval);
                clipit.push((
                    x2.wrapping_sub(bsz),
                    y2.wrapping_sub(bsz),
                    x2.wrapping_sub(bsz),
                    y2.wrapping_add(bsz),
                ));
                clipobjectval.push(oval);
                bsz = walldist;
                if gy < 0 {
                    bsz = -bsz;
                }
                clipit.push((
                    x1.wrapping_add(bsz),
                    y1.wrapping_sub(bsz),
                    x1.wrapping_sub(bsz),
                    y1.wrapping_sub(bsz),
                ));
                clipobjectval.push(oval);
                clipit.push((
                    x2.wrapping_add(bsz),
                    y2.wrapping_sub(bsz),
                    x2.wrapping_sub(bsz),
                    y2.wrapping_sub(bsz),
                ));
                clipobjectval.push(oval);
                let mut dax = walldist;
                if dy > 0 {
                    dax = -dax;
                }
                let mut day = walldist;
                if dx < 0 {
                    day = -day;
                }
                clipit.push((
                    x1.wrapping_add(dax),
                    y1.wrapping_add(day),
                    x2.wrapping_add(dax),
                    y2.wrapping_add(day),
                ));
                clipobjectval.push(oval);
            } else if wal.nextsector >= 0 {
                let ns = wal.nextsector;
                let mut found = false;
                for &listed in clipsectorlist.iter().take(clipsectnum) {
                    if listed == ns {
                        found = true;
                        break;
                    }
                }
                if !found && clipsectnum < MAXSECTORS {
                    clipsectorlist[clipsectnum] = ns;
                    clipsectnum += 1;
                }
            }
            j += 1;
        }
    }

    // Face sprites in BFS sectors (Chocolate `cstat&48==0` L-clips).
    if dasprclipmask != 0 {
        for (si, spr) in sprites.iter().enumerate() {
            if spr.sectnum < 0 {
                continue;
            }
            let mut in_bfs = false;
            for &listed in clipsectorlist.iter().take(clipsectnum) {
                if listed == spr.sectnum {
                    in_bfs = true;
                    break;
                }
            }
            if !in_bfs {
                continue;
            }
            if spr.cstat & dasprclipmask == 0 || spr.tile_height <= 0 || spr.yrepeat == 0 {
                continue;
            }
            if (spr.cstat & 48) != 0 {
                continue; // wall/floor sprites later
            }
            let x1 = spr.x;
            let y1 = spr.y;
            if x1 < xmin || x1 > xmax || y1 < ymin || y1 > ymax {
                continue;
            }
            let k = (i32::from(spr.tile_height) * i32::from(spr.yrepeat)) << 2;
            let mut daz = spr.z;
            if spr.cstat & 128 != 0 {
                daz += k >> 1;
            }
            if spr.picanm_yoff != 0 {
                daz -= i32::from(spr.picanm_yoff) * i32::from(spr.yrepeat) * 4;
            }
            if !(z < daz.wrapping_add(ceildist) && z > daz.wrapping_sub(k).wrapping_sub(flordist)) {
                continue;
            }
            let oval = si as i32 + 49152;
            let mut bsz = (i32::from(spr.clipdist) << 2) + walldist;
            if gx < 0 {
                bsz = -bsz;
            }
            clipit.push((
                x1.wrapping_sub(bsz),
                y1.wrapping_sub(bsz),
                x1.wrapping_sub(bsz),
                y1.wrapping_add(bsz),
            ));
            clipobjectval.push(oval);
            bsz = (i32::from(spr.clipdist) << 2) + walldist;
            if gy < 0 {
                bsz = -bsz;
            }
            clipit.push((
                x1.wrapping_add(bsz),
                y1.wrapping_sub(bsz),
                x1.wrapping_sub(bsz),
                y1.wrapping_sub(bsz),
            ));
            clipobjectval.push(oval);
        }
    }

    let mut retval = 0_i32;
    let mut xvect = xvect;
    let mut yvect = yvect;
    let mut hitwalls = [0_i32; 8];
    let mut cnt = 3_i32; // clipmoveboxtracenum
    let clipmoveboxtracenum = 3_i32;
    loop {
        let mut intx = goalx;
        let mut inty = goaly;
        let hitwall = raytrace_clipit(*x, *y, &mut intx, &mut inty, &clipit);
        if hitwall >= 0 {
            let (x1, y1, x2, y2) = clipit[hitwall as usize];
            let lx = x2.wrapping_sub(x1);
            let ly = y2.wrapping_sub(y1);
            let templong2 = lx.wrapping_mul(lx).wrapping_add(ly.wrapping_mul(ly));
            if templong2 > 0 {
                let templong1 = (goalx.wrapping_sub(intx))
                    .wrapping_mul(lx)
                    .wrapping_add((goaly.wrapping_sub(inty)).wrapping_mul(ly));
                let i = if (klabs(templong1) >> 11) < templong2 {
                    divscale(templong1, templong2, 20)
                } else {
                    0
                };
                goalx = mulscale(lx, i, 20).wrapping_add(intx);
                goaly = mulscale(ly, i, 20).wrapping_add(inty);
            }

            let templong1 = dmulscale(lx, oxvect, ly, oyvect, 6);
            let mut i = cnt + 1;
            while i <= clipmoveboxtracenum {
                let jj = hitwalls[i as usize];
                let (cx1, cy1, cx2, cy2) = clipit[jj as usize];
                let templong2 = dmulscale(
                    cx2.wrapping_sub(cx1),
                    oxvect,
                    cy2.wrapping_sub(cy1),
                    oyvect,
                    6,
                );
                if (templong1 ^ templong2) < 0 {
                    updatesector(*x, *y, sect, sectors, walls);
                    return retval;
                }
                i += 1;
            }

            keepaway_clipit(&mut goalx, &mut goaly, hitwall, &clipit);
            // Residual stays world-unit (matches goal = x + xvect API).
            xvect = goalx.wrapping_sub(intx);
            yvect = goaly.wrapping_sub(inty);

            if cnt == clipmoveboxtracenum {
                retval = clipobjectval.get(hitwall as usize).copied().unwrap_or(0);
            }
            hitwalls[cnt as usize] = hitwall;
        }
        cnt -= 1;
        *x = intx;
        *y = inty;
        if !(((xvect | yvect) != 0) && hitwall >= 0 && cnt > 0) {
            break;
        }
    }

    for j in 0..clipsectnum {
        if inside(*x, *y, clipsectorlist[j], sectors, walls) {
            *sect = clipsectorlist[j];
            return retval;
        }
    }
    updatesector(*x, *y, sect, sectors, walls);
    retval
}

fn keepaway_clipit(x: &mut i32, y: &mut i32, w: i32, clipit: &[(i32, i32, i32, i32)]) {
    let (x1, y1, x2, y2) = clipit[w as usize];
    let dx = x2.wrapping_sub(x1);
    let dy = y2.wrapping_sub(y1);
    let ox = ksgn(-dy);
    let oy = ksgn(dx);
    let mut first = klabs(dx) <= klabs(dy);
    loop {
        if dx.wrapping_mul(y.wrapping_sub(y1)) > (x.wrapping_sub(x1)).wrapping_mul(dy) {
            return;
        }
        if !first {
            *x = x.wrapping_add(ox);
        } else {
            *y = y.wrapping_add(oy);
        }
        first = !first;
    }
}

fn raytrace_clipit(
    x3: i32,
    y3: i32,
    x4: &mut i32,
    y4: &mut i32,
    clipit: &[(i32, i32, i32, i32)],
) -> i32 {
    let mut hitwall = -1_i32;
    for z in (0..clipit.len()).rev() {
        let (x1, y1, x2, y2) = clipit[z];
        let x21 = x2.wrapping_sub(x1);
        let y21 = y2.wrapping_sub(y1);
        let topu = x21
            .wrapping_mul(y3.wrapping_sub(y1))
            .wrapping_sub((x3.wrapping_sub(x1)).wrapping_mul(y21));
        if topu <= 0 {
            continue;
        }
        if x21.wrapping_mul(y4.wrapping_sub(y1)) > (x4.wrapping_sub(x1)).wrapping_mul(y21) {
            continue;
        }
        let x43 = x4.wrapping_sub(x3);
        let y43 = y4.wrapping_sub(y3);
        if x43.wrapping_mul(y1.wrapping_sub(y3)) > (x1.wrapping_sub(x3)).wrapping_mul(y43) {
            continue;
        }
        if x43.wrapping_mul(y2.wrapping_sub(y3)) <= (x2.wrapping_sub(x3)).wrapping_mul(y43) {
            continue;
        }
        let bot = x43.wrapping_mul(y21).wrapping_sub(x21.wrapping_mul(y43));
        if bot == 0 {
            continue;
        }
        let mut top = topu;
        let mut cnt = 256_i32;
        let mut nintx;
        let mut ninty;
        loop {
            cnt -= 1;
            if cnt < 0 {
                *x4 = x3;
                *y4 = y3;
                return z as i32;
            }
            nintx = x3.wrapping_add(scale(x43, top, bot));
            ninty = y3.wrapping_add(scale(y43, top, bot));
            top -= 1;
            // Chocolate: while (x21*(ninty-y1) <= (nintx-x1)*y21) keep refining.
            if x21.wrapping_mul(ninty.wrapping_sub(y1)) > (nintx.wrapping_sub(x1)).wrapping_mul(y21)
            {
                break;
            }
        }
        if klabs(x3.wrapping_sub(nintx)).wrapping_add(klabs(y3.wrapping_sub(ninty)))
            < klabs(x3.wrapping_sub(*x4)).wrapping_add(klabs(y3.wrapping_sub(*y4)))
        {
            *x4 = nintx;
            *y4 = ninty;
            hitwall = z as i32;
        }
    }
    hitwall
}

/// Chocolate `clipinsidebox(x, y, wallnum, walldist)`.
///
/// Returns `1` when the point's `walldist` box overlaps the wall segment on the
/// front side (push candidate), `2` for back-side overlap, else `0`.
#[must_use]
pub fn clipinsidebox(x: i32, y: i32, wallnum: i16, walldist: i32, walls: &[WallXY]) -> i32 {
    let wi = wallnum as usize;
    if wi >= walls.len() {
        return 0;
    }
    let wal = walls[wi];
    let p2 = wal.point2 as usize;
    if p2 >= walls.len() {
        return 0;
    }
    let r = walldist << 1;
    let x1 = wal.x.wrapping_add(walldist).wrapping_sub(x);
    let y1 = wal.y.wrapping_add(walldist).wrapping_sub(y);
    let wb = walls[p2];
    let mut x2 = wb.x.wrapping_add(walldist).wrapping_sub(x);
    let mut y2 = wb.y.wrapping_add(walldist).wrapping_sub(y);

    if x1 < 0 && x2 < 0 {
        return 0;
    }
    if y1 < 0 && y2 < 0 {
        return 0;
    }
    if x1 >= r && x2 >= r {
        return 0;
    }
    if y1 >= r && y2 >= r {
        return 0;
    }

    x2 = x2.wrapping_sub(x1);
    y2 = y2.wrapping_sub(y1);
    // Front
    if x2.wrapping_mul(walldist.wrapping_sub(y1)) >= y2.wrapping_mul(walldist.wrapping_sub(x1)) {
        let xx = if x2 > 0 {
            x2.wrapping_mul(0i32.wrapping_sub(y1))
        } else {
            x2.wrapping_mul(r.wrapping_sub(y1))
        };
        let yy = if y2 > 0 {
            y2.wrapping_mul(r.wrapping_sub(x1))
        } else {
            y2.wrapping_mul(0i32.wrapping_sub(x1))
        };
        return i32::from(xx < yy);
    }
    let xx = if x2 > 0 {
        x2.wrapping_mul(r.wrapping_sub(y1))
    } else {
        x2.wrapping_mul(0i32.wrapping_sub(y1))
    };
    let yy = if y2 > 0 {
        y2.wrapping_mul(0i32.wrapping_sub(x1))
    } else {
        y2.wrapping_mul(r.wrapping_sub(x1))
    };
    i32::from(xx >= yy) << 1
}

/// Chocolate `pushmove` (walls / portals only — no sprite branch).
///
/// Returns `0` when clear, `-1` when a push occurred (or still overlapping after
/// the iteration cap). Matches Atomic `pushmove` walldist/ceil/flor checks.
pub fn pushmove(
    x: &mut i32,
    y: &mut i32,
    z: i32,
    sect: &mut i16,
    walldist: i32,
    ceildist: i32,
    flordist: i32,
    cliptype: u32,
    sectors: &[SectorWalls],
    sector_z: &[SectorZ],
    walls: &[WallXY],
) -> i32 {
    if *sect < 0 {
        return -1;
    }
    let dawalclipmask = (cliptype & 65535) as u16;
    let boxdist = walldist.wrapping_sub(4);
    let mut k = 32_i32;
    let mut dir: i32 = 1;
    let mut bad;
    loop {
        bad = 0;
        let mut clipsectorlist = vec![0i16; MAXSECTORS];
        clipsectorlist[0] = *sect;
        let mut clipsectcnt = 0_usize;
        let mut clipsectnum = 1_usize;
        while clipsectcnt < clipsectnum {
            let sn = clipsectorlist[clipsectcnt];
            if sn < 0 || sn as usize >= sectors.len() {
                clipsectcnt += 1;
                continue;
            }
            let sec = sectors[sn as usize];
            let (startwall, endwall) = if dir > 0 {
                (
                    i32::from(sec.wallptr),
                    i32::from(sec.wallptr) + i32::from(sec.wallnum),
                )
            } else {
                let endwall = i32::from(sec.wallptr);
                (endwall + i32::from(sec.wallnum), endwall)
            };
            let mut i = startwall;
            while i != endwall {
                if i >= 0 && (i as usize) < walls.len() {
                    let wi = i as i16;
                    if clipinsidebox(*x, *y, wi, boxdist, walls) == 1 {
                        let wal = walls[i as usize];
                        let mut j = 0_i32;
                        if wal.nextsector < 0 {
                            j = 1;
                        }
                        if wal.cstat & dawalclipmask != 0 {
                            j = 1;
                        }
                        if j == 0 && wal.nextsector >= 0 {
                            let ns = wal.nextsector;
                            if (ns as usize) < sector_z.len() && (sn as usize) < sector_z.len() {
                                let p2 = wal.point2 as usize;
                                if p2 < walls.len() {
                                    let mut dax = walls[p2].x.wrapping_sub(wal.x);
                                    let mut day = walls[p2].y.wrapping_sub(wal.y);
                                    let daz = dax
                                        .wrapping_mul(x.wrapping_sub(wal.x))
                                        .wrapping_add(day.wrapping_mul(y.wrapping_sub(wal.y)));
                                    let t = if daz <= 0 {
                                        0
                                    } else {
                                        let daz2 = dax
                                            .wrapping_mul(dax)
                                            .wrapping_add(day.wrapping_mul(day));
                                        if daz >= daz2 {
                                            1 << 30
                                        } else {
                                            divscale(daz, daz2, 30)
                                        }
                                    };
                                    dax = wal.x.wrapping_add(mulscale(dax, t, 30));
                                    day = wal.y.wrapping_add(mulscale(day, t, 30));

                                    let (_, daz_f) = getzsofslope(sn, dax, day, sector_z, walls);
                                    let (_, daz2_f) = getzsofslope(ns, dax, day, sector_z, walls);
                                    let sec2 = sector_z[ns as usize];
                                    if daz2_f < daz_f.wrapping_sub(1 << 8)
                                        && (sec2.floorstat & 1) == 0
                                        && z >= daz2_f.wrapping_sub(flordist.wrapping_sub(1))
                                    {
                                        j = 1;
                                    }
                                    let (daz_c, _) = getzsofslope(sn, dax, day, sector_z, walls);
                                    let (daz2_c, _) = getzsofslope(ns, dax, day, sector_z, walls);
                                    if daz2_c > daz_c.wrapping_add(1 << 8)
                                        && (sec2.ceilingstat & 1) == 0
                                        && z <= daz2_c.wrapping_add(ceildist.wrapping_sub(1))
                                    {
                                        j = 1;
                                    }
                                }
                            }
                        }
                        if j != 0 {
                            let p2 = wal.point2 as usize;
                            if p2 < walls.len() {
                                let ang = getangle(
                                    walls[p2].x.wrapping_sub(wal.x),
                                    walls[p2].y.wrapping_sub(wal.y),
                                );
                                // sintable[(ang+1024)&2047]>>11, sintable[(ang+512)&2047]>>11
                                let (sin_opp, _) = sincos_q14(ang.wrapping_add(1024));
                                let (_, cos) = sincos_q14(ang);
                                let dx = sin_opp >> 11;
                                let dy = cos >> 11;
                                let mut bad2 = 16_u8;
                                loop {
                                    *x = x.wrapping_add(dx);
                                    *y = y.wrapping_add(dy);
                                    bad2 = bad2.wrapping_sub(1);
                                    if bad2 == 0 {
                                        break;
                                    }
                                    if clipinsidebox(*x, *y, wi, boxdist, walls) == 0 {
                                        break;
                                    }
                                }
                                bad = -1;
                                k -= 1;
                                if k <= 0 {
                                    return bad;
                                }
                                updatesector(*x, *y, sect, sectors, walls);
                            }
                        } else if wal.nextsector >= 0 {
                            let ns = wal.nextsector;
                            let mut found = false;
                            for &listed in clipsectorlist.iter().take(clipsectnum) {
                                if listed == ns {
                                    found = true;
                                    break;
                                }
                            }
                            if !found && clipsectnum < MAXSECTORS {
                                clipsectorlist[clipsectnum] = ns;
                                clipsectnum += 1;
                            }
                        }
                    }
                }
                i = i.wrapping_add(dir);
            }
            clipsectcnt += 1;
        }
        dir = -dir;
        if bad == 0 {
            break;
        }
    }
    bad
}

/// Nearest tagged solid wall along look angle (bootstrap — one sector, walls only).
pub fn neartag_wall(
    x: i32,
    y: i32,
    z: i32,
    sectnum: i16,
    ang: i16,
    neartagrange: i32,
    sectors: &[SectorWalls],
    walls: &[WallXY],
    wall_lotag: &[i16],
) -> Option<(i16, i32)> {
    if neartagrange <= 0 {
        return None;
    }
    let (sin, cos) = approx_sincos_q14(i32::from(ang));
    // Direction length ~ neartagrange in world units (Q14 scale).
    let vx = ((i64::from(cos) * i64::from(neartagrange)) >> 14) as i32;
    let vy = ((i64::from(sin) * i64::from(neartagrange)) >> 14) as i32;
    let h = hitscan(x, y, z, sectnum, vx, vy, 0, 0, sectors, walls);
    if h.hitwall < 0 {
        return None;
    }
    let wi = h.hitwall as usize;
    let tag = wall_lotag.get(wi).copied().unwrap_or(0);
    if tag == 0 {
        return None;
    }
    let dx = i64::from(h.hitx) - i64::from(x);
    let dy = i64::from(h.hity) - i64::from(y);
    let dist = crate::helpers::length2d(dx as i32, dy as i32);
    if dist > neartagrange {
        return None;
    }
    Some((h.hitwall, dist))
}

/// Same-sector line-of-sight (bootstrap): no solid wall crosses the segment.
/// Chocolate `cansee` — portal walk with floor/ceiling Z gates at each crossing.
///
/// Wall `cstat` bit 5 (`32`) blocks LOS. Solid walls (`nextsector < 0`) block.
/// Crossing portals are accepted when the interpolated Z sits strictly between
/// both sectors' `getzsofslope` ceiling and floor at the hit point.
pub fn cansee(
    x1: i32,
    y1: i32,
    z1: i32,
    sect1: i16,
    x2: i32,
    y2: i32,
    z2: i32,
    sect2: i16,
    sectors: &[SectorZ],
    walls: &[WallXY],
) -> bool {
    if sect1 < 0 || sect2 < 0 || sect1 as usize >= sectors.len() || sect2 as usize >= sectors.len()
    {
        return false;
    }
    if x1 == x2 && y1 == y2 {
        return sect1 == sect2;
    }

    let x21 = x2.wrapping_sub(x1);
    let y21 = y2.wrapping_sub(y1);
    let z21 = z2.wrapping_sub(z1);

    let mut clipsectorlist = vec![0i16; MAXSECTORS];
    clipsectorlist[0] = sect1;
    let mut danum = 1_usize;
    let mut dacnt = 0_usize;
    while dacnt < danum {
        let dasectnum = clipsectorlist[dacnt];
        dacnt += 1;
        if dasectnum < 0 || dasectnum as usize >= sectors.len() {
            continue;
        }
        let sec = sectors[dasectnum as usize];
        let start = sec.wallptr as i32;
        let end = start + i32::from(sec.wallnum);
        let mut wi = start;
        while wi < end {
            if wi < 0 || wi as usize >= walls.len() {
                wi += 1;
                continue;
            }
            let wal = walls[wi as usize];
            let p2 = wal.point2 as usize;
            if p2 >= walls.len() {
                wi += 1;
                continue;
            }
            let wal2 = walls[p2];
            let x31 = wal.x.wrapping_sub(x1);
            let x34 = wal.x.wrapping_sub(wal2.x);
            let y31 = wal.y.wrapping_sub(y1);
            let y34 = wal.y.wrapping_sub(wal2.y);

            let bot = y21.wrapping_mul(x34).wrapping_sub(x21.wrapping_mul(y34));
            if bot <= 0 {
                wi += 1;
                continue;
            }

            let t_u = y21.wrapping_mul(x31).wrapping_sub(x21.wrapping_mul(y31));
            if (t_u as u32) >= (bot as u32) {
                wi += 1;
                continue;
            }
            let t_t = y31.wrapping_mul(x34).wrapping_sub(x31.wrapping_mul(y34));
            if (t_t as u32) >= (bot as u32) {
                wi += 1;
                continue;
            }

            let nexts = wal.nextsector;
            if nexts < 0 || (wal.cstat & 32) != 0 {
                return false;
            }

            let t = divscale(t_t, bot, 24);
            let x = x1.wrapping_add(mulscale(x21, t, 24));
            let y = y1.wrapping_add(mulscale(y21, t, 24));
            let z = z1.wrapping_add(mulscale(z21, t, 24));

            let (cz, fz) = getzsofslope(dasectnum, x, y, sectors, walls);
            if z <= cz || z >= fz {
                return false;
            }
            let (cz2, fz2) = getzsofslope(nexts, x, y, sectors, walls);
            if z <= cz2 || z >= fz2 {
                return false;
            }

            let mut seen = false;
            for i in (0..danum).rev() {
                if clipsectorlist[i] == nexts {
                    seen = true;
                    break;
                }
            }
            if !seen && danum < MAXSECTORS {
                clipsectorlist[danum] = nexts;
                danum += 1;
            }
            wi += 1;
        }
    }
    for i in (0..danum).rev() {
        if clipsectorlist[i] == sect2 {
            return true;
        }
    }
    false
}

/// Intersect ray `(ox,oy)+t*(vx,vy)` with segment `(x1,y1)-(x2,y2)`.
/// Returns `(t_q16, hx, hy)` with `t_q16` in 16.16 fixed (`t=1.0 → 65536`).
/// True when `(px,py)` lies on the infinite line through wall `a`→`b` (exact integer).
#[allow(dead_code)] // kept for hitscan/clip diagnostics
fn point_on_wall_line(px: i32, py: i32, a: WallXY, b: WallXY) -> bool {
    let dx = i64::from(b.x) - i64::from(a.x);
    let dy = i64::from(b.y) - i64::from(a.y);
    let ex = i64::from(px) - i64::from(a.x);
    let ey = i64::from(py) - i64::from(a.y);
    dx * ey - dy * ex == 0
}

fn ray_seg_intersect(
    ox: i32,
    oy: i32,
    vx: i32,
    vy: i32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) -> Option<(i64, i32, i32)> {
    let rdx = i64::from(vx);
    let rdy = i64::from(vy);
    let sdx = i64::from(x2) - i64::from(x1);
    let sdy = i64::from(y2) - i64::from(y1);
    let den = rdx * sdy - rdy * sdx;
    if den == 0 {
        return None;
    }
    let qdx = i64::from(x1) - i64::from(ox);
    let qdy = i64::from(y1) - i64::from(oy);
    // t = (qdx*sdy - qdy*sdx) / den   in true units of the direction vector
    let t_num = qdx * sdy - qdy * sdx;
    let u_num = qdx * rdy - qdy * rdx;
    // u in [0,1] along segment: u = u_num/den
    if den > 0 {
        if u_num < 0 || u_num > den {
            return None;
        }
        if t_num <= 0 {
            return None;
        }
    } else if u_num > 0 || u_num < den || t_num >= 0 {
        return None;
    }
    let t_q16 = (t_num << 16) / den;
    if t_q16 <= 0 {
        return None;
    }
    let hx = ox.wrapping_add(((rdx * t_q16) >> 16) as i32);
    let hy = oy.wrapping_add(((rdy * t_q16) >> 16) as i32);
    Some((t_q16, hx, hy))
}

/// Lowest ceiling / highest floor under a disc (sector planes only — bootstrap).
///
/// Chocolate `MAXCLIPDIST` used by [`getzrange`]’s portal disc.
const MAX_CLIP_DIST: i32 = 1024;

/// See `docs/build-engine-primitives/06-getzrange.md`.
///
/// `clipmask`: low 16 = wall cstat mask, high 16 = sprite cstat mask
/// (Chocolate `CLIPMASK0 = (1<<16)+1`).
pub fn getzrange(
    x: i32,
    y: i32,
    z: i32,
    sectnum: i16,
    walldist: i32,
    clipmask: u32,
    sectors: &[SectorZ],
    walls: &[WallXY],
) -> ZRange {
    getzrange_with_sprites(x, y, z, sectnum, walldist, clipmask, sectors, walls, &[])
}

/// [`getzrange`] plus blocking sprites overlapping the disc (face sprites).
pub fn getzrange_with_sprites(
    x: i32,
    y: i32,
    z: i32,
    sectnum: i16,
    walldist: i32,
    clipmask: u32,
    sectors: &[SectorZ],
    walls: &[WallXY],
    sprites: &[SpriteZClip],
) -> ZRange {
    if sectnum < 0 || sectnum as usize >= sectors.len() {
        return ZRange {
            ceilz: i32::MIN,
            ceilhit: -1,
            florz: i32::MAX,
            florhit: -1,
        };
    }

    let dawalclipmask = (clipmask & 0xffff) as u16;
    let dasprclipmask = ((clipmask >> 16) & 0xffff) as u16;

    let box_ext = walldist + MAX_CLIP_DIST + 1;
    let xmin = x - box_ext;
    let ymin = y - box_ext;
    let xmax = x + box_ext;
    let ymax = y + box_ext;

    let (mut ceilz, mut florz) = getzsofslope(sectnum, x, y, sectors, walls);
    let mut ceilhit = i32::from(sectnum) + 16384;
    let mut florhit = i32::from(sectnum) + 16384;

    // Collect sectors overlapping the disc (Chocolate clipsectorlist BFS).
    let mut clipsectors: Vec<i16> = vec![sectnum];
    let mut clipsectcnt = 0usize;
    while clipsectcnt < clipsectors.len() {
        let snum = clipsectors[clipsectcnt];
        let sec = sectors[snum as usize];
        let start = sec.wallptr as usize;
        let end = start.saturating_add(sec.wallnum as usize);
        for wi in start..end.min(walls.len()) {
            let wal = walls[wi];
            let k = wal.nextsector;
            if k < 0 || k as usize >= sectors.len() {
                continue;
            }
            let p2 = wal.point2 as usize;
            if p2 >= walls.len() {
                continue;
            }
            let wal2 = walls[p2];
            let (x1, x2, y1, y2) = (wal.x, wal2.x, wal.y, wal2.y);
            if (x1 < xmin && x2 < xmin) || (x1 > xmax && x2 > xmax) {
                continue;
            }
            if (y1 < ymin && y2 < ymin) || (y1 > ymax && y2 > ymax) {
                continue;
            }
            let dx = i64::from(x2 - x1);
            let dy = i64::from(y2 - y1);
            if dx * (i64::from(y) - i64::from(y1)) < (i64::from(x) - i64::from(x1)) * dy {
                continue;
            }
            let dax = if dx > 0 {
                dx * i64::from(ymin - y1)
            } else {
                dx * i64::from(ymax - y1)
            };
            let day = if dy > 0 {
                dy * i64::from(xmax - x1)
            } else {
                dy * i64::from(xmin - x1)
            };
            if dax >= day {
                continue;
            }
            if wal.cstat & dawalclipmask != 0 {
                continue;
            }
            let nsec = sectors[k as usize];
            if (nsec.ceilingstat & 1) == 0 && z <= nsec.ceilingz + (3 << 8) {
                continue;
            }
            if (nsec.floorstat & 1) == 0 && z >= nsec.floorz - (3 << 8) {
                continue;
            }
            if !clipsectors.contains(&k) {
                clipsectors.push(k);
            }

            let md = MAX_CLIP_DIST;
            if (x1 < xmin + md && x2 < xmin + md) || (x1 > xmax - md && x2 > xmax - md) {
                continue;
            }
            if (y1 < ymin + md && y2 < ymin + md) || (y1 > ymax - md && y2 > ymax - md) {
                continue;
            }
            let dax2 = if dx > 0 {
                dax + dx * i64::from(md)
            } else {
                dax - dx * i64::from(md)
            };
            let day2 = if dy > 0 {
                day - dy * i64::from(md)
            } else {
                day + dy * i64::from(md)
            };
            if dax2 >= day2 {
                continue;
            }
            let (daz, daz2) = getzsofslope(k, x, y, sectors, walls);
            if daz > ceilz {
                ceilz = daz;
                ceilhit = i32::from(k) + 16384;
            }
            if daz2 < florz {
                florz = daz2;
                florhit = i32::from(k) + 16384;
            }
        }
        clipsectcnt += 1;
    }

    // Sprites in collected sectors (Chocolate face=0 / floor=32; wall=16 later).
    if dasprclipmask != 0 {
        for (j, spr) in sprites.iter().enumerate() {
            if spr.sectnum < 0 || !clipsectors.contains(&spr.sectnum) {
                continue;
            }
            if spr.cstat & dasprclipmask == 0 || spr.tile_height <= 0 || spr.yrepeat == 0 {
                continue;
            }
            let kind = spr.cstat & 48;
            let (daz, daz2, hit) = match kind {
                0 => {
                    // Face sprite cylinder.
                    let rad = walldist + (i32::from(spr.clipdist) << 2) + 1;
                    if (spr.x - x).abs() > rad || (spr.y - y).abs() > rad {
                        continue;
                    }
                    let mut daz = spr.z;
                    let hk = (i32::from(spr.tile_height) * i32::from(spr.yrepeat)) << 1;
                    if spr.cstat & 128 != 0 {
                        daz += hk;
                    }
                    if spr.picanm_yoff != 0 {
                        daz -= i32::from(spr.picanm_yoff) * i32::from(spr.yrepeat) * 4;
                    }
                    (daz, daz - (hk << 1), true)
                }
                32 => {
                    // Floor sprite: flat Z at spr.z; AABB overlap (rotated quad later).
                    if spr.cstat & 64 != 0 {
                        let below = z > spr.z;
                        let onesided_top = spr.cstat & 8 == 0;
                        if below == onesided_top {
                            continue;
                        }
                    }
                    let tw = i32::from(spr.tile_width).max(1);
                    let th = i32::from(spr.tile_height).max(1);
                    let mut xoff = i32::from(spr.picanm_xoff) + i32::from(spr.xoffset);
                    let mut yoff = i32::from(spr.picanm_yoff) + i32::from(spr.yoffset);
                    if spr.cstat & 4 != 0 {
                        xoff = -xoff;
                    }
                    if spr.cstat & 8 != 0 {
                        yoff = -yoff;
                    }
                    let hw = ((tw >> 1) + xoff)
                        .saturating_mul(i32::from(spr.xrepeat))
                        .abs()
                        + walldist
                        + 4;
                    let hh = ((th >> 1) + yoff)
                        .saturating_mul(i32::from(spr.yrepeat))
                        .abs()
                        + walldist
                        + 4;
                    if (spr.x - x).abs() > hw || (spr.y - y).abs() > hh {
                        continue;
                    }
                    (spr.z, spr.z, true)
                }
                _ => continue,
            };
            if !hit {
                continue;
            }
            if z > daz && daz > ceilz {
                ceilz = daz;
                ceilhit = j as i32 + 49152;
            }
            if z < daz2 && daz2 < florz {
                florz = daz2;
                florhit = j as i32 + 49152;
            }
        }
    }

    ZRange {
        ceilz,
        ceilhit,
        florz,
        florhit,
    }
}

impl BuildWorld {
    /// Convenience when walls/sectors are stored on the world (D3+).
    pub fn inside_xy(
        &self,
        x: i32,
        y: i32,
        sectnum: i16,
        sectors: &[SectorWalls],
        walls: &[WallXY],
    ) -> bool {
        let _ = self.numsectors.min(MAXSECTORS as i16);
        inside(x, y, sectnum, sectors, walls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Axis-aligned unit square [0,100]×[0,100], sector 0, walls 0..3.
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
        let sectors = vec![SectorWalls {
            wallptr: 0,
            wallnum: 4,
        }];
        (sectors, walls)
    }

    #[test]
    fn inside_unit_square() {
        let (sec, walls) = unit_square();
        assert!(inside(50, 50, 0, &sec, &walls));
        assert!(!inside(-1, 50, 0, &sec, &walls));
        assert!(!inside(50, -1, 0, &sec, &walls));
        assert!(!inside(150, 50, 0, &sec, &walls));
        assert!(!inside(50, 50, 1, &sec, &walls)); // bad sect
        assert!(!inside(50, 50, -1, &sec, &walls));
    }

    #[test]
    fn inside_matches_even_odd_oracle_on_grid() {
        let (sec, walls) = unit_square();
        for y in (-20..120).step_by(7) {
            for x in (-20..120).step_by(7) {
                let got = inside(x, y, 0, &sec, &walls);
                let exp = even_odd_oracle(x, y, &walls);
                assert_eq!(got, exp, "at ({x},{y})");
            }
        }
    }

    fn even_odd_oracle(x: i32, y: i32, walls: &[WallXY]) -> bool {
        let mut c = 0;
        for a in walls {
            let b = walls[a.point2 as usize];
            if super::edge_crosses_ray(x, y, a.x, a.y, b.x, b.y) {
                c += 1;
            }
        }
        c % 2 == 1
    }

    #[test]
    fn updatesector_hint_and_scan() {
        let (sec, walls) = unit_square();
        let mut s = 0_i16;
        assert_eq!(updatesector(50, 50, &mut s, &sec, &walls), 0);
        s = 99;
        assert_eq!(updatesector(50, 50, &mut s, &sec, &walls), 0);
        assert_eq!(s, 0);
        assert_eq!(updatesector(999, 999, &mut s, &sec, &walls), -1);
        assert_eq!(s, -1);
    }

    #[test]
    fn clipmove_open_applies_full_vector() {
        let (sec, walls) = unit_square();
        let mut x = 50;
        let mut y = 50;
        let mut s = 0_i16;
        let hit = clipmove_xy(&mut x, &mut y, &mut s, 10, 0, 4, &sec, &walls);
        assert_eq!(hit, 0);
        assert_eq!((x, y), (60, 50));
    }

    #[test]
    fn clipmove_stops_before_wall() {
        let (sec, walls) = unit_square();
        let mut x = 50;
        let mut y = 50;
        let mut s = 0_i16;
        let hit = clipmove_xy(&mut x, &mut y, &mut s, 100, 0, 10, &sec, &walls);
        assert_ne!(hit & HIT_WALL_BOOTSTRAP, 0);
        assert!(x < 100, "x={x}");
        assert_eq!(x, 89); // 100 - (walldist+1)
        assert_eq!(y, 50);
    }

    #[test]
    fn clipmove_slides_along_wall() {
        // Move into the right wall with a +y component; expect slide along x=100.
        let (sec, walls) = unit_square();
        let mut x = 80;
        let mut y = 20;
        let mut s = 0_i16;
        let hit = clipmove_xy(&mut x, &mut y, &mut s, 50, 40, 4, &sec, &walls);
        assert_ne!(hit & HIT_WALL_BOOTSTRAP, 0);
        assert!(x < 100, "x={x}");
        assert!(y > 20, "y={y} (should slide up)");
    }

    #[test]
    fn neartag_finds_tagged_wall() {
        let (sec, walls) = unit_square();
        let mut tags = vec![0_i16; walls.len()];
        tags[1] = 42; // right wall
        let hit = neartag_wall(50, 50, 0, 0, 0, 200, &sec, &walls, &tags);
        // ang 0 → +x (cos=1,sin=0) toward wall 1
        assert_eq!(hit.map(|(w, _)| w), Some(1));
    }

    fn open_z(secs: &[SectorWalls]) -> Vec<SectorZ> {
        secs.iter()
            .map(|s| SectorZ {
                wallptr: s.wallptr,
                wallnum: s.wallnum,
                ceilingz: -1_000_000,
                floorz: 1_000_000,
                ceilingstat: 0,
                floorstat: 0,
                ceilingheinum: 0,
                floorheinum: 0,
            })
            .collect()
    }

    #[test]
    fn cansee_same_sector_open() {
        let (sec, walls) = unit_square();
        let sz = open_z(&sec);
        assert!(cansee(20, 20, 0, 0, 80, 80, 0, 0, &sz, &walls));
    }

    #[test]
    fn cansee_blocked_by_wall_across() {
        // Two points on opposite sides of a solid mid wall inside a larger box.
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
            // Internal solid divider x=50, y=10..90
            WallXY {
                x: 50,
                y: 10,
                point2: 5,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 50,
                y: 90,
                point2: 4,
                nextsector: -1,
                cstat: 0,
            },
        ];
        // Sector lists only outer loop for inside(); hitscan still walks wallptr..+wallnum
        let sec = vec![SectorWalls {
            wallptr: 0,
            wallnum: 6,
        }];
        let sz = open_z(&sec);
        assert!(!cansee(20, 50, 0, 0, 80, 50, 0, 0, &sz, &walls));
    }

    #[test]
    fn cansee_portal_walk_two_sectors() {
        // Two unit squares sharing an open portal on x=100.
        let walls = vec![
            // sector 0: [0,100]×[0,100], right wall portals to 1
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
                nextsector: 1,
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
            // sector 1: [100,200]×[0,100], left wall portals to 0
            WallXY {
                x: 100,
                y: 0,
                point2: 5,
                nextsector: 0,
                cstat: 0,
            },
            WallXY {
                x: 200,
                y: 0,
                point2: 6,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 200,
                y: 100,
                point2: 7,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 100,
                y: 100,
                point2: 4,
                nextsector: 0,
                cstat: 0,
            },
        ];
        let secs = vec![
            SectorWalls {
                wallptr: 0,
                wallnum: 4,
            },
            SectorWalls {
                wallptr: 4,
                wallnum: 4,
            },
        ];
        let sz = open_z(&secs);
        assert!(cansee(50, 50, 0, 0, 150, 50, 0, 1, &sz, &walls));
    }

    #[test]
    fn hitscan_hits_right_wall() {
        let (sec, walls) = unit_square();
        let h = hitscan(50, 50, 0, 0, 100, 0, 0, 0, &sec, &walls);
        assert_eq!(h.hitwall, 1, "{h:?}");
        assert_eq!(h.hitsect, 0);
        assert!((h.hitx - 100).abs() <= 1, "hitx={}", h.hitx);
        assert!((h.hity - 50).abs() <= 1, "hity={}", h.hity);
    }

    #[test]
    fn hitscan_hits_flat_floor() {
        let (wall_sec, walls) = unit_square();
        let sectors = vec![SectorZ {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -16384,
            floorz: 0,
            ceilingstat: 0,
            floorstat: 0,
            ceilingheinum: 0,
            floorheinum: 0,
        }];
        // From z=-1000 shooting down toward floor z=0.
        let h = hitscan_full(
            50,
            50,
            -1000,
            0,
            0,
            0,
            1000,
            0,
            &wall_sec,
            &walls,
            &[],
            Some(&sectors),
        );
        assert_eq!(h.hitsect, 0, "{h:?}");
        assert_eq!(h.hitwall, -1);
        assert_eq!(h.hitsprite, -1);
        assert!((h.hitz).abs() <= 2, "hitz={}", h.hitz);
    }

    #[test]
    fn hitscan_hits_face_sprite_before_wall() {
        let (sec, walls) = unit_square();
        let sprites = [SpriteHit::cylinder(70, 50, 0, 0, 8, 256)];
        let h = hitscan_with_sprites(50, 50, 0, 0, 100, 0, 0, 0, &sec, &walls, &sprites);
        assert_eq!(h.hitsprite, 0, "{h:?}");
        assert_eq!(h.hitwall, -1);
        assert!(h.hitx < 100);
    }

    #[test]
    fn hitscan_walks_portal_to_far_wall() {
        // Sect0 [0,100]x[0,100] portal on right to sect1 [100,200]x[0,100].
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
                nextsector: 1,
                cstat: 0,
            }, // portal
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
            WallXY {
                x: 100,
                y: 0,
                point2: 5,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 200,
                y: 0,
                point2: 6,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 200,
                y: 100,
                point2: 7,
                nextsector: -1,
                cstat: 0,
            }, // far solid
            WallXY {
                x: 100,
                y: 100,
                point2: 4,
                nextsector: 0,
                cstat: 0,
            },
        ];
        let sec = vec![
            SectorWalls {
                wallptr: 0,
                wallnum: 4,
            },
            SectorWalls {
                wallptr: 4,
                wallnum: 4,
            },
        ];
        let h = hitscan(50, 50, 0, 0, 200, 0, 0, 0, &sec, &walls);
        // Wall 5 is (200,0)→(200,100), the far solid of sect1.
        assert_eq!(h.hitwall, 5, "{h:?}");
        assert_eq!(h.hitsect, 1);
        assert!((h.hitx - 200).abs() <= 1, "hitx={}", h.hitx);
    }

    #[test]
    fn hitscan_miss_open_dir() {
        // No solid wall in +x from outside looking away — start outside sector
        // still only scans sector 0 walls; ray from (50,50) toward open portal
        // (none) leftward hits wall 3 at x=0.
        let (sec, walls) = unit_square();
        let h = hitscan(50, 50, 0, 0, -100, 0, 0, 0, &sec, &walls);
        assert_eq!(h.hitwall, 3, "{h:?}");
    }

    #[test]
    fn getzrange_flat_room() {
        let walls = unit_square().1;
        let sectors = vec![SectorZ {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            ceilingstat: 0,
            floorstat: 0,
            ceilingheinum: 0,
            floorheinum: 0,
        }];
        let r = getzrange(50, 50, 0, 0, 10, 0, &sectors, &walls);
        assert_eq!(r.ceilz, -8192);
        assert_eq!(r.florz, 0);
        assert_eq!(r.ceilhit, 16384);
        assert_eq!(r.florhit, 16384);
    }

    #[test]
    fn getzrange_face_sprite_raises_ceiling() {
        // Own-sprite ceiling: daz = z - yoff*yrepeat*4 (Chocolate APLAYER case).
        let walls = unit_square().1;
        let sectors = vec![SectorZ {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -38912,
            floorz: 49152,
            ceilingstat: 0,
            floorstat: 0,
            ceilingheinum: 0,
            floorheinum: 0,
        }];
        let sprites = [SpriteZClip {
            x: 50,
            y: 50,
            z: -24896,
            cstat: 1,
            clipdist: 64,
            xrepeat: 42,
            yrepeat: 36,
            xoffset: 0,
            yoffset: 0,
            ang: 0,
            sectnum: 0,
            tile_width: 40,
            tile_height: 82,
            picanm_xoff: -1,
            picanm_yoff: 1,
        }];
        const CLIPMASK0: u32 = (1 << 16) + 1;
        let r = getzrange_with_sprites(
            50, 50, -24896, 0, 163, CLIPMASK0, &sectors, &walls, &sprites,
        );
        assert_eq!(r.ceilz, -25040);
        assert_eq!(r.ceilhit, 49152);
    }

    #[test]
    fn getzsofslope_flat() {
        let walls = unit_square().1;
        let sectors = vec![SectorZ {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -16384,
            floorz: 0,
            ceilingstat: 0,
            floorstat: 0,
            ceilingheinum: 0,
            floorheinum: 0,
        }];
        assert_eq!(getzsofslope(0, 50, 50, &sectors, &walls), (-16384, 0));
    }

    #[test]
    fn getzsofslope_floor_hinge_unchanged_on_first_wall() {
        // First wall along y=0 from (0,0)→(100,0); heinum 4096 → 45° in Build units.
        let walls = unit_square().1;
        let sectors = vec![SectorZ {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -16384,
            floorz: 0,
            ceilingstat: 0,
            floorstat: 2,
            ceilingheinum: 0,
            floorheinum: 4096,
        }];
        // On the hinge wall (y=0): cross=0 → floorz unchanged.
        let (_c, f) = getzsofslope(0, 50, 0, &sectors, &walls);
        assert_eq!(f, 0);
        // Chocolate: i=nsqrt(10000)<<5, j=dmulscale3(100,50,0,0)=625, Δ=scale(4096,625,i).
        let (_c, f2) = getzsofslope(0, 50, 50, &sectors, &walls);
        let i = nsqrtasm(10_000).wrapping_shl(5);
        let expect = scale(4096, 625, i);
        assert_eq!(f2, expect);
    }

    #[test]
    fn two_adjacent_sectors_neighbour_path() {
        // Left [0,100]×[0,100], right [100,200]×[0,100]; shared edge at x=100.
        let walls = vec![
            // sector 0
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
                nextsector: 1,
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
            // sector 1
            WallXY {
                x: 100,
                y: 0,
                point2: 5,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 200,
                y: 0,
                point2: 6,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 200,
                y: 100,
                point2: 7,
                nextsector: -1,
                cstat: 0,
            },
            WallXY {
                x: 100,
                y: 100,
                point2: 4,
                nextsector: 0,
                cstat: 0,
            },
        ];
        let sectors = vec![
            SectorWalls {
                wallptr: 0,
                wallnum: 4,
            },
            SectorWalls {
                wallptr: 4,
                wallnum: 4,
            },
        ];
        assert!(inside(50, 50, 0, &sectors, &walls));
        assert!(inside(150, 50, 1, &sectors, &walls));
        let mut s = 0;
        assert_eq!(updatesector(150, 50, &mut s, &sectors, &walls), 1);
    }
}

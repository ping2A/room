//! Classic-mode CPU helpers (debug / goldens). Not pixel-exact vs DOS Build.
//! Spec: `docs/25` §9 / D3 — full soft path may later live in `room-render-soft`.

use crate::view::BuildView;
use crate::world::BuildWorld;
use std::collections::BTreeMap;

/// Owned ART tiles for soft textured draw (column-major pixels).
#[derive(Clone, Debug, Default)]
pub struct SoftTiles {
    tiles: BTreeMap<i16, (u16, u16, Vec<u8>)>,
}

impl SoftTiles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert tile `picnum` (column-major palette indices, Build layout).
    pub fn insert(&mut self, picnum: i16, width: u16, height: u16, pixels: Vec<u8>) {
        self.tiles.insert(picnum, (width, height, pixels));
    }

    /// Copy from a `room_build::Tile`.
    pub fn insert_tile(&mut self, picnum: i16, tile: &room_build::art::Tile<'_>) {
        self.insert(picnum, tile.width, tile.height, tile.pixels.to_vec());
    }

    /// Pull tiles referenced by `picnums` from a [`room_build::art::TileSet`].
    pub fn from_tile_set_pics(set: &room_build::art::TileSet<'_>, picnums: &[i16]) -> Self {
        let mut out = Self::new();
        for &pic in picnums {
            if let Some(tile) = set.get(i32::from(pic)) {
                out.insert_tile(pic, tile);
            }
        }
        out
    }

    fn sample(&self, picnum: i16, u: u32, v: u32) -> Option<u8> {
        let (w, h, pix) = self.tiles.get(&picnum)?;
        if *w == 0 || *h == 0 {
            return None;
        }
        let x = (u as usize) % (*w as usize);
        let y = (v as usize) % (*h as usize);
        // column-major: pixels[x * height + y]
        pix.get(x * (*h as usize) + y).copied()
    }
}

/// 8-bit indexed framebuffer (palette indices; 0 = clear).
#[derive(Clone, Debug)]
pub struct SoftFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl SoftFrame {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        }
    }

    fn put(&mut self, x: i32, y: i32, c: u8) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        self.pixels[(y as u32 * self.width + x as u32) as usize] = c;
    }

    /// Bresenham line in palette index `c`.
    pub fn line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: u8) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x0, y0, c);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
}

/// First-person walls textured from [`SoftTiles`] (fallback to flat colour).
///
/// Fills floor / ceiling / parallax sky from the camera sector first, then
/// draws solid walls.
pub fn render_fp_textured(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
) {
    fill_fp_flats_sky(world, view, frame, Some(tiles));
    let mut zbuf = render_fp_walls(world, view, frame, Some(tiles));
    render_fp_sprites(world, view, frame, Some(tiles), &mut zbuf);
}

/// First-person solid walls (flat colour per wall) with per-column depth.
///
/// Fills floor / ceiling / sky flats first. Wall colour from `picnum`/`shade`.
/// Portals skipped. Face sprites drawn after walls.
pub fn render_fp_solid(world: &BuildWorld, view: &BuildView, frame: &mut SoftFrame) {
    fill_fp_flats_sky(world, view, frame, None);
    let mut zbuf = render_fp_walls(world, view, frame, None);
    render_fp_sprites(world, view, frame, None, &mut zbuf);
}

/// Build `ceilingstat` / `floorstat` bit 0: parallax.
const STAT_PARALLAX: u16 = 1;

/// Horizon-split floor / ceiling / sky fill for the camera sector (bootstrap).
///
/// Not pixel-exact Build: one sector, flat horizon, optional tile sample.
/// Parallax ceiling (`ceilingstat & 1`) draws a sky strip keyed by `ang`.
pub fn fill_fp_flats_sky(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
) {
    let w = frame.width as i32;
    let h = frame.height as i32;
    if w <= 0 || h <= 0 {
        return;
    }
    let cy = h / 2;
    // Build horiz ~100 at eye level; shift horizon with look.
    let horizon = (cy - (i32::from(view.horiz) - 100) / 2).clamp(0, h);

    let sec = if view.sect >= 0 && view.sect < world.numsectors {
        &world.sectors[view.sect as usize]
    } else {
        return;
    };

    let floor_flat = wall_color(sec.floorpicnum, sec.floorshade);
    let ceil_flat = wall_color(sec.ceilingpicnum, sec.ceilingshade);
    let sky_parallax = (sec.ceilingstat & STAT_PARALLAX) != 0;
    let floor_parallax = (sec.floorstat & STAT_PARALLAX) != 0;

    for col in 0..w {
        // Sky / ceiling above horizon.
        for y in 0..horizon {
            let pix = if sky_parallax {
                sample_sky(tiles, sec.ceilingpicnum, view.ang, col, y, w, horizon)
                    .unwrap_or_else(|| sky_color(sec.ceilingpicnum, view.ang, col, y))
            } else {
                tiles
                    .and_then(|ts| {
                        let u = ((col as u32).wrapping_add(sec.ceilingxpanning as u32)) % 64;
                        let v = ((y as u32).wrapping_add(sec.ceilingypanning as u32)) % 64;
                        ts.sample(sec.ceilingpicnum, u, v)
                            .filter(|&p| p != room_build::art::TRANSPARENT)
                    })
                    .unwrap_or(ceil_flat)
            };
            frame.put(col, y, pix);
        }
        // Floor below horizon.
        for y in horizon..h {
            let pix = if floor_parallax {
                sample_sky(
                    tiles,
                    sec.floorpicnum,
                    view.ang,
                    col,
                    y - horizon,
                    w,
                    h - horizon,
                )
                .unwrap_or_else(|| sky_color(sec.floorpicnum, view.ang, col, y))
            } else {
                tiles
                    .and_then(|ts| {
                        let u = ((col as u32).wrapping_add(sec.floorxpanning as u32)) % 64;
                        let v =
                            (((y - horizon) as u32).wrapping_add(sec.floorypanning as u32)) % 64;
                        ts.sample(sec.floorpicnum, u, v)
                            .filter(|&p| p != room_build::art::TRANSPARENT)
                    })
                    .unwrap_or(floor_flat)
            };
            frame.put(col, y, pix);
        }
    }
}

fn sample_sky(
    tiles: Option<&SoftTiles>,
    picnum: i16,
    ang: i16,
    col: i32,
    y: i32,
    w: i32,
    band: i32,
) -> Option<u8> {
    let ts = tiles?;
    let u = ((i32::from(ang) * 2 + col * 4) & 0x7fff) as u32;
    let v = if band > 0 {
        ((y * 64) / band.max(1)) as u32
    } else {
        0
    };
    let _ = w;
    ts.sample(picnum, u, v)
        .filter(|&p| p != room_build::art::TRANSPARENT)
}

fn sky_color(picnum: i16, ang: i16, col: i32, y: i32) -> u8 {
    let base = (picnum.wrapping_mul(19).wrapping_add(40) as u8).max(48);
    let band = ((i32::from(ang) + col + y / 4) & 31) as u8;
    base.saturating_add(band / 4).min(200)
}

/// `cstat` bit 15 — invisible.
const CSTAT_INVISIBLE: u16 = 32768;
/// `cstat` bit 4 — wall-aligned sprite (skip in face-sprite bootstrap).
const CSTAT_WALL_ALIGN: u16 = 16;
/// `cstat` bit 5 — floor-aligned sprite (skip in face-sprite bootstrap).
const CSTAT_FLOOR_ALIGN: u16 = 32;

fn render_fp_walls(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
) -> Vec<i32> {
    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let w = frame.width as i32;
    let h = frame.height as i32;
    let cx = w / 2;
    let cy = h / 2;
    let focal = w;
    let mut zbuf = vec![i32::MAX; w as usize];

    for &s in &view.visible_sectors {
        if s < 0 || s >= world.numsectors {
            continue;
        }
        let sec = &world.sectors[s as usize];
        let start = sec.wallptr as usize;
        let end = start
            .saturating_add(sec.wallnum as usize)
            .min(world.walls.len());
        for wall in &world.walls[start..end] {
            if wall.nextsector >= 0 {
                continue;
            }
            let p2 = wall.point2 as usize;
            if p2 >= world.walls.len() {
                continue;
            }
            let b = &world.walls[p2];
            let wall_len =
                crate::helpers::length2d(b.x.wrapping_sub(wall.x), b.y.wrapping_sub(wall.y)).max(1);
            let Some((x0, y0f, z0)) = cam_project(view, sin, cos, wall.x, wall.y, sec.floorz)
            else {
                continue;
            };
            let Some((x1, y1f, z1)) = cam_project(view, sin, cos, b.x, b.y, sec.floorz) else {
                continue;
            };
            let Some((x0c, y0c, z0c)) = cam_project(view, sin, cos, wall.x, wall.y, sec.ceilingz)
            else {
                continue;
            };
            let Some((x1c, y1c, z1c)) = cam_project(view, sin, cos, b.x, b.y, sec.ceilingz) else {
                continue;
            };
            let proj = |x: i64, y: i64, z: i64| -> Option<(i32, i32, i32)> {
                if z <= 16 {
                    return None;
                }
                let px = cx as i64 + (focal as i64 * x) / z;
                let py = cy as i64 + (focal as i64 * y) / z;
                Some((px as i32, py as i32, z as i32))
            };
            let Some((sx0, sy0f, sz0)) = proj(x0, y0f, z0) else {
                continue;
            };
            let Some((sx1, sy1f, sz1)) = proj(x1, y1f, z1) else {
                continue;
            };
            let Some((_sx0c, sy0c, _)) = proj(x0c, y0c, z0c) else {
                continue;
            };
            let Some((_sx1c, sy1c, _)) = proj(x1c, y1c, z1c) else {
                continue;
            };
            let (left, right, yf_l, yf_r, yc_l, yc_r, zl, zr, u0, u1) = if sx0 <= sx1 {
                (sx0, sx1, sy0f, sy1f, sy0c, sy1c, sz0, sz1, 0_i32, wall_len)
            } else {
                (sx1, sx0, sy1f, sy0f, sy1c, sy0c, sz1, sz0, wall_len, 0_i32)
            };
            if right <= left {
                continue;
            }
            let flat = wall_color(wall.picnum, wall.shade);
            let span = (right - left) as i64;
            let xrepeat = wall.xrepeat.max(1) as i32;
            let yrepeat = wall.yrepeat.max(1) as i32;
            for col in left.max(0)..right.min(w) {
                let t = (i64::from(col - left) << 16) / span;
                let y_floor = yf_l + ((((yf_r - yf_l) as i64) * t) >> 16) as i32;
                let y_ceil = yc_l + ((((yc_r - yc_l) as i64) * t) >> 16) as i32;
                let z = zl + ((((zr - zl) as i64) * t) >> 16) as i32;
                let u = u0 + ((((u1 - u0) as i64) * t) >> 16) as i32;
                let cu = col as usize;
                if z >= zbuf[cu] {
                    continue;
                }
                zbuf[cu] = z;
                let y0 = y_ceil.min(y_floor).max(0);
                let y1 = y_ceil.max(y_floor).min(h);
                let yspan = (y1 - y0).max(1);
                for y in y0..y1 {
                    let v = ((i64::from(y - y0) << 8) / i64::from(yspan)) as i32;
                    let pix = tiles
                        .and_then(|ts| {
                            let uu = ((u.wrapping_mul(xrepeat)) / 8).max(0) as u32;
                            let vv = ((v.wrapping_mul(yrepeat)) / 8).max(0) as u32;
                            ts.sample(wall.picnum, uu, vv)
                                .filter(|&p| p != room_build::art::TRANSPARENT)
                        })
                        .unwrap_or(flat);
                    frame.put(col, y, pix);
                }
            }
        }
    }
    zbuf
}

/// Face sprites in visible sectors (bootstrap). Skips invisible / wall / floor
/// aligned (`cstat` 16/32/32768). Depth-tested against the wall z-buffer.
pub fn render_fp_sprites(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
) {
    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let w = frame.width as i32;
    let h = frame.height as i32;
    let cx = w / 2;
    let cy = h / 2;
    let focal = w;
    let horiz_off = (i32::from(view.horiz) - 100) / 2;

    for i in 0..crate::world::MAXSPRITES {
        let sp = &world.sprites[i];
        if sp.sectnum < 0
            || lists_stat_free(world, i)
            || (sp.cstat & CSTAT_INVISIBLE) != 0
            || (sp.cstat & CSTAT_WALL_ALIGN) != 0
            || (sp.cstat & CSTAT_FLOOR_ALIGN) != 0
        {
            continue;
        }
        if !view.visible_sectors.contains(&sp.sectnum) {
            continue;
        }
        let Some((rx, ry, rz)) = cam_project(view, sin, cos, sp.x, sp.y, sp.z) else {
            continue;
        };
        if rz <= 16 {
            continue;
        }
        let sx = cx as i64 + (focal as i64 * rx) / rz;
        let sy = cy as i64 + (focal as i64 * ry) / rz - i64::from(horiz_off);
        let z = rz as i32;
        // Screen size from Build repeats + depth (rough classic face scale).
        let xrep = i32::from(sp.xrepeat).max(1);
        let yrep = i32::from(sp.yrepeat).max(1);
        let half_w = ((i64::from(xrep) * i64::from(focal)) / (rz * 4)).max(1) as i32;
        let half_h = ((i64::from(yrep) * i64::from(focal)) / (rz * 4)).max(1) as i32;
        let left = sx as i32 - half_w;
        let right = sx as i32 + half_w;
        let top = sy as i32 - half_h;
        let bot = sy as i32 + half_h;
        let flat = wall_color(sp.picnum, sp.shade);
        let span_x = (right - left).max(1);
        let span_y = (bot - top).max(1);
        for col in left.max(0)..right.min(w) {
            let cu = col as usize;
            if cu >= zbuf.len() || z >= zbuf[cu] {
                continue;
            }
            let u = (((col - left) as i64 * 64) / i64::from(span_x)) as u32;
            for y in top.max(0)..bot.min(h) {
                let v = (((y - top) as i64 * 64) / i64::from(span_y)) as u32;
                let pix = tiles
                    .and_then(|ts| {
                        ts.sample(sp.picnum, u, v)
                            .filter(|&p| p != room_build::art::TRANSPARENT)
                    })
                    .unwrap_or(flat);
                frame.put(col, y, pix);
            }
            // Face sprites write z so nearer sprites occlude farther ones.
            zbuf[cu] = z;
        }
    }
}

fn lists_stat_free(world: &BuildWorld, i: usize) -> bool {
    world.lists.sprite_stat[i] == crate::world::MAXSTATUS as i16
}

fn wall_color(picnum: i16, shade: i8) -> u8 {
    let base = (picnum.wrapping_mul(37).wrapping_add(17) as u8).max(32);
    let sh = (shade as i16).clamp(0, 48) as u8;
    base.saturating_sub(sh / 2).max(16)
}

/// First-person wireframe: project solid walls in camera space and draw edges.
///
/// Bootstrap only (no textures/flats/sky, no occlusion). Uses Q14 sin/cos.
pub fn render_fp_wire(world: &BuildWorld, view: &BuildView, frame: &mut SoftFrame) {
    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let w = frame.width as i32;
    let h = frame.height as i32;
    let cx = w / 2;
    let cy = h / 2;
    let focal = w; // ~90° FOV-ish
    for &s in &view.visible_sectors {
        if s < 0 || s >= world.numsectors {
            continue;
        }
        let sec = &world.sectors[s as usize];
        let start = sec.wallptr as usize;
        let end = start
            .saturating_add(sec.wallnum as usize)
            .min(world.walls.len());
        for wall in &world.walls[start..end] {
            if wall.nextsector >= 0 {
                continue; // portals: skip wire for now
            }
            let p2 = wall.point2 as usize;
            if p2 >= world.walls.len() {
                continue;
            }
            let b = &world.walls[p2];
            let Some((sx0, sy0, sz0)) = cam_project(view, sin, cos, wall.x, wall.y, sec.floorz)
            else {
                continue;
            };
            let Some((sx1, sy1, sz1)) = cam_project(view, sin, cos, b.x, b.y, sec.floorz) else {
                continue;
            };
            let Some((sx2, sy2, sz2)) = cam_project(view, sin, cos, b.x, b.y, sec.ceilingz) else {
                continue;
            };
            let Some((sx3, sy3, sz3)) = cam_project(view, sin, cos, wall.x, wall.y, sec.ceilingz)
            else {
                continue;
            };
            // Perspective: screen_x = cx + focal * x / z, screen_y = cy + focal * y / z
            let proj = |x: i64, y: i64, z: i64| -> Option<(i32, i32)> {
                if z <= 16 {
                    return None;
                }
                let px = cx as i64 + (focal as i64 * x) / z;
                let py = cy as i64 + (focal as i64 * y) / z;
                Some((px as i32, py as i32))
            };
            let pts = [
                proj(sx0, sy0, sz0),
                proj(sx1, sy1, sz1),
                proj(sx2, sy2, sz2),
                proj(sx3, sy3, sz3),
            ];
            // Quad edges 0-1-2-3-0
            let order = [(0, 1), (1, 2), (2, 3), (3, 0)];
            for (a, b) in order {
                if let (Some(p), Some(q)) = (pts[a], pts[b]) {
                    frame.line(p.0, p.1, q.0, q.1, 200);
                }
            }
        }
    }
}

/// World → camera: +Z forward, +X right, +Y up (Build Z inverted to up).
fn cam_project(
    view: &BuildView,
    sin: i32,
    cos: i32,
    x: i32,
    y: i32,
    z: i32,
) -> Option<(i64, i64, i64)> {
    let dx = i64::from(x) - i64::from(view.posx);
    let dy = i64::from(y) - i64::from(view.posy);
    // Rotate by -ang into camera forward (+Z).
    let rx = (dx * i64::from(cos) + dy * i64::from(sin)) >> 14;
    let rz = (-dx * i64::from(sin) + dy * i64::from(cos)) >> 14;
    let ry = (i64::from(view.posz) - i64::from(z)) >> 4; // Build z→rough world
    if rz <= 0 {
        return None;
    }
    Some((rx, ry, rz))
}

/// Top-down wireframe of walls in `view.visible_sectors` (debug / golden).
///
/// Map XY is scaled into the frame with `view` position centred.
pub fn render_topdown(world: &BuildWorld, view: &BuildView, frame: &mut SoftFrame, scale: i32) {
    let scale = scale.max(1);
    let cx = (frame.width / 2) as i32;
    let cy = (frame.height / 2) as i32;
    let map = |x: i32, y: i32| -> (i32, i32) {
        let sx = cx + (x - view.posx) / scale;
        let sy = cy + (y - view.posy) / scale;
        (sx, sy)
    };
    for &s in &view.visible_sectors {
        if s < 0 || s >= world.numsectors {
            continue;
        }
        let sec = &world.sectors[s as usize];
        let start = sec.wallptr as usize;
        let end = start
            .saturating_add(sec.wallnum as usize)
            .min(world.walls.len());
        for w in &world.walls[start..end] {
            let a = (w.x, w.y);
            let b = {
                let p2 = w.point2 as usize;
                if p2 >= world.walls.len() {
                    continue;
                }
                (world.walls[p2].x, world.walls[p2].y)
            };
            let (x0, y0) = map(a.0, a.1);
            let (x1, y1) = map(b.0, b.1);
            let color = if w.nextsector >= 0 { 16 } else { 255 };
            frame.line(x0, y0, x1, y1, color);
        }
    }
    // Camera marker.
    frame.put(cx, cy, 128);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BuildWorld;
    use crate::types::{SectorT, SpriteT, WallT};

    #[test]
    fn fp_flats_fill_horizon() {
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            floorpicnum: 3,
            ceilingpicnum: 4,
            floorshade: 0,
            ceilingshade: 0,
            ..SectorT::default()
        }];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 400,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 400,
                y: 400,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 400,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        let w = BuildWorld::load_arrays(sectors, walls, &[], [200, 200, -1024], 0, 100, 0).unwrap();
        let v = w.build_view(200, 200, -1024, 0, 100, 0);
        let mut f = SoftFrame::new(64, 48);
        fill_fp_flats_sky(&w, &v, &mut f, None);
        let floor_c = wall_color(3, 0);
        let ceil_c = wall_color(4, 0);
        assert_eq!(f.pixels[0], ceil_c); // top-left = ceiling
        let last = f.pixels.len() - 1;
        assert_eq!(f.pixels[last], floor_c); // bottom-right = floor
    }

    #[test]
    fn fp_sky_parallax_differs_from_flat_ceiling() {
        let mut sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            ceilingpicnum: 89,
            ceilingstat: 1, // parallax
            ..SectorT::default()
        }];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 200,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 200,
                y: 200,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 200,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        let w = BuildWorld::load_arrays(
            sectors.clone(),
            walls.clone(),
            &[],
            [100, 100, -1024],
            0,
            100,
            0,
        )
        .unwrap();
        let v = w.build_view(100, 100, -1024, 256, 100, 0);
        let mut sky = SoftFrame::new(32, 24);
        fill_fp_flats_sky(&w, &v, &mut sky, None);
        sectors[0].ceilingstat = 0;
        let w2 =
            BuildWorld::load_arrays(sectors, walls, &[], [100, 100, -1024], 0, 100, 0).unwrap();
        let mut flat = SoftFrame::new(32, 24);
        fill_fp_flats_sky(&w2, &v, &mut flat, None);
        assert_ne!(
            sky.pixels[1], flat.pixels[1],
            "parallax sky should differ from flat ceil"
        );
    }

    #[test]
    fn fp_textured_samples_tile() {
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            ..SectorT::default()
        }];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                picnum: 7,
                xrepeat: 8,
                yrepeat: 8,
                ..WallT::default()
            },
            WallT {
                x: 400,
                y: 0,
                point2: 2,
                nextsector: -1,
                picnum: 7,
                xrepeat: 8,
                yrepeat: 8,
                ..WallT::default()
            },
            WallT {
                x: 400,
                y: 400,
                point2: 3,
                nextsector: -1,
                picnum: 7,
                xrepeat: 8,
                yrepeat: 8,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 400,
                point2: 0,
                nextsector: -1,
                picnum: 7,
                xrepeat: 8,
                yrepeat: 8,
                ..WallT::default()
            },
        ];
        // 2x2 tile: distinct colours
        let mut tiles = SoftTiles::new();
        tiles.insert(7, 2, 2, vec![10, 20, 30, 40]);
        let w = BuildWorld::load_arrays(sectors, walls, &[], [200, 80, -1024], 512, 0, 1).unwrap();
        let v = w.build_view(200, 80, -1024, 512, 100, 0);
        let mut f = SoftFrame::new(160, 100);
        render_fp_textured(&w, &v, &mut f, &tiles);
        let hit = f.pixels.iter().any(|&p| matches!(p, 10 | 20 | 30 | 40));
        assert!(hit, "expected textured palette indices");
    }

    #[test]
    fn fp_solid_fills_columns() {
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            ..SectorT::default()
        }];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                picnum: 1,
                ..WallT::default()
            },
            WallT {
                x: 400,
                y: 0,
                point2: 2,
                nextsector: -1,
                picnum: 2,
                ..WallT::default()
            },
            WallT {
                x: 400,
                y: 400,
                point2: 3,
                nextsector: -1,
                picnum: 3,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 400,
                point2: 0,
                nextsector: -1,
                picnum: 4,
                ..WallT::default()
            },
        ];
        let w = BuildWorld::load_arrays(sectors, walls, &[], [200, 80, -1024], 512, 0, 1).unwrap();
        let v = w.build_view(200, 80, -1024, 512, 100, 0);
        let mut f = SoftFrame::new(160, 100);
        render_fp_solid(&w, &v, &mut f);
        let ink = f.pixels.iter().filter(|&&p| p >= 16).count();
        assert!(ink > 100, "expected solid wall fill, got {ink}");
    }

    #[test]
    fn fp_wire_draws_something() {
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            ..SectorT::default()
        }];
        // Room [0,400]²; camera near south wall looking +y (ang 512).
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 400,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 400,
                y: 400,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 400,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        let w = BuildWorld::load_arrays(sectors, walls, &[], [200, 80, -1024], 512, 0, 1).unwrap();
        let v = w.build_view(200, 80, -1024, 512, 100, 0);
        let mut f = SoftFrame::new(160, 100);
        render_fp_wire(&w, &v, &mut f);
        let ink = f.pixels.iter().filter(|&&p| p == 200).count();
        assert!(ink > 10, "expected FP wire pixels, got {ink}");
    }

    #[test]
    fn fp_solid_draws_face_sprite() {
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            floorz: 0,
            ceilingz: -(64 << 8),
            ..SectorT::default()
        }];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 2000,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 2000,
                y: 2000,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 2000,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        // Soft cam: ang 0 looks +Y (`cam_project` rz = dy).
        let sprites = vec![SpriteT {
            x: 1000,
            y: 1400,
            z: -(32 << 8),
            picnum: 99,
            sectnum: 0,
            statnum: 0,
            xrepeat: 64,
            yrepeat: 64,
            shade: 0,
            ..SpriteT::default()
        }];
        let w = BuildWorld::load_arrays(sectors, walls, &sprites, [1000, 400, -(32 << 8)], 0, 0, 1)
            .unwrap();
        let v = w.build_view(1000, 400, -(32 << 8), 0, 100, 0);
        let mut f = SoftFrame::new(128, 96);
        render_fp_solid(&w, &v, &mut f);
        let sprite_flat = (99i16.wrapping_mul(37).wrapping_add(17) as u8).max(32);
        let hit = f.pixels.iter().filter(|&&p| p == sprite_flat).count();
        assert!(
            hit > 8,
            "expected face-sprite pixels (~{sprite_flat}), got {hit}"
        );
    }

    #[test]
    fn topdown_draws_walls() {
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ..SectorT::default()
        }];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 100,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 100,
                y: 100,
                point2: 3,
                nextsector: -1,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 100,
                point2: 0,
                nextsector: -1,
                ..WallT::default()
            },
        ];
        let w = BuildWorld::load_arrays(sectors, walls, &[], [50, 50, 0], 0, 0, 1).unwrap();
        let v = w.build_view(50, 50, 0, 0, 100, 0);
        let mut f = SoftFrame::new(64, 64);
        render_topdown(&w, &v, &mut f, 4);
        assert!(
            f.pixels.iter().any(|&p| p == 255),
            "expected solid wall ink"
        );
        assert_eq!(f.pixels[(32 * 64 + 32) as usize], 128); // camera
    }
}

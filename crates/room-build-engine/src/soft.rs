//! Classic-mode CPU helpers (debug / goldens). Not pixel-exact vs DOS Build.
//! Spec: `docs/25` §9 / D3 — full soft path may later live in `room-render-soft`.

use crate::types::SpriteT;
use crate::view::BuildView;
use crate::world::BuildWorld;
use std::collections::BTreeMap;

/// Owned ART tiles for soft textured draw (column-major pixels).
#[derive(Clone, Debug, Default)]
pub struct SoftTiles {
    tiles: BTreeMap<i16, (u16, u16, Vec<u8>)>,
    /// Build `LOOKUP.DAT` remaps keyed by sprite/wall palette.
    palette_lookups: BTreeMap<u8, [u8; 256]>,
    /// Build shade tables from `PALETTE.DAT`, brightest first.
    shade_tables: Vec<u8>,
    shade_count: usize,
    /// Build's optional indexed-color translucency lookup from `PALETTE.DAT`.
    translucency: Option<Vec<u8>>,
    /// ART picanm cycle and frame duration in 120 Hz Build clock ticks.
    animations: BTreeMap<i16, (Vec<i16>, u32)>,
    /// ART picanm sprite-origin offsets, keyed by base tile.
    sprite_offsets: BTreeMap<i16, (i8, i8)>,
    /// Per-sky 45-degree ART tile offsets (`prelevel`'s `pskyoff[]`).
    sky_offsets: BTreeMap<i16, [i16; 8]>,
    /// Per-sky Duke `parallaxyscale` (16.16; absent = 65536).
    sky_yscale: BTreeMap<i16, i32>,
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
                out.sprite_offsets
                    .insert(pic, (tile.x_offset(), tile.y_offset()));
                let count = (tile.picanm & 63) as i16;
                let speed = (tile.picanm >> 24) & 15;
                let frames: Vec<i16> = match (tile.picanm >> 6) & 3 {
                    1 if count > 0 => (0..=count).chain((1..count).rev()).collect(),
                    2 if count > 0 => (0..=count).collect(),
                    3 if count > 0 => (0..=count).map(|step| -step).collect(),
                    _ => Vec::new(),
                }
                .into_iter()
                .map(|offset| pic.saturating_add(offset))
                .filter(|frame| set.get(i32::from(*frame)).is_some())
                .collect();
                if frames.len() > 1 {
                    out.animations.insert(pic, (frames, 1_u32 << speed));
                }
            }
        }
        out
    }

    fn sprite_offset(&self, picnum: i16) -> (i8, i8) {
        self.sprite_offsets
            .get(&picnum)
            .copied()
            .unwrap_or_default()
    }

    /// Sample one tile in palette indices; `None` means the tile is unavailable.
    #[must_use]
    pub fn sample(&self, picnum: i16, u: u32, v: u32) -> Option<u8> {
        self.sample_at(picnum, u, v, 0)
    }

    /// Resolve a tile (with its ART animation frame) once, for span drawing.
    pub(crate) fn resolve(&self, picnum: i16, clock_120hz: u32) -> Option<TileRef<'_>> {
        let resolved = self
            .animations
            .get(&picnum)
            .map_or(picnum, |(frames, step)| {
                let frame = (clock_120hz / (*step).max(1)) as usize % frames.len();
                frames[frame]
            });
        let (w, h, pix) = self.tiles.get(&resolved)?;
        (*w != 0 && *h != 0).then_some(TileRef {
            w: usize::from(*w),
            h: usize::from(*h),
            pix,
        })
    }

    /// `LOOKUP.DAT` remap for `pal` (`None` = identity).
    pub(crate) fn pal_table(&self, pal: u8) -> Option<&[u8; 256]> {
        self.palette_lookups.get(&pal)
    }

    /// Shade row for `shade` (`None` without shade tables).
    pub(crate) fn shade_row(&self, shade: i8) -> Option<&[u8]> {
        if self.shade_count == 0 {
            return None;
        }
        let row = usize::from(shade.max(0) as u8).min(self.shade_count - 1);
        self.shade_tables.get(row * 256..row * 256 + 256)
    }

    /// Sample a tile at a 120 Hz Build animation clock.
    #[must_use]
    pub fn sample_at(&self, picnum: i16, u: u32, v: u32, clock_120hz: u32) -> Option<u8> {
        let resolved = self
            .animations
            .get(&picnum)
            .map_or(picnum, |(frames, step)| {
                let frame = (clock_120hz / (*step).max(1)) as usize % frames.len();
                frames[frame]
            });
        let (w, h, pix) = self.tiles.get(&resolved)?;
        if *w == 0 || *h == 0 {
            return None;
        }
        let x = (u as usize) % (*w as usize);
        let y = (v as usize) % (*h as usize);
        // column-major: pixels[x * height + y]
        pix.get(x * (*h as usize) + y).copied()
    }

    /// Pixel dimensions of a loaded tile.
    #[must_use]
    pub fn dimensions(&self, picnum: i16) -> Option<(u16, u16)> {
        self.tiles.get(&picnum).map(|(w, h, _)| (*w, *h))
    }

    /// Install one Build palette remap table from `LOOKUP.DAT`.
    pub fn set_palette_lookup(&mut self, pal: u8, table: [u8; 256]) {
        if pal != 0 {
            self.palette_lookups.insert(pal, table);
        }
    }

    /// Install Build shade tables from `PALETTE.DAT`.
    pub fn set_shade_tables(&mut self, count: usize, tables: Vec<u8>) {
        let count = count.min(tables.len() / 256);
        self.shade_tables = tables.into_iter().take(count * 256).collect();
        self.shade_count = count;
    }

    /// Install the optional 256×256 Build translucency lookup.
    pub fn set_translucency_table(&mut self, table: Option<Vec<u8>>) {
        self.translucency = table.filter(|bytes| bytes.len() >= 65_536);
    }

    fn composite_sprite(&self, background: u8, foreground: u8, cstat: u16) -> u8 {
        const TRANSLUCENT: u16 = 2;
        const REVERSE_TRANSLUCENT: u16 = 512;
        self.composite_layer(
            background,
            foreground,
            cstat & TRANSLUCENT != 0,
            cstat & REVERSE_TRANSLUCENT != 0,
        )
    }

    fn composite_layer(
        &self,
        background: u8,
        foreground: u8,
        translucent: bool,
        reverse: bool,
    ) -> u8 {
        if !translucent {
            return foreground;
        }
        let Some(table) = self.translucency.as_deref() else {
            return foreground;
        };
        let (back, front) = if reverse {
            (foreground, background)
        } else {
            (background, foreground)
        };
        table[usize::from(back) * 256 + usize::from(front)]
    }

    /// Remap a non-transparent ART index by a Build sprite/wall palette.
    #[must_use]
    pub fn remap_palette(&self, pal: u8, pixel: u8) -> u8 {
        if pixel == room_build::art::TRANSPARENT {
            return pixel;
        }
        self.palette_lookups
            .get(&pal)
            .map_or(pixel, |table| table[usize::from(pixel)])
    }

    /// Apply a Build shade row after palette remapping. Negative shades are
    /// full-bright and transparent ART pixels remain transparent.
    #[must_use]
    pub fn shade_palette(&self, shade: i8, pixel: u8) -> u8 {
        if pixel == room_build::art::TRANSPARENT || self.shade_count == 0 {
            return pixel;
        }
        let row = usize::from(shade.max(0) as u8).min(self.shade_count - 1);
        self.shade_tables[row * 256 + usize::from(pixel)]
    }

    /// Set the Atomic `pskyoff[]` frame offsets for a parallax base tile.
    pub fn set_sky_offsets(&mut self, base: i16, offsets: [i16; 8]) {
        self.sky_offsets.insert(base, offsets);
    }

    /// Set Duke `parallaxyscale` (16.16) for a parallax base tile: how far
    /// the sky follows the view horizon when looking up or down.
    pub fn set_sky_yscale(&mut self, base: i16, scale: i32) {
        self.sky_yscale.insert(base, scale);
    }

    /// Per-column part of [`Self::sample_sky_parascan`]: the sky tile for
    /// this view angle, its texel column and the horizon the sky follows.
    pub(crate) fn sky_column(
        &self,
        base: i16,
        angle: i16,
        col: i32,
        width: i32,
        height: i32,
        horizon: i32,
        clock_120hz: u32,
    ) -> Option<(TileRef<'_>, i64, i64)> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let look = (i32::from(angle) + ((col - width / 2) * 512) / width).rem_euclid(2048);
        let slot = (look >> 8) as usize;
        let offset = self.sky_offsets.get(&base).map_or(0, |o| o[slot]);
        let tile = self.resolve(base.saturating_add(offset), clock_120hz)?;
        let u = i64::from(look & 255) * tile.w as i64 / 256;
        let yscale = i64::from(self.sky_yscale.get(&base).copied().unwrap_or(65536));
        let half = i64::from(height / 2);
        let sky_horizon = half + ((i64::from(horizon) - half) * yscale >> 16);
        Some((tile, u, sky_horizon))
    }

    /// Build `parascan`: the sky is a wall at fixed distance whose middle
    /// texel row sits on the (scaled) horizon at one texel per 200-line row,
    /// wrapping vertically. `row` and `horizon` are framebuffer rows.
    #[allow(clippy::too_many_arguments)]
    fn sample_sky_parascan(
        &self,
        base: i16,
        angle: i16,
        col: i32,
        width: i32,
        height: i32,
        row: i32,
        horizon: i32,
        ypanning: u8,
        clock_120hz: u32,
    ) -> Option<u8> {
        let (tile_width, tile_height) = self.dimensions(base)?;
        if width <= 0 || height <= 0 {
            return None;
        }
        let look = (i32::from(angle) + ((col - width / 2) * 512) / width).rem_euclid(2048);
        let slot = (look >> 8) as usize;
        let offset = self.sky_offsets.get(&base).map_or(0, |o| o[slot]);
        let pic = base.saturating_add(offset);
        let u = ((look & 255) * i32::from(tile_width) / 256) as u32;
        let yscale = i64::from(self.sky_yscale.get(&base).copied().unwrap_or(65536));
        let half = i64::from(height / 2);
        let sky_horizon = half + ((i64::from(horizon) - half) * yscale >> 16);
        let th = i64::from(tile_height.max(1));
        let v = th / 2 + (i64::from(row) - sky_horizon) * 200 / i64::from(height)
            + i64::from(ypanning) * th / 256;
        let v = v.rem_euclid(th) as u32;
        self.sample_at(pic, u, v, clock_120hz)
            .filter(|&p| p != room_build::art::TRANSPARENT)
    }

    fn sample_sky_at(
        &self,
        base: i16,
        angle: i16,
        col: i32,
        width: i32,
        y: i32,
        band: i32,
        clock_120hz: u32,
    ) -> Option<u8> {
        let (tile_width, tile_height) = self.dimensions(base)?;
        if width <= 0 {
            return None;
        }
        // The bootstrap projection has a 90-degree horizontal field of view.
        // Build's 2048-unit circle is divided into eight 256-unit sky slots.
        let look = (i32::from(angle) + ((col - width / 2) * 512) / width).rem_euclid(2048);
        let slot = (look >> 8) as usize;
        let offset = self.sky_offsets.get(&base).map_or(0, |o| o[slot]);
        let pic = base.saturating_add(offset);
        let u = ((look & 255) * i32::from(tile_width) / 256) as u32;
        let v = if band > 0 {
            (y * i32::from(tile_height) / band.max(1)).clamp(0, i32::from(tile_height) - 1) as u32
        } else {
            0
        };
        self.sample_at(pic, u, v, clock_120hz)
            .filter(|&p| p != room_build::art::TRANSPARENT)
    }
}

/// A resolved ART tile (column-major pixels) for per-span sampling.
#[derive(Clone, Copy)]
pub(crate) struct TileRef<'a> {
    pub w: usize,
    pub h: usize,
    pub pix: &'a [u8],
}

impl TileRef<'_> {
    /// Texel at wrapped `(u, v)`.
    #[inline]
    pub fn at(&self, u: i64, v: i64) -> u8 {
        let x = u.rem_euclid(self.w as i64) as usize;
        let y = v.rem_euclid(self.h as i64) as usize;
        self.pix.get(x * self.h + y).copied().unwrap_or(room_build::art::TRANSPARENT)
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
#[path = "soft_portal.rs"]
mod portal;

/// Build distance fog for a sprite pixel (see `portal::Ctx::fog_shade`).
fn sprite_fog_shade(world: Option<&BuildWorld>, shade: i8, depth: i64, sect: i16) -> i8 {
    let v = world
        .and_then(|w| w.sectors.get(sect as usize))
        .map_or(0, |s| s.visibility);
    let globvis = if v == 0 {
        512_i64
    } else {
        (512 * i64::from(v.wrapping_add(16))) >> 4
    };
    (i64::from(shade) + ((depth.max(0) * globvis) >> 18)).clamp(0, 127) as i8
}

pub fn render_fp_textured(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
) {
    render_fp_textured_at(world, view, frame, tiles, 0);
}

/// Textured first-person draw with ART animations evaluated at Build's
/// 120 Hz clock. `clock_120hz` is derived from the deterministic game tic.
pub fn render_fp_textured_at(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
    clock_120hz: u32,
) {
    let zbuf = render_fp_textured_without_mirrors_at(world, view, frame, tiles, clock_120hz);
    render_fp_mirror_at(world, view, frame, tiles, &zbuf, clock_120hz, 560);
}

fn render_fp_textured_without_mirrors_at(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
    clock_120hz: u32,
) -> Vec<i32> {
    let (mut zbuf, drawn) = portal::render(world, view, frame, tiles, clock_120hz);
    // Sprites are drawn for every sector the portal pass actually reached.
    let mut sprite_view = view.clone();
    sprite_view.visible_sectors = drawn;
    render_fp_sprites_at(world, &sprite_view, frame, Some(tiles), &mut zbuf, clock_120hz);
    zbuf
}

/// Draw the nearest visible Atomic planar mirror from a reflected camera.
///
/// This bootstrap path renders one mirror per frame, matching Chocolate's
/// nearest-visible mirror choice. The reflected pass is deliberately
/// non-recursive so opposing mirrors cannot recurse indefinitely.
fn render_fp_mirror_at(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
    scene_depth: &[i32],
    clock_120hz: u32,
    mirror_tile: i16,
) {
    let mut nearest: Option<(i64, usize, i16)> = None;
    for &sector_index in &view.visible_sectors {
        let Some(sector) = usize::try_from(sector_index)
            .ok()
            .and_then(|index| world.sectors.get(index))
        else {
            continue;
        };
        let start = usize::try_from(sector.wallptr).unwrap_or(usize::MAX);
        let end = start
            .saturating_add(usize::try_from(sector.wallnum).unwrap_or(0))
            .min(world.walls.len());
        let Some(walls) = world.walls.get(start..end) else {
            continue;
        };
        for (offset, wall) in walls.iter().enumerate() {
            if wall.cstat & 32 == 0 || wall.nextsector < 0 {
                continue;
            }
            // `prelevel` resolves MIRROR through the active CON and marks
            // its destination sector with that tile. Prefer this runtime
            // marker so add-on CONs need not use Atomic's tile 560; retain
            // the Atomic fallback for callers that render an unprepared map.
            let prepared_mirror = usize::try_from(wall.nextsector)
                .ok()
                .and_then(|index| world.sectors.get(index))
                .is_some_and(|sector| {
                    sector.ceilingpicnum == wall.overpicnum && sector.floorpicnum == wall.overpicnum
                });
            if wall.overpicnum != mirror_tile && !prepared_mirror {
                continue;
            }
            let wall_index = start + offset;
            let Some(endpoint) = usize::try_from(wall.point2)
                .ok()
                .and_then(|index| world.walls.get(index))
            else {
                continue;
            };
            // Chocolate's displayrooms chooses a mirror by Manhattan
            // distance to the source wall's first vertex, not its midpoint.
            let distance = (i64::from(wall.x) - i64::from(view.posx)).abs()
                + (i64::from(wall.y) - i64::from(view.posy)).abs();
            if let Some(source_sector) = world.sectors.get(sector_index as usize) {
                let Some(quad) = project_mirror_quad(
                    view,
                    frame.width,
                    frame.height,
                    (wall.x, wall.y),
                    (endpoint.x, endpoint.y),
                    source_sector.ceilingz,
                    source_sector.floorz,
                ) else {
                    continue;
                };
                let min_x = quad.iter().map(|point| point.0).min().unwrap_or_default();
                let max_x = quad.iter().map(|point| point.0).max().unwrap_or_default();
                let min_y = quad.iter().map(|point| point.1).min().unwrap_or_default();
                let max_y = quad.iter().map(|point| point.1).max().unwrap_or_default();
                if max_x < 0
                    || min_x >= frame.width as i32
                    || max_y < 0
                    || min_y >= frame.height as i32
                {
                    continue;
                }
            } else {
                continue;
            }
            if nearest.is_none_or(|(best, _, _)| distance < best) {
                nearest = Some((distance, wall_index, sector_index));
            }
        }
    }
    let Some((_, wall_index, owner_sector)) = nearest else {
        return;
    };
    let Some(wall) = world.walls.get(wall_index).copied() else {
        return;
    };
    let Some(endpoint) = usize::try_from(wall.point2)
        .ok()
        .and_then(|index| world.walls.get(index).copied())
    else {
        return;
    };
    let Some(source_sector) = usize::try_from(owner_sector)
        .ok()
        .and_then(|index| world.sectors.get(index))
    else {
        return;
    };
    let Some(reflected) =
        reflect_camera(view.posx, view.posy, wall.x, wall.y, endpoint.x, endpoint.y)
    else {
        return;
    };
    let wall_angle = crate::helpers::getangle(
        endpoint.x.wrapping_sub(wall.x),
        endpoint.y.wrapping_sub(wall.y),
    );
    let reflected_angle = (wall_angle * 2 - i32::from(view.ang)).rem_euclid(2048) as i16;
    let reflected_view = world.build_view(
        reflected.0,
        reflected.1,
        view.posz,
        reflected_angle,
        view.horiz,
        wall.nextsector,
    );
    let mut reflected_frame = SoftFrame::new(frame.width, frame.height);
    render_fp_textured_without_mirrors_at(
        world,
        &reflected_view,
        &mut reflected_frame,
        tiles,
        clock_120hz,
    );

    let Some(points) = project_mirror_quad(
        view,
        frame.width,
        frame.height,
        (wall.x, wall.y),
        (endpoint.x, endpoint.y),
        source_sector.ceilingz,
        source_sector.floorz,
    ) else {
        return;
    };
    draw_mirror_triangle(
        frame,
        &reflected_frame,
        scene_depth,
        [points[0], points[1], points[2]],
    );
    draw_mirror_triangle(
        frame,
        &reflected_frame,
        scene_depth,
        [points[0], points[2], points[3]],
    );
}

fn reflect_camera(x: i32, y: i32, ax: i32, ay: i32, bx: i32, by: i32) -> Option<(i32, i32)> {
    let dx = i128::from(bx) - i128::from(ax);
    let dy = i128::from(by) - i128::from(ay);
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0 {
        return None;
    }
    let px = i128::from(x) - i128::from(ax);
    let py = i128::from(y) - i128::from(ay);
    let projection = px * dx + py * dy;
    let projected_x = i128::from(ax) + projection * dx / length_squared;
    let projected_y = i128::from(ay) + projection * dy / length_squared;
    Some((
        i32::try_from(projected_x * 2 - i128::from(x)).ok()?,
        i32::try_from(projected_y * 2 - i128::from(y)).ok()?,
    ))
}

fn project_mirror_quad(
    view: &BuildView,
    width: u32,
    height: u32,
    a: (i32, i32),
    b: (i32, i32),
    top_z: i32,
    bottom_z: i32,
) -> Option<[(i32, i32, i32); 4]> {
    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let project = |x, y, z| {
        let (rx, ry, rz) = cam_project(view, sin, cos, x, y, z)?;
        if rz <= 16 {
            return None;
        }
        let cx = width as i64 / 2;
        let cy = height as i64 / 2;
        let fx = cx.max(1);
        let fy = cy.max(1);
        let horizon = (i32::from(view.horiz) - 100) / 2;
        Some((
            i32::try_from(cx + fx * i64::from(rx) / i64::from(rz)).ok()?,
            i32::try_from(cy - fy * i64::from(ry) / i64::from(rz) - i64::from(horizon)).ok()?,
            i32::try_from(rz).ok()?,
        ))
    };
    Some([
        project(a.0, a.1, top_z)?,
        project(b.0, b.1, top_z)?,
        project(b.0, b.1, bottom_z)?,
        project(a.0, a.1, bottom_z)?,
    ])
}

fn draw_mirror_triangle(
    target: &mut SoftFrame,
    reflected: &SoftFrame,
    scene_depth: &[i32],
    points: [(i32, i32, i32); 3],
) {
    let [(x0, y0, z0), (x1, y1, z1), (x2, y2, z2)] = points;
    let edge = |ax: i32, ay: i32, bx: i32, by: i32, px: i32, py: i32| {
        i64::from(px - ax) * i64::from(by - ay) - i64::from(py - ay) * i64::from(bx - ax)
    };
    let area = edge(x0, y0, x1, y1, x2, y2);
    if area == 0 {
        return;
    }
    let min_x = x0.min(x1).min(x2).max(0);
    let max_x = x0.max(x1).max(x2).min(target.width as i32 - 1);
    let min_y = y0.min(y1).min(y2).max(0);
    let max_y = y0.max(y1).max(y2).min(target.height as i32 - 1);
    let width = target.width as usize;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let w0 = edge(x1, y1, x2, y2, x, y);
            let w1 = edge(x2, y2, x0, y0, x, y);
            let w2 = edge(x0, y0, x1, y1, x, y);
            if (area > 0 && (w0 < 0 || w1 < 0 || w2 < 0))
                || (area < 0 && (w0 > 0 || w1 > 0 || w2 > 0))
            {
                continue;
            }
            let depth = ((w0 * i64::from(z0) + w1 * i64::from(z1) + w2 * i64::from(z2)) / area)
                .max(1) as i32;
            let index = y as usize * width + x as usize;
            if index >= scene_depth.len() || scene_depth[index].saturating_add(2) < depth {
                continue;
            }
            target.pixels[index] = reflected.pixels[index];
        }
    }
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
    fill_fp_flats_sky_at(world, view, frame, tiles, 0);
}

/// Floor, ceiling and sky fill with ART animation evaluated at 120 Hz.
pub fn fill_fp_flats_sky_at(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    clock_120hz: u32,
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

    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let focal_x = (w / 2).max(1);
    let focal_y = (h / 2).max(1);
    let slope_sectors = world.sector_z();
    let slope_walls = world.wall_xy();
    for col in 0..w {
        for y in 0..h {
            let above_horizon = y < horizon;
            let (picnum, flat, parallax, xpanning, ypanning, plane_z, pal, sec_shade, plane_stat) =
                if above_horizon {
                    (
                        sec.ceilingpicnum,
                        ceil_flat,
                        sky_parallax,
                        sec.ceilingxpanning,
                        sec.ceilingypanning,
                        sec.ceilingz,
                        sec.ceilingpal,
                        sec.ceilingshade,
                        sec.ceilingstat,
                    )
                } else {
                    (
                        sec.floorpicnum,
                        floor_flat,
                        floor_parallax,
                        sec.floorxpanning,
                        sec.floorypanning,
                        sec.floorz,
                        sec.floorpal,
                        sec.floorshade,
                        sec.floorstat,
                    )
                };
            let pix = if parallax {
                tiles
                    .and_then(|ts| {
                        ts.sample_sky_at(
                            picnum,
                            view.ang,
                            col,
                            w,
                            if above_horizon { y } else { y - horizon },
                            if above_horizon { horizon } else { h - horizon },
                            clock_120hz,
                        )
                    })
                    .unwrap_or_else(|| sky_color(picnum, view.ang, col, y))
            } else {
                project_plane_pixel(
                    tiles,
                    picnum,
                    flat,
                    view,
                    sin,
                    cos,
                    col,
                    y,
                    horizon,
                    focal_x,
                    focal_y,
                    plane_z,
                    view.sect,
                    above_horizon,
                    &slope_sectors,
                    &slope_walls,
                    plane_stat,
                    xpanning,
                    ypanning,
                    clock_120hz,
                )
            };
            let pix = tiles.map_or(pix, |ts| {
                ts.shade_palette(sec_shade, ts.remap_palette(pal, pix))
            });
            frame.put(col, y, pix);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn project_plane_pixel(
    tiles: Option<&SoftTiles>,
    picnum: i16,
    flat: u8,
    view: &BuildView,
    sin: i32,
    cos: i32,
    col: i32,
    row: i32,
    horizon: i32,
    focal_x: i32,
    focal_y: i32,
    plane_z: i32,
    sectnum: i16,
    ceiling: bool,
    sectors: &[crate::geom::SectorZ],
    walls: &[crate::geom::WallXY],
    plane_stat: u16,
    xpanning: u8,
    ypanning: u8,
    clock_120hz: u32,
) -> u8 {
    let dy_screen = row - horizon;
    if dy_screen == 0 {
        return flat;
    }
    let flat_dz = (i64::from(view.posz) - i64::from(plane_z)) >> 4;
    let sloped = plane_stat & 2 != 0;
    if !sloped && (flat_dz == 0 || (dy_screen > 0) != (flat_dz < 0)) {
        return flat;
    }
    let mut depth = (i64::from(focal_y) * flat_dz.abs()) / i64::from(dy_screen.abs());
    if depth <= 0 && !sloped {
        return flat;
    }

    // Camera forward is (cos, sin); screen-right is (-sin, cos) (Build Y
    // points south). Rays stay fixed-point so the soft renderer uses the same
    // integer camera basis as wall projection.
    let side_q14 = (i64::from(col - focal_x) << 14) / i64::from(focal_x);
    let ray_x = i64::from(cos) - ((i64::from(sin) * side_q14) >> 14);
    let ray_y = i64::from(sin) + ((i64::from(cos) * side_q14) >> 14);
    // A Build slope is a plane, z = z_cam + gx*(x-posx) + gy*(y-posy): sample
    // it under the eye and solve the ray/plane intersection exactly (the old
    // fixed-point iteration diverged on steep slopes and fell back to `flat`).
    if plane_stat & 2 != 0 && (sectnum as usize) < sectors.len() {
        let z_at = |x: i32, y: i32| {
            let (c, f) = crate::geom::getzsofslope(sectnum, x, y, sectors, walls);
            f64::from(if ceiling { c } else { f })
        };
        const STEP: i32 = 4096;
        let z0 = z_at(view.posx, view.posy);
        let gx = (z_at(view.posx.saturating_add(STEP), view.posy) - z0) / f64::from(STEP);
        let gy = (z_at(view.posx, view.posy.saturating_add(STEP)) - z0) / f64::from(STEP);
        // Ray direction per unit of forward depth.
        let (rx, ry) = (ray_x as f64 / 16384.0, ray_y as f64 / 16384.0);
        let g = gx * rx + gy * ry;
        let fy = f64::from(focal_y);
        // Row below the horizon for plane height z at depth t:
        // dy = fy * (z - posz) / 16 / t, with z = z0 + g*t.
        let denom = f64::from(dy_screen) - g * fy / 16.0;
        if denom == 0.0 {
            return flat;
        }
        let t = (z0 - f64::from(view.posz)) * fy / 16.0 / denom;
        if !(t > 0.0) || !t.is_finite() {
            return flat;
        }
        depth = t.min(1.0e9) as i64;
        if depth <= 0 {
            return flat;
        }
    }
    let world_x = i64::from(view.posx) + ((ray_x * depth) >> 14);
    let world_y = i64::from(view.posy) + ((ray_y * depth) >> 14);
    let (base_tex_x, base_tex_y) = if plane_stat & 64 != 0 {
        // Build's relative alignment derives the plane's texture axes from
        // the sector's first wall. Use a normalized integer basis so the
        // texture keeps the unaligned path's world-space texel scale.
        let aligned = sectors
            .get(sectnum as usize)
            .and_then(|sector| walls.get(sector.wallptr.max(0) as usize))
            .and_then(|first| {
                walls
                    .get(first.point2.max(0) as usize)
                    .map(|next| (*first, *next))
            });
        if let Some((first, next)) = aligned {
            let dx = next.x.wrapping_sub(first.x);
            let dy = next.y.wrapping_sub(first.y);
            let len_sq = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) as u32;
            let len = i64::from(crate::numeric::nsqrtasm(len_sq));
            if len > 0 {
                let rel_x = world_x - i64::from(first.x);
                let rel_y = world_y - i64::from(first.y);
                let rel_u = (i128::from(rel_x) * i128::from(dx)
                    + i128::from(rel_y) * i128::from(dy))
                    / i128::from(len);
                let rel_v = (i128::from(rel_y) * i128::from(dx)
                    - i128::from(rel_x) * i128::from(dy))
                    / i128::from(len);
                (
                    rel_u.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
                    rel_v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
                )
            } else {
                (world_x, world_y)
            }
        } else {
            (world_x, world_y)
        }
    } else {
        (world_x, world_y)
    };
    let (mut tex_x, mut tex_y) = if plane_stat & 4 != 0 {
        (base_tex_y, base_tex_x)
    } else {
        (base_tex_x, base_tex_y)
    };
    if plane_stat & 16 != 0 {
        tex_x = -tex_x;
    }
    if plane_stat & 32 != 0 {
        tex_y = -tex_y;
    }
    let (tile_width, tile_height) = tiles
        .and_then(|ts| ts.dimensions(picnum))
        .unwrap_or((64, 64));
    // Chocolate's plane basis uses picsiz to scale each axis, so ART tile
    // dimensions affect world texel density. Sector bit 3 doubles physical
    // texel size. Keep texture coordinates in Q8 until the panning fraction
    // has been added: xpanning/ypanning are 1/256-tile values, not texels.
    // Chocolate `hlineasm4` takes the top log2(size) bits of a coordinate
    // pre-shifted by `8 - log2(size)`: one texel per 16 world units for any
    // tile size, twice as dense with sector bit 3.
    let scale_q8 = |_dimension: u16| if plane_stat & 8 != 0 { 32_i64 } else { 16_i64 };
    let u = ((tex_x * scale_q8(tile_width) + i64::from(xpanning) * i64::from(tile_width)) >> 8)
        .rem_euclid(1 << 30) as u32;
    let v = ((tex_y * scale_q8(tile_height) + i64::from(ypanning) * i64::from(tile_height)) >> 8)
        .rem_euclid(1 << 30) as u32;
    tiles
        .and_then(|ts| {
            ts.sample_at(picnum, u, v, clock_120hz)
                .filter(|&p| p != room_build::art::TRANSPARENT)
        })
        .unwrap_or(flat)
}

fn sky_color(picnum: i16, ang: i16, col: i32, y: i32) -> u8 {
    let base = (picnum.wrapping_mul(19).wrapping_add(40) as u8).max(48);
    let band = ((i32::from(ang) + col + y / 4) & 31) as u8;
    base.saturating_add(band / 4).min(200)
}

/// `cstat` bit 15 — invisible.
const CSTAT_INVISIBLE: u16 = 32768;
/// Build sprite cstat bit 2: flip face sprite about its vertical axis.
const CSTAT_XFLIP: u16 = 4;
/// Build sprite cstat bit 3: flip face sprite about its horizontal axis.
const CSTAT_YFLIP: u16 = 8;
/// `cstat` bit 4 — wall-aligned sprite (skip in face-sprite bootstrap).
const CSTAT_WALL_ALIGN: u16 = 16;
/// `cstat` bit 5 — floor-aligned sprite (skip in face-sprite bootstrap).
const CSTAT_FLOOR_ALIGN: u16 = 32;
/// Build sprite cstat bit 6: draw a wall-aligned sprite from its front side only.
const CSTAT_ONE_SIDED: u16 = 64;
/// Build sprite cstat bit 7: anchor vertical extent around sprite Z.
const CSTAT_CENTERED: u16 = 128;

fn render_fp_walls(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
) -> Vec<i32> {
    render_fp_walls_at(world, view, frame, tiles, 0)
}

fn render_fp_walls_at(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    clock_120hz: u32,
) -> Vec<i32> {
    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let w = frame.width as i32;
    let h = frame.height as i32;
    let cx = w / 2;
    let cy = h / 2;
    let focal_x = (w / 2).max(1);
    let focal_y = (h / 2).max(1);
    let horiz_off = (i32::from(view.horiz) - 100) / 2;
    // A column-only depth buffer hides every sector behind the first portal
    // surface. Keep depth per pixel so upper/lower portal bands occlude while
    // the opening remains available for the next visible sector.
    let mut zbuf = vec![i32::MAX; (w as usize).saturating_mul(h as usize)];

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
            let p2 = wall.point2 as usize;
            if p2 >= world.walls.len() {
                continue;
            }
            let b = &world.walls[p2];
            let spans: Vec<(i32, i32)> = if wall.nextsector < 0 || wall.cstat & 32 != 0 {
                vec![(sec.ceilingz, sec.floorz)]
            } else if let Some(adjacent) = world.sectors.get(wall.nextsector as usize) {
                let mut spans = Vec::with_capacity(2);
                // Build Z increases downwards: expose the neighboring ceiling
                // drop and floor rise as solid wall bands, leaving the portal
                // opening transparent for geometry in the adjacent sector.
                if adjacent.ceilingz > sec.ceilingz {
                    spans.push((sec.ceilingz, adjacent.ceilingz));
                }
                if adjacent.floorz < sec.floorz {
                    spans.push((adjacent.floorz, sec.floorz));
                }
                spans
            } else {
                continue;
            };
            if spans.is_empty() {
                continue;
            }
            let wall_len =
                crate::helpers::length2d(b.x.wrapping_sub(wall.x), b.y.wrapping_sub(wall.y)).max(1);
            let proj = |x: i64, y: i64, z: i64| -> Option<(i32, i32, i32)> {
                if z <= 16 {
                    return None;
                }
                let px = cx as i64 + (focal_x as i64 * x) / z;
                let py = cy as i64 - (focal_y as i64 * y) / z - i64::from(horiz_off);
                Some((px as i32, py as i32, z as i32))
            };
            for (top_z, bottom_z) in spans {
                let Some((x0, y0t, z0)) = cam_project(view, sin, cos, wall.x, wall.y, top_z) else {
                    continue;
                };
                let Some((x1, y1t, z1)) = cam_project(view, sin, cos, b.x, b.y, top_z) else {
                    continue;
                };
                let Some((x0b, y0b, z0b)) = cam_project(view, sin, cos, wall.x, wall.y, bottom_z)
                else {
                    continue;
                };
                let Some((x1b, y1b, z1b)) = cam_project(view, sin, cos, b.x, b.y, bottom_z) else {
                    continue;
                };
                let Some((sx0, sy0t, sz0)) = proj(x0, y0t, z0) else {
                    continue;
                };
                let Some((sx1, sy1t, sz1)) = proj(x1, y1t, z1) else {
                    continue;
                };
                let Some((_sx0b, sy0b, _)) = proj(x0b, y0b, z0b) else {
                    continue;
                };
                let Some((_sx1b, sy1b, _)) = proj(x1b, y1b, z1b) else {
                    continue;
                };
                let (left, right, yt_l, yt_r, yb_l, yb_r, zl, zr, u0, u1) = if sx0 <= sx1 {
                    (sx0, sx1, sy0t, sy1t, sy0b, sy1b, sz0, sz1, 0_i32, wall_len)
                } else {
                    (sx1, sx0, sy1t, sy0t, sy1b, sy0b, sz1, sz0, wall_len, 0_i32)
                };
                if right <= left {
                    continue;
                }
                let flat = wall_color(wall.picnum, wall.shade);
                let (tile_width, tile_height) = tiles
                    .and_then(|ts| ts.dimensions(wall.picnum))
                    .unwrap_or((64, 64));
                let sample_width = u32::from(tile_width).max(1);
                let sample_height = u32::from(tile_height).max(1);
                // Build panning bytes are fractions of the source tile. Keep
                // the offset in texels so non-64px ART tiles pan consistently.
                let xpan = (u32::from(wall.xpanning) * sample_width) / 256;
                let ypan = (u32::from(wall.ypanning) * sample_height) / 256;
                let span = i64::from(right - left);
                let xrepeat = wall.xrepeat.max(1) as i32;
                let yrepeat = wall.yrepeat.max(1) as i32;
                for col in left.max(0)..right.min(w) {
                    let t = (i64::from(col - left) << 16) / span;
                    let y_top = yt_l + ((((yt_r - yt_l) as i64) * t) >> 16) as i32;
                    let y_bottom = yb_l + ((((yb_r - yb_l) as i64) * t) >> 16) as i32;
                    let depth = zl + ((((zr - zl) as i64) * t) >> 16) as i32;
                    let u = u0 + ((((u1 - u0) as i64) * t) >> 16) as i32;
                    let y0 = y_top.min(y_bottom).max(0);
                    let y1 = y_top.max(y_bottom).min(h);
                    let yspan = (y1 - y0).max(1);
                    for y in y0..y1 {
                        let depth_index = y as usize * w as usize + col as usize;
                        if depth >= zbuf[depth_index] {
                            continue;
                        }
                        let v = ((i64::from(y - y0) << 8) / i64::from(yspan)) as i32;
                        let sample = tiles.and_then(|ts| {
                            let uu = ((u.wrapping_mul(xrepeat)) / 8).max(0) as u32 + xpan;
                            let vv = ((v.wrapping_mul(yrepeat)) / 8).max(0) as u32 + ypan;
                            let uu = if wall.cstat & 8 != 0 {
                                sample_width - 1 - uu % sample_width
                            } else {
                                uu
                            };
                            let vv = if wall.cstat & 256 != 0 {
                                sample_height - 1 - vv % sample_height
                            } else {
                                vv
                            };
                            ts.sample_at(wall.picnum, uu, vv, clock_120hz)
                        });
                        let pix = match sample {
                            Some(pixel)
                                if pixel == room_build::art::TRANSPARENT
                                    && wall.cstat & 16 != 0 =>
                            {
                                continue;
                            }
                            Some(pixel) if pixel != room_build::art::TRANSPARENT => {
                                tiles.map_or(pixel, |ts| {
                                    ts.shade_palette(wall.shade, ts.remap_palette(wall.pal, pixel))
                                })
                            }
                            _ => flat,
                        };
                        let output = tiles.map_or(pix, |ts| {
                            ts.composite_layer(
                                frame.pixels[depth_index],
                                pix,
                                wall.cstat & 128 != 0,
                                wall.cstat & 512 != 0,
                            )
                        });
                        zbuf[depth_index] = depth;
                        frame.put(col, y, output);
                    }
                }
            }
        }
    }
    zbuf
}

/// Face, wall-aligned and floor-aligned sprites in visible sectors. All sprites
/// are depth-tested against world walls.
pub fn render_fp_sprites(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
) {
    render_fp_sprites_at(world, view, frame, tiles, zbuf, 0);
}

/// Face sprites rendered with ART animation evaluated at Build's 120 Hz clock.
pub fn render_fp_sprites_at(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
    clock_120hz: u32,
) {
    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let w = frame.width as i32;
    let h = frame.height as i32;
    let cx = w / 2;
    let cy = h / 2;
    let focal_x = (w / 2).max(1);
    let focal_y = (h / 2).max(1);
    let horiz_off = (i32::from(view.horiz) - 100) / 2;

    for i in 0..crate::world::MAXSPRITES {
        let sp = &world.sprites[i];
        if sp.sectnum < 0 || lists_stat_free(world, i) || (sp.cstat & CSTAT_INVISIBLE) != 0 {
            continue;
        }
        if !view.visible_sectors.contains(&sp.sectnum) {
            continue;
        }
        // Build `drawsprite` skips zero-repeat sprites and tiles without art.
        if sp.xrepeat == 0 || sp.yrepeat == 0 {
            continue;
        }
        if let Some(ts) = tiles
            && ts.dimensions(sp.picnum).is_none_or(|(tw, th)| tw == 0 || th == 0)
        {
            continue;
        }
        if sp.cstat & CSTAT_WALL_ALIGN != 0 {
            render_wall_aligned_sprite(view, frame, tiles, zbuf, sp, clock_120hz);
            continue;
        }
        if sp.cstat & CSTAT_FLOOR_ALIGN != 0 {
            render_floor_aligned_sprite(view, frame, tiles, zbuf, sp, clock_120hz);
            continue;
        }
        let Some((rx, ry, rz)) = cam_project(view, sin, cos, sp.x, sp.y, sp.z) else {
            continue;
        };
        if rz <= 16 {
            continue;
        }
        let sx = cx as i64 + (focal_x as i64 * rx) / rz;
        let sy = cy as i64 - (focal_y as i64 * ry) / rz - i64::from(horiz_off);
        let z = rz as i32;
        // Screen size from Build repeats + depth (rough classic face scale).
        let xrep = i32::from(sp.xrepeat).max(1);
        let yrep = i32::from(sp.yrepeat).max(1);
        let (tile_w, tile_h) = tiles
            .and_then(|ts| ts.dimensions(sp.picnum))
            .unwrap_or((64, 64));
        // Build: a sprite is tilesize*repeat/4 world units across (and
        // tilesize*repeat*4 Z units tall, i.e. /4 in XY-equivalent units).
        let half_w =
            (i64::from(tile_w) * i64::from(xrep) * i64::from(focal_x) / (8 * rz)).max(1) as i32;
        let half_h =
            (i64::from(tile_h) * i64::from(yrep) * i64::from(focal_y) / (8 * rz)).max(1) as i32;
        let (tile_xoff, tile_yoff) = tiles.map_or((0, 0), |ts| ts.sprite_offset(sp.picnum));
        let mut xoff = i32::from(tile_xoff) + i32::from(sp.xoffset);
        let mut yoff = i32::from(tile_yoff) + i32::from(sp.yoffset);
        if sp.cstat & CSTAT_XFLIP != 0 {
            xoff = -xoff;
        }
        if sp.cstat & CSTAT_YFLIP != 0 {
            yoff = -yoff;
        }
        let x_shift = (i64::from(xoff) * i64::from(xrep) * i64::from(focal_x) / (4 * rz)) as i32;
        let y_shift = (i64::from(yoff) * i64::from(yrep) * i64::from(focal_y) / (4 * rz)) as i32;
        // Build `drawsprite`: a positive x offset moves the sprite left.
        let left = sx as i32 - x_shift - half_w;
        let right = sx as i32 - x_shift + half_w;
        // Build: a face sprite stands on its z (feet), unless cstat 128
        // centres it there.
        let (top, bot) = if sp.cstat & 128 != 0 {
            (sy as i32 - y_shift - half_h, sy as i32 - y_shift + half_h)
        } else {
            (sy as i32 - y_shift - 2 * half_h, sy as i32 - y_shift)
        };
        let flat = wall_color(sp.picnum, sp.shade);
        let span_x = i64::from((right - left).max(1));
        let span_y = i64::from((bot - top).max(1));
        let tile = tiles.and_then(|ts| ts.resolve(sp.picnum, clock_120hz));
        let (tw, th) = tile.map_or((64, 64), |t| (t.w as i64, t.h as i64));
        let fog = sprite_fog_shade(Some(world), sp.shade, i64::from(z), sp.sectnum);
        let pal_table = tiles.and_then(|ts| ts.pal_table(sp.pal));
        let shade_row = tiles.and_then(|ts| ts.shade_row(fog));
        for col in left.max(0)..right.min(w) {
            let cu = col as usize;
            let mut u = (i64::from(col - left) * tw) / span_x;
            if sp.cstat & CSTAT_XFLIP != 0 {
                u = tw - 1 - u.min(tw - 1);
            }
            for y in top.max(0)..bot.min(h) {
                let depth_index = y as usize * w as usize + cu;
                if depth_index >= zbuf.len() || z >= zbuf[depth_index] {
                    continue;
                }
                let mut v = (i64::from(y - top) * th) / span_y;
                if sp.cstat & CSTAT_YFLIP != 0 {
                    v = th - 1 - v.min(th - 1);
                }
                let pix = match tile.map(|t| t.at(u, v)) {
                    Some(pixel) if pixel == room_build::art::TRANSPARENT => continue,
                    Some(pixel) => {
                        let p = pal_table.map_or(pixel, |t| t[usize::from(pixel)]);
                        shade_row.map_or(p, |r| r[usize::from(p)])
                    }
                    None => flat,
                };
                frame.put(
                    col,
                    y,
                    tiles.map_or(pix, |ts| {
                        ts.composite_sprite(frame.pixels[depth_index], pix, sp.cstat)
                    }),
                );
                zbuf[depth_index] = z;
            }
        }
    }
}

/// Draw a horizontal Build sprite as two textured, depth-tested triangles.
/// Build's floor-aligned tiles lie in the XY plane, with their dimensions
/// scaled by the sprite repeats (four Build units per texel/repeat unit).
fn render_floor_aligned_sprite(
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
    sprite: &SpriteT,
    clock_120hz: u32,
) {
    let Some(tiles) = tiles else { return };
    let Some((tile_width, tile_height)) = tiles.dimensions(sprite.picnum) else {
        return;
    };
    if tile_width == 0 || tile_height == 0 || sprite.xrepeat == 0 || sprite.yrepeat == 0 {
        return;
    }
    if sprite.cstat & CSTAT_ONE_SIDED != 0
        && (view.posz > sprite.z) == (sprite.cstat & CSTAT_YFLIP == 0)
    {
        // Build floor sprites may be one-sided; Y-flip reverses their visible side.
        return;
    }
    // Build floor sprites: half extents (tilesize/2)*repeat/4 XY units; the
    // tile x axis runs along (sin, -cos), its y axis along (cos, sin).
    let half_width = i32::from(tile_width) * i32::from(sprite.xrepeat) / 8;
    let half_height = i32::from(tile_height) * i32::from(sprite.yrepeat) / 8;
    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(sprite.ang));
    let (sin_view, cos_view) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let center_x = frame.width as i32 / 2;
    let center_y = frame.height as i32 / 2;
    let focal_x = (frame.width as i32 / 2).max(1);
    let focal_y = (frame.height as i32 / 2).max(1);
    let horizon_off = (i32::from(view.horiz) - 100) / 2;
    let (tile_xoff, tile_yoff) = tiles.sprite_offset(sprite.picnum);
    let mut xoff = i32::from(tile_xoff) + i32::from(sprite.xoffset);
    let mut yoff = i32::from(tile_yoff) + i32::from(sprite.yoffset);
    if sprite.cstat & CSTAT_XFLIP != 0 {
        xoff = -xoff;
    }
    if sprite.cstat & CSTAT_YFLIP != 0 {
        yoff = -yoff;
    }
    let shift_x = xoff.wrapping_mul(i32::from(sprite.xrepeat)) / 4;
    let shift_y = yoff.wrapping_mul(i32::from(sprite.yrepeat)) / 4;
    let center_x_world = sprite.x.wrapping_add(
        ((i64::from(sin) * i64::from(shift_x) + i64::from(cos) * i64::from(shift_y)) >> 14) as i32,
    );
    let center_y_world = sprite.y.wrapping_add(
        ((-i64::from(cos) * i64::from(shift_x) + i64::from(sin) * i64::from(shift_y)) >> 14) as i32,
    );
    // Tile x axis (sin, -cos) and y axis (cos, sin), as Build `drawsprite`.
    let axis_x = ((i64::from(sin) * i64::from(half_width)) >> 14) as i32;
    let axis_y = ((-i64::from(cos) * i64::from(half_width)) >> 14) as i32;
    let depth_x = ((i64::from(cos) * i64::from(half_height)) >> 14) as i32;
    let depth_y = ((i64::from(sin) * i64::from(half_height)) >> 14) as i32;
    let corners = [
        (
            center_x_world - axis_x - depth_x,
            center_y_world - axis_y - depth_y,
            0,
            0,
        ),
        (
            center_x_world + axis_x - depth_x,
            center_y_world + axis_y - depth_y,
            63,
            0,
        ),
        (
            center_x_world + axis_x + depth_x,
            center_y_world + axis_y + depth_y,
            63,
            63,
        ),
        (
            center_x_world - axis_x + depth_x,
            center_y_world - axis_y + depth_y,
            0,
            63,
        ),
    ];
    let mut projected = [(0i32, 0i32, 0i32, 0i32, 0i32); 4];
    for (index, &(x, y, u, v)) in corners.iter().enumerate() {
        let Some((rx, ry, rz)) = cam_project(view, sin_view, cos_view, x, y, sprite.z) else {
            return;
        };
        if rz <= 16 {
            return;
        }
        projected[index] = (
            center_x + ((i64::from(focal_x) * rx) / rz) as i32,
            center_y - ((i64::from(focal_y) * ry) / rz) as i32 - horizon_off,
            rz as i32,
            u,
            v,
        );
    }
    if sprite.cstat & CSTAT_XFLIP != 0 {
        for vertex in &mut projected {
            vertex.3 = 63 - vertex.3;
        }
    }
    if sprite.cstat & CSTAT_YFLIP != 0 {
        for vertex in &mut projected {
            vertex.4 = 63 - vertex.4;
        }
    }
    let flat = wall_color(sprite.picnum, sprite.shade);
    draw_sprite_triangle(
        tiles,
        frame,
        zbuf,
        sprite,
        clock_120hz,
        flat,
        [projected[0], projected[1], projected[2]],
    );
    draw_sprite_triangle(
        tiles,
        frame,
        zbuf,
        sprite,
        clock_120hz,
        flat,
        [projected[0], projected[2], projected[3]],
    );
}

fn draw_sprite_triangle(
    tiles: &SoftTiles,
    frame: &mut SoftFrame,
    zbuf: &mut [i32],
    sprite: &SpriteT,
    clock_120hz: u32,
    flat: u8,
    vertices: [(i32, i32, i32, i32, i32); 3],
) {
    let [
        (x0, y0, z0, u0, v0),
        (x1, y1, z1, u1, v1),
        (x2, y2, z2, u2, v2),
    ] = vertices;
    let tile = tiles.resolve(sprite.picnum, clock_120hz);
    let pal_table = tiles.pal_table(sprite.pal);
    let edge = |ax: i32, ay: i32, bx: i32, by: i32, px: i32, py: i32| {
        i64::from(px - ax) * i64::from(by - ay) - i64::from(py - ay) * i64::from(bx - ax)
    };
    let area = edge(x0, y0, x1, y1, x2, y2);
    if area == 0 {
        return;
    }
    let min_x = x0.min(x1).min(x2).max(0);
    let max_x = x0.max(x1).max(x2).min(frame.width as i32 - 1);
    let min_y = y0.min(y1).min(y2).max(0);
    let max_y = y0.max(y1).max(y2).min(frame.height as i32 - 1);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let w0 = edge(x1, y1, x2, y2, x, y);
            let w1 = edge(x2, y2, x0, y0, x, y);
            let w2 = edge(x0, y0, x1, y1, x, y);
            if (area > 0 && (w0 < 0 || w1 < 0 || w2 < 0))
                || (area < 0 && (w0 > 0 || w1 > 0 || w2 > 0))
            {
                continue;
            }
            let depth = ((w0 * i64::from(z0) + w1 * i64::from(z1) + w2 * i64::from(z2)) / area)
                .max(1) as i32;
            let index = y as usize * frame.width as usize + x as usize;
            if index >= zbuf.len() || depth >= zbuf[index] {
                continue;
            }
            // Vertex u/v are in a 64×64 parameter space; map onto the tile.
            let u = ((w0 * i64::from(u0) + w1 * i64::from(u1) + w2 * i64::from(u2)) / area)
                .clamp(0, 63);
            let v = ((w0 * i64::from(v0) + w1 * i64::from(v1) + w2 * i64::from(v2)) / area)
                .clamp(0, 63);
            let pixel = match tile.map(|t| t.at(u * t.w as i64 / 64, v * t.h as i64 / 64)) {
                Some(pixel) if pixel == room_build::art::TRANSPARENT => continue,
                Some(pixel) => {
                    let p = pal_table.map_or(pixel, |t| t[usize::from(pixel)]);
                    let shade =
                        sprite_fog_shade(None, sprite.shade, i64::from(depth), sprite.sectnum);
                    tiles.shade_row(shade).map_or(p, |r| r[usize::from(p)])
                }
                None => flat,
            };
            zbuf[index] = depth;
            frame.put(
                x,
                y,
                tiles.composite_sprite(frame.pixels[index], pixel, sprite.cstat),
            );
        }
    }
}

/// Draw a vertical Build sprite as a camera-projected textured quad.
///
/// The quad follows `sprite.ang`, uses Build's four-map-unit repeat scale,
/// honors the centered-origin flag and tests every transparent texel against
/// the shared per-pixel depth buffer. This is a classic-mode approximation:
/// perspective correction, translucency and one-sided backface rejection are
/// still handled by the eventual full Build renderer.
fn render_wall_aligned_sprite(
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
    sprite: &SpriteT,
    clock_120hz: u32,
) {
    let Some(tiles) = tiles else { return };
    let Some((tile_width, tile_height)) = tiles.dimensions(sprite.picnum) else {
        return;
    };
    let wall_tile = tiles.resolve(sprite.picnum, clock_120hz);
    if tile_width == 0 || tile_height == 0 || sprite.xrepeat == 0 || sprite.yrepeat == 0 {
        return;
    }
    // Build `drawsprite` (cstat 16): the tile spans tilesizx*xrepeat/4 XY
    // units along (sin, -cos) — across the sprite's facing — and
    // tilesizy*yrepeat*4 Z units.
    let half_width = i32::from(tile_width) * i32::from(sprite.xrepeat) / 8;
    let world_height = i32::from(tile_height) * i32::from(sprite.yrepeat) * 4;
    let (sin, cos) = crate::helpers::approx_sincos_q14(i32::from(sprite.ang));
    let dx = ((i64::from(sin) * i64::from(half_width)) >> 14) as i32;
    let dy = ((-i64::from(cos) * i64::from(half_width)) >> 14) as i32;
    let (tile_xoff, tile_yoff) = tiles.sprite_offset(sprite.picnum);
    let mut xoff = i32::from(tile_xoff) + i32::from(sprite.xoffset);
    let mut yoff = i32::from(tile_yoff) + i32::from(sprite.yoffset);
    if sprite.cstat & CSTAT_XFLIP != 0 {
        xoff = -xoff;
    }
    if sprite.cstat & CSTAT_YFLIP != 0 {
        yoff = -yoff;
    }
    let x_shift = xoff.wrapping_mul(i32::from(sprite.xrepeat)) / 4;
    let shift_x = ((i64::from(sin) * i64::from(x_shift)) >> 14) as i32;
    let shift_y = ((-i64::from(cos) * i64::from(x_shift)) >> 14) as i32;
    let center_x = sprite.x.wrapping_add(shift_x);
    let center_y = sprite.y.wrapping_add(shift_y);
    let left_x = center_x.wrapping_sub(dx);
    let left_y = center_y.wrapping_sub(dy);
    let right_x = center_x.wrapping_add(dx);
    let right_y = center_y.wrapping_add(dy);
    let z_shift = yoff.wrapping_mul(i32::from(sprite.yrepeat)).wrapping_mul(4);
    let (top_z, bottom_z) = if sprite.cstat & CSTAT_CENTERED != 0 {
        (
            sprite
                .z
                .wrapping_sub(z_shift)
                .wrapping_sub(world_height / 2),
            sprite
                .z
                .wrapping_sub(z_shift)
                .wrapping_add(world_height / 2),
        )
    } else {
        (
            sprite.z.wrapping_sub(z_shift).wrapping_sub(world_height),
            sprite.z.wrapping_sub(z_shift),
        )
    };
    let (sin_view, cos_view) = crate::helpers::approx_sincos_q14(i32::from(view.ang));
    let focal_x = (frame.width as i32 / 2).max(1);
    let focal_y = (frame.height as i32 / 2).max(1);
    let center_x = frame.width as i32 / 2;
    let center_y = frame.height as i32 / 2;
    let horizon_off = (i32::from(view.horiz) - 100) / 2;
    let project = |x: i32, y: i32, z: i32| -> Option<(i32, i32, i32)> {
        let (rx, ry, rz) = cam_project(view, sin_view, cos_view, x, y, z)?;
        if rz <= 16 {
            return None;
        }
        Some((
            center_x + ((i64::from(focal_x) * rx) / rz) as i32,
            center_y - ((i64::from(focal_y) * ry) / rz) as i32 - horizon_off,
            rz as i32,
        ))
    };
    let Some(mut a_top) = project(left_x, left_y, top_z) else {
        return;
    };
    let Some(mut a_bottom) = project(left_x, left_y, bottom_z) else {
        return;
    };
    let Some(mut b_top) = project(right_x, right_y, top_z) else {
        return;
    };
    let Some(mut b_bottom) = project(right_x, right_y, bottom_z) else {
        return;
    };
    let mut reverse_u = sprite.cstat & CSTAT_XFLIP != 0;
    if a_top.0 > b_top.0 {
        // A one-sided Build wall sprite is culled when viewed from its back.
        if sprite.cstat & CSTAT_ONE_SIDED != 0 {
            return;
        }
        std::mem::swap(&mut a_top, &mut b_top);
        std::mem::swap(&mut a_bottom, &mut b_bottom);
        reverse_u = !reverse_u;
    }
    let left = a_top.0.max(0);
    let right = b_top.0.min(frame.width as i32);
    if right <= left || b_top.0 == a_top.0 {
        return;
    }
    let screen_width = i64::from(b_top.0 - a_top.0);
    let flat = wall_color(sprite.picnum, sprite.shade);
    for col in left..right {
        let t = (i64::from(col - a_top.0) << 16) / screen_width;
        let interp = |a: i32, b: i32| a + (((i64::from(b - a) * t) >> 16) as i32);
        let top = interp(a_top.1, b_top.1);
        let bottom = interp(a_bottom.1, b_bottom.1);
        let depth = interp(a_top.2, b_top.2).max(1);
        if bottom <= top {
            continue;
        }
        let y0 = top.max(0);
        let y1 = bottom.min(frame.height as i32);
        let yspan = i64::from(bottom - top);
        for y in y0..y1 {
            let index = y as usize * frame.width as usize + col as usize;
            if index >= zbuf.len() || depth >= zbuf[index] {
                continue;
            }
            let tw = i64::from(tile_width);
            let th = i64::from(tile_height);
            let u0 = ((i64::from(col - a_top.0) * tw) / screen_width).clamp(0, tw - 1);
            let u = if reverse_u { tw - 1 - u0 } else { u0 };
            let v0 = ((i64::from(y - top) * th) / yspan).clamp(0, th - 1);
            let v = if sprite.cstat & CSTAT_YFLIP != 0 {
                th - 1 - v0
            } else {
                v0
            };
            let pix = match wall_tile.map(|t| t.at(u, v)) {
                Some(pixel) if pixel == room_build::art::TRANSPARENT => continue,
                Some(pixel) => tiles.shade_palette(
                    sprite_fog_shade(None, sprite.shade, i64::from(depth), sprite.sectnum),
                    tiles.remap_palette(sprite.pal, pixel),
                ),
                None => flat,
            };
            zbuf[index] = depth;
            frame.put(
                col,
                y,
                tiles.composite_sprite(frame.pixels[index], pix, sprite.cstat),
            );
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
    let focal_x = (w / 2).max(1);
    let focal_y = (h / 2).max(1);
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
            // Camera Y points up, so screen Y subtracts the projected height.
            let proj = |x: i64, y: i64, z: i64| -> Option<(i32, i32)> {
                if z <= 16 {
                    return None;
                }
                let px = cx as i64 + (focal_x as i64 * x) / z;
                let py = cy as i64 - (focal_y as i64 * y) / z;
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
    // Build heading `ang` faces (cos, sin); with Y pointing south, screen
    // right is (-sin, cos). (The old basis swapped them: the view looked 90°
    // to the right of the player and was mirrored, so turning felt inverted.)
    let rz = (dx * i64::from(cos) + dy * i64::from(sin)) >> 14;
    let rx = (-dx * i64::from(sin) + dy * i64::from(cos)) >> 14;
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
    fn duke_sky_sampling_selects_the_configured_angular_tile() {
        let mut tiles = SoftTiles::new();
        tiles.insert(100, 1, 1, vec![11]);
        tiles.insert(102, 1, 1, vec![22]);
        tiles.set_sky_offsets(100, [0, 2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(tiles.sample_sky_at(100, 128, 0, 1, 0, 1, 0), Some(11));
        assert_eq!(tiles.sample_sky_at(100, 384, 0, 1, 0, 1, 0), Some(22));
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
        // Build heading 512 looks +Y (south).
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
        let v = w.build_view(1000, 400, -(32 << 8), 512, 100, 0);
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
    fn fp_textured_portal_renders_height_differences_and_keeps_opening() {
        let sectors = vec![
            SectorT {
                wallptr: 0,
                wallnum: 4,
                ceilingz: -8192,
                floorz: 0,
                ceilingpicnum: 3,
                floorpicnum: 4,
                ..SectorT::default()
            },
            SectorT {
                wallptr: 4,
                wallnum: 4,
                ceilingz: -4096,
                floorz: -2048,
                ceilingpicnum: 3,
                floorpicnum: 4,
                ..SectorT::default()
            },
        ];
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                picnum: 30,
                ..WallT::default()
            },
            WallT {
                x: 1000,
                y: 0,
                point2: 2,
                nextsector: -1,
                picnum: 30,
                ..WallT::default()
            },
            WallT {
                x: 1000,
                y: 1000,
                point2: 3,
                nextsector: 1,
                picnum: 5,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 1000,
                point2: 0,
                nextsector: -1,
                picnum: 30,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 1000,
                point2: 5,
                nextsector: 0,
                picnum: 5,
                ..WallT::default()
            },
            WallT {
                x: 1000,
                y: 1000,
                point2: 6,
                nextsector: -1,
                picnum: 31,
                ..WallT::default()
            },
            WallT {
                x: 1000,
                y: 2000,
                point2: 7,
                nextsector: -1,
                picnum: 31,
                ..WallT::default()
            },
            WallT {
                x: 0,
                y: 2000,
                point2: 4,
                nextsector: -1,
                picnum: 31,
                ..WallT::default()
            },
        ];
        let world = BuildWorld::load_arrays(sectors, walls, &[], [500, 500, -4096], 0, 0, 1)
            .expect("two adjacent portal sectors");
        // Build heading 512 looks +Y, through the portal wall at y = 1000.
        let view = world.build_view(500, 500, -4096, 512, 100, 0);
        let mut frame = SoftFrame::new(128, 96);
        render_fp_textured(&world, &view, &mut frame, &SoftTiles::new());
        let portal_wall = wall_color(5, 0);
        let far_wall = wall_color(31, 0);
        assert_eq!(
            frame.pixels[30 * 128 + 64],
            portal_wall,
            "upper portal band"
        );
        assert_eq!(frame.pixels[50 * 128 + 64], far_wall, "visible opening");
        assert_eq!(
            frame.pixels[66 * 128 + 64],
            portal_wall,
            "lower portal band"
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

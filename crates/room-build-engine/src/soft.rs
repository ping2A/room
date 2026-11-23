//! Classic-mode CPU helpers (debug / goldens). Not pixel-exact vs DOS Build.
//! Spec: `docs/25` §9 / D3 — full soft path may later live in `room-render-soft`.

use crate::types::SpriteT;
use crate::view::BuildView;
use crate::world::BuildWorld;
use room_build::kvx::KvxDocument;
use std::sync::Arc;

fn sky_look_q16(angle_q16: i32, col: i32, width: i32) -> i64 {
    let column_angle = (i64::from(col - width / 2) * 512 / i64::from(width)) << 16;
    (i64::from(angle_q16) + column_angle).rem_euclid(2048_i64 << 16)
}
use std::collections::BTreeMap;

/// A DEF voxel model assigned to an ART tile, ready for the software Build
/// sprite pass.
#[derive(Clone, Debug)]
pub struct DefVoxelTile {
    /// Parsed KVX model shared by every tile in the declaration's range.
    pub model: Arc<KvxDocument>,
    /// Zero-based model slot used by direct `(cstat & 48) == 48` sprites.
    pub model_index: i16,
    /// DEF model scale (positive, with one meaning native size).
    pub scale: f32,
    /// Whether the sprite's Build angle rotates the voxel model.
    pub rotate: bool,
    /// Slab voxel counts for each mip, cached once when the DEF model loads.
    pub mip_sample_counts: Arc<[usize]>,
    /// KVX embedded-palette indices mapped into Build's base palette.
    pub palette_map: Arc<[u8; 256]>,
}

/// Map a KVX model's 6-bit embedded RGB palette to the nearest Build palette
/// indices. This is done once while loading the DEF resources, not per frame.
#[must_use]
pub fn kvx_palette_map(model: &KvxDocument, build_palette: &[u8; 768]) -> [u8; 256] {
    let mut mapped = [0u8; 256];
    for (index, output) in mapped.iter_mut().enumerate() {
        let source = index * 3;
        let color = [
            expand_kvx_channel(model.palette[source]),
            expand_kvx_channel(model.palette[source + 1]),
            expand_kvx_channel(model.palette[source + 2]),
        ];
        let mut best_distance = u32::MAX;
        let mut best_index = 0u8;
        // Index 255 is Build's transparent color and is not a valid opaque
        // destination for a voxel palette entry.
        for candidate in 0..255usize {
            let target = candidate * 3;
            let distance = (0..3).fold(0u32, |sum, channel| {
                let delta =
                    u32::from(color[channel]).abs_diff(u32::from(build_palette[target + channel]));
                sum.saturating_add(delta.saturating_mul(delta))
            });
            if distance < best_distance {
                best_distance = distance;
                best_index = candidate as u8;
            }
        }
        *output = best_index;
    }
    mapped
}

fn expand_kvx_channel(channel: u8) -> u8 {
    let channel = channel & 63;
    (channel << 2) | (channel >> 4)
}

/// Owned ART tiles for soft textured draw (column-major pixels).
#[derive(Clone, Debug, Default)]
pub struct SoftTiles {
    tiles: BTreeMap<i16, (u16, u16, Vec<u8>)>,
    /// DEF `texture` image overrides keyed by `(tile, palette)`.
    palette_tiles: BTreeMap<(i16, u8), (u16, u16, Vec<u8>)>,
    /// Build `LOOKUP.DAT` remaps keyed by sprite/wall palette.
    palette_lookups: BTreeMap<u8, [u8; 256]>,
    /// Build shade tables from `PALETTE.DAT`, brightest first.
    shade_tables: Vec<u8>,
    shade_count: usize,
    /// Build's optional indexed-color translucency lookup from `PALETTE.DAT`.
    translucency: Option<Vec<u8>>,
    /// Classic indexed blend tables declared by DEF (table zero stays in `translucency`).
    blend_tables: BTreeMap<u8, Vec<u8>>,
    /// Decoded DEF cubemap faces keyed by `(parallax tile, palette)`.
    skyboxes: BTreeMap<(i16, u8), room_build::def::ImportedSkybox>,
    /// ART picanm cycle and frame duration in 120 Hz Build clock ticks.
    animations: BTreeMap<i16, (Vec<i16>, u32)>,
    /// ART picanm sprite-origin offsets, keyed by base tile.
    sprite_offsets: BTreeMap<i16, (i8, i8)>,
    /// Per-sky 45-degree ART tile offsets (`prelevel`'s `pskyoff[]`).
    sky_offsets: BTreeMap<i16, [i16; 8]>,
    /// Per-sky Duke `parallaxyscale` (16.16; absent = 65536).
    sky_yscale: BTreeMap<i16, i32>,
    /// Source colors of DEF RGB tile images (`0xFFRRGGBB`, `0` transparent),
    /// laid out like the matching `tiles` entry; drawn in true colour (ED7).
    true_color: BTreeMap<i16, Arc<[u32]>>,
    /// Source colors of DEF RGB palette-branch images.
    palette_true_color: BTreeMap<(i16, u8), Arc<[u32]>>,
    /// Base palette as 8-bit RGB, for blending true-colour translucency.
    palette_rgb: Option<Box<[u8; 768]>>,
    /// DEF `tint` per palette, applied to true-colour images (Polymost
    /// `hictinting`).
    tints: BTreeMap<u8, HicTint>,
}

/// One DEF `tint` (EDuke32 `polytint_t` colour and `HICTINT_*` flags).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HicTint {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub flags: u16,
}

impl HicTint {
    const GRAYSCALE: u16 = 1;
    const INVERT: u16 = 2;
    const COLORIZE: u16 = 4;
    const APPLY_OVER_ALT_PAL: u16 = 32;
    const BLEND_MASK: u16 = 64 | 128;
    const BLEND_SCREEN: u16 = 64;
    const BLEND_OVERLAY: u16 = 128;
    const BLEND_HARDLIGHT: u16 = 192;
    /// Effects EDuke32 bakes into the texture (`HICTINT_IN_MEMORY`).
    const IN_MEMORY: u16 = Self::GRAYSCALE | Self::INVERT | Self::COLORIZE | Self::BLEND_MASK;

    /// Tint one `0xAARRGGBB` texel as Polymost does: the in-memory effects
    /// (`hictinting_applypixcolor`), then, unless the tint was precomputed
    /// (colorize/blend), the `hictinting_apply` colour multiply.
    #[must_use]
    pub fn apply(self, color: u32) -> u32 {
        if color == 0 {
            return 0;
        }
        let alpha = color & 0xFF00_0000;
        let mut r = i32::try_from((color >> 16) & 0xFF).unwrap_or(0);
        let mut g = i32::try_from((color >> 8) & 0xFF).unwrap_or(0);
        let mut b = i32::try_from(color & 0xFF).unwrap_or(0);
        let (tr, tg, tb) = (i32::from(self.r), i32::from(self.g), i32::from(self.b));
        if self.flags & Self::GRAYSCALE != 0 {
            // EDuke32's coefficient order, verbatim.
            let gray = (f64::from(b) * 0.3 + f64::from(g) * 0.59 + f64::from(r) * 0.11) as i32;
            (r, g, b) = (gray, gray, gray);
        }
        if self.flags & Self::INVERT != 0 {
            (r, g, b) = (255 - r, 255 - g, 255 - b);
        }
        if self.flags & Self::COLORIZE != 0 {
            r = ((r * tr) >> 6).min(255);
            g = ((g * tg) >> 6).min(255);
            b = ((b * tb) >> 6).min(255);
        }
        let blend = |c: i32, t: i32| match self.flags & Self::BLEND_MASK {
            Self::BLEND_SCREEN => 255 - (((255 - c) * (255 - t)) >> 8),
            Self::BLEND_OVERLAY if c < 128 => (c * t) >> 7,
            Self::BLEND_OVERLAY => 255 - (((255 - c) * (255 - t)) >> 7),
            Self::BLEND_HARDLIGHT if t < 128 => (c * t) >> 7,
            Self::BLEND_HARDLIGHT => 255 - (((255 - c) * (255 - t)) >> 7),
            _ => c,
        };
        (r, g, b) = (blend(r, tr), blend(g, tg), blend(b, tb));
        if self.flags & (Self::COLORIZE | Self::BLEND_MASK) == 0 {
            // Fixed-function colour multiply, rounded like an 8-bit target.
            let multiply = |c: i32, t: i32| (c * t + 127) / 255;
            (r, g, b) = (multiply(r, tr), multiply(g, tg), multiply(b, tb));
        }
        let channel = |c: i32| u32::try_from(c.clamp(0, 255)).unwrap_or(0);
        alpha | (channel(r) << 16) | (channel(g) << 8) | channel(b)
    }

    /// Whether Polymost tints an image loaded for this very palette (only
    /// in-memory effects or `HICTINT_APPLYOVERALTPAL` do).
    fn applies_over_own_palette(self) -> bool {
        self.flags & (Self::IN_MEMORY | Self::APPLY_OVER_ALT_PAL) != 0
    }
}

impl SoftTiles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert tile `picnum` (column-major palette indices, Build layout).
    pub fn insert(&mut self, picnum: i16, width: u16, height: u16, pixels: Vec<u8>) {
        self.tiles.insert(picnum, (width, height, pixels));
        self.true_color.remove(&picnum);
    }

    /// Attach source colors to the tile just inserted at `picnum`; ignored
    /// unless they match its texel count.
    pub fn set_true_color(&mut self, picnum: i16, colors: Option<Arc<[u32]>>) {
        match colors {
            Some(colors)
                if self
                    .tiles
                    .get(&picnum)
                    .is_some_and(|(_, _, pixels)| pixels.len() == colors.len()) =>
            {
                self.true_color.insert(picnum, colors);
            }
            _ => {
                self.true_color.remove(&picnum);
            }
        }
    }

    /// Attach source colors to a DEF palette-branch image.
    pub fn set_palette_true_color(&mut self, picnum: i16, palette: u8, colors: Option<Arc<[u32]>>) {
        match colors {
            Some(colors)
                if self
                    .palette_tiles
                    .get(&(picnum, palette))
                    .is_some_and(|(_, _, pixels)| pixels.len() == colors.len()) =>
            {
                self.palette_true_color.insert((picnum, palette), colors);
            }
            _ => {
                self.palette_true_color.remove(&(picnum, palette));
            }
        }
    }

    /// Whether any tile or skybox carries true-colour source data.
    #[must_use]
    pub fn has_true_color(&self) -> bool {
        !self.true_color.is_empty()
            || !self.palette_true_color.is_empty()
            || self
                .skyboxes
                .values()
                .any(|skybox| skybox.faces.iter().any(|face| face.true_color.is_some()))
    }

    /// Install the DEF `tint` for `palette` (later declarations win).
    pub fn set_tint(&mut self, palette: u8, tint: HicTint) {
        self.tints.insert(palette, tint);
    }

    /// Base palette (8-bit RGB) used to blend true-colour translucency.
    pub fn set_palette_rgb(&mut self, rgb: &[u8; 768]) {
        self.palette_rgb = Some(Box::new(*rgb));
    }

    /// 8-bit RGB of palette index `index` as `0xFFRRGGBB`, if known.
    #[must_use]
    pub fn palette_color(&self, index: u8) -> Option<u32> {
        let rgb = self.palette_rgb.as_deref()?;
        let i = usize::from(index) * 3;
        Some(room_build::def::pack_true_color([
            rgb[i],
            rgb[i + 1],
            rgb[i + 2],
        ]))
    }

    /// EDuke32 Polymost `getshadefactor`: scale a true-colour texel by
    /// `(numshades - clamp(shade, 0, numshades)) / numshades`.
    #[must_use]
    pub fn shade_true_color(&self, color: u32, shade: i8) -> u32 {
        if color == 0 || self.shade_count == 0 {
            return color;
        }
        let shades = self.shade_count as u32;
        let keep = shades - u32::try_from(shade.max(0)).unwrap_or(0).min(shades);
        let channel = |shift: u32| (((color >> shift) & 0xFF) * keep / shades) << shift;
        0xFF00_0000 | channel(16) | channel(8) | channel(0)
    }

    /// Blend two true-colour pixels like Polymost translucency: the
    /// foreground keeps 2/3 (`cstat 2`) or 1/3 (with `cstat 512`).
    #[must_use]
    pub fn blend_true_color(background: u32, foreground: u32, reverse: bool) -> u32 {
        let (fg, bg) = if reverse { (1, 2) } else { (2, 1) };
        let channel = |shift: u32| {
            ((((foreground >> shift) & 0xFF) * fg + ((background >> shift) & 0xFF) * bg) / 3)
                << shift
        };
        0xFF00_0000 | channel(16) | channel(8) | channel(0)
    }

    /// Insert a Build-layout tile and its sprite origin offsets.
    pub fn insert_with_offset(
        &mut self,
        picnum: i16,
        width: u16,
        height: u16,
        pixels: Vec<u8>,
        x_offset: i8,
        y_offset: i8,
    ) {
        self.insert(picnum, width, height, pixels);
        self.sprite_offsets.insert(picnum, (x_offset, y_offset));
    }

    /// Insert a DEF texture branch for one Build palette bank.
    pub fn insert_palette_variant(
        &mut self,
        picnum: i16,
        palette: u8,
        width: u16,
        height: u16,
        pixels: Vec<u8>,
    ) {
        if palette != 0 && usize::from(width).saturating_mul(usize::from(height)) == pixels.len() {
            self.palette_tiles
                .insert((picnum, palette), (width, height, pixels));
            self.palette_true_color.remove(&(picnum, palette));
        }
    }

    /// Insert decoded DEF skybox faces, replacing the same tile/palette pair.
    pub fn insert_skybox(&mut self, skybox: room_build::def::ImportedSkybox) {
        self.skyboxes.insert((skybox.tile, skybox.palette), skybox);
    }

    /// Whether a DEF cubemap exists for this parallax tile and palette.
    #[must_use]
    pub fn has_skybox(&self, tile: i16, palette: u8) -> bool {
        self.skyboxes.contains_key(&(tile, palette))
    }

    /// Copy an already loaded tile, including its animation and sprite origin.
    pub fn copy_tile(&mut self, source: i16, destination: i16) -> bool {
        let Some(tile) = self.tiles.get(&source).cloned() else {
            return false;
        };
        self.tiles.insert(destination, tile);
        match self.true_color.get(&source).cloned() {
            Some(colors) => {
                self.true_color.insert(destination, colors);
            }
            None => {
                self.true_color.remove(&destination);
            }
        }
        let variants: Vec<_> = self
            .palette_tiles
            .iter()
            .filter(|((picnum, _), _)| *picnum == source)
            .map(|((_, palette), tile)| ((*palette), tile.clone()))
            .collect();
        for (palette, tile) in variants {
            let colors = self.palette_true_color.get(&(source, palette)).cloned();
            self.palette_tiles.insert((destination, palette), tile);
            match colors {
                Some(colors) => {
                    self.palette_true_color
                        .insert((destination, palette), colors);
                }
                None => {
                    self.palette_true_color.remove(&(destination, palette));
                }
            }
        }
        if let Some(offset) = self.sprite_offsets.get(&source).copied() {
            self.sprite_offsets.insert(destination, offset);
        }
        if let Some(animation) = self.animations.get(&source).cloned() {
            self.animations.insert(destination, animation);
        }
        true
    }

    /// Set an ART-style animation range from EDuke32 DEF metadata.
    pub fn set_animation_range(&mut self, start: i16, end: i16, speed: u8, animation: u8) {
        if animation == 0 {
            self.animations.remove(&start);
            return;
        }
        let frames: Vec<i16> = match animation {
            1 => (start..=end)
                .chain((start.saturating_add(1)..end).rev())
                .collect(),
            2 => (start..=end).collect(),
            3 => (end..=start).rev().collect(),
            _ => Vec::new(),
        };
        if frames.len() > 1 {
            self.animations
                .insert(start, (frames, 1_u32 << speed.min(15)));
        } else {
            self.animations.remove(&start);
        }
    }

    /// Reserve a blank transparent tile with explicit dimensions.
    pub fn insert_dummy(&mut self, picnum: i16, width: u16, height: u16) {
        let count = usize::from(width) * usize::from(height);
        self.insert_with_offset(picnum, width, height, vec![255; count], 0, 0);
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

    #[must_use]
    pub fn sprite_offset(&self, picnum: i16) -> (i8, i8) {
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
        let resolved = self.animated_picnum(picnum, clock_120hz);
        let (w, h, pix) = self.tiles.get(&resolved)?;
        (*w != 0 && *h != 0).then_some(TileRef {
            w: usize::from(*w),
            h: usize::from(*h),
            pix,
            rgb: self.true_color.get(&resolved).map(|colors| &colors[..]),
            tint: None,
        })
    }

    /// Resolve a tile animation, preferring a declared DEF image for `palette`.
    pub(crate) fn resolve_for_palette(
        &self,
        picnum: i16,
        palette: u8,
        clock_120hz: u32,
    ) -> Option<TileRef<'_>> {
        let resolved = self.animated_picnum(picnum, clock_120hz);
        // True colour follows the image actually drawn: a palette branch's
        // own colors, or the base image's only when `palette` does not remap
        // it through LOOKUP.DAT (then the indexed path keeps the tint).
        // A DEF `tint` for `palette` keeps the base image's true colour and
        // tints it, as Polymost draws a hightile under another palette.
        let tint = (palette != 0)
            .then(|| self.tints.get(&palette).copied())
            .flatten();
        let (tile, rgb, tint) = match self.palette_tiles.get(&(resolved, palette)) {
            Some(tile) => (
                tile,
                self.palette_true_color.get(&(resolved, palette)),
                tint.filter(|tint| tint.applies_over_own_palette()),
            ),
            None => (
                self.tiles.get(&resolved)?,
                self.true_color.get(&resolved).filter(|_| {
                    palette == 0 || tint.is_some() || !self.palette_lookups.contains_key(&palette)
                }),
                tint,
            ),
        };
        let (w, h, pix) = tile;
        let rgb = rgb.map(|colors| &colors[..]);
        (*w != 0 && *h != 0).then_some(TileRef {
            w: usize::from(*w),
            h: usize::from(*h),
            pix,
            tint: tint.filter(|_| rgb.is_some()),
            rgb,
        })
    }

    /// Shaded true color of a base tile texel as drawn by HUD paths that
    /// sample the base image (`sample`) and remap `palette` through
    /// LOOKUP.DAT: `0` when there is no source color or the palette remaps
    /// without a DEF `tint` (a tint keeps the colour and tints it).
    #[must_use]
    pub fn base_true_color_at(&self, picnum: i16, palette: u8, shade: i8, u: u32, v: u32) -> u32 {
        let tint = (palette != 0)
            .then(|| self.tints.get(&palette).copied())
            .flatten();
        if palette != 0 && tint.is_none() && self.palette_lookups.contains_key(&palette) {
            return 0;
        }
        self.resolve(picnum, 0).map_or(0, |tile| {
            let color = tile.at_rgb(i64::from(u), i64::from(v));
            self.shade_true_color(tint.map_or(color, |tint| tint.apply(color)), shade)
        })
    }

    /// True-colour texel (`0` = none) for sampling outside span drawing.
    #[must_use]
    pub fn true_color_at_for_palette(
        &self,
        picnum: i16,
        palette: u8,
        u: u32,
        v: u32,
        clock_120hz: u32,
    ) -> u32 {
        self.resolve_for_palette(picnum, palette, clock_120hz)
            .map_or(0, |tile| tile.at_rgb(i64::from(u), i64::from(v)))
    }

    /// Whether this frame has an explicit DEF image for the requested palette.
    pub(crate) fn has_palette_variant(&self, picnum: i16, palette: u8, clock_120hz: u32) -> bool {
        let resolved = self.animated_picnum(picnum, clock_120hz);
        self.palette_tiles.contains_key(&(resolved, palette))
    }

    fn animated_picnum(&self, picnum: i16, clock_120hz: u32) -> i16 {
        self.animations
            .get(&picnum)
            .map_or(picnum, |(frames, step)| {
                let frame = (clock_120hz / (*step).max(1)) as usize % frames.len();
                frames[frame]
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
        self.sample_resolved(self.resolve(picnum, clock_120hz), u, v)
    }

    /// Sample the palette-specific DEF image, falling back to the base tile.
    #[must_use]
    pub fn sample_at_for_palette(
        &self,
        picnum: i16,
        palette: u8,
        u: u32,
        v: u32,
        clock_120hz: u32,
    ) -> Option<u8> {
        self.sample_resolved(self.resolve_for_palette(picnum, palette, clock_120hz), u, v)
    }

    fn sample_resolved(&self, tile: Option<TileRef<'_>>, u: u32, v: u32) -> Option<u8> {
        let tile = tile?;
        let (w, h, pix) = (tile.w, tile.h, tile.pix);
        if w == 0 || h == 0 {
            return None;
        }
        let x = (u as usize) % w;
        let y = (v as usize) % h;
        // column-major: pixels[x * height + y]
        pix.get(x * h + y).copied()
    }

    /// Pixel dimensions of a loaded tile.
    #[must_use]
    pub fn dimensions(&self, picnum: i16) -> Option<(u16, u16)> {
        self.tiles.get(&picnum).map(|(w, h, _)| (*w, *h))
    }

    /// Pixel dimensions for a palette override, falling back to the base tile.
    #[must_use]
    pub fn dimensions_for_palette(&self, picnum: i16, palette: u8) -> Option<(u16, u16)> {
        self.palette_tiles
            .get(&(picnum, palette))
            .or_else(|| self.tiles.get(&picnum))
            .map(|(width, height, _)| (*width, *height))
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

    /// Install one DEF 256×256 indexed blend table. Table zero is reserved for
    /// the game's original `PALETTE.DAT` translucency table.
    pub fn insert_blend_table(&mut self, table: u8, pixels: Vec<u8>) {
        if table != 0 && pixels.len() == 65_536 {
            self.blend_tables.insert(table, pixels);
        }
    }

    /// Whether a nonzero custom blend table was loaded.
    #[must_use]
    pub fn has_blend_table(&self, table: u8) -> bool {
        table != 0 && self.blend_tables.contains_key(&table)
    }

    fn composite_sprite(&self, background: u8, foreground: u8, cstat: u16, blend_table: u8) -> u8 {
        const TRANSLUCENT: u16 = 2;
        const REVERSE_TRANSLUCENT: u16 = 512;
        self.composite_layer_with_table(
            background,
            foreground,
            cstat & TRANSLUCENT != 0,
            cstat & REVERSE_TRANSLUCENT != 0,
            blend_table,
        )
    }

    fn composite_layer(
        &self,
        background: u8,
        foreground: u8,
        translucent: bool,
        reverse: bool,
    ) -> u8 {
        self.composite_layer_with_table(background, foreground, translucent, reverse, 0)
    }

    fn composite_layer_with_table(
        &self,
        background: u8,
        foreground: u8,
        translucent: bool,
        reverse: bool,
        blend_table: u8,
    ) -> u8 {
        if !translucent {
            return foreground;
        }
        let table = self
            .blend_tables
            .get(&blend_table)
            .map(Vec::as_slice)
            .or_else(|| self.translucency.as_deref());
        let Some(table) = table else {
            return foreground;
        };
        let (back, front) = if reverse {
            (foreground, background)
        } else {
            (background, foreground)
        };
        table[usize::from(back) * 256 + usize::from(front)]
    }

    /// Composite a screen-space EDuke32 `rotatesprite` pixel. Orientation bit
    /// 1 selects translucency and bit 32 selects the reverse (lighter) table;
    /// bit 64 disables both masking and translucency at the caller.
    #[must_use]
    pub fn composite_hud_pixel(&self, background: u8, foreground: u8, orientation: i32) -> u8 {
        self.composite_layer(
            background,
            foreground,
            orientation & 1 != 0,
            orientation & 32 != 0,
        )
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
        palette: u8,
        angle_q16: i32,
        col: i32,
        width: i32,
        height: i32,
        horizon: i32,
        clock_120hz: u32,
    ) -> Option<(TileRef<'_>, i64, i64)> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let look_q16 = sky_look_q16(angle_q16, col, width);
        let look = (look_q16 >> 16) as i32;
        let slot = (look >> 8) as usize;
        let offset = self.sky_offsets.get(&base).map_or(0, |o| o[slot]);
        let tile = self.resolve_for_palette(base.saturating_add(offset), palette, clock_120hz)?;
        let u = look_q16.rem_euclid(256_i64 << 16) * i64::try_from(tile.w).ok()? / (256_i64 << 16);
        let yscale = i64::from(self.sky_yscale.get(&base).copied().unwrap_or(65536));
        let half = i64::from(height / 2);
        let sky_horizon = half + ((i64::from(horizon) - half) * yscale >> 16);
        Some((tile, u, sky_horizon))
    }

    /// Sample a DEF cubemap for one screen pixel. A palette-specific skybox
    /// wins; palette zero is the fallback for unlisted banks.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sample_skybox_at(
        &self,
        base: i16,
        palette: u8,
        angle_q16: i32,
        col: i32,
        width: i32,
        y: i32,
        height: i32,
        horizon: i32,
    ) -> Option<u8> {
        self.sample_skybox_color_at(base, palette, angle_q16, col, width, y, height, horizon)
            .map(|(pixel, _)| pixel)
    }

    /// [`Self::sample_skybox_at`] plus the face's source color (`0` = none).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sample_skybox_color_at(
        &self,
        base: i16,
        palette: u8,
        angle_q16: i32,
        col: i32,
        width: i32,
        y: i32,
        height: i32,
        horizon: i32,
    ) -> Option<(u8, u32)> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let skybox = self.skyboxes.get(&(base, palette)).or_else(|| {
            (palette != 0)
                .then(|| self.skyboxes.get(&(base, 0)))
                .flatten()
        })?;

        let (sin, cos) = crate::helpers::sincos_q14_16(angle_q16);
        let focal_x = i64::from((width / 2).max(1));
        let focal_y = i64::from((height / 2).max(1));
        let dx = i64::from(col - width / 2);
        let ray_x = i64::from(cos) * focal_x - i64::from(sin) * dx;
        let ray_y = i64::from(sin) * focal_x + i64::from(cos) * dx;
        let ray_up = i64::from(horizon - y) * focal_x * (1 << 14) / focal_y;
        let abs_x = ray_x.abs();
        let abs_y = ray_y.abs();
        let abs_up = ray_up.abs();

        // Cubemap coordinates are expressed as signed face-local values and
        // converted to [0, 1] only after selecting the dominant direction.
        // Face placement follows EDuke32 Polymost's sky cube (`skywal` corners
        // (-512,-512) → (512,-512) → (512,512) → (-512,512), wall i using DEF
        // face i): front is Build north (-y), right east (+x), back south
        // (+y), left west (-x); each wall's u runs from `skywal[i]` to
        // `skywal[i+1]` and v from top to bottom. The ceiling maps u/v to
        // world +x/+y and the floor to +x/-y (Polymost's floor y-flip).
        let (face, u_axis, v_axis, dominant) = if abs_x >= abs_y && abs_x >= abs_up {
            if ray_x >= 0 {
                (1, ray_y, -ray_up, abs_x) // right (+X, east)
            } else {
                (3, -ray_y, -ray_up, abs_x) // left (-X, west)
            }
        } else if abs_y >= abs_up {
            if ray_y >= 0 {
                (2, -ray_x, -ray_up, abs_y) // back (+Y, south)
            } else {
                (0, ray_x, -ray_up, abs_y) // front (-Y, north)
            }
        } else if ray_up >= 0 {
            (4, ray_x, ray_y, abs_up) // up
        } else {
            (5, ray_x, -ray_y, abs_up) // down
        };
        if dominant == 0 {
            return None;
        }
        let texture_coordinate =
            |axis: i64| -> u32 { ((axis + dominant) * 32_768 / dominant).clamp(0, 65_535) as u32 };
        let face = &skybox.faces[face];
        if face.width == 0 || face.height == 0 {
            return None;
        }
        let u = (u64::from(texture_coordinate(u_axis)) * u64::from(face.width) >> 16) as usize;
        let v = (u64::from(texture_coordinate(v_axis)) * u64::from(face.height) >> 16) as usize;
        let offset = u.min(usize::from(face.width) - 1) * usize::from(face.height)
            + v.min(usize::from(face.height) - 1);
        let pixel = face
            .pixels
            .get(offset)
            .copied()
            .filter(|pixel| *pixel != room_build::art::TRANSPARENT)?;
        let color = face
            .true_color
            .as_ref()
            .and_then(|colors| colors.get(offset).copied())
            .unwrap_or(0);
        Some((pixel, color))
    }

    /// Build `parascan`: the sky is a wall at fixed distance whose middle
    /// texel row sits on the (scaled) horizon at one texel per 200-line row,
    /// wrapping vertically. `row` and `horizon` are framebuffer rows.
    #[allow(clippy::too_many_arguments)]
    #[allow(
        dead_code,
        reason = "Build parascan sky, kept for the classic sky mode"
    )]
    fn sample_sky_parascan(
        &self,
        base: i16,
        angle_q16: i32,
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
        let look_q16 = sky_look_q16(angle_q16, col, width);
        let look = (look_q16 >> 16) as i32;
        let slot = (look >> 8) as usize;
        let offset = self.sky_offsets.get(&base).map_or(0, |o| o[slot]);
        let pic = base.saturating_add(offset);
        let u =
            (look_q16.rem_euclid(256_i64 << 16) * i64::from(tile_width) / (256_i64 << 16)) as u32;
        let yscale = i64::from(self.sky_yscale.get(&base).copied().unwrap_or(65536));
        let half = i64::from(height / 2);
        let sky_horizon = half + ((i64::from(horizon) - half) * yscale >> 16);
        let th = i64::from(tile_height.max(1));
        let v = th / 2
            + (i64::from(row) - sky_horizon) * 200 / i64::from(height)
            + i64::from(ypanning) * th / 256;
        let v = v.rem_euclid(th) as u32;
        self.sample_at(pic, u, v, clock_120hz)
            .filter(|&p| p != room_build::art::TRANSPARENT)
    }

    fn sample_sky_at(
        &self,
        base: i16,
        palette: u8,
        angle_q16: i32,
        col: i32,
        width: i32,
        y: i32,
        band: i32,
        clock_120hz: u32,
    ) -> Option<u8> {
        if width <= 0 {
            return None;
        }
        // The bootstrap projection has a 90-degree horizontal field of view.
        // Build's 2048-unit circle is divided into eight 256-unit sky slots.
        let look_q16 = sky_look_q16(angle_q16, col, width);
        let look = (look_q16 >> 16) as i32;
        let slot = (look >> 8) as usize;
        let offset = self.sky_offsets.get(&base).map_or(0, |o| o[slot]);
        let pic = base.saturating_add(offset);
        let (tile_width, tile_height) = self.dimensions_for_palette(pic, palette)?;
        let u =
            (look_q16.rem_euclid(256_i64 << 16) * i64::from(tile_width) / (256_i64 << 16)) as u32;
        let v = if band > 0 {
            (y * i32::from(tile_height) / band.max(1)).clamp(0, i32::from(tile_height) - 1) as u32
        } else {
            0
        };
        self.sample_at_for_palette(pic, palette, u, v, clock_120hz)
            .filter(|&p| p != room_build::art::TRANSPARENT)
    }
}

/// A resolved ART tile (column-major pixels) for per-span sampling.
#[derive(Clone, Copy)]
pub(crate) struct TileRef<'a> {
    pub w: usize,
    pub h: usize,
    pub pix: &'a [u8],
    /// Source colors of a DEF RGB image, same layout as `pix`.
    pub rgb: Option<&'a [u32]>,
    /// DEF `tint` applied to `rgb` samples.
    pub tint: Option<HicTint>,
}

impl TileRef<'_> {
    /// Texel at wrapped `(u, v)`.
    #[inline]
    pub fn at(&self, u: i64, v: i64) -> u8 {
        let x = u.rem_euclid(self.w as i64) as usize;
        let y = v.rem_euclid(self.h as i64) as usize;
        self.pix
            .get(x * self.h + y)
            .copied()
            .unwrap_or(room_build::art::TRANSPARENT)
    }

    /// Source color at wrapped `(u, v)` (`0` without true-colour data).
    #[inline]
    pub fn at_rgb(&self, u: i64, v: i64) -> u32 {
        let Some(rgb) = self.rgb else {
            return 0;
        };
        let x = u.rem_euclid(self.w as i64) as usize;
        let y = v.rem_euclid(self.h as i64) as usize;
        let color = rgb.get(x * self.h + y).copied().unwrap_or(0);
        self.tint.map_or(color, |tint| tint.apply(color))
    }
}

/// 8-bit indexed framebuffer (palette indices; 0 = clear).
#[derive(Clone, Debug)]
pub struct SoftFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Optional true-colour layer (`0xFFRRGGBB`; `0` shows `pixels`), set
    /// where DEF RGB images were drawn (ED7).
    pub true_color: Option<Vec<u32>>,
    deferred_translucency: Option<Vec<TranslucentPixel>>,
}

/// One indexed-color fragment held until all opaque geometry and sprites have
/// populated the shared depth buffer.
#[derive(Clone, Copy, Debug)]
pub(super) struct TranslucentPixel {
    pub(super) index: usize,
    pub(super) depth: i32,
    pub(super) foreground: u8,
    pub(super) reverse: bool,
    pub(super) blend_table: u8,
    /// True color of the foreground (`0` = indexed only).
    pub(super) true_color: u32,
}

const MAX_DEFERRED_TRANSLUCENT_PIXELS: usize = 2_000_000;

impl SoftFrame {
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_pixels(width, height, vec![0; (width * height) as usize])
    }

    /// Construct a framebuffer from an existing palette-index buffer.
    ///
    /// The caller supplies exactly `width * height` pixels, in row-major
    /// order, matching the public framebuffer fields.
    pub fn with_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels,
            true_color: None,
            deferred_translucency: None,
        }
    }

    /// Start recording true-colour pixels (all indexed until written).
    pub fn enable_true_color(&mut self) {
        self.true_color = Some(vec![0; self.pixels.len()]);
    }

    /// Final color of pixel `index`: its true color, else `palette`'s entry.
    #[must_use]
    pub fn pixel_color(&self, index: usize, palette: &[u8; 768]) -> [u8; 3] {
        let color = self
            .true_color
            .as_ref()
            .and_then(|colors| colors.get(index).copied())
            .unwrap_or(0);
        if color != 0 {
            return [(color >> 16) as u8, (color >> 8) as u8, color as u8];
        }
        let i = usize::from(self.pixels.get(index).copied().unwrap_or(0)) * 3;
        [palette[i], palette[i + 1], palette[i + 2]]
    }

    /// Set (or with `0`, clear) the true color of pixel `index`; a no-op
    /// when this frame has no true-colour layer.
    #[inline]
    pub fn set_true_color(&mut self, index: usize, color: u32) {
        if let Some(colors) = self.true_color.as_mut()
            && let Some(slot) = colors.get_mut(index)
        {
            *slot = color;
        }
    }

    /// Put an indexed pixel with its true color (`0` = indexed only).
    pub(crate) fn put_rgb(&mut self, x: i32, y: i32, c: u8, color: u32) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let index = (y as u32 * self.width + x as u32) as usize;
        self.pixels[index] = c;
        self.set_true_color(index, color);
    }

    pub(super) fn begin_deferred_translucency(&mut self) {
        self.deferred_translucency = Some(Vec::new());
    }

    pub(super) fn defer_translucent_pixel(&mut self, pixel: TranslucentPixel) -> bool {
        let Some(pixels) = self.deferred_translucency.as_mut() else {
            return false;
        };
        let limit = self
            .pixels
            .len()
            .saturating_mul(8)
            .min(MAX_DEFERRED_TRANSLUCENT_PIXELS);
        if pixels.len() < limit {
            pixels.push(pixel);
        }
        true
    }

    pub(super) fn finish_deferred_translucency(&mut self, tiles: &SoftTiles, zbuf: &[i32]) {
        let Some(mut pixels) = self.deferred_translucency.take() else {
            return;
        };
        // Stable ordering retains wall/sprite submission order at equal depth.
        pixels.sort_by_key(|pixel| std::cmp::Reverse(pixel.depth));
        for pixel in pixels {
            if pixel.index >= zbuf.len() || pixel.depth >= zbuf[pixel.index] {
                continue;
            }
            let background = self.pixels[pixel.index];
            let color = tiles.composite_layer_with_table(
                background,
                pixel.foreground,
                true,
                pixel.reverse,
                pixel.blend_table,
            );
            self.pixels[pixel.index] = color;
            self.blend_true_color_pixel(
                tiles,
                pixel.index,
                background,
                pixel.foreground,
                pixel.true_color,
                pixel.reverse,
            );
        }
    }

    /// Mirror an indexed translucent write in the true-colour layer: blend
    /// in RGB when either side is true colour, else stay indexed.
    pub fn blend_true_color_pixel(
        &mut self,
        tiles: &SoftTiles,
        index: usize,
        background: u8,
        foreground: u8,
        foreground_color: u32,
        reverse: bool,
    ) {
        let Some(colors) = self.true_color.as_ref() else {
            return;
        };
        let background_color = colors.get(index).copied().unwrap_or(0);
        if background_color == 0 && foreground_color == 0 {
            return;
        }
        let (Some(bg), Some(fg)) = (
            (background_color != 0)
                .then_some(background_color)
                .or_else(|| tiles.palette_color(background)),
            (foreground_color != 0)
                .then_some(foreground_color)
                .or_else(|| tiles.palette_color(foreground)),
        ) else {
            self.set_true_color(index, 0);
            return;
        };
        self.set_true_color(index, SoftTiles::blend_true_color(bg, fg, reverse));
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_sprite_pixel(
        &mut self,
        tiles: Option<&SoftTiles>,
        index: usize,
        depth: i32,
        foreground: u8,
        cstat: u16,
        blend_table: u8,
        true_color: u32,
    ) {
        const TRANSLUCENT: u16 = 2;
        const REVERSE_TRANSLUCENT: u16 = 512;
        let is_translucent = cstat & TRANSLUCENT != 0;
        if is_translucent
            && self.defer_translucent_pixel(TranslucentPixel {
                index,
                depth,
                foreground,
                reverse: cstat & REVERSE_TRANSLUCENT != 0,
                blend_table,
                true_color,
            })
        {
            return;
        }
        let background = self.pixels[index];
        let color = tiles.map_or(foreground, |tiles| {
            tiles.composite_sprite(background, foreground, cstat, blend_table)
        });
        self.pixels[index] = color;
        if is_translucent {
            if let Some(tiles) = tiles {
                self.blend_true_color_pixel(
                    tiles,
                    index,
                    background,
                    foreground,
                    true_color,
                    cstat & REVERSE_TRANSLUCENT != 0,
                );
            } else {
                self.set_true_color(index, 0);
            }
        } else {
            self.set_true_color(index, true_color);
        }
    }

    fn put(&mut self, x: i32, y: i32, c: u8) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let index = (y as u32 * self.width + x as u32) as usize;
        self.pixels[index] = c;
        self.set_true_color(index, 0);
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
    let global = i64::from(world.map_or(512, |w| w.render_visibility).max(0));
    let globvis = if v == 0 {
        global
    } else {
        (global * i64::from(v.wrapping_add(16))) >> 4
    };
    (i64::from(shade) + ((depth.max(0) * globvis) >> 19)).clamp(0, 127) as i8
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
    render_fp_textured_at_with_sprite_angle_offsets(world, view, frame, tiles, clock_120hz, None);
}

/// Textured draw with presentation-only MHK sprite angle offsets. Sprite
/// overrides are indexed by the original Build map sprite slot and never
/// modify the playsim world.
pub fn render_fp_textured_at_with_sprite_angle_offsets(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
    clock_120hz: u32,
    angle_offsets: Option<&BTreeMap<usize, i16>>,
) {
    render_fp_textured_at_with_def_voxels(
        world,
        view,
        frame,
        tiles,
        clock_120hz,
        angle_offsets,
        None,
    );
}

/// Textured Build draw with DEF voxel replacements and render-only MHK angles.
/// The voxel map is presentation data and cannot mutate the Build world.
pub fn render_fp_textured_at_with_def_voxels(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
    clock_120hz: u32,
    angle_offsets: Option<&BTreeMap<usize, i16>>,
    voxel_tiles: Option<&BTreeMap<i16, DefVoxelTile>>,
) {
    let (zbuf, drawn) = render_fp_textured_without_mirrors_at_with_sprite_angle_offsets(
        world,
        view,
        frame,
        tiles,
        clock_120hz,
        angle_offsets,
        voxel_tiles,
    );
    // Mirrors are searched among the sectors the portal pass reached, not
    // the (camera-only) `visible_sectors` of the incoming view.
    let mut mirror_view = view.clone();
    mirror_view.visible_sectors = drawn;
    render_fp_mirror_at(
        world,
        &mirror_view,
        frame,
        tiles,
        &zbuf,
        clock_120hz,
        560,
        angle_offsets,
        voxel_tiles,
    );
}

fn render_fp_textured_without_mirrors_at_with_sprite_angle_offsets(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: &SoftTiles,
    clock_120hz: u32,
    angle_offsets: Option<&BTreeMap<usize, i16>>,
    voxel_tiles: Option<&BTreeMap<i16, DefVoxelTile>>,
) -> (Vec<i32>, Vec<i16>) {
    let (mut zbuf, drawn, translucent_walls) =
        portal::render(world, view, frame, tiles, clock_120hz);
    frame.begin_deferred_translucency();
    portal::enqueue_translucent(frame, translucent_walls);
    // Sprites are drawn for every sector the portal pass actually reached.
    let mut sprite_view = view.clone();
    sprite_view.visible_sectors = drawn;
    render_fp_sprites_at_with_angle_offsets_and_voxels(
        world,
        &sprite_view,
        frame,
        Some(tiles),
        &mut zbuf,
        clock_120hz,
        angle_offsets,
        voxel_tiles,
    );
    frame.finish_deferred_translucency(tiles, &zbuf);
    (zbuf, sprite_view.visible_sectors)
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
    angle_offsets: Option<&BTreeMap<usize, i16>>,
    voxel_tiles: Option<&BTreeMap<i16, DefVoxelTile>>,
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
    if std::env::var_os("ROOM_MIRROR_DEBUG").is_some() {
        eprintln!(
            "mirror: visible {} nearest {nearest:?}",
            view.visible_sectors.len()
        );
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
    let reflected_ang_q16 = (i64::from(wall_angle * 2).wrapping_shl(16) - i64::from(view.ang_q16))
        .rem_euclid(2048_i64 << 16) as i32;
    let reflected_angle = (reflected_ang_q16 >> 16) as i16;
    let reflected_view = world.build_view_with_camera_q16(
        reflected.0,
        reflected.1,
        view.posz,
        reflected_angle,
        reflected_ang_q16,
        view.horiz,
        view.horiz_q16,
        wall.nextsector,
    );
    let mut reflected_frame = SoftFrame::new(frame.width, frame.height);
    if frame.true_color.is_some() {
        reflected_frame.enable_true_color();
    }
    let mirror_world = mirror_render_world(world);
    let mirror_world = mirror_world.as_ref().unwrap_or(world);
    let _ = render_fp_textured_without_mirrors_at_with_sprite_angle_offsets(
        mirror_world,
        &reflected_view,
        &mut reflected_frame,
        tiles,
        clock_120hz,
        angle_offsets,
        voxel_tiles,
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

fn mirror_render_world(world: &BuildWorld) -> Option<BuildWorld> {
    if world.mirror_viewer.is_none()
        && world.mirror_render_sprites.is_empty()
        && world.mirror_render_sprite_blends.is_empty()
    {
        return None;
    }
    let mut copy = world.clone();
    for (&index, sprite) in &world.mirror_render_sprites {
        if let Some(target) = copy.sprites.get_mut(index) {
            *target = *sprite;
        }
    }
    copy.render_sprite_blends.extend(
        world
            .mirror_render_sprite_blends
            .iter()
            .map(|(&index, &blend)| (index, blend)),
    );
    if let Some((index, yrepeat)) = world.mirror_viewer
        && let Ok(index) = usize::try_from(index)
        && let Some(sprite) = copy.sprites.get_mut(index)
    {
        // Chocolate draws the viewing player in mirrors only. Preserve the
        // event-mutated transient fields, restoring only the fields needed to
        // make the mirror copy drawable.
        sprite.yrepeat = yrepeat;
        sprite.cstat &= !0x8000;
    }
    Some(copy)
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
    let (sin, cos) = crate::helpers::sincos_q14_16(view.ang_q16);
    // Clip the mirror segment against the near plane: standing close to a
    // mirror puts one end beside/behind the camera, and the whole mirror used
    // to be rejected (leaving the bare MIRROR placeholder tile on screen).
    const NEAR: i64 = 32;
    let depth = |(x, y): (i32, i32)| {
        let dx = i64::from(x) - i64::from(view.posx);
        let dy = i64::from(y) - i64::from(view.posy);
        (dx * i64::from(cos) + dy * i64::from(sin)) >> 14
    };
    let (da, db) = (depth(a), depth(b));
    if da < NEAR && db < NEAR {
        return None;
    }
    let toward = |from: (i32, i32), to: (i32, i32), d_from: i64, d_to: i64| {
        let span = d_to - d_from;
        let lerp = |p: i32, q: i32| {
            let delta = i64::from(q) - i64::from(p);
            i64::from(p) + (delta * (NEAR + 1 - d_from) + span - 1) / span
        };
        (
            i32::try_from(lerp(from.0, to.0)).unwrap_or(from.0),
            i32::try_from(lerp(from.1, to.1)).unwrap_or(from.1),
        )
    };
    let a = if da < NEAR { toward(a, b, da, db) } else { a };
    let b = if db < NEAR { toward(b, a, db, da) } else { b };
    let project = |x, y, z| {
        let (rx, ry, rz) = cam_project(view, sin, cos, x, y, z)?;
        if rz <= 16 {
            return None;
        }
        let cx = width as i64 / 2;
        let cy = height as i64 / 2;
        let fx = cx.max(1);
        let fy = cy.max(1);
        let horizon = -horiz_pixel_offset_q16(view.horiz_q16, fy);
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
            // Perspective-correct: 1/z, not z, is linear in screen space.
            // Linear z overshoots between a near-clipped and a far vertex and
            // used to reject the whole mirror when standing close to it.
            let inv = (w0 as f64 / f64::from(z0.max(1))
                + w1 as f64 / f64::from(z1.max(1))
                + w2 as f64 / f64::from(z2.max(1)))
                / area as f64;
            let depth = if inv > 0.0 {
                (1.0 / inv) as i32
            } else {
                i32::MAX
            }
            .max(1);
            let index = y as usize * width + x as usize;
            if index >= scene_depth.len() || scene_depth[index].saturating_add(2) < depth {
                continue;
            }
            // Chocolate `completemirror`: the reflected camera's image is
            // mirrored left-right before it lands in the mirror window.
            let source = y as usize * width + (width - 1 - x as usize);
            target.pixels[index] = reflected.pixels[source];
            let color = reflected
                .true_color
                .as_ref()
                .and_then(|colors| colors.get(source).copied())
                .unwrap_or(0);
            target.set_true_color(index, color);
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
/// Build `horiz` → screen rows of horizon shift for a projection with
/// vertical focal length `focal_y` (pixels). Chocolate's hitscan slope for
/// `horiz` is `(horiz-100)/128` (in xy-equivalent units), so this keeps the
/// screen centre on the bullet path. `horiz > 100` looks up: the horizon row
/// moves down the screen.
#[must_use]
pub fn horiz_pixel_offset(horiz: i16, focal_y: i64) -> i32 {
    ((i64::from(horiz) - 100) * focal_y / 128) as i32
}

/// EDuke32 16.16 horizon → integer framebuffer pixel shift. The camera
/// retains sub-integer horizon values through projection and quantizes only at
/// the final pixel offset, which avoids losing small CON adjustments before
/// scaling them by the viewport height.
#[must_use]
pub fn horiz_pixel_offset_q16(horiz_q16: i32, focal_y: i64) -> i32 {
    let offset =
        (i128::from(horiz_q16) - (100_i128 << 16)) * i128::from(focal_y) / (128_i128 << 16);
    offset.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
}

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
    let horizon = (cy + horiz_pixel_offset_q16(view.horiz_q16, i64::from(cy.max(1)))).clamp(0, h);

    let sec = if view.sect >= 0 && view.sect < world.numsectors {
        &world.sectors[view.sect as usize]
    } else {
        return;
    };

    let floor_flat = wall_color(sec.floorpicnum, sec.floorshade);
    let ceil_flat = wall_color(sec.ceilingpicnum, sec.ceilingshade);
    let sky_parallax = (sec.ceilingstat & STAT_PARALLAX) != 0;
    let floor_parallax = (sec.floorstat & STAT_PARALLAX) != 0;

    let (sin, cos) = crate::helpers::sincos_q14_16(view.ang_q16);
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
                        ts.sample_skybox_at(picnum, pal, view.ang_q16, col, w, y, h, horizon)
                            .or_else(|| {
                                ts.sample_sky_at(
                                    picnum,
                                    pal,
                                    view.ang_q16,
                                    col,
                                    w,
                                    if above_horizon { y } else { y - horizon },
                                    if above_horizon { horizon } else { h - horizon },
                                    clock_120hz,
                                )
                            })
                    })
                    .unwrap_or_else(|| sky_color(picnum, view.ang, col, y))
            } else {
                project_plane_pixel(
                    tiles,
                    picnum,
                    pal,
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
                let palette_pixel = if ts.has_palette_variant(picnum, pal, clock_120hz) {
                    pix
                } else {
                    ts.remap_palette(pal, pix)
                };
                ts.shade_palette(sec_shade, palette_pixel)
            });
            frame.put(col, y, pix);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn project_plane_pixel(
    tiles: Option<&SoftTiles>,
    picnum: i16,
    palette: u8,
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
        .and_then(|ts| ts.dimensions_for_palette(picnum, palette))
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
            ts.sample_at_for_palette(picnum, palette, u, v, clock_120hz)
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
/// Build sprite cstat bit 1: composite through the translucency lookup.
const CSTAT_TRANSLUCENT: u16 = 2;
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
    let (sin, cos) = crate::helpers::sincos_q14_16(view.ang_q16);
    let w = frame.width as i32;
    let h = frame.height as i32;
    let cx = w / 2;
    let cy = h / 2;
    let focal_x = (w / 2).max(1);
    let focal_y = (h / 2).max(1);
    let horiz_off =
        -horiz_pixel_offset_q16(view.horiz_q16, i64::from((frame.height as i32 / 2).max(1)));
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
                    .and_then(|ts| ts.dimensions_for_palette(wall.picnum, wall.pal))
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
                            ts.sample_at_for_palette(wall.picnum, wall.pal, uu, vv, clock_120hz)
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
                                    let palette_pixel = if ts.has_palette_variant(
                                        wall.picnum,
                                        wall.pal,
                                        clock_120hz,
                                    ) {
                                        pixel
                                    } else {
                                        ts.remap_palette(wall.pal, pixel)
                                    };
                                    ts.shade_palette(wall.shade, palette_pixel)
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
    render_fp_sprites_at_with_angle_offsets(world, view, frame, tiles, zbuf, clock_120hz, None);
}

/// Face, wall-aligned and floor-aligned sprites with render-only angle
/// offsets from a matched EDuke32 MHK file.
pub fn render_fp_sprites_at_with_angle_offsets(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
    clock_120hz: u32,
    angle_offsets: Option<&BTreeMap<usize, i16>>,
) {
    render_fp_sprites_at_with_angle_offsets_and_voxels(
        world,
        view,
        frame,
        tiles,
        zbuf,
        clock_120hz,
        angle_offsets,
        None,
    );
}

/// Face, wall-aligned, floor-aligned, and DEF voxel sprites. Voxel assets are
/// render-only inputs, like ART tiles and the angle override table.
pub fn render_fp_sprites_at_with_angle_offsets_and_voxels(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
    clock_120hz: u32,
    angle_offsets: Option<&BTreeMap<usize, i16>>,
    voxel_tiles: Option<&BTreeMap<i16, DefVoxelTile>>,
) {
    let (sin, cos) = crate::helpers::sincos_q14_16(view.ang_q16);
    let w = frame.width as i32;
    let h = frame.height as i32;
    let cx = w / 2;
    let cy = h / 2;
    let focal_x = (w / 2).max(1);
    let focal_y = (h / 2).max(1);
    let horiz_off =
        -horiz_pixel_offset_q16(view.horiz_q16, i64::from((frame.height as i32 / 2).max(1)));

    // Build renders opaque sprites before its translucent pass. Translucent
    // sprites then blend far-to-near against the completed opaque scene, and
    // must not occlude later translucent layers with a depth write.
    let mut sprite_order = (0..crate::world::MAXSPRITES)
        .filter(|&index| {
            let sprite = &world.sprites[index];
            sprite.sectnum >= 0
                && !lists_stat_free(world, index)
                && sprite.cstat & CSTAT_INVISIBLE == 0
                && sprite.xrepeat != 0
                && sprite.yrepeat != 0
                && view.visible_sectors.contains(&sprite.sectnum)
        })
        .map(|index| {
            let sprite = &world.sprites[index];
            let translucent = sprite.cstat & CSTAT_TRANSLUCENT != 0;
            let depth = if translucent {
                cam_project(view, sin, cos, sprite.x, sprite.y, sprite.z)
                    .map_or(i64::MIN, |(_, _, depth)| depth)
            } else {
                0
            };
            (translucent, depth, index)
        })
        .collect::<Vec<_>>();
    sprite_order.sort_by(|left, right| match (left.0, right.0) {
        (false, false) => left.2.cmp(&right.2),
        (false, true) => std::cmp::Ordering::Less,
        (true, false) => std::cmp::Ordering::Greater,
        (true, true) => right.1.cmp(&left.1).then_with(|| left.2.cmp(&right.2)),
    });

    let mut voxel_budget = MAX_SOFT_VOXEL_SAMPLES;
    let direct_voxel_models = (voxel_tiles.is_some()
        && sprite_order
            .iter()
            .any(|(_, _, index)| world.sprites[*index].cstat & 48 == 48))
    .then(|| {
        let mut models = BTreeMap::new();
        for voxel in voxel_tiles.into_iter().flat_map(|tiles| tiles.values()) {
            models.entry(voxel.model_index).or_insert(voxel);
        }
        models
    });
    for (_, _, i) in sprite_order {
        let original = &world.sprites[i];
        let mut sprite_copy;
        let sp = if let Some(offset) = angle_offsets.and_then(|overrides| overrides.get(&i)) {
            sprite_copy = *original;
            sprite_copy.ang =
                (i32::from(sprite_copy.ang) + i32::from(*offset)).rem_euclid(2048) as i16;
            &sprite_copy
        } else {
            original
        };
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
        let voxel = if sp.cstat & 48 == 48 {
            direct_voxel_models
                .as_ref()
                .and_then(|models| models.get(&sp.picnum).copied())
        } else {
            voxel_tiles.and_then(|voxel_tiles| voxel_tiles.get(&sp.picnum))
        };
        if voxel.is_none()
            && let Some(ts) = tiles
            && ts
                .dimensions_for_palette(sp.picnum, sp.pal)
                .is_none_or(|(tw, th)| tw == 0 || th == 0)
        {
            continue;
        }
        let blend_table = world
            .render_sprite_blends
            .get(&i)
            .and_then(|value| u8::try_from(*value).ok())
            .unwrap_or(0);
        if let Some(voxel) = voxel
            && render_def_voxel_sprite(
                world,
                view,
                frame,
                tiles,
                zbuf,
                sp,
                voxel,
                blend_table,
                &mut voxel_budget,
            )
        {
            continue;
        }
        if sp.cstat & CSTAT_WALL_ALIGN != 0 {
            render_wall_aligned_sprite(
                world,
                view,
                frame,
                tiles,
                zbuf,
                sp,
                clock_120hz,
                blend_table,
            );
            continue;
        }
        if sp.cstat & CSTAT_FLOOR_ALIGN != 0 {
            render_floor_aligned_sprite(
                world,
                view,
                frame,
                tiles,
                zbuf,
                sp,
                clock_120hz,
                blend_table,
            );
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
            .and_then(|ts| ts.dimensions_for_palette(sp.picnum, sp.pal))
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
        let tile = tiles.and_then(|ts| ts.resolve_for_palette(sp.picnum, sp.pal, clock_120hz));
        let (tw, th) = tile.map_or((64, 64), |t| (t.w as i64, t.h as i64));
        let fog = sprite_fog_shade(Some(world), sp.shade, i64::from(z), sp.sectnum);
        let use_palette_variant =
            tiles.is_some_and(|ts| ts.has_palette_variant(sp.picnum, sp.pal, clock_120hz));
        let pal_table = (!use_palette_variant)
            .then(|| tiles.and_then(|ts| ts.pal_table(sp.pal)))
            .flatten();
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
                let (pix, color) = match tile.map(|t| (t.at(u, v), t)) {
                    Some((pixel, _)) if pixel == room_build::art::TRANSPARENT => continue,
                    Some((pixel, t)) => {
                        let p = pal_table.map_or(pixel, |t| t[usize::from(pixel)]);
                        (
                            shade_row.map_or(p, |r| r[usize::from(p)]),
                            tiles.map_or(0, |ts| ts.shade_true_color(t.at_rgb(u, v), fog)),
                        )
                    }
                    None => (flat, 0),
                };
                frame.draw_sprite_pixel(tiles, depth_index, z, pix, sp.cstat, blend_table, color);
                if sp.cstat & CSTAT_TRANSLUCENT == 0 {
                    zbuf[depth_index] = z;
                }
            }
        }
    }
}

const MAX_SOFT_VOXEL_SAMPLES: usize = 250_000;
const MAX_SOFT_VOXEL_OCCUPANCY_CELLS: usize = 4_194_304;

/// Draw one KVX model as exposed, depth-tested cube faces. The sample and
/// occupancy limits bound the work for unusually dense user models; `false`
/// asks the caller to retain the ART sprite fallback in that case.
#[allow(clippy::too_many_arguments)]
fn render_def_voxel_sprite(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
    sprite: &SpriteT,
    voxel: &DefVoxelTile,
    blend_table: u8,
    sample_budget: &mut usize,
) -> bool {
    let Some(base_mip) = voxel.model.mipmaps.first() else {
        return false;
    };
    if !voxel.scale.is_finite() || voxel.scale <= 0.0 {
        return false;
    }
    let base_dimensions = base_mip.dimensions;
    let frame_width_i32 = i32::try_from(frame.width).unwrap_or(i32::MAX);
    let frame_height_i32 = i32::try_from(frame.height).unwrap_or(i32::MAX);
    let focal_x = f64::from((frame_width_i32 / 2).max(1));
    let focal_y = f64::from((frame_height_i32 / 2).max(1));
    let center_depth = cam_project(
        view,
        crate::helpers::sincos_q14_16(view.ang_q16).0,
        crate::helpers::sincos_q14_16(view.ang_q16).1,
        sprite.x,
        sprite.y,
        sprite.z,
    )
    .map_or(16, |(_, _, depth)| depth.max(16));
    let base_scale_xy = f64::from(voxel.scale) * f64::from(sprite.xrepeat) / 64.0 * 16.0;
    let base_scale_z = f64::from(voxel.scale) * f64::from(sprite.yrepeat) / 64.0 * 256.0;
    let projected_voxel = (focal_x * base_scale_xy / center_depth as f64)
        .max(focal_y * base_scale_z / (16.0 * center_depth as f64))
        .max(0.001);
    let target_mip_factor = (1.0 / projected_voxel).max(1.0);
    let mut selected_mip_index = 0usize;
    let mut selected_mip = base_mip;
    let mut selected_factor = 1.0f64;
    for (index, mip) in voxel.model.mipmaps.iter().enumerate().skip(1) {
        let factor = (0..3)
            .map(|axis| f64::from(base_dimensions[axis]) / f64::from(mip.dimensions[axis].max(1)))
            .fold(1.0f64, f64::max);
        if factor > target_mip_factor || factor < selected_factor {
            break;
        }
        selected_mip_index = index;
        selected_mip = mip;
        selected_factor = factor;
    }
    let mip_sample_count = voxel
        .mip_sample_counts
        .get(selected_mip_index)
        .copied()
        .unwrap_or_else(|| selected_mip.surface_voxel_count());
    if mip_sample_count > *sample_budget {
        return false;
    }
    let mip = selected_mip;
    let [size_x, size_y, size_z] = mip.dimensions.map(usize::from);
    if size_x == 0 || size_y == 0 || size_z == 0 {
        return false;
    }
    let Some(occupancy_len) = size_x
        .checked_mul(size_y)
        .and_then(|value| value.checked_mul(size_z))
        .filter(|&value| value <= MAX_SOFT_VOXEL_OCCUPANCY_CELLS)
    else {
        return false;
    };
    let mut occupancy = vec![0u16; occupancy_len];
    let voxel_index = |x: usize, y: usize, z: usize| (x * size_y + y) * size_z + z;
    for (column_index, column) in mip.columns.iter().enumerate() {
        let voxel_x = column_index / size_y;
        let voxel_y = column_index % size_y;
        for slab in column {
            for (z_offset, &color) in slab.colors.iter().enumerate() {
                let voxel_z = usize::from(slab.z_top) + z_offset;
                if voxel_x < size_x && voxel_y < size_y && voxel_z < size_z {
                    occupancy[voxel_index(voxel_x, voxel_y, voxel_z)] = u16::from(color) + 1;
                }
            }
        }
    }

    let angle = if voxel.rotate { sprite.ang } else { 0 };
    let (sin, cos) = crate::helpers::sincos_q14(i32::from(angle));
    let (view_sin, view_cos) = crate::helpers::sincos_q14_16(view.ang_q16);
    let tile_scale_x = base_scale_xy * f64::from(base_dimensions[0]) / f64::from(mip.dimensions[0]);
    let tile_scale_y = base_scale_xy * f64::from(base_dimensions[1]) / f64::from(mip.dimensions[1]);
    let tile_scale_z = base_scale_z * f64::from(base_dimensions[2]) / f64::from(mip.dimensions[2]);
    if tile_scale_x <= 0.0 || tile_scale_y <= 0.0 || tile_scale_z <= 0.0 {
        return false;
    }
    let pivot = mip.pivot.map(|value| f64::from(value) / 256.0);
    let palette_map = tiles.and_then(|tiles| tiles.pal_table(sprite.pal));
    let shade_row = tiles.and_then(|tiles| {
        let center_depth = cam_project(view, view_sin, view_cos, sprite.x, sprite.y, sprite.z)
            .map_or(0, |(_, _, depth)| depth);
        tiles.shade_row(sprite_fog_shade(
            Some(world),
            sprite.shade,
            center_depth,
            sprite.sectnum,
        ))
    });
    let sprite_sin = f64::from(sin) / 16384.0;
    let sprite_cos = f64::from(cos) / 16384.0;
    let camera_dx = f64::from(view.posx) - f64::from(sprite.x);
    let camera_dy = f64::from(view.posy) - f64::from(sprite.y);
    let camera_local = [
        (camera_dx * sprite_cos + camera_dy * sprite_sin) / tile_scale_x + pivot[0],
        (-camera_dx * sprite_sin + camera_dy * sprite_cos) / tile_scale_y + pivot[1],
        (f64::from(sprite.z) - f64::from(view.posz)) / tile_scale_z + pivot[2],
    ];
    let center_x = f64::from(frame_width_i32 / 2);
    let center_y = f64::from(frame_height_i32 / 2);
    let horizon_offset = f64::from(-horiz_pixel_offset_q16(
        view.horiz_q16,
        i64::from((frame_height_i32 / 2).max(1)),
    ));
    let mut sprite_depth = vec![i32::MAX; zbuf.len()];
    let mut sprite_pixels = vec![0u8; zbuf.len()];
    let mut touched_pixels = Vec::new();

    for (column_index, column) in mip.columns.iter().enumerate() {
        let voxel_x = column_index / size_y;
        let voxel_y = column_index % size_y;
        for slab in column {
            for (z_offset, &color) in slab.colors.iter().enumerate() {
                let voxel_z = usize::from(slab.z_top) + z_offset;
                if voxel_x >= size_x || voxel_y >= size_y || voxel_z >= size_z {
                    continue;
                }
                let voxel_coord = [voxel_x, voxel_y, voxel_z];
                let origin = [voxel_x as f64, voxel_y as f64, voxel_z as f64];
                let color = voxel.palette_map[usize::from(color)];
                let mut pixel = palette_map.map_or(color, |table| table[usize::from(color)]);
                if let Some(row) = shade_row {
                    pixel = row[usize::from(pixel)];
                }

                for (axis, sign) in [(0usize, -1i32), (0, 1), (1, -1), (1, 1), (2, -1), (2, 1)] {
                    let center = origin[axis] + 0.5;
                    if f64::from(sign) * (camera_local[axis] - center) <= 0.0 {
                        continue;
                    }
                    let neighbor_coord = if sign < 0 {
                        if voxel_coord[axis] == 0 {
                            None
                        } else {
                            Some(voxel_coord[axis] - 1)
                        }
                    } else {
                        let coordinate = voxel_coord[axis] + 1;
                        (coordinate < [size_x, size_y, size_z][axis]).then_some(coordinate)
                    };
                    let neighbor_solid = neighbor_coord.is_some_and(|coordinate| {
                        let mut neighbor = voxel_coord;
                        neighbor[axis] = coordinate;
                        occupancy[voxel_index(neighbor[0], neighbor[1], neighbor[2])] != 0
                    });
                    if neighbor_solid {
                        continue;
                    }
                    let corners = voxel_face_corners(origin, axis, sign);
                    let projected = corners.map(|point| {
                        project_voxel_point(
                            view,
                            sprite,
                            point,
                            pivot,
                            tile_scale_x,
                            tile_scale_y,
                            tile_scale_z,
                            sin,
                            cos,
                            view_sin,
                            view_cos,
                            focal_x,
                            focal_y,
                            center_x,
                            center_y,
                            horizon_offset,
                        )
                    });
                    let [Some(p0), Some(p1), Some(p2), Some(p3)] = projected else {
                        continue;
                    };
                    raster_voxel_triangle(
                        frame,
                        [p0, p1, p2],
                        pixel,
                        &mut sprite_depth,
                        &mut sprite_pixels,
                        &mut touched_pixels,
                    );
                    raster_voxel_triangle(
                        frame,
                        [p0, p2, p3],
                        pixel,
                        &mut sprite_depth,
                        &mut sprite_pixels,
                        &mut touched_pixels,
                    );
                }
            }
        }
    }
    for depth_index in touched_pixels {
        let depth = sprite_depth[depth_index];
        if depth >= zbuf[depth_index] {
            sprite_depth[depth_index] = i32::MAX;
            continue;
        }
        let pixel = sprite_pixels[depth_index];
        frame.draw_sprite_pixel(
            tiles,
            depth_index,
            depth,
            pixel,
            sprite.cstat,
            blend_table,
            0,
        );
        if sprite.cstat & CSTAT_TRANSLUCENT == 0 {
            zbuf[depth_index] = depth;
        }
        sprite_depth[depth_index] = i32::MAX;
    }
    *sample_budget = sample_budget.saturating_sub(mip_sample_count);
    true
}

#[allow(clippy::too_many_arguments)]
fn project_voxel_point(
    view: &BuildView,
    sprite: &SpriteT,
    point: [f64; 3],
    pivot: [f64; 3],
    tile_scale_x: f64,
    tile_scale_y: f64,
    tile_scale_z: f64,
    sprite_sin: i32,
    sprite_cos: i32,
    view_sin: i32,
    view_cos: i32,
    focal_x: f64,
    focal_y: f64,
    center_x: f64,
    center_y: f64,
    horizon_offset: f64,
) -> Option<ProjectedVoxelPoint> {
    let local_x = (point[0] - pivot[0]) * tile_scale_x;
    let local_y = (point[1] - pivot[1]) * tile_scale_y;
    let local_up = (point[2] - pivot[2]) * tile_scale_z;
    let world_x = f64::from(sprite.x)
        + (local_x * f64::from(sprite_cos) - local_y * f64::from(sprite_sin)) / 16384.0;
    let world_y = f64::from(sprite.y)
        + (local_x * f64::from(sprite_sin) + local_y * f64::from(sprite_cos)) / 16384.0;
    let world_z = f64::from(sprite.z) - local_up;
    let world_x = finite_i32(world_x)?;
    let world_y = finite_i32(world_y)?;
    let world_z = finite_i32(world_z)?;
    let (rx, ry, depth) = cam_project(view, view_sin, view_cos, world_x, world_y, world_z)?;
    if depth <= 16 {
        return None;
    }
    let depth_f = depth as f64;
    let screen_x = center_x + focal_x * rx as f64 / depth_f;
    let screen_y = center_y - focal_y * ry as f64 / depth_f - horizon_offset;
    if !screen_x.is_finite() || !screen_y.is_finite() {
        return None;
    }
    Some(ProjectedVoxelPoint {
        x: screen_x,
        y: screen_y,
        depth: depth_f,
    })
}

#[derive(Clone, Copy)]
struct ProjectedVoxelPoint {
    x: f64,
    y: f64,
    depth: f64,
}

fn voxel_face_corners(origin: [f64; 3], axis: usize, sign: i32) -> [[f64; 3]; 4] {
    let mut low = origin;
    let mut high = origin.map(|value| value + 1.0);
    let face = if sign > 0 { high[axis] } else { low[axis] };
    low[axis] = face;
    high[axis] = face;
    let (u_axis, v_axis) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let mut corners = [low; 4];
    corners[1][u_axis] = high[u_axis];
    corners[2][u_axis] = high[u_axis];
    corners[2][v_axis] = high[v_axis];
    corners[3][v_axis] = high[v_axis];
    corners
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn raster_voxel_triangle(
    frame: &mut SoftFrame,
    points: [ProjectedVoxelPoint; 3],
    color: u8,
    sprite_depth: &mut [i32],
    sprite_pixels: &mut [u8],
    touched_pixels: &mut Vec<usize>,
) {
    let area = voxel_edge(points[0], points[1], points[2].x, points[2].y);
    if !area.is_finite() || area.abs() < 1.0e-9 {
        return;
    }
    let width = frame.width as i32;
    let height = frame.height as i32;
    if width <= 0 || height <= 0 {
        return;
    }
    let left = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0)
        .min(f64::from(width)) as i32;
    let right = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .max(0.0)
        .min(f64::from(width)) as i32;
    let top = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0)
        .min(f64::from(height)) as i32;
    let bottom = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .max(0.0)
        .min(f64::from(height)) as i32;
    for y in top..bottom {
        for x in left..right {
            let sample_x = f64::from(x) + 0.5;
            let sample_y = f64::from(y) + 0.5;
            let weights = [
                voxel_edge(points[1], points[2], sample_x, sample_y) / area,
                voxel_edge(points[2], points[0], sample_x, sample_y) / area,
                voxel_edge(points[0], points[1], sample_x, sample_y) / area,
            ];
            if weights.iter().any(|weight| *weight < -1.0e-9) {
                continue;
            }
            let reciprocal_depth = weights
                .iter()
                .zip(points)
                .map(|(weight, point)| weight / point.depth)
                .sum::<f64>();
            if reciprocal_depth <= 0.0 || !reciprocal_depth.is_finite() {
                continue;
            }
            let Some(depth) = finite_i32((1.0 / reciprocal_depth).round()) else {
                continue;
            };
            let depth_index = y as usize * frame.width as usize + x as usize;
            let Some(current_depth) = sprite_depth.get_mut(depth_index) else {
                continue;
            };
            if depth >= *current_depth {
                continue;
            }
            if *current_depth == i32::MAX {
                touched_pixels.push(depth_index);
            }
            *current_depth = depth;
            if let Some(pixel) = sprite_pixels.get_mut(depth_index) {
                *pixel = color;
            }
        }
    }
}

fn voxel_edge(a: ProjectedVoxelPoint, b: ProjectedVoxelPoint, x: f64, y: f64) -> f64 {
    (x - a.x) * (b.y - a.y) - (y - a.y) * (b.x - a.x)
}

fn finite_i32(value: f64) -> Option<i32> {
    (value.is_finite() && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX))
        .then(|| value.round() as i32)
}

/// Draw a horizontal Build sprite as two textured, depth-tested triangles.
/// Build's floor-aligned tiles lie in the XY plane, with their dimensions
/// scaled by the sprite repeats (four Build units per texel/repeat unit).
fn render_floor_aligned_sprite(
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
    sprite: &SpriteT,
    clock_120hz: u32,
    blend_table: u8,
) {
    let Some(tiles) = tiles else { return };
    let Some((tile_width, tile_height)) = tiles.dimensions_for_palette(sprite.picnum, sprite.pal)
    else {
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
    let (sin_view, cos_view) = crate::helpers::sincos_q14_16(view.ang_q16);
    let center_x = frame.width as i32 / 2;
    let center_y = frame.height as i32 / 2;
    let focal_x = (frame.width as i32 / 2).max(1);
    let focal_y = (frame.height as i32 / 2).max(1);
    let horizon_off =
        -horiz_pixel_offset_q16(view.horiz_q16, i64::from((frame.height as i32 / 2).max(1)));
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
        world,
        tiles,
        frame,
        zbuf,
        sprite,
        clock_120hz,
        flat,
        blend_table,
        [projected[0], projected[1], projected[2]],
    );
    draw_sprite_triangle(
        world,
        tiles,
        frame,
        zbuf,
        sprite,
        clock_120hz,
        flat,
        blend_table,
        [projected[0], projected[2], projected[3]],
    );
}

fn draw_sprite_triangle(
    world: &BuildWorld,
    tiles: &SoftTiles,
    frame: &mut SoftFrame,
    zbuf: &mut [i32],
    sprite: &SpriteT,
    clock_120hz: u32,
    flat: u8,
    blend_table: u8,
    vertices: [(i32, i32, i32, i32, i32); 3],
) {
    let [
        (x0, y0, z0, u0, v0),
        (x1, y1, z1, u1, v1),
        (x2, y2, z2, u2, v2),
    ] = vertices;
    let tile = tiles.resolve_for_palette(sprite.picnum, sprite.pal, clock_120hz);
    let use_palette_variant = tiles.has_palette_variant(sprite.picnum, sprite.pal, clock_120hz);
    let pal_table = (!use_palette_variant)
        .then(|| tiles.pal_table(sprite.pal))
        .flatten();
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
            let texel = tile.map(|t| (t, u * t.w as i64 / 64, v * t.h as i64 / 64));
            let (pixel, color) = match texel.map(|(t, tu, tv)| (t.at(tu, tv), t, tu, tv)) {
                Some((pixel, ..)) if pixel == room_build::art::TRANSPARENT => continue,
                Some((pixel, t, tu, tv)) => {
                    let p = pal_table.map_or(pixel, |t| t[usize::from(pixel)]);
                    let shade = sprite_fog_shade(
                        Some(world),
                        sprite.shade,
                        i64::from(depth),
                        sprite.sectnum,
                    );
                    (
                        tiles.shade_row(shade).map_or(p, |r| r[usize::from(p)]),
                        tiles.shade_true_color(t.at_rgb(tu, tv), shade),
                    )
                }
                None => (flat, 0),
            };
            if sprite.cstat & CSTAT_TRANSLUCENT == 0 {
                zbuf[index] = depth;
            }
            frame.draw_sprite_pixel(
                Some(tiles),
                index,
                depth,
                pixel,
                sprite.cstat,
                blend_table,
                color,
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
    world: &BuildWorld,
    view: &BuildView,
    frame: &mut SoftFrame,
    tiles: Option<&SoftTiles>,
    zbuf: &mut [i32],
    sprite: &SpriteT,
    clock_120hz: u32,
    blend_table: u8,
) {
    let Some(tiles) = tiles else { return };
    let Some((tile_width, tile_height)) = tiles.dimensions_for_palette(sprite.picnum, sprite.pal)
    else {
        return;
    };
    let wall_tile = tiles.resolve_for_palette(sprite.picnum, sprite.pal, clock_120hz);
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
    let (sin_view, cos_view) = crate::helpers::sincos_q14_16(view.ang_q16);
    let focal_x = f64::from((frame.width as i32 / 2).max(1));
    let focal_y = f64::from((frame.height as i32 / 2).max(1));
    let center_x = f64::from(frame.width as i32 / 2);
    let center_y = f64::from(frame.height as i32 / 2);
    let horizon_off = f64::from(-horiz_pixel_offset_q16(
        view.horiz_q16,
        i64::from((frame.height as i32 / 2).max(1)),
    ));
    // Camera space (same basis as `cam_project`), without its behind-camera
    // rejection: the segment is clipped against a near plane instead, so a
    // poster whose one end is behind the eye still draws its visible part.
    let to_cam = |x: i32, y: i32| -> (f64, f64) {
        let dx = (i64::from(x) - i64::from(view.posx)) as f64;
        let dy = (i64::from(y) - i64::from(view.posy)) as f64;
        let (s, c) = (f64::from(sin_view) / 16384.0, f64::from(cos_view) / 16384.0);
        (-dx * s + dy * c, dx * c + dy * s)
    };
    const NEAR: f64 = 16.0;
    let (lrx, lrz) = to_cam(left_x, left_y);
    let (rrx, rrz) = to_cam(right_x, right_y);
    if lrz <= NEAR && rrz <= NEAR {
        return;
    }
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    if lrz < NEAR {
        t0 = (NEAR - lrz) / (rrz - lrz);
    }
    if rrz < NEAR {
        t1 = (NEAR - lrz) / (rrz - lrz);
    }
    let at = |t: f64| (lrx + (rrx - lrx) * t, lrz + (rrz - lrz) * t);
    let (mut ax, mut az) = at(t0);
    let (mut bx, mut bz) = at(t1);
    let (mut ua, mut ub) = (t0, t1);
    let ry_top = (f64::from(view.posz) - f64::from(top_z)) / 16.0;
    let ry_bottom = (f64::from(view.posz) - f64::from(bottom_z)) / 16.0;
    let mut sxa = center_x + focal_x * ax / az;
    let mut sxb = center_x + focal_x * bx / bz;
    let mut reverse_u = sprite.cstat & CSTAT_XFLIP != 0;
    if sxa > sxb {
        // A one-sided Build wall sprite is culled when viewed from its back.
        if sprite.cstat & CSTAT_ONE_SIDED != 0 {
            return;
        }
        std::mem::swap(&mut sxa, &mut sxb);
        std::mem::swap(&mut ax, &mut bx);
        std::mem::swap(&mut az, &mut bz);
        std::mem::swap(&mut ua, &mut ub);
    }
    let _ = (ax, bx);
    let left = (sxa.ceil() as i32).max(0);
    let right = (sxb.ceil() as i32).min(frame.width as i32);
    if right <= left || sxb - sxa < 1e-6 {
        return;
    }
    let flat = wall_color(sprite.picnum, sprite.shade);
    let tw = i64::from(tile_width);
    let th = i64::from(tile_height);
    let (iza, izb) = (1.0 / az, 1.0 / bz);
    for col in left..right {
        // Perspective-correct: 1/z and u/z are linear in screen space.
        let s = ((f64::from(col) + 0.5 - sxa) / (sxb - sxa)).clamp(0.0, 1.0);
        let inv_z = iza + (izb - iza) * s;
        let u_frac = (ua * iza + (ub * izb - ua * iza) * s) / inv_z;
        let depth = (1.0 / inv_z) as i32;
        let top = center_y - focal_y * ry_top * inv_z - horizon_off;
        let bottom = center_y - focal_y * ry_bottom * inv_z - horizon_off;
        if bottom <= top {
            continue;
        }
        // Posters sit flush on walls: win depth ties against the wall.
        let test_depth = depth - (depth >> 7) - 2;
        let y0 = (top.ceil() as i32).max(0);
        let y1 = (bottom.ceil() as i32).min(frame.height as i32);
        let u0 = ((u_frac * tw as f64) as i64).clamp(0, tw - 1);
        let u = if reverse_u { tw - 1 - u0 } else { u0 };
        for y in y0..y1 {
            let index = y as usize * frame.width as usize + col as usize;
            if index >= zbuf.len() || test_depth >= zbuf[index] {
                continue;
            }
            let v0 =
                (((f64::from(y) + 0.5 - top) / (bottom - top) * th as f64) as i64).clamp(0, th - 1);
            let v = if sprite.cstat & CSTAT_YFLIP != 0 {
                th - 1 - v0
            } else {
                v0
            };
            let (pix, color) = match wall_tile.map(|t| (t.at(u, v), t)) {
                Some((pixel, _)) if pixel == room_build::art::TRANSPARENT => continue,
                Some((pixel, t)) => {
                    let palette_pixel =
                        if tiles.has_palette_variant(sprite.picnum, sprite.pal, clock_120hz) {
                            pixel
                        } else {
                            tiles.remap_palette(sprite.pal, pixel)
                        };
                    let shade = sprite_fog_shade(
                        Some(world),
                        sprite.shade,
                        i64::from(depth),
                        sprite.sectnum,
                    );
                    (
                        tiles.shade_palette(shade, palette_pixel),
                        tiles.shade_true_color(t.at_rgb(u, v), shade),
                    )
                }
                None => (flat, 0),
            };
            if sprite.cstat & CSTAT_TRANSLUCENT == 0 {
                zbuf[index] = depth;
            }
            frame.draw_sprite_pixel(
                Some(tiles),
                index,
                depth,
                pix,
                sprite.cstat,
                blend_table,
                color,
            );
        }
    }
    let _ = &mut reverse_u;
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
    let (sin, cos) = crate::helpers::sincos_q14_16(view.ang_q16);
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
    fn kvx_palette_maps_embedded_six_bit_rgb_to_build_indices() {
        let mut model_palette = [0u8; 768];
        model_palette[2 * 3..2 * 3 + 3].copy_from_slice(&[63, 0, 0]);
        model_palette[3 * 3..3 * 3 + 3].copy_from_slice(&[0, 63, 0]);
        let model = KvxDocument {
            mipmaps: Vec::new(),
            palette: model_palette,
        };
        let mut build_palette = [0u8; 768];
        build_palette[7 * 3..7 * 3 + 3].copy_from_slice(&[255, 0, 0]);
        build_palette[11 * 3..11 * 3 + 3].copy_from_slice(&[0, 255, 0]);

        let mapped = kvx_palette_map(&model, &build_palette);

        assert_eq!(mapped[2], 7);
        assert_eq!(mapped[3], 11);
    }

    #[test]
    fn mirror_render_world_applies_per_view_sprite_and_blend_edits() {
        let mut world = BuildWorld::new();
        world.sprites[0] = SpriteT {
            cstat: 0x8000,
            yrepeat: 0,
            ..SpriteT::default()
        };
        world.sprites[5] = SpriteT {
            x: 100,
            xrepeat: 16,
            yrepeat: 16,
            ..SpriteT::default()
        };
        world.mirror_viewer = Some((0, 40));
        world.mirror_render_sprites.insert(
            5,
            SpriteT {
                x: 900,
                ..world.sprites[5]
            },
        );
        world.render_sprite_blends.insert(5, 2);
        world.mirror_render_sprite_blends.insert(5, 201);

        let mirrored = mirror_render_world(&world).expect("mirror render copy");

        assert_eq!(mirrored.sprites[0].yrepeat, 40);
        assert_eq!(mirrored.sprites[0].cstat & 0x8000, 0);
        assert_eq!(mirrored.sprites[5].x, 900);
        assert_eq!(mirrored.render_sprite_blends.get(&5), Some(&201));
        assert_eq!(world.sprites[0].yrepeat, 0);
        assert_ne!(world.sprites[0].cstat & 0x8000, 0);
        assert_eq!(world.sprites[5].x, 100);
        assert_eq!(world.render_sprite_blends.get(&5), Some(&2));
    }

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
    fn parallax_ceiling_draw_uses_loaded_def_skybox_faces() {
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            ceilingz: -8192,
            floorz: 0,
            ceilingpicnum: 42,
            ceilingstat: 1,
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
        let world =
            BuildWorld::load_arrays(sectors, walls, &[], [100, 100, -1024], 0, 100, 0).unwrap();
        let view = world.build_view(100, 100, -1024, 0, 100, 0);
        let face = || room_build::def::ImportedSkyboxFace {
            width: 1,
            height: 1,
            pixels: std::sync::Arc::from(vec![77]),
            true_color: None,
        };
        let mut tiles = SoftTiles::new();
        tiles.insert_skybox(room_build::def::ImportedSkybox {
            tile: 42,
            palette: 0,
            faces: [face(), face(), face(), face(), face(), face()],
        });
        let mut frame = SoftFrame::new(32, 24);

        fill_fp_flats_sky(&world, &view, &mut frame, Some(&tiles));

        assert_eq!(frame.pixels[1], 77);
    }

    #[test]
    fn duke_sky_sampling_selects_the_configured_angular_tile() {
        let mut tiles = SoftTiles::new();
        tiles.insert(100, 1, 1, vec![11]);
        tiles.insert(102, 1, 1, vec![22]);
        tiles.set_sky_offsets(100, [0, 2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            tiles.sample_sky_at(100, 0, 128 << 16, 0, 1, 0, 1, 0),
            Some(11)
        );
        assert_eq!(
            tiles.sample_sky_at(100, 0, 384 << 16, 0, 1, 0, 1, 0),
            Some(22)
        );
    }

    #[test]
    fn palette_specific_def_tile_falls_back_to_base_image() {
        let mut tiles = SoftTiles::new();
        tiles.insert(9000, 1, 1, vec![12]);
        tiles.insert_palette_variant(9000, 21, 1, 1, vec![34]);

        assert_eq!(tiles.sample_at_for_palette(9000, 21, 0, 0, 0), Some(34));
        assert_eq!(tiles.sample_at_for_palette(9000, 22, 0, 0, 0), Some(12));
        assert_eq!(tiles.dimensions_for_palette(9000, 21), Some((1, 1)));
    }

    #[test]
    fn def_skybox_samples_directional_faces_and_palette_zero_fallback() {
        let face = |pixel| room_build::def::ImportedSkyboxFace {
            width: 1,
            height: 1,
            pixels: std::sync::Arc::from(vec![pixel]),
            true_color: None,
        };
        let mut tiles = SoftTiles::new();
        tiles.insert_skybox(room_build::def::ImportedSkybox {
            tile: 42,
            palette: 0,
            faces: [face(10), face(20), face(30), face(40), face(50), face(60)],
        });
        let center = |angle| tiles.sample_skybox_at(42, 0, angle << 16, 1, 2, 1, 4, 1);

        // EDuke32 Polymost: front faces Build north (angle 1536), right east.
        assert_eq!(center(1536), Some(10)); // front
        assert_eq!(center(0), Some(20)); // right
        assert_eq!(center(512), Some(30)); // back
        assert_eq!(center(1024), Some(40)); // left
        assert_eq!(tiles.sample_skybox_at(42, 5, 0, 1, 2, 1, 4, 1), Some(20));
        assert_eq!(tiles.sample_skybox_at(42, 0, 0, 1, 2, 0, 4, 3), Some(50)); // top
        assert_eq!(tiles.sample_skybox_at(42, 0, 0, 1, 2, 3, 4, 0), Some(60)); // down
    }

    #[test]
    fn def_skybox_faces_follow_polymost_texture_orientation() {
        // Column-major faces: walls are 2×1 (u0, u1); top/bottom are 1×2 (v0, v1).
        let wall = |left, right| room_build::def::ImportedSkyboxFace {
            width: 2,
            height: 1,
            pixels: std::sync::Arc::from(vec![left, right]),
            true_color: None,
        };
        let flat = |top, bottom| room_build::def::ImportedSkyboxFace {
            width: 1,
            height: 2,
            pixels: std::sync::Arc::from(vec![top, bottom]),
            true_color: None,
        };
        let mut tiles = SoftTiles::new();
        tiles.insert_skybox(room_build::def::ImportedSkybox {
            tile: 42,
            palette: 0,
            faces: [
                wall(1, 2),
                wall(3, 4),
                wall(5, 6),
                wall(7, 8),
                flat(9, 10),
                flat(11, 12),
            ],
        });
        // Screen-right of center on an 8-wide view, at the horizon.
        let right_of_center =
            |angle: i32| tiles.sample_skybox_at(42, 0, angle << 16, 7, 8, 4, 8, 4);
        // Each wall's u runs along Polymost `skywal[i]` → `skywal[i + 1]`,
        // which is left → right as seen from inside the cube.
        assert_eq!(right_of_center(1536), Some(2)); // front, facing north
        assert_eq!(right_of_center(0), Some(4)); // right, facing east
        assert_eq!(right_of_center(512), Some(6)); // back, facing south
        assert_eq!(right_of_center(1024), Some(8)); // left, facing west
        // Looking steeply up: the ceiling's v grows towards world +y (south).
        let up = |angle: i32| tiles.sample_skybox_at(42, 0, angle << 16, 4, 8, 0, 64, 40);
        assert_eq!(up(1536), Some(9)); // facing north: the north (top) row
        assert_eq!(up(512), Some(10)); // facing south: the south (bottom) row
        // Looking steeply down: the floor is y-flipped, v grows towards -y.
        let down = |angle: i32| tiles.sample_skybox_at(42, 0, angle << 16, 4, 8, 63, 64, 24);
        assert_eq!(down(1536), Some(12));
        assert_eq!(down(512), Some(11));
    }

    #[test]
    fn true_color_follows_the_drawn_image_and_palette_remaps() {
        let mut tiles = SoftTiles::new();
        tiles.insert(9000, 1, 2, vec![12, 13]);
        tiles.set_true_color(9000, Some(Arc::from(vec![0xFF11_2233, 0xFF44_5566])));
        // Wrong-sized colors are refused rather than misaligned.
        tiles.set_true_color(9001, Some(Arc::from(vec![1, 2, 3])));
        let mut lookup = [0_u8; 256];
        for (index, slot) in lookup.iter_mut().enumerate() {
            *slot = index as u8;
        }
        tiles.set_palette_lookup(21, lookup);

        let base = tiles.resolve_for_palette(9000, 0, 0).unwrap();
        assert_eq!(base.at_rgb(0, 1), 0xFF44_5566);
        // A LOOKUP.DAT palette keeps the indexed tint (no true colour) ...
        assert!(
            tiles
                .resolve_for_palette(9000, 21, 0)
                .unwrap()
                .rgb
                .is_none()
        );
        // ... while a palette without a lookup draws the base image's colors.
        assert!(tiles.resolve_for_palette(9000, 7, 0).unwrap().rgb.is_some());
        // A DEF palette branch carries its own colors.
        tiles.insert_palette_variant(9000, 21, 1, 1, vec![40]);
        tiles.set_palette_true_color(9000, 21, Some(Arc::from(vec![0xFF77_8899])));
        assert_eq!(
            tiles.resolve_for_palette(9000, 21, 0).unwrap().at_rgb(0, 0),
            0xFF77_8899
        );
        // Re-inserting a tile drops stale colors.
        tiles.insert(9000, 1, 2, vec![12, 13]);
        assert!(tiles.resolve(9000, 0).unwrap().rgb.is_none());
        assert!(tiles.has_true_color());
    }

    #[test]
    fn def_tint_follows_polymost_hictinting() {
        let tint = |r, g, b, flags| HicTint { r, g, b, flags };
        // flags 0: colour multiply.
        assert_eq!(tint(255, 128, 0, 0).apply(0xFF80_FF40), 0xFF80_8000);
        // flags 1: grayscale (EDuke32 coefficient order) then multiply.
        let gray = (f64::from(0x40) * 0.3 + f64::from(0x80) * 0.59 + f64::from(0x20) * 0.11) as u32;
        assert_eq!(
            tint(255, 255, 255, 1).apply(0xFF20_8040),
            0xFF00_0000 | (gray << 16) | (gray << 8) | gray
        );
        // flags 2: invert.
        assert_eq!(tint(255, 255, 255, 2).apply(0xFF00_FF10), 0xFFFF_00EF);
        // Colorize replaces the multiply: c * t >> 6, clamped.
        assert_eq!(tint(64, 128, 32, 4).apply(0xFF10_FF40), 0xFF10_FF20);
        // Screen blend.
        assert_eq!(tint(255, 0, 128, 64).apply(0xFF00_8080), 0xFFFF_81C0);
        // Transparent texels stay transparent.
        assert_eq!(tint(1, 2, 3, 0).apply(0), 0);

        let mut tiles = SoftTiles::new();
        tiles.insert(9000, 1, 1, vec![12]);
        tiles.set_true_color(9000, Some(Arc::from(vec![0xFF80_8080])));
        let mut lookup = [0_u8; 256];
        for (index, slot) in lookup.iter_mut().enumerate() {
            *slot = index as u8;
        }
        tiles.set_palette_lookup(21, lookup);
        assert!(
            tiles
                .resolve_for_palette(9000, 21, 0)
                .unwrap()
                .rgb
                .is_none()
        );
        // A tinted palette keeps the base image's true colour and tints it.
        tiles.set_tint(21, tint(255, 0, 128, 0));
        assert_eq!(
            tiles.resolve_for_palette(9000, 21, 0).unwrap().at_rgb(0, 0),
            0xFF80_0040
        );
        assert_eq!(tiles.base_true_color_at(9000, 21, 0, 0, 0), 0xFF80_0040);
        // Palette 0 is never tinted.
        tiles.set_tint(0, tint(0, 0, 0, 0));
        assert_eq!(
            tiles.resolve_for_palette(9000, 0, 0).unwrap().at_rgb(0, 0),
            0xFF80_8080
        );
        // A palette's own image is tinted only by in-memory effects.
        tiles.insert_palette_variant(9000, 21, 1, 1, vec![40]);
        tiles.set_palette_true_color(9000, 21, Some(Arc::from(vec![0xFF77_8899])));
        assert_eq!(
            tiles.resolve_for_palette(9000, 21, 0).unwrap().at_rgb(0, 0),
            0xFF77_8899
        );
        tiles.set_tint(21, tint(255, 255, 255, 2));
        assert_eq!(
            tiles.resolve_for_palette(9000, 21, 0).unwrap().at_rgb(0, 0),
            0xFF88_7766
        );
    }

    #[test]
    fn true_color_shading_and_translucency_follow_polymost() {
        let mut tiles = SoftTiles::new();
        // Without shade tables a color passes through unchanged.
        assert_eq!(tiles.shade_true_color(0xFF80_4020, 16), 0xFF80_4020);
        tiles.set_shade_tables(32, vec![0; 32 * 256]);
        assert_eq!(tiles.shade_true_color(0xFF80_4020, 0), 0xFF80_4020);
        assert_eq!(tiles.shade_true_color(0xFF80_4020, 16), 0xFF40_2010);
        assert_eq!(tiles.shade_true_color(0xFF80_4020, 40), 0xFF00_0000);
        assert_eq!(tiles.shade_true_color(0xFF80_4020, -8), 0xFF80_4020);
        // Polymost alpha: 2/3 foreground, or 1/3 with the reverse bit.
        assert_eq!(
            SoftTiles::blend_true_color(0xFF00_0000, 0xFF96_9696, false),
            0xFF64_6464
        );
        assert_eq!(
            SoftTiles::blend_true_color(0xFF00_0000, 0xFF96_9696, true),
            0xFF32_3232
        );

        let mut palette = [0_u8; 768];
        palette[3 * 5..3 * 5 + 3].copy_from_slice(&[30, 60, 90]);
        tiles.set_palette_rgb(&palette);
        let mut frame = SoftFrame::new(2, 1);
        frame.enable_true_color();
        frame.pixels[0] = 5;
        // Opaque sprite texel: its true color replaces the pixel.
        frame.draw_sprite_pixel(Some(&tiles), 0, 10, 9, 0, 0, 0xFF96_9696);
        assert_eq!(frame.true_color.as_ref().unwrap()[0], 0xFF96_9696);
        // An indexed write clears the true color underneath it.
        frame.put(0, 0, 3);
        assert_eq!(frame.true_color.as_ref().unwrap()[0], 0);
        // Translucent texel over an indexed pixel blends with its palette RGB.
        frame.pixels[1] = 5;
        frame.draw_sprite_pixel(Some(&tiles), 1, 10, 9, 2, 0, 0xFF96_9696);
        assert_eq!(frame.true_color.as_ref().unwrap()[1], 0xFF6E_7882);
        assert_eq!(frame.pixel_color(1, &palette), [0x6E, 0x78, 0x82]);
        assert_eq!(frame.pixel_color(0, &palette), [0, 0, 0]);
    }

    #[test]
    fn translucent_sprite_uses_its_def_blend_table() {
        let mut tiles = SoftTiles::new();
        tiles.set_translucency_table(Some(vec![3; 65_536]));
        let mut custom = vec![5; 65_536];
        custom[usize::from(10_u8) * 256 + usize::from(20_u8)] = 42;
        tiles.insert_blend_table(201, custom);

        assert_eq!(tiles.composite_sprite(10, 20, 2, 201), 42);
        assert_eq!(tiles.composite_sprite(10, 20, 2, 0), 3);
        assert_eq!(tiles.composite_sprite(10, 20, 0, 201), 20);
    }

    #[test]
    fn translucent_sprite_and_wall_layers_share_depth_order() {
        let mut tiles = SoftTiles::new();
        let mut base = vec![0; 65_536];
        base[usize::from(10_u8) * 256 + usize::from(20_u8)] = 31;
        base[usize::from(30_u8) * 256 + usize::from(20_u8)] = 99;
        tiles.set_translucency_table(Some(base));
        let mut custom = vec![0; 65_536];
        custom[usize::from(31_u8) * 256 + usize::from(30_u8)] = 41;
        tiles.insert_blend_table(201, custom);

        let mut frame = SoftFrame::new(1, 1);
        frame.pixels[0] = 10;
        frame.begin_deferred_translucency();
        portal::enqueue_translucent(
            &mut frame,
            vec![TranslucentPixel {
                index: 0,
                depth: 150,
                foreground: 20,
                reverse: false,
                blend_table: 0,
                true_color: 0,
            }],
        );
        frame.draw_sprite_pixel(Some(&tiles), 0, 100, 30, CSTAT_TRANSLUCENT, 201, 0);
        frame.finish_deferred_translucency(&tiles, &[200]);

        assert_eq!(frame.pixels, [41]);
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
    fn translucent_sprites_render_back_to_front_without_depth_writes() {
        let sprites = [
            SpriteT {
                x: 200,
                y: 0,
                z: 0,
                cstat: CSTAT_TRANSLUCENT,
                picnum: 8,
                xrepeat: 255,
                yrepeat: 255,
                sectnum: 0,
                statnum: 0,
                ..SpriteT::default()
            },
            SpriteT {
                x: 350,
                y: 0,
                z: 0,
                cstat: 0,
                picnum: 7,
                xrepeat: 255,
                yrepeat: 255,
                sectnum: 0,
                statnum: 0,
                ..SpriteT::default()
            },
            SpriteT {
                x: 275,
                y: 0,
                z: 0,
                cstat: CSTAT_TRANSLUCENT,
                picnum: 9,
                xrepeat: 255,
                yrepeat: 255,
                sectnum: 0,
                statnum: 0,
                ..SpriteT::default()
            },
        ];
        let world = BuildWorld::load_arrays(
            vec![SectorT::default()],
            Vec::new(),
            &sprites,
            [50, 0, 0],
            0,
            0,
            1,
        )
        .unwrap();
        let view = BuildView {
            posx: 50,
            posy: 0,
            posz: 0,
            ang: 0,
            ang_q16: 0,
            horiz: 100,
            horiz_q16: 100 << 16,
            sect: 0,
            visible_sectors: vec![0],
        };
        let mut tiles = SoftTiles::new();
        tiles.insert(7, 1, 1, vec![10]);
        tiles.insert(8, 1, 1, vec![20]);
        tiles.insert(9, 1, 1, vec![30]);
        let mut translucency = vec![0; 65_536];
        translucency[usize::from(10_u8) * 256 + usize::from(30_u8)] = 80;
        translucency[usize::from(80_u8) * 256 + usize::from(20_u8)] = 77;
        tiles.set_translucency_table(Some(translucency));
        let mut frame = SoftFrame::new(64, 48);
        let mut depth = vec![i32::MAX; frame.pixels.len()];
        render_fp_sprites_at(&world, &view, &mut frame, Some(&tiles), &mut depth, 0);

        let pixel = frame
            .pixels
            .iter()
            .position(|&value| value == 77)
            .expect("near translucent sprite should blend over the farther opaque sprite");
        let (sin, cos) = crate::helpers::approx_sincos_q14(0);
        let (_, _, far_depth) =
            cam_project(&view, sin, cos, sprites[1].x, sprites[1].y, sprites[1].z).unwrap();
        assert_eq!(depth[pixel], far_depth as i32);
    }

    #[test]
    fn aligned_sprites_fog_with_the_render_visibility() {
        // Shade row `n` maps every colour to `n`, so a drawn pixel reads back
        // its fog shade.
        let mut shade_tables = Vec::with_capacity(64 * 256);
        for row in 0..64_u8 {
            shade_tables.extend(std::iter::repeat_n(row, 256));
        }
        let mut tiles = SoftTiles::new();
        tiles.insert(7, 8, 8, vec![10; 64]);
        tiles.set_shade_tables(64, shade_tables);
        let view = BuildView {
            posx: 0,
            posy: 0,
            posz: 0,
            ang: 0,
            ang_q16: 0,
            horiz: 100,
            horiz_q16: 100 << 16,
            sect: 0,
            visible_sectors: vec![0],
        };
        let max_shade = |cstat: u16, visibility: i32| {
            let sprite = SpriteT {
                x: 1500,
                y: 0,
                z: if cstat == 32 { 32 << 8 } else { 0 },
                cstat,
                picnum: 7,
                xrepeat: 255,
                yrepeat: 255,
                sectnum: 0,
                statnum: 0,
                ..SpriteT::default()
            };
            let mut world = BuildWorld::load_arrays(
                vec![SectorT::default()],
                Vec::new(),
                &[sprite],
                [0, 0, 0],
                0,
                0,
                1,
            )
            .unwrap();
            world.render_visibility = visibility;
            let mut frame = SoftFrame::new(64, 48);
            frame.pixels.fill(255);
            let mut depth = vec![i32::MAX; frame.pixels.len()];
            render_fp_sprites_at(&world, &view, &mut frame, Some(&tiles), &mut depth, 0);
            frame.pixels.iter().copied().filter(|&p| p != 255).max()
        };
        for cstat in [16, 32] {
            assert_eq!(max_shade(cstat, 0), Some(0), "cstat {cstat} without fog");
            let fogged = max_shade(cstat, 4096).expect("aligned sprite should draw");
            assert!(fogged > 0, "cstat {cstat} ignores render visibility");
        }
    }

    #[test]
    fn mhk_sprite_angle_offset_changes_only_the_render_copy() {
        let sprite = SpriteT {
            x: 200,
            y: 200,
            z: 0,
            cstat: CSTAT_FLOOR_ALIGN,
            picnum: 7,
            xrepeat: 255,
            yrepeat: 255,
            sectnum: 0,
            statnum: 0,
            ang: 0,
            ..SpriteT::default()
        };
        let world = BuildWorld::load_arrays(
            vec![SectorT::default()],
            Vec::new(),
            &[sprite],
            [50, 200, -1024],
            0,
            0,
            1,
        )
        .unwrap();
        let view = BuildView {
            posx: 50,
            posy: 200,
            posz: -1024,
            ang: 0,
            ang_q16: 0,
            horiz: 100,
            horiz_q16: 100 << 16,
            sect: 0,
            visible_sectors: vec![0],
        };
        let mut tiles = SoftTiles::new();
        tiles.insert(7, 2, 2, vec![10, 20, 30, 40]);
        let offsets = BTreeMap::from([(0, 512)]);
        let mut original = SoftFrame::new(64, 48);
        let mut hacked = SoftFrame::new(64, 48);
        render_fp_sprites_at(
            &world,
            &view,
            &mut original,
            Some(&tiles),
            &mut vec![i32::MAX; 64 * 48],
            0,
        );
        render_fp_sprites_at_with_angle_offsets(
            &world,
            &view,
            &mut hacked,
            Some(&tiles),
            &mut vec![i32::MAX; 64 * 48],
            0,
            Some(&offsets),
        );
        assert_ne!(original.pixels, hacked.pixels);
        assert_eq!(world.sprites[0].ang, 0);
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
    fn fp_textured_draw_resolves_direct_and_def_mapped_voxel_sprites() {
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
        let sprites = vec![SpriteT {
            x: 1000,
            y: 1400,
            z: -(32 << 8),
            picnum: 7,
            cstat: CSTAT_WALL_ALIGN | CSTAT_FLOOR_ALIGN,
            sectnum: 0,
            statnum: 0,
            xrepeat: 64,
            yrepeat: 64,
            ..SpriteT::default()
        }];
        let mut world =
            BuildWorld::load_arrays(sectors, walls, &sprites, [1000, 400, -(32 << 8)], 512, 0, 1)
                .unwrap();
        let view = world.build_view(1000, 400, -(32 << 8), 512, 100, 0);
        let mut frame = SoftFrame::new(128, 96);
        let model = Arc::new(KvxDocument {
            mipmaps: vec![room_build::kvx::KvxMipLevel {
                dimensions: [2, 1, 1],
                pivot: [0, 0, 0],
                columns: vec![
                    vec![room_build::kvx::KvxSlab {
                        z_top: 0,
                        visibility: 0x3f,
                        colors: vec![203],
                    }],
                    vec![room_build::kvx::KvxSlab {
                        z_top: 0,
                        visibility: 0x3f,
                        colors: vec![204],
                    }],
                ],
            }],
            palette: [0; 768],
        });
        let voxel_tiles = BTreeMap::from([(
            99,
            DefVoxelTile {
                model,
                model_index: 7,
                scale: 1.0,
                rotate: true,
                mip_sample_counts: Arc::from([2usize]),
                palette_map: Arc::new(std::array::from_fn(|index| index as u8)),
            },
        )]);

        render_fp_textured_at_with_def_voxels(
            &world,
            &view,
            &mut frame,
            &SoftTiles::new(),
            0,
            None,
            Some(&voxel_tiles),
        );

        assert!(
            frame.pixels.contains(&203),
            "the mapped KVX color should replace the aligned ART sprite"
        );
        assert!(
            frame.pixels.contains(&204),
            "adjacent KVX surface voxels should keep their own palette colors"
        );

        world.sprites[0].picnum = 99;
        world.sprites[0].cstat = 0;
        frame = SoftFrame::new(128, 96);
        render_fp_textured_at_with_def_voxels(
            &world,
            &view,
            &mut frame,
            &SoftTiles::new(),
            0,
            None,
            Some(&voxel_tiles),
        );
        assert!(
            frame.pixels.contains(&203) && frame.pixels.contains(&204),
            "the DEF ART tile mapping should continue to render after direct IDs are added"
        );

        let distant_model = Arc::new(KvxDocument {
            mipmaps: vec![
                room_build::kvx::KvxMipLevel {
                    dimensions: [2, 1, 1],
                    pivot: [0, 0, 0],
                    columns: vec![
                        vec![room_build::kvx::KvxSlab {
                            z_top: 0,
                            visibility: 0x3f,
                            colors: vec![203],
                        }],
                        vec![room_build::kvx::KvxSlab {
                            z_top: 0,
                            visibility: 0x3f,
                            colors: vec![204],
                        }],
                    ],
                },
                room_build::kvx::KvxMipLevel {
                    dimensions: [1, 1, 1],
                    pivot: [0, 0, 0],
                    columns: vec![vec![room_build::kvx::KvxSlab {
                        z_top: 0,
                        visibility: 0x3f,
                        colors: vec![205],
                    }]],
                },
            ],
            palette: [0; 768],
        });
        let distant_voxel = DefVoxelTile {
            model: distant_model,
            model_index: 0,
            scale: 1.0,
            rotate: false,
            mip_sample_counts: Arc::from([2usize, 1]),
            palette_map: Arc::new(std::array::from_fn(|index| index as u8)),
        };
        let mut distant_view = view.clone();
        distant_view.posy = -8_600;
        let mut distant_frame = SoftFrame::new(128, 96);
        let mut distant_depth = vec![i32::MAX; 128 * 96];
        let mut distant_budget = MAX_SOFT_VOXEL_SAMPLES;
        assert!(render_def_voxel_sprite(
            &world,
            &distant_view,
            &mut distant_frame,
            None,
            &mut distant_depth,
            &world.sprites[0],
            &distant_voxel,
            0,
            &mut distant_budget,
        ));
        assert_eq!(distant_budget, MAX_SOFT_VOXEL_SAMPLES - 1);

        let mut occluded_frame = SoftFrame::new(128, 96);
        let mut opaque_depth = vec![1; 128 * 96];
        let mut sample_budget = MAX_SOFT_VOXEL_SAMPLES;
        assert!(render_def_voxel_sprite(
            &world,
            &view,
            &mut occluded_frame,
            None,
            &mut opaque_depth,
            &world.sprites[0],
            &voxel_tiles[&99],
            0,
            &mut sample_budget,
        ));
        assert!(occluded_frame.pixels.iter().all(|&pixel| pixel == 0));
    }

    #[test]
    fn oversized_def_voxel_falls_back_without_unbounded_raster_work() {
        let sprite = SpriteT {
            x: 0,
            y: 1000,
            z: 0,
            picnum: 99,
            cstat: 0,
            sectnum: 0,
            statnum: 0,
            xrepeat: 64,
            yrepeat: 64,
            ..SpriteT::default()
        };
        let model = Arc::new(KvxDocument {
            mipmaps: vec![room_build::kvx::KvxMipLevel {
                dimensions: [1, 1, 1],
                pivot: [0, 0, 0],
                columns: vec![vec![room_build::kvx::KvxSlab {
                    z_top: 0,
                    visibility: 0x3f,
                    colors: vec![3],
                }]],
            }],
            palette: [0; 768],
        });
        let voxel = DefVoxelTile {
            model,
            model_index: 0,
            scale: 1.0,
            rotate: false,
            mip_sample_counts: Arc::from([MAX_SOFT_VOXEL_SAMPLES + 1]),
            palette_map: Arc::new(std::array::from_fn(|index| index as u8)),
        };
        let world = BuildWorld::default();
        let view = BuildView {
            posx: 0,
            posy: 0,
            posz: 0,
            ang: 0,
            ang_q16: 0,
            horiz: 100,
            horiz_q16: 100 << 16,
            sect: 0,
            visible_sectors: vec![0],
        };
        let mut frame = SoftFrame::new(16, 16);
        let mut zbuf = vec![i32::MAX; 16 * 16];
        let mut sample_budget = MAX_SOFT_VOXEL_SAMPLES;
        assert!(!render_def_voxel_sprite(
            &world,
            &view,
            &mut frame,
            None,
            &mut zbuf,
            &sprite,
            &voxel,
            0,
            &mut sample_budget,
        ));
        assert!(frame.pixels.iter().all(|&pixel| pixel == 0));
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
                cstat: 128,
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
                cstat: 128,
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
        let portal_wall = wall_color(5, 0);
        let far_wall = wall_color(31, 0);
        let mut tiles = SoftTiles::new();
        let mut blend = vec![0; 65_536];
        blend[usize::from(portal_wall)] = 77;
        tiles.set_translucency_table(Some(blend));
        render_fp_textured(&world, &view, &mut frame, &tiles);
        assert_eq!(
            frame.pixels[30 * 128 + 64],
            77,
            "upper translucent portal band blends with the cleared scene"
        );
        assert_eq!(frame.pixels[50 * 128 + 64], far_wall, "visible opening");
        assert_eq!(
            frame.pixels[66 * 128 + 64],
            77,
            "lower translucent portal band blends with the cleared scene"
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

    #[test]
    fn def_animation_range_uses_speed_and_forward_frames() {
        let mut tiles = SoftTiles::new();
        tiles.insert(10, 1, 1, vec![10]);
        tiles.insert(11, 1, 1, vec![11]);
        tiles.set_animation_range(10, 11, 1, 2);
        assert_eq!(tiles.sample_at(10, 0, 0, 0), Some(10));
        assert_eq!(tiles.sample_at(10, 0, 0, 1), Some(10));
        assert_eq!(tiles.sample_at(10, 0, 0, 2), Some(11));
    }
}

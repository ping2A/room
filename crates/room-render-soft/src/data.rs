//! CPU pictures for the software renderer: palette, `COLORMAP`, wall
//! textures as columns, flats as 64×64 tiles.
//!
//! Pictures are decoded by `room-render`'s existing public
//! [`MaterialCatalog::source_image`] (TEXTUREX composition, patches, flats,
//! PNG), never by a copy of that code (`docs/40` §6).

use room_formats::{Image, ImagePixels, parse_animated, parse_patch, parse_texturex};
use room_render::MaterialCatalog;
use room_vfs::{Namespace, Vfs};
use std::collections::HashMap;

/// Colormaps in a `COLORMAP` lump that the renderer indexes (`NUMCOLORMAPS`).
pub const NUMCOLORMAPS: usize = 32;

/// A wall texture as Doom's renderer reads it: column-major palette
/// indices, `height` bytes per column.
#[derive(Clone, Debug)]
pub struct SoftTexture {
    /// Name (upper case).
    pub name: String,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// `texturewidthmask`: largest power of two ≤ width, minus one.
    pub width_mask: u32,
    /// Column-major pixels (`width * height`); holes are 0.
    pub columns: Vec<u8>,
    /// Column-major opacity (0 = hole), same layout.
    pub alpha: Vec<u8>,
    /// Column-major 0x00RRGGBB of a true-colour source picture (PNG and
    /// other RGBA images; SR5), drawn instead of the palette indices by the
    /// true-colour drawers; empty for paletted pictures.
    pub rgb: Vec<u32>,
}

impl SoftTexture {
    /// `R_GetColumn`: the column `col` (wrapped by the width mask) and the
    /// rest of the texture after it, as vanilla's composite buffer reads past
    /// a short column into the next one.
    #[inline]
    #[must_use]
    pub fn column(&self, col: i32) -> &[u8] {
        let c = (col as u32 & self.width_mask) as usize;
        let start = (c * self.height as usize).min(self.columns.len());
        &self.columns[start..]
    }

    /// [`Self::column`] of the true-colour pixels, when the picture has them.
    #[inline]
    #[must_use]
    pub fn rgb_column(&self, col: i32) -> Option<&[u32]> {
        if self.rgb.is_empty() {
            return None;
        }
        let c = (col as u32 & self.width_mask) as usize;
        let start = (c * self.height as usize).min(self.rgb.len());
        Some(&self.rgb[start..])
    }

    /// Opacity of the same column.
    #[inline]
    #[must_use]
    pub fn column_alpha(&self, col: i32) -> &[u8] {
        let c = (col as u32 & self.width_mask) as usize;
        let start = (c * self.height as usize).min(self.alpha.len());
        &self.alpha[start..]
    }
}

/// A KVX voxel model (first mip): every voxel of its slabs with its
/// palette index (in the game palette) and colour.
#[derive(Clone, Debug, PartialEq)]
pub struct SoftVoxel {
    /// Size in voxels (x, y, z; KVX z runs down).
    pub size: [u32; 3],
    /// Pivot (voxel units).
    pub pivot: [f32; 3],
    /// Voxels: x, y, z, game palette index.
    pub voxels: Vec<[u8; 4]>,
    /// Their colours (0x00RRGGBB).
    pub rgb: Vec<u32>,
}

/// One post of a sprite column: rows `top..top + pixels.len()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftPost {
    /// First row.
    pub top: i32,
    /// Palette indices.
    pub pixels: Vec<u8>,
    /// True-colour pixels (0x00RRGGBB) of an RGBA source picture.
    pub rgb: Option<Vec<u32>>,
}

/// A sprite picture (`patch_t`): columns of posts and Doom's offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftPatch {
    /// Width (`spritewidth`).
    pub width: i32,
    /// Height.
    pub height: i32,
    /// `leftoffset` (`spriteoffset`).
    pub left: i32,
    /// `topoffset` (`spritetopoffset`).
    pub top: i32,
    /// Posts per column.
    pub columns: Vec<Vec<SoftPost>>,
}

/// One frame of a sprite (`spriteframe_t`): eight rotations or one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoftSpriteFrame {
    /// Uses the viewer's angle.
    pub rotate: bool,
    /// Patch id per rotation (index into [`SoftData::patches`]); `None` = missing.
    pub lump: [Option<u32>; 8],
    /// Drawn mirrored.
    pub flip: [bool; 8],
}

/// A texture or flat animation (`anim_t`): the frames of a range in
/// texture / flat order, switched every `speed` tics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftAnim {
    /// Wall textures (`true`) or flats.
    pub istexture: bool,
    /// Frame ids in order (`basepic` … `picnum`).
    pub pics: Vec<u16>,
    /// Position of the first frame in texture / flat order (`basepic`).
    pub base: u32,
    /// Tics per frame.
    pub speed: u32,
}

/// Vanilla `animdefs[]` (used when there is no Boom `ANIMATED` lump):
/// `(istexture, endname, startname)`, all at speed 8.
const VANILLA_ANIMS: [(bool, &str, &str); 22] = [
    (false, "NUKAGE3", "NUKAGE1"),
    (false, "FWATER4", "FWATER1"),
    (false, "SWATER4", "SWATER1"),
    (false, "LAVA4", "LAVA1"),
    (false, "BLOOD3", "BLOOD1"),
    (false, "RROCK08", "RROCK05"),
    (false, "SLIME04", "SLIME01"),
    (false, "SLIME08", "SLIME05"),
    (false, "SLIME12", "SLIME09"),
    (true, "BLODGR4", "BLODGR1"),
    (true, "SLADRIP3", "SLADRIP1"),
    (true, "BLODRIP4", "BLODRIP1"),
    (true, "FIREWALL", "FIREWALA"),
    (true, "GSTFONT3", "GSTFONT1"),
    (true, "FIRELAVA", "FIRELAV3"),
    (true, "FIREMAG3", "FIREMAG1"),
    (true, "FIREBLU2", "FIREBLU1"),
    (true, "ROCKRED3", "ROCKRED1"),
    (true, "BFALL4", "BFALL1"),
    (true, "SFALL4", "SFALL1"),
    (true, "WFALL4", "WFALL1"),
    (true, "DBRAIN4", "DBRAIN1"),
];

/// Palette, colormaps and every picture a frame needs.
pub struct SoftData {
    /// `PLAYPAL` palette 0.
    pub palette: [[u8; 3]; 256],
    /// `COLORMAP` (at least 34 × 256 bytes; identity when missing).
    pub colormaps: Vec<u8>,
    /// Loaded textures; index 0 is "no texture".
    pub textures: Vec<SoftTexture>,
    tex_index: HashMap<String, u16>,
    /// Loaded flats (64×64 row-major).
    pub flats: Vec<Vec<u8>>,
    /// True-colour 64×64 flats (0x00RRGGBB) of RGBA source pictures, by
    /// flat id; `None` for paletted flats.
    pub flat_rgb: Vec<Option<Vec<u32>>>,
    flat_index: HashMap<String, u16>,
    /// Loaded sprite pictures.
    pub patches: Vec<SoftPatch>,
    /// Loaded KVX voxel models (SR8).
    pub voxels: Vec<SoftVoxel>,
    /// `(file, override palette)` → voxel id (`None` = unreadable).
    voxel_index: HashMap<(String, bool), Option<u32>>,
    /// Sprite lump name → patch id (`None` = unreadable).
    patch_index: HashMap<String, Option<u32>>,
    /// Sprite name (4 chars) → frames, built from the `S_START` lump names.
    sprite_defs: HashMap<String, Vec<SoftSpriteFrame>>,
    /// Sprite lump names per (sprite, frame) still to load.
    sprite_lumps: HashMap<String, Vec<(usize, usize, String, bool)>>,
    /// Boom colormaps (`C_START`…`C_END`); set 0 is [`Self::colormaps`].
    extra_colormaps: Vec<Vec<u8>>,
    colormap_index: HashMap<String, u16>,
    /// Translucency tables (`tranmap[(dest << 8) + src]`): 0 = `TRANMAP`
    /// or Boom's 66 % filter, then custom 64 KiB lumps.
    pub tranmaps: Vec<Vec<u8>>,
    tranmap_index: HashMap<String, u16>,
    /// Texture and flat animations.
    pub anims: Vec<SoftAnim>,
    /// Texture id → (animation, frame) / flat id → (animation, frame).
    tex_anim: HashMap<u16, (u32, u32)>,
    flat_anim: HashMap<u16, (u32, u32)>,
    /// Nearest palette index per 5:5:5 colour (ZDoom's `RGB32k`), built on
    /// first use by the 8-bit blending styles.
    rgb555: std::sync::OnceLock<Vec<u8>>,
    /// The palette shaded at every plain light level (true colour, SR9).
    shade_lut: std::sync::OnceLock<Vec<[u32; 256]>>,
    catalog: MaterialCatalog,
}

impl SoftData {
    /// Read `PLAYPAL`, `COLORMAP` and TEXTUREX from `vfs`.
    #[must_use]
    pub fn load(vfs: &Vfs) -> Self {
        let mut palette = [[0u8; 3]; 256];
        for (i, p) in palette.iter_mut().enumerate() {
            let v = i as u8;
            *p = [v, v, v];
        }
        if let Some(bytes) = lump(vfs, "PLAYPAL") {
            for (i, p) in palette.iter_mut().enumerate() {
                if let Some(rgb) = bytes.get(i * 3..i * 3 + 3) {
                    *p = [rgb[0], rgb[1], rgb[2]];
                }
            }
        }
        let mut colormaps = lump(vfs, "COLORMAP").unwrap_or_default();
        if colormaps.len() < 34 * 256 {
            // No COLORMAP: identity rows (still lets the renderer run).
            colormaps = (0..34 * 256).map(|i| (i % 256) as u8).collect();
        }
        let mut catalog = MaterialCatalog::new();
        catalog.load_playpal(vfs);
        catalog.load_texturex(vfs);
        Self {
            palette,
            colormaps,
            textures: vec![SoftTexture {
                name: "-".into(),
                width: 1,
                height: 1,
                width_mask: 0,
                columns: vec![0],
                alpha: vec![0],
                rgb: Vec::new(),
            }],
            tex_index: HashMap::new(),
            flats: Vec::new(),
            flat_rgb: Vec::new(),
            flat_index: HashMap::new(),
            patches: Vec::new(),
            voxels: Vec::new(),
            voxel_index: HashMap::new(),
            patch_index: HashMap::new(),
            sprite_defs: HashMap::new(),
            sprite_lumps: sprite_lump_table(vfs),
            extra_colormaps: Vec::new(),
            colormap_index: HashMap::new(),
            tranmaps: Vec::new(),
            tranmap_index: HashMap::new(),
            anims: Vec::new(),
            tex_anim: HashMap::new(),
            flat_anim: HashMap::new(),
            rgb555: std::sync::OnceLock::new(),
            shade_lut: std::sync::OnceLock::new(),
            catalog,
        }
        .with_anims(vfs)
    }

    /// `P_InitPicAnims`: Boom `ANIMATED` when present, else vanilla's list.
    /// Every frame is loaded now so the renderer only reads.
    fn with_anims(mut self, vfs: &Vfs) -> Self {
        let defs: Vec<(bool, String, String, u32)> =
            match lump(vfs, "ANIMATED").and_then(|b| parse_animated(&b).ok()) {
                Some(recs) => recs
                    .into_iter()
                    .filter(|r| r.kind >= 0)
                    .map(|r| (r.kind != 0, r.last, r.first, r.speed.max(1) as u32))
                    .collect(),
                None => VANILLA_ANIMS
                    .iter()
                    .map(|&(t, e, b)| (t, e.to_owned(), b.to_owned(), 8))
                    .collect(),
            };
        if defs.is_empty() {
            return self;
        }
        let texture_order = texture_order(vfs);
        let flat_order = flat_order(vfs);
        for (istexture, end, start, speed) in defs {
            let order = if istexture {
                &texture_order
            } else {
                &flat_order
            };
            // Textures: the first of a name (`R_CheckTextureNumForName`);
            // flats: the last lump (`W_CheckNumForName`).
            let find = |n: &str| {
                let hit = |o: &String| o.eq_ignore_ascii_case(n.trim());
                if istexture {
                    order.iter().position(hit)
                } else {
                    order.iter().rposition(hit)
                }
            };
            // Vanilla skips an animation whose first picture is missing.
            let (Some(first), Some(last)) = (find(&start), find(&end)) else {
                continue;
            };
            if last <= first {
                continue;
            }
            let names: Vec<String> = order[first..=last].to_vec();
            let pics: Vec<u16> = names
                .iter()
                .map(|n| {
                    if istexture {
                        self.texture(vfs, n)
                    } else {
                        self.flat(vfs, n)
                    }
                })
                .collect();
            let a = self.anims.len() as u32;
            for (k, &pic) in pics.iter().enumerate() {
                let map = if istexture {
                    &mut self.tex_anim
                } else {
                    &mut self.flat_anim
                };
                if pic != 0 || !istexture {
                    map.insert(pic, (a, k as u32));
                }
            }
            self.anims.push(SoftAnim {
                istexture,
                pics,
                base: first as u32,
                speed,
            });
        }
        self
    }

    fn anim_frame(&self, slot: Option<&(u32, u32)>, id: u16, leveltime: u32) -> u16 {
        let Some(&(a, k)) = slot else {
            return id;
        };
        let anim = &self.anims[a as usize];
        let n = anim.pics.len() as u32;
        // `pic = basepic + ((leveltime / speed + i) % numpics)` with `i` the
        // absolute picture number.
        let i = anim.base + k;
        anim.pics[((leveltime / anim.speed + i) % n) as usize]
    }

    /// The palette shaded at each plain light level 0..=256 (true colour).
    pub(crate) fn shade_lut(&self) -> &[[u32; 256]] {
        self.shade_lut
            .get_or_init(|| crate::render::shade_lut(&self.palette))
    }

    /// The palette index nearest an RGB colour (through a 32 K table).
    #[inline]
    #[must_use]
    pub fn nearest_rgb(&self, [r, g, b]: [u32; 3]) -> u8 {
        let lut = self.rgb555.get_or_init(|| {
            (0..32768u32)
                .map(|i| {
                    let c = |v: u32| ((v & 31) * 255 / 31) as u8;
                    nearest(&self.palette, c(i >> 10), c(i >> 5), c(i))
                })
                .collect()
        });
        let q = |v: u32| v.min(255) >> 3;
        lut[((q(r) << 10) | (q(g) << 5) | q(b)) as usize]
    }

    /// `texturetranslation[id]` at `leveltime`.
    #[inline]
    #[must_use]
    pub fn anim_texture(&self, id: u16, leveltime: u32) -> u16 {
        if self.tex_anim.is_empty() {
            return id;
        }
        self.anim_frame(self.tex_anim.get(&id), id, leveltime)
    }

    /// `flattranslation[id]` at `leveltime`.
    #[inline]
    #[must_use]
    pub fn anim_flat(&self, id: u16, leveltime: u32) -> u16 {
        if self.flat_anim.is_empty() {
            return id;
        }
        self.anim_frame(self.flat_anim.get(&id), id, leveltime)
    }

    /// `R_ColormapNumForName`: `COLORMAP` is set 0, a `C_START` lump a later
    /// set (loaded on first use); `None` when `name` is no colormap.
    pub fn colormap_number(&mut self, vfs: &Vfs, name: &str) -> Option<u16> {
        let key = name.trim().to_ascii_uppercase();
        if key == "COLORMAP" {
            return Some(0);
        }
        if let Some(&n) = self.colormap_index.get(&key) {
            return Some(n);
        }
        let bytes = vfs
            .find(&key, Namespace::Colormaps)
            .and_then(|id| vfs.read(id).ok())?;
        if bytes.len() < 33 * 256 {
            return None;
        }
        let mut set = bytes[..bytes.len().min(34 * 256)].to_vec();
        // Short maps (32 or 33 rows) borrow the main map's last rows.
        while set.len() < 34 * 256 {
            let row = set.len() / 256;
            set.extend_from_slice(&self.colormaps[row * 256..row * 256 + 256]);
        }
        self.extra_colormaps.push(set);
        let n = self.extra_colormaps.len() as u16;
        self.colormap_index.insert(key, n);
        Some(n)
    }

    /// Row `level` of colormap set `set` (0 = `COLORMAP`).
    #[inline]
    #[must_use]
    pub fn colormap_in(&self, set: u16, level: usize) -> &[u8] {
        match set
            .checked_sub(1)
            .and_then(|i| self.extra_colormaps.get(usize::from(i)))
        {
            Some(maps) => {
                let l = level.min(maps.len() / 256 - 1);
                &maps[l * 256..l * 256 + 256]
            }
            None => self.colormap(level),
        }
    }

    /// Make sure translucency table 0 exists: the `TRANMAP` lump, else
    /// Boom's `R_InitTranMap` with `tran_filter_pct` 66.
    pub fn ensure_tranmap(&mut self, vfs: &Vfs) {
        if !self.tranmaps.is_empty() {
            return;
        }
        let map = lump(vfs, "TRANMAP")
            .filter(|b| b.len() >= 65536)
            .map(|b| b[..65536].to_vec())
            .unwrap_or_else(|| boom_tranmap(&self.palette, 66));
        self.tranmaps.push(map);
    }

    /// A 64 KiB lump named `name` as a translucency table (Boom line 260's
    /// middle texture); `None` when no such lump.
    pub fn tranmap_lump(&mut self, vfs: &Vfs, name: &str) -> Option<u16> {
        self.ensure_tranmap(vfs);
        let key = name.trim().to_ascii_uppercase();
        if key == "TRANMAP" {
            return Some(0);
        }
        if let Some(&n) = self.tranmap_index.get(&key) {
            return Some(n);
        }
        let bytes = vfs
            .find(&key, Namespace::Global)
            .and_then(|id| vfs.read(id).ok())
            .filter(|b| b.len() == 65536)?;
        self.tranmaps.push(bytes.to_vec());
        let n = (self.tranmaps.len() - 1) as u16;
        self.tranmap_index.insert(key, n);
        Some(n)
    }

    /// `R_InstallSpriteLump` for one sprite, loading its pictures (first
    /// use only). Returns the frames, `None` for a sprite with no lumps.
    pub fn sprite(&mut self, vfs: &Vfs, name: &str) -> Option<&[SoftSpriteFrame]> {
        let key = name.to_ascii_uppercase();
        if !self.sprite_defs.contains_key(&key) {
            let lumps = self.sprite_lumps.remove(&key)?;
            let mut frames: Vec<SoftSpriteFrame> = Vec::new();
            for (frame, rotation, lump, flipped) in lumps {
                if frames.len() <= frame {
                    frames.resize(frame + 1, SoftSpriteFrame::default());
                }
                let patch = self.patch(vfs, &lump);
                let f = &mut frames[frame];
                if rotation == 0 {
                    f.rotate = false;
                    f.lump = [patch; 8];
                    f.flip = [flipped; 8];
                } else {
                    f.rotate = true;
                    f.lump[rotation - 1] = patch;
                    f.flip[rotation - 1] = flipped;
                }
            }
            self.sprite_defs.insert(key.clone(), frames);
        }
        self.sprite_defs.get(&key).map(Vec::as_slice)
    }

    /// Frames of a sprite already loaded by [`Self::sprite`].
    #[must_use]
    pub fn sprite_frames(&self, name: &str) -> Option<&[SoftSpriteFrame]> {
        // Sprite names in frames are upper case already: no allocation.
        if !name.bytes().any(|b| b.is_ascii_lowercase()) {
            return self.sprite_defs.get(name).map(Vec::as_slice);
        }
        self.sprite_defs
            .get(&name.to_ascii_uppercase())
            .map(Vec::as_slice)
    }

    /// Load a KVX voxel (VOXELDEF file name: `csawa`, `voxels/x.kvx`; first
    /// use only). `override_palette` reads its indices as the game palette
    /// (`OverridePalette`); otherwise its own palette's colours are mapped
    /// to the nearest game colours.
    pub fn voxel(&mut self, vfs: &Vfs, file: &str, override_palette: bool) -> Option<u32> {
        let key = (file.trim().to_ascii_lowercase(), override_palette);
        if let Some(id) = self.voxel_index.get(&key) {
            return *id;
        }
        let id = read_voxel_file(vfs, file)
            .and_then(|bytes| room_render::parse_kvx(&bytes).ok())
            .and_then(|kvx| {
                let h = kvx.header.as_ref()?;
                let six = |c: u8| {
                    let c = c & 63;
                    u32::from((c << 2) | (c >> 4))
                };
                let mut voxels = Vec::with_capacity(kvx.voxels.len());
                let mut rgb = Vec::with_capacity(kvx.voxels.len());
                for v in &kvx.voxels {
                    let (index, colour) = match (&kvx.palette, override_palette) {
                        (Some(pal), false) => {
                            let i = usize::from(v.color) * 3;
                            let c = [six(pal[i]), six(pal[i + 1]), six(pal[i + 2])];
                            (self.nearest_rgb(c), (c[0] << 16) | (c[1] << 8) | c[2])
                        }
                        _ => {
                            let [r, g, b] = self.palette[usize::from(v.color)];
                            (
                                v.color,
                                (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
                            )
                        }
                    };
                    voxels.push([v.x, v.y, v.z, index]);
                    rgb.push(colour);
                }
                self.voxels.push(SoftVoxel {
                    size: [h.x_size, h.y_size, h.z_size],
                    pivot: h.pivot,
                    voxels,
                    rgb,
                });
                Some((self.voxels.len() - 1) as u32)
            });
        self.voxel_index.insert(key, id);
        id
    }

    /// Load a picture by lump name (decals; first use only).
    pub fn picture(&mut self, vfs: &Vfs, name: &str) -> Option<u32> {
        self.patch(vfs, &name.trim().to_ascii_uppercase())
    }

    /// A picture loaded by [`Self::picture`].
    #[must_use]
    pub fn picture_id(&self, name: &str) -> Option<u32> {
        self.patch_index
            .get(&name.trim().to_ascii_uppercase())
            .copied()
            .flatten()
    }

    fn patch(&mut self, vfs: &Vfs, lump: &str) -> Option<u32> {
        if let Some(id) = self.patch_index.get(lump) {
            return *id;
        }
        let pic = vfs
            .find(lump, Namespace::Sprites)
            .or_else(|| vfs.find_any_ns(lump))
            .and_then(|id| vfs.read(id).ok())
            .and_then(|bytes| load_patch(&bytes, &self.palette));
        let id = pic.map(|p| {
            self.patches.push(p);
            (self.patches.len() - 1) as u32
        });
        self.patch_index.insert(lump.to_owned(), id);
        id
    }

    /// Texture id for `name` (`0` = none / unknown), loading it on first use.
    pub fn texture(&mut self, vfs: &Vfs, name: &str) -> u16 {
        let key = name.trim().to_ascii_uppercase();
        if key.is_empty() || key == "-" {
            return 0;
        }
        if let Some(id) = self.tex_index.get(&key) {
            return *id;
        }
        let img = self.catalog.source_image(vfs, &key);
        let rgb = img.as_ref().map(rgb_columns).unwrap_or_default();
        let id = img.and_then(|img| to_columns(&img, &self.palette)).map_or(
            0,
            |(w, h, columns, alpha)| {
                let mut mask = 1u32;
                while mask * 2 <= w {
                    mask *= 2;
                }
                self.textures.push(SoftTexture {
                    name: key.clone(),
                    width: w,
                    height: h,
                    width_mask: mask - 1,
                    columns,
                    alpha,
                    rgb,
                });
                (self.textures.len() - 1) as u16
            },
        );
        self.tex_index.insert(key, id);
        id
    }

    /// A camera texture canvas (`ANIMDEFS cameratexture`, SR7): a blank
    /// `width` × `height` texture under `name` that [`Self::set_canvas`]
    /// paints. Returns its id.
    pub fn canvas(&mut self, name: &str, width: u32, height: u32) -> u16 {
        let key = name.trim().to_ascii_uppercase();
        let (w, h) = (width.max(1), height.max(1));
        if let Some(&id) = self.tex_index.get(&key) {
            if let Some(t) = self.textures.get(usize::from(id)).filter(|_| id != 0) {
                if t.width == w && t.height == h {
                    return id;
                }
            }
        }
        let mut mask = 1u32;
        while mask * 2 <= w {
            mask *= 2;
        }
        let n = (w * h) as usize;
        self.textures.push(SoftTexture {
            name: key.clone(),
            width: w,
            height: h,
            width_mask: mask - 1,
            columns: vec![0; n],
            alpha: vec![255; n],
            rgb: Vec::new(),
        });
        let id = (self.textures.len() - 1) as u16;
        self.tex_index.insert(key, id);
        id
    }

    /// Paint canvas `id` with `frame` scaled to the canvas (nearest pixel;
    /// its true colour too when it has one).
    pub fn set_canvas(&mut self, id: u16, frame: &crate::SoftTarget) {
        let Some(t) = self.textures.get_mut(usize::from(id)) else {
            return;
        };
        let (w, h) = (t.width as usize, t.height as usize);
        let (fw, fh) = (frame.width as usize, frame.height as usize);
        if fw == 0 || fh == 0 {
            return;
        }
        let tc = frame.rgb.len() == frame.pixels.len();
        if tc {
            t.rgb.resize(w * h, 0);
        } else {
            t.rgb.clear();
        }
        for c in 0..w {
            let sx = c * fw / w;
            for r in 0..h {
                let s = (r * fh / h) * fw + sx;
                t.columns[c * h + r] = frame.pixels[s];
                if tc {
                    t.rgb[c * h + r] = frame.rgb[s];
                }
            }
        }
    }

    /// Flat id for `name`, loading it on first use (unknown flats become a
    /// grey tile so planes still draw).
    pub fn flat(&mut self, vfs: &Vfs, name: &str) -> u16 {
        let key = name.trim().to_ascii_uppercase();
        if let Some(id) = self.flat_index.get(&key) {
            return *id;
        }
        let raw = vfs
            .find(&key, Namespace::Flats)
            .and_then(|id| vfs.read(id).ok())
            .filter(|b| b.len() == 4096 || (b.len() >= 4096 && !is_image(b)))
            .map(|b| b[..4096].to_vec());
        let mut rgb = None;
        let pixels = raw
            .or_else(|| {
                let img = self.catalog.source_image(vfs, &key)?;
                rgb = flat_rgb(&img);
                to_flat(&img, &self.palette)
            })
            .unwrap_or_else(|| vec![96; 4096]);
        self.flats.push(pixels);
        self.flat_rgb.push(rgb);
        let id = (self.flats.len() - 1) as u16;
        self.flat_index.insert(key, id);
        id
    }

    /// Colormap row `level` (0 = full bright … 31 = dark, 32 = invulnerability).
    #[inline]
    #[must_use]
    pub fn colormap(&self, level: usize) -> &[u8] {
        let l = level.min(self.colormaps.len() / 256 - 1);
        &self.colormaps[l * 256..l * 256 + 256]
    }
}

impl std::fmt::Debug for SoftData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoftData")
            .field("textures", &self.textures.len())
            .field("flats", &self.flats.len())
            .finish_non_exhaustive()
    }
}

/// Sprite lumps by sprite name: `(frame, rotation, lump, flipped)` in
/// `R_InitSpriteDefs` order; the newest lump of each name wins.
fn sprite_lump_table(vfs: &Vfs) -> HashMap<String, Vec<(usize, usize, String, bool)>> {
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (_, info) in vfs.iter() {
        if info.namespace != Namespace::Sprites {
            continue;
        }
        let n = info.short_name.as_str().to_ascii_uppercase();
        if n.len() >= 6 && seen.insert(n.clone()) {
            names.push(n);
        }
    }
    let mut out: HashMap<String, Vec<(usize, usize, String, bool)>> = HashMap::new();
    for n in names {
        let b = n.as_bytes();
        let entry = out.entry(n[..4].to_owned()).or_default();
        let fr = |c: u8| usize::from(c.wrapping_sub(b'A'));
        let rot = |c: u8| usize::from(c.wrapping_sub(b'0'));
        if fr(b[4]) < 29 && rot(b[5]) <= 8 {
            entry.push((fr(b[4]), rot(b[5]), n.clone(), false));
        }
        if b.len() >= 8 && fr(b[6]) < 29 && rot(b[7]) <= 8 {
            entry.push((fr(b[6]), rot(b[7]), n.clone(), true));
        }
    }
    out
}

/// A Doom picture's posts, or an image's opaque runs as posts.
fn load_patch(bytes: &[u8], palette: &[[u8; 3]; 256]) -> Option<SoftPatch> {
    if let Ok(p) = parse_patch(bytes) {
        return Some(SoftPatch {
            width: i32::from(p.width),
            height: i32::from(p.height),
            left: i32::from(p.left_offset),
            top: i32::from(p.top_offset),
            columns: p
                .columns
                .iter()
                .map(|c| {
                    c.posts
                        .iter()
                        .map(|post| SoftPost {
                            top: i32::from(post.top),
                            pixels: post.pixels.clone(),
                            rgb: None,
                        })
                        .collect()
                })
                .collect(),
        });
    }
    let img = room_formats::decode_image(bytes).ok()?;
    let (w, h, rows, alpha) = indexed(&img, palette);
    let (wu, hu) = (w as usize, h as usize);
    let rgb_rows: Option<Vec<u32>> = match &img.pixels {
        ImagePixels::Rgba8(p) => Some(
            p.pixels
                .chunks_exact(4)
                .map(|c| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
                .collect(),
        ),
        ImagePixels::Paletted(_) => None,
    };
    let columns = (0..wu)
        .map(|x| {
            let mut posts = Vec::new();
            let mut y = 0;
            while y < hu {
                if alpha[y * wu + x] < 128 {
                    y += 1;
                    continue;
                }
                let top = y;
                let mut pixels = Vec::new();
                let mut rgb = rgb_rows.as_ref().map(|_| Vec::new());
                while y < hu && alpha[y * wu + x] >= 128 {
                    pixels.push(rows[y * wu + x]);
                    if let (Some(out), Some(src)) = (rgb.as_mut(), rgb_rows.as_ref()) {
                        out.push(src[y * wu + x]);
                    }
                    y += 1;
                }
                posts.push(SoftPost {
                    top: top as i32,
                    pixels,
                    rgb,
                });
            }
            posts
        })
        .collect();
    Some(SoftPatch {
        width: w as i32,
        height: h as i32,
        left: img.left_offset,
        top: img.top_offset,
        columns,
    })
}

/// Wall texture names in `TEXTURE1`, then `TEXTURE2`, order (texture numbers).
fn texture_order(vfs: &Vfs) -> Vec<String> {
    let mut out = vec!["-".to_owned()];
    for name in ["TEXTURE1", "TEXTURE2"] {
        if let Some(defs) = lump(vfs, name).and_then(|b| parse_texturex(&b).ok()) {
            out.extend(defs.into_iter().map(|d| d.name.to_ascii_uppercase()));
        }
    }
    out
}

/// Flat names in lump order (`firstflat` …), all `F_START` blocks merged.
fn flat_order(vfs: &Vfs) -> Vec<String> {
    vfs.iter()
        .filter(|(_, info)| info.namespace == Namespace::Flats)
        .map(|(_, info)| info.short_name.as_str().to_ascii_uppercase())
        .collect()
}

/// Boom `R_InitTranMap`: for every (dest, src) the palette entry nearest
/// `pct` % of src plus the rest of dest; ties go to the higher index.
#[must_use]
pub fn boom_tranmap(palette: &[[u8; 3]; 256], pct: i64) -> Vec<u8> {
    const TSC: i64 = 12;
    let w1 = (pct << TSC) / 100;
    let w2 = (1 << TSC) - w1;
    let pal: Vec<[i64; 3]> = palette
        .iter()
        .map(|c| [i64::from(c[0]), i64::from(c[1]), i64::from(c[2])])
        .collect();
    let tot: Vec<i64> = pal
        .iter()
        .map(|c| (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) << (TSC - 1))
        .collect();
    let mut out = vec![0u8; 65536];
    for i in 0..256 {
        let (r1, g1, b1) = (pal[i][0] * w2, pal[i][1] * w2, pal[i][2] * w2);
        for j in 0..256 {
            let r = pal[j][0] * w1 + r1;
            let g = pal[j][1] * w1 + g1;
            let b = pal[j][2] * w1 + b1;
            let mut best = i64::MAX;
            let mut pick = 0u8;
            for color in (0..256).rev() {
                let err = tot[color] - pal[color][0] * r - pal[color][1] * g - pal[color][2] * b;
                if err < best {
                    best = err;
                    pick = color as u8;
                }
            }
            out[i * 256 + j] = pick;
        }
    }
    out
}

fn lump(vfs: &Vfs, name: &str) -> Option<Vec<u8>> {
    let id = vfs
        .find(name, Namespace::Global)
        .or_else(|| vfs.find_any_ns(name))?;
    vfs.read(id).ok().map(|b| b.to_vec())
}

/// Nearest palette index for an RGB colour (true-colour pictures).
fn nearest(palette: &[[u8; 3]; 256], r: u8, g: u8, b: u8) -> u8 {
    let mut best = (u32::MAX, 0u8);
    for (i, p) in palette.iter().enumerate() {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).unsigned_abs();
        let dist = d(p[0], r).pow(2) + d(p[1], g).pow(2) + d(p[2], b).pow(2);
        if dist < best.0 {
            best = (dist, i as u8);
        }
    }
    best.1
}

/// Row-major indices + opacity of any picture.
fn indexed(img: &Image, palette: &[[u8; 3]; 256]) -> (u32, u32, Vec<u8>, Vec<u8>) {
    match &img.pixels {
        ImagePixels::Paletted(p) => {
            let alpha = if p.alpha.len() == p.indices.len() {
                p.alpha.clone()
            } else {
                vec![255; p.indices.len()]
            };
            (p.width, p.height, p.indices.clone(), alpha)
        }
        ImagePixels::Rgba8(p) => {
            let mut cache: HashMap<[u8; 3], u8> = HashMap::new();
            let mut idx = Vec::with_capacity(p.pixels.len() / 4);
            let mut alpha = Vec::with_capacity(p.pixels.len() / 4);
            for px in p.pixels.chunks_exact(4) {
                let rgb = [px[0], px[1], px[2]];
                let i = *cache
                    .entry(rgb)
                    .or_insert_with(|| nearest(palette, rgb[0], rgb[1], rgb[2]));
                idx.push(i);
                alpha.push(px[3]);
            }
            (p.width, p.height, idx, alpha)
        }
    }
}

type Columns = (u32, u32, Vec<u8>, Vec<u8>);

fn to_columns(img: &Image, palette: &[[u8; 3]; 256]) -> Option<Columns> {
    let (w, h, rows, alpha) = indexed(img, palette);
    if w == 0 || h == 0 {
        return None;
    }
    let (wu, hu) = (w as usize, h as usize);
    let mut cols = vec![0u8; wu * hu];
    let mut calpha = vec![0u8; wu * hu];
    for y in 0..hu {
        for x in 0..wu {
            let a = alpha[y * wu + x];
            if a != 0 {
                cols[x * hu + y] = rows[y * wu + x];
                calpha[x * hu + y] = 255;
            }
        }
    }
    Some((w, h, cols, calpha))
}

/// Whether `bytes` start like a PNG / JPEG image rather than a raw flat.
fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG") || bytes.starts_with(&[0xff, 0xd8, 0xff])
}

/// Column-major 0x00RRGGBB of an RGBA picture (empty for paletted ones).
fn rgb_columns(img: &Image) -> Vec<u32> {
    let ImagePixels::Rgba8(p) = &img.pixels else {
        return Vec::new();
    };
    let (wu, hu) = (p.width as usize, p.height as usize);
    let mut out = vec![0u32; wu * hu];
    for (i, c) in p.pixels.chunks_exact(4).enumerate() {
        let (x, y) = (i % wu, i / wu);
        out[x * hu + y] = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
    }
    out
}

/// A 64×64 true-colour tile of an RGBA picture, sampled like [`to_flat`].
fn flat_rgb(img: &Image) -> Option<Vec<u32>> {
    let ImagePixels::Rgba8(p) = &img.pixels else {
        return None;
    };
    let (w, h) = (p.width as usize, p.height as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let mut out = vec![0u32; 4096];
    for y in 0..64usize {
        for x in 0..64usize {
            let i = ((y * h / 64) * w + x * w / 64) * 4;
            let c = &p.pixels[i..i + 4];
            out[y * 64 + x] = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        }
    }
    Some(out)
}

fn to_flat(img: &Image, palette: &[[u8; 3]; 256]) -> Option<Vec<u8>> {
    let (w, h, rows, _) = indexed(img, palette);
    if w == 0 || h == 0 {
        return None;
    }
    // Sample a 64×64 tile (vanilla flats are always 64×64).
    let mut out = vec![0u8; 4096];
    for y in 0..64usize {
        for x in 0..64usize {
            let sx = x * w as usize / 64;
            let sy = y * h as usize / 64;
            out[y * 64 + x] = rows[sy * w as usize + sx];
        }
    }
    Some(out)
}

/// A VOXELDEF file's bytes: the path as written, with `.kvx`, under
/// `voxels/`, or the lump in the voxels namespace.
fn read_voxel_file(vfs: &Vfs, file: &str) -> Option<Vec<u8>> {
    let norm = file.trim().replace('\\', "/");
    let stem = norm.rsplit('/').next().unwrap_or(norm.as_str());
    let stem = stem
        .strip_suffix(".kvx")
        .or_else(|| stem.strip_suffix(".KVX"))
        .unwrap_or(stem);
    for path in [
        norm.clone(),
        format!("{norm}.kvx"),
        format!("voxels/{stem}.kvx"),
        format!("voxels/{stem}"),
    ] {
        if let Some(id) = vfs.find_full(&path) {
            return vfs.read(id).ok().map(|b| b.to_vec());
        }
    }
    vfs.find(stem, Namespace::Voxels)
        .or_else(|| vfs.find_any_ns(stem))
        .and_then(|id| vfs.read(id).ok())
        .map(|b| b.to_vec())
}

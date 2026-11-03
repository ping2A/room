//! Cached Doom picture lumps for HUD / UI blitting.

use room_formats::{Patch, parse_patch, parse_playpal};
use room_vfs::{Namespace, Vfs};
use std::collections::BTreeMap;

/// Decoded patch ready for CPU blit.
///
/// Transparency comes from the column mask (`alpha`), **not** palette index 0 —
/// Doom posts may paint real colour 0 (dark metal on weapons).
#[derive(Clone, Debug)]
pub struct SoftPatch {
    /// Width in texels.
    pub width: u16,
    /// Height in texels.
    pub height: u16,
    /// Doom left offset (screen placement).
    pub left_offset: i16,
    /// Doom top offset (screen placement).
    pub top_offset: i16,
    /// Row-major palette indices (valid where `alpha != 0`).
    pub indices: Vec<u8>,
    /// Row-major opacity (`0` = hole, `255` = solid).
    pub alpha: Vec<u8>,
    /// Truecolor pixels (`width * height * 4`) when the lump is PNG/JPEG/….
    /// Palette `indices` are unused while this is set.
    pub rgba: Option<Vec<u8>>,
}

impl SoftPatch {
    fn from_patch(p: Patch) -> Self {
        let (indices, alpha) = p.to_indices_masked();
        Self {
            width: p.width,
            height: p.height,
            left_offset: p.left_offset,
            top_offset: p.top_offset,
            indices,
            alpha,
            rgba: None,
        }
    }

    /// Truecolor or paletted image (PNG `grAb` offsets included).
    fn from_image(img: room_formats::Image) -> Option<Self> {
        let w = u16::try_from(img.width()).ok()?;
        let h = u16::try_from(img.height()).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let n = usize::from(w) * usize::from(h);
        let clamp_off =
            |v: i32| -> i16 { i16::try_from(v).unwrap_or(if v < 0 { i16::MIN } else { i16::MAX }) };
        let left_offset = clamp_off(img.left_offset);
        let top_offset = clamp_off(img.top_offset);
        match img.pixels {
            room_formats::ImagePixels::Rgba8(rgba) => {
                if rgba.pixels.len() < n * 4 {
                    return None;
                }
                let alpha: Vec<u8> = rgba.pixels.chunks_exact(4).take(n).map(|p| p[3]).collect();
                Some(Self {
                    width: w,
                    height: h,
                    left_offset,
                    top_offset,
                    indices: vec![0; n],
                    alpha,
                    rgba: Some(rgba.pixels),
                })
            }
            room_formats::ImagePixels::Paletted(p) => {
                if p.indices.len() < n {
                    return None;
                }
                let alpha = if p.alpha.len() >= n {
                    p.alpha[..n].to_vec()
                } else {
                    p.indices[..n]
                        .iter()
                        .map(|&i| if i == 0 { 0 } else { 255 })
                        .collect()
                };
                Some(Self {
                    width: w,
                    height: h,
                    left_offset,
                    top_offset,
                    indices: p.indices[..n].to_vec(),
                    alpha,
                    rgba: None,
                })
            }
        }
    }

    /// Nearest-neighbour RGBA8 raster at `dest_w`×`dest_h` (holes `a=0`).
    ///
    /// Prefer this over [`Self::paint`] for HUD weapons — one image blit beats
    /// thousands of per-span fill rects (plasma / BFG patches are especially hot).
    #[must_use]
    pub fn scale_rgba(&self, dest_w: u32, dest_h: u32, palette: &[[u8; 3]; 256]) -> Vec<u8> {
        let sw = usize::from(self.width);
        let sh = usize::from(self.height);
        let dw = dest_w as usize;
        let dh = dest_h as usize;
        let mut out = vec![0u8; dw.saturating_mul(dh).saturating_mul(4)];
        if let Some(src) = &self.rgba {
            if sw == 0 || sh == 0 || dw == 0 || dh == 0 || src.len() < sw * sh * 4 {
                return out;
            }
            for dy in 0..dh {
                let sy = dy * sh / dh;
                for dx in 0..dw {
                    let sx = dx * sw / dw;
                    let si = (sy * sw + sx) * 4;
                    if src[si + 3] == 0 {
                        continue;
                    }
                    let o = (dy * dw + dx) * 4;
                    out[o..o + 4].copy_from_slice(&src[si..si + 4]);
                }
            }
            return out;
        }
        if sw == 0 || sh == 0 || dw == 0 || dh == 0 || self.indices.len() < sw * sh {
            return out;
        }
        for dy in 0..dh {
            let sy = dy * sh / dh;
            for dx in 0..dw {
                let sx = dx * sw / dw;
                let si = sy * sw + sx;
                if self.alpha.get(si).copied().unwrap_or(0) == 0 {
                    continue;
                }
                let idx = usize::from(self.indices[si]);
                let [r, g, b] = palette[idx];
                let o = (dy * dw + dx) * 4;
                out[o] = r;
                out[o + 1] = g;
                out[o + 2] = b;
                out[o + 3] = 255;
            }
        }
        out
    }

    /// Paint opaque runs via `emit(x, y, w, h, rgba)`.
    ///
    /// Destination bounds use floor/ceil so fractional HUD scales (e.g. 1280×720
    /// letterbox) do not leave 1-pixel gaps between texels — important for weapon
    /// overlays. Uses a uniform scale so the gun is not stretched by letterboxing.
    pub fn paint(
        &self,
        origin_x: f32,
        origin_y: f32,
        scale_x: f32,
        scale_y: f32,
        palette: &[[u8; 3]; 256],
        mut emit: impl FnMut(f32, f32, f32, f32, [f32; 4]),
    ) {
        let w = usize::from(self.width);
        let h = usize::from(self.height);
        if w == 0 || h == 0 || self.indices.len() < w * h || self.alpha.len() < w * h {
            return;
        }
        // Near-uniform (weapon / font) keeps Doom aspect under letterbox.
        // Distinct sx/sy (full-screen WIMAP / TITLEPIC / INTERPIC) must stretch
        // to the requested destination or the map preview leaves black gutters.
        let sx = scale_x.abs().max(1e-6);
        let sy = scale_y.abs().max(1e-6);
        let uniform = (sx - sy).abs() <= sx.max(sy) * 0.02;
        let (sx, sy) = if uniform {
            let s = sx.min(sy);
            (s, s)
        } else {
            (sx, sy)
        };
        for row in 0..h {
            let y0 = (origin_y + row as f32 * sy).floor();
            let y1 = (origin_y + (row + 1) as f32 * sy).ceil();
            let ph = (y1 - y0).max(1.0);
            let mut col = 0usize;
            while col < w {
                let di = row * w + col;
                if self.alpha[di] == 0 {
                    col += 1;
                    continue;
                }
                let idx = self.indices[di];
                let start = col;
                col += 1;
                while col < w {
                    let dj = row * w + col;
                    if self.alpha[dj] == 0 || self.indices[dj] != idx {
                        break;
                    }
                    col += 1;
                }
                let [r, g, b] = palette[usize::from(idx)];
                let rgba = [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ];
                let x0 = (origin_x + start as f32 * sx).floor();
                let x1 = (origin_x + col as f32 * sx).ceil();
                let pw = (x1 - x0).max(1.0);
                emit(x0, y0, pw, ph, rgba);
            }
        }
    }
}

/// Lazy patch loader keyed by lump name (uppercase).
#[derive(Clone, Debug)]
pub struct PatchCache {
    /// PLAYPAL[0] RGB triples.
    palette: [[u8; 3]; 256],
    /// Loaded patches.
    patches: BTreeMap<String, SoftPatch>,
    /// Sprite frame table (built on first use), see [`Self::sprite_frame_lump`].
    sprite_frames: Option<SpriteFrames>,
}

/// `(sprite, frame)` → per-rotation `(lump, flip)` for rotations 1–8.
type SpriteFrames = std::collections::HashMap<(String, u8), [Option<(String, bool)>; 8]>;

/// Frame letter → index (`A` = 0 … `Z` = 25, `[` `\\` `]` = 26–28).
fn frame_index(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'[' => Some(26),
        b'\\' => Some(27),
        b']' => Some(28),
        _ => None,
    }
}

/// Record one `frame`/`rot` pair of a sprite lump (GZDoom `R_InstallSprite`):
/// rotation 0 fills all eight views; a later rotated lump overrides its view.
fn install_frame(table: &mut SpriteFrames, sprite: &str, frame: u8, rot: u8, lump: &str, flip: bool) {
    let slots = table
        .entry((sprite.to_owned(), frame))
        .or_insert_with(|| std::array::from_fn(|_| None));
    if rot == 0 {
        for slot in slots.iter_mut() {
            *slot = Some((lump.to_owned(), flip));
        }
    } else if let Some(slot) = slots.get_mut(usize::from(rot - 1)) {
        *slot = Some((lump.to_owned(), flip));
    }
}

/// Parse sprite lump names (`POSSA1`, `POSSA2A8`, `BOS2A2C8`) in load order.
///
/// The second pair of an 8-character name is the mirrored view of a frame
/// that may be a different letter: `BOS2A2C8` is frame A seen from 2 and
/// frame C seen from 8 (flipped). Brutal Doom names most monster frames this
/// way; matching only `A2A8` left those frames with no lump, so enemies
/// vanished mid-animation from some angles.
#[must_use]
fn build_sprite_frames<'a>(names: impl Iterator<Item = &'a str>) -> SpriteFrames {
    let mut table = SpriteFrames::new();
    for name in names {
        let b = name.as_bytes();
        if (b.len() != 6 && b.len() != 8) || !name.is_ascii() {
            continue;
        }
        let sprite = &name[..4];
        let rot_of = |c: u8| (b'0'..=b'8').contains(&c).then(|| c - b'0');
        let (Some(f1), Some(r1)) = (frame_index(b[4]), rot_of(b[5])) else {
            continue;
        };
        if b.len() == 8 {
            let (Some(f2), Some(r2)) = (frame_index(b[6]), rot_of(b[7])) else {
                continue;
            };
            install_frame(&mut table, sprite, f1, r1, name, false);
            install_frame(&mut table, sprite, f2, r2, name, true);
        } else {
            install_frame(&mut table, sprite, f1, r1, name, false);
        }
    }
    table
}

impl Default for PatchCache {
    fn default() -> Self {
        let mut palette = [[0u8; 3]; 256];
        for (i, slot) in palette.iter_mut().enumerate() {
            let g = u8::try_from(i).unwrap_or(255);
            *slot = [g, g, g];
        }
        Self {
            palette,
            patches: BTreeMap::new(),
            sprite_frames: None,
        }
    }
}

impl PatchCache {
    /// Load PLAYPAL from `vfs` (falls back to greyscale if missing).
    #[must_use]
    pub fn from_vfs(vfs: &Vfs) -> Self {
        let mut palette = [[0u8; 3]; 256];
        for (i, slot) in palette.iter_mut().enumerate() {
            let g = u8::try_from(i).unwrap_or(255);
            *slot = [g, g, g];
        }
        if let Some(id) = vfs.find("PLAYPAL", Namespace::Global) {
            if let Ok(bytes) = vfs.read(id) {
                if let Ok(pp) = parse_playpal(&bytes) {
                    if let Some(pal) = pp.palettes.first() {
                        for (i, slot) in palette.iter_mut().enumerate() {
                            let o = i * 3;
                            *slot = [pal.rgb[o], pal.rgb[o + 1], pal.rgb[o + 2]];
                        }
                    }
                }
            }
        }
        Self {
            palette,
            patches: BTreeMap::new(),
            sprite_frames: None,
        }
    }

    /// Lump for `sprite` `frame` seen from rotation `rot` (1–8), with its flip.
    ///
    /// Built once from every sprite-namespace lump, later archives overriding
    /// earlier ones, like GZDoom's sprite installer. Handles mixed-letter
    /// pairs (`BOS2A2C8`) that a name guess cannot find.
    pub fn sprite_frame_lump(
        &mut self,
        vfs: &Vfs,
        sprite: &str,
        frame: u8,
        rot: u8,
    ) -> Option<(String, bool)> {
        let table = self.sprite_frames.get_or_insert_with(|| {
            let names: Vec<String> = vfs
                .iter()
                .filter(|(_, li)| li.namespace == Namespace::Sprites)
                .map(|(_, li)| li.short_name.as_str().to_ascii_uppercase())
                .collect();
            build_sprite_frames(names.iter().map(String::as_str))
        });
        let slots = table.get(&(sprite.trim().to_ascii_uppercase(), frame))?;
        slots.get(usize::from(rot.clamp(1, 8) - 1))?.clone()
    }

    /// Ensure `name` is loaded (Sprites → Graphics → Global).
    pub fn ensure(&mut self, vfs: &Vfs, name: &str) -> Option<&SoftPatch> {
        let key = name.to_ascii_uppercase();
        if self.patches.contains_key(&key) {
            return self.patches.get(&key);
        }
        let id = vfs
            .find(&key, Namespace::Sprites)
            .or_else(|| vfs.find(&key, Namespace::Graphics))
            .or_else(|| vfs.find(&key, Namespace::Global))
            .or_else(|| vfs.find_any_ns(&key))?;
        let bytes = vfs.read(id).ok()?;
        // Brutal Doom (and most GZDoom mods) ship sprites as PNG, not Doom patches.
        let patch = if room_formats::is_png(&bytes)
            || room_formats::is_jpeg(&bytes)
            || room_formats::is_bmp(&bytes)
            || room_formats::is_pcx(&bytes)
            || room_formats::is_tga(&bytes)
        {
            SoftPatch::from_image(room_formats::decode_image(&bytes).ok()?)?
        } else {
            let parsed = parse_patch(&bytes).ok()?;
            if parsed.width == 0 || parsed.height == 0 {
                return None;
            }
            SoftPatch::from_patch(parsed)
        };
        self.patches.insert(key.clone(), patch);
        self.patches.get(&key)
    }

    /// Replace (or add) the cached picture for `name` (editor live edits).
    pub fn insert(&mut self, name: &str, patch: SoftPatch) {
        self.patches.insert(name.to_ascii_uppercase(), patch);
    }

    /// Whether `name` has been loaded.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.patches.contains_key(&name.to_ascii_uppercase())
    }

    /// Look up a previously loaded patch.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SoftPatch> {
        self.patches.get(&name.to_ascii_uppercase())
    }

    /// PLAYPAL entry 0.
    #[must_use]
    pub fn palette(&self) -> &[[u8; 3]; 256] {
        &self.palette
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::path::PathBuf;

    #[test]
    fn png_sprite_keeps_its_own_palette() {
        // 1×1 indexed PNG: palette red, index 0, no tRNS → hole (ZDoom).
        // Second pixel isn't needed; a 1×2 image: index 0 hole, index 1 green opaque.
        let mut raw = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut raw, 2, 1);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![0, 0, 0, 0, 255, 0]);
            encoder.set_trns(vec![0, 255]);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0, 1]).unwrap();
        }
        let dir = std::env::temp_dir().join(format!("room-png-sprite-{}", std::process::id()));
        let sprites = dir.join("sprites");
        std::fs::create_dir_all(&sprites).unwrap();
        std::fs::write(sprites.join("POSSA1.png"), &raw).unwrap();
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&dir)
            .unwrap()
            .build()
            .unwrap();
        let mut cache = PatchCache::from_vfs(&vfs);
        let patch = cache.ensure(&vfs, "POSSA1").expect("png sprite");
        assert_eq!((patch.width, patch.height), (2, 1));
        let pal = [[9u8; 3]; 256];
        let rgba = patch.scale_rgba(2, 1, &pal);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0], "index 0 / tRNS stays a hole");
        assert_eq!(
            &rgba[4..8],
            &[0, 255, 0, 255],
            "colour comes from the PNG palette"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn brutal_pk3_png_sprites_decode() {
        let Ok(pk3) = std::env::var("ROOM_BRUTALDOOM_PK3") else {
            return;
        };
        if !std::path::Path::new(&pk3).is_file() {
            return;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&pk3)
            .unwrap()
            .build()
            .unwrap();
        let mut cache = PatchCache::from_vfs(&vfs);
        for name in ["PISTE0", "PISTI0", "PSPOA1"] {
            let patch = cache
                .ensure(&vfs, name)
                .unwrap_or_else(|| panic!("{name} missing"));
            assert!(patch.width > 8 && patch.height > 8, "{name} size");
            let rgba = patch
                .rgba
                .as_deref()
                .unwrap_or_else(|| panic!("{name} palette"));
            assert!(
                rgba.chunks(4).any(|px| px[3] > 0),
                "{name} has a visible texel"
            );
        }
    }

    #[test]
    fn scale_rgba_keeps_opaque_texels() {
        let gun = SoftPatch {
            width: 2,
            height: 1,
            left_offset: 0,
            top_offset: 0,
            indices: vec![1, 2],
            alpha: vec![255, 0],
            rgba: None,
        };
        let mut pal = [[0u8; 3]; 256];
        pal[1] = [10, 20, 30];
        pal[2] = [40, 50, 60];
        let rgba = gun.scale_rgba(4, 2, &pal);
        assert_eq!(&rgba[0..4], &[10, 20, 30, 255]);
        assert_eq!(&rgba[4..8], &[10, 20, 30, 255]);
        assert_eq!(&rgba[8..12], &[0, 0, 0, 0]);
        assert_eq!(&rgba[12..16], &[0, 0, 0, 0]);
    }

    #[test]
    fn paint_covers_fractional_scale_without_gaps() {
        let patch = SoftPatch {
            width: 4,
            height: 2,
            left_offset: 0,
            top_offset: 0,
            indices: vec![1, 1, 2, 2, 3, 3, 4, 4],
            alpha: vec![255; 8],
            rgba: None,
        };
        let mut pal = [[0u8; 3]; 256];
        pal[1] = [255, 0, 0];
        pal[2] = [0, 255, 0];
        pal[3] = [0, 0, 255];
        pal[4] = [255, 255, 0];
        let mut covered = [false; 8 * 4]; // 7.2≈ceil → use 8×4
        patch.paint(0.0, 0.0, 1.8, 1.8, &pal, |x, y, w, h, _| {
            let x0 = x as i32;
            let y0 = y as i32;
            let x1 = (x + w) as i32;
            let y1 = (y + h) as i32;
            for py in y0.max(0)..y1.min(4) {
                for px in x0.max(0)..x1.min(8) {
                    covered[(py * 8 + px) as usize] = true;
                }
            }
        });
        // Destination is 4*1.8 = 7.2 → ceil 8 wide, 2*1.8=3.6 → ceil 4 tall.
        let expected = 8 * 4;
        let n = covered.iter().filter(|&&c| c).count();
        assert_eq!(n, expected, "expected full coverage, got {n}/{expected}");
    }

    #[test]
    fn paint_keeps_palette_zero_when_posted() {
        // Colour 0 in a post is opaque black, not a hole.
        let patch = SoftPatch {
            width: 3,
            height: 1,
            left_offset: 0,
            top_offset: 0,
            indices: vec![0, 1, 0],
            alpha: vec![255, 255, 0],
            rgba: None,
        };
        let mut pal = [[0u8; 3]; 256];
        pal[0] = [10, 20, 30];
        pal[1] = [255, 0, 0];
        let mut emits = Vec::new();
        patch.paint(0.0, 0.0, 1.0, 1.0, &pal, |x, _y, w, _h, rgba| {
            emits.push((x, w, rgba));
        });
        assert_eq!(emits.len(), 2);
        assert!((emits[0].2[0] - 10.0 / 255.0).abs() < 1e-4);
        assert!((emits[1].2[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn insert_replaces_cached_picture() {
        let mut cache = PatchCache::default();
        assert!(!cache.contains("possa1"));
        let pic = |w: u16| SoftPatch {
            width: w,
            height: 1,
            left_offset: 3,
            top_offset: 4,
            indices: vec![7; usize::from(w)],
            alpha: vec![255; usize::from(w)],
            rgba: None,
        };
        cache.insert("possa1", pic(2));
        cache.insert("POSSA1", pic(5));
        assert!(cache.contains("PossA1"));
        assert_eq!(cache.get("POSSA1").map(|p| p.width), Some(5));
    }

    #[test]
    fn doom1_pistol_keeps_posted_zero() {
        let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let mut cache = PatchCache::from_vfs(&vfs);
        let p = cache.ensure(&vfs, "PISGA0").expect("PISGA0");
        let zeros = p
            .indices
            .iter()
            .zip(p.alpha.iter())
            .filter(|(i, a)| **a == 255 && **i == 0)
            .count();
        assert!(
            zeros > 100,
            "expected posted palette-0 metal pixels, got {zeros}"
        );
    }

    #[test]
    fn loads_stbar_from_freedoom_if_present() {
        let wad =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom2.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = room_vfs::VfsBuilder::new()
            .mount_path(&wad)
            .unwrap()
            .build()
            .unwrap();
        let mut cache = PatchCache::from_vfs(&vfs);
        let p = cache.ensure(&vfs, "STBAR").expect("STBAR");
        assert_eq!(p.width, 320);
        assert_eq!(p.height, 32);
    }
}

#[cfg(test)]
mod sprite_frame_tests {
    use super::*;

    #[test]
    fn mixed_letter_pairs_install_the_mirrored_frame() {
        let t = build_sprite_frames(
            ["BOS2A1C1", "BOS2A2C8", "BOS2A3C7", "BOS2A5C5", "BOS2C4", "POSSA0"].into_iter(),
        );
        let a = t.get(&("BOS2".into(), 0)).unwrap();
        let c = t.get(&("BOS2".into(), 2)).unwrap();
        assert_eq!(a[1], Some(("BOS2A2C8".into(), false)));
        // Frame C from view 8 is the flipped second half of A2C8.
        assert_eq!(c[7], Some(("BOS2A2C8".into(), true)));
        assert_eq!(c[0], Some(("BOS2A1C1".into(), true)));
        assert_eq!(c[3], Some(("BOS2C4".into(), false)));
        let p = t.get(&("POSS".into(), 0)).unwrap();
        assert!(p.iter().all(|s| s.as_ref().is_some_and(|(n, f)| n == "POSSA0" && !f)));
    }

    #[test]
    fn later_rotated_lump_overrides_one_view_of_rotation_zero() {
        let t = build_sprite_frames(["TROOA0", "TROOA3"].into_iter());
        let a = t.get(&("TROO".into(), 0)).unwrap();
        assert_eq!(a[2].as_ref().unwrap().0, "TROOA3");
        assert_eq!(a[0].as_ref().unwrap().0, "TROOA0");
    }
}

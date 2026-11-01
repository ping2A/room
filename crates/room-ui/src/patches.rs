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
        }
    }

    /// Nearest-neighbour RGBA8 raster at `dest_w`×`dest_h` (holes `a=0`).
    ///
    /// Prefer this over [`Self::paint`] for HUD weapons — one image blit beats
    /// thousands of per-span fill rects (plasma / BFG patches are especially hot).
    #[must_use]
    pub fn scale_rgba(
        &self,
        dest_w: u32,
        dest_h: u32,
        palette: &[[u8; 3]; 256],
    ) -> Vec<u8> {
        let sw = usize::from(self.width);
        let sh = usize::from(self.height);
        let dw = dest_w as usize;
        let dh = dest_h as usize;
        let mut out = vec![0u8; dw.saturating_mul(dh).saturating_mul(4)];
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
        }
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
        let patch = SoftPatch::from_patch(parse_patch(&bytes).ok()?);
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
    fn scale_rgba_keeps_opaque_texels() {
        let gun = SoftPatch {
            width: 2,
            height: 1,
            left_offset: 0,
            top_offset: 0,
            indices: vec![1, 2],
            alpha: vec![255, 0],
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

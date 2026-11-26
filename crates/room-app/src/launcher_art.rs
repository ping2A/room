//! Launcher backdrops: each game's title picture (`TITLEPIC`), softened and
//! darkened toward the left where the menu sits, and an accent colour taken
//! from it, so the launcher takes on the colours of the game under the
//! cursor. Games without a title picture (Duke GRPs, PK3 IWADs) get a dark
//! gradient lit in their engine's colour.

use crate::library::Engine;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

/// Backdrop size (16:10, like the 320×200 title pictures).
pub(crate) const WIDTH: u32 = 640;
/// Backdrop height.
pub(crate) const HEIGHT: u32 = 400;

/// A launcher backdrop.
#[derive(Clone, Debug)]
pub(crate) struct Backdrop {
    /// Texture key (the game's path).
    pub key: String,
    /// RGBA, [`WIDTH`]×[`HEIGHT`], opaque.
    pub rgba: Arc<Vec<u8>>,
    /// Highlight colour.
    pub accent: [u8; 3],
}

/// The backdrop for the game at `path`.
pub(crate) fn backdrop(path: &Path, engine: Engine) -> Backdrop {
    let art = match engine {
        Engine::Doom => title_picture(path),
        Engine::Duke => None,
    };
    let fallback_hue = match engine {
        Engine::Doom => 8.0,
        Engine::Duke => 38.0,
    };
    let (rgba, accent) = match art {
        Some((w, h, src)) => {
            let hue = dominant_hue(&src).unwrap_or(fallback_hue);
            (grade(&resample(w, h, &src)), hsv(hue, 0.78, 0.98))
        }
        None => (glow(fallback_hue), hsv(fallback_hue, 0.78, 0.98)),
    };
    Backdrop {
        key: format!("launcher:{}", path.display()),
        rgba: Arc::new(rgba),
        accent,
    }
}

/// `TITLEPIC` of a WAD as RGBA (first `PLAYPAL` palette).
fn title_picture(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let lumps = read_wad_lumps(path, &["TITLEPIC", "PLAYPAL"])?;
    let image = room_formats::decode_image(lumps.get("TITLEPIC")?).ok()?;
    let palette = lumps
        .get("PLAYPAL")
        .and_then(|b| room_formats::parse_playpal(b).ok())
        .and_then(|p| p.palettes.into_iter().next());
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 {
        return None;
    }
    let rgba = match image.pixels {
        room_formats::ImagePixels::Rgba8(r) => r.pixels,
        room_formats::ImagePixels::Paletted(p) => {
            let pal = palette?;
            let mut out = Vec::with_capacity(p.indices.len() * 4);
            for &i in &p.indices {
                let o = usize::from(i) * 3;
                out.extend_from_slice(&[pal.rgb[o], pal.rgb[o + 1], pal.rgb[o + 2], 255]);
            }
            out
        }
    };
    (rgba.len() >= (w * h * 4) as usize).then_some((w, h, rgba))
}

/// Read the lumps `names` of a WAD (directory, then only those lumps).
fn read_wad_lumps(path: &Path, names: &[&str]) -> Option<HashMap<String, Vec<u8>>> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;
    if &header[1..4] != b"WAD" {
        return None;
    }
    let count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let dir_at = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    if count > 200_000 {
        return None;
    }
    let mut dir = vec![0u8; count * 16];
    file.seek(SeekFrom::Start(u64::from(dir_at))).ok()?;
    file.read_exact(&mut dir).ok()?;
    let mut out = HashMap::new();
    for entry in dir.chunks_exact(16) {
        let name: String = entry[8..16]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| char::from(b).to_ascii_uppercase())
            .collect();
        if !names.contains(&name.as_str()) || out.contains_key(&name) {
            continue;
        }
        let at = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]);
        let len = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]) as usize;
        if len > 4 << 20 {
            continue;
        }
        let mut data = vec![0u8; len];
        file.seek(SeekFrom::Start(u64::from(at))).ok()?;
        file.read_exact(&mut data).ok()?;
        out.insert(name, data);
    }
    Some(out)
}

/// Bilinear resample to the backdrop size.
fn resample(w: u32, h: u32, src: &[u8]) -> Vec<f32> {
    let mut out = vec![0f32; (WIDTH * HEIGHT * 3) as usize];
    for y in 0..HEIGHT {
        let fy = ((y as f32 + 0.5) * h as f32 / HEIGHT as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let y0 = fy.floor() as u32;
        let y1 = (y0 + 1).min(h - 1);
        let ty = fy - y0 as f32;
        for x in 0..WIDTH {
            let fx = ((x as f32 + 0.5) * w as f32 / WIDTH as f32 - 0.5).clamp(0.0, (w - 1) as f32);
            let x0 = fx.floor() as u32;
            let x1 = (x0 + 1).min(w - 1);
            let tx = fx - x0 as f32;
            for c in 0..3 {
                let p = |xx: u32, yy: u32| f32::from(src[((yy * w + xx) * 4) as usize + c]);
                let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
                let bottom = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
                out[((y * WIDTH + x) * 3) as usize + c] = top * (1.0 - ty) + bottom * ty;
            }
        }
    }
    out
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Soften (two 3×3 box blurs), desaturate a little, and darken: deep on the
/// left (the menu), the art showing on the right, a fade at the bottom.
fn grade(lin: &[f32]) -> Vec<u8> {
    let (w, h) = (WIDTH as usize, HEIGHT as usize);
    let mut a = lin.to_vec();
    let mut b = vec![0f32; a.len()];
    for _ in 0..3 {
        for y in 0..h {
            for x in 0..w {
                for c in 0..3 {
                    let mut sum = 0.0;
                    let mut n = 0.0;
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                            if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                                sum += a[(yy as usize * w + xx as usize) * 3 + c];
                                n += 1.0;
                            }
                        }
                    }
                    b[(y * w + x) * 3 + c] = sum / n;
                }
            }
        }
        std::mem::swap(&mut a, &mut b);
    }
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let fy = y as f32 / h as f32;
        let bottom = 1.0 - 0.6 * smoothstep(0.5, 1.0, fy);
        let top = 1.0 - 0.45 * (1.0 - smoothstep(0.0, 0.3, fy));
        for x in 0..w {
            let fx = x as f32 / w as f32;
            let side = 0.07 + 0.75 * smoothstep(0.3, 1.0, fx);
            let k = side * bottom * top;
            let i = (y * w + x) * 3;
            let (r, g, bl) = (a[i], a[i + 1], a[i + 2]);
            let grey = 0.299 * r + 0.587 * g + 0.114 * bl;
            let o = (y * w + x) * 4;
            for (c, v) in [r, g, bl].into_iter().enumerate() {
                let v = (v * 0.8 + grey * 0.2) * k;
                out[o + c] = v.round().clamp(0.0, 255.0) as u8;
            }
            out[o + 3] = 255;
        }
    }
    out
}

/// A dark backdrop with a soft glow of `hue` on the right.
fn glow(hue: f32) -> Vec<u8> {
    let [r, g, b] = hsv(hue, 0.7, 0.55);
    let mut out = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let dx = (x as f32 / WIDTH as f32 - 0.82) * 1.6;
            let dy = y as f32 / HEIGHT as f32 - 0.35;
            let k = (1.0 - (dx * dx + dy * dy).sqrt() * 1.4)
                .clamp(0.0, 1.0)
                .powf(1.8);
            let o = ((y * WIDTH + x) * 4) as usize;
            for (c, v) in [r, g, b].into_iter().enumerate() {
                out[o + c] = (14.0 + f32::from(v) * k).min(255.0) as u8;
            }
            out[o + 3] = 255;
        }
    }
    out
}

/// Hue (degrees) of the most present saturated colour, if any.
fn dominant_hue(rgba: &[u8]) -> Option<f32> {
    let mut bins = [0f32; 36];
    for p in rgba.chunks_exact(4) {
        let (h, s, v) = to_hsv(p[0], p[1], p[2]);
        if s > 0.35 && v > 0.25 {
            bins[((h / 10.0) as usize).min(35)] += s * v;
        }
    }
    let (best, weight) =
        bins.iter().enumerate().fold(
            (0, 0.0),
            |acc, (i, &w)| if w > acc.1 { (i, w) } else { acc },
        );
    (weight > 0.0).then_some(best as f32 * 10.0 + 5.0)
}

fn to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max <= 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

/// HSV (degrees, 0–1, 0–1) to RGB bytes.
pub(crate) fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|t| ((t + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAD with `lumps`.
    fn wad(lumps: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut data = b"IWAD".to_vec();
        data.extend_from_slice(&(lumps.len() as u32).to_le_bytes());
        let body: usize = lumps.iter().map(|l| l.1.len()).sum();
        data.extend_from_slice(&((12 + body) as u32).to_le_bytes());
        let mut at = 12u32;
        let mut dir = Vec::new();
        for (name, bytes) in lumps {
            data.extend_from_slice(bytes);
            dir.extend_from_slice(&at.to_le_bytes());
            dir.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            let mut n = [0u8; 8];
            n[..name.len()].copy_from_slice(name.as_bytes());
            dir.extend_from_slice(&n);
            at += bytes.len() as u32;
        }
        data.extend_from_slice(&dir);
        data
    }

    #[test]
    fn a_red_title_picture_makes_a_red_accent_and_a_dark_left() {
        // 2×1 Doom patch, both pixels palette index 1 (pure red).
        let mut patch = Vec::new();
        for v in [2u16, 1, 0, 0] {
            patch.extend_from_slice(&v.to_le_bytes());
        }
        for off in [16u32, 22] {
            patch.extend_from_slice(&off.to_le_bytes());
        }
        for _ in 0..2 {
            patch.extend_from_slice(&[0, 1, 0, 1, 0, 255]);
        }
        let mut playpal = vec![0u8; 768];
        playpal[3] = 220;
        playpal[4] = 30;
        playpal[5] = 20;
        let dir = std::env::temp_dir().join(format!("room-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.wad");
        std::fs::write(&path, wad(&[("PLAYPAL", playpal), ("TITLEPIC", patch)])).unwrap();
        let b = backdrop(&path, Engine::Doom);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(b.rgba.len(), (WIDTH * HEIGHT * 4) as usize);
        let (h, s, _) = to_hsv(b.accent[0], b.accent[1], b.accent[2]);
        assert!(h < 15.0 && s > 0.7, "accent {:?}", b.accent);
        let px = |x: u32, y: u32| b.rgba[((y * WIDTH + x) * 4) as usize];
        // Darker on the menu side than on the art side.
        assert!(
            px(10, 200) < px(600, 200) / 3,
            "{} {}",
            px(10, 200),
            px(600, 200)
        );
    }

    #[test]
    fn games_without_a_title_picture_still_get_one() {
        let b = backdrop(Path::new("/nonexistent/duke3d.grp"), Engine::Duke);
        assert_eq!(b.rgba.len(), (WIDTH * HEIGHT * 4) as usize);
        let (h, _, _) = to_hsv(b.accent[0], b.accent[1], b.accent[2]);
        assert!((30.0..50.0).contains(&h), "{h}");
    }
}

//! Face detection. F3 ships a pure-Rust heuristic (box + 5 landmarks).
//! YuNet / rustface can replace this behind a feature later (`docs/26` §3.4 / §12).

use crate::image::RgbaImage;

/// 2D point in image pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
}

/// One detected face (`docs/26` §3.4).
#[derive(Clone, Debug, PartialEq)]
pub struct FaceDetection {
    /// Axis-aligned box `(x, y, w, h)`.
    pub bbox: (f32, f32, f32, f32),
    /// Left eye, right eye, nose, mouth left, mouth right.
    pub landmarks: [Pt; 5],
    /// Detector confidence 0..=1.
    pub score: f32,
}

/// Run face detection. Returns zero or more faces (heuristic; never panics).
#[must_use]
pub fn detect(image: &RgbaImage) -> Vec<FaceDetection> {
    if image.width < 16 || image.height < 16 || image.pixels.len() < 16 * 4 {
        return Vec::new();
    }
    // Downscale to ≤128 on the long edge for speed.
    let scale = 128.0 / (image.width.max(image.height) as f32);
    let scale = if scale > 1.0 { 1.0 } else { scale };
    let dw = ((image.width as f32) * scale).round().max(8.0) as u32;
    let dh = ((image.height as f32) * scale).round().max(8.0) as u32;
    let small = downsample(image, dw, dh);

    // Skin-likeness score map + integral for sliding windows.
    let mut skin = vec![0u8; (dw * dh) as usize];
    for y in 0..dh {
        for x in 0..dw {
            let i = ((y * dw + x) * 4) as usize;
            let r = small.pixels[i];
            let g = small.pixels[i + 1];
            let b = small.pixels[i + 2];
            skin[(y * dw + x) as usize] = if is_skin(r, g, b) { 1 } else { 0 };
        }
    }

    let mut best: Option<(f32, u32, u32, u32, u32)> = None; // score, x, y, w, h
    let min_s = (dw.min(dh) / 4).max(12);
    let max_s = dw.min(dh) * 9 / 10;
    let mut s = min_s;
    while s <= max_s {
        let step = (s / 6).max(2);
        let mut y = 0u32;
        while y + s <= dh {
            let mut x = 0u32;
            while x + s <= dw {
                let dens = skin_density(&skin, dw, x, y, s, s);
                if dens > 0.35 {
                    // Prefer upper-half density (faces sit high in portraits).
                    let upper = skin_density(&skin, dw, x, y, s, s / 2);
                    let score = dens * 0.6 + upper * 0.4;
                    let replace = match best {
                        None => true,
                        Some((bs, ..)) => score > bs,
                    };
                    if replace {
                        best = Some((score, x, y, s, s));
                    }
                }
                x += step;
            }
            y += step;
        }
        s = s + s / 4 + 4;
    }

    let Some((score, bx, by, bw, bh)) = best else {
        return Vec::new();
    };
    if score < 0.38 {
        return Vec::new();
    }

    // Eye blobs: darkest regions in upper half of the box.
    let (lx, ly, rx, ry) = find_eyes(&small, bx, by, bw, bh);
    let nose = Pt {
        x: (lx + rx) * 0.5,
        y: (ly + ry) * 0.5 + bh as f32 * 0.12,
    };
    let mouth_y = by as f32 + bh as f32 * 0.72;
    let mouth_l = Pt {
        x: lx + (rx - lx) * 0.25,
        y: mouth_y,
    };
    let mouth_r = Pt {
        x: lx + (rx - lx) * 0.75,
        y: mouth_y,
    };

    let inv = 1.0 / scale;
    vec![FaceDetection {
        bbox: (
            bx as f32 * inv,
            by as f32 * inv,
            bw as f32 * inv,
            bh as f32 * inv,
        ),
        landmarks: [
            Pt {
                x: lx * inv,
                y: ly * inv,
            },
            Pt {
                x: rx * inv,
                y: ry * inv,
            },
            Pt {
                x: nose.x * inv,
                y: nose.y * inv,
            },
            Pt {
                x: mouth_l.x * inv,
                y: mouth_l.y * inv,
            },
            Pt {
                x: mouth_r.x * inv,
                y: mouth_r.y * inv,
            },
        ],
        score: score.clamp(0.0, 1.0),
    }]
}

fn is_skin(r: u8, g: u8, b: u8) -> bool {
    // Simple RGB skin gate (works on synthetic + many photos).
    r > 60
        && g > 30
        && b > 15
        && r > g
        && r > b
        && (i32::from(r) - i32::from(g)) > 10
        && (i32::from(r) - i32::from(b)) > 15
}

fn skin_density(skin: &[u8], stride: u32, x: u32, y: u32, w: u32, h: u32) -> f32 {
    let mut n = 0u32;
    let mut hit = 0u32;
    for yy in y..y + h {
        for xx in x..x + w {
            n += 1;
            hit += u32::from(skin[(yy * stride + xx) as usize]);
        }
    }
    if n == 0 {
        0.0
    } else {
        hit as f32 / n as f32
    }
}

fn find_eyes(img: &RgbaImage, bx: u32, by: u32, bw: u32, bh: u32) -> (f32, f32, f32, f32) {
    let eye_h = (bh / 3).max(4);
    let mid = bx + bw / 2;
    let left = darkest_blob(img, bx, by + bh / 10, mid, by + eye_h);
    let right = darkest_blob(img, mid, by + bh / 10, bx + bw, by + eye_h);
    let (lx, ly) = left.unwrap_or((bx as f32 + bw as f32 * 0.3, by as f32 + bh as f32 * 0.35));
    let (rx, ry) = right.unwrap_or((bx as f32 + bw as f32 * 0.7, by as f32 + bh as f32 * 0.35));
    (lx, ly, rx, ry)
}

fn darkest_blob(
    img: &RgbaImage,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
) -> Option<(f32, f32)> {
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut best = 255u32;
    let mut sx = 0u64;
    let mut sy = 0u64;
    let mut n = 0u64;
    for y in y0..y1.min(img.height) {
        for x in x0..x1.min(img.width) {
            let i = ((y * img.width + x) * 4) as usize;
            let lum = u32::from(img.pixels[i])
                + u32::from(img.pixels[i + 1])
                + u32::from(img.pixels[i + 2]);
            if lum < best + 30 {
                if lum + 30 < best {
                    best = lum;
                    sx = u64::from(x);
                    sy = u64::from(y);
                    n = 1;
                } else {
                    sx += u64::from(x);
                    sy += u64::from(y);
                    n += 1;
                }
            }
        }
    }
    if n == 0 {
        None
    } else {
        Some((sx as f32 / n as f32, sy as f32 / n as f32))
    }
}

fn downsample(src: &RgbaImage, dw: u32, dh: u32) -> RgbaImage {
    let mut out = RgbaImage::new(dw, dh);
    for y in 0..dh {
        for x in 0..dw {
            let sx = ((x as f32 + 0.5) * src.width as f32 / dw as f32) as u32;
            let sy = ((y as f32 + 0.5) * src.height as f32 / dh as f32) as u32;
            let sx = sx.min(src.width - 1);
            let sy = sy.min(src.height - 1);
            let i = ((sy * src.width + sx) * 4) as usize;
            out.put(
                x,
                y,
                [
                    src.pixels[i],
                    src.pixels[i + 1],
                    src.pixels[i + 2],
                    src.pixels[i + 3],
                ],
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn synthetic_portrait() -> RgbaImage {
        let mut img = RgbaImage::new(160, 200);
        // Skin ellipse.
        let cx = 80.0_f32;
        let cy = 95.0_f32;
        for y in 0..200u32 {
            for x in 0..160u32 {
                let nx = (x as f32 - cx) / 48.0;
                let ny = (y as f32 - cy) / 62.0;
                if nx * nx + ny * ny <= 1.0 {
                    img.put(x, y, [210, 165, 135, 255]);
                } else {
                    img.put(x, y, [30, 40, 50, 255]);
                }
            }
        }
        // Eyes.
        for (ex, ey) in [(58.0_f32, 80.0), (102.0, 80.0)] {
            for dy in -3i32..=3 {
                for dx in -4i32..=4 {
                    let x = (ex as i32 + dx).clamp(0, 159) as u32;
                    let y = (ey as i32 + dy).clamp(0, 199) as u32;
                    img.put(x, y, [25, 25, 40, 255]);
                }
            }
        }
        img
    }

    #[test]
    fn detects_synthetic_face() {
        let faces = detect(&synthetic_portrait());
        assert_eq!(faces.len(), 1);
        let f = &faces[0];
        assert!(f.score > 0.35);
        assert!(f.landmarks[0].x < f.landmarks[1].x);
        assert!(f.bbox.2 > 20.0 && f.bbox.3 > 20.0);
    }

    #[test]
    fn empty_on_tiny() {
        assert!(detect(&RgbaImage::new(4, 4)).is_empty());
    }
}

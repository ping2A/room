//! Similarity-transform alignment into the canonical 128×128 face crop.

use crate::image::{AlphaMask, RgbaImage};
use crate::profile::{
    CANON_CHIN_Y, CANON_LEFT_EYE, CANON_RIGHT_EYE, Consent, FaceId, FaceProfile, FaceView, Rgb,
};

/// Manual landmark points in source-image pixel space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManualPoints {
    /// Left eye centre.
    pub left_eye: (f32, f32),
    /// Right eye centre.
    pub right_eye: (f32, f32),
    /// Chin point (vertical scale hint).
    pub chin: (f32, f32),
}

/// Alignment failures.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AlignError {
    /// Zero-sized source.
    #[error("empty source image")]
    EmptySource,
    /// Eyes coincide (undefined scale/rotation).
    #[error("left and right eye points are too close")]
    DegenerateEyes,
    /// Profile construction failed.
    #[error(transparent)]
    Profile(#[from] crate::profile::ProfileError),
}

/// Align a source RGBA image with manual eye/chin points into a [`FaceProfile`].
///
/// Source pixels only — EXIF / metadata must already have been stripped by the
/// caller (`docs/26` §3.1).
pub fn align_manual(
    source: &RgbaImage,
    points: ManualPoints,
    name: impl Into<String>,
    id: FaceId,
    consent: Consent,
    created: u64,
) -> Result<FaceProfile, AlignError> {
    if source.width == 0 || source.height == 0 {
        return Err(AlignError::EmptySource);
    }
    let xform = similarity_params(points)?;
    let mut rgba = RgbaImage::new(128, 128);
    for y in 0..128u32 {
        for x in 0..128u32 {
            let (px, py) = xform.map_canon_to_source(x as f32 + 0.5, y as f32 + 0.5);
            rgba.put(x, y, source.sample_bilinear(px, py));
        }
    }
    let mask = AlphaMask::feathered_ellipse(128, 128, 0.18);
    for i in 0..128 * 128 {
        let a = mask.data[i];
        let o = i * 4;
        let ma = u16::from(rgba.pixels[o + 3]) * u16::from(a) / 255;
        rgba.pixels[o + 3] = ma as u8;
        if a == 0 {
            rgba.pixels[o] = 0;
            rgba.pixels[o + 1] = 0;
            rgba.pixels[o + 2] = 0;
        }
    }
    let (tone, hair) = estimate_tone_hair(&rgba, &mask);
    let front = FaceView::new(rgba, mask)?;
    Ok(FaceProfile {
        id,
        name: name.into(),
        front,
        left: None,
        right: None,
        back: None,
        tone,
        hair,
        consent,
        created,
    })
}

struct Similarity {
    src_left: (f32, f32),
    scale_x: f32,
    scale_y: f32,
    angle: f32,
    canon_left: (f32, f32),
}

impl Similarity {
    fn map_canon_to_source(&self, cx: f32, cy: f32) -> (f32, f32) {
        let dx = cx - self.canon_left.0;
        let dy = cy - self.canon_left.1;
        let inv_sx = if self.scale_x.abs() < 1e-6 {
            0.0
        } else {
            1.0 / self.scale_x
        };
        let inv_sy = if self.scale_y.abs() < 1e-6 {
            0.0
        } else {
            1.0 / self.scale_y
        };
        let ux = dx * inv_sx;
        let uy = dy * inv_sy;
        let (s, c) = (libm::sinf(-self.angle), libm::cosf(-self.angle));
        let rx = c * ux - s * uy;
        let ry = s * ux + c * uy;
        (rx + self.src_left.0, ry + self.src_left.1)
    }
}

fn similarity_params(p: ManualPoints) -> Result<Similarity, AlignError> {
    let (lx, ly) = p.left_eye;
    let (rx, ry) = p.right_eye;
    let dx = rx - lx;
    let dy = ry - ly;
    let src_dist = libm::sqrtf(dx * dx + dy * dy);
    if src_dist < 1.0 {
        return Err(AlignError::DegenerateEyes);
    }
    let (clx, cly) = CANON_LEFT_EYE;
    let (crx, cry) = CANON_RIGHT_EYE;
    let cdx = crx - clx;
    let cdy = cry - cly;
    let canon_dist = libm::sqrtf(cdx * cdx + cdy * cdy);
    let scale = canon_dist / src_dist;
    let src_ang = libm::atan2f(dy, dx);
    let canon_ang = libm::atan2f(cdy, cdx);
    let angle = canon_ang - src_ang;
    let chin_c = {
        let dx = p.chin.0 - lx;
        let dy = p.chin.1 - ly;
        let (s, c) = (libm::sinf(angle), libm::cosf(angle));
        let rx = (c * dx - s * dy) * scale + clx;
        let ry = (s * dx + c * dy) * scale + cly;
        (rx, ry)
    };
    let sy = if (chin_c.1 - cly).abs() > 1.0 {
        let want = CANON_CHIN_Y - cly;
        let got = chin_c.1 - cly;
        (want / got).clamp(0.7, 1.4) * scale
    } else {
        scale
    };
    Ok(Similarity {
        src_left: (lx, ly),
        scale_x: scale,
        scale_y: sy,
        angle,
        canon_left: (clx, cly),
    })
}

fn mean_rgb(acc: [u64; 3], n: u64, fallback: Rgb) -> Rgb {
    let Some(r) = acc[0].checked_div(n) else {
        return fallback;
    };
    let Some(g) = acc[1].checked_div(n) else {
        return fallback;
    };
    let Some(b) = acc[2].checked_div(n) else {
        return fallback;
    };
    Rgb {
        r: r as u8,
        g: g as u8,
        b: b as u8,
    }
}

fn estimate_tone_hair(rgba: &RgbaImage, mask: &AlphaMask) -> (Rgb, Rgb) {
    let mut tone_acc = [0u64; 3];
    let mut tone_n = 0u64;
    let mut hair_acc = [0u64; 3];
    let mut hair_n = 0u64;
    for y in 0..128u32 {
        for x in 0..128u32 {
            let a = mask.get(x, y);
            if a < 32 {
                continue;
            }
            let i = ((y * 128 + x) * 4) as usize;
            let px = [rgba.pixels[i], rgba.pixels[i + 1], rgba.pixels[i + 2]];
            if y >= 70 {
                tone_acc[0] += u64::from(px[0]);
                tone_acc[1] += u64::from(px[1]);
                tone_acc[2] += u64::from(px[2]);
                tone_n += 1;
            }
            if y < 40 {
                hair_acc[0] += u64::from(px[0]);
                hair_acc[1] += u64::from(px[1]);
                hair_acc[2] += u64::from(px[2]);
                hair_n += 1;
            }
        }
    }
    let tone = mean_rgb(
        tone_acc,
        tone_n,
        Rgb {
            r: 180,
            g: 140,
            b: 120,
        },
    );
    let hair = mean_rgb(
        hair_acc,
        hair_n,
        Rgb {
            r: 40,
            g: 30,
            b: 20,
        },
    );
    (tone, hair)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::profile::CANON_LEFT_EYE;

    fn synthetic_face(w: u32, h: u32, eye_y: f32, eye_sep: f32) -> (RgbaImage, ManualPoints) {
        let mut img = RgbaImage::new(w, h);
        // Skin ellipse fill.
        let cx = (w as f32) * 0.5;
        let cy = (h as f32) * 0.5;
        for y in 0..h {
            for x in 0..w {
                let nx = (x as f32 - cx) / (w as f32 * 0.35);
                let ny = (y as f32 - cy) / (h as f32 * 0.45);
                if nx * nx + ny * ny <= 1.0 {
                    img.put(x, y, [200, 160, 130, 255]);
                }
            }
        }
        let lx = cx - eye_sep * 0.5;
        let rx = cx + eye_sep * 0.5;
        // Dark eye dots.
        for (ex, ey) in [(lx, eye_y), (rx, eye_y)] {
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let x = (ex as i32 + dx).clamp(0, w as i32 - 1) as u32;
                    let y = (ey as i32 + dy).clamp(0, h as i32 - 1) as u32;
                    img.put(x, y, [20, 20, 40, 255]);
                }
            }
        }
        let points = ManualPoints {
            left_eye: (lx, eye_y),
            right_eye: (rx, eye_y),
            chin: (cx, cy + h as f32 * 0.35),
        };
        (img, points)
    }

    #[test]
    fn eyes_land_near_canonical() {
        let (img, pts) = synthetic_face(200, 240, 90.0, 60.0);
        let profile = align_manual(&img, pts, "test", FaceId(1), Consent::SelfCamera, 0).unwrap();
        // Sample near canonical left eye — should be dark (the eye dot).
        let i = ((CANON_LEFT_EYE.1 as u32) * 128 + (CANON_LEFT_EYE.0 as u32)) * 4;
        let r = profile.front.rgba.pixels[i as usize];
        assert!(r < 80, "left eye pixel too bright: {r}");
    }
}

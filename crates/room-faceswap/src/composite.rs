//! Front-view face → sprite compositing (`docs/26` §5).

use crate::anchor::{HeadAnchor, Visibility};
use crate::color::{nearest_oklab, oklab_to_srgb8, srgb8_to_oklab, transfer_oklab};
use crate::profile::FaceProfile;

/// Paletted sprite lump input / output (indices + alpha, Doom offsets).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalettedSprite {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Left offset (unchanged by composite).
    pub left_offset: i32,
    /// Top offset (unchanged by composite).
    pub top_offset: i32,
    /// Row-major PLAYPAL indices.
    pub indices: Vec<u8>,
    /// Row-major opacity (`0` = hole).
    pub alpha: Vec<u8>,
}

/// Tunables for [`composite_front`].
#[derive(Clone, Debug, PartialEq)]
pub struct CompositeOptions {
    /// Colour-transfer strength (0 = keep face colours, 1 = full match).
    pub transfer_strength: f32,
    /// Skip these palette indices when quantizing (transparent / fullbright).
    pub skip_indices: Vec<u8>,
    /// Allow painting 1 px outside the silhouette.
    pub allow_overflow: bool,
}

impl Default for CompositeOptions {
    fn default() -> Self {
        Self {
            transfer_strength: 0.7,
            // Doom tools sometimes treat 247 as transparent; green player ramp
            // 112..=127 is avoided so translations don't recolour the face.
            skip_indices: {
                let mut v = vec![247];
                v.extend(112u8..=127);
                v
            },
            allow_overflow: false,
        }
    }
}

/// Composite errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CompositeError {
    /// Width/height do not match buffer lengths.
    #[error("sprite buffer size mismatch")]
    SizeMismatch,
}

/// Paste `profile.front` onto `sprite` using `anchor` (front yaw only in F0).
///
/// Output has the **same** size and offsets. Alpha is never expanded beyond
/// the original silhouette unless `allow_overflow` is set.
pub fn composite_front(
    sprite: &PalettedSprite,
    palette: &[u8; 768],
    anchor: &HeadAnchor,
    profile: &FaceProfile,
    opts: &CompositeOptions,
) -> Result<PalettedSprite, CompositeError> {
    let n = (sprite.width as usize).saturating_mul(sprite.height as usize);
    if sprite.indices.len() != n || sprite.alpha.len() != n {
        return Err(CompositeError::SizeMismatch);
    }
    if matches!(anchor.visible, Visibility::Hidden) {
        return Ok(sprite.clone());
    }

    let mut out = sprite.clone();
    let (head_mean, head_std, head_lum_mean) = head_stats(sprite, palette, anchor);
    let (face_mean, face_std) = face_stats(profile);

    let roll = anchor.roll * (core::f32::consts::PI / 180.0);
    let (rs, rc) = (libm::sinf(roll), libm::cosf(roll));
    let keep_y = anchor.center.1 - anchor.radius.1 + anchor.keep_top * 2.0 * anchor.radius.1;

    // 4× supersample then box downsample.
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            let si = (y * sprite.width + x) as usize;
            let orig_a = sprite.alpha[si];
            if orig_a == 0 && !opts.allow_overflow {
                continue;
            }
            // keep_top: leave original pixels in the top band of the ellipse.
            let py = y as f32 + 0.5;
            let px = x as f32 + 0.5;
            if inside_ellipse(px, py, anchor) && py < keep_y {
                continue;
            }
            if !inside_ellipse(px, py, anchor) {
                continue;
            }

            let mut acc = [0.0f32; 4];
            let mut wsum = 0.0f32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let jx = px + (sx as f32 + 0.5) * 0.25 - 0.5;
                    let jy = py + (sy as f32 + 0.5) * 0.25 - 0.5;
                    if let Some(sample) = sample_face_at(profile, anchor, jx, jy, rc, rs) {
                        for i in 0..4 {
                            acc[i] += f32::from(sample[i]);
                        }
                        wsum += 1.0;
                    }
                }
            }
            if wsum < 1.0 {
                continue;
            }
            for v in &mut acc {
                *v /= wsum;
            }
            let face_a = (acc[3] + 0.5) as u8;
            if face_a < 8 {
                continue;
            }

            // Colour transfer in Oklab.
            let face_lab = srgb8_to_oklab(acc[0] as u8, acc[1] as u8, acc[2] as u8);
            let mut lab = transfer_oklab(
                face_lab,
                face_mean,
                face_std,
                head_mean,
                head_std,
                opts.transfer_strength,
            );
            // Re-apply sprite shading: scale L by local head luminance / mean.
            let oi = sprite.indices[si] as usize * 3;
            let (or, og, ob) = (palette[oi], palette[oi + 1], palette[oi + 2]);
            let orig_lab = srgb8_to_oklab(or, og, ob);
            if head_lum_mean > 1e-4 {
                lab[0] *= orig_lab[0] / head_lum_mean;
            }
            let rgb = oklab_to_srgb8(lab);

            // Feather: blend with original near the ellipse edge / mask edge.
            let edge = ellipse_edge_factor(px, py, anchor);
            let blend = (f32::from(face_a) / 255.0) * edge;
            let fr = (f32::from(rgb[0]) * blend + f32::from(or) * (1.0 - blend)) as u8;
            let fg = (f32::from(rgb[1]) * blend + f32::from(og) * (1.0 - blend)) as u8;
            let fb = (f32::from(rgb[2]) * blend + f32::from(ob) * (1.0 - blend)) as u8;

            out.indices[si] = nearest_oklab(palette, fr, fg, fb, &opts.skip_indices);
            // Never expand alpha beyond original (unless overflow allowed).
            if opts.allow_overflow && orig_a == 0 {
                out.alpha[si] = face_a;
            }
        }
    }
    Ok(out)
}

fn inside_ellipse(x: f32, y: f32, a: &HeadAnchor) -> bool {
    let nx = (x - a.center.0) / a.radius.0.max(0.5);
    let ny = (y - a.center.1) / a.radius.1.max(0.5);
    nx * nx + ny * ny <= 1.0
}

fn ellipse_edge_factor(x: f32, y: f32, a: &HeadAnchor) -> f32 {
    let nx = (x - a.center.0) / a.radius.0.max(0.5);
    let ny = (y - a.center.1) / a.radius.1.max(0.5);
    let d = libm::sqrtf(nx * nx + ny * ny);
    if d <= 0.85 {
        1.0
    } else if d >= 1.0 {
        0.0
    } else {
        (1.0 - d) / 0.15
    }
}

fn sample_face_at(
    profile: &FaceProfile,
    anchor: &HeadAnchor,
    x: f32,
    y: f32,
    rc: f32,
    rs: f32,
) -> Option<[u8; 4]> {
    // Map sprite pixel → unit ellipse → canonical face UV.
    let dx = x - anchor.center.0;
    let dy = y - anchor.center.1;
    // Un-roll.
    let ux = rc * dx + rs * dy;
    let uy = -rs * dx + rc * dy;
    let nx = ux / anchor.radius.0.max(0.5);
    let ny = uy / anchor.radius.1.max(0.5);
    if nx * nx + ny * ny > 1.0 {
        return None;
    }
    // Ellipse [-1,1]² → face crop. Eyes sit at y≈52/128; map ellipse centre to crop centre.
    let fx = (nx * 0.5 + 0.5) * 127.0;
    let fy = (ny * 0.5 + 0.5) * 127.0;
    let rgba = profile.front.rgba.sample_bilinear(fx, fy);
    let mx = fx.clamp(0.0, 127.0) as u32;
    let my = fy.clamp(0.0, 127.0) as u32;
    let ma = profile.front.mask.get(mx, my);
    let a = (u16::from(rgba[3]) * u16::from(ma) / 255) as u8;
    Some([rgba[0], rgba[1], rgba[2], a])
}

fn head_stats(
    sprite: &PalettedSprite,
    palette: &[u8; 768],
    anchor: &HeadAnchor,
) -> ([f32; 3], [f32; 3], f32) {
    let mut labs = Vec::new();
    for y in 0..sprite.height {
        for x in 0..sprite.width {
            let i = (y * sprite.width + x) as usize;
            if sprite.alpha[i] == 0 {
                continue;
            }
            if !inside_ellipse(x as f32 + 0.5, y as f32 + 0.5, anchor) {
                continue;
            }
            let o = sprite.indices[i] as usize * 3;
            labs.push(srgb8_to_oklab(palette[o], palette[o + 1], palette[o + 2]));
        }
    }
    mean_std_lum(&labs)
}

fn face_stats(profile: &FaceProfile) -> ([f32; 3], [f32; 3]) {
    let mut labs = Vec::new();
    for y in 0..128u32 {
        for x in 0..128u32 {
            if profile.front.mask.get(x, y) < 32 {
                continue;
            }
            let i = ((y * 128 + x) * 4) as usize;
            let p = &profile.front.rgba.pixels;
            labs.push(srgb8_to_oklab(p[i], p[i + 1], p[i + 2]));
        }
    }
    let (m, s, _) = mean_std_lum(&labs);
    (m, s)
}

fn mean_std_lum(labs: &[[f32; 3]]) -> ([f32; 3], [f32; 3], f32) {
    if labs.is_empty() {
        return ([0.5, 0.0, 0.0], [0.1, 0.05, 0.05], 0.5);
    }
    let n = labs.len() as f32;
    let mut mean = [0.0f32; 3];
    for lab in labs {
        mean[0] += lab[0];
        mean[1] += lab[1];
        mean[2] += lab[2];
    }
    mean[0] /= n;
    mean[1] /= n;
    mean[2] /= n;
    let mut var = [0.0f32; 3];
    for lab in labs {
        for i in 0..3 {
            let d = lab[i] - mean[i];
            var[i] += d * d;
        }
    }
    let std = [
        libm::sqrtf(var[0] / n),
        libm::sqrtf(var[1] / n),
        libm::sqrtf(var[2] / n),
    ];
    (mean, std, mean[0])
}

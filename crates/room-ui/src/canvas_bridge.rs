//! Rasterize [`UiDrawList`] into renderer [`room_render::Canvas`] commands.
//!
//! Kept here so `room-ui` can stay GPU-free while shells share one path.
//! The renderer canvas only supports fill/text; lines become thin rects.

/// Minimal canvas sink (avoids a hard dep on room-render from unit tests).
pub trait CanvasSink {
    /// Filled axis-aligned rect in screen pixels.
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]);
    /// Text run (fallback when no [`crate::FontRegistry`] glyph is available).
    fn text(&mut self, x: f32, y: f32, text: &str, color: [f32; 4]);
    /// Pre-rasterized RGBA8 image (`w*h*4`, straight alpha). Default expands to fills.
    fn blit_rgba(&mut self, x: f32, y: f32, w: u32, h: u32, pixels: &[u8]) {
        let need = (w as usize).saturating_mul(h as usize).saturating_mul(4);
        if w == 0 || h == 0 || pixels.len() < need {
            return;
        }
        for row in 0..h {
            for col in 0..w {
                let i = ((row * w + col) * 4) as usize;
                if pixels[i + 3] == 0 {
                    continue;
                }
                let r = f32::from(pixels[i]) / 255.0;
                let g = f32::from(pixels[i + 1]) / 255.0;
                let b = f32::from(pixels[i + 2]) / 255.0;
                let a = f32::from(pixels[i + 3]) / 255.0;
                self.fill_rect(x + col as f32, y + row as f32, 1.0, 1.0, [r, g, b, a]);
            }
        }
    }

    /// [`Self::blit_rgba`] handing over the pixels: a sink that keeps them
    /// (a canvas command list) takes them without another copy.
    fn blit_rgba_owned(&mut self, x: f32, y: f32, w: u32, h: u32, pixels: Vec<u8>) {
        self.blit_rgba(x, y, w, h, &pixels);
    }
}

use crate::draw::{DrawCmd, UiDrawList, Vec2};
use crate::font::FontRegistry;
use crate::patches::PatchCache;

/// Scale virtual 320×200 cmds into a pixel framebuffer of `fb_w`×`fb_h`.
pub fn blit_draw_list(list: &UiDrawList, fb_w: f32, fb_h: f32, sink: &mut impl CanvasSink) {
    blit_draw_list_with_fonts(list, fb_w, fb_h, None, sink);
}

/// Like [`blit_draw_list`], but paints [`DrawCmd::Text`] via soft fonts when present.
pub fn blit_draw_list_with_fonts(
    list: &UiDrawList,
    fb_w: f32,
    fb_h: f32,
    fonts: Option<&FontRegistry>,
    sink: &mut impl CanvasSink,
) {
    blit_draw_list_full(list, fb_w, fb_h, fonts, None, sink);
}

/// Doom `INVERSECOLORMAP` applied to a palette: white→black inverted luminance.
#[must_use]
pub fn inverse_palette(pal: &[[u8; 3]; 256]) -> [[u8; 3]; 256] {
    pal.map(|[r, g, b]| {
        let lum = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
        let v = 255 - lum.min(255) as u8;
        [v, v, v]
    })
}

/// Vanilla `fuzzoffset` signs (`±FUZZOFF` → ±1 row). Docs/30 N8.
const FUZZ_SIGN: [i8; 50] = [
    1, -1, 1, -1, 1, 1, -1, 1, 1, -1, 1, 1, 1, -1, 1, 1, 1, -1, -1, -1, -1, 1, -1, -1, 1, 1, 1, 1,
    -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, -1, -1, -1, 1, 1, 1, 1, -1, 1, 1, -1, 1,
];

/// Apply Doom software fuzz to a scaled RGBA patch (docs/30 N8).
///
/// Without a framebuffer sample of the scene behind the gun, each opaque pixel
/// is replaced by a **darkened** (±1-row jittered) sample from the same patch —
/// colormap row 6 ≈ keep ~50 % luminance — matching the spectre silhouette.
fn apply_psprite_fuzz(pixels: &mut [u8], w: u32, h: u32) {
    if w == 0 || h == 0 {
        return;
    }
    let src = pixels.to_vec();
    let mut fuzzpos = 0usize;
    for row in 0..h {
        for col in 0..w {
            let i = ((row * w + col) * 4) as usize;
            if src[i + 3] < 16 {
                pixels[i + 3] = 0;
                continue;
            }
            let dy = i32::from(FUZZ_SIGN[fuzzpos]);
            fuzzpos = (fuzzpos + 1) % FUZZ_SIGN.len();
            let sy = (row as i32 + dy).clamp(0, h as i32 - 1) as u32;
            let si = ((sy * w + col) * 4) as usize;
            // Colormap 6: mid darken (vanilla samples the *background*).
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                pixels[i] = (u16::from(src[si]) / 2) as u8;
                pixels[i + 1] = (u16::from(src[si + 1]) / 2) as u8;
                pixels[i + 2] = (u16::from(src[si + 2]) / 2) as u8;
                pixels[i + 3] = 220;
            }
        }
    }
}

/// The part of a `w`×`h` RGBA image drawn at `(x0, y0)` inside the
/// framebuffer rectangle `clip`, as `(x, y, w, h, pixels)`.
fn crop_rgba(
    pixels: &[u8],
    x0: f32,
    y0: f32,
    w: u32,
    h: u32,
    clip: [f32; 4],
) -> Option<(f32, f32, u32, u32, Vec<u8>)> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let to_px = |v: f32, lim: u32| (v.round().max(0.0) as u32).min(lim);
    let left = to_px(clip[0] - x0, w);
    let top = to_px(clip[1] - y0, h);
    let right = to_px(clip[2] - x0, w);
    let bottom = to_px(clip[3] - y0, h);
    if right <= left || bottom <= top {
        return None;
    }
    let (cw, ch) = (right - left, bottom - top);
    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for row in top..bottom {
        let start = ((row * w + left) * 4) as usize;
        out.extend_from_slice(&pixels[start..start + (cw * 4) as usize]);
    }
    #[allow(clippy::cast_precision_loss)]
    Some((x0 + left as f32, y0 + top as f32, cw, ch, out))
}

/// Full blit path: soft fonts + optional Doom picture cache for [`DrawCmd::Patch`].
pub fn blit_draw_list_full(
    list: &UiDrawList,
    fb_w: f32,
    fb_h: f32,
    fonts: Option<&FontRegistry>,
    patches: Option<&PatchCache>,
    sink: &mut impl CanvasSink,
) {
    let sx = fb_w / list.virtual_w;
    let sy = fb_h / list.virtual_h;
    // `DrawCmd::Clip`, in framebuffer pixels: `[x0, y0, x1, y1]`.
    let mut clip: Option<[f32; 4]> = None;
    for cmd in &list.cmds {
        match cmd {
            DrawCmd::Clip { rect } => {
                clip = rect.map(|r| [r.x * sx, r.y * sy, (r.x + r.w) * sx, (r.y + r.h) * sy]);
            }
            DrawCmd::Fill { rect, color } => {
                sink.fill_rect(
                    rect.x * sx,
                    rect.y * sy,
                    rect.w * sx,
                    rect.h * sy,
                    [color.r, color.g, color.b, color.a],
                );
            }
            DrawCmd::Text {
                font,
                pos,
                text,
                color,
                scale,
            } => {
                if let Some(soft) = fonts.and_then(|r| r.get(font)) {
                    let px = pos.x * sx;
                    let py = pos.y * sy;
                    // Match virtual→fb axes independently. Using `min(sx,sy)` for
                    // glyph advance while positions use full `sx` crushed
                    // BigFont/SmallFont on widescreen (overlapping QUALITY etc.).
                    soft.paint_text_xy(
                        text,
                        px,
                        py,
                        (*scale) * sx,
                        (*scale) * sy,
                        *color,
                        |x, y, w, h, rgba| {
                            sink.fill_rect(x, y, w, h, rgba);
                        },
                    );
                } else {
                    sink.text(
                        pos.x * sx,
                        pos.y * sy,
                        text,
                        [color.r, color.g, color.b, color.a],
                    );
                }
            }
            DrawCmd::Patch {
                name,
                pos,
                size,
                apply_offsets,
                inverse,
                light,
                fuzz,
                flip,
                mirror,
                angle,
                pivot_x,
                pivot_y,
                tint,
                alpha,
                recolor,
                corners,
            } => {
                if let Some(cache) = patches {
                    if let Some(patch) = cache.get(name) {
                        let pos = if *apply_offsets {
                            Vec2::new(
                                pos.x - f32::from(patch.left_offset),
                                pos.y - f32::from(patch.top_offset),
                            )
                        } else {
                            *pos
                        };
                        let dest_w = if size.x > 0.0 {
                            size.x
                        } else {
                            f32::from(patch.width)
                        };
                        let dest_h = if size.y > 0.0 {
                            size.y
                        } else {
                            f32::from(patch.height)
                        };
                        let mut scale_x = (dest_w / f32::from(patch.width).max(1.0)) * sx;
                        let mut scale_y = (dest_h / f32::from(patch.height).max(1.0)) * sy;
                        // Match SoftPatch::paint: near-uniform keeps Doom aspect.
                        if (scale_x - scale_y).abs() <= scale_x.max(scale_y) * 0.02 {
                            let s = scale_x.min(scale_y);
                            scale_x = s;
                            scale_y = s;
                        }
                        let inverted;
                        let palette = if *inverse {
                            inverted = inverse_palette(cache.palette());
                            &inverted
                        } else {
                            cache.palette()
                        };
                        let origin_x = pos.x * sx;
                        let origin_y = pos.y * sy;
                        let x0 = origin_x.floor();
                        let y0 = origin_y.floor();
                        let x1 = (origin_x + f32::from(patch.width) * scale_x).ceil();
                        let y1 = (origin_y + f32::from(patch.height) * scale_y).ceil();
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let dw = (x1 - x0).max(1.0) as u32;
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let dh = (y1 - y0).max(1.0) as u32;
                        let mut pixels = patch.scale_rgba(dw, dh, palette);
                        let lit = light.clamp(0.0, 1.0);
                        if *fuzz {
                            // Vanilla `R_DrawFuzzColumn`: ±1 row jitter via
                            // `fuzzoffset[50]`, then colormap row 6 (~darken).
                            apply_psprite_fuzz(&mut pixels, dw, dh);
                        } else if lit < 0.999 {
                            for px in pixels.chunks_exact_mut(4) {
                                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                                {
                                    px[0] = (f32::from(px[0]) * lit) as u8;
                                    px[1] = (f32::from(px[1]) * lit) as u8;
                                    px[2] = (f32::from(px[2]) * lit) as u8;
                                }
                            }
                        }
                        apply_rgb_tint(&mut pixels, *tint);
                        if let Some(ramp) = recolor {
                            apply_recolor(&mut pixels, ramp);
                        }
                        if *alpha < 0.999 {
                            for px in pixels.chunks_exact_mut(4) {
                                px[3] = (f32::from(px[3]) * alpha.clamp(0.0, 1.0)) as u8;
                            }
                        }
                        if *flip {
                            mirror_rgba_rows(&mut pixels, dw);
                        }
                        if *mirror {
                            mirror_rgba_cols(&mut pixels, dw, dh);
                        }
                        let warped = corners.iter().any(|c| c.x.abs() > 1e-3 || c.y.abs() > 1e-3);
                        if warped {
                            let offs = corners.map(|c| (c.x * sx, c.y * sy));
                            let (out, qw, qh, ox, oy) =
                                warp_rgba(&pixels, dw, dh, offs, *angle, *pivot_x, *pivot_y);
                            sink.blit_rgba_owned(x0 + ox, y0 + oy, qw, qh, out);
                        } else if angle.abs() > 0.01 {
                            let (rotated, rw, rh, ox, oy) =
                                rotate_rgba(&pixels, dw, dh, *angle, *pivot_x, *pivot_y);
                            sink.blit_rgba_owned(x0 + ox, y0 + oy, rw, rh, rotated);
                        } else if let Some([cx0, cy0, cx1, cy1]) = clip {
                            if let Some((bx, by, bw, bh, cropped)) =
                                crop_rgba(&pixels, x0, y0, dw, dh, [cx0, cy0, cx1, cy1])
                            {
                                sink.blit_rgba_owned(bx, by, bw, bh, cropped);
                            }
                        } else {
                            sink.blit_rgba_owned(x0, y0, dw, dh, pixels);
                        }
                        continue;
                    }
                }
                // Not in the patch cache: the shells prefetch every patch a
                // draw list names before blitting, so this lump doesn't exist.
                // Draw nothing, like GZDoom (a grey box used to show here).
            }
            DrawCmd::FlatPoly {
                edges,
                flat,
                tex,
                light,
            } => {
                let Some(cache) = patches else {
                    continue;
                };
                let Some(patch) = cache.get_flat(flat) else {
                    continue;
                };
                let fb_clip = clip.unwrap_or([0.0, 0.0, fb_w, fb_h]);
                if let Some((x0, y0, w, h, px)) = raster_flat_poly(
                    edges,
                    patch,
                    cache.palette(),
                    *tex,
                    *light,
                    (sx, sy),
                    [
                        fb_clip[0].max(0.0),
                        fb_clip[1].max(0.0),
                        fb_clip[2].min(fb_w),
                        fb_clip[3].min(fb_h),
                    ],
                ) {
                    sink.blit_rgba_owned(x0, y0, w, h, px);
                }
            }
            DrawCmd::Line {
                a,
                b,
                color,
                thickness,
            } => {
                // Proper segment (AABB made every diagonal a solid blob — unusable for automap).
                let x0 = a.x * sx;
                let y0 = a.y * sy;
                let x1 = b.x * sx;
                let y1 = b.y * sy;
                let t = (*thickness * sx.max(sy)).max(1.0);
                let rgba = [color.r, color.g, color.b, color.a];
                stroke_line(sink, x0, y0, x1, y1, t, rgba);
            }
        }
    }
}

/// Rasterize a [`DrawCmd::FlatPoly`] (even-odd fill of `edges`, scaled to
/// the framebuffer by `scale`, clipped to `clip` `[x0, y0, x1, y1]`).
/// Returns the covered rect and its RGBA pixels.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn raster_flat_poly(
    edges: &[[Vec2; 2]],
    patch: &crate::SoftPatch,
    palette: &[[u8; 3]; 256],
    tex: [f32; 6],
    light: f32,
    (sx, sy): (f32, f32),
    clip: [f32; 4],
) -> Option<(f32, f32, u32, u32, Vec<u8>)> {
    let fb: Vec<[(f32, f32); 2]> = edges
        .iter()
        .map(|[a, b]| [(a.x * sx, a.y * sy), (b.x * sx, b.y * sy)])
        .collect();
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for [a, b] in &fb {
        for p in [a, b] {
            minx = minx.min(p.0);
            miny = miny.min(p.1);
            maxx = maxx.max(p.0);
            maxy = maxy.max(p.1);
        }
    }
    let x0 = minx.max(clip[0]).floor();
    let y0 = miny.max(clip[1]).floor();
    let x1 = maxx.min(clip[2]).ceil();
    let y1 = maxy.min(clip[3]).ceil();
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let mut out = vec![0u8; (w * h * 4) as usize];
    let lit = light.clamp(0.0, 1.0);
    let mut xs: Vec<f32> = Vec::new();
    for row in 0..h {
        let py = y0 + row as f32 + 0.5;
        xs.clear();
        for [a, b] in &fb {
            if (a.1 <= py) != (b.1 <= py) {
                xs.push(a.0 + (py - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
        xs.sort_by(f32::total_cmp);
        for span in xs.chunks_exact(2) {
            let sa = (span[0] - x0 - 0.5).ceil().max(0.0) as u32;
            let sb = ((span[1] - x0 - 0.5).floor() as i64).min(i64::from(w) - 1);
            if sb < i64::from(sa) {
                continue;
            }
            for col in sa..=sb as u32 {
                // Back to virtual pixels for the texture map.
                let vx = (x0 + col as f32 + 0.5) / sx;
                let vy = py / sy;
                let u = tex[0] * vx + tex[1] * vy + tex[2];
                let v = tex[3] * vx + tex[4] * vy + tex[5];
                if let Some(p) = patch.texel_wrapped(u.floor() as i32, v.floor() as i32, palette) {
                    let o = ((row * w + col) * 4) as usize;
                    out[o] = (f32::from(p[0]) * lit) as u8;
                    out[o + 1] = (f32::from(p[1]) * lit) as u8;
                    out[o + 2] = (f32::from(p[2]) * lit) as u8;
                    out[o + 3] = p[3];
                }
            }
        }
    }
    Some((x0, y0, w, h, out))
}

/// GZDoom font translation: map each pixel's luminance (normalised over the
/// patch) onto the `[dark, bright]` colour ramp.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn apply_recolor(pixels: &mut [u8], ramp: &[[f32; 3]; 2]) {
    let lum =
        |p: &[u8]| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for p in pixels.chunks_exact(4).filter(|p| p[3] > 0) {
        let l = lum(p);
        lo = lo.min(l);
        hi = hi.max(l);
    }
    if lo > hi {
        return;
    }
    let span = (hi - lo).max(1.0);
    for p in pixels.chunks_exact_mut(4) {
        if p[3] == 0 {
            continue;
        }
        // Keep some shading even on flat glyphs: 0.35..1 of the ramp.
        let t = 0.35 + 0.65 * ((lum(p) - lo) / span);
        for c in 0..3 {
            let v = ramp[0][c] + (ramp[1][c] - ramp[0][c]) * t;
            p[c] = (v.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
}

/// Multiply RGB by a 0–1 tint. White leaves the pixels alone.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn apply_rgb_tint(pixels: &mut [u8], tint: [f32; 3]) {
    if tint[0] > 0.999 && tint[1] > 0.999 && tint[2] > 0.999 {
        return;
    }
    for px in pixels.chunks_exact_mut(4) {
        px[0] = (f32::from(px[0]) * tint[0].clamp(0.0, 1.0)) as u8;
        px[1] = (f32::from(px[1]) * tint[1].clamp(0.0, 1.0)) as u8;
        px[2] = (f32::from(px[2]) * tint[2].clamp(0.0, 1.0)) as u8;
    }
}

/// Rotate tightly packed RGBA around a fractional pivot.
/// Positive degrees are counter-clockwise on a y-down screen.
/// Returns the new buffer, its size, and the top-left shift from the old origin.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
/// Draw `src` (`width`×`height`) on the quad its corners make after moving
/// them by `offs` (top-left, bottom-left, top-right, bottom-right, as UZDoom
/// `A_OverlayVertexOffset`) and turning them `degrees` counter-clockwise about
/// the pivot. Mapped like the hardware renderer: two triangles,
/// (TL, BL, TR) and (BL, TR, BR), each affine. Returns the pixels, their size
/// and their offset from the unwarped top-left.
fn warp_rgba(
    src: &[u8],
    width: u32,
    height: u32,
    offs: [(f32, f32); 4],
    degrees: f32,
    pivot_x: f32,
    pivot_y: f32,
) -> (Vec<u8>, u32, u32, f32, f32) {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    let base = [(0.0, 0.0), (0.0, h), (w, 0.0), (w, h)];
    let uv = [(0.0f32, 0.0f32), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)];
    let (px, py) = (pivot_x * w, pivot_y * h);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let mut v = [(0.0f32, 0.0f32); 4];
    for i in 0..4 {
        let dx = base[i].0 + offs[i].0 - px;
        let dy = base[i].1 + offs[i].1 - py;
        v[i] = (px + dx * cos + dy * sin, py - dx * sin + dy * cos);
    }
    let min_x = v.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor();
    let min_y = v.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor();
    let max_x = v.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil();
    let max_y = v.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let dw = (max_x - min_x).clamp(1.0, 4096.0) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let dh = (max_y - min_y).clamp(1.0, 4096.0) as u32;
    let mut dst = vec![0u8; dw as usize * dh as usize * 4];
    let tris = [[0usize, 1, 2], [1, 2, 3]];
    for j in 0..dh {
        for i in 0..dw {
            let p = (min_x + i as f32 + 0.5, min_y + j as f32 + 0.5);
            for t in &tris {
                let (a, b, c) = (v[t[0]], v[t[1]], v[t[2]]);
                let den = (b.1 - c.1) * (a.0 - c.0) + (c.0 - b.0) * (a.1 - c.1);
                if den.abs() < 1e-6 {
                    continue;
                }
                let l0 = ((b.1 - c.1) * (p.0 - c.0) + (c.0 - b.0) * (p.1 - c.1)) / den;
                let l1 = ((c.1 - a.1) * (p.0 - c.0) + (a.0 - c.0) * (p.1 - c.1)) / den;
                let l2 = 1.0 - l0 - l1;
                let eps = -1e-4;
                if l0 < eps || l1 < eps || l2 < eps {
                    continue;
                }
                let u = l0 * uv[t[0]].0 + l1 * uv[t[1]].0 + l2 * uv[t[2]].0;
                let vv = l0 * uv[t[0]].1 + l1 * uv[t[1]].1 + l2 * uv[t[2]].1;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let sx = ((u * w) as u32).min(width.max(1) - 1);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let sy = ((vv * h) as u32).min(height.max(1) - 1);
                let si = (sy as usize * width as usize + sx as usize) * 4;
                let di = (j as usize * dw as usize + i as usize) * 4;
                if let (Some(s), Some(d)) = (src.get(si..si + 4), dst.get_mut(di..di + 4)) {
                    d.copy_from_slice(s);
                }
                break;
            }
        }
    }
    (dst, dw, dh, min_x, min_y)
}

fn rotate_rgba(
    src: &[u8],
    width: u32,
    height: u32,
    degrees: f32,
    pivot_x: f32,
    pivot_y: f32,
) -> (Vec<u8>, u32, u32, f32, f32) {
    let w = width.max(1);
    let h = height.max(1);
    let px = pivot_x * w as f32;
    let py = pivot_y * h as f32;
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (cx, cy) in [
        (0.0, 0.0),
        (w as f32, 0.0),
        (0.0, h as f32),
        (w as f32, h as f32),
    ] {
        let dx = cx - px;
        let dy = cy - py;
        let rx = dx * cos + dy * sin;
        let ry = -dx * sin + dy * cos;
        min_x = min_x.min(rx);
        min_y = min_y.min(ry);
        max_x = max_x.max(rx);
        max_y = max_y.max(ry);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let dw = (max_x - min_x).ceil().max(1.0) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let dh = (max_y - min_y).ceil().max(1.0) as u32;
    let mut dst = vec![0u8; dw as usize * dh as usize * 4];
    for j in 0..dh {
        for i in 0..dw {
            let rx = min_x + i as f32 + 0.5;
            let ry = min_y + j as f32 + 0.5;
            let dx = rx * cos - ry * sin;
            let dy = rx * sin + ry * cos;
            let sx = px + dx - 0.5;
            let sy = py + dy - 0.5;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let ix = sx.round() as i32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let iy = sy.round() as i32;
            if ix < 0 || iy < 0 || ix >= w as i32 || iy >= h as i32 {
                continue;
            }
            let src_i = (iy as usize * w as usize + ix as usize) * 4;
            let dst_i = (j as usize * dw as usize + i as usize) * 4;
            dst[dst_i..dst_i + 4].copy_from_slice(&src[src_i..src_i + 4]);
        }
    }
    (dst, dw, dh, px + min_x, py + min_y)
}

/// Reverse the rows of tightly packed RGBA pixels.
fn mirror_rgba_cols(pixels: &mut [u8], width: u32, height: u32) {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h < 2 {
        return;
    }
    let row = w * 4;
    for y in 0..h / 2 {
        let a = y * row;
        let b = (h - 1 - y) * row;
        for i in 0..row {
            pixels.swap(a + i, b + i);
        }
    }
}

/// Reverse each row of tightly packed RGBA pixels.
fn mirror_rgba_rows(pixels: &mut [u8], width: u32) {
    let w = width as usize;
    if w < 2 {
        return;
    }
    for row in pixels.chunks_exact_mut(w * 4) {
        for x in 0..w / 2 {
            let a = x * 4;
            let b = (w - 1 - x) * 4;
            for channel in 0..4 {
                row.swap(a + channel, b + channel);
            }
        }
    }
}

/// Thick line as stepped axis-aligned stamps along the segment.
fn stroke_line(
    sink: &mut impl CanvasSink,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    rgba: [f32; 4],
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = dx.hypot(dy);
    if len < 1e-3 {
        let half = thickness * 0.5;
        sink.fill_rect(x0 - half, y0 - half, thickness, thickness, rgba);
        return;
    }
    // Step ≈ half thickness so stamps overlap; caps stay round-ish.
    let step = (thickness * 0.5).max(1.0);
    let n = ((len / step).ceil() as i32).max(1);
    let half = thickness * 0.5;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = x0 + dx * t;
        let y = y0 + dy * t;
        sink.fill_rect(x - half, y - half, thickness, thickness, rgba);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::draw::{DrawCmd, Rgba, Vec2};
    use crate::font::{FontId, FontRegistry};
    use std::path::PathBuf;

    #[test]
    fn warp_rgba_moves_one_corner_and_stretches_the_quad() {
        // 2×2: red, green / blue, white. Pull the top-right corner 4 right.
        let src = [
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let offs = [(0.0, 0.0), (0.0, 0.0), (4.0, 0.0), (0.0, 0.0)];
        let (dst, dw, dh, ox, oy) = warp_rgba(&src, 2, 2, offs, 0.0, 0.5, 0.5);
        assert_eq!((dw, dh, ox, oy), (6, 2, 0.0, 0.0));
        let px =
            |x: usize, y: usize| &dst[(y * dw as usize + x) * 4..(y * dw as usize + x) * 4 + 4];
        assert_eq!(px(0, 0), &[255, 0, 0, 255]);
        // The top row now reaches past x = 3 in green; the slanted right
        // edge leaves the far corner and the bottom right empty.
        assert_eq!(px(3, 0), &[0, 255, 0, 255]);
        assert_eq!(px(5, 0)[3], 0, "outside the quad");
        assert_eq!(px(5, 1)[3], 0, "outside the quad");
        assert_eq!(px(0, 1), &[0, 0, 255, 255]);
        // Untouched corners and no turn: same as the source.
        let (same, w2, h2, _, _) = warp_rgba(&src, 2, 2, [(0.0, 0.0); 4], 0.0, 0.5, 0.5);
        assert_eq!((w2, h2), (2, 2));
        assert_eq!(same, src);
    }

    #[test]
    fn rotate_rgba_sends_the_right_pixel_up() {
        // Two pixels: red on the left, green on the right. 90° CCW lifts green.
        let src = [
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
        ];
        let (dst, dw, dh, _, _) = rotate_rgba(&src, 2, 1, 90.0, 0.5, 0.5);
        assert!(dw >= 1 && dh >= 2, "rotated size {dw}x{dh}");
        let top = &dst[0..4];
        let bottom_row = (dh as usize - 1) * dw as usize * 4;
        let bottom = &dst[bottom_row..bottom_row + 4];
        assert_eq!(top, &[0, 255, 0, 255], "top should be the right-hand green");
        assert_eq!(
            bottom,
            &[255, 0, 0, 255],
            "bottom should be the left-hand red"
        );
    }

    #[test]
    fn mirror_rgba_cols_swaps_the_top_and_bottom() {
        let mut pixels = [
            255, 0, 0, 255, // top red
            0, 255, 0, 255, // bottom green
        ];
        mirror_rgba_cols(&mut pixels, 1, 2);
        assert_eq!(&pixels[0..4], &[0, 255, 0, 255]);
        assert_eq!(&pixels[4..8], &[255, 0, 0, 255]);
    }

    #[test]
    fn green_tint_keeps_green_and_dims_red() {
        let mut pixels = [200, 200, 200, 255];
        apply_rgb_tint(&mut pixels, [0.5, 1.0, 0.25]);
        assert_eq!(pixels[0], 100);
        assert_eq!(pixels[1], 200);
        assert_eq!(pixels[2], 50);
    }

    #[test]
    fn mirror_rgba_rows_swaps_the_ends_of_a_row() {
        let mut pixels = [
            1, 0, 0, 255, //
            2, 0, 0, 255, //
            3, 0, 0, 255,
        ];
        mirror_rgba_rows(&mut pixels, 3);
        assert_eq!(pixels[0], 3);
        assert_eq!(pixels[8], 1);
        assert_eq!(pixels[4], 2);
    }

    #[test]
    fn inverse_palette_is_inverted_greyscale() {
        let mut pal = [[0u8; 3]; 256];
        pal[1] = [255, 255, 255];
        pal[2] = [255, 0, 0];
        let inv = inverse_palette(&pal);
        assert_eq!(inv[0], [255, 255, 255]);
        assert_eq!(inv[1], [0, 0, 0]);
        assert_eq!(inv[2], [179, 179, 179]);
    }

    #[test]
    fn inverse_patch_paints_through_inverted_palette() {
        struct Colors(Vec<[f32; 4]>);
        impl CanvasSink for Colors {
            fn fill_rect(&mut self, _: f32, _: f32, _: f32, _: f32, c: [f32; 4]) {
                self.0.push(c);
            }
            fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {}
        }
        let mut cache = PatchCache::default();
        cache.insert(
            "GUN",
            crate::patches::SoftPatch {
                width: 1,
                height: 1,
                left_offset: 0,
                top_offset: 0,
                indices: vec![0],
                alpha: vec![255],
                rgba: None,
            },
        );
        let mut list = UiDrawList::new();
        list.patch("GUN", Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0));
        list.invert_since(0);
        let mut sink = Colors(Vec::new());
        blit_draw_list_full(&list, 320.0, 200.0, None, Some(&cache), &mut sink);
        assert_eq!(sink.0.len(), 1);
        let [r, g, b, _] = sink.0[0];
        assert!(r == g && g == b, "greyscale: {:?}", sink.0[0]);
        assert!(r > 0.9, "black palette index 0 inverts to white");
    }

    struct CountSink {
        fills: usize,
        texts: usize,
    }
    impl CanvasSink for CountSink {
        fn fill_rect(&mut self, _: f32, _: f32, _: f32, _: f32, _: [f32; 4]) {
            self.fills += 1;
        }
        fn text(&mut self, _: f32, _: f32, _: &str, _: [f32; 4]) {
            self.texts += 1;
        }
    }

    #[test]
    fn softfont_text_blits_as_fills() {
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
        let reg = FontRegistry::load_doom_fonts(&vfs);
        let font = reg.get(FontId::BIG).unwrap();
        let mut list = UiDrawList::new();
        font.draw(
            &mut list,
            FontId::BIG,
            Vec2::new(10.0, 10.0),
            "A",
            Rgba::WHITE,
            1.0,
        );
        let mut sink = CountSink { fills: 0, texts: 0 };
        blit_draw_list_with_fonts(&list, 320.0, 200.0, Some(&reg), &mut sink);
        assert!(sink.fills > 0, "expected glyph pixel fills");
        assert_eq!(sink.texts, 0, "soft font should not use text fallback");
    }

    #[test]
    fn fullscreen_hud_blits_visible_sttnum_pixels() {
        use crate::hud::HudState;
        use crate::patches::PatchCache;
        use crate::statusbar::{DoomStatusBar, StatusBar};

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
        let mut patches = PatchCache::from_vfs(&vfs);
        let mut hud = HudState::doom_default();
        hud.fullscreen = true;
        hud.health = 100;
        hud.ammo = 50;
        hud.armor = 0;
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        for cmd in &list.cmds {
            if let DrawCmd::Patch { name, .. } = cmd {
                let _ = patches.ensure(&vfs, name);
            }
        }
        let mut sink = CountSink { fills: 0, texts: 0 };
        blit_draw_list_full(&list, 640.0, 400.0, None, Some(&patches), &mut sink);
        assert!(
            sink.fills > 80,
            "fullscreen HUD must paint STTNUM pixels, got {} fills",
            sink.fills
        );
        assert_eq!(
            sink.texts, 0,
            "HUD must not rely on CanvasCmd::Text fallback"
        );
    }

    #[test]
    fn classic_stbar_blits_bar_digits_and_face() {
        use crate::hud::HudState;
        use crate::patches::PatchCache;
        use crate::statusbar::{DoomStatusBar, StatusBar};

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
        let mut patches = PatchCache::from_vfs(&vfs);
        let hud = HudState::doom_default();
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        for cmd in &list.cmds {
            if let DrawCmd::Patch { name, .. } = cmd {
                let _ = patches.ensure(&vfs, name);
            }
        }
        assert!(patches.get("STBAR").is_some());
        assert!(patches.get("STTNUM1").is_some());
        assert!(patches.get("STFST00").is_some());
        let mut sink = CountSink { fills: 0, texts: 0 };
        blit_draw_list_full(&list, 640.0, 400.0, None, Some(&patches), &mut sink);
        assert!(
            sink.fills > 400,
            "classic STBAR must paint bar + digits + face, got {} fills",
            sink.fills
        );
    }

    #[test]
    fn freedoom_pistol_flash_uses_leftoffset() {
        use crate::patches::PatchCache;
        use crate::weapon::{draw_weapon, psprite_pos};

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
        let mut patches = PatchCache::from_vfs(&vfs);
        let gun = patches.ensure(&vfs, "PISGA0").cloned().unwrap();
        let flash = patches.ensure(&vfs, "PISFA0").cloned().unwrap();
        let g = psprite_pos(&gun, 0.0, 0.0, false);
        let f = psprite_pos(&flash, 0.0, 0.0, false);
        let dx = f.x - g.x;
        let expect = f32::from(gun.left_offset) - f32::from(flash.left_offset);
        assert!(
            (dx - expect).abs() < 0.5,
            "flash dx={dx} leftoverset delta={expect} gun_lo={} flash_lo={}",
            gun.left_offset,
            flash.left_offset
        );
        assert!(
            f.x > g.x && f.x < g.x + f32::from(gun.width),
            "flash x={} must sit on gun [{}, {}]",
            f.x,
            g.x,
            g.x + f32::from(gun.width)
        );
        let mut list = UiDrawList::new();
        draw_weapon(&mut list, "PISGA0", false, Some(&gun), 0.0, 0.0);
        draw_weapon(&mut list, "PISFA0", false, Some(&flash), 0.0, 0.0);
        match &list.cmds[..] {
            [DrawCmd::Patch { pos: a, .. }, DrawCmd::Patch { pos: b, .. }] => {
                assert!((a.x - g.x).abs() < 1e-3);
                assert!((b.x - f.x).abs() < 1e-3);
            }
            other => panic!("expected gun+flash patches, got {other:?}"),
        }
    }

    #[test]
    fn pause_menu_blits_visible_bigfont_pixels() {
        use crate::font::FontRegistry;
        use crate::menu::ListMenu;
        use crate::patches::PatchCache;

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
        let fonts = FontRegistry::load_doom_fonts(&vfs);
        let mut patches = PatchCache::from_vfs(&vfs);
        let mut list = UiDrawList::new();
        ListMenu::pause().draw(&mut list, 0, Some(&fonts));
        for cmd in &list.cmds {
            if let DrawCmd::Patch { name, .. } = cmd {
                let _ = patches.ensure(&vfs, name);
            }
        }
        let mut sink = CountSink { fills: 0, texts: 0 };
        blit_draw_list_full(&list, 640.0, 400.0, Some(&fonts), Some(&patches), &mut sink);
        assert!(
            sink.fills > 200,
            "pause menu must paint BigFont glyphs + dim, got {} fills",
            sink.fills
        );
        assert_eq!(sink.texts, 0, "BigFont must not fall back to canvas text");
    }
}

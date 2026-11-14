//! Styled MAP widget for mods (`docs/28` BE2): a clean tactical map rather
//! than the Doom automap look.
//!
//! - Walls are light outlines, ledges dimmer, steps and doors faint; lines
//!   with no height change are left out, so rooms read as shapes.
//! - Everything is clipped to the widget frame.
//! - Zone 0 tints the area outside its circle red; rings are drawn on top
//!   (next circle dashed).
//! - Markers are framed squares; the viewer is a white arrow.
//!
//! Line colours are opaque: the canvas blit stamps line segments, so a
//! translucent colour would show the overlaps as dots.

use crate::automap::line_hidden;
use crate::draw::{DrawCmd, Rect, Rgba, UiDrawList, Vec2};
use room_map::MapGeometry;
use room_sim::mod_zones::{ModZones, flags as zone_flags};

const ML_SECRET: i16 = 32;
const ML_MAPPED: i16 = 0x0100;
const AML_NOT_MAPPED: i32 = 8;
/// Floor step a player cannot walk up (Doom max step is 24).
const LEDGE: f64 = 24.0;

const WALL: Rgba = Rgba::new(0.84, 0.88, 0.93, 1.0);
const LEDGE_COLOR: Rgba = Rgba::new(0.47, 0.53, 0.62, 1.0);
const STEP: Rgba = Rgba::new(0.27, 0.31, 0.37, 1.0);
const FRAME: Rgba = Rgba::new(0.30, 0.35, 0.43, 1.0);
const SHADOW: Rgba = Rgba::new(0.02, 0.03, 0.05, 1.0);
const ARROW: Rgba = Rgba::new(1.0, 1.0, 1.0, 1.0);

/// Line thickness in virtual pixels (the blit scales it with the screen).
const WALL_T: f32 = 0.34;
const THIN_T: f32 = 0.24;

/// Projection from map units to the widget, with optional rotation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MapProj {
    pub center_x: f64,
    pub center_y: f64,
    pub scale: f32,
    pub rect: Rect,
    /// Viewer yaw in degrees (Doom: 0 = east).
    pub angle_deg: f32,
    /// Turn the map so the viewer's facing points up.
    pub rotate: bool,
}

impl MapProj {
    pub fn project(&self, x: f64, y: f64) -> Vec2 {
        let mut dx = x - self.center_x;
        let mut dy = y - self.center_y;
        if self.rotate {
            // Viewer's facing points up.
            let rad = f64::from(90.0 - self.angle_deg).to_radians();
            let (s, c) = (rad.sin(), rad.cos());
            let rx = dx * c - dy * s;
            let ry = dx * s + dy * c;
            dx = rx;
            dy = ry;
        }
        Vec2::new(
            self.rect.x + self.rect.w * 0.5 + dx as f32 * self.scale,
            self.rect.y + self.rect.h * 0.5 - dy as f32 * self.scale,
        )
    }
}

/// Liang–Barsky clip of `a`–`b` to `r`.
pub(crate) fn clip_segment(a: Vec2, b: Vec2, r: Rect) -> Option<(Vec2, Vec2)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    for (p, q) in [
        (-dx, a.x - r.x),
        (dx, r.x + r.w - a.x),
        (-dy, a.y - r.y),
        (dy, r.y + r.h - a.y),
    ] {
        if p.abs() < 1e-6 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            if t > t1 {
                return None;
            }
            t0 = t0.max(t);
        } else {
            if t < t0 {
                return None;
            }
            t1 = t1.min(t);
        }
    }
    Some((
        Vec2::new(a.x + dx * t0, a.y + dy * t0),
        Vec2::new(a.x + dx * t1, a.y + dy * t1),
    ))
}

fn line(list: &mut UiDrawList, a: Vec2, b: Vec2, r: Rect, color: Rgba, thickness: f32) {
    if let Some((a, b)) = clip_segment(a, b, r) {
        list.cmds.push(DrawCmd::Line {
            a,
            b,
            color,
            thickness,
        });
    }
}

/// Mix `c` over the widget background so line colours stay opaque.
fn over(c: Rgba, bg: Rgba, alpha: f32) -> Rgba {
    let k = alpha.clamp(0.0, 1.0);
    Rgba::new(
        bg.r + (c.r - bg.r) * k,
        bg.g + (c.g - bg.g) * k,
        bg.b + (c.b - bg.b) * k,
        bg.a.max(k),
    )
}

/// Frame + background.
pub(crate) fn draw_frame(list: &mut UiDrawList, r: Rect, bg: Rgba, alpha: f32) {
    let shadow = Rgba::new(SHADOW.r, SHADOW.g, SHADOW.b, 0.55 * alpha);
    list.fill(Rect::new(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0), shadow);
    list.fill(r, bg);
    let f = Rgba::new(FRAME.r, FRAME.g, FRAME.b, alpha);
    let t = 0.5;
    list.fill(Rect::new(r.x, r.y, r.w, t), f);
    list.fill(Rect::new(r.x, r.y + r.h - t, r.w, t), f);
    list.fill(Rect::new(r.x, r.y, t, r.h), f);
    list.fill(Rect::new(r.x + r.w - t, r.y, t, r.h), f);
}

/// Level outlines. `reveal`: every line, not only the explored ones.
pub(crate) fn draw_geometry(
    map: &MapGeometry,
    proj: &MapProj,
    list: &mut UiDrawList,
    bg: Rgba,
    alpha: f32,
    reveal: bool,
) {
    let wall = over(WALL, bg, alpha);
    let ledge = over(LEDGE_COLOR, bg, alpha);
    let step = over(STEP, bg, alpha);
    // Draw faint lines first so walls sit on top.
    let mut walls: Vec<(Vec2, Vec2)> = Vec::new();
    for l in &map.lines {
        if l.automap_style == AML_NOT_MAPPED || line_hidden(map, l) {
            continue;
        }
        if !reveal && (l.flags & ML_MAPPED) == 0 {
            continue;
        }
        let (Some(v1), Some(v2)) = (
            map.vertices.get(l.v1.raw() as usize),
            map.vertices.get(l.v2.raw() as usize),
        ) else {
            continue;
        };
        let a = proj.project(v1.pos.x(), v1.pos.y());
        let b = proj.project(v2.pos.x(), v2.pos.y());
        let front = l.front_sector.and_then(|s| map.sectors.get(s.raw() as usize));
        let back = l.back_sector.and_then(|s| map.sectors.get(s.raw() as usize));
        let (Some(fs), Some(bs)) = (front, back) else {
            walls.push((a, b));
            continue;
        };
        if (l.flags & ML_SECRET) != 0 {
            walls.push((a, b));
            continue;
        }
        let floor = (fs.floor_height - bs.floor_height).abs();
        let closed = fs.ceiling_height <= bs.floor_height || bs.ceiling_height <= fs.floor_height
            || fs.ceiling_height <= fs.floor_height
            || bs.ceiling_height <= bs.floor_height;
        if floor > LEDGE {
            line(list, a, b, proj.rect, ledge, THIN_T);
        } else if floor > 0.5 || closed || (fs.ceiling_height - bs.ceiling_height).abs() > 0.5 {
            line(list, a, b, proj.rect, step, THIN_T);
        }
    }
    for (a, b) in walls {
        line(list, a, b, proj.rect, wall, WALL_T);
    }
}

fn rgba_u32(c: u32) -> Rgba {
    Rgba::new(
        ((c >> 24) & 0xff) as f32 / 255.0,
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        (c & 0xff) as f32 / 255.0,
    )
}

fn ring(list: &mut UiDrawList, proj: &MapProj, cx: f64, cy: f64, r: f64, color: Rgba, t: f32, dashed: bool) {
    if r <= 0.0 {
        return;
    }
    let px = r as f32 * proj.scale;
    let n = ((px * 0.9) as usize).clamp(24, 160);
    let n = if dashed { n & !1 } else { n };
    let at = |i: usize| {
        let a = std::f64::consts::TAU * i as f64 / n as f64;
        proj.project(cx + r * a.cos(), cy + r * a.sin())
    };
    let mut prev = at(0);
    for i in 1..=n {
        let p = at(i);
        if !dashed || i % 2 == 1 {
            line(list, prev, p, proj.rect, color, t);
        }
        prev = p;
    }
}

/// Zone tint outside zone 0, then every visible zone's rings.
pub(crate) fn draw_zones(
    zones: &ModZones,
    maptime: u32,
    ticfrac_milli: u32,
    proj: &MapProj,
    list: &mut UiDrawList,
    bg: Rgba,
    alpha: f32,
) {
    let r = proj.rect;
    if let Some(z) = zones.get(0).filter(|z| z.flags & zone_flags::HIDDEN == 0) {
        let (cx, cy, rad) = z.interp_current(maptime, ticfrac_milli);
        let base = rgba_u32(z.rgba);
        let tint = Rgba::new(base.r, base.g * 0.5, base.b * 0.5, 0.30 * alpha);
        let c = proj.project(f64::from(cx), f64::from(cy));
        let pr = rad.max(0) as f32 * proj.scale;
        let strip: f32 = 0.5;
        let mut y = r.y;
        while y < r.y + r.h {
            let h = strip.min(r.y + r.h - y);
            let dy = y + h * 0.5 - c.y;
            if dy.abs() >= pr {
                list.fill(Rect::new(r.x, y, r.w, h), tint);
            } else {
                let half = (pr * pr - dy * dy).sqrt();
                let (x0, x1) = ((c.x - half).max(r.x), (c.x + half).min(r.x + r.w));
                if x0 > r.x {
                    list.fill(Rect::new(r.x, y, (x0 - r.x).min(r.w), h), tint);
                }
                if x1 < r.x + r.w {
                    let from = x1.max(r.x);
                    list.fill(Rect::new(from, y, r.x + r.w - from, h), tint);
                }
            }
            y += strip;
        }
    }
    for z in zones.iter() {
        if z.flags & zone_flags::HIDDEN != 0 {
            continue;
        }
        let color = over(rgba_u32(z.rgba | 0xff), bg, alpha);
        let (cx, cy, rad) = z.interp_current(maptime, ticfrac_milli);
        if z.next_r > 0 {
            let next = over(Rgba::new(1.0, 1.0, 1.0, 1.0), bg, 0.85 * alpha);
            ring(
                list,
                proj,
                f64::from(z.next_cx),
                f64::from(z.next_cy),
                f64::from(z.next_r),
                next,
                THIN_T,
                true,
            );
        }
        ring(list, proj, f64::from(cx), f64::from(cy), f64::from(rad), color, 0.45, false);
    }
}

/// Framed square marker; `false` when outside the widget.
pub(crate) fn draw_marker(list: &mut UiDrawList, r: Rect, p: Vec2, size: f32, color: Rgba) -> bool {
    if p.x < r.x || p.y < r.y || p.x > r.x + r.w || p.y > r.y + r.h {
        return false;
    }
    let o = size * 0.5 + 0.5;
    list.fill(
        Rect::new(p.x - o, p.y - o, o * 2.0, o * 2.0),
        Rgba::new(SHADOW.r, SHADOW.g, SHADOW.b, color.a),
    );
    let h = size * 0.5;
    list.fill(Rect::new(p.x - h, p.y - h, size, size), color);
    true
}

/// Viewer arrow at `p`, pointing along `angle_deg` on screen (0 = east).
pub(crate) fn draw_arrow(list: &mut UiDrawList, r: Rect, p: Vec2, screen_angle_deg: f32, size: f32) {
    let at = |deg: f32, len: f32| {
        let a = deg.to_radians();
        Vec2::new(p.x + a.cos() * len, p.y - a.sin() * len)
    };
    let tip = at(screen_angle_deg, size);
    let left = at(screen_angle_deg + 140.0, size * 0.7);
    let right = at(screen_angle_deg - 140.0, size * 0.7);
    let back = at(screen_angle_deg + 180.0, size * 0.25);
    for (t, c) in [(0.9, SHADOW), (0.45, ARROW)] {
        for (a, b) in [(left, tip), (tip, right), (right, back), (back, left)] {
            line(list, a, b, r, c, t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_segments_to_the_frame() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let (a, b) = clip_segment(Vec2::new(-5.0, 5.0), Vec2::new(15.0, 5.0), r).unwrap();
        assert!((a.x - 0.0).abs() < 1e-4 && (b.x - 10.0).abs() < 1e-4);
        assert!(clip_segment(Vec2::new(-5.0, -5.0), Vec2::new(-1.0, -1.0), r).is_none());
    }

    #[test]
    fn rotation_puts_the_facing_up() {
        let north_up = MapProj {
            center_x: 0.0,
            center_y: 0.0,
            scale: 1.0,
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            angle_deg: 0.0,
            rotate: false,
        };
        assert!(north_up.project(0.0, 10.0).y < 45.0, "+y is up");
        let facing_east = MapProj { rotate: true, ..north_up };
        let p = facing_east.project(10.0, 0.0);
        assert!(p.y < 45.0 && (p.x - 50.0).abs() < 1e-3, "facing (east) is up: {p:?}");
    }
}

//! Mod zone rings on the automap (`docs/28` §9 BE1).

use crate::automap::AutomapView;
use crate::draw::{DrawCmd, Rgba, UiDrawList, Vec2};
use room_sim::mod_zones::{ModZones, flags};

fn project(view: &AutomapView, x: f64, y: f64) -> Vec2 {
    let dx = (x - view.center_x) as f32 * view.scale;
    let dy = (view.center_y - y) as f32 * view.scale;
    Vec2::new(
        view.screen_x + view.width * 0.5 + dx,
        view.screen_y + view.height * 0.5 + dy,
    )
}

fn rgba_from_u32(c: u32) -> Rgba {
    Rgba {
        r: ((c >> 24) & 0xff) as f32 / 255.0,
        g: ((c >> 16) & 0xff) as f32 / 255.0,
        b: ((c >> 8) & 0xff) as f32 / 255.0,
        a: (c & 0xff) as f32 / 255.0,
    }
}

fn circle_lines(
    view: &AutomapView,
    cx: i32,
    cy: i32,
    radius: i32,
    color: Rgba,
    thickness: f32,
    segments: usize,
    list: &mut UiDrawList,
) {
    if radius <= 0 {
        return;
    }
    let r = f64::from(radius);
    let cx = f64::from(cx);
    let cy = f64::from(cy);
    let n = segments.max(12);
    let mut prev = project(view, cx + r, cy);
    for i in 1..=n {
        let t = std::f64::consts::TAU * (i as f64) / (n as f64);
        let p = project(view, cx + r * t.cos(), cy + r * t.sin());
        list.cmds.push(DrawCmd::Line {
            a: prev,
            b: p,
            color,
            thickness,
        });
        prev = p;
    }
}

/// Draw current + next zone rings into an automap widget.
pub fn draw_mod_zones_automap(
    zones: &ModZones,
    maptime: u32,
    ticfrac_milli: u32,
    view: &AutomapView,
    list: &mut UiDrawList,
) {
    for zone in zones.iter() {
        if (zone.flags & flags::HIDDEN) != 0 {
            continue;
        }
        let color = rgba_from_u32(zone.rgba);
        let (cx, cy, r) = zone.interp_current(maptime, ticfrac_milli);
        circle_lines(view, cx, cy, r, color, 2.0, 48, list);
        if zone.next_r > 0 {
            let mut next = color;
            next.a *= 0.55;
            circle_lines(
                view,
                zone.next_cx,
                zone.next_cy,
                zone.next_r,
                next,
                1.25,
                40,
                list,
            );
        }
    }
}

/// Full-screen tint when `player` is outside the primary zone (`id` 0).
pub fn draw_outside_zone_tint(
    zones: &ModZones,
    maptime: u32,
    ticfrac_milli: u32,
    player_x: f64,
    player_y: f64,
    list: &mut UiDrawList,
) {
    let Some(zone) = zones.get(0) else {
        return;
    };
    if (zone.flags & flags::HIDDEN) != 0 {
        return;
    }
    let (cx, cy, r) = zone.interp_current(maptime, ticfrac_milli);
    if r <= 0 {
        return;
    }
    let dx = player_x - f64::from(cx);
    let dy = player_y - f64::from(cy);
    if dx * dx + dy * dy <= f64::from(r * r) {
        return;
    }
    let base = rgba_from_u32(zone.rgba);
    list.fill(
        crate::draw::Rect::new(0.0, 0.0, list.virtual_w, list.virtual_h),
        Rgba {
            r: base.r,
            g: base.g,
            b: base.b,
            a: 0.22,
        },
    );
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use room_sim::mod_zones::ModZones;

    #[test]
    fn automap_emits_zone_ring_lines() {
        let mut zones = ModZones::new();
        assert_eq!(
            zones.set(0, 0, 256, 256, 128, 300, 300, 64, 0xff0000ff, 0, 1),
            0
        );
        let view = AutomapView {
            center_x: 256.0,
            center_y: 256.0,
            scale: 0.5,
            width: 160.0,
            height: 120.0,
            ..Default::default()
        };
        let mut list = UiDrawList::default();
        draw_mod_zones_automap(&zones, 1, 1000, &view, &mut list);
        let lines = list
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCmd::Line { .. }))
            .count();
        assert!(lines >= 48, "expected a full circle of segments");
    }
}

//! Retained Mod HUD drawing (`docs/27` §9 / MH1).

use crate::doom_text::{draw_stcfn_string, measure_stcfn_string};
use crate::draw::{Rect, Rgba, UiDrawList, Vec2};
use room_map::MapGeometry;
use room_sim::mod_hud::{HudWidget, ModHud, WidgetKey, flags, kind, layer, map as map_layers};
use room_sim::mod_zones::ModZones;
use room_sim::{MAX_PLAYERS, World};

/// Virtual HUD width.
const VW: f32 = 320.0;
/// Virtual HUD height.
const VH: f32 = 200.0;

/// World data for MAP widgets (BE2).
pub struct ModHudMapCtx<'a> {
    /// Level geometry (`None` → geometry layer skipped).
    pub map: Option<&'a MapGeometry>,
    /// Zone rings (BE1).
    pub zones: &'a ModZones,
    /// Map pose per seat (`x`, `y` map units, `angle_deg`).
    pub player_pose: &'a [(f64, f64, f32)],
    /// Full widget store (MARKER layer on the map).
    pub hud: &'a ModHud,
    /// Computer area map / iddt.
    pub allmap: bool,
    /// Interpolation for zone rings (milli, 0..=1000).
    pub ticfrac_milli: u32,
}

/// Draw retained mod widgets for `seat` (0-based) into `list`.
///
/// Layer order: UNDER_HUD → (caller draws status bar / messages) → OVER_HUD →
/// MODAL. Call [`draw_mod_hud_layer`] three times around the status bar, or
/// use [`draw_mod_hud`] for a single pass after the status bar (OVER+MODAL only
/// is wrong — prefer the layered helpers).
pub fn draw_mod_hud(
    hud: &ModHud,
    seat: u8,
    maptime: u32,
    ticfrac: f32,
    list: &mut UiDrawList,
    map_ctx: Option<&ModHudMapCtx<'_>>,
) {
    draw_mod_hud_layer(hud, seat, maptime, ticfrac, list, layer::UNDER_HUD, map_ctx);
    draw_mod_hud_layer(hud, seat, maptime, ticfrac, list, layer::OVER_HUD, map_ctx);
    let _ = draw_mod_hud_modals(hud, seat, maptime, ticfrac, list, map_ctx);
}

/// Draw one non-modal layer.
pub fn draw_mod_hud_layer(
    hud: &ModHud,
    seat: u8,
    maptime: u32,
    ticfrac: f32,
    list: &mut UiDrawList,
    which: u8,
    map_ctx: Option<&ModHudMapCtx<'_>>,
) {
    let mut items: Vec<(&WidgetKey, &HudWidget)> = hud
        .for_seat(seat)
        .filter(|(_, w)| w.layer == which && w.flags & flags::HIDDEN == 0)
        .collect();
    items.sort_by(|a, b| a.1.z.cmp(&b.1.z).then(a.0.id.cmp(&b.0.id)));
    for (key, w) in items {
        let alpha = widget_alpha(w, maptime, ticfrac);
        if alpha <= 0.0 {
            continue;
        }
        draw_one(list, key, w, alpha, seat, maptime, map_ctx);
    }
}

/// Dim + MODAL widgets. Returns `true` if any modal was drawn (hide crosshair).
#[must_use]
pub fn draw_mod_hud_modals(
    hud: &ModHud,
    seat: u8,
    maptime: u32,
    ticfrac: f32,
    list: &mut UiDrawList,
    map_ctx: Option<&ModHudMapCtx<'_>>,
) -> bool {
    let mut items: Vec<(&WidgetKey, &HudWidget)> = hud
        .for_seat(seat)
        .filter(|(_, w)| w.layer == layer::MODAL && w.flags & flags::HIDDEN == 0)
        .collect();
    if items.is_empty() {
        return false;
    }
    list.fill(
        Rect::new(0.0, 0.0, list.virtual_w.max(VW), list.virtual_h.max(VH)),
        Rgba::new(0.0, 0.0, 0.0, 0.45),
    );
    items.sort_by(|a, b| a.1.z.cmp(&b.1.z).then(a.0.id.cmp(&b.0.id)));
    for (key, w) in items {
        let alpha = widget_alpha(w, maptime, ticfrac);
        if alpha <= 0.0 {
            continue;
        }
        draw_one(list, key, w, alpha, seat, maptime, map_ctx);
    }
    true
}

/// True when the seat has a visible modal widget.
#[must_use]
pub fn seat_has_modal(hud: &ModHud, seat: u8) -> bool {
    hud.for_seat(seat)
        .any(|(_, w)| w.layer == layer::MODAL && w.flags & flags::HIDDEN == 0)
}

fn widget_alpha(w: &HudWidget, maptime: u32, ticfrac: f32) -> f32 {
    let base = (f32::from(w.alpha) / 1000.0).clamp(0.0, 1.0);
    let age = maptime.saturating_sub(w.created_tic) as f32 + ticfrac.clamp(0.0, 1.0);
    let fade_in = f32::from(w.fade_in);
    let hold = f32::from(w.hold);
    let fade_out = f32::from(w.fade_out);
    let mut mul = 1.0;
    if fade_in > 0.0 {
        mul = (age / fade_in).clamp(0.0, 1.0);
    }
    if hold > 0.0 {
        let after_hold = age - fade_in - hold;
        if after_hold > 0.0 {
            if fade_out > 0.0 {
                mul *= (1.0 - after_hold / fade_out).clamp(0.0, 1.0);
            } else {
                mul = 0.0;
            }
        }
    }
    base * mul
}

fn anchor_origin(anchor: u8, vw: f32, vh: f32) -> (f32, f32) {
    // 0..8: TL, TC, TR, ML, C, MR, BL, BC, BR
    let (fx, fy) = match anchor {
        0 => (0.0, 0.0),
        1 => (0.5, 0.0),
        2 => (1.0, 0.0),
        3 => (0.0, 0.5),
        4 => (0.5, 0.5),
        5 => (1.0, 0.5),
        6 => (0.0, 1.0),
        7 => (0.5, 1.0),
        _ => (1.0, 1.0),
    };
    (fx * vw, fy * vh)
}

fn widget_rect(w: &HudWidget, vw: f32, vh: f32) -> Rect {
    let (ax, ay) = anchor_origin(w.anchor, vw, vh);
    let ww = if w.w > 0 { f32::from(w.w) } else { 8.0 };
    let hh = if w.h > 0 { f32::from(w.h) } else { 8.0 };
    let mut x = ax + f32::from(w.x);
    let mut y = ay + f32::from(w.y);
    // Centre the rect on the anchor point (TEXT does this separately via measure).
    if matches!(w.anchor, 1 | 4 | 7) {
        x -= ww * 0.5;
    }
    if matches!(w.anchor, 3 | 4 | 5) {
        y -= hh * 0.5;
    }
    Rect::new(x, y, ww, hh)
}

/// TEXTCOLO index (SDK `text_color_index` order) → dark/bright font ramp.
fn text_ramp(index: u8) -> Option<[[f32; 3]; 2]> {
    let rgb = |c: u32| {
        [
            ((c >> 16) & 0xff) as f32 / 255.0,
            ((c >> 8) & 0xff) as f32 / 255.0,
            (c & 0xff) as f32 / 255.0,
        ]
    };
    let (dark, bright) = match index {
        0 => (0x3c_1010, 0xff_b8b8), // brick
        1 => (0x4a_3a20, 0xff_e6b0), // tan
        2 | 20 => (0x2a_2a2a, 0xd8_d8d8), // gray, dark gray
        3 | 16 => (0x0c_3a0c, 0x9c_ff9c), // green
        4 | 18 => (0x3a_2410, 0xd8_a060), // brown
        5 => (0x4a_2c00, 0xff_e050),  // gold
        6 | 17 => (0x3c_0000, 0xff_5050), // red
        7 | 24 => (0x00_0c4a, 0x80_a0ff), // blue
        8 | 23 => (0x4a_1c00, 0xff_a040), // orange
        9 => (0x60_6060, 0xff_ffff),  // white
        10 => (0x4a_4000, 0xff_ff60), // yellow
        13 | 22 => (0x10_3050, 0xb0_e0ff), // light blue, ice
        14 => (0x50_4430, 0xff_f4dc), // cream
        19 => (0x2c_0c3c, 0xd8_90ff), // purple
        21 | 25 => (0x00_3a3a, 0x70_ffff), // cyan, teal
        _ => return None,
    };
    Some([rgb(dark), rgb(bright)])
}

fn rgba_u32(c: u32, alpha: f32) -> Rgba {
    let a = ((c & 0xff) as f32 / 255.0) * alpha;
    Rgba::new(
        ((c >> 24) & 0xff) as f32 / 255.0,
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        a,
    )
}

/// Replace `{key:…}` tokens in widget text (`docs/27` §9.1 E10).
///
/// `resolve` maps the token body (e.g. `+use`, `slot 1`) to a display string.
/// Render-only — may differ per client without desync.
#[must_use]
pub fn expand_key_tokens(text: &str, mut resolve: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && text[i..].starts_with("{key:") {
            if let Some(end) = text[i + 5..].find('}') {
                let body = &text[i + 5..i + 5 + end];
                out.push_str(&resolve(body));
                i += 5 + end + 1;
                continue;
            }
        }
        let ch = text[i..]
            .chars()
            .next()
            .map(|c| {
                let n = c.len_utf8();
                out.push(c);
                n
            })
            .unwrap_or(1);
        i += ch;
    }
    out
}

fn draw_one(
    list: &mut UiDrawList,
    _key: &WidgetKey,
    w: &HudWidget,
    alpha: f32,
    seat: u8,
    maptime: u32,
    map_ctx: Option<&ModHudMapCtx<'_>>,
) {
    let vw = list.virtual_w.max(VW);
    let vh = list.virtual_h.max(VH);
    let rect = widget_rect(w, vw, vh);
    match w.kind {
        kind::TEXT => {
            // SoftFont tinting is unreliable on the canvas blit path (`doom_text`
            // docs); STCFN patches match the status bar / menu look.
            let text = expand_key_tokens(&w.text, |tok| tok.to_ascii_uppercase());
            let scale = match w.font {
                1 => 2.0, // BigFont → double STCFN
                _ => 1.0,
            };
            let (ax, ay) = anchor_origin(w.anchor, vw, vh);
            let x0 = ax + f32::from(w.x);
            let mut y = ay + f32::from(w.y);
            // One row per `\n`; each row is aligned on its own.
            // `align`: 1 = centre, 2 = right; otherwise follow the anchor
            // column (centre anchors centre the text).
            let line_h = 10.0 * scale;
            let start = list.cmds.len();
            for line in text.split('\n') {
                let width = measure_stcfn_string(line, scale);
                let x = match w.align & 3 {
                    1 => x0 - width * 0.5,
                    2 => x0 - width,
                    _ if matches!(w.anchor, 1 | 4 | 7) => x0 - width * 0.5,
                    _ => x0,
                };
                draw_stcfn_string(list, x, y, line, scale);
                y += line_h;
            }
            list.style_since(start, alpha, text_ramp(w.text_color));
        }
        kind::FILL => {
            list.fill(rect, rgba_u32(w.color, alpha));
        }
        kind::PANEL => {
            list.fill(rect, rgba_u32(w.color, alpha));
            let border = rgba_u32(w.color2, alpha);
            let t = 1.0;
            list.fill(Rect::new(rect.x, rect.y, rect.w, t), border);
            list.fill(Rect::new(rect.x, rect.y + rect.h - t, rect.w, t), border);
            list.fill(Rect::new(rect.x, rect.y, t, rect.h), border);
            list.fill(Rect::new(rect.x + rect.w - t, rect.y, t, rect.h), border);
        }
        kind::BOX => {
            let border = rgba_u32(w.color, alpha);
            let t = 1.0;
            list.fill(Rect::new(rect.x, rect.y, rect.w, t), border);
            list.fill(Rect::new(rect.x, rect.y + rect.h - t, rect.w, t), border);
            list.fill(Rect::new(rect.x, rect.y, t, rect.h), border);
            list.fill(Rect::new(rect.x + rect.w - t, rect.y, t, rect.h), border);
        }
        kind::BAR => {
            list.fill(rect, rgba_u32(w.color2, alpha));
            if w.max > 0 {
                let frac = (w.value as f32 / w.max as f32).clamp(0.0, 1.0);
                list.fill(
                    Rect::new(rect.x, rect.y, rect.w * frac, rect.h),
                    rgba_u32(w.color, alpha),
                );
            }
        }
        kind::IMAGE => {
            if !w.name.is_empty() {
                list.patch(
                    &w.name,
                    Vec2::new(rect.x, rect.y),
                    Vec2::new(rect.w, rect.h),
                );
            }
        }
        kind::MARKER if w.flags & flags::MAP_ONLY != 0 => {}
        kind::MARKER => {
            let (sx, sy, off) = project_marker(w, vw, vh);
            let col = rgba_u32(w.color, alpha);
            let size = if w.w > 0 { f32::from(w.w) } else { 6.0 };
            list.fill(Rect::new(sx - size * 0.5, sy - size * 0.5, size, size), col);
            if off {
                // Edge chevron: small diamond pointing inward.
                list.fill(Rect::new(sx - 2.0, sy - 6.0, 4.0, 4.0), col);
            }
            // Off screen the marker is only an edge pip; the label shows once
            // the spot is in view (a pinned "VAULT" read like a message).
            if !off && !w.text.is_empty() {
                let label = expand_key_tokens(&w.text, |tok| tok.to_ascii_uppercase());
                draw_stcfn_string(list, sx + 4.0, sy - 8.0, &label, 1.0);
            }
        }
        kind::MAP => {
            if let Some(ctx) = map_ctx {
                draw_map_widget(list, w, alpha, seat, maptime, ctx);
            }
        }
        _ => {}
    }
}

fn draw_map_widget(
    list: &mut UiDrawList,
    w: &HudWidget,
    alpha: f32,
    seat: u8,
    maptime: u32,
    ctx: &ModHudMapCtx<'_>,
) {
    use crate::mod_map::{MapProj, draw_arrow, draw_frame, draw_geometry, draw_marker, draw_zones};
    let vw = list.virtual_w.max(VW);
    let vh = list.virtual_h.max(VH);
    let rect = widget_rect(w, vw, vh);
    if rect.w <= 1.0 || rect.h <= 1.0 {
        return;
    }
    let layers = if w.max != 0 {
        w.max
    } else {
        map_layers::DEFAULT_LAYERS
    };
    let follow = map_layers::follow_seat(w.font, seat);
    let (px, py, player_angle) = player_map_pose(ctx.player_pose, follow);
    let fixed = (layers & map_layers::FIXED_CENTER) != 0;
    let (center_x, center_y) = if fixed {
        (f64::from(w.world_xyz[0]), f64::from(w.world_xyz[1]))
    } else {
        (px, py)
    };
    let rotate = !fixed && (w.flags & flags::MAP_ROTATE) != 0;
    let mut bg = rgba_u32(if w.color2 != 0 { w.color2 } else { 0x0b10_17ff }, 1.0);
    bg.a = (bg.a * alpha).max(0.0);
    draw_frame(list, rect, bg, alpha);
    let proj = MapProj {
        center_x,
        center_y,
        scale: map_layers::scale_from_value(w.value),
        rect,
        angle_deg: player_angle,
        rotate,
    };
    let reveal = ctx.allmap || (layers & map_layers::REVEAL) != 0;
    if (layers & map_layers::GEOMETRY) != 0
        && let Some(map) = ctx.map
    {
        draw_geometry(map, &proj, list, bg, alpha, reveal);
    }
    if (layers & map_layers::ZONES) != 0 {
        draw_zones(ctx.zones, maptime, ctx.ticfrac_milli, &proj, list, bg, alpha);
    }
    if (layers & map_layers::MARKERS) != 0 || (layers & map_layers::ITEMS) != 0 {
        for (_, mw) in ctx
            .hud
            .for_seat(seat)
            .filter(|(_, mw)| mw.kind == kind::MARKER && mw.flags & flags::HIDDEN == 0)
        {
            if (layers & map_layers::ITEMS) != 0 && mw.name.is_empty() {
                continue;
            }
            let p = proj.project(f64::from(mw.world_xyz[0]), f64::from(mw.world_xyz[1]));
            let size = if mw.w > 0 { f32::from(mw.w) * 0.5 } else { 2.0 };
            let col = rgba_u32(mw.color, alpha);
            if draw_marker(list, rect, p, size, col) && !mw.text.is_empty() {
                let label = expand_key_tokens(&mw.text, |tok| tok.to_ascii_uppercase());
                let lw = measure_stcfn_string(&label, 0.5);
                let lx = (p.x + size).min(rect.x + rect.w - lw - 1.0);
                let start = list.cmds.len();
                draw_stcfn_string(list, lx, p.y - 2.0, &label, 0.5);
                list.style_since(start, alpha, text_ramp(if mw.text_color == 255 { 9 } else { mw.text_color }));
            }
        }
    }
    if (layers & map_layers::HIDE_PLAYER) == 0 {
        let p = proj.project(px, py);
        let screen_angle = if rotate { 90.0 } else { player_angle };
        let size = (rect.w.min(rect.h) / 14.0).clamp(2.5, 6.0);
        if p.x >= rect.x && p.y >= rect.y && p.x <= rect.x + rect.w && p.y <= rect.y + rect.h {
            draw_arrow(list, rect, p, screen_angle, size);
        }
    }
}

fn player_map_pose(poses: &[(f64, f64, f32)], seat: u8) -> (f64, f64, f32) {
    poses
        .get(usize::from(seat))
        .copied()
        .unwrap_or((0.0, 0.0, 0.0))
}

/// Camera for MARKER projection (map XY → virtual HUD).
#[derive(Clone, Copy, Debug)]
pub struct MarkerCam {
    /// Player / view X (map units).
    pub x: f64,
    /// Player / view Y.
    pub y: f64,
    /// Facing degrees.
    pub yaw_deg: f64,
    /// Legacy radar scale (unused since markers project in perspective).
    pub scale: f64,
    /// Eye height (map units) for perspective markers.
    pub z: f64,
}

impl Default for MarkerCam {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            yaw_deg: 0.0,
            scale: 1024.0,
            z: 0.0,
        }
    }
}

thread_local! {
    static MARKER_CAM: core::cell::Cell<MarkerCam> = const { core::cell::Cell::new(MarkerCam {
        x: 0.0,
        y: 0.0,
        yaw_deg: 0.0,
        scale: 1024.0,
        z: 0.0,
    }) };
}

/// Per-seat map poses for MAP widgets (`x`, `y`, `angle_deg`).
#[must_use]
pub fn player_map_poses(world: &World) -> [(f64, f64, f32); MAX_PLAYERS] {
    let mut out = [(0.0, 0.0, 0.0); MAX_PLAYERS];
    for (i, slot) in world.players.iter().enumerate().take(MAX_PLAYERS) {
        let Some(p) = slot.as_ref() else {
            continue;
        };
        let Some(mo) = p.mo else {
            continue;
        };
        let Some(a) = world.actors.get(mo) else {
            continue;
        };
        out[i] = (a.pos.x(), a.pos.y(), a.angle.degrees() as f32);
    }
    out
}

/// Set the camera used by MARKER widgets for this frame.
pub fn set_marker_cam(cam: MarkerCam) {
    MARKER_CAM.with(|c| c.set(cam));
}

/// Project a MARKER's world point into the HUD like the 3D view does (Doom
/// angles: 0 = east, counter-clockwise; focal length 160 per 200 virtual
/// rows). Points behind the camera or off screen pin to the screen edge.
fn project_marker(w: &HudWidget, vw: f32, vh: f32) -> (f32, f32, bool) {
    let cam = MARKER_CAM.with(|c| c.get());
    let dx = f64::from(w.world_xyz[0]) - cam.x;
    let dy = f64::from(w.world_xyz[1]) - cam.y;
    let dz = f64::from(w.world_xyz[2]) + 16.0 - cam.z;
    let rad = cam.yaw_deg.to_radians();
    let (s, c) = (rad.sin(), rad.cos());
    let forward = dx * c + dy * s;
    let right = dx * s - dy * c;
    let focal = 160.0 * f64::from(vh) / 200.0;
    let (cx, cy) = (f64::from(vw) * 0.5, f64::from(vh) * 0.5);
    let margin = 8.0f32;
    let (mut sx, mut sy, mut off) = if forward > 16.0 {
        (
            (cx + right * focal / forward) as f32,
            (cy - dz * focal / forward) as f32,
            false,
        )
    } else {
        // Behind: pin to the bottom edge on the side it lies.
        let side = if right >= 0.0 { vw - margin } else { margin };
        (side, vh - margin, true)
    };
    if sx < margin || sx > vw - margin || sy < margin || sy > vh - margin {
        off = true;
        sx = sx.clamp(margin, vw - margin);
        sy = sy.clamp(margin, vh - margin);
    }
    (sx + f32::from(w.x), sy + f32::from(w.y), off)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::draw::DrawCmd;
    use room_sim::mod_hud::HudWidget;

    fn sample_panel() -> HudWidget {
        let mut w = HudWidget::default();
        w.kind = kind::PANEL;
        w.layer = layer::OVER_HUD;
        w.anchor = 4; // center
        w.x = -60;
        w.y = -40;
        w.w = 120;
        w.h = 80;
        w.color = 0x2020_40ff;
        w.color2 = 0xffff_ffff;
        w
    }

    #[test]
    fn draws_text_panel_bar_at_320x200() {
        let mut hud = ModHud::new();
        let mut text = HudWidget::default();
        text.kind = kind::TEXT;
        text.layer = layer::OVER_HUD;
        text.anchor = 1;
        text.text = "ROUND 5".into();
        text.color = 0xffff_00ff;
        assert_eq!(hud.set(0, 1, 1, text, 0), 0);
        assert_eq!(hud.set(0, 1, 2, sample_panel(), 0), 0);
        let mut bar = HudWidget::default();
        bar.kind = kind::BAR;
        bar.layer = layer::OVER_HUD;
        bar.anchor = 0;
        bar.x = 8;
        bar.y = 8;
        bar.w = 100;
        bar.h = 8;
        bar.value = 50;
        bar.max = 100;
        bar.color = 0xff00_00ff;
        bar.color2 = 0x4040_40ff;
        assert_eq!(hud.set(0, 1, 3, bar, 0), 0);

        let mut list = UiDrawList::new();
        draw_mod_hud(&hud, 0, 10, 0.0, &mut list, None);
        assert!(
            list.cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Patch { name, .. } if name.starts_with("STCFN"))),
            "expected STCFN patches for Mod HUD text: {:?}",
            list.cmds
        );
        assert!(
            list.cmds.iter().any(|c| matches!(c, DrawCmd::Fill { .. })),
            "expected fills for panel/bar"
        );
    }

    #[test]
    fn expands_key_tokens() {
        assert_eq!(
            expand_key_tokens("Press {key:+use} or {key:slot 1}", |t| match t {
                "+use" => "E".into(),
                "slot 1" => "1".into(),
                _ => t.into(),
            }),
            "Press E or 1"
        );
    }

    #[test]
    fn marker_clamps_off_screen() {
        set_marker_cam(MarkerCam {
            x: 0.0,
            y: 0.0,
            yaw_deg: 0.0,
            scale: 256.0,
            z: 0.0,
        });
        let mut hud = ModHud::new();
        let mut m = HudWidget::default();
        m.kind = kind::MARKER;
        m.layer = layer::OVER_HUD;
        m.world_xyz = [10_000, 0, 0];
        m.color = 0xff00_00ff;
        m.w = 8;
        m.h = 8;
        assert_eq!(hud.set(0, 1, 9, m, 0), 0);
        let mut list = UiDrawList::new();
        draw_mod_hud(&hud, 0, 0, 0.0, &mut list, None);
        assert!(
            list.cmds.iter().any(|c| matches!(c, DrawCmd::Fill { .. })),
            "expected clamped marker fill: {:?}",
            list.cmds
        );
    }

    #[test]
    fn modal_dims_screen() {
        let mut hud = ModHud::new();
        let mut m = sample_panel();
        m.layer = layer::MODAL;
        assert_eq!(hud.set(0, 1, 1, m, 0), 0);
        let mut list = UiDrawList::new();
        assert!(draw_mod_hud_modals(&hud, 0, 0, 0.0, &mut list, None));
        assert!(list.cmds.len() >= 2);
        assert!(seat_has_modal(&hud, 0));
    }

    #[test]
    fn under_over_layer_split() {
        let mut hud = ModHud::new();
        let mut under = HudWidget::default();
        under.kind = kind::FILL;
        under.layer = layer::UNDER_HUD;
        under.w = 10;
        under.h = 10;
        under.color = 0xff00_00ff;
        let mut over = under.clone();
        over.layer = layer::OVER_HUD;
        over.color = 0x00ff_00ff;
        assert_eq!(hud.set(0, 0, 1, under, 0), 0);
        assert_eq!(hud.set(0, 0, 2, over, 0), 0);
        let mut list = UiDrawList::new();
        draw_mod_hud_layer(&hud, 0, 0, 0.0, &mut list, layer::UNDER_HUD, None);
        assert_eq!(list.cmds.len(), 1);
        draw_mod_hud_layer(&hud, 0, 0, 0.0, &mut list, layer::OVER_HUD, None);
        assert_eq!(list.cmds.len(), 2);
    }

    #[test]
    fn map_widget_emits_lines_and_zone_cmds() {
        use crate::mod_zones::draw_mod_zones_automap;
        use room_nodebuild::apply_nodes;
        use room_sim::mod_zones::ModZones;
        use room_testkit::{MapBuilder, rect};

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut zones = ModZones::default();
        assert_eq!(
            zones.set(0, 0, 128, 128, 96, 128, 128, 64, 0x00ff00ff, 0, 0),
            0
        );
        let mut hud = ModHud::new();
        let mut w = HudWidget::default();
        w.kind = kind::MAP;
        w.layer = layer::OVER_HUD;
        w.anchor = 2;
        w.x = -72;
        w.y = 8;
        w.w = 64;
        w.h = 48;
        w.value = map_layers::value_from_scale(0.2);
        w.max = map_layers::GEOMETRY | map_layers::ZONES;
        assert_eq!(hud.set(0, 1, 50, w, 0), 0);
        let poses = [(128.0, 128.0, 90.0_f32); room_sim::MAX_PLAYERS];
        let ctx = ModHudMapCtx {
            map: Some(&map),
            zones: &zones,
            player_pose: &poses,
            hud: &hud,
            allmap: true,
            ticfrac_milli: 0,
        };
        let mut list = UiDrawList::new();
        draw_mod_hud_layer(&hud, 0, 0, 0.0, &mut list, layer::OVER_HUD, Some(&ctx));
        let lines = list
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCmd::Line { .. }))
            .count();
        assert!(lines >= 4, "geometry + zone rings should emit lines");
        let mut probe = UiDrawList::new();
        let view = crate::automap::AutomapView {
            center_x: 128.0,
            center_y: 128.0,
            scale: 0.2,
            screen_x: 0.0,
            screen_y: 0.0,
            width: 64.0,
            height: 48.0,
            fog: false,
            allmap: true,
            ..Default::default()
        };
        draw_mod_zones_automap(&zones, 0, 0, &view, &mut probe);
        assert!(!probe.cmds.is_empty());
    }
}

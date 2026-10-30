//! Retained Mod HUD drawing (`docs/27` §9 / MH1).

use crate::automap::{AutomapView, draw_automap_ex2, draw_player_arrow};
use crate::doom_text::{draw_stcfn_string, measure_stcfn_string};
use crate::draw::{Rect, Rgba, UiDrawList, Vec2};
use crate::mod_zones::draw_mod_zones_automap;
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
            let mut x = ax + f32::from(w.x);
            let y = ay + f32::from(w.y);
            // Horizontally centre when the widget uses a centre column anchor.
            if matches!(w.anchor, 1 | 4 | 7) {
                x -= measure_stcfn_string(&text, scale) * 0.5;
            }
            // Alpha/tint reserved until STCFN patches support color multiply.
            let _ = (alpha, w.color, w.font);
            draw_stcfn_string(list, x, y, &text, scale);
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
        kind::MARKER => {
            let (sx, sy, off) = project_marker(w, vw, vh);
            let col = rgba_u32(w.color, alpha);
            let size = if w.w > 0 { f32::from(w.w) } else { 6.0 };
            list.fill(Rect::new(sx - size * 0.5, sy - size * 0.5, size, size), col);
            if off {
                // Edge chevron: small diamond pointing inward.
                list.fill(Rect::new(sx - 2.0, sy - 6.0, 4.0, 4.0), col);
            }
            if !w.text.is_empty() {
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
    let (center_x, center_y, player_angle) = player_map_pose(ctx.player_pose, follow);
    let scale = map_layers::scale_from_value(w.value);
    let rotate = (w.flags & flags::MAP_ROTATE) != 0;
    let bg = rgba_u32(if w.color2 != 0 { w.color2 } else { 0xff000000 }, alpha);
    list.fill(rect, bg);
    let view = AutomapView {
        center_x,
        center_y,
        scale,
        screen_x: rect.x,
        screen_y: rect.y,
        width: rect.w,
        height: rect.h,
        angle_deg: if rotate { player_angle } else { 0.0 },
        allmap: ctx.allmap,
        fog: true,
    };
    if (layers & map_layers::GEOMETRY) != 0 {
        if let Some(map) = ctx.map {
            draw_automap_ex2(map, &view, list, false, false);
        }
    }
    if (layers & map_layers::ZONES) != 0 {
        draw_mod_zones_automap(ctx.zones, maptime, ctx.ticfrac_milli, &view, list);
    }
    if (layers & map_layers::MARKERS) != 0 || (layers & map_layers::ITEMS) != 0 {
        draw_map_markers(list, ctx, seat, &view, rotate, layers, alpha);
    }
    let arrow_view = AutomapView {
        angle_deg: player_angle,
        ..view
    };
    draw_player_arrow(&arrow_view, list);
}

fn player_map_pose(poses: &[(f64, f64, f32)], seat: u8) -> (f64, f64, f32) {
    poses
        .get(usize::from(seat))
        .copied()
        .unwrap_or((0.0, 0.0, 0.0))
}

fn draw_map_markers(
    list: &mut UiDrawList,
    ctx: &ModHudMapCtx<'_>,
    seat: u8,
    view: &AutomapView,
    rotate: bool,
    layers: i32,
    alpha: f32,
) {
    for (_, mw) in ctx
        .hud
        .for_seat(seat)
        .filter(|(_, mw)| mw.kind == kind::MARKER)
    {
        if (layers & map_layers::ITEMS) != 0 && mw.name.is_empty() {
            continue;
        }
        if mw.flags & flags::HIDDEN != 0 {
            continue;
        }
        let wx = f64::from(mw.world_xyz[0]);
        let wy = f64::from(mw.world_xyz[1]);
        let p = project_map(view, wx, wy, rotate);
        let col = rgba_u32(mw.color, alpha);
        let size = if mw.w > 0 { f32::from(mw.w) } else { 4.0 };
        list.fill(
            Rect::new(p.x - size * 0.5, p.y - size * 0.5, size, size),
            col,
        );
    }
}

fn project_map(view: &AutomapView, x: f64, y: f64, rotate: bool) -> Vec2 {
    let mut dx = x - view.center_x;
    let mut dy = y - view.center_y;
    if rotate && view.angle_deg.abs() > 0.01 {
        let rad = f64::from(view.angle_deg).to_radians();
        let (s, c) = (rad.sin(), rad.cos());
        let rx = dx * c + dy * s;
        let ry = -dx * s + dy * c;
        dx = rx;
        dy = ry;
    }
    let sx = (dx as f32) * view.scale;
    let sy = -(dy as f32) * view.scale;
    Vec2::new(
        view.screen_x + view.width * 0.5 + sx,
        view.screen_y + view.height * 0.5 + sy,
    )
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
    /// Map units that span half the virtual width at centre.
    pub scale: f64,
}

impl Default for MarkerCam {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            yaw_deg: 0.0,
            scale: 1024.0,
        }
    }
}

thread_local! {
    static MARKER_CAM: core::cell::Cell<MarkerCam> = const { core::cell::Cell::new(MarkerCam {
        x: 0.0,
        y: 0.0,
        yaw_deg: 0.0,
        scale: 1024.0,
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

fn project_marker(w: &HudWidget, vw: f32, vh: f32) -> (f32, f32, bool) {
    let cam = MARKER_CAM.with(|c| c.get());
    let wx = f64::from(w.world_xyz[0]);
    let wy = f64::from(w.world_xyz[1]);
    let dx = wx - cam.x;
    let dy = wy - cam.y;
    let rad = cam.yaw_deg.to_radians();
    let (s, c) = (rad.sin(), rad.cos());
    // Forward = +Y in Doom; rotate into view space (right, forward).
    let right = dx * c + dy * s;
    let forward = -dx * s + dy * c;
    let scale = cam.scale.max(1.0);
    let mut sx = (vw * 0.5) + (right / scale) as f32 * (vw * 0.5);
    let mut sy = (vh * 0.5) - (forward / scale) as f32 * (vh * 0.5);
    let margin = 8.0;
    let mut off = false;
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

//! Classic automap line drawer (`am_map` subset).

use crate::draw::{DrawCmd, Rgba, UiDrawList, Vec2};
use room_map::MapGeometry;

/// Automap view parameters.
#[derive(Clone, Debug)]
pub struct AutomapView {
    /// Center X (map units).
    pub center_x: f64,
    /// Center Y.
    pub center_y: f64,
    /// Scale: screen pixels per map unit.
    pub scale: f32,
    /// Screen offset of the automap widget.
    pub screen_x: f32,
    /// Screen Y.
    pub screen_y: f32,
    /// Widget width.
    pub width: f32,
    /// Widget height.
    pub height: f32,
    /// Follow player arrow angle (degrees).
    pub angle_deg: f32,
    /// Computer Area Map — treat all lines as explored (shell sets from player).
    pub allmap: bool,
    /// Hide linedefs that lack Doom `ML_MAPPED` (explored fog-of-war).
    pub fog: bool,
}

impl Default for AutomapView {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_y: 0.0,
            scale: 0.15,
            screen_x: 0.0,
            screen_y: 0.0,
            width: 320.0,
            height: 168.0,
            angle_deg: 0.0,
            allmap: false,
            fog: true,
        }
    }
}

// Classic Doom `am_map` palette (approximate RGB from PLAYPAL ranges).
/// One-sided solid walls (`WALLCOLORS` / reds).
const COLOR_ONESIDED: Rgba = Rgba {
    r: 0.72,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
/// Two-sided same-height (`GRAYS`).
const COLOR_TWOSIDED: Rgba = Rgba {
    r: 0.45,
    g: 0.45,
    b: 0.45,
    a: 1.0,
};
/// Floor-height change (`FDWALLCOLORS` / browns).
const COLOR_FLOOR_DIFF: Rgba = Rgba {
    r: 0.59,
    g: 0.43,
    b: 0.20,
    a: 1.0,
};
/// Ceiling-height change (`CDWALLCOLORS` / yellows).
const COLOR_CEIL_DIFF: Rgba = Rgba {
    r: 0.88,
    g: 0.72,
    b: 0.0,
    a: 1.0,
};
/// Linedef with a special (doors / switches / teleports).
const COLOR_SPECIAL: Rgba = Rgba {
    r: 0.55,
    g: 0.55,
    b: 1.0,
    a: 1.0,
};
/// Player arrow (`THINGCOLORS` / greens).
const COLOR_PLAYER: Rgba = Rgba {
    r: 0.15,
    g: 0.85,
    b: 0.15,
    a: 1.0,
};
/// Secret linedefs (`ML_SECRET`) when revealed via `allmap`.
const COLOR_SECRET: Rgba = Rgba {
    r: 1.0,
    g: 0.2,
    b: 0.2,
    a: 1.0,
};
/// Grid lines.
const COLOR_GRID: Rgba = Rgba {
    r: 0.12,
    g: 0.12,
    b: 0.12,
    a: 1.0,
};

/// Doom `ML_SECRET`.
const ML_SECRET: i16 = 32;
/// Doom `ML_MAPPED` — linedef has been seen / revealed on the automap.
const ML_MAPPED: i16 = 0x0100;
/// Doom `ML_DONTDRAW`.
const ML_DONTDRAW: i16 = 0x0080;

/// UDMF / `Line_SetAutomapStyle` `AML_*` (GZDoom `am_map.h` subset).
const AML_ONE_SIDED: i32 = 1;
const AML_TWO_SIDED: i32 = 2;
const AML_FLOOR_DIFF: i32 = 3;
const AML_CEIL_DIFF: i32 = 4;
#[allow(dead_code)]
const AML_EXTRA_FLOOR: i32 = 5;
const AML_SPECIAL: i32 = 6;
const AML_SECRET: i32 = 7;
const AML_NOT_MAPPED: i32 = 8;

/// Append automap lines + player arrow into `list`.
pub fn draw_automap(map: &MapGeometry, view: &AutomapView, list: &mut UiDrawList) {
    draw_automap_ex(map, view, list, true);
}

/// Automap into a widget rect; optionally skip the player chevron (MAP widget draws its own).
pub fn draw_automap_ex(
    map: &MapGeometry,
    view: &AutomapView,
    list: &mut UiDrawList,
    draw_player: bool,
) {
    draw_automap_ex2(map, view, list, draw_player, true);
}

/// Like [`draw_automap_ex`] but can skip the black background fill (MAP widget supplies its own).
pub fn draw_automap_ex2(
    map: &MapGeometry,
    view: &AutomapView,
    list: &mut UiDrawList,
    draw_player: bool,
    draw_background: bool,
) {
    if draw_background {
        list.fill(
            crate::draw::Rect::new(view.screen_x, view.screen_y, view.width, view.height),
            Rgba::new(0.0, 0.0, 0.0, 1.0),
        );
    }
    draw_grid(view, list);

    let pad = 4.0;
    let clip = ClipRect {
        x0: view.screen_x - pad,
        y0: view.screen_y - pad,
        x1: view.screen_x + view.width + pad,
        y1: view.screen_y + view.height + pad,
    };

    for (li, line) in map.lines.iter().enumerate() {
        let Some(v1) = map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        if line.automap_style == AML_NOT_MAPPED {
            continue;
        }
        // Fog of war: only explored linedefs (unless Computer Area Map / iddt).
        if view.fog && !view.allmap && (line.flags & ML_MAPPED) == 0 {
            continue;
        }
        // `ML_DONTDRAW` lines and lines bounded only by UDMF `hidden` sectors
        // never show in play (the editor view, without fog, keeps them).
        if view.fog && line_hidden(map, line) {
            continue;
        }
        let ml_secret = (line.flags & ML_SECRET) != 0;
        // Hidden secret walls stay off the map until Computer Area Map.
        if ml_secret && !view.allmap {
            continue;
        }
        // UDMF `drawfullheight`: drawn like a solid wall.
        let full = map.udmf_user.line_ext(li) & room_map::udmf::ext::LINE_DRAWFULLHEIGHT != 0;
        let color = if full && !ml_secret {
            COLOR_ONESIDED
        } else {
            line_color(map, line, ml_secret)
        };
        let a = project(view, v1.pos.x(), v1.pos.y());
        let b = project(view, v2.pos.x(), v2.pos.y());
        if !clip.segment_visible(a, b) {
            continue;
        }
        list.cmds.push(DrawCmd::Line {
            a,
            b,
            color,
            thickness: 1.25,
        });
    }

    if draw_player {
        draw_player_arrow(view, list);
    }
}

/// One thing for [`draw_automap_things`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutomapThing {
    /// Map X (already including the class `AutomapOffsets`).
    pub x: f64,
    /// Map Y.
    pub y: f64,
    /// Facing in degrees.
    pub angle_deg: f32,
    /// Radius in map units.
    pub radius: f32,
    /// Marker colour.
    pub color: Rgba,
}

/// `AM_drawThings` (second `iddt`): a triangle per thing, pointing where it
/// faces.
pub fn draw_automap_things(view: &AutomapView, things: &[AutomapThing], list: &mut UiDrawList) {
    for t in things {
        let c = project(view, t.x, t.y);
        if c.x < view.screen_x
            || c.y < view.screen_y
            || c.x > view.screen_x + view.width
            || c.y > view.screen_y + view.height
        {
            continue;
        }
        let r = (t.radius * view.scale).clamp(2.0, 24.0);
        // The automap is north-up; `arrow_offset` flips Y for the screen.
        let a = t.angle_deg;
        let tip = arrow_offset(a, r);
        let left = arrow_offset(a + 140.0, r * 0.7);
        let right = arrow_offset(a - 140.0, r * 0.7);
        let tip_p = Vec2::new(c.x + tip.0, c.y + tip.1);
        let left_p = Vec2::new(c.x + left.0, c.y + left.1);
        let right_p = Vec2::new(c.x + right.0, c.y + right.1);
        for (p0, p1) in [(left_p, tip_p), (tip_p, right_p), (right_p, left_p)] {
            list.cmds.push(DrawCmd::Line {
                a: p0,
                b: p1,
                color: t.color,
                thickness: 1.0,
            });
        }
    }
}

/// Player chevron at the widget centre (`AM_drawPlayer`).
pub fn draw_player_arrow(view: &AutomapView, list: &mut UiDrawList) {
    let c = Vec2::new(
        view.screen_x + view.width * 0.5,
        view.screen_y + view.height * 0.5,
    );
    let tip = arrow_offset(view.angle_deg, 12.0);
    let left = arrow_offset(view.angle_deg + 140.0, 7.0);
    let right = arrow_offset(view.angle_deg - 140.0, 7.0);
    let tip_p = Vec2::new(c.x + tip.0, c.y + tip.1);
    let left_p = Vec2::new(c.x + left.0, c.y + left.1);
    let right_p = Vec2::new(c.x + right.0, c.y + right.1);
    for (a, b) in [(left_p, tip_p), (tip_p, right_p), (right_p, left_p)] {
        list.cmds.push(DrawCmd::Line {
            a,
            b,
            color: COLOR_PLAYER,
            thickness: 2.0,
        });
    }
}

/// `ML_DONTDRAW`, or every sector touching the line is UDMF `hidden`.
fn line_hidden(map: &MapGeometry, line: &room_map::Line) -> bool {
    if (line.flags & ML_DONTDRAW) != 0 {
        return true;
    }
    let hidden = |s: Option<usize>| {
        s.is_none_or(|s| map.udmf_user.sector_ext(s) & room_map::udmf::ext::SECTOR_HIDDEN != 0)
    };
    let front = line.front_sector.map(|s| s.raw() as usize);
    let back = line.back_sector.map(|s| s.raw() as usize);
    front.is_some() && hidden(front) && hidden(back)
}

fn line_color(map: &MapGeometry, line: &room_map::Line, ml_secret: bool) -> Rgba {
    match line.automap_style {
        AML_ONE_SIDED => COLOR_ONESIDED,
        AML_TWO_SIDED => COLOR_TWOSIDED,
        AML_FLOOR_DIFF => COLOR_FLOOR_DIFF,
        AML_CEIL_DIFF => COLOR_CEIL_DIFF,
        AML_SPECIAL => COLOR_SPECIAL,
        AML_SECRET => COLOR_SECRET,
        _ => {
            if ml_secret {
                return COLOR_SECRET;
            }
            if line.special != 0 {
                return COLOR_SPECIAL;
            }
            let Some(back) = line.back_sector else {
                return COLOR_ONESIDED;
            };
            let Some(front) = line.front_sector else {
                return COLOR_ONESIDED;
            };
            let Some(fs) = map.sectors.get(front.raw() as usize) else {
                return COLOR_TWOSIDED;
            };
            let Some(bs) = map.sectors.get(back.raw() as usize) else {
                return COLOR_TWOSIDED;
            };
            let floor_diff = (fs.floor_height - bs.floor_height).abs() > 0.5;
            let ceil_diff = (fs.ceiling_height - bs.ceiling_height).abs() > 0.5;
            if floor_diff {
                COLOR_FLOOR_DIFF
            } else if ceil_diff {
                COLOR_CEIL_DIFF
            } else {
                COLOR_TWOSIDED
            }
        }
    }
}

fn draw_grid(view: &AutomapView, list: &mut UiDrawList) {
    // Doom `am_grid` — 128 mu cells, clipped to the widget.
    const GRID: f64 = 128.0;
    if view.scale < 0.04 {
        return; // too zoomed out — grid would be noise
    }
    let half_w = (view.width * 0.5) as f64 / f64::from(view.scale);
    let half_h = (view.height * 0.5) as f64 / f64::from(view.scale);
    let x0 = ((view.center_x - half_w) / GRID).floor() * GRID;
    let x1 = ((view.center_x + half_w) / GRID).ceil() * GRID;
    let y0 = ((view.center_y - half_h) / GRID).floor() * GRID;
    let y1 = ((view.center_y + half_h) / GRID).ceil() * GRID;
    let mut x = x0;
    while x <= x1 + 0.5 {
        let a = project(view, x, view.center_y - half_h);
        let b = project(view, x, view.center_y + half_h);
        list.cmds.push(DrawCmd::Line {
            a: Vec2::new(a.x, view.screen_y),
            b: Vec2::new(b.x, view.screen_y + view.height),
            color: COLOR_GRID,
            thickness: 1.0,
        });
        x += GRID;
    }
    let mut y = y0;
    while y <= y1 + 0.5 {
        let a = project(view, view.center_x - half_w, y);
        let b = project(view, view.center_x + half_w, y);
        list.cmds.push(DrawCmd::Line {
            a: Vec2::new(view.screen_x, a.y),
            b: Vec2::new(view.screen_x + view.width, b.y),
            color: COLOR_GRID,
            thickness: 1.0,
        });
        y += GRID;
    }
}

struct ClipRect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl ClipRect {
    fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x0 && p.x <= self.x1 && p.y >= self.y0 && p.y <= self.y1
    }

    fn segment_visible(&self, a: Vec2, b: Vec2) -> bool {
        if self.contains(a) || self.contains(b) {
            return true;
        }
        // Cheap reject: both ends share an outside half-plane.
        let left = a.x < self.x0 && b.x < self.x0;
        let right = a.x > self.x1 && b.x > self.x1;
        let above = a.y < self.y0 && b.y < self.y0;
        let below = a.y > self.y1 && b.y > self.y1;
        !(left || right || above || below)
    }
}

fn project(view: &AutomapView, x: f64, y: f64) -> Vec2 {
    let dx = (x - view.center_x) as f32 * view.scale;
    let dy = (view.center_y - y) as f32 * view.scale; // flip Y for screen
    Vec2::new(
        view.screen_x + view.width * 0.5 + dx,
        view.screen_y + view.height * 0.5 + dy,
    )
}

fn arrow_offset(angle_deg: f32, len: f32) -> (f32, f32) {
    let rad = angle_deg.to_radians();
    // Screen: 0° = east, 90° = north → up is -Y
    (rad.cos() * len, -rad.sin() * len)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn automap_emits_lines() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut list = UiDrawList::new();
        let view = AutomapView {
            center_x: 128.0,
            center_y: 128.0,
            fog: false,
            ..AutomapView::default()
        };
        draw_automap(&map, &view, &mut list);
        let lines = list
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCmd::Line { color, .. } if color == &COLOR_PLAYER))
            .count();
        assert_eq!(lines, 3, "player chevron should be three edges");
    }

    #[test]
    fn arrow_points_east_at_zero_degrees() {
        let (dx, dy) = arrow_offset(0.0, 10.0);
        assert!((dx - 10.0).abs() < 1e-3);
        assert!(dy.abs() < 1e-3);
    }

    #[test]
    fn automapstyle_not_mapped_skips_line() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let total = map.lines.len();
        for line in &mut map.lines {
            line.automap_style = AML_NOT_MAPPED;
        }
        let view = AutomapView {
            center_x: 128.0,
            center_y: 128.0,
            fog: false,
            ..AutomapView::default()
        };
        let mut list = UiDrawList::new();
        draw_automap(&map, &view, &mut list);
        let walls = list
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCmd::Line { color, .. } if color != &COLOR_PLAYER && color != &COLOR_GRID))
            .count();
        assert_eq!(walls, 0, "AML_NotMapped should hide all {total} lines");
    }

    #[test]
    fn secret_lines_hidden_until_allmap() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        if let Some(line) = map.lines.first_mut() {
            line.flags |= ML_SECRET;
        }
        let view = AutomapView {
            center_x: 128.0,
            center_y: 128.0,
            allmap: false,
            fog: false,
            ..AutomapView::default()
        };
        let mut hidden = UiDrawList::new();
        draw_automap(&map, &view, &mut hidden);
        let without = hidden
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCmd::Line { color, .. } if color != &COLOR_PLAYER && color != &COLOR_GRID))
            .count();
        let mut shown = UiDrawList::new();
        let view_all = AutomapView {
            allmap: true,
            ..view
        };
        draw_automap(&map, &view_all, &mut shown);
        let with = shown
            .cmds
            .iter()
            .filter(|c| matches!(c, DrawCmd::Line { color, .. } if color != &COLOR_PLAYER && color != &COLOR_GRID))
            .count();
        assert!(with > without, "allmap should reveal secret linedef");
        assert!(
            shown
                .cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Line { color, .. } if color == &COLOR_SECRET))
        );
    }

    #[test]
    fn fog_hides_unmapped_lines() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let view = AutomapView {
            center_x: 128.0,
            center_y: 128.0,
            fog: true,
            allmap: false,
            ..AutomapView::default()
        };
        let mut fogged = UiDrawList::new();
        draw_automap(&map, &view, &mut fogged);
        let walls_fog = fogged
            .cmds
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    DrawCmd::Line { color, .. } if color != &COLOR_PLAYER && color != &COLOR_GRID
                )
            })
            .count();
        assert_eq!(walls_fog, 0, "unexplored map must show no walls");

        for line in &mut map.lines {
            line.flags |= ML_MAPPED;
        }
        let mut revealed = UiDrawList::new();
        draw_automap(&map, &view, &mut revealed);
        let walls_ok = revealed
            .cmds
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    DrawCmd::Line { color, .. } if color != &COLOR_PLAYER && color != &COLOR_GRID
                )
            })
            .count();
        assert!(walls_ok > 0, "ML_MAPPED walls must draw");
    }

    #[test]
    fn floor_diff_uses_brown() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .room("b", rect(0.0, 128.0, 256.0, 256.0), 24.0, 128.0)
            .player_start(128.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let view = AutomapView {
            center_x: 128.0,
            center_y: 128.0,
            scale: 0.5,
            fog: false,
            ..AutomapView::default()
        };
        let mut list = UiDrawList::new();
        draw_automap(&map, &view, &mut list);
        assert!(
            list.cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Line { color, .. } if color == &COLOR_FLOOR_DIFF)),
            "shared edge with floor delta should be brown"
        );
    }
}

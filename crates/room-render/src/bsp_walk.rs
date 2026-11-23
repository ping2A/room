//! CPU BSP walk → visible subsector list.

use crate::clipper::{Clipper, pseudo_angle};
use room_map::{GameNodes, MapGeometry, NF_SUBSECTOR};

/// Result of a visibility walk.
#[derive(Clone, Debug, Default)]
pub struct VisibleSet {
    /// Subsector indices in front-to-back discovery order.
    pub subsectors: Vec<u32>,
}

/// Walk classic/extended game nodes from `view_x/y`, listing subsectors in
/// front-to-back order.
///
/// Solid walls cull nothing: portal and mirror views put their eye behind
/// walls and rely on a clip plane instead. Only UDMF `blockrendering` sides
/// facing the viewer occlude, and the walk stops once they cover the full
/// circle. (An earlier version also let one-sided walls fill the circle
/// while visiting in a wrong order, which could drop visible space.)
#[must_use]
pub fn walk_bsp(map: &MapGeometry, view_x: f64, view_y: f64) -> VisibleSet {
    walk(map, view_x, view_y, false)
}

/// [`walk_bsp`] that skips every BSP node whose bounding box is hidden behind
/// solid walls facing the viewer (`R_AddLine` / `R_CheckBBox`): the subsectors a viewer standing at
/// `view_x/y` inside the map can see, in front-to-back order.
///
/// Only for an eye inside its sector's open height (a solid wall hides what
/// is behind it at every height there). Polyobject lines never occlude: their
/// segs sit in the BSP at the anchor, not where the polyobject is.
#[must_use]
pub fn walk_bsp_occluded(map: &MapGeometry, view_x: f64, view_y: f64) -> VisibleSet {
    walk(map, view_x, view_y, true)
}

/// [`walk_bsp_occluded`] that also skips what lies outside `view_arc`, the
/// counter-clockwise arc `(from, to)` of map angles (radians) the camera can
/// show (`R_CheckBBox` with vanilla's `clipangle`). `None` culls by walls
/// only.
#[must_use]
pub fn walk_bsp_occluded_in_view(
    map: &MapGeometry,
    view_x: f64,
    view_y: f64,
    view_arc: Option<(f64, f64)>,
) -> VisibleSet {
    walk_with(map, view_x, view_y, true, view_arc)
}

/// Map angle (radians) → clipper angle.
fn rad_to_bam(rad: f64) -> room_core::Bam {
    let t = (rad / core::f64::consts::TAU).rem_euclid(1.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    room_core::Bam((t * room_core::math::BAM_PER_TURN) as u32)
}

/// Walk state shared by every node visit.
struct Walk<'a> {
    map: &'a MapGeometry,
    nodes: &'a GameNodes,
    vx: f64,
    vy: f64,
    /// Skip hidden far children ([`walk_bsp_occluded`]).
    cull: bool,
    /// Lines that never occlude (polyobjects), when culling.
    no_occlude: Vec<bool>,
}

fn walk(map: &MapGeometry, view_x: f64, view_y: f64, cull: bool) -> VisibleSet {
    walk_with(map, view_x, view_y, cull, None)
}

fn walk_with(
    map: &MapGeometry,
    view_x: f64,
    view_y: f64,
    cull: bool,
    view_arc: Option<(f64, f64)>,
) -> VisibleSet {
    let Some(nodes) = map.game_nodes.as_ref() else {
        return VisibleSet::default();
    };
    let mut no_occlude = Vec::new();
    if cull {
        for po in &map.setup.polyobjects {
            for &lid in &po.lines {
                let i = lid.raw() as usize;
                if no_occlude.len() <= i {
                    no_occlude.resize(i + 1, false);
                }
                no_occlude[i] = true;
            }
        }
    }
    let w = Walk {
        map,
        nodes,
        vx: view_x,
        vy: view_y,
        cull,
        no_occlude,
    };
    let mut clip = Clipper::new();
    if let Some((from, to)) = view_arc.filter(|(f, t)| t - f < core::f64::consts::TAU) {
        // Everything outside the view: from its right edge round to its left.
        clip.add_range(rad_to_bam(to), rad_to_bam(from));
    }
    let mut out = VisibleSet::default();
    if nodes.nodes.is_empty() {
        if !nodes.subsectors.is_empty() {
            visit_subsector(&w, 0, &mut clip, &mut out);
        }
        return out;
    }
    let root = (nodes.nodes.len() - 1) as u32;
    walk_node(&w, root, &mut clip, &mut out);
    out
}

fn walk_node(w: &Walk<'_>, ni: u32, clip: &mut Clipper, out: &mut VisibleSet) {
    if clip.is_full() {
        return;
    }
    let Some(node) = w.nodes.nodes.get(ni as usize) else {
        return;
    };
    let side = point_on_side(node, w.vx, w.vy);
    let front = node.children[side];
    let back = node.children[side ^ 1];
    // The child the viewer is in is always walked; the far child only when
    // some of its bounding box is not yet hidden behind solid walls
    // (`R_CheckBBox`). Without this every frame rebuilt the whole map.
    visit_child(w, front, clip, out);
    if !clip.is_full() && (!w.cull || bbox_visible(clip, &node.bbox[side ^ 1], w.vx, w.vy)) {
        visit_child(w, back, clip, out);
    }
}

/// `R_CheckBBox`: can any part of `bbox` (`top, bottom, left, right`) still
/// be seen past the solid walls already in `clip`?
///
/// The box's angular extent from the viewer is widened by a small margin,
/// so rounding in [`pseudo_angle`] can only keep a box, never drop one.
fn bbox_visible(clip: &Clipper, bbox: &[i16; 4], vx: f64, vy: f64) -> bool {
    let [top, bottom, left, right] = bbox.map(f64::from);
    // Inside (or on the edge of) the box: everything around may be in it.
    if vx >= left - 1.0 && vx <= right + 1.0 && vy >= bottom - 1.0 && vy <= top + 1.0 {
        return true;
    }
    let cx = (left + right) * 0.5;
    let cy = (top + bottom) * 0.5;
    let center = libm::atan2(cy - vy, cx - vx);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        let mut d = libm::atan2(y - vy, x - vx) - center;
        if d > core::f64::consts::PI {
            d -= core::f64::consts::TAU;
        } else if d < -core::f64::consts::PI {
            d += core::f64::consts::TAU;
        }
        lo = lo.min(d);
        hi = hi.max(d);
    }
    // About a fifth of a degree each side.
    const MARGIN: f64 = 0.004;
    let (lo, hi) = (center + lo - MARGIN, center + hi + MARGIN);
    if hi - lo >= core::f64::consts::PI {
        return true;
    }
    clip.is_range_visible(rad_to_bam(lo), rad_to_bam(hi))
}

fn visit_child(w: &Walk<'_>, child: u32, clip: &mut Clipper, out: &mut VisibleSet) {
    if child & NF_SUBSECTOR != 0 {
        let ss = child & !NF_SUBSECTOR;
        visit_subsector(w, ss, clip, out);
    } else {
        walk_node(w, child, clip, out);
    }
}

fn visit_subsector(w: &Walk<'_>, ss: u32, clip: &mut Clipper, out: &mut VisibleSet) {
    let (map, nodes, vx, vy) = (w.map, w.nodes, w.vx, w.vy);
    let Some(sub) = nodes.subsectors.get(ss as usize) else {
        return;
    };
    out.subsectors.push(ss);
    let start = sub.first_seg as usize;
    let end = start + sub.num_segs as usize;
    for seg in nodes.segs.get(start..end).into_iter().flatten() {
        let Some(line_id) = seg.line else {
            continue;
        };
        if w.no_occlude
            .get(line_id.raw() as usize)
            .copied()
            .unwrap_or(false)
        {
            continue;
        }
        let Some(line) = map.lines.get(line_id.raw() as usize) else {
            continue;
        };
        // Solid one-sided lines occlude only in the culled walk; a two-sided
        // line whose viewed side has UDMF `blockrendering` occludes in every
        // walk (UZDoom `WALLF_BLOCKRENDERING`, portal views included).
        let blocks = match line.back_sector {
            None => w.cull,
            Some(_) => seg_blocks_rendering(map, line, seg.side),
        };
        if !blocks {
            continue;
        }
        let Some(v1) = map.vertices.get(seg.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = map.vertices.get(seg.v2.raw() as usize) else {
            continue;
        };
        // Only a seg facing the viewer occludes (vanilla `R_AddLine`).
        let dx = v2.pos.x() - v1.pos.x();
        let dy = v2.pos.y() - v1.pos.y();
        let cross = dx * (vy - v1.pos.y()) - dy * (vx - v1.pos.x());
        // Doom winding: the side's front is on the right of v1→v2, so a
        // viewer on that side sees the seg clockwise (v2 → v1 is the
        // counter-clockwise span the clipper takes).
        if cross >= 0.0 {
            continue;
        }
        let a1 = pseudo_angle(vx, vy, v2.pos.x(), v2.pos.y());
        let a2 = pseudo_angle(vx, vy, v1.pos.x(), v1.pos.y());
        if a1.0 != a2.0 && clip.is_range_visible(a1, a2) {
            clip.add_range(a1, a2);
        }
    }
}

/// `blockrendering` on the side this seg is drawn from.
fn seg_blocks_rendering(map: &MapGeometry, line: &room_map::Line, seg_side: i16) -> bool {
    line.sides[usize::from(seg_side != 0)]
        .and_then(|sid| map.sides.get(sid.raw() as usize))
        .is_some_and(|s| s.block_rendering)
}

/// Vanilla `R_PointOnSide`: 0 = front (right) child, 1 = back (left).
///
/// Occlusion culling needs exact front-to-back order: a wall from the far
/// side of a partition must never reach the clipper before the near side
/// has been visited. (An earlier version had the diagonal case reversed.)
fn point_on_side(node: &room_map::Node, x: f64, y: f64) -> usize {
    let dx = f64::from(node.dx);
    let dy = f64::from(node.dy);
    if dx == 0.0 {
        return if x <= f64::from(node.x) {
            usize::from(dy > 0.0)
        } else {
            usize::from(dy < 0.0)
        };
    }
    if dy == 0.0 {
        return if y <= f64::from(node.y) {
            usize::from(dx < 0.0)
        } else {
            usize::from(dx > 0.0)
        };
    }
    let left = dy * (x - f64::from(node.x));
    let right = (y - f64::from(node.y)) * dx;
    usize::from(right >= left)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn walk_finds_subsectors() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let vis = walk_bsp(&map, 128.0, 128.0);
        assert!(!vis.subsectors.is_empty());
    }
}

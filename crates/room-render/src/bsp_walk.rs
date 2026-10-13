//! CPU BSP walk → visible subsector list.

use crate::clipper::{Clipper, pseudo_angle};
use room_map::{GameNodes, MapGeometry, NF_SUBSECTOR};

/// Result of a visibility walk.
#[derive(Clone, Debug, Default)]
pub struct VisibleSet {
    /// Subsector indices in front-to-back discovery order.
    pub subsectors: Vec<u32>,
}

/// Walk classic/extended game nodes from `view_x/y`, collecting visible subsectors.
///
/// Solid one-sided lines add their angular span to the clipper. Two-sided lines
/// do not occlude (portal/passable). Full ZDoom clipper rules land later.
#[must_use]
pub fn walk_bsp(map: &MapGeometry, view_x: f64, view_y: f64) -> VisibleSet {
    let Some(nodes) = map.game_nodes.as_ref() else {
        return VisibleSet::default();
    };
    let mut clip = Clipper::new();
    let mut out = VisibleSet::default();
    if nodes.nodes.is_empty() {
        if !nodes.subsectors.is_empty() {
            visit_subsector(map, nodes, 0, view_x, view_y, &mut clip, &mut out);
        }
        return out;
    }
    let root = (nodes.nodes.len() - 1) as u32;
    walk_node(map, nodes, root, view_x, view_y, &mut clip, &mut out);
    out
}

fn walk_node(
    map: &MapGeometry,
    nodes: &GameNodes,
    ni: u32,
    vx: f64,
    vy: f64,
    clip: &mut Clipper,
    out: &mut VisibleSet,
) {
    if clip.is_full() {
        return;
    }
    let Some(node) = nodes.nodes.get(ni as usize) else {
        return;
    };
    let side = point_on_side(node, vx, vy);
    let front = node.children[side];
    let back = node.children[side ^ 1];
    visit_child(map, nodes, front, vx, vy, clip, out);
    if !clip.is_full() {
        visit_child(map, nodes, back, vx, vy, clip, out);
    }
}

fn visit_child(
    map: &MapGeometry,
    nodes: &GameNodes,
    child: u32,
    vx: f64,
    vy: f64,
    clip: &mut Clipper,
    out: &mut VisibleSet,
) {
    if child & NF_SUBSECTOR != 0 {
        let ss = child & !NF_SUBSECTOR;
        visit_subsector(map, nodes, ss, vx, vy, clip, out);
    } else {
        walk_node(map, nodes, child, vx, vy, clip, out);
    }
}

fn visit_subsector(
    map: &MapGeometry,
    nodes: &GameNodes,
    ss: u32,
    vx: f64,
    vy: f64,
    clip: &mut Clipper,
    out: &mut VisibleSet,
) {
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
        let Some(line) = map.lines.get(line_id.raw() as usize) else {
            continue;
        };
        // Only solid one-sided lines occlude.
        if line.back_sector.is_some() {
            continue;
        }
        let Some(v1) = map.vertices.get(seg.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = map.vertices.get(seg.v2.raw() as usize) else {
            continue;
        };
        // Only occlude if the seg faces the viewer (cross product > 0 for Doom
        // front-facing convention: v1→v2 with view on the left).
        let dx = v2.pos.x() - v1.pos.x();
        let dy = v2.pos.y() - v1.pos.y();
        let cross = dx * (vy - v1.pos.y()) - dy * (vx - v1.pos.x());
        if cross <= 0.0 {
            continue;
        }
        let a1 = pseudo_angle(vx, vy, v1.pos.x(), v1.pos.y());
        let a2 = pseudo_angle(vx, vy, v2.pos.x(), v2.pos.y());
        if a1.0 != a2.0 && clip.is_range_visible(a1, a2) {
            clip.add_range(a1, a2);
        }
    }
}

fn point_on_side(node: &room_map::Node, x: f64, y: f64) -> usize {
    let dx = f64::from(node.dx);
    let dy = f64::from(node.dy);
    if dx == 0.0 {
        return if x <= f64::from(node.x) {
            usize::from(dy > 0.0)
        } else {
            usize::from(dy <= 0.0)
        };
    }
    if dy == 0.0 {
        return if y <= f64::from(node.y) {
            usize::from(dx < 0.0)
        } else {
            usize::from(dx >= 0.0)
        };
    }
    let left = (x - f64::from(node.x)) * dy;
    let right = (y - f64::from(node.y)) * dx;
    usize::from(right < left)
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

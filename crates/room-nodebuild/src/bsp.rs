//! Recursive BSP partitioner (ZDBSP-inspired, M3 subset).
//!
//! Picks splitters with ZDBSP-style cost (`SplitCost`, imbalance, AA preference).
//! Straddling segs are cut at the partition intersection and snapped to a
//! 16.16 fixed grid (ZDBSP vertex precision). GL minisegs still TODO.

use room_core::{LineId, VertexId};
use room_map::{GameNodes, MapGeometry, NF_SUBSECTOR, Node, Seg, Subsector};

/// Recursion guard against splits that keep straddling after vertex snapping.
const MAX_DEPTH: u32 = 256;
const SPLIT_COST: i32 = 8;
const AA_PREFERENCE: i32 = 16;

#[derive(Clone, Copy)]
struct WorkSeg {
    v1: VertexId,
    v2: VertexId,
    v1x: f64,
    v1y: f64,
    v2x: f64,
    v2y: f64,
    line: Option<LineId>,
    side: i16,
    /// Sector on this seg's side (`u32::MAX` when the side has none).
    sector: u32,
}

struct Builder {
    /// Growing vertex pool (map verts + split verts); coords are 16.16-snapped.
    verts: Vec<(f64, f64)>,
    /// Sectors lie right of their lines (Doom winding): collinear segs facing
    /// the other way go behind the splitter. Outward-wound maps (fronts on the
    /// left) keep both sides together.
    opposite_back: bool,
}

impl Builder {
    fn add_vertex(&mut self, x: f64, y: f64) -> VertexId {
        let (x, y) = snap_fixed(x, y);
        // Reuse an existing vertex at the same snapped position.
        if let Some(i) = self
            .verts
            .iter()
            .position(|&(vx, vy)| (vx - x).abs() < 1e-9 && (vy - y).abs() < 1e-9)
        {
            return VertexId::from_raw(i as u32);
        }
        let id = VertexId::from_raw(self.verts.len() as u32);
        self.verts.push((x, y));
        id
    }
}

/// Build game nodes via recursive bipartition of linedef segs.
#[must_use]
pub fn build_bsp(map: &MapGeometry) -> GameNodes {
    let verts: Vec<(f64, f64)> = map
        .vertices
        .iter()
        .map(|v| snap_fixed(v.pos.x(), v.pos.y()))
        .collect();
    let orig_vert_count = verts.len();
    let mut b = Builder {
        verts,
        opposite_back: false,
    };

    let mut work = Vec::new();
    for (li, line) in map.lines.iter().enumerate() {
        let v1 = line.v1.raw() as usize;
        let v2 = line.v2.raw() as usize;
        if v1 >= b.verts.len() || v2 >= b.verts.len() {
            continue;
        }
        let (ax, ay) = b.verts[v1];
        let (bx, by) = b.verts[v2];
        if (ax - bx).abs() < 1e-9 && (ay - by).abs() < 1e-9 {
            continue;
        }
        work.push(WorkSeg {
            v1: line.v1,
            v2: line.v2,
            v1x: ax,
            v1y: ay,
            v2x: bx,
            v2y: by,
            line: Some(LineId::from_raw(li as u32)),
            side: 0,
            sector: line.front_sector.map_or(u32::MAX, |s| s.raw()),
        });
        if let Some(back) = line.back_sector {
            work.push(WorkSeg {
                v1: line.v2,
                v2: line.v1,
                v1x: bx,
                v1y: by,
                v2x: ax,
                v2y: ay,
                line: Some(LineId::from_raw(li as u32)),
                side: 1,
                sector: back.raw(),
            });
        }
    }

    // Twice the signed area of every sector seen from its segs: negative when
    // sectors are on the right (clockwise in y-up map space).
    let area: f64 = work
        .iter()
        .filter(|s| s.sector != u32::MAX)
        .map(|s| s.v1x * s.v2y - s.v2x * s.v1y)
        .sum();
    b.opposite_back = area < 0.0;

    let mut segs = Vec::new();
    let mut subsectors = Vec::new();
    let mut nodes = Vec::new();
    if work.is_empty() {
        return GameNodes::default();
    }
    let _ = build_node(&mut b, &work, &mut segs, &mut subsectors, &mut nodes, 0);

    let extra_vertices = b
        .verts
        .iter()
        .skip(orig_vert_count)
        .map(|&(x, y)| room_core::DVec2::new(x, y))
        .collect();

    GameNodes {
        segs,
        subsectors,
        nodes,
        extra_vertices,
    }
}

fn build_node(
    b: &mut Builder,
    segs_in: &[WorkSeg],
    out_segs: &mut Vec<Seg>,
    out_ss: &mut Vec<Subsector>,
    out_nodes: &mut Vec<Node>,
    depth: u32,
) -> u32 {
    if segs_in.is_empty() {
        let first = out_segs.len() as u32;
        out_ss.push(Subsector {
            num_segs: 0,
            first_seg: first,
        });
        return NF_SUBSECTOR | (out_ss.len() as u32 - 1);
    }

    if segs_in.len() <= 1 || depth >= MAX_DEPTH {
        return make_subsector(segs_in, out_segs, out_ss);
    }

    let Some(si) = best_split(segs_in, b.opposite_back) else {
        return make_subsector(segs_in, out_segs, out_ss);
    };
    let splitter = segs_in[si];
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (i, s) in segs_in.iter().enumerate() {
        if i == si {
            right.push(*s);
            continue;
        }
        match classify(splitter, *s, b.opposite_back) {
            Side::Front => right.push(*s),
            Side::Back => left.push(*s),
            Side::On => right.push(*s),
            Side::Split => match split_seg(b, splitter, *s) {
                Some((front, back)) => {
                    right.push(front);
                    left.push(back);
                }
                None => {
                    // Degenerate: midpoint fallback.
                    let mx = (s.v1x + s.v2x) * 0.5;
                    let my = (s.v1y + s.v2y) * 0.5;
                    if point_side(splitter, mx, my) >= 0 {
                        right.push(*s);
                    } else {
                        left.push(*s);
                    }
                }
            },
        }
    }

    if left.is_empty() || right.is_empty() {
        return make_subsector(segs_in, out_segs, out_ss);
    }

    let right_child = build_node(b, &right, out_segs, out_ss, out_nodes, depth + 1);
    let left_child = build_node(b, &left, out_segs, out_ss, out_nodes, depth + 1);

    let dx = splitter.v2x - splitter.v1x;
    let dy = splitter.v2y - splitter.v1y;
    out_nodes.push(Node {
        x: clamp_i16(splitter.v1x),
        y: clamp_i16(splitter.v1y),
        dx: clamp_i16(dx),
        dy: clamp_i16(dy),
        bbox: [bbox_of(&right), bbox_of(&left)],
        children: [right_child, left_child],
    });
    (out_nodes.len() - 1) as u32
}

/// Split `s` at its intersection with `splitter`. Returns (front_piece, back_piece).
fn split_seg(b: &mut Builder, splitter: WorkSeg, s: WorkSeg) -> Option<(WorkSeg, WorkSeg)> {
    let (ix, iy) = line_intersect(
        (splitter.v1x, splitter.v1y),
        (splitter.v2x, splitter.v2y),
        (s.v1x, s.v1y),
        (s.v2x, s.v2y),
    )?;
    let mid = b.add_vertex(ix, iy);
    let (mx, my) = b.verts[mid.raw() as usize];

    let s1 = point_side(splitter, s.v1x, s.v1y);
    // Piece from v1 → mid, and mid → v2.
    let a = WorkSeg {
        v1: s.v1,
        v2: mid,
        v1x: s.v1x,
        v1y: s.v1y,
        v2x: mx,
        v2y: my,
        line: s.line,
        side: s.side,
        sector: s.sector,
    };
    let c = WorkSeg {
        v1: mid,
        v2: s.v2,
        v1x: mx,
        v1y: my,
        v2x: s.v2x,
        v2y: s.v2y,
        line: s.line,
        side: s.side,
        sector: s.sector,
    };
    // Front piece first in the returned pair.
    if s1 >= 0 { Some((a, c)) } else { Some((c, a)) }
}

fn line_intersect(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
) -> Option<(f64, f64)> {
    let (ax, ay) = a;
    let (bx, by) = b;
    let (cx, cy) = c;
    let (dx, dy) = d;
    let dax = bx - ax;
    let day = by - ay;
    let dbx = dx - cx;
    let dby = dy - cy;
    let denom = dax * dby - day * dbx;
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = ((cx - ax) * dby - (cy - ay) * dbx) / denom;
    let u = ((cx - ax) * day - (cy - ay) * dax) / denom;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    Some((ax + t * dax, ay + t * day))
}

fn make_subsector(
    segs_in: &[WorkSeg],
    out_segs: &mut Vec<Seg>,
    out_ss: &mut Vec<Subsector>,
) -> u32 {
    let first = out_segs.len() as u32;
    for s in segs_in {
        out_segs.push(Seg {
            v1: s.v1,
            v2: s.v2,
            angle: 0,
            line: s.line,
            side: s.side,
            offset: 0,
        });
    }
    let count = (out_segs.len() as u32).saturating_sub(first);
    out_ss.push(Subsector {
        num_segs: count,
        first_seg: first,
    });
    NF_SUBSECTOR | (out_ss.len() as u32 - 1)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Front,
    Back,
    On,
    Split,
}

/// Collinear segs facing the splitter's way are in front; with
/// `opposite_back`, opposite ones are behind (ZDBSP), so the two sides of a
/// line never share a leaf.
fn classify(splitter: WorkSeg, s: WorkSeg, opposite_back: bool) -> Side {
    let a = point_side(splitter, s.v1x, s.v1y);
    let b = point_side(splitter, s.v2x, s.v2y);
    if a == 0 && b == 0 {
        let dot = (splitter.v2x - splitter.v1x) * (s.v2x - s.v1x)
            + (splitter.v2y - splitter.v1y) * (s.v2y - s.v1y);
        if opposite_back && dot < 0.0 {
            Side::Back
        } else {
            Side::On
        }
    } else if a >= 0 && b >= 0 {
        Side::Front
    } else if a <= 0 && b <= 0 {
        Side::Back
    } else {
        Side::Split
    }
}

fn point_side(splitter: WorkSeg, x: f64, y: f64) -> i8 {
    let dx = splitter.v2x - splitter.v1x;
    let dy = splitter.v2y - splitter.v1y;
    let cross = (x - splitter.v1x) * dy - (y - splitter.v1y) * dx;
    if cross > 1e-6 {
        1
    } else if cross < -1e-6 {
        -1
    } else {
        0
    }
}

/// A convex single-sector set is a leaf. A convex set mixing sectors (both
/// sides of a line, either winding) is still split on a seg with segs behind it.
fn best_split(segs: &[WorkSeg], opposite_back: bool) -> Option<usize> {
    best_split_by(segs, false, opposite_back).or_else(|| {
        let mixed = segs.iter().any(|s| s.sector != segs[0].sector);
        if mixed {
            best_split_by(segs, true, opposite_back)
        } else {
            None
        }
    })
}

fn best_split_by(segs: &[WorkSeg], force: bool, opposite_back: bool) -> Option<usize> {
    let mut best_i = None;
    let mut best_cost = i32::MAX;
    for (i, cand) in segs.iter().enumerate() {
        let mut left = 0i32;
        let mut right = 0i32;
        let mut splits = 0i32;
        for (j, s) in segs.iter().enumerate() {
            if i == j {
                continue;
            }
            match classify(*cand, *s, opposite_back) {
                Side::Front | Side::On => right += 1,
                Side::Back => left += 1,
                Side::Split => splits += 1,
            }
        }
        if left == 0 || (right == 0 && splits == 0 && !force) {
            continue;
        }
        let mut cost = splits * SPLIT_COST + (left - right).abs();
        let dx = (cand.v2x - cand.v1x).abs();
        let dy = (cand.v2y - cand.v1y).abs();
        if dx < 1e-6 || dy < 1e-6 {
            cost -= AA_PREFERENCE;
        }
        if cost < best_cost {
            best_cost = cost;
            best_i = Some(i);
        }
    }
    best_i
}

fn bbox_of(segs: &[WorkSeg]) -> [i16; 4] {
    let mut top = f64::NEG_INFINITY;
    let mut bottom = f64::INFINITY;
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for s in segs {
        for (x, y) in [(s.v1x, s.v1y), (s.v2x, s.v2y)] {
            top = top.max(y);
            bottom = bottom.min(y);
            left = left.min(x);
            right = right.max(x);
        }
    }
    if !top.is_finite() {
        return [0, 0, 0, 0];
    }
    [
        clamp_i16(top),
        clamp_i16(bottom),
        clamp_i16(left),
        clamp_i16(right),
    ]
}

fn clamp_i16(v: f64) -> i16 {
    if v > f64::from(i16::MAX) {
        i16::MAX
    } else if v < f64::from(i16::MIN) {
        i16::MIN
    } else {
        v.round() as i16
    }
}

fn snap_fixed(x: f64, y: f64) -> (f64, f64) {
    (
        (x * 65536.0).round() / 65536.0,
        (y * 65536.0).round() / 65536.0,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_map::{
        MapFormat, RawLinedef, RawSector, RawSidedef, RawThing, RawVertex, link_geometry,
    };
    use room_testkit::{MapBuilder, rect};
    use std::collections::BTreeSet;

    #[test]
    fn two_sided_square_builds_tree() {
        let vertices = vec![
            RawVertex { x: 0, y: 0 },
            RawVertex { x: 128, y: 0 },
            RawVertex { x: 128, y: 128 },
            RawVertex { x: 0, y: 128 },
        ];
        let linedefs = (0u16..4)
            .map(|i| RawLinedef {
                v1: i,
                v2: (i + 1) % 4,
                flags: 1,
                special: 0,
                tag: 0,
                side_front: i,
                side_back: 0xFFFF,
                args: [0; 5],
                hexen: false,
                activation: 0,
                arg0_str: String::new(),
                midtex3d: false,
                midtex3d_impassible: false,
                check_switch_range: false,
                first_side_only: false,
                block_floaters: false,
                lock_number: 0,
                block_projectiles: false,
                block_sight: false,
                block_hitscan: false,
                extra_ids: Default::default(),
                alpha: 1.0,
                mid_additive: false,
                health: 0,
                health_group: 0,
                automap_style: 0,
                clip_midtex: false,
                wrap_midtex: false,
                ext: 0,
                user: Default::default(),
            })
            .collect();
        let sidedefs = (0u16..4)
            .map(|_| RawSidedef {
                x_offset: 0,
                y_offset: 0,
                upper: "-".into(),
                lower: "-".into(),
                middle: "WALL".into(),
                sector: 0,
                light: i16::MIN,
                light_absolute: false,
                light_top: i16::MIN,
                light_mid: i16::MIN,
                light_bottom: i16::MIN,
                light_absolute_top: false,
                light_absolute_mid: false,
                light_absolute_bottom: false,
                scale_x: 1.0,
                scale_y: 1.0,
                scale_x_top: 1.0,
                scale_y_top: 1.0,
                scale_x_bottom: 1.0,
                scale_y_bottom: 1.0,
                x_offset_top: 0,
                y_offset_top: 0,
                x_offset_bottom: 0,
                y_offset_bottom: 0,
                no_fake_contrast: false,
                smooth_lighting: false,
                scroll_x: 0.0,
                scroll_y: 0.0,
                clip_midtex: false,
                wrap_midtex: false,
                no_decals: false,
                alpha: None,
                block_rendering: false,
                user: Default::default(),
            })
            .collect();
        let sectors = vec![RawSector {
            floor_height: 0,
            ceiling_height: 128,
            floor_pic: "FLAT".into(),
            ceiling_pic: "FLAT".into(),
            light: 160,
            special: 0,
            tag: 0,
            floor_plane: None,
            ceiling_plane: None,
            extra_ids: Vec::new(),
            floor_x_pan: 0.0,
            floor_y_pan: 0.0,
            ceiling_x_pan: 0.0,
            ceiling_y_pan: 0.0,
            floor_rotation: 0.0,
            ceiling_rotation: 0.0,
            floor_x_scale: 1.0,
            floor_y_scale: 1.0,
            ceiling_x_scale: 1.0,
            ceiling_y_scale: 1.0,
            gravity: -1.0,
            friction: 0,
            damage_amount: 0,
            damage_type: String::new(),
            damage_interval: 0,
            leakiness: 0,
            silent: false,
            sound_sequence: String::new(),
            waterzone: false,
            drop_actors: true,
            no_falling_damage: false,
            light_color: -1,
            fade_color: -1,
            fog_density: 0,
            desaturation: 0,
            color_floor: -1,
            color_ceiling: -1,
            color_walltop: -1,
            color_wallbottom: -1,
            color_sprites: -1,
            health_floor: 0,
            health_ceiling: 0,
            health_3d: 0,
            floor_alpha: 1.0,
            ceiling_alpha: 1.0,
            floor_additive: false,
            ceiling_additive: false,
            ext: 0,
            user: Default::default(),
        }];
        let map = link_geometry(
            "MAP01",
            MapFormat::Doom,
            vertices,
            linedefs,
            sidedefs,
            sectors,
            Vec::<RawThing>::new(),
            [0; 16],
        );
        let nodes = build_bsp(&map);
        assert!(!nodes.segs.is_empty());
        assert!(!nodes.subsectors.is_empty());
        assert!(nodes.nodes.len() <= 1);
    }

    #[test]
    fn two_rooms_may_split_segs() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 512.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let nodes = build_bsp(&map);
        assert!(nodes.segs.len() >= 8);
        assert!(!nodes.subsectors.is_empty());
        // Extra verts only if a split actually occurred.
        let _ = nodes.extra_vertices.len();
    }

    #[test]
    fn two_sided_line_sides_never_share_a_leaf() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 512.0, 256.0), 32.0, 128.0)
            .room("c", rect(512.0, 0.0, 768.0, 256.0), 64.0, 128.0)
            .room("d", rect(256.0, 256.0, 512.0, 512.0), 16.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        // Doom winding (sectors right of their lines); `MapBuilder` winds outward.
        let mut map = map;
        for line in &mut map.lines {
            std::mem::swap(&mut line.v1, &mut line.v2);
            line.delta = -line.delta;
            if let (Some(front), Some(back)) = (line.front_sector, line.back_sector) {
                line.sides.swap(0, 1);
                (line.front_sector, line.back_sector) = (Some(back), Some(front));
            }
        }
        let nodes = build_bsp(&map);
        for (i, ss) in nodes.subsectors.iter().enumerate() {
            let first = ss.first_seg as usize;
            let mut seen = BTreeSet::new();
            for s in &nodes.segs[first..first + ss.num_segs as usize] {
                let line = s.line.unwrap().raw();
                assert!(
                    seen.insert(line),
                    "subsector {i} holds both sides of line {line}"
                );
            }
        }
    }

    #[test]
    fn snap_fixed_roundtrip() {
        let (x, y) = snap_fixed(1.0 / 3.0, -2.5);
        let expect = (1.0_f64 / 3.0 * 65536.0).round() / 65536.0;
        assert!((x - expect).abs() < 1e-15);
        assert!((y + 2.5).abs() < 1e-15);
    }
}

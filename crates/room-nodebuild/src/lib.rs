//! Internal BSP / GL node builder.
//!
//! M3: recursive bipartition ([`bsp::build_bsp`]) with ZDBSP-style splitter
//! cost. Intersection splits + GL minisegs still TODO. [`build_nodes_fan`]
//! remains for simple per-sector leaves.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod bsp;
mod gl;

use room_core::{LineId, SectorId};
use room_map::{GameNodes, MapGeometry, NF_SUBSECTOR, Node, Seg, Subsector};

pub use bsp::build_bsp;
pub use gl::game_to_gl;

/// Build classic game nodes for `map` (recursive BSP).
pub fn build_nodes(map: &MapGeometry) -> GameNodes {
    build_bsp(map)
}

/// One subsector per sector from that sector's linedefs (no real partitioning).
#[must_use]
pub fn build_nodes_fan(map: &MapGeometry) -> GameNodes {
    let mut segs = Vec::new();
    let mut subsectors = Vec::new();

    for (si, sector) in map.sectors.iter().enumerate() {
        let first = segs.len() as u32;
        let mut count = 0u32;
        for &LineId(li) in &sector.lines {
            let Some(line) = map.lines.get(li as usize) else {
                continue;
            };
            let facing = match (line.front_sector, line.back_sector) {
                (Some(SectorId(s)), _) if s as usize == si => 0i16,
                (_, Some(SectorId(s))) if s as usize == si => 1i16,
                _ => continue,
            };
            let (v1, v2) = if facing == 0 {
                (line.v1, line.v2)
            } else {
                (line.v2, line.v1)
            };
            segs.push(Seg {
                v1,
                v2,
                angle: 0,
                line: Some(LineId::from_raw(li)),
                side: facing,
                offset: 0,
            });
            count += 1;
        }
        if count > 0 {
            subsectors.push(Subsector {
                num_segs: count,
                first_seg: first,
            });
        }
    }

    let nodes = if subsectors.len() <= 1 {
        Vec::new()
    } else {
        let mut nodes = Vec::new();
        let mut left = NF_SUBSECTOR | (subsectors.len() as u32 - 1);
        for i in (0..subsectors.len() - 1).rev() {
            let right = NF_SUBSECTOR | i as u32;
            nodes.push(Node {
                x: 0,
                y: 0,
                dx: 0,
                dy: 1,
                bbox: [[i16::MAX, i16::MIN, i16::MAX, i16::MIN]; 2],
                children: [right, left],
            });
            left = (nodes.len() - 1) as u32;
        }
        nodes
    };

    GameNodes {
        segs,
        subsectors,
        nodes,
        extra_vertices: Vec::new(),
    }
}

/// Attach built nodes onto a map (in place), appending any split vertices,
/// and derive GL nodes with closing minisegs.
pub fn apply_nodes(map: &mut MapGeometry) {
    let mut gn = build_nodes(map);
    for v in gn.extra_vertices.drain(..) {
        map.vertices.push(room_map::Vertex { pos: v });
    }
    let gl = game_to_gl(map, &gn);
    map.gl_nodes = Some(gl);
    map.game_nodes = Some(gn);
}

/// Keep IWAD/embedded game nodes when present; only build when missing.
/// Always ensure [`MapGeometry::gl_nodes`] is derived from the game tree.
pub fn ensure_nodes(map: &mut MapGeometry) {
    if map.game_nodes.is_none() {
        apply_nodes(map);
        return;
    }
    if map.gl_nodes.is_none() {
        if let Some(gn) = map.game_nodes.as_ref() {
            let gl = game_to_gl(map, gn);
            map.gl_nodes = Some(gl);
        }
    }
}

/// Point-in-subsector using BSP ([`NF_SUBSECTOR`] child = subsector).
///
/// Empty node list ⇒ subsector 0.
#[must_use]
pub fn point_in_subsector(nodes: &GameNodes, x: f64, y: f64) -> Option<u32> {
    if nodes.subsectors.is_empty() {
        return None;
    }
    if nodes.nodes.is_empty() {
        return Some(0);
    }
    let mut ni = nodes.nodes.len() - 1;
    loop {
        let node = nodes.nodes.get(ni)?;
        let side = point_on_side(node, x, y);
        let child = node.children[side];
        if child & NF_SUBSECTOR != 0 {
            return Some(child & !NF_SUBSECTOR);
        }
        ni = child as usize;
    }
}

/// Resolve the sector facing a point via BSP + first seg of the subsector.
#[must_use]
pub fn point_in_sector(map: &MapGeometry, x: f64, y: f64) -> Option<SectorId> {
    point_in_sector_with(map, map.game_nodes.as_ref()?, x, y)
}

/// Like [`point_in_sector`] but with an explicit node tree (parity tests).
#[must_use]
pub fn point_in_sector_with(
    map: &MapGeometry,
    nodes: &GameNodes,
    x: f64,
    y: f64,
) -> Option<SectorId> {
    let ss = point_in_subsector(nodes, x, y)?;
    let sub = nodes.subsectors.get(ss as usize)?;
    let start = sub.first_seg as usize;
    let segs = nodes
        .segs
        .get(start..start.checked_add(sub.num_segs as usize)?)
        .filter(|s| !s.is_empty())
        .unwrap_or(nodes.segs.get(start..=start)?);
    let seg_sector = |seg: &room_map::Seg| {
        let line = map.lines.get(seg.line?.raw() as usize)?;
        if seg.side == 0 {
            line.front_sector
        } else {
            line.back_sector
        }
    };
    let first = seg_sector(&segs[0]);
    if segs.iter().all(|s| seg_sector(s) == first) {
        return first;
    }
    // A degenerate leaf holding both sides of a line: the nearest seg that
    // has the point on its inner (right) side owns it.
    segs.iter()
        .filter_map(|seg| {
            let a = map.vertices.get(seg.v1.raw() as usize)?.pos;
            let b = map.vertices.get(seg.v2.raw() as usize)?.pos;
            let (dx, dy) = (b.x() - a.x(), b.y() - a.y());
            let len = dx.hypot(dy);
            if len < 1e-9 {
                return None;
            }
            let cross = (dx * (y - a.y()) - dy * (x - a.x())) / len;
            (cross <= 0.0).then(|| (-cross, seg_sector(seg)))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(first, |(_, s)| s)
}

/// Result of [`validate_subsectors`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidateReport {
    /// Subsectors that failed closure or convexity.
    pub failures: Vec<String>,
}

impl ValidateReport {
    /// True when every subsector passed.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Check each subsector's seg polygon is closed and convex (same-turn winding).
///
/// Degenerate / collinear edges are tolerated. Open chains and reflex turns fail.
#[must_use]
pub fn validate_subsectors(map: &MapGeometry, nodes: &GameNodes) -> ValidateReport {
    let mut failures = Vec::new();
    for (si, sub) in nodes.subsectors.iter().enumerate() {
        let start = sub.first_seg as usize;
        let end = start + sub.num_segs as usize;
        if end > nodes.segs.len() || sub.num_segs == 0 {
            failures.push(format!("subsector {si}: bad seg range"));
            continue;
        }
        let segs = &nodes.segs[start..end];
        // Collect ordered vertex positions.
        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(segs.len() + 1);
        for (i, seg) in segs.iter().enumerate() {
            let Some(v1) = map.vertices.get(seg.v1.raw() as usize) else {
                failures.push(format!("subsector {si}: missing v1 on seg {i}"));
                continue;
            };
            if i == 0 {
                pts.push((v1.pos.x(), v1.pos.y()));
            } else {
                let (px, py) = pts[pts.len() - 1];
                // Closure between consecutive segs: previous v2 == this v1.
                if (px - v1.pos.x()).abs() > 1e-6 || (py - v1.pos.y()).abs() > 1e-6 {
                    // Doom segs are not always chained in lump order; try to
                    // rebuild by matching endpoints rather than failing hard.
                    // Fall through: rebuild polygon below.
                    pts.clear();
                    break;
                }
            }
            let Some(v2) = map.vertices.get(seg.v2.raw() as usize) else {
                failures.push(format!("subsector {si}: missing v2 on seg {i}"));
                pts.clear();
                break;
            };
            pts.push((v2.pos.x(), v2.pos.y()));
        }
        if pts.is_empty() {
            // Rebuild: walk segs by matching endpoints (O(n²), n is small).
            pts = match order_seg_polygon(map, segs) {
                Ok(p) => p,
                Err(e) => {
                    failures.push(format!("subsector {si}: {e}"));
                    continue;
                }
            };
        }
        if pts.len() < 4 {
            // Need ≥3 edges ⇒ ≥4 points including close.
            if pts.len() >= 2 {
                let (fx, fy) = pts[0];
                let (lx, ly) = pts[pts.len() - 1];
                if (fx - lx).abs() > 1e-3 || (fy - ly).abs() > 1e-3 {
                    failures.push(format!("subsector {si}: not closed"));
                }
            }
            continue;
        }
        let (fx, fy) = pts[0];
        let (lx, ly) = pts[pts.len() - 1];
        if (fx - lx).abs() > 1e-3 || (fy - ly).abs() > 1e-3 {
            failures.push(format!("subsector {si}: not closed"));
            continue;
        }
        // Drop duplicate closing vertex for turn tests.
        let ring = &pts[..pts.len() - 1];
        if ring.len() < 3 {
            continue;
        }
        if !is_convex(ring) {
            failures.push(format!("subsector {si}: not convex"));
        }
    }
    ValidateReport { failures }
}

fn order_seg_polygon(map: &MapGeometry, segs: &[Seg]) -> Result<Vec<(f64, f64)>, &'static str> {
    if segs.is_empty() {
        return Err("empty segs");
    }
    let mut used = vec![false; segs.len()];
    let mut pts = Vec::with_capacity(segs.len() + 1);
    used[0] = true;
    let v1 = map
        .vertices
        .get(segs[0].v1.raw() as usize)
        .ok_or("missing vertex")?;
    let v2 = map
        .vertices
        .get(segs[0].v2.raw() as usize)
        .ok_or("missing vertex")?;
    pts.push((v1.pos.x(), v1.pos.y()));
    pts.push((v2.pos.x(), v2.pos.y()));
    let start = (v1.pos.x(), v1.pos.y());
    for _ in 1..segs.len() {
        let (cx, cy) = pts[pts.len() - 1];
        let mut found = None;
        for (i, seg) in segs.iter().enumerate() {
            if used[i] {
                continue;
            }
            let Some(a) = map.vertices.get(seg.v1.raw() as usize) else {
                continue;
            };
            let Some(b) = map.vertices.get(seg.v2.raw() as usize) else {
                continue;
            };
            if (a.pos.x() - cx).abs() < 1e-6 && (a.pos.y() - cy).abs() < 1e-6 {
                found = Some((i, (b.pos.x(), b.pos.y())));
                break;
            }
            if (b.pos.x() - cx).abs() < 1e-6 && (b.pos.y() - cy).abs() < 1e-6 {
                found = Some((i, (a.pos.x(), a.pos.y())));
                break;
            }
        }
        let Some((i, next)) = found else {
            return Err("open chain");
        };
        used[i] = true;
        pts.push(next);
    }
    let (lx, ly) = pts[pts.len() - 1];
    if (lx - start.0).abs() > 1e-3 || (ly - start.1).abs() > 1e-3 {
        return Err("not closed");
    }
    Ok(pts)
}

fn is_convex(ring: &[(f64, f64)]) -> bool {
    let n = ring.len();
    let mut sign = 0i8;
    for i in 0..n {
        let (ax, ay) = ring[i];
        let (bx, by) = ring[(i + 1) % n];
        let (cx, cy) = ring[(i + 2) % n];
        let cross = (bx - ax) * (cy - by) - (by - ay) * (cx - bx);
        if cross.abs() < 1e-9 {
            continue;
        }
        let s = if cross > 0.0 { 1i8 } else { -1i8 };
        if sign == 0 {
            sign = s;
        } else if s != sign {
            return false;
        }
    }
    true
}

fn point_on_side(node: &Node, x: f64, y: f64) -> usize {
    let dx = f64::from(node.dx);
    let dy = f64::from(node.dy);
    if dx == 0.0 {
        return if x <= f64::from(node.x) {
            if dy > 0.0 { 1 } else { 0 }
        } else if dy < 0.0 {
            1
        } else {
            0
        };
    }
    if dy == 0.0 {
        return if y <= f64::from(node.y) {
            if dx < 0.0 { 1 } else { 0 }
        } else if dx > 0.0 {
            1
        } else {
            0
        };
    }
    // Cross product — match Doom `R_PointOnSide`: right < left ⇒ front (0).
    let left = (x - f64::from(node.x)) * dy;
    let right = (y - f64::from(node.y)) * dx;
    if right < left { 0 } else { 1 }
}

const FRACBITS: i32 = 16;

#[inline]
fn fixed_mul(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> FRACBITS) as i32
}

/// Doom `R_PointOnSide` using 16.16 fixed point (node fields are map-unit i16).
#[must_use]
pub fn point_on_side_vanilla(node: &Node, x: i32, y: i32) -> usize {
    let nx = i32::from(node.x) << FRACBITS;
    let ny = i32::from(node.y) << FRACBITS;
    let ndx = i32::from(node.dx) << FRACBITS;
    let ndy = i32::from(node.dy) << FRACBITS;

    if ndx == 0 {
        return if x <= nx {
            usize::from(ndy > 0)
        } else {
            usize::from(ndy < 0)
        };
    }
    if ndy == 0 {
        return if y <= ny {
            usize::from(ndx < 0)
        } else {
            usize::from(ndx > 0)
        };
    }

    let dx = x - nx;
    let dy = y - ny;

    // Sign-bit quick path from linuxdoom `r_main.c`.
    if (ndy ^ ndx ^ dx ^ dy) & i32::MIN != 0 {
        return if (ndy ^ dx) & i32::MIN != 0 { 1 } else { 0 };
    }

    let left = fixed_mul(ndy >> FRACBITS, dx);
    let right = fixed_mul(dy, ndx >> FRACBITS);
    if right < left { 0 } else { 1 }
}

/// Subsector via vanilla fixed-point side tests on `nodes`.
#[must_use]
pub fn point_in_subsector_vanilla(nodes: &GameNodes, x: f64, y: f64) -> Option<u32> {
    if nodes.subsectors.is_empty() {
        return None;
    }
    if nodes.nodes.is_empty() {
        return Some(0);
    }
    let xf = (x * f64::from(1 << FRACBITS)).round() as i32;
    let yf = (y * f64::from(1 << FRACBITS)).round() as i32;
    let mut ni = nodes.nodes.len() - 1;
    loop {
        let node = nodes.nodes.get(ni)?;
        let side = point_on_side_vanilla(node, xf, yf);
        let child = node.children[side];
        if child & NF_SUBSECTOR != 0 {
            return Some(child & !NF_SUBSECTOR);
        }
        ni = child as usize;
    }
}

/// Sector via vanilla BSP walk + first seg (parity oracle for original nodes).
#[must_use]
pub fn point_in_sector_vanilla(map: &MapGeometry, x: f64, y: f64) -> Option<SectorId> {
    let nodes = map.game_nodes.as_ref()?;
    let ss = point_in_subsector_vanilla(nodes, x, y)?;
    let sub = nodes.subsectors.get(ss as usize)?;
    let seg = nodes.segs.get(sub.first_seg as usize)?;
    let line = map.lines.get(seg.line?.raw() as usize)?;
    if seg.side == 0 {
        line.front_sector
    } else {
        line.back_sector
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_map::{
        MapFormat, RawLinedef, RawSector, RawSidedef, RawThing, RawVertex, link_geometry,
    };

    #[test]
    fn fan_for_square_room() {
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
        let nodes = build_nodes_fan(&map);
        assert_eq!(nodes.subsectors.len(), 1);
        assert_eq!(nodes.segs.len(), 4);
        assert_eq!(point_in_subsector(&nodes, 64.0, 64.0), Some(0));
        let report = validate_subsectors(&map, &nodes);
        assert!(report.ok(), "{:?}", report.failures);

        let bsp = build_nodes(&map);
        assert!(!bsp.segs.is_empty());
        assert!(!bsp.subsectors.is_empty());
    }
}

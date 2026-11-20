//! Line-of-sight (`P_CheckSight` / `P_CrossBSPNode`).

use crate::actor::ActorId;
use crate::world::World;
use room_core::{DVec2, FRACBITS, Fixed, VertexId};
use room_map::{GameNodes, NF_SUBSECTOR};

/// True if `from` can see `to` (REJECT + BSP segs + Doom opening slopes).
pub fn check_sight(world: &World, from: ActorId, to: ActorId) -> bool {
    let Some(a) = world.actors.get(from) else {
        return false;
    };
    let Some(b) = world.actors.get(to) else {
        return false;
    };
    if world.map.setup.portal_groups.has_links()
        || world
            .map
            .setup
            .portals
            .lines
            .iter()
            .any(|p| p.target.is_some())
    {
        return crate::sight_portal::check_sight_portals(world, a, b);
    }
    let p0 = DVec2::new(a.pos.x(), a.pos.y());
    let p1 = DVec2::new(b.pos.x(), b.pos.y());

    // Doom: REJECT from each mobj's sector (we re-point via vanilla BSP).
    if let (Some(s0), Some(s1)) = (
        room_nodebuild::point_in_sector_vanilla(&world.map, p0.x(), p0.y())
            .or_else(|| room_nodebuild::point_in_sector(&world.map, p0.x(), p0.y())),
        room_nodebuild::point_in_sector_vanilla(&world.map, p1.x(), p1.y())
            .or_else(|| room_nodebuild::point_in_sector(&world.map, p1.x(), p1.y())),
    ) {
        if s0 != s1 && same_portal_group(world, s0.raw() as usize, s1.raw() as usize) {
            if let Some(reject) = world.map.reject.as_ref() {
                if reject.rejects(s0.raw() as usize, s1.raw() as usize) {
                    return false;
                }
            }
        }
    }

    let Some(nodes) = world.map.game_nodes.as_ref() else {
        return true;
    };
    if nodes.nodes.is_empty() && nodes.subsectors.is_empty() {
        return true;
    }

    let sight_z = Fixed::from_f64(a.pos.z() + a.height) - Fixed(Fixed::from_f64(a.height).0 >> 2);
    let mut ctx = SightTrace {
        p0x: Fixed::from_f64(p0.x()).0,
        p0y: Fixed::from_f64(p0.y()).0,
        p1x: Fixed::from_f64(p1.x()).0,
        p1y: Fixed::from_f64(p1.y()).0,
        sight_z,
        top_slope: Fixed::from_f64(b.pos.z() + b.height) - sight_z,
        bottom_slope: Fixed::from_f64(b.pos.z()) - sight_z,
        visited: vec![false; world.map.lines.len()],
    };
    if nodes.nodes.is_empty() {
        return cross_subsector(world, nodes, &mut ctx, 0);
    }
    cross_bsp_node(world, nodes, &mut ctx, nodes.nodes.len() as u32 - 1)
}

struct SightTrace {
    p0x: i32,
    p0y: i32,
    p1x: i32,
    p1y: i32,
    sight_z: Fixed,
    top_slope: Fixed,
    bottom_slope: Fixed,
    visited: Vec<bool>,
}

fn cross_bsp_node(world: &World, nodes: &GameNodes, ctx: &mut SightTrace, bspnum: u32) -> bool {
    if bspnum & NF_SUBSECTOR != 0 {
        let ss = if bspnum == u32::MAX {
            0
        } else {
            bspnum & !NF_SUBSECTOR
        };
        return cross_subsector(world, nodes, ctx, ss);
    }
    let Some(node) = nodes.nodes.get(bspnum as usize) else {
        return false;
    };
    let nx = i32::from(node.x) << FRACBITS;
    let ny = i32::from(node.y) << FRACBITS;
    let ndx = i32::from(node.dx) << FRACBITS;
    let ndy = i32::from(node.dy) << FRACBITS;
    let mut side = p_divline_side(ctx.p0x, ctx.p0y, nx, ny, ndx, ndy);
    if side == 2 {
        side = 0;
    }
    if !cross_bsp_node(world, nodes, ctx, node.children[side as usize]) {
        return false;
    }
    if side == p_divline_side(ctx.p1x, ctx.p1y, nx, ny, ndx, ndy) {
        return true;
    }
    cross_bsp_node(world, nodes, ctx, node.children[(side as usize) ^ 1])
}

fn cross_subsector(world: &World, nodes: &GameNodes, ctx: &mut SightTrace, ss: u32) -> bool {
    let Some(sub) = nodes.subsectors.get(ss as usize) else {
        return false;
    };
    let start = sub.first_seg as usize;
    let end = start.saturating_add(sub.num_segs as usize);
    for seg in nodes.segs.get(start..end).into_iter().flatten() {
        let Some(lid) = seg.line else {
            continue;
        };
        let li = lid.raw() as usize;
        if ctx.visited.get(li).copied().unwrap_or(true) {
            continue;
        }
        if let Some(v) = ctx.visited.get_mut(li) {
            *v = true;
        }
        let Some(line) = world.map.lines.get(li) else {
            continue;
        };
        let Some((v1x, v1y)) = vertex_xy(world, nodes, line.v1) else {
            continue;
        };
        let Some((v2x, v2y)) = vertex_xy(world, nodes, line.v2) else {
            continue;
        };
        if p_divline_side(
            v1x,
            v1y,
            ctx.p0x,
            ctx.p0y,
            ctx.p1x - ctx.p0x,
            ctx.p1y - ctx.p0y,
        ) == p_divline_side(
            v2x,
            v2y,
            ctx.p0x,
            ctx.p0y,
            ctx.p1x - ctx.p0x,
            ctx.p1y - ctx.p0y,
        ) {
            continue;
        }
        if p_divline_side(ctx.p0x, ctx.p0y, v1x, v1y, v2x - v1x, v2y - v1y)
            == p_divline_side(ctx.p1x, ctx.p1y, v1x, v1y, v2x - v1x, v2y - v1y)
        {
            continue;
        }
        if line.back_sector.is_none() {
            if world
                .map
                .setup
                .portals
                .line(lid)
                .is_some_and(|p| p.is_traversable())
            {
                continue;
            }
            return false;
        }
        // UDMF `blocksight` / `ML_BLOCKEVERYTHING` — opaque to LOS.
        if line.block_sight || (world.line_flags(lid) & crate::world::ML_BLOCKEVERYTHING) != 0 {
            return false;
        }
        let (front_id, back_id) = if seg.side == 0 {
            (line.front_sector, line.back_sector)
        } else {
            (line.back_sector, line.front_sector)
        };
        let Some(fs) = front_id.and_then(|s| world.sectors.get(s.raw() as usize)) else {
            return false;
        };
        let Some(bs) = back_id.and_then(|s| world.sectors.get(s.raw() as usize)) else {
            return false;
        };
        let ff = Fixed::from_f64(fs.floor_z);
        let bf = Fixed::from_f64(bs.floor_z);
        let fc = Fixed::from_f64(fs.ceiling_z);
        let bc = Fixed::from_f64(bs.ceiling_z);
        if ff == bf && fc == bc {
            continue;
        }
        let open_top = if fc.0 < bc.0 { fc } else { bc };
        let open_bottom = if ff.0 > bf.0 { ff } else { bf };
        if open_bottom.0 >= open_top.0 {
            return false;
        }
        let frac = p_intercept_vector2(
            [ctx.p0x, ctx.p0y, ctx.p1x - ctx.p0x, ctx.p1y - ctx.p0y],
            [v1x, v1y, v2x - v1x, v2y - v1y],
        );
        if ff != bf {
            let slope = (open_bottom - ctx.sight_z) / frac;
            if slope > ctx.bottom_slope {
                ctx.bottom_slope = slope;
            }
        }
        if fc != bc {
            let slope = (open_top - ctx.sight_z) / frac;
            if slope < ctx.top_slope {
                ctx.top_slope = slope;
            }
        }
        if ctx.top_slope.0 <= ctx.bottom_slope.0 {
            return false;
        }
    }
    true
}

fn same_portal_group(world: &World, s0: usize, s1: usize) -> bool {
    let g = &world.map.setup.portal_groups.sector_group;
    match (g.get(s0), g.get(s1)) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

fn vertex_xy(world: &World, nodes: &GameNodes, id: VertexId) -> Option<(i32, i32)> {
    let i = id.raw() as usize;
    let p = if let Some(v) = world.map.vertices.get(i) {
        v.pos
    } else {
        *nodes
            .extra_vertices
            .get(i.checked_sub(world.map.vertices.len())?)?
    };
    Some((Fixed::from_f64(p.x()).0, Fixed::from_f64(p.y()).0))
}

/// 0 = front, 1 = back, 2 = on (`P_DivlineSide`, including the `!dy` `x==node->y` quirk).
fn p_divline_side(x: i32, y: i32, nx: i32, ny: i32, ndx: i32, ndy: i32) -> i32 {
    if ndx == 0 {
        if x == nx {
            return 2;
        }
        if x <= nx {
            return i32::from(ndy > 0);
        }
        return i32::from(ndy < 0);
    }
    if ndy == 0 {
        if x == ny {
            return 2;
        }
        if y <= ny {
            return i32::from(ndx < 0);
        }
        return i32::from(ndx > 0);
    }
    let dx = x - nx;
    let dy = y - ny;
    let left = (ndy >> FRACBITS) * (dx >> FRACBITS);
    let right = (dy >> FRACBITS) * (ndx >> FRACBITS);
    if right < left {
        0
    } else if left == right {
        2
    } else {
        1
    }
}

/// Doom `P_InterceptVector2` (`v2` = sight `[x,y,dx,dy]`, `v1` = linedef).
fn p_intercept_vector2(sight: [i32; 4], line: [i32; 4]) -> Fixed {
    let v2dx = Fixed(sight[2]);
    let v2dy = Fixed(sight[3]);
    let v1dx = Fixed(line[2]);
    let v1dy = Fixed(line[3]);
    let den = Fixed(v1dy.0 >> 8).mul(v2dx) - Fixed(v1dx.0 >> 8).mul(v2dy);
    if den.0 == 0 {
        return Fixed::ZERO;
    }
    let num =
        Fixed((line[0] - sight[0]) >> 8).mul(v1dy) + Fixed((sight[1] - line[1]) >> 8).mul(v1dx);
    num / den
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::World;
    use room_core::{Angle, DVec3};
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn sight_clear_in_open_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let a = w.thinkers.default[0];
        let b = w
            .spawn_at(
                room_core::ClassId::from_raw(0),
                DVec3::new(192.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        assert!(check_sight(&w, a, b));
    }

    #[test]
    fn blocksight_opaque_on_two_sided_opening() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        for line in &mut map.lines {
            if line.front_sector.is_some() && line.back_sector.is_some() {
                line.block_sight = true;
            }
        }
        let mut w = World::from_map(map).unwrap();
        let player = w.thinkers.default[0];
        let other = w
            .spawn_at(
                room_core::ClassId::from_raw(0),
                DVec3::new(192.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        assert!(
            !check_sight(&w, player, other),
            "blocksight should occlude equal-height openings"
        );
    }

    #[test]
    fn sight_blocked_by_wall_between_rooms() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 384.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let player = w.thinkers.default[0];
        let other = w
            .spawn_at(
                room_core::ClassId::from_raw(0),
                DVec3::new(320.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        assert!(!check_sight(&w, player, other));
    }
}

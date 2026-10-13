//! Walkover line activation (`P_CrossSpecialLine` subset).

use crate::actor::ActorId;
use crate::physics::lines_near;
use crate::specials::{ActivationType, activate_line};
use crate::world::World;
use ferrite_core::{DVec2, DVec3, LineId};

/// After a successful XY move from `from` → `to`, activate crossed specials.
pub fn cross_special_lines(world: &mut World, mo: ActorId, from: DVec2, to: DVec2) {
    if (from.x() - to.x()).abs() < 1e-9 && (from.y() - to.y()).abs() < 1e-9 {
        return;
    }
    if cross_line_portals(world, mo, from, to) {
        return;
    }
    let mid = DVec2::new((from.x() + to.x()) * 0.5, (from.y() + to.y()) * 0.5);
    let radius = {
        let dx = to.x() - from.x();
        let dy = to.y() - from.y();
        (dx * dx + dy * dy).sqrt() * 0.5 + 8.0
    };
    let mut crossed: Vec<LineId> = Vec::new();
    for lid in lines_near(&world.map, mid, radius) {
        if world.line_special(lid) == 0 {
            continue;
        }
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        if segments_cross(from, to, v1.pos, v2.pos) {
            crossed.push(lid);
        }
    }
    for lid in crossed {
        activate_line(world, lid, Some(mo), ActivationType::Cross);
    }
}

/// Teleport through a traversable `Line_SetPortal` pair (kinds 1–3).
fn cross_line_portals(world: &mut World, mo: ActorId, from: DVec2, to: DVec2) -> bool {
    let portals: Vec<_> = world.map.setup.portals.lines.clone();
    for p in portals {
        if !p.is_traversable() {
            continue;
        }
        let Some(dest_id) = p.target else {
            continue;
        };
        let Some(src) = world.map.lines.get(p.line.raw() as usize) else {
            continue;
        };
        let Some(sv1) = world.map.vertices.get(src.v1.raw() as usize) else {
            continue;
        };
        let Some(sv2) = world.map.vertices.get(src.v2.raw() as usize) else {
            continue;
        };
        if !segments_cross(from, to, sv1.pos, sv2.pos) {
            continue;
        }
        let Some(dst) = world.map.lines.get(dest_id.raw() as usize) else {
            continue;
        };
        let Some(dv1) = world.map.vertices.get(dst.v1.raw() as usize) else {
            continue;
        };
        let Some(dv2) = world.map.vertices.get(dst.v2.raw() as usize) else {
            continue;
        };
        let src_mid = DVec2::new(
            (sv1.pos.x() + sv2.pos.x()) * 0.5,
            (sv1.pos.y() + sv2.pos.y()) * 0.5,
        );
        let dst_mid = DVec2::new(
            (dv1.pos.x() + dv2.pos.x()) * 0.5,
            (dv1.pos.y() + dv2.pos.y()) * 0.5,
        );
        let dx = dst_mid.x() - src_mid.x();
        let dy = dst_mid.y() - src_mid.y();
        if let Some(a) = world.actors.get_mut(mo) {
            a.pos = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z());
        }
        return true;
    }
    false
}

/// Proper intersection of open segments a→b and c→d.
fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let d1 = orient(c, d, a);
    let d2 = orient(c, d, b);
    let d3 = orient(a, b, c);
    let d4 = orient(a, b, d);
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

fn orient(a: DVec2, b: DVec2, p: DVec2) -> f64 {
    (b.x() - a.x()) * (p.y() - a.y()) - (b.y() - a.y()) * (p.x() - a.x())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::World;
    use crate::specials::num;
    use ferrite_core::DVec3;
    use ferrite_nodebuild::apply_nodes;
    use ferrite_testkit::{MapBuilder, rect};

    #[test]
    fn crossing_exit_line_sets_exiting() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        // Shared wall at x=128.
        for line in &mut map.lines {
            if line.back_sector.is_none() {
                continue;
            }
            let v1 = map.vertices[line.v1.raw() as usize].pos;
            let v2 = map.vertices[line.v2.raw() as usize].pos;
            if (v1.x() - 128.0).abs() < 1.0 && (v2.x() - 128.0).abs() < 1.0 {
                line.special = num::EXIT_NORMAL as i16;
                line.args = [0; 5];
                break;
            }
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let from = DVec2::new(100.0, 64.0);
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(from.x(), from.y(), 0.0);
        }
        cross_special_lines(&mut w, mo, from, DVec2::new(160.0, 64.0));
        assert!(w.level.exiting);
    }

    fn stamp_paired_portals(map: &mut ferrite_map::MapGeometry) {
        let mut a_east = None;
        let mut b_west = None;
        for (i, line) in map.lines.iter().enumerate() {
            if line.back_sector.is_some() {
                continue;
            }
            let v1 = map.vertices[line.v1.raw() as usize].pos;
            let v2 = map.vertices[line.v2.raw() as usize].pos;
            if (v1.x() - 256.0).abs() < 1.0 && (v2.x() - 256.0).abs() < 1.0 {
                a_east = Some(i);
            }
            if (v1.x() - 512.0).abs() < 1.0 && (v2.x() - 512.0).abs() < 1.0 {
                b_west = Some(i);
            }
        }
        for i in [a_east.unwrap(), b_west.unwrap()] {
            let line = &mut map.lines[i];
            line.special = 156;
            line.args = [42, 0, 1, 0, 0];
            line.tag = 42;
        }
        map.setup = ferrite_map::build_setup(map);
    }

    #[test]
    fn line_portal_teleports_across_pair() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("b", rect(512.0, 0.0, 768.0, 256.0), 0.0, 128.0)
            .player_start(200.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        stamp_paired_portals(&mut map);
        apply_nodes(&mut map);
        assert!(
            map.setup
                .portals
                .lines
                .iter()
                .any(|p| p.is_traversable() && p.target.is_some()),
            "expected paired Line_SetPortal"
        );
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert!(crate::physics::try_move(
            &mut w,
            mo,
            DVec3::new(280.0, 64.0, 0.0)
        ));
        let pos = w.actors.get(mo).unwrap().pos;
        assert!(
            pos.x() > 500.0,
            "should teleport into room b, x={}",
            pos.x()
        );
    }
}

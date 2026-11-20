//! Walkover line activation (`P_CrossSpecialLine` subset).

use crate::actor::ActorId;
use crate::physics::lines_near;
use crate::specials::{ActivationType, activate_line};
use crate::world::World;
use room_core::{DVec2, LineId};

/// After a successful XY move from `from` → `to`, activate crossed specials.
pub fn cross_special_lines(world: &mut World, mo: ActorId, from: DVec2, to: DVec2) {
    if (from.x() - to.x()).abs() < 1e-9 && (from.y() - to.y()).abs() < 1e-9 {
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
    use room_core::DVec3;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

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
}

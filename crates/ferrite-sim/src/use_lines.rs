//! Use-line activation (`P_UseLines` subset).

use crate::actor::ActorId;
use crate::physics::lines_near;
use crate::specials::{ActivationType, activate_line};
use crate::world::World;
use ferrite_core::DVec2;
use ferrite_core::dmath;

/// Doom `USERANGE` ≈ 64.
const USE_RANGE: f64 = 64.0;

/// Trace a short ray from the actor and activate the first usable linedef.
pub fn use_lines(world: &mut World, mo: ActorId) -> bool {
    let Some(actor) = world.actors.get(mo).cloned() else {
        return false;
    };
    let rad = actor.angle.radians();
    let dir = DVec2::new(dmath::cos(rad), dmath::sin(rad));
    let origin = DVec2::new(actor.pos.x(), actor.pos.y());
    let end = DVec2::new(
        origin.x() + dir.x() * USE_RANGE,
        origin.y() + dir.y() * USE_RANGE,
    );
    let mid = DVec2::new((origin.x() + end.x()) * 0.5, (origin.y() + end.y()) * 0.5);
    let mut best: Option<(ferrite_core::LineId, f64)> = None;
    for lid in lines_near(&world.map, mid, USE_RANGE) {
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
        if let Some(t) = ray_hits_seg(origin, end, v1.pos, v2.pos) {
            if best.as_ref().is_none_or(|(_, bt)| t < *bt) {
                best = Some((lid, t));
            }
        }
    }
    if let Some((lid, t)) = best {
        if !cfg!(test) {
            let special = world.line_special(lid);
            eprintln!(
                "ferrite: USE @{tic} mo=#{} line={} special={special} t={t:.2}",
                mo.index(),
                lid.raw(),
                tic = world.level.time,
            );
        }
        // A special was found — even if activation refused (locked door), this
        // is not a "nothing usable" noway case for the player thinker.
        let ok = activate_line(world, lid, Some(mo), ActivationType::Use);
        if ok {
            let cue = world
                .actors
                .get(mo)
                .and_then(|a| world.classes.get(a.class))
                .map(|c| c.use_sound.clone())
                .filter(|s| !s.is_empty());
            if let Some(name) = cue {
                let (x, y, z) = world
                    .actors
                    .get(mo)
                    .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
                    .unwrap_or((0.0, 0.0, 0.0));
                world.play_sound(&name, x, y, z, 1.0);
            }
        }
        true
    } else {
        false
    }
}

/// Parametric hit of ray a→b against segment c→d; returns t in [0,1] along a→b.
fn ray_hits_seg(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<f64> {
    let r = DVec2::new(b.x() - a.x(), b.y() - a.y());
    let s = DVec2::new(d.x() - c.x(), d.y() - c.y());
    let denom = r.x() * s.y() - r.y() * s.x();
    if denom.abs() < 1e-9 {
        return None;
    }
    let qp = DVec2::new(c.x() - a.x(), c.y() - a.y());
    let t = (qp.x() * s.y() - qp.y() * s.x()) / denom;
    let u = (qp.x() * r.y() - qp.y() * r.x()) / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::World;
    use crate::specials::num;
    use ferrite_core::Angle;
    use ferrite_nodebuild::apply_nodes;
    use ferrite_testkit::{MapBuilder, rect};

    #[test]
    fn use_activates_door_special_on_line() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(128.0, 32.0, 90)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        // North wall of the room — find a line near y=256 and give it Door_Raise.
        for line in &mut map.lines {
            let v1 = map.vertices[line.v1.raw() as usize].pos;
            let v2 = map.vertices[line.v2.raw() as usize].pos;
            if (v1.y() - 256.0).abs() < 1.0 && (v2.y() - 256.0).abs() < 1.0 {
                line.special = num::DOOR_RAISE as i16;
                line.args = [1, 16, 150, 0, 0];
                break;
            }
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        // Face north (+Y).
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(90.0);
            a.pos = ferrite_core::DVec3::new(128.0, 200.0, 0.0);
        }
        assert!(use_lines(&mut w, mo));
        assert!(!w.doors.is_empty());
    }

    #[test]
    fn use_plays_activator_use_sound() {
        use crate::{ClassRegistry, StateTable};
        use ferrite_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  UseSound "switches/normbutn"
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(128.0, 32.0, 90)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        for line in &mut map.lines {
            let v1 = map.vertices[line.v1.raw() as usize].pos;
            let v2 = map.vertices[line.v2.raw() as usize].pos;
            if (v1.y() - 256.0).abs() < 1.0 && (v2.y() - 256.0).abs() < 1.0 {
                line.special = num::DOOR_RAISE as i16;
                line.args = [1, 16, 150, 0, 0];
                break;
            }
        }
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(90.0);
            a.pos = ferrite_core::DVec3::new(128.0, 200.0, 0.0);
        }
        assert!(use_lines(&mut w, mo));
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "switches/normbutn")
        );
    }
}

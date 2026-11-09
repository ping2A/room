//! Use-line activation (`P_UseLines` subset).

use crate::actor::ActorId;
use crate::physics::lines_near;
use crate::specials::{ActivationType, activate_line};
use crate::world::World;
use room_core::DVec2;
use room_core::dmath;

/// Doom `USERANGE` ≈ 64.
const USE_RANGE: f64 = 64.0;

fn trace_vehicle_use() -> bool {
    true
}

/// Trace a short ray from the actor and activate the first usable linedef.
pub fn use_lines(world: &mut World, mo: ActorId) -> bool {
    let Some(actor) = world.actors.get(mo).cloned() else {
        return false;
    };
    let rad = actor.angle.radians();
    let dir = DVec2::new(dmath::cos(rad), dmath::sin(rad));
    let origin = DVec2::new(actor.pos.x(), actor.pos.y());
    // `Player.UseRange` replaces Doom's 64.
    let use_range = world
        .classes
        .get(actor.class)
        .map(|c| c.ext.use_range)
        .filter(|r| *r > 0.0)
        .unwrap_or(USE_RANGE);
    let end = DVec2::new(
        origin.x() + dir.x() * use_range,
        origin.y() + dir.y() * use_range,
    );
    let mid = DVec2::new((origin.x() + end.x()) * 0.5, (origin.y() + end.y()) * 0.5);
    // Every crossed line with a special (plus UDMF `blockuse` occluders),
    // nearest first. A `passuse` special line lets the use continue.
    let mut hits: Vec<(room_core::LineId, f64)> = Vec::new();
    for lid in lines_near(&world.map, mid, use_range) {
        let ext = world.map.udmf_user.line_ext(lid.raw() as usize);
        if world.line_special(lid) == 0 && ext & room_map::udmf::ext::LINE_BLOCKUSE == 0 {
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
            hits.push((lid, t));
        }
    }
    hits.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut best: Option<(room_core::LineId, f64)> = None;
    let mut passed = false;
    for (lid, t) in hits {
        let ext = world.map.udmf_user.line_ext(lid.raw() as usize);
        if world.line_special(lid) == 0 {
            // `blockuse` without a special: the use stops dead here.
            break;
        }
        if ext & room_map::udmf::ext::LINE_PASSUSE != 0 && ext & room_map::udmf::ext::LINE_BLOCKUSE == 0 {
            activate_line(world, lid, Some(mo), ActivationType::Use);
            passed = true;
            continue;
        }
        best = Some((lid, t));
        break;
    }
    if let Some((lid, t)) = best {
        if !cfg!(test) {
            let special = world.line_special(lid);
            eprintln!(
                "room: USE @{tic} mo=#{} line={} special={special} t={t:.2}",
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
        passed
    }
}

/// Hexen `UsePuzzleItem` line / thing special number.
const USE_PUZZLE_ITEM: i16 = 129;

/// `P_UsePuzzleItem`: the nearest line in use range (or a thing within it)
/// whose special is `UsePuzzleItem` with `arg0 == item` runs ACS script
/// `arg1` with `arg2..arg4`, and the special is cleared. Returns whether a
/// match was found.
pub fn use_puzzle_item(world: &mut World, mo: ActorId, item: i32) -> bool {
    let Some(actor) = world.actors.get(mo).cloned() else {
        return false;
    };
    let rad = actor.angle.radians();
    let dir = DVec2::new(dmath::cos(rad), dmath::sin(rad));
    let origin = DVec2::new(actor.pos.x(), actor.pos.y());
    let use_range = world
        .classes
        .get(actor.class)
        .map(|c| c.ext.use_range)
        .filter(|r| *r > 0.0)
        .unwrap_or(USE_RANGE);
    let end = DVec2::new(
        origin.x() + dir.x() * use_range,
        origin.y() + dir.y() * use_range,
    );
    let mid = DVec2::new((origin.x() + end.x()) * 0.5, (origin.y() + end.y()) * 0.5);
    let mut best: Option<(room_core::LineId, f64)> = None;
    for lid in lines_near(&world.map, mid, use_range) {
        if world.line_special(lid) != USE_PUZZLE_ITEM || world.line_args(lid)[0] != item {
            continue;
        }
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        let (Some(v1), Some(v2)) = (
            world.map.vertices.get(line.v1.raw() as usize),
            world.map.vertices.get(line.v2.raw() as usize),
        ) else {
            continue;
        };
        if let Some(t) = ray_hits_seg(origin, end, v1.pos, v2.pos) {
            if best.as_ref().is_none_or(|(_, bt)| t < *bt) {
                best = Some((lid, t));
            }
        }
    }
    if let Some((lid, _)) = best {
        let a = world.line_args(lid);
        world.line_specials.insert(lid.raw(), 0);
        let script = [a[1], 0, a[2], a[3], a[4]];
        let _ = crate::acs::acs_execute(world, &script, false, Some(mo), Some(lid));
        return true;
    }
    // Things carrying the special (`P_UsePuzzleItem` also checks actors).
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        let Some(t) = world.actors.get(id) else {
            continue;
        };
        if t.destroyed || i32::from(t.special) != i32::from(USE_PUZZLE_ITEM) || t.args[0] != item {
            continue;
        }
        let dx = t.pos.x() - origin.x();
        let dy = t.pos.y() - origin.y();
        if dx * dir.x() + dy * dir.y() < 0.0
            || dmath::sqrt(dx * dx + dy * dy) > use_range + t.radius
        {
            continue;
        }
        let a = t.args;
        if let Some(t) = world.actors.get_mut(id) {
            t.special = 0;
        }
        let script = [a[1], 0, a[2], a[3], a[4]];
        let _ = crate::acs::acs_execute(world, &script, false, Some(mo), None);
        return true;
    }
    false
}

/// Use the nearest `+USESPECIAL` actor in front of `mo`.
///
/// Actors normally expose a `Use` state. Vehicle classes (including the
/// Brutal Doom `Tank` base) expose `Active` instead; this is the state their
/// native use handler activates in GZDoom-compatible content.
pub fn use_actors(world: &mut World, mo: ActorId) -> bool {
    let Some(user) = world.actors.get(mo).cloned() else {
        return false;
    };
    let is_vehicle_rider = world
        .classes
        .get(user.class)
        .is_some_and(|class| class.name.eq_ignore_ascii_case("PlayerBike"));
    if is_vehicle_rider {
        // Let Brutal Doom's vehicle-control ACS see USE so it can dismount.
        // Selecting the bike again here would consume the key before ACS.
        eprintln!(
            "room: VEHICLE USE tic={} rider=#{} class=PlayerBike forwarding USE to vehicle ACS",
            world.level.time,
            mo.index(),
        );
        return false;
    }
    let rad = user.angle.radians();
    let dir = DVec2::new(dmath::cos(rad), dmath::sin(rad));
    let origin = DVec2::new(user.pos.x(), user.pos.y());
    if trace_vehicle_use() {
        let class = world
            .classes
            .get(user.class)
            .map(|c| c.name.as_str())
            .unwrap_or("<missing>");
        eprintln!(
            "room: VEHICLE USE tic={} player=#{} class={} pos=({:.1},{:.1},{:.1}) angle={:.1} use_down={}",
            world.level.time,
            mo.index(),
            class,
            user.pos.x(),
            user.pos.y(),
            user.pos.z(),
            user.angle.degrees(),
            world
                .players
                .iter()
                .flatten()
                .find(|p| p.mo == Some(mo))
                .is_some_and(|p| p.use_down),
        );
    }
    let mut best: Option<(ActorId, f64)> = None;
    for &id in &world.thinkers.default {
        if id == mo {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        let vehicle = class_is_tank_derived(world, a.class);
        let class_name = world
            .classes
            .get(a.class)
            .map(|c| c.name.as_str())
            .unwrap_or("<missing>");
        if a.destroyed
            || a.health <= 0
            || ((a.flags & crate::class::FLAG_USESPECIAL) == 0 && !vehicle)
        {
            continue;
        }
        let dx = a.pos.x() - origin.x();
        let dy = a.pos.y() - origin.y();
        let dist = dx.hypot(dy);
        let actor_range = USE_RANGE + a.radius + if vehicle { 32.0 } else { 0.0 };
        if trace_vehicle_use() && vehicle {
            let forward = dx * dir.x() + dy * dir.y();
            eprintln!(
                "room: VEHICLE CANDIDATE tic={} actor=#{} class={} hp={} tid={} flags_use={} vehicle={} dist={:.1}/{:.1} forward={:.1}",
                world.level.time,
                id.index(),
                class_name,
                a.health,
                a.tid,
                (a.flags & crate::class::FLAG_USESPECIAL) != 0,
                vehicle,
                dist,
                actor_range,
                forward,
            );
        }
        if dist > actor_range {
            continue;
        }
        if dx * dir.x() + dy * dir.y() < -8.0 {
            continue;
        }
        if best.as_ref().is_none_or(|(_, d)| dist < *d) {
            best = Some((id, dist));
        }
    }
    let Some((id, _)) = best else {
        if trace_vehicle_use() {
            eprintln!(
                "room: VEHICLE USE tic={} result=no eligible actor",
                world.level.time
            );
        }
        return false;
    };
    if let Some(a) = world.actors.get_mut(id) {
        a.target = Some(mo);
    }
    let Some(class) = world.actors.get(id).map(|a| a.class) else {
        return false;
    };
    let use_state = world.states.find_label(class, "Use");
    let st = use_state.or_else(|| world.states.find_label(class, "Active"));
    let Some(st) = st else {
        return false;
    };
    if use_state.is_none() && class_is_tank_derived(world, class) {
        // Brutal Doom's vehicle ACS hardcodes TID 261 as the active vehicle
        // for GetVehiclePreviousHealth / Thing_Move. Map bikes briefly share
        // that TID during their spawn states, so reserve it for the one the
        // player just used before Active starts.
        let mut cleared_tids = Vec::new();
        for &other in &world.thinkers.default {
            if other != id && world.actors.get(other).is_some_and(|a| a.tid == 261) {
                if let Some(a) = world.actors.get_mut(other) {
                    a.tid = 0;
                    cleared_tids.push(other.index());
                }
            }
        }
        if let Some(a) = world.actors.get_mut(id) {
            a.tid = 261;
        }
        if trace_vehicle_use() {
            eprintln!(
                "room: VEHICLE TID reserve tic={} selected=#{} cleared_tid261={:?}",
                world.level.time,
                id.index(),
                cleared_tids,
            );
        }
    }
    if trace_vehicle_use() {
        if let Some(actor) = world.actors.get(id) {
            let name = world
                .classes
                .get(class)
                .map(|c| c.name.as_str())
                .unwrap_or("<missing>");
            eprintln!(
                "room: VEHICLE USE tic={} selected=#{} class={} state={} target=#{} tid={} hp={} pos=({:.1},{:.1},{:.1})",
                world.level.time,
                id.index(),
                name,
                if use_state.is_some() { "Use" } else { "Active" },
                mo.index(),
                actor.tid,
                actor.health,
                actor.pos.x(),
                actor.pos.y(),
                actor.pos.z(),
            );
        }
    }
    world.set_state(id, Some(st), true);
    if class_is_tank_derived(world, class)
        && world
            .classes
            .get(class)
            .is_some_and(|def| def.name.eq_ignore_ascii_case("Bike"))
    {
        // BIKERACE's BikePowerup is the same PowerupGiver used by the mod's
        // touch-to-mount path. A_Activate/Active only starts the bike's health
        // monitor; mounting also needs to apply its PowerMorph.PlayerClass.
        if let Some(giver) = world.classes.find_name("BikePowerup") {
            let granted = crate::inventory::give_inventory(world, mo, giver, 1);
            let morphed = granted && crate::action::apply_powerup_giver(world, mo, giver);
            eprintln!(
                "room: VEHICLE MOUNT tic={} bike=#{} rider=#{} giver=BikePowerup granted={} morphed={}",
                world.level.time,
                id.index(),
                mo.index(),
                granted,
                morphed,
            );
        } else {
            eprintln!(
                "room: VEHICLE MOUNT tic={} bike=#{} BikePowerup class missing",
                world.level.time,
                id.index(),
            );
        }
    }
    true
}

fn class_is_tank_derived(world: &World, class: room_core::ClassId) -> bool {
    let mut current = Some(class);
    for _ in 0..64 {
        let Some(def) = current.and_then(|id| world.classes.get(id)) else {
            return false;
        };
        if def.name.eq_ignore_ascii_case("Tank") {
            return true;
        }
        current = def.parent;
    }
    false
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
    use room_core::Angle;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

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
            a.pos = room_core::DVec3::new(128.0, 200.0, 0.0);
        }
        assert!(use_lines(&mut w, mo));
        assert!(!w.doors.is_empty());
    }

    fn door_room(ext_on_door: u32, blocker: bool) -> (World, ActorId) {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(128.0, 32.0, 90)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        let mut door = 0;
        for (i, line) in map.lines.iter_mut().enumerate() {
            let v1 = map.vertices[line.v1.raw() as usize].pos;
            let v2 = map.vertices[line.v2.raw() as usize].pos;
            if (v1.y() - 256.0).abs() < 1.0 && (v2.y() - 256.0).abs() < 1.0 {
                line.special = num::DOOR_RAISE as i16;
                line.args = [1, 16, 150, 0, 0];
                door = i;
                break;
            }
        }
        map.udmf_user.line_ext.resize(map.lines.len(), 0);
        map.udmf_user.line_ext[door] = ext_on_door;
        if blocker {
            // A special-less `blockuse` line between the player and the door.
            let base = map.vertices.len() as u32;
            map.vertices.push(room_map::Vertex { pos: room_core::DVec2::new(64.0, 230.0) });
            map.vertices.push(room_map::Vertex { pos: room_core::DVec2::new(192.0, 230.0) });
            let mut l = map.lines[door].clone();
            l.v1 = room_core::VertexId::from_raw(base);
            l.v2 = room_core::VertexId::from_raw(base + 1);
            l.delta = room_core::DVec2::new(128.0, 0.0);
            l.special = 0;
            map.lines.push(l);
            map.udmf_user.line_ext.push(room_map::udmf::ext::LINE_BLOCKUSE);
            map.blockmap = room_map::blockmap::build_blockmap(&map.vertices, &map.lines);
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(90.0);
            a.pos = room_core::DVec3::new(128.0, 200.0, 0.0);
        }
        (w, mo)
    }

    #[test]
    fn udmf_blockuse_line_stops_the_use_trace() {
        let (mut w, mo) = door_room(0, true);
        assert!(!use_lines(&mut w, mo));
        assert!(w.doors.is_empty());
    }

    #[test]
    fn udmf_passuse_special_still_counts_as_a_use() {
        let (mut w, mo) = door_room(room_map::udmf::ext::LINE_PASSUSE, false);
        assert!(use_lines(&mut w, mo));
        assert!(!w.doors.is_empty());
    }

    #[test]
    fn use_plays_activator_use_sound() {
        use crate::{ClassRegistry, StateTable};
        use room_decorate::parse_decorate;
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
            a.pos = room_core::DVec3::new(128.0, 200.0, 0.0);
        }
        assert!(use_lines(&mut w, mo));
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "switches/normbutn")
        );
    }

    #[test]
    fn use_actor_takes_money_and_prints_a_tip() {
        use crate::{ClassRegistry, StateTable, count_inventory, give_inventory};
        use room_core::DVec3;
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor DukeMoney : Actor
{
  Inventory.Amount 1
  Inventory.MaxAmount 999
  +COUNTITEM
  States { Spawn: CLIP A -1 Stop }
}
actor TipBabe : Actor 12345
{
  Radius 16
  Height 56
  +USESPECIAL
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Use:
    PLAY A 0 A_JumpIfInTargetInventory("DukeMoney", 1, "Tip")
    PLAY A 1 A_Print("She wants a tip.")
    Goto Spawn
  Tip:
    PLAY A 0 A_TakeFromTarget("DukeMoney", 1)
    PLAY A 1 A_Print("You tip her.")
    Goto Spawn
  }
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
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let babe_c = w.classes.find_name("TipBabe").unwrap();
        let money = w.classes.find_name("DukeMoney").unwrap();
        let babe = w
            .spawn_at(
                babe_c,
                DVec3::new(128.0, 80.0, 0.0),
                Angle::from_degrees(270.0),
            )
            .unwrap();
        assert!(
            w.actors
                .get(babe)
                .is_some_and(|a| a.flags & crate::class::FLAG_USESPECIAL != 0),
            "babe should carry +USESPECIAL"
        );
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(90.0);
            a.pos = DVec3::new(128.0, 32.0, 0.0);
        }
        assert!(give_inventory(&mut w, mo, money, 3));
        assert_eq!(count_inventory(&w, mo, money), 3);
        assert!(use_actors(&mut w, mo));
        assert_eq!(count_inventory(&w, mo, money), 2);
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m.contains("You tip her.")),
            "tip message missing"
        );
        assert!(use_actors(&mut w, mo));
        assert_eq!(count_inventory(&w, mo, money), 1);
    }

    #[test]
    fn use_actor_without_money_asks_for_a_tip() {
        use crate::{ClassRegistry, StateTable};
        use room_core::DVec3;
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor DukeMoney : Actor
{
  Inventory.Amount 1
  Inventory.MaxAmount 999
  States { Spawn: CLIP A -1 Stop }
}
actor TipBabe : Actor 12345
{
  Radius 16
  Height 56
  +USESPECIAL
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Use:
    PLAY A 0 A_JumpIfInTargetInventory("DukeMoney", 1, "Tip")
    PLAY A 1 A_Print("She wants a tip.")
    Goto Spawn
  Tip:
    PLAY A 0 A_TakeFromTarget("DukeMoney", 1)
    PLAY A 1 A_Print("You tip her.")
    Goto Spawn
  }
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
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let babe_c = w.classes.find_name("TipBabe").unwrap();
        w.spawn_at(
            babe_c,
            DVec3::new(128.0, 80.0, 0.0),
            Angle::from_degrees(270.0),
        )
        .unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(90.0);
            a.pos = DVec3::new(128.0, 32.0, 0.0);
        }
        assert!(use_actors(&mut w, mo));
        assert!(
            w.drain_messages()
                .iter()
                .any(|m| m.contains("She wants a tip.")),
            "ask-for-tip message missing"
        );
    }

    #[test]
    fn use_fountain_heals_the_player_up_to_the_cap() {
        use crate::{ClassRegistry, StateTable};
        use room_core::DVec3;
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor Fountain : Actor 12346
{
  Radius 16
  Height 32
  +USESPECIAL
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Use:
    PLAY A 0 A_HealTarget(1, 50)
    PLAY A 1
    Goto Spawn
  }
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
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let fountain = w.classes.find_name("Fountain").unwrap();
        w.spawn_at(
            fountain,
            DVec3::new(128.0, 80.0, 0.0),
            Angle::from_degrees(270.0),
        )
        .unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(90.0);
            a.pos = DVec3::new(128.0, 32.0, 0.0);
            a.health = 20;
        }
        assert!(use_actors(&mut w, mo));
        assert_eq!(w.actors.get(mo).unwrap().health, 21);
        if let Some(a) = w.actors.get_mut(mo) {
            a.health = 80;
        }
        assert!(use_actors(&mut w, mo));
        assert_eq!(w.actors.get(mo).unwrap().health, 80);
    }

    #[test]
    fn use_actor_skips_a_dead_fixture() {
        use crate::{ClassRegistry, StateTable};
        use room_core::DVec3;
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor Fountain : Actor 12346
{
  Radius 16
  Height 32
  +USESPECIAL
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Use:
    PLAY A 0 A_HealTarget(1, 50)
    PLAY A 1
    Goto Spawn
  }
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
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let fountain = w.classes.find_name("Fountain").unwrap();
        let id = w
            .spawn_at(
                fountain,
                DVec3::new(128.0, 80.0, 0.0),
                Angle::from_degrees(270.0),
            )
            .unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(90.0);
            a.pos = DVec3::new(128.0, 32.0, 0.0);
            a.health = 20;
        }
        if let Some(a) = w.actors.get_mut(id) {
            a.health = 0;
        }
        assert!(!use_actors(&mut w, mo));
        assert_eq!(w.actors.get(mo).unwrap().health, 20);
    }
}

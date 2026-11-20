//! Playsim through portals, on the `room-testkit` portal maps.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use crate::World;
use crate::actor::ActorId;
use crate::class::{FLAG_NOGRAVITY, FLAG_SHOOTABLE, FLAG_SOLID};
use room_core::{Angle, DVec3};
use room_nodebuild::apply_nodes;
use room_testkit::PolyMap;

fn world(m: &PolyMap) -> World {
    let mut map = room_map::load_udmf_text("MAP01", &m.textmap()).unwrap();
    apply_nodes(&mut map);
    World::from_map(map).unwrap()
}

fn spawn(w: &mut World, x: f64, y: f64, z: f64) -> ActorId {
    let id = w
        .spawn_at(
            room_core::ClassId::from_raw(0),
            DVec3::new(x, y, z),
            Angle::ZERO,
        )
        .unwrap();
    let a = w.actors.get_mut(id).unwrap();
    a.radius = 16.0;
    a.height = 56.0;
    id
}

fn player(w: &World) -> ActorId {
    w.players[0].as_ref().unwrap().mo.unwrap()
}

#[test]
fn walking_through_a_linked_portal_comes_out_on_the_other_side() {
    let mut w = world(&room_testkit::linked_line_portal_rooms(3));
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(240.0, 128.0, 0.0);
    assert!(crate::physics::try_move(
        &mut w,
        p,
        DVec3::new(262.0, 128.0, 0.0)
    ));
    let pos = w.actors.get(p).unwrap().pos;
    assert!(
        (pos.x() - 1030.0).abs() < 1e-6 && (pos.y() - 128.0).abs() < 1e-6,
        "{pos:?}"
    );
    assert_eq!(w.portal_moves.len(), 1);
    assert_eq!(w.portal_moves[0].offset, [768.0, 0.0, 0.0]);
    // And back.
    assert!(crate::physics::try_move(
        &mut w,
        p,
        DVec3::new(1010.0, 128.0, 0.0)
    ));
    let pos = w.actors.get(p).unwrap().pos;
    assert!((pos.x() - 242.0).abs() < 1e-6, "{pos:?}");
}

#[test]
fn an_interactive_portal_turns_the_mover() {
    let mut w = world(&room_testkit::rotated_line_portal_rooms(2));
    let p = player(&w);
    {
        let a = w.actors.get_mut(p).unwrap();
        a.pos = DVec3::new(250.0, 128.0, 0.0);
        a.vel = DVec3::new(8.0, 0.0, 0.0);
        a.angle = Angle::from_degrees(0.0);
    }
    assert!(crate::physics::try_move(
        &mut w,
        p,
        DVec3::new(258.0, 128.0, 0.0)
    ));
    let a = w.actors.get(p).unwrap();
    assert!(
        (a.pos.x() - 1152.0).abs() < 1e-6 && (a.pos.y() - 1026.0).abs() < 1e-6,
        "{:?}",
        a.pos
    );
    assert!(a.vel.x().abs() < 1e-9 && (a.vel.y() - 8.0).abs() < 1e-9);
    assert!((a.angle.degrees() - 90.0).abs() < 1e-9);
}

#[test]
fn things_block_across_a_linked_portal() {
    let mut w = world(&room_testkit::linked_line_portal_rooms(3));
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(230.0, 128.0, 0.0);
    // A solid thing just past the portal, in room B.
    let blocker = spawn(&mut w, 1032.0, 128.0, 0.0);
    w.actors.get_mut(blocker).unwrap().flags |= FLAG_SOLID;
    let me = w.actors.get(p).unwrap().clone();
    let r = crate::physics::check_position(&w, &me, DVec3::new(248.0, 128.0, 0.0), Some(p));
    assert!(!r.ok);
    assert_eq!(r.blocking_actor, Some(blocker));
    // Without the portal's other side in play, the empty strip behind the
    // portal does not count.
    let r = crate::physics::check_position(&w, &me, DVec3::new(200.0, 128.0, 0.0), Some(p));
    assert!(r.ok);
}

#[test]
fn sight_and_distance_across_a_linked_portal() {
    let mut w = world(&room_testkit::linked_line_portal_rooms(3));
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(128.0, 128.0, 0.0);
    let m = spawn(&mut w, 1152.0, 128.0, 0.0);
    assert!(crate::check_sight(&w, m, p));
    assert!(crate::check_sight(&w, p, m));
    let (a, b) = (w.actors.get(m).unwrap(), w.actors.get(p).unwrap());
    let v = crate::portal::vec2_to(&w, a, b);
    assert!((v.x() + 256.0).abs() < 1e-9 && v.y().abs() < 1e-9, "{v:?}");
    // A closed door in room B between them breaks the line.
    let mut m2 = PolyMap::new();
    let a = m2.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    m2.rect(256.0, 0.0, 272.0, 256.0, 0.0, 128.0, 0);
    let b = m2.rect(1024.0, 0.0, 1100.0, 256.0, 0.0, 128.0, 0);
    m2.rect(1100.0, 0.0, 1110.0, 256.0, 0.0, 0.0, 0);
    m2.rect(1110.0, 0.0, 1280.0, 256.0, 0.0, 128.0, 0);
    m2.rect(1008.0, 0.0, 1024.0, 256.0, 0.0, 128.0, 0);
    m2.line(
        (256.0, 0.0),
        (256.0, 256.0),
        156,
        [2, 1, 3, 0, 0],
        1,
        Some(a),
    );
    m2.line(
        (1024.0, 0.0),
        (1024.0, 256.0),
        156,
        [1, 2, 3, 0, 0],
        2,
        Some(b),
    );
    m2.player(128.0, 128.0, 0);
    let mut w2 = world(&m2);
    let p2 = player(&w2);
    w2.actors.get_mut(p2).unwrap().pos = DVec3::new(128.0, 128.0, 0.0);
    let m2 = spawn(&mut w2, 1152.0, 128.0, 0.0);
    assert!(!crate::check_sight(&w2, m2, p2));
}

#[test]
fn hitscans_go_through_a_linked_portal() {
    let mut w = world(&room_testkit::linked_line_portal_rooms(3));
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(128.0, 128.0, 0.0);
    let target = spawn(&mut w, 1152.0, 128.0, 0.0);
    {
        let t = w.actors.get_mut(target).unwrap();
        t.flags |= FLAG_SHOOTABLE;
        t.health = 1000;
    }
    let hit = crate::hitscan::line_attack_with_slope(&mut w, p, Angle::ZERO, 2048.0, 5, 0.0);
    assert_eq!(hit.actor, Some(target));
    // Doom hits a thing on its bounding-box diagonal: its centre, straight on.
    assert!(
        (hit.distance - (1152.0 - 768.0 - 128.0)).abs() < 0.1,
        "{}",
        hit.distance
    );
}

#[test]
fn ceilings_and_floors_continue_through_linked_plane_portals() {
    let mut w = world(&room_testkit::linked_plane_portal_rooms());
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(128.0, 128.0, 0.0);
    let me = w.actors.get(p).unwrap().clone();
    let r = crate::physics::check_position(&w, &me, me.pos, Some(p));
    // Room A's ceiling portal opens on room B, whose ceiling is at 512.
    assert_eq!(r.ceiling_z, 512.0);
    assert_eq!(r.floor_z, 0.0);
    // Rising past the portal plane moves the actor into room B.
    let id = spawn(&mut w, 128.0, 128.0, 140.0);
    w.actors.get_mut(id).unwrap().flags |= FLAG_NOGRAVITY;
    assert!(crate::portal::check_portal_transition(&mut w, id));
    let pos = w.actors.get(id).unwrap().pos;
    assert_eq!((pos.x(), pos.y(), pos.z()), (1152.0, 128.0, 140.0));
    // And dropping below B's floor portal moves it back.
    w.actors.get_mut(id).unwrap().pos = DVec3::new(1152.0, 128.0, 100.0);
    assert!(crate::portal::check_portal_transition(&mut w, id));
    assert_eq!(w.actors.get(id).unwrap().pos.x(), 128.0);
}

#[test]
fn noise_alerts_cross_portals() {
    let mut w = world(&room_testkit::linked_line_portal_rooms(3));
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(128.0, 128.0, 0.0);
    crate::action::noise_alert(&mut w, p);
    // Room B is sector 2.
    assert_eq!(w.sectors[2].soundtarget, Some(p));
    let mut w = world(&room_testkit::linked_plane_portal_rooms());
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(128.0, 128.0, 0.0);
    crate::action::noise_alert(&mut w, p);
    assert_eq!(w.sectors[1].soundtarget, Some(p));
}

#[test]
fn line_set_portal_target_at_runtime() {
    let mut m = room_testkit::rotated_line_portal_rooms(1);
    m.line((0.0, 0.0), (256.0, 0.0), 0, [0; 5], 3, None);
    let mut w = world(&m);
    let lid = w
        .map
        .setup
        .portals
        .lines
        .iter()
        .find(|p| p.target.is_some())
        .unwrap()
        .line;
    let args = [1, 0, 0, 0, 0];
    let r = crate::specials::execute_special(
        &mut w,
        crate::specials::num::LINE_SET_PORTAL_TARGET,
        &args,
        None,
        Some(lid),
    );
    assert_eq!(r, 1);
    assert!(w.map.setup.portals.line(lid).unwrap().target.is_none());
}

#[test]
fn things_block_across_a_linked_plane_portal() {
    let mut w = world(&room_testkit::linked_plane_portal_rooms());
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(128.0, 128.0, 0.0);
    // Standing on B's floor, straight above the player across A's ceiling.
    let above = spawn(&mut w, 1152.0 + 8.0, 128.0, 130.0);
    w.actors.get_mut(above).unwrap().flags |= FLAG_SOLID | FLAG_NOGRAVITY;
    let mut me = w.actors.get(p).unwrap().clone();
    me.radius = 16.0;
    me.height = 56.0;
    // Head well below the portal plane: nothing in B is in reach.
    let r = crate::physics::check_position(&w, &me, DVec3::new(128.0, 128.0, 0.0), Some(p));
    assert!(r.ok);
    // Rising so the head passes the plane meets the thing above.
    let r = crate::physics::check_position(&w, &me, DVec3::new(128.0, 128.0, 80.0), Some(p));
    assert!(!r.ok);
    assert_eq!(r.blocking_actor, Some(above));
    let g = crate::portal::group_at(&w, 128.0, 128.0);
    let groups =
        crate::portal::connected_groups(&w, g, DVec3::new(128.0, 128.0, 80.0), 136.0, 16.0);
    assert_eq!(groups.len(), 2, "{groups:?}");
}

#[test]
fn blasts_reach_across_a_linked_portal() {
    let mut w = world(&room_testkit::linked_line_portal_rooms(3));
    let p = player(&w);
    w.actors.get_mut(p).unwrap().pos = DVec3::new(64.0, 64.0, 0.0);
    let bomb = spawn(&mut w, 240.0, 128.0, 0.0);
    // Just past the portal in room B: 40 units from the blast as seen
    // through it, 760 in map coordinates.
    let near = spawn(&mut w, 1040.0, 128.0, 0.0);
    // Deep in room B: out of reach either way.
    let far = spawn(&mut w, 1240.0, 128.0, 0.0);
    for id in [near, far] {
        let a = w.actors.get_mut(id).unwrap();
        a.flags |= FLAG_SHOOTABLE;
        a.health = 1000;
    }
    crate::action::radius_attack(&mut w, bomb, DVec3::new(240.0, 128.0, 0.0), 128, 128.0);
    assert!(w.actors.get(near).unwrap().health < 1000);
    assert_eq!(w.actors.get(far).unwrap().health, 1000);
}

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use super::*;
use room_testkit::PolyMap;

fn load(m: &PolyMap) -> MapGeometry {
    crate::load_udmf_text("MAP01", &m.textmap()).unwrap()
}

fn line_at(map: &MapGeometry, x: f64) -> LineId {
    let i = map
        .lines
        .iter()
        .position(|l| {
            let a = map.vertices[l.v1.raw() as usize].pos;
            let b = map.vertices[l.v2.raw() as usize].pos;
            (a.x() - x).abs() < 1e-6 && (b.x() - x).abs() < 1e-6
        })
        .unwrap();
    LineId::from_raw(i as u32)
}

#[test]
fn linked_line_portals_pair_up_with_a_displacement() {
    let map = load(&room_testkit::linked_line_portal_rooms(3));
    let t = &map.setup.portals;
    assert_eq!(t.lines.len(), 2);
    let a = *t.line(line_at(&map, 256.0)).unwrap();
    let b = *t.line(line_at(&map, 1024.0)).unwrap();
    assert_eq!(a.kind, PORTT_LINKED);
    assert!(a.is_linked() && a.is_interactive() && a.is_traversable());
    assert_eq!(a.target, Some(b.line));
    assert_eq!(a.displacement, (768.0, 0.0));
    assert_eq!(b.displacement, (-768.0, 0.0));
    // Each side (room + empty strip behind the portal) is its own group.
    let g = &map.setup.portal_groups;
    assert!(g.has_links());
    let (ga, gb) = (g.group_of(0).unwrap(), g.group_of(2).unwrap());
    assert_ne!(ga, gb);
    assert_eq!(g.group_of(1), Some(ga));
    assert_eq!(g.group_of(3), Some(gb));
    assert_eq!(g.offset(ga, gb), (768.0, 0.0));
    assert_eq!(g.offset(gb, ga), (-768.0, 0.0));
    assert!(t.has_passable_lines);
}

#[test]
fn a_one_way_linked_portal_becomes_a_teleporter() {
    let mut m = room_testkit::linked_line_portal_rooms(3);
    // B's line no longer points back at A.
    m.line((1024.0, 0.0), (1024.0, 256.0), 0, [0; 5], 2, None);
    let map = load(&m);
    let t = &map.setup.portals;
    assert_eq!(t.lines.len(), 1);
    let a = t.lines[0];
    assert_eq!(a.kind, PORTT_TELEPORT);
    assert!(a.is_traversable() && !a.is_interactive());
    assert!(!map.setup.portal_groups.has_links());
}

#[test]
fn interactive_portals_turn_positions_and_velocities() {
    let map = load(&room_testkit::rotated_line_portal_rooms(2));
    let t = &map.setup.portals;
    let a = *t.line(line_at(&map, 256.0)).unwrap();
    assert_eq!(a.kind, PORTT_INTERACTIVE);
    assert!(a.is_interactive());
    assert_eq!(a.angle_diff, 90.0);
    let (x, y) = a.translate_xy(&map, 250.0, 128.0);
    assert!(
        (x - 1152.0).abs() < 1e-9 && (y - 1018.0).abs() < 1e-9,
        "{x} {y}"
    );
    let (vx, vy) = a.translate_vel(10.0, 0.0);
    assert!(vx.abs() < 1e-9 && (vy - 10.0).abs() < 1e-9);
    assert_eq!(a.translate_angle_deg(0.0), 90.0);
    // Not linked: no portal groups.
    assert!(!map.setup.portal_groups.has_links());
}

#[test]
fn interactive_portals_with_z_alignment_become_teleporters() {
    let mut m = room_testkit::rotated_line_portal_rooms(2);
    m.line(
        (256.0, 0.0),
        (256.0, 256.0),
        156,
        [2, 1, 2, 1, 0],
        1,
        Some(0),
    );
    let map = load(&m);
    let a = *map.setup.portals.line(line_at(&map, 256.0)).unwrap();
    assert_eq!(a.kind, PORTT_TELEPORT);
    assert_eq!(a.align, PORG_FLOOR);
}

#[test]
fn traversable_portals_without_a_back_sector_are_visual() {
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    let b = m.rect(1024.0, 0.0, 1280.0, 256.0, 0.0, 128.0, 0);
    m.line(
        (256.0, 0.0),
        (256.0, 256.0),
        156,
        [2, 1, 3, 0, 0],
        1,
        Some(a),
    );
    m.line(
        (1024.0, 0.0),
        (1024.0, 256.0),
        156,
        [1, 2, 3, 0, 0],
        2,
        Some(b),
    );
    let map = load(&m);
    for p in &map.setup.portals.lines {
        assert_eq!(p.kind, PORTT_VISUAL);
        assert!(p.is_visual() && !p.is_traversable());
        assert!(p.skip_wall());
    }
}

#[test]
fn quick_portals_pair_lines_with_the_same_id() {
    let mut m = room_testkit::linked_line_portal_rooms(3);
    m.line((256.0, 0.0), (256.0, 256.0), 301, [0; 5], 7, Some(0));
    m.line((1024.0, 0.0), (1024.0, 256.0), 301, [0; 5], 7, Some(2));
    let map = load(&m);
    let t = &map.setup.portals;
    assert_eq!(t.lines.len(), 2);
    assert!(
        t.lines
            .iter()
            .all(|p| p.kind == PORTT_LINKED && p.is_linked())
    );
    assert!(map.setup.portal_groups.has_links());
}

#[test]
fn eternity_two_line_portals_link_both_ways() {
    let mut m = room_testkit::linked_line_portal_rooms(3);
    m.line(
        (256.0, 0.0),
        (256.0, 256.0),
        156,
        [0, 0, 4, 0, 0],
        9,
        Some(0),
    );
    m.line(
        (1024.0, 0.0),
        (1024.0, 256.0),
        156,
        [1, 0, 4, 0, 0],
        9,
        Some(2),
    );
    let map = load(&m);
    let t = &map.setup.portals;
    assert_eq!(t.lines.len(), 2);
    assert!(t.lines.iter().all(|p| p.is_linked()));
}

#[test]
fn linked_plane_portals_join_rooms_above_and_below() {
    let map = load(&room_testkit::linked_plane_portal_rooms());
    let t = &map.setup.portals;
    assert!(t.plane_is_linked(0, PLANE_CEILING));
    assert!(t.plane_is_linked(1, PLANE_FLOOR));
    assert!(!t.plane_is_linked(0, PLANE_FLOOR));
    assert_eq!(t.plane_z(0, PLANE_CEILING), 128.0);
    assert_eq!(t.plane_z(1, PLANE_FLOOR), 128.0);
    assert_eq!(t.plane_displacement(0, PLANE_CEILING), (1024.0, 0.0));
    assert_eq!(t.plane_displacement(1, PLANE_FLOOR), (-1024.0, 0.0));
    assert!(!t.plane_blocks_movement(0, PLANE_CEILING));
    assert!(t.plane_blocks_movement(0, PLANE_FLOOR));
    assert!(t.has_linked_planes);
    let g = &map.setup.portal_groups;
    assert_eq!(
        g.offset(g.group_of(0).unwrap(), g.group_of(1).unwrap()),
        (1024.0, 0.0)
    );
}

#[test]
fn udmf_portal_flags_switch_planes_off() {
    let mut m = room_testkit::linked_plane_portal_rooms();
    m.sector_fields(0, "portal_ceil_nopass = true; portal_ceil_norender = true;");
    let map = load(&m);
    let t = &map.setup.portals;
    assert!(t.plane_blocks_movement(0, PLANE_CEILING));
    assert!(t.plane_blocks_view(0, PLANE_CEILING));
    assert!(t.plane_blocks_sight(0, PLANE_CEILING));
    assert!(!t.plane_blocks_sound(0, PLANE_CEILING));
}

#[test]
fn stacked_sectors_from_sector_set_portal_type_0() {
    let mut m = room_testkit::linked_plane_portal_rooms();
    // Same layout, but visual stacks (type 0) instead of linked ones.
    m.line((0.0, 0.0), (256.0, 0.0), 57, [1, 0, 1, 0, 128], 0, Some(0));
    m.line(
        (1024.0, 0.0),
        (1280.0, 0.0),
        57,
        [1, 0, 1, 1, 0],
        0,
        Some(1),
    );
    m.line(
        (0.0, 256.0),
        (256.0, 256.0),
        57,
        [2, 0, 0, 1, 0],
        0,
        Some(0),
    );
    m.line(
        (1024.0, 256.0),
        (1280.0, 256.0),
        57,
        [2, 0, 0, 0, 0],
        0,
        Some(1),
    );
    let map = load(&m);
    let t = &map.setup.portals;
    let p = t.plane_portal(0, PLANE_CEILING).unwrap();
    assert_eq!(p.kind, PORTS_PORTAL);
    assert_eq!(p.displacement, (1024.0, 0.0));
    assert!((t.plane_alpha[0][PLANE_CEILING] - 128.0 / 255.0).abs() < 1e-6);
    assert!(!t.plane_is_linked(0, PLANE_CEILING));
    assert!(!map.setup.portal_groups.has_links());
    assert_eq!(
        t.visible_plane_portal(0, PLANE_CEILING, false),
        Some(t.plane_portal_index(0, PLANE_CEILING))
    );
}

#[test]
fn stack_things_make_stacked_sectors() {
    let mut m = PolyMap::new();
    let low = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    let high = m.rect(1024.0, 0.0, 1280.0, 256.0, 128.0, 256.0, 0);
    // LowerStackLookOnly in the low room (its ceiling shows the high room),
    // UpperStackLookOnly in the high room (its floor shows the low room).
    m.thing(THING_LOWER_STACK.into(), 128.0, 128.0, 0.0, 0, 5, [0; 5]);
    m.thing(THING_UPPER_STACK.into(), 1152.0, 128.0, 0.0, 0, 5, [0; 5]);
    let map = load(&m);
    let t = &map.setup.portals;
    let c = t.plane_portal(low, PLANE_CEILING).unwrap();
    assert_eq!(c.kind, PORTS_STACKEDSECTORTHING);
    assert_eq!(c.displacement, (1024.0, 0.0));
    let f = t.plane_portal(high, PLANE_FLOOR).unwrap();
    assert_eq!(f.kind, PORTS_STACKEDSECTORTHING);
    assert_eq!(f.displacement, (-1024.0, 0.0));
    assert_eq!(c.partner, Some(t.plane_portal_index(high, PLANE_FLOOR)));
}

#[test]
fn sky_viewpoints_and_pickers() {
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    let b = m.rect(256.0, 0.0, 512.0, 256.0, 0.0, 128.0, 0);
    m.textures(a, "FLOOR0_1", "F_SKY1");
    m.textures(b, "FLOOR0_1", "F_SKY1");
    let _box = m.rect(2048.0, 2048.0, 2304.0, 2304.0, 0.0, 256.0, 0);
    // Default sky box (no tid) and a second one picked by sector b.
    m.thing(
        THING_SKY_VIEWPOINT.into(),
        2176.0,
        2176.0,
        64.0,
        90,
        0,
        [0; 5],
    );
    m.thing(
        THING_SKY_VIEWPOINT.into(),
        2100.0,
        2100.0,
        0.0,
        0,
        3,
        [0; 5],
    );
    m.thing(
        THING_SKY_PICKER.into(),
        384.0,
        128.0,
        0.0,
        0,
        0,
        [3, 1, 0, 0, 0],
    );
    let map = load(&m);
    let t = &map.setup.portals;
    let default = t.sectors[0];
    assert!(default.skybox.is_some());
    assert_eq!(t.plane_portal_index(a, PLANE_CEILING), 0);
    assert_eq!(t.visible_plane_portal(a, PLANE_CEILING, true), Some(0));
    // Not on non-sky planes (`PORTSF_SKYFLATONLY`).
    assert_eq!(t.visible_plane_portal(a, PLANE_FLOOR, false), None);
    // SkyPicker arg1 = 1: ceiling only.
    let pick = t.plane_portal_index(b, PLANE_CEILING);
    assert!(pick >= 2);
    assert_eq!(t.plane_portal_index(b, PLANE_FLOOR), 0);
    let th = t.things[&t.sectors[pick as usize].skybox.unwrap()];
    assert_eq!((th.x, th.y), (2100.0, 2100.0));
    let dth = t.things[&default.skybox.unwrap()];
    assert_eq!((dth.z_offset, dth.angle), (64.0, 90.0));
}

#[test]
fn ee_skybox_from_sky_cam_compat() {
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 4);
    let cam = m.rect(2048.0, 2048.0, 2304.0, 2304.0, 0.0, 256.0, 0);
    // The setup line sits in the camera's sector: Sector_SetPortal(tag, 2, plane 2).
    m.line(
        (2048.0, 2048.0),
        (2304.0, 2048.0),
        57,
        [4, 2, 2, 0, 0],
        0,
        Some(cam),
    );
    m.thing(
        THING_SKY_CAM_COMPAT.into(),
        2176.0,
        2176.0,
        0.0,
        0,
        0,
        [0; 5],
    );
    let map = load(&m);
    let t = &map.setup.portals;
    let c = t.plane_portal_index(a, PLANE_CEILING);
    let f = t.plane_portal_index(a, PLANE_FLOOR);
    assert!(c >= 2 && c == f);
    let p = t.sectors[c as usize];
    assert_eq!(p.kind, PORTS_SKYVIEWPOINT);
    assert_eq!(p.flags & PORTSF_SKYFLATONLY, 0);
    // Shown on any texture.
    assert_eq!(t.visible_plane_portal(a, PLANE_FLOOR, false), Some(c));
}

#[test]
fn horizon_and_mirror_lines() {
    let mut m = PolyMap::new();
    let a = m.rect(0.0, 0.0, 256.0, 256.0, 0.0, 128.0, 0);
    m.line((0.0, 0.0), (256.0, 0.0), 9, [0; 5], 0, Some(a));
    m.line((0.0, 256.0), (256.0, 256.0), 182, [0; 5], 0, Some(a));
    let map = load(&m);
    let t = &map.setup.portals;
    assert_eq!(t.lines.len(), 2);
    assert!(t.lines.iter().any(|p| p.is_horizon() && p.skip_wall()));
    assert!(t.lines.iter().any(|p| p.is_mirror() && !p.skip_wall()));
    assert!(!t.has_passable_lines);
}

#[test]
fn line_set_portal_target_repoints_a_portal() {
    let mut m = room_testkit::rotated_line_portal_rooms(1);
    // A third candidate line with id 3.
    m.line((0.0, 0.0), (256.0, 0.0), 0, [0; 5], 3, None);
    let mut map = load(&m);
    let a = line_at(&map, 256.0).raw() as usize;
    assert!(change_portal(&mut map, Some(a), 0, 3));
    let p = map.setup.portals.line(LineId::from_raw(a as u32)).unwrap();
    assert!(p.target.is_some());
    assert_eq!(map.setup.portals.revision, 1);
    assert!(change_portal(&mut map, Some(a), 0, 0));
    let p = map.setup.portals.line(LineId::from_raw(a as u32)).unwrap();
    assert!(p.target.is_none() && p.flags == 0);
}

#[test]
fn point_in_sector_without_nodes() {
    let map = load(&room_testkit::linked_line_portal_rooms(3));
    assert_eq!(sector_at(&map, 128.0, 128.0).map(|s| s.raw()), Some(0));
    assert_eq!(sector_at(&map, 264.0, 10.0).map(|s| s.raw()), Some(1));
    assert_eq!(sector_at(&map, 2000.0, 10.0), None);
}

#[test]
fn a_moved_polyobject_portal_updates_the_displacement_table() {
    let mut map = load(&room_testkit::linked_line_portal_rooms(3));
    let la = line_at(&map, 256.0);
    let lb = line_at(&map, 1024.0);
    // Slide line A 16 units east, as a polyobject carrying it would.
    let (v1, v2) = {
        let l = &map.lines[la.raw() as usize];
        (l.v1, l.v2)
    };
    for v in [v1, v2] {
        let p = &mut map.vertices[v.raw() as usize].pos;
        *p = room_core::DVec2::new(p.x() + 16.0, p.y());
    }
    update_poly_portals(&mut map, &[la]);
    let t = &map.setup.portals;
    assert_eq!(t.line(la).unwrap().displacement, (752.0, 0.0));
    assert_eq!(t.line(lb).unwrap().displacement, (-752.0, 0.0));
    let g = &map.setup.portal_groups;
    let (ga, gb) = (g.group_of(0).unwrap(), g.group_of(2).unwrap());
    assert_eq!(g.offset(ga, gb), (752.0, 0.0));
    assert_eq!(g.offset(gb, ga), (-752.0, 0.0));
    assert_eq!(poly_portal_kind(&map, &[la]), 2);
    assert_eq!(poly_portal_kind(&map, &[line_at(&map, 0.0)]), 0);
}

#[test]
fn a_turned_polyobject_portal_turns_its_view() {
    let mut map = load(&room_testkit::rotated_line_portal_rooms(2));
    let la = line_at(&map, 256.0);
    let before = map.setup.portals.line(la).unwrap().angle_diff;
    // Turn line A a quarter turn about its middle (it now runs east–west).
    let (v1, v2) = {
        let l = &map.lines[la.raw() as usize];
        (l.v1, l.v2)
    };
    map.vertices[v1.raw() as usize].pos = room_core::DVec2::new(128.0, 128.0);
    map.vertices[v2.raw() as usize].pos = room_core::DVec2::new(384.0, 128.0);
    let d = room_core::DVec2::new(256.0, 0.0);
    map.lines[la.raw() as usize].delta = d;
    update_poly_portals(&mut map, &[la]);
    let after = map.setup.portals.line(la).unwrap().angle_diff;
    let turn = (after - before).rem_euclid(360.0);
    assert!(
        (turn - 90.0).abs() < 1e-9 || (turn - 270.0).abs() < 1e-9,
        "{before} → {after}"
    );
}

//! Outdoor flats must stay near floor Z — not climb into the sky band.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_render::{Camera, build_scene_with_sky_ex, is_sky_flat};
use room_vfs::VfsBuilder;
use glam::Vec3;
use std::path::PathBuf;

#[test]
fn outdoor_flat_verts_stay_near_floor() {
    let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/freedoom/freedoom1.wad");
    if !wad.is_file() {
        return;
    }
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);

    let mut candidates = Vec::new();
    for (si, sec) in map.sectors.iter().enumerate() {
        if !is_sky_flat(&sec.ceiling_pic) {
            continue;
        }
        for line in &map.lines {
            if line.front_sector.is_some_and(|s| s.raw() as usize == si)
                || line.back_sector.is_some_and(|s| s.raw() as usize == si)
            {
                let a = &map.vertices[line.v1.raw() as usize];
                let b = &map.vertices[line.v2.raw() as usize];
                candidates.push((
                    0.5 * (a.pos.x() + b.pos.x()),
                    0.5 * (a.pos.y() + b.pos.y()),
                    sec.floor_height + 41.0,
                    sec.floor_height,
                    sec.ceiling_height,
                ));
            }
        }
    }
    let (cx, cy, eye_z, floor_z, ceil_z) = candidates
        .into_iter()
        .find(|&(x, y, _, _, _)| {
            room_nodebuild::point_in_sector(&map, x, y)
                .or_else(|| room_nodebuild::point_in_sector_vanilla(&map, x, y))
                .is_some_and(|sid| is_sky_flat(&map.sectors[sid.raw() as usize].ceiling_pic))
        })
        .expect("outdoor point");
    let camera = Camera {
        eye: Vec3::new(cx as f32, cy as f32, eye_z as f32),
        forward: Vec3::new(0.0, 1.0, 0.15).normalize(),
        ..Camera::default()
    };
    let scene = build_scene_with_sky_ex(&map, &camera, None, Some("SKY1"), None);

    // Among flat verts near the camera XY, Z should be near floor or (non-sky) ceiling —
    // never a steep wall of floor tris filling the sky.
    let mut near_high = 0u32;
    let mut near_total = 0u32;
    let mut max_z = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    for v in &scene.flats {
        let dx = v.pos[0] - camera.eye.x;
        let dy = v.pos[1] - camera.eye.y;
        if dx * dx + dy * dy > 512.0 * 512.0 {
            continue;
        }
        near_total += 1;
        max_z = max_z.max(v.pos[2]);
        min_z = min_z.min(v.pos[2]);
        // Well above the eye toward the sky ceiling band.
        if v.pos[2] > eye_z as f32 + 80.0 {
            near_high += 1;
        }
    }
    eprintln!(
        "near flats={near_total} high_above_eye={near_high} z=[{min_z:.1},{max_z:.1}] floor={floor_z} ceil={ceil_z} eye={eye_z:.1}"
    );
    // Sky ceilings are skipped — nearby flats should not cluster up near ceil_z.
    assert!(
        near_high < near_total / 2,
        "too many nearby flat verts above the eye (high={near_high} total={near_total}); floors are filling the sky"
    );
}

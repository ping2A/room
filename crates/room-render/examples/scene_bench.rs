//! Time the per-frame CPU scene build the web client runs on MAP01.
use glam::Vec3;
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_render::{Camera, build_scene_with_sky_ex_shared, walk_bsp};
use room_vfs::VfsBuilder;
use std::time::Instant;

fn main() {
    let wad = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "testdata/doom/doom2.wad".into());
    let name = std::env::args().nth(2).unwrap_or_else(|| "MAP01".into());
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, &name).unwrap();
    ensure_nodes(&mut map);
    let start = map
        .things
        .iter()
        .find(|t| t.type_id == 1)
        .expect("player start");
    let a = f32::from(start.angle).to_radians();
    let floor = room_nodebuild::point_in_sector(&map, start.x.into(), start.y.into())
        .map_or(0.0, |s| map.sectors[s.raw() as usize].floor_height);
    #[allow(clippy::cast_possible_truncation)]
    let cam = Camera {
        eye: Vec3::new(f32::from(start.x), f32::from(start.y), floor as f32 + 41.0),
        forward: Vec3::new(a.cos(), a.sin(), 0.0),
        ..Camera::default()
    };
    let n = 200;
    let t = Instant::now();
    for _ in 0..n {
        let vis = walk_bsp(&map, start.x.into(), start.y.into());
        std::hint::black_box(&vis);
    }
    let bsp = t.elapsed() / n;
    let t = Instant::now();
    for _ in 0..n {
        let lists =
            build_scene_with_sky_ex_shared(&map, &cam, None, Some("SKY1"), None, None, true);
        std::hint::black_box(&lists);
    }
    let full = t.elapsed() / n;
    // `SCENE_BENCH_CULLED_ONLY=1` skips the unculled timing (for profiling).
    let unculled_runs = if std::env::var_os("SCENE_BENCH_CULLED_ONLY").is_some() {
        0
    } else {
        n
    };
    let t = Instant::now();
    for _ in 0..unculled_runs {
        let lists =
            build_scene_with_sky_ex_shared(&map, &cam, None, Some("SKY1"), None, None, false);
        std::hint::black_box(&lists);
    }
    let unculled = t.elapsed() / unculled_runs.max(1);
    let l = build_scene_with_sky_ex_shared(&map, &cam, None, Some("SKY1"), None, None, true);
    let wv = std::mem::size_of::<room_render::WallVertex>();
    println!(
        "verts: opaque {} sky {} transl {} flats {} tflats {} skyparts {} sprites {} | ranges walls {} flats {} | wallvertex {}B",
        l.opaque_walls.len(),
        l.sky_walls.len(),
        l.translucent_walls.len(),
        l.flats.len(),
        l.translucent_flats.len(),
        l.sky_parts.len(),
        l.sprites.len(),
        l.wall_ranges.len(),
        l.flat_ranges.len(),
        wv
    );
    // Cross-check R_PointInSubsector against the BSP walk from the start.
    let nodes = map.game_nodes.as_ref().unwrap();
    let pis = |x: f64, y: f64| -> usize {
        let mut index = (nodes.nodes.len() - 1) as u32;
        loop {
            if index & 0x8000_0000 != 0 {
                return (index & 0x7fff_ffff) as usize;
            }
            let n = &nodes.nodes[index as usize];
            let cross =
                f64::from(n.dx) * (y - f64::from(n.y)) - f64::from(n.dy) * (x - f64::from(n.x));
            index = n.children[usize::from(cross >= 0.0)];
        }
    };
    let vis = walk_bsp(&map, start.x.into(), start.y.into());
    let ss = pis(start.x.into(), start.y.into());
    let seg = &nodes.segs[nodes.subsectors[ss].first_seg as usize];
    println!(
        "start subsector {ss} visible={} (vis {} of {}) seg0 line {:?}",
        vis.subsectors.contains(&(ss as u32)),
        vis.subsectors.len(),
        nodes.subsectors.len(),
        seg.line
    );
    println!(
        "{name}: lines {} sectors {} | walk_bsp {:?} | build_scene culled {:?} unculled {:?} | culled vis {} of {}",
        map.lines.len(),
        map.sectors.len(),
        bsp,
        full,
        unculled,
        room_render::walk_bsp_occluded(&map, start.x.into(), start.y.into())
            .subsectors
            .len(),
        nodes.subsectors.len()
    );
}

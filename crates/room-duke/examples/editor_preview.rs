//! Render a raw Build map the way the web editor's native preview does.

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_build_engine::world::BuildWorld;

fn main() {
    let grp = std::fs::read("testdata/duke/duke3d.grp")
        .or_else(|_| std::fs::read("../../testdata/duke/duke3d.grp"))
        .expect("duke3d.grp");
    let renderer =
        room_duke::DukeSoftRenderer::from_grp_layers(&[grp.as_slice()]).expect("renderer");
    let parsed = room_build::Grp::parse(&grp).expect("grp");
    let map = parsed.get("E1L1.MAP").expect("E1L1");
    let world = BuildWorld::load_map_bytes(map).expect("world");
    let (w, h) = (640, 400);
    let t = std::time::Instant::now();
    let rgba = renderer.render_editor_view(
        &world,
        22000,
        55500,
        -8192 - (40 << 8),
        512,
        100,
        -1,
        w,
        h,
        0,
    );
    eprintln!("render {:?}", t.elapsed());
    let mut f = format!("P6\n{w} {h}\n255\n").into_bytes();
    for c in rgba.chunks(4) {
        f.extend_from_slice(&c[..3]);
    }
    std::fs::write("/tmp/editor_preview.ppm", f).unwrap();
    for (nx, ny) in [
        (0.0, 0.0),
        (0.0, -0.7),
        (0.0, -0.81),
        (-0.86, -0.15),
        (0.0, 0.9),
    ] {
        println!(
            "pick {nx},{ny} -> {}",
            pick(
                &renderer,
                &world,
                22000,
                55500,
                -8192 - (40 << 8),
                512,
                100,
                nx,
                ny
            )
        );
    }
}

fn pick(
    renderer: &room_duke::DukeSoftRenderer,
    world: &BuildWorld,
    x: i32,
    y: i32,
    z: i32,
    ang: i32,
    horiz: i32,
    nx: f64,
    ny: f64,
) -> String {
    use room_build_engine::geom::{SpriteHit, hitscan_full, updatesector};
    let sizes = |picnum: i16| renderer.tile_dimensions(picnum);
    let sectors = world.sector_walls();
    let walls = world.wall_xy();
    let sector_z = world.sector_z();
    let mut sect = -1;
    updatesector(x, y, &mut sect, &sectors, &walls);
    let a = f64::from(ang & 2047) * std::f64::consts::TAU / 2048.0;
    let (sin, cos) = a.sin_cos();
    let (dx, dy) = (cos - nx * sin, sin + nx * cos);
    let len = 16384.0;
    let slope = ny + f64::from(horiz - 100) / 128.0;
    let (vx, vy, vz) = (dx * len, dy * len, -slope * 16.0 * len * dx.hypot(dy));
    let live = world.lists.sprite_stat.len();
    let sprites: Vec<SpriteHit> = world
        .sprites
        .iter()
        .enumerate()
        .map(|(index, sprite)| SpriteHit {
            x: sprite.x,
            y: sprite.y,
            z: sprite.z,
            sectnum: if index < live
                && world.lists.sprite_stat[index] < room_build_engine::world::MAXSTATUS as i16
            {
                sprite.sectnum
            } else {
                -1
            },
            clipdist: 32,
            cstat: sprite.cstat | 256,
            face_span: i32::from(sizes(sprite.picnum).0) * i32::from(sprite.xrepeat),
            face_height: (i32::from(sizes(sprite.picnum).1) * i32::from(sprite.yrepeat)) << 2,
        })
        .collect();
    let hit = hitscan_full(
        x,
        y,
        z,
        sect,
        vx as i32,
        vy as i32,
        vz as i32,
        (256 << 16) | 64,
        &sectors,
        &walls,
        &sprites,
        Some(&sector_z),
    );
    format!(
        "sect {} wall {} sprite {} at ({}, {}, {})",
        hit.hitsect, hit.hitwall, hit.hitsprite, hit.hitx, hit.hity, hit.hitz
    )
}

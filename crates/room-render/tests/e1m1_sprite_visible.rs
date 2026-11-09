//! Solo E1M1: a ZombieMan billboard must upload POSS and appear in the framebuffer.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use glam::Vec3;
use room_map::load_map;
use room_nodebuild::ensure_nodes;
use room_render::{
    Camera, GpuContext, MaterialCatalog, OffscreenTarget, SpriteInstance, UnlitPass,
    append_sprites, build_scene, read_rgba, sprite_lump_candidates_rot, sprite_rotation_digit,
};
use room_sim::{
    ActivationType, ActorId, ClassRegistry, StateTable, TicCmd, World, activate_line, check_sight,
    floor_at_xy,
};
use room_vfs::VfsBuilder;
use std::path::PathBuf;
use std::sync::Arc;

fn doom1() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
    p.is_file().then_some(p)
}

fn set_player(world: &mut World, mo: ActorId, x: f64, y: f64, ang_deg: f64) {
    let z = floor_at_xy(world, x, y).unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = room_core::DVec3::new(x, y, z);
        a.angle = room_core::Angle::from_degrees(ang_deg);
        a.vel = room_core::DVec3::ZERO;
    }
}

#[test]
fn e1m1_zombie_sprite_visible_offscreen() {
    let Some(wad) = doom1() else {
        return;
    };
    let Ok(gpu) = pollster::block_on(GpuContext::headless()) else {
        eprintln!("skip: no wgpu adapter");
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = room_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let world = World::from_map_with_classes(map.clone(), Arc::clone(&classes), states).unwrap();

    let zid = world
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            world
                .classes
                .get(world.actors.get(id).unwrap().class)
                .is_some_and(|c| c.name == "ZombieMan")
        })
        .expect("zombie");
    let z = world.actors.get(zid).unwrap();
    let zx = z.pos.x() as f32;
    let zy = z.pos.y() as f32;
    let zz = z.pos.z() as f32;
    let sprite = z.sprite.clone();
    let frame = z.frame;

    // Camera 96 units south of zombie, eye-height, looking north.
    let camera = Camera {
        eye: Vec3::new(zx, zy - 96.0, zz + 41.0),
        forward: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };

    let rot = sprite_rotation_digit(
        z.pos.x(),
        z.pos.y(),
        z.angle.degrees(),
        f64::from(camera.eye.x),
        f64::from(camera.eye.y),
    );
    let candidates = sprite_lump_candidates_rot(&sprite, frame, rot);
    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);

    let mut uploaded = None;
    for (name, flip) in &candidates {
        if mats
            .get_or_upload(&gpu.device, &gpu.queue, &vfs, name)
            .is_some()
        {
            uploaded = Some((name.clone(), *flip));
            break;
        }
    }
    eprintln!(
        "zombie at ({zx:.0},{zy:.0}) sprite={sprite}{frame} candidates={candidates:?} uploaded={uploaded:?}"
    );
    let Some((tex, flip_u)) = uploaded else {
        panic!("MaterialCatalog failed to upload any POSS candidate for ZombieMan");
    };

    let mut scene = build_scene(&map, &camera, Some(&[]));
    let before = scene.sprites.len();
    append_sprites(
        &mut scene,
        &[SpriteInstance {
            pos: Vec3::new(zx, zy, zz + 1.0),
            width: 33.0,
            height: 56.0,
            light: 1.0,
            alpha: 1.0,
            fuzz: false,
            texture: tex.clone(),
            flip_u,
            anchor_u: 0.5,
            orient: room_render::SpriteOrient::Billboard,
            additive: false,
            roll: 0.0,
        }],
        &camera,
    );
    assert_eq!(scene.sprites.len(), before + 6);

    let target = OffscreenTarget::new(&gpu, 640, 400);
    let mut pass = UnlitPass::new(&gpu, target.format);
    pass.prepare(&gpu, &camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("e1m1-sprite"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("e1m1-sprite"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.stencil_aux_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Discard,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.draw_materials(&gpu, &mut rp, &scene, &mut mats, &vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let rgba = read_rgba(&gpu, &target).expect("readback");

    let mut bright = 0u32;
    let mut non_clear = 0u32;
    for px in rgba.chunks_exact(4) {
        let dr = (i16::from(px[0]) - 13).unsigned_abs();
        let dg = (i16::from(px[1]) - 13).unsigned_abs();
        let db = (i16::from(px[2]) - 20).unsigned_abs();
        if dr > 8 || dg > 8 || db > 8 {
            non_clear += 1;
        }
        if u16::from(px[0]) + u16::from(px[1]) + u16::from(px[2]) > 120 {
            bright += 1;
        }
    }
    eprintln!("framebuffer non_clear={non_clear} bright={bright} tex={tex}");
    assert!(
        bright > 200,
        "expected POSS sprite pixels in view (bright={bright} non_clear={non_clear} tex={tex})"
    );
}

#[test]
fn e1m1_zombie_visible_through_open_computer_door() {
    let Some(wad) = doom1() else {
        return;
    };
    let Ok(gpu) = pollster::block_on(GpuContext::headless()) else {
        eprintln!("skip: no wgpu adapter");
        return;
    };
    let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
    let mut map = load_map(&vfs, "E1M1").unwrap();
    ensure_nodes(&mut map);
    let src = include_str!("../../../testdata/actors/doom_basic.txt");
    let ir = room_decorate::parse_decorate(src).unwrap();
    let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
    let states = Arc::new(StateTable::from_decorate(&ir, &classes));
    let mut world =
        World::from_map_with_classes(map.clone(), Arc::clone(&classes), states).unwrap();
    let mo = world.players[0].as_ref().unwrap().mo.unwrap();

    // Raise tag-4 computer door.
    assert!(activate_line(
        &mut world,
        room_core::LineId::from_raw(476),
        Some(mo),
        ActivationType::Cross,
    ));
    for _ in 0..80 {
        world.set_ticcmds(&[Default::default(); 4]);
        world.tick();
    }
    // Mirror dynamic plane heights into the render map (as room-app does).
    for (i, s) in world.sectors.iter().enumerate() {
        if let Some(sec) = map.sectors.get_mut(i) {
            sec.floor_height = s.floor_z;
            sec.ceiling_height = s.ceiling_z;
            sec.light = s.light;
        }
    }
    eprintln!(
        "door sec2 floor={} ceil={}",
        map.sectors[2].floor_height, map.sectors[2].ceiling_height
    );

    // Run the same wake/chase as the invisible-killer probe, then capture the
    // frame at first damage — that's the "I got shot by something invisible" beat.
    set_player(&mut world, mo, 1888.0, -2848.0, 90.0);
    {
        let mut cmd = TicCmd::default();
        cmd.buttons = room_sim::Buttons(room_sim::Buttons::ATTACK);
        world.set_ticcmds(&[
            cmd,
            Default::default(),
            Default::default(),
            Default::default(),
        ]);
        world.tick();
    }
    let start_hp = world.actors.get(mo).unwrap().health;
    let mut hit = false;
    for _ in 0..35 * 30 {
        world.set_ticcmds(&[Default::default(); 4]);
        world.tick();
        // Keep render map planes in sync while the door settles / monsters move.
        for (i, s) in world.sectors.iter().enumerate() {
            if let Some(sec) = map.sectors.get_mut(i) {
                sec.floor_height = s.floor_z;
                sec.ceiling_height = s.ceiling_z;
                sec.light = s.light;
            }
        }
        if world.actors.get(mo).map(|a| a.health).unwrap_or(0) < start_hp {
            hit = true;
            break;
        }
    }
    assert!(hit, "expected damage at computer door");

    let zid = world
        .thinkers
        .default
        .iter()
        .copied()
        .find(|&id| {
            let a = world.actors.get(id).unwrap();
            a.target == Some(mo)
                && world
                    .classes
                    .get(a.class)
                    .is_some_and(|c| c.name == "ZombieMan")
        })
        .expect("attacking zombie");
    let z = world.actors.get(zid).unwrap();
    let (zx, zy, zz) = (z.pos.x() as f32, z.pos.y() as f32, z.pos.z() as f32);
    let sprite = z.sprite.clone();
    let frame = z.frame;
    let p = world.actors.get(mo).unwrap();
    let (px, py) = (p.pos.x() as f32, p.pos.y() as f32);

    // Camera matches the player eye at the moment of damage, looking forward (north).
    let eye = Vec3::new(px, py, zz.max(0.0) + 41.0);
    let forward = Vec3::new(0.0, 1.0, 0.0);
    let camera = Camera {
        eye,
        forward,
        ..Camera::default()
    };

    let rot = sprite_rotation_digit(
        z.pos.x(),
        z.pos.y(),
        z.angle.degrees(),
        f64::from(camera.eye.x),
        f64::from(camera.eye.y),
    );
    let candidates = sprite_lump_candidates_rot(&sprite, frame, rot);
    let mut mats = MaterialCatalog::new();
    mats.load_playpal(&vfs);
    mats.load_texturex(&vfs);
    mats.ensure_sampler(&gpu.device);

    let mut uploaded = None;
    for (name, flip) in &candidates {
        if mats
            .get_or_upload(&gpu.device, &gpu.queue, &vfs, name)
            .is_some()
        {
            uploaded = Some((name.clone(), *flip));
            break;
        }
    }
    let Some((tex, flip_u)) = uploaded else {
        panic!("no POSS upload: candidates={candidates:?}");
    };
    eprintln!(
        "cam={eye:?} fwd={forward:?} zombie=({zx:.0},{zy:.0}) tex={tex} sees={}",
        check_sight(&world, mo, zid)
    );

    // Dump door-line mid/upper textures around sector 2.
    for (li, line) in map.lines.iter().enumerate() {
        let touches = line.front_sector.is_some_and(|s| s.raw() == 2)
            || line.back_sector.is_some_and(|s| s.raw() == 2);
        if !touches {
            continue;
        }
        for si in 0..2 {
            if let Some(sid) = line.sides[si] {
                let side = &map.sides[sid.raw() as usize];
                if !side.middle.is_empty() || !side.upper.is_empty() {
                    eprintln!(
                        "  line {li} side {si} mid='{}' up='{}' low='{}' flags={:#x}",
                        side.middle, side.upper, side.lower, line.flags
                    );
                }
            }
        }
    }

    let sprites = [SpriteInstance {
        pos: Vec3::new(zx, zy, zz + 1.0),
        width: 33.0,
        height: 56.0,
        light: 1.0,
        alpha: 1.0,
        fuzz: false,
        texture: tex.clone(),
        flip_u,
        anchor_u: 0.5,
        orient: room_render::SpriteOrient::Billboard,
        additive: false,
        roll: 0.0,
    }];
    let scene = room_render::build_scene_with_sky_ex(&map, &camera, Some(&sprites), None, None);
    eprintln!(
        "scene walls={} sprites={} sprite_ranges={}",
        scene.opaque_walls.len(),
        scene.sprites.len(),
        scene.sprite_ranges.len()
    );

    let target = OffscreenTarget::new(&gpu, 640, 400);
    let mut pass = UnlitPass::new(&gpu, target.format);
    pass.light_mode = room_render::LightMode::DoomSoftware;
    pass.prepare(&gpu, &camera, &scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("door-sprite"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("door-sprite"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.stencil_aux_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Discard,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.draw_materials(&gpu, &mut rp, &scene, &mut mats, &vfs);
    }
    gpu.queue.submit(Some(encoder.finish()));
    let rgba = read_rgba(&gpu, &target).expect("readback");

    // Count pixels that look like Doom palette skin/armor greens/browns (POSS),
    // not just "brighter than clear".
    let mut bright = 0u32;
    let mut non_clear = 0u32;
    let mut greenish = 0u32;
    for px in rgba.chunks_exact(4) {
        let dr = (i16::from(px[0]) - 13).unsigned_abs();
        let dg = (i16::from(px[1]) - 13).unsigned_abs();
        let db = (i16::from(px[2]) - 20).unsigned_abs();
        if dr > 8 || dg > 8 || db > 8 {
            non_clear += 1;
        }
        if u16::from(px[0]) + u16::from(px[1]) + u16::from(px[2]) > 120 {
            bright += 1;
        }
        // POSS uses muted greens/browns; require some chroma so walls alone don't pass.
        let maxc = px[0].max(px[1]).max(px[2]);
        let minc = px[0].min(px[1]).min(px[2]);
        if maxc > 40 && u16::from(maxc) - u16::from(minc) > 15 {
            greenish += 1;
        }
    }
    eprintln!("fb non_clear={non_clear} bright={bright} chroma={greenish} tex={tex}");
    assert!(
        bright > 200 && greenish > 50,
        "zombie behind open computer door not visible (bright={bright} chroma={greenish} tex={tex})"
    );
}

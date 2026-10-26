//! Soft FP smoke against real Duke maps when `testdata/duke/duke3d.grp` exists
//! (not committed; test no-ops without it).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_build::grp::Grp;
use room_build_engine::{BuildWorld, FreeFly, FreeFlyCmd, SoftFrame, render_fp_solid};
use std::path::PathBuf;

fn grp_bytes() -> Option<Vec<u8>> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke/duke3d.grp");
    std::fs::read(p).ok()
}

fn map_names_e1_through_e4() -> Vec<String> {
    let mut v = Vec::new();
    for ep in 1..=4 {
        for lv in 1..=11 {
            v.push(format!("E{ep}L{lv}.MAP"));
        }
    }
    v
}

#[test]
fn soft_fp_renders_e1l1_start_view() {
    let Some(bytes) = grp_bytes() else {
        return;
    };
    let grp = Grp::parse(&bytes).unwrap();
    let map = grp.get("E1L1.MAP").expect("E1L1.MAP");
    let world = BuildWorld::load_map_bytes(map).unwrap();
    assert!(world.numsectors > 10, "sectors={}", world.numsectors);

    let view = world.build_view(
        world.start[0],
        world.start[1],
        world.start[2],
        world.start_ang,
        100,
        world.start_sector,
    );
    assert!(
        view.visible_sectors.len() >= 2,
        "visible={}",
        view.visible_sectors.len()
    );

    let mut frame = SoftFrame::new(160, 100);
    render_fp_solid(&world, &view, &mut frame);
    let lit = frame.pixels.iter().filter(|&&p| p != 0).count();
    assert!(lit > 1000, "lit pixels={lit}");

    // Free-fly step still produces a view.
    let mut fly = FreeFly::from_view(&view);
    fly.apply(
        &world,
        FreeFlyCmd {
            forward: 128,
            turn: 32,
            ..FreeFlyCmd::default()
        },
    );
    let v2 = fly.view(&world);
    let mut f2 = SoftFrame::new(80, 50);
    render_fp_solid(&world, &v2, &mut f2);
    assert!(f2.pixels.iter().any(|&p| p != 0));
}

#[test]
fn soft_fp_loads_all_e1_e4_maps() {
    let Some(bytes) = grp_bytes() else {
        return;
    };
    let grp = Grp::parse(&bytes).unwrap();
    let mut ok = 0_u32;
    for name in map_names_e1_through_e4() {
        let Some(map) = grp.get(&name) else {
            continue;
        };
        let world = BuildWorld::load_map_bytes(map).unwrap_or_else(|e| panic!("{name}: {e}"));
        let view = world.build_view(
            world.start[0],
            world.start[1],
            world.start[2],
            world.start_ang,
            100,
            world.start_sector,
        );
        let mut frame = SoftFrame::new(64, 48);
        render_fp_solid(&world, &view, &mut frame);
        assert!(
            frame.pixels.iter().any(|&p| p != 0),
            "{name} empty soft frame"
        );
        ok += 1;
    }
    // Shareware has E1 only; full has E1–E4. Require at least E1L1–E1L6.
    assert!(ok >= 6, "rendered maps={ok}");
}

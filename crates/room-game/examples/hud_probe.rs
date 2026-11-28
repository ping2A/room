//! Draw a mod's ZScript status bar headless and save it as a PPM:
//! `hud_probe <iwad> <mod> [map] [tics] [out.ppm] [fullscreen]`.
//!
//! Patches and fills are blitted on a 640×400 canvas (text is listed, not
//! drawn); every draw command is printed so a layout can be checked.

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_game::{GameOptions, Headless, Skill};
use room_ui::{DrawCmd, PatchCache, UiDrawList};
use room_vfs::VfsBuilder;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let map_name = a.get(3).cloned().unwrap_or_else(|| "MAP01".into());
    let tics: u32 = a.get(4).map_or(40, |s| s.parse().unwrap());
    let out = a.get(5).cloned().unwrap_or_else(|| "/tmp/hud.ppm".into());
    let fullscreen = a.get(6).is_some_and(|s| s == "fullscreen");
    let vfs = room_game::mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_path(&a[1])
        .unwrap()
        .mount_path(&a[2])
        .unwrap()
        .build()
        .unwrap();
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        ..GameOptions::default()
    });
    let defs = room_defs::DefinitionsBuilder::new(&vfs).build().unwrap();
    let handlers = defs.zscript_event_handlers();
    let mod_cvars: Vec<(String, String)> =
        room_defs::mod_options::mod_options(&defs.cvars, &defs.menudef, &defs.language)
            .iter()
            .flat_map(|g| &g.items)
            .map(|i| (i.cvar.clone(), i.default.clone()))
            .collect();
    let class = defs
        .mapinfo
        .statusbar_class
        .clone()
        .unwrap_or_else(|| "DoomStatusBar".into());
    game.set_keyconf(&defs.keyconf);
    game.set_mapinfo(Arc::new(defs.mapinfo));
    let zs = room_game::load_zscript_from_vfs(&vfs);
    game.set_zscript_uzdoom(zs.uzdoom_library);
    game.set_zscript_lumps(room_game::zscript_wad_lumps(&vfs, zs.program.as_ref()));
    game.set_zscript(zs.program, handlers);
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    game.set_defs(data.classes, data.states);
    game.drop_weapons_without_sprites(&vfs);
    let mut map = room_map::load_map(&vfs, &map_name).unwrap();
    room_nodebuild::ensure_nodes(&mut map);
    game.start_custom_map(map, Skill::new(3)).unwrap();
    for _ in 0..tics {
        game.tick(&[room_sim::TicCmd::default()]);
    }
    let w = game.world().unwrap();
    let items = w.status_bar_items(0).expect("status bar items");
    let health = w.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .and_then(|m| w.actors.get(m))
        .map_or(0, |m| m.health);
    let snap = room_ui::SbarSnapshot {
        health,
        keys: 0,
        slots: items.slots,
        face: "STFST01".into(),
        inventory: items.inventory,
        icons: items.icons,
        ready_weapon: items.ready_weapon,
        ammo1: items.ammo1,
        ammo2: items.ammo2,
        frags: 0,
        inv_sel: items.inv_sel,
        inv_bar: items.inv_bar,
        inventory_tics: items.inventory_tics,
        weapons: items
            .weapons
            .into_iter()
            .map(|(class, slot, ammo1, ammo2)| room_ui::SbarWeapon {
                class,
                slot,
                ammo1,
                ammo2,
            })
            .collect(),
        spawn_sprites: items.spawn_sprites,
        max_amounts: items.max_amounts,
        power_tics: items.power_tics,
        ancestors: items.ancestors,
        player_class: items.player_class,
        pose: items.pose,
    };
    println!(
        "snapshot weapon={:?} ammo1={:?} ammo2={:?}",
        snap.ready_weapon, snap.ammo1, snap.ammo2
    );
    let prog = room_game::load_status_bar_program(&vfs).expect("status bar program");
    let mut bar = room_ui::ScriptStatusBar::new(prog, &class).expect("status bar class");
    for (name, value) in &mod_cvars {
        bar.set_mod_cvar(name, value);
    }
    // `CVARS=name=value,…` overrides.
    for kv in std::env::var("CVARS").unwrap_or_default().split(',') {
        if let Some((k, v)) = kv.split_once('=') {
            bar.set_mod_cvar(k, v);
        }
    }
    // Window size (`SCREEN=1280x720`, the default): the HUD is laid out as
    // the app does and drawn through the same 320×200 box mapping.
    let (cw, ch) = std::env::var("SCREEN")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse::<usize>().ok()?, h.parse::<usize>().ok()?))
        })
        .unwrap_or((1280, 720));
    #[allow(clippy::cast_precision_loss)]
    bar.set_screen_size(cw as f32, ch as f32);
    let mut patches = PatchCache::from_vfs(&vfs);
    let mut list = UiDrawList::new();
    // Two passes: the first learns which pictures the bar wants.
    for pass in 0..60u32 {
        // `TICDIV=2`: two screen frames per game tic, as the app at 70 fps.
        let div: u32 = std::env::var("TICDIV")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
            .max(1);
        bar.set_game_tic(tics + pass / div);
        list = UiDrawList::new();
        let dims = |name: &str| {
            patches.get(name).map(|p| room_ui::PatchDims {
                w: f32::from(p.width),
                h: f32::from(p.height),
                left: f32::from(p.left_offset),
                top: f32::from(p.top_offset),
            })
        };
        bar.draw(&snap, fullscreen, 1.0, &dims, &mut list);
        let names: Vec<String> = list
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Patch { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        if std::env::var_os("PASSES").is_some() {
            let sig: Vec<String> = list
                .cmds
                .iter()
                .filter_map(|c| match c {
                    DrawCmd::Patch {
                        name, pos, alpha, ..
                    } => Some(format!("{name}@{:.0},{:.0}a{:.2}", pos.x, pos.y, alpha)),
                    _ => None,
                })
                .collect();
            println!("pass {pass}: {} cmds {}", list.cmds.len(), sig.join(" "));
        }
        for n in names.into_iter().chain(bar.take_unknown_patches()) {
            let _ = patches.ensure(&vfs, &n);
        }
    }
    if let Some(e) = bar.last_error.take() {
        println!("status bar {class} error: {e}");
    }
    println!(
        "class {class}: {} commands, virtual {}x{}",
        list.cmds.len(),
        list.virtual_w,
        list.virtual_h
    );
    #[allow(clippy::cast_precision_loss)]
    let (fw, fh) = (cw as f32, ch as f32);
    let fit = (fw / list.virtual_w.max(1.0)).min(fh / list.virtual_h.max(1.0));
    let (bx, by) = (
        (fw - list.virtual_w * fit) * 0.5,
        (fh - list.virtual_h * fit) * 0.5,
    );
    let mut img = vec![40u8; cw * ch * 3];
    let mut clip: Option<(f32, f32, f32, f32)> = None;
    let palette = *patches.palette();
    let blend =
        |img: &mut Vec<u8>, clip: Option<(f32, f32, f32, f32)>, x: usize, y: usize, c: [f32; 4]| {
            if x >= cw || y >= ch {
                return;
            }
            #[allow(clippy::cast_precision_loss)]
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            if let Some((x0, y0, x1, y1)) = clip
                && (fx < x0 || fy < y0 || fx >= x1 || fy >= y1)
            {
                return;
            }
            let i = (y * cw + x) * 3;
            for k in 0..3 {
                let v = f32::from(img[i + k]) * (1.0 - c[3]) + c[k] * 255.0 * c[3];
                img[i + k] = v.clamp(0.0, 255.0) as u8;
            }
        };
    for c in &list.cmds {
        match c {
            DrawCmd::Patch {
                name,
                pos,
                size,
                apply_offsets,
                alpha,
                ..
            } => {
                println!(
                    "patch {name} at ({:.1},{:.1}) size ({:.1},{:.1}) offsets={apply_offsets} alpha={alpha}",
                    pos.x, pos.y, size.x, size.y
                );
                let Some(p) = patches.get(name) else {
                    println!("  (missing)");
                    continue;
                };
                let (mut x, mut y) = (pos.x, pos.y);
                if *apply_offsets {
                    x -= f32::from(p.left_offset);
                    y -= f32::from(p.top_offset);
                }
                let (w, h) = if size.x > 0.0 && size.y > 0.0 {
                    (size.x, size.y)
                } else {
                    (f32::from(p.width), f32::from(p.height))
                };
                let dw = (w * fit).round().max(1.0) as u32;
                let dh = (h * fit).round().max(1.0) as u32;
                let rgba = p.scale_rgba(dw, dh, &palette);
                let (ox, oy) = ((bx + x * fit).round(), (by + y * fit).round());
                for yy in 0..dh as usize {
                    for xx in 0..dw as usize {
                        let i = (yy * dw as usize + xx) * 4;
                        let Some(px) = rgba.get(i..i + 4) else {
                            continue;
                        };
                        if px[3] == 0 {
                            continue;
                        }
                        let (tx, ty) = (ox + xx as f32, oy + yy as f32);
                        if tx < 0.0 || ty < 0.0 {
                            continue;
                        }
                        let a = f32::from(px[3]) / 255.0 * alpha;
                        blend(
                            &mut img,
                            clip,
                            tx as usize,
                            ty as usize,
                            [
                                f32::from(px[0]) / 255.0,
                                f32::from(px[1]) / 255.0,
                                f32::from(px[2]) / 255.0,
                                a,
                            ],
                        );
                    }
                }
            }
            DrawCmd::Fill { rect, color } => {
                println!("fill {rect:?} {color:?}");
                let (x0, y0) = ((bx + rect.x * fit).max(0.0), (by + rect.y * fit).max(0.0));
                let (x1, y1) = (bx + (rect.x + rect.w) * fit, by + (rect.y + rect.h) * fit);
                for yy in y0 as usize..(y1.max(0.0) as usize).min(ch) {
                    for xx in x0 as usize..(x1.max(0.0) as usize).min(cw) {
                        blend(&mut img, clip, xx, yy, [color.r, color.g, color.b, color.a]);
                    }
                }
            }
            DrawCmd::Clip { rect } => {
                println!("clip {rect:?}");
                clip = rect.map(|r| {
                    (
                        bx + r.x * fit,
                        by + r.y * fit,
                        bx + (r.x + r.w) * fit,
                        by + (r.y + r.h) * fit,
                    )
                });
            }
            DrawCmd::Text {
                font,
                pos,
                text,
                scale,
                ..
            } => println!(
                "text {font} at ({:.1},{:.1}) x{scale:.2} {text:?}",
                pos.x, pos.y
            ),
            other => println!("{other:?}"),
        }
    }
    let mut ppm = format!("P6\n{cw} {ch}\n255\n").into_bytes();
    ppm.extend_from_slice(&img);
    std::fs::write(&out, ppm).unwrap();
    println!("wrote {out}");
}

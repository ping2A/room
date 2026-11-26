//! Headless replay of the desktop `--native-duke --con <mod>/eduke.con` start:
//! the same mod-asset scan, renderer, EDuke32 session, then `TICS` tics of
//! walking/turning/firing with CON runtime errors printed and PPM frames saved.
//!
//! `duke_mod_run <mod dir> <grp> [MAP] [TICS] [OUT]`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_duke::DukeInput;
use std::path::{Path, PathBuf};

fn walk(root: &Path, dir: &Path, out: &mut [Vec<(String, Vec<u8>)>; 4]) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let slot = match ext.as_deref() {
            Some("def") => 0,
            Some("png" | "bmp" | "jpg" | "jpeg" | "kvx" | "pal" | "raw") => 1,
            Some("mhk") => 2,
            Some("ogg" | "wav" | "flac" | "mp3" | "mid" | "midi" | "mus") => 3,
            _ => continue,
        };
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        out[slot].push((rel, std::fs::read(&path).unwrap()));
    }
}

/// `room-app`'s `DukeConIncludes`, plus a case-insensitive fallback so this
/// case-sensitive filesystem behaves like macOS.
struct Includes {
    directory: PathBuf,
}

impl Includes {
    fn find_ci(dir: &Path, rel: &str) -> Option<PathBuf> {
        let mut cur = dir.to_path_buf();
        for part in rel.split('/').filter(|p| !p.is_empty() && *p != ".") {
            let next = std::fs::read_dir(&cur)
                .ok()?
                .flatten()
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
            cur = next.path();
        }
        Some(cur)
    }
}

impl room_duke::con::IncludeResolver for Includes {
    fn read_include(&mut self, name: &str) -> Option<Vec<u8>> {
        let normalized = name.replace('\\', "/");
        let requested = Path::new(&normalized);
        let candidates = if requested.is_absolute() {
            vec![requested.to_path_buf()]
        } else {
            vec![
                self.directory.join(requested),
                self.directory.join(normalized.to_ascii_uppercase()),
                self.directory.join(normalized.to_ascii_lowercase()),
            ]
        };
        candidates
            .into_iter()
            .find_map(|p| std::fs::read(p).ok())
            .or_else(|| std::fs::read(Self::find_ci(&self.directory, &normalized)?).ok())
    }
}

fn save(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let mut f = format!("P6\n{w} {h}\n255\n").into_bytes();
    for c in rgba.chunks(4) {
        f.extend_from_slice(&c[..3]);
    }
    std::fs::write(path, f).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mod_dir = PathBuf::from(&args[1]);
    let grp = std::fs::read(&args[2]).expect("GRP");
    let map = args.get(3).cloned().unwrap_or_else(|| "E1L1".into());
    let tics: u32 = args.get(4).and_then(|t| t.parse().ok()).unwrap_or(350);
    let out = PathBuf::from(
        args.get(5)
            .cloned()
            .unwrap_or_else(|| "/tmp/duke_mod_run".into()),
    );
    std::fs::create_dir_all(&out).unwrap();

    let t0 = std::time::Instant::now();
    let mut assets: [Vec<(String, Vec<u8>)>; 4] = Default::default();
    walk(&mod_dir, &mod_dir, &mut assets);
    println!(
        "assets: {} def, {} tile/voxel/palette, {} mhk, {} music ({:.1}s)",
        assets[0].len(),
        assets[1].len(),
        assets[2].len(),
        assets[3].len(),
        t0.elapsed().as_secs_f32()
    );
    let refs = |v: &[(String, Vec<u8>)]| -> Vec<(String, Vec<u8>)> { v.to_vec() };
    let defs = refs(&assets[0]);
    let tiles = refs(&assets[1]);
    let mhks = refs(&assets[2]);
    let d: Vec<(&str, &[u8])> = defs
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let t: Vec<(&str, &[u8])> = tiles
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let m: Vec<(&str, &[u8])> = mhks
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    let layers = [grp.clone()];
    let layer_slices: Vec<&[u8]> = layers.iter().map(Vec::as_slice).collect();
    let t1 = std::time::Instant::now();
    let renderer = match room_duke::DukeSoftRenderer::from_grp_layers_with_mod_assets_and_mhks(
        &layer_slices,
        None,
        &d,
        &t,
        &m,
    ) {
        Ok(r) => r,
        Err(e) => {
            println!("RENDERER ERROR: {e}");
            std::process::exit(1);
        }
    };
    println!("renderer ok ({:.1}s)", t1.elapsed().as_secs_f32());

    let con_path = mod_dir.join("eduke.con");
    let con = std::fs::read(&con_path).expect("eduke.con");
    let mut includes = Includes {
        directory: mod_dir.clone(),
    };
    let layer_refs: Vec<&[u8]> = layers.iter().map(Vec::as_slice).collect();
    let t2 = std::time::Instant::now();
    let mut s = match room_duke::session_from_grp_layers_with_con_and_includes_dialect(
        &layer_refs,
        &map,
        2,
        Some(&con),
        Some(&mut includes),
        room_duke::ConDialect::EDuke32,
    ) {
        Ok(s) => s,
        Err(e) => {
            println!("SESSION ERROR: {e}");
            std::process::exit(1);
        }
    };
    println!("session ok ({:.1}s)", t2.elapsed().as_secs_f32());
    if std::env::var_os("DMR_PROBE_WEAPON").is_some() {
        for _ in 0..40 {
            s.tick(DukeInput::default());
        }
        let actor = usize::try_from(s.player.sprite_i).unwrap_or(0);
        let result = s.execute_eduke_event("EVENT_DISPLAYWEAPON", 0, actor, 200_000, 64);
        println!(
            "probe EVENT_DISPLAYWEAPON: {:?}",
            result.as_ref().map(|e| e.len())
        );
        if let Some(vm) = s.eduke_vm_state.as_ref() {
            for name in [
                "g_weapon_mode",
                "ws_drawweapon",
                "ws_curr_weapon",
                "ws_handled",
                "ws_slot",
                "RETURN",
                "currentweapon",
                "looking_arc",
                "gun_pos",
                "weapon_xoffset",
                "weaponcount",
            ] {
                println!("  {name} = {:?}", vm.get_variable(name, 0, actor));
            }
        }
        for error in s.take_eduke_runtime_errors() {
            println!("  runtime: {error}");
        }
        return;
    }

    if std::env::var_os("DMR_NVG").is_some() {
        s.player.heat_amount = 1200;
        s.player.heat_on = true;
    }
    let (w, h) = (640u32, 400u32);
    let mut errors = 0usize;
    let mut seen = std::collections::BTreeSet::new();
    for tic in 0..tics {
        let mut input = DukeInput::default();
        // Walk forward, sweep the view, fire in bursts.
        if tic > 35 {
            input.fvel = if (tic / 70) % 2 == 0 { 40 } else { 0 };
            input.avel = if (tic / 70) % 3 == 1 { 8 } else { 0 };
        }
        if tic > 70 && tic % 20 < 6 {
            input.bits |= 1 << 2;
        }
        s.tick(input);
        let world = match s.render_world_with_eduke_events(0) {
            Ok(Some(world)) => Some(world),
            Ok(None) => None,
            Err(error) => {
                println!("tic {tic}: EVENT_ANIMATESPRITES failed: {error}");
                s.render_world()
            }
        };
        for error in s.take_eduke_runtime_errors() {
            errors += 1;
            let key: String = error.chars().filter(|c| !c.is_ascii_digit()).collect();
            if seen.insert(key) {
                println!("tic {tic}: [Duke CON] {error}");
            }
        }
        if tic % 70 == 0 || tic + 1 == tics {
            if let Some(world) = &world {
                println!(
                    "tic {tic}: hp {}, hud draws {}, native weapon hidden {}, event camera {:?}",
                    s.player.extra,
                    world.render_hud_draws.len(),
                    world.render_hide_native_weapon,
                    world
                        .render_camera
                        .map(|c| (c.posz, c.q16_horizoff, c.rotscrnang))
                );
            }
            if let Some(world) = world {
                renderer.set_overlay_quote(s.active_quote());
                let rgba = renderer.render_rgba_interpolated(
                    &world,
                    &s.player,
                    w,
                    h,
                    s.tic,
                    s.earthquake_time,
                    1000,
                );
                let path = out.join(format!("{map}_{tic:04}.ppm"));
                save(&path, w, h, &rgba);
            }
        }
    }
    println!(
        "done: {tics} tics, {errors} CON runtime errors ({} distinct), player hp {} at ({}, {}) sector {}",
        seen.len(),
        s.player.extra,
        s.player.posx,
        s.player.posy,
        s.player.cursectnum
    );
}

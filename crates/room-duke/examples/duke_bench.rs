//! ED9 per-tic budget: a scripted E1L1 full fight, timing only `DukeSession::tick`.
//!
//! ```text
//! ROOM_DUKE_MOD=testdata/brutal-duke-v0.2.0/brutal \
//!   cargo run --release -p room-duke --example duke_bench -- [MAP] [GRP]
//! ```
//!
//! Phases: a short settle, then the player is placed in sight of every
//! enemy in turn (god mode, pistol with ammo) and fires until it dies or a
//! per-target limit passes, then the level runs on with every corpse, gib,
//! casing and blood decal the fight left behind. Placement and aiming happen
//! between ticks and are not timed. Prints per-phase mean / p50 / p95 / p99 /
//! max milliseconds per tic, the live sprite peak, and the ED9 verdict
//! (mean ≤ 2 ms native; a wasm build of the same code is measured in the
//! browser). Without `ROOM_DUKE_MOD` the Atomic CON runs (baseline).

// A developer benchmark: missing inputs should stop it loudly.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_build_engine::world::MAXSTATUS;
use room_duke::{ConDialect, DukeInput, DukeSession, IncludeResolver};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const FIRE: u32 = 1 << 2;
/// ED9 acceptance: mean native milliseconds per tic over the fight.
const NATIVE_BUDGET_MS: f64 = 2.0;

/// Loose CON files of a mod directory, served to the compiler's includes.
struct ConDirectory {
    by_path: BTreeMap<String, Vec<u8>>,
    by_basename: BTreeMap<String, String>,
}

impl ConDirectory {
    fn load(root: &Path) -> std::io::Result<Self> {
        fn collect(dir: &Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {
            let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(fs::DirEntry::path);
            for entry in entries {
                let path = entry.path();
                let ty = entry.file_type()?;
                if ty.is_dir() {
                    collect(&path, paths)?;
                } else if ty.is_file()
                    && path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("con"))
                {
                    paths.push(path);
                }
            }
            Ok(())
        }
        let root = root.canonicalize()?;
        let mut paths = Vec::new();
        collect(&root, &mut paths)?;
        paths.sort();
        let mut by_path = BTreeMap::new();
        let mut by_basename = BTreeMap::new();
        for path in paths {
            let relative = path.strip_prefix(&root).unwrap_or(&path);
            let key = relative
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches("./")
                .to_ascii_lowercase();
            let basename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            by_basename.entry(basename).or_insert_with(|| key.clone());
            by_path.insert(key, fs::read(&path)?);
        }
        Ok(Self {
            by_path,
            by_basename,
        })
    }

    fn root_con(&self) -> Option<Vec<u8>> {
        self.by_path
            .get("eduke.con")
            .or_else(|| self.by_path.get("game.con"))
            .cloned()
    }
}

impl IncludeResolver for ConDirectory {
    fn read_include(&mut self, name: &str) -> Option<Vec<u8>> {
        let key = name
            .trim()
            .trim_matches('"')
            .replace('\\', "/")
            .trim_start_matches("./")
            .to_ascii_lowercase();
        self.by_path
            .get(&key)
            .or_else(|| {
                let base = key.rsplit('/').next().unwrap_or(&key);
                self.by_basename.get(base).and_then(|p| self.by_path.get(p))
            })
            .cloned()
    }
}

/// Milliseconds per tic for one phase.
#[derive(Default)]
struct Samples {
    ms: Vec<f64>,
}

impl Samples {
    fn push(&mut self, ms: f64) {
        self.ms.push(ms);
    }

    fn report(&self, name: &str) -> Option<f64> {
        if self.ms.is_empty() {
            println!("{name:<10} (no tics)");
            return None;
        }
        let mut v = self.ms.clone();
        v.sort_by(f64::total_cmp);
        let pick = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        println!(
            "{name:<10} tics {:>6}  mean {mean:>7.3}  p50 {:>7.3}  p95 {:>7.3}  p99 {:>7.3}  max {:>8.3} ms",
            v.len(),
            pick(0.5),
            pick(0.95),
            pick(0.99),
            v[v.len() - 1],
        );
        Some(mean)
    }
}

struct Bench {
    s: DukeSession,
    peak_live: usize,
    /// ED10 restore check: a second session loaded from the main session's
    /// mid-fight `.fsave`, driven identically and compared after every tic.
    shadow: Option<DukeSession>,
    /// Tics compared since the restore, and the first mismatching one.
    restore_compared: usize,
    restore_mismatch: Option<usize>,
}

impl Bench {
    fn live(&self) -> usize {
        self.s
            .lists
            .sprite_stat
            .iter()
            .filter(|&&st| st != MAXSTATUS as i16)
            .count()
    }

    fn alive(&self, i: usize) -> bool {
        self.s
            .lists
            .sprite_stat
            .get(i)
            .is_some_and(|&st| st != MAXSTATUS as i16)
            && self
                .s
                .world
                .as_ref()
                .is_some_and(|w| w.sprites[i].extra > 0)
    }

    /// One timed tic; god mode keeps the scripted player standing.
    fn tick(&mut self, input: DukeInput, into: &mut Samples) {
        self.s.player.extra = 100;
        let t0 = Instant::now();
        self.s.tick(input);
        into.push(t0.elapsed().as_secs_f64() * 1000.0);
        self.peak_live = self.peak_live.max(self.live());
        if let Ok(names) = std::env::var("ROOM_BENCH_TRACE_VARS") {
            // `name,name,...`: print gamevars (seat 0, player sprite) and the
            // pistol state every tic, for debugging script-driven weapons.
            let actor = usize::try_from(self.s.player.sprite_i).unwrap_or(0);
            let vm = self.s.eduke_vm_state.as_ref();
            let vars: Vec<String> = names
                .split(',')
                .map(|name| {
                    let value = vm.and_then(|vm| vm.get_variable(name, 0, actor).ok());
                    format!("{name}={value:?}")
                })
                .collect();
            println!(
                "trace weapon {} kickback {} ammo {} pos {} | {}",
                self.s.player.curr_weapon,
                self.s.player.kickback_pic,
                self.s.player.ammo[1],
                self.s.player.weapon_pos,
                vars.join(" ")
            );
        }
        if let Some(shadow) = self.shadow.as_mut() {
            shadow.player.extra = 100;
            shadow.tick(input);
            if self.restore_mismatch.is_none() {
                let main = self.s.save_fsave().expect("save main session");
                let copy = shadow.save_fsave().expect("save restored session");
                if main != copy {
                    self.restore_mismatch = Some(self.restore_compared);
                }
            }
            self.restore_compared += 1;
        }
    }

    /// Save the main session mid-fight and load it into `fresh`, which then
    /// follows every later input and scripted move.
    fn start_restore_check(&mut self, mut fresh: DukeSession) {
        let bytes = self.s.save_fsave().expect("mid-fight .fsave");
        // Loading runs EVENT_PRELOADGAME / EVENT_LOADGAME (Brutal Duke
        // reconciles state there), so the running session reloads the same
        // save: both then went through one load and must stay identical.
        self.s
            .load_fsave(&bytes)
            .expect("reload the mid-fight .fsave");
        fresh.load_fsave(&bytes).expect("load the mid-fight .fsave");
        let reloaded = fresh.save_fsave().expect("save the restored session");
        if reloaded != self.s.save_fsave().expect("save the reloaded session") {
            self.restore_mismatch = Some(0);
        }
        println!(
            "restore check: saved {} bytes at the current fight tic",
            bytes.len()
        );
        self.shadow = Some(fresh);
    }

    /// Point the view at a world point (heading and pitch), like a mouse.
    fn aim(&mut self, x: i32, y: i32, z: i32) {
        aim_session(&mut self.s, x, y, z);
        if let Some(shadow) = self.shadow.as_mut() {
            aim_session(shadow, x, y, z);
        }
    }
}

/// Point a session's view at a world point (heading and pitch).
fn aim_session(s: &mut DukeSession, x: i32, y: i32, z: i32) {
    {
        let p = &mut s.player;
        let dx = f64::from(x - p.posx);
        let dy = f64::from(y - p.posy);
        let ang = dy.atan2(dx) * 1024.0 / std::f64::consts::PI;
        p.ang = (ang.round() as i32).rem_euclid(2048) as i16;
        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
        let firing_z = p.posz.saturating_add(p.pyoff).saturating_add(2 << 8);
        let slope = f64::from(z - firing_z) / dist;
        p.horiz = (100.0 - slope * 8.0).round().clamp(0.0, 299.0) as i16;
        p.horizoff = 0;
        p.q16_horiz = i32::from(p.horiz).wrapping_shl(16);
        p.q16_horizoff = 0;
        p.return_to_center = 0;
        let (ang, body) = (p.ang, p.sprite_i);
        if let Ok(body) = usize::try_from(body) {
            if let Some(sp) = s.world.as_mut().and_then(|w| w.sprites.get_mut(body)) {
                sp.ang = ang;
            }
        }
    }
}

impl Bench {
    /// Stand the player somewhere with a clear line to sprite `t`.
    fn place_near(&mut self, t: usize) -> bool {
        let placed = place_session(&mut self.s, t);
        if let Some(shadow) = self.shadow.as_mut() {
            assert_eq!(
                place_session(shadow, t),
                placed,
                "restored session placement differs"
            );
        }
        placed
    }
}

/// Stand a session's player somewhere with a clear line to sprite `t`.
fn place_session(s: &mut DukeSession, t: usize) -> bool {
    {
        let w = s.world.as_ref().unwrap();
        let sp = w.sprites[t];
        for radius in [700, 1400, 2100] {
            for k in 0..16 {
                let a = (i32::from(sp.ang) + k * 128) & 2047;
                let (sin, cos) = room_build_engine::helpers::sincos_q14(a);
                let x = sp.x + ((radius * cos) >> 14);
                let y = sp.y + ((radius * sin) >> 14);
                let mut sect = sp.sectnum;
                room_build_engine::geom::updatesector(
                    x,
                    y,
                    &mut sect,
                    &w.sector_walls(),
                    &w.wall_xy(),
                );
                let Ok(si) = usize::try_from(sect) else {
                    continue;
                };
                let sector = &w.sectors[si];
                let eye = sector.floorz - (40 << 8);
                if eye < sector.ceilingz {
                    continue;
                }
                let target_z = sp.z.saturating_sub(24 << 8);
                if !room_build_engine::geom::cansee(
                    x,
                    y,
                    eye,
                    sect,
                    sp.x,
                    sp.y,
                    target_z,
                    sp.sectnum,
                    &w.sector_z(),
                    &w.wall_xy(),
                ) {
                    continue;
                }
                let p = &mut s.player;
                p.posx = x;
                p.posy = y;
                p.posz = eye;
                p.oposx = x;
                p.oposy = y;
                p.oposz = eye;
                p.cursectnum = sect;
                // A weapon chosen with `ROOM_BENCH_WEAPON_KEY` fights as is.
                if std::env::var_os("ROOM_BENCH_WEAPON_KEY").is_none() {
                    p.curr_weapon = 1;
                    p.ammo[1] = 200;
                }
                return true;
            }
        }
        false
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map = args.get(1).map_or("E1L1", String::as_str);
    let grp_path = args.get(2).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke/duke3d.grp"),
        PathBuf::from,
    );
    let grp = fs::read(&grp_path).expect("read the Atomic GRP");
    let load = Instant::now();
    let make_session = || {
        if let Some(mod_root) = std::env::var_os("ROOM_DUKE_MOD") {
            let mut includes = ConDirectory::load(Path::new(&mod_root)).expect("mod CON tree");
            let root = includes.root_con().expect("mod has eduke.con or GAME.CON");
            // `ROOM_BENCH_MAP_FILE=path`: a loose map (e.g. the mod's box.map).
            let s = if let Some(map_file) = std::env::var_os("ROOM_BENCH_MAP_FILE") {
                let map_bytes = fs::read(&map_file).expect("read the map file");
                room_duke::session_from_grp_map_bytes_with_con_and_includes_dialect(
                    &[&grp],
                    &map_bytes,
                    2,
                    Some(&root),
                    Some(&mut includes),
                    ConDialect::EDuke32,
                )
            } else {
                room_duke::session_from_grp_layers_with_con_and_includes_dialect(
                    &[&grp],
                    &format!("{map}.MAP"),
                    2,
                    Some(&root),
                    Some(&mut includes),
                    ConDialect::EDuke32,
                )
            }
            .expect("load the mod session");
            (s, "EDuke32 mod")
        } else {
            (
                room_duke::session_from_grp_map(&grp, &format!("{map}.MAP"), 2).expect("session"),
                "Atomic",
            )
        }
    };
    let (s, label) = make_session();
    // `ROOM_BENCH_RESTORE_AT=N`: after N fight tics, save to `.fsave`, load
    // it into a fresh session and check both stay identical (ED10).
    let restore_at = std::env::var("ROOM_BENCH_RESTORE_AT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok());
    println!(
        "{label} {map}: loaded in {:.0} ms",
        load.elapsed().as_secs_f64() * 1000.0
    );
    let mut b = Bench {
        s,
        peak_live: 0,
        shadow: None,
        restore_compared: 0,
        restore_mismatch: None,
    };
    let (mut settle, mut fight, mut after) =
        (Samples::default(), Samples::default(), Samples::default());
    for _ in 0..70 {
        b.tick(DukeInput::default(), &mut settle);
    }
    // `ROOM_BENCH_CHEAT=stuff` applies a cheat before the fight;
    // `ROOM_BENCH_WEAPON_KEY=K` then presses weapon key K (`1`..`10`) until
    // the weapon changes, or until `ROOM_BENCH_WEAPON=N` is selected.
    if let Ok(code) = std::env::var("ROOM_BENCH_CHEAT") {
        let quote = b.s.apply_player_cheat(&code);
        println!("cheat {code}: quote {quote:?}");
        for _ in 0..40 {
            b.tick(DukeInput::default(), &mut settle);
        }
    }
    if let Some(key) = std::env::var("ROOM_BENCH_WEAPON_KEY")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
    {
        let wanted = std::env::var("ROOM_BENCH_WEAPON")
            .ok()
            .and_then(|v| v.parse::<i16>().ok());
        let before = b.s.player.curr_weapon;
        for press in 0..12 {
            let done = match wanted {
                Some(weapon) => b.s.player.curr_weapon == weapon,
                None => b.s.player.curr_weapon != before,
            };
            if done {
                break;
            }
            let bits = if press % 2 == 0 {
                key << room_duke::weapons::INPUT_WEAPON_SHIFT
            } else {
                0
            };
            b.tick(
                DukeInput {
                    bits,
                    ..DukeInput::default()
                },
                &mut settle,
            );
            for _ in 0..20 {
                b.tick(DukeInput::default(), &mut settle);
            }
        }
        println!(
            "weapon key {key}: weapon {} -> {}",
            before, b.s.player.curr_weapon
        );
    }
    let weapon_at_fight = b.s.player.curr_weapon;
    let ammo_at_fight = b.s.player.weapon_ammo(weapon_at_fight);
    let actortype = b.s.script.as_ref().unwrap().actortype.clone();
    let targets: Vec<usize> = (0..b.s.lists.sprite_stat.len())
        .filter(|&i| {
            let st = b.s.lists.sprite_stat[i];
            let sp = b.s.world.as_ref().unwrap().sprites[i];
            (st == 1 || st == 2)
                && sp.xrepeat > 0
                && sp.extra > 0
                && room_duke::badguy::badguy_pic(sp.picnum, &actortype)
        })
        .collect();
    // `ROOM_BENCH_TARGETS=N` caps the fight (profiling runs).
    let cap = std::env::var("ROOM_BENCH_TARGETS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    let after_tics = if cap == usize::MAX { 35 * 20 } else { 35 };
    // `ROOM_BENCH_TICS_PER_TARGET=N` (default 240) bounds each engagement.
    let tics_per_target = std::env::var("ROOM_BENCH_TICS_PER_TARGET")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(240);
    // `ROOM_BENCH_SCENARIO=pistol` (ED4 acceptance scenario): one pistol
    // round at the first LIZTROOP (else the first target), then 300 idle
    // tics; report the target's damage and the sprites the shot produced.
    if std::env::var("ROOM_BENCH_SCENARIO").as_deref() == Ok("pistol") {
        let image = b.s.script.clone().unwrap();
        let liztroop = image.labels.get("LIZTROOP").map(|label| label.value as i16);
        let w = b.s.world.as_ref().unwrap();
        let target = targets
            .iter()
            .copied()
            .find(|&t| Some(w.sprites[t].picnum) == liztroop)
            .or_else(|| targets.first().copied())
            .expect("a target");
        let picnum = w.sprites[target].picnum;
        let extra_before = w.sprites[target].extra;
        assert!(b.place_near(target), "no clear spot near the target");
        let w = b.s.world.as_ref().unwrap();
        let before: std::collections::BTreeSet<usize> = (0..w.sprites.len())
            .filter(|&i| b.s.lists.sprite_stat[i] != MAXSTATUS as i16)
            .collect();
        let ammo_before = b.s.player.ammo[1];
        let mut seen: std::collections::BTreeSet<(usize, i16)> = Default::default();
        for tic in 0..302 {
            let sp = b.s.world.as_ref().unwrap().sprites[target];
            b.aim(sp.x, sp.y, sp.z.saturating_sub(24 << 8));
            // `ROOM_BENCH_SCENARIO_ROUNDS=N` taps fire N times (default 1).
            let rounds = std::env::var("ROOM_BENCH_SCENARIO_ROUNDS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1);
            let bits = if tic < rounds * 8 && tic % 8 < 2 {
                FIRE
            } else {
                0
            };
            b.tick(
                DukeInput {
                    bits,
                    ..DukeInput::default()
                },
                &mut fight,
            );
            let w = b.s.world.as_ref().unwrap();
            for i in 0..w.sprites.len() {
                if b.s.lists.sprite_stat[i] != MAXSTATUS as i16 && !before.contains(&i) {
                    seen.insert((i, w.sprites[i].picnum));
                }
            }
        }
        let w = b.s.world.as_ref().unwrap();
        // Every sprite that appeared during the 300 tics, by tile.
        let mut spawned: BTreeMap<i16, usize> = BTreeMap::new();
        for &(_, tile) in &seen {
            *spawned.entry(tile).or_default() += 1;
        }
        let label = |tile: i16| {
            image
                .labels
                .iter()
                .find(|(_, label)| !label.is_ptr && label.value == i32::from(tile))
                .map_or_else(|| tile.to_string(), |(name, _)| name.clone())
        };
        println!(
            "scenario pistol: target {target} {} extra {extra_before} -> {} (status {}), rounds {}",
            label(picnum),
            w.sprites[target].extra,
            b.s.lists.sprite_stat[target],
            ammo_before - b.s.player.ammo[1]
        );
        let mut rows: Vec<_> = spawned.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        for (tile, count) in rows.iter().take(12) {
            println!("scenario spawned {} ({tile}): {count}", label(*tile));
        }
        // `ROOM_BENCH_SCENARIO_VARS=a,b`: gamevars to print afterwards (the
        // mod's damage hooks keep their last target in such variables).
        if let (Ok(names), Some(vm)) = (
            std::env::var("ROOM_BENCH_SCENARIO_VARS"),
            b.s.eduke_vm_state.as_ref(),
        ) {
            for name in names.split(',') {
                println!(
                    "scenario var {name} = {:?}",
                    vm.get_variable(name, 0, target).ok()
                );
            }
        }
        for error in b.s.take_eduke_runtime_errors().iter().take(5) {
            println!("scenario runtime error: {error}");
        }
        return;
    }
    let (mut killed, mut skipped) = (0, 0);
    for &t in targets.iter().take(cap) {
        if !b.alive(t) {
            continue;
        }
        if !b.place_near(t) {
            skipped += 1;
            continue;
        }
        for k in 0..tics_per_target {
            if !b.alive(t) {
                killed += 1;
                break;
            }
            let sp = b.s.world.as_ref().unwrap().sprites[t];
            b.aim(sp.x, sp.y, sp.z.saturating_sub(24 << 8));
            // Tap fire: semi-automatic weapons need the button released.
            let bits = if k % 4 < 2 { FIRE } else { 0 };
            if restore_at == Some(fight.ms.len()) && b.shadow.is_none() {
                b.start_restore_check(make_session().0);
            }
            b.tick(
                DukeInput {
                    bits,
                    ..DukeInput::default()
                },
                &mut fight,
            );
        }
    }
    let after_fight = b.live();
    if std::env::var_os("ROOM_BENCH_WEAPON_KEY").is_some() {
        println!(
            "fight weapon {weapon_at_fight}: ammo {ammo_at_fight} -> {}; now weapon {}",
            b.s.player.weapon_ammo(weapon_at_fight),
            b.s.player.curr_weapon
        );
    }
    if std::env::var_os("ROOM_BENCH_SPRITE_CENSUS").is_some() {
        let w = b.s.world.as_ref().unwrap();
        let mut survivors: BTreeMap<i16, usize> = BTreeMap::new();
        for &t in &targets {
            if b.alive(t) {
                *survivors.entry(w.sprites[t].picnum).or_default() += 1;
            }
        }
        println!("survivors by picnum: {survivors:?}");
        let mut errors: BTreeMap<String, usize> = BTreeMap::new();
        for error in b.s.take_eduke_runtime_errors() {
            if std::env::var_os("ROOM_BENCH_RAW_ERRORS").is_some() {
                println!("raw runtime error: {error}");
            }
            let key: String = error.chars().filter(|c| !c.is_ascii_digit()).collect();
            *errors.entry(key).or_default() += 1;
        }
        for (error, count) in errors.iter().take(8) {
            println!("runtime error x{count}: {error}");
        }
        if std::env::var_os("ROOM_BENCH_RAW_ERRORS").is_some() {
            // (raw errors were already drained above; rerun to inspect)
        }
    }
    if std::env::var_os("ROOM_BENCH_SPRITE_CENSUS").is_some() {
        // Most common live (picnum, statnum) pairs after the fight.
        let w = b.s.world.as_ref().unwrap();
        let mut census: BTreeMap<(i16, i16), usize> = BTreeMap::new();
        for (i, &st) in b.s.lists.sprite_stat.iter().enumerate() {
            if st != MAXSTATUS as i16 {
                *census.entry((w.sprites[i].picnum, st)).or_default() += 1;
            }
        }
        let mut rows: Vec<_> = census.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        for ((picnum, stat), count) in rows.iter().take(15) {
            println!("census picnum {picnum:>5} stat {stat:>3}: {count}");
        }
        let image = b.s.script.as_ref().unwrap();
        for ((picnum, stat), _) in rows.iter().take(15) {
            let entry = image.eduke_actor_entries.get(picnum);
            let sample = (0..w.sprites.len())
                .find(|&i| b.s.lists.sprite_stat[i] == *stat && w.sprites[i].picnum == *picnum);
            let temps = sample.and_then(|i| b.s.actor_temps.get(i)).map(|t| t.g_t);
            println!("census {picnum}: eduke entry {entry:?}, sample g_t {temps:?}");
            if let Some(i) = sample {
                let sp = w.sprites[i];
                println!(
                    "census {picnum}: sprite {i} cstat {} xr {} extra {} owner {} xvel {} zvel {} shade {}",
                    sp.cstat, sp.xrepeat, sp.extra, sp.owner, sp.xvel, sp.zvel, sp.shade
                );
            }
        }
    }
    for _ in 0..after_tics {
        b.tick(DukeInput::default(), &mut after);
    }
    println!(
        "targets {}  killed {killed}  no clear spot {skipped}  live sprites after fight {after_fight}  peak {}",
        targets.len(),
        b.peak_live
    );
    if restore_at.is_some() {
        match (b.shadow.is_some(), b.restore_mismatch) {
            (false, _) => println!("restore check: fight ended before the restore tic"),
            (true, None) => println!(
                "restore check: PASS, {} tics after the mid-fight restore matched byte for byte",
                b.restore_compared
            ),
            (true, Some(tic)) => println!(
                "restore check: FAIL, first difference {tic} tics after the restore ({} compared)",
                b.restore_compared
            ),
        }
    }
    settle.report("settle");
    let fight_mean = fight.report("fight");
    after.report("aftermath");
    let mut all = Samples::default();
    for s in [&settle, &fight, &after] {
        all.ms.extend_from_slice(&s.ms);
    }
    let overall = all.report("overall");
    let mean = fight_mean.or(overall).unwrap_or(0.0);
    let verdict = if mean <= NATIVE_BUDGET_MS {
        "PASS"
    } else {
        "OVER"
    };
    println!(
        "ED9 native budget: fight mean {mean:.3} ms/tic vs {NATIVE_BUDGET_MS} ms -> {verdict}"
    );
}

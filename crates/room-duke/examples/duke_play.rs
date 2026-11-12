//! Scripted headless play-throughs of native Duke levels, with frame dumps.
//!
//! `cargo run --release -p room-duke --example duke_play -- e1l1-vent /tmp/out`
//!
//! Scenarios drive `DukeSession` with real `DukeInput`s (walking, aiming,
//! firing) and only steer the heading/pitch directly, like a player with a
//! mouse. Each prints a per-tic event log and writes PPM frames.
use room_build_engine::world::MAXSTATUS;
use room_duke::{DukeInput, DukeSession, DukeSoftRenderer};
use std::path::{Path, PathBuf};

const FIRE: u32 = 1 << 2;

struct Run {
    s: DukeSession,
    r: DukeSoftRenderer,
    out: PathBuf,
    tic: u32,
}

impl Run {
    fn alive(&self, i: usize) -> bool {
        self.s
            .lists
            .sprite_stat
            .get(i)
            .is_some_and(|&st| st != MAXSTATUS as i16)
    }

    fn sprite(&self, i: usize) -> room_build_engine::types::SpriteT {
        self.s.world.as_ref().unwrap().sprites[i]
    }

    /// Point heading (and pitch, from eye to `z`) at a world point.
    fn aim(&mut self, x: i32, y: i32, z: i32) {
        let p = &mut self.s.player;
        let dx = f64::from(x - p.posx);
        let dy = f64::from(y - p.posy);
        let ang = dy.atan2(dx) * 1024.0 / std::f64::consts::PI;
        p.ang = (ang.round() as i32).rem_euclid(2048) as i16;
        // Hitscan zvel = (100 - horiz) * 2048 per 16384 XY: z/xy = (100-h)/8.
        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
        let slope = f64::from(z - p.posz) / dist;
        p.horiz = (100.0 - slope * 8.0).round().clamp(0.0, 299.0) as i16;
        p.horizoff = 0;
    }

    fn dist_to(&self, x: i32, y: i32) -> f64 {
        let p = &self.s.player;
        f64::from(x - p.posx).hypot(f64::from(y - p.posy))
    }

    fn step(&mut self, input: DukeInput) {
        self.s.tick(input);
        self.tic += 1;
    }

    fn shot(&self, name: &str) {
        let (w, h) = (640, 400);
        let px = self
            .r
            .render_rgba(&self.s.render_world().unwrap(), &self.s.player, w, h);
        let mut f = format!("P6\n{w} {h}\n255\n").into_bytes();
        for c in px.chunks(4) {
            f.extend_from_slice(&c[..3]);
        }
        let path = self.out.join(format!("{name}.ppm"));
        std::fs::write(&path, f).unwrap();
        println!("frame {} -> {}", self.tic, path.display());
    }

    fn sectors(&self, ids: &[usize]) -> Vec<(i32, i32)> {
        let w = self.s.world.as_ref().unwrap();
        ids.iter()
            .map(|&i| (w.sectors[i].ceilingz, w.sectors[i].floorz))
            .collect()
    }

    fn live_count(&self) -> usize {
        self.s
            .lists
            .sprite_stat
            .iter()
            .filter(|&&st| st != MAXSTATUS as i16)
            .count()
    }
}

fn load(map: &str, out: &Path) -> Run {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke");
    let grp = std::fs::read(root.join("duke3d.grp")).unwrap();
    let s = room_duke::session_from_grp_map(&grp, &format!("{map}.MAP"), 2).unwrap();
    let r = DukeSoftRenderer::from_grp(&grp).unwrap();
    std::fs::create_dir_all(out).unwrap();
    Run {
        s,
        r,
        out: out.to_path_buf(),
        tic: 0,
    }
}

/// E1L1: walk to the rooftop fan vent, shoot a gas tank (SEENINE, hitag
/// 253), and check the chain reaction blows the vent open and the player can
/// drop through it.
fn e1l1_vent(out: &Path) {
    let mut run = load("E1L1", out);
    for _ in 0..30 {
        run.step(DukeInput::default());
    }
    run.shot("00_start");
    const TANK: usize = 562;
    let watch = [268, 269, 272, 273];
    let before = run.sectors(&watch);
    let t = run.sprite(TANK);
    println!(
        "tank {TANK}: pic {} at ({}, {}, {})",
        t.picnum, t.x, t.y, t.z
    );
    // Walk until ~2500 units from the tank.
    let stop: f64 = std::env::var("ROOM_STOP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2500.0);
    room_duke::shoot::set_hit_debug(true);
    for _ in 0..400 {
        if run.dist_to(t.x, t.y) < stop {
            break;
        }
        run.aim(t.x, t.y, t.z - (8 << 8));
        let ang = run.s.player.ang;
        run.step(DukeInput::from_local(40, 0, ang, 0, 0, 0));
        if run.tic % 50 == 0 {
            let p = &run.s.player;
            println!(
                "  walk tic {} pos ({}, {}, {}) ang {} vel ({}, {}) sect {}",
                run.tic, p.posx, p.posy, p.posz, p.ang, p.posxv, p.posyv, p.cursectnum
            );
        }
    }
    println!(
        "tic {} walked to ({}, {}) dist {:.0} sect {}",
        run.tic,
        run.s.player.posx,
        run.s.player.posy,
        run.dist_to(t.x, t.y),
        run.s.player.cursectnum
    );
    run.aim(t.x, t.y, t.z - (12 << 8));
    if std::env::var("ROOM_PISTOL").is_ok() {
        println!("weapon {} -> pistol", run.s.player.curr_weapon);
        run.s.player.curr_weapon = 1;
    }
    run.shot("01_aim");
    let sprites_before = run.live_count();
    let mut exploded_at = None;
    for k in 0..240 {
        let tank = (run.alive(TANK) && run.sprite(TANK).picnum == t.picnum).then_some(());
        if tank.is_some() {
            run.aim(t.x, t.y, t.z - (12 << 8));
            if std::env::var("ROOM_FLAT").is_ok() {
                run.s.player.horiz = 100;
            }
        }
        let fire = tank.is_some() && k % 12 == 0;
        run.step(DukeInput {
            bits: if fire { FIRE } else { 0 },
            ..DukeInput::default()
        });
        for line in room_duke::shoot::take_hit_log() {
            println!("  [tic {}] {line}", run.tic);
        }
        if exploded_at.is_none() && !(run.alive(TANK) && run.sprite(TANK).picnum == t.picnum) {
            exploded_at = Some(run.tic);
            println!(
                "tic {} tank {TANK} gone (shot {k} tics after aiming)",
                run.tic
            );
        }
        if k == 20 || k == 60 {
            if std::env::var("ROOM_LIST").is_ok() {
                for (i, &st) in run.s.lists.sprite_stat.iter().enumerate() {
                    let sp = run.sprite(i);
                    if st == MAXSTATUS as i16
                        || f64::from(sp.x - t.x).hypot(f64::from(sp.y - t.y)) > 3000.0
                    {
                        continue;
                    }
                    println!(
                        "  live {i} pic {} stat {st} z {} cstat {} xr {} yr {} shade {} pal {}",
                        sp.picnum, sp.z, sp.cstat, sp.xrepeat, sp.yrepeat, sp.shade, sp.pal
                    );
                }
            }
            run.shot(&format!("02_after_{k}"));
        }
    }
    let after = run.sectors(&watch);
    for (i, (b, a)) in watch.iter().zip(before.iter().zip(after.iter())) {
        println!(
            "sector {i}: ceil/floor {:?} -> {:?}{}",
            b,
            a,
            if b == a { "" } else { "  CHANGED" }
        );
    }
    let live = |i: usize| if run.alive(i) { "alive" } else { "gone" };
    for i in [553, 555, 556, 558, 559, 560, 561, 562, 614, 70, 557, 554] {
        println!("sprite {i} pic {} {}", run.sprite(i).picnum, live(i));
    }
    println!("live sprites {sprites_before} -> {}", run.live_count());
    run.shot("03_settled");
    let f = run.sprite(553);
    println!(
        "fan 553 pic {} cstat {} sect {}",
        f.picnum, f.cstat, f.sectnum
    );
    // Walk through the broken fan into the shaft (sector 269, its SE7).
    let fan = run.sprite(554);
    for _ in 0..200 {
        if run.dist_to(fan.x, fan.y) < 64.0 || run.s.player.cursectnum == 269 {
            break;
        }
        run.aim(fan.x, fan.y, fan.z);
        let ang = run.s.player.ang;
        run.step(DukeInput::from_local(40, 0, ang, 0, 0, 0));
    }
    run.s.player.horiz = 100;
    for k in 0..120 {
        run.step(DukeInput::default());
        if k % 5 == 0 {
            let p = &run.s.player;
            let pi = p.sprite_i as usize;
            let se = run.sprite(554);
            println!(
                "  fall tic {} pos ({}, {}, {}) sect {} body sect {} stat {} | SE554 stat {} owner {} z {} | hold {} jump {} onground {}",
                run.tic,
                p.posx,
                p.posy,
                p.posz,
                p.cursectnum,
                run.sprite(pi).sectnum,
                run.s.lists.sprite_stat[pi],
                run.s.lists.sprite_stat[554],
                se.owner,
                se.z,
                p.transporter_hold,
                p.jumping_counter,
                p.on_ground
            );
        }
    }
    println!(
        "tic {} after vent: pos ({}, {}, {}) sect {} health {}",
        run.tic,
        run.s.player.posx,
        run.s.player.posy,
        run.s.player.posz,
        run.s.player.cursectnum,
        run.s.player.extra
    );
    run.shot("04_below");
    println!("exploded_at {exploded_at:?}");
    if let Ok(w) = std::env::var("ROOM_WATCH") {
        watch_actor(&mut run, w.parse().unwrap());
        return;
    }
    street_fight(&mut run);
}

/// Stand still (looking at `i`) and log whether the actor wakes and acts.
fn watch_actor(run: &mut Run, i: usize) {
    for k in 0..300 {
        let sp = run.sprite(i);
        run.aim(sp.x, sp.y, sp.z - (24 << 8));
        if std::env::var("ROOM_EVERY").is_ok() && run.tic >= 760 && run.tic < 840 {
            let t = &run.s.actor_temps[i];
            let sp = run.sprite(i);
            println!(
                "    T{} g_t {:?} lotag {} hitag {} ang {} dist {:.0}",
                run.tic,
                t.g_t,
                sp.lotag,
                sp.hitag,
                sp.ang,
                run.dist_to(sp.x, sp.y)
            );
        }
        if k % 10 == 0 {
            let t = &run.s.actor_temps[i];
            let name = |v: i32| {
                run.s
                    .script
                    .as_ref()
                    .and_then(|sc| {
                        sc.labels
                            .iter()
                            .find(|(n, l)| {
                                l.value == v && (n.starts_with("AI") || n.starts_with("A"))
                            })
                            .map(|(n, _)| n.clone())
                    })
                    .unwrap_or_default()
            };
            println!(
                "    ai {} action {} move {}",
                name(t.g_t[5]),
                name(t.g_t[4]),
                name(t.g_t[1])
            );
            println!(
                "tic {} actor {i} stat {} pic {} xy ({}, {}) z {} ang {} xvel {} extra {} g_t {:?} | player hp {} dist {:.0}",
                run.tic,
                run.s.lists.sprite_stat[i],
                sp.picnum,
                sp.x,
                sp.y,
                sp.z,
                sp.ang,
                sp.xvel,
                sp.extra,
                t.g_t,
                run.s.player.extra,
                run.dist_to(sp.x, sp.y)
            );
        }
        if k == 100 {
            run.shot("07_watch");
        }
        let before = run.s.player.extra;
        run.step(DukeInput::default());
        let proj: Vec<String> = run
            .s
            .lists
            .sprite_stat
            .iter()
            .enumerate()
            .filter(|(_, st)| **st == 4)
            .map(|(j, _)| {
                let p = run.sprite(j);
                format!(
                    "{j}:pic{} own{} d{:.0}",
                    p.picnum,
                    p.owner,
                    run.dist_to(p.x, p.y)
                )
            })
            .collect();
        if !proj.is_empty() || run.s.player.extra != before {
            println!(
                "  tic {} proj {:?} hp {} -> {}",
                run.tic, proj, before, run.s.player.extra
            );
        }
    }
}

/// Teleport next to every enemy on the map, shoot it with the pistol and
/// report whether it reacts (extra drops) and dies.
fn kill_all(map: &str, out: &Path) {
    let mut run = load(map, out);
    for _ in 0..5 {
        run.step(DukeInput::default());
    }
    let actortype = run.s.script.as_ref().unwrap().actortype.clone();
    let targets: Vec<usize> = (0..run.s.lists.sprite_stat.len())
        .filter(|&i| {
            let st = run.s.lists.sprite_stat[i];
            let sp = run.sprite(i);
            (st == 1 || st == 2)
                && sp.xrepeat > 0
                && room_duke::badguy::badguy_pic(sp.picnum, &actortype)
        })
        .collect();
    room_duke::shoot::set_hit_debug(true);
    let (mut killed, mut hurt, mut untouched) = (0, 0, Vec::new());
    for &t in &targets {
        if let Some(wt) = std::env::var("ROOM_WATCHT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            println!(
                "  watch before #{t}: #{wt} g_t {:?} extra {} stat {}",
                run.s.actor_temps[wt].g_t,
                run.sprite(wt).extra,
                run.s.lists.sprite_stat[wt]
            );
        }
        if !run.alive(t) {
            continue;
        }
        let sp = run.sprite(t);
        let w = run.s.world.as_ref().unwrap();
        // Find a free spot 700 units away with a clear line of sight.
        let mut placed = false;
        for k in 0..16 {
            let a = (i32::from(sp.ang) + k * 128) & 2047;
            let (sin, cos) = room_build_engine::helpers::sincos_q14(a);
            let x = sp.x + ((700 * cos) >> 14);
            let y = sp.y + ((700 * sin) >> 14);
            let mut sect = sp.sectnum;
            room_build_engine::geom::updatesector(x, y, &mut sect, &w.sector_walls(), &w.wall_xy());
            if sect < 0 {
                continue;
            }
            let fz = w.sectors[sect as usize].floorz;
            let eye = fz - (40 << 8);
            if !room_build_engine::geom::cansee(
                x,
                y,
                eye,
                sect,
                sp.x,
                sp.y,
                sp.z - (24 << 8),
                sp.sectnum,
                &w.sector_z(),
                &w.wall_xy(),
            ) {
                continue;
            }
            let p = &mut run.s.player;
            p.posx = x;
            p.posy = y;
            p.posz = eye;
            p.oposx = x;
            p.oposy = y;
            p.oposz = eye;
            p.cursectnum = sect;
            p.extra = 100;
            p.curr_weapon = 1;
            p.ammo[1] = 200;
            placed = true;
            break;
        }
        if !placed {
            println!(
                "#{t} pic {} stat {}: no clear spot",
                sp.picnum, run.s.lists.sprite_stat[t]
            );
            continue;
        }
        let start_extra = sp.extra;
        if std::env::var("ROOM_TRACE_T")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            == Some(t)
        {
            for k in 0..60 {
                let c = run.sprite(t);
                let tt = &run.s.actor_temps[t];
                println!(
                    "  trace #{t} k {k} extra {} hit_extra {} hit_pic {} g_t {:?} stat {} sect {} lotag {}",
                    c.extra,
                    tt.hit_extra,
                    tt.hit_picnum,
                    tt.g_t,
                    run.s.lists.sprite_stat[t],
                    c.sectnum,
                    run.s.world.as_ref().unwrap().sectors[c.sectnum as usize].lotag
                );
                run.step(DukeInput::default());
            }
        }
        let mut first_hit = String::new();
        let mut dead = false;
        for k in 0..240 {
            if !run.alive(t) || run.sprite(t).extra <= 0 {
                dead = true;
                break;
            }
            let cur = run.sprite(t);
            run.aim(cur.x, cur.y, cur.z - (24 << 8));
            run.s.player.extra = 100;
            let wt = std::env::var("ROOM_WATCHT")
                .ok()
                .and_then(|v| v.parse::<usize>().ok());
            let before = wt.map(|w| run.s.actor_temps[w].g_t);
            room_build_engine::numeric::krand_log::start();
            run.step(DukeInput {
                bits: if k % 8 == 0 { FIRE } else { 0 },
                ..DukeInput::default()
            });
            let calls = room_build_engine::numeric::krand_log::drain();
            if let (Some(w), Some(b)) = (wt, before) {
                if run.s.actor_temps[w].g_t != b {
                    println!(
                        "  #{w} g_t changed at tic {} (target #{t}): {b:?} -> {:?}",
                        run.tic, run.s.actor_temps[w].g_t
                    );
                    for c in calls.iter().take(40) {
                        println!("     krand {} {}", c.0, c.1);
                    }
                }
            }
            for l in room_duke::shoot::take_hit_log() {
                if first_hit.is_empty() {
                    first_hit = l;
                }
            }
        }
        let now = if run.alive(t) {
            run.sprite(t).extra
        } else {
            -1
        };
        if dead {
            killed += 1;
        } else if now < start_extra {
            hurt += 1;
        } else {
            untouched.push(t);
        }
        println!(
            "#{t} pic {} stat {} extra {start_extra} -> {now} dead {dead} | {first_hit}",
            sp.picnum,
            run.s.lists.sprite_stat.get(t).copied().unwrap_or(-1)
        );
    }
    println!(
        "targets {} killed {killed} hurt {hurt} untouched {:?}",
        targets.len(),
        untouched
    );
    for _ in 0..90 {
        run.step(DukeInput::default());
    }
    let w = run.s.world.as_ref().unwrap();
    for (i, &st) in run.s.lists.sprite_stat.iter().enumerate() {
        if st == MAXSTATUS as i16 || st > 2 {
            continue;
        }
        let sp = w.sprites[i];
        if sp.sectnum < 0 || sp.xrepeat == 0 {
            continue;
        }
        let fz = w.sectors[sp.sectnum as usize].floorz;
        if fz - sp.z > 4096 && sp.extra <= 0 && room_duke::badguy::badguy_pic(sp.picnum, &actortype)
        {
            let t = &run.s.actor_temps[i];
            println!(
                "FLOATING #{i} pic {} stat {st} z {} floor {} zvel {} hit_floorz {} cgg {} g_t {:?}",
                sp.picnum,
                sp.z,
                fz,
                sp.zvel,
                t.hit_floorz,
                t.cgg,
                &t.g_t[..6]
            );
        }
    }
}

/// `use-at MAP X Y ANG`: stand at (X, Y) facing ANG, render, press USE,
/// render again 60 tics later.
fn use_at(args: &[String], out: &Path) {
    let mut run = load(&args[0], out);
    for _ in 0..5 {
        run.step(DukeInput::default());
    }
    let v: Vec<i32> = args[1..4].iter().map(|a| a.parse().unwrap()).collect();
    let w = run.s.world.as_ref().unwrap();
    let mut sect = args
        .get(4)
        .and_then(|a| a.parse().ok())
        .unwrap_or(run.s.player.cursectnum);
    room_build_engine::geom::updatesector(v[0], v[1], &mut sect, &w.sector_walls(), &w.wall_xy());
    if let Some(h) = args.get(5).and_then(|a| a.parse::<i32>().ok()) {
        run.s.player.horiz = h as i16;
    }
    let fz = w.sectors[sect.max(0) as usize].floorz;
    {
        let p = &mut run.s.player;
        p.posx = v[0];
        p.posy = v[1];
        p.posz = fz - (40 << 8);
        p.oposx = v[0];
        p.oposy = v[1];
        p.oposz = p.posz;
        p.cursectnum = sect;
        p.ang = v[2] as i16;
        if args.get(5).is_none() {
            p.horiz = 100;
        }
    }
    run.step(DukeInput::default());
    println!(
        "at sect {sect} lotag {}",
        run.s.world.as_ref().unwrap().sectors[sect.max(0) as usize].lotag
    );
    run.shot("use_before");
    let fire = std::env::var_os("ROOM_FIRE").is_some();
    if fire {
        run.s.player.curr_weapon = 1;
    }
    for k in 0..60 {
        let bit = if fire { FIRE } else { 1 << 29 };
        run.step(DukeInput {
            bits: if k == 1 { bit } else { 0 },
            ..DukeInput::default()
        });
        if fire && k % 6 == 0 {
            let pieces: Vec<String> = (0..run.s.lists.sprite_stat.len())
                .filter(|&j| run.alive(j) && (1031..=1033).contains(&run.sprite(j).picnum))
                .map(|j| {
                    let q = run.sprite(j);
                    format!("{j}:z{}v{}r{}", q.z, q.zvel, q.xrepeat)
                })
                .collect();
            println!(
                "k{k} glass {}: {:?}",
                pieces.len(),
                pieces.iter().take(4).collect::<Vec<_>>()
            );
        }
        if fire && k == 8 {
            run.shot("glass_mid");
        }
    }
    run.shot("use_after");
    if let Ok(list) = std::env::var("ROOM_SECTS") {
        let w = run.s.world.as_ref().unwrap();
        for v in list.split(',') {
            let i: usize = v.parse().unwrap();
            let s = w.sectors[i];
            println!(
                "  sect {i} c {} f {} lotag {} cshade {} fshade {}",
                s.ceilingz, s.floorz, s.lotag, s.ceilingshade, s.floorshade
            );
        }
    }
    if std::env::var_os("ROOM_WALK").is_some() {
        for k in 0..90 {
            let ang = run.s.player.ang;
            run.step(DukeInput::from_local(40, 0, ang, 0, 0, 0));
            if k % 10 == 0 {
                let p = &run.s.player;
                println!(
                    "walk {k} pos ({}, {}) z {} sect {} fz {}",
                    p.posx,
                    p.posy,
                    p.posz,
                    p.cursectnum,
                    run.s.world.as_ref().unwrap().sectors[p.cursectnum.max(0) as usize].floorz
                );
            }
        }
        run.shot("use_walk");
        if let Ok(ws) = std::env::var("ROOM_WALLS") {
            let w = run.s.world.as_ref().unwrap();
            for r in ws.split(',') {
                let i: usize = r.parse().unwrap();
                println!(
                    "  wall {i} ({}, {}) next {}",
                    w.walls[i].x, w.walls[i].y, w.walls[i].nextsector
                );
            }
        }
        let (px, py) = (run.s.player.posx, run.s.player.posy);
        for (i, &st) in run.s.lists.sprite_stat.iter().enumerate() {
            let sp = run.sprite(i);
            if st == MAXSTATUS as i16 || f64::from(sp.x - px).hypot(f64::from(sp.y - py)) > 600.0 {
                continue;
            }
            println!(
                "  near {i} pic {} stat {st} cstat {} clip {} xy ({}, {}) z {} sect {}",
                sp.picnum, sp.cstat, sp.clipdist, sp.x, sp.y, sp.z, sp.sectnum
            );
        }
    }
    let p = &run.s.player;
    println!(
        "weapon {} last {} pos {} kick {} sway {} wang {} look {} hard {} qk {}",
        p.curr_weapon,
        p.last_weapon,
        p.weapon_pos,
        p.kickback_pic,
        p.weapon_sway,
        p.weapon_ang,
        p.look_ang,
        p.hard_landing,
        p.quick_kick
    );
    let env = room_duke::weapon_view::WeaponViewEnv {
        body_xrepeat: 42,
        body_extra: 100,
        ..Default::default()
    };
    println!("{:?}", room_duke::weapon_view::display_weapon(p, &env));
}

/// Stand in front of every operable sector (doors, lifts...) and press USE;
/// report whether its ceiling/floor or walls move.
fn open_all(map: &str, out: &Path) {
    let mut run = load(map, out);
    for _ in 0..5 {
        run.step(DukeInput::default());
    }
    let only: Option<i32> = std::env::var("ROOM_SECT").ok().and_then(|v| v.parse().ok());
    let nsect = run.s.world.as_ref().unwrap().sectors.len();
    let (mut total, mut moved) = (0, 0);
    for sn in 0..nsect {
        if only.is_some_and(|o| o != sn as i32) {
            continue;
        }
        let w = run.s.world.as_ref().unwrap();
        let sec = w.sectors[sn];
        let lt = sec.lotag & 0x7fff;
        let operable = matches!(lt & 0xff, 9 | 15..=23 | 25 | 26 | 29);
        if !operable {
            continue;
        }
        total += 1;
        run = load(map, out);
        for _ in 0..5 {
            run.step(DukeInput::default());
        }
        let w = run.s.world.as_ref().unwrap();
        // A two-sided wall of this sector: stand on its other side.
        let first = sec.wallptr as usize;
        let mut placed = None;
        for wi in first..first + sec.wallnum as usize {
            let wl = w.walls[wi];
            if wl.nextsector < 0 || w.sectors[wl.nextsector as usize].lotag != 0 {
                continue;
            }
            let p2 = w.walls[wl.point2 as usize];
            let (mx, my) = ((wl.x + p2.x) / 2, (wl.y + p2.y) / 2);
            let (dx, dy) = (f64::from(p2.x - wl.x), f64::from(p2.y - wl.y));
            let len = dx.hypot(dy).max(1.0);
            // Build walls run clockwise: the inside is on the right, so the
            // other sector is on the left (-dy, dx).
            let (nx, ny) = (dy / len, -dx / len);
            // Prefer the side facing away from the door sector's centre.
            let (mut cx, mut cy) = (0_i64, 0_i64);
            for k in first..first + sec.wallnum as usize {
                cx += i64::from(w.walls[k].x);
                cy += i64::from(w.walls[k].y);
            }
            let n = i64::from(sec.wallnum.max(1));
            let (cx, cy) = ((cx / n) as i32, (cy / n) as i32);
            let away = f64::from(mx - cx) * nx + f64::from(my - cy) * ny;
            let order = if away >= 0.0 {
                [220.0, -220.0]
            } else {
                [-220.0, 220.0]
            };
            for dist in order {
                let x = mx + (nx * dist) as i32;
                let y = my + (ny * dist) as i32;
                let mut sect = wl.nextsector;
                room_build_engine::geom::updatesector(
                    x,
                    y,
                    &mut sect,
                    &w.sector_walls(),
                    &w.wall_xy(),
                );
                if sect < 0 || sect as usize == sn {
                    continue;
                }
                placed = Some((x, y, sect, cx, cy));
                break;
            }
            if placed.is_some() {
                break;
            }
        }
        let Some((x, y, sect, mx, my)) = placed else {
            println!(
                "sector {sn} lotag {} hitag {}: no approach",
                sec.lotag, sec.hitag
            );
            continue;
        };
        let snap = |run: &Run| {
            let w = run.s.world.as_ref().unwrap();
            let s = w.sectors[sn];
            let first = s.wallptr as usize;
            let xy: Vec<(i32, i32)> = (first..first + s.wallnum as usize)
                .map(|i| (w.walls[i].x, w.walls[i].y))
                .collect();
            (s.ceilingz, s.floorz, xy)
        };
        let before = snap(&run);
        {
            let fz = run.s.world.as_ref().unwrap().sectors[sect as usize].floorz;
            let p = &mut run.s.player;
            p.posx = x;
            p.posy = y;
            p.posz = fz - (40 << 8);
            p.oposx = x;
            p.oposy = y;
            p.oposz = p.posz;
            p.cursectnum = sect;
            p.extra = 100;
        }
        run.aim(mx, my, run.s.player.posz);
        run.s.player.horiz = 100;
        if std::env::var_os("ROOM_SHOTS").is_some() {
            run.shot(&format!("door_{sn}"));
        }
        let mut trace = Vec::new();
        let mut log = String::new();
        for k in 0..90 {
            run.step(DukeInput {
                bits: if k == 1 { 1 << 29 } else { 0 },
                ..DukeInput::default()
            });
            if k == 1 {
                log = run.s.active_quote().unwrap_or_default().to_string();
            }
            if k % 15 == 0 {
                let w = run.s.world.as_ref().unwrap();
                trace.push((w.sectors[sn].ceilingz, w.sectors[sn].floorz));
            }
        }
        let after = snap(&run);
        if std::env::var_os("ROOM_USE_DEBUG").is_some() {
            for (j, &st) in run.s.lists.sprite_stat.iter().enumerate() {
                let q = run.sprite(j);
                if st == 3 && q.sectnum as usize == sn {
                    println!(
                        "   SE #{j} lotag {} hitag {} ang {} yvel {} g_t {:?}",
                        q.lotag,
                        q.hitag,
                        q.ang,
                        q.yvel,
                        &run.s.actor_temps[j].g_t[..6]
                    );
                }
            }
        }
        let ok = before != after;
        if ok {
            moved += 1;
        }
        println!(
            "sector {sn} lotag {} hitag {} from sect {sect}: {} {log} {trace:?}",
            sec.lotag,
            sec.hitag,
            if ok { "MOVED" } else { "STUCK" }
        );
    }
    println!("operable {total} moved {moved}");
}

/// Teleport next to every shootable prop (hydrants, barrels, lights...) and
/// fire the pistol at it; report whether its picnum changes or it vanishes.
fn break_all(map: &str, out: &Path) {
    let mut run = load(map, out);
    for _ in 0..5 {
        run.step(DukeInput::default());
    }
    let names = std::env::var("ROOM_PROPS").unwrap_or_else(|_| "HYDRENT,EXPLODINGBARREL,EXPLODINGBARREL2,FIREBARREL,WOODENHORSE,HORSEONSIDE,TOILET,STALL,FIREEXT,TREE1,TREE2,CACTUS,HANGLIGHT,GENERICPOLE2,VASE,WATERFOUNTAIN,FETUS,SEENINE,OOZFILTER,CHAIR3,PLANT,BOTTLE1,CAMERA1,SPACEMARINE,STATUE,MASKWALL7".to_string());
    let labels = run.s.script.as_ref().unwrap().labels.clone();
    let pics: Vec<(i16, String)> = names
        .split(',')
        .filter_map(|n| labels.get(n).map(|l| (l.value as i16, n.to_string())))
        .collect();
    let targets: Vec<usize> = (0..run.s.lists.sprite_stat.len())
        .filter(|&i| {
            run.alive(i)
                && run.sprite(i).xrepeat > 0
                && pics.iter().any(|(p, _)| *p == run.sprite(i).picnum)
        })
        .collect();
    room_duke::shoot::set_hit_debug(true);
    let (mut broke, mut total) = (0, 0);
    for &t in &targets {
        if !run.alive(t) {
            continue;
        }
        let sp = run.sprite(t);
        let name = pics
            .iter()
            .find(|(p, _)| *p == sp.picnum)
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        let w = run.s.world.as_ref().unwrap();
        let mut placed = false;
        for k in 0..16 {
            let a = (i32::from(sp.ang) + k * 128) & 2047;
            let (sin, cos) = room_build_engine::helpers::sincos_q14(a);
            let x = sp.x + ((500 * cos) >> 14);
            let y = sp.y + ((500 * sin) >> 14);
            let mut sect = sp.sectnum;
            room_build_engine::geom::updatesector(x, y, &mut sect, &w.sector_walls(), &w.wall_xy());
            if sect < 0 {
                continue;
            }
            let eye = w.sectors[sect as usize].floorz - (40 << 8);
            if !room_build_engine::geom::cansee(
                x,
                y,
                eye,
                sect,
                sp.x,
                sp.y,
                sp.z - (4 << 8),
                sp.sectnum,
                &w.sector_z(),
                &w.wall_xy(),
            ) {
                continue;
            }
            let p = &mut run.s.player;
            p.posx = x;
            p.posy = y;
            p.posz = eye;
            p.oposx = x;
            p.oposy = y;
            p.oposz = eye;
            p.cursectnum = sect;
            p.extra = 100;
            p.curr_weapon = 1;
            p.ammo[1] = 200;
            placed = true;
            break;
        }
        total += 1;
        if !placed {
            println!("#{t} {name}: no clear spot");
            continue;
        }
        let mut log = Vec::new();
        let mut changed = false;
        for k in 0..48 {
            if !run.alive(t) || run.sprite(t).picnum != sp.picnum {
                changed = true;
                break;
            }
            let cur = run.sprite(t);
            let up = if cur.cstat & 128 != 0 { 0 } else { 6 << 8 };
            run.aim(cur.x, cur.y, cur.z - up);
            run.s.player.extra = 100;
            run.step(DukeInput {
                bits: if k % 8 == 0 { FIRE } else { 0 },
                ..DukeInput::default()
            });
            log.extend(room_duke::shoot::take_hit_log());
        }
        if changed {
            broke += 1;
        }
        if changed && std::env::var_os("ROOM_AFTER").is_some() {
            for k in 0..40 {
                run.step(DukeInput::default());
                if k % 8 == 0 {
                    for (j, &st) in run.s.lists.sprite_stat.iter().enumerate() {
                        let q = run.sprite(j);
                        if st == MAXSTATUS as i16
                            || f64::from(q.x - sp.x).hypot(f64::from(q.y - sp.y)) > 300.0
                        {
                            continue;
                        }
                        let gt = run.s.actor_temps.get(j).map(|t| t.g_t).unwrap_or_default();
                        println!(
                            "    k{k} #{j} pic {} stat {st} xr {} yr {} z {} zvel {} g_t {:?}",
                            q.picnum,
                            q.xrepeat,
                            q.yrepeat,
                            q.z,
                            q.zvel,
                            &gt[..5]
                        );
                    }
                }
            }
            run.shot(&format!("broken_{t}"));
        }
        let now = if run.alive(t) {
            run.sprite(t).picnum
        } else {
            -1
        };
        println!(
            "#{t} {name} pic {} -> {now} cstat {} broke {changed} | {}",
            sp.picnum,
            sp.cstat,
            log.first().cloned().unwrap_or_default()
        );
    }
    println!("props {total} broke {broke}");
}

/// Look for live actors (STAT_ACTOR) near the player, then fight: aim at the
/// nearest, fire every few tics, and log damage both ways.
fn street_fight(run: &mut Run) {
    const STAT_ACTOR: i16 = 1;
    let mut killed = 0;
    let start_health = run.s.player.extra;
    let mut frames = 0;
    for k in 0..900 {
        // Nearest live actor with health.
        let mut best: Option<(usize, f64)> = None;
        for (i, &st) in run.s.lists.sprite_stat.iter().enumerate() {
            if st != STAT_ACTOR {
                continue;
            }
            let sp = run.sprite(i);
            if sp.extra <= 0 {
                continue;
            }
            let d = run.dist_to(sp.x, sp.y);
            if d < 7000.0 && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        let mut input =
            DukeInput::from_local(0, 0, run.s.player.ang, 0, 0, room_duke::INPUT_AUTO_AIM);
        if let Some((i, d)) = best {
            let sp = run.sprite(i);
            run.aim(sp.x, sp.y, sp.z - (24 << 8));
            if std::env::var("ROOM_FLAT").is_ok() {
                // Like a mouse player: level view, slightly off-centre.
                run.s.player.horiz = 100;
                run.s.player.ang = (run.s.player.ang + 12) & 2047;
            }
            if k % 6 == 0 {
                input.bits |= FIRE;
            }
            if k % 30 == 0 {
                println!(
                    "tic {} target {i} pic {} dist {d:.0} extra {} cstat {} xr {} yr {} clip {} owner {} z {} stat {} | player health {}",
                    run.tic,
                    sp.picnum,
                    sp.extra,
                    sp.cstat,
                    sp.xrepeat,
                    sp.yrepeat,
                    sp.clipdist,
                    sp.owner,
                    sp.z,
                    run.s.lists.sprite_stat[i],
                    run.s.player.extra
                );
            }
            if frames < 3 && k % 120 == 0 {
                run.shot(&format!("05_fight_{frames}"));
                frames += 1;
            }
        } else if k % 60 == 0 {
            println!("tic {} no live actor within 7000", run.tic);
            // Walk toward the cinema (forward) to find enemies.
        }
        if best.is_none() {
            let ang = run.s.player.ang;
            input = DukeInput::from_local(40, 0, ang, if k % 90 < 45 { 0 } else { 8 }, 0, 0);
        }
        let before: Vec<i16> = best
            .map(|(i, _)| vec![run.sprite(i).extra])
            .unwrap_or_default();
        run.step(input);
        if let (Some((i, _)), Some(&b)) = (best, before.first()) {
            let alive = run.alive(i) && run.sprite(i).extra > 0;
            if b > 0 && !alive {
                killed += 1;
                println!("tic {} actor {i} killed", run.tic);
            }
        }
        if run.s.player.extra <= 0 {
            println!("tic {} player died", run.tic);
            break;
        }
    }
    println!(
        "street fight: killed {killed}, health {} -> {}",
        start_health, run.s.player.extra
    );
    run.shot("06_end");
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let scenario = a.get(1).map_or("e1l1-vent", String::as_str);
    let out = PathBuf::from(a.get(2).map_or("/tmp/duke_play", String::as_str));
    match scenario {
        "e1l1-vent" => e1l1_vent(&out),
        "kill-all" => kill_all(a.get(3).map_or("E1L1", String::as_str), &out),
        "reload" => {
            let mut run = load("E1L1", &out);
            for _ in 0..40 {
                run.step(DukeInput::default());
            }
            run.s.player.curr_weapon = 1;
            run.s.player.ammo[1] = 13;
            for k in 0..32 {
                run.step(DukeInput {
                    bits: if k == 0 { FIRE } else { 0 },
                    ..DukeInput::default()
                });
                let kb = run.s.player.kickback_pic;
                if [1, 6, 12, 17, 21, 24].contains(&kb) {
                    run.shot(&format!("reload_{kb:02}"));
                }
            }
        }
        "bonus" => {
            let mut run = load("E1L1", &out);
            for _ in 0..90 {
                run.step(DukeInput::default());
            }
            let mut b = run.s.bonus_screen();
            for (k, clocks) in [(0, 200), (1, 300), (2, 900)] {
                b.tick(clocks);
                let px = run.r.render_bonus_screen(640, 400, &b);
                let mut f = b"P6\n640 400\n255\n".to_vec();
                for c in px.chunks(4) {
                    f.extend_from_slice(&c[..3]);
                }
                std::fs::write(out.join(format!("bonus_{k}.ppm")), f).unwrap();
            }
            println!("level {:?} stats {:?}", b.level_name, b.stats);
        }
        "menus" => {
            use room_duke::menu::{DukeMenu, MenuKey};
            let run = load("E1L1", &out);
            let sc = run.s.script.as_ref().unwrap();
            let mut m = DukeMenu::new(false, sc.volume_names.to_vec(), sc.skill_names.to_vec());
            let save = |name: &str, px: Vec<u8>| {
                let mut f = b"P6\n640 400\n255\n".to_vec();
                for c in px.chunks(4) {
                    f.extend_from_slice(&c[..3]);
                }
                std::fs::write(out.join(format!("{name}.ppm")), f).unwrap();
            };
            save("menu_main", run.r.render_menu(640, 400, &m, 40, true));
            m.key(MenuKey::Enter);
            save("menu_episode", run.r.render_menu(640, 400, &m, 40, true));
            m.key(MenuKey::Enter);
            save("menu_skill", run.r.render_menu(640, 400, &m, 40, true));
            let mut c = DukeMenu::new(false, Vec::new(), Vec::new());
            for _ in 0..4 {
                c.key(MenuKey::Down);
            }
            c.key(MenuKey::Enter);
            save("menu_credits", run.r.render_menu(640, 400, &c, 40, true));
            let g = DukeMenu::new(true, Vec::new(), Vec::new());
            let game = run
                .r
                .render_rgba(&run.s.render_world().unwrap(), &run.s.player, 640, 400);
            let over = run.r.render_menu(640, 400, &g, 40, false);
            let mixed: Vec<u8> = game
                .chunks(4)
                .zip(over.chunks(4))
                .flat_map(|(a, b)| {
                    if b[3] == 0 {
                        [a[0], a[1], a[2], 255]
                    } else {
                        [b[0], b[1], b[2], 255]
                    }
                })
                .collect();
            save("menu_ingame", mixed);
        }
        "airkill" => {
            // Lift an enemy, shoot it dead in the air, watch it fall.
            let mut run = load("E1L1", &out);
            for _ in 0..5 {
                run.step(DukeInput::default());
            }
            let t: usize = a.get(3).and_then(|v| v.parse().ok()).unwrap_or(514);
            let lift: i32 = std::env::var("ROOM_LIFT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20000);
            let sp = run.sprite(t);
            let w = run.s.world.as_ref().unwrap();
            let fz = w.sectors[sp.sectnum as usize].floorz;
            run.s.world.as_mut().unwrap().sprites[t].z = sp.z - lift;
            let p = &mut run.s.player;
            p.posx = sp.x - 600;
            p.posy = sp.y;
            p.posz = fz - (40 << 8);
            p.oposx = p.posx;
            p.oposy = p.posy;
            p.oposz = p.posz;
            p.cursectnum = sp.sectnum;
            p.curr_weapon = 1;
            p.ammo[1] = 200;
            println!(
                "target #{t} pic {} floor {fz} start z {}",
                sp.picnum,
                sp.z - lift
            );
            for k in 0..120 {
                let cur = run.sprite(t);
                run.aim(cur.x, cur.y, cur.z - (24 << 8));
                run.s.player.extra = 100;
                let fire = k % 6 == 0 && cur.extra > 0;
                run.step(DukeInput {
                    bits: if fire { FIRE } else { 0 },
                    ..DukeInput::default()
                });
                let c = run.sprite(t);
                let tt = &run.s.actor_temps[t];
                if k % 6 == 0 || !run.alive(t) {
                    println!(
                        "k{k} alive {} stat {} pic {} z {} zvel {} extra {} hitfz {} cgg {} action {}",
                        run.alive(t),
                        run.s.lists.sprite_stat[t],
                        c.picnum,
                        c.z,
                        c.zvel,
                        c.extra,
                        tt.hit_floorz,
                        tt.cgg,
                        tt.g_t[4]
                    );
                }
                if !run.alive(t) {
                    break;
                }
            }
        }
        "use-at" => use_at(&a[3..], &out),
        "open-all" => open_all(a.get(3).map_or("E1L1", String::as_str), &out),
        "break-all" => break_all(a.get(3).map_or("E1L1", String::as_str), &out),
        other => eprintln!("unknown scenario {other}"),
    }
}

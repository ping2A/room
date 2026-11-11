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
        self.s.lists.sprite_stat.get(i).is_some_and(|&st| st != MAXSTATUS as i16)
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
    println!("tank {TANK}: pic {} at ({}, {}, {})", t.picnum, t.x, t.y, t.z);
    // Walk until ~2500 units from the tank.
    for _ in 0..400 {
        if run.dist_to(t.x, t.y) < 2500.0 {
            break;
        }
        run.aim(t.x, t.y, t.z - (8 << 8));
        let ang = run.s.player.ang;
        run.step(DukeInput::from_local(40, 0, ang, 0, 0, 0));
        if run.tic % 50 == 0 {
            let p = &run.s.player;
            println!("  walk tic {} pos ({}, {}, {}) ang {} vel ({}, {}) sect {}", run.tic, p.posx, p.posy, p.posz, p.ang, p.posxv, p.posyv, p.cursectnum);
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
        }
        let fire = tank.is_some() && k % 12 == 0;
        run.step(DukeInput {
            bits: if fire { FIRE } else { 0 },
            ..DukeInput::default()
        });
        if exploded_at.is_none() && !(run.alive(TANK) && run.sprite(TANK).picnum == t.picnum) {
            exploded_at = Some(run.tic);
            println!("tic {} tank {TANK} gone (shot {k} tics after aiming)", run.tic);
        }
        if k == 20 || k == 60 {
            if std::env::var("ROOM_LIST").is_ok() {
                for (i, &st) in run.s.lists.sprite_stat.iter().enumerate() {
                    let sp = run.sprite(i);
                    if st == MAXSTATUS as i16 || f64::from(sp.x - t.x).hypot(f64::from(sp.y - t.y)) > 3000.0 { continue; }
                    println!("  live {i} pic {} stat {st} z {} cstat {} xr {} yr {} shade {} pal {}", sp.picnum, sp.z, sp.cstat, sp.xrepeat, sp.yrepeat, sp.shade, sp.pal);
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
    println!("fan 553 pic {} cstat {} sect {}", f.picnum, f.cstat, f.sectnum);
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
            println!("  fall tic {} pos ({}, {}, {}) sect {} body sect {} stat {} | SE554 stat {} owner {} z {} | hold {} jump {} onground {}", run.tic, p.posx, p.posy, p.posz, p.cursectnum, run.sprite(pi).sectnum, run.s.lists.sprite_stat[pi], run.s.lists.sprite_stat[554], se.owner, se.z, p.transporter_hold, p.jumping_counter, p.on_ground);
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
            println!("    T{} g_t {:?} lotag {} hitag {} ang {} dist {:.0}", run.tic, t.g_t, sp.lotag, sp.hitag, sp.ang, run.dist_to(sp.x, sp.y));
        }
        if k % 10 == 0 {
            let t = &run.s.actor_temps[i];
            let name = |v: i32| run.s.script.as_ref().and_then(|sc| sc.labels.iter().find(|(n, l)| l.value == v && (n.starts_with("AI") || n.starts_with("A"))).map(|(n, _)| n.clone())).unwrap_or_default();
            println!("    ai {} action {} move {}", name(t.g_t[5]), name(t.g_t[4]), name(t.g_t[1]));
            println!(
                "tic {} actor {i} stat {} pic {} xy ({}, {}) z {} ang {} xvel {} extra {} g_t {:?} | player hp {} dist {:.0}",
                run.tic, run.s.lists.sprite_stat[i], sp.picnum, sp.x, sp.y, sp.z, sp.ang, sp.xvel, sp.extra, t.g_t, run.s.player.extra, run.dist_to(sp.x, sp.y)
            );
        }
        if k == 100 { run.shot("07_watch"); }
        let before = run.s.player.extra;
        run.step(DukeInput::default());
        let proj: Vec<String> = run.s.lists.sprite_stat.iter().enumerate().filter(|(_, st)| **st == 4).map(|(j, _)| { let p = run.sprite(j); format!("{j}:pic{} own{} d{:.0}", p.picnum, p.owner, run.dist_to(p.x, p.y)) }).collect();
        if !proj.is_empty() || run.s.player.extra != before { println!("  tic {} proj {:?} hp {} -> {}", run.tic, proj, before, run.s.player.extra); }
    }
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
        let mut input = DukeInput::from_local(0, 0, run.s.player.ang, 0, 0, room_duke::INPUT_AUTO_AIM);
        if let Some((i, d)) = best {
            let sp = run.sprite(i);
            run.aim(sp.x, sp.y, sp.z - (24 << 8));
            if k % 6 == 0 {
                input.bits |= FIRE;
            }
            if k % 30 == 0 {
                println!(
                    "tic {} target {i} pic {} dist {d:.0} extra {} | player health {}",
                    run.tic, sp.picnum, sp.extra, run.s.player.extra
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
        let before: Vec<i16> = best.map(|(i, _)| vec![run.sprite(i).extra]).unwrap_or_default();
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
        other => eprintln!("unknown scenario {other}"),
    }
}

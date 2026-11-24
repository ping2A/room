//! Native-runtime side of the DEMO sync oracle (see `scripts/duke-oracle/`).
//!
//! `cargo run --release -p room-duke --example duke_krand -- DEMO1 <tics>`
//! plays the demo in the honest native mode (no trace fixups, no RNG pads)
//! and prints one line per `krand` call (`tic seed-before call-site`) plus
//! `# endtic <seed>` markers; compare with the oracle's `k.log` using
//! `scripts/duke-oracle/krand_cmp.py`. Optional env:
//! `ROOM_DUMP=<file>` sprite dump after the last tic (oracle `sp.txt` format),
//! `ROOM_SHOW=i,j` sprite fields after load, `ROOM_SECT=i,j` sector planes,
//! `ROOM_TEMPS=i,j` CON temps, `ROOM_PT=1` per-tic player action state,
//! `ROOM_CON_OP_TRACE=<sprite>` every CON opcode that sprite executes.
use room_build_engine::numeric::krand_log;
use std::path::PathBuf;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = a.get(1).map_or("DEMO1", String::as_str);
    let tics: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke");
    let g = std::fs::read(root.join("duke3d.grp")).unwrap();
    let dm = std::fs::read(root.join(format!("demos/{d}.DMO"))).unwrap();
    krand_log::start();
    let (mut s, demo) = room_duke::session_from_grp_demo(&g, &dm).unwrap();
    s.trace_fixups = false;
    s.rng_pad_per_tic = 0;
    for (seed, site) in krand_log::drain() {
        println!("-1 {seed} {site}");
    }
    if let Ok(ids) = std::env::var("ROOM_SHOW") {
        let w = s.world.as_ref().unwrap();
        for id in ids.split(',') {
            let i: usize = id.parse().unwrap();
            let sp = &w.sprites[i];
            eprintln!(
                "spr {i} pic={} stat={} lotag={} hitag={} sect={} ang={} xvel={} owner={} sect_lotag={}",
                sp.picnum,
                s.lists.sprite_stat[i],
                sp.lotag,
                sp.hitag,
                sp.sectnum,
                sp.ang,
                sp.xvel,
                sp.owner,
                w.sectors[sp.sectnum as usize].lotag
            );
        }
    }
    let tic_marks = std::env::var_os("ROOM_CON_OP_TRACE").is_some();
    for t in 0..tics {
        if tic_marks {
            // Op traces carry no tic: mark each tic with its k.log number.
            eprintln!("CONOP-TIC {t}");
        }
        s.tick(room_duke::DukeInput::from_dmo(&demo.tics[t]));
        if std::env::var_os("ROOM_PLAYER").is_some() {
            let p = &s.player;
            eprintln!(
                "PLAYER {t} pos=({},{},{}) zv={} jc={} ground={} fall={} sect={} ang={} truefz={} sectfz={}",
                p.posx,
                p.posy,
                p.posz,
                p.poszv,
                p.jumping_counter,
                p.on_ground,
                p.falling_counter,
                p.cursectnum,
                p.ang,
                0,
                s.world
                    .as_ref()
                    .map_or(0, |w| w.sectors[p.cursectnum.max(0) as usize].floorz)
            );
        }
        for (seed, site) in krand_log::drain() {
            println!("{t} {seed} {site}");
        }
        println!("# endtic {}", s.randomseed);
        if std::env::var("ROOM_PT").is_ok() {
            let pi = s.player.sprite_i as usize;
            let tt = &s.actor_temps[pi];
            eprintln!(
                "tic{t} p_i={pi} t0={} t2={} t4={} lotag={} xvel={}",
                tt.g_t[0],
                tt.g_t[2],
                tt.g_t[4],
                s.world.as_ref().unwrap().sprites[pi].lotag,
                s.world.as_ref().unwrap().sprites[pi].xvel
            );
        }
    }
    if let Ok(ids) = std::env::var("ROOM_SECT") {
        let w = s.world.as_ref().unwrap();
        for id in ids.split(',') {
            let i: usize = id.parse().unwrap();
            let se = &w.sectors[i];
            eprintln!(
                "sect {i} lotag={} hitag={} floorz={} ceilz={} fstat={} cstat={}",
                se.lotag, se.hitag, se.floorz, se.ceilingz, se.floorstat, se.ceilingstat
            );
        }
    }
    if let Ok(ids) = std::env::var("ROOM_TEMPS") {
        for id in ids.split(',') {
            let i: usize = id.parse().unwrap();
            if let Some(t) = s.actor_temps.get(i) {
                eprintln!(
                    "temps {i} warp_floorz={:?} warp_ceilingz={:?} g_t={:?}",
                    t.warp_floorz, t.warp_ceilingz, t.g_t
                );
            }
        }
    }
    if let Ok(path) = std::env::var("ROOM_DUMP") {
        let w = s.world.as_ref().unwrap();
        let mut out = String::new();
        let mut n = 0;
        for (i, &st) in s.lists.sprite_stat.iter().enumerate() {
            if st == room_build_engine::world::MAXSTATUS as i16 {
                continue;
            }
            n += 1;
            let sp = &w.sprites[i];
            let tt = s.actor_temps.get(i).map(|t| t.g_t).unwrap_or([0; 6]);
            out += &format!(
                "{} {} {} {} {} {} sect={} ang={} xv={} zv={} ext={} own={} xr={} yr={} cs={} t0={} t1={} t2={} t3={} t5={}\n",
                i,
                sp.picnum,
                st,
                sp.x,
                sp.y,
                sp.z,
                sp.sectnum,
                sp.ang,
                sp.xvel,
                sp.zvel,
                sp.extra,
                sp.owner,
                sp.xrepeat,
                sp.yrepeat,
                sp.cstat,
                tt[0],
                tt[1],
                tt[2],
                tt[3],
                tt[5]
            );
        }
        std::fs::write(path, format!("numsprites={n}\n{out}")).unwrap();
    }
}

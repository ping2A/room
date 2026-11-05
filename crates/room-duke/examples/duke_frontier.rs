//! DEMO1/2/3 sync frontier: `cargo run -p room-duke --example duke_frontier [real|legacy] [DEMO1] [limit]`.
use std::path::PathBuf;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).map_or("real", String::as_str);
    let demo = a.get(2).map_or("DEMO1", String::as_str);
    let limit: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke");
    let grp = root.join("duke3d.grp");
    let dmo = root.join(format!("demos/{demo}.DMO"));
    let tr = root.join(format!("traces/{demo}.trace"));
    if mode == "pics" {
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let demo = room_build::DmoDemo::parse(&d).unwrap();
        let name = room_duke::demo_sync::duke_map_lump(&demo);
        let grp = room_build::grp::Grp::parse(&g).unwrap();
        let w =
            room_build_engine::world::BuildWorld::load_map_bytes(grp.get(&name).unwrap()).unwrap();
        let mut h = std::collections::BTreeMap::new();
        for s in w.sprites.iter() {
            *h.entry(s.picnum).or_insert(0) += 1;
        }
        println!(
            "{name} skill={} sprites={} {:?}",
            demo.skill,
            w.sprites.len(),
            h
        );
        return;
    }
    if mode == "krand" {
        // duke_frontier krand DEMO1 <choc_krand.log> [tics]: compare per-call
        // krand seeds against a Chocolate log (seed before each call,
        // `# enterlevel` / `# endtic` markers), print the first mismatch.
        let choc_log = a.get(3).expect("choc krand log path");
        let tics: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(5000);
        let text = std::fs::read_to_string(choc_log).unwrap();
        let mut segs: Vec<Vec<(i32, String)>> = vec![Vec::new()];
        let mut started = false;
        for l in text.lines() {
            if l.starts_with("# enterlevel") {
                if started {
                    break;
                }
                started = true;
                continue;
            }
            if !started {
                continue;
            }
            if l.starts_with("# endtic") {
                segs.push(Vec::new());
                continue;
            }
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() >= 3 {
                let who = f.get(3).map_or(String::new(), |g| format!(" g_i={g}"));
                segs.last_mut()
                    .unwrap()
                    .push((f[2].parse().unwrap(), format!("{}{who}", f[1])));
            }
        }
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        room_build_engine::numeric::krand_log::start();
        let (mut s, demo) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        s.trace_fixups = false;
        for (t, inp) in demo.tics.iter().take(tics).enumerate() {
            s.tick(room_duke::player::DukeInput::from_dmo(inp));
            let room = room_build_engine::numeric::krand_log::drain();
            let choc = segs.get(t).cloned().unwrap_or_default();
            let n = room.len().max(choc.len());
            for k in 0..n {
                let r = room.get(k).map(|x| x.0);
                let c = choc.get(k).map(|x| x.0);
                if r != c {
                    println!(
                        "tic {t}: first krand mismatch at call {k} (room {} calls, choc {} calls)",
                        room.len(),
                        choc.len()
                    );
                    let from = if std::env::var_os("KRAND_ALL").is_some() {
                        0
                    } else {
                        k.saturating_sub(3)
                    };
                    for j in from..n.min(k + 12) {
                        let rr = room
                            .get(j)
                            .map_or("-".to_string(), |x| format!("{} {}", x.0, x.1));
                        let cc = choc
                            .get(j)
                            .map_or("-".to_string(), |x| format!("{} {}", x.0, x.1));
                        println!("  #{j:3} room {rr:<60} choc {cc}");
                    }
                    return;
                }
            }
        }
        println!("krand calls identical for {tics} tics");
        return;
    }
    if mode == "sprites" {
        // duke_frontier sprites DEMO1 i,j,k : pic / stat / pos after load.
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (s, _) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        let w = s.world.as_ref().unwrap();
        for id in a
            .get(3)
            .map_or("", String::as_str)
            .split(',')
            .filter_map(|x| x.parse::<usize>().ok())
        {
            let sp = &w.sprites[id];
            println!(
                "#{id} pic={} stat={} sect={} xy=({},{}) z={} lotag={} hitag={} cstat={} owner={}",
                sp.picnum,
                s.lists.sprite_stat.get(id).copied().unwrap_or(-1),
                sp.sectnum,
                sp.x,
                sp.y,
                sp.z,
                sp.lotag,
                sp.hitag,
                sp.cstat,
                sp.owner
            );
        }
        return;
    }
    if mode == "seed" {
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (s, _) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        let mut x: u32 = 17;
        let mut n = 0;
        while x as i32 != s.randomseed && n < 100_000 {
            x = x.wrapping_mul(27_584_621).wrapping_add(1);
            n += 1;
        }
        println!("pre-tic0 seed {} = 17 after {n} krand steps", s.randomseed);
        return;
    }
    let r = if mode == "legacy" {
        room_duke::demo_sync::demo1_frontier(&grp, &dmo, &tr, limit)
    } else {
        room_duke::demo_sync::demo1_frontier_real(&grp, &dmo, &tr, limit)
    };
    match r {
        Ok(None) => println!("{demo} {mode}: in sync for {limit} tics"),
        Ok(Some(d)) => println!("{demo} {mode}: first diff {d}"),
        Err(e) => println!("{demo} {mode}: error {e}"),
    }
}

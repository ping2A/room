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
    if mode == "start-map" {
        // duke_frontier start-map E1L1 [skill]: exercise a free-game GRP load
        // without a DMO, as the interactive native runtime will do.
        let map = a.get(2).map_or("E1L1", String::as_str);
        let skill = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
        let g = std::fs::read(&grp).expect("testdata/duke/duke3d.grp");
        match room_duke::session_from_grp_map(&g, map, skill) {
            Ok(s) => {
                let w = s.world.as_ref().expect("loaded map world");
                let live = s
                    .lists
                    .sprite_stat
                    .iter()
                    .filter(|&&stat| stat != room_build_engine::world::MAXSTATUS as i16)
                    .count();
                let start_sector = &w.sectors[s.player.cursectnum.max(0) as usize];
                println!(
                    "started {map} skill={skill} sectors={} walls={} sprites={live} player=({}, {}, {}) ang={} horiz={} sector={} floor={} ceiling={}",
                    w.numsectors,
                    w.numwalls,
                    s.player.posx,
                    s.player.posy,
                    s.player.posz,
                    s.player.ang,
                    s.player.horiz,
                    s.player.cursectnum,
                    start_sector.floorz,
                    start_sector.ceilingz
                );
            }
            Err(e) => {
                eprintln!("failed to start {map}: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if mode == "render-map" {
        // duke_frontier render-map E1L1 /tmp/duke.ppm [skill] [angle] [walk] [horiz] [animation-tic]:
        // render a native Build frame, optionally after walking / view override.
        let map = a.get(2).map_or("E1L1", String::as_str);
        let output = a.get(3).map_or("/tmp/room-duke.ppm", String::as_str);
        let skill = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(3);
        let angle = a.get(5).and_then(|s| s.parse::<i16>().ok());
        let walk_tics = a.get(6).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
        let horiz = a.get(7).and_then(|s| s.parse::<i16>().ok());
        let g = std::fs::read(&grp).expect("testdata/duke/duke3d.grp");
        let renderer = room_duke::DukeSoftRenderer::from_grp(&g).expect("load Duke ART/palette");
        let mut session = room_duke::session_from_grp_map(&g, map, skill).expect("load Duke map");
        for _ in 0..walk_tics {
            session.tick(room_duke::DukeInput {
                fvel: 1280,
                ..room_duke::DukeInput::default()
            });
        }
        let animation_tic = a
            .get(8)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(session.tic);
        let world = session.world.as_ref().expect("loaded map world");
        let (width, height) = (640_u32, 400_u32);
        let mut player = session.player.clone();
        if let Some(angle) = angle {
            player.ang = angle & 2047;
        }
        if let Some(horiz) = horiz {
            player.horiz = horiz;
        }
        let view = world.build_view(
            player.posx,
            player.posy,
            player.posz,
            player.ang,
            player.horiz,
            player.cursectnum,
        );
        let (sin, cos) = room_build_engine::helpers::approx_sincos_q14(i32::from(player.ang));
        let mut nearest = None::<i64>;
        for sector_id in &view.visible_sectors {
            let sec = &world.sectors[*sector_id as usize];
            for wall in &world.walls[sec.wallptr as usize..(sec.wallptr + sec.wallnum) as usize] {
                if wall.nextsector >= 0 || wall.point2 < 0 {
                    continue;
                }
                let end = &world.walls[wall.point2 as usize];
                let camera = |x: i32, y: i32| {
                    let dx = i64::from(x) - i64::from(player.posx);
                    let dy = i64::from(y) - i64::from(player.posy);
                    (
                        (dx * i64::from(cos) + dy * i64::from(sin)) >> 14,
                        (-dx * i64::from(sin) + dy * i64::from(cos)) >> 14,
                    )
                };
                let (x0, z0) = camera(wall.x, wall.y);
                let (x1, z1) = camera(end.x, end.y);
                if z0 <= 0 || z1 <= 0 || (x0 > 0 && x1 > 0) || (x0 < 0 && x1 < 0) || x0 == x1 {
                    continue;
                }
                let depth = z0 + ((z1 - z0) * -x0) / (x1 - x0);
                if depth > 0 && nearest.is_none_or(|n| depth < n) {
                    nearest = Some(depth);
                }
            }
        }
        let sec = &world.sectors[view.sect as usize];
        if let Some(depth) = nearest {
            let fy = i64::from(height / 2)
                - (i64::from(height / 2) * (i64::from(player.posz - sec.floorz) >> 4)) / depth
                - i64::from((player.horiz - 100) / 2);
            eprintln!(
                "camera: sector={} visible={} nearest_wall={depth} floor_y≈{fy} floor_pic={} ceil_pic={}",
                view.sect,
                view.visible_sectors.len(),
                sec.floorpicnum,
                sec.ceilingpicnum
            );
        }
        let rgba = renderer.render_rgba_at(world, &player, width, height, animation_tic);
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        ppm.reserve((width * height * 3) as usize);
        for pixel in rgba.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(output, ppm).expect("write PPM frame");
        println!(
            "rendered {map} skill={skill} walk={walk_tics} animation_tic={animation_tic} to {output} ({width}×{height})"
        );
        return;
    }
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
    if mode == "wall-anims" {
        // Report premap/animatewalls candidates so native D7 RNG calls can be
        // matched to concrete E1L1 wall records instead of guessed from a hash.
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let demo = room_build::DmoDemo::parse(&d).unwrap();
        let name = room_duke::demo_sync::duke_map_lump(&demo);
        let grp = room_build::grp::Grp::parse(&g).unwrap();
        let w =
            room_build_engine::world::BuildWorld::load_map_bytes(grp.get(&name).unwrap()).unwrap();
        let screenbreaks = [
            263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 4120, 4123, 4125,
            4127, 4128, 4129,
        ];
        let mut count = 0;
        for (i, wall) in w.walls.iter().enumerate() {
            let screen = screenbreaks.contains(&wall.picnum);
            let forcefield = (663..=665).contains(&wall.overpicnum) || wall.overpicnum == 230;
            if screen || forcefield {
                count += 1;
                println!(
                    "wall={i} pic={} overpic={} cstat={} shade={} extra={} branch={}",
                    wall.picnum,
                    wall.overpicnum,
                    wall.cstat,
                    wall.shade,
                    wall.extra,
                    if screen { "screenbreak" } else { "forcefield" }
                );
            }
        }
        println!("{name}: animatewalls candidates={count}");
        return;
    }
    if mode == "states" {
        // duke_frontier states DEMO1 [tics] [trace] [zero-input-tic]: compare
        // against a reference; optional zero-input tic isolates side effects.
        let n: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(8);
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let trace = a.get(4).map_or_else(|| tr.clone(), PathBuf::from);
        let zero_input_tic = a.get(5).and_then(|s| s.parse::<usize>().ok());
        let choc = room_duke::load_trace(&trace).expect("reference trace");
        let (mut s, demo) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        s.trace_fixups = false;
        for (tic, inp) in demo.tics.iter().take(n).enumerate() {
            let input = if zero_input_tic == Some(tic) {
                room_duke::DukeInput::default()
            } else {
                room_duke::DukeInput::from_dmo(inp)
            };
            s.tick(input);
            let r = s.emit_trace_tic();
            let Some(c) = choc.tics.get(tic) else {
                break;
            };
            let rp = r.players.first();
            let cp = c.players.first();
            let pose_eq = match (rp, cp) {
                (Some(rp), Some(cp)) => {
                    (
                        rp.posx,
                        rp.posy,
                        rp.posz,
                        rp.ang,
                        rp.horiz,
                        rp.extra,
                        rp.curr_weapon,
                        &rp.ammo,
                    ) == (
                        cp.posx,
                        cp.posy,
                        cp.posz,
                        cp.ang,
                        cp.horiz,
                        cp.extra,
                        cp.curr_weapon,
                        &cp.ammo,
                    )
                }
                _ => false,
            };
            println!(
                "tic {tic}: seed {} / {} global {} / {} sprites {} / {} player_eq={} hash_eq={}",
                c.randomseed,
                r.randomseed,
                c.global_random,
                r.global_random,
                c.numsprites,
                r.numsprites,
                pose_eq,
                c.sprite_hash == r.sprite_hash,
            );
        }
        return;
    }
    if mode == "snapshot" {
        // duke_frontier snapshot DEMO1 <tic>: emit live sprites at a trace tic.
        let want: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (mut s, demo) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        s.trace_fixups = false;
        for inp in demo.tics.iter().take(want + 1) {
            s.tick(room_duke::DukeInput::from_dmo(inp));
        }
        let world = s.world.as_ref().expect("loaded Duke map");
        let mut count = 0;
        for (i, stat) in s.lists.sprite_stat.iter().copied().enumerate() {
            if stat == room_build_engine::world::MAXSTATUS as i16 {
                continue;
            }
            let sp = &world.sprites[i];
            let sector = world
                .sectors
                .get(sp.sectnum.max(0) as usize)
                .copied()
                .unwrap_or_default();
            println!(
                "{i} {} {stat} {} {} {} sect={} lotag={} floor={} ceil={} ang={} xvel={} zvel={} hitag={} owner={} hit_picnum={} con_t=[{},{},{},{}] con_action={} con_actioncount={} con_move={} hit_floor={} hit_ceil={}",
                sp.picnum,
                sp.x,
                sp.y,
                sp.z,
                sp.sectnum,
                sector.lotag,
                sector.floorz,
                sector.ceilingz,
                sp.ang,
                sp.xvel,
                sp.zvel,
                sp.hitag,
                sp.owner,
                s.actor_temps.get(i).map_or(0, |t| t.hit_picnum),
                s.actor_temps.get(i).map_or(0, |t| t.g_t[0]),
                s.actor_temps.get(i).map_or(0, |t| t.g_t[1]),
                s.actor_temps.get(i).map_or(0, |t| t.g_t[2]),
                s.actor_temps.get(i).map_or(0, |t| t.g_t[3]),
                s.actor_temps.get(i).map_or(0, |t| t.g_t[4]),
                s.actor_temps.get(i).map_or(0, |t| t.g_t[2]),
                s.actor_temps.get(i).map_or(0, |t| t.g_t[1]),
                s.actor_temps.get(i).map_or(0, |t| t.hit_floorz),
                s.actor_temps.get(i).map_or(0, |t| t.hit_ceilingz),
            );
            count += 1;
        }
        eprintln!("numsprites={count}");
        return;
    }
    if mode == "actorinfo" {
        // duke_frontier actorinfo DEMO1 <sprite-index>: inspect raw and CON
        // initialization state for a map actor.
        let index: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(160);
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (s, _) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        let script = s.script.as_ref().expect("compiled GAME.CON");
        if let Ok(grp) = room_build::Grp::parse(&g)
            && let Some(map) = grp.get("E3L11.MAP")
            && let Ok(raw_world) = room_build_engine::BuildWorld::load_map_bytes(map)
        {
            if let Some(sprite) = raw_world.sprites.get(index) {
                println!(
                    "raw-map-sprite {index}: pic={} owner={} stat={}",
                    sprite.picnum, sprite.owner, raw_world.lists.sprite_stat[index]
                );
            }
        }
        if let Some(sprite) = s.world.as_ref().and_then(|w| w.sprites.get(index)) {
            println!(
                "initialized-sprite {index}: pic={} stat={} hit_picnum={} actor-entry={} actor-type={} hitag={}",
                sprite.picnum,
                s.lists.sprite_stat[index],
                s.actor_temps.get(index).map_or(0, |t| t.hit_picnum),
                script
                    .actorscrptr
                    .get(sprite.picnum.max(0) as usize)
                    .copied()
                    .unwrap_or(0),
                script
                    .actortype
                    .get(sprite.picnum.max(0) as usize)
                    .copied()
                    .unwrap_or(0),
                room_duke::con::actor_header_hitag(script, sprite.picnum)
            );
        }
        return;
    }
    if mode == "conwords" {
        // duke_frontier conwords DEMO1 <start> <end>: dump compiled CON words.
        let start: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(4790);
        let end: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(4890);
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (s, _) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        let script = s.script.as_ref().expect("compiled GAME.CON");
        for i in start..end.min(script.script.len()) {
            println!("{i:5}: {}", script.script[i]);
        }
        return;
    }
    if mode == "conlabels" {
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (s, _) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        let script = s.script.as_ref().expect("compiled GAME.CON");
        for name in [
            "PTREDWATER",
            "PSWIMMING",
            "PSWIMMINGGO",
            "PSWIMMINGWAIT",
            "BUBMOVE",
            "BUBMOVEFAST",
        ] {
            println!("{name}={}", script.labels.get(name).map_or(0, |l| l.value));
        }
        println!("WATERBUBBLE_ENTRY={}", script.actorscrptr[661]);
        return;
    }
    if mode == "krand-map" {
        // krand-map DEMO1 <choc_krand.log> [calls]: map Room calls to the
        // reference tic/caller labels.
        let choc_log = a.get(3).map_or_else(|| tr.clone(), PathBuf::from);
        let limit: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(80);
        let text = std::fs::read_to_string(&choc_log).unwrap();
        let mut choc_calls = Vec::new();
        let mut started = false;
        for line in text.lines() {
            if line.starts_with("# enterlevel") {
                if started {
                    break;
                }
                started = true;
                continue;
            }
            if !started || line.starts_with('#') {
                continue;
            }
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() >= 3 {
                if let Ok(seed) = f[2].parse::<i32>() {
                    let actor = f.get(3).copied().unwrap_or("?");
                    choc_calls.push((seed, format!("ref_tic={} {} actor={actor}", f[0], f[1])));
                }
            }
        }
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        room_build_engine::numeric::krand_log::start();
        let (mut s, demo) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        s.trace_fixups = false;
        let mut call = 0;
        for (tic, inp) in demo.tics.iter().enumerate() {
            if call >= limit {
                break;
            }
            s.tick(room_duke::DukeInput::from_dmo(inp));
            for (seed, site) in room_build_engine::numeric::krand_log::drain() {
                let choc = choc_calls.get(call).map_or_else(
                    || "<end of reference>".to_string(),
                    |c| format!("{} {}", c.0, c.1),
                );
                println!("#{call:4} tick={tic} room={seed} {site} | choc={choc}");
                call += 1;
                if call >= limit {
                    break;
                }
            }
        }
        return;
    }
    if mode == "krand" {
        // duke_frontier krand DEMO1 <choc_krand.log> [tics]: compare per-call
        // krand seeds and end-of-tic seeds against Chocolate. The second check
        // catches missing/extra calls even when the remaining call stream lines up.
        let choc_log = a.get(3).expect("choc krand log path");
        let tics: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(5000);
        let text = std::fs::read_to_string(choc_log).unwrap();
        let mut choc_calls: Vec<(i32, String)> = Vec::new();
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
            if l.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() >= 3 {
                let who = f.get(3).map_or(String::new(), |g| format!(" g_i={g}"));
                if let Ok(seed) = f[2].parse() {
                    choc_calls.push((seed, format!("{} ref_tic={}{}", f[1], f[0], who)));
                }
            }
        }
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let choc_trace = room_duke::load_trace(&tr).expect("reference state trace");
        room_build_engine::numeric::krand_log::start();
        let (mut s, demo) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        s.trace_fixups = false;
        let mut choc_cursor = 0;
        for (t, inp) in demo.tics.iter().take(tics).enumerate() {
            if std::env::var_os("ROOM_FTA_DEBUG").is_some() {
                eprintln!("== tic {t}");
            }
            s.tick(room_duke::player::DukeInput::from_dmo(inp));
            let room = room_build_engine::numeric::krand_log::drain();
            let choc = &choc_calls[choc_cursor.min(choc_calls.len())..];
            let n = room.len();
            for k in 0..n {
                let r = room.get(k).map(|x| x.0);
                let c = choc.get(k).map(|x| x.0);
                if r != c {
                    println!(
                        "tic {t}: first krand mismatch at stream call {} (tic calls: room {}, choc remaining {})",
                        choc_cursor + k,
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
            if let Some(expected) = choc_trace.tics.get(t)
                && s.randomseed != expected.randomseed
            {
                println!(
                    "tic {t}: end-of-tic randomseed mismatch after {} logged calls: room {} / choc {}",
                    room.len(),
                    s.randomseed,
                    expected.randomseed
                );
                for (j, (seed, site)) in room.iter().enumerate() {
                    let c = choc.get(j).map_or_else(
                        || "<end of reference tic>".to_string(),
                        |(seed, site)| format!("{seed} {site}"),
                    );
                    println!("  #{j:3} room {seed} {site:<60} choc {c}");
                }
                if let Some((seed, site)) = choc.get(room.len()) {
                    println!("  missing choc {seed} {site}");
                }
                return;
            }
            choc_cursor += room.len();
        }
        println!("krand calls identical for {tics} tics");
        return;
    }
    if mode == "inputs" {
        // duke_frontier inputs DEMO1 <from> <to>: the recorded per-tic input.
        let from: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let to: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(from);
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (_, demo) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        for (tic, inp) in demo.tics.iter().enumerate().take(to + 1).skip(from) {
            println!(
                "{tic} turn={} horizon={} forward={} strafe={} bits={:#010x}",
                inp.turn, inp.horizon, inp.forward, inp.strafe, inp.bits
            );
        }
        return;
    }
    if mode == "player" {
        // duke_frontier player DEMO1 <from> <to>: seat-0 state after each tic.
        let from: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let to: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(from);
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (mut s, demo) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        s.trace_fixups = false;
        for (tic, inp) in demo.tics.iter().enumerate().take(to + 1) {
            s.tick(room_duke::DukeInput::from_dmo(inp));
            if tic >= from {
                let p = &s.player;
                println!(
                    "{tic} extra={} boots={} horiz={} pos=({},{},{}) sect={} on_ground={} aim={} weapon={} kickback={}",
                    p.extra,
                    p.boot_amount,
                    p.horiz,
                    p.posx,
                    p.posy,
                    p.posz,
                    p.cursectnum,
                    p.on_ground,
                    p.aim_mode,
                    p.curr_weapon,
                    p.kickback_pic
                );
            }
        }
        return;
    }
    if mode == "dump" {
        // duke_frontier dump DEMO1 <tic> <out>: oracle-format sprite dump
        // (`i pic stat x y z` + extras) after `tic` (oracle's dump tic).
        let n: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let out = a.get(4).map_or("/tmp/room_dump.txt", String::as_str);
        let g = std::fs::read(&grp).unwrap();
        let d = std::fs::read(&dmo).unwrap();
        let (mut s, demo) = room_duke::demo_sync::session_from_grp_demo(&g, &d).unwrap();
        s.trace_fixups = false;
        for inp in demo.tics.iter().take(n + 1) {
            s.tick(room_duke::DukeInput::from_dmo(inp));
        }
        let w = s.world.as_ref().unwrap();
        let mut txt = String::new();
        for (i, sp) in w.sprites.iter().enumerate() {
            let st = s.lists.sprite_stat.get(i).copied().unwrap_or(1024);
            if st == 1024 {
                continue;
            }
            let t = s.actor_temps.get(i).map(|t| t.g_t).unwrap_or_default();
            txt.push_str(&format!(
                "{i} {} {st} {} {} {} sect={} ang={} xv={} zv={} ext={} own={} xr={} yr={} cs={} t={:?}\n",
                sp.picnum, sp.x, sp.y, sp.z, sp.sectnum, sp.ang, sp.xvel, sp.zvel, sp.extra, sp.owner,
                sp.xrepeat, sp.yrepeat, sp.cstat, t
            ));
        }
        std::fs::write(out, txt).unwrap();
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

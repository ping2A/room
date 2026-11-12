//! Render a native Duke view headless to a binary PPM.
//! `cargo run --release -p room-duke --example duke_shot -- E1L1 out.ppm [tics] [w h]`
use std::path::PathBuf;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let map = a.get(1).map_or("E1L1", String::as_str);
    let out = a.get(2).map_or("/tmp/duke.ppm", String::as_str);
    let tics: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let w: u32 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(640);
    let h: u32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(400);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/duke");
    let grp = std::fs::read(root.join("duke3d.grp")).unwrap();
    let mut s = room_duke::session_from_grp_map(&grp, &format!("{map}.MAP"), 2).unwrap();
    for _ in 0..tics { s.tick(room_duke::DukeInput::default()); }
    if std::env::var("ROOM_NEAR").is_ok() { let w = s.world.as_ref().unwrap(); for (i, se) in w.sectors[..w.numsectors as usize].iter().enumerate() { let wl = &w.walls[se.wallptr as usize]; let d = ((wl.x - s.player.posx) as i64).abs() + ((wl.y - s.player.posy) as i64).abs(); if d < 6000 { eprintln!("near sect {i} d={d} floor={} fpic={} fstat={} fheinum={} ceil={} cpic={} cstat={}", se.floorz, se.floorpicnum, se.floorstat, se.floorheinum, se.ceilingz, se.ceilingpicnum, se.ceilingstat); } } }
    if let Ok(a) = std::env::var("ROOM_ANG") { s.player.ang = a.parse().unwrap(); }
    if let Ok(p) = std::env::var("ROOM_POS") { let v: Vec<i32> = p.split(',').map(|x| x.parse().unwrap()).collect(); s.player.posx = v[0]; s.player.posy = v[1]; if v.len() > 2 { s.player.horiz = v[2] as i16; } let w = s.world.as_ref().unwrap(); let mut sect = s.player.cursectnum; room_build_engine::geom::updatesector(s.player.posx, s.player.posy, &mut sect, &w.sector_walls(), &w.wall_xy()); if sect >= 0 { s.player.cursectnum = sect; s.player.posz = w.sectors[sect as usize].floorz - (40 << 8); } }
    if let Ok(r) = std::env::var("ROOM_SPRITES") { let r: i64 = r.parse().unwrap(); let w = s.world.as_ref().unwrap(); for (i, sp) in w.sprites.iter().enumerate() { let st = w.lists.sprite_stat[i]; if st >= 1024 || sp.sectnum < 0 { continue; } let d = ((sp.x - s.player.posx) as i64).abs() + ((sp.y - s.player.posy) as i64).abs(); if d < r { let t = s.actor_temps.get(i).map(|t| t.g_t).unwrap_or_default(); eprintln!("spr {i} pic={} stat={st}{} g_t={t:?} sect={} d={d} xy=({},{}) z={} lotag={} hitag={} cstat={} xr={} extra={} owner={}", sp.picnum, "", sp.sectnum, sp.x, sp.y, sp.z, sp.lotag, sp.hitag, sp.cstat, sp.xrepeat, sp.extra, sp.owner); } } }
    if let Ok(ids) = std::env::var("ROOM_ADJ") { let w = s.world.as_ref().unwrap(); for id in ids.split(',') { let i: usize = id.parse().unwrap(); let se = &w.sectors[i]; let mut n = Vec::new(); for wi in se.wallptr as usize..(se.wallptr + se.wallnum) as usize { let wl = &w.walls[wi]; n.push(format!("w{wi}({},{})->{}{} pic{} over{} cs{} sh{}", wl.x, wl.y, wl.nextsector, if wl.cstat & 16 != 0 {"m"} else {""}, wl.picnum, wl.overpicnum, wl.cstat, wl.shade)); } eprintln!("sect {i} lotag={} hitag={} c={} f={} cpic={} fpic={} cstat={} fstat={} next=[{}]", se.lotag, se.hitag, se.ceilingz, se.floorz, se.ceilingpicnum, se.floorpicnum, se.ceilingstat, se.floorstat, n.join(",")); } }
    if let Ok(ids) = std::env::var("ROOM_SECT") { let w = s.world.as_ref().unwrap(); for id in ids.split(',') { let i: usize = id.parse().unwrap(); let se = &w.sectors[i]; eprintln!("sect {i} ceil={} floor={} cstat={} fstat={} cpic={} fpic={} cheinum={} fheinum={} walls={}+{}", se.ceilingz, se.floorz, se.ceilingstat, se.floorstat, se.ceilingpicnum, se.floorpicnum, se.ceilingheinum, se.floorheinum, se.wallptr, se.wallnum); } }
    let r = room_duke::DukeSoftRenderer::from_grp(&grp).unwrap();
    if let Ok(t) = std::env::var("ROOM_TILE") { let pic: i16 = t.parse().unwrap(); font_sheet(&r, "A", |_| Some(pic), "/tmp/tile.ppm"); eprintln!("tile {pic} dims {:?}", r.tile_indices(pic).map(|t| (t.0, t.1))); }
    if std::env::var("ROOM_FONTS").is_ok() {
        font_sheet(&r, "NEW GAME: SAVE 0123456789 - LOAD!?", |c| { let c = c.to_ascii_uppercase(); match c { b'A'..=b'Z' => Some(2940 + i16::from(c - b'A')), b'0'..=b'9' => Some(2930 + i16::from(c - b'0')), b'-' => Some(2929), b'!' => Some(3004), b'?' => Some(3005), b':' => Some(3006), _ => None } }, "/tmp/big.ppm");
        font_sheet(&r, "Picked up a pistol! 123 Health: 100%", |c| (b'!'..=b'~').contains(&c).then(|| 2822 + i16::from(c - b'!')), "/tmp/small.ppm");
    }
    let t0 = std::time::Instant::now();
    let px = r.render_rgba(&s.render_world().unwrap(), &s.player, w, h);
    eprintln!("render {:?}", t0.elapsed());
    let mut f = format!("P6\n{w} {h}\n255\n").into_bytes();
    for c in px.chunks(4) { f.extend_from_slice(&c[..3]); }
    std::fs::write(out, f).unwrap();
    eprintln!("player pos=({},{},{}) ang={} sect={}", s.player.posx, s.player.posy, s.player.posz, s.player.ang, s.player.cursectnum);
}

#[allow(dead_code)]
fn font_sheet(r: &room_duke::DukeSoftRenderer, text: &str, base_of: impl Fn(u8) -> Option<i16>, out: &str) {
    let pal = r.ui_palette_rgb();
    let glyphs: Vec<_> = text.bytes().map(|c| base_of(c).and_then(|p| r.tile_indices(p))).collect();
    let h = glyphs.iter().flatten().map(|g| g.1).max().unwrap_or(1) as usize;
    let w: usize = glyphs.iter().map(|g| g.as_ref().map_or(6, |g| g.0 as usize + 1)).sum();
    let mut img = vec![40u8; w * h * 3];
    let mut x0 = 0;
    for g in &glyphs {
        let Some((gw, gh, idx)) = g else { x0 += 6; continue };
        for y in 0..*gh as usize { for x in 0..*gw as usize { let i = idx[y * *gw as usize + x]; if i != 255 { let o = (y * w + x0 + x) * 3; img[o..o + 3].copy_from_slice(&pal[i as usize]); } } }
        x0 += *gw as usize + 1;
    }
    let mut f = format!("P6\n{w} {h}\n255\n").into_bytes(); f.extend(img); std::fs::write(out, f).unwrap();
}

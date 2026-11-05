//! Write a small UDMF test arena PWAD (`MAP01`): one big lit room, the player
//! at the west end facing east, and a monster `ednum` 512 units ahead.
//! Usage: bd_arena_wad OUT.wad [ednum=3001] [count=1]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let out = a.get(1).cloned().unwrap_or_else(|| "arena.wad".into());
    let ednum: i32 = a.get(2).map_or(3001, |s| s.parse().unwrap());
    let count: i32 = a.get(3).map_or(1, |s| s.parse().unwrap());
    let mut b = room_testkit::MapBuilder::new()
        .room("a", room_testkit::rect(0.0, 0.0, 2048.0, 512.0), 0.0, 192.0)
        .sector_light("a", 200)
        .player_start(64.0, 256.0, 0);
    for i in 0..count {
        b = b.thing(
            ednum,
            900.0 + f64::from(i) * 48.0,
            (256.0 + 110.0 + f64::from(i % 3 - 1) * 96.0),
            180,
        );
    }
    let wad = b.build_udmf_wad("MAP01").unwrap();
    std::fs::write(&out, wad).unwrap();
    println!("wrote {out}");
}

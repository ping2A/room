//! Solo / co-op on generated maps: monsters spawn and the game runs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_nodebuild::ensure_nodes;
use room_sim::TicCmd;

fn solo(opts: &room_mapgen::Options, skill: u8) -> (Headless, room_mapgen::Generated) {
    let g = room_mapgen::generate(opts).unwrap();
    let mut map = g.map.to_geometry().unwrap();
    ensure_nodes(&mut map);
    let data = load_embedded_engine_data().unwrap();
    let skill = Skill::new(skill);
    let mut game = Headless::new(GameOptions {
        skill,
        coop_players: 1,
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);
    game.start_custom_map(map, skill).unwrap();
    (game, g)
}

#[test]
fn generated_solo_map_spawns_monsters_and_runs() {
    for (seed, biomes) in [
        (3, room_mapgen::Biomes::Mixed),
        (8, room_mapgen::Biomes::Caves),
    ] {
        let opts = room_mapgen::Options {
            seed,
            players: 4,
            monsters: 2,
            biomes,
            ..Default::default()
        };
        let (mut easy, _) = solo(&opts, 1);
        let (mut hard, g) = solo(&opts, 4);
        let idle = [TicCmd::default(); 1];
        easy.tick(&idle);
        hard.tick(&idle);
        let easy_n = easy.world().unwrap().level.total_kills;
        let hard_n = hard.world().unwrap().level.total_kills;
        assert!(easy_n >= 8, "seed {seed}: {easy_n} monsters on easy");
        assert!(
            hard_n > easy_n,
            "skill flags: hard {hard_n} vs easy {easy_n}"
        );
        assert!(hard_n as usize <= g.report.monsters);
        assert_eq!(
            hard.world().unwrap().level.total_secrets as usize,
            g.report.secrets,
            "engine counts the secret closets"
        );
        // Half a minute of the level running (monsters wake, roam, fight).
        for _ in 0..35 * 30 {
            hard.tick(&idle);
        }
        assert!(hard.world().is_some());
    }
}

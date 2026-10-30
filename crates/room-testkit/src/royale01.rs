//! Dedicated battle royale map (`docs/28` §8): ~5.4k×5.4k open grid, 18 DM starts.

use crate::{MapBuilder, rect, Rect};

const M: f64 = 256.0;
const CELL: f64 = 1920.0;
const ORIGIN: f64 = M;

fn cell(col: u32, row: u32) -> Rect {
    let x0 = ORIGIN + f64::from(col) * CELL;
    let y0 = ORIGIN + f64::from(row) * CELL;
    rect(x0, y0, x0 + CELL, y0 + CELL)
}

fn field(b: MapBuilder, name: &str, r: Rect, floor_tex: &str, ceil_tex: &str, light: i32) -> MapBuilder {
    b.room(name, r, 0.0, 288.0)
        .sector_textures(name, floor_tex, ceil_tex)
        .sector_light(name, light)
}

fn cover(b: MapBuilder, name: &str, r: Rect) -> MapBuilder {
    b.room(name, r, 32.0, 192.0)
        .sector_textures(name, "FLOOR7_1", "CEIL3_5")
        .sector_light(name, 176)
}

/// Large Freedoom-textured BR arena: 3×3 fields, cover pillars, ≥16 DM starts (type 11).
#[must_use]
pub fn royale01() -> MapBuilder {
    let mut b = MapBuilder::new().wall_texture("STARTAN3");

    // --- open fields (named sectors appear as UDMF comments) ---
    b = field(
        b,
        "sw_yard",
        cell(0, 0),
        "FLOOR4_8",
        "FLAT5_4",
        168,
    );
    b = field(
        b,
        "s_courtyard",
        cell(1, 0),
        "FLOOR0_1",
        "CEIL3_5",
        184,
    );
    b = field(
        b,
        "se_depot",
        cell(2, 0),
        "FLOOR5_1",
        "FLAT5_4",
        172,
    );
    b = field(
        b,
        "w_ridge",
        cell(0, 1),
        "FLOOR6_2",
        "CEIL3_5",
        192,
    );
    b = field(
        b,
        "center_plaza",
        cell(1, 1),
        "FLOOR4_8",
        "CEIL5_1",
        208,
    );
    b = field(
        b,
        "e_ridge",
        cell(2, 1),
        "FLOOR6_2",
        "CEIL3_5",
        192,
    );
    b = field(
        b,
        "nw_yard",
        cell(0, 2),
        "FLOOR4_8",
        "FLAT5_4",
        168,
    );
    b = field(
        b,
        "n_courtyard",
        cell(1, 2),
        "FLOOR0_1",
        "CEIL3_5",
        184,
    );
    b = field(
        b,
        "ne_depot",
        cell(2, 2),
        "FLOOR5_1",
        "FLAT5_4",
        172,
    );

    // --- low cover (32 u slabs) ---
    let pillar = |b: MapBuilder, name: &str, cx: f64, cy: f64| {
        cover(b, name, rect(cx - 192.0, cy - 192.0, cx + 192.0, cy + 192.0))
    };
    b = pillar(b, "cover_sw", ORIGIN + CELL * 0.5, ORIGIN + CELL * 0.5);
    b = pillar(b, "cover_se", ORIGIN + CELL * 2.5, ORIGIN + CELL * 0.5);
    b = pillar(b, "cover_nw", ORIGIN + CELL * 0.5, ORIGIN + CELL * 2.5);
    b = pillar(b, "cover_ne", ORIGIN + CELL * 2.5, ORIGIN + CELL * 2.5);
    b = pillar(b, "cover_center", ORIGIN + CELL * 1.5, ORIGIN + CELL * 1.5);
    b = pillar(b, "cover_mid_w", ORIGIN + CELL * 0.5, ORIGIN + CELL * 1.5);
    b = pillar(b, "cover_mid_e", ORIGIN + CELL * 2.5, ORIGIN + CELL * 1.5);
    b = pillar(b, "cover_mid_s", ORIGIN + CELL * 1.5, ORIGIN + CELL * 0.5);
    b = pillar(b, "cover_mid_n", ORIGIN + CELL * 1.5, ORIGIN + CELL * 2.5);

    let cx = ORIGIN + CELL * 1.5;
    let cy = ORIGIN + CELL * 1.5;
    b = b.player_start(cx, cy, 90);

    // 18 deathmatch starts (type 11) — corners, edges, ring.
    let dm: [(f64, f64); 18] = [
        (ORIGIN + 384.0, ORIGIN + 384.0),
        (ORIGIN + CELL * 3.0 - 384.0, ORIGIN + 384.0),
        (ORIGIN + 384.0, ORIGIN + CELL * 3.0 - 384.0),
        (ORIGIN + CELL * 3.0 - 384.0, ORIGIN + CELL * 3.0 - 384.0),
        (ORIGIN + CELL * 0.5, ORIGIN + CELL * 0.5),
        (ORIGIN + CELL * 2.5, ORIGIN + CELL * 0.5),
        (ORIGIN + CELL * 0.5, ORIGIN + CELL * 2.5),
        (ORIGIN + CELL * 2.5, ORIGIN + CELL * 2.5),
        (cx, ORIGIN + 512.0),
        (cx, ORIGIN + CELL * 3.0 - 512.0),
        (ORIGIN + 512.0, cy),
        (ORIGIN + CELL * 3.0 - 512.0, cy),
        (ORIGIN + CELL, cy),
        (ORIGIN + CELL * 2.0, cy),
        (cx, ORIGIN + CELL),
        (cx, ORIGIN + CELL * 2.0),
        (ORIGIN + CELL * 1.25, ORIGIN + CELL * 1.25),
        (ORIGIN + CELL * 1.75, ORIGIN + CELL * 1.75),
    ];
    for (x, y) in dm {
        b = b.thing(11, x, y, 0);
    }

    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use room_nodebuild::apply_nodes;

    #[test]
    fn royale01_has_enough_dm_starts_and_nodes() {
        let mut map = royale01().build_udmf("ROYALE01").expect("udmf");
        let dm = map.things.iter().filter(|t| t.type_id == 11).count();
        assert!(dm >= 16, "dm starts: {dm}");
        apply_nodes(&mut map);
        assert!(map.game_nodes.is_some());
    }
}

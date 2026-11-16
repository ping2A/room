//! Visual themes: wall / flat / decoration palettes per room style.
//!
//! Every slot lists candidates in order of preference (Doom II / Freedoom 2
//! names first). [`Palette::resolve`] keeps the first one the loaded game
//! has, so a map built for another IWAD still gets valid textures.

use serde::{Deserialize, Serialize};

/// Overall look of a generated map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// UAC base: grey panels, computers, tech lights.
    #[default]
    Tech,
    /// Brown industrial base: bronze trims, slime.
    Base,
    /// Marble and blood: hell keep, lava pits, torches.
    Hell,
    /// Stone fortress: grey bricks, water, torches.
    Castle,
    /// Ruins under the sky: rock, grass, ponds, trees.
    Ruins,
    /// Districts of several themes, blended by area.
    Mixed,
}

impl Theme {
    /// Concrete themes used by [`Theme::Mixed`].
    pub const CONCRETE: [Theme; 5] = [
        Theme::Tech,
        Theme::Base,
        Theme::Hell,
        Theme::Castle,
        Theme::Ruins,
    ];
}

/// Texture or flat namespace (for availability checks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Wall texture (`TEXTURE1/2`).
    Texture,
    /// Floor / ceiling flat.
    Flat,
}

/// Candidate lists for one theme.
struct Spec {
    wall: &'static [&'static str],
    trim: &'static [&'static str],
    pillar: &'static [&'static str],
    floor: &'static [&'static str],
    floor_alt: &'static [&'static str],
    ceil: &'static [&'static str],
    light: &'static [&'static str],
    step: &'static [&'static str],
    skirt: &'static [&'static str],
    crate_wall: &'static [&'static str],
    crate_top: &'static [&'static str],
    switch: &'static [&'static str],
    liquid: &'static [&'static str],
    liquid_damage: i16,
    sky_wall: &'static [&'static str],
    sky_floor: &'static [&'static str],
    lamp: i16,
    small_decor: i16,
}

const OUT_WALL: &[&str] = &["ROCK2", "ASHWALL2", "STONE2", "BROWNHUG", "STARTAN3"];
const OUT_FLOOR: &[&str] = &[
    "RROCK19", "GRASS1", "RROCK09", "FLAT10", "FLOOR6_2", "FLOOR0_1",
];

fn spec(theme: Theme) -> Spec {
    match theme {
        Theme::Tech | Theme::Mixed => Spec {
            wall: &["STARTAN3", "STARG3", "STARGR1", "STARTAN2"],
            trim: &["STARGR1", "STARG2", "STARTAN3"],
            pillar: &["SUPPORT2", "METAL", "COMPTALL", "STARGR1"],
            floor: &["FLOOR0_1", "FLOOR0_3", "FLOOR4_8"],
            floor_alt: &["FLOOR0_6", "FLAT14", "FLOOR4_8"],
            ceil: &["CEIL3_5", "CEIL3_3", "FLAT18", "CEIL1_1"],
            light: &["TLITE6_5", "TLITE6_4", "CEIL3_5"],
            step: &["STEP1", "STEP2", "STARGR1"],
            skirt: &["STEP6", "SUPPORT2", "STARGR1"],
            crate_wall: &["CRATE1", "CRATE2", "STARGR1"],
            crate_top: &["CRATOP2", "CRATOP1", "FLOOR4_8"],
            switch: &["SW1STRTN", "SW1COMP", "SW1STARG"],
            liquid: &["NUKAGE1", "SLIME01", "FWATER1"],
            liquid_damage: 7,
            sky_wall: OUT_WALL,
            sky_floor: OUT_FLOOR,
            lamp: 85,
            small_decor: 2035,
        },
        Theme::Base => Spec {
            wall: &["BROWN1", "BROWNGRN", "BROWN96", "STARTAN3"],
            trim: &["BRONZE1", "BROWN144", "BROWN1"],
            pillar: &["SUPPORT3", "BRONZE1", "METAL"],
            floor: &["FLOOR5_1", "FLOOR7_1", "FLAT5_4"],
            floor_alt: &["FLOOR5_2", "FLAT5_4", "FLOOR4_8"],
            ceil: &["CEIL5_1", "CEIL5_2", "CEIL3_5"],
            light: &["TLITE6_6", "TLITE6_1", "TLITE6_5"],
            step: &["STEP4", "STEP3", "STEP1"],
            skirt: &["BRONZE1", "STEP4", "BROWN1"],
            crate_wall: &["CRATE2", "CRATE1", "BROWN1"],
            crate_top: &["CRATOP1", "CRATOP2", "FLOOR5_1"],
            switch: &["SW1BRN1", "SW1BRNGN", "SW1STRTN"],
            liquid: &["SLIME01", "NUKAGE1", "FWATER1"],
            liquid_damage: 7,
            sky_wall: OUT_WALL,
            sky_floor: OUT_FLOOR,
            lamp: 2028,
            small_decor: 2035,
        },
        Theme::Hell => Spec {
            wall: &["MARBLE1", "MARBLE2", "MARBGRAY", "STONE2"],
            trim: &["SP_HOT1", "MARBFAC2", "MARBLE3"],
            pillar: &["MARBGRAY", "GSTONE1", "STONE3"],
            floor: &["MFLR8_1", "FLAT5_7", "FLOOR6_2", "FLOOR0_1"],
            floor_alt: &["FLOOR6_1", "FLAT1_2", "FLOOR7_2"],
            ceil: &["CEIL5_2", "FLOOR6_2", "CEIL3_5"],
            light: &["TLITE6_6", "FLAT17", "TLITE6_5"],
            step: &["STEP6", "MARBGRAY", "STEP1"],
            skirt: &["MARBGRAY", "STONE3", "STEP6"],
            crate_wall: &["GSTONE1", "MARBGRAY", "STONE2"],
            crate_top: &["FLOOR7_2", "MFLR8_1"],
            switch: &["SW1MARB", "SW1GSTON", "SW1STRTN"],
            liquid: &["LAVA1", "NUKAGE1"],
            liquid_damage: 5,
            sky_wall: &["ASHWALL2", "ROCK2", "SP_ROCK1", "STONE2"],
            sky_floor: &["RROCK09", "FLAT5_7", "FLOOR6_2", "FLOOR0_1"],
            lamp: 46,
            small_decor: 70,
        },
        Theme::Castle => Spec {
            wall: &["STONE2", "STONE3", "GSTONE1", "STARTAN3"],
            trim: &["STONE", "STONE3", "GSTONE1"],
            pillar: &["STONE", "GSTVINE1", "STONE3"],
            floor: &["FLAT1", "FLOOR6_2", "FLAT5_4", "FLOOR0_1"],
            floor_alt: &["FLAT5_4", "FLOOR6_2", "FLAT1"],
            ceil: &["CEIL1_1", "FLAT1", "CEIL3_5"],
            light: &["CEIL1_3", "TLITE6_6", "TLITE6_5"],
            step: &["STEP6", "STONE3", "STEP1"],
            skirt: &["STONE3", "STONE", "STEP6"],
            crate_wall: &["STONE", "STONE3"],
            crate_top: &["FLAT1", "FLOOR6_2"],
            switch: &["SW1STON1", "SW1GSTON", "SW1STRTN"],
            liquid: &["FWATER1", "NUKAGE1"],
            liquid_damage: 0,
            sky_wall: &["STONE2", "ROCK2", "ASHWALL2"],
            sky_floor: &["GRASS1", "FLAT10", "RROCK19", "FLOOR6_2"],
            lamp: 44,
            small_decor: 45,
        },
        Theme::Ruins => Spec {
            wall: &["ROCK2", "ASHWALL2", "BROWNHUG", "STONE2"],
            trim: &["STONE3", "ROCK2", "BROWNHUG"],
            pillar: &["STONE", "ROCK2", "STONE3"],
            floor: &["RROCK19", "GRASS1", "RROCK09", "FLOOR6_2"],
            floor_alt: &["FLAT10", "RROCK09", "FLOOR6_2"],
            ceil: &["RROCK09", "CEIL1_1", "CEIL3_5"],
            light: &["TLITE6_6", "TLITE6_5"],
            step: &["STEP6", "STONE3", "STEP1"],
            skirt: &["STONE3", "ROCK2"],
            crate_wall: &["ROCK2", "STONE3"],
            crate_top: &["RROCK09", "FLOOR6_2"],
            switch: &["SW1STON1", "SW1STRTN"],
            liquid: &["FWATER1", "SLIME01", "NUKAGE1"],
            liquid_damage: 0,
            sky_wall: OUT_WALL,
            sky_floor: OUT_FLOOR,
            lamp: 54,
            small_decor: 47,
        },
    }
}

/// Biome materials per theme (caves, wood, buildings, paving, marsh).
struct BiomeSpec {
    cave_wall: &'static [&'static str],
    cave_floor: &'static [&'static str],
    cave_ceil: &'static [&'static str],
    wood: &'static [&'static str],
    building: &'static [&'static str],
    building_alt: &'static [&'static str],
    paving: &'static [&'static str],
    swamp_liquid: &'static [&'static str],
}

fn biome_spec(theme: Theme) -> BiomeSpec {
    const WOOD: &[&str] = &["FLAT5_2", "FLAT5_1", "CEIL1_1", "FLOOR0_1"];
    match theme {
        Theme::Tech | Theme::Mixed => BiomeSpec {
            cave_wall: &["ROCK2", "ASHWALL2", "SP_ROCK1", "STONE2"],
            cave_floor: &["RROCK09", "RROCK19", "FLAT10", "FLOOR6_2"],
            cave_ceil: &["RROCK10", "RROCK09", "FLAT10", "CEIL3_5"],
            wood: WOOD,
            building: &["BRICK10", "BIGBRIK2", "BRICK12", "STONE2"],
            building_alt: &["BRICK11", "STUCCO", "BRICK7", "STARTAN3"],
            paving: &["FLAT1", "FLAT5_4", "FLOOR0_1"],
            swamp_liquid: &["NUKAGE1", "FWATER1", "SLIME01"],
        },
        Theme::Base => BiomeSpec {
            cave_wall: &["ROCK4", "ROCK2", "ASHWALL2", "BROWN1"],
            cave_floor: &["RROCK19", "FLAT10", "FLOOR6_2", "FLOOR5_1"],
            cave_ceil: &["RROCK10", "FLAT10", "CEIL5_1"],
            wood: WOOD,
            building: &["BRICK7", "BRICK6", "BROWN1"],
            building_alt: &["BRICK5", "BROWN96", "BRONZE1"],
            paving: &["FLAT5_4", "FLOOR5_1", "FLAT1"],
            swamp_liquid: &["SLIME01", "FWATER1", "NUKAGE1"],
        },
        Theme::Hell => BiomeSpec {
            cave_wall: &["SP_ROCK1", "ROCK5", "ASHWALL2", "MARBLE1"],
            cave_floor: &["RROCK05", "RROCK09", "FLOOR6_1", "MFLR8_1"],
            cave_ceil: &["RROCK03", "RROCK09", "FLOOR6_2", "CEIL5_2"],
            wood: &["CEIL1_1", "FLAT5_2", "FLOOR7_2"],
            building: &["MARBLE1", "SP_HOT1", "GSTONE1"],
            building_alt: &["MARBGRAY", "MARBLE2", "STONE3"],
            paving: &["MFLR8_1", "FLAT5_7", "FLOOR7_2"],
            swamp_liquid: &["BLOOD1", "LAVA1", "NUKAGE1"],
        },
        Theme::Castle => BiomeSpec {
            cave_wall: &["ROCK2", "STONE3", "ASHWALL2"],
            cave_floor: &["FLAT10", "RROCK19", "FLOOR6_2"],
            cave_ceil: &["FLAT10", "RROCK10", "CEIL1_1"],
            wood: WOOD,
            building: &["BIGBRIK1", "STONE2", "STONE3"],
            building_alt: &["STONE3", "GSTONE1", "BIGBRIK2"],
            paving: &["FLAT1", "FLAT5_4", "FLOOR6_2"],
            swamp_liquid: &["FWATER1", "SLIME01", "NUKAGE1"],
        },
        Theme::Ruins => BiomeSpec {
            cave_wall: &["ROCK3", "ROCK2", "ASHWALL2", "STONE2"],
            cave_floor: &["RROCK19", "RROCK09", "GRASS1", "FLAT10"],
            cave_ceil: &["RROCK10", "RROCK09", "FLAT10"],
            wood: WOOD,
            building: &["BIGBRIK3", "BRICK8", "STONE3"],
            building_alt: &["BIGBRIK1", "STONE2", "ROCK2"],
            paving: &["FLAT1", "FLAT10", "RROCK19"],
            swamp_liquid: &["FWATER1", "SLIME01", "NUKAGE1"],
        },
    }
}

/// A theme with every slot resolved to a name the game has.
#[derive(Clone, Debug)]
pub struct Palette {
    /// Theme this palette was built from.
    pub theme: Theme,
    /// Main wall.
    pub wall: String,
    /// Corridor / accent wall.
    pub trim: String,
    /// Pillars and columns.
    pub pillar: String,
    /// Main floor.
    pub floor: String,
    /// Corridors and raised areas.
    pub floor_alt: String,
    /// Ceiling.
    pub ceil: String,
    /// Ceiling light panels.
    pub light: String,
    /// Stair risers.
    pub step: String,
    /// Sides of raised platforms.
    pub skirt: String,
    /// Crate sides.
    pub crate_wall: String,
    /// Crate tops.
    pub crate_top: String,
    /// Exit switch.
    pub switch: String,
    /// Liquid flat.
    pub liquid: String,
    /// Doom sector damage special for the liquid (0 = harmless).
    pub liquid_damage: i16,
    /// Walls of rooms under the sky.
    pub sky_wall: String,
    /// Ground of rooms under the sky.
    pub sky_floor: String,
    /// Tall light decoration (DoomEdNum).
    pub lamp: i16,
    /// Small decoration (DoomEdNum).
    pub small_decor: i16,
    /// Cavern walls.
    pub cave_wall: String,
    /// Cavern floor.
    pub cave_floor: String,
    /// Cavern ceiling.
    pub cave_ceil: String,
    /// Boardwalks and bridges (flat).
    pub wood: String,
    /// Building walls.
    pub building: String,
    /// Second building wall.
    pub building_alt: String,
    /// Courtyard / street paving.
    pub paving: String,
    /// Marsh and moat liquid.
    pub swamp_liquid: String,
}

impl Palette {
    /// Resolve `theme` against what the game has (`has(kind, name)`).
    pub fn resolve(theme: Theme, has: &dyn Fn(Kind, &str) -> bool) -> Self {
        let s = spec(theme);
        let b = biome_spec(theme);
        let pick = |kind: Kind, list: &[&str]| -> String {
            list.iter()
                .find(|n| has(kind, n))
                .or_else(|| list.first())
                .map_or_else(|| "-".to_owned(), |n| (*n).to_owned())
        };
        let t = |l: &[&str]| pick(Kind::Texture, l);
        let f = |l: &[&str]| pick(Kind::Flat, l);
        Self {
            theme,
            wall: t(s.wall),
            trim: t(s.trim),
            pillar: t(s.pillar),
            floor: f(s.floor),
            floor_alt: f(s.floor_alt),
            ceil: f(s.ceil),
            light: f(s.light),
            step: t(s.step),
            skirt: t(s.skirt),
            crate_wall: t(s.crate_wall),
            crate_top: f(s.crate_top),
            switch: t(s.switch),
            liquid: f(s.liquid),
            liquid_damage: s.liquid_damage,
            sky_wall: t(s.sky_wall),
            sky_floor: f(s.sky_floor),
            lamp: s.lamp,
            small_decor: s.small_decor,
            cave_wall: t(b.cave_wall),
            cave_floor: f(b.cave_floor),
            cave_ceil: f(b.cave_ceil),
            wood: f(b.wood),
            building: t(b.building),
            building_alt: t(b.building_alt),
            paving: f(b.paving),
            swamp_liquid: f(b.swamp_liquid),
        }
    }
}

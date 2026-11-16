//! Procedural multiplayer maps for Doom.
//!
//! [`generate`] builds a complete, playable deathmatch / co-op map from a
//! seed and a few knobs, as an editable [`RawMap`] (the editor's document),
//! so a result can be played at once, tweaked by hand, or saved to a WAD.
//!
//! The pipeline:
//! 1. **Layout** ([`layout`]): rooms of varied outlines (rectangles,
//!    octagons with 45° corners, crosses, long halls, big arenas) scattered
//!    with room for corridors, sized by player count.
//! 2. **Flow**: a spanning tree of corridors, then extra loops between
//!    rooms that are close on the map but far in the tree (no dead-end
//!    camping), routed with A* around everything else.
//! 3. **Verticality**: floor heights per room; corridors climb with 16-unit
//!    stairs, sunken pits, daises with one or two tiers.
//! 4. **Biomes** ([`biomes`]): districts of caverns, canyons, swamps,
//!    courtyards with a keep and moat, urban plazas, or built interiors;
//!    organic outlines by cellular automata, smoothed to 45°, terrain
//!    height fields clamped to walkable slopes.
//! 5. **Set pieces** ([`features`]): pillars, colonnades, crates, central
//!    blocks, skylights, light panels, pools; outdoor rooms under the sky
//!    with hills, boulders and ponds; teleporter pairs between far rooms.
//! 6. **Items** ([`things`]): deathmatch starts by farthest-point sampling,
//!    weapons tiered by room prestige and spread apart, ammo by its weapon,
//!    health / armour / power-ups by player count, decorations.
//! 7. **Check** ([`check`]): every start and item reachable from the others
//!    (step heights, headroom, teleporters); a failed seed is retried.
//! 8. **Emit** ([`emit`]): merged linedefs, textured sidedefs, sectors.
//!
//! The same seed and options always give the same map.

pub mod biomes;
pub mod check;
pub mod emit;
pub mod features;
pub mod grid;
pub mod layout;
pub mod noise;
pub mod rng;
pub mod theme;
pub mod things;

pub use biomes::{Biome, Biomes};
pub use room_map::RawMap;
use serde::{Deserialize, Serialize};
pub use theme::{Kind, Palette, Theme};

/// Map size relative to the player count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    /// Tight and fast.
    Small,
    /// Fits the player count.
    #[default]
    Medium,
    /// Room to roam.
    Large,
    /// Sprawling.
    Huge,
}

/// Generator settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Seed (same seed + options = same map).
    pub seed: u64,
    /// Map lump name.
    pub name: String,
    /// Players the map is built for (2–32).
    pub players: u8,
    /// Overall size.
    pub size: Size,
    /// Look.
    pub theme: Theme,
    /// Kinds of places: built interiors, caverns, canyons, swamps, plazas…
    pub biomes: Biomes,
    /// Height changes: 0 flat, 1 rolling, 2 dramatic.
    pub verticality: u8,
    /// Cover density: 0 open, 1 normal, 2 dense.
    pub cover: u8,
    /// Rooms under the sky: 0 none, 1 some, 2 many.
    pub outdoor: u8,
    /// Teleporter pairs between far rooms.
    pub teleporters: bool,
    /// Nukage / lava / water pools.
    pub liquids: bool,
    /// Megasphere / soulsphere and (8+ players) the BFG.
    pub power_items: bool,
    /// Monsters for solo / co-op: 0 none (deathmatch), 1 light, 2 normal,
    /// 3 hordes.
    pub monsters: u8,
    /// Position in an episode (0 = first map): tougher monsters later on.
    pub progress: u8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            seed: 1,
            name: "MAP01".into(),
            players: 8,
            size: Size::Medium,
            theme: Theme::Tech,
            biomes: Biomes::Mixed,
            verticality: 1,
            cover: 1,
            outdoor: 1,
            teleporters: true,
            liquids: true,
            power_items: true,
            monsters: 0,
            progress: 0,
        }
    }
}

/// What was built.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    /// Seed of the accepted attempt.
    pub seed: u64,
    /// Attempts until a playable map.
    pub attempts: u32,
    /// Map extent in world units.
    pub width: i32,
    /// Map extent in world units.
    pub height: i32,
    /// Rooms.
    pub rooms: usize,
    /// Corridors (tree + loops).
    pub corridors: usize,
    /// Loop corridors.
    pub loops: usize,
    /// Teleporter pairs.
    pub teleporters: usize,
    /// Deathmatch starts.
    pub dm_starts: usize,
    /// Weapons.
    pub weapons: usize,
    /// Health, armour, power-ups.
    pub items: usize,
    /// Monsters (all skills).
    pub monsters: usize,
    /// Secret closets.
    pub secrets: usize,
    /// Sectors.
    pub sectors: usize,
    /// Linedefs.
    pub lines: usize,
    /// Things.
    pub things: usize,
    /// Room set pieces, e.g. `["dais", "skylight"]` per room.
    pub features: Vec<Vec<String>>,
    /// Biome per room.
    pub biomes: Vec<Biome>,
}

/// A generated map and its report.
#[derive(Clone, Debug)]
pub struct Generated {
    /// The map document.
    pub map: RawMap,
    /// Summary.
    pub report: Report,
    /// Room boxes in map units (`x0, y0, x1, y1`) and their biome.
    pub rooms: Vec<([i32; 4], Biome)>,
}

/// Biome of the room containing map point `(x, y)`.
#[must_use]
pub fn biome_at(g: &Generated, x: i16, y: i16) -> Option<Biome> {
    let (x, y) = (i32::from(x), i32::from(y));
    g.rooms
        .iter()
        .find(|(b, _)| x >= b[0] && y >= b[1] && x < b[2] && y < b[3])
        .map(|(_, biome)| *biome)
}

/// Generation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Generate with every texture assumed present.
pub fn generate(opts: &Options) -> Result<Generated, Error> {
    generate_with(opts, &|_, _| true)
}

/// Generate, keeping only textures / flats `has` accepts (the loaded game).
/// Every monster is assumed available (see [`generate_for_game`]).
pub fn generate_with(opts: &Options, has: &dyn Fn(Kind, &str) -> bool) -> Result<Generated, Error> {
    generate_for_game(opts, has, &|_| true)
}

/// Generate for a loaded game: `has` checks textures / flats and
/// `has_sprite` a 4-letter sprite prefix (only monsters the game can draw
/// are placed, e.g. no Doom II monsters on Doom 1).
pub fn generate_for_game(
    opts: &Options,
    has: &dyn Fn(Kind, &str) -> bool,
    has_sprite: &dyn Fn(&str) -> bool,
) -> Result<Generated, Error> {
    let players = opts.players.clamp(1, 64);
    let opts = Options {
        players,
        verticality: opts.verticality.min(2),
        cover: opts.cover.min(2),
        outdoor: opts.outdoor.min(2),
        ..opts.clone()
    };
    let factor = match opts.size {
        Size::Small => 0.8,
        Size::Medium => 1.0,
        Size::Large => 1.25,
        Size::Huge => 1.55,
    };
    let themes: Vec<Theme> = if opts.theme == Theme::Mixed {
        Theme::CONCRETE.to_vec()
    } else {
        vec![opts.theme]
    };
    let pals: Vec<Palette> = themes.iter().map(|&t| Palette::resolve(t, has)).collect();
    let gate = ["GATE3", "GATE1", "GATE4", "GATE2", "TLITE6_5"]
        .iter()
        .find(|n| has(Kind::Flat, n))
        .copied()
        .unwrap_or("GATE3");
    for attempt in 0..24u32 {
        let seed = opts
            .seed
            .wrapping_add(u64::from(attempt).wrapping_mul(0x9E37_79B9));
        let mut rng = rng::Rng::new(seed);
        let side = (f64::from(40 + 5 * i32::from(players.min(32))) * factor).round() as i32;
        let side = side.clamp(40, 200);
        let aspect = 0.85 + rng.f64() * 0.3;
        let (gw, gh) = (side, (f64::from(side) * aspect).round() as i32);
        let rooms = layout::place_rooms(&mut rng, gw, gh, &opts, pals.len());
        let Some(mut lay) = layout::connect(&mut rng, gw, gh, rooms, &opts) else {
            continue;
        };
        let mut grid = grid::Grid::new(gw, gh);
        layout::paint(&mut grid, &lay, &pals, &mut rng);
        // Pads first: set pieces then work around them.
        let pads = features::teleporters(&mut grid, &mut lay, &opts, &mut rng, gate);
        features::dress(&mut grid, &mut lay, &pals, &opts, &mut rng);
        features::exit_switch(&mut grid, &lay, &pals);
        let want_secrets = 1 + usize::from(opts.monsters > 0) + usize::from(lay.rooms.len() > 15);
        let secrets = features::secrets(&mut grid, &lay, &pals, &mut rng, want_secrets);
        let mut placed = things::place(&grid, &lay, &pals, &pads, &opts, &mut rng, has_sprite);
        things::secret_prizes(&mut placed, &secrets, &opts, &mut rng, has_sprite);
        if placed.dm_starts < (usize::from(players)).min(8) || placed.weapons < 2 {
            continue;
        }
        if !check::playable(&grid, &placed.must_reach, &pads) {
            continue;
        }
        let report = Report {
            seed,
            attempts: attempt + 1,
            width: gw * grid::CELL,
            height: gh * grid::CELL,
            rooms: lay.rooms.len(),
            corridors: lay.corridors.len(),
            loops: lay.corridors.iter().filter(|c| c.is_loop).count(),
            teleporters: pads.len(),
            dm_starts: placed.dm_starts,
            weapons: placed.weapons,
            items: placed.items,
            monsters: placed.monsters,
            secrets: secrets.len(),
            sectors: 0,
            lines: 0,
            things: placed.things.len(),
            features: lay
                .rooms
                .iter()
                .map(|r| r.features.iter().map(|s| (*s).to_owned()).collect())
                .collect(),
            biomes: lay.rooms.iter().map(|r| r.biome).collect(),
        };
        let (ox, oy) = (gw * grid::CELL / 2, gh * grid::CELL / 2);
        let rooms = lay
            .rooms
            .iter()
            .map(|r| {
                let c = grid::CELL;
                (
                    [
                        r.x0 * c - ox,
                        r.y0 * c - oy,
                        (r.x0 + r.w) * c - ox,
                        (r.y0 + r.h) * c - oy,
                    ],
                    r.biome,
                )
            })
            .collect();
        let name = if opts.name.trim().is_empty() {
            "MAP01".to_owned()
        } else {
            opts.name.trim().to_ascii_uppercase()
        };
        let map = emit::emit(&grid, &name, placed.things);
        if map.validate().is_err() || map.linedefs.len() >= 0xFFFF || map.sidedefs.len() >= 0xFFFF {
            continue;
        }
        let report = Report {
            sectors: map.sectors.len(),
            lines: map.linedefs.len(),
            ..report
        };
        return Ok(Generated { map, report, rooms });
    }
    Err(Error(
        "no playable layout after 24 attempts; try another seed or a larger size".into(),
    ))
}

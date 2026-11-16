# room-mapgen

Procedural multiplayer maps for Doom (deathmatch, Battle Royale, co-op),
generated as an editable `room_map::RawMap`: play it at once, tweak it in the
editor, or write it to a WAD.

```rust
let map = room_mapgen::generate(&room_mapgen::Options {
    seed: 42,
    players: 8,
    theme: room_mapgen::Theme::Hell,
    ..Default::default()
})?;
let lumps = map.map.to_lumps()?; // MAP01, THINGS, LINEDEFS, …
```

Same seed + options = same map (own xoshiro PRNG, no OS randomness).

## Pipeline

| Step | Module | What it does |
|---|---|---|
| Layout | `layout` | Rooms (rectangles, octagons with 45° corners, crosses, long halls, big arenas) scattered with corridor space; count and canvas grow with players. |
| Flow | `layout` | Spanning tree of A*-routed corridors (turn penalty → straight halls), then loops between rooms close on the map but ≥ 3 hops apart in the tree. |
| Height | `layout` | Floor height per room along the tree; corridors climb with 16-unit stairs (only where the corridor is long enough). |
| Biomes | `biomes` | Each district gets a kind of place: built interiors, open yards, **caverns** (cellular-automata outlines smoothed to 45°, rolling floors, uneven rock ceilings, dim light, liquid streams), **canyons** (rock valleys under the sky, broad 16-unit terraces), **swamps** (liquid, dry islands, boardwalks from every doorway to a central platform), **courtyards** (keep with doorways, moat, bridges, corner towers), **plazas** (building blocks, some hollow, streets between). Height fields are clamped to ≤ 16 between neighbours and fade to the room's height at doorways. |
| Dressing | `features` | Pilasters, lowered ceiling ring with trim band, daises (one or two tiers), sunken pits / liquid pools, pillars, colonnades, crates, central blocks, skylights, light panels; outdoor rooms under the sky with hills, boulders, ponds. Teleporter pairs between far rooms; exit switch in the room farthest from the hub. |
| Items | `things` | Deathmatch starts by farthest-point sampling, co-op starts in the hub, heavy weapons in prestige rooms far apart, light weapons elsewhere, ammo by its weapon, health / armour / power-ups by player count, bonus trails in corridors, lamps / torches / trees in corners. |
| Check | `check` | Every start, weapon and item reachable from the first start *and back* (≤ 24 steps, 56 headroom, teleporters one-way). A failed seed is retried. |
| Emit | `emit` | Cell boundaries → linedefs merged along straight runs, front sides into sectors (teleport pads entered front-to-back), textures per side, continuous texture offsets. |

Biome mixes: `mixed` (default, by district), `indoor`, `caves`, `wilderness`
(canyons, swamps, caves), `urban` (plazas, courtyards).

Themes (`tech`, `base`, `hell`, `castle`, `ruins`, `mixed` districts) list
candidate textures in order; `generate_with` keeps the first one the loaded
game has, so maps stay valid on any IWAD (best on Doom II / Freedoom 2).

## Where it is used

- **Editor**: *🎲 Generate…* (Ctrl+G) opens a panel; the result is a normal
  map document (undo, edit, play, export).
- **CLI**: `room map generate -o arenas.wad --count 8 --players 16 --theme mixed --biomes wilderness`.
- **Tests**: `room-game` runs a full 8-bot Battle Royale on generated maps;
  `room-render --test mapgen_preview -- --ignored` writes screenshots to /tmp.

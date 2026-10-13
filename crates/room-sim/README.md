# room-sim

Playsim: World, thinkers, actors, physics, and specials.

## Public API (M5 bootstrap)

- `World::from_map` / `from_map_with_classes` — build dynamic state, spawn map things
- `World::tick` / `checksum` — physics + state timers + determinism hash
- `ClassRegistry::from_decorate` — DECORATE IR → class defaults / DoomEdNums
- `StateTable::from_decorate` — flat state graph (`Loop`/`Goto`/`Stop`)
- `try_move` / `xy_movement` / `z_movement` — blockmap collision + gravity
- `cross_special_lines` / `use_lines` / `execute_special` — walkover, use, Hexen specials
- Door / floor / plat / ceiling / strobe movers; teleport; secrets; sector damage
- `Actor` / `ActorArena` — generational mobj store
- `CompatFlags` / `CompatMode` — GZDoom `compatflags` bitfields (`COMPLVL` → presets)

Determinism rules in `docs/02-conventions.md` apply to all code here.

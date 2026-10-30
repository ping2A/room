# doom1-survival — Survival Director

Room WASM mod (`docs/27`) for Doom / Freedoom maps: skill-scaled Director waves,
elites, kill streaks and ammo rewards. Written in Rust with `room-script-sdk`.
AssemblyScript cousin: `mods/doom1-bounty`.

**Status:** **SV0**–**SV9** + engine **MH0**–**MH4** done (`docs/27`). Optional
**MH5** / C6 translation render / C10 sector mood deferred.

## Behaviour

- Director phases (Warmup → Assault → Peak/Clear → Intermission); threat budget
  and alive-threat cap scale with skill and player count.
- Wave archetypes (Swarm, Pincer, Ambush, Elite Pack, Siege, Horde Rush, Boss).
- Spawns at learned map spots (≥ 384 from the team); stuck monsters recycle.
- Coop: targeting / distance use every alive seat (waves continue if seat 0 dies).
- Elites (`EliteZombie` / `EliteImp` / `EliteDemon`) with affixes (Brute, Haste,
  Regenerating, Vampiric, Volatile, Splitter, Summoner, Phasing); UV guarantees
  one by round 3.
- Announcer queue: ≤ one team line per 1.5 s; music restore for `ExMy` and `MAPxx`.
- Between waves: on-screen perk draft (Mod HUD MODAL + input capture); Marks currency.
- Stock classes: `PhantomImp`, `Juggernaut`. Kill streaks, heal-on-kill, ammo on clear.

## Layout

```
src/
  lib.rs        export_mod! + RoomMod wiring
  host.rs       Host trait, SdkHost, FakeHost (tests)
  director.rs   phase machine + wave release
  elites.rs     affixes + elite subclasses
  announcer.rs  priority / rate-limited messages
  spots.rs      spot collect / pick / stuck
  tables.rs     tuning + monster pool
  register.rs   PhantomImp / Juggernaut / Elite*
  …
```

## Build

```bash
./scripts/room-server.sh mods   # → survival.{wasm,wad} (+ other mods)
```

Release `survival.wasm` is about **59 KiB** (opt-level `z`, LTO, stripped; SV8).
`survival.wad` holds `ROOMMOD` + `SURVIVAL` + Track B `DECORATE`/`GLDEFS`/`SNDINFO`.

## Play

```bash
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad \
  --file mods/doom1-survival/survival.wad --map E1M1 --play --skill 4

./scripts/play-web.sh   # ?skill=4&mod=survival
./scripts/room-server.sh start
```

## Tests

```bash
cargo test --manifest-path mods/doom1-survival/Cargo.toml
cargo test -p room-game --test wasm_mod_survival
```

# doom1-royale — Battle Royale 2.0

Room WASM mod for Doom / Freedoom maps (`docs/28`). Last player standing on
any stock map — up to **8 seats**, deterministic lockstep, no monsters,
fists-only start, a fixed set of weapons placed on the map, a moving zone,
spectate, and bots to fill empty seats. Kept simple on purpose: free-for-all
only, nothing is dropped during the match (no supply crates, no death drops),
and no interaction between players beyond shooting each other.

**Status:** **BR0**–**BR9** + engine **BE1**–**BE12** done (`docs/21` §0). Dedicated
**ROYALE01** map + balance pass shipped; **BE1** wgpu wall, **S-BR2** leaderboards deferred.

## Modes

| Mode | When |
|------|------|
| **Solo practice** | One human in the room — dying ends the match (not in lobby *Play solo*; `room.toml` `solo = false`). |
| **Free-for-all** | 2–8 players; every seat for themselves. |

Bots (`fill_bots` / empty seats) use the same sim brain as humans so rooms
feel full; 6 bots + 2 humans is a typical test.

## Match flow (short)

1. **Pregame** (10 s) — fists only, no damage.
2. **Drop select** (15 s) — full-screen map of the whole level: walls, the
   safe zone (red outside it) and every gun placed for the match, colour
   coded. Move the cursor with movement / turn; **USE** or **ATTACK** locks
   your landing. Other players' landings show with their names.
3. **Deploy** (3 s) — the weapons spawn, then everyone lands near their pick
   (next to loot rather than on it when there is room); brief landing shield.
4. **Loot** (60 s grace) — pick up what is on the map; the zone is visible.
5. **Zone phases** — hold → shrink (centre moves); damage outside; final circle ~60 s to zero.
6. **Victory** — MODAL + standings → `game::end_match_ex`.

## Loot

Placed once, when drop select starts; nothing is added later.

- **Where:** the spots of the map's own pickups (weapons, ammo, health,
  armour, powerups — keys stay), which the mod removes on load. Mappers put
  items where players can walk, so these spots are reachable. Monster spots
  and player starts are the fallback; deathmatch starts and some other
  spots stay free for landings.
- **What** (P = players, at least 2): P shotguns, P/2 chainguns, P/4 rocket
  launchers (near the zone centre), 1 chainsaw — each with its ammo beside
  it, at least 448 units apart when the map has room — plus P shells, clips,
  stimpacks and armour bonuses, P/2 medikits and green armours, P/4 rockets
  and one berserk (fractions round up).
- A dead player drops nothing: the engine's own weapon drop is removed.

## Controls

| Input | Role in this mod |
|-------|------------------|
| Drop select | Move + **USE** or **ATTACK** to lock your landing. |
| After death | **ATTACK** / **+mod1** cycles spectate targets (killer first). |

## HUD (Mod HUD)

- Top right: square minimap of the level around you (north up, whole level
  shown, red outside the zone, next zone dashed), with **ALIVE** and
  **KILLS** under it.
- Top centre: zone timer + bar (red while the zone closes).
- Kill feed, elimination toast, hit marker, damage direction.
- Death: elimination MODAL (~4 s recap), then spectate (BE9 camera).

In-game kill feed / HUD use session **player names** (BE3); fallback **Player N** when unset.

## Content (Track B)

`royale.wad` packs `ROOMMOD`, module lump **ROYALE**, a sound lump and an embedded map:

- **SNDINFO** — `royale/*` aliases to shareware Doom SFX (zone, supply, kill, …).
- **ROYALE01** — GPL UDMF arena (~5.4k units, 18 DM starts, open sightlines + cover pillars). Source: `room_testkit::royale01()`; regenerate with `cargo run -p room-testkit --example write_royale01`.

### Map **ROYALE01**

Built from `mods/doom1-royale/maps/ROYALE01.wad` (merged into `royale.wad` by `room-server.sh mods`). Play on any IWAD/Freedoom that provides the stock textures:

```bash
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad \
  --file mods/doom1-royale/royale.wad --map ROYALE01 --play

curl -X POST http://127.0.0.1:8765/api/rooms -H 'Content-Type: application/json' \
  -d '{"map":"ROYALE01","skill":3,"max_players":8,"mod":"royale","fill_bots":true,"quick_play":true}'
```

With `"quick_play": true` the room starts on a timer (server `quick_play_timeout_secs`, 60 s by
default) after the first player joins, and the empty slots become bots. The lobby's *Host game*
dialog sets both flags when the Battle Royale mod is chosen and **Fill empty slots with bots** is
ticked (the default). Bots use the deterministic `room-sim` bot brain, the same one as *Quick play BR*.

## Layout

```
src/
  lib.rs          export_mod! + event routing
  host.rs         Host / SdkHost / FakeHost
  match_flow.rs   Phase machine (pregame → resolution)
  zone.rs         Moving circles + damage
  loot.rs         Map loot: spots, plan, spawn
  death.rs        Recap + spectate
  moments.rs      Banners, music, victory + standings
  ui.rs           Mod HUD widgets
  …
lumps/            SNDINFO (Track B)
maps/             ROYALE01.wad (embedded at pack time)
```

## Build

```bash
./scripts/room-server.sh mods   # → royale.{wasm,wad} (+ other mods)
```

Release `royale.wasm` is built with `opt-level = "z"`, LTO, stripped.

## Play

```bash
# Room server (multiplayer)
./scripts/room-server.sh start   # http://127.0.0.1:8765/
# Lobby → Host game → Mod → Battle Royale, map + up to 8 players

curl -X POST http://127.0.0.1:8765/api/rooms -H 'Content-Type: application/json' \
  -d '{"map":"E1M1","skill":3,"max_players":8,"mod":"royale","fill_bots":true}'

# Desktop practice (multiplayer mod, single local player)
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad \
  --file mods/doom1-royale/royale.wad --map E1M1 --play
```

Every client downloads `royale.wad` from the server and checks its fingerprint
before load (`docs/22` §Mods).

## Tests

```bash
cargo test --manifest-path mods/doom1-royale/Cargo.toml   # zone, loot, native unit
cargo test -p room-game --test wasm_mod_royale              # 18+ E2E scenarios
cargo test -p room-server --features verifier --test verifier_headless
```

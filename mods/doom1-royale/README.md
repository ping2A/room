# doom1-royale — Battle Royale 2.0

Room WASM mod for Doom / Freedoom maps (`docs/28`). Last player or squad
standing on any stock map — up to **8 seats**, deterministic lockstep, no
monsters, fists-only start, tiered loot, moving zone, supply drops, squads,
spectate, and bots to fill empty seats.

**Status:** **BR0**–**BR9** + engine **BE1**–**BE12** done (`docs/21` §0). Dedicated
**ROYALE01** map + balance pass shipped; **BE1** wgpu wall, **S-BR2** leaderboards deferred.

## Modes

| Mode | When |
|------|------|
| **Solo practice** | One human in the room — dying ends the match (not in lobby *Play solo*; `room.toml` `solo = false`). |
| **Free-for-all** | 2–8 players; every seat for themselves. |
| **Duos** | ≥ 4 contenders — teams `seat/2`, no friendly fire, downed/revive (see Squads). |

Bots (`fill_bots` / empty seats) use the same sim brain as humans so rooms
feel full; 6 bots + 2 humans is a typical pit test.

## Match flow (short)

1. **Pregame** (~45 s max) — fists only, no damage; ready = hold **USE** 1 s.
2. **Drop select** (15 s) — full-screen map; move cursor with movement / turn; **USE** or **ATTACK** to confirm landing.
3. **Deploy** (~3 s) — teleport to pick; brief landing shield.
4. **Loot** (60 s grace) — zone visible; first supply inbound ~30 s, lands ~45 s in the **next** circle.
5. **Zone phases** — hold → shrink (centre moves); damage outside; final circle ~60 s to zero.
6. **Victory** — MODAL + rich standings → `game::end_match_ex`.

## Controls (mod buttons)

Desktop defaults (`room-input`): **G** = `+mod1`, **M** = `+mod2`. Web: **B** / **M**.

| Input | Role in this mod |
|-------|------------------|
| **+mod1** (G / B) | Squad **ping** at crosshair (team-only marker). |
| **USE** (hold) | Open supply crate within 64 u (2 s bar); revive teammate 4 s; pregame ready. |
| **ATTACK** / **+mod1** | After death: cycle spectate targets (killer first). |
| Drop select | Move + **USE** or **ATTACK** to confirm. |

Map/minimap: Mod HUD **MAP** widget during drop select and in-game minimap when the zone is active (BE2).

## HUD (Mod HUD)

- Alive count, zone timer + bar, kill feed, elimination toast, hit marker, damage direction.
- Squad panel (duos): teammate status, downed / revive / bleed bars, ping markers.
- Supply inbound marker + progress bar while opening crate.
- Death: elimination MODAL (~4 s recap), then spectate (BE9 camera).

In-game kill feed / HUD use session **player names** (BE3); fallback **Player N** when unset.

## Content (Track B)

`royale.wad` packs `ROOMMOD`, module lump **ROYALE**, optional lumps, and embedded maps:

- **DECORATE** — supply beacon + rarity glow actor stubs (runtime classes in WASM).
- **GLDEFS** — beacon + rarity point lights.
- **SNDINFO** — `royale/*` aliases to shareware Doom SFX (zone, supply, kill, …).
- **ROYALE01** — GPL UDMF arena (~5.4k units, 18 DM starts, open sightlines + cover pillars). Source: `room_testkit::royale01()`; regenerate with `cargo run -p room-testkit --example write_royale01`.

### Map **ROYALE01**

Built from `mods/doom1-royale/maps/ROYALE01.wad` (merged into `royale.wad` by `room-server.sh mods`). Play on any IWAD/Freedoom that provides the stock textures:

```bash
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad \
  --file mods/doom1-royale/royale.wad --map ROYALE01 --play

curl -X POST http://127.0.0.1:8765/api/rooms -H 'Content-Type: application/json' \
  -d '{"map":"ROYALE01","skill":3,"max_players":8,"mod":"royale","fill_bots":true}'
```

## Layout

```
src/
  lib.rs          export_mod! + event routing
  host.rs         Host / SdkHost / FakeHost
  match_flow.rs   Phase machine (pregame → resolution)
  zone.rs         Moving circles + damage
  loot.rs         Rarity, supply, vault, death drops
  teams.rs        Duos, downed, revive, pings
  death.rs        Recap + spectate
  moments.rs      Banners, music, victory + standings
  ui.rs           Mod HUD widgets
  …
lumps/            DECORATE, GLDEFS, SNDINFO (Track B)
maps/             ROYALE01.wad (embedded at pack time)
```

## Build

```bash
./scripts/room-server.sh mods   # → royale.{wasm,wad} (+ other mods)
```

Release `royale.wasm` is built with `opt-level = "z"`, LTO, stripped.

## Play

```bash
# Room server (multiplayer pit)
./scripts/room-server.sh start   # http://127.0.0.1:8765/
# Lobby → Open a pit → Mod → Battle Royale, map + up to 8 players

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

# doom1-bounty — example Room mod in AssemblyScript

"Bounty Hunter" for `doom1.wad` (works on any Doom map). One AssemblyScript
file, `bounty.ts`, using the game API in `crates/room-web/as/room.ts`. Start
here if you want to write your own mod; the language and API are documented in
docs/12 §16.

- **Bounties.** 5 s after the map starts, then 4 s after each claim, a random
  living monster is marked. Its health is multiplied by a skill-based
  toughness (150 % on ITYTD up to 400 % on Nightmare), and it gets bigger and
  faster.
- **Hints.** Every 6–15 s (depending on skill), a message gives the bounty's
  distance, compass direction and whether it is in sight. Hitting it reports
  its remaining health.
- **Payouts.** Each claim adds score (50–250 depending on skill) and gives a
  reward that rotates through Shotgun, Chaingun, Rocket Launcher and extra
  shells, plus +25 health (capped at 200). Kills within 3 s of each other chain
  into combos, and backpacks are worth +100 score.
- **Final boss.** When no monsters are left, the mod class `Warden` (a Baron of
  Hell variant with 1500 HP, scaled 130 %) teleports in far from the player and
  hunts them.

| What it shows | Where in `bounty.ts` |
|---|---|
| Declaring a class | `register()` → `defineClass` / `classProperty` / `endClass` |
| Timers in tics | `onTick` with `TICRATE` |
| Querying actors | `actorsOfClass("")`, `isMonster`, `isDead`, `canSee`, `distanceTo` |
| Changing actors | `health`, `speed`, `setFloat(FProp.SCALE_X, …)`, `target`, `setState` |
| Inventory | `give(weapon)`, `give(ammo, n)` |
| Spawning | `spawn("Warden", x, y, NaN, angle, Spawn.TELEFOG \| Spawn.CHECK_POSITION)` |
| Deterministic math / RNG | `atan2`, `sqrt`, `random` |
| Events | `onLoad`, `onTick`, `onThingDamaged`, `onThingDied`, `onItemPickup`, `onPlayerDied`, `onUnload` |

## Build

```bash
./scripts/room-server.sh mods   # → mods/doom1-bounty/bounty.{wasm,wad} (and the other mods)
```

The script installs AssemblyScript on first use (`scripts/fetch-assemblyscript.sh`),
compiles with `node scripts/asc-compile.mjs` (same options as the web editor) and
packs `bounty.wad` with three lumps:

| Lump | Content |
|---|---|
| `ROOMMOD` | manifest (`room.toml`) |
| `BOUNTY` | compiled module (`bounty.wasm`) |
| `S_BOUNTY` | AssemblyScript source, so the editor can reopen it |

## Play

```bash
# Desktop
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad \
  --file mods/doom1-bounty/bounty.wad --map E1M1 --play --skill 4

# Web: pick "Bounty Hunter (AssemblyScript)" in the play page's Mod select
# (http://127.0.0.1:8080/?skill=4&mod=bounty)
./scripts/play-web.sh

# Multiplayer: the lobby's "Open a pit" → Mod → Bounty Hunter
# (every player downloads /api/mods/bounty.wad before the map loads)
./scripts/room-server.sh start   # http://127.0.0.1:8765/
```

## Edit it live in the browser

1. `./scripts/play-web.sh`, open `editor.html`, *Scripts* tab.
2. *Example: Bounty Hunter (AS)*, then *Copy into editor for editing*. The
   module opens as AssemblyScript.
3. *▶ Play*, change something (for example `FIRST_BOUNTY` or the `PAY` table),
   then press `Ctrl+S`. The mod reloads into the running level.
4. *Export WAD* saves your version (manifest, module and source).

Test: `cargo test -p room-game --test wasm_mod_bounty`.

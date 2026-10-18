# doom1-survival — example Room WASM mod

"Survival Director" for `doom1.wad` (works on any Doom map): written in Rust
with `room-script-sdk`, it takes control of the game through the WASM game API
(docs/12 §15). For the same kind of mod in AssemblyScript (no Rust toolchain,
editable live in the web editor), see `mods/doom1-bounty`.

- Monster waves spawn (with teleport fog) at the map's own thing spots, out of
  least 320 units from the player; one new monster type unlocks per wave.
- Skill sets the pace: a wave every 60 / 50 / 40 / 32 / 25 s (ITYTD → NM), more
  monsters per wave and less healing on harder skills.
- Every fifth wave is a boss wave: Barons and music `D_E1M8`.
- Two mod classes: `PhantomImp` (translucent, faster imp) and `Juggernaut`
  (big, heavy, 400 HP demon).
- Kill-streak messages, heal on kill, ammo reward when a wave is cleared,
  low-health warning.

## Build

```bash
./scripts/room-server.sh mods   # → mods/doom1-survival/survival.{wasm,wad} (and the other mods)
```

`survival.wad` holds a `ROOMMOD` manifest (`room.toml`) and the module lump
`SURVIVAL`.

## Play

```bash
# Desktop
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad \
  --file mods/doom1-survival/survival.wad --map E1M1 --play --skill 4

# Web: pick Skill + "Survival Director (Rust)" in the play page's top-left bar
# (http://127.0.0.1:8080/?skill=4&mod=survival), or in the editor's
# Scripts tab → "Example: Survival (Rust)", then Play
./scripts/play-web.sh

# Multiplayer: the lobby's "Open a pit" → Mod → Survival Director
./scripts/room-server.sh start   # http://127.0.0.1:8765/
```

Test: `cargo test -p room-game --test wasm_mod_survival`.

# doom1-royale — example Room WASM mod (multiplayer)

"Battle Royale" for `doom1.wad` rooms on `room-server` (works on any Doom
map): written in Rust with `room-script-sdk`. Players fight each other until
one is left.

- **No enemies.** Every monster is removed when the map starts; each monster
  spot (and unused player start) becomes a loot drop (weighted random,
  deterministic). Loot is tiered by distance to the zone centre: rocket
  launchers and chainguns only in the inner rings, shotguns / chainsaw /
  small pickups outside. Guns are spaced ≥ 384 units apart (the map's own
  weapons count), rocket launchers are capped at ⌈players/2⌉ and chainguns at
  one per player (extras become ammo), every gun is dropped with its ammo, a
  shotgun is placed near each player, and maps without a rocket launcher get
  one at the drop nearest the centre.
- **No guns.** Players start with their fists only (pistol and bullets are
  taken away) and spawn on deathmatch starts (fallback: free monster spots),
  spread as far apart as possible, facing the zone.
- **Deathmatch rules, one life.** Players hurt each other and frags count on
  the status bar (`game::rule::DEATHMATCH`); dead players stay down until the
  next map (`game::rule::RESPAWN = 0`). A frag heals the killer by 25.
- **Closing zone.** After 45 s a zone centred near the middle of the map
  starts closing: each phase holds 25 s, then shrinks to 60 % in 20 s
  (minimum radius 160). Standing outside costs 3, 5, 7 … HP per second (max 25);
  players outside get a private hint with the direction and distance.
- **Last one standing wins.** Eliminations are announced to everyone
  ("Player 1 fragged Player 2 - 1 left", "… was swallowed by the zone");
  the victim is told their placing. 3 s after the winner (or "No survivors")
  is announced the mod calls `game::end_match`: every client shows the final
  standings screen and the server closes the room.
- **Room time limit.** "60 / 10 seconds left!" warnings; when time runs out
  the surviving player with the most frags (then health) wins, a tie is a draw.
- With a single player it runs as a practice round (dying ends it).
- Zone logic runs once per second, not every tic.

Players are named by seat ("Player 1" … ); the web client's kill feed still
shows callsigns. Doom coop has four player starts, so rooms take 2–4 players.

## Build

```bash
./scripts/room-server.sh mods   # → mods/doom1-royale/royale.{wasm,wad} (and the other mods)
```

`royale.wad` holds a `ROOMMOD` manifest (`room.toml`) and the module lump
`ROYALE`.

## Play

```bash
# Multiplayer: the lobby's "Open a pit" → Mod → Battle Royale, 2–4 players
./scripts/room-server.sh start   # http://127.0.0.1:8765/
# or create the room directly
curl -X POST http://127.0.0.1:8765/api/rooms -H 'Content-Type: application/json' \
  -d '{"map":"E1M1","skill":3,"max_players":4,"mod":"royale"}'

# Practice round on the desktop
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad \
  --file mods/doom1-royale/royale.wad --map E1M1 --play
```

Every player in the room downloads `royale.wad` from the server and checks
its fingerprint before the map loads (docs/22 §4 *Mods*).

Tests: `cargo test -p room-game --test wasm_mod_royale` (monsters cleared,
fists only, deathmatch spawns, guns with ammo, PvP, no respawn, zone,
winner + match end, time limit, solo practice, determinism, doom1 E1M1) and `cargo test -p room-server --features verifier
--test verifier_headless` (server verifier runs the mod).

# room-game

Game state machine: startup, levels, hubs, intermission, saves, demos.

## Public API (M5 bootstrap)

- `Headless` — load custom map, `tick` / `run_tics`, checksum, optional demo record
- `continue_to_map` / `default_next_map_name` — intermission → next MAPxx / ExMy
- `enter_finale` / `leave_finale` — episode end when `next_map` is `None`
- `load_embedded_engine_data` / `mount_embedded_engine` — `wadsrc/room` packed at build time
- `DemoRecorder` / `DemoPlayer::verify` — periodic world checksum verification
- `SoundSystem` — load SNDINFO from VFS, resolve logical names → DMX PCM, translate sim events
- `play_map_music` / `doom2_music_lump` — MAPINFO or Doom II defaults → MUS into `MusicPlayer`
- `hud_state` — `PlayerHud` → `room-ui::HudState` for the Doom status bar
- `Skill` / `GameOptions` / `GameState` (`coop_players` spawns seats 2–4)

## Multiplayer tests

```bash
cargo test -p room-game --test coop_netplay
# UDP lobby + dual-seat lockstep, independent movement, friendly-fire block, desync
cargo run -p room-game --example udp_coop_doom1 --release
```

# 17 — Netplay, demos, save games

Crates: `room-net`, `room-game::{demo, save}`. GZDoom references: `src/d_net.cpp`,
`src/d_protocol.cpp`, `src/g_game.cpp` (demo record/playback), `src/serializer*.cpp`,
`src/g_level.cpp` (`G_SnapshotLevel`, hubs).

## 1. Netplay

### 1.1 Model

Deterministic **lockstep**, like GZDoom: every peer runs the full simulation; only
`TicCmd`s and `NetCommand`s are exchanged. Requirements (from `02 §4.2`) make this work across platforms.

- Topologies: **peer-to-peer** (each peer sends its commands to all; GZDoom default for ≤ 4 players) and **packet-server** (host relays; GZDoom `-netmode 1`), selectable; packet-server is default for > 4 players and for web peers.
- Players: up to 8 in 1.0 (storage supports 16).
- Input delay: adaptive `netdelay` (default auto: RTT/2 rounded up to tics + 1, clamp 1..=8). Local input is buffered for `delay` tics; the sim only advances when all peers' commands for tic N are present (stall otherwise; show "waiting for player" after 1 s).
- `ticdup` (GZDoom `-dup`): send one command per N tics — S.
- No rollback/prediction in 1.0 (deterministic rollback is a possible 2.0 feature; the architecture — fast `World` snapshot/restore — keeps that door open; record as ADR).

### 1.2 Session lifecycle

1. **Lobby/handshake**: host (`-host N`) listens; joiners (`-join addr`) connect. Handshake exchanges: engine version, ABI version, load-order fingerprint (ordered list of container MD5s + `Definitions` hash), module identities (sha256), player userinfo. Mismatch → refuse with a readable diff.
2. **Arbitrator** (host) chooses game settings (map, skill, `dmflags`, server CVars), broadcasts them, and assigns player numbers. All peers start the game on the same tic with the same RNG seed (`rngseed` chosen by arbitrator).
3. **Load gate (web coop)**: after `Start`, each peer loads the map then broadcasts `LobbyMsg::Ready`; lockstep tic 0 begins only when every seat is Ready (avoids host fill/advance while a joiner is still loading).
4. **In game**: per tic, each peer sends `{tic, cmd, netcmds[]}` with redundancy (last K unacknowledged tics in every packet) over unreliable transport; acks piggyback.
5. **Consistency**: every 35 tics, each peer sends `World::checksum()` of tic N−delay−1; mismatch → "Desync detected at tic N" message; optionally dump both peers' world states (`net_desync_dump`) for debugging (critical tooling for determinism bugs).
6. **Leave/drop**: peer timeout (`net_timeout`, default 10 s) → `PlayerDisconnected` event, player removed on all peers at an agreed tic (arbitrator broadcasts drop tic). Host migration: S.
7. Pause (`pause` command broadcast as net command), chat, `kick`, `say`, `say_team`. Voting is out of scope (C).

Join-in-progress is not supported (same as GZDoom).

### 1.3 Transports

```rust
pub trait Transport { fn send(&mut self, peer: PeerId, bytes: &[u8]); fn poll(&mut self, out: &mut Vec<(PeerId, Bytes)>); fn peers(&self) -> &[PeerId]; }
```

- Native transports (`room-app --transport`):
  - **`udp`** (default): `UdpTransport` — non-blocking UDP, peer-to-peer (≤ 4). NAT traversal not provided (port forwarding / direct IP, like GZDoom).
  - **`tcp`**: `TcpTransport` — length-prefixed TCP mesh, peer-to-peer.
  - **`tcp-server`**: `TcpRelayServer` / `TcpRelayClient` — host packet-server relay (GZDoom `-netmode 1`); one reachable address; up to 8 seats.
- Optional **`--noise`**: `NoiseTransport` wraps any of the above after lobby with `Noise_XX_25519_ChaChaPoly_BLAKE2s` (`snow`, pure-Rust resolver).
- Optional: WebRTC via `matchbox_socket` for native↔web cross-play (feature `webrtc`).
- Web: WebRTC data channels (unordered, unreliable) via `matchbox_socket` + a small signalling server binary `room-signal` (separate crate, can be self-hosted).
- Web coop (browser): `room-ws` WebSocket packet-server speaking `FTRL` (`MSG_HELLO` / `WELCOME` / `DATA` / `REJECT` / `VOICE` / `PEER_JOIN` / `PEER_LEAVE`); `room-web` lockstep on `MSG_DATA`, live mic PCM on `MSG_VOICE` (out-of-band), and console presence on join/leave.
- Packet format: versioned header, varint tics, delta-encoded `TicCmd` (port GZDoom's `UnpackUserCmd`/`WriteUserCmdMessage` delta idea), `NetCommand`s encoded with `postcard`.

### 1.4 What is synchronized

Only inputs. User CVars (`name`, `color`, `skin`, `autoaim`, …) change via `NetCommand::ChangeUserInfo` and take effect at that tic for all peers. Server CVar changes are arbitrator-only net commands. UI-scope module events reach play scope only through `NetCommand::NetEvent`.

## 2. Demos

### 2.1 Room demo format (`.fdem`)

IFF-like container (chunk = 4-byte id + u32 length):

| Chunk | Content |
|---|---|
| `FDHD` | magic version, engine version, ABI version, start map, skill, game options, rng seed, player count & player numbers, consoleplayer |
| `LOAD` | load order fingerprint: list of `(container name, md5)`, `Definitions` hash, module identities |
| `CVAR` | server CVars + per-player synced userinfo at start |
| `BODY` | per tic: for each player `TicCmd` (delta-encoded) + `NetCommand`s; end marker |
| `CSUM` | optional periodic world checksums (every 35 tics) for playback verification |
| `SAVE` | optional embedded start save (demos recorded from a save game) |

- Record: `-record name`, `record` command; stop with `stop` or level/game end per `demo_*` options.
- Playback: `-playdemo`, `playdemo`; `-timedemo` (no frame limiter, reports fps); fast-forward/skip-to-tic (re-simulate quickly; with periodic in-memory snapshots for seeking backwards — S); view other players (`spynext`).
- Verification mode (`room-cli demo verify file.fdem --wads ...`): runs headless, compares `CSUM`s, exits non-zero on mismatch — used in CI.

### 2.2 Foreign demos

- **Vanilla Doom 1.9 / Boom / MBF / complevel demos**: playback only, requires `compatmode vanilla|boom|mbf|mbf21` (`19-compatibility.md`), fixed-point paths and vanilla RNG. Goal: sync for a curated test set (see `20`).
- **ZDoom/GZDoom `.lmp` (ZDEM)**: best-effort playback (S); not expected to stay in sync on complex maps.

## 3. Save games

### 3.1 Container

A ZIP file (`.fsave`, stored or deflate) — inspectable and versionable:

```
info.json          { format_version, engine_version, created_utc, title, map, map_title, level_time, total_time, skill,
                     game, iwad, load_order: [{name, md5}], defs_hash, modules: [{mod_id, name, sha256}], player_names, comment }
thumbnail.png      240×180 (renderer capture at save time; GZDoom `savepicwidth/height` style)
globals.bin        game-wide state: players' persistent data between maps, ACS world/global vars, statistics, RNG seeds, hub list, visited maps
world.bin          current World (postcard), incl. ACS state, sound sequence thinkers, HUD messages
hubs/<MAP>.bin     snapshots of other maps in the current hub
modules/<id>.bin   per play-module instance snapshot (memory + globals + optional on_save blob)
render.bin         decals (renderer-owned), sky scroll state, automap marks/seen lines
audio.bin          looping/ambient sounds & music position
```

### 3.2 Rules

- Save only between tics (`G_DoSaveGame` runs from `GameAction`).
- Serialization uses IDs (`02 §6`). `Definitions` are not saved; the load checks `defs_hash` and module identities. On mismatch: refuse with a dialog listing differences; `save_force_load` allows trying (only host-side state for mismatched modules, see `12 §10`).
- `format_version` is a single integer bumped on any breaking change; keep migration functions for previous versions after 1.0.
- Autosaves (`Autosave` special, `autosave` on level start with `disableautosave` MAPINFO, `autosavecount`, `autosavenum` rotation), quicksave (`quicksaverotation`, `quicksaverotationcount`), `save` / `load` commands, savegame directory per game (`save_dir`), names `room-save-NN.fsave`, `auto-NN`, `quick-NN`.
- After load: re-link actors (blockmap, sectors), rebuild render interpolation state (no interpolation on first frame), restore sounds/music, run `WorldLoaded(is_save_game=true)` events. ACS scripts simply resume from their saved state; no script type fires on load.
- GZDoom saves (`.zds`) are **not** loadable (different internal model) — non-goal.

### 3.3 Hub snapshots

`World::snapshot_for_hub()` serializes the World minus players (players travel). Stored in memory during play and written into `hubs/` on save. `Teleport_NewMap` within a hub restores the target map snapshot if present and runs `RETURN` scripts.

## 4. Tests

- Netplay: in-process simulation of N peers over a lossy/latency-injecting fake transport; random inputs for 10 000 tics; assert identical checksums and no stalls beyond expected.
- Cross-platform determinism: CI matrix records a demo on Linux x86-64 and verifies on macOS aarch64 and wasm32 (Node + wasmi backend).
- Save/load fuzz: at random tics, save → load → continue; checksum of continued run must equal uninterrupted run (strongest test of complete serialization).
- Demo verify on Freedoom-based goldens.

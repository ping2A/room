# Room

Pure-Rust, library-first re-implementation of the ZDoom/GZDoom engine family.

- **wgpu** rendering on desktop (Vulkan / Metal / DX12) and in the browser (WebGPU, WebGL2 fallback)
- **WebAssembly** modules instead of ZScript
- Same content GZDoom loads: IWADs, PWADs, PK3s, UDMF, DECORATE, ACS, MAPINFO, …
- Crates you can reuse outside the full game (maps, VFS, sim, renderer, …)

Agents and contributors: start with [`AGENTS.md`](AGENTS.md).

| Doc | Purpose |
|-----|---------|
| [`docs/00-overview.md`](docs/00-overview.md) | Goals, non-goals, feature matrix |
| [`docs/20-roadmap-testing.md`](docs/20-roadmap-testing.md) | Milestones and acceptance tests |
| [`docs/21-milestone-progress.md`](docs/21-milestone-progress.md) | Living checklist (engine **M14**; server **S1–S5** done) |
| [`docs/22-room-server.md`](docs/22-room-server.md) | Multi-room web server |
| [`crates/room-server/README.md`](crates/room-server/README.md) | Server CLI, lobby, admin, ICE/TURN |

## Status

Engine milestones through **M13** are done; active work is **M14** (1.0 hardening).
Heretic/Hexen (**M8**) and Strife (**M9**) are deferred until IWADs are available.
The web multiplayer server track (**S1–S5**) is complete.

## Prerequisites

- Rust stable + `wasm32-unknown-unknown` for the browser build:
  `rustup target add wasm32-unknown-unknown`
- An IWAD for real play (not committed). Typical paths:
  - Doom 1: `testdata/doom/doom1.wad`
  - Freedoom: `./scripts/fetch-freedoom.sh` → `testdata/freedoom/`
- Optional SoftFont music: `./scripts/fetch-soundfont.sh`

## Play — desktop

```bash
# Doom 1 E1M1 (needs testdata/doom/doom1.wad)
./scripts/play-doom1.sh
ROOM_TITLE=1 ./scripts/play-doom1.sh          # title menu
# Force OPL (GENMIDI) instead of SoftFont:
cargo run -p room-app --release -- \
  --iwad testdata/doom/doom1.wad --map E1M1 --play --mididevice opl

# Freedoom
./scripts/fetch-freedoom.sh
./scripts/verify-freedoom-demos.sh            # headless demos
./scripts/play-combat-demo.sh                 # arena: WASD / mouse / Ctrl / 1–7
cargo run -p room-app --release -- \
  --iwad testdata/freedoom/freedoom2.wad --map MAP01 --play

# 2-player coop (udp default; also tcp / tcp-server; optional Noise)
./scripts/play-doom1-coop.sh
ROOM_TRANSPORT=tcp-server ROOM_NOISE=1 ./scripts/play-doom1-coop.sh
```

## Play — browser & multiplayer

### Single-player in the browser

```bash
./scripts/play-web.sh
# Serves target/room-web (auto-loads doom1.wad / freedoom if present).
# Click the canvas for mouse look; press a key to unlock audio.
```

### Multi-room server (recommended for web coop)

```bash
./scripts/room-server.sh start
./scripts/room-server.sh status
./scripts/room-server.sh stop
```

| URL | What |
|-----|------|
| http://127.0.0.1:8765/ | Public lobby — create / join rooms |
| http://127.0.0.1:8765/play/ | Game client (same origin, built `room-web`) |
| http://127.0.0.1:8766/admin | Operator console |

From the lobby: **Play** opens the game in this tab; use a second browser/profile
(or **New tab**) for another seat. Lockstep starts when every seat has joined and
is Ready. Optional built-in STUN/TURN (Lightcall) is used for WebRTC voice when
enabled on the server.

One-shot helper (build client, start server, print URLs):

```bash
./scripts/play-web-coop.sh --v2 --no-open
```

### Browser controls (coop)

| Input | Action |
|-------|--------|
| WASD / arrows | Move / turn |
| Mouse (click canvas) | Look (pointer lock) |
| Ctrl / LMB | Attack |
| E / Space | Use |
| 1–7 | Weapons |
| **V** (hold) | Push-to-talk (WebRTC) |
| Top-left **Net** panel | Lockstep wait / rx·tx / held keys (debug) |

Keep both browser tabs focused — background tabs throttle `requestAnimationFrame`
and stall lockstep for everyone.

## Develop

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo build --target wasm32-unknown-unknown -p room-web --features browser
```

Licence: **GPL-3.0-or-later**. Upstream behaviour reference:
[GZDoom](https://github.com/ZDoom/gzdoom), [ZDoom wiki](https://zdoom.org/wiki/Main_Page).

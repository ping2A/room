# Room

A pure-Rust, library-first re-implementation of the ZDoom/GZDoom engine family:
wgpu rendering (desktop + web), and WebAssembly modules in place of ZScript.

Start with [`AGENTS.md`](AGENTS.md). Overview: [`docs/00-overview.md`](docs/00-overview.md).
Build order: [`docs/20-roadmap-testing.md`](docs/20-roadmap-testing.md)
(active: **M11 → M12**; M8–M10/M13–M14 deferred — see [`docs/21-milestone-progress.md`](docs/21-milestone-progress.md)).

```text
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo build --target wasm32-unknown-unknown -p room-web
```

## Play

```bash
# Doom 1 (place doom1.wad / doom.wad at testdata/doom/doom1.wad — not committed):
./scripts/fetch-soundfont.sh   # once — SoftFont music (optional but recommended)
./scripts/play-doom1.sh
# Title menu instead of jumping into E1M1:
ROOM_TITLE=1 ./scripts/play-doom1.sh
# Force OPL (GENMIDI) instead of SoftFont:
cargo run -p room-app --release -- --iwad testdata/doom/doom1.wad --map E1M1 --play --mididevice opl

./scripts/fetch-freedoom.sh
# Headless demos (MAP01 walk/fire + committed episode bundles):
./scripts/verify-freedoom-demos.sh
# Combat arena (monsters + weapons; WASD / mouse / Ctrl fire / 1–7 guns):
./scripts/play-combat-demo.sh
# Interactive Freedoom MAP01:
cargo run -p room-app --release -- \
  --iwad testdata/freedoom/freedoom2.wad --map MAP01 --play

# 2-player coop (Doom E1M1). Transports: udp (default), tcp, tcp-server. Optional Noise:
./scripts/play-doom1-coop.sh
ROOM_TRANSPORT=tcp-server ROOM_NOISE=1 ./scripts/play-doom1-coop.sh

# Browser shell (wasm32 + local server; IWAD auto-load when present):
./scripts/play-web.sh
# PORT=9090 ROOM_WEB_NO_OPEN=1 ./scripts/play-web.sh
# Browser 2-player coop (WebSocket relay + two tabs):
./scripts/play-web-coop.sh
```

Upstream references: [GZDoom](https://github.com/ZDoom/gzdoom), [ZDoom wiki](https://zdoom.org/wiki/Main_Page).

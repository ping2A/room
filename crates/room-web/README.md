# room-web

Browser shell (`wasm-bindgen` + `web-sys` + wgpu).

```bash
# One-shot: build wasm, bindgen, serve on http://127.0.0.1:8080/
./scripts/play-web.sh

cargo test -p room-web
cargo build -p room-web --target wasm32-unknown-unknown --features browser
```

With `browser` on `wasm32`, startup finds `#room` and runs a wgpu shell
(WebGPU / WebGL2). Without an IWAD it shows the embedded UDMF demo room; with
Doom/Freedoom IWAD bytes it starts a Headless play session (E1M1 / MAP01).

```bash
# Copies testdata/doom/doom1.wad into the serve dir when present, then opens the browser.
./scripts/play-web.sh
# Or: ROOM_WEB_IWAD=/path/to/doom1.wad ./scripts/play-web.sh
```

Controls: click/key to unlock audio + mouse look; WASD move, E/Space use, LMB/Ctrl attack, 1–7 weapons.

### WebSocket coop

```bash
# Relay + two browser tabs (same IWAD fingerprint required)
./scripts/play-web-coop.sh
# Or manually:
#   cargo run -p room-ws -- --bind 127.0.0.1:8765 --players 2
#   ./scripts/play-web.sh   # then open:
#   http://127.0.0.1:8080/?ws=ws://127.0.0.1:8765&players=2&name=p1
#   http://127.0.0.1:8080/?ws=ws://127.0.0.1:8765&players=2&name=p2
```

Query params: `ws`, `players` (2..=4), `name`, `map`, `skill`, `seed` (host seed).

JS embed API (M13 + play):

- `roomVersion()` — package version
- `roomUrlParam(name)` — `?map=` / `?iwad=` / `?ws=` query helpers
- `roomStartIwad(Uint8Array)` — mount IWAD + start play (or join `?ws=` lobby)
- `roomOpfsSet` / `roomOpfsGet` — durable key/value for the host page
  (`localStorage` today; OPFS-ready naming)

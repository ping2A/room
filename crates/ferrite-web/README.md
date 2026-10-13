# ferrite-web

Browser shell (`wasm-bindgen` + `web-sys` + wgpu).

```bash
# One-shot: build wasm, bindgen, serve on http://127.0.0.1:8080/
./scripts/play-web.sh

cargo test -p ferrite-web
cargo build -p ferrite-web --target wasm32-unknown-unknown --features browser
```

With `browser` on `wasm32`, startup finds `#ferrite` and runs a wgpu shell
(WebGPU / WebGL2). Without an IWAD it shows the embedded UDMF demo room; with
Doom/Freedoom IWAD bytes it starts a Headless play session (E1M1 / MAP01).

```bash
# Copies testdata/doom/doom1.wad into the serve dir when present, then opens the browser.
./scripts/play-web.sh
# Or: FERRITE_WEB_IWAD=/path/to/doom1.wad ./scripts/play-web.sh
```

Controls: click/key to unlock audio + mouse look; WASD move, E/Space use, LMB/Ctrl attack, 1–7 weapons.

JS embed API (M13 + play):

- `ferriteVersion()` — package version
- `ferriteUrlParam(name)` — `?map=` / `?iwad=` query helpers
- `ferriteStartIwad(Uint8Array)` — mount IWAD + start play (also auto-fetches `/doom1.wad`)
- `ferriteOpfsSet` / `ferriteOpfsGet` — durable key/value for the host page
  (`localStorage` today; OPFS-ready naming)

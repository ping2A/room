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

Controls: any key/click/touch unlocks audio (Web Audio autoplay) and click locks the
mouse; WASD move, E/Space use, LMB/Ctrl attack, 1–7 weapons.

Audio: SFX + OPL/`GENMIDI` music via Web Audio (`AudioWorklet` playback with
`ScriptProcessor` fallback). Context resumes on tab focus; ACS `SetMusic` is honoured.
The worklet reports its fill level every ~10 ms and the main thread tops it up to an
adaptive 25–160 ms target (grows on underrun, shrinks when stable); inspect it with
`roomAudioStats()` in the console.

### Editor (`editor.html`)

Map + lump editor running the real engine in the 3D pane. `editor.js` owns the map
document; `editor-tools.js` adds:

- **Live test panel** while playing — god / noclip / freeze / give / kill monsters /
  summon / set HP & armor (`roomEditorCheat`, `roomEditorPlayerState`).
- **Clipboard & transforms** — Ctrl+C / X / V (at mouse) / D, R (Shift+R) rotate 90°,
  H (Shift+H) mirror.
- **Map checker** (F8) — starts, refs, unclosed sectors, missing/unknown textures,
  tags, flags; one-click and "fix all safe" repairs.
- **Auto-align** (J) — x offsets along connected walls sharing a texture.
- **Texture composer** — edit TEXTURE1/TEXTURE2 + PNAMES, apply live.
- `window.roomEditor` — console hook (`state.doc`, `commit()`, `undo()`, …).

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

### Live voice (coop)

With `?ws=`, `voice.js` (Lightcall AudioWorklet PCM) multiplexes mic audio on the
same WebSocket as lockstep (`MSG_VOICE`). **Hold `V`** (or the on-screen pad) for
push-to-talk; speaker toggle is separate. The voice chrome never takes keyboard
focus so WASD / pointer-lock keep working. Frames are out-of-band (not lockstep).

JS embed API (M13 + play):

- `roomVersion()` — package version
- `roomUrlParam(name)` — `?map=` / `?iwad=` / `?ws=` query helpers
- `roomStartIwad(Uint8Array)` — mount IWAD + start play (or join `?ws=` lobby)
- `roomOpfsSet` / `roomOpfsGet` — durable key/value for the host page
  (`localStorage` today; OPFS-ready naming)
- `roomVoiceSend` / `roomVoicePoll` / `roomVoiceActive` — coop voice bridge

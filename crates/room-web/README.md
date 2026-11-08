# room-web

Browser shell (`wasm-bindgen` + `web-sys` + wgpu).

```bash
# One-shot: build wasm, bindgen, serve on http://127.0.0.1:8080/
./scripts/play-web.sh

# Standalone site for upload (mods included; optional --zip / --iwad freedoom)
./scripts/package-web.sh --zip

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
mouse; WASD move, E/Space use, LMB/Ctrl attack, 1–7 weapons, wheel to cycle.

Phones / tablets (`touch.js`, on for coarse pointers or at the first touch;
`?touch=1` forces it, `?touch=0` turns it off): left thumb is a floating stick
(forward / back / strafe, push to the rim to run), right thumb drags to look,
**FIRE** (drag on it to aim while firing), **USE**, ‹ › weapons, **TAB** scores,
⛶ fullscreen (+ landscape lock where allowed) and ☰ for the skill / mod bar.
Look speed: `localStorage.roomTouchLook` (default `1`). The buttons feed the same
input path as the keyboard (`roomInputKey`, `roomInputLook`,
`roomInputWeaponCycle`), so solo, coop rooms and mods behave the same. Portrait
screens get a 4:3 letterboxed view and a "turn sideways" hint.

Audio: SFX + OPL/`GENMIDI` music via Web Audio (`AudioWorklet` playback with
`ScriptProcessor` fallback). Context resumes on tab focus; ACS `SetMusic` is honoured.
The worklet reports its fill level every ~10 ms and the main thread tops it up to an
adaptive 25–160 ms target (grows on underrun, shrinks when stable); inspect it with
`roomAudioStats()` in the console.

Native Duke audio can be configured by the embedding page through
`roomSetDukeAudioOptions(sound, music, talk, lockout, ambience, reverseStereo,
sfxVolume, musicVolume)`. Volumes use the Atomic 0–255 range; all channels are
enabled at full volume by default, with lockout and reverse stereo off.

The play page's top-left bar picks the **skill** (`?skill=1..5`) and a **mod**
(`?mod=survival` / `?mod=bounty` / `?mod=royale` mounts `mods/<name>.wad`;
`?mod=voxel` mounts `mods/voxel.pk3`, the Voxel Doom pack, and you still
fight the map's monsters) via `roomAddPwad`, single-player only;
changing either reloads into a new game. It hides while the mouse is captured.

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
- **3D selection overlay** — selected things are boxed in solid yellow, the
  thing under the mouse in dashed blue (`roomEditorThingRects`); things hidden
  behind walls are drawn faint.

`editor-paint.js` is the pixel editor for wall textures, flats, patches, sprites and
graphics (✎ next to texture fields, right-click a browser tile, double-click or P on a 3D surface or thing, or
"✎ Edit pixels" on a picture lump). It paints in the IWAD palette with pencil / eraser /
fill / line / rect / picker, brush size, mouse-wheel zoom around the cursor, grid, a 3×3 tiling preview, flip / rotate /
half-shift, resize, offsets, PNG import (palette-quantized) and export, **Camera face…**
(webcam → circular paste onto an enemy sprite, or full-frame replace), plus undo/redo.
From a selected thing, the inspector also has **Camera face…**.
Every edit shows in the running view while you paint: textures and flats on the
walls and floors, patches by recomposing every texture that uses them, sprites on
actors (size and offsets included), and HUD / status-bar / menu graphics in play
mode. Resized wall textures wait for Save, which restarts the level. Edits are exported as `TX_` / `FF_` / `PP_` /
`SS_` override lumps in the PWAD.

Sprites get an **Animation** panel (`roomEditorSpriteAnims`): every state
sequence of the actor (Spawn, See, Missile, Pain, Death, …) plays with its real
tic timing at any of the 8 rotations, aligned on the sprite offsets. Next to it
are the sequence's frame strip and an "All images" grid of every lump the actor
uses. Click a frame to edit it in the same tab.
- **Scripts** tab (`editor-scripts.js`, `src/browser/editor/scripts.rs`) — WASM game
  mods: write AssemblyScript (`as/room.ts` API) or WAT, both compiled in the
  browser, or import `.wasm`, in a syntax-highlighted code editor; edit the
  manifest, live mod log. Example buttons
  mount `mods/survival.wad` (Rust) or `mods/bounty.wad` (AssemblyScript); "Copy
  into editor for editing" imports a module whose WAD ships its `S_*` source.
  Ctrl+S during a play test reloads the mods live. Saved as `ROOMMOD` + module
  (+ `S_*` source) lumps in the exported PWAD. Guide and API: docs/12 §16. The AssemblyScript
  compiler is vendored by `scripts/fetch-assemblyscript.sh` (run automatically by
  `play-web.sh` when npm exists; otherwise loaded from jsDelivr).
- **Import a PK3**: **📦 Open PK3…** in the toolbar (or drop the file on the
  editor) opens a `.pk3` / `.pk7` / `.zip` / `.ipk3`, with or without an IWAD.
  With an IWAD loaded the pack is mounted on top of it. Without one, the editor
  first tries the served IWAD (`doom1.wad`, `freedoom1.wad`, `freedoom2.wad`);
  if none is found, the pack opens on its own and the editor warns about it.
  The pack becomes the working archive: the Lumps tab opens on its tree, its
  maps (including `maps/*.wad`) are listed and open, its ZScript files can be
  edited and checked, and **Export PK3** rebuilds it by default. A pack with
  one wrapper folder (`MyMod/zscript.txt`, …) is unwrapped. Broken DECORATE or
  ZScript no longer stops the import; the error is shown instead.
- **PK3 packs** (`src/pk3_edit.rs`): the Lumps tab shows PK3 / PK7 / folder
  packs as a folder tree. Text files are edited by full path (saved to
  `editor-overrides.pk3`, mounted last); **+ New file in pack…** adds one.
  `maps/*.wad` inside a pack opens like any map. **Export PK3** rebuilds the
  opened pack (or a new one) with every edit: files, lumps, maps as
  `maps/<name>.wad`, and actor edits as `decorate.room-edits`. **Export WAD**
  still writes a PWAD.
- **Text editor**: syntax colors for ZScript, DECORATE, ACS and MAPINFO
  (`highlight` / `textLang` in `editor-scripts.js`), covering States frames,
  Default flags and properties, and labels. Use the `#include` buttons or
  Ctrl/⌘+click an `#include` line to open the file. **Check ZScript**
  (`roomEditorCheckZscript`, also run on save) lists errors; click one to jump to
  its line. Tests: `node --test crates/room-web/tests/editor_highlight.test.mjs`.
- **🔊 / 🔇** in the toolbar mutes game sound and music (remembered).
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

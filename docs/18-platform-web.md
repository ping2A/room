# 18 — Platform shells: desktop, web, headless

Crates: `room-app` (native binary), `room-web` (wasm32 binary), `room-game::Headless`.

## 1. Platform abstraction

`room-game` depends only on these traits, implemented by each shell:

```rust
pub trait Platform {
    fn config_store(&self) -> &dyn KvStore;              // room.toml text, autoexec, history
    fn save_store(&self) -> &dyn FileStore;              // save games, screenshots, demos, node cache
    fn resource_sources(&self) -> &dyn ResourceProvider; // IWAD search, user files → ContainerSource
    fn clipboard(&self) -> Option<&dyn Clipboard>;
    fn set_mouse_capture(&self, on: bool);
    fn set_title(&self, s: &str);
    fn open_url(&self, url: &str);
    fn now(&self) -> Instant;                             // monotonic; web: performance.now()
    fn request_quit(&self);
}
```

## 2. Native desktop (`room-app`)

| Concern | Choice |
|---|---|
| Window/events | `winit` (latest), raw mouse via `DeviceEvent::MouseMotion` while captured; cursor grab + hide |
| GPU | `wgpu` surface from the window; present modes: `vid_vsync` → Fifo / Mailbox / Immediate; `vid_maxfps` limiter (sleep + spin) |
| Audio | `cpal` default device (WASAPI/CoreAudio/ALSA/PipeWire via ALSA or PulseAudio backends); device selection by name |
| Gamepad | `gilrs` (feature `gamepad`) |
| Paths | `directories` crate: config (`~/.config/room`, `%APPDATA%\room`, `~/Library/Application Support/room`), data (saves, screenshots, autoload/), cache (node cache, compiled WASM cache) |
| Clipboard | `arboard` |
| File dialogs | `rfd` (for "Add IWAD…" in the launcher) |
| Fullscreen | borderless fullscreen default; exclusive fullscreen `vid_fullscreen 2` (S) |
| HiDPI | render scale independent of window scale factor; UI scale auto from DPI |
| Crash reporting | panic hook writing `crash-<time>.txt` with backtrace, engine version, load order, last 200 console lines; shows a native message box (`rfd::MessageDialog`) |
| Launcher | in-engine IWAD/mod picker (when `queryiwad` or multiple IWADs): list detected IWADs, drag & drop mods, autoload toggles, remembers last selection |

Platforms: Windows 10+ (x86-64, aarch64), macOS 11+ (aarch64, x86-64), Linux (x86-64, aarch64; X11 & Wayland). Packaging: zip/tarball with binary + `room.pk3` (also embedded) + optional extras; AppImage/Flatpak manifests (S).

## 3. Web (`room-web`)

### 3.1 Build & deploy

- Target `wasm32-unknown-unknown`, `wasm-bindgen` + `web-sys` + `js-sys`. Build via a small `xtask` (`cargo xtask web`) that runs `wasm-bindgen` CLI, copies `index.html`, `room.js` loader, `audio-worklet.js`, and `room.pk3`.
- Size budget: engine `.wasm` ≤ 12 MiB uncompressed release (`opt-level = "z"` for the web profile is *not* used; use `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`), served with brotli.
- Hosting requirements documented: for threads/`SharedArrayBuffer` the page must be cross-origin isolated (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`). The engine must also work **without** isolation (single-threaded audio path).

### 3.2 Graphics

- `wgpu` with `Backends::BROWSER_WEBGPU | Backends::GL`: try WebGPU first, fall back to WebGL2 (downlevel tier, `13 §1`).
- Canvas sizing to CSS size × `devicePixelRatio` (cap at 2 by default for performance), resize observer.
- Main loop on `requestAnimationFrame`; when the tab is hidden, the sim keeps running only in netplay (via a `setTimeout`-driven tick fallback); single player pauses.

### 3.3 Input

- Pointer Lock API for mouse look (requested on canvas click), `movementX/Y`; keyboard via `KeyboardEvent.code` mapped to `KEY_*`; prevent default for game keys while captured; Gamepad API polled each frame; Fullscreen API (`F11`/menu option).
- **Touch (phones / tablets)** — `crates/room-web/touch.js`, enabled for `(pointer: coarse)` or at the first `touchstart` (`?touch=1|0` overrides). Pointer Events, multi-touch: floating left stick → `KeyW/S/A/D` with a 0.28 dead zone, `ShiftLeft` (run) past 85 % deflection; right-half drag and drag-on-FIRE → look (0.3 mouse counts per CSS px × `localStorage.roomTouchLook`, fractional remainder carried; pitch × 0.6); FIRE / USE / scores hold buttons, ‹ › weapon cycle, fullscreen + `screen.orientation.lock("landscape")` where allowed, ☰ shows the skill / mod bar. Everything goes through `roomInputKey` / `roomInputLook` / `roomInputWeaponCycle` into the same input hub as the keyboard, so netplay tic commands are identical. Keys are released on `blur` / `visibilitychange` / `pointercancel`. The page disables pinch / double-tap zoom, pull-to-refresh and long-press callouts, respects `env(safe-area-inset-*)`, and letterboxes the view at ≥ 4:3 in portrait. Audio unlocks on `touchend` / `click` too (iOS Safari).

### 3.4 Audio

- `AudioContext` created/resumed on first user gesture (show "Click to start").
- Preferred: `AudioWorkletNode` running a tiny JS processor that reads PCM from a `SharedArrayBuffer` ring filled by the engine's mixer on the main thread (or a Worker) — requires isolation.
- Fallback without isolation: engine mixes on the main thread in 2048-frame blocks and posts `Float32Array`s to the worklet via `port.postMessage` (higher latency, acceptable).
- Music synthesis (SoundFont/OPL/modules) runs in the same mixer path; budget-limit SoundFont polyphony on web (`snd_web_maxvoices`).

### 3.5 Files and storage

- **Import**: drag & drop or `<input type=file multiple>` for IWADs/PWADs/PK3s; File System Access API directory picker where available (Chromium) to "mount" a local Doom folder (re-permission on next visit).
- **Persistence**: imported files copied into **OPFS** (Origin Private File System) under `/library/`, with a manifest (name, size, md5, detected IWAD kind). Config, saves, demos, screenshots, node cache also in OPFS. Fallback to IndexedDB when OPFS is unavailable. Call `navigator.storage.persist()`.
- **Access model**: v1 reads containers fully into memory (`MemorySource`) at mount time; warn when total > 1 GiB (wasm32 4 GiB address space limit). S: run the engine in a Worker with `OffscreenCanvas` and use `FileSystemSyncAccessHandle` for random-access reads without loading everything.
- Remote content: `?file=<url>` parameters fetch mods over HTTPS (CORS required); integrity by optional `#sha256=`. Optional "Get Freedoom" button that downloads Freedoom (BSD-licensed) from a deploy-configured URL. **Never** bundle commercial IWADs.
- Downloads (screenshots, demos, saves export): via `Blob` + anchor download.

### 3.6 Threads and scripting

- No rayon on web in 1.0 (all parallel code paths have sequential fallbacks behind `cfg(target_arch = "wasm32")` or a runtime flag). S: `wasm-bindgen-rayon` when cross-origin isolated.
- WASM mods run on the `wasmi` backend inside the engine (`12 §4`). S: `web-native` backend.

### 3.7 URL & embedding API

`index.html?iwad=freedoom2.wad&file=mod.pk3&warp=1&skill=3&+sv_cheats=1` maps to command-line semantics. A small JS API (`window.room.start({ args, files })`) allows embedding Room in other pages (e.g. mod showcase sites).

## 4. Headless (`room-game::Headless`)

No window/GPU/audio: `NullAudio`, no renderer, in-memory stores. Used by tests, demo verification, bots/AI research, and a future dedicated packet-server relay (`room-cli serve` — S). Must compile on `wasm32-unknown-unknown` too (used by the determinism CI on Node).

## 5. Tests

- `wasm-bindgen-test` headless browser tests (Chrome + Firefox via webdriver) for: start-up with Freedoom from fixture URL, render one frame (WebGPU and WebGL2 paths), audio context creation, OPFS read/write round-trip.
- Native smoke test in CI on all three OSes: `room-app --headless-smoke` runs 350 tics of a demo with an offscreen surface.

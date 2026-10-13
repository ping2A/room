# Fresh-machine UX checklist (M13)

Run on a clean profile after `scripts/package.sh all` (or `cargo build -p room-app --release`).

## Native (Win / macOS / Linux)

- [ ] `room --launcher` lists IWADs (or explains how to fetch Freedoom)
- [ ] `room --combat-demo` enters the arena with mouse look on
- [ ] WASD: W forward, S back, A strafe left, D strafe right (facing north, D moves east)
- [ ] Mouse: move right → look right; Ctrl / F / LMB fires
- [ ] Esc pause → Options toggles `invertmouse` / `cl_run`; Write config creates `room.ini`
- [ ] `--import-ini` / auto-import of `gzdoom.ini` when present
- [ ] Panic writes a crash log under the platform log directory

## Web (Chrome / Firefox / Safari)

- [ ] `room-web` bundle loads `#room` canvas (WebGPU or WebGL2)
- [ ] `roomUrlParam("map")` reads `?map=`
- [ ] `roomOpfsSet` / `roomOpfsGet` round-trip a string (localStorage-backed)

## Notes

Safari WebGPU may fall back to WebGL2 via wgpu. Commercial IWAD paths are local-only.

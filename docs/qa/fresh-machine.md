# Fresh-machine UX checklist (M13)

Run on a clean profile after `scripts/package.sh all` (or `cargo build -p ferrite-app --release`).

## Native (Win / macOS / Linux)

- [ ] `ferrite --launcher` lists IWADs (or explains how to fetch Freedoom)
- [ ] `ferrite --combat-demo` enters the arena with mouse look on
- [ ] WASD: W forward, S back, A strafe left, D strafe right (facing north, D moves east)
- [ ] Mouse: move right → look right; Ctrl / F / LMB fires
- [ ] Esc pause → Options toggles `invertmouse` / `cl_run`; Write config creates `ferrite.ini`
- [ ] `--import-ini` / auto-import of `gzdoom.ini` when present
- [ ] Panic writes a crash log under the platform log directory

## Web (Chrome / Firefox / Safari)

- [ ] `ferrite-web` bundle loads `#ferrite` canvas (WebGPU or WebGL2)
- [ ] `ferriteUrlParam("map")` reads `?map=`
- [ ] `ferriteOpfsSet` / `ferriteOpfsGet` round-trip a string (localStorage-backed)

## Notes

Safari WebGPU may fall back to WebGL2 via wgpu. Commercial IWAD paths are local-only.

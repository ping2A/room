# ferrite-app

Native desktop binary (`ferrite`): winit + wgpu map viewer (M4) and Headless play shell (M5).

```
cargo run -p ferrite-app
# or with an IWAD/PWAD:
cargo run -p ferrite-app -- --iwad /path/to/freedoom1.wad --map MAP01
# Playsim (enters `--map` immediately; WASD → TicCmd, camera follows player):
cargo run -p ferrite-app --release -- --iwad testdata/freedoom/freedoom2.wad --map MAP01 --play
# Combat arena demo (zombies/imps/shotgunners + full arsenal):
./scripts/play-combat-demo.sh
# or:
cargo run -p ferrite-app --release -- --combat-demo
# Title / main menu first:
cargo run -p ferrite-app -- --iwad … --play --title
# 2-player UDP coop on Doom E1M1 (two windows):
./scripts/play-doom1-coop.sh
# or manually:
cargo run -p ferrite-app --release -- --iwad testdata/doom/doom1.wad --map E1M1 --host 5029 --players 2 --play
cargo run -p ferrite-app --release -- --iwad testdata/doom/doom1.wad --map E1M1 --join 127.0.0.1:5029 --play
```

Viewer controls: WASD move, Space/C up/down, Shift sprint, right-mouse look, Tab automap, Esc quit.

Play mode (`--play`): starts the requested map (use `--title` for main menu → New Game → skill). WASD move, ←→/Q turn, mouse look (`L` toggles; RMB does not release grab), Shift run, E use, Ctrl / LMB attack, keys `1`–`7` weapons, Tab automap, P / Esc pause (Resume / Options / End Game / Quit), menus: ↑↓ or W/S, Enter, mouse hover+click, `` ` `` console (`map MAP02`, cheats), F5 fullscreen HUD, F6 god, F7 noclip, F8 give all, F9 kill monsters.

`--combat-demo`: built-in arena with monsters and weapon/ammo pickups (implies `--play --give-all`). Needs Freedoom IWAD textures (`--iwad` or `testdata/freedoom/freedoom2.wad`). `--skill 1..=5` selects difficulty.

Embedded `ferrite.pk3` supplies DECORATE/SNDINFO; SFX/music → cpal. Music: `--soundfont path.sf2` (SoftFont) or IWAD `GENMIDI` via `--mididevice opl` / `auto` fallback.

With `--iwad`, TEXTUREX/PNAMES materials are uploaded and drawn per range. Doom status bar + optional automap overlay use `ferrite-ui` → canvas. Window title shows rolling fps.

### M4 manual checklist
- [ ] `cargo run -p ferrite-app -- --iwad testdata/freedoom/freedoom2.wad --map MAP01`
- [ ] Title shows ≥30 fps on a mid-range laptop
- [ ] WASD + right-mouse look explores the map without crashes

### M5 play checklist
- [ ] `cargo run -p ferrite-app -- --play` starts without panic
- [ ] WASD moves the player pawn; camera follows
- [ ] Ctrl attack / E use reach the Headless ticcmd path

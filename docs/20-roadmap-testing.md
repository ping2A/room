# 20 — Roadmap, acceptance tests, test assets, CI

Work strictly milestone by milestone. A milestone is done when **all** its
acceptance criteria pass in CI. Inside a milestone, follow the listed order.

Track execution status in [`21-milestone-progress.md`](21-milestone-progress.md)
(checklists, %, log). Update that file whenever milestone work lands.

**Active order (override):** M0–M7 + M10–**M13** complete. Active → **M14** (1.0
hardening). **M8** when Heretic/Hexen IWADs available; M9 deferred (see `21`).

## 1. Milestones

### M0 — Workspace and core
- Workspace skeleton with all crates (empty `lib.rs` + README), lints, CI pipeline (§4).
- `ferrite-core`: `Name`/`NameTable`, typed IDs, generational arena, `DVec2/3`, `Angle`, `Fixed`, `BAM`, `dmath` (deterministic sin/cos/tan/atan2/sqrt-wrappers/pow/exp/log), `conv` helpers, SFMT-607 + `FRandom`-compatible named RNG set, vanilla `rndtable`.
- **Accept:** SFMT reference vectors; `dmath` results bit-identical on x86-64, aarch64, wasm32 (CI job compares hashes of 1M sampled outputs); arena property tests.

### M1 — Resources
- `ferrite-vfs` (WAD, ZIP incl. deflate/bzip2/lzma, 7z, directories, nested WADs, filters, single-root stripping), IWAD detection with IWADINFO, load order & autoload.
- `ferrite-cli wad ls|extract|info`.
- **Accept:** VFS unit/integration tests (`03 §9`); detection of every IWAD kind using *synthetic* WADs that contain only the `MustContain` lump names; fuzz targets run 60 s in CI without crash.

### M2 — Formats and definitions bootstrap
- `ferrite-formats`: patches, flats, PNG (+grAb/alPh), JPEG, TGA/PCX/BMP, PLAYPAL/COLORMAP, DMX/WAV/OGG/FLAC/MP3 decoding, MUS→MIDI, fonts.
- `ferrite-sc` scanner; `ferrite-defs` phases 0–6 and 9: GAMEINFO, LANGUAGE, CVARINFO, TEXTURES/TEXTUREx/PNAMES, ANIMATED/ANIMDEFS/SWITCHES, FONTDEFS, MAPINFO (all syntaxes) + UMAPINFO, SNDINFO, SNDSEQ.
- `ferrite-cli defs dump`.
- **Accept:** golden dumps (insta) for Freedoom Phase 1/2 definitions; MAPINFO parser accepts every MAPINFO in a corpus of test files (§3) with zero errors.

### M3 — Maps
- Doom/Hexen/UDMF loaders, validation, XLAT translation, node loading (all formats), `ferrite-nodebuild`, blockmap build, reject, slopes, 3D floor & portal setup data, polyobject setup.
- `ferrite-testkit` crate: a UDMF **map builder DSL** for tests (rooms, doors, slopes, portals in a few lines of Rust).
- `ferrite-cli map info|render2d|nodebuild` (2D SVG/PNG of lines/subsectors).
- **Accept:** all Freedoom maps load; node builder convexity/closure checks; point-in-sector parity vs original nodes on a grid; UDMF lexer ≥ 100 MB/s benchmark.

### M4 — Renderer foundation + shells
- `ferrite-render`: device setup (full + downlevel tiers), texture manager, BSP walk + clipper, walls/flats/sprites (static), classic sky, sector lighting modes incl. `DoomSoftware` and `ClassicPalette`, fog, 2D canvas, fonts, screenshots, offscreen API.
- `ferrite-app` (window, free-fly camera "map viewer" mode), `ferrite-web` (same viewer in browser, WebGPU + WebGL2).
- **Accept:** golden images for test maps; viewer runs Freedoom MAP01 at target fps on desktop and in Chrome/Firefox.

### M5 — Doom playsim
- `ferrite-decorate` (full parser; built-in Doom actors in `ferrite.pk3` as DECORATE); `ferrite-dehacked` (vanilla + BEX); `ferrite-sim` core: world, thinkers, movement/collision, sight, hitscan, damage, inventory/weapons, player, Doom AI, Doom specials via XLAT, sector specials, movers, lights, switches, teleports, level flow, intermission/finale, title loop.
- `ferrite-audio` SFX + `ferrite-music` (MUS/MIDI → SoundFont; OPL3), `ferrite-input`, `ferrite-console` (core commands, CVars, binds, config), `ferrite-ui` (Doom menus, Doom status bar, fullscreen HUD, automap, messages).
- Ferrite demo recording/playback + checksums.
- **Accept:** Freedoom Phase 1 & 2 fully playable start→finish (manual checklist + automated "speedrun" demos recorded by maintainers, verified in CI); cross-platform determinism job green (demo recorded on Linux verifies on macOS arm64 and wasm32).

### M6 — Boom, MBF, MBF21, vanilla strict
- Boom generalized specials & extended line/sector types, friction, pushers, scrollers, transfer heights, colormaps, TRANMAP, MBF (friends, dogs, `A_` additions), MBF21 (DeHackEd extensions, flags, groups), UMAPINFO semantics, DSDHacked, `COMPLVL`.
- Strict vanilla mode (`19 §4`) + vanilla/Boom/MBF demo playback.
- Per-map fix database (`19 §3`).
- **Accept:** public MBF21 test WADs behave as in DSDA-Doom (manual checklist); vanilla demo sync suite (§3.3) green locally with commercial IWADs (gated), Freedoom-based subset in CI.

### M7 — ZDoom feature set
- Hexen-format maps & specials, UDMF `zdoom` namespace complete, ACS VM complete, slopes/3D floors/portals/polyobjects in sim **and** renderer, full DECORATE (expressions, anonymous functions, all action functions), full MAPINFO, hubs, SBARINFO, TERRAIN, DECALDEF (+renderer decals), LOCKDEFS, KEYCONF, MENUDEF, skyboxes, camera textures, mirrors, interpolation of all movers.
- **Accept:** synthetic feature maps (testkit) for each feature; a corpus of DECORATE/ACS mods (§3.2) load without errors and pass smoke playthrough (scripted inputs, no traps/crashes, sensible end state).

### M8 — Heretic and Hexen
- Built-in actors, specials, status bars, menus, intermissions, finales, Hexen classes/hubs/puzzles, startup screen (native, S).
- **Accept:** Heretic E1 and Hexen hub 1 playthrough demos verified in CI (gated IWAD jobs run on a self-hosted runner or locally).

### M9 — Strife
- Conversations (binary + USDF), quests/log, merchants, alarms, Strife bar/popups, special actions.
- **Accept:** Strife MAP02→MAP07 gated playthrough demo.

### M10 — GZDoom rendering extras
- GLDEFS lights (all types) + clustered shading + 1D shadowmaps; brightmaps, glows, materials (normal/specular/PBR); MODELDEF (MD2/MD3/OBJ, then IQM/UE1), VOXELDEF/KVX; particles; render styles complete; post-processing (SSAO, bloom, tonemaps incl. palette, FXAA); custom GLSL shaders via naga; lightmaps (S).
- **Accept:** golden images per feature; `lights.pk3`/`brightmaps.pk3` content renders; performance targets (`13 §14`).

### M11 — WASM modules (ZScript replacement)
- ABI v1 IDL + `ferrite-abi-gen`; `ferrite-script` with `wasmi` backend first, then `wasmtime`; validation; registration; classes/states/fields/virtuals/actions; events; thinkers; ACS interop; UI modules (status bars, menus, overlays, network events); saves/hot reload; Rust SDK + macros + `cargo ferrite`; C header + example.
- **Accept:** conformance suite green on both backends with identical checksums; sample mods: (a) "FireImp" replacement, (b) custom weapon with overlays & alt-fire, (c) event-handler gameplay mod (e.g. "monsters drop health bonuses"), (d) custom fullscreen HUD, (e) custom options menu; save/load mid-game with modules; benchmark targets (`12 §14`).

### M12 — Netplay
- `ferrite-net` lockstep, UDP + WebRTC (`matchbox`), lobby/handshake, desync detection & dump tooling, `ferrite-signal`.
- **Accept:** in-process lossy-network simulation tests; 4-player coop across native + web peers for 30 minutes without desync (manual); automated 2-peer smoke test in CI over loopback.

### M13 — Product polish
- Launcher (IWAD/mod picker), gzdoom.ini import, full option menus, web library (OPFS), URL params & JS embed API, packaging, crash reporter, localization of engine strings.
- **Accept:** fresh-machine UX checklist on Windows/macOS/Linux/Chrome/Firefox/Safari (WebGL2 path).

### M14 — 1.0 hardening
- 24h fuzz campaigns on all parsers; save/load fuzz; compatibility corpus run with report; performance profiling pass; documentation (user manual, modding guide, ZScript migration guide).

### Stretch (post-1.0)
Classic software renderer · ZScript→Rust/WASM transpiler · bots · rollback netcode · Timidity/GUS, ADLMIDI/OPNMIDI, GME ports · HRTF · component-model ABI v2 · mobile (Android/iOS) shells · join-in-progress.

## 2. Testing layers

| Layer | Tooling | Where |
|---|---|---|
| Unit | `cargo test` | every crate |
| Property | `proptest` | math, arena, parsers round-trips, node builder invariants |
| Fuzz | `cargo fuzz` (libFuzzer) — run natively in CI for short durations | `fuzz/` |
| Snapshot | `insta` (RON/YAML dumps) | definitions, translations, map dumps |
| Scenario | `ferrite-testkit` map DSL + headless world + assertions | sim, specials, actions, ACS, modules |
| Determinism | demo record/verify across targets | CI matrix |
| Golden image | offscreen wgpu on lavapipe (Linux CI), SSIM compare | renderer, UI |
| Browser | `wasm-bindgen-test` + headless Chrome/Firefox | web shell |
| Manual | checklists in `docs/qa/` | playthroughs, UX |

### 2.1 Scenario test example (to set the style)

```rust
#[test]
fn door_raise_opens_and_closes() {
    let map = MapBuilder::new()
        .room("a", rect(0, 0, 256, 256), floor(0), ceil(128))
        .room("b", rect(256, 0, 512, 256), floor(0), ceil(128))
        .door_between("a", "b", DoorSpec { special: spec!(Door_Raise, 0, 16, 150), activation: SPAC_Use, tag: 0 })
        .player_start(64, 128, 0)
        .build_udmf("zdoom");
    let mut g = Headless::with_test_defs().start_custom_map(map, Skill::new(3)).unwrap();
    g.face_line_and_use(line!("door"));
    g.run_tics(40);
    assert!(g.world().sector(sector!("door")).ceiling_height() >= 120.0);
    g.run_tics(150 + 40);
    assert_eq!(g.world().sector(sector!("door")).ceiling_height(), 0.0);
}
```

## 3. Test assets

### 3.1 Always available (committable)
- **Freedoom** Phase 1, Phase 2, FreeDM (BSD-3-Clause) — downloaded by a pinned-version script into `testdata/freedoom/` (cached in CI), not committed if size is an issue.
- Synthetic content generated by `ferrite-testkit` (maps, WADs, PK3s, DECORATE snippets, ACS objects compiled offline and committed with sources).
- Small hand-made fixtures for each binary format.

### 3.2 Compatibility corpus (not committed; fetched by script with licences checked per item)
Lists maintained in `testdata/corpus.toml` with URL, sha256, expected engine features and expected result (`loads`, `smoke-playthrough`). Categories: vanilla/limit-removing megawads, Boom megawads, MBF/MBF21 megawads, UMAPINFO projects, ZDoom-format/ACS maps, DECORATE gameplay mods, GZDoom-feature (GLDEFS/MODELDEF) mods, Hexen/Heretic PWADs. Examples of well-known community releases to include when their licences allow automated download: Sigil (vanilla), Valiant (MBF), Sunlust and Ancient Aliens (Boom), Eviternity (MBF), Back to Saturn X (limit-removing). Mods that require ZScript are tracked separately as "expected unsupported".

### 3.3 Gated (commercial IWADs)
Tests behind `FERRITE_IWAD_DIR`: vanilla demo sync suite (Doom 1.9 demos from the IWADs themselves, DEMO1–4; curated community demo archives for Boom/MBF complevels), Heretic/Hexen/Strife playthroughs. Never upload IWADs to CI caches of public repos.

## 4. CI

- Matrix: Linux x86-64 (primary), Windows x86-64, macOS aarch64; `wasm32-unknown-unknown` build + Node-based headless tests.
- Jobs: `fmt`, `clippy -D warnings`, `test` (all), `doc` (no broken intra-doc links), `fuzz-smoke` (each target 60 s), `determinism` (record on Linux → verify on macOS + wasm32), `golden-images` (Linux + lavapipe), `web-browser-tests`, `bench` (nightly; regression alerts at +10%), `cargo deny` (licences/advisories; enforce Appendix A policy, reject crates with C build scripts unless allowlisted).
- Artifacts: native builds per OS, web bundle, test reports, failing golden diffs.

# 00 — Overview

## 1. Mission

Build an engine that:

1. Runs the existing GZDoom content ecosystem (vanilla Doom/Doom II/Heretic/Hexen/
   Strife/Chex/Harmony-style IWADs, Boom/MBF/MBF21 maps, ZDoom-format and UDMF maps,
   DECORATE/ACS/MAPINFO mods, GZDoom graphical extensions).
2. Is written in idiomatic, safe, pure Rust, as a set of independently usable crates.
3. Uses a modern renderer (wgpu) with a faithful "classic" look available.
4. Replaces ZScript with sandboxed **WebAssembly** modules, so modders can use
   Rust, Zig, C (via clang → wasm), AssemblyScript, Go (TinyGo), or any other
   wasm-targeting language — with a first-class Rust SDK.
5. Ships natively (Windows, macOS, Linux) and in the browser from the same code.

## 2. Non-goals

- **Running ZScript source.** ZScript mods will not run. See
  `11-legacy-mod-formats.md §ZScript policy` for the migration story
  (a separate, optional ZScript→WASM transpiler is listed as a stretch milestone).
- Bit-exact reproduction of GZDoom's hardware renderer output. Visual parity
  is the goal; pixel parity is not.
- Supporting OpenGL directly, the Direct3D 9 path, or GZDoom's legacy
  software "softpoly" renderer. A classic CPU column/span renderer is an
  optional late milestone (`13-renderer.md §Classic software renderer`).
- Supporting obsolete platforms (32-bit Windows, macOS < 11).
- Zandronum/Odamex-style client-server netplay (lockstep only; see `17`).

## 3. Feature scope matrix

Legend: **M** = must (1.0), **S** = should (1.x), **C** = could (stretch).

| Area | Feature | Priority |
|------|---------|----------|
| Resources | WAD (IWAD/PWAD), PK3/ZIP, PK7/7z, directories, WAD-in-PK3, `.deh`/`.bex` loose files | M |
| Resources | GAMEINFO, IWADINFO, IWAD auto-detection, `filter/` directories | M |
| Maps | Doom, Hexen, UDMF (all ZDoom namespaces), ZDBSP/XNOD/ZNOD/XGLN/XGL2/XGL3 nodes, internal node builder | M |
| Maps | Slopes (line, vertex, things), 3D floors, line & sector portals, polyobjects, sector/line UDMF fields | M |
| Playsim | Doom/Heretic/Hexen/Strife actors, DECORATE, DeHackEd/BEX/MBF21, ACS (ACS0/ACSE/ACSe), all action specials | M |
| Playsim | Compatibility flags & compat levels; `compatibility.txt`-style per-map fixes | M |
| Scripting | WASM modules: actor classes, event handlers, HUD/status bar, menus, CVars, network events | M |
| Rendering | wgpu: walls, flats, sprites, sky (single/double/skybox/dome), dynamic lights, fog, colored sector light, 3D floors, portals, mirrors, decals, particles | M |
| Rendering | Models: MD2, MD3, OBJ, IQM (skeletal), UE1 vertex meshes, KVX voxels | M (MD2/MD3/OBJ/KVX), S (IQM, UE1) |
| Rendering | Palette-emulation "classic" mode (COLORMAP banding), post-processing (bloom, tonemap, SSAO, FXAA), custom material shaders | M / S |
| Rendering | Classic CPU software renderer | C |
| Audio | SFX: DMX, WAV, OGG, FLAC, MP3, 3D positional, SNDINFO, ambient sounds, reverb environments | M |
| Audio | Music: MUS, MIDI (SoundFont synth), OPL3 emulation, MOD/S3M/XM/IT, OGG/FLAC/MP3/WAV streams | M |
| Audio | GME-style chiptunes (VGM/SPC/NSF) | C |
| UI | MENUDEF menus, SBARINFO status bars, alt HUD, automap, intermissions/finales, LANGUAGE localization, fonts (FON1/FON2/BMF/console/TrueType via pure-Rust rasterizer) | M |
| Input | Keyboard/mouse/gamepad, KEYCONF, bindings, console, CVars, CVARINFO, aliases, `exec` | M |
| Net | Peer-to-peer lockstep (2–8 players), co-op/deathmatch, desync detection; web via WebRTC | S |
| Demos | ZDoom `.lmp` demo record/playback; vanilla demo playback in strict compat mode | M |
| Saves | Save/load with versioned format, including WASM module state | M |
| Platform | Native desktop (winit), Web (wasm32 + wasm-bindgen), headless (servers/tests) | M |

## 4. Design principles

1. **Data-oriented sim, object-oriented surface.** Internally actors live in
   struct-of-arrays-friendly storage addressed by generational handles; modders
   see classes, inheritance, virtual methods.
2. **Parse once, compile to tables.** Every text definition (DECORATE, MAPINFO,
   SNDINFO …) compiles into immutable, interned registries before a game starts.
   The sim never parses text.
3. **Two-phase world.** `Definitions` (immutable, `Arc`-shared after load) and
   `World` (mutable game state, snapshot-able, serializable).
4. **Fixed 35 Hz simulation**, uncapped interpolated rendering.
5. **Everything observable is testable headless.** A tic can be run with no GPU
   or audio; a frame can be rendered offscreen to a PNG.

## 5. Terminology

See `appendix-c-glossary.md`. Key words used everywhere:

- **Lump** — a named resource inside an archive.
- **Tic** — one simulation step, 1/35 s.
- **Thinker** — any object that runs per tic (actors, sector movers, light effects, scripts).
- **Actor** — a thinker with position, state machine, and class (ZDoom `AActor`).
- **Special** — a numbered action (line special / sector special) with up to 5 integer args.
- **Definitions** — compiled, immutable game data registries.
- **Module** — a WASM mod package (replaces ZScript code).

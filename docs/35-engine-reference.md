# 35 — Room engine reference (one page)

Pure-Rust, library-first re-implementation of the ZDoom/GZDoom family
(+ Duke Nukem 3D / Build), with a wgpu renderer for desktop and web, lockstep
netplay, a multi-room server, WASM mods and a ZScript VM.
41 crates, ~350 k lines of Rust (2026-11-01). Rules: `AGENTS.md`;
details: `docs/00`–`34`.

## 1. Layers

```mermaid
flowchart TB
  subgraph Shells["Shells (binaries)"]
    app[room-app<br/>desktop]
    web[room-web<br/>browser]
    cli[room-cli<br/>tools]
    srv[room-server<br/>multi-room server]
  end
  subgraph Game["Game orchestration"]
    game[room-game<br/>Headless: startup, levels, saves, demos]
  end
  subgraph Engine["Engine subsystems"]
    sim[room-sim<br/>playsim World]
    render[room-render<br/>wgpu]
    ui[room-ui<br/>HUD, menus, automap]
    audio[room-audio + room-music]
    input[room-input + room-console]
    net[room-net<br/>lockstep]
  end
  subgraph Script["Scripting"]
    acs[room-acs<br/>ACS VM]
    script[room-script<br/>WASM host]
    zvm[room-zscript + room-zvm<br/>ZScript]
  end
  subgraph Content["Content & definitions"]
    defs[room-defs<br/>text lumps → Definitions]
    deco[room-decorate + room-dehacked]
    map[room-map + room-nodebuild]
    fmt[room-formats + room-sc]
    vfs[room-vfs<br/>WAD/PK3/dirs]
  end
  core[room-core<br/>math, IDs, names, RNG, dmath]
  Shells --> Game --> Engine
  Engine --> Script
  Engine --> Content
  Script --> Content
  Content --> core
  Engine --> core
```

## 2. Crate dependency graph (internal deps only)

```mermaid
flowchart LR
  core[room-core]
  sc[room-sc]; formats[room-formats]; vfs[room-vfs]
  defs[room-defs]; deco[room-decorate]; deh[room-dehacked]
  map[room-map]; nb[room-nodebuild]
  acs[room-acs]; sabi[room-script-abi]; script[room-script]
  zvm[room-zvm]; zsc[room-zscript]
  sim[room-sim]; ui[room-ui]; render[room-render]
  audio[room-audio]; music[room-music]; input[room-input]; console[room-console]
  net[room-net]; proto[room-proto]; game[room-game]
  build[room-build]; bengine[room-build-engine]; duke[room-duke]
  faceswap[room-faceswap]; py[room-python-host]
  app[room-app]; web[room-web]; cli[room-cli]; server[room-server]
  sfu[room-sfu]; ws[room-ws]; signal[room-signal]
  sdk[room-script-sdk]; macros[room-script-sdk-macros]

  defs --> core & formats & sc & vfs
  deco --> sc
  deh --> deco
  map --> core & formats & vfs
  nb --> core & map
  acs --> core
  script --> sabi
  zvm --> core
  zsc --> core & vfs & zvm
  sim --> acs & core & deco & map & nb & script & sabi & zvm & zsc
  ui --> defs & formats & map & sim & vfs & zvm
  render --> core & defs & formats & map & nb & vfs
  audio --> core
  music --> formats
  game --> audio & core & deco & defs & formats & map & music & script & sim & ui & vfs
  build --> formats & vfs
  bengine --> core & build
  duke --> build & bengine & core
  faceswap --> sc
  py --> core & game & sim
  sdk --> sabi & macros
  ws --> net
  app --> game & render & ui & audio & music & input & console & net & build & faceswap & py
  web --> game & render & ui & audio & music & input & net & proto & script & build & faceswap
  cli --> game & sim & acs & duke & zsc & zvm & faceswap
  server --> net & proto & sfu & game & sim & build
```

(`room-app`, `room-web`, `room-cli`, `room-server` also use most content crates
directly; edges omitted for readability. `room-script-sdk` is linked by **guest**
mods compiled to wasm32, never by the engine.)

## 3. Startup / load pipeline

```mermaid
flowchart LR
  A[IWAD + PWAD/PK3 + mods] --> B[room-vfs<br/>mount order, namespaces, lump lookup]
  B --> C[room-defs<br/>MAPINFO, SNDINFO, GLDEFS, TEXTURES,<br/>LANGUAGE, KEYCONF, CVARINFO, MENUDEF, SBARINFO…]
  B --> D[room-zscript → room-zvm<br/>ZScript compiled first]
  B --> E[room-decorate + room-dehacked<br/>DECORATE / DEH / BEX]
  D --> F[ClassRegistry + StateTable<br/>room-sim]
  E --> F
  B --> G[room-script<br/>ROOMMOD → WASM mods register classes]
  G --> F
  C --> H[Definitions immutable]
  F --> I[room-game Headless<br/>new game / load map]
  H --> I
  I --> J[room-map + room-nodebuild<br/>map model, BSP/GL nodes, blockmap]
  J --> K[room-sim World]
```

## 4. Runtime: one tic and one frame

```mermaid
sequenceDiagram
  participant In as room-input (+ room-net)
  participant G as room-game Headless
  participant S as room-sim World (35 Hz, deterministic)
  participant M as Scripts (ACS / WASM / ZScript)
  participant V as SimView + outputs
  participant R as room-render / room-ui / room-audio
  In->>G: TicCmd per player (lockstep-agreed)
  G->>S: tick(cmds)
  S->>S: players, thinkers, physics, specials, AI
  S->>M: ACS scripts, state actions, events (WorldTick, ThingDied…)
  M-->>S: mutations via host APIs (deterministic)
  S->>V: snapshot + drained outputs (sounds, messages, decals, FX, mod HUD)
  V->>R: frame: interpolate(alpha) → 3D scene, post, 2D canvas, mixer
```

Key rule: **the renderer never mutates game state**; everything inside a tic is
bit-reproducible on x86-64 / aarch64 / wasm32 (no HashMap iteration, no std
transcendental floats, no wall clock, no threads — `room_core::dmath`, playsim RNG).

## 5. Scripting stack

```mermaid
flowchart TB
  subgraph Content
    acsO[ACS objects<br/>BEHAVIOR / LOADACS]
    decoS[DECORATE states<br/>native actions + expressions]
    zs[ZScript source]
    wasmM[WASM mods<br/>Rust / AssemblyScript / C / Python host]
  end
  acsO --> acsVM[room-acs VM]
  decoS --> act[room-sim action table<br/>action.rs, deco_eval.rs]
  zs --> zc[room-zscript compiler] --> zvm2[room-zvm interpreter / JIT]
  wasmM --> host[room-script<br/>wasmi / wasmtime / browser]
  acsVM & act & zvm2 & host --> world[room-sim World]
  host -. ABI v1 append-only .- abi[room-script-abi + room-abi-gen IDL]
```

## 6. Networking and server

```mermaid
flowchart LR
  c1[client room-web / room-app] -- TicCmds FTR2 / WS --> srv[room-server<br/>rooms, lobby, admin, verifier]
  c2[client] -- TicCmds --> srv
  srv -- TIC_FRAME fan-out --> c1 & c2
  srv --- sfu[room-sfu<br/>WebRTC voice/video]
  srv -. headless verifier .- game2[room-game + room-sim]
  c1 -. P2P/LAN .- net2[room-net UDP/TCP/WebRTC] -. relay .- ws2[room-ws / room-signal]
```

Every peer runs the full sim from the same inputs (lockstep); the server
relays inputs, compares checksums, and can verify headless. Mods (WASM WAD,
ZScript in PK3) are fingerprinted so all seats run identical bytes.

## 7. Crate reference

| Crate | Layer | LOC | Role | Doc |
|---|---|---:|---|---|
| room-core | base | 3.0 k | math, fixed/float types, names, IDs, RNG, deterministic `dmath` | 01, 02 |
| room-sc | content | 0.9 k | ZDoom text scanner shared by all text-lump parsers | 05 |
| room-formats | content | 3.0 k | binary lumps: patches, flats, palettes, sounds, music, demos, PNG/OGG/FLAC | 04 |
| room-vfs | content | 2.4 k | WAD/PK3/PK7/dirs, load order, namespaces, IWAD detection, nested WADs | 03 |
| room-defs | content | 9.1 k | text lumps → immutable `Definitions` (MAPINFO, SNDINFO, GLDEFS, TEXTURES, …) | 05 |
| room-decorate | content | 3.8 k | DECORATE parser + expressions → ClassDef IR | 11 |
| room-dehacked | content | 2.2 k | DeHackEd/BEX/MBF21 patches on the IR | 11 |
| room-map | content | 9.2 k | map model, Doom/Hexen/UDMF loaders, blockmap, reject | 06 |
| room-nodebuild | content | 1.4 k | internal BSP + GL nodes (ZDBSP-equivalent) | 06 |
| room-sim | engine | 76.5 k | playsim `World`: actors, thinkers, physics, specials, inventory, weapons, AI, mod HUD, FX events | 07, 08, 09, 30 |
| room-acs | script | 7.3 k | ACS loader + VM (ACS0/ACSE/ACSe) | 10 |
| room-script-abi | script | 1.4 k | `no_std` ABI types/constants shared by host and guests | 12 |
| room-script | script | 4.1 k | WASM host: validation, budgets, registration, game imports | 12, 31 |
| room-script-sdk (+ macros) | guest | 2.4 k | Rust guest SDK for mods (`RoomMod`, `export_mod!`, HUD, art/sound) | 12 |
| room-abi-gen | tool | 0.1 k | IDL → host glue, Rust/C/Zig/AS bindings | 12 |
| room-zscript | script | 4.2 k | ZScript lexer, preprocessor, parser, semantics, bytecode compiler | 34 |
| room-zvm | script | 3.0 k | ZScript register VM: interpreter, heap, GC, natives | 34 |
| room-render | engine | 20.1 k | wgpu renderer: BSP walk, portals, sprites/models/voxels, lights, post, 2D canvas | 13, 29 |
| room-render-soft | engine | stub | classic CPU renderer (stretch) | 13 §12 |
| room-ui | engine | 7.2 k | `UiDrawList`, fonts, menus, status bars/SBARINFO, HUD, automap, intermission | 16 |
| room-audio | engine | 0.5 k | mixer, SFX, positional audio, reverb, backends | 14 |
| room-music | engine | 1.4 k | MUS/MIDI, SoundFont, OPL3, trackers, streams | 14 |
| room-input | engine | 0.9 k | devices, bindings, TicCmd building, mod buttons | 15 |
| room-console | engine | 1.6 k | console, commands, CVars, config | 15 |
| room-net | engine | 3.6 k | lockstep sessions, UDP/TCP/WS/WebRTC transports, desync detection | 17 |
| room-game | game | 28.2 k | `Headless`: startup, levels, hubs, intermission, saves, demos, mod loading | 01, 17 |
| room-faceswap | feature | 2.5 k | face capture alignment + sprite-head compositing | 26 |
| room-build | Duke | 21.2 k | GRP/ART/MAP readers, converter to a Room UDMF WAD | 23 |
| room-build-engine | Duke | 5.2 k | Build world model + clean-room primitives | 25 |
| room-duke | Duke | 64.1 k | native Duke 3D runtime: CON compiler/VM, game logic, 30 Hz | 25 |
| room-python-host | feature | 0.5 k | optional Python play mods (desktop) | — |
| room-proto | server | 1.0 k | wire types/codecs, protocol v2 (FTR2) + JSON control | 22 |
| room-server | server | 9.8 k | multi-room WS server: lobby, admin, verifier, mods, BR stats | 22, 28 |
| room-sfu | server | 1.2 k | WebRTC SFU for voice/video | 22 §8 |
| room-ws | server | 0.3 k | WebSocket packet relay | 17 |
| room-signal | server | 0.3 k | WebRTC signalling helper | 17 |
| room-app | shell | 8.6 k | desktop binary (winit + wgpu + cpal) | 18 |
| room-web | shell | 12.1 k | browser shell (wasm-bindgen, WebGPU/WebGL2, editor) | 18 |
| room-cli | shell | 2.1 k | tools: WAD, maps, nodes, demos, Duke traces, ZScript check, render-to-PNG | — |
| room-testkit | test | 1.0 k | UDMF map builder DSL, headless scenario runners | 20 |

## 8. Where to start
- **Gameplay bug** → `room-sim` (`world.rs`, `action.rs`, `think_player.rs`), tests in `room-sim` / `room-game/tests`.
- **Content won't load** → `room-vfs` → `room-defs` / `room-decorate` → `ClassRegistry` (`room-sim/src/class.rs`); `room-cli` to inspect.
- **Visual bug** → `room-render` (`pass.rs`, `shaders/`, `frame.rs`), goldens in `testdata/goldens`.
- **Mod API** → `room-script-abi/api/*.ron`, `room-script/src/game.rs`, `room-sim/src/script_mods.rs`, SDK.
- **Netplay/desync** → `room-net`, `room-server`, `World::checksum`.
- **Progress & plans** → `docs/21` (milestones), feature specs `docs/26`–`34`.

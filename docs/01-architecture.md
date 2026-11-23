# 01 — Architecture

## 1. Workspace layout

A single Cargo workspace. Crate names use the `room-` prefix (Rust module
names `room_*`). The edition is 2024; MSRV is the latest stable at project start, bumped deliberately.

```
room/
  Cargo.toml                 # [workspace], shared lints, profiles
  crates/
    room-core/            # math, fixed point, angles, Name interning, RNG, errors, ids
    room-vfs/             # archives, load order, namespaces, lump lookup
    room-formats/         # binary parsers (no engine knowledge): WAD dir, patches, PNG, MUS, DMX, nodes…
    room-sc/              # ZDoom-style text scanner shared by all text lump parsers
    room-defs/            # compiles text lumps into immutable registries ("Definitions")
    room-decorate/        # DECORATE parser → ClassDef IR
    room-dehacked/        # DeHackEd/BEX/MBF21 patcher → applies to ClassDef IR
    room-map/             # map model, loaders (Doom/Hexen/UDMF), validation, blockmap, reject
    room-nodebuild/       # internal BSP + GL node builder (ZDBSP-equivalent)
    room-sim/             # the playsim: World, thinkers, actors, physics, specials
    room-acs/             # ACS loader + VM
    room-script-abi/      # #![no_std] shared ABI types/constants between host and guests
    room-script/          # WASM host runtime (wasmtime / wasmi / browser backends)
    room-script-sdk/      # guest-side Rust SDK (#![no_std] + alloc), proc-macros in -sdk-macros
    room-script-sdk-macros/
    room-render/          # wgpu renderer (3D scene + 2D canvas backend)
    room-audio/           # mixer, SFX, 3D audio, reverb, output backends
    room-music/           # MUS/MIDI sequencer, SoundFont synth, OPL3, tracker modules, streams
    room-ui/              # 2D draw list API, fonts, menus, status bars, HUD, automap, intermission
    room-input/           # device abstraction, bindings, ticcmd building
    room-console/         # console, commands, CVars, config persistence
    room-net/             # lockstep netcode, transports
    room-game/            # game state machine: startup, levels, hubs, intermission, saves, demos
    room-app/             # native desktop binary (winit + wgpu + cpal)
    room-web/             # wasm32 binary (wasm-bindgen + web-sys)
    room-cli/             # tools: wad ls/extract, map dump, nodebuild, demo verify, render-to-png
    room-testkit/         # test-only: UDMF map builder DSL, headless scenario helpers
    room-abi-gen/         # tool: generates host glue + guest bindings (Rust, C, Zig, AssemblyScript) from the scripting IDL
    room-signal/          # tiny WebRTC signalling server for netplay (optional)
    room-render-soft/     # classic CPU software renderer (planned, docs/40)
  fuzz/                      # cargo-fuzz targets
  testdata/                  # generated fixtures, Freedoom-derived goldens
  docs/                      # these documents + ADRs in docs/decisions/
```

## 2. Dependency graph (allowed edges only)

```
                 room-core
                      │
      ┌───────────────┼──────────────────────────┐
  room-vfs   room-formats   room-sc   room-script-abi
      │               │              │                │
      └──────┬────────┴──────┬───────┘                │
             │         room-decorate               │
             │         room-dehacked               │
             ▼               ▼                        │
          room-defs ◄─────┘                        │
             │                                        │
     ┌───────┼───────────────┐                        │
 room-map │          room-nodebuild             │
     │  ◄────┼───────────────┘                        │
     ▼       ▼                                        │
  room-acs   room-script ◄──────────────────────┘
     │            │
     └──► room-sim ◄──┘   (sim depends on acs + script via traits defined in sim; see §5)
              │
   ┌──────────┼──────────────┬──────────────┬─────────────┐
room-render room-audio room-ui  room-net  room-console/input
   (reads SimView)  (reads SoundEvents)  (reads SimView + UI state)
              │
         room-game  (owns everything, drives the loop)
              │
      room-app / room-web / room-cli
```

Rules:

- `room-sim` **must not** depend on render/audio/ui/input/net/game.
- `room-render` **must not** depend on `room-sim` internals; it consumes
  `room_sim::view::SimView` (a public, read-only snapshot type) and `Definitions`.
- Only `room-app` and `room-web` know about winit/web-sys/cpal device creation.
- `room-script-abi` and `room-script-sdk` must compile for `wasm32-unknown-unknown`
  with `#![no_std]` (+ `alloc` for the SDK).

## 3. Core runtime objects

```rust
/// Immutable after load. Shared with Arc between sim, renderer, audio, UI.
pub struct Definitions {
    pub game: GameInfo,                 // which game, IWAD-derived defaults
    pub names: NameTable,               // interned names (case-insensitive ASCII)
    pub classes: ClassRegistry,         // all actor classes (built-ins, DECORATE, DeHackEd, WASM)
    pub states: StateTable,             // flat array of all states, indexed by StateId
    pub sprites: SpriteRegistry,
    pub textures: TextureRegistry,      // metadata only (size, offsets, scale, source); pixels on demand
    pub animations: AnimDefs,           // ANIMATED/ANIMDEFS/SWITCHES
    pub mapinfo: MapInfo,               // episodes, clusters, maps, skills, gameinfo section
    pub sounds: SoundRegistry,          // SNDINFO logical sounds, aliases, random lists, player sounds
    pub terrain: TerrainDefs,
    pub decals: DecalDefs,
    pub locks: LockDefs,
    pub language: Language,             // string tables per locale
    pub gldefs: GlDefs,                 // lights, glows, brightmaps, materials, skyboxes
    pub models: ModelDefs,              // MODELDEF
    pub voxels: VoxelDefs,
    pub fonts: FontDefs,
    pub menus: MenuDefs,
    pub sbarinfo: Option<SbarInfo>,
    pub cvar_decls: Vec<CvarDecl>,      // CVARINFO + module-declared
    pub translations: TranslationDefs,  // TRNSLATE + built-in player translations
    pub compat: CompatDatabase,         // per-map fixes keyed by MD5
    pub acs_libraries: Vec<AcsModuleRef>,
    pub script_modules: Vec<ModuleRef>, // WASM modules in load order
}

/// Mutable game state. Fully serializable. Deterministic.
pub struct World { /* see 07-playsim.md */ }

/// Read-only per-tic snapshot used by renderer/audio/UI.
pub struct SimView<'w> { /* borrowed view, see §6 */ }
```

`Definitions` is built by `room_defs::DefinitionsBuilder` from a mounted
`Vfs`. Building is deterministic: same load order ⇒ same IDs.

## 4. The main loop

The loop is owned by `room_game::Engine` and driven by the platform shell,
which calls `Engine::frame(now: Instant-like, input_events)` once per display frame.

```
frame(now):
  input.process(events)                 # updates binding state, console, menus
  if ui.consumes_input(): route to ui
  accumulator += now - last
  while accumulator >= TIC (1/35 s) and tics_this_frame < MAX_CATCHUP (e.g. 5):
      cmd = input.build_ticcmd()        # 07-playsim §Ticcmd
      cmds = net.exchange(cmd)          # local: [cmd]; lockstep: wait/predict per 17
      game.tick(&cmds)                  # sim.tick + game state machine (intermission etc.)
      audio.consume(world.drain_sound_events())
      accumulator -= TIC
  alpha = accumulator / TIC             # interpolation fraction 0..1
  view = world.view(alpha)
  renderer.render_frame(&view, &ui.draw_list(&view))
  audio.update_listener(view.listener())
```

- On web, `frame` is invoked from `requestAnimationFrame`.
- Headless/test mode calls `game.tick` directly N times.
- Fast-forward (demo `-timedemo`) ignores the accumulator.
- When paused (menu open in single player, `pause` command) sim tics stop, UI keeps ticking at 35 Hz on its own `UiClock` (menus animate on tics, like GZDoom).

## 5. Breaking cycles: sim ↔ scripting

`room-sim` defines traits; `room-acs` and `room-script` implement
them; `room-game` wires them together. This keeps the sim testable without a
WASM runtime.

```rust
// in room-sim
pub trait ScriptHost {
    /// Called for actor action functions implemented by modules.
    fn call_action(&mut self, w: &mut WorldCtx, func: ActionFnId, actor: ActorId, args: &ActionArgs) -> ActionResult;
    /// Virtual method dispatch for module-defined classes (Tick, PostBeginPlay, DamageMobj, ...).
    fn call_virtual(&mut self, w: &mut WorldCtx, vfn: VirtualSlot, this: ActorId, args: &[Value]) -> Value;
    /// Event handler fan-out (WorldThingSpawned, WorldTick, ...).
    fn dispatch_event(&mut self, w: &mut WorldCtx, ev: &WorldEvent);
    fn snapshot(&self) -> ScriptSnapshot;          // for saves / netplay checksums
    fn restore(&mut self, s: &ScriptSnapshot) -> Result<()>;
}

pub trait AcsHost {
    fn start_script(&mut self, w: &mut WorldCtx, req: AcsStart) -> bool;
    fn run_tic(&mut self, w: &mut WorldCtx);
    // ...
}
```

`WorldCtx` is a `&mut` view of the world that also exposes a command queue for
deferred operations (spawns during iteration, destruction) — see `07-playsim.md §Deferred mutation`.

## 6. Snapshot / interpolation model

At the end of each tic the sim records, for every actor and every moving
surface, `prev` and `cur` values for interpolated quantities (position, angle,
pitch, roll, floor/ceiling heights, polyobject vertices, scroll offsets, camera
values). `SimView` exposes `lerp(prev, cur, alpha)`. GZDoom-compatible rules:

- Teleporting actors set a `no_interp` flag for one tic (`TeleportFlags`, `A_Warp` with `WARPF_INTERPOLATE` off).
- Sector plane interpolation only for planes that moved this tic.
- Player view uses the local unconfirmed input for mouse look (`cl_capfps 0` behaviour): the renderer may apply the latest mouse yaw/pitch delta on top of the interpolated angle for zero-latency look (GZDoom does this) — configurable by `r_mouselook_prediction` (default on).

## 7. Threading model

| Thread | Native | Web |
|--------|--------|-----|
| Main | window events, input, sim, game logic, render command encoding | same (single-threaded) |
| Render prep | optional rayon pool for BSP traversal/clipping/geometry batching | not used |
| Audio | cpal callback thread: mixer pulls from lock-free ring of `AudioCommand`s | AudioWorklet (or ScriptProcessor fallback) fed via `SharedArrayBuffer` when cross-origin isolated; else main-thread mixing in larger blocks |
| Loading | rayon pool: decode textures, build nodes, compile definitions in parallel where deterministic | single-threaded, chunked across frames with `async` |

The sim is single-threaded by design (determinism). Parallelism inside the sim is
**forbidden** in 1.0.

## 8. Error-handling architecture

- Load-time: errors are collected, not fatal, wherever GZDoom is lenient
  (e.g. an unknown SNDINFO command prints a warning and is skipped). Where GZDoom
  is strict (DECORATE syntax errors abort startup after reporting all errors in
  the lump), Room is strict too. The per-lump policy is listed in `05` and `11`.
  Each error carries a `SourceSpan { lump: LumpId, line, col }`.
- Fatal errors bubble to `room-game`, which shows them in a startup error screen (native) or overlay (web) — never `panic!`.
- Runtime script traps (WASM trap, ACS runaway script) are contained: the
  offending script/actor is disabled, a console error is logged, the game continues. Configurable `script_trap_policy = log | pause | abort`.

## 9. Configuration and paths

- Config file: `room.toml` (TOML) in the platform config dir (`directories` crate), with an importer for `gzdoom.ini` (`15-input-console-cvars.md §Config`).
- Search paths for IWADs/PWADs: config `[paths] iwad = [...]`, `DOOMWADDIR`, `DOOMWADPATH`, executable dir, platform data dirs, Steam/GOG install locations (detection list in `03-vfs.md §IWAD search`).
- Web: virtual filesystem backed by OPFS (`18-platform-web.md`).

## 10. Public API surface (library users)

The library must support these external use cases with no `room-app` dependency:

```rust
// 1. Inspect archives
let vfs = Vfs::builder().mount_path("doom2.wad")?.build()?;
for lump in vfs.lumps() { println!("{} {}", lump.full_name(), lump.size()); }

// 2. Load and analyze a map without running it
let defs = DefinitionsBuilder::new(&vfs).build()?;
let map = room_map::load(&vfs, &defs, "MAP01")?;

// 3. Run a headless simulation (bots/AI research, demo verification)
let mut game = room_game::Headless::new(vfs, GameOptions::default())?;
game.start_map("MAP01", Skill::new(3))?;
for _ in 0..350 { game.tick(&[TicCmd::default()]); }
let hash = game.world().checksum();

// 4. Render a map to an image offscreen
let png = room_render::offscreen::render_view(&game.world().view(0.0), 1280, 720)?;
```

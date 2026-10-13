# 01 — Architecture

## 1. Workspace layout

A single Cargo workspace. Crate names use the `ferrite-` prefix (Rust module
names `ferrite_*`). The edition is 2024; MSRV is the latest stable at project start, bumped deliberately.

```
ferrite/
  Cargo.toml                 # [workspace], shared lints, profiles
  crates/
    ferrite-core/            # math, fixed point, angles, Name interning, RNG, errors, ids
    ferrite-vfs/             # archives, load order, namespaces, lump lookup
    ferrite-formats/         # binary parsers (no engine knowledge): WAD dir, patches, PNG, MUS, DMX, nodes…
    ferrite-sc/              # ZDoom-style text scanner shared by all text lump parsers
    ferrite-defs/            # compiles text lumps into immutable registries ("Definitions")
    ferrite-decorate/        # DECORATE parser → ClassDef IR
    ferrite-dehacked/        # DeHackEd/BEX/MBF21 patcher → applies to ClassDef IR
    ferrite-map/             # map model, loaders (Doom/Hexen/UDMF), validation, blockmap, reject
    ferrite-nodebuild/       # internal BSP + GL node builder (ZDBSP-equivalent)
    ferrite-sim/             # the playsim: World, thinkers, actors, physics, specials
    ferrite-acs/             # ACS loader + VM
    ferrite-script-abi/      # #![no_std] shared ABI types/constants between host and guests
    ferrite-script/          # WASM host runtime (wasmtime / wasmi / browser backends)
    ferrite-script-sdk/      # guest-side Rust SDK (#![no_std] + alloc), proc-macros in -sdk-macros
    ferrite-script-sdk-macros/
    ferrite-render/          # wgpu renderer (3D scene + 2D canvas backend)
    ferrite-audio/           # mixer, SFX, 3D audio, reverb, output backends
    ferrite-music/           # MUS/MIDI sequencer, SoundFont synth, OPL3, tracker modules, streams
    ferrite-ui/              # 2D draw list API, fonts, menus, status bars, HUD, automap, intermission
    ferrite-input/           # device abstraction, bindings, ticcmd building
    ferrite-console/         # console, commands, CVars, config persistence
    ferrite-net/             # lockstep netcode, transports
    ferrite-game/            # game state machine: startup, levels, hubs, intermission, saves, demos
    ferrite-app/             # native desktop binary (winit + wgpu + cpal)
    ferrite-web/             # wasm32 binary (wasm-bindgen + web-sys)
    ferrite-cli/             # tools: wad ls/extract, map dump, nodebuild, demo verify, render-to-png
    ferrite-testkit/         # test-only: UDMF map builder DSL, headless scenario helpers
    ferrite-abi-gen/         # tool: generates host glue + guest bindings (Rust, C, Zig, AssemblyScript) from the scripting IDL
    ferrite-signal/          # tiny WebRTC signalling server for netplay (optional)
    ferrite-render-soft/     # stretch: classic CPU software renderer
  fuzz/                      # cargo-fuzz targets
  testdata/                  # generated fixtures, Freedoom-derived goldens
  docs/                      # these documents + ADRs in docs/decisions/
```

## 2. Dependency graph (allowed edges only)

```
                 ferrite-core
                      │
      ┌───────────────┼──────────────────────────┐
  ferrite-vfs   ferrite-formats   ferrite-sc   ferrite-script-abi
      │               │              │                │
      └──────┬────────┴──────┬───────┘                │
             │         ferrite-decorate               │
             │         ferrite-dehacked               │
             ▼               ▼                        │
          ferrite-defs ◄─────┘                        │
             │                                        │
     ┌───────┼───────────────┐                        │
 ferrite-map │          ferrite-nodebuild             │
     │  ◄────┼───────────────┘                        │
     ▼       ▼                                        │
  ferrite-acs   ferrite-script ◄──────────────────────┘
     │            │
     └──► ferrite-sim ◄──┘   (sim depends on acs + script via traits defined in sim; see §5)
              │
   ┌──────────┼──────────────┬──────────────┬─────────────┐
ferrite-render ferrite-audio ferrite-ui  ferrite-net  ferrite-console/input
   (reads SimView)  (reads SoundEvents)  (reads SimView + UI state)
              │
         ferrite-game  (owns everything, drives the loop)
              │
      ferrite-app / ferrite-web / ferrite-cli
```

Rules:

- `ferrite-sim` **must not** depend on render/audio/ui/input/net/game.
- `ferrite-render` **must not** depend on `ferrite-sim` internals; it consumes
  `ferrite_sim::view::SimView` (a public, read-only snapshot type) and `Definitions`.
- Only `ferrite-app` and `ferrite-web` know about winit/web-sys/cpal device creation.
- `ferrite-script-abi` and `ferrite-script-sdk` must compile for `wasm32-unknown-unknown`
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

`Definitions` is built by `ferrite_defs::DefinitionsBuilder` from a mounted
`Vfs`. Building is deterministic: same load order ⇒ same IDs.

## 4. The main loop

The loop is owned by `ferrite_game::Engine` and driven by the platform shell,
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

`ferrite-sim` defines traits; `ferrite-acs` and `ferrite-script` implement
them; `ferrite-game` wires them together. This keeps the sim testable without a
WASM runtime.

```rust
// in ferrite-sim
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
  the lump), Ferrite is strict too. The per-lump policy is listed in `05` and `11`.
  Each error carries a `SourceSpan { lump: LumpId, line, col }`.
- Fatal errors bubble to `ferrite-game`, which shows them in a startup error screen (native) or overlay (web) — never `panic!`.
- Runtime script traps (WASM trap, ACS runaway script) are contained: the
  offending script/actor is disabled, a console error is logged, the game continues. Configurable `script_trap_policy = log | pause | abort`.

## 9. Configuration and paths

- Config file: `ferrite.toml` (TOML) in the platform config dir (`directories` crate), with an importer for `gzdoom.ini` (`15-input-console-cvars.md §Config`).
- Search paths for IWADs/PWADs: config `[paths] iwad = [...]`, `DOOMWADDIR`, `DOOMWADPATH`, executable dir, platform data dirs, Steam/GOG install locations (detection list in `03-vfs.md §IWAD search`).
- Web: virtual filesystem backed by OPFS (`18-platform-web.md`).

## 10. Public API surface (library users)

The library must support these external use cases with no `ferrite-app` dependency:

```rust
// 1. Inspect archives
let vfs = Vfs::builder().mount_path("doom2.wad")?.build()?;
for lump in vfs.lumps() { println!("{} {}", lump.full_name(), lump.size()); }

// 2. Load and analyze a map without running it
let defs = DefinitionsBuilder::new(&vfs).build()?;
let map = ferrite_map::load(&vfs, &defs, "MAP01")?;

// 3. Run a headless simulation (bots/AI research, demo verification)
let mut game = ferrite_game::Headless::new(vfs, GameOptions::default())?;
game.start_map("MAP01", Skill::new(3))?;
for _ in 0..350 { game.tick(&[TicCmd::default()]); }
let hash = game.world().checksum();

// 4. Render a map to an image offscreen
let png = ferrite_render::offscreen::render_view(&game.world().view(0.0), 1280, 720)?;
```

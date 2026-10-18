# AGENTS.md — Implementation guide for coding agents

You are implementing **Room**, a pure-Rust, modern re-implementation of the
ZDoom/GZDoom family of Doom source ports. This file is your entry point. Read it
fully before writing code, then read the documents listed under *Reading order*.

## 1. What you are building

A library-first game engine that loads the same content GZDoom loads (IWADs,
PWADs, PK3s, UDMF maps, DECORATE, ACS, MAPINFO, SNDINFO, GLDEFS, …) and plays it
with the same behaviour, while:

- being written in **pure Rust** (no vendored C/C++; see `docs/appendix-a-dependencies.md`),
- rendering through **wgpu** on desktop (Vulkan/Metal/DX12) **and** the web (WebGPU, WebGL2 fallback),
- replacing **ZScript** with **WebAssembly modules** (any language that targets
  `wasm32-unknown-unknown`, first-class SDK in Rust) — see `docs/12-wasm-scripting.md`,
- exposing every subsystem as a reusable crate so tools (map viewers, editors,
  WAD inspectors, servers) can depend on parts of it.

## 2. Reading order

| # | Document | Why |
|---|----------|-----|
| 1 | `docs/00-overview.md` | Goals, non-goals, scope, terminology |
| 2 | `docs/01-architecture.md` | Crate graph, data flow, threading, the frame/tic loop |
| 3 | `docs/02-conventions.md` | Code style, error handling, numeric types, determinism rules — **mandatory** |
| 4 | `docs/20-roadmap-testing.md` | Milestones and their acceptance tests — **work in this order** |
| 5 | `docs/21-milestone-progress.md` | Living checklist — **update as work lands**; see current status |
| 6+ | The subsystem docs | Read each one when its milestone starts |

## 3. Non-negotiable rules

1. **Determinism of the playsim.** Everything that runs inside a game tic must
   be bit-for-bit reproducible across platforms (x86-64, aarch64, wasm32) given
   the same inputs. Read `docs/02-conventions.md §Determinism` before touching
   `room-sim`, `room-acs` or `room-script`. No `HashMap` iteration in
   sim code, no `f32`/`f64` transcendental functions from `std`, no wall-clock
   reads, no threads.
2. **The renderer never mutates game state.** It reads an immutable snapshot
   (`SimView`) produced at the end of each tic, interpolated by a fraction.
3. **Pure Rust.** Do not add a dependency that compiles C/C++ (`cc`, `cmake`
   build scripts) without it being listed as allowed in Appendix A. System API
   bindings (graphics drivers via wgpu, ALSA/CoreAudio/WASAPI via cpal) are allowed.
4. **Library first.** No crate except `room-app` may assume a window, a
   filesystem, a GPU, or an audio device exists. Inject them via traits.
5. **Compatibility is a feature, not an afterthought.** When documentation and
   GZDoom behaviour disagree, GZDoom behaviour wins for content compatibility,
   unless a document explicitly lists the deviation under "Intentional deviations".
6. **Every parser is fuzzable.** Every function that takes untrusted bytes or
   text returns `Result` and never panics. Add a `cargo fuzz` target per parser.
7. **No `unsafe`** outside crates explicitly allowed to use it
   (`room-render` for buffer casting via `bytemuck` only, `room-script`
   for host↔guest memory access, `room-script-sdk` for WASM import/export
   stubs). Each `unsafe` block carries a `// SAFETY:` comment.

## 4. Licensing guidance (read before porting any code)

GZDoom is GPL-3.0-or-later (with parts under other compatible licences: id's
Doom source GPL-2.0-or-later, Raven sources, BSD-licensed parts). If you
**translate GZDoom/ZDoom source code** into Rust — even loosely — the result is
a derivative work and the Room workspace must be distributed under
**GPL-3.0-or-later**. The default project licence is therefore
**GPL-3.0-or-later**. Behavioural tables that are facts (state tables from
`info.c`, line special numbers, sprite names) are taken as data. Keep a
`THIRD_PARTY.md` listing every source a table was derived from.

If the project owner later wants a permissive licence, a clean-room process
is required (one agent writes behavioural specs from the GZDoom source, a
different agent implements from the specs only). These docs are written so
that clean-room implementation is possible.

## 5. How to work

- Implement milestone by milestone (`docs/20-roadmap-testing.md`). Don't start
  a milestone until the previous milestone's acceptance tests pass. Keep
  `docs/21-milestone-progress.md` in sync: check off items, bump Status/%, and
  append a short Log line when a chunk of work lands.
- **Current priority override:** M7 + M10–M13 done; active **M14** (1.0 hardening).
  **M8** when Heretic/Hexen IWADs available; M9 remains deferred (see
  `docs/21-milestone-progress.md` §0).
- Each crate has its own `README.md` stating its responsibility and public API
  surface. Keep public APIs small; prefer `pub(crate)`.
- When a spec is ambiguous, look at the GZDoom source at
  `https://github.com/ZDoom/gzdoom` (paths listed in
  `docs/appendix-b-gzdoom-source-map.md`) and the ZDoom wiki
  `https://zdoom.org/wiki/`. Record the decision in `docs/decisions/NNNN-title.md`
  (ADR format: Context / Decision / Consequences).
- Tests live next to code (`#[cfg(test)]`) for units and in `tests/` for
  integration. Golden files under `testdata/` (never commit commercial IWAD
  content; use Freedoom and generated fixtures — see roadmap doc §Test assets).
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  and `cargo build --target wasm32-unknown-unknown -p room-web` before
  considering a task done.

## 6. Document index

```
docs/
  00-overview.md               goals, scope, feature matrix
  01-architecture.md           crates, data flow, loop, threading
  02-conventions.md            style, errors, numerics, determinism, logging
  03-vfs.md                    WAD/PK3/PK7/dir resource system, IWAD detection, namespaces
  04-binary-formats.md         lumps: maps, graphics, palettes, sounds, music, demos
  05-text-lumps.md             shared scanner + MAPINFO, SNDINFO, GLDEFS, TEXTURES, … (40+ lumps)
  06-maps-geometry.md          map model, node building, blockmap, slopes, 3D floors, portals, polyobjects
  07-playsim.md                tic loop, thinkers, movement, collision, RNG, sector movers
  08-actors.md                 actor model, states, flags, properties, inventory, weapons, players, AI
  09-specials.md               line/sector specials, Doom/Boom/Hexen translation, generalized types
  10-acs.md                    ACS bytecode VM
  11-legacy-mod-formats.md     DECORATE, DeHackEd/BEX, ZScript policy
  12-wasm-scripting.md         the ZScript replacement: WASM modules, ABI, SDK, sandboxing
  13-renderer.md               wgpu renderer: scene, lights, models, portals, post, palette mode
  14-audio.md                  sound system, SNDINFO semantics, music (MIDI/OPL/modules/streams)
  15-input-console-cvars.md    input, bindings, console, commands, CVars, config
  16-ui.md                     2D drawing, fonts, menus, status bars, HUD, automap, intermission
  17-netplay-demos-saves.md    lockstep netcode, demos, save games
  18-platform-web.md           desktop shell, web build, storage, threading on web
  19-compatibility.md          games, compat flags, complevels, quirks database
  20-roadmap-testing.md        milestones, acceptance tests, test assets, CI
  21-milestone-progress.md     living progress checklists for M0–M14 (update often)
  22-room-server.md            multi-room web server (room-proto / room-server)
  23-build-engine.md           Duke Nukem 3D / Build: GRP→UDMF conversion, roadmap
  appendix-a-dependencies.md   allowed crates and policy
  appendix-b-gzdoom-source-map.md  GZDoom source → Room crate mapping
  appendix-c-glossary.md       terms
```

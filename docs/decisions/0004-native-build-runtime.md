# 0004 — Native Build runtime for Duke Nukem 3D

## Context

Room already plays Duke Nukem 3D Atomic Edition by converting `DUKE3D.GRP` into
a UDMF PWAD + DECORATE and running it on the Doom playsim (`room-build`,
`docs/23-build-engine.md`). That path walks the campaign and feels roughly
right, but it cannot reach "full support" as defined in
`docs/25-duke3d-full-support.md` §1:

1. **Geometry** — Build sectors may overlap in 2D and walls move at runtime
   (`dragpoint`). A static Doom BSP cannot represent either without heuristics.
2. **CON** — `GAME.CON` is a program. Mapping opcodes onto Doom AI never reaches
   parity and cannot run user CON mods.
3. **Timing** — Duke is 30 Hz with `krand` / `TRAND`. Room Doom runs at 35 Hz, so
   original `.DMO` demos can never stay in sync.

Ken Silverman's Build engine sources (`ENGINE.C`, …) are under **BUILDLIC**, which
is not GPL-compatible. Translating that engine into Room would block a
GPL-3.0-only distribution. The Duke *game* sources (3D Realms) are
GPL-2.0-or-later and may be translated.

## Decision

1. **Native runtime.** Add a Build world model, clean-room engine primitives, a
   full CON compiler/VM, and vanilla Duke game logic at **30 Hz**, plugged into
   Room shells through a game-agnostic `GameRuntime` trait. Stop extending the
   CON→DECORATE / UDMF converter for gameplay; keep it as an **export** tool
   (editor, Doom-mode mashups).
2. **Crate split.**
   - `room-build-engine` — Build world + clean-room primitives (no Duke
     knowledge). Depends on `room-core` (and shared format types from
     `room-build` where appropriate). Specs live in
     `docs/build-engine-primitives/` and are written by a *different* agent than
     the one that implements them.
   - `room-duke` — CON compiler/VM, actors/player/sector logic, 30 Hz tic,
     saves/demos. Depends on `room-build`, `room-build-engine`, `room-core`.
     **Must not** depend on `room-sim`.
3. **Clean-room engine.** Do not translate BUILDLIC or EDuke32/Raze engine code.
   Behavioural specs from `BUILDINF.TXT`, public docs, and black-box testing
   only. GRP data tables (`TABLES.DAT`, palettes) are loaded at runtime, never
   embedded as copied engine tables.
4. **`GameRuntime`.** Doom `Headless` / `World` and future `DukeRuntime` both
   implement it (`tic_rate`, `tick`, checksum, later view/save/load). Shells use
   `tic_rate()` instead of a hardcoded 35 Hz constant.
5. **Doom-side Duke extensions** in `room-sim` (`ContactDamage`, water portal
   200, …) stay until after D11; then keep as documented GZDoom deviations or
   remove.
6. **Reference traces (D1).** Prefer Chocolate Duke3D for DEMO1–3 traces;
   confirm sync before relying on it. Fallback: DOSBox + original `DUKE3D.EXE`.
7. **WebGL2 classic palookup** for Duke in browsers without WebGPU is an open
   follow-up (docs/25 §12 item 5); D0 does not change the renderer.

Milestones **D0–D11** are tracked in `docs/20-roadmap-testing.md` and
`docs/21-milestone-progress.md`. Spec detail: `docs/25-duke3d-full-support.md`.

## Consequences

- Play continues on the converter path (`play-duke3d.sh`, lobby) until D11
  switches shells to the native runtime.
- CI builds empty `room-build-engine` / `room-duke` for host and
  `wasm32-unknown-unknown` from D0.
- Licensing stays GPL-3.0-or-later for the Room tree; BUILDLIC code never enters
  the repo.
- `THIRD_PARTY.md` will list translated Duke game sources when D5+ lands.

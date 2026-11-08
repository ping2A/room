# room-duke

Native **Duke Nukem 3D** game runtime: CON compiler + VM, vanilla actors /
player / sector logic, 30 Hz tic, saves and demos.

| | |
|--|--|
| Spec | [`docs/25-duke3d-full-support.md`](../../docs/25-duke3d-full-support.md) |
| ADR | [`docs/decisions/0004-native-build-runtime.md`](../../docs/decisions/0004-native-build-runtime.md) |
| Depends on | `room-core`, `room-build`, `room-build-engine` |
| Must **not** depend on | `room-sim` |

The desktop and browser shells now have native `DukeSession` paths; the
GRP→UDMF converter remains available as an explicit export/tool path. Duke
game sources (3D Realms, GPL-2.0+) may be translated; Build *engine* sources
(BUILDLIC) must not.

## Modules

| Module | Status |
|--------|--------|
| `trace` | D1 — parse / format / diff / sprite SHA-256 (`docs/duke-trace-format.md`); CLI: `room duke trace-diff` |
| `con` | D4 compiler (Atomic script SHA = Chocolate) + D5 VM bootstrap (`execute` / `ifrnd` / `state` / …) |
| `session` | Native 30 Hz session, Build sprite lists/RNG, Atomic SAV import/export bridge, multiplayer roster and campaign cursor |
| `actors` / `stat` | D5 — `moveactors` + status-list constants |
| `player` | Native movement, weapons, inventory, damage and view state (parity remains incomplete) |
| `audio` | Deterministic presentation-side `definesound` pitch mapping shared by desktop and browser shells |
| `spawn` | Native map actor classification and Atomic prelevel setup |
| `fta` | Native sleeping-actor wake/recognition path |
| `demo_sync` | DEMO1 GRP+DMO playback → Room traces; last measured frontier tic 134; newer static correction remains unremeasured |
| `demo` | D9 — versioned native `.fdem` recordings of Duke inputs with per-tic state verification; distinct from Atomic `.DMO` |
| `view` | Bootstrap ART/palette-backed Build world → RGBA CPU frame (`DukeSoftRenderer`); shell integration exists, renderer parity remains open |

## Starting a native session

`session_from_grp_map(grp_bytes, map_name, skill)` loads a live map directly
from an Atomic GRP, compiles `GAME.CON`, and initializes `DukeSession` without
requiring a demo. Map names may be `E1L1`, `E1L1.MAP`, or another 1–8 character
Build map lump name; skill is 0–4. `DukeSoftRenderer` renders the native world
to a palette-correct RGBA frame. Desktop and browser shells can start this
native session directly; the Build renderer is still a bootstrap and lacks
several Atomic rendering rules.

## Test script

```bash
./scripts/test-duke-native.sh              # lib + DEMO1 frontier
./scripts/test-duke-native.sh frontier     # frontier only
./scripts/test-duke-native.sh tic23        # water-dive tic check
./scripts/test-duke-native.sh soft-view    # room-cli soft FP PGM
./scripts/test-duke-native.sh all          # build-engine + duke + manifest + frontier
```

Requires `testdata/duke/duke3d.grp` (and DEMO1 demo/trace for frontier). Interactive
play defaults to the native runtime through `./scripts/play-duke3d.sh`; pass
`--runtime converter` for the legacy path.

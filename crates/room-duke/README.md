# room-duke

Native **Duke Nukem 3D** game runtime: CON compiler + VM, vanilla actors /
player / sector logic, 30 Hz tic, saves and demos.

| | |
|--|--|
| Spec | [`docs/25-duke3d-full-support.md`](../../docs/25-duke3d-full-support.md) |
| ADR | [`docs/decisions/0004-native-build-runtime.md`](../../docs/decisions/0004-native-build-runtime.md) |
| Depends on | `room-core`, `room-build`, `room-build-engine` |
| Must **not** depend on | `room-sim` |

Shells keep using the GRP→UDMF converter until D11. Duke game sources
(3D Realms, GPL-2.0+) may be translated; Build *engine* sources (BUILDLIC)
must not.

## Modules

| Module | Status |
|--------|--------|
| `trace` | D1 — parse / format / diff / sprite SHA-256 (`docs/duke-trace-format.md`); CLI: `room duke trace-diff` |
| `con` | D4 compiler (Atomic script SHA = Chocolate) + D5 VM bootstrap (`execute` / `ifrnd` / `state` / …) |
| `session` | D5 — `DukeSession` 30 Hz stub + `SpriteLists` / RNG (no `room-sim`) |
| `actors` / `stat` | D5 — `moveactors` + status-list constants |
| `player` | D5 — `processinput_move` (avel/fvel/svel + friction + clipmove) |
| `spawn` | D5 — coarse post-load sprite status classify |
| `fta` | D5 — `movefta` (wake sleeping actors on cansee) |
| `demo_sync` | D5 — DEMO1 GRP+DMO playback → Room traces; frontier vs Chocolate |

## Test script

```bash
./scripts/test-duke-native.sh              # lib + DEMO1 frontier
./scripts/test-duke-native.sh frontier     # frontier only
./scripts/test-duke-native.sh tic23        # water-dive tic check
./scripts/test-duke-native.sh soft-view    # room-cli soft FP PGM
./scripts/test-duke-native.sh all          # build-engine + duke + manifest + frontier
```

Requires `testdata/duke/duke3d.grp` (and DEMO1 demo/trace for frontier). Interactive
play still uses `./scripts/play-duke3d.sh` (converter) until D11.

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
| `view` | ART/palette-backed Build world → RGBA CPU frame (`DukeSoftRenderer`); supports host-mounted DEF/PNG/BMP tiles and map-hash-selected MHK sprite angle offsets; renderer parity remains open |

## Starting a native session

`session_from_grp_map(grp_bytes, map_name, skill)` loads a live map directly
from an Atomic GRP, compiles `GAME.CON`, and initializes `DukeSession` without
requiring a demo. Map names may be `E1L1`, `E1L1.MAP`, or another 1–8 character
Build map lump name; skill is 0–4. `DukeSoftRenderer` renders the native world
to a palette-correct RGBA frame. Desktop and browser shells can start this
native session directly; the Build renderer is still a bootstrap and lacks
several Atomic rendering rules.

### Loading an EDuke32 CON mod (experimental)

The GRP session loader can compile an external CON root and resolve its
includes through a host-provided `IncludeResolver`. Select the EDuke32 dialect
explicitly with
`session_from_grp_layers_with_con_and_includes_dialect`; the regular loader
continues to use the Atomic dialect. The EDuke32 path initializes its VM before
map sprite setup so level hooks can run against the live world.

`duke_play` can load a local directory whose root contains `eduke.con` (or
`game.con`) and whose CON includes are under that directory. With the bundled
Brutal Duke tree, inspect the compile inventory and run the current E1L1
headless sweep with:

```bash
cargo run -p room-cli -- duke mod-check testdata/brutal-duke-v0.2.0/brutal \
  --dialect eduke32 --md /tmp/brutal-duke-mod-check.md
ROOM_DUKE_MOD=testdata/brutal-duke-v0.2.0/brutal \
  cargo run -p room-duke --example duke_play -- kill-all /tmp/duke-kill-all E1L1
```

The mod check verifies parsing and inventories unsupported commands; it does
not prove full gameplay compatibility. The headless scenario currently reports
enemy damage but does not yet pass the ED5 full weapon acceptance check.

Desktop launch with `--con <mod>/eduke.con` also scans that mod directory for
DEF files and referenced PNG/BMP assets. Hosts that already mount mod files can
pass them directly to `DukeSoftRenderer::from_grp_layers_with_mod_assets`; the
renderer keeps image dimensions, maps RGB through the GRP palette, and applies
DEF alpha cutoffs and sprite offsets, then applies `copytile`, dummy tile and
animation-range declarations in source order. For MHK support, pass files with
`DukeSoftRenderer::from_grp_layers_with_mod_assets_and_mhks`, then call
`load_map_hack_for_map(map_name, map_bytes)` once the map is selected. The
renderer finds the matching DEF `mapinfo` entry using its filename and MD4,
loads the referenced `.mhk`, and applies supported `angoff` adjustments only
while rendering. Model transforms and Polymer lights are parsed but still need
the ED7 renderer. The desktop sound bank also resolves loose WAV/OGG sound
files from the same mod directory. The browser editor’s asset mount is still
pending.

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

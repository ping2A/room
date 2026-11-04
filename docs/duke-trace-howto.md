# Generating Duke reference traces (out of tree)

Room cannot ship a patched Chocolate Duke3D binary: Ken's engine is
**BUILDLIC**. Keep the instrumented reference **outside** this repo. Only the
NDJSON traces and their SHA-256 enter git (via
[`testdata/duke-trace-manifest.toml`](../testdata/duke-trace-manifest.toml)).

## Emitter contract

Prefer the ready-made hook in
[`third_party/chocolate-duke3d-trace/`](../third_party/chocolate-duke3d-trace/)
(`room_trace_after_tic()` at the end of `domovethings()`). It writes NDJSON
matching [`duke-trace-format.md`](duke-trace-format.md) when `ud.recstat == 2`.

```text
DUKE_TRACE 1
meta demo=DEMO players=1 engine=chocolate
```

## Produce DEMO1–3

0. Extract demos from a local GRP (git-ignored tree):

```bash
room duke extract-demos testdata/duke/duke3d.grp -o testdata/duke/demos
```

1. Run Atomic Edition with those demos in the patched reference. Prefer
   **Docker `linux/386`** (see `third_party/chocolate-duke3d-trace/README.md`);
   64-bit Chocolate builds crash on pointer truncation. Use DOS flags:
   `/dDEMO1.DMO /ns /nm`, with `ROOM_TRACE=…` and `SDL_AUDIODRIVER=dummy`
   under `xvfb-run`.
2. Traces land in `testdata/duke/traces/DEMO1.trace` (etc.). That directory is
   git-ignored with the GRP; only SHA-256 hashes are committed.
3. Hash and update the manifest:

```bash
room duke trace-hash testdata/duke/traces/DEMO1.trace
# paste lowercase hex into testdata/duke-trace-manifest.toml
room duke trace-check-manifest testdata/duke-trace-manifest.toml
```

4. Later, `room-duke` emits the same format; CI with a local GRP+traces runs
   `room duke trace-diff` against the reference files.

## Soft check without assets

`room duke trace-check-manifest` exits **0** and prints `missing` for empty
`sha256` or absent files, so default CI stays green. Exit **1** only when a
file exists but its hash does not match the manifest.

## krand log (per-call RNG diff)

Trace lines only say *that* the seed diverged at a tic. To see *which call*
differs, log every `krand()`:

1. `python3 third_party/chocolate-duke3d-trace/apply_krand_log.py $CHOCOLATE_DUKE`
   (adds the logging hooks; idempotent), and copy `room_trace.c` from this
   directory (it writes `# endtic` markers).
2. Build 32-bit without Docker on x86-64 Linux:
   `third_party/chocolate-duke3d-trace/build_i386_native.sh` (multilib + i386
   SDL runtime; header comments list the apt packages). The result reproduces
   `DEMO1.trace` byte for byte.
3. Run with `ROOM_NOSOUND=1 SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy
   ROOM_KRAND_LOG=krand1.log ./chocolate-duke3d /ns /nm /dDEMO1.DMO`.
4. `cargo run --release -p room-duke --example duke_frontier -- krand DEMO1 krand1.log`
   prints the first call where the seeds differ, with Room's call site
   (`file:line` via `#[track_caller]`) and Chocolate's (`addr` + `g_i`; resolve
   with `addr2line -f -p -e chocolate-duke3d <addr-1>`). `KRAND_ALL=1` lists the
   whole tic.

`duke_frontier real` gives the honest frontier (no `trace_fixups`).
`duke_frontier legacy` gives the old fix-up path.

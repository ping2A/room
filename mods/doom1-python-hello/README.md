# doom1-python-hello — desktop Python RoomMod

Mirrors [`testdata/mods/hello_as.ts`](../../testdata/mods/hello_as.ts): greet on
load, spawn an Imp after 3 seconds, heal the player on kills.

This runs as a **host CPython** module inside desktop `room-app` (not a WASM
guest). It is **not netplay-safe**. For multiplayer, port the same logic to
AssemblyScript (`mods/doom1-bounty`) or Rust (`room-script-sdk`).

| Hook | WASM (AS / Rust) | This file |
|------|------------------|-----------|
| Map start | `onLoad` / `Event::Loaded` | `on_load` |
| Per tic | `onTick` | `on_tick` |
| Kill | `onThingDied` | `on_thing_died` |
| Print / spawn | ABI imports | `self.print` / `self.spawn` |

## Desktop (real game)

```bash
# Needs a local Python (3.10+) discoverable by pyo3 / pkg-config.
./scripts/pack-python-mod.sh mods/doom1-python-hello

PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 \
  cargo run -p room-app --features python-mods -- \
  --iwad testdata/doom/doom1.wad \
  --file mods/doom1-python-hello/hello.wad \
  --play
```

Without `--features python-mods`, the engine prints that Python mods were found
but skipped.

## Educational REPL (headless, no WAD pack)

```bash
./scripts/setup-python-edu.sh
python -m room mods/doom1-python-hello/hello_mod.py --skill 4
```

API reference: [`docs/12-wasm-scripting.md`](../../docs/12-wasm-scripting.md).
Host crate: `crates/room-python-host`.

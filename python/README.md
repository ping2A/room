# Room — educational Python bindings

Headless playsim access from CPython, plus a **Python `RoomMod` host** that
mirrors the WASM guest API (`docs/12-wasm-scripting.md`).

| Use this for… | Use WASM (Rust / AssemblyScript) for… |
|---------------|----------------------------------------|
| Classrooms, REPL exploration, prototyping mod logic | Netplay, servers, shipped PWADs |

The extension crate is **not** a Cargo workspace member (building it needs a
local Python). Install with Maturin:

```bash
./scripts/setup-python-edu.sh
# or: cd python && python3 -m venv .venv && source .venv/bin/activate
#        pip install maturin && maturin develop --release
```

## Quick start

```python
from room import Session
from room.host import run_mod
from room.sdk import ModBase, TICRATE

session = Session.arena(skill=4)
session.cheat_give("all")

class Pulse(ModBase):
    def on_load(self, maptime: int) -> None:
        self.print("arena ready")

    def on_tick(self, t: int) -> None:
        if t == TICRATE:
            self.spawn("DoomImp")

msgs = run_mod(session, Pulse(), tics=35 * 2, on_message=print)
```

Example mod: [`mods/doom1-python-hello`](../mods/doom1-python-hello).

For the **desktop game** (same mod packed as a PWAD), see that README’s
“Desktop (real game)” section — `room-app --features python-mods`.

```bash
python -m room mods/doom1-python-hello/hello_mod.py --skill 4
```

## Layout

| Path | Role |
|------|------|
| `room/_native` (Rust/`pyo3`) | `Session`, `ActorView`, cheats, spawn |
| `room/sdk.py` | `RoomMod` / `ModBase`, `TICRATE`, `Actor` |
| `room/host.py` | Dispatches load / tick / deaths like the WASM host |
| `src/lib.rs` | PyO3 extension sources |

## Licence

GPL-3.0-or-later (same as Room).

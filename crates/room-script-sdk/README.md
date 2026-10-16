# room-script-sdk

Guest-side Rust SDK (`#![no_std]`, `alloc` opt-in feature) for Room WASM play/UI modules.

## Game mods

`game` wraps every game import (`Actor`, `spawn`, `player`, `print`, `random`,
`skill`, `level_stat`, …) and `export_mod!` turns a `RoomMod` into a module:

```rust
use room_script_sdk::game::{self, Event, RoomMod, event_mask};
use room_script_sdk::{class_find, export_mod};

#[derive(Default)]
struct Hello;

impl RoomMod for Hello {
    const NAME: &'static str = "Hello";
    const EVENTS: u64 = event_mask::WORLD_LOADED | event_mask::WORLD_THING_DIED;
    fn on_event(&mut self, event: Event) {
        match event {
            Event::Loaded { .. } => game::print(None, "hello"),
            Event::ThingDied { source: Some(s), .. } => {
                let _ = s.give(class_find("Stimpack"), 1);
            }
            _ => {}
        }
    }
}

export_mod!(Hello);
```

Full example: `mods/doom1-survival`. The same game API exists for AssemblyScript
(`crates/room-web/as/room.ts`, example `mods/doom1-bounty`, docs/12 §16).

## Status (M11)

- Host wrappers: `log`, `level_time`, `player_count`, `actor_exists`, `name_intern`, `class_find`
- Required exports: `room_abi_version`, `room_alloc` (bump), `room_free`
- Lifecycle: `export_module!` → `room_register` / `room_init` / `room_dispatch`
- Macros: `#[actor]`, `#[register_stub]` (Actor derive / `states!` still open)

```rust
use room_script_sdk::{export_module, log, level_time, LOG_INFO, register_stub};

#[register_stub]
pub struct FireImp;

export_module!(classes = [FireImp]);

fn tick() {
    log(LOG_INFO, "hello");
    let _t = level_time();
}
```

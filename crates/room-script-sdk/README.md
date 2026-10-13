# room-script-sdk

Guest-side Rust SDK (`#![no_std]` + `alloc`) for Room WASM play/UI modules.

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

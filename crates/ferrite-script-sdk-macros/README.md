# ferrite-script-sdk-macros

Proc-macros for the Ferrite WASM guest SDK (`docs/12` §11).

## Status (M11)

- `#[actor]` — placeholder (keeps the item; full derive later)
- `#[register_stub]` — adds `__ferrite_register() -> i32` for `export_module!(classes = […])`

Re-exported from `ferrite-script-sdk`.
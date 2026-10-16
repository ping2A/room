# room-script

WASM host runtime for Room play/UI modules (`docs/12-wasm-scripting.md`).

## Status (M11)

- Validation: single memory, reject memory64/shared/foreign imports, require §5.1 exports
- `WasmBackend` / `WasmInstance` / `GuestLifecycle`
- `wasmi` (default) and `wasmtime` (`--features wasmtime`) backends
- Host imports: common + `reg_class_*` / `reg_state*` (`RegistrationSession`)
- Game imports (`game.rs`, `GameFn` + `GameBridge`): 44 functions for actors,
  players, level, math and RNG; the bridge is `room_sim::script_mods::ModRuntime`
  (docs/12 §15)
- Samples: `hello_mod`, `fireimp_reg`, `event_tick`, `script_call`, `ui_tick`, `save_counter`

```bash
cargo test -p room-script --lib
cargo test -p room-script --features wasmtime --lib
```

Next: class/state registration ABI, events/thinkers, conformance checksums.

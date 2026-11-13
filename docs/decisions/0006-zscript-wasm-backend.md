# ADR 0006 — ZScript bytecode → WebAssembly backend

Date: 2026-11-03. Status: accepted; ZW0–ZW3 implemented 2026-11-13 (`docs/37` §0.1). Amends: ADR 0005 (the "ZScript→WASM
transpiler stretch goal is dropped" line).

## Context
ADR 0005 chose a native ZScript VM (`room-zscript` + `room-zvm`) and dropped
the transpiler. Two needs remain open:
- **Speed on web.** A JIT cannot be built inside the wasm32 engine.
- **ZS9.** The planned desktop JIT is a bespoke Cranelift backend.

Separately, mod authors want to ship ZScript mods precompiled and
sandboxed.

## Decision
Add `room-zvm-wasm`, which compiles **ZVM bytecode** (not ZScript source)
to WebAssembly (`docs/37`):
- `Target::Aot` is Tier 2 of the VM, running on wasmtime/Cranelift or the
  browser JIT and sharing the engine heap;
- `Target::RoomMod` emits ABI v1 mods linked with the `room-zrt` runtime.

The interpreter stays the semantic reference, and differential tests gate
every change. ZS9 (custom Cranelift JIT) is superseded by ZW3.

## Consequences
- **Positive:**
  - one front end and one semantics;
  - JIT speed on desktop and web without writing a JIT;
  - ZScript becomes a source language for WASM mods.
- **Negative:**
  - two executors to keep in sync, mitigated by differential tests and the
    shared opcode table;
  - import overhead for heap operations, mitigated by ZW5;
  - the interpreter changes its fuel accounting to per-block, which changes
    fuel-exhaustion points for existing tests.

## Amendment (2026-11-13)
`Target::RoomMod` keeps the heap and natives in the engine instead of a
`room-zrt` guest runtime (`docs/37` §0.2, chosen by the project owner): a
converted mod ships its compiled code and program image, runs through the
AOT tier, and is not an ABI v1 ROOMMOD.

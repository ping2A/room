# room-zscript

ZScript compiler front end for Room (`docs/34-zscript-vm.md`).

## Responsibility

- Lex, preprocess (`#include`, `version`), parse → AST
- Semantic analysis (types, scopes, classes, layouts, vtables)
- Emit `room-zvm` bytecode
- Diagnostics with file:line:column

## Public API

- [`parse_file`] / [`parse_sources`] — ZS0 parse
- [`analyze`] — ZS1 type-check against Room stdlib declarations
- [`compile`] — ZS2 bytecode program
- [`check`] — CLI-facing compile-or-parse pipeline

WASM modules (`docs/12`) remain the recommended API for *new* mods; this crate
exists so GZDoom ZScript mods (first: Brutal Doom) run unmodified.

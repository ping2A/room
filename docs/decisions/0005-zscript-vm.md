# 0005 — Execute ZScript with an engine VM

**Status:** accepted (2026-11-01). Supersedes `docs/11-legacy-mod-formats.md` §3.1–3.2.

## Context
`docs/11` §3 decided that Room would not execute ZScript and would offer WASM
modules plus an optional ZScript→WASM transpiler. Running popular GZDoom mods
unmodified (first target: Brutal Doom v22, `docs/33`) requires their ZScript to
run. Hand-written compatibility modules per mod don't scale.

## Decision
Room compiles ZScript at load time with its own front end (`room-zscript`) and
executes it in an engine VM (`room-zvm`): register bytecode, deterministic
heap and GC, native binding layer implemented incrementally with coverage
reports, optional Cranelift JIT on desktop. Specification: `docs/34-zscript-vm.md`.
WASM modules remain the recommended API for new mods.

## Consequences
- New crates and a large native binding surface, prioritised by a mod corpus.
- `docs/00` non-goals, `docs/11` §3, `docs/12`, `docs/05` are updated.
- Per-mod compatibility modules (e.g. the earlier `compat-brutaldoom` idea) are
  no longer planned.
- The ZScript→WASM transpiler stretch goal is dropped.

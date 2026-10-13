# ferrite-core

Shared primitives used by every other Ferrite crate.

## Public API

- `Name` / `NameTable` — case-insensitive interned strings (GZDoom `FName`)
- Typed IDs (`ClassId`, `StateId`, `TextureId`, …)
- `Arena` — generational handles (`TObjPtr`-style stale-to-`None`)
- `DVec2` / `DVec3`, `Angle`, `Bam`, `Fixed`
- `dmath` — portable `sin`/`cos`/`tan`/`atan2`/`sqrt`/`pow`/`exp`/`log`
- `conv` — saturating float→int helpers (`xs_CRoundToInt`, `xs_FloorToInt`)
- `rng` — SFMT-607, named `FRandom` set, vanilla `rndtable`, `UiRng`

This crate is deterministic: sim code must not call `std` transcendentals or iterate `HashMap`.

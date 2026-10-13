# 02 — Conventions (mandatory)

## 1. Code style

- `rustfmt` default config; `clippy::pedantic` warnings allowed to be selectively disabled per crate in `lib.rs` with justification.
- Workspace lints (in root `Cargo.toml`):
  ```toml
  [workspace.lints.rust]
  unsafe_code = "deny"          # crates that need it override with #![allow(unsafe_code)] + SAFETY comments
  missing_docs = "warn"
  [workspace.lints.clippy]
  unwrap_used = "deny"          # in non-test code
  expect_used = "warn"
  float_arithmetic = "allow"    # but see §4 for sim restrictions
  ```
- Public types get doc comments that cite the GZDoom equivalent where one exists:
  `/// Equivalent of GZDoom's AActor::flags (MF_*).`
- No global mutable state (`static mut`, `lazy_static` with mutex) except logging and the panic hook.

## 2. Errors

- Library crates use `thiserror` enums; binaries use `anyhow`-style reporting (`color-eyre` optional, pure Rust).
- Parsers return `Result<T, ParseError>` with spans. A parse may also return
  warnings via a `Diagnostics` sink (`&mut dyn DiagnosticSink`) — this mirrors
  GZDoom printing a warning and continuing.
- Never `panic!` on input data. `debug_assert!` for internal invariants only.

## 3. Identifiers & interning

- `Name`: case-insensitive interned string (ZDoom `FName`). ASCII case folding only.
  Stored as `u32` index into `NameTable`. `Name::NONE = 0`.
- Typed IDs: `ClassId(u32)`, `StateId(u32)`, `TextureId(u32)`, `SoundId(u32)`, `LumpId(u32)`, `SectorId(u32)`, `LineId(u32)`, `SideId(u32)`, `VertexId(u32)`.
- Runtime object handles: `ActorId { index: u32, generation: u32 }` (generational arena). A stale handle resolves to `None` — this is how ZDoom's `TObjPtr` "read barrier → null when destroyed" semantic is reproduced.
- Lump names: 8-character uppercase ASCII for WAD lumps, full paths (case-insensitive, `/`-separated) for archive files.

## 4. Numerics

### 4.1 Types

| Concept | Type | Notes |
|---|---|---|
| Sim coordinates | `f64` (ZDoom uses `double`) | see determinism rules |
| Legacy-compat coordinates | `Fixed` (`i32` 16.16) | used only in `compat_strict`/vanilla-demo mode paths |
| Angles | `Angle` newtype over `f64` degrees (GZDoom `DAngle`), plus `BAM` (`u32`) for vanilla paths | conversion functions exact for multiples of 1/2^32 turns |
| Render math | `f32` / `glam` | renderer only |
| Audio | `f32` | audio only |

`ferrite-core::math` provides `DVec2`, `DVec3` (thin wrappers around `[f64; N]`
with explicit ops — **not** `glam::DVec*`, whose SIMD paths and `mul_add` usage
may vary by target feature).

### 4.2 Determinism (sim, ACS, WASM-in-sim)

All code reachable from `World::tick` must obey:

1. **IEEE-754 basic ops only**: `+ - * /`, `sqrt` (correctly rounded by IEEE), comparisons, conversions. These are deterministic across x86-64 (SSE2), aarch64 and wasm32.
2. **No fused multiply-add** unless explicit `mul_add` is written *and* accepted in an ADR (it is not by default). Ensure no `-C target-cpu=native` for release builds of the sim; `fma` target-feature off. Rust does not auto-contract, so plain `a*b+c` is safe.
3. **Transcendentals** (`sin`, `cos`, `atan2`, `pow`, `exp`, `log`) come from `ferrite_core::dmath`, a pure-Rust, portable implementation (port of a correctly-rounded or fixed-algorithm libm such as the `libm` crate with no target-specific code paths). Never `f64::sin` from std (it calls the platform libm).
   Additionally GZDoom uses lookup tables for some paths (`finesine`, `finetangent` for vanilla behaviour); include those as `const` tables for compat paths.
4. **Iteration order**: no `HashMap`/`HashSet` iteration in sim. Use `Vec`, `IndexMap` with insertion order, or `BTreeMap`. `HashMap` lookups (no iteration) are fine with a fixed hasher (`foldhash`/`ahash` seeded constant) — prefer `IndexMap`.
5. **No time, no threads, no I/O** in a tic. Random numbers only from `World::rng` (see `07-playsim.md §RNG`).
6. **NaN handling**: sim code must not produce NaN. Where division may hit zero, guard explicitly. WASM backends run with NaN canonicalization enabled.
7. **Float → int conversions** use explicit helpers mirroring GZDoom (`xs_CRoundToInt`, `xs_FloorToInt`), implemented with Rust's saturating `as` on the result of `floor`/`round` — define once in `ferrite_core::conv`.

Determinism is enforced by CI: the same demo is run on x86-64 Linux, aarch64 macOS and wasm32 (Node/wasmtime) and the per-tic world checksums must match (`20-roadmap-testing.md`).

## 5. Logging & diagnostics

- `tracing` for structured logs. Targets per crate (`ferrite_sim`, …).
- Console output (`Printf` equivalents) goes through `ferrite_console::print(level, msg)` which also forwards to `tracing`.
- Colour codes: GZDoom's `\c` escape colour system (`TEXTCOLOR_*`) is preserved in console strings and rendered by the UI; strip when logging.

## 6. Serialization

- Save games and snapshots: `serde` + a compact binary format (`postcard` or `bincode` v2) inside a ZIP container (see `17`). Every serialized struct has an explicit version field or lives behind a versioned envelope.
- Never derive `Serialize` on types that contain `Arc<Definitions>` pointers; serialize IDs and re-resolve on load. IDs must be stable for a given load order; saves store the load-order fingerprint and refuse mismatches (with a "load anyway" override).

## 7. Testing conventions

- Unit tests next to code; property tests (`proptest`) for math and parsers; fuzz targets for every parser.
- Golden tests: `insta` snapshots for parsed definition dumps (YAML/RON); PNG goldens with perceptual tolerance for the renderer.
- Never require commercial IWADs in CI. Gated tests (`#[ignore]` + env `FERRITE_IWAD_DIR`) may use them locally.

## 8. Feature flags

Cargo features are additive and documented in each crate README. Mandatory set:

| Crate | Feature | Effect |
|---|---|---|
| ferrite-vfs | `7z` | PK7 support |
| ferrite-script | `wasmtime` (default on native), `wasmi` (default on web, available everywhere) | runtime backends |
| ferrite-music | `opl`, `soundfont`, `tracker`, `gme` | music engines |
| ferrite-render | `models-iqm`, `postfx` | optional pipelines |
| ferrite-app | `gamepad` | gilrs input |

## 9. Commit/PR hygiene

One logical change per commit. Commit message: `crate: imperative summary`. Every behavioural port of GZDoom logic cites the source file and function in the commit body.

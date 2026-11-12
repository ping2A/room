# 34 — ZScript in Room: compiler and VM (`room-zscript`, `room-zvm`)

**Audience:** an external coding agent working in the Room repo. Read
`AGENTS.md`, `docs/01-architecture.md`, `docs/02-conventions.md`
(§Determinism), `docs/05-text-lumps.md` (load pipeline), `docs/08-actors.md`,
`docs/10-acs.md`, `docs/11-legacy-mod-formats.md`, `docs/12-wasm-scripting.md`
first. Decision record: `docs/decisions/0005-zscript-vm.md`.
**Status:** **ZS0**–**ZS8**/**ZS10** done (`docs/21`); **ZS9** Cranelift deferred;
formal §9 ms gates deferred. Commercial BD gated on `ROOM_BRUTALDOOM_PK3`.
Milestones `ZS0`–`ZS10` (§12). First acceptance target: Brutal Doom v22 (`docs/33`).

**Goal.** Run GZDoom ZScript mods **unmodified**: compile ZScript source from
the loaded archives at startup and execute it in a fast, deterministic,
sandboxed VM that is part of the engine, on desktop and on the web. WASM
modules (`docs/12`) stay the recommended way to write *new* mods; ZScript
support is for compatibility with the existing GZDoom mod library.

---

## 0. TL;DR

1. This reverses `docs/11` §3 ("ZScript is not executed"). The reasons it gave
   (language size, ~3,000 native bindings) are real, so the design limits the
   cost: one compiler front end, one compact VM, and a **native binding layer
   implemented incrementally, driven by a corpus of real mods** with coverage
   reports.
2. Two new crates: **`room-zscript`** (lexer, preprocessor, parser, semantic
   analysis, class layout, bytecode compiler, diagnostics) and **`room-zvm`**
   (register-based interpreter, object heap with deterministic GC, native
   function table). Integration lives in `room-sim` (play scope) and `room-ui`
   (ui scope).
3. ZScript actors **are** Room actors: no parallel object world. Script fields
   live in a per-actor block; virtual overrides are entered from Rust only when
   a class overrides them (zero cost otherwise).
4. Performance: typed registers, link-time field/method resolution, vtables,
   superinstructions, fuel counting; optional **Cranelift JIT on desktop**
   (Cranelift is already in the tree through `wasmtime`) with bit-identical
   semantics. Web runs the interpreter (compiled into the engine's wasm32).
5. The same compiler also compiles **DECORATE expressions and anonymous action
   blocks** to bytecode, replacing today's string re-evaluation
   (`deco_eval.rs`), which helps `docs/33` too.

---

## 1. Goals and non-goals

**Goals**
- Language coverage of GZDoom ZScript up to the current 4.x line (versioned:
  each file's `version "x.y"` gates features, as in GZDoom).
- Behaviour compatibility for play-scope code (actors, inventory, weapons,
  event handlers, thinkers) and ui-scope code (status bars, HUD, menus).
- Determinism: play scope bit-identical on x86-64, aarch64 and wasm32.
- Speed: a ZScript `Tick` override on 10,000 actors at 35 Hz in ≤ 4 ms/tic
  (desktop, interpreter) and ≤ 1.5 ms with the JIT; ≤ 12 ms on web.
- Saves, demos, netplay (lockstep) and the room-server verifier keep working.
- Good diagnostics: GZDoom-style errors with file:line:column; a
  `room-cli zscript check` tool.

**Non-goals (v1)**
- Bit-exact reproduction of GZDoom's VM internals or JIT.
- Natives with no Room equivalent (renderer-internal APIs, OpenGL specifics):
  they become documented stubs.
- A ZScript debugger UI (hooks only).

---

## 2. Architecture

```
archives ──► room-vfs ──► ZSCRIPT lumps (+ #include)        DECORATE lumps
                               │                                  │
                     room-zscript (front end)                     │
   lexer → preprocessor → parser → AST → semantic analysis        │
   (types, scopes, versions, classes, layouts, vtables)           │
            → IR → register allocation → ZVM bytecode ◄───────────┘ (expressions / action blocks)
                               │
                       CompiledProgram (classes, layouts, vtables, functions, constants, states)
                               │
      ┌────────────────────────┴────────────────────────┐
  room-sim (play scope)                            room-ui (ui scope)
  ClassRegistry + StateTable merge                 StatusBar, menus, HUD drawing
  actor script fields, virtual hooks, events       client-local, not lockstep
      │                                                  │
      └──────────► room-zvm: interpreter, heap/GC, natives ◄┘
```

Load order (GZDoom): ZScript is compiled **before** DECORATE in the
`Definitions` pipeline (`docs/05` §2 phase 7); DECORATE classes may inherit
ZScript classes; ZScript refers to DECORATE classes only by name at run time.
Both feed one `ClassRegistry`/`StateTable`. WASM module classes register after
both (unchanged).

---

## 3. Language coverage (front end, `room-zscript`)

Reference grammar: GZDoom `src/common/scripting/frontend/zcc-parse.lemon`,
`zcc_parser.cpp`, `zcc_compile.cpp`, `codegeneration/codegen.cpp`; standard
declarations in `wadsrc/static/zscript/`. Implement in Rust (hand-written
recursive-descent + Pratt expressions; no C build steps).

| Area | Must support |
|---|---|
| Lexing | GZDoom token set incl. `name` literals `'Foo'`, string concatenation of adjacent literals, numeric suffixes, `#include`, `version "4.x"` header, `//`/`/* */` comments, case-insensitive keywords and identifiers |
| Top level | `class` (with `: Parent`, `replaces`, `abstract`, `native`, `ui`/`play`, `version(...)`), `struct`, `enum`, `const`, `extend class/struct`, `mixin class` + `mixin X;`, `include` |
| Class bodies | fields (`readonly`, `transient`, `internal`, `private`/`protected`, `meta`, `native`), `Default { … }` (properties, `+FLAG`/`-FLAG`), `States { … }` (same syntax as DECORATE + anonymous functions + `Light()`, `Offset()`, `Fast`, `Slow`, `NoDelay`, `CanRaise`), `Property name: field, …;`, `FlagDef name: field, bit;`, methods |
| Methods | `static`, `virtual`, `override`, `final`, `action`, `native`, `deprecated("v")`, `version("v")`, `clearscope`/`play`/`ui`/`virtualscope`, `const`, `out`/`ref` params, default args, multiple return values, varargs (natives) |
| Statements | blocks, `let`, local declarations with multi-assign, `if/else`, `for`, `while`, `do…while/until`, `switch/case/default`, `break`, `continue`, `return`, `foreach` (4.x), static arrays initialisers |
| Expressions | full operator table (`** ~== <>= >>> ..` string concat, vector ops, `dot`/`cross`, `is`, casts `Class(obj)`, `(type)` casts, ternary, `sizeof`/`alignof`, `null`, `self`, `invoker`, `Super.`), member/array access, function calls with named args |
| Types | `int`, `uint`, `int8/16`, `uint8/16`, `bool`, `float`/`double` (64-bit), `name`, `sound`, `color`, `string`, `state`, `StateLabel`, `SpriteID`, `TextureID`, `class<T>`, `vector2/3/4`, `quat`, `fvector*`, enums, structs (value + native reference), `Array<T>`, static arrays, `Map<K,V>`/`MapIterator` (4.10+), `Dictionary` (native), `readonly<T>` |
| Semantics | scope checking (play/ui/data scope rules, `clearscope`), const-correctness, access control, version gating, implicit conversions as GZDoom, `Super` calls, virtual/override checks, `action` function `self`/`invoker` binding, state label resolution (`Goto Super::X`, `ResolveState`), `replaces` chains |

Error behaviour: like GZDoom, a compile error aborts loading **that mod** with
all diagnostics printed (file:line:col, source line, caret). Warnings for
deprecated features honour the file's version.

---

## 4. The VM (`room-zvm`)

### 4.1 Bytecode
- **Register machine.** Each function frame has typed register banks:
  `i` (i64, used for int/uint/bool/name/sound/color/enum), `f` (f64),
  `s` (string handles), `a` (object/actor/struct/array references). Separate
  banks keep GC roots precise and avoid tagging.
- **Encoding:** 32-bit instructions `op:8 | a:8 | b:8 | c:8` with an
  extension word for wide operands and constants; constant pools per bank.
- **Core ops:** moves, arithmetic/compare/branch (fused compare-and-branch),
  conversions, vector ops on register triples, string ops (concat, compare),
  field load/store with **pre-resolved offsets**, array ops (bounds-checked),
  calls (`CALL_STATIC`, `CALL_VIRTUAL vtable_index`, `CALL_NATIVE id`,
  `RETURN`), `NEW`, casts/`IS` (class ancestry check by precomputed ancestor
  bitsets / depth+id tables), state jumps for action functions.
- **Superinstructions** chosen from profiling of the mod corpus, e.g. "load
  `self` native field", "compare int field with constant and branch", "call
  native with ≤ 3 args from registers".

### 4.2 Calls and dispatch
- Static calls resolved at link time; virtual calls through per-class vtables
  (indices fixed at compile time); `final`/non-overridden virtuals
  devirtualised.
- Natives: `type NativeFn = fn(&mut ZvmCtx, args: &[Reg], out: &mut [Reg]) -> Result<(), Trap>`;
  table built at startup from the binding layer (§6); signature checked
  against the ZScript `native` declaration at link time (mismatch = load
  error, never UB).
- Recursion limit and **fuel** (instruction budget per call / per tic) like WASM
  (`docs/12` §4.2): GZDoom hangs on an infinite ZScript loop; Room aborts with a
  diagnostic (deterministic, so all peers abort identically).

### 4.3 Objects, memory, GC
- **Actors** are Room actors (`World.actors`); the ZScript reference to an
  actor is the `ActorId` (index + generation). Script-declared fields of
  ZScript actor classes live in a per-actor **script field block** (a
  `Box<[Reg]>`-like slab laid out by the compiler) stored with the actor.
  Native fields (`pos`, `vel`, `angle`, `health`, `target`, flags `bFOO`, …)
  map to Rust fields through generated accessors; the hottest get dedicated
  opcodes.
- **Non-actor Objects** (`Object`, `Thinker`, `EventHandler`, user classes like
  `ZC_IntStrMap`, `Array` backing stores) live in a `room-zvm` heap: arena +
  generational handles.
- **Destroyed objects read as null** (GZDoom semantics): every dereference
  checks the generation; `Destroy()` bumps it.
- **GC:** precise, non-moving mark-sweep, run **between tics** at a
  deterministic point with an incremental budget; roots = thinker lists,
  event handlers, globals/statics, actor script blocks, live VM frames
  (none between tics). GC never depends on addresses or wall-clock time.
- **Strings:** immutable, reference-counted, copy-on-write semantics like
  GZDoom `FString`; deterministic hashing (fixed seeds) for `Map`/`Dictionary`.
- **Names** share Room's global name table (`NameRef`).

### 4.4 Numeric determinism
- f64 IEEE operations only; no fused multiply-add unless explicitly modelled;
  transcendental functions (`sin`, `cos`, `atan2`, `sqrt`, `exp`, `pow`, …)
  from `room_core::dmath`; NaN canonicalised where it could become observable
  (serialisation, bit casts).
- Random: `Random`, `FRandom`, `Random2`, `RandomPick`, named RNGs map to the
  playsim RNG (`World::p_random`, per-name streams in non-vanilla mode).

### 4.5 Errors (traps)
Null dereference, array out of bounds, division by zero, failed cast, fuel
exhaustion, stack overflow → `Trap` with ZScript stack trace (function,
file:line). Policy as WASM `script_trap_policy` (`docs/12` §4.2): default
single-player "abort the action, print the error, keep playing" where GZDoom
would abort; netplay deterministic abort for all peers. A
`zscript_trap_policy = gzdoom` option reproduces GZDoom's fatal error.

### 4.6 Speed tiers
- **Tier 0 — interpreter** (all platforms, including the web build). Rust
  `match`-based dispatch over a compact `Vec<u32>`, no allocation in the
  loop, registers in a reusable frame stack.
- **Tier 1 — Cranelift JIT** (desktop, optional feature `zvm-jit`): compiles
  bytecode functions that exceed a call-count threshold. Same semantics
  (natives called through the same table, same trap points, same fuel
  accounting at basic-block granularity). A conformance mode runs both tiers
  and compares results.
- **Later (evaluate):** on the web, compile hot functions to a WASM module at
  load (`wasm-encoder` is in the tree) and run them on the browser's JIT, with
  actor field access through a shared-memory view. Only if Tier 0 misses the
  web budget.
- **Compiler optimisations:** constant folding, dead code elimination,
  copy propagation, register allocation (linear scan), devirtualisation,
  inlining of trivial natives (`min`, `max`, `abs`, `clamp`), loop-invariant
  field loads where provably safe.

---

## 5. Engine integration

### 5.1 Classes, defaults, states
- `ClassRegistry` gains ZScript classes with their `Default` blocks (same
  property/flag tables DECORATE uses — `docs/33` §3.4/§3.5 completes those),
  `Property`/`FlagDef` declarations (user properties/flags mapped to script
  fields), `replaces`, `meta` fields (per-class storage).
- States compile into the shared `StateTable`; anonymous functions and
  `action` functions are ZVM functions referenced from the state
  (`State.action = Action::Zvm(func_id)`; DECORATE states use the same path).
- **DECORATE expressions** (`A_JumpIf(x > 1, …)`, `random(…)` arguments,
  `{ … }` blocks) compile through the same front end into ZVM bytecode at load,
  replacing per-call string evaluation in `deco_eval.rs`.

### 5.2 The ZScript "standard library"
GZDoom defines its engine classes in ZScript with `native` members
(`wadsrc/static/zscript/*.zs`: `Object`, `Thinker`, `Actor`, `Inventory`,
`Weapon`, `PlayerPawn`, `EventHandler`, `StatusBar`, menus, structs like
`LevelLocals`, `PlayerInfo`, `CVar`, `Wads`, `Console`, `String` methods,
`Array`, …).
Room ships **`wadsrc/room/zscript/`**: the same declarations (names,
signatures, field names) so mods compile unchanged, where:
- `native` members bind to Rust (`room-sim`, `room-ui`, `room-vfs`, …) through
  the binding layer (§6);
- non-native method bodies are ported from GZDoom's ZScript (GPL-3.0 —
  compatible with Room's licence; record in `THIRD_PARTY.md`);
- Room's Rust behaviour stays authoritative for core gameplay
  (inventory/weapons/monsters are native in Room), so many GZDoom ZScript
  bodies become `native` in Room's version.

### 5.3 Virtual hooks from Rust
Per class, the compiler records which virtuals are overridden
(`Tick`, `BeginPlay`, `PostBeginPlay`, `Activate`, `Deactivate`, `Die`,
`DamageMobj`, `TakeSpecialDamage`, `Touch`, `CanCollideWith`, `CanPass`,
`SpecialMissileHit`, `OnDestroy`, `TryPickup`, `HandlePickup`, `Use`,
`DoEffect`, `AttachToOwner`, `DetachFromOwner`, `ModifyDamage`,
`AbsorbDamage`, `GetSpeedFactor`, `CreateCopy`, `CreateTossable`, weapon
`GetUpState`/`GetReadyState`, …). Rust checks a bitset and enters the VM only
for overridden methods; `Super.X()` in ZScript calls the Rust default. Place the
hook calls exactly where GZDoom calls them (source map: `docs/appendix-b`).

### 5.4 Events, thinkers, ACS, CVars, networking
- `EventHandler` / `StaticEventHandler`: all events of `docs/12` §9 with
  GZDoom payload structs (`WorldEvent`, `PlayerEvent`, `RenderEvent`,
  `UiEvent`, `InputEvent`, `ConsoleEvent`, `ReplaceEvent`, `ReplacedEvent`);
  registered from MAPINFO `gameinfo.AddEventHandlers` / `EventHandlers`,
  `EventHandler.Find`, `Order`, `ForceDamageEventScripts`.
- `Thinker` subclasses with statnums, `ThinkerIterator`,
  `BlockThingsIterator`, `BlockLinesIterator`, `LineTracer`.
- ACS ↔ ZScript: `ScriptCall` (ACSF 210) calls static ZScript functions; ZScript
  calls `ACS_Execute*`/`ACS_NamedExecute*`; `CallACS` in DECORATE expressions.
- CVars: `CVar.FindCVar`, `GetCVar(name, player)`, `GetInt/GetFloat/GetString`,
  `SetInt/…` (scope rules), CVARINFO `handlerclass`.
- `SendNetworkEvent` / `NetworkProcess`: carried in the tic stream (same as the
  WASM path, `docs/27` §9.4).

### 5.5 UI scope
Status bars (`BaseStatusBar` subclasses, `DrawImage`, `DrawString`,
`HUDFont`, `DrawBar`, …), HUD overlays (`RenderOverlay`), menus (`OptionMenu`,
`ListMenu`, custom `MenuItem`s), `Screen.Draw*`, `Font`, `TexMan`. Run per
client, never in lockstep; ui code may read play data but not mutate it
(enforced by scope checks at compile time and by the binding layer at run
time). Output goes into `room-ui`'s `UiDrawList`.

### 5.6 Saves, demos, netplay
- Saves: actor script field blocks, the object heap (reachable objects),
  event handlers, statics, class `meta` data that is mutable, in a stable order;
  `transient` fields skipped (GZDoom semantics). Saves include a hash of the
  compiled program; loading with a different program refuses unless
  `save_allow_script_mismatch`.
- Demos/netplay: play-scope execution is part of the deterministic sim; the
  world checksum includes script fields and heap objects reachable from play
  roots. The room-server verifier runs the same compiled program.

### 5.7 Coexistence with WASM modules
Both may define classes and handlers; registration order: ZScript, DECORATE,
WASM. A WASM module may subclass a ZScript class (by name) and vice versa via
the shared registry. Event handler order follows `Order`, then registration.

---

## 6. Native binding layer

- One declarative table per area (`crates/room-zvm/natives/*.rs` via a
  `zs_natives!` macro, or `*.ron` + codegen): ZScript qualified name →
  Rust function, signature, scope (play/ui/clearscope), and whether it is
  implemented, a stub (returns default + warns once), or unsupported (load
  error if called by the mod).
- **Coverage report**: `room-cli zscript coverage <archives>` lists every
  native a mod references and its status; CI tracks coverage for the corpus.
- **Implementation order driven by the corpus** (§10). For Brutal Doom
  (`docs/33` §3.10) the needed set is small:
  `Object.new`, `Destroy`, `Array<T>.Size/Insert/Delete/Push/Clear`,
  `String.Format/ToInt/Length/Mid/CharCodeAt`, `Console.Printf`,
  `Wads.CheckNumForFullName/CheckNumForName/FindLump/ReadLump`,
  `EventHandler.Find`, `players[]`, `PlayerInfo.mo/GetGender`, `consoleplayer`,
  `MAXPLAYERS`, `gamestate`, `skill`, `CVar.GetCVar/GetInt`,
  `Actor.TID/ChangeTID/SoundClass/GiveInventory/bISMONSTER`, `is`, casts,
  events `PlayerEntered`, `WorldThingSpawned`.

---

## 7. Tooling and diagnostics
- `room-cli zscript check <archives> [--json]`: compile only; all errors and
  warnings; native coverage; per-class summary.
- `room-cli zscript disasm <class.method>`: bytecode listing with source lines.
- Console: `zs_profile 1` (per-function time and call counts), `zs_dump
  <actor>` (script fields), `zs_trace <func>`.
- Trap messages include a ZScript stack trace with file:line.

## 8. Security / sandbox
No file system, network or OS access from ZScript; only engine natives. Lump
reading (`Wads.*`) is read-only within mounted archives. Memory and fuel
limits per tic; heap size limit with a clear error.

## 9. Performance targets and benchmarks
| Benchmark | Desktop interp. | Desktop JIT | Web |
|---|---|---|---|
| 10,000 actors with a trivial `Tick` override | ≤ 4 ms/tic | ≤ 1.5 ms | ≤ 12 ms |
| `WorldThingSpawned` handler on 5,000 spawns/tic | ≤ 2 ms | ≤ 1 ms | ≤ 6 ms |
| Compile GZDoom-size stdlib + a large mod | ≤ 1.5 s | — | ≤ 4 s |
| Status bar `Draw` per frame | ≤ 0.3 ms | — | ≤ 1 ms |
Benchmarks live in `crates/room-zvm/benches` (criterion, native) and a
headless soak test; add soft CI floors like the renderer's.

## 10. Test corpus
- **Front-end stress test:** parse + type-check GZDoom's own
  `wadsrc/static/zscript` (fetched by a script, not committed).
- **Conformance tests:** small ZScript fixtures per feature with expected
  output (stdout via `Console.Printf`, world state), run on Tier 0 and Tier 1.
- **Mod corpus** (opt-in, env paths, never committed): Brutal Doom v22
  (`ROOM_BRUTALDOOM_PK3`, minimal ZScript), then heavier ZScript mods (for
  example Project Brutality, Hideous Destructor, Beautiful Doom, Trailblazer,
  Final Doomer+) — each with a pinned SHA-256, a native-coverage target and a
  scripted headless run.
- **Determinism:** run each corpus scenario twice and on x86-64 vs aarch64 vs
  wasm32; compare world checksums every 35 tics.

## 11. Changes to existing documents
- `docs/11` §3: superseded by this document (keep the history).
- `docs/00`: remove "Running ZScript source" from non-goals; list ZScript under
  compatibility.
- `docs/12`: WASM modules remain the recommended API for new mods; add a note
  on ZScript/WASM coexistence (§5.7).
- `docs/05`: add ZSCRIPT to the lump table and the load pipeline (before
  DECORATE).
- `docs/33`: BD's ZScript runs on the VM (BD6).
- ADR `docs/decisions/0005-zscript-vm.md`.

---

## 12. Milestones

| ID | Scope | Acceptance |
|---|---|---|
| **ZS0** | `room-zscript` lexer, preprocessor (`#include`, `version`), parser, AST; `room-cli zscript check` (parse only) | Parses all of GZDoom's `wadsrc/static/zscript` and Brutal Doom's `zscript.zc` tree with 0 errors |
| **ZS1** | Semantic analysis: types, scopes, classes/structs/enums/consts, inheritance, mixins, extends, versions, vtables, layouts; Room stdlib declarations (`wadsrc/room/zscript/`) for the BD subset | BD type-checks; GZDoom stdlib type-checks against Room's declarations (stubs allowed) |
| **ZS2** | Bytecode compiler + Tier 0 interpreter, heap, strings, arrays, GC, traps, fuel; conformance fixtures | Fixture suite green; GC/heap determinism test |
| **ZS3** | Engine integration (play): classes/defaults/states in the registry, actor script fields, virtual hooks, action functions, anonymous state functions | A ZScript actor with `Tick`/`PostBeginPlay`/states runs in a headless map; checksum stable across two runs |
| **ZS4** | Event handlers (all play events), MAPINFO `AddEventHandlers`, `ScriptCall`, CVars, `Wads.*`, `Console`; **Brutal Doom ZScript runs** | skill ≥ 6 `Damage_Control` (commercial); `BD_Hash` Mid/`Lump*` path; MAP01 handlers; hash→MES on Doom2 IWAD deferred |
| **ZS5** | DECORATE expressions/action blocks compiled through the front end (replace `deco_eval` string evaluation) | All `deco_eval` tests pass; BD expression report clean; measurable speed-up |
| **ZS6** | Saves, demos, netplay, verifier | Save/load mid-level with script objects; 2-peer lockstep with a ZScript mod, no desync |
| **ZS7** | UI scope: status bars, HUD, menus (`room-ui`) | A ZScript status bar mod renders correctly (golden screenshot) |
| **ZS8** | Native coverage sprint driven by the corpus | ≥ 95 % of natives referenced by the corpus implemented; the rest documented stubs |
| **ZS9** | Tier 1 Cranelift JIT (desktop, feature-gated) with conformance mode | Tier 0 ≡ Tier 1 on the fixture suite and corpus checksums; §9 JIT targets met **Superseded by `docs/37` ZW3 (WASM AOT via wasmtime/browser JIT).** |
| **ZS10** | Performance pass (superinstructions, devirtualisation, inlining); web budget check; optional WASM AOT evaluation | §9 table met on desktop and web |

ZS0–ZS4 are enough for Brutal Doom. Track in `docs/21` (new ZS table).

**Status 2026-11-02 (§14):** ZS0 done. ZS1–ZS4 exist as scaffolding only: the code compiles BD without errors but miscompiles loops, `switch`, compound assignment, calls and string/array use, and the play hooks are not applied. They are re-planned as **ZS1b–ZS4b, ZS6b** in §14.7.

## 13. Risks
- **Native surface**: the biggest cost. Mitigated by the coverage tool,
  stubs with warnings, and corpus-driven priorities.
- **GZDoom version drift**: features are version-gated; track GZDoom releases
  and add a corpus mod per new feature.
- **Behaviour parity**: Room's Rust gameplay must call virtual hooks at the same
  points as GZDoom; add per-hook tests from GZDoom source behaviour.
- **Web performance**: interpreter only; the optional WASM AOT path is the
  fallback if budgets are missed.
- **Determinism**: floats, hashing, GC timing and iteration orders must be
  specified and tested across architectures from ZS2 on.

---

## 14. Implementation audit (2026-11-02) and completion spec

### 14.1 What exists and what it actually does

I audited `crates/room-zscript` (4.2 k LOC), `crates/room-zvm` (3.0 k), and
the integration code: `room-sim/src/zscript.rs`, `deco_zvm.rs`, `acs.rs`
`script_call`, `class.rs::merge_zscript_defaults`,
`room-game/src/zscript_load.rs` and `headless.rs`. Then I ran **Brutal Doom's
real ZScript** (`zscript.zc`, `zscript/BD_WCS.zc`,
`zscript/DamageHandler.zc`, 8.4 KB, taken from `brutal22test6.pk3`) through
`room_zscript::check`, together with a probe class, and executed it on
`room_zvm::Zvm`. The probe is in §14.6.

**Front end.** `check()` reports **0 errors** and produces 16 classes and 32
functions. That is misleading, because compiling and executing are different
things:

| Probe (expected) | Room result | Cause |
|---|---|---|
| `int t=2; t+=3; return t;` (5) | **2** | compound assignment is dropped |
| `for (i=0;i<10;i++) t+=i` (45) | **0** | `for` not compiled |
| `while (i<7) i++` (7) | **0** | `while` not compiled |
| `switch(2){case 2: return 20…}` (20) | **2** | `switch` not compiled; falls out returning the arg register |
| `(x>3)?1:-1` with x=5 (1) | **0** | ternary not compiled |
| `if (x>3) return 1; return -1;` with x=1 (−1) | **0** | negative literal / trailing return lost |
| `return Probe.Ret()+1` (6) | **1** | call results are not propagated (`CallStatic` writes `ret=Some((0,a))`; caller reads wrong reg) |
| `String.Format("%i",42).ToInt(10)` (42) | **0** | `String` taken as a string expression; varargs not passed |
| `"AB".CharCodeAt(1)` (66) | **trap** `unresolved CallStatic 'b.CharCodeAt'` | method not known, so it becomes a fake static call |
| `ZC_IntStrMap m=new(…); m.insert(5,"abc"); m.get(5).Length()` (3) | **trap** `unresolved CallStatic 'm.insert'` | no instance-method calls on script objects, no `self` passing, no string args |
| local `Array<int> a; a.Push(3)…` (309) | **trap** `Array.Push: not Array<int>` | local dynamic arrays are not allocated |
| `ZandroCompat.LumpOpen` + `LumpRead` | **1** (garbage) | string args are not marshalled; `Wads.FindLump` is a stub |
| `ZandroCompat.PlayerInGame(0)` | **trap** `NullDeref` | `players[]` not bound |

Root causes in `compile.rs` (814 LOC):

1. **No type information.** `sema.rs` builds name tables only: `MethodSym`
   and `FieldSym` have no types, and `layouts` is a field count. The emitter
   guesses the register bank from expression *syntax*. For example,
   `place_args` treats string literals as S and everything else as I, and
   object locals overwrite S slots (`MoveA si`).
2. **Statement coverage is incomplete.** `compile_stmt` handles
   `Local/Return/Expr/If/Block` only. There is no `for/while/do/until/switch/
   break/continue/foreach`, and compound assignment is ignored.
3. **Calls are pattern-matched by name.** `emit_call_i`/`emit_call_s`
   special-case `giveinventory`, `size`, `push`, `length`, `toint`, `getint`,
   `changetid`, `mid` and `format`. Everything else becomes
   `CallStatic "<text of callee>"`, resolved by **string at runtime**. The
   call ABI copies ints only: no `self`, no strings, no objects, no out
   params, and the return register is wrong.
4. **Silent wrong code.** Unknown constructs compile to `LoadConstI 0` or
   `LoadConstS ""` with no diagnostic. This is the most dangerous property:
   the compile-check passes while behaviour is wrong.

**VM.** The interpreter, opcode set, peephole pass, fuel and traps are a
usable base. Gaps:
- The heap strings are UTF-8 `String`. ZScript strings are **byte strings**,
  and `Wads.ReadLump` plus `CharCodeAt` read binary lumps.
- `ObjKind` has no float, bool or object arrays, and no actor-proxy objects.
- Natives receive a register window, but host access goes through
  **thread-locals** (`natives_bd.rs` `HOST_FNS`, plus 11 `thread_local!`
  maps in `room-sim/src/zscript.rs`).

**Integration.**
- **Events carry fake data.** `WorldThingSpawned` gets a **synthetic** object
  `{_, is_monster}`, not the spawned actor. Its `GiveInventory` call ends up
  in `HOST_GRANTS`.
- **Grants are never applied.** `HOST_GRANTS` is only *logged* under
  `ROOM_ZSCRIPT_DEBUG` (`headless.rs:1240`); TID changes are the same. So
  `Damage_Control` is never given.
- **Player events are hard-coded.** `PlayerEntered` is fired for seat 0
  only, at level start.
- **`ModifyDamage` is never called.** `damage.rs::damage_mobj` has no
  ZScript hook and no damage type parameter.
- **ZScript-defined actor classes are not registered.**
  `merge_zscript_defaults` only patches classes that **already exist** in the
  registry, so `Damage_Control` (defined only in ZScript) is skipped. Its
  `Default` body is a re-joined token string (`"INVENTORY . MaxAmount 1 ; +
  INVENTORY . IGNORESKILL…"`) that the space-splitting parser cannot read.
- **ACS `ScriptCall` passes ints only.** `LumpOpen(name, …)` and
  `LumpReadString` (which returns a string, cast with `(str)` in ACS) cannot
  work.
- **Saves are incomplete.** `ZscriptSaveBlob` saves `fields_i` only, not
  strings, objects or arrays.
- **Tests do not exercise the real code.** `testdata/zscript/bd_like/` is a
  **734-byte reduction** of BD's 6 KB `BD_WCS.zc`, which is why
  `bd_map01_smoke` and `zscript_play` pass.

**Verdict.** ZS0 (parse) is done. ZS1–ZS4 are scaffolding, so Brutal Doom's
ZScript does not run correctly in Room today. The fix is a typed compiler
core and a real host interface. That replaces `compile.rs`,
`natives_bd.rs` and the thread-local plumbing, and keeps the lexer, parser,
AST, preprocessor, opcode/interp skeleton and heap. The rest of this section
is the spec plus reference code for that work.

### 14.2 What Brutal Doom needs, exactly (complete list)

Taken from the pk3: the ZScript sources, `MAPINFO.txt` and every
`ScriptCall` in `src/*.acs`/`*.bcs`.

| Entry | Caller | Signature | Needs |
|---|---|---|---|
| `ZC_TMapHack` | MAPINFO `AddEventHandlers` | EventHandler, `static play ZC_TMapHack fetch()` | `EventHandler.Find`, cast `ZC_TMapHack(x)`, `new("…")`, object fields |
| `Script_DamageMult.PlayerEntered` | event | `(PlayerEvent e)` | `GameState`, `GS_LEVEL`, `Skill`, `players[n].mo`, `Actor.GiveInventory` |
| `Script_DamageMult.WorldThingSpawned` | event | `(WorldEvent e)` | `e.Thing`, `bISMONSTER`, `GiveInventory` |
| `Damage_Control` | given by the handler | `Inventory` subclass + `Default{…}` + `override ModifyDamage(int, Name, out int, bool, Actor, Actor, int)` | class registration, flags, `out` params, `Name` compare, `PlayerPawn(x)` cast, `GetClassName()` |
| `ZandroCompat.HasZScript` | `zancmpat.bcs` | `static int()` | — |
| `ZandroCompat.LumpOpen` | `zancmpat.bcs` (BD_Hash) | `static int(String name, int start, int flags)` | ACS string → ZScript string; `Wads.CheckNumForFullName/CheckNumForName/FindLump/ReadLump`; `ZC_IntStrMap` |
| `ZandroCompat.LumpClose` | `zancmpat.bcs` | `static void(int)` | maps |
| `ZandroCompat.LumpGetInfoInt` / `LumpGetInfoStr` | `zancmpat.bcs` | `static int(int,int)` / `static String(int,int)` | `switch`, string return → ACS string |
| `ZandroCompat.LumpReadString` | `zancmpat.bcs` | `static String(int,int,int)` | `String.Mid`, string return → ACS |
| `ZandroCompat.LumpRead` | `zancmpat.bcs` | `static int(int,int,int)` | `CharCodeAt` on **binary** data, `switch`, shifts, `&`, `|` |
| `ZandroCompat.IsClient` / `ConsolePlayerNumber` | `BCSFunc.acs`, `flashlightshow.acs` | `static int()` | `consoleplayer` |
| `ZandroCompat.PlayerInGame` | `Options.acs` | `static bool(int)` | `players[n].mo`, `!!` |
| `ZandroCompat.Get/SetPlayerTID` | `Options.acs` | `static int(int)` / `static void(int,int)` | `MAXPLAYERS`, `mo.TID`, `ChangeTID` |
| `ZandroCompat.GetGender` | `Options.acs` | `static void(int)` | `PlayerInfo` struct ref, `GetGender()`, `CVar.GetCVar(name, player).GetInt()`, `mo.SoundClass` (write), `TakeInventory/GiveInventory` |

Language features used: classes, plain classes, `static`, `play`,
`protected`, `const`, `Array<int>`, `Array<string>`, `for`, `switch`,
`break`, ternary, compound assignment, `++`, casts, `new`, `out`, `!!`,
string methods, `Name` literals, `struct` refs (`PlayerInfo`), and global
`players[]`.

### 14.3 Compiler core (replace `compile.rs`; extend `sema.rs`)

Pipeline: `parse → sema (declare) → sema (resolve: types, layouts, vtables)
→ lower (typed AST → bytecode) → link`. Every unresolved name is an
**error**. A diagnostic with `Severity::Error` stops the program from being
installed. There is no zero/empty fallback anywhere.

#### 14.3.1 Types and banks (`room-zscript/src/types.rs`, new)

```rust
//! ZScript types and their VM register bank.
use room_zvm::Bank;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZType {
    Void,
    Bool,
    Int { bits: u8, signed: bool }, // int, uint, int8…; all live in I bank
    Double,                          // float/double → F bank (f64)
    Name,                            // interned name id in I bank
    Sound,                           // sound id in I bank
    Color,                           // u32 in I bank
    State,                           // state id in I bank
    SpriteId, TextureId,             // I bank
    String,                          // byte string, S bank
    Vector2, Vector3,                // 2/3 consecutive F regs
    Class(String),                   // class<T> = class id in I bank
    Object(String),                  // reference (Object/Actor/…) in A bank
    Struct(String),                  // by-reference struct (A bank, heap or native)
    DynArray(Box<ZType>),            // Array<T>: A bank handle
    FixedArray(Box<ZType>, u32),     // inline in owner layout
    Map(Box<ZType>, Box<ZType>),     // Map<K,V> (4.10+)
    Null,                            // literal null, coerces to Object/Class
}

impl ZType {
    pub fn bank(&self) -> Bank {
        use ZType::*;
        match self {
            Double | Vector2 | Vector3 => Bank::F,
            String => Bank::S,
            Object(_) | Struct(_) | DynArray(_) | Map(..) | Null => Bank::A,
            Void => Bank::I,
            _ => Bank::I,
        }
    }
    pub fn width(&self) -> u8 {
        match self { ZType::Vector2 => 2, ZType::Vector3 => 3, _ => 1 }
    }
    pub fn is_numeric(&self) -> bool {
        matches!(self, ZType::Int { .. } | ZType::Double | ZType::Bool | ZType::Name | ZType::Color)
    }
}

/// Implicit conversion allowed by ZScript (assignment / argument passing).
pub fn coerce(from: &ZType, to: &ZType, classes: &ClassTable) -> Option<Conv> {
    use ZType::*;
    Some(match (from, to) {
        (a, b) if a == b => Conv::None,
        (Int { .. } | Bool, Double) => Conv::I2F,
        (Double, Int { .. }) => Conv::F2I,           // ZScript allows with warning
        (Int { .. }, Bool) | (Double, Bool) | (Object(_), Bool) | (String, Bool) => Conv::ToBool,
        (Bool, Int { .. }) | (Int { .. }, Int { .. }) => Conv::None,
        (Null, Object(_) | Class(_) | Struct(_) | DynArray(_)) => Conv::None,
        (Object(a), Object(b)) if classes.is_subclass(a, b) => Conv::None,
        (Class(a), Class(b)) if classes.is_subclass(a, b) => Conv::None,
        (String, Name) => Conv::StrToName,
        (Name, String) => Conv::NameToStr,
        (String, Class(base)) => Conv::StrToClass(base.clone()), // class<X>("Foo") literal
        _ => return None,
    })
}
```

#### 14.3.2 Symbol tables with types (extend `sema.rs`)

```rust
pub struct FieldInfo { pub name: String, pub ty: ZType, pub bank: Bank, pub slot: u16,
                       pub native: Option<String>, pub readonly: bool, pub scope: Scope }
pub struct Param    { pub name: String, pub ty: ZType, pub out: bool, pub default: Option<Expr> }
pub struct FuncSig  { pub params: Vec<Param>, pub rets: Vec<ZType>, pub varargs: bool }
pub struct MethodInfo {
    pub owner: String, pub name: String, pub sig: FuncSig,
    pub is_static: bool, pub is_virtual: bool, pub vslot: Option<u16>,
    pub native: bool, pub scope: Scope, pub action: bool,
    pub func: Option<u32>,          // index into CompiledProgram.functions after lowering
}
pub struct ClassInfo {
    pub name: String, pub parent: Option<String>, pub is_struct: bool, pub native: bool,
    pub fields: Vec<FieldInfo>,     // inherited first, then own
    pub n_slots: [u16; 4],          // per bank, inherited included
    pub methods: BTreeMap<String /*lower*/, MethodInfo>,
    pub vtable: Vec<(String /*owner*/, String /*method*/)>,
    pub consts: BTreeMap<String, ConstVal>, pub enums: BTreeMap<String, i64>,
    pub defaults: Option<DefaultBlock>, pub states: Option<StatesBlock>,
    pub actor_derived: bool,        // descends from Actor → engine-registered class
}
pub struct ClassTable { by_lower: BTreeMap<String, ClassInfo> }
impl ClassTable {
    pub fn is_subclass(&self, a: &str, b: &str) -> bool { /* walk parents, case-insensitive */ }
    pub fn lookup_method(&self, class: &str, name: &str) -> Option<&MethodInfo> { /* walk parents */ }
    pub fn lookup_field(&self, class: &str, name: &str) -> Option<&FieldInfo> { /* walk parents */ }
}
```

Resolution order follows GZDoom: locals, then `self` fields/methods, then
the class constants/enums, the global constants, the globals table
(`players`, `level`, `consoleplayer`, `gamestate`, `skill`, `multiplayer`,
`gametic`, …, each declared in `wadsrc/room/zscript/globals.zs` as
`native readonly`), and finally class names (for static calls or casts).

#### 14.3.3 VM changes (`room-zvm`)

Add a bank enum, a uniform call ABI with an argument stack, typed field and
array ops, byte strings and actor proxies.

```rust
#[repr(u8)] #[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bank { I = 0, F = 1, S = 2, A = 3 }

// New opcodes (append to Op; keep existing numbering stable):
//  Arg      a=bank b=reg            push reg onto the VM arg stack
//  Call     ext=func_idx a=nret     pop args into callee params (by signature order), run
//  CallVirt ext=vslot a=nret        receiver = first A-bank arg; func = vtable[class][vslot]
//  CallNat  ext=native_id a=nret    natives use the same arg stack
//  Ret      a=bank b=reg c=index    set return value #index (index>0 = out params)
//  GetRet   a=bank b=dst c=index    copy return value #index after a call
//  LdFld    a=dst b=obj c=slot      ext=bank  — field load from a[b]
//  StFld    a=src b=obj c=slot      ext=bank
//  NewObj   a=dst ext=class_id
//  NewArr   a=dst ext=bank
//  ArrGet / ArrSet / ArrLen        a,b,c regs; ext=bank (trap on OOB like GZDoom VM)
//  IsA      a=dst(i) b=obj ext=class_id   (cast = IsA + select null)
//  StrCat, StrEq, StrLt, BitAnd/Or/Xor/Shl/Shr/UShr, Neg, Not, BNot, JmpIfZ/JmpIfNZ (exists)

pub struct Frame { func: u32, pc: u32, i: Vec<i64>, f: Vec<f64>, s: Vec<StrHandle>, a: Vec<ObjHandle> }
pub struct Zvm {
    pub program: CompiledProgram,
    pub heap: ZvmHeap,
    pub natives: NativeTable,
    args: Vec<Value>,          // argument stack (cleared per call)
    rets: [Value; 8],          // return slots: 0 = result, 1.. = out params
    pub config: ZvmConfig,
    fuel_left: u64,
}
#[derive(Clone, Copy, Debug)]
pub enum Value { I(i64), F(f64), S(StrHandle), A(ObjHandle) }
```

**Calling convention (the bug at the heart of today's VM).**
- The caller emits `Arg` once per argument in signature order (`self`
  first for instance methods).
- The callee prologue is implicit: the VM pops args into the callee's
  parameter registers. Each parameter's `(bank, reg)` is stored in
  `CompiledFunction::params`, so no register guessing happens at run time.
- `out` params are copied back by the caller from `rets[k]` with `GetRet`.

```rust
fn enter(&mut self, stack: &mut Vec<Frame>, func_idx: u32) -> Result<(), Trap> {
    let f = &self.program.functions[func_idx as usize];
    if stack.len() >= self.config.stack_limit { return Err(Trap::StackOverflow); }
    let mut fr = Frame::new(func_idx, f);
    let args = std::mem::take(&mut self.args);
    if args.len() < f.params.len() { return Err(Trap::Native(format!("{}: arity", f.name))); }
    for (p, v) in f.params.iter().zip(args) {
        match (p.bank, v) {
            (Bank::I, Value::I(x)) => fr.i[p.reg as usize] = x,
            (Bank::F, Value::F(x)) => fr.f[p.reg as usize] = x,
            (Bank::S, Value::S(x)) => fr.s[p.reg as usize] = x,
            (Bank::A, Value::A(x)) => fr.a[p.reg as usize] = x,
            _ => return Err(Trap::Native(format!("{}: arg bank mismatch", f.name))),
        }
    }
    stack.push(fr);
    Ok(())
}
```

**Heap.**
- `StrEntry.value: Vec<u8>` (byte strings). `String.Format` and `Printf`
  format bytes, and output to the console is decoded lossily as Latin-1.
- `ObjKind` gains `ArrayF(Vec<f64>)`, `ArrayA(Vec<ObjHandle>)`,
  `MapIS/MapSS…` (BTreeMap for determinism) and
  **`Actor { id_bits: u64 }`**: a proxy for a world actor. Proxies are
  interned per actor (`BTreeMap<ActorId, ObjHandle>`), so `a == b` holds.
  When the actor is destroyed, the proxy becomes null (GZDoom semantics).
- A native struct ref such as `PlayerInfo` is
  `ObjKind::Native { kind: NativeKind::PlayerInfo, index: u32 }`.

**Natives get a host, not thread-locals.**

```rust
pub type NativeFn = fn(&mut NatCtx<'_>) -> Result<(), Trap>;
pub struct NatCtx<'a> {
    pub args: &'a [Value],
    pub rets: &'a mut [Value; 8],
    pub heap: &'a mut ZvmHeap,
    pub host: &'a mut dyn ZHost,     // implemented by room-sim World (play) / room-ui (ui)
}
/// Everything script code can reach in the engine. Play scope only here.
pub trait ZHost {
    fn actor_alive(&self, a: u64) -> bool;
    fn actor_class_name(&self, a: u64) -> &str;
    fn actor_get_i(&self, a: u64, field: NativeField) -> i64;   // health, TID, flags…
    fn actor_set_i(&mut self, a: u64, field: NativeField, v: i64);
    fn actor_get_s(&self, a: u64, field: NativeField) -> Vec<u8>; // SoundClass…
    fn actor_set_s(&mut self, a: u64, field: NativeField, v: &[u8]);
    fn give_inventory(&mut self, a: u64, class: &str, amount: i32) -> bool;
    fn take_inventory(&mut self, a: u64, class: &str, amount: i32) -> bool;
    fn change_tid(&mut self, a: u64, tid: i32);
    fn player_mo(&self, n: i32) -> Option<u64>;
    fn player_gender(&self, n: i32) -> i32;
    fn cvar_int(&self, name: &str, player: Option<i32>) -> Option<i32>;
    fn lump_check_full(&self, name: &str) -> i32;
    fn lump_check_name(&self, name: &str, ns: i32) -> i32;
    fn lump_find(&self, name: &str, start: i32) -> i32;
    fn lump_read(&self, lump: i32) -> Option<Vec<u8>>;
    fn printf(&mut self, msg: &[u8]);
    fn globals(&self) -> ZGlobals;  // skill, gamestate, consoleplayer, gametic, multiplayer
    fn event_handler(&self, class_lower: &str) -> ObjHandle;
}
```

`Zvm::call(&mut self, func, args, host: &mut dyn ZHost)` threads the host
through `run`. `room-sim` implements `ZHost` on a small `WorldZHost<'w> {
world: &'w mut World }`. To avoid aliasing with `world.zscript`, the
runtime is taken out of `World` around each call:

```rust
impl World {
    pub(crate) fn with_zscript<R>(&mut self, f: impl FnOnce(&mut ZscriptRuntime, &mut WorldZHost<'_>) -> R) -> Option<R> {
        let mut rt = std::mem::take(&mut self.zscript);
        if !rt.is_active() { self.zscript = rt; return None; }
        let r = { let mut host = WorldZHost { world: self }; f(&mut rt, &mut host) };
        self.zscript = rt;
        Some(r)
    }
}
```

**Effects are applied immediately and deterministically**, inside the sim
tic. Delete `HOST_GRANTS`, `HOST_TID_CHANGES`, `HOST_WADS`, `HOST_CVARS`,
`HOST_PLAYERS_IN_GAME`, `HOST_PLAYER_TIDS`, and `natives_bd.rs`'s `HOST_FNS`.

#### 14.3.4 Lowering (`room-zscript/src/lower.rs`, replaces `compile.rs`)

```rust
pub struct Fx<'a> {
    ct: &'a ClassTable, prog: &'a mut ProgramBuilder,
    f: CompiledFunction, class: Option<&'a ClassInfo>, sig: &'a FuncSig,
    scopes: Vec<BTreeMap<String /*lower*/, Local>>,
    next: [u16; 4], high: [u16; 4],                 // register allocator per bank
    breaks: Vec<Vec<usize>>, continues: Vec<Vec<usize>>,
    diags: &'a mut Vec<Diagnostic>,
}
#[derive(Clone)] pub struct Local { ty: ZType, reg: u16 }
#[derive(Clone)] pub struct Val { ty: ZType, reg: u16, temp: bool } // bank = ty.bank()

impl Fx<'_> {
    fn alloc(&mut self, b: Bank) -> u16 { let r = self.next[b as usize]; self.next[b as usize] += 1;
        self.high[b as usize] = self.high[b as usize].max(self.next[b as usize]); r }
    fn mark(&self) -> [u16; 4] { self.next }
    fn release(&mut self, m: [u16; 4]) { self.next = m; }   // stack discipline for temps

    pub fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local { ty, names, init, span } => {
                let t = self.resolve_type(ty.as_ref(), init.as_ref(), *span);
                for (k, n) in names.iter().enumerate() {
                    let reg = self.alloc(t.bank());
                    self.declare(n, Local { ty: t.clone(), reg });
                    match (k, init) {
                        (0, Some(e)) => { let m = self.mark(); let v = self.expr(e); self.store_conv(&v, &t, reg, *span); self.release(m); }
                        _ => self.zero_init(&t, reg),   // ints 0, strings "", objects null, Array<T> → NewArr
                    }
                }
            }
            Stmt::Expr { expr, .. } => { let m = self.mark(); self.expr_discard(expr); self.release(m); }
            Stmt::Return { values, span } => self.ret(values, *span),
            Stmt::If { cond, then_b, else_b, .. } => {
                let m = self.mark(); let c = self.cond(cond); let j = self.emit_jz(c); self.release(m);
                self.block(then_b);
                if let Some(e) = else_b { let jend = self.emit_jmp(); self.patch(j); self.block(e); self.patch(jend); }
                else { self.patch(j); }
            }
            Stmt::While { cond, body, .. } => self.loop_(None, Some(cond), None, body, false),
            Stmt::DoWhile { cond, body, .. } => self.loop_(None, Some(cond), None, body, true),
            Stmt::For { init, cond, step, body, .. } => { self.push_scope(); if let Some(i) = init { self.stmt(i); }
                self.loop_(None, cond.as_ref(), step.as_ref(), body, false); self.pop_scope(); }
            Stmt::Switch { value, cases, span } => self.switch(value, cases, *span),
            Stmt::Break(span) => { let j = self.emit_jmp(); self.breaks.last_mut().map(|v| v.push(j)).unwrap_or_else(|| self.err(*span, "break outside loop/switch")); }
            Stmt::Continue(span) => { let j = self.emit_jmp(); self.continues.last_mut().map(|v| v.push(j)).unwrap_or_else(|| self.err(*span, "continue outside loop")); }
            Stmt::Block(b) => { self.push_scope(); self.block(b); self.pop_scope(); }
            Stmt::ForEach { var, coll, body, span } => self.foreach(var, coll, body, *span),
            other => self.err(other.span(), "unsupported statement"),   // never silent
        }
    }

    fn loop_(&mut self, _lbl: Option<&str>, cond: Option<&Expr>, step: Option<&Expr>, body: &Block, do_first: bool) {
        self.breaks.push(vec![]); self.continues.push(vec![]);
        let top = self.f.code.len();
        let jexit = if !do_first { cond.map(|c| { let m = self.mark(); let r = self.cond(c); let j = self.emit_jz(r); self.release(m); j }) } else { None };
        self.block(body);
        let cont_target = self.f.code.len();
        for j in self.continues.pop().unwrap() { self.patch_to(j, cont_target); }
        if let Some(s) = step { let m = self.mark(); self.expr_discard(s); self.release(m); }
        if do_first { if let Some(c) = cond { let m = self.mark(); let r = self.cond(c); self.emit_jnz_to(r, top); self.release(m); } }
        else { self.emit_jmp_to(top); }
        let end = self.f.code.len();
        if let Some(j) = jexit { self.patch_to(j, end); }
        for j in self.breaks.pop().unwrap() { self.patch_to(j, end); }
    }

    /// GZDoom switch: integer/name/string compare chain, fall-through, default.
    fn switch(&mut self, value: &Expr, cases: &[SwitchCase], span: Span) {
        let v = self.expr(value);
        let mut body_jumps = Vec::new();
        for c in cases.iter().filter(|c| c.label.is_some()) {
            let m = self.mark();
            let k = self.expr(c.label.as_ref().unwrap());
            let eq = self.compare(BinOp::Eq, &v, &k, span);
            body_jumps.push((self.emit_jnz(eq.reg), c as *const _));
            self.release(m);
        }
        let jdefault = self.emit_jmp();
        self.breaks.push(vec![]);
        let mut had_default = false;
        for c in cases {
            let here = self.f.code.len();
            if c.label.is_none() { self.patch_to(jdefault, here); had_default = true; }
            for (j, p) in &body_jumps { if std::ptr::eq(*p, c) { self.patch_to(*j, here); } }
            for s in &c.body { self.stmt(s); }    // fall-through is natural
        }
        let end = self.f.code.len();
        if !had_default { self.patch_to(jdefault, end); }
        for j in self.breaks.pop().unwrap() { self.patch_to(j, end); }
    }

    fn ret(&mut self, values: &[Expr], span: Span) {
        for (k, e) in values.iter().enumerate() {
            let want = self.sig.rets.get(k).cloned().unwrap_or(ZType::Void);
            let m = self.mark(); let v = self.expr(e); let v = self.conv(v, &want, span);
            self.f.emit(Op::Ret, v.ty.bank() as u8, v.reg as u8, k as u8);   // reg > 255 → spill, see emit
            self.release(m);
        }
        // copy back out params as rets[1..]
        for (k, p) in self.sig.params.iter().filter(|p| p.out).enumerate() {
            let l = self.lookup(&p.name).unwrap();
            self.f.emit(Op::Ret, l.ty.bank() as u8, l.reg as u8, (1 + k) as u8);
        }
        self.f.emit(Op::Return, 0, 0, 0);
    }
}
```

Expressions return a typed `Val`. Key cases:

```rust
fn expr(&mut self, e: &Expr) -> Val {
    match e {
        Expr::Int(v, _) => self.const_i(*v, ZType::int()),
        Expr::Float(v, _) => self.const_f(*v),
        Expr::String(s, _) => self.const_s(s.as_bytes()),
        Expr::Name(n, _) => self.const_name(n),                // NameId interned at link time
        Expr::Bool(b, _) => self.const_i(*b as i64, ZType::Bool),
        Expr::Null(_) => self.const_null(),
        Expr::Ident(n, sp) => self.load_ident(n, *sp),          // local → field → const → global → error
        Expr::Unary { op, rhs, .. } => self.unary(*op, rhs),    // -, !, ~, ++x, --x, x++, x--
        Expr::Binary { op: BinOp::AndAnd | BinOp::OrOr, .. } => self.short_circuit(e),
        Expr::Binary { op, lhs, rhs, span } => self.binary(*op, lhs, rhs, *span), // numeric promotion, string ops
        Expr::Assign { op, lhs, rhs, span } => self.assign(*op, lhs, rhs, *span), // =, +=, -=, *=, /=, %=, &=, |=, ^=, <<=, >>=
        Expr::Ternary { cond, a, b, span } => self.ternary(cond, a, b, *span),
        Expr::Member { obj, field, span } => self.member(obj, field, *span),
        Expr::Index { obj, idx, span } => self.index(obj, idx, *span),
        Expr::Call { callee, args, span } => self.call(callee, args, *span),
        Expr::Cast { ty, expr, span } => self.cast(ty, expr, *span),
        Expr::Is { expr, class, span } => self.is_a(expr, class, *span),
        Expr::New { class, span } => self.new_obj(class, *span),
        Expr::Vector { items, span } => self.vector(items, *span),
        other => { self.err(other.span(), "unsupported expression"); self.poison() }
    }
}

/// Calls: decide static / instance / virtual / native from symbols, not from text.
fn call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Val {
    // 1. Resolve target
    let (recv, mi) = match callee {
        // `Type.Func(...)` (static) or `obj.Func(...)` (instance/struct/string/array builtin)
        Expr::Member { obj, field, .. } => {
            if let Some(class) = self.as_type_name(obj) {
                match self.ct.lookup_method(&class, field) {
                    Some(mi) if mi.is_static => (None, mi.clone()),
                    Some(_) => { self.err(span, format!("{class}.{field} is not static")); return self.poison(); }
                    None => { self.err(span, format!("unknown function {class}.{field}")); return self.poison(); }
                }
            } else {
                let r = self.expr(obj);
                match self.builtin_method(&r.ty, field) {        // String.*, Array<T>.*, Vector.*
                    Some(mi) => (Some(r), mi),
                    None => match r.ty.class_name().and_then(|c| self.ct.lookup_method(c, field)) {
                        Some(mi) => (Some(r), mi.clone()),
                        None => { self.err(span, format!("no method '{field}' on {:?}", r.ty)); return self.poison(); }
                    },
                }
            }
        }
        // bare `Func(...)`: self method, else global function, else class cast `Foo(x)`
        Expr::Ident(n, _) => {
            if let Some(cls) = self.ct.get(n) { return self.dyn_cast(cls.name.clone(), &args[0], span); }
            match self.class.and_then(|c| self.ct.lookup_method(&c.name, n)) {
                Some(mi) if mi.is_static => (None, mi.clone()),
                Some(mi) => (Some(self.load_self()), mi.clone()),
                None => { self.err(span, format!("unknown function {n}")); return self.poison(); }
            }
        }
        _ => { self.err(span, "call target"); return self.poison(); }
    };
    // 2. Arguments: self first, then params with defaults; varargs typed by expression
    let mut outs = Vec::new();
    if let Some(r) = &recv { self.f.emit(Op::Arg, r.ty.bank() as u8, r.reg as u8, 0); }
    for (k, p) in mi.sig.params.iter().enumerate() {
        let e = args.get(k).or(p.default.as_ref());
        let Some(e) = e else { self.err(span, format!("missing argument '{}'", p.name)); return self.poison(); };
        if p.out { outs.push((k, self.lvalue(e))); }
        let v = self.expr(e); let v = self.conv(v, &p.ty, span);
        self.f.emit(Op::Arg, v.ty.bank() as u8, v.reg as u8, 0);
    }
    if mi.sig.varargs { for e in &args[mi.sig.params.len()..] { let v = self.expr(e); self.f.emit(Op::Arg, v.ty.bank() as u8, v.reg as u8, 1 /* tagged */); } }
    else if args.len() > mi.sig.params.len() { self.err(span, "too many arguments"); }
    // 3. Emit call
    if mi.native { let nid = self.prog.native_id(&mi.owner, &mi.name, span, self.diags); self.f.emit_ext(Op::CallNat, 0, 0, 0, nid); }
    else if let (Some(_), Some(slot)) = (&recv, mi.vslot) { self.f.emit_ext(Op::CallVirt, 0, 0, 0, slot as u32); }
    else { let fid = self.prog.func_ref(&mi.owner, &mi.name); self.f.emit_ext(Op::Call, 0, 0, 0, fid); } // patched at link
    // 4. Out params, then result
    for (k, (_, lv)) in outs.iter().enumerate() { self.store_from_ret(lv, 1 + k as u8); }
    let rt = mi.sig.rets.first().cloned().unwrap_or(ZType::Void);
    let dst = self.alloc(rt.bank());
    self.f.emit(Op::GetRet, rt.bank() as u8, dst as u8, 0);
    Val { ty: rt, reg: dst, temp: true }
}
```

Builtins: `String.Format` and `Console.Printf` are varargs natives. The
`Arg` op's tag bit makes the VM push `Value::{I,F,S,A}` and the native
formats by `%i %d %s %f %x %c %p`. `String` methods are `Length`, `Mid`,
`Left`, `Right`, `CharAt`, `CharCodeAt`, `ToInt(base)`, `ToDouble`,
`IndexOf`, `RightIndexOf`, `Replace`, `MakeUpper/Lower`, `Split`,
`AppendFormat`, `Filter`, `ByteAt`, `DeleteLastCharacter`, and
`static Format`. `Array<T>` methods are `Size`, `Push`, `Pop`, `Insert`,
`Delete(i,n=1)`, `Clear`, `Find`, `Copy`, `Move`, `Reserve` and `Resize`.
All are declared in `wadsrc/room/zscript/builtins.zs` with real signatures
and bound in `room-zvm/src/natives/{string,array,math}.rs`.

**Natives are declared, then bound.** Every `native` method in the Room
stdlib (`wadsrc/room/zscript/*.zs`) has an entry in `NativeTable` keyed by
`"Owner.Name"`. At link time:
- a declared native with no binding is an **error** listing the names (with
  `--allow-stubs`, it is linked to `Trap::Unimplemented(name)`, never a
  default value);
- a binding with no declaration is a warning.

`room-zvm/src/coverage.rs` reports the counts per corpus.

#### 14.3.5 Class registration and `Default`/`States`

ZScript is compiled **before** DECORATE (as §5.1 says). For every class
with `actor_derived`, `room-game` calls
`ClassRegistry::register_zscript_class(name, parent, defaults, states,
script_class_id)`, which creates a real class (as DECORATE would), so
`GiveInventory("Damage_Control")` and `summon` work.

- **Defaults are parsed from tokens, not from a re-joined string.** The
  parser keeps the `Default` block as `Vec<Token>`. The integration
  converts it with `room_decorate::parse_property_tokens(&tokens)`, which
  is the same property/flag table DECORATE uses. The ZScript `Prop.Sub
  value;` form maps 1:1, and `+INVENTORY.IGNORESKILL` becomes a flag op.
  Delete `merge_zscript_defaults`' ad-hoc splitter.
- **States** go through the DECORATE state parser, as today. Anonymous
  functions `{ … }` and `A_*` calls resolve against compiled script
  methods first, then native actions.
- A class that overrides virtuals gets `ClassDef::script: Some(ScriptHooks
  { tick, post_begin_play, modify_damage, … })`, with function indices
  precomputed once at load. Rust checks `def.script.as_ref()?.modify_damage`
  and pays nothing when it is `None`.

### 14.4 Engine hooks needed by Brutal Doom

| Hook | Where | Code |
|---|---|---|
| `WorldThingSpawned(WorldEvent)` | `world.rs` spawn, **after** `PostBeginPlay`, before the first tick (GZDoom order) | `self.with_zscript(\|rt,h\| rt.dispatch(h, Event::ThingSpawned { thing: id }))`, which builds a `WorldEvent` struct with `Thing = proxy(id)` |
| `PlayerEntered(PlayerEvent)` | `headless.rs` level start: **every** `playeringame` seat, in seat order; also on a netplay join | `for n in seats { rt.dispatch(h, Event::PlayerEntered { n, is_return: hub_returning }) }`. Remove the hard-coded `notify_player_entered(0)` |
| `WorldLoaded`, `WorldUnloaded`, `WorldTick`, `WorldThingDied`, `WorldThingDamaged`, `WorldThingDestroyed` | the matching `World` points (WASM `ScriptEvent` already marks them) | one `dispatch` match arm per event; `WorldEvent` fields filled from engine data |
| `Inventory.ModifyDamage` | `damage.rs::damage_mobj`: add `damage_type: NameId`, then before armor: walk **target** inventory (passive=true), then **source** inventory (passive=false), like `AActor::DamageMobj` + `ApplyDamageFactors` | see below |
| `ScriptCall` | `acs.rs::script_call` | marshal by signature (below) |

```rust
// damage.rs — GZDoom order: inflictor/source ModifyDamage (active), then target (passive)
fn apply_modify_damage(world: &mut World, target: ActorId, inflictor: Option<ActorId>,
                       source: Option<ActorId>, damage: i32, dtype: NameId, flags: i32) -> i32 {
    let mut dmg = damage;
    if let Some(src) = source {
        for item in world.inventory_chain(src) {             // owner's inventory, link order
            if let Some(f) = world.script_hook(item, |h| h.modify_damage) {
                dmg = world.with_zscript(|rt, h| rt.call_modify_damage(h, f, item, dmg, dtype, false, inflictor, source, flags))
                    .and_then(Result::ok).unwrap_or(dmg);
            }
        }
    }
    for item in world.inventory_chain(target) {
        if let Some(f) = world.script_hook(item, |h| h.modify_damage) {
            dmg = world.with_zscript(|rt, h| rt.call_modify_damage(h, f, item, dmg, dtype, true, inflictor, source, flags))
                .and_then(Result::ok).unwrap_or(dmg);
        }
    }
    dmg
}

impl ZscriptRuntime {
    pub fn call_modify_damage(&mut self, h: &mut dyn ZHost, f: u32, item: ActorId, damage: i32,
            dtype: NameId, passive: bool, inflictor: Option<ActorId>, source: Option<ActorId>, flags: i32)
            -> Result<i32, Trap> {
        let vm = self.vm.as_mut().ok_or(Trap::NoProgram)?;
        let p = |vm: &mut Zvm, a: Option<ActorId>| a.map_or(ObjHandle::NULL, |a| vm.heap.actor_proxy(a.bits()));
        let args = [Value::A(p(vm, Some(item))), Value::I(damage.into()), Value::I(dtype.0.into()),
                    Value::I(damage.into()) /* out newDamage, initialised = damage */,
                    Value::I(passive as i64), Value::A(p(vm, inflictor)), Value::A(p(vm, source)), Value::I(flags.into())];
        vm.call_index(f, &args, h)?;
        Ok(vm.ret_i(1).clamp(0, i32::MAX as i64) as i32)    // rets[1] = first out param (newDamage)
    }
}
```

```rust
// acs.rs — ScriptCall marshalling by callee signature
fn script_call(&mut self, args: &[i32], activator: u64) -> i32 {
    let class = self.world.acs.resolve_string(args[0]);
    let func  = self.world.acs.resolve_string(args[1]);
    let rest = &args[2..];
    let r = self.world.with_zscript(|rt, h| {
        let Some((fid, sig)) = rt.static_method(&class, &func) else { return Err(Trap::Native(format!("ScriptCall: {class}.{func} not found"))); };
        let mut vals = Vec::with_capacity(sig.params.len());
        for (k, p) in sig.params.iter().enumerate() {
            let raw = rest.get(k).copied().unwrap_or(0);
            vals.push(match p.ty.bank() {
                Bank::S => Value::S(rt.vm_mut().heap.intern_bytes(h.acs_string(raw).as_slice())),
                Bank::F => Value::F(f64::from(raw) / 65536.0),   // ACS fixed → double
                Bank::A => Value::A(h.acs_actor_proxy(raw, activator)), // actor params take a TID (0 = activator)
                Bank::I => Value::I(raw.into()),
            });
        }
        rt.vm_mut().call_index(fid, &vals, h)?;
        Ok(match sig.rets.first().map(ZType::bank) {
            Some(Bank::S) => h.acs_intern_string(rt.vm().heap.bytes(rt.vm().ret_s(0))),  // (str)ScriptCall
            Some(Bank::F) => (rt.vm().ret_f(0) * 65536.0) as i32,
            Some(Bank::A) => h.actor_tid(rt.vm().ret_a(0)),
            _ => rt.vm().ret_i(0) as i32,
        })
    });
    match r { Some(Ok(v)) => v, Some(Err(e)) => { self.world.zscript_trap(e); 0 }, None => self.legacy_script_call(&class, &func, rest) }
}
```

`Wads.*` on `room-vfs`:
- `CheckNumForFullName` searches PK3 full paths, case-insensitively.
- `CheckNumForName(name, ns)` follows the GZDoom namespace rules.
- `FindLump(name, start)` returns the next index after `start`.
- `ReadLump` returns raw bytes.

Lump numbers must match the numbering ACS uses
(`room_vfs::LumpId::index()`). BD_Hash reads the current map's lumps, so
`LumpOpen` with `start == -2` is the common case.

### 14.5 Saves, determinism, netplay

- `ZscriptSaveBlob` v2 serialises the **whole heap** in slot order: every
  `ObjKind`, string bytes, array contents, and actor proxies as actor
  indices. It also saves handler objects by class name and the
  `program_hash`.
- A load with a different hash is refused, as GZDoom refuses mismatched
  saves.
- Event dispatch order is the handler order from MAPINFO, then
  `Order`/`priority` (GZDoom `EventHandler.Order`).
- No `BTreeMap` keyed by handle values crosses a save.
- `World::checksum` mixes `ZscriptRuntime::checksum_bytes()`, which already
  exists, extended to all banks.

### 14.6 Acceptance: the probe becomes the conformance gate

Add `crates/room-zscript/tests/conformance.rs`. It compiles the probe below
with the Room stdlib, runs each function on `Zvm` with a `MockHost`, and
asserts the exact values. Today it fails; it must pass before ZS2 is
called done again.

```zscript
// testdata/zscript/conformance/probe.zs
class Probe
{
    static int Ret()     { return 5; }
    static int Add()     { int t = 2; t += 3; return t; }                          // 5
    static int Loop()    { int t = 0; for (int i = 0; i < 10; i++) t += i; return t; } // 45
    static int Wh()      { int i = 0; while (i < 7) i++; return i; }                // 7
    static int DoW()     { int i = 0; do { i += 2; } while (i < 7); return i; }     // 8
    static int Sw(int x) { switch (x) { case 1: return 10; case 2: return 20; default: return 99; } } // 20 / 99
    static int SwFall(int x) { int r = 0; switch (x) { case 1: r += 1; case 2: r += 2; break; case 3: r = 30; } return r; } // x=1 → 3
    static int Tern(int x) { return (x > 3) ? 1 : -1; }                             // 1 / -1
    static int If1(int x)  { if (x > 3) return 1; return -1; }                      // -1 for x=1
    static int Call()    { return Probe.Ret() + 1; }                                // 6
    static int Brk()     { int i; for (i = 0; i < 100; i++) { if (i == 4) break; } return i; } // 4
    static int Cont()    { int t = 0; for (int i = 0; i < 5; i++) { if (i == 2) continue; t += i; } return t; } // 8
    static int Fmt()     { String s = String.Format("%i", 42); return s.ToInt(10); } // 42
    static int Chr()     { String b = "AB"; return b.CharCodeAt(1); }               // 66
    static int Mid()     { String b = "hello"; return b.Mid(1, 3).Length(); }       // 3
    static int Arr()     { Array<int> a; a.Push(3); a.Push(4); a.Insert(0, 9); return a.Size() * 100 + a[0]; } // 309
    static int ArrDel()  { Array<int> a; a.Push(3); a.Push(4); a.Delete(0); return a[0]; } // 4
    static int ArrS()    { Array<string> a; a.Push("x"); a.Insert(0, "yz"); return a[0].Length() + a.Size() * 10; } // 22
    static int MapLen()  { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insert(5, "abc"); return m.get(5).Length(); } // 3
    static int MapInt()  { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insertInt(7, 9); m.insertInt(3, 1); return m.getInt(7) * 10 + m.getInt(3); } // 91
    static int MapDel()  { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insert(1, "a"); m.remove(1); return m.checkKey(1); } // 0
    static int Bits()    { int v = 0x1234; return ((v >> 4) & 0xff) | (1 << 12); }   // 0x1123
    static int Neg()     { int temp = 200; return (temp > 127) ? (-temp) : temp; }   // -200
    static bool NotNot() { Actor a = null; return !!a; }                            // false
    static int Cast()    { Object o = new("ZC_IntStrMap"); ZC_IntStrMap m = ZC_IntStrMap(o); return m != null; } // 1
    static int CastBad() { Object o = new("ZC_IntStrMap"); EventHandler e = EventHandler(o); return e == null; } // 1
    static double Fl()   { double d = 1.5; return d * 2; }                          // 3.0
    static int Out()     { int r; Probe.SetOut(r); return r; }                      // 77
    static void SetOut(out int r) { r = 77; }
}
```

Plus BD-specific integration tests.
- **Fixture.** The tests need the real BD sources. Read them from
  `$ROOM_BD_PK3` when it is set, otherwise from
  `testdata/zscript/bd_real/`, which holds the three files verbatim.
  `bd_like/` stays for the old smoke test.
- **Placement.** Each test lives in `room-sim/tests/zscript_bd.rs` or
  `room-game/tests/bd_zscript_e2e.rs`, as listed.

| Test | Asserts |
|---|---|
| `bd_compiles_and_links` | `check()` 0 errors, 0 unbound natives, 0 `poison` values |
| `bd_lump_api_via_acs` | ACS `ScriptCall("ZandroCompat","LumpOpen","MAP01",-2,0)` on a test WAD returns lump id; `LumpRead(l,0,UBYTE)` equals byte 0 of the lump; `(str)LumpReadString(l,0,4)` equals the first 4 bytes, including bytes ≥ 0x80; `LumpGetInfoInt(l,SIZE)` = lump size; `LumpClose` then `LumpRead` → 0 + printf |
| `bd_damage_control_skill6` | skill index 6: after level start the player owns `Damage_Control` (amount 1), and **every** monster spawned by the map and by `summon` owns it |
| `bd_damage_scaling` | skill 6: a zombieman hitscan that would deal *d* deals 7·*d* to the player; `Burn` type → 3·*d*; `TehLostSoul` source → 2·*d*; skill < 6 → *d* |
| `bd_player_tid_roundtrip` | `SetPlayerTID(0, 1000)` then `GetPlayerTID(0)` = 1000 and `World` actor TID = 1000 |
| `bd_gender` | `bd_Gender=1` → `SoundClass == "fplayer"`, owns `GenderFemale`, not `GenderMale` |
| `bd_save_load_mid_map` | save after `LumpOpen` (open handles in `ZC_TMapHack` maps), load, `LumpReadString` still works; checksums equal |
| `bd_two_runs_same_checksum` | 2 × 2,000 tics with identical inputs → identical `World::checksum` |

### 14.7 Work plan (replaces the "done" claims for ZS1–ZS4)

| ID | Scope | Size (est.) | Done when |
|---|---|---:|---|
| **ZS1b** | Typed sema: `types.rs`, `ClassTable`, fields/methods with signatures, layouts per bank, vtables, consts/enums folding, globals.zs/builtins.zs declarations | ~1.8 k LOC | BD + probe type-check; unknown names are errors |
| **ZS2b** | `lower.rs` (all statements/expressions above), VM ABI (`Arg/Call/CallVirt/CallNat/Ret/GetRet`), field/array/obj ops, byte strings, `ZHost`, string/array/math natives; delete `compile.rs`, `natives_bd.rs` | ~3.5 k LOC | §14.6 probe 100 % green, interpreter fuzz (no panics) |
| **ZS3b** | `register_zscript_class` + token-based `Default` + script hooks, actor proxies, `with_zscript`, delete thread-locals | ~1.2 k LOC | ZScript `Inventory` subclass givable/summonable; `Tick`/`PostBeginPlay` overrides run with real `self` |
| **ZS4b** | Event dispatch with real `WorldEvent`/`PlayerEvent`, `ModifyDamage` hook (+ `damage_type` plumbing), `ScriptCall` marshalling, `Wads.*` on VFS | ~0.9 k LOC | all `bd_*` tests in §14.6 green |
| **ZS6b** | Save blob v2 (whole heap), checksum all banks | ~0.4 k LOC | `bd_save_load_mid_map`, `bd_two_runs_same_checksum` |

Order: ZS1b → ZS2b → ZS3b → ZS4b → ZS6b. ZS5 (DECORATE expressions) and
ZS7+ keep their current definitions. `deco_zvm.rs` must move to the new
`lower.rs` API once ZS2b lands, replacing its private `compile_expr`.

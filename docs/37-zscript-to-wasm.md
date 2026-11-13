# 37 — ZScript → WebAssembly compiler (`room-zvm-wasm`)

Status: **ZW0–ZW9 implemented (2026-11-13)**, with the §6 design replaced by
the engine-side heap design of §0.2; ZW4 not yet run in a browser. See §0.1.
Originally a proposal (2026-11-03). Audience: an engineer or LLM implementing the
crate. Builds on `docs/34` (ZScript front end and VM), `docs/12` (WASM mod
ABI v1) and `docs/31` (mod API extensions). It amends ADR
`docs/decisions/0005-zscript-vm.md`: the dropped ZScript→WASM transpiler
returns, but as a **backend of the ZScript VM**, not as a second ZScript
implementation (ADR `0006`).

---

## 0. TL;DR

1. **What.** A new crate, `crates/room-zvm-wasm`, translates a ZVM
   `CompiledProgram` (the register bytecode produced by
   `room-zscript/src/lower.rs`) into a WebAssembly module. There is one
   front end, one bytecode and one semantics; WASM is just another way to
   execute the bytecode.
2. **Two targets from one code generator.**
   - **`Target::Aot` (Tier 2 of the ZVM).** The module imports a small
     runtime (`"zvm"` module) that operates on the engine's existing
     `ZvmHeap`, natives and play host. The wasm engine already in each shell
     compiles it to machine code: wasmtime/Cranelift on desktop, the
     browser's WebAssembly JIT on web. This gives JIT speed on the web
     (impossible with a hand-written JIT inside a wasm32 engine) and
     **replaces the bespoke Cranelift JIT planned as ZS9**.
   - **`Target::RoomMod`.** A self-contained `.wasm` mod that follows the
     existing **ABI v1** (`room_register`/`room_dispatch`, `"room"`
     imports). It links a small guest runtime (`room-zrt`: heap, strings,
     arrays, GC in linear memory). ZScript mods can then ship precompiled
     and run on the existing WASM host with its sandbox, fuel, saves and
     fingerprints, with no ZScript compiler at load time.
3. **Correctness is defined by the interpreter.** Every program must give
   the same results, the same traps (kind, function, line) and the same fuel
   consumption on interpreter and WASM. A differential test harness
   enforces this (§8). Two small interpreter changes make that possible:
   fuel charged per basic block, and an explicit overflow trap for
   `i64::MIN / -1`.
4. **Main use case: converting existing mods** (§6.6). `room-cli mod
   convert in.pk3 -o out.pk3` replaces a legacy mod's ZScript with a WASM
   module and keeps all other lumps. A new `zs_native` import gives WASM
   mods access to every native the ZVM implements. The converted mod is
   verified tic by tic against the original running on the VM.
5. Input is bytecode, not AST, so DECORATE expressions compiled to ZVM
   (`deco_zvm.rs`, ZS5) can also be AOT-compiled at no extra cost.
6. Milestones **ZW0–ZW9** are in §9.

### 0.1 Implementation status (2026-11-13)

| ID | Status | What exists |
|---|---|---|
| ZW0 | **done** | `room_zvm::has_ext`, `block_costs` / `jump_target` / `next_insn` (`program.rs`); the interpreter charges fuel per basic block on entry; `i64::MIN / -1` and `% -1` trap `Trap::Overflow`; `Zvm::native_call` shared by both tiers; heap opcodes moved to `room-zvm/src/ops.rs` and used by the interpreter and the AOT imports |
| ZW1 | **done** | `crates/room-zvm-wasm`: decoder, basic blocks from the shared leader set, CFG, reverse postorder, Cooper–Harvey–Kennedy dominators, reducibility check, Ramsey structuring (`loop`/`block`/`if` + `br`), every opcode except legacy `CallStatic`/`CallNative`, fuel per block, depth limit, `wasmparser` validation, `--wat` via `wasmprinter` |
| ZW2 | **done** | `Arg` snapshots in temps, static `Call` (register image, `write_value` resolved at compile time), `CallVirt` through `zvm.vcall` + `call_indirect`, return slots as tagged cells in linear memory, `CallNat` through argument cells, `zvm.call_interp` bridge into the interpreter for unsupported callees, the interpreter's `Return`→caller `i[0]` quirk |
| ZW3 | **done** (perf gate partly) | `room-zvm` features `aot` (wasmi) and `aot-wasmtime` (wasmtime 26 / Cranelift): `src/aot.rs` holds the engine-neutral imports and run logic, `src/aot/{wasmi,wasmtime}_backend.rs` the engine glue. `Zvm::attach_aot`, tier switch in `run_with_result`, module/artifact caches keyed by content digest. `ROOM_ZVM_TIER=aot` in `room-sim` (features `zvm-aot` / `zvm-aot-jit`; `room-app` uses `zvm-aot-jit`); `ROOM_ZVM_ENGINE=wasmi` forces wasmi. ZS9 is superseded. The "≥ 5× on trivial tick" gate is **not met**: see the timings below |
| ZW4 | **implemented, not run in a browser** | `room-zvm` feature `aot-web` (`src/aot/web_backend.rs`): the module is instantiated asynchronously by the browser's `WebAssembly` (its JIT); the `"zvm"` imports are JS closures calling the same engine-neutral import bodies; guest memory is mirrored through a 16 KiB scratch copy. `room-web/src/zvm_web.rs` compiles the play program (or takes a converted mod's shipped module) when the page has `?zvm=aot` (`aot-all` = every function), interprets until the instance is ready, then attaches it through `room_zvm::set_aot_provider` / `ZscriptRuntime::refresh_aot`. Type-checked on the host only: this environment has no wasm32 standard library and no browser |
| ZW5 | **partial** | (1) `lower.rs` loaded every library global at every function entry: the UZDoom library made each function start with ~65 native calls; it now loads only globals the function can name (stdlib bytecode 421k → 139k instructions, wasm 11.3 MB → 3.1 MB, same `stdlib_probe` and test results); (2) `auto` entry policy: only functions with a loop or > 48 instructions are entered through wasm from the host, short ones stay on the cheaper interpreter entry (`ROOM_ZVM_TIER=aot-all` / `AotPolicy::All` enters everything); (3) the VM is swapped, not reallocated, around a call and `ZvmPlayHost` is boxed (VM 1,728 → 648 bytes); entry masks cached; module/artifact caches. Open: heap view (inline field access), peephole |
| ZW6 | **done (§0.2 design)** | `Target::RoomMod`: the AOT module of the *unlinked* program (native ids = `native_imports` indices, mapped at attach by `Zvm::attach_shipped`) plus custom sections `room.zvm.program` (`CompiledProgram::to_bytes`, `room-zvm/src/program_codec.rs`), `room.zvm.flags`, `room.zvm.abi` and `room.zvm.sources`. `read_room_mod` validates the module, its imports and sections; `register_shipped` / `shipped_for` hand it to the VM. No `room-zrt` |
| ZW7 | **done** | `room zscript wasm … --report <md>` (`room-zvm-wasm/src/report.rs`): natives referenced, implemented or missing, call sites, functions left interpreted. Checked in as `docs/qa/zscript-wasm-coverage.md` for Brutal Doom (28 natives, 0 missing, 32/32 functions) and the UZDoom library (968 natives, 0 missing, 2,181/2,182 functions) |
| ZW8 | **done (§0.2 design)** | No `zs_native` ABI import or host-object table is needed: compiled code calls every ZVM native through `zvm.native`, against the engine's own objects. Brutal Doom converts with 0 unavailable natives. Registration order is the existing ZScript pipeline's (the program's `States`/`Default` bodies merge into the class registry as before). UI scope: when the mod sets `GameInfo.StatusBarClass`, `room mod convert` also writes `room/zscript_ui.wasm` (UZDoom library + mod), which `load_status_bar_program` uses |
| ZW9 | **done** | `room mod convert in.pk3|dir -o out.pk3 [--iwad] [--map] [--verify N] [--skill] [--keep-zscript] [--allow-missing-natives]` (`room-game/src/mod_convert.rs`): writes every input entry except the ZScript sources, `room/zscript.wasm`, optionally `room/zscript_ui.wasm` and `room/zscript-src/*.src`, and `room/convert.json`; verification plays the original (interpreter) and the converted package (shipped wasm, every function entered through wasm) headless with scripted input, saves and reloads both halfway, and compares world and ZScript checksums every tic. `load_zscript_from_vfs` loads converted packages without compiling ZScript and warns when the record does not say `"verified": true`. Brutal Doom: 32/32 functions, 2,000 tics on Freedoom E1M1 at skill 6 identical, 3,158 calls as wasm |

### 0.2 Amendment: engine-side heap instead of `room-zrt` (2026-11-13)

§6 (`Target::RoomMod` as a standalone ABI v1 mod with its own guest runtime)
is **not** what was built. The project owner chose to keep the heap,
strings, arrays, natives and actor proxies in the engine for converted mods,
exactly as the AOT tier uses them:

- A converted package ships `room/zscript.wasm`: the AOT code plus the
  program image (functions, constants, classes, vtables, native names,
  `States`/`Default` bodies). The game reads the program from the module
  instead of compiling ZScript, and runs the shipped code through the AOT
  tier (wasmtime on desktop, the browser engine with `?zvm=aot`, the
  interpreter otherwise). The module imports only the `"zvm"` runtime, is
  validated before use, and is fuel- and depth-limited by its own code.
- Consequences: one semantics end to end (the converted mod *is* the
  interpreter's program), saves and checksums work unchanged, and
  verification compares raw checksums (no semantic digest needed). The
  module is not an ABI v1 ROOMMOD: no `room.toml`, it does not run on the
  `room-script` host, and it depends on the Room build's native table (the
  `room.zvm.abi` section rejects modules built against another import list).
- ZScript is compiled as one program per game. The module records the
  sources outside the package that went into it (the engine's own
  `room.pk3` ZScript); if the game loads other ZScript too (another
  unconverted mod), the module cannot be used and the loader says to convert
  again. Converting several ZScript mods together is not supported.
- `zs_native_find`/`zs_native_call`, the host-object table and
  `room_register_late` (§6.6.1, §6.6.2 step 5) are unnecessary in this
  design and were not added.

Differences from the plan, decided while implementing:
- **Calling convention.** Every compiled function has the wasm type `() -> ()`.
  Arguments go through a register image at `IMG` in the module's own memory
  (zeroed, then the parameters written with the interpreter's `write_value`
  rules). The prologue loads parameter registers plus a fixed entry window
  (`i[0..8]`, `f[0..4]`, `s[0..4]`, `a[0..8]`); every other register starts at
  zero. A host entry whose frame has a non-zero register outside that set runs
  on the interpreter. One function type makes `CallVirt` a plain
  `call_indirect`; the host builds the callee image in `zvm.vcall`.
- **Return slots** are eight tagged 16-byte cells (`aot_abi::RETS`), not
  globals: natives and the interpreter bridge write them directly.
- **Exit record.** The outermost AOT frame writes `kind` (Ret / Return /
  implicit), the value cell, `i[0..4]`, `s[0]`, `f[0]` at `aot_abi::EXIT`, so
  `last_i` / `last_s` / `last_f` / the result match the interpreter exactly.
- **Traps** raised inside imports (null, bounds, natives, unbound virtual
  slot) are recorded in the store and abort the wasm call; generated code
  traps through `zvm.trap(kind, func, pc, extra)` for divide, overflow,
  explicit `Trap`, fuel and depth.
- **Host safety.** During a call the `Zvm` is swapped into the wasm store
  (which keeps a placeholder VM between calls) and swapped back afterwards,
  so imports reach it without `unsafe`.
- The shared contract (memory layout, cell tags, import list, image layout)
  is `room_zvm::aot_abi`, used by both the code generator and the host.

Evidence (2026-11-13, local runs):
- `cargo test -p room-zvm-wasm`: differential tests on hand-written bytecode
  (arithmetic, loops, divide-by-zero, `MIN/-1`, fuel exhaustion at nine fuel
  budgets, NaN / ±0 / infinity / `F2I` saturation, `CmpF`, calls, `Return`
  quirk, `GetRet` bank mismatches, recursion with stack overflow at four limits,
  objects, arrays, strings, virtual calls, null and bounds traps, explicit
  `Trap`, legacy fallback through the bridge) and on ZScript source (loops,
  `switch` with fall-through and `continue`, `&&`/`||`/`?:`, nested calls).
  Results, traps (kind, function, line), fuel left, heap digest and the whole
  play-host state must be equal.
- UZDoom stdlib (`wadsrc/uzdoom`): 2,181 / 2,182 functions compile (one
  `ArgAcrossBlocks`), 11.3 MB of wasm, ~0.7 s codegen.
  `room-sim/tests/stdlib_probe` (351 stdlib loads, every stdlib action run on
  a live world) prints the same effect report on both tiers.
- `room-sim` tests (`zscript_play`, `skill_levels`, `weapon_timeline`, …) give
  the same pass/fail set with `ROOM_ZVM_TIER=aot` as on the interpreter.
- **Brutal Doom** (`brutal22test6`): all 32 functions compile (28 KB).
  `room zscript wasm <bd.pk3|dir> --bd` runs 2,318 calls identically on both
  tiers: `ZC_IntStrMap` against a Rust model of BD's algorithm (duplicates
  included), BD's `ZandroCompat` lump API reading SNDINFO/KEYCONF/CVARINFO back
  byte by byte, and every function with seeded arguments.
  `room-game/tests/bd_zscript_aot.rs` runs BD on Freedoom E1M1 at skill 6 for
  2,100 tics twice (interpreter, AOT): `World::checksum` and the ZScript
  checksum are equal every tic, 2,855 ZScript calls served by wasm, on both
  wasmi and wasmtime.
- All of the above also pass with `--features wasmtime` (and with
  `ROOM_ZVM_ENGINE=wasmi` on that build).

Timings (`cargo run --release -p room-zvm-wasm --features wasmtime --example
aot_bench`, x86-64 container, wasmtime 26):

| Workload | Interpreter | AOT (wasmtime) | |
|---|---|---|---|
| Collatz sweep 1..30000 (one call, loops + calls) | 924 ms | 24 ms | ×39 |
| float polynomial, 2·10⁶ iterations | 763 ms | 3.2 ms | ×239 |
| 10,000 calls of a one-line function | 1.8 ms | 3.4 ms | ×0.5 |
| Brutal Doom workloads (2,318 calls) | 13.5 ms | 5.7 ms | ×2.4 |

Compiled loops are 40–240× faster. Each top-level call into wasm costs
about 0.3 µs of fixed host overhead (VM swap, register image, return cells,
globals). Since ZW5's `auto` policy, short straight-line functions keep the
interpreter's cheaper entry, so 10,000 calls of a one-line function now take
the same 1.6 ms on both tiers (they were ×0.5 before). The UZDoom library is
3.1 MB of wasm after the ZW5 global-load fix (was 11.3 MB); it is compiled
once per process and cached.

Commands:

```
room mod convert brutal22test6.pk3 -o bd-converted.pk3 --iwad doom2.wad --skill 6
room zscript wasm brutal22test6.pk3 --report coverage.md
room zscript wasm --stdlib --report stdlib-coverage.md
room zscript wasm brutal22test6.pk3 -o bd.wasm --wat bd.wat --check
room zscript wasm brutal22test6.pk3 --bd
ROOM_ZVM_TIER=aot ./target/release/room ...        # desktop game, AOT tier
ROOM_BRUTALDOOM_PK3=… cargo test -p room-game --features zvm-aot-jit --test bd_zscript_aot
cargo run --release -p room-zvm-wasm --features wasmtime --example aot_bench [bd.pk3]
```

---

## 1. Why, and what it replaces

| Need | Today | With `room-zvm-wasm` |
|---|---|---|
| Fast ZScript on **web** | interpreter only (no JIT possible inside wasm32) | the browser JIT compiles the AOT module |
| Fast ZScript on **desktop** | ZS9 "Cranelift JIT" (custom codegen, 40%, deferred) | wasmtime already embeds Cranelift; Room only emits WASM. **ZS9 is cancelled and replaced by ZW3** |
| Ship ZScript mods without the compiler, with the WASM sandbox | not possible | `--target room-mod` produces a ROOMMOD module |
| UZDoom stdlib (`docs/36` §4) hot code | interpreted | AOT-compiled, cached by program hash |

What it does **not** do:
- It does not replace the interpreter. The interpreter stays the reference,
  handles cold code, and serves hosts without a wasm engine (`wasmi` gives
  no speed-up).
- It does not give ZScript a second semantics.
- It does not run arbitrary GZDoom mods through ABI v1 when they use natives
  the ABI does not expose (§6.5 says how that is reported).

---

## 2. Architecture

```mermaid
flowchart LR
  zs[ZScript sources] --> fe[room-zscript<br/>parse · sema · lower]
  deco[DECORATE exprs] --> dz[room-sim deco_zvm]
  fe --> bc[CompiledProgram<br/>ZVM bytecode]
  dz --> bc
  bc --> interp[room-zvm interpreter<br/>Tier 0 · reference]
  bc --> cg[room-zvm-wasm<br/>decode → CFG → structure → emit]
  cg -->|Target::Aot| aot[module imports "zvm"]
  cg -->|Target::RoomMod| link[link with room-zrt.wasm] --> mod[ROOMMOD .wasm<br/>ABI v1]
  aot --> wt[wasmtime / Cranelift<br/>desktop]
  aot --> br[browser WebAssembly<br/>room-web]
  mod --> host[room-script host<br/>wasmi · wasmtime · browser]
```

New crates:

| Crate | Kind | Purpose |
|---|---|---|
| `room-zvm-wasm` | lib (+ `room-cli zscript wasm`) | bytecode → WASM code generator, both targets |
| `room-zrt` | `no_std` lib built for `wasm32-unknown-unknown` | guest runtime for `Target::RoomMod`: heap, strings, arrays, maps, GC, payload decoding, `room_*` exports |

Dependencies:
- `wasm-encoder` emits modules.
- `wasmparser` validates them (already a workspace dependency, 0.224).
- `walrus` links `RoomMod` modules into the `room-zrt` template.
- The `wasmtime` backend in `room-script` gains a `zvm-aot` feature.

Nothing new runs inside `room-sim`'s deterministic core except calls into
an already-compiled module.

---

## 3. Input contract (`room-zvm` bytecode)

Source of truth: `crates/room-zvm/src/{opcode,program,interp}.rs`. The code
generator matches **exhaustively** on `Op`, with no `_ =>` arm, so that adding
an opcode is a compile error in `room-zvm-wasm`.

- **Instruction encoding:** a `u32` word, `pack(op, a, b, c)`, optionally
  followed by one extension word (`ext`). The ops with an ext word are the
  ones `emit_ext` produces: `LoadConst*`, `Jmp`, `JmpIf`, `JmpIfNot`,
  `CallStatic`, `CallNative`, `Call`, `CallVirt`, `CallNat`, `NewObj`,
  `IsClass`, and any later additions. `room-zvm` must export
  `pub fn has_ext(op: Op) -> bool`, used by both the interpreter and the
  code generator, so they cannot disagree.
- **Jumps:** `ext` is a signed offset relative to the `pc` after the ext
  word (check `take_ext`). The decoder converts it to absolute word indices.
- **Registers:**
  - I bank `i64`, F bank `f64`, S bank `StrHandle(u32)`, A bank
    `ObjHandle` (`index: u32`, `generation: u32`).
  - Counts are `n_i/n_f/n_s/n_a: u8`.
  - Parameters are `CompiledFunction::params: Vec<FuncParam { bank, reg }>`.
- **Calls:** `Arg a=bank b=reg` pushes onto the VM argument stack. Then:
  - `Call ext=func_idx` pops them into the callee's `params`;
  - `CallVirt ext=slot` resolves through `program.vtables[class_id][slot]`,
    where the receiver is the first A-bank argument;
  - `CallNat ext=native_id`, where `b` encodes the result bank (0 = i,
    1 = f, 2 = s, 3 = a).

  `Ret a=bank b=reg c=index` sets `rets[index]` and returns.
  `GetRet a=bank b=dst c=index` reads it after a call. `Return` returns with
  no value.
- **Legacy ops:** `CallStatic` (name-resolved) and `CallNative` are produced
  only by legacy paths. The code generator **rejects** them, and the
  function stays interpreted (`Unsupported::LegacyCall`). `lower.rs` must
  not emit them for new code.
- **Traps:** an explicit `Trap` op, plus implicit traps on divide by zero,
  null dereference, bad array index, fuel and stack depth.

---

## 4. Code generation

### 4.1 Pipeline per function

```
decode(code) → Vec<Insn{pc, op, a, b, c, ext}>
  → split basic blocks (leaders: 0, jump targets, insn after Jmp/JmpIf/JmpIfNot/Ret/Return/Trap)
  → CFG + reverse postorder + dominator tree (Cooper–Harvey–Kennedy)
  → reducibility check (every back edge targets a dominator); irreducible → Unsupported
  → structured emission (Ramsey, "Beyond Relooper", 2022): loop headers → `loop`,
    merge nodes → nested `block`s ordered by RPO, edges → `br`/`br_if`/fallthrough
  → per-insn lowering (§4.3) into wasm-encoder `Function`
```

ZScript has no `goto`, and `lower.rs` emits only structured jumps, so the
CFG is always reducible. The check is a guard, and an `Unsupported` function
is never an error: it stays interpreted, and calls into it go through a
bridge import (`zvm.call_interp`), so programs can mix tiers.

Reference sketch of the structuring step:

```rust
fn do_tree(&mut self, x: BlockId, ctx: &mut Vec<Ctx>) {
    let merges: Vec<BlockId> = self.dom.children(x).filter(|&c| self.is_merge(c))
        .sorted_by_key(|&c| std::cmp::Reverse(self.rpo_index[c])).collect();
    if self.is_loop_header(x) {
        self.f.instruction(&Instruction::Loop(BlockType::Empty));
        ctx.push(Ctx::LoopHeadedBy(x));
        self.node_within(x, &merges, ctx);
        ctx.pop();
        self.f.instruction(&Instruction::End);
    } else {
        self.node_within(x, &merges, ctx);
    }
}
fn node_within(&mut self, x: BlockId, merges: &[BlockId], ctx: &mut Vec<Ctx>) {
    if let Some((&y, rest)) = merges.split_first() {
        self.f.instruction(&Instruction::Block(BlockType::Empty));
        ctx.push(Ctx::BlockFollowedBy(y));
        self.node_within(x, rest, ctx);
        ctx.pop();
        self.f.instruction(&Instruction::End);
        self.do_tree(y, ctx);
    } else {
        self.emit_block_body(x, ctx);                // §4.3 straight-line code
        match self.terminator(x) {
            Term::Jmp(t) => self.do_branch(x, t, ctx),
            Term::Cond { reg, if_true, if_false } => {   // JmpIf/JmpIfNot normalised
                self.local_get_i(reg); self.f.instruction(&Instruction::I64Const(0));
                self.f.instruction(&Instruction::I64Ne);
                self.f.instruction(&Instruction::If(BlockType::Empty));
                ctx.push(Ctx::IfThenElse);
                self.do_branch(x, if_true, ctx);
                self.f.instruction(&Instruction::Else);
                self.do_branch(x, if_false, ctx);
                ctx.pop();
                self.f.instruction(&Instruction::End);
            }
            Term::Return | Term::Trap => {}            // already emitted
            Term::Fallthrough(t) => self.do_branch(x, t, ctx),
        }
    }
}
fn do_branch(&mut self, src: BlockId, dst: BlockId, ctx: &mut Vec<Ctx>) {
    if self.is_backward(src, dst) || self.is_merge(dst) {
        let depth = ctx.iter().rev().position(|c| c.targets(dst)).expect("structured");
        self.f.instruction(&Instruction::Br(depth as u32));
    } else {
        self.do_tree(dst, ctx);
    }
}
```

### 4.2 Locals, banks and function signatures

| ZVM bank | WASM type | Notes |
|---|---|---|
| I (`i64`) | `i64` | booleans are 0/1 in `i64` |
| F (`f64`) | `f64` | — |
| S (`StrHandle`) | `i32` | handle into the string table of the target runtime |
| A (`ObjHandle`) | `i64` | `index as u64 | (generation as u64) << 32`; null = 0 |

- **Locals.** Each register gets one local:
  `local_index = n_params + bank_base[bank] + reg`, with banks laid out
  I, F, S, A. Parameters map to WASM params in `FuncParam` order, and the
  prologue copies them into their register locals. The copy keeps lowering
  uniform, and Cranelift and browser JITs eliminate it. Extra temporaries
  (Arg snapshots, §4.4) come after the register locals.
- **Signatures.** Function type = params by bank, no WASM results (results
  travel through return slots, §4.4). Types are interned, and functions with
  equal param-bank sequences share a type. All implementations of one
  virtual slot are guaranteed to share a type (same ZScript signature),
  which `call_indirect` needs.

### 4.3 Instruction lowering (exact semantics)

Every row must reproduce `interp.rs` bit for bit. `rt.*` means a runtime
function: an import for `Aot`, a `room-zrt` internal function for `RoomMod`
(§5, §6).

| Op | WASM | Must match interpreter on |
|---|---|---|
| `Nop` | — | — |
| `MoveI/F/S/A a,b` | `local.get b; local.set a` | — |
| `LoadConstI a ext` | `i64.const const_i[ext]` | — |
| `LoadConstF a ext` | `f64.const const_f[ext]` (emit the **bits**, `f64.const` via `f64::from_bits`) | NaN payloads, −0.0 |
| `LoadConstS a ext` | `i32.const <string id>`; `Aot`: id = `rt.str_const(func, ext)` cached in a lazily-filled global table at instantiation; `RoomMod`: string bytes in a data segment, interned in `room_init` | — |
| `LoadNull a` | `i64.const 0` | — |
| `AddI/SubI/MulI` | `i64.add/sub/mul` | wrapping (the interpreter uses wrapping ops; verify and keep both wrapping) |
| `DivI/ModI` | `if c==0 → rt.trap(DivZero)`; `if b==i64::MIN && c==-1 → rt.trap(Overflow)`; else `i64.div_s/rem_s` | **Interpreter change (ZW0):** `lb / rc` panics on `MIN/-1` in Rust; it must return `Trap::Overflow` |
| `LtI…GeI` | `i64.lt_s…` then `i64.extend_i32_u` | result is 0/1 |
| `I2F` | `f64.convert_i64_s` | round-to-nearest, same as Rust `as` |
| `F2I` | `i64.trunc_sat_f64_s` | Rust `as` saturates and maps NaN to 0; plain `i64.trunc_f64_s` would **trap**. Requires the nontrapping-fptoint feature (all three engines have it) |
| `AddF/SubF/MulF` | `f64.add/sub/mul` | IEEE, no FMA fusion (wasm never fuses) |
| `DivF` | `if c == 0.0 → rt.trap(DivZero)` (compare with `f64.eq`, so −0.0 also traps) else `f64.div` | — |
| `BitAnd/Or/Xor` | `i64.and/or/xor` | — |
| `Shl/Shr` | `i64.shl` / `i64.shr_s` | the interpreter masks with `& 63`, and WASM masks shift counts mod 64: identical |
| `Jmp/JmpIf/JmpIfNot` | structured (§4.1) | condition is `!= 0` on the I register |
| `IsNull a b` | `local.get b; i64.eqz; i64.extend_i32_u` | — |
| `IsClass a b ext` | `rt.is_class(obj, class_id) -> i32` | null → 0 |
| `NewObj a ext` | `rt.new_obj(class_id) -> i64` | allocation order (deterministic heap) |
| `LoadField{I,S,A} a b c` / `StoreField…` | `rt.ld_i(obj, slot)`… / `rt.st_i(obj, slot, v)`… | null → `NullDeref` trap with the same func/line |
| `ArrayLen/Get/Set/Push{I,S}` | `rt.arr_*` | out of range → `Bounds` trap |
| `ConcatS a b c` | `rt.str_concat(s1, s2) -> i32` | byte strings |
| `Arg/Call/CallVirt/CallNat/Ret/GetRet/Return` | §4.4 | — |
| `Trap a ext` | `rt.trap(kind, func, pc); unreachable` | — |
| `CallStatic`, `CallNative` (legacy) | reject → `Unsupported::LegacyCall` | — |

The `pc` passed to `rt.trap` is the **bytecode** word index, so the host maps
it to a line with the existing `CompiledFunction::lines`. No custom DWARF is
needed.

### 4.4 Calls, arguments and return slots

**Arg snapshots.** `Arg` pushes the register's value *at that point*, and
later instructions may overwrite the register before `Call`. The code
generator keeps a compile-time list of pending arguments per basic block.
For each `Arg`, it allocates a fresh temp local of the right type and emits
`local.get reg; local.set tmp`. At the call, it emits the temps in push
order.

Precondition: every `Arg` and the call that consumes it are in the **same
basic block**. `lower.rs` guarantees this, because argument expressions with
control flow (`&&`, `?:`) are evaluated into a register before the `Arg`. If
the precondition is violated, the function is `Unsupported::ArgAcrossBlocks`
and is reported, so `lower.rs` can be fixed.

| Call op | Lowering |
|---|---|
| `Call ext=f` | `call $f` with the pending temps (callee is AOT); callee `Unsupported` → `call $zvm.call_interp(f, argbuf)` (§5.3) |
| `CallVirt ext=slot` | recv = first A-bank temp; `rt.vcall_target(recv, slot) -> i32` (table index, traps if unbound); `call_indirect (type $slot_sig)` |
| `CallNat ext=id b=retbank` | §5.2 (`Aot`) / §6.4 (`RoomMod`) |

**Return slots.** The interpreter has `rets: [Option<Value>; 8]` per VM. The
module mirrors them as **globals**: `$ri0..$ri7: i64`, `$rf0..$rf7: f64`,
`$rs0..$rs7: i32`, `$ra0..$ra7: i64`, plus `$rset: i32`, an 8-bit mask
emulating `Option`.
- `Ret a=bank b=reg c=idx` writes the bank's global `idx`, sets bit `idx`,
  then `return`.
- `GetRet a=bank b=dst c=idx` reads the global if its bit is set, else the
  interpreter's default `Value::I(0)` coerced to the bank. Coercion must
  match `write_value`: check it and mirror it.
- Every `Call`, `CallVirt` and `CallNat` clears `$rset` first, as the
  interpreter does with `self.rets = [None; 8]`.

**Depth limit.** The interpreter traps at `stack_limit` (256) frames.
Generated code keeps a global `$depth`: the prologue increments it and traps
at the limit with `StackOverflow`, and every exit decrements it. This makes
the trap point identical. The native wasm stack is sized to hold 256 frames
of the largest function: wasmtime `max_wasm_stack` = 2 MiB, and browsers are
far above that.

### 4.5 Fuel (deterministic and tier-independent)

Today the interpreter decrements fuel **per instruction**. Charging the
same amounts in WASM would need a check after every instruction. Instead,
**both tiers charge per basic block** (interpreter change in ZW0):

- `cost(block)` = number of bytecode instructions in the block (ext words
  don't count). `room-zvm` computes it once per function into
  `CompiledFunction::block_costs` (a pc → cost map, cached on `finalize()`).
- On block entry, the interpreter does
  `if fuel < cost { trap FuelExhausted } fuel -= cost`. The generated code
  does the same with a global:

```wasm
global.get $fuel  i64.const COST  i64.lt_u
if  i32.const FUNC  i32.const PC  call $rt.fuel_exhausted  unreachable  end
global.get $fuel  i64.const COST  i64.sub  global.set $fuel
```

A call costs its own block plus the callee's blocks. A native call costs one
instruction plus whatever the native charges through `rt.fuel_charge(n)`,
identical in both tiers.

The result: a program that exhausts fuel does so at the same block on every
tier and every peer, before any side effect of that block. Netplay never
depends on which tier a peer uses. The tier is still logged in desync
reports.

### 4.6 Floating point and determinism

- WASM f64 `add/sub/mul/div/sqrt` are IEEE-754 exact and never fused:
  identical to the interpreter on every architecture.
- Transcendentals are always **natives** (`room_core::dmath`); generated
  code never uses a host libm.
- NaN bits are not observable from ZScript (no reinterpret ops). WASM
  engines may produce non-canonical NaNs, but nothing can read the payload.
  `F2I` saturates and maps NaN to 0.
- `f64.min/max` must never be emitted for ZScript `min`/`max`, because WASM
  propagates NaN and treats −0/+0 differently from Rust `f64::min`. Those
  stay natives, or are lowered as explicit compare-and-select matching the
  Rust definition.

---

## 5. `Target::Aot` (Tier 2 of the ZVM)

### 5.1 Module shape

- **Imports** (module `"zvm"`): `rt.*` functions listed below, `memory` (for
  the argument buffer, 4 KiB, owned by the engine), `call_interp`, `trap`,
  `fuel_exhausted`, `fuel_charge`, and `native` (§5.2).
- **Exports:**
  - `f<idx>` for every compiled function;
  - `__fuel` / `__depth` globals (mutable, exported so the host can set
    fuel before a call and read it after);
  - return-slot globals `__rets_*`, so the host reads results without a
    call;
  - `__table` (funcref table for `CallVirt`).
- **Table:** one slot per compiled function. `rt.vcall_target` returns the
  slot index from `program.vtables`. An interpreted callee has a thunk slot
  that calls `call_interp`.

```text
(import "zvm" "new_obj"      (func (param i32) (result i64)))
(import "zvm" "is_class"     (func (param i64 i32) (result i32)))
(import "zvm" "ld_i"  (func (param i64 i32) (result i64)))   (import "zvm" "st_i" (func (param i64 i32 i64)))
(import "zvm" "ld_s"  (func (param i64 i32) (result i32)))   (import "zvm" "st_s" (func (param i64 i32 i32)))
(import "zvm" "ld_a"  (func (param i64 i32) (result i64)))   (import "zvm" "st_a" (func (param i64 i32 i64)))
(import "zvm" "arr_len" (func (param i64) (result i64)))     ;; + arr_get_i/arr_set_i/arr_push_i, _s variants
(import "zvm" "str_concat" (func (param i32 i32) (result i32)))
(import "zvm" "str_const"  (func (param i32 i32) (result i32)))   ;; (func_idx, const_idx) → StrHandle
(import "zvm" "vcall_target" (func (param i64 i32 i32 i32) (result i32))) ;; (recv, slot, func, pc)
(import "zvm" "native" (func (param i32 i32 i32 i32) (result i32)))     ;; (id, n_args, func, pc) → 0 ok, else trap
(import "zvm" "call_interp" (func (param i32 i32) (result i32)))       ;; (func_idx, n_args)
(import "zvm" "trap" (func (param i32 i32 i32)))                        ;; (kind, func, pc)
(import "zvm" "fuel_exhausted" (func (param i32 i32)))
(import "zvm" "memory" (memory 1))
```

Field access, arrays and strings go through imports because the heap stays
**in the engine** (`ZvmHeap`). Saves, GC, actor proxies, checksums and the
interpreter share it unchanged, which is what makes tiers interchangeable
mid-session. Hot field access is the main cost: ZW5 adds an optional
"shared heap view" (§10).

### 5.2 Natives in `Aot`

`CallNat id` runs as follows:
1. Write the pending arguments as tagged 16-byte `Value` cells into the
   imported argument buffer, `memory[0..]` as `{tag: u32, pad, payload: u64}`.
2. Call `zvm.native(id, n, func, pc)`.
3. The host decodes the cells with exactly the code the interpreter uses
   (factor `CallNat`'s bank-sorting into
   `fn native_call(&mut self, id, args: &[Value], ret_bank) -> Result<Value>`
   in `interp.rs`, called by both tiers).
4. The host writes the result into the return-slot globals through their
   exports, or returns a trap code, and the generated code then does
   `if result != 0 → unreachable` (the host has already recorded the trap).

The deferred `ZsActorOp` queue (`natives.rs`) works unchanged, because it
lives in `ZvmPlayHost`.

### 5.3 Host integration (`room-zvm`)

```rust
pub enum ExecTier { Interp, Aot }            // extends tier1.rs's enum
pub struct AotModule {                        // one per CompiledProgram hash
    pub program_hash: u64,
    pub compiled: Vec<bool>,                  // per function: AOT or interpreted
    instance: Box<dyn AotInstance>,           // wasmtime (desktop) or JS-backed (web)
}
pub trait AotInstance {
    fn call(&mut self, func: u32, args: &[Value], vm: &mut Zvm) -> Result<(), Trap>;
    fn set_fuel(&mut self, fuel: u64); fn fuel(&self) -> u64;
    fn rets(&self) -> [Option<Value>; 8];
}
impl Zvm {
    pub fn call_index_tiered(&mut self, f: u32, args: &[Value]) -> Result<(), Trap> {
        match (&mut self.aot, self.config.tier) {
            (Some(m), ExecTier::Aot) if m.compiled[f as usize] => {
                m.set_fuel(self.fuel_left);
                let r = m.call(f, args, self);   // imports re-enter `self` through a host pointer
                self.fuel_left = m.fuel(); self.rets = m.rets(); r
            }
            _ => self.call_index_interp(f, args),
        }
    }
}
```

- **Desktop (`room-app`, `room-server` verifier).** wasmtime with Cranelift.
  `Config::cranelift_nan_canonicalization(true)` is harmless but not
  required (§4.6). `Config::consume_fuel(false)`, because Room meters
  itself. Serialize compiled artifacts to `~/.cache/room/zvm-aot/<hash>.cwasm`.
- **Web (`room-web`).**
  - The engine is itself a wasm module. JS does
    `WebAssembly.compile(bytes)` and instantiates with imports that are the
    **engine instance's own exported functions** (`zvm_rt_new_obj`, …,
    exported `#[no_mangle] extern "C"` from `room-web`) plus the engine's
    `memory`. Calls between the two modules are then direct wasm→wasm
    calls, with no JS glue on the hot path.
  - The host pointer is a global in the engine module, set before each call.
  - Compiled modules are cached in IndexedDB by program hash.
- **wasmi builds** keep `Interp`: AOT on an interpreter gives no gain.
- **GC** runs only between tics (`docs/34` §4.3). No AOT frames are live
  then, so locals holding `ObjHandle`s never need to be roots. Assert
  `depth == 0` when GC runs.

---

## 6. `Target::RoomMod` (standalone ABI v1 mods)

### 6.1 Module layout

`room-zrt` is a `no_std` Rust crate compiled once to `room-zrt.wasm` (the
template). It contains:
- the six required exports (`room_abi_version`, `room_alloc`, `room_free`,
  `room_register`, `room_init`, `room_dispatch`);
- the guest heap (objects, byte strings, `Array<T>` for all banks,
  `Map<K,V>` as sorted vectors) with a deterministic mark-sweep GC;
- payload decoders for every `room-script-abi` payload struct;
- the "native shims" that implement ZScript natives on top of `"room"`
  imports (§6.4).

`room-zvm-wasm` loads the template with `walrus` and then:
1. appends one function per compiled ZScript function, lowered against
   `room-zrt`'s internal `rt_*` functions (direct calls, not imports);
2. appends the `CallVirt` table segment;
3. appends a data segment with string constants and a **registration
   script** (§6.2);
4. replaces two template functions, `zrt_dispatch_user(cb, payload)` and
   `zrt_register_user()`, with generated bodies;
5. runs `wasmparser` validation and writes the module.

The result is a normal ROOMMOD. Packaging (`room.toml` with `[[module]]
scope="play"`) and the loader (`room-script/src/lib.rs`) need no change.

### 6.2 Registration

ZScript declarations become `reg_*` calls (`docs/12` §6), executed in
`room_register`:

| ZScript | Registration |
|---|---|
| `class Foo : Bar replaces Baz` (actor-derived) | `reg_class_begin("Foo", class_find("Bar"), class_find("Baz"), doomednum, flags)` |
| `Default { Health 60; +FLOAT; Speed 8; Inventory.MaxAmount 1; … }` | `reg_class_property_i32/f64/str`, `reg_class_flag` (PropId/FlagId from the ABI crate; unknown property → compile error) |
| `States { Spawn: TROO A 10 A_Look; Loop … }` | `reg_state_label`, `reg_state`, `reg_state_flow`, `reg_state_action` (native actions) |
| state calling a ZScript action or anonymous function `{ … }` | `reg_state(..., callback_id)` where `callback_id` names the compiled function |
| member fields of actor classes | `reg_class_field(name, FieldType, len)` → host-side per-actor storage (`docs/12` §7); `LoadField*`/`StoreField*` on an actor receiver lower to `actor_field_get/set` imports instead of guest heap access |
| `override void Tick()` and other virtuals | `reg_class_virtual(VirtualSlot::Tick, cb)` |
| `Super.Tick()` | `actor_call_super(self, slot, …)` |
| `class H : EventHandler` | `reg_event_handler("H", order, flags, event_mask_of_overrides, cb_base)` |
| `static` functions (ACS `ScriptCall`) | `reg_static_function("Class", "Func", cb)` |
| plain classes (non-actor `Object`) | nothing: they live in the guest heap |

Callback ids are dense indices into a generated dispatch table. The
`room_dispatch(cb, ptr, len)` body is a `br_table` over `cb`: each arm
decodes the payload (`ActionPayload`, `DamagePayload`, `WorldEventPayload`,
…), materialises proxies and event structs in the guest heap, calls the
compiled function, then encodes the return (`i64`: damage, bool,
`StateRef`).

### 6.3 Actors and objects in the guest

- **Actors** are host objects. In the guest, an A-bank value is either a
  **guest object handle** (bit 63 clear: `index | generation << 32` into
  the zrt heap) or an **`ActorRef`** (bit 63 set, low 63 bits = ABI
  `ActorRef`). `rt_is_class`, field access and virtual calls branch on that
  bit. For actors, they use `actor_class` and `class_is_descendant`, and
  `actor_field_*` for script fields. Native fields (`health`, `pos`, `vel`,
  `angle`, `bFLAG`) become `actor_get_*` / `actor_set_*` / `actor_flag`
  with stable `PropId`/`FlagId` from a table generated from
  `room-script-abi`.
- **Guest heap.** It uses the same object model as `ZvmHeap`: slots with a
  generation, byte strings, typed arrays. That keeps semantics such as
  null-after-destroy and equality aligned. GC runs at the end of
  `room_dispatch` when the outermost call returns, with roots = event
  handler objects + globals + static fields. It is deterministic (index
  order).
- **Saves** use the existing linear-memory snapshot (`docs/12` §10), so
  nothing new is needed.

### 6.4 Natives on ABI v1

A generated table, `room-zvm-wasm/natives_abi.ron`, maps each ZScript native
to one of:
- an **ABI import** with marshalling, for example
  `Actor.GiveInventory → actor_give_inventory`,
  `Actor.A_StartSound → actor_start_sound`, `level.time → level_time`,
  `Wads.ReadLump → lump_find/lump_read`;
- a **zrt implementation** (String, Array, Map, math via `room_core::dmath`
  compiled into zrt, `Console.Printf → log`);
- **unavailable**.

```ron
( native: "Actor.GiveInventory", abi: "actor_give_inventory",
  args: [Self_, ClassFromName(0), I32(1), Const(0)], ret: Bool ),
( native: "String.Mid", zrt: "zrt_str_mid" ),
( native: "Actor.LineTrace", unavailable: "no `trace` import in ABI v1 yet (docs/12 §5.3)" ),
```

### 6.5 Diagnostics

`room-cli zscript wasm --target room-mod` fails if any reachable function
calls an `unavailable` native or uses UI scope with no ABI UI import. It
lists every blocker with its source location, and suggests the ABI addition
(new imports are appended under the ABI's append-only rule, `docs/12` §5).
The same report, run over the BD and UZDoom corpora, drives which imports
`docs/31`/ABI should add next.

---

### 6.6 Converting existing mods: `room-cli mod convert` (main use case)

The goal is to take a **legacy pk3/wad that contains ZScript** (Brutal Doom,
weapon mods, gameplay mods written for GZDoom/UZDoom) and produce a Room
package in which **all ZScript has been replaced by one WASM module**. Every
other lump (DECORATE, ACS, sprites, sounds, MAPINFO…) is kept. The converted
mod then runs without the ZScript compiler, inside the WASM sandbox, with
WASM saves and fingerprints.

§6.1–§6.5 alone are **not enough** for that, for three reasons:
1. **Native gap.** Old mods call hundreds of engine natives (`LineTrace`,
   `A_SpawnItemEx`, `ThinkerIterator`, `Sector` fields, `Screen.Draw*`,
   `CVar`, `StatusBar`…). ABI v1 exposes only a fraction of them, so the
   `unavailable` rule in §6.4 would reject almost every real mod.
2. **The mod is more than ZScript.** Other lumps name ZScript classes:
   - DECORATE classes inherit from ZScript classes and vice versa;
   - MAPINFO references them (`AddEventHandlers`, `StatusBarClass`,
     `PlayerClasses`);
   - ACS calls them through `ScriptCall`;
   - MENUDEF uses ZScript menu classes;
   - KEYCONF/CVARINFO and `replaces` chains do too.

   The converter must keep all of these working.
3. **There is no proof that the converted mod behaves like the original.**

The converter fixes the three points as follows.

#### 6.6.1 Full native access: ABI addition `zs_native` (closes the gap)

Add two append-only imports to ABI v1 (module `"room"`, play and ui scopes
following each native's scope):

```
zs_native_find(qname_ptr, qname_len) -> i32                 // "Actor.LineTrace" → native id (0 = unknown); registration or init time
zs_native_call(id, args_ptr, n_args, ret_ptr, ret_bank) -> i32   // 0 = ok, else trap code
```

- **Arguments and results** are the same tagged 16-byte `Value` cells as
  `Target::Aot` (§5.2), written in the mod's linear memory.
- **Marshalling.** Handles are tagged by their two high bits:

| Bits 63–62 | Meaning | Converted to / from |
|---|---|---|
| `0b10` | `ActorRef` | engine actor proxy |
| `0b11` | **host object** | opaque id of an engine object the mod cannot store in its own heap: sector, line, level, thinker iterator, `Font`, `CVar`, `StatusBar` state… The engine keeps a per-instance host-object table, saved with the mod; ids are stable in index order |
| `0b00` | guest heap object | never passed to the engine; a native that needs one is a compile error |

- **Implementation.** The engine runs the **same `room-zvm` native
  functions** (`natives.rs`) that the VM uses. The two paths therefore share
  one implementation, coverage report and determinism rules.
- **Result.** `RoomMod` can reach every native the ZVM implements, so the
  converter is limited only by ZVM native coverage (the `docs/36` UZ3 sprint).
- **Precedence.** In `natives_abi.ron`, an explicit ABI mapping (§6.4)
  still wins when one exists, because it is faster and typed.
  `unavailable` now means "not implemented by the ZVM either", which is the
  same answer the VM would give.

#### 6.6.2 Conversion pipeline

```
room-cli mod convert <in.pk3|wad|dir> -o <out.pk3> [--iwad doom2.wad] [--keep-zscript] [--verify N]
```

1. **Mount.** Mount the input with `room-vfs` (same order and namespace
   rules as loading it in-game).
2. **Collect ZScript.** Use the same lumps and order as
   `room-game/src/zscript_load.rs`: the root `ZSCRIPT` lump plus
   `#include`s, every `.zs/.zsc/.zc`, `version` gates.
3. **Compile with the stdlib.** Compile with `room-zscript` against the
   Room ZScript stdlib (Room stubs + vendored UZDoom `zscript/`, `docs/36`
   §4). Only the **mod's** classes are emitted: stdlib classes stay in the
   engine and are reached as natives/host classes. Compile errors stop the
   conversion and print the source locations.
4. **Generate the module.** Run `room-zvm-wasm` with `Target::RoomMod`,
   producing `zscript.wasm`.
5. **Resolve cross references.** The converter builds a symbol map from
   class name to origin (`zscript` | `decorate` | `engine`) for the whole
   package, then:
   - **DECORATE → ZScript parent.** Registration order matters, so the WASM
     module registers in pipeline phase 7, **before** DECORATE (as ZScript
     does in `docs/34` §5.1). The converter checks that every DECORATE
     parent named in the package resolves.
   - **ZScript → DECORATE parent.** These are `reg_class_begin` with a
     parent registered *later*. They are reported, and the converter emits
     a **two-phase registration**: classes with DECORATE parents are
     registered in a `room_register_late()` export that the host calls
     after DECORATE. This is a new optional export, append-only.
   - **MAPINFO `AddEventHandlers`, `StatusBarClass`, `PlayerClasses`;
     MENUDEF class names; ACS `ScriptCall` targets.** These are by name,
     and the WASM registration uses the same names, so nothing is
     rewritten. The converter verifies each name is registered by the
     module or the engine and lists the misses.
   - **KEYCONF / CVARINFO.** They are kept as is: `CVar.FindCVar` calls go
     through `zs_native`.
6. **Write the output package:**
   - every input lump **except** the ZScript sources (kept under
     `room/zscript-src/` only with `--keep-zscript`, for debugging);
   - `zscript.wasm` and a generated `room.toml` (`[[module]] file =
     "zscript.wasm"`, scope `play`, plus a `ui` module when the mod has
     ui-scope code: status bars, menus, `RenderOverlay`; §6.6.3);
   - `room-convert.json` with the input SHA-256, Room version, stdlib
     commit, the list of natives used, warnings, and the verification
     result.
7. **Verify** (§6.6.4, default `--verify 2000`).

#### 6.6.3 UI-scope code in old mods

Status bars (`BaseStatusBar`), menus (`OptionMenu`/`ListMenu`/custom
items), `RenderOverlay`/`UiTick` event handlers and `Screen`/`Font`/`TexMan`
drawing are compiled into a **separate ui module** (`zscript_ui.wasm`),
because ABI scopes are per module (`docs/12` §3). Shared plain classes are
compiled into both. Ui-to-play communication works as in GZDoom: only
through `SendNetworkEvent` (`send_network_event` import), which the
compiler already enforces from ZScript scope rules. The ui natives
(`Screen.DrawTexture`, `DrawText`, `StatusBar.DrawImage`…) go through
`zs_native` with ui scope. Without ZS7 ui natives they fail the conversion
with a clear report.

#### 6.6.4 Proving the conversion: `--verify`

After writing the package, the converter runs **both versions headless**
with identical inputs:

- **A:** the original package, with ZScript on the VM interpreter;
- **B:** the converted package, with WASM via wasmi, which is deterministic
  and available everywhere.

Each run covers *N* tics on the first map of the IWAD (or `--map`). The
input script is scripted (`room-testkit`): walk, fire every weapon slot,
use, kill monsters. It also triggers a save and a load at tic *N*/2.

The two runs must match every tic on:
- `World::checksum`, with the ZScript runtime part replaced on both sides by
  a **semantic digest**, namely the actor-visible state (health, inventory,
  flags, positions, states, script fields through `reg_class_field`). The
  VM heap and the WASM linear memory differ in layout, so their raw bytes
  cannot be compared;
- the console and printf output;
- HUD draw lists for ui mods (`UiDrawList` digest).

The first diverging tic is reported with the actor and field that differ.
A conversion that does not verify is still written, but marked
`"verified": false`, and `room-game` warns when loading it.

#### 6.6.5 What still cannot be converted (explicit list)

| Construct | Why | Behaviour |
|---|---|---|
| Natives the ZVM does not implement | no implementation anywhere | conversion fails, listing the natives (same as the VM trapping) |
| `native` classes or fields declared by the mod itself | only engine code can declare natives | compile error, also an error in GZDoom for mods |
| Reflection that depends on engine internals (`GetClass()` on stdlib classes compared to mod classes) | class ids differ per instance | works by **name**: class refs are compared through `class_find`; documented difference |
| Code that relies on GZDoom's unspecified behaviour (iteration order of `ThinkerIterator` across statnums, float corner cases) | Room is deterministic, GZDoom is not | same result as Room's VM, which is what `--verify` checks |
| Very old mods using deprecated features removed in UZDoom 5 (e.g. `const` functions mutating `self`) | `version` gate | follows the `version` declared by the mod, like UZDoom |

#### 6.6.6 Legal note

The converter is a local tool. Converting a mod creates a derivative of
it, and redistributing the converted package needs the original mod's
licence or author permission (many mods are GPL-compatible; Brutal Doom's
terms must be checked). `room-convert.json` records the source package's
name and hash so the origin stays traceable. Room never uploads or shares
conversions automatically.

---

## 7. API and CLI

```rust
pub struct Options {
    pub target: Target,                 // Aot | RoomMod { template: Option<PathBuf> }
    pub opt: OptLevel,                  // O0 (debug: 1 wasm func per bytecode block for stepping) | O1
    pub emit_names: bool,               // name section with `Class.method` (default true)
    pub features: WasmFeatures,         // nontrapping_fptoint (required), multi_value (unused), simd (off)
}
pub struct Artifact {
    pub bytes: Vec<u8>,
    pub compiled: Vec<bool>,            // per CompiledFunction
    pub unsupported: Vec<(u32, Unsupported)>,
    pub stats: Stats,                   // functions, blocks, imports used, code size
}
pub fn compile(program: &CompiledProgram, opts: &Options) -> Result<Artifact, Vec<CodegenError>>;
```

```
room-cli zscript wasm <pk3|dir|file.zs> --target aot|mod [-o out.wasm] [--wat out.wat]
                     [--check]            # differential run of all fixtures against the interpreter
                     [--report]           # unsupported functions + unavailable natives (RoomMod)
```

---

## 8. Testing

1. **Validation.** Every emitted module passes `wasmparser::Validator` with
   exactly the declared features.
2. **Differential conformance.** For every function in the corpus, run the
   interpreter and the AOT module (wasmtime, and wasmi as a third
   implementation) on the same inputs and compare:
   - the return slots;
   - the trap kind, function and line;
   - `fuel_left`;
   - the heap digest (`ZvmHeap` checksum);
   - the queued `ZsActorOp`s.

   The corpus is:
   - `docs/34` §14.6 probe;
   - the `zscript_play` tests;
   - BD's real ZScript;
   - the UZDoom stdlib (`docs/36` UZ3) once it compiles;
   - all DECORATE expressions from the BD pk3 via `deco_zvm`.
3. **Generated programs.** `proptest` builds random *structured* programs
   (nested if/while/switch, arithmetic with edge values `i64::MIN`, `-1`,
   `0`, NaN, ±0.0, ±inf, calls, virtual calls, field and array ops), lowers
   them through `lower.rs`, and differentially runs both tiers.
4. **Determinism.** The same session is replayed on x86-64 + wasmtime,
   aarch64 + wasmtime, Chrome and Firefox (`room-web` headless). Inputs
   follow `docs/33` §3.13: 2,000 tics of BD MAP01 with ZScript at skill 6.
   `World::checksum` must be equal every tic, and also with **mixed tiers**
   (peer A interpreter, peer B AOT).
5. **RoomMod end to end.** Compile the BD ZScript subset that ABI v1
   supports, and a new sample `mods/zs-sample/` (an actor with
   `Tick`/fields/states, an event handler, a `ScriptCall` target, an
   inventory item with `ModifyDamage`) to `.wasm`. Load it as a normal mod
   in `room-game/tests/zscript_wasm_mod.rs`; behaviour must equal the same
   sources run on the VM.
6. **Performance gates** (`benches/aot.rs`, criterion, CI informational):
   - `trivial_tick_program` ×10k: AOT ≥ 5× the interpreter on wasmtime and
     ≥ 3× in Chrome;
   - BD MAP01 2,000 tics: no regression against the interpreter, even with
     the import overhead.

---

## 9. Milestones

| ID | Scope | Depends | Acceptance |
|---|---|---|---|
| **ZW0** | Interpreter prerequisites: `has_ext`; block-based fuel (`block_costs`); `MIN/-1` → `Trap::Overflow`; factor `native_call`; `lower.rs` guarantees Arg/Call same block (debug assert); no legacy `CallStatic`/`CallNative` from new code | `docs/34` ZS2b | existing ZVM tests green; fuel snapshot tests updated; overflow test |
| **ZW1** | `room-zvm-wasm` skeleton: decoder, blocks, CFG, dominators, Ramsey structuring, I/F ops, consts, jumps, `Return`, traps, fuel, depth; `wasmparser` validation; `--wat` | ZW0 | arithmetic/loop/switch subset of the probe passes differentially on wasmtime + wasmi |
| **ZW2** | Calls: Arg snapshots, `Call`, `CallVirt` (table), return-slot globals, `CallNat` via arg buffer, `call_interp` bridge for unsupported callees | ZW1 | full `docs/34` §14.6 probe differential green; proptest 10k cases clean |
| **ZW3** | `Target::Aot` desktop: `zvm` imports in `room-zvm` (heap ops, natives, traps), wasmtime instance, cache, `ExecTier::Aot`, `zvm_tier` cvar (`interp`/`aot`/`auto`), desync logs include the tier. **ZS9 closed as superseded** | ZW2 | BD MAP01 2,000-tic checksum identical across interp/aot and mixed peers; ≥ 5× on trivial tick |
| **ZW4** | `Target::Aot` web: `room-web` exports `zvm_rt_*`, JS instantiation with direct wasm→wasm imports, IndexedDB cache, fallback to interpreter on compile failure | ZW3 | Chrome + Firefox determinism run equal to desktop; ≥ 3× in Chrome |
| **ZW5** | Performance: inline field access for `Aot` through an engine-exported "heap view" (read-only object layout table in engine memory, so `ld_i/st_i` become loads/stores; writes still go through barriers when GC needs them), constant-string pre-interning, peephole on wasm (redundant `local.set/get`) | ZW3 | ≥ 2× fewer import calls on BD MAP01; no checksum change |
| **ZW6** | `room-zrt` runtime (heap, GC, strings, arrays, maps, payload decoding, exports) + `Target::RoomMod` linker (walrus) + registration generation (§6.2) | ZW2 | `mods/zs-sample` builds to `.wasm`, loads as ROOMMOD, behaviour equal to VM run |
| **ZW7** | `natives_abi.ron` coverage + `--report`; ABI additions it motivates (append-only, `docs/31` follow-up); BD subset as RoomMod; docs (`docs/12` §9 updated: "write ZScript, ship WASM") | ZW6 | report on BD + UZDoom stdlib checked in (`docs/qa/zscript-wasm-coverage.md`) |
| **ZW8** | `zs_native_find`/`zs_native_call` ABI imports + host-object table (saved) + handle tagging; `room_register_late` export; ui module split | ZW6, `docs/34` native coverage | BD ZScript compiles as RoomMod with **0 unavailable natives** |
| **ZW9** | `room-cli mod convert` (§6.6.2) + `--verify` (§6.6.4) + `room-convert.json` + load-time warning for unverified packages | ZW8 | `brutal22test6.pk3` converts; `--verify 2000` on MAP01 (Doom2 IWAD) green at skill 6, including save/load; converted pk3 runs in desktop and web builds |

Order: ZW0 → ZW1 → ZW2 → (ZW3 → ZW4 → ZW5) ∥ (ZW6 → ZW7 → ZW8 → ZW9). For the mod-conversion goal the critical path is ZW0 → ZW1 → ZW2 → ZW6 → ZW8 → ZW9; ZW3–ZW5 (speed) can wait. Add a **ZW**
table to `docs/21` and mark **ZS9** "superseded by ZW3".

---

## 10. Risks and decisions

| Risk | Mitigation |
|---|---|
| Two executors drift apart | interpreter = spec; exhaustive `Op` match; differential tests on every corpus; shared `native_call` and heap code |
| Import-call overhead (heap in engine) | ZW5 heap view; most hot code is arithmetic, control flow and natives, which are calls in both tiers anyway |
| Browser differences (stack depth, feature support) | depth counter ensures deterministic overflow; `nontrapping-fptoint` is baseline in all evergreen browsers; compile failure → interpreter |
| Bytecode format churn while ZS1b–ZS4b land | ZW starts after `docs/34` ZS2b; `has_ext` and the opcode table are shared |
| `RoomMod` cannot express everything ABI v1 lacks | explicit report (§6.5); fallback is to ship ZScript source and use the VM |
| Determinism of mixed tiers | block fuel + identical heap + identical natives; checksum tests with mixed peers |
| Security | AOT modules are generated by Room from verified bytecode and only import `zvm` functions; RoomMod outputs go through the normal `room-script` validation and budgets |

ADR `docs/decisions/0006-zscript-wasm-backend.md` records the reversal of
0005's "transpiler dropped" line: the transpiler returns as a VM backend
with interpreter-defined semantics, so it adds speed and distribution
without a second language implementation.

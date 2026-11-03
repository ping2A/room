# room-zvm

ZScript virtual machine for Room (`docs/34-zscript-vm.md` §4).

## Responsibility

Register-based bytecode interpreter, generational object heap, deterministic
mark-sweep GC, native function table, fuel/trap policy. Optional `zvm-jit`
(Cranelift) is feature-gated for desktop (ZS9); today Tier-1 is peephole +
same interpreter with conformance checks (`conformance_call`).

Play-scope execution is part of the deterministic sim; UI-scope uses the same
interpreter without lockstep (`room-ui::ZscriptStatusBar`).

## ZS5–ZS10 surfaces

| Milestone | Surface |
|-----------|---------|
| ZS5 | Integer DECORATE exprs compile through this VM (`room-sim::deco_zvm`) |
| ZS6 | Heap/fields feed `ZscriptRuntime` save blobs + world checksums |
| ZS7 | UI natives emit draw cmds (client-local) |
| ZS8 | `coverage_report` / `room zscript coverage` + BD stub map |
| ZS9 | `zvm-jit` feature + Tier0≡Tier1 conformance |
| ZS10 | Peephole pass, `benches/tick_override`, `budget` smoke |

## Benches

```bash
cargo bench -p room-zvm --bench tick_override
```

# room-build-engine

Build engine **world model** and **clean-room** engine primitives (clipmove,
hitscan, getzrange, …). No Duke knowledge — that lives in `room-duke`.

| | |
|--|--|
| Spec | [`docs/25-duke3d-full-support.md`](../../docs/25-duke3d-full-support.md) §5 |
| ADR | [`docs/decisions/0004-native-build-runtime.md`](../../docs/decisions/0004-native-build-runtime.md) |
| Primitive specs | `docs/build-engine-primitives/` (from D2) |
| Depends on | `room-core` only |

D2 bootstraps clean-room primitives + golden harness. D3 adds `BuildWorld`
load from `.MAP` (`load_map_bytes`), soft FP path (`SoftFrame` / flats+sky /
textured walls), and noclip [`FreeFly`]. Do not port BUILDLIC / EDuke32 /
Raze engine code here.

# Build engine primitives (clean-room specs)

Specs for `room-build-engine`. **Do not** translate Ken's Build sources
(BUILDLIC). Each file is a behavioural contract: inputs, outputs, numeric
rules, edge cases. Implement from these specs only; validate against
black-box reference outputs (Chocolate Duke / generated maps), not by reading
`ENGINE.C`.

Parent plan: [`docs/25-duke3d-full-support.md`](../25-duke3d-full-support.md) §5.
ADR: [`docs/decisions/0004-native-build-runtime.md`](../decisions/0004-native-build-runtime.md).

| Spec | Primitive |
|------|-----------|
| [00-numeric.md](00-numeric.md) | Coordinates, angles, `mulscale`/`divscale`, `krand` |
| [01-world.md](01-world.md) | `BuildWorld`, capacity, sprite sector/stat lists |
| [02-inside.md](02-inside.md) | `inside(x,y,sect)` |
| [03-updatesector.md](03-updatesector.md) | `updatesector` |
| [04-getzsofslope.md](04-getzsofslope.md) | `getzsofslope` |
| [05-clipmove.md](05-clipmove.md) | `clipmove` / `pushmove` (contract; impl after goldens) |
| [06-getzrange.md](06-getzrange.md) | `getzrange` (sector-disc bootstrap) |
| [07-hitscan.md](07-hitscan.md) | `hitscan` (portals, sprites, planes) |
| [08-neartag-cansee.md](08-neartag-cansee.md) | `neartag` / `cansee` |
| [09-drag-helpers.md](09-drag-helpers.md) | `dragpoint`, `rotatepoint`, `getangle`, … |
| [10-goldens.md](10-goldens.md) | Black-box golden harness |

**Process:** expand a stub into a full contract → implement + unit/fuzz →
golden vs black-box on ≥50 maps before calling D2 done.

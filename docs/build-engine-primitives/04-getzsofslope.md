# 04 — `getzsofslope(sectnum, x, y) -> (ceilz, florz)`

## Purpose

Ceiling and floor z at `(x, y)` inside sector `sectnum`, including `heinum`
slopes.

## Contract (bootstrap)

- **Flat** when `(stat & 2) == 0` or `heinum == 0`: return `ceilingz` /
  `floorz`.
- **Sloped:** hinge is the sector’s first wall (`wallptr` → `point2`):
  - `dx, dy` = second vertex − first
  - `i = nsqrtasm(dx² + dy²) << 5` (`i == 0` → flat)
  - `j = dmulscale3(dx, y−wy, −dy, x−wx)`
  - `z = basez + scale(heinum, j, i)` (toward-zero `/`, wrapping add)
- Same formula for ceiling and floor with their own `*stat` / `*heinum` /
  `*z`.
- Invalid `sectnum` → `(0, 0)`.

Bitexact with Chocolate `getzsofslope` when `nsqrtasm` tables match Ken’s
`initksqrt`.

## Acceptance

- [x] Flat sector unit test
- [x] Sloped floor: z unchanged on hinge; `Δz = scale(heinum, j, nsqrt<<5)`
- [ ] Chocolate / black-box goldens on Atomic maps
- [ ] Fuzz: no panic; monotonic along slope direction for synthetic wedges

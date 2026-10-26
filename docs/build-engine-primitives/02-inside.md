# 02 — `inside(x, y, sectnum) -> bool`

## Purpose

Point-in-sector test for sector `sectnum` at map `(x, y)`.

## Contract

- Walk every wall in `sector.wallptr .. wallptr+wallnum`; each edge is
  `(wall[i].x,y) → (wall[point2].x,y)`.
- **Build XOR / sign-bit** algorithm (Ken), not classical even-odd:
  1. For each edge, let `y1 = wy - y`, `y2 = point2.y - y`.
  2. If `(y1 ^ y2) < 0` (edge straddles the query Y):
     - `x1 = wx - x`, `x2 = point2.x - x`.
     - If `(x1 ^ x2) >= 0`: `acc ^= x1` (as `u32` bit pattern).
     - Else: `acc ^= (x1*y2 - x2*y1) ^ y2` (wrapping `i32` mul/sub).
  3. Inside iff `(acc >> 31) != 0`.
- Invalid `sectnum` → `false`.
- Points on vertices/edges follow Build (locked by Chocolate goldens).

## Inputs / outputs

- In: `x: i32`, `y: i32`, `sectnum: i16`.
- Out: `bool`.

## Acceptance

- Synthetic convex/concave sectors (Chocolate + bootstrap fixtures).
- Per-MAP Chocolate `inside` at start pose (all episode maps).
- Fuzz / proptest no-panic; do **not** compare against classical even-odd.

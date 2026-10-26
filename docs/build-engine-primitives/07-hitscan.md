# 07 — `hitscan`

## Surface

```text
hitscan(
  x, y, z,           // origin
  sectnum,           // start sector
  vx, vy, vz,        // ray direction (not necessarily unit)
  clipmask,          // what to hit (walls / sprites / …)
) -> HitScan
  // hitwall, hitsprite, hitsect, hitx, hity, hitz
  // hitwall/hitsprite == -1 if none
```

## Purpose

Cast a ray through the Build world: walls, sloped floors/ceilings, and
sprites (face / wall / floor). Used by weapons, `cansee` helpers, and AI.

## Contract (bootstrap)

1. Parametric ray: `P(t) = origin + t * dir` for `t >= 0` in XY; Z follows
   `z + t * vz` with the same `t` scaled in XY length **(exact param linking
   verify vs Chocolate)**.
2. **Walls:** for each wall in the current sector (then BFS into
   `nextsector` when the ray crosses a portal), intersect the infinite line
   with the wall segment in XY; keep the smallest `t > ε`.
   - Solid wall (`nextsector < 0`) or portal whose opening rejects the ray’s
     Z at the crossing → record as wall hit.
   - One-sided / hitscan-skip `cstat` bits: honour when goldens define them
     (cstat 256 is “hitscan sensitive” on sprites — wall bits TBD).
3. **Floors/ceilings:** intersect ray with the sector plane from
   `getzsofslope` (plane through first wall + heinum). If the hit XY lies
   `inside` the sector and `t` is best, record sector hit (no wall index).
4. **Sprites:** face sprites as vertical billboards; wall/floor sprites as
   oriented quads — bootstrap may omit sprites until AABB helpers land.
5. **clipmask:** opaque `u32` until call-site goldens exist.

## Bootstrap scope (implemented)

- XY ray vs solid walls with **portal BFS** (`nextsector`; Z-gap ignored).
- Face sprites as XY cylinders (`clipdist`, require `cstat & 256`).
- Floor/ceiling via `hitscan_full` + `SectorZ` (flat exact; sloped one refine).

## Acceptance

- [x] Spec + solid-wall bootstrap + unit tests
- [x] Portal BFS unit test
- [x] Face-sprite cylinder hits
- [x] Flat floor/ceiling plane hits
- [ ] Wall/floor sprites + Z-gap on portals
- [ ] Chocolate goldens

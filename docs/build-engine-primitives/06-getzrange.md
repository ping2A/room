# 06 — `getzrange`

## Surface

```text
getzrange(
  x, y, z, sectnum,
  walldist,          // disc half-extent in XY
  clipmask,          // which sprites participate (opaque until goldens)
) -> (ceilz, ceilhit, florz, florhit)
```

## Purpose

Under a horizontal disc centred at `(x,y)` with radius/`walldist`, find the
**lowest ceiling** and **highest floor** the actor of height around `z` would
meet — including sloped sector planes and blocking sprite tops/bottoms.

Used by `clipmove` / player gravity / CON `getzrange`.

## Contract (bootstrap)

1. Start from `sectnum` (must be valid or results are `(0,0,0,0)`).
2. **Sector planes:** sample `getzsofslope` at the disc centre and at the four
   cardinal offsets `(±walldist,0)`, `(0,±walldist)` (clamped into the sector
   via `inside` / skip samples that leave the sector in the bootstrap).
   - `ceilz = min(samples.ceil)`
   - `florz = max(samples.flor)`
3. **Neighbour portals:** for each wall of the start sector with
   `nextsector >= 0`, if the portal opening overlaps the disc, also sample
   that neighbour’s plane at the closest point on the portal **(full rule in
   goldens; bootstrap may only sample the current sector)**.
4. **Sprites:** for each blocking sprite overlapping the disc (mask via
   `clipmask` / `cstat`), push `ceilz` down by sprite bottom or `florz` up by
   sprite top as appropriate **(bootstrap: skipped until sprite AABB helper
   exists)**.
5. **Hit codes (draft — verify):**
   - Sector floor/ceiling: `florhit` / `ceilhit` = `sectnum` (low bits) with a
     type tag in high bits (document exact Chocolate bits in goldens).
   - Sprite: high-bit tag + sprite index.
   - Bootstrap returns `ceilhit = sectnum`, `florhit = sectnum` when only
     sector planes contribute.

## Acceptance

- [x] Spec + sector-only bootstrap + unit test (flat room)
- [x] Portal neighbour sampling (Chocolate BFS / `MAXCLIPDIST`)
- [x] Face-sprite top/bottom (`cstat&48==0`; wall/floor sprites later)
- [ ] Chocolate goldens (full map corpus)
- [ ] Wall-aligned / floor sprites in `getzrange`

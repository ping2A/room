# 08 — `neartag` / `cansee`

## `neartag`

```text
neartag(x, y, z, sectnum, ang, &neartagsector, &neartagwall, &neartagsprite,
        &neartaghitdist, neartagrange, tagsearch)
```

Find the nearest **tagged** wall, sector, or sprite in front of `(x,y,z)`
within `neartagrange` along look angle `ang` (0..2047). Drives every “use”
action (`open`, switches, …).

### Bootstrap contract

1. Cast a short hitscan-like probe in direction `(cos(ang), sin(ang))` for
   length `neartagrange` (bootstrap: reuse solid-wall `hitscan` in the start
   sector only).
2. If a wall is hit and that wall’s `lotag != 0` (or sector behind has lotag —
   verify), return it with distance.
3. Sprites with `lotag != 0` in a cone: deferred.
4. `tagsearch` selects wall/sector/sprite classes — opaque until goldens.

Bootstrap returns wall hits only; sector/sprite outputs stay `-1`.

## `cansee`

```text
cansee(x1,y1,z1, sect1, x2,y2,z2, sect2) -> bool
```

True if a line segment between the two points does not hit a solid wall
(portal walk allowed when openings fit both Z values).

### Contract

- Chocolate portal BFS: cross `nextsector` walls when interpolated Z sits
  strictly between both sectors' `getzsofslope` ceiling/floor; `cstat&32` or
  solid walls block.
- Same XY endpoints → `sect1 == sect2`.

## Acceptance

- [x] Spec + wall `neartag` / same-sector `cansee` bootstrap + unit tests
- [x] Portal walk + Z gates vs Chocolate `cansee` (DEMO1 tic38 SHOTGUNSPRITE wake)
- [ ] Sprites / lotag extras locked vs Chocolate

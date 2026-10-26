# 03 — `updatesector(x, y, &mut sectnum)`

## Purpose

Given a hint sector, find which sector contains `(x, y)`.

## Draft search order (behaviour — verify)

1. If `*sectnum` is valid and `inside(x,y,*sectnum)`, keep it.
2. Else search **neighbouring** sectors (via walls' `nextsector`) first.
3. Else scan **all** sectors `0..numsectors`.
4. If none contain the point, set `*sectnum = -1`.

Neighbour visit order must be fixed and match goldens (wall index order).

## Acceptance

- Maps with overlapping sector projections (rooms over rooms): still 2D
  `inside` only — overlapping is resolved by caller context later.
- Goldens: start hint in/out/adjacent/far.

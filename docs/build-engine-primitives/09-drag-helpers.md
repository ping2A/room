# 09 — Helpers

| Helper | Status | Notes |
|--------|--------|-------|
| `sectorofwall(w)` | bootstrap | First sector whose `[wallptr, wallptr+wallnum)` contains `w` |
| `lastwall(w)` | bootstrap | Wall with `point2 == w` |
| `rotatepoint` | bootstrap | Q14 linear sin/cos until `TABLES.DAT` |
| `length2d` / `ksqrt` | bootstrap | |
| `getangle` | stub | Needs sintable binary search |
| `dragpoint` | bootstrap | Move all wall starts matching the old `(x,y)` of wall `w` |
| `nextsectorneighborz` | stub | |
| `alignflorslope` / `alignceilslope` | stub | |
| `animatewalls` / `picanm` | stub | `totalclock` ~120 Hz **(verify)** |

## `rotatepoint` bootstrap

`ang` in `0..2047`. Offset from pivot rotated with approximate sin/cos in
Q14 (`16384` full scale), right-shift 14. **Replace** with GRP `sintable`
before DEMO sync.

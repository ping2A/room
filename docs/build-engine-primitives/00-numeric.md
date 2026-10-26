# 00 — Numeric model

## Coordinates

- World **x/y**: `i32`, Build map units.
- World **z**: `i32`, Build z units. Display / XY-comparable height is
  typically `z >> 8` (Build's historical 4:1 z scale). Exact compare rules for
  clip vs floor/ceiling are specified per primitive.
- All sim math is **integer**. No `f32`/`f64` in `room-build-engine` playsim
  paths.

## Angles

- Angle space is **2048 units** per turn (`ang & 2047`).
- Trig comes from GRP `TABLES.DAT` `sintable[2048]` (loaded at runtime, never
  embedded as a translated table from engine source). Cosine is
  `sintable[(ang + 512) & 2047]` (verify against black-box).

## Scale helpers

Implement wrapping 32-bit semantics matching historical Build pragmas
(document each):

| Op | Contract (draft) |
|----|------------------|
| `mulscale(a, b, s)` | `(i64(a) * i64(b)) >> s` truncated to `i32` with wrap |
| `divscale(a, b, s)` | `(i64(a) << s) / i64(b)` truncated; define `/0` via golden |
| `dmulscale(a,b,c,d,s)` | `((a*b)+(c*d)) >> s` as `i64` then trunc |
| `scale(a, b, c)` | `(a * b) / c` with documented `/0` |
| `klabs(x)` | absolute value; `i32::MIN` → documented (often stays `MIN`) |
| `ksqrt(n)` | integer square root of non-negative; define negative input |
| `getangle(x, y)` | atan2-like into 0..2047 using sintable / binary search |

Use `wrapping_*` / explicit casts. Never panic on overflow.

## Random: `krand`

- State: `randomseed: i32` on the world.
- Step (draft, verify vs DEMO traces):  
  `randomseed = randomseed.wrapping_mul(2_758_4621).wrapping_add(1)`  
  return `((randomseed as u32) >> 16) as i32` (high 16 bits as signed-ish
  value used by game code — confirm with black-box goldens).
- `TRAND` (Watcom `rand`) belongs to Duke game code (`room-duke`), not this
  crate — but engine code must not call libc `rand`.

## Acceptance

- Unit tests for each helper vs fixed vectors from black-box runs.
- Fuzz: no panic; `/0` and `i32::MIN` paths covered.

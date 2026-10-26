# 05 — `clipmove` / `pushmove`

Hardest primitive. This contract is **behavioural**; implement only after
black-box goldens exist for the cases below. Do not translate BUILDLIC sources.

## Surface

```text
clipmove(
  &mut x, &mut y, &mut z,   // world position (Build units)
  &mut sectnum,             // hint sector; updated via updatesector
  xvect, yvect,             // desired XY delta this call (often vel<<something)
  walldist,                 // keep this many units from solid walls
  ceildist, flordist,       // keep this many units from ceiling / floor z
  cliptype,                 // bit mask selecting what to collide with
) -> hit
  hit == 0                 → moved freely (or zero vect)
  hit  16384|wall          → hit wall index `wall`
  hit  49152|sprite        → hit sprite index `sprite`
  (exact bit layout: verify vs Chocolate; document in goldens)

pushmove(
  &mut x, &mut y, &mut z, &mut sectnum,
  walldist, ceildist, flordist, cliptype,
) -> ok: bool
  // Nudge out of penetration; false if still stuck after attempts
```

`z` is updated only for floor/ceiling constraints (standing on slopes via
`getzrange` / `getzsofslope`), not by `xvect`/`yvect`.

## Pipeline (draft — lock order via goldens)

1. If `xvect == 0 && yvect == 0`: optionally still clamp Z; return 0.
2. **Clip against walls** in the current sector and reachable neighbours
   (portal walls with `nextsector >= 0` are passable if height gap allows —
   gap test uses `getzsofslope` / `getzrange` at the crossing).
3. **Clip against sprites** whose `cstat` and `cliptype` say they block;
   shape depends on sprite type (face / wall / floor — see below).
4. On a hit: **slide** along the blocking surface with the residual velocity;
   historically up to **3** clip/slide passes **(verify)**.
5. After XY settles: `updatesector`; clamp / rest on floor-ceiling via
   `getzrange` (or equivalent) using `flordist` / `ceildist`.
6. Return the last hit code (0 if the full vector applied).

## Wall clipping

- Treat the mover as an **axis-aligned square** of half-extent `walldist`
  (diamond vs square: verify — many Build callers pass `walldist` as both
  clip radius and “keep-off” distance).
- Solid wall: `nextsector < 0`, or portal whose opening is too short for
  `(z - flordist) .. (z + ceildist)` at the crossing.
- One-sided / masking walls: honour `cstat` bits that disable blocking
  **(verify bit numbers against Atomic goldens)**.
- Red walls (portals): may push into the neighbour sector; then
  `updatesector` must accept the new side.

## Sprite clipping

| Sprite | Shape (draft) |
|--------|----------------|
| Face (`cstat` wall/floor bits clear) | Vertical cylinder / box of radius from `clipdist` (or tile size × repeat — verify) |
| Wall-aligned | Thin quad along sprite angle |
| Floor-aligned | Disc / box on the floor plane |

Skip sprites that fail `cliptype` mask or have blocking bit clear.

## `cliptype` (draft — verify)

Callers pass composites such as “clip walls + sprites” vs “walls only”.
Until goldens exist, treat `cliptype` as an opaque `u32` and branch only on
values observed in Atomic DEMO / `GAME.CON` call sites when recording
reference vectors.

## `pushmove`

- Chocolate wall/portal push: `clipinsidebox(..., walldist-4)==1`, then solid
  if `nextsector < 0`, `cstat & dawalclipmask`, or floor/ceil step vs `z`.
- Escape along wall normal (`getangle` + `sintable>>11`), up to 16 steps;
  outer iteration cap 32; return `-1` if still overlapping / pushed.
- Sprites not in this path (Atomic `pushmove` is walls-only).

## Dependencies

Requires working: `inside`, `updatesector`, `getzsofslope`, `getzrange`,
`ksqrt`, sprite lists.

## Bootstrap (`clipmove_xy`)

- Clear segment → apply full `(xvect,yvect)` + `updatesector`.
- Solid wall hit within the move → stop `walldist + 1` short (Chocolate
  `keepaway`), project residual onto wall tangent, repeat up to **3** passes
  (slide). Hit code `0x8000 | wall`.
- No sprites / portals / Z / `cliptype` yet.

## Acceptance

- [x] Hard-stop + slide bootstrap + unit tests (open / blocked / slide)
- [x] Contract reviewed against Chocolate black-box vectors (open room,
      single wall hard-stop + slide ±1; portal/sprite deferred)
- [x] Unit tests for each golden (`choc_clip_*`; slide may set `pos_atol: 1`)
- [x] Fuzz: no panic; position stays in some sector or `-1` only when outside
      the map hull (`primitives_proptest` / `build_primitives`)
- [ ] Full slide + sprite + portal implementation

## Non-goals (this file)

- Exact pixel-identical slide vs 1996 DOS in pathological corners — DEMO sync
  is the bar, not screenshots.

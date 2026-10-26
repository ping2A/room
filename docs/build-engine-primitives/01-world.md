# 01 — World model and sprite lists

## Capacities (Atomic / verify)

| Symbol | Value |
|--------|------:|
| `MAXSECTORS` | 1024 |
| `MAXWALLS` | 8192 |
| `MAXSPRITES` | 4096 |
| `MAXSTATUS` | 1024 |
| `MAXTILES` | 6144 |

## Structures

`SectorT` / `WallT` / `SpriteT` mirror Build map v7 fields (already parsed in
`room-build`). Runtime copies live in `BuildWorld`.

```text
BuildWorld
  sectors: Vec<SectorT>     // len == numsectors ≤ MAXSECTORS
  walls:   Vec<WallT>
  sprites: [SpriteT; MAXSPRITES]
  headspritesect[MAXSECTORS+1], headspritestat[MAXSTATUS+1]
  prev/next spritesect/spritestat [MAXSPRITES]
  randomseed: i32
  // visibility, parallax, totalclock, …
```

## List invariants

- Sprites are linked in **two** intrusive lists: by sector and by status.
- Free sprites live on a free list (status or sector sentinel — document exact
  vanilla free-list convention when goldens exist).
- `insertsprite(sect, stat)` → first free index; **head-insert** into both
  lists; return index or `-1` if full.
- `deletesprite(i)` → unlink both lists; return to free list.
- `changespritesect` / `changespritestat` → unlink + head-insert into new
  list; no-op if unchanged.
- **Iteration order is behaviour.** `nextspritestat[head]` walk must match
  vanilla for demo sync.

## Acceptance

- Property tests: random insert/delete/change preserves list integrity
  (no cycles, every live sprite reachable from exactly one sector head and
  one status head).
- Golden: sequence of ops vs black-box reference indices.

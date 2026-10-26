# Duke per-tic trace format (D1)

> Used to prove demo sync (docs/25 §10.2 / T1.1). The **reference** emitter is
> an out-of-tree patched Chocolate Duke3D (or DOSBox + `DUKE3D.EXE`). Room's
> native runtime (`room-duke`) must emit the same bytes. Diff with
> `room duke trace-diff`.

## File layout

- Encoding: UTF-8, LF newlines.
- First non-empty, non-comment line: `DUKE_TRACE 1`
- Optional metadata lines: `meta key=value` (repeatable). Common keys:
  - `demo` — source demo file stem (`DEMO1`)
  - `players` — seat count (1..=8)
  - `engine` — `chocolate` | `room` | other
  - `grp_sha256` — lowercase hex of the base GRP (optional)
- Then one **JSON object per tic**, one object per line (NDJSON).
- Lines starting with `#` are comments (ignored).
- Empty lines ignored.

## Tic object

| Field | Type | Meaning |
|-------|------|---------|
| `tic` | u32 | Game tic index from 0 (demo start) |
| `randomseed` | i32 | Build `randomseed` after the tic (`krand` LCG state) |
| `global_random` | i32 | Duke `global_random` / `TRAND` seed state after the tic |
| `numsprites` | u32 | Count of live sprites (`statnum != MAXSTATUS`) |
| `players` | array | Length = `meta players` (default 1). See below |
| `sprite_hash` | string | 64 lowercase hex chars — SHA-256 over live sprites |

### `players[i]`

| Field | Type | Meaning |
|-------|------|---------|
| `posx` | i32 | Build units |
| `posy` | i32 | Build units |
| `posz` | i32 | Build z (×16 vs xy — same as on-disk / `ps[].posz`) |
| `ang` | i16 | 0..2047 |
| `horiz` | i16 | Look pitch (vanilla `horiz`) |
| `extra` | i16 | Health (`ps[].extra`) |
| `curr_weapon` | i16 | Current weapon index |
| `ammo` | i16[] | `ps[].ammo_amount[0..]` — length is engine `MAX_WEAPONS` (Atomic: **12**) |

### `sprite_hash` input

For every sprite index `0 .. MAXSPRITES-1` in ascending order where the sprite
is live (`statnum != MAXSTATUS`):

Concatenate little-endian fields (**16 bytes** per sprite):

`i32 x`, `i32 y`, `i32 z`, `i16 picnum`, `i16 statnum`

Then SHA-256. Empty world → hash of empty input
(`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`).

`MAXSPRITES` / `MAXSTATUS` follow Atomic 1.5 (**4096** / **1024**) unless a
`meta maxsprites=` / `meta maxstatus=` override is present **(verify before
locking D5 emitters)**.

## Example

```text
DUKE_TRACE 1
meta demo=DEMO1 players=1 engine=fixture
# synthetic
{"tic":0,"randomseed":1,"global_random":2,"numsprites":0,"players":[{"posx":0,"posy":0,"posz":0,"ang":0,"horiz":100,"extra":100,"curr_weapon":1,"ammo":[48,0,0,0,0,0,0,0,0,0,0,0]}],"sprite_hash":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}
{"tic":1,"randomseed":3,"global_random":4,"numsprites":0,"players":[{"posx":16,"posy":0,"posz":0,"ang":0,"horiz":100,"extra":100,"curr_weapon":1,"ammo":[48,0,0,0,0,0,0,0,0,0,0,0]}],"sprite_hash":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}
```

## On-disk corpus

| Path | Git | Role |
|------|-----|------|
| `testdata/duke/traces/DEMO1.trace` … `DEMO3.trace` | ignored (`testdata/duke/`) | Reference NDJSON |
| [`testdata/duke-trace-manifest.toml`](../testdata/duke-trace-manifest.toml) | committed | Expected SHA-256 of each `.trace` |

Generate traces out of tree (see [`duke-trace-howto.md`](duke-trace-howto.md)),
copy into `testdata/duke/traces/`, then:

```bash
room duke trace-hash testdata/duke/traces/DEMO1.trace
# update manifest sha256 fields
room duke trace-check-manifest
room duke trace-diff ref.trace room.trace
```

Committed fixtures for the diff tool live under
`crates/room-duke/testdata/` (not Atomic demos).

## Diff semantics

`room duke trace-diff A B` walks tics in order and prints the **first** mismatch
as a JSON object: `{ "tic", "field", "a", "b" }` (field path like
`players[0].posx`). Exit status 0 = identical (ignoring comments/meta order);
1 = diverge; 2 = I/O or parse error.

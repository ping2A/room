# 10 — Black-box golden harness

D2 acceptance needs goldens from a **reference Build engine** (Chocolate Duke
under `linux/386`) without reading BUILDLIC sources.

## Fixture layout

```text
crates/room-build-engine/testdata/primitives/
  manifest.toml          # list of cases
  cases/
    inside_square.json
    getz_flat.json
    …
```

Each case file:

```json
{
  "name": "inside_square_center",
  "op": "inside",
  "walls": [{"x":0,"y":0,"point2":1,"nextsector":-1}, …],
  "sectors": [{"wallptr":0,"wallnum":4}],
  "input": {"x":50,"y":50,"sect":0},
  "expect": {"inside": true}
}
```

Supported `op` values (bootstrap): `inside`, `updatesector`, `getzsofslope`,
`getzrange`, `hitscan`, `clipmove_xy`, `cansee`, `dragpoint`.

Optional `pos_atol` (clipmove only): max `|Δ|` on `x`/`y` vs expect (slide
cases may use `1` while hard-stop / open stay exact). Hit codes are unsigned
`0x8000 | wall` (Chocolate `clipobjectval` as u16).

## Recording from Chocolate (out of tree)

1. Build Chocolate + Room hook on `linux/386` (see
   `third_party/chocolate-duke3d-trace/README.md`).
2. Add a **separate** probe binary/mod that loads a tiny `.MAP`, calls the
   engine primitive, prints one JSON object per line (do not commit Chocolate
   sources).
3. Copy JSON into `testdata/primitives/cases/` and list it in `manifest.toml`.
4. `cargo test -p room-build-engine --test primitives_goldens` must pass.

Until Chocolate probes exist, hand-authored fixtures lock the bootstrap
contracts so CI stays green; replace expects when reference vectors land.

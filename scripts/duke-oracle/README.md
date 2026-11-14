# Duke3D sync oracle

A patched [Chocolate Duke3D](https://github.com/fabiensanglard/chocolate_duke3D)
(commit `1cfa190`) that plays an Atomic `.DMO` headless and records, per tic,
the same NDJSON trace as `testdata/duke/traces/*.trace`, plus a log of **every
`krand()` call** (tic, caller address, seed before the call, current CON actor
`g_i`, its picnum and `t0/t2/t4`) and an optional full sprite dump (position,
sector, velocities, owner, repeats, cstat, temps, `hittype` floor/ceiling).

The fresh build reproduces the committed DEMO1 and DEMO2 traces byte for byte.

```sh
scripts/duke-oracle/build.sh                 # needs gcc -m32 and 32-bit SDL 1.2 + SDL_mixer
scripts/duke-oracle/run.sh DEMO1 155 60      # trace + krand log + sprite dump at tic 155
cargo run --release -p room-duke --example duke_krand -- DEMO1 160 > /tmp/room.k
CHOC=scripts/duke-oracle/build/chocolate-duke3d \
  python3 scripts/duke-oracle/krand_cmp.py scripts/duke-oracle/build/run/DEMO1/k.log /tmp/room.k
```

`krand_cmp.py` prints the first tic whose sequence of seeds differs, with the
Chocolate function (via `addr2line`) next to the Room call site for each call.
The oracle's krand tic labels start at -1 (level load and the first tic share
that label); the script shifts them to Room's numbering.

`run.sh` copies the demo over `demo1.dmo` so any of DEMO1–3 plays first. It
runs in real time behind ~25 s of intro animation; pass a longer timeout to
reach later tics (about 30 tics per second).

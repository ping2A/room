#!/bin/bash
# Play one Atomic demo headless in the oracle and write, under build/run/DEMOn/:
#   t.trace  NDJSON sync trace (same format as testdata/duke/traces)
#   k.log    one line per krand(): tic, return address, seed before, g_i, CON temps
#   sp.txt   full sprite dump at trace tic $2 (fields match zz/room dumps)
# usage: run.sh DEMO1 <dump-tic> [seconds]   (plays in real time; ~25 s of intro first)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
d=$1; t=${2:-0}; dir=$here/build/run/$d; mkdir -p "$dir"; cd "$dir"
ln -sf "$root/testdata/duke/duke3d.grp" duke3d.grp
cp "$root/testdata/duke/demos/$d.DMO" demo1.dmo
rm -f t.trace k.log sp.txt
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy ROOM_NOSOUND=1 ROOM_TRACE=$dir/t.trace \
  ROOM_KRAND_LOG=$dir/k.log ROOM_SPRITE_DUMP=$dir/sp.txt ROOM_SPRITE_DUMP_TIC=$t \
  timeout "${3:-60}" "$here/build/chocolate-duke3d" > run.log 2>&1 || true
echo "$(grep -c '^{' t.trace) tics traced in $dir"

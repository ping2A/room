#!/usr/bin/env bash
# Local DEMO1 timedemo vs Chocolate Doom 1.9 (requires `brew install chocolate-doom`).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IWAD="${ROOM_IWAD:-$ROOT/testdata/freedoom/freedoom2.wad}"
if [[ ! -f "$IWAD" ]]; then
  echo "missing IWAD: $IWAD (run scripts/fetch-freedoom.sh)" >&2
  exit 1
fi
if ! command -v chocolate-doom >/dev/null; then
  echo "chocolate-doom not on PATH (brew install chocolate-doom)" >&2
  exit 1
fi
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
# Extract DEMO1 lump to a short name Chocolate accepts as -timedemo.
python3 - "$IWAD" "$TMP/demo1.lmp" <<'PY'
import struct, sys
from pathlib import Path
wad, out = Path(sys.argv[1]), Path(sys.argv[2])
data = wad.read_bytes()
_, n, off = struct.unpack_from("<4sII", data, 0)
for i in range(n):
    o, s, name = struct.unpack_from("<II8s", data, off + i * 16)
    nm = name.split(b"\0")[0].decode("ascii", "replace").upper()
    if nm == "DEMO1":
        out.write_bytes(data[o : o + s])
        print(f"extracted DEMO1 ({s} bytes) → {out}")
        break
else:
    raise SystemExit("DEMO1 lump not found")
PY
export SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy
cd "$TMP"
# Expect: "timed N gametics" with N matching DEMO1 tic count (~2664 for Freedoom2).
chocolate-doom -iwad "$IWAD" -nosound -nomusic -nodraw -nogui -nomouse -nograbmouse \
  -timedemo demo1

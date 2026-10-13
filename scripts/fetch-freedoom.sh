#!/usr/bin/env bash
# Download pinned Freedoom WADs into testdata/freedoom/ (not committed).
# Portable across Linux/macOS/Windows (Git Bash): prefers `unzip`, else Python.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/testdata/freedoom"
# Freedoom 0.13.0 — BSD-3-Clause
VER="0.13.0"
URL="https://github.com/freedoom/freedoom/releases/download/v${VER}/freedoom-${VER}.zip"
mkdir -p "$OUT"
ZIP="$OUT/freedoom-${VER}.zip"
need=0
[[ -f "$OUT/freedoom1.wad" ]] || need=1
[[ -f "$OUT/freedoom2.wad" ]] || need=1
[[ -f "$OUT/freedm.wad" ]] || need=1
if [[ "$need" -eq 1 ]]; then
  echo "fetching $URL"
  curl -fsSL -o "$ZIP" "$URL"
  if command -v unzip >/dev/null 2>&1; then
    unzip -o -j "$ZIP" "freedoom-${VER}/freedoom1.wad" "freedoom-${VER}/freedoom2.wad" "freedoom-${VER}/freedm.wad" -d "$OUT" || \
      unzip -o -j "$ZIP" "*.wad" -d "$OUT"
  else
    python3 - "$ZIP" "$OUT" "$VER" <<'PY'
import sys, zipfile
from pathlib import Path
zip_path, out, ver = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
want = {f"freedoom-{ver}/freedoom1.wad", f"freedoom-{ver}/freedoom2.wad", f"freedoom-{ver}/freedm.wad"}
with zipfile.ZipFile(zip_path) as z:
    for name in z.namelist():
        base = name.replace("\\", "/").split("/")[-1].lower()
        if name.replace("\\", "/") in want or base in {"freedoom1.wad", "freedoom2.wad", "freedm.wad"}:
            target = out / base
            target.write_bytes(z.read(name))
PY
  fi
  rm -f "$ZIP"
fi
ls -la "$OUT"/*.wad

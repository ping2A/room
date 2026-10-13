#!/usr/bin/env bash
# Pack wadsrc/ferrite/ into target/ferrite.pk3 (stored ZIP).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/wadsrc/ferrite"
OUT="$ROOT/target/ferrite.pk3"
mkdir -p "$ROOT/target"
rm -f "$OUT"
# Prefer zip; fall back to Python zipfile.
if command -v zip >/dev/null 2>&1; then
  (cd "$SRC" && zip -rq "$OUT" . -x 'README.md' -x '*/.DS_Store')
else
  python3 - <<PY
import pathlib, zipfile
src = pathlib.Path("$SRC")
out = pathlib.Path("$OUT")
with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for path in sorted(src.rglob("*")):
        if path.is_file() and path.name != "README.md":
            zf.write(path, path.relative_to(src).as_posix())
print(f"wrote {out}")
PY
fi
echo "packed $OUT ($(wc -c < "$OUT") bytes)"

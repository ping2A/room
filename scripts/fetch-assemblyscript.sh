#!/usr/bin/env bash
# Vendor the AssemblyScript compiler for the web editor's live mods
# (Scripts tab → "+ New AssemblyScript mod"). Without it the editor loads the
# same pinned version from cdn.jsdelivr.net.
#
#   ./scripts/fetch-assemblyscript.sh            # → target/asc-vendor/*.js
#
# play-web.sh copies the result into <out>/vendor/asc/.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ASC_VERSION:-0.28.20}"
WORK="$ROOT/target/asc-npm"
OUT="$ROOT/target/asc-vendor"

command -v npm >/dev/null || { echo "need npm (Node.js) to vendor AssemblyScript" >&2; exit 1; }

mkdir -p "$WORK" "$OUT"
if [[ ! -f "$WORK/node_modules/assemblyscript/package.json" ]] ||
  ! grep -q "\"version\": \"$VERSION\"" "$WORK/node_modules/assemblyscript/package.json"; then
  echo "room: npm install assemblyscript@${VERSION}…"
  [[ -f "$WORK/package.json" ]] || (cd "$WORK" && npm init -y >/dev/null)
  (cd "$WORK" && npm install --no-audit --no-fund "assemblyscript@$VERSION" >/dev/null)
fi

NM="$WORK/node_modules"
# Browser ES modules: resolve the bundles' bare imports to sibling files.
sed -e 's/from"assemblyscript"/from".\/assemblyscript.js"/g' \
  -e 's/from"binaryen"/from".\/binaryen.js"/g' \
  "$NM/assemblyscript/dist/asc.js" >"$OUT/asc.js"
sed -e 's/from"binaryen"/from".\/binaryen.js"/g' \
  -e 's/from"long"/from".\/long.js"/g' \
  "$NM/assemblyscript/dist/assemblyscript.js" >"$OUT/assemblyscript.js"
cp "$NM/binaryen/index.js" "$OUT/binaryen.js"
cp "$NM/long/index.js" "$OUT/long.js"
for lic in assemblyscript binaryen long; do
  cp "$NM/$lic/LICENSE" "$OUT/LICENSE.$lic" 2>/dev/null || true
done
echo "$VERSION" >"$OUT/VERSION"
echo "room: AssemblyScript $VERSION vendored in $OUT ($(du -sh "$OUT" | cut -f1))"

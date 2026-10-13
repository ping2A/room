#!/usr/bin/env bash
# Launch the built-in combat arena (monsters + full arsenal) with Freedoom assets.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

IWAD="${FERRITE_IWAD:-testdata/freedoom/freedoom2.wad}"
if [[ ! -f "$IWAD" ]]; then
  echo "fetching Freedoom…"
  ./scripts/fetch-freedoom.sh
  IWAD="testdata/freedoom/freedoom2.wad"
fi

exec cargo run -p ferrite-app --release -- \
  --iwad "$IWAD" \
  --combat-demo \
  --skill "${FERRITE_SKILL:-3}" \
  "$@"

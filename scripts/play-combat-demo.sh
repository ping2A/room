#!/usr/bin/env bash
# Launch the built-in combat arena (monsters + full arsenal) with Freedoom assets.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

IWAD="${ROOM_IWAD:-testdata/freedoom/freedoom2.wad}"
if [[ ! -f "$IWAD" ]]; then
  echo "fetching Freedoom…"
  ./scripts/fetch-freedoom.sh
  IWAD="testdata/freedoom/freedoom2.wad"
fi

exec cargo run -p room-app --release -- \
  --iwad "$IWAD" \
  --combat-demo \
  --skill "${ROOM_SKILL:-3}" \
  "$@"

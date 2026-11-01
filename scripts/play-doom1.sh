#!/usr/bin/env bash
# Play Doom (shareware / Ultimate) from testdata/doom/doom1.wad — E1M1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

IWAD="${ROOM_IWAD:-testdata/doom/doom1.wad}"
if [[ ! -f "$IWAD" ]]; then
  echo "missing IWAD: $IWAD" >&2
  echo "Place doom1.wad (or doom.wad) at testdata/doom/doom1.wad" >&2
  exit 1
fi

# SoftFont music (much better than incomplete OPL); fetch once if missing.
if [[ ! -f soundfonts/TimGM6mb.sf2 && ! -f soundfonts/room.sf2 ]]; then
  ./scripts/fetch-soundfont.sh || echo "warn: no SoundFont — falling back to OPL" >&2
fi

MAP="${ROOM_MAP:-E1M1}"
SKILL="${ROOM_SKILL:-3}"
EXTRA=()
if [[ "${ROOM_TITLE:-0}" == "1" ]]; then
  EXTRA+=(--title)
fi

echo "room: Doom1 — $IWAD  map=$MAP  skill=$SKILL"
echo "  WASD move, mouse look, Ctrl/F/LMB fire, 1–7 weapons, E use, Esc pause"
echo "  F5 Graphics (shaders live)  ·  or Esc → Options → Graphics"
echo "  F12 screenshot, F11 start/stop GIF  (saved under room/screenshots)"

# Empty `"${EXTRA[@]}"` trips `set -u` on older bash (macOS).
exec cargo run -p room-app --release -- \
  --iwad "$IWAD" \
  --map "$MAP" \
  --play \
  --skill "$SKILL" \
  ${EXTRA[@]+"${EXTRA[@]}"} \
  "$@"

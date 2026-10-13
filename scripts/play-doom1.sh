#!/usr/bin/env bash
# Play Doom (shareware / Ultimate) from testdata/doom/doom1.wad — E1M1.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

IWAD="${FERRITE_IWAD:-testdata/doom/doom1.wad}"
if [[ ! -f "$IWAD" ]]; then
  echo "missing IWAD: $IWAD" >&2
  echo "Place doom1.wad (or doom.wad) at testdata/doom/doom1.wad" >&2
  exit 1
fi

# SoftFont music (much better than incomplete OPL); fetch once if missing.
if [[ ! -f soundfonts/TimGM6mb.sf2 && ! -f soundfonts/ferrite.sf2 ]]; then
  ./scripts/fetch-soundfont.sh || echo "warn: no SoundFont — falling back to OPL" >&2
fi

MAP="${FERRITE_MAP:-E1M1}"
SKILL="${FERRITE_SKILL:-3}"
EXTRA=()
if [[ "${FERRITE_TITLE:-0}" == "1" ]]; then
  EXTRA+=(--title)
fi

echo "ferrite: Doom1 — $IWAD  map=$MAP  skill=$SKILL"
echo "  WASD move, mouse look, Ctrl/F/LMB fire, 1–7 weapons, E use, Esc pause"
echo "  F12 screenshot, F11 start/stop GIF  (saved under ferrite/screenshots)"

# Empty `"${EXTRA[@]}"` trips `set -u` on older bash (macOS).
exec cargo run -p ferrite-app --release -- \
  --iwad "$IWAD" \
  --map "$MAP" \
  --play \
  --skill "$SKILL" \
  ${EXTRA[@]+"${EXTRA[@]}"} \
  "$@"

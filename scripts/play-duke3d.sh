#!/usr/bin/env bash
# Play Duke Nukem 3D from testdata/duke/duke3d.grp — E1L1 (Hollywood Holocaust).
# The GRP is converted to a Doom-format WAD in memory (`room-build`).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

IWAD="${ROOM_IWAD:-testdata/duke/duke3d.grp}"
if [[ ! -f "$IWAD" ]]; then
  echo "missing GRP: $IWAD" >&2
  echo "Place DUKE3D.GRP (shareware or full) at testdata/duke/duke3d.grp" >&2
  exit 1
fi

# SoundFont for the MIDI music; fetch once if missing.
if [[ ! -f soundfonts/TimGM6mb.sf2 && ! -f soundfonts/room.sf2 ]]; then
  ./scripts/fetch-soundfont.sh || echo "warn: no SoundFont — falling back to OPL" >&2
fi

# Episode E, level L is map ExMy (E1L1 = E1M1).
MAP="${ROOM_MAP:-E1M1}"
SKILL="${ROOM_SKILL:-3}"
EXTRA=()
if [[ "${ROOM_TITLE:-0}" == "1" ]]; then
  EXTRA+=(--title)
fi

echo "room: Duke Nukem 3D — $IWAD  map=$MAP  skill=$SKILL"
echo "  WASD move, mouse look, Ctrl/F/LMB fire, 1–9 weapons, E use, Esc pause"
echo "  F12 screenshot, F11 start/stop GIF  (saved under room/screenshots)"

# Empty `"${EXTRA[@]}"` trips `set -u` on older bash (macOS).
exec cargo run -p room-app --release -- \
  --iwad "$IWAD" \
  --map "$MAP" \
  --play \
  --skill "$SKILL" \
  ${EXTRA[@]+"${EXTRA[@]}"} \
  "$@"

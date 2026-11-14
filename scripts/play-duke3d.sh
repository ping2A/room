#!/usr/bin/env bash
# Play Duke Nukem 3D from testdata/duke/duke3d.grp — E1L1 (Hollywood Holocaust).
# `converter` uses GRP→UDMF; `native` runs room-duke directly from Build maps.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RUNTIME="${ROOM_DUKE_RUNTIME:-native}"
APP_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --runtime)
      if [[ $# -lt 2 ]]; then
        echo "room: --runtime requires converter or native" >&2
        exit 2
      fi
      RUNTIME="${2:-}"
      shift 2
      ;;
    --help|-h)
      cat <<'EOF'
usage: play-duke3d.sh [--runtime native|converter] [room-app options]

  --runtime native|converter  Select runtime (default: native, or ROOM_DUKE_RUNTIME)
  native                      Native room-duke Build runtime (default)
  converter                   Legacy GRP-to-UDMF gameplay path
  --give-all                  Native Duke: grant every weapon with full ammo
  --grp PATH                  Native Duke: mount an add-on GRP (repeatable)
  --map PATH                  Native Duke: load a standalone Build .MAP file
  --con PATH                  Native Duke: override the root CON script (-x style)
EOF
      exit 0
      ;;
    --)
      shift
      APP_ARGS=("$@")
      break
      ;;
    *)
      APP_ARGS=("$@")
      break
      ;;
  esac
done

case "$RUNTIME" in
  converter)
    ;;
  native)
    ;;
  *)
    echo "room: unknown Duke runtime '$RUNTIME' (choose converter or native)" >&2
    exit 2
    ;;
esac

IWAD="${ROOM_IWAD:-testdata/duke/duke3d.grp}"
if [[ ! -f "$IWAD" ]]; then
  echo "missing GRP: $IWAD" >&2
  echo "Place DUKE3D.GRP (shareware or full) at testdata/duke/duke3d.grp" >&2
  exit 1
fi

# SoundFont for converter MIDI music.
if [[ "$RUNTIME" == "converter" && ! -f soundfonts/TimGM6mb.sf2 && ! -f soundfonts/room.sf2 ]]; then
  ./scripts/fetch-soundfont.sh || echo "warn: no SoundFont — falling back to OPL" >&2
fi

# Episode E, level L is map ExMy (E1L1 = E1M1 in converter naming).
if [[ "$RUNTIME" == "native" ]]; then
  MAP="${ROOM_MAP:-E1L1}"
else
  MAP="${ROOM_MAP:-E1M1}"
fi
SKILL="${ROOM_SKILL:-3}"
# A --map / --skill given on the command line wins over the defaults above
# (room-app rejects repeated flags), e.g. `play-duke3d.sh --map E1L2`.
USER_MAP=0
USER_SKILL=0
for ((i = 0; i < ${#APP_ARGS[@]}; i++)); do
  arg="${APP_ARGS[$i]}"
  case "$arg" in
    --map) USER_MAP=1; MAP="${APP_ARGS[$((i + 1))]:-$MAP}" ;;
    --map=*) USER_MAP=1; MAP="${arg#--map=}" ;;
    --skill) USER_SKILL=1; SKILL="${APP_ARGS[$((i + 1))]:-$SKILL}" ;;
    --skill=*) USER_SKILL=1; SKILL="${arg#--skill=}" ;;
  esac
done
DEFAULT_ARGS=()
[[ "$USER_MAP" == 1 ]] || DEFAULT_ARGS+=(--map "$MAP")
[[ "$USER_SKILL" == 1 ]] || DEFAULT_ARGS+=(--skill "$SKILL")
EXTRA=()
if [[ "$RUNTIME" == "converter" && "${ROOM_TITLE:-0}" == "1" ]]; then
  EXTRA+=(--title)
fi

echo "room: Duke Nukem 3D — $IWAD  map=$MAP  skill=$SKILL"
if [[ "$RUNTIME" == "native" ]]; then
  echo "room: runtime=native Build simulation"
  echo "  Add content with --grp addon.grp; --map may be a standalone .MAP path"
  echo "  WASD move/strafe, mouse or Q/arrows turn, 1–0 weapons, Space jump, C crouch, Ctrl/F/LMB fire, E use, L toggle mouse look"
else
  echo "room: runtime=converter (GRP → UDMF)"
  echo "  WASD move, mouse look, Ctrl/F/LMB fire, 1–9 weapons, E use, Esc pause"
fi
echo "  F12 screenshot, F11 start/stop GIF  (saved under room/screenshots)"

NATIVE_ARGS=()
if [[ "$RUNTIME" == "native" ]]; then
  NATIVE_ARGS+=(--native-duke)
fi

# Empty arrays can trip `set -u` on older bash (macOS).
exec cargo run -p room-app --release -- \
  ${NATIVE_ARGS[@]+"${NATIVE_ARGS[@]}"} \
  --iwad "$IWAD" \
  --play \
  ${DEFAULT_ARGS[@]+"${DEFAULT_ARGS[@]}"} \
  ${EXTRA[@]+"${EXTRA[@]}"} \
  ${APP_ARGS[@]+"${APP_ARGS[@]}"}

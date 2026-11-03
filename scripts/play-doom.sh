#!/usr/bin/env bash
# Play Doom with an IWAD chosen for the files you pass.
#
#   ./scripts/play-doom.sh
#   ./scripts/play-doom.sh --file ~/Downloads/brutal22test6.pk3
#   ./scripts/play-doom.sh --game doom2 --map MAP02
#   ./scripts/play-doom.sh --iwad testdata/doom/doom2.wad --map MAP01 --file mod.pk3 --skill 4 --title
#
# Brutal Doom (filename or gameinfo.txt contains "brutal doom") uses
# testdata/doom/doom2.wad and MAP01 unless you pass --iwad / --game / --map.
# Anything else defaults to testdata/doom/doom1.wad and E1M1.
#
# Extra arguments are forwarded to room-app (for example --god --give-all).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
usage: play-doom.sh [options] [-- room-app args]

  --game doom1|doom2|freedoom1|freedoom2
                         IWAD preset (default: doom2 for Brutal Doom, else doom1)
  --iwad PATH            IWAD file (overrides --game)
  --map NAME             Map lump (default: E1M1 or MAP01 for Doom 2)
  --file PATH            PWAD / PK3 mounted after the IWAD (repeatable)
  --skill N              Skill 1..5 (default 3)
  --title                Stay on the title screen
  --no-zscript-debug     Hide the ZScript load trace
  -h, --help             Show this help

Extra flags are passed through to room-app.
EOF
}

GAME=""
IWAD=""
MAP=""
SKILL="3"
TITLE=0
ZSCRIPT_DEBUG=1
FILES=()
PASSTHRU=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --game)
      GAME="${2:-}"
      shift 2
      ;;
    --iwad)
      IWAD="${2:-}"
      shift 2
      ;;
    --map)
      MAP="${2:-}"
      shift 2
      ;;
    --file)
      FILES+=("${2:-}")
      shift 2
      ;;
    --skill)
      SKILL="${2:-}"
      shift 2
      ;;
    --title)
      TITLE=1
      shift
      ;;
    --zscript-debug)
      ZSCRIPT_DEBUG=1
      shift
      ;;
    --no-zscript-debug)
      ZSCRIPT_DEBUG=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      PASSTHRU+=("$@")
      break
      ;;
    *)
      PASSTHRU+=("$1")
      shift
      ;;
  esac
done

is_brutal_mod() {
  local path="$1"
  local base
  base="$(basename "$path")"
  local lower
  lower="$(printf '%s' "$base" | tr '[:upper:]' '[:lower:]')"
  if [[ "$lower" == *brutal* ]]; then
    return 0
  fi
  case "$lower" in
    *.pk3|*.pk7|*.zip)
      if unzip -p "$path" gameinfo.txt 2>/dev/null | grep -qi 'brutal doom'; then
        return 0
      fi
      ;;
  esac
  return 1
}

preset_iwad() {
  case "$1" in
    doom1) printf '%s\n' "testdata/doom/doom1.wad" ;;
    doom2)
      if [[ -f testdata/doom/doom2.wad ]]; then
        printf '%s\n' "testdata/doom/doom2.wad"
      else
        printf '%s\n' "testdata/doom/DOOM2.WAD"
      fi
      ;;
    freedoom1) printf '%s\n' "testdata/freedoom/freedoom1.wad" ;;
    freedoom2) printf '%s\n' "testdata/freedoom/freedoom2.wad" ;;
    *)
      echo "unknown --game $1 (want doom1, doom2, freedoom1, freedoom2)" >&2
      exit 2
      ;;
  esac
}

default_map_for() {
  local base
  base="$(basename "$1" | tr '[:upper:]' '[:lower:]')"
  case "$base" in
    doom2.wad|freedoom2.wad) printf '%s\n' "MAP01" ;;
    *) printf '%s\n' "E1M1" ;;
  esac
}

brutal=0
for f in "${FILES[@]+"${FILES[@]}"}"; do
  if [[ ! -f "$f" ]]; then
    echo "missing file: $f" >&2
    exit 1
  fi
  if is_brutal_mod "$f"; then
    brutal=1
  fi
done

if [[ -z "$IWAD" && -z "$GAME" && "$brutal" == 1 ]]; then
  GAME="doom2"
  echo "room: Brutal Doom — using Doom 2 IWAD and MAP01"
fi
if [[ -z "$IWAD" ]]; then
  IWAD="$(preset_iwad "${GAME:-doom1}")"
fi
if [[ -z "$MAP" ]]; then
  if [[ -n "$GAME" ]]; then
    case "$GAME" in
      doom2|freedoom2) MAP="MAP01" ;;
      *) MAP="E1M1" ;;
    esac
  else
    MAP="$(default_map_for "$IWAD")"
  fi
fi

if [[ ! -f "$IWAD" ]]; then
  echo "missing IWAD: $IWAD" >&2
  echo "Place the IWAD under testdata/doom/ or pass --iwad PATH" >&2
  exit 1
fi

if [[ ! -f soundfonts/TimGM6mb.sf2 && ! -f soundfonts/room.sf2 ]]; then
  ./scripts/fetch-soundfont.sh || echo "warn: no SoundFont — falling back to OPL" >&2
fi

if [[ "$ZSCRIPT_DEBUG" == 1 ]]; then
  export ROOM_ZSCRIPT_DEBUG=1
else
  export ROOM_ZSCRIPT_DEBUG=0
fi

EXTRA=()
if [[ "$TITLE" == 1 ]]; then
  EXTRA+=(--title)
fi
for f in "${FILES[@]+"${FILES[@]}"}"; do
  EXTRA+=(--file "$f")
done

echo "room: $IWAD  map=$MAP  skill=$SKILL"
echo "  WASD move, mouse look, Ctrl/F/LMB fire, 1–7 weapons, E use, Esc pause"
echo "  F5 Graphics (shaders live)  ·  or Esc → Options → Graphics"
echo "  F12 screenshot, F11 start/stop GIF  (saved under room/screenshots)"

exec cargo run -p room-app --release -- \
  --iwad "$IWAD" \
  --map "$MAP" \
  --play \
  --skill "$SKILL" \
  ${EXTRA[@]+"${EXTRA[@]}"} \
  ${PASSTHRU[@]+"${PASSTHRU[@]}"}

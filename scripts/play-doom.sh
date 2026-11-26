#!/usr/bin/env bash
# Play Doom with an IWAD chosen for the files you pass.
#
#   ./scripts/play-doom.sh
#   ./scripts/play-doom.sh --file ~/Downloads/brutal22test6.pk3
#   ./scripts/play-doom.sh --game doom2 --map MAP02
#   ./scripts/play-doom.sh --iwad testdata/doom/doom2.wad --map MAP01 --file mod.pk3 --skill 4 --title
#   ./scripts/play-doom.sh --file brutal.pk3 --colors rainbow
#   ./scripts/play-doom.sh --file brutal.pk3 --noir
#   ./scripts/play-doom.sh --file mods/colorworld/colorworld.pk3 --set colorworld_mode=7
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
  --set NAME=VALUE       Set a mod option (CVar) at start-up (repeatable),
                         e.g. --set colorworld_mode=1
  --colors MODE          Load the Color World mod (built if needed) in MODE:
                         normal rainbow bands hue gray sepia invert thermal
                         gameboy neon, or a number 0..9
  --colors-speed PCT     Color World animation speed, 0..400 (default 100)
  --colors-strength PCT  Color World effect strength, 0..100 (default 100)
  --noir [MODE]          Load the Red Noir mod (built if needed): a grey world
                         with red blood. MODE: blood (default), reds, bw
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
SETS=()
COLORS=""
NOIR=""
PASSTHRU=()

colorworld_mode() {
  case "$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')" in
    0|normal|off) echo 0 ;;
    1|rainbow) echo 1 ;;
    2|bands|rainbow-bands) echo 2 ;;
    3|hue|hue-cycle) echo 3 ;;
    4|gray|grey|grayscale|greyscale) echo 4 ;;
    5|sepia) echo 5 ;;
    6|invert|inverted) echo 6 ;;
    7|thermal) echo 7 ;;
    8|gameboy|game-boy) echo 8 ;;
    9|neon|neon-edges) echo 9 ;;
    *)
      echo "unknown --colors $1 (want normal rainbow bands hue gray sepia invert thermal gameboy neon, or 0..9)" >&2
      exit 2
      ;;
  esac
}

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
    --set)
      if [[ "${2:-}" != *=* ]]; then
        echo "--set wants NAME=VALUE (got '${2:-}')" >&2
        exit 2
      fi
      SETS+=("$2")
      shift 2
      ;;
    --set=*)
      SETS+=("${1#--set=}")
      shift
      ;;
    --colors)
      COLORS="$(colorworld_mode "${2:-}")"
      shift 2
      ;;
    --noir)
      case "${2:-}" in
        blood|reds|bw|1|2|3)
          case "$2" in blood) NOIR=1 ;; reds) NOIR=2 ;; bw) NOIR=3 ;; *) NOIR="$2" ;; esac
          shift 2
          ;;
        *)
          NOIR=1
          shift
          ;;
      esac
      ;;
    --colors-speed)
      SETS+=("colorworld_speed=${2:-100}")
      shift 2
      ;;
    --colors-strength)
      SETS+=("colorworld_strength=${2:-100}")
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

if [[ -n "$COLORS" ]]; then
  CW_PK3="mods/colorworld/colorworld.pk3"
  if [[ ! -f "$CW_PK3" ]] || [[ -n "$(find mods/colorworld/pk3 -type f -newer "$CW_PK3" 2>/dev/null)" ]]; then
    ./mods/colorworld/pack.sh
  fi
  have_cw=0
  for f in "${FILES[@]+"${FILES[@]}"}"; do
    [[ "$(basename "$f")" == colorworld.pk3 ]] && have_cw=1
  done
  [[ "$have_cw" == 1 ]] || FILES+=("$CW_PK3")
  SETS+=("colorworld_mode=$COLORS")
fi

if [[ -n "$NOIR" ]]; then
  RN_PK3="mods/rednoir/rednoir.pk3"
  if [[ ! -f "$RN_PK3" ]] || [[ -n "$(find mods/rednoir/pk3 -type f -newer "$RN_PK3" 2>/dev/null)" ]]; then
    ./mods/rednoir/pack.sh
  fi
  have_rn=0
  for f in "${FILES[@]+"${FILES[@]}"}"; do
    [[ "$(basename "$f")" == rednoir.pk3 ]] && have_rn=1
  done
  [[ "$have_rn" == 1 ]] || FILES+=("$RN_PK3")
  SETS+=("noir_mode=$NOIR")
fi

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
for kv in "${SETS[@]+"${SETS[@]}"}"; do
  EXTRA+=(--set "$kv")
done

echo "room: $IWAD  map=$MAP  skill=$SKILL"
if [[ ${#SETS[@]} -gt 0 ]]; then
  echo "  options: ${SETS[*]}"
fi
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

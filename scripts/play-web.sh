#!/usr/bin/env bash
# Build room-web (wasm32) and serve it in a local browser.
#
# Usage:
#   ./scripts/play-web.sh              # release build, serve on :8080, open browser
#   PORT=9090 ./scripts/play-web.sh    # custom port
#   ROOM_WEB_NO_OPEN=1 ./scripts/play-web.sh   # serve only (no browser open)
#   ROOM_WEB_DEBUG=1 ./scripts/play-web.sh     # debug wasm (faster compile)
#   ./scripts/play-web.sh mymod.pk3 maps.wad   # also mount PK3 / PK7 / ZIP / PWAD files
#   ./scripts/play-web.sh --pwad mymod.pk3     # same, explicit
#   ROOM_WEB_PWAD=a.pk3:b.wad ./scripts/play-web.sh   # same, from the environment
#   ROOM_WEB_MAP=MAP01 ./scripts/play-web.sh mymod.pk3 # start on a map instead of the title
#
# PWADs are copied to <out>/pwads/ and loaded with ?pwad=pwads/<name>, after
# the IWAD (load order = argument order). In the page you can also add PK3s
# with the PK3 button or by dropping them on the window.
#
# Packaging for upload (no server): ./scripts/package-web.sh --zip
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PORT="${PORT:-8080}"
OUT="${ROOM_WEB_OUT:-target/room-web}"

PWADS=()
if [[ -n "${ROOM_WEB_PWAD:-}" ]]; then
  IFS=':' read -r -a ENV_PWADS <<<"$ROOM_WEB_PWAD"
  PWADS+=("${ENV_PWADS[@]}")
fi
while [[ $# -gt 0 ]]; do
  case "$1" in
    --pwad|--file|-file)
      [[ $# -ge 2 ]] || { echo "room: $1 needs a path" >&2; exit 2; }
      PWADS+=("$2")
      shift 2
      ;;
    --pwad=*|--file=*)
      PWADS+=("${1#*=}")
      shift
      ;;
    --map)
      [[ $# -ge 2 ]] || { echo "room: --map needs a name" >&2; exit 2; }
      ROOM_WEB_MAP="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '2,20p' "$0"
      exit 0
      ;;
    -*)
      echo "room: unknown option $1 (see --help)" >&2
      exit 2
      ;;
    *)
      PWADS+=("$1")
      shift
      ;;
  esac
done
for pw in "${PWADS[@]+"${PWADS[@]}"}"; do
  [[ -f "$pw" ]] || { echo "room: PWAD not found: $pw" >&2; exit 1; }
done

# Local play may include a Doom IWAD from testdata when present (never committed).
# package-web.sh auto mode skips commercial IWADs; for the local server we
# prefer doom1.wad / freedoom when available.
PKG_ARGS=(--out "$OUT")
if [[ "${ROOM_WEB_DEBUG:-0}" == "1" ]]; then
  PKG_ARGS+=(--debug)
fi
if [[ "${ROOM_WEB_NO_ASC:-0}" == "1" ]]; then
  PKG_ARGS+=(--no-asc)
fi
if [[ -n "${ROOM_WEB_IWAD:-}" ]]; then
  PKG_ARGS+=(--iwad "$ROOM_WEB_IWAD")
elif [[ -f "$ROOT/testdata/doom/doom1.wad" ]]; then
  PKG_ARGS+=(--iwad "$ROOT/testdata/doom/doom1.wad")
elif [[ -f "$ROOT/testdata/freedoom/freedoom1.wad" ]]; then
  PKG_ARGS+=(--iwad freedoom)
elif [[ -f "$ROOT/testdata/freedoom/freedoom2.wad" ]]; then
  PKG_ARGS+=(--iwad freedoom)
else
  PKG_ARGS+=(--iwad none)
fi

./scripts/package-web.sh "${PKG_ARGS[@]}"

# Duke Nukem 3D: optional local GRP for ?iwad=duke3d.grp (never packaged for the web by default).
if [[ -f "$ROOT/testdata/duke/duke3d.grp" ]]; then
  cp "$ROOT/testdata/duke/duke3d.grp" "$OUT/duke3d.grp"
  echo "room: Duke Nukem 3D available at ?iwad=duke3d.grp"
fi

# User PWADs / PK3s: served next to the build, mounted via ?pwad=.
QUERY=()
rm -rf "$OUT/pwads"
if [[ ${#PWADS[@]} -gt 0 ]]; then
  mkdir -p "$OUT/pwads"
  PWAD_URLS=()
  i=0
  for pw in "${PWADS[@]}"; do
    base="$(basename "$pw")"
    # Keep names unique and URL-safe while preserving the extension.
    safe="$(printf '%s' "$base" | tr -c 'A-Za-z0-9._-' '_')"
    if [[ -e "$OUT/pwads/$safe" ]]; then
      safe="${i}_$safe"
    fi
    cp "$pw" "$OUT/pwads/$safe"
    PWAD_URLS+=("pwads/$safe")
    echo "room: PWAD $pw -> ?pwad=pwads/$safe"
    i=$((i + 1))
  done
  QUERY+=("pwad=$(IFS=,; echo "${PWAD_URLS[*]}")")
fi
if [[ -n "${ROOM_WEB_MAP:-}" ]]; then
  QUERY+=("map=$ROOM_WEB_MAP")
fi

serve() {
  local dir="$1" port="$2"
  if command -v python3 >/dev/null 2>&1; then
    (cd "$dir" && exec python3 -u -m http.server "$port" --bind 127.0.0.1)
  elif command -v python >/dev/null 2>&1; then
    (cd "$dir" && exec python -u -m http.server "$port")
  elif command -v ruby >/dev/null 2>&1; then
    (cd "$dir" && exec ruby -run -ehttpd . -p"$port")
  else
    echo "need python3 (or ruby) to serve $dir" >&2
    exit 1
  fi
}

URL="http://127.0.0.1:${PORT}/"
if [[ ${#QUERY[@]} -gt 0 ]]; then
  URL="${URL}?$(IFS='&'; echo "${QUERY[*]}")"
fi
echo "room: serving $URL  (Ctrl+C to stop)"
if [[ -f "$OUT/doom1.wad" || -f "$OUT/freedoom1.wad" ]]; then
  echo "  Doom play: IWAD auto-loads; click canvas for mouse look (WASD / E / fire)"
else
  echo "  Doom play: choose an IWAD in the page overlay (or set ROOM_WEB_IWAD=/path/to.wad)"
fi

open_browser() {
  local url="$1"
  if [[ "${ROOM_WEB_NO_OPEN:-0}" == "1" ]]; then
    return 0
  fi
  if command -v open >/dev/null 2>&1; then
    open "$url" || true
  elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open "$url" || true
  fi
}

( sleep 0.6 && open_browser "$URL" ) &
serve "$OUT" "$PORT"

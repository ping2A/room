#!/usr/bin/env bash
# Build room-web (wasm32) and serve it in a local browser.
#
# Usage:
#   ./scripts/play-web.sh              # release build, serve on :8080, open browser
#   PORT=9090 ./scripts/play-web.sh    # custom port
#   ROOM_WEB_NO_OPEN=1 ./scripts/play-web.sh   # serve only (no browser open)
#   ROOM_WEB_DEBUG=1 ./scripts/play-web.sh     # debug wasm (faster compile)
#
# Packaging for upload (no server): ./scripts/package-web.sh --zip
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PORT="${PORT:-8080}"
OUT="${ROOM_WEB_OUT:-target/room-web}"

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

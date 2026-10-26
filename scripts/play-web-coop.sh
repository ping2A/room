#!/usr/bin/env bash
# Build room-web + start a coop relay, then serve coop-ready pages.
#
# Usage:
#   ./scripts/play-web-coop.sh
#   ./scripts/play-web-coop.sh --server              # multi-room room-server (docs/22)
#   ./scripts/play-web-coop.sh --no-open --players 3 --port 9090
#   ./scripts/play-web-coop.sh --server --map E1M1 --ws-port 8765 --debug
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PORT=8080
WS_PORT=8765
PLAYERS=2
MAP=E1M1
OUT="${ROOT}/target/room-web"
USE_ROOM_SERVER=0
USE_V2=0
NO_OPEN=0
DEBUG=0
IWAD=""
BINDGEN_VER=0.2.129

usage() {
  cat <<'EOF'
Usage: play-web-coop.sh [options]
  --server          Use room-server (multi-room) instead of room-ws
  --v2              Use protocol v2 WebSocket path (implies --server)
  --no-open         Do not open browser tabs
  --port N          HTTP port (default 8080)
  --ws-port N       WebSocket / API port (default 8765)
  --players N       Seat count 2..8 (default 2)
  --map NAME        Start map (default E1M1)
  --iwad PATH       Copy this IWAD into the web root as doom1.wad
  --out DIR         Output directory for wasm/html (default target/room-web)
  --debug           Build room-web debug profile
  --bindgen-ver V   wasm-bindgen-cli version (default 0.2.129)
  -h, --help        Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --server) USE_ROOM_SERVER=1; shift ;;
    --v2) USE_V2=1; USE_ROOM_SERVER=1; shift ;;
    --no-open) NO_OPEN=1; shift ;;
    --port) PORT="${2:?}"; shift 2 ;;
    --ws-port) WS_PORT="${2:?}"; shift 2 ;;
    --players) PLAYERS="${2:?}"; shift 2 ;;
    --map) MAP="${2:?}"; shift 2 ;;
    --iwad) IWAD="${2:?}"; shift 2 ;;
    --out) OUT="${2:?}"; shift 2 ;;
    --debug) DEBUG=1; shift ;;
    --bindgen-ver) BINDGEN_VER="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

WS_URL="ws://127.0.0.1:${WS_PORT}"

cleanup() {
  if [[ -n "${RELAY_PID:-}" ]] && kill -0 "$RELAY_PID" 2>/dev/null; then
    kill "$RELAY_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

if [[ "${USE_ROOM_SERVER}" -eq 1 ]]; then
  echo "room: building room-server…"
  cargo build -p room-server --release --bin room-server
  echo "room: starting room-server on ${WS_URL}…"
  cargo run -p room-server --release --bin room-server -- --bind "127.0.0.1:${WS_PORT}" &
  RELAY_PID=$!
  sleep 0.5
  if ! kill -0 "$RELAY_PID" 2>/dev/null; then
    echo "room-server failed to start" >&2
    exit 1
  fi
  CREATE_JSON="$(curl -sf -X POST "http://127.0.0.1:${WS_PORT}/api/rooms" \
    -H 'content-type: application/json' \
    -d "{\"map\":\"${MAP}\",\"skill\":3,\"max_players\":${PLAYERS}}")"
  ROOM_ID="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"${CREATE_JSON}")"
  if [[ "${USE_V2}" -eq 1 ]]; then
    WS_URL="ws://127.0.0.1:${WS_PORT}/v2/rooms/${ROOM_ID}/ws"
    echo "room: created room ${ROOM_ID}"
    echo "room: v2 ws ${WS_URL}"
  else
    WS_URL="ws://127.0.0.1:${WS_PORT}/ws/${ROOM_ID}"
    echo "room: created room ${ROOM_ID}"
    echo "room: legacy ws ${WS_URL}"
  fi
else
  echo "room: building room-ws…"
  cargo build -p room-ws --release
  echo "room: starting relay on ${WS_URL} (${PLAYERS} players)…"
  cargo run -p room-ws --release -- --bind "127.0.0.1:${WS_PORT}" --players "${PLAYERS}" &
  RELAY_PID=$!
  sleep 0.4
  if ! kill -0 "$RELAY_PID" 2>/dev/null; then
    echo "room-ws failed to start" >&2
    exit 1
  fi
fi

PROFILE=release
CARGO_PROFILE_FLAG=(--release)
if [[ "${DEBUG}" -eq 1 ]]; then
  PROFILE=debug
  CARGO_PROFILE_FLAG=()
fi

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    return 1
  fi
}
need_cmd cargo
need_cmd rustup

rustup target add wasm32-unknown-unknown >/dev/null
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  cargo install wasm-bindgen-cli --locked --version "$BINDGEN_VER"
else
  got="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')"
  if [[ "$got" != "$BINDGEN_VER" ]]; then
    cargo install wasm-bindgen-cli --locked --version "$BINDGEN_VER" --force
  fi
fi

echo "room: building room-web ($PROFILE / wasm32)…"
cargo build -p room-web --target wasm32-unknown-unknown --features browser \
  "${CARGO_PROFILE_FLAG[@]+"${CARGO_PROFILE_FLAG[@]}"}"

WASM="target/wasm32-unknown-unknown/${PROFILE}/room_web.wasm"
rm -rf "$OUT"
mkdir -p "$OUT"
wasm-bindgen "$WASM" --out-dir "$OUT" --target web --no-typescript
cp crates/room-web/index.html "$OUT/index.html"
cp crates/room-web/voice.js "$OUT/voice.js"
cp crates/room-web/rtc.js "$OUT/rtc.js"
cp crates/room-web/netgraph.js "$OUT/netgraph.js"
cp crates/room-web/touch.js "$OUT/touch.js"
cp crates/room-web/settings.js "$OUT/settings.js"
cp crates/room-web/mixer-worklet.js "$OUT/mixer-worklet.js"
CACHE_TS="$(date +%s)"
if [[ "$(uname)" == Darwin ]]; then
  sed -i '' "s|./room_web.js|./room_web.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i '' "s|./voice.js|./voice.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i '' "s|./rtc.js|./rtc.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i '' "s|./netgraph.js|./netgraph.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i '' "s|./touch.js|./touch.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i '' "s|room_web_bg.wasm|room_web_bg.wasm?v=${CACHE_TS}|g" "$OUT/room_web.js"
else
  sed -i "s|./room_web.js|./room_web.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i "s|./voice.js|./voice.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i "s|./rtc.js|./rtc.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i "s|./netgraph.js|./netgraph.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i "s|./touch.js|./touch.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i "s|room_web_bg.wasm|room_web_bg.wasm?v=${CACHE_TS}|g" "$OUT/room_web.js"
fi

copy_iwad() {
  local src="$1" dest_name="$2"
  if [[ -f "$src" ]]; then
    cp "$src" "$OUT/$dest_name"
    echo "room: serving IWAD as /$dest_name"
    return 0
  fi
  return 1
}
if [[ -n "${IWAD}" ]] && copy_iwad "${IWAD}" "doom1.wad"; then
  :
elif copy_iwad "$ROOT/testdata/doom/doom1.wad" "doom1.wad"; then
  :
elif copy_iwad "$ROOT/testdata/freedoom/freedoom1.wad" "freedoom1.wad"; then
  :
elif copy_iwad "$ROOT/testdata/freedoom/freedoom2.wad" "freedoom1.wad"; then
  :
else
  echo "room: no local IWAD — use the file picker in each tab" >&2
fi

BASE="http://127.0.0.1:${PORT}/?ws=${WS_URL}&players=${PLAYERS}&map=${MAP}"

open_browser() {
  local url="$1"
  if [[ "${NO_OPEN}" -eq 1 ]]; then
    return 0
  fi
  if command -v open >/dev/null 2>&1; then
    open "$url" || true
  elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open "$url" || true
  fi
}

echo "room: coop URLs (open one tab per player, same IWAD):"
for ((i = 0; i < PLAYERS; i++)); do
  echo "  ${BASE}&name=p$((i + 1))"
done

(
  sleep 0.8
  for ((i = 0; i < PLAYERS; i++)); do
    open_browser "${BASE}&name=p$((i + 1))"
    sleep 0.3
  done
) &

serve() {
  local dir="$1" port="$2"
  if command -v python3 >/dev/null 2>&1; then
    (cd "$dir" && exec python3 -u -m http.server "$port" --bind 127.0.0.1)
  elif command -v python >/dev/null 2>&1; then
    (cd "$dir" && exec python -u -m http.server "$port")
  else
    echo "need python3 to serve $dir" >&2
    exit 1
  fi
}

echo "room: serving http://127.0.0.1:${PORT}/  (Ctrl+C stops web + relay)"
serve "$OUT" "$PORT"

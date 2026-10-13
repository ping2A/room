#!/usr/bin/env bash
# Build room-web + start room-ws relay, then serve coop-ready pages.
#
# Usage:
#   ./scripts/play-web-coop.sh              # relay :8765, web :8080, open 2 tabs
#   PLAYERS=3 PORT=9090 ./scripts/play-web-coop.sh
#   ROOM_WEB_NO_OPEN=1 ./scripts/play-web-coop.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PORT="${PORT:-8080}"
WS_PORT="${WS_PORT:-8765}"
PLAYERS="${PLAYERS:-2}"
WS_URL="${WS_URL:-ws://127.0.0.1:${WS_PORT}}"
OUT="${ROOM_WEB_OUT:-target/room-web}"

cleanup() {
  if [[ -n "${RELAY_PID:-}" ]] && kill -0 "$RELAY_PID" 2>/dev/null; then
    kill "$RELAY_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

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

BINDGEN_VER="${ROOM_WASM_BINDGEN_VER:-0.2.129}"
PROFILE=release
CARGO_PROFILE_FLAG=(--release)
if [[ "${ROOM_WEB_DEBUG:-0}" == "1" ]]; then
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

copy_iwad() {
  local src="$1" dest_name="$2"
  if [[ -f "$src" ]]; then
    cp "$src" "$OUT/$dest_name"
    echo "room: serving IWAD as /$dest_name"
    return 0
  fi
  return 1
}
if [[ -n "${ROOM_WEB_IWAD:-}" ]] && copy_iwad "$ROOM_WEB_IWAD" "doom1.wad"; then
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

MAP="${MAP:-E1M1}"
BASE="http://127.0.0.1:${PORT}/?ws=${WS_URL}&players=${PLAYERS}&map=${MAP}"

open_browser() {
  local url="$1"
  if [[ "${ROOM_WEB_NO_OPEN_TABS:-0}" == "1" ]]; then
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

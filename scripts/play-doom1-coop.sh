#!/usr/bin/env bash
# Launch 2-player coop on Doom E1M1 (two room windows).
# Env: ROOM_TRANSPORT=udp|tcp|tcp-server  ROOM_NOISE=1  ROOM_PORT=5029
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IWAD="${ROOM_IWAD:-$ROOT/testdata/doom/doom1.wad}"
PORT="${ROOM_PORT:-5029}"
MAP="${ROOM_MAP:-E1M1}"
TRANSPORT="${ROOM_TRANSPORT:-udp}"
NOISE_ARGS=()
if [[ "${ROOM_NOISE:-0}" == "1" ]]; then
  NOISE_ARGS=(--noise)
fi

if [[ ! -f "$IWAD" ]]; then
  echo "missing IWAD: $IWAD" >&2
  exit 1
fi

cd "$ROOT"
cargo build -p room-app --release
BIN="$ROOT/target/release/room"

echo "host: $BIN --iwad $IWAD --map $MAP --host $PORT --players 2 --transport $TRANSPORT ${NOISE_ARGS[*]:-} --play"
"$BIN" --iwad "$IWAD" --map "$MAP" --host "$PORT" --players 2 --transport "$TRANSPORT" "${NOISE_ARGS[@]}" --play "$@" &
HOST_PID=$!
sleep 1
echo "join: $BIN --iwad $IWAD --map $MAP --join 127.0.0.1:$PORT --transport $TRANSPORT ${NOISE_ARGS[*]:-} --play"
"$BIN" --iwad "$IWAD" --map "$MAP" --join "127.0.0.1:$PORT" --transport "$TRANSPORT" "${NOISE_ARGS[@]}" --play "$@" &
JOIN_PID=$!

cleanup() {
  kill "$HOST_PID" "$JOIN_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM
wait "$HOST_PID" "$JOIN_PID"

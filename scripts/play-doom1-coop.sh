#!/usr/bin/env bash
# Launch 2-player coop on Doom E1M1 (two ferrite windows).
# Env: FERRITE_TRANSPORT=udp|tcp|tcp-server  FERRITE_NOISE=1  FERRITE_PORT=5029
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IWAD="${FERRITE_IWAD:-$ROOT/testdata/doom/doom1.wad}"
PORT="${FERRITE_PORT:-5029}"
MAP="${FERRITE_MAP:-E1M1}"
TRANSPORT="${FERRITE_TRANSPORT:-udp}"
NOISE_ARGS=()
if [[ "${FERRITE_NOISE:-0}" == "1" ]]; then
  NOISE_ARGS=(--noise)
fi

if [[ ! -f "$IWAD" ]]; then
  echo "missing IWAD: $IWAD" >&2
  exit 1
fi

cd "$ROOT"
cargo build -p ferrite-app --release
BIN="$ROOT/target/release/ferrite"

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

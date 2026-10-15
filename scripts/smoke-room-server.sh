#!/usr/bin/env bash
# Automated smoke for docs/22 S1 (`play-web-coop.sh --server` HTTP/WS path).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIND="127.0.0.1:18765"
MAP=E1M1
PLAYERS=2

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bind) BIND="${2:?}"; shift 2 ;;
    --map) MAP="${2:?}"; shift 2 ;;
    --players) PLAYERS="${2:?}"; shift 2 ;;
    -h|--help) echo "Usage: $0 [--bind host:port] [--map M] [--players N]"; exit 0 ;;
    *) echo "unknown: $1" >&2; exit 2 ;;
  esac
done

cargo build -p room-server --release --bin room-server
cargo run -p room-server --release --bin room-server -- --bind "${BIND}" --max-conn-per-ip 0 &
PID=$!
cleanup() { kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; }
trap cleanup EXIT INT TERM

for _ in $(seq 1 80); do
  if curl -sf "http://${BIND}/healthz" >/dev/null; then break; fi
  sleep 0.05
done
curl -sf "http://${BIND}/healthz" | grep -q ok
curl -sf "http://${BIND}/readyz" | grep -q ok

CREATE="$(curl -sf -X POST "http://${BIND}/api/rooms" \
  -H 'content-type: application/json' \
  -d "{\"map\":\"${MAP}\",\"skill\":3,\"max_players\":${PLAYERS}}")"
ROOM_ID="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"${CREATE}")"
WS_LEGACY="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["ws_legacy"])' <<<"${CREATE}")"
WS_V2="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["ws_v2"])' <<<"${CREATE}")"
echo "smoke: room ${ROOM_ID} legacy=${WS_LEGACY} v2=${WS_V2}"

JOIN="$(curl -sf -X POST "http://${BIND}/api/rooms/${ROOM_ID}/join" \
  -H 'content-type: application/json' \
  -d '{"name":"smoke"}')"
python3 -c 'import json,sys; j=json.load(sys.stdin); assert j["ticket"] and j["ws_v2"] and j["node_id"]' <<<"${JOIN}"

curl -sf "http://${BIND}/metrics" | grep -q 'room_server_rooms'
curl -sf "http://${BIND}/admin/api/v1/summary" | python3 -c 'import json,sys; assert json.load(sys.stdin)["rooms"]>=1'

# Integration suite covers the same wire path as browser tabs under --server/--v2.
cargo test -p room-server --test multi_room_legacy v2_tic_frame_two_seats -- --exact

echo "smoke: play-web-coop --server path ok (health/ready/create/join/metrics/admin + v2 tic)"

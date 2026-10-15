#!/usr/bin/env bash
# Memory soak for room-server (docs/22 §6). CLI args only.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

HOURS=1
ROOMS=256
CONCURRENCY=32
PLAYERS=2
TICS=35
MAX_RSS_GROWTH_KIB=65536

while [[ $# -gt 0 ]]; do
  case "$1" in
    --hours) HOURS="${2:?}"; shift 2 ;;
    --rooms) ROOMS="${2:?}"; shift 2 ;;
    --concurrency) CONCURRENCY="${2:?}"; shift 2 ;;
    --players) PLAYERS="${2:?}"; shift 2 ;;
    --tics) TICS="${2:?}"; shift 2 ;;
    --max-rss-growth-kib) MAX_RSS_GROWTH_KIB="${2:?}"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--hours N] [--rooms N] [--concurrency N] [--players N] [--tics N] [--max-rss-growth-kib KiB]"
      exit 0
      ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

SOAK_SECS=$((HOURS * 3600))
# All rooms must soak in parallel (one batch).
if [[ "${CONCURRENCY}" -lt "${ROOMS}" ]]; then
  CONCURRENCY="${ROOMS}"
fi
echo "soak-room-server: hours=${HOURS} rooms=${ROOMS} players=${PLAYERS} concurrency=${CONCURRENCY} soak_secs=${SOAK_SECS}"

cargo run -p room-server --release --bin room-load -- \
  --rooms "${ROOMS}" \
  --players "${PLAYERS}" \
  --tics "${TICS}" \
  --concurrency "${CONCURRENCY}" \
  --soak-secs "${SOAK_SECS}" \
  --sample-rss \
  --max-rss-growth-kib "${MAX_RSS_GROWTH_KIB}" \
  --max-p99-us 2000

echo "soak-room-server: clean"

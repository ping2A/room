#!/usr/bin/env bash
# Fuzz room-proto decoders (docs/22 S2). CLI seconds, not env.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SECONDS_TOTAL=3600
TARGET=room_proto_binary

while [[ $# -gt 0 ]]; do
  case "$1" in
    --seconds) SECONDS_TOTAL="${2:?}"; shift 2 ;;
    --target) TARGET="${2:?}"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--seconds N] [--target NAME]"
      echo "  Default: --seconds 3600 --target room_proto_binary"
      exit 0
      ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if ! command -v cargo-fuzz >/dev/null 2>&1 && ! cargo fuzz --help >/dev/null 2>&1; then
  echo "installing cargo-fuzz…"
  cargo install cargo-fuzz --locked
fi

echo "fuzz: target=${TARGET} seconds=${SECONDS_TOTAL}"
cargo +nightly fuzz run "${TARGET}" --fuzz-dir fuzz -- -max_total_time="${SECONDS_TOTAL}"
echo "fuzz: clean (${TARGET}, ${SECONDS_TOTAL}s)"

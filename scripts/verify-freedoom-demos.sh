#!/usr/bin/env bash
# Verify committed Freedoom playthrough demos + MAP01 real-play smoke.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ ! -f testdata/freedoom/freedoom2.wad ]]; then
  echo "fetching Freedoom…"
  ./scripts/fetch-freedoom.sh
fi

echo "== m7 feature smoke =="
cargo test -p room-game --test m7_feature_smoke -- --nocapture

echo "== Freedoom MAP01 committed smoke demo =="
cargo test -p room-game --test freedoom_smoke -- freedoom2_map01_committed_demo_verifies --nocapture

echo "== Freedoom MAP01 door use =="
cargo test -p room-game --test freedoom_smoke -- freedoom2_map01_use_opens_translated_door --nocapture

echo "== Freedoom MAP01 real-play (walk/fire + door) =="
cargo test -p room-game --test freedoom_smoke -- freedoom2_map01_real_play_records_and_verifies --nocapture

echo "== Freedoom MAP01–03 bundle =="
cargo test -p room-game --test freedoom_smoke -- freedoom2_committed_map01_03_bundle_verifies --nocapture

echo "== Freedoom Phase 1 E1 bundle =="
cargo test -p room-game --test freedoom_smoke -- freedoom1_committed_e1_bundle_verifies --nocapture

echo "ok: Freedoom demos verified"

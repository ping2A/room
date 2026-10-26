#!/usr/bin/env bash
# Native Duke Nukem 3D runtime track (docs/25 / room-duke) — not the GRP→UDMF
# converter path (`scripts/play-duke3d.sh`).
#
# Usage:
#   ./scripts/test-duke-native.sh              # lib tests + DEMO1 frontier
#   ./scripts/test-duke-native.sh frontier     # frontier only
#   ./scripts/test-duke-native.sh tic23        # tic23 dive/warp check
#   ./scripts/test-duke-native.sh soft-view    # E1L1 soft FP → soft-view.pgm
#   ./scripts/test-duke-native.sh all          # build-engine + duke + frontier
#
# Needs testdata/duke/duke3d.grp (+ demos/traces for frontier). Commercial GRP
# is local-only — never commit it.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

GRP="${ROOM_IWAD:-testdata/duke/duke3d.grp}"
DEMO="${ROOM_DEMO:-testdata/duke/demos/DEMO1.DMO}"
TRACE="${ROOM_TRACE:-testdata/duke/traces/DEMO1.trace}"
MODE="${1:-default}"

need_grp() {
  if [[ ! -f "$GRP" ]]; then
    echo "missing GRP: $GRP" >&2
    echo "Place DUKE3D.GRP (shareware or full) at testdata/duke/duke3d.grp" >&2
    exit 1
  fi
}

need_demo_corpus() {
  need_grp
  if [[ ! -f "$DEMO" || ! -f "$TRACE" ]]; then
    echo "missing DEMO1 corpus:" >&2
    echo "  demo:  $DEMO" >&2
    echo "  trace: $TRACE" >&2
    echo "Extract demos: cargo run -p room-cli -- duke extract-demos" >&2
    echo "Chocolate traces: see docs/duke-trace-howto.md / third_party/chocolate-duke3d-trace" >&2
    exit 1
  fi
}

run_frontier() {
  need_demo_corpus
  echo "== DEMO1 frontier (first Chocolate divergence) =="
  cargo test -p room-duke demo1_frontier -- --nocapture
}

run_tic23() {
  need_demo_corpus
  echo "== DEMO1 tic23 (water dive / seed) =="
  cargo test -p room-duke demo1_tic23_diff -- --nocapture
}

run_lib() {
  echo "== room-duke lib tests =="
  cargo test -p room-duke --lib
}

run_engine() {
  echo "== room-build-engine tests =="
  cargo test -p room-build-engine
}

run_soft_view() {
  need_grp
  local out="${ROOM_SOFT_OUT:-soft-view.pgm}"
  local map="${ROOM_MAP_LUMP:-E1L1.MAP}"
  echo "== soft FP view  map=$map  → $out =="
  cargo run -p room-cli --release -- duke soft-view "$GRP" \
    --map "$map" \
    -o "$out" \
    ${ROOM_SOFT_EXTRA:+$ROOM_SOFT_EXTRA}
  echo "wrote $out"
}

run_manifest() {
  echo "== trace manifest soft-check =="
  cargo run -p room-cli -- duke trace-check-manifest
}

case "$MODE" in
  default|"")
    run_lib
    run_frontier
    ;;
  frontier)
    run_frontier
    ;;
  tic23)
    run_tic23
    ;;
  soft-view|soft_view)
    run_soft_view
    ;;
  manifest)
    run_manifest
    ;;
  all)
    run_engine
    run_lib
    run_manifest
    run_frontier
    ;;
  -h|--help|help)
    awk 'NR==1{next} /^#/{sub(/^# ?/,""); print; next} {exit}' "$0"
    exit 0
    ;;
  *)
    echo "unknown mode: $MODE (try: default|frontier|tic23|soft-view|manifest|all)" >&2
    exit 1
    ;;
esac

echo "ok: native Duke ($MODE)"

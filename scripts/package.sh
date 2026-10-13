#!/usr/bin/env bash
# Package Room binaries per OS (M13).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT="${ROOM_DIST:-dist}"
TARGET="${1:-}"
PROFILE="${ROOM_PROFILE:-release}"

mkdir -p "$OUT"

build_native() {
  local triple="$1"
  local name="$2"
  echo "==> building $name ($triple)"
  if [[ -n "$triple" && "$triple" != "host" ]]; then
    cargo build -p room-app --"$PROFILE" --target "$triple"
    BIN="target/${triple}/${PROFILE}/room"
  else
    cargo build -p room-app --"$PROFILE"
    BIN="target/${PROFILE}/room"
  fi
  local dest="${OUT}/${name}"
  mkdir -p "$dest"
  cp "$BIN" "$dest/"
  cp README.md LICENSE "$dest/" 2>/dev/null || true
  cp scripts/play-combat-demo.sh "$dest/" 2>/dev/null || true
  echo "    -> $dest"
}

build_web() {
  echo "==> building web (wasm32)"
  if ! command -v wasm-pack >/dev/null 2>&1; then
    echo "wasm-pack not found; skipping web bundle (install wasm-pack to enable)"
    return 0
  fi
  (cd crates/room-web && wasm-pack build --target web --out-dir ../../"${OUT}/web" -- --features browser)
  cp crates/room-web/index.html "${OUT}/web/" 2>/dev/null || true
  echo "    -> ${OUT}/web"
}

case "${TARGET:-host}" in
  host|native)
    build_native host "room-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
    ;;
  web)
    build_web
    ;;
  all)
    build_native host "room-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
    build_web
    ;;
  *)
    echo "usage: $0 [host|web|all]"
    exit 1
    ;;
esac

echo "done. artifacts under ${OUT}/"

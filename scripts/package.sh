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
  echo "==> building web (wasm32 + mods)"
  # Full static site (HTML/JS/WASM/mods); same layout as ./scripts/package-web.sh
  ROOM_WEB_OUT="${OUT}/web" ./scripts/package-web.sh --out "${OUT}/web" --iwad none
  echo "    -> ${OUT}/web  (zip with: ./scripts/package-web.sh --zip)"
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

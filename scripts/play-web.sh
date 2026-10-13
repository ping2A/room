#!/usr/bin/env bash
# Build ferrite-web (wasm32) and serve it in a local browser.
#
# Usage:
#   ./scripts/play-web.sh              # release build, serve on :8080, open browser
#   PORT=9090 ./scripts/play-web.sh    # custom port
#   FERRITE_WEB_NO_OPEN=1 ./scripts/play-web.sh   # serve only (no browser open)
#   FERRITE_WEB_DEBUG=1 ./scripts/play-web.sh     # debug wasm (faster compile)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PORT="${PORT:-8080}"
OUT="${FERRITE_WEB_OUT:-target/ferrite-web}"
BINDGEN_VER="${FERRITE_WASM_BINDGEN_VER:-0.2.129}"
PROFILE=release
CARGO_PROFILE_FLAG=(--release)
if [[ "${FERRITE_WEB_DEBUG:-0}" == "1" ]]; then
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

echo "ferrite: ensuring wasm32-unknown-unknown target…"
rustup target add wasm32-unknown-unknown >/dev/null

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "ferrite: installing wasm-bindgen-cli ${BINDGEN_VER}…"
  cargo install wasm-bindgen-cli --locked --version "$BINDGEN_VER"
else
  got="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')"
  if [[ "$got" != "$BINDGEN_VER" ]]; then
    echo "ferrite: wasm-bindgen is $got (want ${BINDGEN_VER}); installing…"
    cargo install wasm-bindgen-cli --locked --version "$BINDGEN_VER" --force
  fi
fi

echo "ferrite: building ferrite-web ($PROFILE / wasm32)…"
cargo build -p ferrite-web --target wasm32-unknown-unknown --features browser \
  "${CARGO_PROFILE_FLAG[@]+"${CARGO_PROFILE_FLAG[@]}"}"

WASM="target/wasm32-unknown-unknown/${PROFILE}/ferrite_web.wasm"
if [[ ! -f "$WASM" ]]; then
  echo "missing wasm artifact: $WASM" >&2
  exit 1
fi

rm -rf "$OUT"
mkdir -p "$OUT"
echo "ferrite: wasm-bindgen → $OUT"
wasm-bindgen "$WASM" --out-dir "$OUT" --target web --no-typescript
cp crates/ferrite-web/index.html "$OUT/index.html"

# Serve a local Doom / Freedoom IWAD when present (never commit commercial IWADs).
copy_iwad() {
  local src="$1" dest_name="$2"
  if [[ -f "$src" ]]; then
    cp "$src" "$OUT/$dest_name"
    echo "ferrite: serving IWAD as /$dest_name ($(du -h "$OUT/$dest_name" | awk '{print $1}'))"
    return 0
  fi
  return 1
}
IWAD_COPIED=0
if [[ -n "${FERRITE_WEB_IWAD:-}" ]] && copy_iwad "$FERRITE_WEB_IWAD" "doom1.wad"; then
  IWAD_COPIED=1
elif copy_iwad "$ROOT/testdata/doom/doom1.wad" "doom1.wad"; then
  IWAD_COPIED=1
elif copy_iwad "$ROOT/testdata/freedoom/freedoom1.wad" "freedoom1.wad"; then
  IWAD_COPIED=1
elif copy_iwad "$ROOT/testdata/freedoom/freedoom2.wad" "freedoom1.wad"; then
  # Freedoom Phase 2 is MAP01-style; still playable via MAP01 fallback.
  IWAD_COPIED=1
fi
if [[ "$IWAD_COPIED" -eq 0 ]]; then
  echo "ferrite: no local IWAD found — use the file picker in the browser"
  echo "  (looked for testdata/doom/doom1.wad or testdata/freedoom/freedoom*.wad)"
fi

# Prefer python3 http.server; fall back to npx serve / ruby.
serve() {
  local dir="$1" port="$2"
  if command -v python3 >/dev/null 2>&1; then
    # Quiet + bind localhost; -u for line-buffered logs.
    (cd "$dir" && exec python3 -u -m http.server "$port" --bind 127.0.0.1)
  elif command -v python >/dev/null 2>&1; then
    (cd "$dir" && exec python -u -m http.server "$port")
  elif command -v ruby >/dev/null 2>&1; then
    (cd "$dir" && exec ruby -run -ehttpd . -p"$port")
  else
    echo "need python3 (or ruby) to serve $dir" >&2
    exit 1
  fi
}

URL="http://127.0.0.1:${PORT}/"
echo "ferrite: web build ready in $OUT"
echo "ferrite: serving $URL  (Ctrl+C to stop)"
if [[ "$IWAD_COPIED" -eq 1 ]]; then
  echo "  Doom play: IWAD auto-loads; click canvas for mouse look (WASD / E / fire)"
else
  echo "  Doom play: choose an IWAD in the page overlay (or set FERRITE_WEB_IWAD=/path/to.wad)"
fi

open_browser() {
  local url="$1"
  if [[ "${FERRITE_WEB_NO_OPEN:-0}" == "1" ]]; then
    return 0
  fi
  if command -v open >/dev/null 2>&1; then
    open "$url" || true
  elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open "$url" || true
  fi
}

# Open after the server is up (brief delay so the first request doesn't 404).
( sleep 0.6 && open_browser "$URL" ) &
serve "$OUT" "$PORT"

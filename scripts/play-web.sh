#!/usr/bin/env bash
# Build room-web (wasm32) and serve it in a local browser.
#
# Usage:
#   ./scripts/play-web.sh              # release build, serve on :8080, open browser
#   PORT=9090 ./scripts/play-web.sh    # custom port
#   ROOM_WEB_NO_OPEN=1 ./scripts/play-web.sh   # serve only (no browser open)
#   ROOM_WEB_DEBUG=1 ./scripts/play-web.sh     # debug wasm (faster compile)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PORT="${PORT:-8080}"
OUT="${ROOM_WEB_OUT:-target/room-web}"
BINDGEN_VER="${ROOM_WASM_BINDGEN_VER:-0.2.129}"
PROFILE=release
CARGO_PROFILE_FLAG=(--release)
if [[ "${ROOM_WEB_DEBUG:-0}" == "1" ]]; then
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

echo "room: ensuring wasm32-unknown-unknown target…"
rustup target add wasm32-unknown-unknown >/dev/null

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "room: installing wasm-bindgen-cli ${BINDGEN_VER}…"
  cargo install wasm-bindgen-cli --locked --version "$BINDGEN_VER"
else
  got="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')"
  if [[ "$got" != "$BINDGEN_VER" ]]; then
    echo "room: wasm-bindgen is $got (want ${BINDGEN_VER}); installing…"
    cargo install wasm-bindgen-cli --locked --version "$BINDGEN_VER" --force
  fi
fi

echo "room: building room-web ($PROFILE / wasm32)…"
cargo build -p room-web --target wasm32-unknown-unknown --features browser \
  "${CARGO_PROFILE_FLAG[@]+"${CARGO_PROFILE_FLAG[@]}"}"

WASM="target/wasm32-unknown-unknown/${PROFILE}/room_web.wasm"
if [[ ! -f "$WASM" ]]; then
  echo "missing wasm artifact: $WASM" >&2
  exit 1
fi

rm -rf "$OUT"
mkdir -p "$OUT"
echo "room: wasm-bindgen → $OUT"
wasm-bindgen "$WASM" --out-dir "$OUT" --target web --no-typescript
cp crates/room-web/index.html "$OUT/index.html"
cp crates/room-web/voice.js "$OUT/voice.js"
cp crates/room-web/rtc.js "$OUT/rtc.js"
cp crates/room-web/netgraph.js "$OUT/netgraph.js"
cp crates/room-web/touch.js "$OUT/touch.js"
cp crates/room-web/mixer-worklet.js "$OUT/mixer-worklet.js"
cp crates/room-web/editor.html "$OUT/editor.html"
cp crates/room-web/editor.js "$OUT/editor.js"
cp crates/room-web/editor-tools.js "$OUT/editor-tools.js"
cp crates/room-web/editor-paint.js "$OUT/editor-paint.js"
cp crates/room-web/editor-scripts.js "$OUT/editor-scripts.js"
mkdir -p "$OUT/mods"
cp mods/doom1-survival/survival.wad "$OUT/mods/survival.wad"
cp mods/doom1-bounty/bounty.wad "$OUT/mods/bounty.wad"
# AssemblyScript live mods: the game API library + a vendored compiler
# (falls back to the jsDelivr CDN in the editor when absent).
mkdir -p "$OUT/as"
cp crates/room-web/as/room.ts crates/room-web/as/room_abi.ts "$OUT/as/"
if [[ ! -f target/asc-vendor/asc.js && "${ROOM_WEB_NO_ASC:-0}" != "1" ]] && command -v npm >/dev/null; then
  ./scripts/fetch-assemblyscript.sh || echo "room: AssemblyScript vendoring failed — editor will use the CDN" >&2
fi
if [[ -f target/asc-vendor/asc.js ]]; then
  mkdir -p "$OUT/vendor/asc"
  cp target/asc-vendor/* "$OUT/vendor/asc/"
fi
cp crates/room-web/editor.css "$OUT/editor.css"
# GM SoundFont for web music (same search order as the desktop shell).
for sf in soundfonts/room.sf2 soundfonts/TimGM6mb.sf2 soundfonts/default.sf2 soundfonts/gzdoom.sf2 soundfonts/FluidR3_GM.sf2; do
  if [[ -f "$sf" ]]; then
    cp "$sf" "$OUT/soundfont.sf2"
    break
  fi
done
CACHE_TS="$(date +%s)"
if [[ "$(uname)" == Darwin ]]; then
  sed -i '' "s|./room_web.js|./room_web.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i '' "s|./voice.js|./voice.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i '' "s|./touch.js|./touch.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i '' "s|room_web_bg.wasm|room_web_bg.wasm?v=${CACHE_TS}|g" "$OUT/room_web.js"
  sed -i '' -e "s|./room_web.js|./room_web.js?v=${CACHE_TS}|g" -e "s|./editor-tools.js|./editor-tools.js?v=${CACHE_TS}|g" -e "s|./editor-paint.js|./editor-paint.js?v=${CACHE_TS}|g" -e "s|./editor-scripts.js|./editor-scripts.js?v=${CACHE_TS}|g" "$OUT/editor.js"
  sed -i '' -e "s|./editor.js|./editor.js?v=${CACHE_TS}|g" -e "s|./editor.css|./editor.css?v=${CACHE_TS}|g" "$OUT/editor.html"
else
  sed -i "s|./room_web.js|./room_web.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i "s|./voice.js|./voice.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i "s|./touch.js|./touch.js?v=${CACHE_TS}|g" "$OUT/index.html"
  sed -i "s|room_web_bg.wasm|room_web_bg.wasm?v=${CACHE_TS}|g" "$OUT/room_web.js"
  sed -i -e "s|./room_web.js|./room_web.js?v=${CACHE_TS}|g" -e "s|./editor-tools.js|./editor-tools.js?v=${CACHE_TS}|g" -e "s|./editor-paint.js|./editor-paint.js?v=${CACHE_TS}|g" -e "s|./editor-scripts.js|./editor-scripts.js?v=${CACHE_TS}|g" "$OUT/editor.js"
  sed -i -e "s|./editor.js|./editor.js?v=${CACHE_TS}|g" -e "s|./editor.css|./editor.css?v=${CACHE_TS}|g" "$OUT/editor.html"
fi

# Serve a local Doom / Freedoom IWAD when present (never commit commercial IWADs).
copy_iwad() {
  local src="$1" dest_name="$2"
  if [[ -f "$src" ]]; then
    cp "$src" "$OUT/$dest_name"
    echo "room: serving IWAD as /$dest_name ($(du -h "$OUT/$dest_name" | awk '{print $1}'))"
    return 0
  fi
  return 1
}
IWAD_COPIED=0
if [[ -n "${ROOM_WEB_IWAD:-}" ]] && copy_iwad "$ROOM_WEB_IWAD" "doom1.wad"; then
  IWAD_COPIED=1
elif copy_iwad "$ROOT/testdata/doom/doom1.wad" "doom1.wad"; then
  IWAD_COPIED=1
elif copy_iwad "$ROOT/testdata/freedoom/freedoom1.wad" "freedoom1.wad"; then
  IWAD_COPIED=1
elif copy_iwad "$ROOT/testdata/freedoom/freedoom2.wad" "freedoom1.wad"; then
  # Freedoom Phase 2 is MAP01-style; still playable via MAP01 fallback.
  IWAD_COPIED=1
fi
# Duke Nukem 3D: open http://…/?iwad=duke3d.grp
if copy_iwad "$ROOT/testdata/duke/duke3d.grp" "duke3d.grp"; then
  echo "room: Duke Nukem 3D available at ?iwad=duke3d.grp"
fi
if [[ "$IWAD_COPIED" -eq 0 ]]; then
  echo "room: no local IWAD found — use the file picker in the browser"
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
echo "room: web build ready in $OUT"
echo "room: serving $URL  (Ctrl+C to stop)"
if [[ "$IWAD_COPIED" -eq 1 ]]; then
  echo "  Doom play: IWAD auto-loads; click canvas for mouse look (WASD / E / fire)"
else
  echo "  Doom play: choose an IWAD in the page overlay (or set ROOM_WEB_IWAD=/path/to.wad)"
fi

open_browser() {
  local url="$1"
  if [[ "${ROOM_WEB_NO_OPEN:-0}" == "1" ]]; then
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

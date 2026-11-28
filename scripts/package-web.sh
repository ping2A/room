#!/usr/bin/env bash
# Build a standalone static Room web client (WASM + mods) you can upload to any
# static host (Netlify, GitHub Pages, S3, nginx, …).
#
# Usage:
#   ./scripts/package-web.sh
#   ./scripts/package-web.sh --zip
#   ./scripts/package-web.sh --out dist/site --iwad freedoom --zip
#   ROOM_WEB_IWAD=/path/to/freedoom1.wad ./scripts/package-web.sh --zip
#
# Environment:
#   ROOM_WEB_OUT / --out     output directory (default: dist/room-web)
#   ROOM_WEB_IWAD            path to an IWAD to include (never a commercial
#                            IWAD unless you pass it yourself)
#   ROOM_WEB_DEBUG=1         debug wasm (faster compile, larger files)
#   ROOM_WEB_NO_ASC=1        skip vendoring the AssemblyScript compiler
#   ROOM_WASM_BINDGEN_VER    wasm-bindgen-cli version (default 0.2.129)
#
# The tree is self-contained: open index.html over HTTPS (or any static file
# server). Query params from play-web still work (?mod=survival, ?skill=3, …).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT="${ROOM_WEB_OUT:-dist/room-web}"
BINDGEN_VER="${ROOM_WASM_BINDGEN_VER:-0.2.129}"
PROFILE=release
CARGO_PROFILE_FLAG=(--release)
MAKE_ZIP=0
IWAD_MODE="${ROOM_WEB_IWAD_MODE:-auto}" # auto | none | freedoom | path
IWAD_PATH="${ROOM_WEB_IWAD:-}"

usage() {
  cat <<'EOF'
Build a standalone static Room web client (WASM + mods) for any static host.

  ./scripts/package-web.sh
  ./scripts/package-web.sh --zip
  ./scripts/package-web.sh --out dist/site --iwad freedoom --zip
  ROOM_WEB_IWAD=/path/to/freedoom1.wad ./scripts/package-web.sh --zip

Options:
  --out DIR              output directory (default: dist/room-web)
  --zip                  also write <out>.zip next to the directory
  --iwad none|auto|freedoom|doom|all|PATH
                         IWAD(s) to ship (default: auto = Freedoom if present;
                         doom = testdata/doom/doom1+doom2; all = doom + Freedoom;
                         PATH = single file you point at)
  --debug                debug wasm build
  --no-asc               skip AssemblyScript compiler vendor
  -h, --help             this help

Env: ROOM_WEB_OUT, ROOM_WEB_IWAD, ROOM_WEB_DEBUG, ROOM_WEB_NO_ASC,
     ROOM_WASM_BINDGEN_VER
EOF
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --out)
      OUT="${2:?--out needs a directory}"
      shift 2
      ;;
    --zip)
      MAKE_ZIP=1
      shift
      ;;
    --iwad)
      case "${2:?--iwad needs a value}" in
        none|auto|freedoom|all|doom)
          IWAD_MODE="$2"
          IWAD_PATH=""
          ;;
        *)
          IWAD_MODE=path
          IWAD_PATH="$2"
          ;;
      esac
      shift 2
      ;;
    --debug)
      ROOM_WEB_DEBUG=1
      shift
      ;;
    --no-asc)
      ROOM_WEB_NO_ASC=1
      shift
      ;;
    *)
      echo "unknown option: $1" >&2
      usage 1
      ;;
  esac
done

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
cp crates/room-web/favicon.png "$OUT/favicon.png"
cp crates/room-web/apple-touch-icon.png "$OUT/apple-touch-icon.png"
cp crates/room-web/voice.js "$OUT/voice.js"
cp crates/room-web/rtc.js "$OUT/rtc.js"
cp crates/room-web/netgraph.js "$OUT/netgraph.js"
cp crates/room-web/touch.js "$OUT/touch.js"
cp crates/room-web/settings.js "$OUT/settings.js"
cp crates/room-web/mixer-worklet.js "$OUT/mixer-worklet.js"
cp crates/room-web/editor.html "$OUT/editor.html"
cp crates/room-web/editor.js "$OUT/editor.js"
cp crates/room-web/editor-tools.js "$OUT/editor-tools.js"
cp crates/room-web/editor-paint.js "$OUT/editor-paint.js"
cp crates/room-web/editor-scripts.js "$OUT/editor-scripts.js"
cp crates/room-web/editor-extras.js "$OUT/editor-extras.js"
cp crates/room-web/editor-geom.js "$OUT/editor-geom.js"
cp crates/room-web/editor.css "$OUT/editor.css"
# Face-swap UI (docs/26): editor-paint dynamically imports face-capture.js.
cp crates/room-web/face-capture.js crates/room-web/face-gallery.js \
  crates/room-web/face-share.js crates/room-web/face-worker.js \
  crates/room-web/settings-faces.js crates/room-web/sound-record.js "$OUT/"

# Play mods (survival / bounty / royale). Names must match index.html options.
mkdir -p "$OUT/mods"
MOD_COUNT=0
while IFS= read -r -d '' wad; do
  base="$(basename "$wad")"
  cp "$wad" "$OUT/mods/$base"
  echo "room: mod  /mods/$base"
  MOD_COUNT=$((MOD_COUNT + 1))
done < <(find mods -mindepth 2 -maxdepth 2 -name '*.wad' -print0 2>/dev/null | sort -z)
# Content mods (Color World, Red Noir: pk3/ + room.toml, no code) are packed
# straight into the site, so a fresh checkout ships them without
# `./room start` having built mods/<name>/<name>.wad first.
for dir in mods/*/; do
  dir="${dir%/}"
  [[ -f "$dir/room.toml" && -d "$dir/pk3" && ! -f "$dir/Cargo.toml" ]] || continue
  compgen -G "$dir/*.ts" >/dev/null && continue
  name="$(basename "$dir")"
  [[ -f "$OUT/mods/$name.wad" ]] || MOD_COUNT=$((MOD_COUNT + 1))
  python3 scripts/pack-content-mod.py "$dir" "$OUT/mods/$name.wad"
done

if [[ -f testdata/voxel/cheello_voxels.zip ]]; then
  # Play + editor fetch either name (GZDoom-style .pk3 alias or the upstream zip).
  cp testdata/voxel/cheello_voxels.zip "$OUT/mods/voxel.pk3"
  cp testdata/voxel/cheello_voxels.zip "$OUT/mods/cheello_voxels.zip"
  echo "room: mod  /mods/voxel.pk3 (+ cheello_voxels.zip)"
  MOD_COUNT=$((MOD_COUNT + 1))
fi

# AssemblyScript live mods: game API + optional vendored compiler.
mkdir -p "$OUT/as"
cp crates/room-web/as/room.ts crates/room-web/as/room_abi.ts "$OUT/as/"
if [[ ! -f target/asc-vendor/asc.js && "${ROOM_WEB_NO_ASC:-0}" != "1" ]] && command -v npm >/dev/null; then
  ./scripts/fetch-assemblyscript.sh || echo "room: AssemblyScript vendoring failed — editor will use the CDN" >&2
fi
if [[ -f target/asc-vendor/asc.js ]]; then
  mkdir -p "$OUT/vendor/asc"
  cp target/asc-vendor/* "$OUT/vendor/asc/"
fi

# GM SoundFont for web music (same search order as the desktop shell).
for sf in soundfonts/room.sf2 soundfonts/TimGM6mb.sf2 soundfonts/default.sf2 soundfonts/gzdoom.sf2 soundfonts/FluidR3_GM.sf2; do
  if [[ -f "$sf" ]]; then
    cp "$sf" "$OUT/soundfont.sf2"
    echo "room: soundfont → /soundfont.sf2 ($(du -h "$OUT/soundfont.sf2" | awk '{print $1}'))"
    break
  fi
done

CACHE_TS="$(date +%s)"
sed_inplace() {
  if [[ "$(uname)" == Darwin ]]; then
    sed -i '' "$@"
  else
    sed -i "$@"
  fi
}
sed_inplace "s|./room_web.js|./room_web.js?v=${CACHE_TS}|g" "$OUT/index.html"
sed_inplace "s|./voice.js|./voice.js?v=${CACHE_TS}|g" "$OUT/index.html"
sed_inplace "s|./touch.js|./touch.js?v=${CACHE_TS}|g" "$OUT/index.html"
sed_inplace "s|./settings.js|./settings.js?v=${CACHE_TS}|g" "$OUT/index.html"
sed_inplace "s|room_web_bg.wasm|room_web_bg.wasm?v=${CACHE_TS}|g" "$OUT/room_web.js"
sed_inplace \
  -e "s|./room_web.js|./room_web.js?v=${CACHE_TS}|g" \
  -e "s|./editor-tools.js|./editor-tools.js?v=${CACHE_TS}|g" \
  -e "s|./editor-paint.js|./editor-paint.js?v=${CACHE_TS}|g" \
  -e "s|./editor-scripts.js|./editor-scripts.js?v=${CACHE_TS}|g" \
  -e "s|./editor-extras.js|./editor-extras.js?v=${CACHE_TS}|g" \
  "$OUT/editor.js"
sed_inplace "s|./editor-geom.js|./editor-geom.js?v=${CACHE_TS}|g" "$OUT/editor-extras.js"
sed_inplace "s|./face-capture.js|./face-capture.js?v=${CACHE_TS}|g" "$OUT/editor-paint.js"
sed_inplace "s|./sound-record.js|./sound-record.js?v=${CACHE_TS}|g" "$OUT/editor.js" "$OUT/editor-paint.js"
sed_inplace "s|./settings-faces.js|./settings-faces.js?v=${CACHE_TS}|g" "$OUT/face-share.js"
sed_inplace \
  -e "s|./editor.js|./editor.js?v=${CACHE_TS}|g" \
  -e "s|./editor.css|./editor.css?v=${CACHE_TS}|g" \
  "$OUT/editor.html"

copy_iwad() {
  local src="$1" dest_name="$2"
  if [[ -f "$src" ]]; then
    cp "$src" "$OUT/$dest_name"
    echo "room: IWAD /$dest_name ($(du -h "$OUT/$dest_name" | awk '{print $1}'))"
    return 0
  fi
  return 1
}

IWAD_COPIED=0
copy_doom_pair() {
  local n=0
  if copy_iwad "$ROOT/testdata/doom/doom1.wad" "doom1.wad" \
    || copy_iwad "$ROOT/testdata/doom/DOOM1.WAD" "doom1.wad" \
    || copy_iwad "$ROOT/testdata/doom/doom.wad" "doom1.wad" \
    || copy_iwad "$ROOT/testdata/doom/DOOM.WAD" "doom1.wad"; then
    n=$((n + 1))
  fi
  if copy_iwad "$ROOT/testdata/doom/doom2.wad" "doom2.wad" \
    || copy_iwad "$ROOT/testdata/doom/DOOM2.WAD" "doom2.wad"; then
    n=$((n + 1))
  fi
  [[ "$n" -gt 0 ]]
}
copy_freedoom_pair() {
  local n=0
  if copy_iwad "$ROOT/testdata/freedoom/freedoom1.wad" "freedoom1.wad"; then
    n=$((n + 1))
  fi
  if copy_iwad "$ROOT/testdata/freedoom/freedoom2.wad" "freedoom2.wad"; then
    n=$((n + 1))
  fi
  [[ "$n" -gt 0 ]]
}

case "$IWAD_MODE" in
  none)
    ;;
  path)
    if [[ -z "$IWAD_PATH" ]]; then
      echo "room: --iwad path is empty" >&2
      exit 1
    fi
    # Keep the basename so ?iwad= matches; common Doom/Freedoom names win.
    dest="$(basename "$IWAD_PATH")"
    case "$dest" in
      doom1.wad|DOOM1.WAD|doom.wad|DOOM.WAD) dest=doom1.wad ;;
      doom2.wad|DOOM2.WAD) dest=doom2.wad ;;
      freedoom1.wad|FREEDOOM1.WAD) dest=freedoom1.wad ;;
      freedoom2.wad|FREEDOOM2.WAD) dest=freedoom2.wad ;;
      duke3d.grp|DUKE3D.GRP) dest=duke3d.grp ;;
    esac
    if copy_iwad "$IWAD_PATH" "$dest"; then
      IWAD_COPIED=1
    else
      echo "room: IWAD not found: $IWAD_PATH" >&2
      exit 1
    fi
    ;;
  freedoom)
    if copy_freedoom_pair; then
      IWAD_COPIED=1
    else
      echo "room: Freedoom not found under testdata/freedoom/ (run scripts/fetch-freedoom.sh)" >&2
      exit 1
    fi
    ;;
  doom)
    if copy_doom_pair; then
      IWAD_COPIED=1
    else
      echo "room: doom1.wad / doom2.wad not found under testdata/doom/" >&2
      exit 1
    fi
    ;;
  all)
    # VPS default: ship every free/local IWAD we have + keep voxels via mods/.
    copy_doom_pair && IWAD_COPIED=1
    copy_freedoom_pair && IWAD_COPIED=1
    if [[ "$IWAD_COPIED" -eq 0 ]]; then
      echo "room: --iwad all found no WADs under testdata/doom or testdata/freedoom" >&2
      exit 1
    fi
    ;;
  auto)
    if [[ -n "$IWAD_PATH" ]] && copy_iwad "$IWAD_PATH" "doom1.wad"; then
      IWAD_COPIED=1
    elif copy_doom_pair; then
      IWAD_COPIED=1
    elif copy_freedoom_pair; then
      IWAD_COPIED=1
    fi
    ;;
esac

# Short deploy note shipped with the site.
cat >"$OUT/README.txt" <<EOF
Room web client (standalone)
============================

This folder is a static site. Upload its contents to any HTTPS host that serves
files as-is (GitHub Pages, Netlify, Cloudflare Pages, S3+CloudFront, nginx, …).

Open the site root (index.html). Optional query params:
  ?mod=survival|bounty|royale|voxel
  ?skill=1..5
  ?iwad=doom1.wad|doom2.wad|freedoom1.wad|freedoom2.wad
  ?voxel=0 to disable Cheello voxels (on by default when packaged)

Editor: open editor.html on the same origin. Use IWAD… in the toolbar to
switch the base game (Open… is for PWADs / maps on top).

Notes:
  - Packaged IWADs depend on --iwad (VPS default is all: doom1/doom2 + Freedoom).
  - Mods shipped: ${MOD_COUNT} under mods/ (includes voxel.pk3 when present)
  - Cross-origin isolation is not required for solo play. Coop needs a
    room-server (see deploy/README.md); this package is the play client only.

Built $(date -u +%Y-%m-%dT%H:%MZ) from $(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown).
EOF

# Optional _headers for Netlify / Cloudflare Pages (COOP/COEP off; wasm MIME).
cat >"$OUT/_headers" <<'EOF'
/*
  Cross-Origin-Opener-Policy: same-origin
  Cross-Origin-Embedder-Policy: credentialless

/*.wasm
  Content-Type: application/wasm
  Cache-Control: public, max-age=31536000, immutable

/mods/*
  Cache-Control: public, max-age=3600
EOF

SIZE="$(du -sh "$OUT" | awk '{print $1}')"
echo
echo "room: standalone web site ready → $OUT  ($SIZE, ${MOD_COUNT} mods)"
if [[ "$IWAD_COPIED" -eq 0 ]]; then
  echo "room: no IWAD packaged — visitors pick a file in the browser, or rebuild with --iwad freedoom"
fi

if [[ "$MAKE_ZIP" -eq 1 ]]; then
  ZIP_PATH="$(dirname "$OUT")/$(basename "$OUT").zip"
  rm -f "$ZIP_PATH"
  (cd "$(dirname "$OUT")" && zip -qr "$(basename "$ZIP_PATH")" "$(basename "$OUT")")
  echo "room: archive → $ZIP_PATH ($(du -h "$ZIP_PATH" | awk '{print $1}'))"
fi

echo
echo "Upload tip:"
echo "  rsync -avz --delete ${OUT}/ user@host:/var/www/room/"
echo "  # or unzip the archive into your site root"
echo "Preview locally:"
echo "  python3 -m http.server 8080 --directory ${OUT} --bind 127.0.0.1"

#!/usr/bin/env bash
# Build a VPS deploy tree: room-server binary + web client + mods + control scripts.
# No Rust toolchain needed on the box — rsync dist-vps/ and run with ./room start.
#
#   ./scripts/package-vps.sh
#   ./scripts/package-vps.sh --zip
#   ROOM_VPS_OUT=dist-vps ./scripts/package-vps.sh
#
# Then: ./scripts/push-vps.sh user@host:~/room/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT="${ROOM_VPS_OUT:-dist-vps}"
MAKE_ZIP=0
IWAD_ARG=(--iwad none)
NO_ASC=0

usage() {
  cat <<'EOF'
Build a standalone VPS tree under dist-vps/ (binary + web + mods + ./room).

  ./scripts/package-vps.sh
  ./scripts/package-vps.sh --zip
  ./scripts/package-vps.sh --iwad freedoom

Options:
  --out DIR              output directory (default: dist-vps)
  --zip                  also write <out>.tar.gz
  --iwad none|freedoom|PATH   IWAD packaged into web/ (default: none;
                              commercial IWADs only when you pass a path)
  --no-asc               skip AssemblyScript vendor in the web client
  -h, --help             this help

Push to a VPS with: ./scripts/push-vps.sh user@host:~/room/
EOF
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --out) OUT="${2:?}"; shift 2 ;;
    --zip) MAKE_ZIP=1; shift ;;
    --iwad)
      case "${2:?}" in
        none|freedoom) IWAD_ARG=(--iwad "$2") ;;
        *) IWAD_ARG=(--iwad "$2") ;;
      esac
      shift 2
      ;;
    --no-asc) NO_ASC=1; shift ;;
    *) echo "unknown option: $1" >&2; usage 1 ;;
  esac
done

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 1
  }
}
need_cmd cargo
need_cmd rustup

echo "==> release room-server"
cargo build -p room-server --release

echo "==> web client + mods"
WEB_FLAGS=(--out "$OUT/web" "${IWAD_ARG[@]}")
[[ "$NO_ASC" -eq 1 ]] && WEB_FLAGS+=(--no-asc)
./scripts/package-web.sh "${WEB_FLAGS[@]}"

echo "==> assemble $OUT"
rm -rf "$OUT/bin" "$OUT/mods" "$OUT/data" "$OUT/logs" "$OUT/run"
mkdir -p "$OUT/bin" "$OUT/mods" "$OUT/data" "$OUT/logs" "$OUT/run/pids"

cp target/release/room-server "$OUT/bin/room-server"
chmod +x "$OUT/bin/room-server"
# Strip when available (smaller upload).
if command -v strip >/dev/null 2>&1; then
  strip -S "$OUT/bin/room-server" 2>/dev/null || true
fi

# Mods next to the binary (same layout as Docker / ROOM_MODS_DIR).
if [[ -d "$OUT/web/mods" ]]; then
  cp -R "$OUT/web/mods/." "$OUT/mods/"
fi

cp "$ROOT/scripts/vps/room" "$OUT/room"
chmod +x "$OUT/room"

cat >"$OUT/start.sh" <<'EOF'
#!/usr/bin/env bash
exec "$(cd "$(dirname "$0")" && pwd)/room" start "$@"
EOF
cat >"$OUT/stop.sh" <<'EOF'
#!/usr/bin/env bash
exec "$(cd "$(dirname "$0")" && pwd)/room" stop "$@"
EOF
cat >"$OUT/status.sh" <<'EOF'
#!/usr/bin/env bash
exec "$(cd "$(dirname "$0")" && pwd)/room" status "$@"
EOF
chmod +x "$OUT/start.sh" "$OUT/stop.sh" "$OUT/status.sh"

cp "$ROOT/deploy/Caddyfile" "$OUT/Caddyfile.example" 2>/dev/null || true
cp "$ROOT/scripts/vps/env.example" "$OUT/env.example"
cp "$ROOT/scripts/vps/README.txt" "$OUT/README.txt"

# Stamp
{
  echo "built=$(date -u +%Y-%m-%dT%H:%MZ)"
  echo "commit=$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "host=$(uname -s)-$(uname -m)"
} >"$OUT/BUILD.txt"

SIZE="$(du -sh "$OUT" | awk '{print $1}')"
echo
echo "==> VPS tree ready → $OUT  ($SIZE)"
echo "    Copy env.example → .env, edit secrets, then: ./room start"
echo "    Push: ./scripts/push-vps.sh user@host:~/room/"

if [[ "$MAKE_ZIP" -eq 1 ]]; then
  TAR="$(dirname "$OUT")/$(basename "$OUT").tar.gz"
  rm -f "$TAR"
  tar -C "$(dirname "$OUT")" -czf "$TAR" "$(basename "$OUT")"
  echo "==> archive → $TAR ($(du -h "$TAR" | awk '{print $1}'))"
fi

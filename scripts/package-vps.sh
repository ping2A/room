#!/usr/bin/env bash
# Build a VPS deploy tree: room-server binary + web client + mods + control scripts.
# No Rust toolchain needed on the box — rsync dist-vps/ and run with ./room start.
#
# From macOS/Windows the server binary is cross-compiled to Linux (default
# x86_64-unknown-linux-gnu via cargo-zigbuild). On Linux it builds natively.
#
#   ./scripts/package-vps.sh
#   ./scripts/package-vps.sh --zip
#   ROOM_VPS_TARGET=aarch64-unknown-linux-gnu ./scripts/package-vps.sh
#   ROOM_VPS_OUT=dist-vps ./scripts/package-vps.sh
#
# Then: ./scripts/push-vps.sh user@host:~/room/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT="${ROOM_VPS_OUT:-dist-vps}"
MAKE_ZIP=0
# Default: ship doom1/doom2 + Freedoom + Cheello voxels when present under testdata/.
IWAD_ARG=(--iwad all)
NO_ASC=0
BUILD_MODS=1
SEED=1
# Empty = auto (native on Linux, x86_64-unknown-linux-gnu elsewhere).
TARGET="${ROOM_VPS_TARGET:-}"

usage() {
  cat <<'EOF'
Build a standalone VPS tree under dist-vps/ (Linux binary + web + mods + ./room).

  ./scripts/package-vps.sh
  ./scripts/package-vps.sh --zip
  ./scripts/package-vps.sh --iwad freedoom
  ROOM_VPS_TARGET=aarch64-unknown-linux-gnu ./scripts/package-vps.sh

Options:
  --out DIR              output directory (default: dist-vps)
  --zip                  also write <out>.tar.gz
  --iwad all|doom|freedoom|none|PATH
                         IWADs into web/ (default: all = doom1+doom2+Freedoom;
                         voxels always packaged when testdata/voxel exists)
  --target TRIPLE        Linux rustc target (default: native on Linux,
                              else x86_64-unknown-linux-gnu)
  --no-asc               skip AssemblyScript vendor in the web client
  --no-mods-build        reuse the mods/*/*.wad already built
  --no-seed              ship no game library / presets (seed/)
  -h, --help             this help

Push to a VPS with: ./room push user@host:~/room/
                    ./room push ubuntu@vps:~/boom/
EOF
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --out) OUT="${2:?}"; shift 2 ;;
    --zip) MAKE_ZIP=1; shift ;;
    --target) TARGET="${2:?}"; shift 2 ;;
    --iwad)
      case "${2:?}" in
        none|freedoom|doom|all) IWAD_ARG=(--iwad "$2") ;;
        *) IWAD_ARG=(--iwad "$2") ;;
      esac
      shift 2
      ;;
    --no-asc) NO_ASC=1; shift ;;
    --no-mods-build) BUILD_MODS=0; shift ;;
    --no-seed) SEED=0; shift ;;
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

# Resolve Linux target for the VPS binary (never ship a macOS Mach-O).
HOST_OS="$(uname -s)"
if [[ -z "$TARGET" ]]; then
  if [[ "$HOST_OS" == "Linux" ]]; then
    TARGET="" # native
  else
    TARGET="x86_64-unknown-linux-gnu"
  fi
fi

build_server_native() {
  echo "==> release room-server + room-admin (native)"
  cargo build -p room-server -p room-admin-tui --release
  SERVER_BIN="target/release/room-server"
  ADMIN_BIN="target/release/room-admin"
}

build_server_cross_docker() {
  local triple="$1"
  local platform="linux/amd64"
  case "$triple" in
    aarch64-*|arm64-*) platform="linux/arm64" ;;
    x86_64-*|amd64-*) platform="linux/amd64" ;;
    *)
      echo "package-vps: unsupported --target $triple for Docker build" >&2
      return 1
      ;;
  esac
  echo "    Docker build --platform $platform (deploy/Dockerfile target build-server)"
  docker build --platform "$platform" -f deploy/Dockerfile --target build-server \
    -t room-vps-server:bin "$ROOT"
  local cid
  cid="$(docker create --platform "$platform" room-vps-server:bin)"
  mkdir -p "target/${triple}/release"
  docker cp "$cid:/out/room-server" "target/${triple}/release/room-server"
  docker cp "$cid:/out/room-admin" "target/${triple}/release/room-admin"
  docker rm "$cid" >/dev/null
}

build_server_cross() {
  local triple="$1"
  echo "==> release room-server + room-admin (cross → $triple)"
  rustup target add "$triple" >/dev/null
  # Prefer Docker: ring/aws-lc often break under zig-cc from macOS.
  if command -v docker >/dev/null 2>&1; then
    build_server_cross_docker "$triple"
  elif command -v cargo-zigbuild >/dev/null 2>&1 && command -v zig >/dev/null 2>&1; then
    cargo zigbuild -p room-server -p room-admin-tui --release --target "$triple"
  elif command -v cross >/dev/null 2>&1; then
    cross build -p room-server -p room-admin-tui --release --target "$triple"
  else
    echo "package-vps: need Docker (recommended), cargo-zigbuild, or cross to build $triple from $HOST_OS" >&2
    exit 1
  fi
  SERVER_BIN="target/${triple}/release/room-server"
  ADMIN_BIN="target/${triple}/release/room-admin"
}

SERVER_BIN=""
ADMIN_BIN=""
if [[ -n "$TARGET" ]]; then
  build_server_cross "$TARGET"
else
  build_server_native
fi

if [[ ! -x "$SERVER_BIN" ]]; then
  echo "package-vps: missing binary at $SERVER_BIN" >&2
  exit 1
fi

# Refuse to ship a macOS binary to a Linux VPS.
if command -v file >/dev/null 2>&1; then
  kind="$(file -b "$SERVER_BIN" || true)"
  echo "    binary: $kind"
  if [[ "$kind" == Mach-O* ]]; then
    echo "package-vps: refusing to package a macOS Mach-O for the VPS" >&2
    echo "  re-run with Linux cross-compile (default off-Linux) or ROOM_VPS_TARGET=…" >&2
    exit 1
  fi
  if [[ "$HOST_OS" != "Linux" && "$kind" != ELF* && "$kind" != *ELF* ]]; then
    # file(1) usually says "ELF 64-bit LSB pie executable…"
    if ! echo "$kind" | grep -qi 'ELF'; then
      echo "package-vps: expected an ELF Linux binary, got: $kind" >&2
      exit 1
    fi
  fi
fi

if [[ "$BUILD_MODS" -eq 1 ]]; then
  echo "==> mods (rebuild every mods/<dir>)"
  if ! ./scripts/room-server.sh mods; then
    echo "package-vps: mod build failed — shipping the mods/*/*.wad already built" >&2
  fi
fi

echo "==> web client + mods"
WEB_FLAGS=(--out "$OUT/web" "${IWAD_ARG[@]}")
[[ "$NO_ASC" -eq 1 ]] && WEB_FLAGS+=(--no-asc)
./scripts/package-web.sh "${WEB_FLAGS[@]}"

# "Play solo" content (Freedoom + Brutal Doom) under web/bundled/.
echo "==> solo content (web/bundled)"
./scripts/bundle-solo-content.sh "$OUT/web"

echo "==> assemble $OUT"
# Keep an existing .env (secrets); refresh everything else we own.
rm -rf "$OUT/bin" "$OUT/mods" "$OUT/data" "$OUT/logs" "$OUT/run" "$OUT/seed"
mkdir -p "$OUT/bin" "$OUT/mods" "$OUT/data" "$OUT/logs" "$OUT/run/pids"

cp "$SERVER_BIN" "$OUT/bin/room-server"
chmod +x "$OUT/bin/room-server"
# Strip when available (smaller upload). Host strip may not understand ELF — ignore errors.
if command -v strip >/dev/null 2>&1; then
  strip -S "$OUT/bin/room-server" 2>/dev/null || true
fi
if [[ ! -x "$ADMIN_BIN" ]]; then
  echo "package-vps: missing room-admin at ${ADMIN_BIN:-?} (needed for ./room admin-tui)" >&2
  exit 1
fi
cp "$ADMIN_BIN" "$OUT/bin/room-admin"
chmod +x "$OUT/bin/room-admin"
if command -v strip >/dev/null 2>&1; then
  strip -S "$OUT/bin/room-admin" 2>/dev/null || true
fi

# Mods next to the binary (same layout as Docker / ROOM_MODS_DIR).
if [[ -d "$OUT/web/mods" ]]; then
  cp -R "$OUT/web/mods/." "$OUT/mods/"
fi

# Shipped server library + presets. ./room start passes seed/ to the server,
# which adds missing files to data/content/ and missing presets to
# data/presets.json (admin edits and deletions are kept).
if [[ "$SEED" -eq 1 ]]; then
  echo "==> server library + presets (seed/)"
  mkdir -p "$OUT/seed/content"
  # Hard links keep the tree small (rsync -H keeps them on the VPS).
  seed_file() {
    local src="$1" name="$2"
    [[ -f "$src" ]] || return 1
    ln "$src" "$OUT/seed/content/$name" 2>/dev/null || cp "$src" "$OUT/seed/content/$name"
    echo "room: seed $name ($(du -h "$src" | awk '{print $1}'))"
  }
  # IWADs the web build shipped (follows --iwad).
  for wad in doom1.wad doom2.wad freedoom1.wad freedoom2.wad; do
    seed_file "$OUT/web/$wad" "$wad" || true
  done
  if [[ -f "$OUT/seed/content/freedoom2.wad" ]]; then
    seed_file "$ROOT/testdata/freedoom/freedm.wad" freedm.wad || true
  fi
  # Brutal Doom + Room's Rust ZScript port (same files as web/bundled).
  BRUTAL_NAME=""
  if [[ -f "$OUT/web/bundled/brutal.pk3" ]]; then
    BRUTAL_NAME="$(basename "${ROOM_BRUTALDOOM_PK3:-brutal.pk3}")"
    for c in "$ROOT"/testdata/brutal*.pk3 "$ROOT"/testdata/*/brutal*.pk3; do
      [[ -z "${ROOM_BRUTALDOOM_PK3:-}" && -f "$c" ]] && { BRUTAL_NAME="$(basename "$c")"; break; }
    done
    seed_file "$OUT/web/bundled/brutal.pk3" "$BRUTAL_NAME" || BRUTAL_NAME=""
    seed_file "$OUT/web/bundled/brutal-zscript.wad" brutal-zscript.wad || true
  fi
  # Duke Nukem 3D (commercial: only with --iwad all|doom, from testdata/duke).
  if [[ "${IWAD_ARG[1]}" == all || "${IWAD_ARG[1]}" == doom ]]; then
    for g in "${ROOM_DUKE_GRP:-}" "$ROOT/testdata/duke/duke3d.grp" "$ROOT/testdata/duke/DUKE3D.GRP"; do
      [[ -n "$g" ]] && seed_file "$g" duke3d.grp && break
    done
  fi
  python3 "$ROOT/scripts/vps/seed-presets.py" "$OUT/seed" "$OUT/mods" "$BRUTAL_NAME"
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
  echo "target=${TARGET:-native}"
} >"$OUT/BUILD.txt"

SIZE="$(du -sh "$OUT" | awk '{print $1}')"
echo
echo "==> VPS tree ready → $OUT  ($SIZE)"
echo "    target=${TARGET:-native}"
echo "    bins: room-server + room-admin (./room admin-tui)"
echo "    Copy env.example → .env (first install), then: ./room start"
echo "    Push: ./room push user@host:~/room/"

if [[ "$MAKE_ZIP" -eq 1 ]]; then
  TAR="$(dirname "$OUT")/$(basename "$OUT").tar.gz"
  rm -f "$TAR"
  tar -C "$(dirname "$OUT")" -czf "$TAR" "$(basename "$OUT")"
  echo "==> archive → $TAR ($(du -h "$TAR" | awk '{print $1}'))"
fi

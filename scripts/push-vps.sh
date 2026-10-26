#!/usr/bin/env bash
# Rsync dist-vps/ to a remote Room install without wiping operator data.
#
#   ./scripts/push-vps.sh
#   ./scripts/push-vps.sh ubuntu@YOUR_VPS:~/room/
#   ROOM_VPS_RSYNC_TARGET=ubuntu@vps:~/room/ ./scripts/push-vps.sh
#
# Preserved on the remote (--delete will not remove):
#   data/   admins, bans, audit, mods.json
#   logs/   service logs
#   .env    secrets and production config
#   run/    pid files
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT="${ROOM_VPS_OUT:-dist-vps}"

usage() {
  cat <<'EOF'
Push dist-vps/ to a remote Room install.

Usage:
  ./scripts/push-vps.sh [user@host:path] [rsync options…]

Target:
  First argument, or ROOM_VPS_RSYNC_TARGET (e.g. ubuntu@203.0.113.10:~/room/)

Preserved on the remote:
  data/  logs/  .env  run/

Examples:
  export ROOM_VPS_RSYNC_TARGET=ubuntu@203.0.113.10:~/room/
  ./scripts/package-vps.sh
  ./scripts/push-vps.sh

  ./scripts/push-vps.sh ubuntu@vps:~/room/ --progress

After push, on the VPS:
  cd ~/room && ./room restart
EOF
}

normalize_dest() {
  local dest="$1"
  [[ "$dest" == */ ]] || dest="${dest}/"
  printf '%s' "$dest"
}

target="${ROOM_VPS_RSYNC_TARGET:-}"
if [[ $# -ge 1 && "$1" != -* ]]; then
  target="$1"
  shift
fi

case "${target}" in
  ""|-h|--help|help)
    usage
    [[ -z "$target" ]] && exit 1
    exit 0
    ;;
esac

if [[ ! -x "$OUT/bin/room-server" ]]; then
  echo "push: $OUT is missing or incomplete — run: ./scripts/package-vps.sh" >&2
  exit 1
fi
if ! command -v rsync >/dev/null 2>&1; then
  echo "push: rsync is required" >&2
  exit 1
fi

target="$(normalize_dest "$target")"
excludes=(
  --exclude 'data/'
  --exclude 'logs/'
  --exclude '.env'
  --exclude 'run/'
)

echo "==> rsync $OUT/ → $target"
echo "    (keeping remote data/, logs/, .env, run/)"
rsync -avz --delete "${excludes[@]}" "$OUT/" "$target" "$@"
echo "==> done. On the VPS: cd to the tree and run ./room restart"

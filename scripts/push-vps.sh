#!/usr/bin/env bash
# Rsync dist-vps/ to a remote Room install without wiping operator data.
#
#   ./room push
#   ./room push ubuntu@YOUR_VPS:~/room/
#   ./room push vps ubuntu@YOUR_VPS:~/boom/
#   ROOM_VPS_RSYNC_TARGET=ubuntu@vps:~/room/ ./room push
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
  ./room push [vps] [user@host:path] [rsync options…]
  ./scripts/push-vps.sh [vps] [user@host:path] [rsync options…]

Target:
  Positional path, or ROOM_VPS_RSYNC_TARGET
  (e.g. ubuntu@203.0.113.10:~/room/  or  ubuntu@vps:~/boom/)

  The optional leading "vps" token matches Boom's ./boom.sh push vps …
  and is ignored otherwise.

Preserved on the remote:
  data/  logs/  .env  run/

Examples:
  export ROOM_VPS_RSYNC_TARGET=ubuntu@203.0.113.10:~/room/
  ./room package
  ./room push

  ./room push ubuntu@vps:~/room/
  ./room push ubuntu@vps:~/boom/
  ./room push vps ubuntu@vps:~/boom/ --progress

After push, on the VPS:
  cd ~/room   # (or ~/boom/, whichever you pushed to)
  ./room restart
EOF
}

normalize_dest() {
  local dest="$1"
  [[ "$dest" == */ ]] || dest="${dest}/"
  printf '%s' "$dest"
}

# Optional Boom-compatible "vps" token: ./room push vps user@host:path
if [[ "${1:-}" == "vps" ]]; then
  shift
fi

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
if [[ $# -gt 0 ]]; then
  rsync -avz --delete "${excludes[@]}" "$OUT/" "$target" "$@"
else
  rsync -avz --delete "${excludes[@]}" "$OUT/" "$target"
fi
echo "==> done. On the VPS: cd to the tree and run ./room restart"

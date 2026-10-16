#!/usr/bin/env bash
# Build the room-server Docker image (single- or multi-arch via buildx).
#
# Examples:
#   ./scripts/docker-build.sh
#       → build for the local arch and load into the local Docker engine
#
#   ./scripts/docker-build.sh --platforms linux/amd64,linux/arm64 --tag room-server:0.1
#       → multi-arch image in the buildx cache (use --push or --output to export)
#
#   ./scripts/docker-build.sh --platforms linux/amd64,linux/arm64 \
#       --tag ghcr.io/example/room-server:latest --push
#       → build + push a multi-arch manifest list
#
#   ./scripts/docker-build.sh --platforms linux/arm64 --load
#       → cross-build one arch and load it (requires qemu / binfmt)
#
# Flags:
#   --tag NAME          Image tag (default: room-server:latest). Repeatable.
#   --platforms LIST    Comma-separated platforms (default: local OS/arch).
#   --push              Push to the registry (implies multi-platform friendly output).
#   --load              docker load into the local engine (single platform only).
#   --output PATH       Export OCI/docker archive to a directory or .tar.
#   --no-cache          Pass --no-cache to buildx.
#   --builder NAME      buildx builder to use (created if missing).
#   --print             Print the resolved buildx command and exit.
#   -h, --help          This help.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TAGS=()
PLATFORMS=""
PUSH=0
LOAD=0
OUTPUT=""
NO_CACHE=0
BUILDER="${ROOM_DOCKER_BUILDER:-room-multiarch}"
PRINT=0

usage() {
  sed -n '2,28p' "$0" | sed 's/^# \?//'
}

local_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64) arch=amd64 ;;
    aarch64|arm64) arch=arm64 ;;
    armv7l|armhf) arch=arm/v7 ;;
    *) arch="$arch" ;;
  esac
  case "$os" in
    linux) echo "linux/${arch}" ;;
    darwin)
      # Docker Desktop builds linux images even on macOS hosts.
      echo "linux/${arch}"
      ;;
    *) echo "linux/${arch}" ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag|-t)
      TAGS+=("$2"); shift 2 ;;
    --tag=*)
      TAGS+=("${1#*=}"); shift ;;
    --platforms|--platform)
      PLATFORMS="$2"; shift 2 ;;
    --platforms=*|--platform=*)
      PLATFORMS="${1#*=}"; shift ;;
    --push)
      PUSH=1; shift ;;
    --load)
      LOAD=1; shift ;;
    --output)
      OUTPUT="$2"; shift 2 ;;
    --output=*)
      OUTPUT="${1#*=}"; shift ;;
    --no-cache)
      NO_CACHE=1; shift ;;
    --builder)
      BUILDER="$2"; shift 2 ;;
    --print)
      PRINT=1; shift ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "docker-build: unknown argument: $1" >&2
      usage >&2
      exit 2 ;;
  esac
done

if [[ ${#TAGS[@]} -eq 0 ]]; then
  TAGS=("room-server:latest")
fi
if [[ -z "$PLATFORMS" ]]; then
  PLATFORMS="$(local_platform)"
fi

# --load only supports a single platform.
platform_count="$(awk -F, '{print NF}' <<<"$PLATFORMS")"
if [[ "$LOAD" -eq 1 && "$platform_count" -gt 1 ]]; then
  echo "docker-build: --load requires a single --platforms value (got: ${PLATFORMS})" >&2
  echo "  tip: omit --load and use --push for multi-arch, or pick one platform." >&2
  exit 2
fi
if [[ "$PUSH" -eq 1 && "$LOAD" -eq 1 ]]; then
  echo "docker-build: use either --push or --load, not both" >&2
  exit 2
fi
if [[ "$PUSH" -eq 0 && "$LOAD" -eq 0 && -z "$OUTPUT" ]]; then
  # Default: load locally when single-platform, otherwise require an explicit sink.
  # (--print skips this requirement so you can inspect multi-arch commands.)
  if [[ "$platform_count" -eq 1 ]]; then
    LOAD=1
  elif [[ "$PRINT" -eq 0 ]]; then
    echo "docker-build: multi-arch build needs --push or --output DIR" >&2
    exit 2
  fi
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker-build: docker not found on PATH" >&2
  exit 1
fi

TAG_ARGS=()
for t in "${TAGS[@]}"; do
  TAG_ARGS+=(--tag "$t")
done

CMD=(
  docker buildx build
  --file deploy/Dockerfile
  --platform "$PLATFORMS"
  "${TAG_ARGS[@]}"
  --progress=plain
)

if [[ "$NO_CACHE" -eq 1 ]]; then
  CMD+=(--no-cache)
fi
if [[ "$PUSH" -eq 1 ]]; then
  CMD+=(--push)
elif [[ -n "$OUTPUT" ]]; then
  CMD+=(--output "type=oci,dest=${OUTPUT}")
elif [[ "$LOAD" -eq 1 ]]; then
  CMD+=(--load)
fi

CMD+=("$ROOT")

echo "docker-build: platforms=${PLATFORMS}"
echo "docker-build: tags=${TAGS[*]}"
if [[ "$PRINT" -eq 1 ]]; then
  printf ' %q' "${CMD[@]}"
  echo
  exit 0
fi

# Ensure a buildx builder that can cross-compile exists.
ensure_builder() {
  if docker buildx inspect "$BUILDER" >/dev/null 2>&1; then
    docker buildx use "$BUILDER" >/dev/null
    return 0
  fi
  echo "docker-build: creating buildx builder '${BUILDER}' (docker-container driver)"
  docker buildx create --name "$BUILDER" --driver docker-container --use >/dev/null
  docker buildx inspect --bootstrap >/dev/null
}

ensure_builder

# The image ships mods/*/*.wad from the build context.
./scripts/room-server.sh mods

"${CMD[@]}"

echo
echo "docker-build: done"
if [[ "$LOAD" -eq 1 ]]; then
  echo "  run:  docker run --rm -p 8765:8765 -p 8766:8766 ${TAGS[0]}"
  echo "  or:   docker compose -f deploy/docker-compose.yml up"
fi
if [[ "$PUSH" -eq 1 ]]; then
  echo "  pushed: ${TAGS[*]}"
fi

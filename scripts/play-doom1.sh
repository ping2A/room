#!/usr/bin/env bash
# Doom 1 shortcut: testdata/doom/doom1.wad, map E1M1.
# Same arguments as play-doom.sh (--map, --file, --skill, --title, …).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
exec "$ROOT/play-doom.sh" --game doom1 "$@"

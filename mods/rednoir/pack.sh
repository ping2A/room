#!/usr/bin/env bash
# Builds rednoir.pk3 (a plain zip of pk3/) next to this script.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/rednoir.pk3"
rm -f "$out"
cd "$here/pk3"
zip -q -r -X "$out" GLDEFS CVARINFO MAPINFO MENUDEF ZSCRIPT shaders
echo "wrote $out"

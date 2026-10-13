#!/usr/bin/env bash
# Download a small freely redistributable GM SoundFont for SoftFont music.
# TimGM6mb — public domain / free for any use (Timidity++ GM bank, ~5.8 MB).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/soundfonts"
mkdir -p "$OUT"
DEST="$OUT/TimGM6mb.sf2"
if [[ -f "$DEST" ]]; then
  ls -la "$DEST"
  exit 0
fi
# Debian-packaged TimGM6mb (same bytes as the classic Timidity bank).
URL="https://deb.debian.org/debian/pool/main/t/timgm6mb-soundfont/timgm6mb-soundfont_1.3.orig.tar.gz"
TMP="$OUT/timgm6mb.tgz"
echo "fetching TimGM6mb SoundFont…"
curl -fsSL -o "$TMP" "$URL"
tar -xzf "$TMP" -C "$OUT"
# Tarball layout varies; find the .sf2
SF=$(find "$OUT" -name 'TimGM6mb.sf2' -type f | head -1)
if [[ -z "$SF" ]]; then
  echo "SoundFont not found inside archive" >&2
  exit 1
fi
if [[ "$SF" != "$DEST" ]]; then
  mv -f "$SF" "$DEST"
fi
rm -f "$TMP"
# Drop any empty extract dirs
find "$OUT" -mindepth 1 -type d -empty -delete 2>/dev/null || true
ls -la "$DEST"
echo "SoftFont ready — room will pick it up from soundfonts/TimGM6mb.sf2"

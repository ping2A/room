#!/usr/bin/env bash
# Put the default "Play solo" content next to a room-web build:
#   <out>/bundled/freedoom1.wad, freedoom2.wad     (Freedoom, BSD-3-Clause)
#   <out>/bundled/brutal.pk3 + brutal-zscript.wad  (Brutal Doom + Room's Rust ZScript port)
#   <out>/bundled/manifest.json                    (presets + map lists, read by the lobby)
#
# Sources (first match wins):
#   Freedoom     testdata/freedoom/*.wad          (./scripts/fetch-freedoom.sh)
#   Brutal Doom  $ROOM_BRUTALDOOM_PK3, else testdata/brutal*.pk3
#   Rust ZScript mods/brutal-zscript-rs/brutal-zscript.wad (its build.sh)
#
# Missing pieces are skipped; the lobby then only lists the presets it can play.
# Usage: scripts/bundle-solo-content.sh <room-web out dir>
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:?usage: bundle-solo-content.sh <out dir>}"
DEST="$OUT/bundled"
mkdir -p "$DEST"

# Hard-link when possible (the Brutal pk3 is ~160 MB), copy otherwise.
place() {
  local src="$1" dst="$2"
  rm -f "$dst"
  ln "$src" "$dst" 2>/dev/null || cp "$src" "$dst"
}

FD="$ROOT/testdata/freedoom"
for wad in freedoom1.wad freedoom2.wad; do
  if [[ -f "$FD/$wad" ]]; then place "$FD/$wad" "$DEST/$wad"; else rm -f "$DEST/$wad"; fi
done
if [[ -f "$FD/COPYING.txt" ]]; then
  cp "$FD/COPYING.txt" "$DEST/FREEDOOM-COPYING.txt"
fi

BRUTAL="${ROOM_BRUTALDOOM_PK3:-}"
if [[ -z "$BRUTAL" ]]; then
  for candidate in "$ROOT"/testdata/brutal*.pk3 "$ROOT"/testdata/*/brutal*.pk3; do
    [[ -f "$candidate" ]] && { BRUTAL="$candidate"; break; }
  done
fi
if [[ -n "$BRUTAL" && -f "$BRUTAL" ]]; then
  place "$BRUTAL" "$DEST/brutal.pk3"
  # Ship Brutal Doom's own licence file next to it.
  if command -v unzip >/dev/null 2>&1; then
    unzip -p "$BRUTAL" DDZLICENSE > "$DEST/BRUTALDOOM-LICENSE.txt" 2>/dev/null || rm -f "$DEST/BRUTALDOOM-LICENSE.txt"
  fi
else
  rm -f "$DEST/brutal.pk3" "$DEST/BRUTALDOOM-LICENSE.txt"
fi
ZS="$ROOT/mods/brutal-zscript-rs/brutal-zscript.wad"
if [[ -f "$ZS" ]]; then cp "$ZS" "$DEST/brutal-zscript.wad"; else rm -f "$DEST/brutal-zscript.wad"; fi

python3 - "$DEST" <<'PY'
import json, re, struct, sys, zipfile
from pathlib import Path

dest = Path(sys.argv[1])
MAP = re.compile(r"^(E\dM\d|MAP\d\d)$", re.I)

def wad_maps(data):
    if len(data) < 12 or data[:4] not in (b"IWAD", b"PWAD"):
        return []
    n, off = struct.unpack_from("<ii", data, 4)
    names = []
    for i in range(n):
        name = data[off + i * 16 + 8: off + i * 16 + 16].split(b"\0")[0].decode("latin-1").upper()
        if MAP.match(name):
            names.append(name)
    return names

def file_maps(path):
    if path.suffix.lower() == ".wad":
        return wad_maps(path.read_bytes())
    maps = []
    try:
        with zipfile.ZipFile(path) as z:
            for name in z.namelist():
                m = re.match(r"^maps/([^/]+)\.wad$", name, re.I)
                if m and MAP.match(m.group(1)):
                    maps.append(m.group(1).upper())
    except zipfile.BadZipFile:
        pass
    return maps

files = {}
for p in sorted(dest.iterdir()):
    if p.suffix.lower() in (".wad", ".pk3"):
        st = p.stat()
        files[p.name] = {
            "url": f"bundled/{p.name}?v={st.st_size:x}-{int(st.st_mtime):x}",
            "bytes": st.st_size,
            "maps": file_maps(p),
        }

def preset(pid, label, iwad, addons, note):
    if iwad not in files or any(a not in files for a in addons):
        return None
    maps = list(dict.fromkeys(files[iwad]["maps"] + [m for a in addons for m in files[a]["maps"]]))
    return {
        "id": pid, "label": label, "note": note,
        "iwad": {"name": iwad, **files[iwad]},
        "addons": [{"name": a, **files[a]} for a in addons],
        "maps": maps,
        "bytes": files[iwad]["bytes"] + sum(files[a]["bytes"] for a in addons),
    }

brutal = ["brutal.pk3"] + (["brutal-zscript.wad"] if "brutal-zscript.wad" in files else [])
presets = [p for p in [
    preset("freedoom2-brutal", "Freedoom: Phase 2 + Brutal Doom", "freedoom2.wad", brutal,
           "Free Doom II-compatible IWAD with Brutal Doom on top."),
    preset("freedoom1-brutal", "Freedoom: Phase 1 + Brutal Doom", "freedoom1.wad", brutal,
           "Episodic Freedoom with Brutal Doom on top."),
    preset("freedoom2", "Freedoom: Phase 2", "freedoom2.wad", [], "Free Doom II-compatible IWAD."),
    preset("freedoom1", "Freedoom: Phase 1", "freedoom1.wad", [], "Free episodic Doom-compatible IWAD."),
] if p]
if presets:
    presets[0]["default"] = True
(dest / "manifest.json").write_text(json.dumps({"presets": presets}, indent=1))
for p in presets:
    print(f"room: bundled preset {p['id']}: {len(p['maps'])} maps, {p['bytes'] / 1048576:.1f} MB")
if not presets:
    print("room: no bundled solo content (run ./scripts/fetch-freedoom.sh)")
PY

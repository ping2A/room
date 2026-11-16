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
#   Duke Atomic  $ROOM_DUKE3D_GRP (user-supplied; never committed)
#   Brutal Duke  $ROOM_BRUTAL_DUKE_DIR (directory or ZIP/PK3; user-supplied)
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

DUKE_GRP="${ROOM_DUKE3D_GRP:-}"
if [[ -n "$DUKE_GRP" && -f "$DUKE_GRP" ]]; then
  place "$DUKE_GRP" "$DEST/duke3d.grp"
else
  rm -f "$DEST/duke3d.grp"
fi

python3 - "$DEST" "${ROOM_BRUTAL_DUKE_DIR:-}" <<'PY'
import json, os, re, shutil, struct, sys, zipfile
from pathlib import Path

dest = Path(sys.argv[1])
duke_source = Path(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[2] else None
MAP = re.compile(r"^(?:E\d(?:M|L)\d+|MAP\d+)$", re.I)
# Keep the browser bundle limited to files the current CON/DEF renderer path
# consumes. In particular, don't eagerly ship large music archives alongside
# the image assets; loose mod audio needs its own lazy audio-bank loader.
WEB_ASSET_SUFFIXES = {".con", ".def", ".png", ".bmp", ".jpg", ".jpeg", ".kvx", ".mhk", ".pal", ".raw"}

def is_web_asset(path):
    return Path(path).suffix.lower() in WEB_ASSET_SUFFIXES

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
    if path.suffix.lower() == ".grp":
        data = path.read_bytes()
        if len(data) < 16 or data[:12] != b"KenSilverman":
            return []
        count = struct.unpack_from("<I", data, 12)[0]
        if count > (len(data) - 16) // 16:
            return []
        names, at = [], 16 + count * 16
        for i in range(count):
            entry = data[16 + i * 16:32 + i * 16]
            name = entry[:12].split(b"\0")[0].decode("ascii", "ignore").upper()
            size = struct.unpack_from("<I", entry, 12)[0]
            if at + size > len(data):
                return []
            at += size
            if name.endswith(".MAP") and MAP.match(name[:-4]):
                names.append(name[:-4])
        return list(dict.fromkeys(names))
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
    if p.is_file() and p.suffix.lower() in (".wad", ".pk3", ".grp"):
        st = p.stat()
        files[p.name] = {
            "url": f"bundled/{p.name}?v={st.st_size:x}-{int(st.st_mtime):x}",
            "bytes": st.st_size,
            "maps": file_maps(p),
        }

duke_mod = None
mod_dir = dest / "brutal-duke"
if mod_dir.exists():
    shutil.rmtree(mod_dir)
mod_manifest_path = dest / "brutal-duke-manifest.json"
mod_manifest_path.unlink(missing_ok=True)
(dest / "brutal-duke-resources.rmod").unlink(missing_ok=True)
if duke_source and duke_source.exists():
    mod_dir.mkdir(parents=True)
    if duke_source.is_dir():
        source_root = duke_source
        # A checkout may point at a release parent containing one `brutal/` dir.
        if not (source_root / "eduke.con").is_file() and (source_root / "brutal" / "eduke.con").is_file():
            source_root = source_root / "brutal"
        for p in source_root.rglob("*"):
            if p.is_file() and not p.is_symlink() and p.name != ".DS_Store" and is_web_asset(p.name):
                relative = p.relative_to(source_root)
                target = mod_dir / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(p, target)
    elif zipfile.is_zipfile(duke_source):
        with zipfile.ZipFile(duke_source) as z:
            members = [n for n in z.namelist() if n and not n.endswith("/") and not n.startswith("__MACOSX/") and not n.endswith("/.DS_Store") and is_web_asset(n)]
            root_candidates = [n.rsplit("/", 1)[0] for n in members if n.lower().endswith("/eduke.con")]
            prefix = min(root_candidates, key=len) + "/" if root_candidates else ""
            for name in members:
                if prefix and not name.startswith(prefix):
                    continue
                relative = (name[len(prefix):] if prefix else name).replace("\\", "/")
                parts = Path(relative).parts
                if not relative or Path(relative).is_absolute() or ".." in parts:
                    continue
                target = mod_dir / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(z.read(name))
    else:
        raise SystemExit(f"ROOM_BRUTAL_DUKE_DIR must name a directory or ZIP/PK3: {duke_source}")
    # The release tree contains many loose screenshots, source previews and
    # alternate images that no DEF references. Keep every script/definition
    # and map-hack file, but only ship images that a DEF actually names. This
    # shrinks the browser download and avoids copying/decoding unrelated art.
    image_suffixes = {".png", ".bmp", ".jpg", ".jpeg", ".kvx"}
    referenced_images = set()
    for definition in mod_dir.rglob("*.def"):
        source = definition.read_text(encoding="utf-8", errors="replace")
        for reference in re.findall(r'"([^"\r\n]+)"', source):
            normalized = reference.replace("\\", "/").lower()
            if Path(normalized).suffix.lower() in image_suffixes:
                referenced_images.add(normalized)
    for asset in mod_dir.rglob("*"):
        if asset.is_file() and asset.suffix.lower() in image_suffixes:
            normalized = asset.relative_to(mod_dir).as_posix().lower()
            if normalized not in referenced_images:
                asset.unlink()
    packaged = sorted(p for p in mod_dir.rglob("*") if p.is_file() and p.name != ".DS_Store")
    if packaged and any(p.name.lower() == "eduke.con" for p in packaged):
        if len(packaged) > 4096:
            raise SystemExit(f"Brutal Duke web bundle has too many files ({len(packaged)} > 4096)")
        bundle_path = dest / "brutal-duke-resources.rmod"
        bundle_tmp = bundle_path.with_suffix(".rmod.tmp")
        source_bytes = sum(p.stat().st_size for p in packaged)
        if source_bytes > 256 * 1024 * 1024:
            raise SystemExit(f"Brutal Duke web bundle is too large ({source_bytes} > 256 MiB)")
        with bundle_tmp.open("wb") as output:
            output.write(b"RDUKMOD1")
            output.write(struct.pack("<I", len(packaged)))
            for p in packaged:
                relative = p.relative_to(mod_dir).as_posix()
                path_bytes = relative.encode("utf-8")
                file_bytes = p.stat().st_size
                if not path_bytes or len(path_bytes) > 65535 or file_bytes > 0xffffffff:
                    raise SystemExit(f"Brutal Duke web asset cannot be packed: {relative}")
                output.write(struct.pack("<HI", len(path_bytes), file_bytes))
                output.write(path_bytes)
                with p.open("rb") as source:
                    shutil.copyfileobj(source, output, length=1024 * 1024)
        bundle_tmp.replace(bundle_path)
        bundle_stat = bundle_path.stat()
        mod_manifest = {
            "bundle": {
                "url": "bundled/brutal-duke-resources.rmod" + f"?v={bundle_stat.st_size:x}-{int(bundle_stat.st_mtime):x}",
                "bytes": bundle_stat.st_size,
                "files": len(packaged),
            },
            "bytes": bundle_stat.st_size,
        }
        mod_path = dest / "brutal-duke-manifest.json"
        mod_path.write_text(json.dumps(mod_manifest, separators=(",", ":")))
        st = mod_path.stat()
        duke_mod = {
            "url": f"bundled/brutal-duke-manifest.json?v={st.st_size:x}-{int(st.st_mtime):x}",
            "bytes": bundle_stat.st_size,
        }
        shutil.rmtree(mod_dir)
    else:
        shutil.rmtree(mod_dir)
        print("room: Brutal Duke source has no eduke.con; skipping web preset")

def preset(pid, label, iwad, addons, note, game="doom", duke_mod=None):
    if iwad not in files or any(a not in files for a in addons):
        return None
    maps = list(dict.fromkeys(files[iwad]["maps"] + [m for a in addons for m in files[a]["maps"]]))
    result = {
        "id": pid, "label": label, "note": note,
        "iwad": {"name": iwad, **files[iwad]},
        "addons": [{"name": a, **files[a]} for a in addons],
        "maps": maps,
        "bytes": files[iwad]["bytes"] + sum(files[a]["bytes"] for a in addons),
        "game": game,
    }
    if duke_mod:
        result["duke_mod"] = duke_mod
        result["bytes"] += duke_mod["bytes"]
    return result

brutal = ["brutal.pk3"] + (["brutal-zscript.wad"] if "brutal-zscript.wad" in files else [])
presets = [p for p in [
    preset("freedoom2-brutal", "Freedoom: Phase 2 + Brutal Doom", "freedoom2.wad", brutal,
           "Free Doom II-compatible IWAD with Brutal Doom on top."),
    preset("freedoom1-brutal", "Freedoom: Phase 1 + Brutal Doom", "freedoom1.wad", brutal,
           "Episodic Freedoom with Brutal Doom on top."),
    preset("freedoom2", "Freedoom: Phase 2", "freedoom2.wad", [], "Free Doom II-compatible IWAD."),
    preset("freedoom1", "Freedoom: Phase 1", "freedoom1.wad", [], "Free episodic Doom-compatible IWAD."),
    preset("duke-brutal", "Duke Nukem 3D + Brutal Duke", "duke3d.grp", [],
           "Uses the supplied Atomic GRP and downloads one path-preserving Brutal Duke resource bundle, cached by the browser.",
           game="duke3d", duke_mod=duke_mod) if duke_mod else None,
] if p]
if presets:
    presets[0]["default"] = True
(dest / "manifest.json").write_text(json.dumps({"presets": presets}, indent=1))
for p in presets:
    print(f"room: bundled preset {p['id']}: {len(p['maps'])} maps, {p['bytes'] / 1048576:.1f} MB")
if not presets:
    print("room: no bundled solo content (run ./scripts/fetch-freedoom.sh or set Duke bundle inputs)")
PY

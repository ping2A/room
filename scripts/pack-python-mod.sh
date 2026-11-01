#!/usr/bin/env bash
# Pack a desktop Python RoomMod directory into a PWAD (ROOMMOD + source lump).
# Usage: ./scripts/pack-python-mod.sh [mods/doom1-python-hello]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
MOD_DIR="${1:-mods/doom1-python-hello}"
MOD_DIR="${MOD_DIR%/}"
if [[ ! -f "$MOD_DIR/room.toml" ]]; then
  echo "missing $MOD_DIR/room.toml" >&2
  exit 1
fi

python3 - "$MOD_DIR" <<'EOF'
import pathlib, struct, sys, tomllib

d = pathlib.Path(sys.argv[1])
raw = (d / "room.toml").read_bytes()
meta = tomllib.loads(raw.decode())
modules = meta.get("module", [])
if not modules:
    sys.exit(f"{d}/room.toml: no [[module]]")

# Prefer a lump-friendly ROOMMOD (drop edu-only tables).
mod = meta.get("mod", {})
lines = ["[mod]"]
for k in ("id", "name", "version"):
    if k in mod:
        lines.append(f'{k} = "{mod[k]}"')
lines.append(f'abi = {mod.get("abi", 1)}')
lumps_src = []
for m in modules:
    file = m["file"]
    name = m.get("name", pathlib.Path(file).stem)
    lang = m.get("language", m.get("lang", "python"))
    lines += [
        "",
        "[[module]]",
        f'file = "{file}"',
        f'name = "{name}"',
        f'language = "{lang}"',
        f'scope = "{m.get("scope", "play")}"',
    ]
    # Resolve .py next to room.toml
    candidates = [
        d / f"{name}.py",
        d / f"{name}_mod.py",
        d / "hello_mod.py",
        d / f"{file}.py",
        d / file,
    ]
    src = next((p for p in candidates if p.is_file()), None)
    if src is None:
        pys = sorted(d.glob("*.py"))
        src = pys[0] if pys else None
    if src is None:
        sys.exit(f"{d}: no .py source for module {name}")
    lumps_src.append((file, src.read_bytes()))

manifest = ("\n".join(lines) + "\n").encode()
lumps = [("ROOMMOD", manifest), *lumps_src]

data, directory, off = bytearray(), bytearray(), 12
data += b"PWAD"
data += struct.pack("<II", len(lumps), 0)
for name, body in lumps:
    start = len(data)
    data += body
    n = name.encode("ascii")[:8].ljust(8, b"\0")
    directory += struct.pack("<II8s", start, len(body), n)
struct.pack_into("<I", data, 8, len(data))
data += directory
out = d / "hello.wad"
out.write_bytes(data)
print(f"wrote {out} ({out.stat().st_size} bytes, {len(lumps)} lumps)")
EOF

echo "run:"
echo "  cargo run -p room-app --features python-mods -- \\"
echo "    --iwad testdata/doom/doom1.wad --file ${MOD_DIR}/hello.wad --play"

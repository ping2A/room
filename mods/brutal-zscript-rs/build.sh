#!/usr/bin/env bash
# Build the Rust port and pack it as brutal-zscript.wad (ROOMMOD + BDZSCRPT).
# Load it after Brutal Doom: --file brutal-doom.pk3 --file brutal-zscript.wad
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/room_mod_brutal_zscript.wasm brutal-zscript.wasm
python3 - <<'PY'
import struct
lumps = [("ROOMMOD", open("room.toml", "rb").read()),
         ("BDZSCRPT", open("brutal-zscript.wasm", "rb").read())]
data, dir_ = b"", b""
for name, body in lumps:
    dir_ += struct.pack("<ii8s", 12 + len(data), len(body), name.encode())
    data += body
open("brutal-zscript.wad", "wb").write(
    b"PWAD" + struct.pack("<ii", len(lumps), 12 + len(data)) + data + dir_)
PY
echo "brutal-zscript.wad ($(wc -c < brutal-zscript.wad) bytes)"

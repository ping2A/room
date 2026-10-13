# room-map

Map model, Doom/Hexen/UDMF loaders, validation, blockmap, reject, SVG dump,
XLAT hook, and load-time setup tables (planes / portals / polyobjects / 3D floors).

## Public API (M3)

- `load_map` — Doom / Hexen / UDMF
- `map_to_svg` — top-down linedef dump
- `build_blockmap` / `parse_blockmap` / `parse_reject` / `build_reject` (connectivity) / `build_reject_zero`
- `translate_doom_linedef` — Doom 0–141 + Boom generalized (`Generic_*`)
- `build_setup` / `SecPlane` — flat planes; polyobj/portal/3D-floor declarations
- UDMF lexer (`UdmfLexer`) — ≥ 100 MB/s target

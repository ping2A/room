# room-build

Build engine (Duke Nukem 3D) content for Room: GRP, ART, `PALETTE.DAT`,
`.MAP` and the `USER.CON` level table. Native gameplay belongs to `room-duke`;
this crate parses Build data and keeps a compatibility exporter that converts
content into UDMF PWADs for editors and legacy paths. See
`docs/23-build-engine.md`.

Public API:

- `Grp`, `is_grp` — GRP container.
- `TileSet`, `Tile` — ART tiles.
- `Palette` — palette and shade tables → `PLAYPAL` / `COLORMAP`.
- `parse_map`, `BuildMap` — Build maps v7/v8.
- `parse_user_con`, `LevelInfo` — level names, par times, music.
- `export::{convert_map, convert_map_with}` — one Build map → `TEXTMAP`; `convert_map_with` adds game
  tiles (`GameTiles`: switches, activators, hurt floors, charges, the fan) and a
  `SpriteMapper` that turns sprites into things (`MapThing`; monsters
  with a `size` are moved off blocking walls and solid decorations;
  `mover_sound` gives door / lift sectors a `MoverSound`: default, `silent`
  or a UDMF `soundsequence`; `wall_break` names the wall pictures that break
  when shot, as a `WallBreak`).
  Moving sectors, floor doors, C-9 charge groups, master switches and
  breakable walls run from an ACS `BEHAVIOR` lump (`ConvertedMap::behavior`);
  `WATER_PORTAL` is the Room water portal kind.
- `ConData` — data from `DEFS.CON` / `USER.CON` / `GAME.CON` (defines,
  sounds, quotes, actions, actor strengths; no CON VM).
- `Duke`, `DukeContent` — Duke classes (`DECORATE`, `KEYCONF`, `SNDINFO`,
  `SNDSEQ`, sounds) and the `SpriteMapper` for Duke maps.
- `SpriteSet`, `SpriteLump`, `Origin` — ART tiles as Doom sprite/HUD lumps.
- `Lookups` — `LOOKUP.DAT` palette swaps.
- `export::{grp_to_wad, convert_grp_with_maps}` — a whole GRP → PWAD bytes (maps, textures, sprites, HUD,
  sounds, composed skies, `MAPINFO`).
- `mount::mount_path`, `mount::mount_bytes` — mount a Doom IWAD or a GRP on a
  `VfsBuilder`.

Every parser returns `Result` and never panics on corrupt input.

# 04 — Binary formats (`room-formats`)

`room-formats` contains pure parsers: `&[u8] → Result<T>`. No VFS, no
engine types beyond `room-core`. All integers are little-endian unless
stated. Every parser has a fuzz target.

## 1. Map lumps

A classic map is a marker lump (`MAP01`, `E1M1`, any name) followed by lumps in
fixed order. Detection:

| Following lumps | Format |
|---|---|
| `TEXTMAP` … `ENDMAP` | UDMF (`06-maps-geometry.md`) |
| `THINGS LINEDEFS SIDEDEFS VERTEXES SEGS SSECTORS NODES SECTORS REJECT BLOCKMAP` + `BEHAVIOR` | Hexen format |
| same without `BEHAVIOR` | Doom format |
| optional trailing: `SCRIPTS` (ACS source, ignored), `GL_*` lumps, `ZNODES`, `DIALOGUE`, `LIGHTMAP` (C) | |

Lumps may be missing (SEGS/SSECTORS/NODES/REJECT/BLOCKMAP); the loader rebuilds.

### 1.1 Doom format records

```
THINGS   (10 bytes): i16 x, i16 y, i16 angle, i16 type, i16 flags
LINEDEFS (14 bytes): u16 v1, u16 v2, i16 flags, i16 special, i16 tag, u16 front_side, u16 back_side (0xFFFF = none)
SIDEDEFS (30 bytes): i16 xoff, i16 yoff, [u8;8] upper, [u8;8] lower, [u8;8] middle, u16 sector
VERTEXES (4 bytes):  i16 x, i16 y
SEGS     (12 bytes): u16 v1, u16 v2, i16 angle(BAM>>16), u16 line, i16 side, i16 offset
SSECTORS (4 bytes):  u16 numsegs, u16 firstseg
NODES    (28 bytes): i16 x, y, dx, dy, i16 bbox[2][4] (top,bottom,left,right), u16 children[2] (bit15 = subsector)
SECTORS  (26 bytes): i16 floorh, i16 ceilh, [u8;8] floorpic, [u8;8] ceilpic, i16 light, i16 special, i16 tag
REJECT:   bit array, (numsectors^2 + 7)/8 bytes, may be short (pad with zeros = visible)
BLOCKMAP: i16 originx, originy, u16 cols, rows, u16 offsets[cols*rows], then lists: 0x0000, line indices…, 0xFFFF
```

Treat `u16` vertex/side/sector indices as unsigned (limit-removing), and treat
blockmap offsets as `u16` with overflow detection — if the blockmap is
malformed or exceeds 64K words, rebuild it (GZDoom behaviour).

### 1.2 Hexen format differences

```
THINGS   (20 bytes): i16 tid, i16 x, i16 y, i16 z, i16 angle, i16 type, i16 flags, u8 special, u8 args[5]
LINEDEFS (16 bytes): u16 v1, u16 v2, i16 flags, u8 special, u8 args[5], u16 front, u16 back
```

Line activation type lives in flags bits 10–12 (`SPAC_*`), see `09-specials.md`.

### 1.3 Extended/GL node formats

| Signature | Format | Notes |
|---|---|---|
| `XNOD` / `ZNOD` | ZDoom extended normal nodes (Z = zlib-compressed) | 32-bit indices, fixed-point vertices |
| `XGLN` / `ZGLN` | ZDoom GL nodes v1 | |
| `XGL2` / `ZGL2` | GL nodes v2 (32-bit line indices) | |
| `XGL3` / `ZGL3` | GL nodes v3 (fixed-point node partition) | |
| `xNd4\0\0\0\0` | DeePBSP nodes | |
| `gNd2`, `gNd5` in `GL_VERT` + `GL_SEGS/GL_SSECT/GL_NODES` | glBSP v2/v5 | |

Layout of XNOD/XGLN (all little endian):

```
u32 orig_verts, u32 new_verts, (i32 x, i32 y fixed16.16) × new_verts
u32 numsubsectors, u32 segcount × numsubsectors
u32 numsegs,
   XNOD: u32 v1, u32 v2, u16 line, u8 side
   XGLN: u32 v1, u32 partner, u16 line (0xFFFF none), u8 side
   XGL2/3: u32 v1, u32 partner, u32 line (0xFFFFFFFF none), u8 side
u32 numnodes,
   XNOD/XGLN/XGL2: i16 x,y,dx,dy ; XGL3: i32 x,y,dx,dy (fixed)
   i16 bbox[2][4], u32 children[2] (bit31 = subsector)
```

Zlib decompression: `miniz_oxide`/`flate2` (rust backend). Nodes may also live in a separate `ZNODES` lump (UDMF).

GL nodes are required by the renderer (closed subsector polygons). If the map
provides only non-GL nodes, the internal node builder generates GL nodes (`06 §Node building`).

## 2. Graphics

### 2.1 Doom picture ("patch")

```
u16 width, u16 height, i16 leftoffset, i16 topoffset, u32 columnofs[width]
column: sequence of posts: u8 topdelta (0xFF ends column), u8 length, u8 pad, u8 data[length], u8 pad
```

- **Tall patches** (DeePsea): if `topdelta <= previous_topdelta`, it is relative: `topdelta += previous_topdelta`.
- Validation (`CheckIfPatch` in GZDoom): width/height in 1..=2048, every column offset within lump, posts terminate. Lumps failing validation are tried as other formats.

### 2.2 Other image formats

| Format | Detection | Parser |
|---|---|---|
| Flat (raw 8-bit) | size 4096 (64²), 8192 (64×128, Heretic), 16384 (128²), 65536 (256²), 262144 (512²), 1048576 (1024²) | built-in |
| Raw fullscreen | size 64000 (320×200) in `Graphics` namespace or named `TITLE`/`HELP*`/… (Heretic/Hexen) | built-in |
| Autopage | Strife `AUTOPAGE`-style raw 320×N | built-in |
| PNG | signature | `png` crate. Honour chunks `grAb` (i32 BE x,y offsets) and `alPh` (palette-index-0 transparency). 8-bit paletted PNGs keep indices for palette-mode rendering. |
| JPEG | `FF D8 FF` | `zune-jpeg` |
| TGA, PCX, BMP | headers | `image` crate subset (pure Rust) or hand-rolled |
| DDS | `DDS ` | `ddsfile` + BC1–BC7 software decode (`texture2ddecoder`) or upload compressed if the device supports BC |
| WebP | `RIFF....WEBP` | `image-webp` |
| IMGZ | `IMGZ` | ZDoom compressed 8-bit image (RLE) |
| Build ART tiles | in `.art` | C |

All decoders output either `Paletted { w, h, indices, mask }` or `Rgba8 { w, h, pixels }`, plus `(left_offset, top_offset)` and `has_translucency`.

### 2.3 Palettes and lighting tables

| Lump | Format | Use |
|---|---|---|
| `PLAYPAL` | 14 × 768 bytes RGB (Heretic/Hexen/Strife similar counts) | palette 0 is the game palette; others are pain/pickup/radiation flashes (GZDoom draws these as blend overlays instead) |
| `COLORMAP` | 34 × 256 (Doom); 0–31 light levels, 32 invulnerability, 33 black | classic light banding (palette mode) |
| `TRANMAP` (Boom), `TINTTAB` (Heretic/Hexen), `XLATAB` (Strife) | 256×256 index blend tables | translucency in palette mode |
| `FOGMAP` (Hexen) | colormap for fog sectors | |
| Boom colormaps in `C_START/C_END` | 34 × 256 each | sector colormaps via line special 242 (Boom deep water / colormaps) — GZDoom converts to sector colour/fade; do the same |

## 3. Sounds

| Format | Detection | Notes |
|---|---|---|
| DMX digital | `u16 format == 3` | `u16 rate, u32 count`, 8-bit unsigned PCM; DMX pads 16 bytes before/after: if `count >= 32`, skip 16 at start and 16 at end (Chocolate Doom behaviour; GZDoom equivalent) |
| PC speaker (`DP*`) | `u16 format == 0` | `u16 count`, then tone indices (0..127) at 140 Hz; synthesized square wave (`snd_pcspeaker` style) |
| WAV | RIFF/WAVE | PCM 8/16/24/32/float, IMA ADPCM, MS ADPCM |
| OGG Vorbis / Opus | `OggS` | `lewton` (Vorbis), `symphonia` alternatives; Opus: C (`opus` pure-Rust decoder if available) |
| FLAC | `fLaC` | `claxon` or `symphonia` |
| MP3 | ID3 / frame sync | `symphonia`/`minimp3`-free pure Rust (`symphonia-bundle-mp3`) |
| VOC | `Creative Voice File` | built-in |
| Raw 8-bit | SNDINFO `$rawsound` | given rate (default 11025) |

Output: `DecodedSound { rate, channels, samples: Vec<f32> }` (mono preferred for 3D; stereo allowed for 2D/UI sounds). `$singular`/`loop` metadata comes from SNDINFO, plus WAV `smpl` and Vorbis `LOOP_START/LOOP_END` (or `LOOPSTART`) comments for loop points.

## 4. Music

| Format | Detection | Engine |
|---|---|---|
| MUS | `MUS\x1A` | convert to MIDI event stream (MUS→MIDI converter; 140 Hz tick) |
| MIDI | `MThd` (types 0, 1, 2); RIFF `RMID` wrapper | sequencer |
| HMI / HMP | `HMI-MIDISONG061595` / `HMIMIDIP` | sequencer (GZDoom supports both) |
| XMI | `FORM....XDIR` | sequencer |
| MOD/S3M/XM/IT/MTM/669/… | magic per format | tracker engine |
| OGG/FLAC/MP3/WAV/Opus | as sounds | streamed decoder |
| IMF / RAW (Rdos) / DRO | `RAWADATA`, `DBRAWOPL` | OPL register stream → OPL3 emulator |
| VGM/VGZ/SPC/NSF/NSFE/GBS/AY/HES/KSS/SAP/GYM | magic | `gme` feature (C) |

Associated lumps: `GENMIDI` (OPL instrument bank, `#OPL_II#` header, 175 instruments), `DMXGUS`/`DMXGUSC` (GUS patch mapping), SoundFonts (`.sf2`, `.sf3`) in `soundfonts/` dir or user paths. See `14-audio.md`.

## 5. Text-mode screens

`ENDOOM`, `ENDTEXT` (Heretic), `ENDSTRF` (Strife): 4000 bytes = 80×25 × (char, attr) in CP437. Rendered via a built-in VGA 8×16 font (bundled in `room.pk3`) on exit if `showendoom` is on (GZDoom shows it in a separate window; Room shows it fullscreen in-window before quitting).

## 6. Fonts

| Format | Signature | Notes |
|---|---|---|
| FON1 | `FON1` | u16 charwidth, height; RLE (byte run) 8-bit glyphs, 256 chars |
| FON2 | `FON2` | variable width; u8 first, last char, u8 constant-width flag, palette, RLE (PackBits) |
| BMF (ByteMap Font) | `\xE1\xE6\xD5\x1A` | |
| Per-glyph directory | `fonts/<FontName>/<hex codepoint>.png|lmp` | e.g. `0041.lmp`; also Doom's `STCFN065` naming |
| Unifont HEX | `.hex` text | GZDoom uses `newconsolefont.hex`/`newsmallfont` for Unicode fallbacks |
| TrueType/OpenType | `.ttf`/`.otf` | **Room extension** (S): rasterized via `ab_glyph`/`fontdue` into a glyph atlas; declared in FONTDEFS with `TrueType "path" Size n` |

## 7. Voxels and models (parsing only; rendering in `13`)

| Format | Notes |
|---|---|
| KVX (Build voxels) | mip levels, slab data, palette (last 768 bytes) |
| MD2 | Quake 2: frames of compressed vertices, GL commands ignored, triangles + st coords |
| MD3 | Quake 3: surfaces, frames, tags (tags ignored by GZDoom except for attachments — not supported) |
| DMD | Doomsday detail model (MD2 variant with LODs) — S |
| OBJ | Wavefront, triangulated, single frame; `usemtl` ignored (skins from MODELDEF) |
| IQM | Inter-Quake Model: meshes, joints, poses, animations, skeletal skinning |
| UE1 | Unreal 1 vertex mesh: `<name>_d.3d` (data) + `<name>_a.3d` (anim) |

## 8. Binary definition lumps

| Lump | Record | Notes |
|---|---|---|
| `ANIMATED` (Boom) | 23 bytes: i8 type (0 flat, 1 texture, 2 = texture w/ decals allowed, 0xFF end), [u8;9] last, [u8;9] first, i32 speed | merged into ANIMDEFS registry |
| `SWITCHES` (Boom) | 20 bytes: [u8;9] off, [u8;9] on, i16 episode (0 end) | |
| `PNAMES` | i32 count, [u8;8] × count | |
| `TEXTURE1/2` | Doom: i32 count, i32 offsets[]; texture: [u8;8] name, i32 masked(flags), i16 w, i16 h, i32 obsolete, i16 patchcount, patches (i16 x, i16 y, i16 pnames index, i16 stepdir, i16 colormap). Strife variant omits `obsolete` and per-patch `stepdir/colormap` (detect: GZDoom checks whether the offsets/sizes are consistent with the 22-byte or 18-byte header layout). The 32-bit "masked" field is reinterpreted by ZDoom: `u16 flags` (0x8000 = world panning), `u8 scalex`, `u8 scaley` (units of 1/8; 0 = 1.0) | |
| `DIALOGUE`, `SCRIPTxx` | Strife binary conversation records (1516 bytes "teaser" / Strife 1.31 formats) | parsed into the same IR as USDF |
| `DEMO*` | demo lumps (§9) | |
| `BEHAVIOR` | ACS object (see `10-acs.md`) | |
| `LOADACS` | text list of ACS libraries (`05`) | |
| `SNDCURVE` | u8 × N (attenuation table) | |
| `GENMIDI`, `DMXGUS` | music support | |

## 9. Demos

### 9.1 Vanilla (Doom 1.9 / Boom / MBF / complevel-tagged)

```
u8 version (109 = Doom 1.9; 111+ alternates; 202/203 Boom; 203 MBF; 221 MBF21 (DSDA) …)
[Doom 1.9] u8 skill, episode, map, deathmatch, respawn, fast, nomonsters, consoleplayer, playeringame[4]
ticcmds: per player per tic: i8 forwardmove, i8 sidemove, u8 angleturn(high byte), u8 buttons
end marker 0x80
```

Supported as **playback only**, in vanilla-compat mode (`19-compatibility.md`). Longtic demos (`-longtics`, version 111) carry `i16 angleturn`.

### 9.2 ZDoom demos

IFF container: `FORM` + type `ZDEM`, chunks:

| Chunk | Content |
|---|---|
| `ZDHD` | engine version, demo version, map name, RNG seed, consoleplayer |
| `VARS` | serialized server CVars |
| `UINF` | per-player user info strings |
| `BODY` | stream of compressed per-tic commands (`DEM_*` packet bytes, deltas of `usercmd_t`, net commands like `DEM_SAY`, `DEM_GIVECHEAT`, `DEM_NETEVENT`…) |
| `COMP` | zlib-compressed BODY variant |
| `NETD` | netplay marker |
| `WEAP` | weapon slot info |

Room records its own variant `FDEM` (same IFF shape, chunk `FDHD` includes the load-order fingerprint and module hashes) and plays back `ZDEM` demos best-effort (they depend on GZDoom version-specific behaviour; only guaranteed for content without ZScript, and even then not bit-exact — document as "S, best effort").

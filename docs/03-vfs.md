# 03 — Virtual filesystem (`room-vfs`)

Equivalent of GZDoom's `FileSystem` / `FResourceFile` (`src/common/filesystem/`).

## 1. Responsibilities

- Mount resource containers in a defined **load order**.
- Expose every file as a **lump** with: short name (8 chars), full path, namespace, size, owning container, and position in load order.
- Resolve lookups exactly like GZDoom (last-loaded wins; namespace-aware; full-path lookups).
- Decompress on demand; cache decoded bytes with a bounded LRU.
- Provide MD5 of containers and of individual map lumps (for the compat database and save fingerprints).

## 2. Supported containers

| Container | Detection | Priority |
|---|---|---|
| WAD (`IWAD`/`PWAD` magic) | header magic | M |
| ZIP (`.pk3`, `.ipk3`, `.zip`, `.pke`) — stored, deflate, deflate64, bzip2, LZMA, XZ, implode/shrink | `PK\x03\x04` / central directory | M (stored/deflate/bzip2/lzma); S (others) |
| 7z (`.pk7`, `.ipk7`, `.7z`) | `7z\xBC\xAF\x27\x1C` | M |
| Directory on disk | path is a dir | M |
| Single loose file (`.deh`, `.bex`, `.lmp`, `.mid`, `.png`, `.acs`…) | fallback | M |
| Build GRP, Quake PAK, Blood RFF, Descent HOG, Hexen II SSI, LUB | magic | C |
| WAD inside archive root, PK3 inside archive root | extension | M (nested mount, see §5.4) |

### 2.1 WAD format

```
Header (12 bytes, little-endian):
  [0..4]  magic "IWAD" | "PWAD"
  [4..8]  i32 numlumps
  [8..12] i32 infotableofs
Directory entry (16 bytes) × numlumps at infotableofs:
  i32 filepos, i32 size, [u8; 8] name (NUL-padded, may lack NUL, uppercase-normalize on read)
```

Robustness rules (from GZDoom): clamp entries whose `filepos+size` exceeds file length (warn, truncate); accept lowercase names (uppercase them); names containing `\0` mid-way end there. A zero-length lump between markers is a marker.

## 3. Namespaces

```rust
pub enum Namespace {
    Global, Sprites, Flats, Colormaps, Acs, NewTextures /*TX_*/, BloodRaw, BloodSfx, BloodMisc,
    Voices /*Strife*/, HiRes, Voxels, Patches, Graphics, Sounds, Music, Fonts, Models, Shaders,
    Brightmaps, Materials, Filter, Maps, Skins, // directory-derived
}
```

### 3.1 WAD marker ranges

| Markers | Namespace |
|---|---|
| `S_START`/`S_END`, `SS_START`/`SS_END` | Sprites |
| `F_START`/`F_END`, `FF_START`/`FF_END` | Flats |
| `C_START`/`C_END` | Colormaps |
| `A_START`/`A_END` | Acs |
| `TX_START`/`TX_END` | NewTextures |
| `V_START`/`V_END` | Voxels |
| `HI_START`/`HI_END` | HiRes |
| `P_START`/`P_END`, `PP_START` | Patches (informational; lookups still global) |

Rules (`FWadFile::SetNamespace` behaviour):

- Unmatched `*_END` markers are ignored with a warning; a missing `*_END` closes the range at end of file.
- **Flat hack:** if a WAD has `F_END`/`FF_END` markers but no start marker (a vanilla-PWAD idiom), every lump of exactly 4096 bytes before the last end marker gets the flag `MAYBE_FLAT` (it stays in `Global`). The texture manager treats `MAYBE_FLAT` lumps as flat candidates.
- Sprite-namespace lumps that fail patch-header validation are demoted to `Global` with a warning.

### 3.2 Archive directory → namespace

Top-level directory names (case-insensitive) map to namespaces:

| Directory | Namespace | | Directory | Namespace |
|---|---|---|---|---|
| `sprites/` | Sprites | | `music/` | Music |
| `flats/` | Flats | | `sounds/` | Sounds |
| `textures/` | NewTextures | | `voices/` | Voices |
| `patches/` | Patches | | `acs/` | Acs |
| `graphics/` | Graphics | | `voxels/` | Voxels |
| `colormaps/` | Colormaps | | `hires/` | HiRes |
| `fonts/` | Fonts (sub-dirs = font names) | | `models/` | Models |
| `maps/` | Maps (each `*.wad` is a map container) | | `shaders/` | Shaders |
| `brightmaps/` | Brightmaps | | `materials/` | Materials |
| `filter/` | Filter (see §5.3) | | everything else / root | Global |

A file nested deeper (`sprites/monsters/troo/TROOA1.png`) keeps its top-level namespace. Short name = file stem (without extension), uppercased, truncated to 8 bytes. Files in non-namespace subdirectories get `Global` namespace but **no short name lookup** (only full-path lookup) — GZDoom marks them `LUMPF_FULLPATH`; replicate exactly: root-level files and namespace directories get short names, other directories do not.

### 3.3 Single-root-folder stripping

If every entry of a ZIP/7z archive lives under the same top-level folder **and** that folder is not itself a namespace name, strip it (GZDoom does this to tolerate badly zipped mods).

## 4. Lookup API

```rust
pub struct Vfs { /* ... */ }

impl Vfs {
    pub fn find(&self, name: &str, ns: Namespace) -> Option<LumpId>;           // last loaded wins
    pub fn find_any_ns(&self, name: &str) -> Option<LumpId>;                    // Global first then others (CheckNumForName w/ ns_global fallback rules)
    pub fn find_full(&self, path: &str) -> Option<LumpId>;                      // "textures/foo.png"; also tries without extension
    pub fn find_all<'a>(&'a self, name: &str, ns: Namespace) -> impl Iterator<Item = LumpId> + 'a; // load order, for SNDINFO/DECORATE/...
    pub fn find_in_container(&self, c: ContainerId, name: &str) -> Option<LumpId>;
    pub fn lump(&self, id: LumpId) -> &LumpInfo;
    pub fn read(&self, id: LumpId) -> Result<Bytes, VfsError>;                  // Bytes = ref-counted, cached
    pub fn container_md5(&self, c: ContainerId) -> [u8; 16];
    pub fn iter(&self) -> impl Iterator<Item = (LumpId, &LumpInfo)>;
}
```

Short-name lookup for the namespaces that GZDoom treats as "global-fallback"
(e.g. `TexMan` looking up a texture name checks `NewTextures`, then `Flats`/`Patches` per usage, then `Global`) is implemented by the *callers*, not by the VFS; the VFS only offers precise lookups.

Indices: build `IndexMap<(Namespace, ShortName), SmallVec<[LumpId; 2]>>` and a full-path map with lowercase keys. `ShortName` is `[u8; 8]` compared as `u64`.

## 5. Load order

### 5.1 Startup sequence

1. **Engine resource** `room.pk3` (embedded in the binary with `include_bytes!` and also loadable from disk for development). Contains IWADINFO, per-game base definitions (DECORATE for built-in actors, MAPINFO, SNDINFO, LANGUAGE, MENUDEF, fonts, shaders, …).
2. **Optional engine extras** if present and enabled: `lights.pk3`, `brightmaps.pk3`, `game_support.pk3`, `game_widescreen_gfx.pk3` (compatible with GZDoom's files; users may supply them).
3. **IWAD** (selected per §6), plus any `Load` entries from its IWADINFO record.
4. **Autoload**: config sections in order `global.autoload`, `<game>.autoload`, `<game>.<iwad-family>.autoload`, `<autoname>.autoload` (mirrors GZDoom's `[doom.Autoload]`, `[doom.doom2.Autoload]`, …). Also `autoload/` directories next to the executable and in the user data dir with the same naming.
5. **Command line** `-file` entries in order, `-deh`/`-bex` patches, then `-loadacs`-style extras.
6. **WASM modules** are discovered inside mounted archives (`modules/*.wasm` + manifest; see `12`) — they are not separate mounts.

### 5.2 Override semantics

- `find` returns the **last** match in load order.
- "Additive" lumps are processed in load order via `find_all` — the list is documented per lump in `05-text-lumps.md` (DECORATE, SNDINFO, MAPINFO, LANGUAGE, ANIMDEFS, GLDEFS, KEYCONF, MENUDEF, CVARINFO, LOCKDEFS, TERRAIN, DECALDEF, FONTDEFS, MODELDEF, VOXELDEF, TRNSLATE, REVERBS, SECRETS, X11R6RGB, TEXTURES, …).
- Map lumps: a map name found in a later container replaces the whole map (map lumps are never mixed across containers).

### 5.3 Filters

Directory `filter/<name>/...` inside an archive is remapped to the archive root
if `<name>` matches one of the active filter names; otherwise ignored. Filter
names are derived from the IWAD's IWADINFO `Autoname`, dotted progressively:
for autoname `doom.id.doom2.commercial` the active filters are
`game-doom`, `doom`, `doom.id`, `doom.id.doom2`, `doom.id.doom2.commercial`. The
`game-<Game>` filter comes from the IWADINFO `Game` key (Doom/Heretic/Hexen/Strife/Chex).
More specific filters are applied after less specific ones (so they override).

### 5.4 Nested containers

- `*.wad` at the root of a PK3/PK7 (or in a directory mount root) is mounted as a separate container immediately after its parent, inheriting the parent's position.
- `maps/<NAME>.wad` is **not** mounted; it is a map container accessed by the map loader (`06`).
- Nested PK3 in root: mounted likewise (GZDoom supports this for some formats) — S priority.

## 6. IWAD detection

Data comes from the `IWADINFO` lump in `room.pk3` (same syntax as GZDoom's `wadsrc/static/iwadinfo.txt`):

```
IWad
{
    Name = "DOOM 2: Hell on Earth"
    Autoname = "doom.id.doom2.commercial"
    Game = "Doom"
    Config = "Doom"
    Mapinfo = "mapinfo/doom2.txt"
    MustContain = "MAP01", "MAP02", ...
    BannerColors = "a8 00 00", "a8 a8 a8"
    IgnoreTitlePatches = 1
    Load = "..."          // optional extra lumps/files
    Required = "..."      // e.g. a PWAD-based IWAD needing a base IWAD
}
Names { "doom2.wad", "doom2f.wad", ... }
Order { "...", ... }      // preference order shown in the picker
```

Algorithm:
1. Collect candidate files from search paths (`DOOMWADDIR`, `DOOMWADPATH`, config paths, exe dir, cwd, platform data dirs, known Steam/GOG/Microsoft Store install paths — list maintained in `room-app/src/iwad_paths.rs`; also check `.ipk3`/`.iwad` files).
2. For each candidate, open its directory only and test every IWADINFO record's `MustContain` list; choose the **most specific** match (the record with the most `MustContain` entries satisfied — GZDoom iterates records in file order and picks the first full match, so order IWADINFO from most to least specific).
3. `-iwad` forces a file. If several IWADs are found and none forced, show the picker (native: in-engine picker UI before the main window if `queryiwad` is true; web: file picker / OPFS library screen).
4. Store chosen IWAD's `Autoname`, `Game`, `Config` and the derived filter list on `GameInfo`.

## 7. Caching and I/O backends

```rust
pub trait ContainerSource: Send + Sync {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
}
```

Implementations: `FileSource` (std::fs, positioned reads), `MemorySource` (`Bytes`; used on web and for embedded `room.pk3`), `DirSource` per-file. Decoded/decompressed lump data is cached in an LRU keyed by `LumpId` with a byte budget (default 256 MiB native, 64 MiB web). `Bytes` returned from `read` keeps data alive independent of eviction.

## 8. Writing (tools only)

`room-vfs::write` provides a `WadWriter` and `ZipWriter` wrapper for tools (`room-cli` extract/repack, savegame containers). Never used by the engine at runtime except for saves.

## 9. Tests

- Round-trip: write a WAD with markers → mount → verify namespaces.
- PK3 with `filter/doom.id/`, single-root-folder, nested WAD, mixed case paths.
- Fuzz: `wad_directory`, `zip_central_directory`, `sevenz_header`.

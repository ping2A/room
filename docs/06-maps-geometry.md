# 06 — Maps and geometry (`room-map`, `room-nodebuild`)

GZDoom references: `src/maploader/*.cpp` (`MapLoader`), `src/maploader/udmf.cpp`,
`src/maploader/slopes.cpp`, `src/maploader/polyobjects.cpp`, `src/playsim/p_3dfloors.cpp`,
`src/playsim/portal.cpp`, `src/common/engine/` (nodebuild in `src/common/utility/nodebuilder/`).

## 1. In-memory map model

All geometry uses `f64` (GZDoom: `double`). Indices are `u32` newtypes.
Static data lives in `MapGeometry` (immutable after load, except where noted);
dynamic per-tic data (plane heights, light levels, texture offsets, alpha …)
lives in `World` (`07-playsim.md`) as parallel arrays indexed by the same IDs
so saves can serialize only dynamic state.

```rust
pub struct MapGeometry {
    pub vertices: Vec<Vertex>,            // DVec2 + optional zfloor/zceiling (UDMF vertex heights)
    pub lines: Vec<Line>,
    pub sides: Vec<Side>,
    pub sectors: Vec<SectorStatic>,
    pub subsectors: Vec<Subsector>,       // GL subsectors (closed polygons)
    pub segs: Vec<Seg>,                   // GL segs incl. minisegs (line = None)
    pub nodes: Vec<Node>,
    pub game_nodes: Option<GameNodes>,    // original non-GL nodes kept for vanilla-compatible point lookups
    pub blockmap: Blockmap,
    pub reject: Option<Reject>,
    pub polyobjects: Vec<PolyobjStatic>,
    pub portals: PortalTables,
    pub udmf_user: UserFields,            // user_* keys per element
    pub md5: [u8; 16],                    // of map lumps, for compat DB + saves
    pub format: MapFormat,                // Doom | Hexen | Udmf { namespace }
}

pub struct Line {
    pub v1: VertexId, pub v2: VertexId, pub delta: DVec2, pub bbox: BBox,
    pub sides: [Option<SideId>; 2], pub front_sector: SectorId, pub back_sector: Option<SectorId>,
    pub flags: LineFlags,                 // ML_* incl. ZDoom extended (BLOCKEVERYTHING, BLOCKUSE, BLOCKSIGHT, BLOCKHITSCAN, 3DMIDTEX, CHECKSWITCHRANGE, FIRSTSIDEONLY, …)
    pub activation: Activation,           // SPAC_* bitflags (Cross, Use, MCross, Impact, Push, PCross, UseThrough, AnyCross, MUse, MPush, UseBack, Damage, Death)
    pub special: Special, pub args: [i32; 5],
    pub ids: SmallVec<[i32; 1]>,          // line IDs (UDMF id + moreids)
    pub alpha: f64, pub render_style: LineRenderStyle,
    pub lock: u16, pub automap_style: u8, pub health: i32, pub health_group: i32,
    pub portal: Option<LinePortalId>,
    // No `validcount` field: GZDoom's validcount dedupe is done with per-query
    // stamp arrays owned by the sim (see §7), keeping MapGeometry immutable.
}

pub struct Side {
    pub sector: SectorId, pub line: LineId,
    pub parts: [SidePart; 3],             // Top, Mid, Bottom: texture, x/y offset, x/y scale, per-part light, colors (UDMF uppercolor_top … ), flags
    pub light: i16, pub flags: SideFlags, // LIGHTABSOLUTE, LIGHTFOG, NOFAKECONTRAST, SMOOTHLIGHTING, CLIPMIDTEX, WRAPMIDTEX, NODECALS, NOGRADIENT…
    pub tier_colors: ...
}

pub struct SectorStatic {
    pub lines: Vec<LineId>, pub bbox: BBox, pub center: DVec2,
    pub tags: SmallVec<[i32; 1]>,
    pub initial: SectorDynamicInit,       // initial planes, textures, light, special, damage, friction, colors, …
    pub subsectors: Range<u32>,
}
```

`SectorDynamic` (in `World`): `floor: SecPlane, ceiling: SecPlane`, per-plane `texture, xoffs, yoffs, xscale, yscale, angle, base_angle, alpha, light (+absolute flag), glow (color, height), skybox/portal ids`, `light_level`, `special`, `damage { amount, type, interval, leakiness }`, `friction, move_factor, gravity`, `colormap { light: Rgb, fade: Rgb, desaturation, fog_density }`, per-wall colours (UDMF `color_floor/ceiling/walltop/wallbottom/sprites`), `sound_target`, `sound_sequence`, `ffloors: Vec<FFloorId>`, `light_list`, `flags (SECF_*)`, `thinglist` head, `touching_thinglist`, `interpolation` refs, `stair_lock`, `prev/next sec`, `health` (destructible geometry).

### 1.1 Planes

```rust
pub struct SecPlane { pub normal: DVec3, pub d: f64, pub neg_inv_c: f64 }
impl SecPlane {
    pub fn z_at(&self, p: DVec2) -> f64 { (self.d + self.normal.x * p.x + self.normal.y * p.y) * self.neg_inv_c }
    pub fn is_slope(&self) -> bool;
    pub fn change_height(&mut self, dz: f64);          // d -= dz * c
}
```

Use GZDoom's exact formulas (`secplane_t` in `r_defs.h`) to keep collision identical.

## 2. Loading pipeline

```
1. Locate map: Vfs::find(name) marker lump (last container wins) or maps/<NAME>.wad container.
2. Gather lumps from that container only (TEXTMAP..ENDMAP or the fixed list).
3. Compute MD5 over map lumps exactly as GZDoom (`MapData::GetChecksum`: THINGS, LINEDEFS, SIDEDEFS, SECTORS, BEHAVIOR or TEXTMAP).
4. Parse into raw records (Doom/Hexen binary, or UDMF).
5. Doom-format only: translate specials & flags through XLAT (`09 §Translation`).
6. Validate & repair (§2.2).
7. Apply compatibility database fixes keyed by MD5 (`19-compatibility.md §Map fixes`).
8. Nodes: load (any supported format) or build (§3). GL nodes always required.
9. Blockmap: load and validate, or build.
10. Reject: load if size-correct, else none.
11. Spawn-time geometry processing (§4) – slopes, 3D floors, portals, polyobjects, static specials.
12. Group sectors (portal groups, sky sectors), compute subsector → sector, line → sector links, sector bboxes.
13. Produce MapGeometry + initial SectorDynamic/SideDynamic arrays + MapThing list for spawning.
```

### 2.1 UDMF

- Grammar (from `udmf.txt`): `translation_unit := global_expr*; global_expr := block | assignment; block := ident '{' assignment* '}'; assignment := ident '=' value ';'`, values: int, float, quoted string, `true`/`false`. Identifiers case-insensitive.
- Must be **fast** (multi-MB TEXTMAPs): write a hand-rolled zero-copy lexer specialised for UDMF instead of the general scanner; target ≥ 100 MB/s.
- Namespaces: `doom`, `heretic`, `hexen`, `strife`, `zdoom`, `zdaemon`, `vavoom`, `eternity` (partial, as GZDoom), `dsda` (S). Non-ZDoom namespaces restrict available keys and select the special set/translator.
- Every key in `udmf.txt` + `udmf_zdoom.txt` (GZDoom repo `specs/`) must be supported. Notable: things (`id, x, y, height, angle, type, skill1..16, class1..16, single, coop, dm, friend, dormant, ambush, standing, strifeally, translucent, invisible, countsecret, conversation, special, arg0..4, arg0str, gravity, health, renderstyle, fillcolor, alpha, score, pitch, roll, scalex/scaley/scale, floatbobphase, friendlyseeblocks, lm_*`), linedefs (all `SPAC` booleans, block flags, `moreids`, `arg0str`, `alpha`, `renderstyle`, `locknumber`, `automapstyle`, `health`, `healthgroup`, `damagespecial`, `deathspecial`, …), sidedefs (per-tier offsets/scales, `light_top/mid/bottom`, `lightabsolute_*`, colours, `nodecals`, `clipmidtex`, `wrapmidtex`, `nogradient_*`, `flipgradient_*`, `useowncolors_*`, `xscroll*`, …), sectors (plane panning/scale/rotation, plane light/absolute, alpha, renderstyle, `lightcolor`, `fadecolor`, `desaturation`, `fogdensity`, `silent`, `nofallingdamage`, `dropactors`, `norespawn`, `hidden`, `waterzone`, `moreids`, `damageamount/type/interval/leakiness/terrainhit/hazard`, `floorterrain/ceilingterrain`, `floorplane_a..d`, `ceilingplane_a..d`, `color_*`, `glow*`, `portal_*`, `skyfloor/skyceiling`, `xscrollfloor…`, `friction`, `gravity`, `soundsequence`, `health*`, …), vertices (`zfloor`, `zceiling`).
- Keys prefixed `user_` are stored in `UserFields` with declared type inferred from the literal.
- Unknown non-user keys: ignored silently (spec says so).
- Comments (`//`, `/* */`) allowed.

### 2.2 Validation and repair (GZDoom `MapLoader` behaviour)

- Line referencing out-of-range vertex: drop line (warn).
- Sidedef sector index out of range: set to 0 (warn).
- Front side missing: check GZDoom's current `MapLoader` handling before implementing (it warns about lines without a front side) and record the choice in an ADR; the default is to remove the line from geometry but keep its special reachable by ID.
- Two-sided flag without back side: clear the flag.
- Zero-length lines: keep for specials but exclude from nodes/collision.
- Sectors with no lines: keep (may be tagged/scripted), no subsectors.
- Things outside map: spawn, will fall into nearest subsector by point lookup.
- Doom-format sidedef textures with names that are numbers on special lines (e.g. 242 colormaps, 260 translucency, 213 transfers) are interpreted contextually — keep raw names on `Side` until §4 resolves them.

## 3. Node building (`room-nodebuild`)

Port of ZDBSP's algorithm (GZDoom embeds it as `FNodeBuilder`). Requirements:

- Input: vertices, lines (with sides/sectors), polyobject start spots (to avoid splitting polyobject lines into separate subsectors — ZDBSP marks polyobj lines "not to be split"; GZDoom handles polyobjects with dynamic subsectors, see §5.4).
- Output: GL nodes (segs with partner segs and minisegs, closed convex subsectors) **and** optionally "extended non-GL" nodes.
- Algorithm: recursive partitioning; candidate splitters = segs (one per unique line partition); cost = `splits * SplitCost + |left-right| imbalance + AA preference` with ZDBSP defaults (`MaxSegs = 64`, `SplitCost = 8`, `AAPreference = 16`); pick minimum cost; split segs at intersections with `f64` math then snap vertices to ZDBSP's fixed-point grid (vertex coordinates stored as 16.16 in ZDBSP; GZDoom's builder uses fixed internally) to match node-builder output used by renders/collision.
- Determinism: same input ⇒ same output on all platforms (pure integer/fixed math inside the builder, like ZDBSP).
- Performance: build MAP29-class maps in < 50 ms native; large community maps (e.g. 100k lines) in < 2 s. Use rayon for candidate evaluation on native (results reduced deterministically by (cost, index)).
- Validate loaded nodes; if invalid (bad indices, non-closing subsectors for GL), rebuild. Cache built nodes on disk keyed by map MD5 (`gl_cachenodes` equivalent) — native only.

## 4. Spawn-time geometry processing

Process in GZDoom's order (`MapLoader::LoadLevel`, `P_SpawnSpecials`, `P_SpawnSlopeMakers`, …). Implementation must keep this order because later steps read results of earlier ones.

1. **Line identification**: `Line_SetIdentification` (121), UDMF `id`, translated Doom tags.
2. **Static init**: `Static_Init` (190) — sector damage/gravity/sky-transfer/ friction; Boom 242 (`Transfer_Heights`: fake floors/ceilings "deep water"), 213/261 (transfer floor/ceiling light), 260 (translucency; with TRANMAP), 271/272 (MBF sky transfers), Boom 223 (friction), 224 (wind) etc. (via XLAT these map to ZDoom specials).
3. **Slopes** (`slopes.cpp`), in order:
   - UDMF explicit plane equations (`floorplane_a..d`).
   - Slope things: 9500/9501 (`SlopeFloorPointLine`/`SlopeCeilingPointLine`), 9502/9503 (`SetFloorSlope`/`SetCeilingSlope`), 9510/9511 (`CopyFloorPlane`/`CopyCeilingPlane`), 9530 (`VavoomFloor`), 1500/1501 (vertex height things) and 1504/1505 (vertex slopes); UDMF vertex `zfloor`/`zceiling` on triangular sectors.
   - `Plane_Align` (181) lines, `Plane_Copy` (118).
4. **3D floors** (`p_3dfloors.cpp`): `Sector_Set3DFloor` (160) with type bits (solid/swimmable/nonsolid/vavoom, `FF_*` flags: render inside, fog, no shade, double shadow, invert alpha, fade, reset lighting, shootable etc.), Legacy-style translation via XLAT; build per-sector `ffloors` sorted top→bottom and `light_list` (light splitting through 3D floors). `Sector_Attach3dMidtex` (51) and 3D midtextures (`ML_3DMIDTEX`).
5. **Portals** (§5).
6. **Polyobjects** (§6).
7. **Scrollers & static specials**: `Scroll_Texture_*`, `Scroll_Floor/Ceiling` (223/224 Hexen-format), Boom scrollers (via XLAT), `Sector_CopyScroller`, `Sector_SetLink` (linked plane movement), `Line_AlignFloor/Ceiling`, `Sector_SetPortal`, … These create thinkers in the World (see `07`). Runtime-only specials such as `Line_SetBlocking` are not processed here.
8. **Sky**: sky sectors (ceiling/floor flat = sky flat, configurable by `gameinfo.skyflatname`), MBF sky transfers.

## 5. Portals

GZDoom's portal model (`portal.cpp`, `portal.h`) must be reproduced; it affects collision, sight, sound and rendering.

### 5.1 Types

| Portal | Source | Effect |
|---|---|---|
| Line portal, visual | `Line_SetPortal(tag, 0, type=0 visual)` | rendered only |
| Line portal, teleporter | type 1 | actors crossing are teleported (non-seamless) |
| Line portal, interactive | type 2 | seamless crossing, no static linking |
| Line portal, static/linked | type 3 | full collision/sight linking; creates **portal groups** |
| Line_QuickPortal (`156`) | | shortcut to linked portal |
| Sector skybox | `SkyViewpoint` (9080), `SkyPicker` (9081), `SectorSkyViewpoint`… | rendered sky through another viewpoint |
| Stacked sectors | `UpperStackLookOnly`/`LowerStackLookOnly` (9077/9078), `Sector_SetPortal` (57) type 0/1 | rendered; linked when "static" |
| Plane portals (static linked) | `Sector_SetPortal(tag, 1|2 linked…)` | collision across floors/ceilings (e.g. room over room) |
| Horizon | `Line_Horizon` (9) | rendering only |
| Mirror | `Line_Mirror` (182) | rendering only |
| Sky viewpoint for whole map | 9080 with tid 0 | |

### 5.2 Linked portal groups

- At load, connected linked portals partition sectors into **groups**. Each pair of groups has a fixed 2D displacement (`FDisplacementTable`). Displacements must be consistent (GZDoom validates; inconsistency disables linking with a warning).
- Every actor has a `group` derived from its sector. Distances between actors use `actor.pos_relative(other)` which adds the displacement between their groups (GZDoom `Vec2To`, `Vec3To`, `PosRelative`). **All** sim distance computations must go through these helpers.
- The blockmap search crosses portals via a **portal blockmap** (lines that are portals, and sectors with linked plane portals per block). `FMultiBlockLinesIterator` and `FMultiBlockThingsIterator` equivalents iterate blocks in all reachable groups within the check radius.
- Hitscans (`P_LineAttack`/`P_TraceBullet`) and sight checks continue through linked portals by re-entering the trace in the other group with transformed origin.

## 6. Polyobjects

- Anchors: thing 9300 (`PolyObject_Anchor`); spawn spots 9301 (normal), 9302 (crushing), 9303 (hurting) — Hexen's 3000–3002 when in Hexen game compat mode as GZDoom does.
- Lines: `Polyobj_StartLine` (1) traverses connected lines; `Polyobj_ExplicitLine` (5) with order args.
- Mirror polyobjects (`args`), sound sequences.
- Movement: rotation and translation of vertex lists; collision through `PolyBlock` links (polyobject lines are linked into blockmap cells via a separate list rebuilt on each move).
- Rendering: polyobject segs are inserted into the subsectors they overlap each tic they move — GZDoom's "polyobject node splitting" (`FPolyObj::CreateSubsectorLinks`, `polynodes`). Port that.
- Rotation/translation are interpolated for rendering.
- `compat_polyobj` and the "grinding polyobj" MAPINFO flag affect crush behaviour.

## 7. Blockmap and spatial queries

- 128×128 map-unit cells, origin from lump or `bbox.min - 8` when built (GZDoom rounding rules).
- Static per-cell line lists (built or loaded). Dynamic per-cell actor lists (`World::block_things`, intrusive doubly-linked via `ActorId` arrays).
- Iterators:
  - `BlockLinesIter(bbox)` — validcount-stamp dedupe (per-query stamp array, not a field on Line).
  - `BlockThingsIter(bbox)`.
  - `PathTraverse(from, to, flags)` — intercept list sorted by fraction (Doom `P_PathTraverse` semantics, including the "early out" order quirks used by use-lines and hitscan). Keep a vanilla-exact variant for compat mode (uses fixed-point `P_InterceptVector` and `divline` math).
- Reject: `rejected(sec_a, sec_b)` is an early-out for sight checks. Ignore the reject table in maps with linked portals, and when its size doesn't match the sector count.

## 8. Point lookup

- `point_in_subsector(p)` walks nodes (`R_PointOnSide` with `f64`; compat path uses fixed-point and original game nodes for vanilla-demo fidelity).
- `point_in_sector(p)`.
- Height queries considering 3D floors and slopes: `floor_z_at(sector, p, actor_z)` / `ceiling_z_at` / `next_lower_floor` etc. Port `P_GetFloorZ`-family and `sector_t::NextHighestCeilingAt`, `NextLowestFloorAt` (used by portals and 3D floors).

## 9. Tests

- Golden: load Freedoom MAP01–MAP32 and E1M1–E4M9; dump counts and checksums of geometry, compare to snapshot.
- Node builder: for each test map, verify every subsector polygon is convex and closed; compare point-in-sector lookups on a 64-unit grid against the input map's own nodes.
- UDMF: parse spec examples; fuzz the UDMF lexer; property test "serialize → parse" round trip using a UDMF writer (also useful for the CLI `convert` tool).
- Portals: synthetic maps (generated by a test helper that writes UDMF) exercising each portal type.

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
- Editable documents (`RawMap`): each `RawLinedef` / `RawSector` / `RawThing` keeps the UDMF booleans that have no raw slot as `ext` bits (`udmf::ext`, key tables `LINE_EXT_KEYS` / `SECTOR_EXT_KEYS` / `THING_EXT_KEYS`) and every record its `user_*` bag (`user`, ints and 16.16 fixed); `write_textmap` writes both back, `coop` / `dm` explicitly, and `RawMap::to_geometry` rebuilds `udmf_user` from them.
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

Room follows UZDoom's portal model (`src/playsim/portal.cpp`,
`src/maploader/specials.cpp`). Load-time tables live in
`room-map/src/portals.rs`, the playsim side in `room-sim/src/portal.rs` and
`room-sim/src/sight_portal.rs`, drawing in `room-render/src/portal_view.rs`.
Maps without portals take none of these paths, so vanilla play, demos and
checksums are unchanged.

### 5.1 Types (implemented 2026-11-19)

| Portal | Source | Room |
|---|---|---|
| Line portal, visual | `Line_SetPortal(target id, this id, 0, align)` (156) | drawn; also one-sided lines |
| Line portal, teleport | type 1 | actors crossing move (position, velocity, angle, Z by `align`); hitscans continue |
| Line portal, interactive | type 2 | as teleport, plus collision checks the other side (`PIT_CheckPortal`); a Z alignment demotes it to teleport |
| Line portal, linked | type 3 | pure XY offset; creates **portal groups**; collision, sight, hitscans, sound and AI distances across it |
| Eternity linked pair | `Line_SetPortal(0, …, 4)` + `(1, …, 4)` sharing a line id | linked both ways |
| `Line_QuickPortal` (301) | two lines with one id | linked (`args[0]` 0) or visual |
| Stacked sectors | `UpperStackLookOnly` / `LowerStackLookOnly` (9077/9078, same TID), `Sector_SetPortal(tag, 0, plane, 0/1)` anchor + reference | drawn |
| Linked plane portal | `Sector_SetPortal(tag, 6, plane, 0/1)` | groups; floor/ceiling heights, movement (`CheckPortalTransition`), sight, hitscans and noise go through |
| Copy | `Sector_SetPortal(tag, 1, …)` / type 5 onto lines | copies an existing portal to more sectors / lines |
| Eternity plane / horizon | `Sector_SetPortal(tag, 3/4, plane)` | drawn as the origin sector's planes to the horizon |
| Sky viewpoint | `SkyViewpoint` (9080; TID 0 = level default), `SkyPicker` (9081), `SkyCamCompat` (9083 / 5006 with `Sector_SetPortal(tag, 2, plane)`) | drawn |
| Horizon | `Line_Horizon` (9) | drawn |
| Mirror | `Line_Mirror` (182) | drawn |
| UDMF plane keys | `portal_floor_*` / `portal_ceil_*` `blocksound`, `disabled`, `nopass`, `norender` | honoured |
| Runtime retarget | `Line_SetPortalTarget(thisid, destid)` (107) | `ChangePortal` |

UZDoom's set-up rules are kept: a traversable line portal without a back
sector becomes visual; a portal whose destination does not lead back is a
teleporter; linked portals must be two-way, unrotated and have consistent
displacements (else they are demoted, as in UZDoom); linked plane portals on
sloped planes are removed; a plane past its portal plane is *obstructed*
(checked against the live height). Hexen-format maps take line ids from the
specials that carry them (UZDoom `SetLineID`).

Room's Build water and fall links (`Sector_SetPortal` types 200 / 201,
`docs/23-build-engine.md`) are separate: `PortalTables::build_links`.

### 5.2 Linked portal groups

- At load, linked portals split sectors into groups (`CollectSectors`); group
  0 is everything no linked portal reaches. `PortalGroups::offset(from, to)`
  is UZDoom's `Displacements.getOffset`; indirect connections are filled in
  (`ConnectPortalGroups`).
- `portal::pos_relative`, `vec2_to`, `vec3_to`, `pos_seen_from` are
  `PosRelative` / `Vec2To` / `Vec3To`. Monster aiming, facing, chase
  direction, melee and missile range checks and missile aim use them.
- Movement: a move whose centre passes a passable line portal from its front
  is checked and lands on the destination side (`P_TryMove`); actors
  touching a line portal also check the other side, at the displaced
  position, with the empty space behind both portal lines ignored. Through
  linked plane portals, floor and ceiling come from the far side
  (`NextLowestFloorAt` / `NextHighestCeilingAt`) and actors that pass the
  plane change group (`CheckPortalTransition`).
- Sight (`SightCheck`): sorted line intercepts; a line portal stops the trace
  and queues a task that re-runs it in the destination group from that
  fraction; planes that open on a linked portal queue the group above or
  below.
- Hitscans continue through passable line portals and linked plane portals
  (up to 4).
- Noise alerts flood through linked planes and sound-passing line portals;
  sound positions are moved into the listener's group.
- The reject table is not used on maps with linked portals.
- `World::portal_moves` lists the actors moved through a portal each tic, so
  the shells snap their interpolation instead of sliding across the map.

- Position queries see every group they reach (UZDoom
  `CollectConnectedGroups` / `FMultiBlockThingsIterator`): thing–thing
  collision, blast damage (`P_RadiusAttack`), `A_RadiusThrust` and
  `A_RadiusGive` take the groups a box touches through linked line portals
  and, from its bottom to its top, through linked floor and ceiling portals
  (straight up / down and over neighbouring sectors), and find actors there
  at their displaced position (`portal::connected_groups`,
  `portal::query_pos`). Room keeps one actor list, not a blockmap per group,
  so this is a filter on that list rather than a second link.
- Polyobject portals: a polyobject whose lines carry a passable portal
  (UZDoom `bHasPortals`) keeps it while it moves. One-sided polyobject lines
  are not demoted to visual portals (`WALLF_POLYOBJ`). After each move a
  linked portal recomputes its displacement, its partner takes the
  opposite and the destination group moves in the displacement table
  (`FPolyObj::UpdateLinks`, `MoveGroup`); other portals on or aimed at the
  lines recompute their rotation, so a turning polyobject turns the view
  and the crossing with it (UZDoom keeps the load-time angle). Polyobjects
  with linked portals refuse `Polyobj_Rotate*` and swinging doors, as
  UZDoom. Portals are set up against the polyobject's start spot.
- ZScript: `Sector.GetPortalType` / `GetPortalPlaneZ` /
  `GetPortalDisplacement` / `GetOppositePortalGroup` / `PortalBlocks*` /
  `PortalIsLinked` (on `level.PointInSector` sectors and on `CurSector`);
  `LevelLocals.Vec2Diff` / `Vec3Diff` / `Vec2Offset` / `Vec3Offset` /
  `Vec2OffsetZ` / `GetDisplacement`; actor `Vec2To` / `Vec3To` /
  `Distance2D` / `Distance3D` / `AngleTo` / `PosRelative` through linked
  portals, and `Vec2Offset` / `Vec3Offset` / `Vec2Angle` / `Vec3Angle` /
  `Vec2OffsetZ` carried through interactive line portals unless
  `absolute` (`GetPortalOffsetPosition`). The playsim publishes a snapshot
  of the portals to the VM per map and whenever a portal changes
  (`room-zvm/src/portal_natives.rs`). Plane blocking answers use the flags
  at load (UZDoom re-checks `PLANEF_OBSTRUCTED` when planes move).

- `Line.isLinePortal` / `isVisualPortal` / `getPortalDestination` /
  `getPortalFlags` / `getPortalAlignment` on the map's `Line` objects
  (`level.Lines[i]`, a sector's `Lines`, a portal's destination); see doc 34
  for the map objects.

Not done: crossing a portal because a polyobject moved into the actor
(UZDoom has the same gap).

Editors: `room_map::portal_report(&MapGeometry)` summarises the linked
tables (line portals and destinations, portal planes with their flags,
groups, polyobject portal lines, `SetupData::portal_warnings`); the web
editor overlays it on the 2D view (doc 18 §3.8).

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

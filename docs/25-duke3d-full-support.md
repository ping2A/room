# 25 — Duke Nukem 3D: full support (implementation spec)

> **Audience:** a coding agent that will implement this. Read `AGENTS.md`,
> `docs/01-architecture.md`, `docs/02-conventions.md` and
> `docs/23-build-engine.md` first. This document is the plan *on top of* 23:
> 23 describes what exists, this one describes what "full" means, why the
> current approach cannot get there on its own, and the work, in order, with
> acceptance tests.
>
> Facts marked **(verify)** are from memory of the original source and must be
> checked against the 3D Realms GPL release (`SOURCE/*.C`) or Raze before
> they are encoded. Record each resolved ambiguity as an ADR in
> `docs/decisions/`.

---

## 0. TL;DR

Room already *walks and largely plays* all four Atomic episodes by converting
`DUKE3D.GRP` into a UDMF PWAD + DECORATE and running it on the Doom playsim
(`crates/room-build`, ~25k lines). That gets ~85% of the feel. The remaining
gap is structural, not a list of missing features:

1. **Doom geometry ≠ Build geometry.** Build sectors may overlap in 2D
   (rooms over rooms, non-Euclidean space) and walls move at runtime
   (`dragpoint`). A static Doom BSP can't represent either, so the converter
   approximates them with 3D floors, polyobjects drawn in a void, and custom
   portal kinds. Each map that breaks needs a new heuristic.
2. **CON is a program, not data.** `GAME.CON` drives every actor through
   `ai` / `move` / `action` / `if*`, and the vanilla C code (`actors.c`,
   `player.c`, `sector.c`) does the rest. Mapping CON commands one by one onto
   Doom AI (the long `CONEVT` / `useractor` list in 23 §3) is an open-ended
   list that never reaches parity, and it can't run user CON mods.
3. **Timing and RNG.** Duke runs at **30 Hz** with its own RNG (`krand`) and
   C-library `rand()` (`TRAND`). Room runs everything at 35 Hz, so original
   `.DMO` demos can never stay in sync. Demo sync is the only objective proof of
   parity.

**Recommendation:** add a **native Build runtime**: a Build world model,
clean-room Build engine primitives, a full CON compiler/VM, and the vanilla
Duke game logic, all at 30 Hz. Plug it into Room's existing shells, netcode,
server, audio, UI and wgpu device through a game-agnostic interface. Keep the
UDMF converter as an export tool (editor, Doom-mode mashups). Stop extending
the CON→DECORATE mapping.

"Full support" is then defined by objective tests (§1), chiefly: **the
Atomic Edition `DEMO1`–`DEMO3.DMO` play to their end in sync**.

---

## 1. Definition of "full support"

### Tier 1 — required (this is "full")

Target: **Duke Nukem 3D: Atomic Edition v1.4/1.5** (`DUKE3D.GRP`, 44 356 548
bytes for 1.5 **(verify)**). Behavioural reference: the 3D Realms source release
(v1.5 code) plus Chocolate Duke3D. Raze/EDuke32 are secondary references
because they changed behaviour.

| # | Criterion | How it's tested |
|---|-----------|-----------------|
| T1.1 | Original `DEMO1`–`DEMO3.DMO` play in sync to their final tic | Per-tic trace compare against a reference engine (§10.2) |
| T1.2 | Every map E1L1–E4L11 (+ secret levels, `E1L8` etc.) loads with no unknown sprite picnum, CON opcode or SE lotag | `room-build` integration test, all maps |
| T1.3 | Every level can be finished; secret/exit sectors and nuke buttons work, and `next level` / secret-level routing matches `USER.CON` | Scripted route per map (recorded `.DMO`-format input made by a human in Room, verified to also sync in the reference engine) |
| T1.4 | Full CON 1.5 language: unmodified `GAME.CON` / `DEFS.CON` / `USER.CON` compile and every actor runs through the VM | Opcode coverage test + T1.1 |
| T1.5 | All sector lotags and SE 0–36 behave as vanilla, including moving sectors that move real walls | Per-effector scenario tests on purpose-built mini maps (§10.3) |
| T1.6 | Rendering: overlapping sectors, moving walls, slopes, parallax (`pskybits`/`pskyoff`/`parallaxyscale`), wall/floor/face sprites with all cstat bits, palookup shading + visibility, palette swaps on any surface, two-level translucency, mirrors, camera screens, the screen-tint `pals` | Golden screenshots in 8-bit "classic" mode vs reference captures (±tolerance) |
| T1.7 | Front end: Duke menus, help screens, episode/skill select, 10 save slots, bonus screen (kills/secrets/time/par/3DR time), ANM cutscenes with their sound cues, credits, quit screen, cheats, third-person view (F7), automap, quotes, RTS remote ridicule | UI scenario tests + manual checklist |
| T1.8 | Original `GAME#.SAV`: load **and** save in the 1.5 format (round-trip) | Load → save → byte-compare the non-volatile sections; load in reference engine |
| T1.9 | Multiplayer: Co-op and DukeMatch (with/without spawn, markers, friendly fire, respawn rules) via Room lockstep, the room-server and its headless verifier | `coop_netplay`-style test for Duke + server checksum test |
| T1.10 | Determinism: identical per-tic checksums on x86-64, aarch64, wasm32 | Existing determinism CI extended to Duke |
| T1.11 | User content: `-map user.map`, user CON (`-x`), add-on GRPs (`-g`) layered over the base GRP | Load a community map + a small CON mod |

### Tier 2 — should

- Shareware and registered **1.3D** GRPs (different CON dialect, no E4, 1.3D
  `.DMO` format, some vanilla bugs).
- Official add-ons on 1.5 code: *Duke It Out in D.C.*, *Life's a Beach*,
  *Nuclear Winter*. They ship their own GRP/CON/ART.
- **20th Anniversary World Tour**: E5, flamethrower, new enemies/bosses,
  changed renderer lighting. Out of scope for the first pass. Only detect it and
  refuse with a clear message until tier 2 work starts.

### Tier 3 — optional (mods)

- EDuke32 extensions: `gamevar`, `onevent` with the full event list,
  `definequote`, `setactor[]`/`getplayer[]` struct access, `DEF` files
  (hi-res replacement, models), `.mhk` map hacks, Polymost/Polymer-only
  features. Implement as a separate CON dialect flag; do not let it change
  vanilla behaviour.
- Other Build games (Blood, Shadow Warrior, Redneck Rampage, NAM/WW2GI).
  The architecture should make them *possible* (shared Build world and
  engine primitives, per-game logic crate). They are not part of this spec.

### Explicit non-goals

- Bit-exact reproduction of the original **software renderer's** pixels. T1.6
  uses a tolerance. Classic 8-bit mode must look the same, not be identical.
- Original IPX/modem/serial net protocol, `SETUP.EXE`, `DUKE3D.CFG` binary
  compatibility (import the keys, don't round-trip).
- Original Ken's `.MAP` v6 or older formats (Duke shipped v7 only).

---

## 2. Current state (what an implementer inherits)

See `docs/23-build-engine.md` for detail. Summary with file pointers:

| Area | Where | Keep / change |
|------|-------|---------------|
| GRP container | `room-build/src/grp.rs` | **Keep**. Add add-on layering + GRP identification table |
| ART tiles, `picanm` | `art.rs` | **Keep**. Native runtime needs raw 8-bit tiles + `picanm` bits, not Doom patches |
| `PALETTE.DAT`, `LOOKUP.DAT` | `palette.rs`, `lookup.rs` | **Keep**. Add: shade tables as palookup (not COLORMAP), translucency table, `TABLES.DAT` (sintable, radarang, fonts, britable) |
| `.MAP` v7 parse | `map.rs` (`BuildMap`, `Sector`, `Wall`, `Sprite`) | **Keep** and make it the *runtime* source of truth |
| ANM (LPF) decode | `anm.rs` | **Keep**. Extend to full playback with timing + sound cues |
| CON parsing (data only) | `con.rs`, `condata.rs` (~6k lines) | **Replace** with a real compiler (§6). Reuse tokenizer pieces if they match vanilla tokenization |
| DMO parse + Ken LZW (`kdfread`) | `dmo.rs` | **Keep**. Feed the native runtime at 30 Hz |
| SAV parse | `sav.rs` (~3.8k lines, applies fields to a Doom world) | **Replace** the apply layer with a direct (de)serializer of the native world |
| UDMF/DECORATE conversion | `convert.rs`, `duke.rs`, `fx.rs`, `polys.rs`, `wad.rs`, `sprites.rs` | **Freeze** as `room-build::export` (editor/tools, Doom-mode). No new gameplay work here |
| Room extensions added for Duke (`ContactDamage`, water portal 200, `Sector_Stretch`, …) | `room-sim` | Keep for Doom-side content. The native runtime does not use them |
| Tests | `room-game/tests/duke3d_play.rs`, `room-render/tests/duke3d_e1l1.rs`, `room-build/tests/duke3d.rs` | Behavioural tests become the seed list for native scenario tests (§10.3). Converter tests stay with the exporter |

---

## 3. Licensing and clean-room rules (read before writing code)

- **Duke game code** (3D Realms release: `GAME.C`, `ACTORS.C`, `PLAYER.C`,
  `SECTOR.C`, `GAMEDEF.C`, `PREMAP.C`, `MENUES.C`, `RTS.C`, …) is
  **GPL-2.0-or-later**. That is compatible with Room's GPL-3.0-or-later.
  Translating it to Rust is allowed. List it in `THIRD_PARTY.md` the same way
  existing rows do.
- **Ken Silverman's Build engine** (`ENGINE.C`, `CACHE1D.C`, `MMULTI.C`,
  `A.ASM`/`A.C`, `PRAGMAS.H`) is under **BUILDLIC**, which is *not*
  GPL-compatible (no commercial use). EDuke32 and Raze still carry code
  derived from it (Raze ships `BUILDLIC.TXT`). **Do not translate or port engine
  code from any of these.** Implement the engine primitives (§5) **clean-room**:
  one agent writes a behavioural spec (inputs, outputs, edge cases, numeric
  rules) from `BUILDINF.TXT`, public docs and black-box testing. A *different*
  agent implements from that spec only. Put the specs in
  `docs/build-engine-primitives/*.md`. Tables that are *data in the GRP*
  (`TABLES.DAT` sintable, palettes) are content and are loaded at runtime,
  never embedded.
- Chocolate Duke3D, EDuke32 and Raze may be **run** as black-box references
  (trace generation, §10.2). Their *engine* sources must not be read by the
  implementing agent of §5.
- Add `docs/decisions/0004-native-build-runtime.md` (ADR) recording this split
  and the architecture choice in §4.

---

## 4. Target architecture

### 4.1 Crates

```
room-build            (existing) formats: GRP, ART, PALETTE/LOOKUP/TABLES, MAP, ANM, DMO, SAV, CON source
                      + export/ (frozen UDMF converter)
room-build-engine     NEW  Build world model + clean-room engine primitives. No Duke knowledge.
room-duke             NEW  Duke game: CON compiler + VM, actors/player/sector logic, 30 Hz tic,
                           DukeView snapshot, save/load, DMO adapter. Depends on room-build-engine.
room-render           + build/  sector-portal ("bunch") renderer path consuming BuildView
room-render-soft      + optional classic CPU Build renderer (reference/debug, 8-bit exact-ish)
room-ui               + duke/   Duke HUD, menus, bonus screen, ANM player, automap
room-game             game-agnostic `GameRuntime` trait; Doom World and Duke world both implement it
```

Allowed edges: `room-build-engine` → `room-core` only (plus `room-build` for
the data types if they are shared). `room-duke` → `room-build`,
`room-build-engine`, `room-core`. `room-duke` must **not** depend on
`room-sim`. The same rules as `room-sim` apply: no render, audio, UI, input,
net or game dependencies.

### 4.2 The runtime interface (`room-game`)

Today `room_game::Engine` owns a `room_sim::World` and a 1/35 s tic. Generalise:

```rust
pub trait GameRuntime {
    /// Tics per second. Doom: 35. Duke: 30.
    fn tic_rate(&self) -> u32;
    fn tick(&mut self, cmds: &[GameCmd]);
    fn view(&self, alpha: Fixed) -> RuntimeView<'_>;   // enum { Doom(SimView), Build(BuildView) }
    fn drain_sound_events(&mut self) -> Vec<SoundEvent>;
    fn checksum(&self) -> u64;                           // netplay / verifier / determinism CI
    fn save(&self) -> Vec<u8>;
    fn load(&mut self, bytes: &[u8]) -> Result<()>;
    fn game_state(&self) -> GameFlowState;               // level running, intermission, finale, title
}
```

- `GameCmd` is an enum: `Doom(TicCmd)` / `Duke(DukeInput)`. `DukeInput` is
  the vanilla 1.5 `input` struct: `avel: i8, horz: i8, fvel: i16, svel: i16,
  bits: u32` **(verify field order and widths in `DUKE.H`)**. Add it to
  `room-proto` with its own tag so the server and lockstep transport carry it.
  Bump the protocol version.
- The frame loop (`docs/01 §4`) uses `runtime.tic_rate()` instead of the
  constant. UI keeps its own 35 Hz `UiClock` for Doom. For Duke, the menus
  run at 30 Hz (Duke menu animations are tic-based) **(verify)**.
- Interpolation: Build has no `prev`/`cur` snapshots in vanilla. Add them in
  `room-duke` (sprite xyz/ang, sector floorz/ceilingz, wall xy for moving
  sectors, player view). This is presentation only. The playsim must never
  read them.
- `room-server` verifier, `room-net` lockstep, `.fdem`/`.fsave` all go
  through `GameRuntime`, so Duke multiplayer and verification come for free
  once the trait is implemented.

### 4.3 Game selection

`room-vfs` IWAD detection already recognises a GRP. Replace "convert to WAD"
with "construct `DukeRuntime`" when the shell starts a Duke game. Keep
`--export-udmf` (room-cli) for the old path. The lobby's `game: duke3d` maps
to the native runtime. Content fingerprint = hash of the base GRP + add-on
GRPs + CON files + user map, in load order.

---

## 5. `room-build-engine`: world model and engine primitives (clean-room)

### 5.1 Numeric model

- **Everything in the playsim is integer**, exactly as Build: `i32` coordinates
  (x/y in Build units, z in Build units ×16 relative to x/y, i.e. `z>>8`
  ≈ x/y scale after the `<<4` z-scale convention **(verify)**), 2048-unit
  angles (`ang & 2047`), 16.16 or 14-bit fixed trig from the GRP's
  `TABLES.DAT` `sintable[2048]`. No floats in sim code. Use the
  `room-core` fixed helpers where they match, otherwise add Build-specific
  ones (`mulscale`, `divscale`, `dmulscale`, `scale`, `klabs`, `ksqrt`,
  `getangle`) with **wrapping/truncation semantics specified per function**,
  because x86 `imul`/`idiv` semantics matter for demo sync.
- Overflow: Build relies on 32-bit wraparound in places. Use
  `wrapping_*` explicitly. Never `checked_*` + panic. Division by zero must
  have a defined result (document what the reference does; a fuzz target
  must hit it).
- Random: `krand()` (engine LCG on `randomseed`, Build constant
  `randomseed*27584621+1`, returns the high 16 bits **(verify)**) and
  `TRAND` = Watcom C `rand()` (ANSI LCG `1103515245/12345`, 15-bit result
  **(verify against DEMO sync)**). Both seeds are part of world state and of
  the checksum.

### 5.2 World model

Fixed-capacity arrays with vanilla limits, so `.SAV` and index-based CON
semantics hold: `MAXSECTORS 1024`, `MAXWALLS 8192`, `MAXSPRITES 4096`,
`MAXSTATUS 1024`, `MAXTILES 6144` (Atomic) **(verify each)**.

```rust
pub struct BuildWorld {
    pub sectors: Vec<SectorT>,      // len == numsectors, capacity MAXSECTORS
    pub walls:   Vec<WallT>,
    pub sprites: Box<[SpriteT; MAXSPRITES]>,
    // Build's intrusive linked lists — keep them, CON and game code depend on the order:
    headspritesect: [i16; MAXSECTORS+1], headspritestat: [i16; MAXSTATUS+1],
    prevspritesect/nextspritesect/prevspritestat/nextspritestat: [i16; MAXSPRITES],
    pub randomseed: i32,
    // parallax, visibility, etc.
}
```

`SectorT`/`WallT`/`SpriteT` mirror the v7 on-disk structs field for field
(they are already parsed in `room-build/src/map.rs`). **Iteration order is
behaviour.** `insertsprite`/`deletesprite`/`changespritesect`/
`changespritestat` must reproduce the vanilla free-list and head-insertion
order, or `moveactors` will visit sprites in a different order and demos
desync.

### 5.3 Primitives to specify and implement

Each one needs a spec file in `docs/build-engine-primitives/` (clean-room,
§3), a unit test suite and a fuzz target.

| Primitive | Purpose | Notes |
|-----------|---------|-------|
| `inside(x,y,sect)` | point-in-sector (even-odd over the sector's wall loops) | Edge rules matter (points on walls) |
| `updatesector(x,y,&sect)` | find sector, searching neighbours first then all | Order of search is behaviour |
| `getzsofslope(sect,x,y)` | ceiling/floor z with `heinum` slopes | Fixed-point exactness |
| `clipmove(&pos,&sect,xvect,yvect,walldist,ceildist,flordist,cliptype)` | the collision core: slide along walls/sprites, return hit info | Hardest primitive. Clip-shape rules for walls, face/wall/floor sprites, `clipdist`, 3 bounce iterations **(verify)** |
| `pushmove` | push an object out of walls | |
| `getzrange` | highest floor / lowest ceiling under a disc, incl. sprites | Returns hit sprite/sector codes |
| `hitscan` | ray vs walls, ceilings/floors (sloped), sprites (face/wall/floor) | cstat 256 hitscan bit, one-sided masks |
| `neartag` | "use" target: nearest tagged wall/sector/sprite | Drives every `open` action |
| `cansee` | sector-portal line of sight | |
| `dragpoint` | move a vertex and every wall sharing it | Needed by every moving sector |
| `rotatepoint`, `getangle`, `ksqrt`, `nextsectorneighborz`, `sectorofwall`, `lastwall`, `alignflorslope`/`alignceilslope` | helpers | |
| `insertsprite`/`deletesprite`/`changespritesect`/`changespritestat` | list management | Order = behaviour (§5.2) |
| `animatewalls` / tile `picanm` animation timing | animation clock | Uses `totalclock` (120 Hz) **(verify)** |

Do these first and test them hard (golden outputs from black-box runs of a
reference engine on generated maps). Everything above them depends on them.

---

## 6. `room-duke` part 1: CON compiler and VM

Replace `condata.rs` with a faithful implementation of the 1.5 compiler
(`GAMEDEF.C`) and interpreter (`execute()` / `parse()` in `GAMEDEF.C`). It is
GPL game code, so translation is allowed.

### 6.1 Compiler

- Input: `GAME.CON` (+ `include`d `DEFS.CON`, `USER.CON`) read from the VFS,
  or `-x` override. Same tokenizer rules as vanilla: case-insensitive keywords,
  `//` and `/* */` comments, numbers and labels, `define` substitution at
  compile time.
- Output: a flat `Vec<i32>` script image + `actorscrptr[MAXTILES]` +
  `actortype[]` + label table + `sounds[]` (`definesound` fields: file,
  minpitch, maxpitch, priority, type bits, volume) + `quotes[]` +
  `level_names`/`level_file_names`/`partime`/`designertime`/`music_fn`
  + `definevolumename` / `defineskillname` + `gamestartup` numbers (the
  long parameter list whose length depends on CON version).
- Keep the **same opcode numbering and argument layout** as vanilla so that
  `.SAV` files (which store script *offsets* in `hittype`/`t_data`) restore
  correctly. This is why a re-designed bytecode is not acceptable.
- Full keyword list for 1.5: every keyword in `GAMEDEF.C`'s `keyw[]` table
  (~ 130). Emit a compile error with span for unknown keywords. Unknown
  keywords do not abort vanilla in every case; match its leniency.
- 1.3D dialect (tier 2): fewer keywords and a shorter `gamestartup`. Gate it
  behind a `ConDialect` enum detected from the GRP.

### 6.2 VM

- `execute(sprite_i, player_p, dist_x)` runs an actor's code once per tic
  from `moveactors`/`movestandables` etc., with the vanilla per-actor state:
  `hittype[i].temp_data[0..6]` (`t[0]` = count, `t[1]` = current action
  ptr, `t[2]` = action frame counter, `t[3]`, `t[4]` = action offset,
  `t[5]` = ai/move ptr **(verify meanings)**), `sprite.hitag/lotag` reuse,
  `spriteext`-free.
- Implement **every** command and condition with vanilla semantics, including
  the side effects folded into `move` (`faceplayer`, `geth`, `getv`,
  `randomangle`, `faceplayerslow`, `spin`, `faceplayersmart`,
  `fleeenemy`, `jumptoplayer`, `seekplayer`, `furthestdir`,
  `dodgebullet`) and `ifrnd` → `TRAND` usage.
- `killit` and `break`/`ends` control flow exactly as vanilla (`killit_flag`).
- Event hooks do not exist in 1.5 CON (`onevent` is EDuke32). Tier 3 adds them
  behind the dialect flag.
- Performance budget: E2L1 at full monster count ≤ 1 ms per tic on wasm32
  (browser). Interpret directly over `i32` words. No allocation per tic.

### 6.3 Acceptance

- `GAME.CON` from every supported GRP compiles. The script image is
  byte-identical to the reference image (dump `script[]` from Chocolate Duke3D
  once and keep the hash in a git-ignored fixture manifest).
- An opcode coverage test hits every opcode at least once over the demo and
  route corpus.

---

## 7. `room-duke` part 2: vanilla game logic

Translate the hardcoded logic. Keep vanilla function boundaries, names and
call order so cross-referencing stays easy (`actors.rs::moveactors`, …).

### 7.1 Tic order

One 30 Hz game tic (`domovethings` → `moveobjects` in vanilla) **(verify
exact order and per-player loop placement)**:

```
for each player: processinput(p); checksectors(p)
movefta()            // wake actors that can see a player
moveweapons()        // projectiles (STAT_PROJECTILE)
movetransports()     // SE7 transporters (STAT_TRANSPORT)
moveplayers()        // player sprites / dead bodies
movefallers()
moveexplosions()     // STAT_MISC
moveactors()         // STAT_ACTOR: CON execute + hardcoded per-picnum logic
moveeffectors()      // STAT_EFFECTOR: all SE lotags
movestandables()     // STAT_STANDABLE
doanimations()       // sector z animations (animateptr/goal/vel)
movefx()             // ambient sound sprites (MUSICANDSFX)
animatewalls()
// + earthquake, cyclers (STAT_CYCLERS?), lockclock++, etc.
```

### 7.2 Catalogue of logic to port

| Source file | Content | Notes |
|-------------|---------|-------|
| `ACTORS.C` | `moveactors`, `moveweapons`, `moveeffectors` (every SE), `movetransports`, `movestandables`, `moveexplosions`, `movefta`, `hitradius`, `movesprite`, `ifhitbyweapon`, `badguy`, `ssp`, `guts`, `ms` | The largest piece. SE lotag table in Appendix B |
| `SECTOR.C` | `operatesectors` (all sector lotags), `operateactivators`, `operatemasterswitches`, `operaterespawns`, `checkhitswitch`, `checkhitwall`, `checkhitsprite`, `checkhitceiling`, `checksectors`, `animatecamsprite`, `allignwarpelevators`, `setanimation`, `doanimations` | Switch/tile tables become data tables generated from source |
| `PLAYER.C` | `processinput` (movement, friction, jump, crouch, jetpack, swim, falling, steroids), `shoot` (all weapon projectiles/hitscans by picnum), weapon state machine (`kickback_pic` per weapon), `displayweapon` (HUD gun animation), `checkweapons`, `addweapon`, `selectweapon`, quick kick, inventory use, `doincrements` (timers), `hurt floors`, view (horiz, look, tilt) | `displayweapon` is presentation: it computes from state, it must not mutate it |
| `GAME.C` | `spawn` (per-picnum spawn rules), `EGS`, `animatesprites` (render-time sprite tweaks: rotation frame selection, shadows, `ifn` cases), `displayrooms` (camera, mirror, view), `moveclouds`, cheats, `domovethings`, quote system, `nonsharedkeys` | Split sim (`spawn`) and presentation (`animatesprites`, `displayrooms`) |
| `PREMAP.C` | `enterlevel`, `prelevel` (sector/sprite preprocessing, `numanimwalls`, mirrors, cyclers, respawns), `resetplayerstats`, `resetweapons`, `resetinventory`, `newgame`, player spawn | Order of prelevel passes is behaviour |
| `GAMEDEF.C` | CON compiler + VM | §6 |
| `MENUES.C` | menus, save/load, `loadplayer`/`saveplayer` | Save format §9.4 |
| `RTS.C`, `SOUNDS.C` | remote ridicule, sound selection/priorities | §9.2 |

Rules:

- **Sim/presentation split.** Vanilla mixes rendering-time logic into the
  frame (`animatesprites` changes `tsprite` copies only, which is fine;
  anything that writes `sprite[]` from the draw path is a bug to flag). Put
  presentation code behind `BuildView` so the renderer never mutates the
  world (AGENTS rule 2).
- **Vanilla bugs are features** in demo-compat mode (default). If a fix is
  wanted, add a `duke_compat` flag set (similar to Doom compat flags in
  `docs/19-compatibility.md`), default off, never on during demo playback or
  netplay unless every peer agrees.

### 7.3 Player physics

Port `processinput` exactly: velocities in Build units with vanilla friction,
`fvel/svel` from `DukeInput`, `<<14` scalings, `TICSPERFRAME`-dependent terms,
`ps.posxv/posyv`, `jumping_counter` curve, `falling_counter` damage,
`hard_landing`, ducking via `SECTOR` clearance, jetpack, underwater (`lotag 2`,
`SE7` water links), `on_crane`, `transporter_hold`. The current Doom-based
approximations (23 §3 "Weapons, items, player") are discarded for the native
runtime.

---

## 8. Rendering: `room-render/src/build/`

The current path turns Build into Doom BSP leaves. The native path renders
Build's sector graph directly on wgpu, the way Raze's hardware renderer does
(sector-portal traversal), so overlapping sectors and moving walls need no
special cases.

### 8.1 Visibility

- Start in the camera's sector. Traverse through two-sided walls (portals),
  clipping each portal to the current screen-space window (1D angular
  clipper per column span, like a Build "bunch" walk). Sectors may be
  visited more than once through different portals. That is what makes
  overlapping space correct. Cap the recursion depth and visited count.
- The existing `room-render/src/clipper.rs` (angular clipper used for BSP)
  is reusable.
- Output: a draw list of wall pieces (upper/lower/masked), floor/ceiling
  polygons (triangulated per sector, **re-triangulated when a sector's walls
  moved this tic**; cache keyed by a per-sector geometry revision counter set
  by `dragpoint`), and sprites (`tsprite` list after `animatesprites`).

### 8.2 Surface rules

| Feature | Rule |
|---------|------|
| Wall texturing | `xrepeat`/`yrepeat`/`xpanning`/`ypanning`, cstat 4 (bottom align), 8 (x-flip), 256 (y-flip), 2 (swap bottoms with `nextwall`), 16 (masked), 32 (one-way masked), 128/512 (translucency level) |
| Floors/ceilings | stat 1 parallax, 2 sloped (`heinum`), 4 swap xy, 8 double smoosh, 16/32 flip, 64 relative alignment to first wall, panning, `floorz`/`ceilingz` |
| Sprites | cstat 1 block, 2 translucent, 4/8 flip, 16 wall, 32 floor, 64 one-sided, 128 real-centred, 256 hitscan, 512 reverse trans, 32768 invisible. Face sprites rotate by view (5/8-view tiles, picnum offset per angle as `animatesprites` decides). `xrepeat`/`yrepeat` scale, `xoffset`/`yoffset` + tile `picanm` offsets |
| Shading | Build palookup: `PALETTE.DAT` shade tables (`numshades`, normally 32), shade = surface shade + distance × `visibility` (global `visibility` and per-sector `visibility` byte), clamped. Implement in a shader that indexes an 8-bit tile and a palookup texture (256 × numshades × numpals) |
| Palette swaps | `pal` on walls, floors, ceilings and sprites selects a `LOOKUP.DAT` palookup. Also `floorpal`/`ceilingpal` |
| Translucency | Two levels from the `PALETTE.DAT` translucency table (65536-byte blend table). In classic mode, sample the table. In true-colour mode, use the 33%/66% alpha equivalent |
| Parallax sky | `pskybits`, `pskyoff[]` (per sky tile, set by `prelevel` from the parallax tile), `parallaxyscale`, `parallaxtype`. Sky is drawn per sector (each parallax sector draws its own tile). Replaces the composed `BSKYnnnn` texture |
| Mirrors | Wall with `overpicnum == MIRROR` and cstat 32 + the mirror sector behind it. Render the reflected view into the mirror sector (vanilla `displayrooms` mirror pass) |
| Camera screens | `VIEWSCREEN` sprites draw `camsprite`'s view into a tile buffer. Room already has camera-to-texture. Wire it to `BuildView` |
| Screen tint | Player `pals[]` / `pals_time` full-screen palette blend (damage red, pickup, night vision green, underwater) |
| View | `horiz` (Y-shearing, not true pitch, in classic mode), `rotscrnang` roll, third-person view (F7) using the vanilla camera offset/clip |

### 8.3 Modes

- **Classic** (default for Duke): 8-bit tiles + palookup + translucency table
  in shaders, Y-shear look, nearest filtering. This is what T1.6 is measured
  in.
- **Enhanced** (optional): true colour, filtering, real perspective pitch,
  Room dynamic lights off by default. Never affects the sim.
- `room-render-soft` may get a CPU Build renderer for debugging and golden
  references. It is not required.

### 8.4 Acceptance

Golden captures at 320×200 and 640×480 for a list of viewpoints (E1L1
start, E1L1 cinema, E1L2 underwater, E2L1 subway, E1L2 mirror, a
rooms-over-rooms spot, a sloped parallax spot, every sky). The references come
from Chocolate Duke3D screenshots at the same pose. Compare with a perceptual
tolerance (`room-render` golden harness).

---

## 9. Front end, audio, input, saves, net

### 9.1 UI (`room-ui/src/duke/`)

- **Fonts**: `STARTALPHANUM` (small), `BIGALPHANUM`, `MINIFONT`, the
  `TABLES.DAT` text fonts. Vanilla `gametext`/`minitext`/`menutext` layout.
- **Menus** (`MENUES.C`): main, new game → episode → skill, options (game,
  controls, sound, video subset), load/save with **10 slots** and a 3D
  preview thumbnail, help (`TEXTSTORY`, `F1HELP`), credits, order info
  (shareware), quit confirm with random quote. Port the layout and cursor
  (spinning nuke `SPINNINGNUKEICON`).
- **HUD**: full status bar, mini HUD (`screen size` steps), weapon overlay
  from `displayweapon`, inventory selector, access cards, quotes (`fta`/`ftq`,
  pause, `user_quote`), crosshair, frag bar in DukeMatch.
- **Bonus screen** (`dobonus`): kills/total, secrets, your time, par,
  3D Realms time, with the stepping sounds and the `BONUSSCREEN` animation.
- **ANM playback**: full LPF playback at the vanilla rate, with the per-frame
  sound cues for each cutscene (hardcoded frame → sound tables in
  `GAME.C`, `playanm`/`endanimsounds`/`logoanimsounds`) **(verify table)**.
  Replace the current still-frame title/finale.
- **Automap** (`drawoverheadmap`): wall lines by type, sprites, follow mode,
  zoom, rotate. Vanilla colours.
- **Cheats**: `dn` prefix set: `dnkroz`/`dncornholio` (god), `dnstuff`,
  `dnitems`, `dnweapons`, `dninventory`, `dnkeys`, `dnclip`, `dnscottyEL`,
  `dnview`, `dnhyper`, `dnunlock`, `dncashman`, `dnrate`, `dnskillN`,
  `dnmonsters`, `dnshowmap`, `dnallen`, `dncosmo`, `dntodd`, `dnbeta`,
  `dndebug`. Take the definitive list from `cheatquotes[]` in `GAME.C`.
- **F-keys**: F1 help, F2 save, F3 load, F4 sound, F5 music, F6 quicksave,
  F7 view mode, F8 messages, F9 quickload, F10 quit, F11 gamma.

### 9.2 Audio

- `definesound` semantics: pitch randomisation (`minpitch`..`maxpitch`),
  priority, type bits (repeat/loop, MSFX, Duke talk (exclusive voice), parental
  lock, global (no 3D)), volume. Port `xyzsound`/`spritesound`/`sound`
  selection, voice stealing by priority, per-sprite sound slot limits and
  `Sound[num].num` counting (CON `ifsound` depends on it).
- 3D: Build distance attenuation and pan from `SOUNDS.C`. Feed Room's mixer
  with explicit gain/pan per voice, not its Doom attenuation model.
- `MUSICANDSFX` sprites: lotag = sound, hitag = radius. They are played and
  stopped by `movefx` (sim) as **sound events**, never from the renderer.
- Music: `.MID` from the GRP through `room-music` (General MIDI / SoundFont
  default; OPL optional).
- RTS: `DUKE.RTS` lumps on Alt+F1..F10 in multiplayer.
- Parental lock (`ud.lockout`): swaps adult sprites/sounds. It's vanilla, so
  implement it, default off.

### 9.3 Input

Build `DukeInput` from Room bindings: `fvel`/`svel`/`avel`/`horz` with
vanilla scaling and running, `bits` for jump, crouch, fire, aim up/down, look
left/right/up/down, centre view, holster, inventory left/right/use, quick
kick, holoduke, jetpack, night vision, medkit, steroids, weapon 1–10,
previous/next weapon (1.5), turn around, open, pause, mouse aiming toggle
**(take the bit layout from `DUKE.H` / `GAME.C getinput`)**. Remove the
Doom-flavoured `up_move`/`+quickkick`/`+invuse` plumbing from the Duke path.

### 9.4 Saves

- Native `.fsave` for `DukeRuntime` = the world arrays + `hittype[]` +
  `ps[]` + `ud` + script image hash + RNG seeds + animation/cycler/mirror
  lists + `totalclock`/`lockclock`. Versioned, deterministic serialization.
- Original `GAME#.SAV` (1.5): implement `loadplayer`/`saveplayer` byte layout
  directly over the native world (`sav.rs` parsing stays; the "apply fields to
  a Doom world" layer goes away). Round-trip test (T1.8).

### 9.5 Demos

- `.DMO` (1.3D/1.5) playback through `DukeRuntime` at 30 Hz with original
  `DukeInput` (no conversion to `TicCmd`). Recording `.DMO` in 1.5 format too,
  so route files (T1.3) can be checked in the reference engine.
- `.fdem` for Duke stores `DukeInput` streams + per-tic checksums.

### 9.6 Multiplayer

- Co-op and DukeMatch through `GameRuntime` + lockstep: `ud.coop`,
  `ud.marker`, `ud.ffire`, `ud.respawn_monsters/items/inventory`,
  `ud.monsters_off`, frag counting, player colours via `pal`, spawn points
  (`APLAYER` with lotag), weapon stay in DukeMatch.
- Server verifier runs `DukeRuntime` headless and compares checksums.

---

## 10. Testing strategy

### 10.1 Assets

No commercial data in git (AGENTS rule). Tests that need `DUKE3D.GRP` read it
from `testdata/duke/duke3d.grp` and skip when it's absent, as today. Generated
fixtures (mini `.MAP`s, ART with synthetic tiles, synthetic CON) are committed.
Write a tiny `.MAP` writer in `room-testkit` for scenario maps.

### 10.2 Reference traces (the core of T1.1)

1. Build a **trace-instrumented reference**: Chocolate Duke3D (a minimal SDL
   port of the original source) patched to print, every game tic:
   `tic, randomseed, global_random, numsprites, for each player
   {posx,posy,posz,ang,horiz,health(extra),curr_weapon,ammo[]},
   hash(sprite[x,y,z,picnum,statnum] for all live sprites)`.
   Keep that patch outside the repo (it links BUILDLIC code). Store only its
   **output traces**, git-ignored next to the GRP (`testdata/duke/traces/`),
   plus their SHA-256 in a committed manifest so CI on a machine that has them
   can verify.
2. `room-duke` emits the same trace format. The test diffs the two and
   reports the **first diverging tic and field**. That is the main debugging
   tool, so make the diff output excellent.
3. Corpus: `DEMO1`–`3.DMO` (Atomic), the shareware `DEMO1`–`2` (1.3D, tier 2),
   plus the human-recorded per-map route `.DMO`s from T1.3.

### 10.3 Scenario tests

One mini map per mechanic (each SE lotag, each sector lotag, each switch kind,
each weapon, each enemy, each pickup, water transitions, conveyors, crushers,
C-9 groups, mirrors, cameras). Assert on world state after N tics. Port the
intent of the existing `duke3d_play.rs` cases (a sliding door opens, the
subway follows locators, …) to native scenarios.

### 10.4 Robustness

- Fuzz targets: MAP, ART, CON compiler, DMO, SAV, ANM, `clipmove`/`hitscan`
  with random geometry.
- Random-input soak: 30 000 tics of random `DukeInput` on every map, no panic,
  determinism across x86-64/aarch64/wasm32 (T1.10).
- `cargo clippy -D warnings`, wasm32 build of `room-web` with the Duke
  runtime (per AGENTS §5).

---

## 11. Milestones (work in this order)

Add these as **D0–D11** to `docs/20-roadmap-testing.md` and
`docs/21-milestone-progress.md`. Don't start a milestone before the previous
one's acceptance passes (AGENTS §5).

| # | Milestone | Deliverables | Acceptance |
|---|-----------|--------------|------------|
| D0 | ADR + scaffolding | ADR 0004, crates `room-build-engine` / `room-duke` (empty, wired into CI + wasm build), `GameRuntime` trait in `room-game` with the Doom `World` behind it, configurable tic rate | All existing tests pass unchanged; Doom demos still verify |
| D1 | Reference traces | Patched reference engine (out of tree), trace format spec `docs/duke-trace-format.md`, trace diff tool in `room-cli` (`room-cli duke trace-diff`) | Traces for DEMO1–3 generated and manifest committed |
| D2 | Engine primitives (clean-room) | Specs in `docs/build-engine-primitives/`, implementation, unit + fuzz tests | Primitive golden tests vs black-box reference outputs on 50+ generated maps |
| D3 | World load + static view | `BuildWorld` from `.MAP`, `prelevel`-equivalent passes needed for display, Build renderer path (classic mode, no sprites yet), free-fly camera | E1L1–E4L11 render. Overlapping-sector viewpoints are correct. Golden captures (walls/flats/sky) |
| D4 | CON compiler | Full 1.5 compiler, script image | Script image hash equals reference for Atomic `GAME.CON` |
| D5 | Core sim loop | 30 Hz tic, `processinput` (movement only), `spawn`, sprite lists, `movefta`, `moveactors` + CON VM, `TRAND`/`krand`, sprites in renderer | DEMO1 stays in sync for the first 300 tics (or to the first diverging feature, logged) |
| D6 | Weapons + damage | `shoot`, `moveweapons`, `hitradius`, `checkhitwall/sprite/ceiling`, `displayweapon`, HUD weapon overlay | DEMO1 in sync to the end |
| D7 | Sectors + effectors | `operatesectors`, every SE in `moveeffectors`, `doanimations`, transports, switches, masterswitches, respawns, cyclers, mirrors, cameras | DEMO2 + DEMO3 in sync to the end. All SE scenario tests pass |
| D8 | Audio + front end | Sound semantics, `MUSICANDSFX`, menus, HUD, bonus screen, ANM playback with cues, automap, cheats, F-keys, third-person | UI checklist + golden screenshots (T1.6, T1.7) |
| D9 | Saves + demos | Native `.fsave`, `GAME#.SAV` load/save round-trip, `.DMO` recording, `.fdem` | T1.8. A recorded `.DMO` syncs in the reference engine |
| D10 | Multiplayer + server | `DukeInput` in `room-proto`, lockstep, coop/DukeMatch rules, verifier | T1.9, T1.10 |
| D11 | Full campaign + user content | Route `.DMO` per map, user maps, `-x` CON, add-on GRP layering, switch shells (`room-app`, `room-web`, lobby) to the native runtime, move the converter under `export/` | T1.2, T1.3, T1.11. Then mark 23 §3 obsolete for gameplay |

Tier 2 (1.3D dialect, add-ons, World Tour) and tier 3 (EDuke32 dialect,
DEF) are separate milestone tracks after D11.

### Rough size

The vanilla game code to translate is on the order of 30–40k lines of C
(`ACTORS`, `PLAYER`, `SECTOR`, `GAME`, `GAMEDEF`, `PREMAP`, `MENUES`).
The engine primitives are a few thousand lines but carry the most risk (the
clean-room process, plus `clipmove` exactness). The renderer path is
comparable in size to the current `scene.rs`. Expect D2, D5 and D7 to dominate.

---

## 12. Decisions the project owner must make before D0

1. **Native runtime vs keep converting.** This spec recommends native (§0).
   The alternative is to keep extending the converter and give up T1.1/T1.4/
   T1.8 (demo sync, real CON, original saves) and accept heuristic geometry.
   "Full support" as defined in §1 is not reachable on that path.
2. **Clean-room budget for engine primitives (§3).** It needs two separate
   agents/sessions with a spec hand-off. If the owner accepts BUILDLIC terms
   for a non-commercial build, a translated engine is faster, but then Room
   can't be distributed as GPL-3 only. Recommend clean-room.
3. **Reference engine for traces.** Chocolate Duke3D (closest to vanilla
   1.5 behaviour) is recommended. Confirm that its demo playback is in sync
   for DEMO1–3 before relying on it. If it is not, fall back to DOSBox +
   original `DUKE3D.EXE` with memory-dump tracing (slower, but authoritative).
4. **Fate of the Doom-side Duke extensions** in `room-sim` (`ContactDamage`,
   water portal 200, `Sector_Stretch`, Duke damage types, …). Keep them as
   documented GZDoom deviations, or remove them after D11.
5. **Default renderer mode** for Duke in browsers without WebGPU (WebGL2
   fallback): classic mode needs an integer-texture palookup path. Confirm
   the WebGL2 fallback supports it or accept true-colour there.

---

## Appendix A — Build `cstat`/`stat` bits (from `BUILDINF.TXT`, data)

**Wall cstat:** 1 blocking · 2 swap bottoms · 4 align bottom · 8 x-flip ·
16 masked · 32 one-way masked · 64 hitscan blocking · 128 translucent ·
256 y-flip · 512 translucent reverse.

**Sprite cstat:** 1 blocking · 2 translucent · 4 x-flip · 8 y-flip ·
16 wall sprite · 32 floor sprite · 64 one-sided · 128 real-centred ·
256 hitscan blocking · 512 translucent reverse · 32768 invisible.

**Sector floorstat/ceilingstat:** 1 parallax · 2 sloped · 4 swap xy ·
8 double smoosh · 16 x-flip · 32 y-flip · 64 align to first wall ·
(128/256 masking: later Build, unused by Duke 1.5) **(verify)**.

## Appendix B — Sector Effector (SE) lotags, indicative

Names are indicative. **The authority is `moveeffectors()` /
`spawn()` in `ACTORS.C`/`GAME.C`.** Generate the final table from source
and keep it as data in `room-duke`.

| SE | Behaviour | SE | Behaviour |
|----|-----------|----|-----------|
| 0 | rotating sector (with SE1) | 19 | ceiling falls when wall shot |
| 1 | rotation pivot | 20 | stretch bridge |
| 2 | earthquake | 21 | drop floor |
| 3 | light out when shot / flicker | 22 | teeth door prongs |
| 4 | random light flicker | 23 | one-way teleport exit |
| 5 | (turret / boss-specific, **verify**) | 24 | conveyor |
| 6 | subway engine | 25 | piston |
| 7 | transporter / water link | 26 | escalator |
| 8 | door-up lights | 27 | demo camera |
| 9 | door-down lights | 28 | lightning |
| 10 | door auto-close | 29 | floating (waves) |
| 11 | swinging door | 30 | two-way train |
| 12 | light switch | 31 | floor rise |
| 13 | C-9 explosive | 32 | ceiling fall |
| 14 | subway car | 33 | quake debris |
| 15 | sliding door | 34 | conveyor (variant) |
| 16 | reactor | 35 | **verify** |
| 17 | transport elevator | 36 | shooter |
| 18 | incremental raise/lower | 40–45 | unfinished room-over-room (unused in shipped maps) |

## Appendix C — Sector lotags, indicative

| Lotag | Meaning | Lotag | Meaning |
|-------|---------|-------|---------|
| 1 | above water | 22 | split door |
| 2 | underwater | 23 | swinging door |
| 9 | sliding star door | 25 | sliding door |
| 15 | warp elevator | 26 | split star door |
| 16/17 | platform down/up | 27 | stretch bridge |
| 18/19 | elevator down/up | 28 | drop floor |
| 20 | ceiling door | 29 | teeth door |
| 21 | floor door | 30 | rotate-rise bridge |
| 31 | two-way train | 32767 | secret |
| 65535 (−1) | end level | 65534 (−2) | end level (alternate, **verify**) |

Hurt floors are by floor tile (slime, lava, …), not lotag. Take the tile list
from `PLAYER.C`/`SECTOR.C`. The current converter's table
(`room-build/src/duke.rs`) is a starting point.

## Appendix D — Sources

- 3D Realms Duke Nukem 3D source release (GPL-2.0-or-later), v1.5 game code.
- Ken Silverman, `BUILDINF.TXT` (format and flag documentation).
- Chocolate Duke3D (reference executable for traces).
- Raze — <https://github.com/ZDoom/Raze> (secondary behavioural reference;
  ships `BUILDLIC.TXT`, see §3) — <https://raze.zdoom.org/about>.
- BUILDLIC text — <https://ftp.riken.jp/Linux/gentoo-portage/licenses/BUILDLIC>.
- Existing Room docs: `docs/23-build-engine.md`, `docs/01-architecture.md`,
  `docs/02-conventions.md`, `docs/19-compatibility.md`.

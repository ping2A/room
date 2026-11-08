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
3. **Timing and RNG.** Duke runs at **30 Hz** with its own RNG (`krand`);
   Chocolate Duke's gameplay `TRAND` macro aliases that same RNG. Room runs
   everything at 35 Hz, so original
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

**Implementation checkpoint (2026-11-06):** D6's native weapon path now has
the Atomic knee weapon cadence (impact at kickback tic 7, held repeat after
tic 14), short-range hitscan damage for badguy sprites, and queued `KICK_HIT`
audio when the reach check connects. The weapon ART frame follows kickback
state in the native soft view. `sync.bits` weapon selectors now trigger direct
owned-weapon selection, previous/next cycling, or a buffered request while a
weapon is firing; selection queues Chocolate's weapon-specific cue. The
Devastator now starts its two-shot five-tic burst, consumes one round per shot,
alternates its firing offset, uses the shared 48-angle `aim` cone for target
selection and queues CAT/RPG fire sounds. Projectile flight and impact remain
partial. The Devastator RPG projectile now also preserves the projectile CON
strength through Chocolate's `EGS()` initialization, `TRAND&7` addition, and
quartering step, and copies its scripted actor `hitag`. Focused
timing test passes for the earlier knee cadence change; recent selection and
Devastator changes were compile-checked only. This remains partial:
wall impact effects, full Build target filtering/impact semantics, and DEMO1
native sync after tic 69 remains open; D6 is not complete.

**Devastator start-frame follow-up (2026-11-07):** Chocolate sets kickback 1,
toggles `hbomb_hold_delay`, and queues `CAT_FIRE` while processing the input,
then the same tic's weapon branch increments it to frame 2. Room now follows
that ordering; projectile bursts land on frames 3 and 5 instead of starting a
tic late. Static `player.c::processinput` review and targeted compile only; no
gameplay/demo comparison.

**D6 ceiling-light tile lookup (2026-11-07):** the player hitscan ceiling path
now resolves the six Atomic source/busted light pairs and `GLASSPIECES` through
active CON labels, with Atomic tile IDs as fallbacks. This lets modified CON
definitions drive the same break and shard path instead of silently using
Atomic-only IDs. The frozen GREENSLIME shatter path now resolves the same
script-defined `GLASSPIECES` label. Static `sector.c::checkhitceiling`,
`game.c::ceilingglass`, and `actors.c` review, formatting, diff check and
targeted compile only; no gameplay or Chocolate comparison.

**Auto-aim update (2026-11-07):** Chocolate sync bit 6 now sets
`player.auto_aim` (2 = full, 1 = bullet-only). The desktop path emits the
Atomic default full-autoaim bit, `.DMO` input preserves its recorded bit, and
legacy player saves default to mode 2. RPG, Freezer, Shrinker, Grower, Shotgun
and Devastator use the shared Chocolate target cone and solve aim height from
ART metadata. Hitscan pistol/shotgun/chaingun retains aim in both enabled modes
(Chocolate's `auto_aim != 0`); the projectile weapons remain gated to mode 2.
Desktop now exposes archived `duke_autoaim` (1 = bullet-only, 2 = full) through
the console CVar and the Duke F4 Sound Setup menu; left/right and Enter cycle
the setting. The browser uses the Atomic full-mode default and still lacks a
user setting. Static source review and
compile checks only; no runtime comparison.

**Atomic demo header correction (2026-11-07):** the v117 DMO reader and writer
now account for the per-player aim-mode byte and ten weapon-choice bytes that
Chocolate writes after the fixed header. The reader previously skipped only
one byte per player, which misaligned the compressed tic stream. The parser
now exposes each seat's input stream and preserves aim-mode/weapon-choice
metadata, and the encoder accepts equal-length streams for up to eight players
and interleaves each tic in Chocolate order. Native session recording,
multiplayer playback and reference sync remain open. Static Chocolate source
review and compile checks only; no tests or runtime/reference comparison.

**Native F7 shoulder view (2026-11-07):** desktop Duke F7 toggles a camera
behind the player and suppresses the first-person weapon. Its desired distance
ramps by 4096 Q16 units per 30 Hz tic to Chocolate's 65536 maximum; a rearward
`CLIPMASK1` hitscan shortens it at active face sprites, walls and sector planes,
keeping four map units from the surface, and the camera Build sector is updated
before rendering. Room saves retain the toggle and distance, and Atomic save
import reads `over_shoulder_on` from the 32-bit player record. Sloped-plane
occlusion, exact sprite offsets, camera roll/animation and screenshots still
need implementation/validation.
Static `game.c::view` / ABI review and compile checks only; no tests, gameplay
or screenshot comparison.

**D7 VIEWSCREEN camera activation correction (2026-11-08):** static review
of Chocolate `sector.c::checksectors` showed that a monitor's `lotag` matches
the inactive `CAMERA1.hitag`, and Chocolate only scans live `STAT_ACTOR`
camera sprites with `yvel == 0`. Entry links the selected monitor's `owner`
to the camera, enables its latch, stores its pose for the player view, and queues
`MONITOR_ACTIVE`; exit clears the latch without replaying the entry cue. The
screen query now uses the 1280-unit use ray, its ART dimensions/offsets, wall
occlusion and three vertical sampling heights. Full Build `neartag` ordering
against every sprite/wall/sector operator is still approximate. Room's software
renderer continues to draw a 64×48 palette-quantized camera feed on a
render-only world copy, selecting the active monitor through its native
`owner` link to the camera sprite. Its indexed feed is cached per camera and
refreshed on first display, after each 12-tic `animatecamsprite` interval, or
when the renderer observes a tic reset. The snapshot world remains read-only;
only the renderer's palette-tile cache changes. Exact `xyzmirror`/`camsprite`
contents and tint, level-change cleanup, mirrors, and screenshot comparison
remain open. Static review of Chocolate `sector.c::checksectors` /
`animatecamsprite`, formatting, diff check and targeted compilation only; no
gameplay or reference comparison.

**D7 VIEWSCREEN exit priority (2026-11-08):** an active remote camera now
consumes the next use edge to exit and release the live CAMERA1 latch before
Room searches for another monitor in the ray. This matches Chocolate's
`newowner` control path and prevents a second visible monitor from trapping
the player in remote view. Static source review, formatting, diff check and
targeted compilation only; no gameplay or reference comparison.

**D7 camera-feed cache lifecycle (2026-11-08):** the software renderer now
discards its render-only VIEWSCREEN feed cache after campaign map replacement,
Atomic demo session replacement, and native save restoration. The prior cache
key used only camera sprite index and map tic, both of which may repeat after
those operations. This prevents the old level's 64×48 feed from being reused
for the first frame of a new/restored world. Other map-transition camera state
and exact tint remain open. Static call-path review, formatting, diff check,
and targeted compile only; no gameplay or reference comparison.

`moveplayers` now also exits the remote VIEWSCREEN pose when the APLAYER dies
and clears the selected CAMERA1 active-monitor latch, matching Chocolate's
`actors.c` APLAYER death branch (`newowner = -1`). Level-change cleanup and
exact tile tint remain open; the renderer cache now follows the 12-tic refresh
cadence but does not implement Chocolate's complete `xyzmirror` effects.
Static `actors.c` review, formatting, diff check and targeted compile only; no
gameplay or screenshot comparison.

**Native multiplayer session foundation (2026-11-07):** `DukeSession` now
configures up to eight local seats from map APLAYER starts, spawns seat-indexed
bodies, accepts per-seat input through `tick_inputs`, advances each player's
movement, and includes the roster in shared actor/damage, transport, explosion,
effector and standable passes. All seats process edge-triggered use actions,
access-card interaction, and the implemented weapon selection/firing paths in
the native tic; inventory use/cycling is also per seat, and `GameRuntime`
forwards every Duke seat command. Extra seats round-trip through Room `.fsave`.
The room server now relays native Duke inputs/frames at 30 Hz and can run an
optional native-session verifier against them; desktop/browser clients are not
yet wired to send and consume those frames, late-join spectator replay is
unavailable, and Atomic multiplayer save state/campaign carry are not restored.
This is a playsim/network foundation, not T1.9 completion. Static Chocolate
source inspection and compile checks only; no gameplay, test, network or
reference comparison.

**Player inventory and movement update (2026-11-07):** implemented the first
native `checksectors` inventory-action slice: first-aid healing, steroid use
and movement boost, jetpack toggle/flight, heat toggle, scuba auto-use
underwater, counters/timers and inventory cycling. Desktop `'` invokes the
selected item; browser defaults use `'` to invoke and `,`/`.` to cycle. Input
actions are rising-edge-triggered to avoid repeated consumption while held.
Holoduke fall/collision, boots/floor hazard handling, palette/HUD display and
exact quote/sound/timer parity remain open. Static Chocolate source inspection
and compile checks only; no runtime/demo comparison.

**Underwater air follow-up (2026-11-07):** Room now initializes and restores
Chocolate's 390-tic `airleft`, refills it outside underwater sectors, and
consumes it when scuba is inactive. After depletion it accumulates
`extra_extra8` in Q8 steps of 32 and applies one health point per eight tics,
with the Atomic low-health pain cue condition. The two state fields round-trip
through Atomic `.SAV` player records. Static `player.c`/`duke3d.h` review,
formatting, diff check and targeted compile only; no gameplay or reference
comparison.

**Holoduke decoy update (2026-11-07):** using selected inventory item 3 now
spawns a native APLAYER decoy in the player sprite list, initializes its pose
and owner linkage, and advances Chocolate's size/facing animation. Reuse turns
it off and expiration/deactivation removes it on the following mover pass.
Fall/collision, palette and multiplayer targeting semantics are still
incomplete. Static Chocolate source inspection and compile checks only.

**Player floor-hazard update (2026-11-07):** `processinput` now handles Atomic
purple lava on the floor/ceiling plus hurtrail, slime and plasma floors, using
Chocolate's `rnd` thresholds, TRAND damage range and boot-consumption rates.
Damage updates player health; boot absorption queues quote 75, and hazard pain
and short-circuit sounds resolve through the loaded CON sound table. Toxic
floor damage applies Chocolate's armor percentage and conditional TRAND roll.
Timed RGB
flash state is composited by the desktop shell. Classic palette math,
`spritebridge` exceptions, and audio overlap gating remain partial. Static
Chocolate `player.c` inspection and compile checks only.

**Native Duke sound toggles (2026-11-07):** the desktop sound bridge now
honors archived `duke_sound` and `duke_lockout` console variables. Lockout
filters starts for CON definitions with `m&8`; explicit stop events continue
to clear active voices. This does not add an in-game options menu or
device-specific voice availability. Static Chocolate `sounds.c::xyzsound`
inspection and compile checks only.

**Native Duke pitch ranges (2026-11-07):** playback now maps CON
`definesound.ps/pe` offsets to the mixer's pitch ratio using the 1200-unit
octave convention from Chocolate's `PITCH_GetScale`. A stable hash of tic,
source and sound picks a value in the half-open range without advancing the
playsim RNG. This intentionally does not reproduce Chocolate's libc `rand()`
sequence. Underwater pitch follows Chocolate's −768 override for non-talk
positional sounds and −1024 offset for pipebomb, laser-trip and RPG explosions;
global sounds bypass this path.

**Native Duke distance offsets (2026-11-07):** the mixer now applies signed
`definesound.vo` distance offsets before attenuation, clamped at zero. The
desktop mixer now reproduces Chocolate's integer `FindDistance3D`, 105-unit
minimum `sndist>>6`, distance-scaled pan level and integer 32-bin
`MV_Play3D` pan table. Positional `m&16` sounds retain their source
pan at zero distance, while global `m&128` sounds use `sound()`'s centered full
volume path. Actor sound starts also use Chocolate's eye-offset
Build `cansee` test and add `sndist>>5` when blocked; global sounds and marker
ambience bypass that check. The desktop audio adapter uses the input-phase
`opos` snapshot for listener position, matching Chocolate's sampling point;
audible calibration remains open. Static review of `sounds.c::xyzsound`,
`global.c::FindDistance3D` and `multivoc.c::MV_Play3D` / `MV_CalcPanTable`;
format/diff/app compile only, no gameplay or sound comparison.

**D8 listener `opos` phase (2026-11-07):** the desktop audio adapter now
spatializes sounds from the player position snapshot stored at input-phase
entry, while retaining the current view angle and sector. Chocolate
`xyzsound` reads `ps[screenpeek].opos*` after actor sounds are queued later in
the tic, so this removes the current-pose displacement from attenuation,
panning and LOS. Static `player.c`/`sounds.c` review, formatting, diff check
and targeted compile only; no audible/gameplay comparison.

**D8 `xyzsound` owner arbitration (2026-11-07):** the mixer now keeps
Chocolate's four-voice early rejection and, below that limit, stops only the
latest same-ID voice when the first owner repeats, when multiple voices are
already active, or when the incoming source is a dead badguy. Player-origin
queued sounds now carry the player sprite owner; non-global actor voices use
overlap channels so generic source replacement cannot bypass the explicit
Atomic owner rule. Static `sounds.c::xyzsound` review, formatting, diff check
and targeted compile only; no playback or gameplay comparison.

**D8 stable sound-owner ordering (2026-11-08):** mixer voice arbitration no
longer treats the current vector index as playback age. Each started voice gets
a monotonic mixer-local sequence; the oldest active voice supplies Chocolate's
`SoundOwner[id][0]`, and explicit same-ID replacement removes the newest one.
This preserves `xyzsound` owner semantics when unrelated voices end and are
removed with `swap_remove`. Static review of Room's voice lifecycle and
Chocolate's owner rules; formatting, diff check, and targeted compile only.
No audio, gameplay, or reference comparison.

**D8 exit-countdown sound gate (2026-11-07):** the desktop adapter now rejects
new `xyzsound` starts when `timebeforeexit` is 1–78 tics, matching Chocolate's
three-second cutoff; queued sound-stop and source-stop events still execute.
Static `sounds.c::xyzsound` review, formatting, diff check and targeted compile
only; no playback/gameplay comparison.

**Atomic level music (2026-11-07):** the CON compiler retains the 4×11 Atomic
`music` filename table and the environment table instead of discarding those
directives. The native desktop loads the selected level's MIDI through the
layered Duke GRP at initial map startup and after campaign transitions, using
the configured SoundFont or GRP `GENMIDI` backend when available. E1L10/E1L11
coordinates are included in the map-name parser. F5 displays the selected
track and Shift-F5 cycles through the 44-entry table while that 100-tic quote
is active, matching Chocolate's shortcut gate. F4 opens a native Sound Setup
menu with sound/music toggles, Duke Talk, and 0–255 sound/music volume sliders
adjusted in four-unit steps with the arrow keys; labels display normalized
percentages. Talk mute filters `definesound.m&4` voices, and changing music resumes
the selected campaign track. Ambience markers carry a distinct event flag so
the menu toggle can stop/mute marker-owned audio without suppressing direct
gameplay sounds; the stereo toggle swaps SFX pan gains. Escape opens a native
pause menu with resume/save/load/sound setup/quit, while F10 opens quit
confirmation. When the pause or quit menu opens, Room selects the first CON
`music 0` environment track from the layered GRP, then restores the selected
campaign track on resume. Re-enabling music in Sound Setup respects whether a
menu or gameplay session is active. Duke `MUSICANDSFX`
reverb markers select a radius-based mixer reverb. Room now ports Chocolate's
sample-rate-scaled multi-tap/IIR network and maps the `FX_SetReverb` 0–255
scale. MUSICANDSFX reverb markers now retain Chocolate T1 enter/leave state in native tics and apply transitions in STAT_FX list order; the current level is saved with `.fsave`. Exact `FX_SetReverbDelay` behavior remains open, alongside pointer-driven volume sliders, Atomic front-end screens beyond the basic pause menu, and audible parity. Static review of Chocolate
`gamedef.c::music`, `premap.c`, and `game.c` F-key handling; no gameplay/audio
comparison.

When all 32 mixer voices are occupied by marker-owned ambience loops,
Chocolate `movefx()` first checks `FX_VoiceAvailable`, then stops a farther
active ambience marker only when the entering marker is closer; if no farther
loop qualifies, the new one is skipped. Room now applies the same priority
gate and distance-based replacement in `Mixer::apply`. The mixer pool remains
fixed at 32 voices; Chocolate's selected-device pool size and audible behavior
have not been verified.

**Native quote and TIP HUD (2026-11-07):** the desktop shell renders the
currently active CON quote near the bottom center with a dark shadow, using
the existing quote timer API. It also rasterizes Atomic ART tiles 2576/2577
as palette-indexed RGBA, animates the basic TIP frames from the CON tip timer,
applies Chocolate's `tip_y` pose and horizon offset, and hides the weapon while
the TIP is displayed. The source `tip_y` table has 21 entries although the
timer starts at 26; Room clamps the index to the last valid entry instead of
reproducing the source's out-of-bounds read. Original palette/shade
translation, look-angle/sway offsets, font/layout, and the rest of the Duke
HUD remain open. Static source inspection and compile checks only; no runtime
or Chocolate comparison.

**Native Duke automap slice (2026-11-07):** the desktop Tab overlay now reads
the active `DukeSession` `BuildWorld` and draws its wall loops around the Duke
player pose, with separate portal/solid colors and the existing zoom control.
It performs Chocolate `scansector`'s facing-portal flood using its wrapped
32-bit cross-product gate and `cstat&32` portal block, remembers reached
sectors while the overlay is open, limits geometry to those sectors and adds
markers only for active, non-invisible sprites in the current portal flood;
remembered-but-not-currently-visible sectors do not leak sprite markers. This
avoids drawing the Doom map held by the shared shell. The renderer's later
view-span/frustum clipping is not ported,
so eligible portal sectors can be marked even when their projected walls
would be clipped. Native secret/door colors and Chocolate's exact
automap scaling/rotation behavior remain open. Static
compile only; no runtime or Chocolate comparison.

The shared target scan now applies Chocolate's weapon-specific exclusions:
Freezer ignores palette-1 immune actors/players, Shrinker ignores small
non-slime targets, and coop player shots do not auto-target another player.
Chocolate's target-specific LOS source height is also applied: `ORGANTIC` and
`ROTATEGUN` are checked from the unshifted source Z; other targets use source Z
minus 32 units. Every Build status-list/cstat combination and other `aim()`
edge cases remain open. Static `player.c::aim` review and crate compilation
only; no tests or dynamic comparison.

**Build radius distance follow-up (2026-11-07):** Chocolate's `sector.c::dist`
calls `FindDistance3D(dx, dy, dz >> 4)`, whose integer approximation is now
ported as `room_build_engine::numeric::find_distance_3d`. The Room Shrinker,
RPG and other implemented sprite-radius scans use this metric instead of
adding absolute vertical separation to `ldist`. It preserves integer shifts
and avoids floating point in gameplay. Static `global.c` and `actors.c` review
plus targeted compilation only; no runtime/reference comparison.

The CON `ifhitbyweapon` path also now returns false for zero-damage SHRINKSPARK
hits on sprites with `xrepeat < 24`, matching Chocolate's early return before
it clears the queued hit. This threshold is only in Chocolate's non-APLAYER
branch; Room's APLAYER hit consumer now follows the separate player branch and
consumes the zero-damage marker regardless of body repeat. The existing
`movefallers` hit probe also preserves the marker on the non-player early-out.
Static source review and targeted compile only; no gameplay/demo comparison.

**Pistol cycle follow-up (2026-11-07):** the Atomic pistol now consumes ammo
when firing begins, shoots on kickback frame 1 in that same tic, ejects a
`SHELL` casing on frame 2, exits the cycle at frame 5 unless the remaining ammo
is exactly a multiple of 12, and plays the clip eject/insert cues at frames 5
and 8 during a reload. The casing shares the existing `SHELL`/`SHOTGUNSHELL`
movement path and uses Chocolate's player pose, three-RNG spawn setup, and
immediate `ssp`. The display-only visibility flash is not represented. Static
`player.c::processinput` and `game.c` review plus targeted compilation only;
no runtime/demo comparison.

The shotgun also now advances from kickback 1 to frame 2 on the activation
tic, so its seven-pellet volley still occurs at frame 4 but one tic earlier
than Room's previous start-and-return sequence. Pistol fire audio queues after
the frame-1 `shoot()` path, following Chocolate's `shoot()` then
`spritesound(PISTOL_FIRE)` order. Static ordering review and targeted
compilation only; no runtime/demo comparison.

**Chaingun cycle follow-up (2026-11-07):** the first input tic now sets and
advances `kickback_pic` in Chocolate's ordering, so the first burst lands on
frame 3 rather than one tic late. Each 3-tic firing frame decrements ammo,
spawns `SHELL` before the hitscan with `xvel += 32` and `z += 3<<8`, then
queues `CHAINGUN_FIRE` before hitscan impact cues. The held-fire reset after
frame 12 retains the native cadence. Static `player.c::processinput` and
`game.c` inspection plus compile checks only; no gameplay/demo comparison.

**Freezer collision follow-up (2026-11-07):** Static review of Chocolate's
`sector.c::checkhitsprite` showed two details missing from the first pass:
`FREEZERHURTOWNER` controls whether a `FREEZEBLAST` can hit its owner, and the
projectile's randomized `extra` value is added to the target's hit damage. Room
now honors the version-specific startup slot and forwards that damage value
through the hit metadata. Invalid owner `-1` is no longer mistaken for sprite
slot zero during projectile collision filtering. The player/actor freeze state
machine and remaining wall/ceiling collision parity are still open. Compile-
checked; no demo or gameplay comparison was run.

The generic projectile-hit path also queues Chocolate's `PISTOL_BODYHIT` cue
at the struck APLAYER sprite when an accepted projectile collision reaches a
player. SPIT and FIRELASER retain their specialized player-impact path, and
palette-1 freeze-immune targets return before the cue, matching `moveweapons`
ordering and positional audio routing.

Direct projectile collisions now accumulate each source sprite's `extra` in
the target's pending hit metadata for both APLAYER and non-player actors.
Chocolate doubles RPG projectile damage against badguy actors before queuing
that hit; player damage and other projectile damage retain the source value.
Radius/explosion damage remains an independent pass and may accumulate with
the direct hit, as in `checkhitsprite` plus `hitradius`. A hit on a sleeping
badguy also promotes it from STAT_DUMMY to STAT_ACTOR and resets its native
`SLEEPTIME` (24 × 64 tics) before the blast scan.

The same wall-impact pass now routes direct Freezer impacts through the
Chocolate `checkhitwall` cases for forcefields, breakable glass, fan overlays,
stained glass, screens, vending machines, technical wall stages, milk shelves,
ATMs, and wall lights. Forcefields animate and emit `FORCERIPPLE`; glass clears
the blocking bits on paired walls, emits shards, and restores its overlay via
the temporary sector effector. Screen cracks are chosen after shard RNG; ATM
impacts drop money; wall lights update linked SE3 flicker state. The remaining
`checkhitwall` material cases and exact shard/RNG parity are still outstanding.

**D6 direct mirror impact (2026-11-08):** the ordinary `checkhitwall` path now
matches Chocolate's mirror source allowlist (`HEAVYHBOMB`, `RADIUSEXPLOSION`,
`RPG`, `HYDRENT`, `SEENINE`, `OOZFILTER`, `EXPLODINGBARREL`). An accepted hit
clears the mirror blocking bit, switches its overlay to `MIRRORBROKE`, emits 70
wall shards and queues `GLASS_HEAVYBREAK`. This is distinct from the existing
nearby-sector radius pass, which already breaks eligible mirrors. Static
`sector.c::checkhitwall` review, formatting, diff check and targeted crate
compilation only; no tests or dynamic gameplay/reference comparison. Other
unimplemented material cases and exact shard placement/RNG remain open.

**D6 wall-light impact RNG order (2026-11-08):** Chocolate chooses the
`WALLLIGHT`/`TECHLIGHT` break sound with `rnd(128)` before calling
`lotsofglass`. Room now consumes that draw before spawning the 30 shards, so
their ordered random values and subsequent SE3 flicker state no longer start
one draw early. Static `sector.c::checkhitwall` review only; no runtime or
Chocolate/Room comparison.

**D6 player forcefield contact (2026-11-08):** the player movement path now
preserves Chocolate `clipmove`'s wall hit code and applies `checkplayerhurt`
to that blocking wall. `W_FORCEFIELD` contact applies five damage, red palette
flash, knockback, pain audio and a small `FORCERIPPLE`; `BIGFORCE` contact
retains its 26-tic cooldown and ripple without applying health damage. The
special 8×8 ripple sizing for `atwith == -1` is now represented in the shared
wall-impact handler. The same collision return now handles cactus sprites with
Chocolate's `<8` hurt-delay gate, five damage, red flash, 16-tic cooldown and
pain cue. Static review of Chocolate `sector.c::checkplayerhurt` and
`checkhitwall`; compile-checked only, no tests or dynamic comparison.

The handler now carries Chocolate's explicit `atwith` tile separately from
the source sprite: player hitscan uses `SHOTSPARK1`, quick-kick uses `KNEE`, and
projectile movement uses the projectile tile. This makes the mirror allowlist
apply to the actual attack and preserves the special forcefield `CHAINGUN`
ripple size (`source repeat + 16`, rather than 32). Static call-site and
`sector.c::checkhitwall` review, formatting, diff check and targeted compilation
only; no tests or dynamic comparison.

**Direct sprite-impact object slice (2026-11-07):** RPG, Freezer/Shrinker,
SPIT and FIRELASER collision paths now run the matching `checkhitsprite` object
arms for ocean sprites, water
fountains, fan sprites, toilets/stalls, hydrants, grates/panels, pipes and the
hydroplant. `TREE1`, `TREE2`, `TIRE`, `CONE` and `BOX` also take their first
qualifying hit from radius explosions, RPG, FIRELASER, hydrant or pipebomb
sources and spawn BURNING once. A `FORCESPHERE` hit also hides the sphere,
updates the projectile owner's temporary counters and spawns EXPLOSION2.
This updates native tile/collision state and spawns basic
water/smoke/steam/burning/explosion effects and sound cues. These handled
objects bypass generic badguy hit metadata, as in Chocolate's switch arms.
Sprite-local `lotsofglass` shards with native per-shard RNG draws are also
implemented for selected bottle/tableware decorations, FETUS, BOTTLE7 and the
second hit on BROKEHYDROPLANT. BOTTLE10 now emits its randomized money spray
before the glass burst; VASE emits its 40-piece glass burst, while STATUE and
STATUEFLASH emit the 40-piece palette-randomized glass burst and heavy-break
cue before the common glass burst. A later hit on FETUSBROKE now traces the 48
Chocolate BLOODSPLAT1 shots, then emits the heavy-break, squish and glass cues
with the ten-piece follow-up burst. Full debris details remain open. SATELITE, FUELPOD, SOLARPANNEL
and ANTENNA now follow the native damage-header gate, 15 ordered scrap spawns,
EXPLOSION2 and deletion path. Randomized palette/cstat details, hit-family
filtering, and other furniture, breakable and human NPC families remain open.
MONK, LUKE, INDY and JURYGUY now play their configured hit sound and
spawn their configured hit sprite, subtract direct projectile strength, and on
death run the eight BLOODSPLAT shots, six `guts()` groups and global SQUISHED
cue before deletion; SPACEMARINE shares the damage/death branch without the
civilian hit sound or child spawn.
Qualifying cactus impacts emit the 64-piece palette-8 scrap burst,
change CACTUS to CACTUSBROKE and clear its blocking bits. HANGLIGHT and
GENERICPOLE2 emit their six-piece scrap burst, heavy-glass cue and deletion.
CHAIR1/CHAIR2 become nonblocking BROKENCHAIR sprites; CHAIR3, MOVIECAMERA,
SCALE, VACUUM, CAMERALIGHT, IVUNIT, POT1–POT3 and TRIPODCAMERA emit 16
Chocolate `RANDOMSCRAP` pieces, the heavy-break cue and are removed.
Direct hits on QUEBALL/STRIPEBALL apply Chocolate's three-in-four rolling
impulse or one-in-four three-shard glass break. A separate STAT_DEFAULT mover
now handles ball-to-ball movement, pockets, wall bounce, speed loss and
player-directed kicks as detailed in the D5 note below.
Formatting, diff and native crate compile checks only; no dynamic
Chocolate/Room comparison.

Pistol, shotgun and chaingun wall hits now invoke this shared material path
after spawning their `SHOTSPARK1` and small-smoke effects; static inspection of
Chocolate `player.c::shoot` confirms it has no 1024-unit cutoff. The native
path now applies the real Build `FindDistance2D` metric for bullet-hole
spacing, creates and wall-aligns the `BULLETHOLE` sprite, and suppresses decals
on tagged walls, doors, forcefields, nonzero-lotag sectors and SE13 borders.
Switches route the exact struck wall through the native switch transition,
including access-card checks, access-animation setup, tagged activators, and
switch response audio. The switch families gated by Chocolate's
`check_activator_motion` are ignored while a linked `ACTIVATOR` sector owns a
live plane/wall animation or one of its blocking SE states (11/30 T5,
18/20/31/32 T1), for both `Open` and hitscan paths. Dip/tech/alien and
access-card switches retain their dedicated paths and are not subject to this
gate.
DIPSWITCH, TECHSWITCH and ALIENSWITCH change only the struck frame instead of
cycling every switch with their lotag. Hitscan impacts on tagged wall and
sprite switches use the struck target.
Tagged Build door textures accepted by Chocolate `isadoorwall` also operate
their linked sectors without changing door art or playing the ordinary switch
cue. A recognized switch or door operator with lotag `-1` now marks the level
exit ready, matching Chocolate's signed `65535` end-of-level tag. DIPSWITCH correct-combination
counting and end-of-level warning, fire-switch response, some bullet-hole
wall edge cases, unported materials and exact per-pellet RNG ordering remain
open. The new bullet-hole path is source-guided but not yet demo-validated.

**APLAYER hit processing (2026-11-07):** the native `moveplayers` pass now
consumes queued `hittype` damage after weapon/projectile movement, updates player
and body health, applies Chocolate's directional knockback multipliers for
explosive sources, suppresses ordinary damage in god mode, permits Shrinker
damage in god mode, and rejects coop player-on-player damage when friendly fire
is disabled (the default). The Atomic tile IDs and the strong-knockback list
were checked against Chocolate's headers. A later D6 follow-up stores
`wackedbyactor` for lethal non-Freezer hits, syncs the dead view to the player
body, and turns it halfway toward a still-active attacker each tic. Player
damage palette/quote feedback, complete frag owner/counter behavior and the
detailed FREEZEBLAST death/freeze transition remain open. Static inspection
and crate compilation only; no gameplay or demo comparison was run.

**Player shield absorption (2026-11-07):** `Player::shield_amount` now stores
Chocolate's 0–100 armor pool; Atomic campaign player spawn initializes it from
`gamestartup[3]` (`max_armour_amount`), with the native 100 default. The player
damage pass applies `incur_damage` semantics: on health loss with armor, one
Build `TRAND` chooses 20–49% damage absorption, capped by remaining armor;
health at zero clears the armor. The random draw occurs only when a positive
armor pool absorbs a health loss. `extra_extra8`, damage palette feedback and
the exact full `processinput` ordering remain open. Static Chocolate source
inspection and compilation only; no gameplay/demo comparison was run.

**CON inventory bridge (2026-11-07):** Chocolate `ifpinventory` and
`addinventory` now exchange Atomic inventory state with the player for
steroids (0), shield (1), scuba (2), holoduke (3), jetpack (4), access cards
(6), heat (7), first aid (9), and boots (10). The shield cap and the legacy
`ifpinventory 1` max-health comparison match `gamedef.c`; inventory icon IDs
also follow its assignments. Counter/icon writes are deferred until actor CON
execution completes, while subsequent inventory predicates in the same script
see the pending value. This enables the corresponding Atomic CON pickup grants.
The Holoduke timer now decrements on every tic while its hologram is active,
and expires at zero or below with its teleport cue and next-available icon
selection. Scuba expiration now turns off scuba before the underwater air
check, so an exhausted tank consumes air on that same tic, as `doincrements`
does. Jetpack expiration also sends global stops for `DUKE_JETPACK_IDLE` and
`DUKE_JETPACK_ON` before leaving flight, matching Chocolate's `stopsound`
calls. Inventory and floor-hazard sound events are now attributed to the
actual APLAYER sprite for each seat, instead of being positioned at seat zero;
the shared actor-sound collector applies normal Atomic sound-definition and
voice policy. Steroid heartbeat playback follows Chocolate's screen-player / co-op
gate: seat zero hears it in solo or match play, while every coop seat may emit
it. Core inventory activation/timers beyond the implemented first pass,
item respawn/visibility details, and pickup
presentation remain open. Static source inspection and compilation only; no
gameplay/demo comparison was run.

Weapon slot 8 now has a partial tripbomb lifecycle: the wall hitscan and
placement filters gate the kickback sequence; successful placement creates the
wall-facing charge and cue, including the pre-angle SSP offset. Its standable
pass counts down to arming, raycasts
with `hitscan_full` to find the laser target and range, spawns linked LASERLINE
segments with the native perpendicular angle and `lasermode` blocking flags,
uses Chocolate `findplayer` distance and nearest living seat for its initial
arm delay, reacts to target-distance changes and queued hits with the native
fuse, then spawns five scraps, configured radius damage and an explosion before
clearing its beam segments. The spawned `EXPLOSION2` receives Chocolate's
immediate clipped `ssp(CLIPMASK0)` step at detonation. The laser trace also
recalculates the temporary muzzle-offset sector as Chocolate's `setsprite`
does and measures the hit range with Build's `FindDistance2D`. Room's ray
still replaces Build `hitasprite`; exact
collision/SSP movement, beam damage and random/order parity remain open. A
queued hit now only arms the 16-tic fuse: Atomic temporarily changes tripbomb
health while running `IFHIT`, restores its original health, and later uses that
health as blast strength. This corrects Room's prior health subtraction; the
broader tripbomb acceptance items remain partial. The
placement duplicate-radius scan resolves the active CON `TRIPBOMB` define, so
add-on scripts that relocate this actor tile do not stack charges using the
Atomic default tile ID. Static review of `player.c::shoot`; no gameplay
comparison. The standable actor mover now resolves the same active CON
`TRIPBOMB` label, with Atomic tile 2566 as fallback, so a relocated definition
continues through its arming, laser, fuse and blast states after placement.

**D6 standable dispatch ordering (2026-11-08):** Room now snapshots the linked
`STAT_STANDABLE` order and dispatches its implemented BOLT/water-bubble maker,
SEENINE/OOZFILTER, and tripbomb branches per sprite in that order. This replaces
the prior grouped passes, which reordered cross-actor changes and random draws
relative to Chocolate's single `movestandables` pass. Remaining standable
actor types and exact iteration behavior after list mutation are still open.
The phase now hands the session's sprite-list state to `BuildWorld` before this
dispatch and copies the resulting links back after the phase, so spawns and
deletions from either handler family persist instead of being overwritten by
the other copy. Static state-flow review, formatting, diff check and targeted
crate compilation only; no tests or dynamic gameplay/reference comparison.
SEENINE/OOZFILTER state changes and tagged CRACK1/FIREEXT hit handling also
resolve those actor tiles through active CON labels, with Atomic tile IDs as
fallbacks. Radius-triggered hitag chains use the same resolved identities.
Static `actors.c::movestandables` review, formatting, diff check and targeted
crate compilation only; no tests or dynamic gameplay/reference comparison.

**Freezer firing slice (2026-11-07):** weapon slot 9 now follows Chocolate's
kickback tic 3 for launching `FREEZEBLAST`, spends one cell per shot, repeats
while fire remains held, and queues `CAT_FIRE`. The projectile has clipped
flight, queues target hit metadata, fizzes with a transporter star at the native
low-lifetime thresholds, skips freeze-immune paletted badguys/players, and
reflects from walls. Ceiling/floor impacts use Chocolate's `bounce()` slope
normal, reflecting horizontal and vertical velocity together before halving
damage, decrementing bounce count and shrinking the bolt by two repeat units.
The shared 48-angle `aim()` now selects the target and sets bolt angle/Z
velocity, gated by sync bit 6 and `player.auto_aim`. Exact Build movement remains open. The archived setting is selectable in the desktop F4 Sound Setup menu; browser settings remain open. `moveactors` now handles
Chocolate's GREENSLIME-only `t[0] == -5` lifecycle: Freezer transition and
damage, frozen gravity/frame/palette, 280-tic thaw, facing quick-kick response,
and a non-Freezer hit's 17-piece `GLASSPIECES` break with ordered RNG draws.
Non-frozen slime AI and other actor transformations remain open, so this is
not full Freeze weapon parity. Static source review and crate compilation only;
no runtime or reference comparison was run.

The same D6 slice now includes Shrinker slot 6's repeating SHRINKSPARK launch
and Grower slot 11's immediate hitscan, spread RNG, impact sprite animation,
ammo use and firing cue. SHRINKSPARK has clipped projectile movement and queues
the native weapon id in target hit metadata. Its impact now uses the configured
`SHRINKERBLASTRADIUS` to mark eligible visible targets with zero direct damage,
and skips the radius-wall pass as Chocolate does. A static follow-up mirrors
Chocolate's seven-status scan order, its blocking-sprite path, badguy LOS test,
shootable-target eligibility, owner/small-target exclusions (including the
SHARK exception), player feet-distance origin and separate target/source LOS
Z offsets. Script-defined `TRIPBOMB` is resolved from the active CON labels.
This still approximates Build's `dist()` with Room's horizontal distance plus
scaled vertical separation, and queues weapon hit metadata for the target. The
weapon/sim path does not hard-code general actor transformation: the target
CON must handle `ifwasweapon SHRINKSPARK` / `GROWSPARK`, then select its action
and size changes. Room already exposes the corresponding hit-weapon and
`sizeat`/`sizeto` VM operations, but Atomic actor-by-actor response and the
resulting sprite/collision evolution have not been audited statically. The full
`aim()` target choice uses the same sync bit and `player.auto_aim` state, with
the Grower's native fallback spread RNG. Direct wall/ceiling effects, remaining
Build cstat/status combinations, and actor/player transformation parity remain
open. These weapons are partial native paths; D6 remains
incomplete. Static Chocolate `actors.c::hitradius` review and crate compilation
only; no runtime or reference comparison was run.

**Weapon activation audio/cadence follow-up (2026-11-07):** Freezer,
Shrinker and Grower now queue their Chocolate weapon sound on activation
(`CAT_FIRE`, `SHRINKER_FIRE`, `EXPANDERSHOOT`) and do not infer the cue from
later ammo consumption. The Freezer advances its first kickback tic in the
activation call and evaluates its held-fire recovery branch before advancing
another cycle, matching Chocolate's state ordering. Static `player.c` review,
formatting, diff check and targeted crate compile only; no dynamic comparison.

**Shrinker/Grower cadence correction (2026-11-07):** the activation path sets
kickback frame 1 in Chocolate's input phase, and its later nonzero-kickback
branch is skipped until the next tic. Room therefore waits until its
post-increment frames 12 (Shrinker) and 5 (Grower) before firing, matching
Chocolate's pre-increment `>10` and `>3` checks. The Freezer low-xrepeat
cancellation is scoped to the active `<4` branch, as in `player.c`. Static
Chocolate review and targeted compile only; no gameplay comparison.
only; no runtime or reference comparison was run.

**Pipebomb/remote slice (2026-11-06):** Room now follows Chocolate's
`player.c::shoot()` hold timing and throw-strength formula for weapon slot 5,
including crouch toss, ammo consumption and automatic transition to slot 10.
The post-spawn `hits()` check reverses and slows the bomb when a wall is within
512 units in front of the player.
The remote weapon clears `hbomb_on` during its fire animation, then returns to
the handbomb when ammunition remains. The native HEAVYHBOMB actor currently
falls, bounces from floors, clips XY movement, triggers from the owning
player's remote state, detonates when its `hit_extra` is set, picks up stopped
bombs after the vanilla delay, and emits the underwater splash. Radius damage
uses the `gamestartup` pipebomb radius when present (fallback otherwise) and
filters badguy/shootable actors. The shared radius-sector pass now checks
nearby ceilings and invokes `check_hit_ceiling` before wall hits, matching
Atomic `hitradius`. Static source review only; no demo/game was run. This is
not a parity claim: remaining `checkhitwall` material types, exact
damage/RNG ordering, non-pipebomb item respawn behavior, and complete explosion FX. All these
radius paths now share a partial wall pass: adjacent sectors are scanned inside
radius with midpoint visibility; mirror/glass textures break, paired glass
blocking flags clear, and fan overlays break in Chocolate's force-wall
condition; stained glass also clears paired blocking under that condition.
Atomic break sounds queue from the blast source. Vending machines, screens,
tech walls/shelves, ATMs and wall lights now receive their common broken-tile
transitions; mirror breaking respects Chocolate's source-tile allowlist.
Wall-positioned `GLASSPIECES` for common breakable surfaces now follow
Chocolate's wall interpolation, velocity RNG and stained-glass palette. Eligible
forcefields create their `FORCERIPPLE` impact and set the animated wall flag;
tagged wall lights arm linked SE3 flicker markers after debris RNG. ATM money
now spawns with Chocolate's count, position and cstat random draws; other
overlay glass also spawns SE128 to animate and reblock both wall overlays after
the native six-tic sequence. Wall visibility resolves the quarter-midpoint's
sector with `updatesector` before LOS. Remaining material types and light
activation effects beyond the linked SE3 path remain open.

**Map-placed HEAVYHBOMB respawn (2026-11-08):** map-owned bombs now honor
`ud.respawn_items` and CON `RESPAWNITEMTIME` after pickup or detonation. Room
hides the original, ticks a red/yellow/green marker, then restores the item
with the teleport cue and transporter-star effect. Marker colors now use
Chocolate's shifted half/quarter thresholds (including retaining the prior
color in the gap), and all three marker states run the shared `makeitfall`
step each tic. Player-thrown bombs still disappear. Static Chocolate
`actors.c` / `gamedef.c` review, formatting, targeted crate compilation and
diff check only; no tests or dynamic Chocolate/Room/game comparison were run.

**CON `hitradius` actor/player slice (2026-11-06):** the CON opcode now records
its radius and damage bands at execution and applies three distance bands to
the Chocolate stat-list order: default blocking sprites, actors, standables,
players, fallers, dummies and misc. It queues actor hit metadata/knockback,
uses the APLAYER feet origin and player-specific LOS heights, and keeps the RPG
small-sprite wall-pass early-out independent from actor scanning.
The shared hitradius wall pass
also applies to CON explosions. This is a partial implementation: more ceiling
effects, wall debris/forcefield effects, other wall-material cases, exact Build `dist()` edge parity, and immediate
ordering relative to subsequent CON commands remain open.
Static source review only; no runtime or demo comparison was run.

**D6 radius target follow-up (2026-11-08):** stat-list traversal in the shared
CON and RPG radius paths now follows Chocolate's order through
`STAT_DEFAULT`, `STAT_ACTOR`, `STAT_STANDABLE`, `STAT_PLAYER`, `STAT_FALLER`,
`STAT_DUMMY`, and `STAT_MISC`. The default/dummy/misc blocking-sprite branch
retains its badguy-only randomized-height LOS check; actor/faller/player
candidates use the separate target/source height LOS check. Actor candidates
include the scripted tripbomb, pool-ball, and lying-dead tiles. The
`AFLAMABLE` BOX/TREE1/TREE2/TIRE/CONE branch also uses active CON tile labels
and the randomized-height LOS rule in whichever scanned list contains it. RPG
and CON radius scans also invoke the existing sprite-impact object handler for
eligible targets, so blast hits can perform the same prop transitions, debris,
sounds, and deletion effects as direct projectile hits. The blast entry points
receive live ART tile-height metadata for sprite-local effects. APLAYER distance
uses the feet origin and its separate LOS heights, with damage queued
for the normal player-hit pass. RPG radius hits use the same queued
metadata/knockback path across the trailing status lists. This records static
source alignment for traversal, not exact
`dist()` edge behavior, every eligibility exception, or immediate CON command
ordering. No runtime/demo comparison was run.

**D6 radius camera interruption (2026-11-08):** after `hitradius` queues damage
for an APLAYER, Room now checks Chocolate's owner-live guard. If the source has
an active owner, the victim's remote camera view is cleared and active
`CAMERA1` actors have their monitor `yvel` latch released. This follows
`actors.c::hitradius`'s player-camera cleanup; dead/unowned sources do not
trigger it. Static source review, formatting, diff check and targeted crate
compilation only; no tests or dynamic gameplay/reference comparison.

The same nearby-sector radius pass now evaluates each visited ceiling before
the sector's walls, following Chocolate's signed vertical-radius check and its
first-wall-vertex / third-vertex Manhattan fallback. A nearby ceiling tile is
sent through `check_hit_ceiling`, including shards, linked SE3/SE12 updates and
the player-positioned glass cue. This closes the previously dormant ceiling
handler call path for RPG and CON radius hits. Static `actors.c::hitradius` /
`sector.c::checkhitceiling` review and targeted crate compilation only; no tests
or dynamic gameplay/reference comparison.

**D6 radius actor transitions (2026-11-08):** RPG and CON `hitradius` now keep
the actor/standable blast damage and knockback that Atomic records before its
post-damage `checkhitsprite` call. That follow-up is restricted to Atomic's
civilian, statue, spacemarine, and pool-ball tile set, so unrelated actors do
not receive prop transitions. This corrects ordering and eligibility while
leaving the deferred normal hit consumer and exact `dist()` edge behavior open.
Static `actors.c::hitradius` review, formatting, diff check and targeted crate
compilation only; no tests or dynamic gameplay/reference comparison.

**Animated-wall and native trace checkpoint (2026-11-06):** native `DukeSession`
builds the Atomic `premap` wall-animation list and runs `animatewalls` every
other 30 Hz tic. Screen-break and force-field updates preserve wall state and
the original `krand` call order. E3L11's four force-field walls reproduce the
five-call transition from Chocolate. Fixing warped WATERBUBBLE `iffloordistl`
to use destination-sector planes advances native-only DEMO1 parity through
tic 40. The next mismatch came from the APLAYER swim action: Room skipped the
inner `PSWIMMING ifrnd 4` on its first in-action tic too, and failed to advance
player `temp_data[0]` outside water. Room now skips only the action-entry tic
and advances the counter like Chocolate's `move()`. Native-only DEMO1 state and
RNG match through tic 69 after correcting the WATERBUBBLE spawn and delaying
the WAIT branch's final swim roll. A static follow-up now suppresses the
trailing APLAYER bubble `ifrnd` on the tic when the swim action changes, so the
old action branch cannot consume its final RNG roll. The verified frontier is
still tic 69 until a later dynamic comparison. The native CON VM also evaluates
`ifgapzl` from cached hittype floor/ceiling planes with Chocolate's arithmetic
`>> 8`, and `ifspawnedby` shares mutable `hittype.picnum` with `ifwasweapon`.
CON `sizeto` now advances mutable sprite repeats one step per tic and applies
the vanilla vertical-clearance condition using loaded ART height metadata.
CON `debris` now inserts randomized EGS fragments in Chocolate's reverse count
order, including the BLIMP/SCRAP1 pickup remapping.
CON `guts` emits player-owned gore fragments with sloped-floor clamping,
COMMANDER offset, small-enemy scaling and JIBS2 quarter-size handling.
CON `ifawayfromwall` now checks the four Chocolate diagonal probes at ±108
Build units against the actor's current sector, using the shared Build
point-in-sector primitive. `ifrespawn` now selects the correct monster,
inventory, or ordinary-item setting. The seven inventory tile IDs follow
Chocolate's `inventory()` classifier, and imported Atomic save globals carry
the three `ud.respawn_*` values into actor CON execution. When no imported
setting exists, these options default to disabled. `ifbulletnear` now walks the
live `STAT_PROJECTILE` list and uses Chocolate `dodge()`'s sector, owner,
forward-dot, approach-dot, and cross-product checks. A threat consumes the
same `TRAND&1024` angle turn and updates the actor angle during the CON run.
`ifnotmoving` reads the previous actor `movesprite()` result and applies
Chocolate's `(movflag & 49152) > 16384` test. The previous value is retained on
actor-move paths that return before calling `movesprite()`. `ifcanshoottarget`
now follows Chocolate's `g_x > 1024` gate, actor-size-dependent lateral ray
offsets, CLIPMASK1 horizontal `hitasprite` checks, same-type sprite suppression,
and badguy forcefield exception. Its index-aligned hit-sprite snapshot is
refreshed per actor, after earlier actors have run, and uses ART-sized face
sprites where applicable. `ifnosounds` now checks the first active owner for
each Atomic sound ID, matching Chocolate's `SoundOwner[id][0]` rule. The
desktop audio mixer supplies an ordered per-ID owner snapshot at the tic
boundary, and CON `sound`, `soundonce`, `stopsound`, and `globalsound` update
that owner table during actor execution so later CON reads see same-tic
changes. `resetactioncount` also clears only `g_t[2]`, following its
Chocolate opcode behavior. The sound-owner table is presentation state and
does not enter deterministic saves. `respawnhitag` queues Chocolate's
`operaterespawns` action and activates the matching `RESPAWN` markers in
`STAT_FX`, including the `monsters_off` / badguy exclusion. Marker countdown
and transporter-star creation remain in the existing respawn tic handler.
`mikesnd` now reads the actor's yvel sound and starts it only when that sound ID
has no active owner, matching Chocolate's `Sound[id].num == 0` guard.
`wackplayer` (opcode 61) consumes the same `TRAND` byte as
`forceplayerangle`, updates the nearest player's horizon/recenter/look/roll
state, and terminates that actor parse. Player roll damping and look-angle
recovery run in `processinput_move`; the software view rotates its indexed
frame using Build fixed-point trig before palette expansion.
`tossweapon` (opcode 104) now follows `checkweapons`: it consumes the weapon
selection roll in CON order, drops the matching Atomic weapon sprite on the
successful branch, and uses the RPG/handbomb `EXPLOSION2` fallback on the other
branch. The item drop inherits the player source, applies the vanilla Z offset,
fall velocity, randomized cstat, palette/shade/size and actor status.
D5 remains incomplete.

**Tic-135 follow-up (2026-11-06):** neutral input at tic 135 leaves Room's
randomseed and sprite count unchanged, so the first mismatch is not caused by
that tic's player input. A trial inversion of the global `everyothertime`
phase regressed at tic 31 and was reverted. `JIBS6` movement now refreshes the
sector with Build `updatesector` in Chocolate's z-velocity band and evaluates
sloped floor/ceiling planes with `getzsofslope`, matching the reference mover's
`setsprite`/`getflorzofslope` path. Native-only DEMO1 still matches through
tic 134 and the last dynamically verified mismatch remains tic 135 (randomseed;
503 vs 504 sprites). Static comparison of Chocolate `player.c::shoot()` found
an omitted RNG call after a valid hitscan: the 1-in-64 `TRAND` roll for
`PISTOL_RICOCHET` (Atomic sound id 1). Omitting it shifts later shotgun pellet
spread, changing hit count and the resulting `JIBS6` spawn. Room consumes the
roll after a normal resolved hit and queues the ricochet cue when that sound
is defined; temporary per-shot RNG padding was removed. Atomic DIPSWITCH,
DIPSWITCH2, DIPSWITCH3 and HANDSWITCH sprite/wall hits now return before that
roll, matching Chocolate's early exits, and only those switch families enter
this player-shot switch path. This correction has only had static source
review and compile checks; the tic-135 parity result is not yet remeasured. Next,
compare the following pellet angles, hit count and end-of-tic seed against
Chocolate before advancing the frontier. The `JIBS6` geometry change above is
independent and does not claim to solve the trace discrepancy by itself.

**Tic-55 CON diagnostic (2026-11-06):** set `ROOM_DUKE_CON_RND_TRACE=1` when
running `duke_frontier real DEMO1 56` to log each WATERBUBBLE `rnd` branch,
including sprite, script cursor, input seed, result, `g_t[0]` and action. Room's
post-tic-54 snapshot and Chocolate's reference dump both show bubble #399 at
`g_t[0]=4`; Chocolate's post-tic-54 `bub_chain` also contains #399. A direct
Chocolate `execute()` entry/exit probe confirms #399 runs through CON at tic
55 and changes `g_t[0]` to zero. After removing Room's synthetic WAIT catch-up
rolls and the erroneous `TRAND` added to `spawn(player, WATERBUBBLE)`, native
DEMO1 matches through tic 69. Chocolate's `spawn()` → `EGS()` path consumes no
RNG; Room's extra spawn-Z draw displaced the later actor and wall-animation
calls. The new first mismatch is tic 70: Room evaluates the APLAYER trailing
bubble `ifrnd`, while Chocolate's script changes action and skips that branch.
The next step is to reproduce that script branch/action transition in the
native player mover. No RNG padding has been added.

**CON actor-state predicates (2026-11-06):** static comparison with Chocolate
`gamedef.c::parse()` now backs `strength`/`addstrength` with live sprite health,
implements the non-player `spritepal` / `getlastpal` pair, `cstat`, `cstator`,
`cactor`, `clipdist`, `count`, `sleeptime`, and `addkills`, and evaluates `ifstrength`,
`ifphealthl`, `ifspritepal`, `ifangdiffl`, `ifactornotstayput`, `ifhitspace`,
`ifoutside`, `ifinspace`, `ifinouterspace`, `ifmultiplayer`, and the `ifp
pkicking` flag for the knee weapon. `ifsquished` follows the vanilla
sector-gap/lotag test and queues
the pal-1 damage marker. These use current actor, sync, player-kickback and
sector state. CON `pkick` starts the player's 14-frame quick-kick countdown;
`ifp pkicking` recognizes both that countdown and the knee weapon kickback.
CON `endofgame N` records the global exit request and player countdown, and
`ifp pcanseeeverything` respects the active timer. The eventual MODE_EOL
transition, countdown sound cues, and campaign routing remain unimplemented.
`addphealth` applies the `gamestartup` health limit, ATOMICHEALTH double cap,
god-mode guard, player sprite synchronization and the
`DUKE_GOTHEALTHATLOW` cue when healing crosses the 25% threshold; camera
detach remains unimplemented. Actor `sound` uses `sprite.yvel` and
queues defined Atomic sound IDs with actor origins. CON `soundonce` starts an
actor sound only when that logical sound ID is not already active, and
`stopsound` stops that sound ID across all origins. Native Duke player cues are
positioned at the player and the desktop adapter updates the mixer listener,
using existing attenuation and stereo panning. The raw Build-to-audio scale has
only been checked against Chocolate's `xyzsound` coordinate treatment; audible
range and balance still need runtime tuning. CON `money N`, `mail N`, and
`paper N` emit the corresponding Chocolate pickups
with three per-object `TRAND` calls. `lotsofglass N` / `spriteglass` emits the
three `GLASSPIECES` variants with randomized position and velocity; random
ordering and shard motion still need comparison. CON `globalsound` plays at
the local player's position in the single-player session. `MUSICANDSFX`
markers now queue `m&2` distance-gated loops and `m&16` randomized sector
sounds, stopping loops by actor source when the player leaves range. The mixer
adapter carries loop/source-stop events. Atomic `definesound` loop bits now
make loop definitions loop in the mixer, and active voices are capped at one
per loop sound or three per ordinary sound; `m&4` talk sounds use an exclusive
voice class. Missing pieces include multiplayer `ud.coop==1` dispatch, sound
lock counters and priority voice stealing, global sound/voice toggles and
lockout, reverb, pitch/volume variation, and music.
`quote N` follows Chocolate FTA suppression/timer rules and exposes the active
`definequote` text through the session API; desktop text rendering is a basic
D8 HUD path. CON `tip` exposes its 26-tic animation countdown and the desktop
draws ART `TIP`/`TIP+1`, suppressing the weapon while the TIP overlay is active.
`ifgotweaponce` implements Chocolate's co-op-only 16-entry weapon reservation
list and actor-owner checks; wiring co-op selection into the multiplayer
session remains outstanding.
The native tic now advances Chocolate's `tipincs`, `fta` and living-player
`timebeforeexit` counters. FTA expiry clears the quote ID, and the level-exit
countdown exposes a one-shot readiness request to the host; campaign routing
and its sound cues remain outstanding.
Player-clipping suppression and the badguy recoil side effect in
`ifsquished` are not represented yet. Player palette restoration in
`getlastpal` player palette restoration and other multiplayer/inventory/world
predicates still need native state. Compilation was checked; no scripted behavior or demo
parity run was performed.

**Interaction checkpoint (2026-11-06):** `gamefunc_Open` (`E`) performs
Build `neartag_wall`, advances Atomic switch textures, and dispatches linked
activators. Sector plane motion is implemented for lotags 16–22, with basic
player/sprite floor carry; lotag 9 computes sliding-door vertex goals and
interpolates them on the 120 Hz Build clock. Seven focused interaction tests
cover switches, activation, plane movers, and a six-wall sliding-sector
fixture. This is an initial subset only: exact Build `nextsectorneighborz`,
overlap-sector handling, `dragpoint` shared-vertex propagation, door blocking,
and remaining sector/effector families are not complete. Wall-switch activation
arms matching one-shot `MASTERSWITCH` markers (`yvel: 0 → 1`); the native tic
now decrements their `hitag`, runs the supported delayed sector mover on expiry,
updates local SE/SEENINE/OOZFILTER markers and removes the switch. Touchplate
pressure now triggers linked activators/master switches and rides the sector
floor, with same-lotag plate propagation. `operateactivators` also toggles
SE18/31/32/36 T1 in activator sectors whose lotag is below 3, preserving local
effector order and emitting each sector sound cue before the sector's own
operator runs. For low-byte lotag 22, it also matches Chocolate's one shared
preliminary `callsound` gate across activators while preserving the additional
cue emitted by every `operatesectors` call. Lock semantics, respawns, and
unsupported `operatesectors` families remain open. Tagged `RESPAWN` markers are
armed by `operaterespawns`; the matching `TRANSPORTERSTAR` is created at this
arming point, then markers count through `movefx`'s extra 54→66 window before
reinserting the hitag tile through Room's EGS/CON path. A serialized temp flag
prevents creating a second star, while a serde-default fallback handles Room
saves made before this timing change. Actor targets receive the
active stat, map-sized fallback, clip distance, palette, and badguy fall setup.
Room's common `operate_activators` tail arms matching `RESPAWN` markers through
`operaterespawns`, regardless of whether activation came from a switch,
touchplate, SE10, or train-station departure/arrival; Chocolate uses the same
shared tail.
The `TRANSPORTERSTAR` uses the reference 48×64 size, -127 shade, cstat and
vertical offset and runs its CON actor when available. `DukeSession::monsters_off`
now culls badguy map actors at load and leaves enemy-target `RESPAWN` markers
unarmed, matching Chocolate's `spawn()` / `operaterespawns` gates. The setting
defaults off and is included in `.fsave`; there is not yet a menu/cheat path
for changing it during a running level, and special non-CON respawn targets
remain incomplete.
Sector lotag 27 now toggles the local SE 20 extend-o-bridge marker; its native
effector advances both cached endpoint vertices with Build `dragpoint`, pans
floor and ceiling X/Y opposite the bridge vector by one eighth, and carries
grounded sprites/players until the marker's SP distance is reached. Generic
sprite carry excludes player bodies, which are moved once in the player pass;
both paths refresh Build sector membership after movement like Chocolate's
`setsprite`. This is a static first implementation; blocked-sector
behavior and unusual map parameter cases remain unverified.
SE 31 true-drop-floor initialization and movement are now native: its map Z is
the travel plane, its original sector floor is retained for the return path,
HITAG delays a second activation, and grounded players/stationary sprites ride
the plane. The effector filters zero-tagged walls with HITAG 9999 as Chocolate
does. This has only received static source review; collision, sound timing and
scenario parity remain to be checked later.
SE 32 true-drop-ceiling uses the same target initialization, retains its own
original ceiling, and moves with the doubled Chocolate arrival threshold.
Actors are not carried by the ceiling; the completion sound is emitted on the
plane transition. This path also has only static source/compile verification.
`ACTIVATORLOCKED` now toggles sector bit 16384 and sends quote 4/8; ordinary
activators obey `hitag` open/closed restrictions, with lock/direction bits
removed before sector mover dispatch. CON `addinventory ACCESSCARD` grants the
palette-matched key, and wall/sprite `ACCESSSWITCH` variants check that key,
hold a 12-tic insertion, activate the tagged switch and consume the key;
missing keys show quotes 70–72. Tagged sprite switches in the current sector
use the `neartag` ray-size checks and the shared linked texture transition.
Player-triggered switches also toggle matching forcefield collision and reset
the associated `animatewalls` phase; forcefield changes from moving effectors
remain unfinished.
Tagged sprite switches can now be used through connected sectors when the
Build `cansee` portal check succeeds. `DukeStatusBar` already draws Atomic's
three palette-coded access icons; the native first-person view now ports
`animateaccess`: `access_incs` selects the hand/card and late laser frames,
the source sprite palette tints the held card, turn/look/horizon/hard-landing
offsets position it, and any live insertion suppresses the weapon. This is
statically checked against Chocolate `player.c`; pixel placement at every
resolution and native menu integration still need visual review.

SE24/SE34 conveyors now calculate their push vector from the loaded Build sine
table and marker `yvel` (`SP`), move eligible grounded status-0/1/5/6 sprites,
apply Chocolate's blood/footprint suppression, skip the four-tile BOLT1 and
SIDEBOLT1 families plus switches, cranes, tripbombs and laser lines, refresh
carried sprites' sector membership, add the player
impulse to the fixed-point movement/friction path, and advance only floor X panning by `SP >> 7`, matching Chocolate’s
SE24/SE34 branch (the directional four-offset panning belongs to SE20). Carried actors cache Chocolate's `bposx`/`bposy`. This is a
static-source-driven implementation only. Sprite displacement adds the
quarter-vector directly, then refreshes sector membership to match Chocolate's
`setsprite` call; it does not invoke `clipmove`. Exact input/effectors ordering
and later scenario verification remain open.

`checkhitswitch` now toggles T5 for every SE24/SE34 whose hitag matches the
operated switch, before the linked activator runs. T5 pauses/resumes the
conveyor and the player receives Chocolate's on/off quote (15/2). Static
review of Chocolate `sector.c::checkhitswitch` and `actors.c::moveeffectors`;
no dynamic comparison.

SE25 piston ceilings save their original ceiling height at map initialization
and cycle between that height and the floor using the marker's `SP` value. SE35
drilling ceilings start at the marker Z, emit the eight-per-tic smoke burst
with Chocolate's angle/velocity/explosion random rolls, and follow the
two-phase ceiling motion. As in Chocolate, SE35's `T5` return clamp remains at
its zero-initialized value; it is not the pre-marker ceiling height. Shared SE
setup assigns marker `yvel` from `sector.extra`, Chocolate's `SP` source. These
are static-source-driven implementations; tic ordering, collision, renderer
interpolation and RNG parity still need later validation.

**View checkpoint (2026-11-06):** the native soft renderer now draws visible
upper/lower wall bands across sector portals and depth-tests per pixel, allowing
walls in the next sector to show through the aperture. A two-sector fixture
verifies both bands and the visible opening. The E1L1 start capture now includes
nearby masonry, railing, and per-angle LA sky tiles. This sky composition uses
the existing Atomic `pskyoff[]` table but has not yet been compared to a
reference capture. Wall/plane pegging, masked walls, sprites, overlapping rooms,
and the all-map golden set remain unfinished; D3 is not complete.

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
- Random: the Build `krand()` engine LCG on `randomseed`, using the Build
  constant `randomseed*27584621+1` and returning the high 16 bits (**verify**).
  Chocolate Duke's `duke3d.h` defines `TRAND` as `krand()`, so gameplay
  `TRAND` calls consume this same stream; do not add a second C-library RNG to
  the compatibility path. The DOS Watcom executable may have platform-specific
  library behavior, but the Chocolate trace is this project's sync reference.
  The DEMO1 frontier is the regression gate for any stream or seed changes.

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
| Tile `picanm` animation | animation clock | Uses the Build 120 Hz animation clock **(verify)** |
| `animatewalls` | gameplay loop | Called every other `domovethings` frame (15 Hz at Duke's 30 Hz) |

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
every other game tic: animatewalls(); movecyclers()
// + earthquake, lockclock++, and other global per-tic state.
```

### 7.2 Catalogue of logic to port

| Source file | Content | Notes |
|-------------|---------|-------|
| `ACTORS.C` | `moveactors`, `moveweapons`, `moveeffectors` (every SE), `movetransports`, `movestandables`, `moveexplosions`, `movefta`, `hitradius`, `movesprite`, `ifhitbyweapon`, `badguy`, `ssp`, `guts`, `ms` | The largest piece. SE lotag table in Appendix B |
| `SECTOR.C` | `operatesectors` (all sector lotags), `operateactivators`, `operatemasterswitches`, `operaterespawns`, `checkhitswitch`, `checkhitwall`, `checkhitsprite`, `checkhitceiling`, `checksectors`, `animatecamsprite`, `allignwarpelevators`, `setanimation`, `doanimations` | Room has wall-switch/activator subsets, shade cyclers, and checksectors secret/exit handling; remaining logic stays open |
| `PLAYER.C` | `processinput` (movement, friction, jump, crouch, jetpack, swim, falling, steroids), `shoot` (all weapon projectiles/hitscans by picnum), weapon state machine (`kickback_pic` per weapon), `displayweapon` (HUD gun animation), `checkweapons`, `addweapon`, `selectweapon`, quick kick, inventory use, `doincrements` (timers), `hurt floors`, view (horiz, look, tilt) | `displayweapon` is presentation: it computes from state, it must not mutate it |
| `GAME.C` | `spawn` (per-picnum spawn rules), `EGS`, `animatesprites` (render-time sprite tweaks: rotation frame selection, shadows, `ifn` cases), `displayrooms` (camera, mirror, view), `moveclouds`, cheats, `domovethings`, quote system, `nonsharedkeys` | Split sim (`spawn`) and presentation (`animatesprites`, `displayrooms`) |
| `PREMAP.C` | `enterlevel`, `prelevel` (sector/sprite preprocessing, `numanimwalls`, mirrors, cyclers, respawns), `resetplayerstats`, `resetweapons`, `resetinventory`, `newgame`, player spawn | Order of prelevel passes is behaviour. Room captures GPSPEED/CYCLER state and initializes mirror destination sectors; reflected-view rendering and several respawn target classes remain open |
| `GAMEDEF.C` | CON compiler + VM | §6 |
| `MENUES.C` | menus, save/load, `loadplayer`/`saveplayer` | Save format §9.4 |
| `RTS.C`, `SOUNDS.C` | remote ridicule, sound selection/priorities | §9.2 |

Rules:

- **Sim/presentation split.** Vanilla mixes rendering-time logic into the
  frame (`animatesprites` changes `tsprite` copies only, which is fine;
  anything that writes `sprite[]` from the draw path is a bug to flag). Put
  presentation code behind `BuildView` so the renderer never mutates the
  world (AGENTS rule 2).
  ART `picanm` tile cycles are sampled from the 120 Hz Build clock using
  `4 * DukeSession::tic` in the native soft renderer; this is render-time
  texture selection and does not implement simulation-side `animatewalls`
  or `doanimations`.
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
| Mirrors | A masked `MIRROR` wall initializes its destination sector's ceiling/floor tiles and wall textures in Chocolate's `premap` pass. Room performs that mutation and renders the nearest projected mirror using a non-recursive reflected camera pass; the renderer recognizes the prelevel-resolved CON tile through the destination-sector marker, with Atomic tile 560 as fallback. Chocolate selects one nearest visible mirror per view, so rendering every mirror in one frame is not required for parity. Mirror-sector composition, visibility scaling and raster parity remain open |
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
  `DukeSession::level_bonus_stats` now uses the Atomic player counters
  `max_actors_killed` and `max_secret_rooms`, initialized by Chocolate's
  prelevel sector/spawn rules; current kill/secret counts use the matching
  `actors_killed` / `secret_rooms` fields. The counters are also decoded from
  and exported to Atomic SAV records. Native solo still displays them on a
  timed, playsim-pausing panel. Dynamic CON-spawned actors, Atomic
  art/animation and stepping sounds, input advance, and multiplayer
  intermission remain required. Campaign transitions reset per-level counters
  using Chocolate's `resetprestat` rule.
- **ANM playback**: full LPF playback at the vanilla rate, with the per-frame
  sound cues for each cutscene (hardcoded frame → sound tables in
  `menues.c`, `playanm`/`endanimsounds`/`logoanimsounds`). Room currently
  plays the `endanimsounds` frames for E1/E2 and `endanimvol41` for E4 through
  active CON labels and the centered sound pipeline; all other cue tables and
  full LPF playback/skip flow remain open. Replace the current still-frame
  title/finale.
- **Automap** (`drawoverheadmap`): wall lines by type, sprites, follow mode,
  zoom, rotate. Vanilla colours.
- **Cheats**: the Chocolate `cheatquotes[]` table in `GAME.C` lists `dn`
  followed by `cornholio`, `stuff`, `scotty###`, `coords`, `view`, `time`,
  `unlock`, `cashman`, `items`, `rate`, `skill#`, `beta`, `hyper`, `monsters`,
  `todd`, `showmap`, `kroz`, `allen`, `clip`, `weapons`, `inventory`, `keys`,
  and `debug`. The current Chocolate source has no `cosmo` cheat.
  Native desktop input now recognizes `DN` followed by `cornholio`, `kroz`,
  `stuff`, `weapons`, `items`, `inventory`, `keys`, `showmap`, `view`, or
  `coords`, `unlock`, `cashman`, `clip`, `hyper`, `beta`, `todd`, `allen`,
  `rate`, `monsters`, `debug`, `time`, `skill#`, and `scotty###` in solo games below
  skill 5 and outside demo recording/playback. `scotty###` validates the
  one-based Atomic volume/map pair and transitions through the campaign loader.
  `rate` toggles the native FPS and campaign map-name overlay corresponding to
  Atomic `ud.tickrate & 1`.
  `monsters` cycles the solo `actor_tog` modes: mode 1 hides badguys and skips
  their CON tick, mode 2 assigns Chocolate's `cstat=257` and leaves actor
  execution enabled, and mode 3 follows the actor code's normal fallthrough.
  `skill#` accepts skill digits 1–5 and reloads the current campaign map with
  the selected difficulty using Chocolate's `MODE_RESTART` fresh player state.
  `debug` toggles a native playsim HUD with tic and live stat counts. Chocolate's
  `caches()` lock-byte list has no direct equivalent in Room's resource model;
  that exact cache diagnostic remains open.
  `time` is accepted as Chocolate's no-op code: it clears the cheat sequence
  without changing visible or simulation state.
  These apply Atomic player god, loadout, inventory, access-card, campaign,
  view, clipping, actor-toggle, and FTA quote state. `showmap` toggles all Build
  automap sectors visible;
  `view` toggles the existing shoulder camera and `coords` toggles the live
  player position / angle / sector overlay. `unlock` clears the locked bit on
  eligible sectors in reverse sector order, invokes their normal operation,
  then toggles every animated force field as `GAME.C::cheats` does. `cashman`
  toggles the Atomic Use-action hook and emits two money sprites per held-use
  tic with Chocolate's ordered TRAND calls. `clip` toggles `ud.clipping` and
  switches player movement between direct XY + `updatesector` and normal
  `clipmove`/`pushmove`; other actors continue to collide normally. This now
  covers every command in the Atomic `GAME.C::cheatquotes[]` table; `time` is
  intentionally a no-op. Original quote presentation for every command,
  Chocolate's cache-lock diagnostic, and cheats outside that table remain
  presentation or extension work rather than missing vanilla commands.
- **F-keys**: F1 help, F2 save, F3 load, F4 sound, F5 music, F6 quicksave,
  F7 view mode, F8 messages, F9 quickload, F10 quit, F11 gamma.
  F11 cycles `ud.brightness` in eight-unit steps (Shift reverses), wraps
  between 0 and 56, and applies `BRITABLE[brightness >> 2]` to the palette.
  The table follows `sintable`, `radarang`, `textfont`, and `smalltextfont`
  in `TABLES.DAT` (offset 7424, 1024 bytes).

### 9.2 Audio

- `definesound` semantics: pitch randomisation (`minpitch`..`maxpitch`),
  priority, type bits (repeat/loop, MSFX, Duke talk (exclusive voice), parental
  lock, global (no 3D)), volume. Port `xyzsound`/`spritesound`/`sound`
  selection, voice stealing by priority, per-sprite sound slot limits and
  `Sound[num].num` counting (CON `ifsound` depends on it).
- 3D: Build distance attenuation and pan from `SOUNDS.C`. Feed Room's mixer
  with explicit gain/pan per voice, not its Doom attenuation model.
- `MUSICANDSFX` sprites: lotag = sound, hitag = radius. They are played and
  stopped by `movefx` (sim) as **sound events**, never from the renderer. The
  randomized sound index and next interval reuse Chocolate's cached
  `global_random` value for that tic; this path must not consume extra
  `randomseed` draws.
- Music: `.MID` from the GRP through `room-music` (General MIDI / SoundFont
  default; OPL optional). Desktop and browser native Duke now select the
  Atomic CON `music` table entry for campaign maps; the browser starts it on
  audio unlock and switches it when the campaign loader changes maps. The
  desktop also supports F5 report/cycle and `MUSICANDSFX` environment tracks;
  browser environment-track switching and RTS playback remain open. The browser
  now consumes full source-positioned Duke sound events, including stop/loop,
  per-sound voice limits, talk exclusivity, ambience, priority and distance
  offset metadata, and feeds its listener and reverb-zone state to the shared
  mixer; its `definesound ps/pe` pitch selection and underwater pitch rules
  use the same presentation helper as desktop. Actor-owned voice replacement,
  dead-badguy cleanup and the timed-exit sound-start cutoff are also wired on
  browser. Browser `cansee` applies the same 1/32 blocked-sound penalty.
  Browser host pages can set sound/music/talk/lockout/ambience/stereo options
  and 0–255 volume through `roomSetDukeAudioOptions`; a built-in browser F4
  menu remains open. Browser native Duke also handles F7 as a local renderer
  shoulder-camera toggle; it applies a copied player view and does not change
  lockstep state or checksums.
- RTS: `DUKE.RTS` lumps on Alt+F1..F10 in multiplayer. `room-build::DukeRts`
  now parses the bounded IWAD directory and exposes borrowed `RTS1`…`RTS10`
  payloads. Desktop decodes these VOC/WAV samples, plays them on Alt+F1..F10,
  honors local sound/lockout settings and broadcasts a Room presentation-only
  event to peers outside deterministic tic input. This Room envelope is not
  Chocolate's network wire protocol. Browser native Duke also loads and plays
  them on the same hotkeys locally, subject to sound/lockout options. A playing
  RTS voice blocks overlapping requests in both shells, following Chocolate's
  `rtsplaying` latch. The desktop HUD shows `RTS PLAYING` while a local or
  received RTS voice is active. Browser peer replication and a visible latch
  indicator remain open. Chocolate's packet handler chooses VOC when the lump begins
  with `C`, otherwise WAV, and marks the remote sound as active.
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
- Room currently implements the first partial format as
  `DukeSession::save_fsave` / `load_fsave`, JSON envelope
  `ROOM-DUKE-FSAVE` v1. It stores the loaded Build world, mutable session
  state, RNG, player, actor temps, tile metadata, active wall/sector motions,
  and active shade cyclers, and validates a fingerprint of the preloaded CON
  image. Imported mirror/cloud tables and parsed campaign/`ud` globals are
  retained in `.fsave` through `AtomicSaveGlobals`, including saved
  animated-sector tables and `gotpic`, but are not yet wired to live behavior.
  Unmodeled runtime effects, complete `GAME#.SAV` round-trip and reference-engine
  demo sync remain open. Atomic v117 `.DMO` recording and playback now support
  1–8 seats, including game/player metadata and map/roster bootstrap, through
  `DukeAtomicDemoRecorder` and `DukeAtomicDemoPlayer`. See D9 in
  `docs/21-milestone-progress.md`.
- The bounded `SavHeader` parser consumes the leading 160×100 save-tile image
  before Chocolate `kdfread` multi-block byte-delta arrays. It preserves Build
  sector panning, sprite offsets/velocities, fixed intrusive lists, cyclers,
  animation walls/targets, sprite deletion queue, mirrors, automap bits,
  `actortype`, board filename, clouds, CON pointer flags/code, actor-script
  offsets, per-sprite flags and every `weaponhit` record. Array counts, save
  ordering and scalar widths follow `MENUES.C::loadplayer`, `duke3d.h`, and
  `filesystem.c::kdfread`. `DukeSession::restore_atomic_sav` restores the
  modeled native world, up to eight connected player records, clocks/RNG, CON
  pointers, actor dispatch and `weaponhit`/script-temp state. The modeled
  player decoder uses pointer-width-invariant offsets through the 532-byte
  prefix before `posx`, then selects the post-`palette` tail offsets from the
  932-byte 32-bit or 944-byte 64-bit record stride. A Rust `repr(C)` witness
  with compile-time size/offset assertions anchors the native ABI; the earlier
  Room 936-byte draft record is accepted for read compatibility. Desktop
  `.SAV` dispatch uses it in native Duke mode.
  Multiplayer restore follows `connecthead` / `connectpoint2` and resolves
  each record through its APLAYER sprite index. `SavHeader::encode_atomic`
  emits the modeled `saveplayer` payload. `DukeSession::save_atomic_sav`
  refreshes modeled world/list data, every active seat's represented player
  state, CON/actor state, clocks/RNG and retained `AtomicSaveGlobals`. It can
  start from an imported template or initialize an Atomic v119 model for a
  fresh session; F2/F3 persist the Atomic companion beside Room's `.fsave`.
  Unmodeled `ps[]`/`ud` state, exact fresh-save defaults, byte round-trip and
  successful loading by Chocolate remain unverified, so D9/T1.8 are
  incomplete. Export fails explicitly if Room's unsigned session tic count
  exceeds Atomic's signed 32-bit `lockclock` field; it never saturates the
  saved clock and changes the resumed timeline silently.
- The sprite array is now read in Chocolate's fixed `sprite[MAXSPRITES]`
  position immediately after `sector[MAXSECTORS]`; the native file contains no
  `numsprites` scalar at this point. A legacy compact-dump fallback remains for
  prior Room fixtures. Static `menues.c::loadplayer` / `saveplayer` review only.
- `SavHeader::encode_atomic_prefix` writes the header through the thumbnail;
  its continuations emit fixed world arrays, intrusive lists, cyclers, player
  records, animation tables, script/actor state and the trailing globals in
  Chocolate order, using `dfwrite` delta records and raw Ken blocks while
  retaining sector/sprite padding. The session export refreshes compiled CON
  words/relocations, actor dispatch, actor-script offsets, modeled
  `weaponhit`, shade cyclers, animated-wall entries, player starts, campaign
  data, all active player records and the supported frag matrix. Fresh models
  use conservative defaults for unmodeled tables; these still need auditing
  against Chocolate initialization. Native helper paths that repurpose CON
  T2/T5/T6 slots still need to maintain the pointer-kind mask. Some
  animated-sector, queue and option state remains template-backed where
  available. No Atomic round-trip or reference-engine load has been run.
- Original `GAME#.SAV` (1.5): complete native-state coverage for every
  `loadplayer`/`saveplayer` field, audit fresh-save defaults and ABI variants,
  then verify load → save → load and nonvolatile byte preservation (T1.8).

### 9.5 Demos

- `.DMO` (1.3D/1.5) playback through `DukeRuntime` at 30 Hz with original
  `DukeInput` (no conversion to `TicCmd`). Recording `.DMO` in 1.5 format too,
  so route files (T1.3) can be checked in the reference engine.
- `.fdem` for Duke stores `DukeInput` streams + per-tic checksums.
- Room's partial `.fdem` v1 is implemented by `DukeDemoRecorder`: it records
  native inputs after a tick and verifies a SHA-256 state digest on replay.
  It uses a Room envelope and is not Atomic `.DMO`. `DukeAtomicDemoRecorder`
  writes 1–8 seat Atomic revision 117 records in raw Ken `strtot=0` chunks
  through the shared `dfwrite` block encoder, preserving coop and friendly-fire
  header settings and per-seat `player_struct::aim_mode` / weapon-preference settings from the
  native session. Playback applies these settings before advancing the first
  frame; preference-based auto-switch follows the saved Chocolate order.
  `DukeAtomicDemoPlayer::load_session` also loads the recorded map/CON and
  builds the saved skill and multiplayer roster from ordered GRP layers;
  `session_from_grp_demo_layers` applies the same saved roster and player
  settings for the native trace/debug loader.
  Map/script fingerprint binding, route orchestration, complete Atomic SAV
  state coverage, and
  verified cross-engine sync remain required. See D9 in
  `docs/21-milestone-progress.md`.

### 9.6 Multiplayer

- Co-op and DukeMatch through `GameRuntime` + lockstep: `ud.coop`,
  `ud.marker`, `ud.ffire`, `ud.respawn_monsters/items/inventory`,
  `ud.monsters_off`, frag counting, player colours via `pal`, spawn points
  (`APLAYER` with lotag), weapon stay in DukeMatch.
- Server verifier runs `DukeRuntime` headless and compares checksums.
- `room-proto` exposes `DukeSyncInput`, a bounded ten-byte little-endian codec
  matching Chocolate's packed `input` record, and FTR2 Duke TIC_CMD /
  seat-masked TIC_FRAME messages with tic indices. These are wire primitives;
  `DukeSession` also exposes a deterministic 64-bit checksum over its complete
  versioned `.fsave` state and loaded CON fingerprint through `GameRuntime`,
  excluding presentation audio queues.
  `DukeLockstepInbox` collects one input for each connected seat at a single
  tic, rejects duplicate/late/out-of-roster inputs, and emits the ordered frame
  only when complete. The room-server 30 Hz ring relays ordered frames and its
  optional verifier advances a native session. Browser v2 and the native
  desktop client send native commands and advance their GRP session from
  ordered frames, packing weapon hotkeys/cycles into `sync.bits`. Browser
  solo and lobby sessions retain the Atomic episode/map cursor on initial
  load and each campaign transition, so later exits continue routing. Desktop uses
  the existing lockstep transport fields as a lossless envelope for Duke's
  ten-byte input. Desktop inputs also piggyback the prior completed
  `DukeSession` checksum and compare it when every peer reports for that tic;
  sessions with trace fixups skip checksum reports. Desktop exposes
  `--duke-game-mode 0|1|2` and `--duke-friendly-fire`; both values enter the
  lobby fingerprint, so peers must launch with matching options. Host-
  authoritative mode negotiation, multi-client lockstep validation, mismatch
  recovery, exact respawn/marker rules, and cross-target validation remain
  open. After the one-time death/frag transition, Room now advances the local
  dead-player view with neutral input: it applies Atomic's water drift,
  `updatesector`, `pushmove`, horizon reset, and floor-relative screen roll.
  Dead seats bypass the remaining live-player access-card, weapon follow-up,
  floor-hazard/inventory, footprint and Holoduke input work; their camera pose
  is not copied back onto the corpse before actor processing updates its angle.
  The one-time death edge also clears the jetpack/Holoduke state and stops the
  active CON `DUKE_JETPACK_IDLE` loop independently of the body's palette.
  Voice-specific scream termination is not modeled yet.
  Full death animation and spectator camera/UI behavior remain incomplete.

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
   For RNG divergence, retain the optional per-call seed log as a lower-level
   diagnostic: compare the seed before each `krand` in stream order, preserve
   Chocolate's tic label on every reference call, and include actor context for
   Room CON calls. Do not treat `endtic` markers as tick boundaries unless the
   reference loop proves they are. Instrumentation must not consume RNG or
   allocate actor labels when disabled. Never repair a trace with seed padding;
   correct the call order or the missing/extra behavior at its source.
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
| D3 | World load + static view | `BuildWorld` from `.MAP`, `prelevel`-equivalent passes needed for display, camera-oriented `scansector` portal flood, classic Build renderer with depth-tested face/wall/floor sprites, sprite origin offsets and flips, wall cstat X/Y flips and one-way visibility, wall/floor sprite one-sided culling, basic wall panning, `LOOKUP.DAT` palette swaps and `PALETTE.DAT` shade rows, free-fly camera | E1L1–E4L11 render. Overlapping-sector viewpoints are correct. Golden captures (walls/flats/sky/sprites) |
| D4 | CON compiler | Full 1.5 compiler, script image | Script image hash equals reference for Atomic `GAME.CON` |
| D5 | Core sim loop | 30 Hz tic, `processinput` (movement only), `spawn`, sprite lists, `movefta`, `moveactors` + CON VM, `TRAND`/`krand`, sprites in renderer | Native-only DEMO1 matches seed, player pose, sprite count and hash through tic 134; last measured mismatch is tic 135 (`randomseed`, 503 vs 504 sprites, one extra `JIBS6`). Static fix added Chocolate's post-hitscan ricochet RNG roll; must remeasure. Assisted trace fixups do not count |
| D6 | Weapons + damage | `shoot`, `moveweapons`, `hitradius`, `checkhitwall/sprite/ceiling`, `displayweapon`, HUD weapon overlay | DEMO1 in sync to the end |
| D7 | Sectors + effectors | `operatesectors`, every SE in `moveeffectors`, `doanimations`, transports, switches, masterswitches, respawns, cyclers, mirrors, cameras | DEMO2 + DEMO3 in sync to the end. All SE scenario tests pass |
| D8 | Audio + front end | Sound semantics, `MUSICANDSFX`, menus, HUD, bonus screen, ANM playback with cues, automap, cheats, F-keys, third-person | UI checklist + golden screenshots (T1.6, T1.7) |
| D9 | Saves + demos | Native `.fsave`, `GAME#.SAV` load/save round-trip, `.DMO` recording, `.fdem` | T1.8. A recorded `.DMO` syncs in the reference engine |
| D10 | Multiplayer + server | `DukeInput` in `room-proto`, lockstep, coop/DukeMatch rules, verifier | T1.9, T1.10 |
| D11 | Full campaign + user content | Route `.DMO` per map, user maps, `-x` CON, add-on GRP layering, switch shells (`room-app`, `room-web`, lobby) to the native runtime, move the converter under `export/` | T1.2, T1.3, T1.11. Then mark 23 §3 obsolete for gameplay |

**D5 implementation note (static port):** Atomic CON coverage now includes
`ifnosounds` against the active sound-owner table, `ifcanshoottarget`,
`ifbulletnear`, `ifnotmoving`, `ifrespawn`, `respawnhitag`, `mikesnd`,
`resetactioncount`, `wackplayer`, `tossweapon`, and the `pstomp` knee attack.
`movefta` chooses Chocolate's nearest player, requires that player's body
sprite to be alive before waking a dummy actor, and routes Atomic barrels,
cans, fire vases, wooden horses and tripbombs into `STAT_STANDABLE` with the
sector-plane shade. Sleeping badguy shade tracks the current sector each tic
while the nearest player is alive. Regular enemies now queue Chocolate's `check_fta_sounds`
recognition cue on promotion, including the palette-dependent boss sounds.
`operate` now defers its actor ray until the world is available, traverses
connected Build sectors to tagged walls, sectors, and sprites, using ART
dimensions/repeats and sprite vertical offsets for sprite tags. It enforces
Chocolate's near-operator/locked-sector/activator gates and dispatches the
sector mover.
`pstomp` gates on visibility, player size, and an idle knee; the player locks
the target for its 15-tic knee cycle, turns toward it, and on a close impact
emits the seven `JIBS6` fragments in Chocolate RNG order, spawns the initial
1×1 `BLOODPOOL` with floor/water/parallax checks, updates badguy kill count,
and arms matching civilian respawn markers. `BLOODPOOL`/`PUKE` now follow the
floor, grow for 32 tics with the reference TRAND order, and shrink at the end
of the growth phase. Nearby players take periodic palette-0 pool damage or
consume a boot charge, with the red palette flash applied on damage; the
long-term pain cue respects Chocolate's single active voice rule. Blood-pool
footprints now use the 3/10 step count, palette/shade, flat-sector and tag
gates, ±384 duplicate suppression, four-way TRAND tile choice, and Chocolate's
spawn-time floor/cstat setup. Shallow-water movement seeds six later dry-ground
footprints with the vanilla floor palette, and Room tracks the `PLAYERONWATER`
dummy sprite until water exit or player death. A `pstomp` APLAYER hit now
applies Chocolate's zero-health, hidden-body and palette flash quickkill,
spawns eight guts unless god mode is active, and records killer/victim seat
frag or self-frag counters. Death animation/respawn, the frag matrix/scoreboard,
warping-sector ripple animation, and full multiplayer attribution remain open.
The `operate` port still lacks animated ART offset handling, exact ray/tag tie
ordering, and same-actor-pass mover ordering. This is a
source-guided implementation slice only: do not treat it as satisfying D5's
trace acceptance until the native trace is remeasured without trace fixups.

**D4/D5 CON presentation opcodes (2026-11-07):** VM opcode 68 `print` now
emits its integer operand as a decimal stdout line, matching Chocolate's
`printf` diagnostic. Opcode 14 `palfrom` queues its duration and three RGB
channels during parsing, then applies the flash to the owning player after
that actor's script, preserving actor-order precedence. Static inspection of
Chocolate `gamedef.c` plus targeted compilation only; no gameplay comparison.

**D5 `resetplayer` (2026-11-07):** VM opcode 42 now follows both modes.
Solo play defers the request to the host; the desktop restores the initial
Room `.fsave` snapshot, or opens the load menu when a previous Duke save slot
exists. Native sessions retain APLAYER starts in Chocolate `po[]` order.
Co-op uses the player seat index; DukeMatch consumes `TRAND` at opcode
execution time and selects `TRAND % numplayersprites`.
Both multiplayer paths restore the player's start pose, baseline loadout,
health, and APLAYER body state. Static inspection of `gamedef.c::resetplayer`,
`premap.c::pickrandomspot`, and the `po[]` fill loop, plus targeted compile;
no gameplay comparison.

**D5 partial `movefallers` port (2026-11-07):** the native tic now includes
Chocolate's `moveplayers → movefallers → moveexplosions` placement. Tagged
blocking sprites are classified to STAT 12; triggered linked groups receive
the original collision-flag transition, `impact_damage` health, accepted-hit
damage filtering, optional delay, randomized launch velocity, horizontal
damping, sky-aware gravity, floor removal and the ordered SCRAP6 piece/RNG
sequence. Its `IFHIT` probe now follows `ifhitbyweapon` for the non-player
path: health-negative actors reject hits, small zero-damage SHRINKSPARK hits
remain queued, valid hits update an active owner, and non-trigger weapons have
their damage rolled back while the zero-damage hit remains latched. The map
classifier follows Chocolate's `cstat&48` gate and excludes
the listed speaker/letter/duck/target/tripbomb/view-screen/crack sprites and
wall switches. SE19's `IFHITSECT` trigger now matches Chocolate's
`ifhitsectors(sectnum)` by scanning STAT_MISC for an EXPLOSION2 in that exact
sector. XY motion uses the shared ordinary `movesprite` path: clipping returns
its native hit code, sector links update only on an actual portal transition,
the base Z step uses cached actor ceiling/floor bounds, and a clear XY result
with blocked Z returns `16384 + sect`. Badguy-specific rollback/angle branches
now preserve Chocolate's stayput/boss/water rollback checks and random-turn
branches; large actors use the 1024-unit clip radius, LIZMAN uses 292, and
other badguys follow the `actortype&3`/192-unit rule. Their candidate Z uses
the cached `hittype.ceilingz/floorz` interval and returns `16384 + sect` when
only the vertical move fails. Accepted badguy portal transitions now update the Build sector lists only
after stayput/boss/water rejection, matching Chocolate's delayed
`changespritesect`. Face, wall-aligned, and floor-aligned sprite clips now use the shared Build
primitive; the final no-BFS sector fallback now scans all XY-containing sectors in reverse
order and chooses by Z clearance using sloped ceiling/floor planes. The native
player, badguy, faller, poolball and pipebomb call paths now provide the live
sector sprite list so overlapping blockers resolve in Chocolate list order. Other exact
sector/sprite collision edges and remaining actor-specific movement cases remain
incomplete. These newly ported wall/floor
clip paths are compile-checked only; Chocolate golden vectors remain needed, especially for list-order ties and
sloped-sector fallback. Static
`game.c::spawn` / `actors.c::movefallers` review plus a native
compile check only; no dynamic Chocolate comparison or gameplay run.

The `STAT_DEFAULT` QUEBALL/STRIPEBALL mover now follows Chocolate's active
ball path: nearby POCKET capture, `CLIPMASK1` XY travel, wall-angle reflection,
ball-to-ball impulse/deflection, one-unit-per-tic friction, and the stopped-ball
proximity kick. STRIPEBALL recomputes its low cstat animation bits as it rolls.
Shot impacts also use the source's 3-in-4 rolling impulse / 1-in-4 glass break.
Player-directed use-bit aiming now follows `toggle_key_flag`: the selected
player must face the ball within the native 64-angle cone and within 1596
distance, nearer balls in that cone block the kick, and holding the key cannot
repeat a kick until release. The per-player latch is restored from Atomic SAV
when present and defaults to clear otherwise. Exact active-sound voice
counting, the full `findplayer` tie order and exact sprite/portal clipmove cases
remain open. This is not pool-table parity validation; only static source
inspection and crate compile checks were used.

**D9 multiplayer Atomic recording (2026-11-07):** `DukeAtomicDemoRecorder` now keeps a rectangular input stream for each of 1–8 seats and `tick_session_inputs` advances the native roster before recording the exact seat-ordered frame. The Atomic v117 writer preserves player count, coop/friendly-fire settings, per-seat aim mode and ten-slot weapon preference, and each seat's packed inputs. Playback applies the captured settings to the native roster before its first tic; `DukeAtomicDemoPlayer::load_session` also loads the recorded map/CON from ordered GRP layers at the stored skill and constructs the correct multiplayer roster. Weapon auto-switch reads the player's configured preference order using Chocolate's one-based weapon IDs. Map/script fingerprint binding, route orchestration, Atomic SAV writing, and cross-engine validation remain outstanding; only static source review and compile checks were run.

**D9/D11 Atomic demo shell playback (2026-11-07):** desktop console command
`playatomicdemo <path.dmo>` now parses an Atomic revision-117 demo, replaces
the current native session with the recorded map/skill/seat roster loaded from
the active ordered GRP stack, and advances seat-ordered inputs at the native
30 Hz tic rate. When a recorded exit completes, the shell uses the native
campaign cursor to load the next map and retains the same Atomic input cursor;
each transition also refreshes the level restart snapshot. End-of-stream and
`stop` terminate playback; recording or starting Room `.fdem` playback cancels
the Atomic stream. Atomic playback now compiles with the active `--con` root
and resolves its external includes from that root's directory, matching
campaign map loads. Route behavior is not reference-validated. Formatting,
diff check and targeted `room-console` / desktop binary compilation only.

**D9 desktop Atomic DMO recording (2026-11-07):** native Duke `record` now
starts the Room `.fdem` recorder and an Atomic v117 recorder from the same
session metadata and active 1–8 seat roster. Each native tic appends its exact
input to both recorders; `stop` writes both `demos/<slot>.fdem` and
`demos/<slot>.dmo`. Direct weapon keys for slots 0–9 and previous/next cycling
are inserted into the DMO's `sync.bits`; the desktop-only direct remote
detonator selector still lacks an idle Atomic selector encoding. This makes
the existing Atomic encoder reachable from the desktop workflow. Campaign
exits remain held while recording, matching the
existing single-map recording path; playback and cross-engine validation are
still separate and open. Static call-path review, formatting, diff check and
targeted compilation only; no test, gameplay or reference comparison.

**D9 Atomic SAV live-state export (2026-11-07):** `DukeSession::save_atomic_sav` updates the parsed Atomic template with current Build geometry, all 4096 sprite slots and exact intrusive lists, plus modeled player pose/momentum/weapons/ammo, inventory, camera, shield, movement/death and selected timer fields for the active roster, skill, campaign coordinates, coop/friendly-fire settings, the modeled frag matrix, clocks and RNG, then writes the complete modeled `saveplayer` payload. Unknown or unmodeled player, CON and global state remains from that template. F2/F3 native slots write/read an Atomic `.sav` companion when one is available. Atomic saves are therefore not standalone for regular sessions, and a true load-save-load round-trip is not verified. Static `menues.c::saveplayer` / `loadplayer` ABI layout review, formatting, diff check and targeted compilation only; no tests or runtime/reference comparison.

**D9 multiplayer SAV player records (2026-11-07):** Atomic export now accepts active multiplayer rosters when an imported template contains Chocolate's fixed 16-entry `ps[]` array. It updates each active seat's modeled pose, prior horizon, momentum, sector, weapon/ammo, health snapshot, shield, oxygen, death, jump/holster/access/weapon timers, reservations, inventory flags, damage cooldown, fall/landing state, weapon aim/selection latches, screen roll, footprint, six active loogie overlay positions/countdown and camera-owner fields, retaining opaque bytes in each record. Restore follows the saved connection chain and maps those fields back to each seat and its APLAYER body, including the saved palette. The loogie arrays' unused entries 6–63 remain template-preserved. The header seat count, `playerswhenstarted`, `connecthead` and seat-ordered `connectpoint2` chain now match the live one-to-eight seat roster. Invalid array sizes/references fail explicitly. Other multiplayer metadata, fields not represented by `Player`, standalone Atomic save creation and the load-save-load round-trip remain open. Static Chocolate `duke3d.h::player_struct` / `menues.c` ABI review, formatting, diff check and targeted compilation only; no gameplay, tests or reference comparison.

**D9 Atomic player-origin table size (2026-11-07):** `save_atomic_sav` now validates all 16 `po[]` entries, matching Chocolate's `MAXPLAYERS == 16` declaration and `dfwrite(po, sizeof(po), ...)`. Room refreshes the entries it models (up to eight seats) and retains the other records from the imported save. This fixes rejection of correctly sized Atomic templates; standalone creation and round-trip verification remain open. Static `duke3d.h` / `menues.c` layout review and targeted compilation only; no runtime/reference comparison.

**D9 live CON / `weaponhit` export follow-up (2026-11-07):** the writer refreshes compiled CON values and pointer flags, actor-script offsets, `actortype`, and modeled `weaponhit` fields from the loaded script and current `ActorConTemps`. T2/T5/T6 pointer kinds are restored from Atomic SAV and tracked on CON `action`, `ai`, and `move` operations and actor-header initialization; native helper paths that repurpose these slots still need to synchronize the mask. A later ABI audit corrected `picnum` to offset 2 and modeled `dispicnum` separately at offset 16; this removes the previous confusion between an actor's CON tile and its display override. Remaining unmodeled `weaponhit` data is retained from the template. Atomic save round-trip remains open. Static `menues.c::saveplayer` / `duke3d.h::weaponhit` review and targeted compilation only; no tests or reference-engine comparison.

**D10 multiplayer `resetplayer` actor state (2026-11-07):** the multiplayer CON `resetplayer` path now clears Room's modeled `hittype[g_i]` values matching Chocolate's `gamedef.c`: queued hit (`extra`), owner, `cgg`, `movflag`, `tempang`, `actorstayput`, and the separate `dispicnum` display override. It preserves the authoritative `picnum` field. Map start selection now follows Chocolate `pickrandomspot`: `po[0]` is the map-header start, selected-mode APLAYER markers follow in STAT_PLAYER head order, co-op indexes by seat, and DukeMatch chooses `TRAND % numplayersprites`. Marker visualization and full multiplayer behavior still need validation. Respawn now also resets the modeled `resetplayerstats` transient state (view/bob, movement, death, camera, holoduke, jetpack/heat, input latches and weapon animation); it preserves coop weapons/inventory, while DukeMatch/no-spawn uses Chocolate’s `resetweapons` / `resetinventory` path. Static source/ABI review and targeted compilation only; no gameplay or reference comparison.

**D9 Atomic player score/settings fields (2026-11-07):** `SavPlayer` now parses the Chocolate `player_struct::frag` and `fraggedself` shorts plus the 32-bit `aim_mode`. Atomic restore applies these values to Room's player state, and live save export refreshes them for the primary record and every active Room multiplayer seat at the verified ABI offsets. Score words are preserved modulo the 16-bit Atomic field width. Full multiplayer SAV round-trip and unmodeled player fields remain open. Static `duke3d.h` offset review and targeted compilation only; no save replay or reference-engine comparison.

**D10 APLAYER spawn grouping (2026-11-07):** `configure_multiplayer_mode` selects map APLAYER starts with Chocolate's exact `lotag == ud.coop` rule for modes 0/1/2 and consumes them in STAT_PLAYER head order, while seat zero uses the map header start as `resetpspritevars` does. The bool `configure_multiplayer` API remains as a 0/1 wrapper. The `po[]` table now keeps the map-header start at index zero and only the selected-mode APLAYER markers afterward; co-op resets use the seat index and DukeMatch uses the source `TRAND % numplayersprites` selection. Marker visualization/target respawns and network mode behavior still need completion/validation. Only static `game.c` / `premap.c` review and compile checks were performed.

**D10 mode-specific initial loadout (2026-11-07):** the multiplayer roster setup now mirrors `resetprestat`: for DukeMatch/no-spawn with more than one seat, `resetweapons` restores pistol/knee/remote and 48 pistol rounds, and `resetinventory` clears carried inventory while restoring the CON-configured max armor. Co-op mode preserves the carried loadout.

**D10 lethal APLAYER transition (2026-11-07):** lethal player-owned hits now retain the killer seat and, at the next `processinput` phase, apply Chocolate's one-time death response: red palette flash, 16-unit body/player Z drop, ordered two-draw `dead_flag`, jetpack/holoduke shutdown, and frag/self-frag credit. Dead seats keep movement and firing neutral; in co-op/DukeMatch, a rising `INPUT_OPEN` edge queues the existing multiplayer `resetplayer` path, while no-spawn mode remains disabled. PSTOMP quickkills use the same transition; Atomic SAV restores `dead_flag` and `frag_ps`. `DukeSoftRenderer` derives Chocolate's floor-relative death screen roll from `dead_flag` and current player/sector Z without mutating simulation state. Killer/victim scores are retained in a session-owned row-major 8×8 frag matrix and `.fsave`. Full support is still open: dead-player camera/motion progression, spectator flow, and exact respawn marker/selection rules are unimplemented. Static Chocolate `player.c` / `quickkill` review, formatting, diff check and targeted crate compilation only; no gameplay, tests or reference comparison.

**D10 dead-player camera progression (2026-11-08):** the dead-player update now keeps the camera at the APLAYER body, turns it halfway toward the last valid attacker each tic, applies the death branch's water-warp Z behavior, refreshes the Build sector with `updatesector`, resolves `pushmove`, and calculates the floor-relative roll from the updated position. Each multiplayer seat receives the same update, and body angle follows the player camera turn. This closes the camera/body-following portion of the earlier death note; full death animation, spectator flow, respawn marker edge cases, and multiplayer/reference validation remain open. Static Chocolate `actors.c` / `player.c` review, formatting, `git diff --check`, and targeted `room-duke` compile only; no tests, game, network session, or reference comparison.

**D10 per-seat player actor processing (2026-11-07):** the APLAYER loop now associates each live body with the matching `Player::sprite_i`, reads that seat's `DukeInput`, and advances water actions against the correct player's movement flags. Holoduke actors now remain active when owned by any seat, and each follows its matching player's angle during spawn animation. This fixes the prior seat-zero-only assumptions in `moveplayers`; it does not complete player respawn or every shared APLAYER script interaction. Static review of Chocolate's player sprite ownership and APLAYER `move()` ordering, formatting, diff check and targeted compile only; no game run or cross-engine comparison.

**D10 v2 room mode negotiation (2026-11-07):** Duke room creation now accepts `duke_game_mode` (`0` DukeMatch, `1` co-op, `2` no-spawn) and `duke_friendly_fire`; the lobby exposes both controls. The server includes them in `WelcomeRoom`, each browser constructs the same native `DukeSession` roster/configuration before sending `Ready`, and the optional server verifier uses those settings too. Legacy room transport still uses its prior default, and deterministic lockstep/network match validation, respawn/spectator behavior and server-side mode policy remain open. Static protocol/Chocolate `ud.coop` mapping review, formatting, diff check and targeted compile only; no multiplayer match was run.

**D10 Atomic player palettes (2026-11-07):** native player bodies now take palette 9 for the map-header seat, then cycle 10–16 for zero-palette APLAYER starts in connect order; nonzero palettes authored on map starts are preserved. Atomic SAV export refreshes each active seat's `player_struct::palookup` from its APLAYER body. Chocolate's `premap.c` assigns these palette values while traversing the STAT_PLAYER list and connection chain. The soft renderer now applies loaded `LOOKUP.DAT` remaps to face/wall-aligned sprites and walls. Sector floor/ceiling palette remaps and palette-dependent shade tables remain open. Static `premap.c` / renderer review, formatting, diff check and targeted compile only; no gameplay or screenshot comparison.

**D11 progress:** Atomic `NUKEBUTTON` use now records its owner and palette,
animates the two button stages, runs the player's `fist_incs` sequence, and
raises the native level-exit request. A serializable campaign cursor applies
the `buttonpalette` / `secretlevel` / `from_bonus` progression rules and the
native shell seeds it from an `E#L#` map name. The desktop shell now retains
GRP/CON layers, consumes next-level transitions, loads the target Atomic map,
rebuilds the session/audio bank, and carries persistent player health, armor,
weapons, ammo and inventory while adopting the next map's spawn. Browser
single-player and v2 lockstep sessions initialize and retain the same E#L#
cursor across route/load/carry transitions. Browser APIs also accept a
standalone Build `.MAP`, a replacement root CON and named external includes;
these use the same native loader and GRP resources. `roomAddDukeGrp` layers
local add-on archives in browser solo sessions, and later archives override
maps, CON includes, ART and sound resources. Browser co-op refuses locally
registered add-ons because the lobby protocol does not distribute or bind
their fingerprints yet. Coop map transitions still lack a server barrier.
Bonus-screen presentation, full end-game menu flow,
map-route audit and reference validation remain open; this does not satisfy
T1.3.

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
| 4 | random light flicker | 23 | no `moveeffectors` case in Atomic 1.5 |
| 5 | boss elevator / locator controller | 24 | conveyor |
| 6 | subway engine | 25 | piston |
| 7 | transporter / water link | 26 | escalator |
| 8 | door-up lights | 27 | demo camera marker (runtime camera-control body disabled in Atomic 1.5) |
| 9 | door-down lights | 28 | lightning |
| 10 | door auto-close | 29 | floating (waves) |
| 11 | swinging door | 30 | two-way train |
| 12 | light switch | 31 | floor rise |
| 13 | C-9 explosive | 32 | ceiling fall |
| 14 | subway car | 33 | quake debris |
| 15 | sliding door | 34 | conveyor (variant) |
| 16 | reactor | 35 | drilling ceiling |
| 17 | transport elevator | 36 | shooter |
| 18 | Kroz room-over-room plane motion | — | — |
| — | — | 40–45 | legacy/extended room-over-room tags; no Atomic 1.5 `moveeffectors` cases |

SE17 elevator motion also carries the `player_struct::truefz` and `truecz`
plane caches alongside player Z. On a paired-sector transfer it replaces
those caches with the destination planes, resets `bobcounter`, and refreshes
the player's previous pose. Normal player movement computes body `xvel` from
post-move displacement, advances `bobcounter` while grounded and then refreshes
`bobpos`, as `player.c::processinput` does. These player fields are represented
in Room state and Atomic SAV load/save.

Room currently advances SE 4's randomized flash-light behavior after
`moveactors`: saved baseline shades/palettes, floor/ceiling updates, wall
updates with linked-wall shade propagation, and face-aligned sprite shading.
SE 12 switch-controlled lights also save the initial floor/ceiling shades,
animate their turn-on transition, restore the saved shades on turn-off, and
remove an effector marked for ceiling-hit extinguish. The player hitscan path breaks the six Atomic ceiling-light tiles, creates
the ceiling glass shards, queues the breaking cue, signals linked SE12
extinguish, and triggers matching SE3 flicker state. RPG ceiling-plane impacts
now call the same handler after Chocolate's sky/palette deletion gate and
relink the projectile sector before detonation. The shared RPG/CON nearby-
sector radius traversal also checks each visited ceiling before its wall loop,
using Chocolate's vertical-radius gate and first/third boundary-vertex distance
checks to invoke the same light/glass effect. Triggered SE3 applies the shared-tic randomized palettes and
shade to the sector and linked walls. This still does not cover other
`checkhitceiling` callers or the rest of the SE catalogue; interaction
scenarios and RNG parity remain unverified. SE19 now responds to sector-local
`EXPLOSION2` actors, removes matching `BIGFORCE` overlays, lowers the shield
ceiling and restores linked SE0 sector palettes/shades. The pivot-to-shield
setup follows Chocolate's SE1/SE19 hitag linkage; exact activation timing and
visual transitions remain statically reviewed only.
SE2 now preserves the floor-heinum baseline and, when its T5 countdown is
armed, runs the finite quake phase, slope recovery, player/sector-sprite
impulse, and Atomic sound/timer cadence. The shared timer is saved and the
desktop/web views apply Chocolate's grounded camera shake. Coop player
impulses, exact clipping, and T5 arming remain open.
SE33 emits Chocolate's one-in-eight `RANDOMSCRAP` burst while the shared quake
timer is active, including the ordered seven RNG draws and `SCRAP6` actor
fields.
SE28 now receives its Chocolate 65-tic initial delay, starts a 64–575 tic storm when a player is within 15,500 Build units, emits the lightning slap/thunder cues, and toggles its hitag-linked NATURALLIGHTNING actors. Visible linked strikes spawn SMALLSMOKE and damage the nearest player within 768 3D units with the original 8–15 health range and pain/short-circuit cues. Because the classic frame path does not yet expose Build's global visibility scalar, the screen flash is represented with the player palette tint; per-seat visibility behavior and RNG parity are unverified. Player state is synchronized back into each APLAYER after the effectors pass so direct environmental damage persists.
SE130/SE131 now use Chocolate's `T1 > 80` / `T1 > 40` expiry thresholds and
per-tic `rnd(64)` / `rnd(32)` gates to create EXPLOSION2 sprites with the original ordered repeat,
height, angle and speed draws, followed by an immediate clipped horizontal
`ssp` step. Exact sprite-aware `ssp` and cross-effector RNG/list ordering still
need validation.
The static `actors.c::moveeffectors` inventory identifies SE5 boss controller
as partially implemented: Room seeks the nearest player's locator route,
spawns Atomic FIRELASER projectiles within the native range, turns toward its
locator, flickers ceiling shade and advances its marker, sector walls and
ceiling. Its `ms()` wall pass rotates map-load vertex offsets by the accumulated
T3 angle around the moving marker, matching Chocolate's boss sector motion.
Queued weapon damage is consumed and the fifth hit applies the native
vertical impulse and boss quote. Its `FIRELASER` attack now reuses the shared
CON-backed effector `shoot()` path for the Atomic projectile speed, strength,
muzzle offset and collision fields instead of a hard-coded constructor.
Map-load setup caches Chocolate's adjacent
zero-hitag sector and advances its ceiling alongside the controller sector.
The marker sector now refreshes after each move through a `setsprite`-equivalent
sector lookup and sprite-list update. Chocolate's SE5 branch does not carry
sector occupants; collision rollback and unusual geometry remain open. SE6/SE14
now have a first-pass train implementation:
map-load route-sector setup, SE6 acceleration/ramp and linked-wagon speed
adjustment, matching locator selection with route wraparound, SE14 steering,
sector-wall rotation/translation, and player/actor occupant carry. The
non-parallax train loop starts at cruise speed and stops while braking. In
parallax sectors with floor palette zero, SE6/SE14 now make Chocolate's
`rnd(8)` attack roll at cruise speed, select the nearest player within 20,480
units, and fire an RPG along the source's temporary player-facing angle.
Moving SE6/SE14 cars now crush live badguys when the train sector clearance is
under 108 Build units, spawning Chocolate's 72 `JIBS6` for SE6 and 24 for
SE14 and queueing `SQUISHED`; this branch is independent of `monsters_off`.
Player quickkill uses the same clearance gate, the source speed thresholds
(SE6 ≥192; SE14 >0), and runs only when `ud.clipping` is off. It checks the
player's sector both immediately before carrying occupants and after the
controller's final sector relink, syncs position and body sprite to the car,
and enters the existing death path with Chocolate's 48-tic palette flash and
eight JIBS6 unless god mode is active. SE30 quickkill also applies these
effects. Exact
linked-train ordering and interpolation remain
open; no parity claim is made. SE30 now has first-pass locator-driven translation,
acceleration/braking, occupant carry, departure/arrival activators, forcefield
toggle, narrow-gap crushing with 24 `JIBS6`, and player quickkill when clipping
is off and the car is moving. Its controller sector relinks after movement.
Map-order interactions, complete collision response and interpolation remain
incomplete.
SE30 activation still toggles its travel state at any point, but emits the
subway switch cue only at the settled `tempang` endpoints 0 and 256, matching
Chocolate's `operatesectors` gate.
SE18 applies the Chocolate map-load plane target and hitag ×4 timer, responds
to master-switch arming, runs one-shot plane motion with floor-rider carry,
and resets at timer expiry. Tagged SEENINE/OOZFILTER explosions arm a matching
SE18 only when its activation state is clear (SE21 remains unconditionally
armed); SE13 restores its cached planes, performs
its ceiling/shade transition, copies the originally cached first map-order
parallax-ceiling sector's tile and shade when restoring the sky flag (stable
across multiple SE13 markers), and emits its seven debris
sprites over its vanilla lifetime. The desktop renderer also interpolates the
sector floor and ceiling heights on its render-only world copy, including SE13
restoration; Chocolate's exact per-effector interpolation registration and
cadence remain unverified. The remote pipebomb-to-destructible path now queues visible in-radius blast
hits on Atomic CRACK1–4/FIREEXT standables. Tagged CRACK hits accept the native
FIREEXT/RPG/radius-explosion/SEENINE/OOZFILTER source set, arm matching
SEENINE/OOZFILTER groups and the supported effector chain, emit eight `RANDOMSCRAP` shards using the current standable as owner, and start
the earthquake timer. Tagged FIREEXT applies the common 32-unit Z offset before
its final eight SCRAP6 shards. FIREEXT spawns its palette-2 scrap
shower and explosion, applies its configured tagged or untagged hitradius,
queues the native explosion/glass sounds, and is removed. Exact random-call and
audio ordering, source actor collision filters, and wall side effects remain
incomplete. SE11 now has first-pass lotag-23 activation, pivot rotation,
obstruction checks and player obstruction rollback. Its movement advances the
marker by `xvel`, relinks its sector after each `ms`, and repeats `ms` after
the exact endpoint-angle snap. SE15 follows Chocolate's translating
door state: `T5` (`g_t[4]`) selects open or close, `T4` (`g_t[3]`) counts to
`SP >> 3`, and active tics set `xvel=16` before translating its marker and
cached sector walls by 16 units along the marker angle. Lotag-25 activation writes the mode to T5 and
uses T5 for Chocolate's conditional repeat sound, preserving the independent
T4 travel count. Chocolate's SE15 branch does not test actors or players for
obstruction. Exact `setsprite` effects, interpolation and global marker
ordering remain unverified. Static review of Chocolate
`sector.c::operatesectors` and `actors.c::moveeffectors`; no dynamic comparison.

Lotag-23 SE11 activation also checks that its local swing-door hitag matches
the linked ACTIVATOR hitag before it arms that group, following Chocolate's
`sprite[j].hitag == SHT` gate. Direct sector/CON activation with no tagged
activator retains the existing path. This fixes unrelated activators opening
the first SE11 group in a sector. Chocolate's `moveeffectors` SE11 branch does
not carry players or actors: it checks badguy/player overlap against current
sector walls, rolls back the angle on player obstruction, then updates the
marker and wall geometry. Room follows those branches; occupant carrying is
not an SE11 parity requirement. Exact `setsectinterpolate` rendering, global
marker ordering, and full map-scenario coverage remain open.

The desktop presentation now keeps previous/current Build wall vertices and
sector floor/ceiling heights at rendered Duke tic boundaries, and interpolates
a render-only `BuildWorld` copy from the 30 Hz remainder. This fills the visual
role of Chocolate's `setsectinterpolate`/`dointerpolations` for moved geometry
without writing interpolated values back into playsim state. Exact registration
set semantics and all effector ordering remain open; simulation/demo checksums
are unaffected by this display path.

Map spawn initializes each SE11 T4 turn direction from its marker angle:
`+2` for angles above 1024 and `-2` otherwise. Lotag-23 activation reverses
that initialized value, matching Chocolate `game.c::spawn` followed by
`sector.c::operatesectors`. This avoids a zero rotation step from an unseeded
temp array. Static source review; no gameplay or dynamic comparison.
Lotag-30 SE0 rotating sectors use the SE1 pivot linked during map spawn. SE1
T1 starts off, and a linked SE19 event arms ordinary pivot rotation. The native
path reproduces Chocolate `ms()` from cached map-load wall offsets, cumulative
angle T3 and marker orbit, updating shared vertices through Build `dragpoint`;
it carries grounded players and eligible sector actors. The SE0 marker stores
the original floor target. Its `tempang` 256-unit transition and 512-unit floor
steps raise and restore the plane according to `extra` and palette direction;
completed turns queue the local sector sound cue. Chocolate `ms()` directly
updates the walls without collision rollback. Rotated sloped floors, effectors
that overlap this sector, and gameplay parity remain open.
SE29 advances its floor with Chocolate's `hitag += 64` and
`mulscale12(SP, sintable[hitag])`, then applies the original four-wall slope
alignment against the adjacent sector reached through wall 2. This port is
statically reviewed; combined effector ordering and map scenarios remain
unverified.
SE26 now captures its map-load origin and shade-derived vertical direction,
cycles the floor in eight tics, transports sector actors and grounded players,
and translates its marker and sector walls along the escalator angle. Grounded
players are selected by their APLAYER body's sector, matching Chocolate's
`sprite[ps.i].sectnum` check; they receive the horizontal component as delayed
fixed-point movement force on their next input sample and the vertical plane
step. Active actors transported into a sloped-floor sector now run Chocolate's
`makeitfall` path after their sector update. The spawn path stores the original
marker coordinates in T4/T5 and initializes zvel from the marker/sector shade
comparison before resetting the runtime shade counter. That force is part of
serialized player state.
Chocolate's exact wall clipping, overlap ordering with other effectors, and
map/gameplay behavior remain unverified.

**D5 shared `makeitfall` gravity (2026-11-08):** Room now matches Chocolate's
outer-space floor/ceiling gravity rules (`MOONSKY1`/`BIGORBIT1`, palette 0),
the water terminal-velocity reset, and the 6144 cap in the common primitive.
This affects native spawn, slope-sector carry, and respawn-marker fall paths.
Static `gamedef.c::makeitfall` review; formatting, diff check and crate
compilation only, with no gameplay or demo comparison.
SE128's progressive glass-wall helper now also follows Chocolate's low-byte
`cstat &= 223` mask before setting translucency on the target and paired wall;
its source condition `(cstat | 32)` is intentionally treated as always true.
Sector activation now toggles SE36's `temp_data[0]` using Chocolate's
`1 - temp_data[0]` rule. Its active timer fires the native enemy
`COOLEXPLOSION1`, `RPG`, and `FREEZEBLAST` paths plus the `SHOTSPARK1` /
`SHOTGUN` / `CHAINGUN` enemy
hitscan path once per 130 tics when `sector.extra` selects the corresponding
tile. Hitscan uses Chocolate's aim-height calculation, jitter, actor-header
damage and RNG order, then queues damage through Room's player/CON path.
`COOLEXPLOSION1` now sweeps against Build geometry and sprites each tic: wall
and plane impacts stop it, ordinary actor contacts do not damage and retain
their source collision behavior, and APLAYER contacts queue actor-header
damage/body-hit audio and stop the projectile. Its shade advances to the
40-tic deletion threshold, and `WIERDSHOT_FLY` is queued through the
sound-once bridge. The hit query is a statically ported `movesprite` analogue;
exact clipmove contact ordering and the global active-voice gate are not yet
verified.
`SPIT` marker launch, target-angle jitter, velocity, launch height, actor-header
damage, gravity and direct-hit damage are also ported. `FIRELASER` launch,
targeting, muzzle offset, speed, moving beam, five-sprite trail and direct-hit
damage are ported. `SPIT` player hits now apply Chocolate's body-hit/pain cues,
horizon kick and 96-tic loogie state; the desktop HUD draws the loogie tiles on
the 320×200 virtual plane. RPG/freezer launch parameters are ported from
Chocolate `shoot`: source height and aim, muzzle offset, speed/repeats, script
strength jitter, RPG target sentinel and freezer bounce configuration. Exact
`rotatesprite` alpha/zoom and actual-resolution placement, projectile
wall/sector impact behavior, ricochet audio and exact sprite filtering remain
open, so SE36 is incomplete.
Animated sector-9 wall vertices use Chocolate Build's `dragpoint` linked-wall
traversal and `lastwall` fallback; this does not yet implement every moving
sector's obstruction or overlap rules.


**D10/D11 lockstep campaign transition (2026-11-07):** browser native Duke now consumes campaign exit requests after authoritative multiplayer frames as well as in solo play. `DukeSession::inherit_campaign_roster` reconstructs the destination map's configured seats, carries each seat's persistent `resetplayerstats` fields while retaining the destination spawn/body, and preserves frag scores, match settings, campaign metadata and actor toggle; desktop map transitions use the same helper. This removes the previous solo-only campaign transition and seat-zero-only carry path. T1.3 route coverage and T1.9 lockstep/network behavior remain incomplete. Static `game.c` / session call-path review, formatting, diff check and targeted compilation only; no network match, tests, gameplay or reference comparison.

**D10 campaign-aware server verifier (2026-11-07):** the optional native Duke verifier now handles campaign exits after each authoritative seat frame. It opens the requested next map from the same Atomic GRP, rebuilds the destination roster with `DukeSession::inherit_campaign_roster`, and continues checksumming the new map state. A failed content load or roster setup terminates verification with a warning instead of silently checking stale state. This is static pipeline integration only; no network match or end-to-end checksum validation was run.

**D9 Atomic SAV live globals refresh (2026-11-07):** `save_atomic_sav` now
reapplies the live versioned `AtomicSaveGlobals` retained in `.fsave` over the
import template before exporting current modeled session state. The writer
refreshes retained mirror/cloud/automap/sky/animated-sector/board tables and
Atomic settings/connection/score globals, with active player/session values
overriding overlapping fields afterward. This preserves changes made since
the template was imported, but it does not enable standalone save creation or
complete Atomic SAV round-trip validation. `cargo fmt --all`,
`git diff --check`, and `cargo check -q -p room-duke -p room-build` pass with
existing warnings; no tests or runtime/reference comparison were run.

**D9/D11 current-map metadata (2026-11-07):** `save_atomic_sav` now writes
the session's live `monsters_off` flag, and `inherit_campaign_roster` updates
the preserved Atomic board filename to the next `E#L#.MAP` along with the
campaign cursor. These close two stale-field paths for saves taken after map
transitions; standalone SAV creation and load-save-load verification remain
open. Static code review, formatting, diff check and targeted compilation
only; no gameplay or reference comparison.

**D9 Atomic SAV final tail (2026-11-07):** the full writer now emits
`gotpic[MAXTILES / 8]` and `playerswhenstarted` after the parallax scale, the
last two fields in Chocolate's `saveplayer` layout. Invalid/missing tile-seen
data is rejected rather than yielding a truncated file. This closes the
encoder's modeled tail omission, but standalone save construction and actual
Atomic load-save-load acceptance remain open. Static `menues.c::saveplayer`
layout review and targeted compilation only; no tests or runtime/reference
comparison were performed.

**D9 standalone Atomic SAV initialization (2026-11-07):** a fresh Room session
now seeds `SavHeader::blank_atomic` with version 119, current campaign/skill,
map dimensions, the full fixed world/script/runtime tables, and a
pointer-width-sized `ps[MAXPLAYERS]` blob before the usual live-state refresh
and encoder. This lets F2 export an Atomic sidecar without an imported save
template. The zero/default tables and custom-map board naming still need
static auditing against Chocolate's initialized save state; successful
Chocolate/Atomic load and load-save-load round-trip are explicitly unverified.
Static `global.c` version, `duke3d.h` ABI, and `menues.c::saveplayer` review,
formatting, diff check and targeted compilation only; no tests or runtime
comparison were run.

**D6/D11 scripted RPG tile resolution (2026-11-08):** the native RPG and
Devastator launch paths now resolve the projectile tile from the active CON
`RPG` label, falling back to Atomic tile 2605 when the script omits it. The
resolved tile initializes actor CON temps, is stored in both sprite metadata
paths, and is used by the projectile mover and RPG radius-damage classification
so a user CON tile override remains recognized through flight and explosion.
This is a targeted user-content compatibility slice, not full weapon parity.
`cargo fmt --all`, `git diff --check`, and `cargo check -q -p room-duke --lib`
pass (the crate emits existing warnings); no tests, gameplay, or dynamic
Chocolate/Room comparison were run.

**D6/D11 scripted handbomb tile resolution (2026-11-08):** handbomb throws
now spawn the tile selected by the active CON `HEAVYHBOMB` label, with Atomic
tile 26 as fallback. The native C actor dispatch and `move_pipebombs` mover
resolve the same label, so custom definitions keep their throw, bounce, remote
detonation and blast behavior together. This does not complete weapon cadence,
all remote-selection rules or multiplayer attribution. Static source-path
inspection, formatting, diff check and targeted compilation only; no tests,
gameplay, or dynamic Chocolate/Room comparison were run.

**D3 translucency table (2026-11-08):** `PALETTE.DAT` parsing now
retains its optional 65,536-byte palette-index blend table after the shade
rows. The native soft renderer composites face-, wall-, and floor-aligned
sprites through that table for `cstat` translucency bit 2, reversing the table
inputs for bit 9. Masked wall surfaces use wall `cstat` bit 7 and reverse bit 9;
opaque behavior is preserved if older palette data has no table. Translucent
sector planes remain unsupported. Static
Chocolate/Build table-layout and `cstat` review; formatting, diff check and
targeted library compilation only; no tests, screenshots or game comparisons.

**D3 sloped floor/ceiling rasterization (2026-11-08):** the classic soft
renderer now intersects floor and ceiling camera rays against the sector's
sloped plane using deterministic fixed-point iterations of the existing
Chocolate-compatible `getzsofslope`, then samples the plane ART at the
intersection. This covers the camera sector's plane fill; depth-tested plane
spans for other visible sectors, sloped portal clipping, and reference
screenshots remain open. Formatting, `cargo check -q -p room-build-engine
-p room-duke`, and `git diff --check` passed; no tests, gameplay or Chocolate
comparison were run.

**D3 sector plane orientation (2026-11-08):** `floorstat` and `ceilingstat`
now control plane ART's XY swap, X/Y flips and double-smoosh scale in the soft
renderer, based on Chocolate `ceilscan`/`florscan`. Relative-alignment bit 6
also rotates plane UVs into a normalized integer basis from the sector's first
wall. Texture scale now follows each ART axis dimension, and panning bytes add
1/256-tile offsets using fixed-point Q8 coordinates. Exact visual verification,
visibility shade and translucent planes remain open. Static source review,
formatting, targeted compilation and diff check only; no tests, gameplay or
dynamic Chocolate comparison were run.

**D8 title/finale animation clock (2026-11-08):** desktop UI animation
selection now uses accumulated monotonic elapsed milliseconds rather than
assuming every rendered frame took 16 ms. Title splash/logo elapsed time and
finale frame selection therefore follow their `TITLEDEF` frame rates at
variable refresh rates; entering a finale resets its animation origin. This
does not add missing ANM sound cues or implement the full front-end checklist.
Static UI path review, formatting, diff check and targeted compilation only;
no tests or gameplay comparison were run.

**D6 quick-kick cadence (2026-11-08):** Chocolate's `doincrements` reduces
`quick_kick` before the ordinary weapon state machine and invokes
`shoot(player, KNEE)` on the tic the counter becomes 8. Room now carries that
edge out of the movement/input phase and executes the same short-range knee
hitscan for the primary player and local co-op seats; a successful hit queues
the script-defined `KICK_HIT` sound. A hit also creates the zero-size `KNEE`
actor and `SMALLSMOKE` cue. Sprite targets pass through the shared
`checkhitsprite` prop transitions; wall targets pass through `checkhitwall`
after the bottom-wall peer adjustment, then tagged switch tiles go through the
existing switch handler. Ordinary actors and lotag `-1` exits are excluded
from switch dispatch. Exact material parity remains open. The counter pauses
for palette-1
frozen players, matching Chocolate's guard. This remains separate from the
normal knee weapon's `kickback_pic` cadence. Static `player.c::doincrements` and
`shoot(KNEE)` review, formatting, diff check and targeted compile only; no
tests, gameplay or Chocolate/Room comparison.

**D6 knee damage source (2026-11-08):** the normal knee weapon and `pkick`
now read the `KNEE` actor-header strength from the active CON, then add the
single `TRAND&7` roll and the Atomic `MAXPLAYERHEALTH/4` bonus while
`steroids_amount` is between 1 and 399. Missing scripts use the Atomic defaults
of 10 strength and 100 max health. Static `player.c::shoot(KNEE)` and Atomic
`USER.CON` review; no tests or runtime/reference comparison.

**D6 lethal player Freezer state (2026-11-08):** a lethal `FREEZEBLAST` hit on
an APLAYER now enters a timed frozen state instead of the ordinary death
transition. Room saves the body palette, applies palette 1, queues the active
CON's `SOMETHINGFROZE` sound when present, and holds the player at zero health
without consuming normal death or respawn handling during the timer. The timer
uses active CON `THAWTIME`, with Atomic's 138-tic value as fallback; thaw restores
the saved palette and gives the player 1 HP. A later non-Freezer hit exits this
state before normal damage handling. Static `GAME.CON` APLAYER/PFROZEN and
`PLAYER.C::shoot(KNEE)` source review plus formatting, diff check and targeted
crate compilation only; no tests or dynamic gameplay/reference comparison.
When a non-Freezer hit interrupts the frozen state, Room restores the saved
palette, emits 17 script-resolved `GLASSPIECES` shards using Chocolate's
angle/x-velocity/z-velocity RNG order, and queues `GLASS_BREAKING` before
continuing normal damage. Matching every Freezer transformation, interruption
edge case and visual-effect semantic remains open.

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


**D6 Atomic screen palettes (2026-11-08):** `LOOKUP.DAT` parsing retains its five trailing six-bit RGB palettes (water, night-vision/slime, title, 3D Realms, ending). The native view follows Chocolate `player.c::setpal`: heat vision selects the night-vision/slime palette; otherwise a FLOORSLIME ceiling selects it; otherwise sector `lotag == 2` selects water; all other views use the base palette. This is applied at final framebuffer conversion, so the world and weapon overlay share the tint and playsim remains read-only. Desktop TIP, loogie, help ART overlays and all desktop canvas commands (status bar, menu, text, fills, patches, debug/quote text and palette flashes) now use the active screen palette. Exact palette-dependent `palookup` shade tables remain open. Static source/layout review, formatting, diff check and targeted crate checks only; no tests, gameplay or Chocolate comparison.

**D6 APLAYER Shrinker/Grower response (2026-11-08):** static inspection of Atomic `GAME.CON`'s APLAYER actor showed that player hit handling needs distinct `SHRINKSPARK` and `GROWSPARK` responses even though player health damage is zero. The native `moveplayers` path now applies the respective palette flash and CON sound cues. Shrinker starts the script-defined `PSHRINKING` move, clears body collision, resets the player actor count, approaches repeat 8×9 during the first 32 tics (or skips to `SHRUNKCOUNT` while steroids are active), restores toward 42×36 from `SHRUNKCOUNT`, and restores cstat at `SHRUNKDONECOUNT`. During recovery, `ifgapzl 24` uses the live sloped ceiling/floor planes and reproduces the CON crush by setting player/body health to zero and queueing `SQUISHED`. Arbitrary APLAYER CON parsing, NPC-specific shrink/grow AI/action transitions and all resulting collision changes remain open. Static Chocolate `GAME.CON` and `player.c` behavior review, `cargo fmt -p room-duke`, `cargo check -q -p room-duke --lib`, and `git diff --check` only; no tests, gameplay, demo, network or Chocolate comparison was run.

**D6 `FRAMEEFFECT1` lifecycle (2026-11-08):** Chocolate `actors.c::moveactors` advances these spawned afterimages for eight actor tics, enables translucency after tics 2 and 4, copies the owner sprite's X/Y offsets, then removes the effect. Room's `STAT_MISC` mover applies that lifecycle to CON-spawned effects. Since APLAYER CON is not yet interpreted generally, Room also creates the same effect at the APLAYER pose on each `PSHRINKING` size tic. Arbitrary APLAYER CON particle branches and missing NPC shrink/grow state transitions remain open. Static Chocolate source/Atomic CON review, `cargo fmt -p room-duke`, `cargo check -q -p room-duke --lib`, and `git diff --check` only; no tests or dynamic game/reference comparison.

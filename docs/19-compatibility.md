# 19 — Compatibility

GZDoom references: `src/doomdef.h` + `src/d_main.cpp` (compat flags & `compatmode`),
`wadsrc/static/compatibility.txt`, `wadsrc/static/zscript/level_compatibility.zs`
(per-map fixes), `wadsrc/static/mapinfo/*.txt` (per-game defaults), `wadsrc/static/iwadinfo.txt`.

## 1. Supported games (IWAD families)

| Game | Content needed in `room.pk3` (all GPL/BSD-compatible data) | Milestone |
|---|---|---|
| Doom / Ultimate Doom / Doom II / Final Doom (TNT, Plutonia) / Freedoom (Phase 1, 2, FreeDM) / BFG & KEX editions (`DOOM.WAD`/`DOOM2.WAD` from the 2024 re-release incl. `extras.wad`-style ids) / Chex Quest 1 & 3 / Hacx / Harmony / Action Doom 2 / Rekkr | actor defs, MAPINFO, SNDINFO, LANGUAGE, status bar, menus, intermission scripts, IWADINFO entries | Doom core: M1–M6 |
| Heretic (Shareware, Registered, SotSR) | as above + Heretic actors, artifacts, Tome of Power, wind/scroll sectors, Heretic intermission | M8 |
| Hexen (+ Deathkings) | classes, hubs, polyobjects, ACS, mana, weapon pieces, puzzle items, Hexen-format specials, startup screen (S) | M8 |
| Strife (+ Veteran Edition data where applicable) | conversations, quests/log, accuracy/stamina upgrades, merchants, alarms, Strife status bar | M9 |

GZDoom's IWADINFO list is the reference for detection keys and `Autoname`s; include all entries it has (Doom 1 shareware, BFG variants, Freedoom variants, Chex variants, Hacx 1.2/2.0, Harmony, Square, Urban Brawl, Delaweare, Blasphemer, …).

## 2. Compatibility flags

Port both bitfields (`compatflags`, `compatflags2`) and every named flag CVar with identical bit positions (saved in configs/demos/MAPINFO by name). Checklist (non-exhaustive; the authoritative list is GZDoom's `compatflags` enums):

`compat_shorttex, compat_stairs, compat_limitpain, compat_silentpickup, compat_nopassover, compat_soundslots, compat_wallrun, compat_notossdrops, compat_useblocking, compat_nodoorlight, compat_ravenscroll, compat_soundtarget, compat_dehhealth, compat_trace, compat_dropoff, compat_boomscroll, compat_invisibility, compat_silentinstantfloors, compat_sectorsounds, compat_missileclip, compat_crossdropoff, compat_anybossdeath, compat_minotaur, compat_mushroom, compat_mbfmonstermove, compat_corpsegibs, compat_noblockfriends, compat_spritesort, compat_hitscan, compat_light, compat_polyobj, compat_maskedmidtex, compat_badangles, compat_floormove, compat_soundcutoff, compat_pointonline, compat_multiexit, compat_teleport, compat_pushwindow, compat_checkswitchrange, compat_explode1, compat_explode2, compat_railing, compat_scriptwait, compat_avoidhazards, compat_stayonlift, compat_nombf21, compat_voodoozombies, compat_fdteleport`.

Each flag's effect is localized at one or more code sites in the sim; tag every site with a comment `// COMPAT(compat_name)` so they can be audited (`grep COMPAT`).

### 2.1 `compatmode` presets

Port GZDoom's `compatmode` CVar presets (Default, Doom, Doom (strict), Boom, Boom (strict), MBF, MBF (strict), MBF21, MBF21 (strict), ZDoom 2.0.63) — each sets a fixed combination of `compatflags`/`compatflags2`. Values identical to GZDoom.

### 2.2 MAPINFO and UMAPINFO per-map flags

MAPINFO `compat_*` keys override per map; UMAPINFO maps get Boom/MBF21 defaults per the UMAPINFO spec; a WAD whose name/contents match known complevel lumps (`COMPLVL` lump: `vanilla`, `boom`, `mbf`, `mbf21`) selects the corresponding preset (GZDoom ≥ 4.11 honours `COMPLVL` — verify and mirror).

## 3. Per-map fix database

GZDoom keeps fixes for broken classic maps in `compatibility.txt` (flag sets keyed by map MD5) and `level_compatibility.zs` (ZScript code applying geometry/thing/special edits keyed by MD5). Room cannot run the ZScript, so it ships the same knowledge as data:

```
room.pk3/compat/
  flags.txt          # same syntax as compatibility.txt: MD5 list { flag; flag; … }
  levelfixes.ron     # MD5 → list of operations
```

Operations (mirror the helper functions used by `level_compatibility.zs`): `ClearSectorTags, AddSectorTag, OffsetSectorPlane, SetSectorSpecial, SetSectorTexture, SetSectorLight, ClearLineIDs, AddLineID, SetLineSpecial, ClearLineSpecial, SetLineFlags(set, clear), SetLineActivation, SetLineSectorRef, SetWallTexture, SetWallTextureID, SetWallYScale, SetThingSkills, SetThingFlags, SetThingXY, SetThingZ, SetThingSpecial, SetThingArgument, SetThingStringArgument, SetVertex, FlipLineSideRefs, FlipLine, SetSectorFloorPlane …`. Convert the ZScript file to RON with a one-off script (manually reviewed); unit test applies each fix to a synthetic map of the right shape.

Applied in `06 §2` step 7, before node building. Room adds `sv_nomapfixes` (default off) to disable them for testing; take the exact operation list from the current `level_compatibility.zs`.

## 4. Strict vanilla mode (Room addition)

GZDoom does not sync vanilla demos. Room offers `compatmode vanilla-demo` used automatically for vanilla/Boom/MBF demo playback and selectable for play:

- Fixed-point (`i32` 16.16) movement/collision paths ported from Doom/Boom/MBF source (`p_map.c`, `p_maputl.c`, `p_mobj.c`, `p_enemy.c`, `p_user.c`) behind a `SimMath` strategy switch — implemented as a separate module `room_sim::vanilla` sharing data structures but with its own physics functions. Do **not** sprinkle `if vanilla` checks through the double-precision code.
- Vanilla RNG table, `P_Random` call order identical to the target engine (Doom 1.9 / Boom 2.02 / MBF / PrBoom+ complevels 2, 9, 11, 21).
- Original nodes (`game_nodes`) for `R_PointInSubsector`, original blockmap including its bugs (blockmap 128-unit origin, "blockmap bug" for large things is vanilla behaviour), original reject.
- Vanilla limits that affect gameplay emulated (e.g. lost soul limit of 21 in `A_PainAttack` — `compat_limitpain`; spechit overflow emulation à la PrBoom+ — S).
- Acceptance: the curated demo set in `20 §3` stays in sync.

## 5. Known intentional deviations from GZDoom

Keep this list in this document; every deviation needs an ADR.

1. No ZScript (see `11 §3`).
2. Sim floating-point math is deterministic across platforms (GZDoom relies on x86-64 SSE2 determinism only).
3. Render-side automap `ML_MAPPED` marking is not a sim write (`16 §5`).
4. Particles excluded from saves and checksums (same as GZDoom saves; checksum is new).
5. Dynamic lights are animated in the renderer, not ticked in the sim (`13 §5.2`).
6. Vanilla demo strict mode exists (§4).
7. Config format is TOML (`15 §7`), with GZDoom ini import.

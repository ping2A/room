# 33 — Running Brutal Doom (v22 test 6) in Room

**Audience:** an external coding agent working in the Room repo. Read
`AGENTS.md`, `docs/02-conventions.md` (§Determinism), `docs/03-vfs.md`,
`docs/05-text-lumps.md`, `docs/08-actors.md`, `docs/10-acs.md`,
`docs/11-legacy-mod-formats.md` (DECORATE, ZScript policy) and
`docs/12-wasm-scripting.md` first. Related: `docs/30-weapons-audit.md`
(weapon logic), `docs/31-mod-api-extensions.md` (alt fire, generated lumps,
`lump_read`), **`docs/34-zscript-vm.md` (ZScript compiler + VM — BD's
ZScript runs on it)**.
**Status:** **BD0**–**BD9** done in `docs/21` (2026-11-01) with listed deferrals
(10-min soak, hash-MES/Doom2 IWAD, GZDoom screenshots, web streamed pk3, 15-min
net verifier). 15m ZS+BD loop may stop — acceptance closed or deferred.
**Target content:** `brutal22test6.pk3` (Brutal Doom v22 test 6, 159.5 MB).
Never commit it (or any extracted file) to the repo; tests read it from
`ROOM_BRUTALDOOM_PK3` and skip when unset.

---

## 0. TL;DR

Brutal Doom (BD) is the most demanding classic GZDoom gameplay mod: 3,901
DECORATE actors, 172 `.dec` files, BCS/GDCC-compiled ACS libraries with
functions and arrays, 2 small ZScript files, KEYCONF/CVARINFO/MENUDEF/SBARINFO,
dynamic lights, models, voxels, ~1,100 OGG sounds and ~20,000 PNG sprites.

Measured with Room's own parser and loaders (§2):

| Area | Result today |
|---|---|
| DECORATE parse (whole file) | **fails** at the first error |
| DECORATE parse (per actor) | **2,746 / 3,797 parse (72 %)**; 1,051 fail on 8 missing syntax features |
| Base classes | **17 native parents missing** (`Inventory`, `CustomInventory`, `Ammo`, `FastProjectile`, `PowerupGiver`, …) → class registry can't be built |
| Actor flags | 128 of the 159 distinct flags BD uses are **silently ignored** (3,641 of 5,738 flag lines) |
| Action functions | `A_JumpIf` (1,634 uses), `A_ZoomFactor`, `A_SetCrosshair` and 16 more **missing**; dispatch is **case-sensitive**, so `A_Takeinventory` (846), `A_WEAPONREADY` (471), … are silently skipped; **line specials as actions** (`ACS_NamedExecuteAlways` 984, `ACS_Execute` 554, …) are not supported |
| Expressions | `CheckClass` (205), `CallACS` (93), `ACS_NamedExecuteWithResult` (113), `waterlevel` (453), `x/y/z/floorz`, `scalex/scaley`, `sin/cos`, `AAPTR_*` missing; `==` / `!=` don't parse |
| ACS | the 3 libraries **load**, but Room ignores the `FUNC/FNAM/ARAY/AINI/SARY/FARY/MINI/MEXP/ALIB/ASTR/ATAG/SFLG/SVCT` chunks BD uses, and PCDs for function calls, arrays, HUD messages and strings are not implemented → BD's ACS cannot run |
| ZScript | 3 files (`zscript.zc` + 2 includes, 8.4 KB); event handlers + Zandronum-compat static functions called from ACS via `ScriptCall` (Room passes ints only today). **Room parses BD's ZScript but does not yet execute it correctly (audit `docs/34` §14); it will through the ZScript VM (`docs/34` ZS1b–ZS4b)** — the pk3 is used as is, no compatibility mod |
| Weapons | needs `WRF_*` flags, `AltFire`/`Reload`/`Zoom`/`User1–4` states and buttons, dual ammo types, 10 weapon slots, `A_ZoomFactor` |

Everything else (OGG, PNG, MD2/MD3, voxels, GLDEFS lights, DECALDEF, KEYCONF,
CVARINFO, MENUDEF, SBARINFO, MAPINFO skills, LANGUAGE) has parsers in Room but
was **not** exercised with BD's data; the harness in §4 must verify them.

Plan: build a **mod compatibility harness** first (BD0), then fix the DECORATE
syntax (BD1), base classes + flags + properties (BD2), actions + expressions
(BD3), weapons/input (BD4), ACS (BD5), BD's ZScript on the new ZScript VM (BD6, `docs/34`
ZS0–ZS4),
remaining lumps/UI (BD7), performance (BD8) and netplay (BD9).

---

## 1. What Brutal Doom is (and what the pk3 contains)

From the ModDB page (https://www.moddb.com/mods/brutal-doom): gore overhaul by
Sergeant_Mark_IV, in development since 2010 (Cacoward, ModDB 2012 Editor's
Choice). v21 Gold is the stable release; v22 test/dev builds add content and a
Horde Mode. The dev build "Requires Zandronum 3.1 or more modern" and
"probably any version of GZDoom can run this". Features: extra blood, gibs,
dismemberment, headshots, fire/explosion effects, blood painting walls and
floors, kickable limbs/heads, destructible scenery, throwable barrels,
chainsaw cutting enemies in half, stealth kills, Berserk executions, crawling
enemies, human shields, kick on Q, reworked weapons (reload, dual wield,
grenades, alt fire), smarter enemies, Horde Mode, many toggles; works with
"almost every megawad".

Inventory of `brutal22test6.pk3` (22,264 files, 223 MB unpacked):

| Content | Count / notes |
|---|---|
| DECORATE | `DECORATE.txt` + 172 `.dec` files under `actors/` (Gore, Weapons, Enemies, PlayerClasses, MapEnhancementSystem, VEHICLES, DECORATIONS, MISC, MeatShields); **3,901 actors**, 143,857 lines after `#include` expansion; 168 `replaces` |
| ZScript | `zscript.zc` (`version "3.2.3"`) + `zscript/BD_WCS.zc` (350 lines: `ZC_IntStrMap`, `ZC_TMapHack` EventHandler, `ZandroCompat` statics incl. lump reading) + `zscript/DamageHandler.zc` (85 lines: `Script_DamageMult` EventHandler) |
| ACS | `LOADACS`: `BD_Main`, `BD_Hash`. Objects: `acs/BD_Main.o` (ACSe, 652 scripts, 540 named, compiled with **BCC/BCS**), `acs/BD_Hash.o` (ACSE, compiled with **GDCC** from C), `acs/zancmpat.o` (library); 47 `.acs` sources in `src/` |
| Text lumps | `MAPINFO.txt` (clearskills, 7+ skills with `ReplaceActor`, `SpawnFilter`, `mustconfirm`; `gameinfo { AddEventHandlers = "ZC_TMapHack", "Script_DamageMult"; ForceDamageEventScripts; DimColor }`), `KEYCONF.txt` (5 player classes, `weaponsection`, `setslot 0–9`, `addkeysection`/`addmenukey`/`alias`/`defaultbind` with `puke`/`pukename`), `CVARINFO.txt` (122 server/user cvars), `MENUDEF.TXT` (1,196 lines), `SBARINFO.BD`, `GLDEFS.txt` + `gldefs.flashlight` + `*.bm` brightmap includes, `MODELDEF` ×6 (`modeldef.*.txt`), `DECALDEF` ×3, `SNDINFO` ×7 (`SNDINFO.Gore`, `.Monsters`, `.Players`, `.Quips`, `.Terrain`, `.Vehicles`, `.BrutalFriends`), `LANGUAGE` ×6 (enu, fr, ita, ptb, pt, rus), `TERRAIN` ×2, `TEXTURES.*`, `ANIMDEFS`, `Fontdefs`, `XHAIRS`, `gameinfo.txt` |
| Assets | 20,323 PNG, 1,122 OGG, 9 WAV, 118 LMP, 24 MD3, 3 MD2, voxels, 9 MID + 5 MUS |
| Maps | 11 WADs: `BDDMMaps.wad`, `BDHordeModeMaps.wad`, `BDPSXMaps.wad` at the root, more in `maps/` |

Lump names come from file names **with the extension stripped**, and every
lump of the same name is loaded (`SNDINFO.Gore` and `SNDINFO.Monsters` are both
`SNDINFO`; `SBARINFO.BD` is `SBARINFO`) — GZDoom semantics Room's VFS must
follow.

---

## 2. Method (reproducible)

1. Copied the current tree (`crates`, `wadsrc`, `Cargo.*`) to a scratch
   directory and built three throw-away examples (not committed):
   - `room-sim/examples/bd_check.rs`: expands `DECORATE.txt` with
     `room_decorate::expand_includes` (case-insensitive path lookup from the
     archive root), prepends `wadsrc/room/actors/doom.txt`, runs
     `parse_decorate` and `ClassRegistry::from_decorate`.
   - `room-sim/examples/deco_many.rs`: parses each actor separately (actors
     cut by brace matching after comment stripping) to count failures by
     cause.
   - `room-acs/examples/acs_info.rs`: `room_acs::load_behavior` on the three
     `.o` files.
2. Inventoried action names, flags, properties, expression identifiers and
   ACS calls with scripts over the expanded DECORATE and `src/*.acs`; compared
   with Room's dispatch tables (`room-sim/src/action.rs` `call_action_args`,
   `think_player.rs`, `class.rs` `apply_flag` / property matcher,
   `deco_eval.rs`, `room-acs/src/pcd.rs`).

BD0 (§6) turns this into a permanent tool: `room-cli mod check`.

---

## 3. Gap analysis and fixes

### 3.1 VFS / archive loading (`crates/room-vfs`)
Not measured; verify with the harness:
- Lump name = file stem (strip extension), case-insensitive; **all**
  same-name lumps are loaded in archive order for additive lumps (SNDINFO,
  DECALDEF, MODELDEF, LANGUAGE, GLDEFS, TERRAIN, …) per `docs/05` §3.
- Embedded WADs at the archive root are mounted (`archive.rs` has a
  nested-WAD check); `maps/*.wad` are map WADs (`maps/RN01.wad` → map `RN01`).
- Namespaces from directories: `sprites/**` (BD nests sub-folders:
  `sprites/Gore…`), `graphics/`, `patches/`, `textures/`, `brightmaps/`,
  `sounds/**`, `music/`, `models/`, `voxels/`, `acs/`.
- Size: 159.5 MB compressed. Desktop: mmap/stream the zip, don't inflate all
  entries at load. Web: download once, cache in IndexedDB/OPFS, inflate entries
  on demand (`docs/18`).

### 3.2 DECORATE syntax (`crates/room-decorate/src/parse.rs`, `room-sc`)
1,051 of 3,797 actors fail. Causes (count = failing actors whose first error
is this; fixing one may reveal the next):

| # | Missing syntax | Actors | Example | Fix |
|---|---|---|---|---|
| D1 | State keywords with arguments: `Light("NAME")`, `Offset(x, y)` after the tics | 713 | `TNT1 A 1 Light("FlashlightLight1")` | Parse the GZDoom state-modifier list: `Bright`, `Fast`, `Slow`, `NoDelay`, `CanRaise`, `Light(name[, name…])`, `Offset(x, y)`. Store lights on the state (attach the GLDEFS light while the state is active) and the offset for psprites. |
| D2 | Parenthesised / expression **property** values | 148 | `Damage (random (30, 40))` | `Damage (expr)` = per-hit expression (GZDoom evaluates it each impact); general rule: a property value may be a parenthesised expression. Store as `DecoExpr`. |
| D3 | `"####"` / `"#"` keep-current sprite / frame | 58 | `"####" "#" 0 A_Jump(128,2)` | Accept quoted sprite and frame; `####` / `#` mean "keep the current". |
| D4 | `==` and `!=` in action arguments | 42 | `A_JumpIf(momz == 0, "Death")` | Add to the expression grammar (`room-decorate/src/expr.rs` already parses `<`, `>`); also `&&`, `||`, `!`, ternary `?:`, bit ops — check the full GZDoom operator table. |
| D5 | Sprite names starting with digits; actor names starting with digits | ~70 | `2IFG A 0 …`, `13EI A 35`, `99ST ABCDEFGH 2`, `ACTOR 50CaseSpawn` | In states, read the sprite as a raw 4-char token even if it starts with a digit (GZDoom scans `[A-Za-z0-9_#-]{4}`); actor names are identifiers that may start with a digit in DECORATE. |
| D6 | `DropItem "Class" [probability [amount]]` | ~20 | `DropItem "HealthPlusNoCount" 128` | Parse the optional numbers; probability 0–255 (255 = always), `amount`. Several `DropItem` lines accumulate. |
| D7 | `|` flag combinations in property values | 5 | `Activation THINGSPEC_Activate \| THINGSPEC_NODEATHSPECIAL` | Constant expressions with named constants (`THINGSPEC_*`, `SXF_*`, `WRF_*`, `CMF_*`, …). |
| D8 | Misc (one each): `Fail:` used as a label in `CustomInventory`, stray `{` in an actor body, `Axe/Hit` (a sound name parsed as actor header because of a preceding syntax error), `DualFlameCannon` numeric literal | ~10 | — | Fix as found; the harness lists them with line numbers. |

Also: the parser stops at the first error for the whole file. GZDoom reports
errors with file/line and aborts loading the mod; Room should **report all
errors with original file and line** (keep a source map through
`#include` expansion — today errors have no position or a position in the
expanded text) and, for compatibility testing, offer a lenient mode that skips
broken actors (`decorate_policy = strict | lenient`).

### 3.3 Base classes and inheritance (`wadsrc/room/actors`, `room-sim/src/class.rs`)
BD derives from native GZDoom classes that don't exist in Room's built-in
DECORATE, so `ClassRegistry::from_decorate` fails ("unknown parent"):

**Runtime follow-up (2026-11-05):** the BD vehicle path was not covered by the
BD0–BD9 loading and map smoke checks. BIKERACE's `Bike : Tank` uses `+USESPECIAL`
with an `Active` state, while Room previously only activated `Use`. Room now
provides the `Tank` compatibility base and falls back to `Active` when a usable
actor has no `Use` state. Keep the built-in `Tank` declaration after
`SwitchableDecoration`: Brutal Doom replaces `Tank` with a subclass of
`SwitchableDecoration`, and the class registry preserves the first definition's
position when applying later replacements. Putting the stub earlier makes that
replacement fail class loading. A headless load of the real BIKERACE WAD now
creates all ten `Bike` actors from thing type 7265 with their `BIKP` spawn
sprite. This gets the bike's activation sequence running;
`PowerupGiver` now resolves `Powerup.Type` to its `PowerMorph` class, the
player class changes while preserving the actor handle, morph weapons are
selected, and `UnMorphActor` restores the original class and weapon. Granted
`CustomInventory` items now run their `Pickup` state with the owner as target
and ACS activator, which allows `BDEnterTank` / `BDLeaveTank` to execute. The
ACS MORPHACTOR opcodes are no longer unconditional failures. Duration/style and
flash effects are not yet implemented, and the complete BIKERACE flow still
needs an in-game run with Doom II content to validate movement, collision and
exit behavior.

| Missing parent | BD children | Behaviour needed |
|---|---|---|
| `Inventory` | 42 (+ `bd_Token` with 355 subclasses) | Generic inventory: `Amount`, `MaxAmount`, `InterHubAmount`, `Icon`, `PickupMessage/Sound`, flags `INVENTORY.*`; **tokens** (undroppable, uncounted items used as state flags with `A_GiveInventory`/`A_JumpIfInventory`) |
| `CustomInventory` | 38 | `Pickup` / `Use` / `Drop` state sequences executed **on the toucher/user**; sequence ending in `Stop` = success (item consumed), `Fail` = rejected; `A_` actions run with the player as `self` |
| `Ammo` | 49 | `Ammo.BackpackAmount`, `Ammo.BackpackMaxAmount`, `Ammo.DropAmount`; subclass of an ammo type counts as that type (`GetParentAmmo`) |
| `FastProjectile` | 27 | Projectile moved in sub-steps (no tunnelling), `MissileType`/`MissileHeight` trail spawning |
| `PowerupGiver`, `Powerup`, `PowerProtection`, `PowerTimeFreezer`, `PowerMorph` | 18 + 5 + 1 + 1 + 8 | `Powerup.Type`, `.Duration`, `.Color`, `.Colormap`; `PowerMorph.PlayerClass` (morph into another player class) |
| `Health` | 8 | `Health.LowMessage`, pickup to `MaxAmount` |
| `BasicArmorPickup`, `BasicArmorBonus` | 2 + 4 | `Armor.SaveAmount`, `.SavePercent`, `.MaxSaveAmount`, `.MaxBonus`, `.MaxBonusMax` |
| `BackpackItem` | 1 | doubles ammo capacity (`Ammo.BackpackMaxAmount`) |
| `DoomWeapon` (and `Weapon` base, see `docs/30` W16) | 7 | weapon defaults + `LightDone` |
| `SwitchableDecoration` | 6 | `Active` / `Inactive` states via `Activation` |
| `RandomSpawner` | 4 | picks one `DropItem` by weight, replaces itself |
| `GibbedMarine` | 2 | Doom decoration base |

Add these to `wadsrc/room/actors/` as built-in DECORATE (where behaviour is
data) plus native behaviour in `room-sim` (`inventory.rs`, `pickup.rs`,
powerups, morph). This is also the prerequisite `docs/31` MA2 needs.

### 3.4 Flags (`room-sim/src/class.rs` `apply_flag`)
`apply_flag` knows ~45 names; anything else hits `_ => return` and is **ignored
without a warning**. BD uses 159 distinct flags; 128 are unknown (3,641 of
5,738 flag lines). Most used unknown flags:

`CLIENTSIDEONLY` 317, `THRUACTORS` 297, `FORCEXYBILLBOARD` 271, `NOCLIP` 243,
`DONTSPLASH` 228, `NOTELEPORT` 203, `MOVEWITHSECTOR` 127, `THRUGHOST` 114,
`NOINTERACTION` 100, `EXPLODEONWATER` 79, `NODAMAGETHRUST` 62,
`FORCEYBILLBOARD` 57, `RANDOMIZE` 57, `FORCERADIUSDMG` 54, `GHOST` 52,
`BLOODLESSIMPACT` 52, `NOEXTREMEDEATH` 51, `BLOODSPLATTER` 49, `MISSILEMORE` 49,
`LOOKALLAROUND` 48, `THRUSPECIES` 46, `INVENTORY.UNDROPPABLE` 40,
`INVENTORY.ALWAYSPICKUP` 39, `USEKILLSCRIPTS` 38, `ACTIVATEMCROSS` 37,
`MISSILEEVENMORE` 34, `NOBLOODDECALS` 33, `MTHRUSPECIES` 32, `SKYEXPLODE` 31,
`NOPAIN` 31, `CANNOTPUSH` 31, `WEAPON.CHEATNOTWEAPON` 29, `DONTHURTSPECIES` 28,
`WEAPON.NOAUTOFIRE` 27, `NOTAUTOAIMED` 26, `EXTREMEDEATH` 24, `DEHEXPLOSION` 22,
`ISMONSTER` 22, `NOBLOCKMONST` 21, `BOUNCEONACTORS` 19,
`INVENTORY.AUTOACTIVATE` 19, `FASTER` 18, `SLIDESONWALLS` 17, `ACTIVATEIMPACT` 16,
`WINDTHRUST` 16, …

Fix: move to the full GZDoom flag table (flag name → field/bit, including
qualified `INVENTORY.*` / `WEAPON.*` / `PLAYERPAWN.*` names and the
deprecated aliases), grow the actor flag storage (one `u64` is full; GZDoom has
8 flag words), implement the behaviour of each flag in the playsim, and **warn
once per unknown flag name** at load. Priority: the movement/collision ones
(`NOINTERACTION`, `NOCLIP`, `THRUACTORS`, `THRUGHOST`, `THRUSPECIES`,
`NOTELEPORT`, `MOVEWITHSECTOR`), damage/blood ones (`NOPAIN`, `NODAMAGETHRUST`,
`FORCERADIUSDMG`, `BLOODSPLATTER`, `BLOODLESSIMPACT`, `EXTREMEDEATH`,
`NOEXTREMEDEATH`, `NOBLOODDECALS`), AI ones (`LOOKALLAROUND`, `ISMONSTER`,
`MISSILEMORE`, `MISSILEEVENMORE`, `NOINFIGHTING`, `DONTHURTSPECIES`), render ones
(`FORCEXYBILLBOARD`, `FORCEYBILLBOARD`, `RANDOMIZE`), inventory/weapon ones.
`CLIENTSIDEONLY` is Zandronum-only: in Room every peer runs the full sim, so
treat it as a normal actor (optionally a hint for the render-only gore mode,
§3.10).

### 3.5 Properties
Unknown or unsupported property names in BD (count of lines): `Decal` 385,
`Game` 193, `Projectile` (combo) 118, `Inventory.Icon` 61, `SpawnID` 59,
`Ammo.BackpackAmount`/`BackpackMaxAmount` 49, `Weapon.AmmoType1/2`,
`AmmoUse1/2`, `AmmoGive1/2` (~115), `Powerup.Duration/Type/Color/Colormap`
(~50), `ProjectilePassHeight` 22, `WallBounceFactor` 19, `BounceCount` 14,
`BounceType` 9, `Player.*` (`ViewHeight`, `AttackZOffset`, `MorphWeapon`,
`DamageScreenColor`, `JumpZ`, `DisplayName`, `Portrait`, `Face`, `ColorRange`,
`CrouchSprite`, `RunHealth`, `SideMove`), `Armor.*`, `Weapon.BobRangeX/Y`,
`BobSpeed`, `BobStyle`, `Weapon.Kickback`, `MeleeSound`, `MeleeDamage`,
`ExplosionRadius`, `ExplosionDamage`, `MissileType`, `StencilColor`,
`VisibleToPlayerClass`, `Inventory.ForbiddenTo`, `Inventory.UseSound`,
`ClearFlags`, `PushFactor`, `Health.LowMessage`, `PowerMorph.PlayerClass`.
Implement them with the base classes (§3.3) and **warn on unknown
properties** (today they are silently dropped).

### 3.6 Action functions (`room-sim/src/action.rs`, `think_player.rs`)
1. **Case-insensitive dispatch.** DECORATE is case-insensitive, but
   `call_action_args` matches exact strings. Silently skipped variants in BD:
   `A_Takeinventory` 846, `A_WEAPONREADY` 471, `A_Giveinventory` 331,
   `A_ChangeFLag` 243, `A_SpawnItemEX` 147, `A_SPawnItemEx` 112,
   `A_Facetarget` 94, `A_Playsound` 80, `A_SetANgle` 65, `A_playsound` 47,
   `A_Custommissile` 34, `A_CheckSIght` 33, `A_PlaySOund` 31, `A_Stopsound` 24,
   `A_WEaponReady`, … Fix: canonicalise action names **at state-table build
   time** (lower-case lookup → canonical name or an action enum), so the hot
   path never compares strings.
2. **Report unknown actions at load** (with class, state, file:line) instead
   of `_ => ActionResult::Continue`.
3. **Missing actions** (BD use counts): `A_JumpIf` **1,634**, `A_ZoomFactor`
   270, `A_SetCrosshair` 100, `A_PlaySoundEx` 60, `A_JumpIfNoAmmo` 56,
   `A_FastChase` 28, `A_SetInvulnerable` / `A_UnSetInvulnerable` 26,
   `A_ClearReFire` 14, `A_SetFloorClip` / `A_UnSetFloorClip` 10,
   `A_BarrelDestroy` 7, `A_MeleeAttack` 5, `A_SPosAttackUseAtkSound` 2,
   `A_PlayWeaponSound` 2, `A_StopSoundEx`, `A_NoGravity`, `A_Gravity`,
   `A_JumpIfInTargetLOS`.
4. **Line specials as state actions.** GZDoom lets any action special be
   called from a state with its name: BD uses `ACS_NamedExecuteAlways` 984,
   `ACS_Execute` 554, `ACS_ExecuteAlways` 164, `ACS_NamedExecute` 9, plus
   `Thing_*`, `Radius_Quake`, etc. Map the special name → number
   (`room-sim/src/specials.rs` tables) and call `special_execute` with the
   actor as activator; `ACS_NamedExecute*` resolve the script by name across
   loaded libraries.
5. **Heavily used existing actions must match GZDoom exactly**: verify
   `A_CustomMissile` (10,830 uses: flags `CMF_*`, pitch, aim modes),
   `A_JumpIfInventory` (6,100), `A_SpawnItemEx` (4,397+: all `SXF_*` flags,
   chance, TID), `A_SpawnItem`, `A_ChangeFlag` (string flag names → the full
   table from §3.4), `A_Warp` (all `WARPF_*`), `A_Recoil`, `A_SetPitch`,
   `A_GiveToTarget`, `A_JumpIfInTargetInventory`, `A_KillChildren`,
   `A_RadiusGive`, `A_QueueCorpse` (corpse queue limit), `A_CheckFloor`,
   `A_CheckCeiling`, `A_SetScale` (negative scale = mirrored), `A_FadeOut`.
   Write per-action unit tests from the GZDoom source (paths in
   `docs/appendix-b-gzdoom-source-map.md`).
6. **Zero-tic chains are recursive** (`World::set_state` calls itself for each
   0-tic state). BD has chains of 15–50 zero-tic spawning states and 0-tic
   loops guarded by jumps; an accidental 0-tic loop overflows the stack.
   Make it iterative with GZDoom's guard (abort the chain after 1,000 states
   with a console warning and leave the actor in the last state).

### 3.7 Expressions (`room-sim/src/deco_eval.rs`, `room-decorate/src/expr.rs`)
`eval_call` supports only `random`, `frandom`, `countinv`; identifiers
`health mass speed radius height angle pitch velx/momx vely/momy velz/momz tid
crouch alpha args user_*`. BD needs (counts inside `A_JumpIf` alone):
`waterlevel` 453, `CheckClass(class, AAPTR_*, match_superclass)` 204,
`x` 111 / `y` 112 / `z` 61, `floorz` 34, `scalex`/`scaley` (in `A_SetScale`),
`CallACS(name, args…)` 93, `ACS_NamedExecuteWithResult` 113, `sin`/`cos` 36,
constants `true`/`false`, `AAPTR_*`, plus `random2`, `abs`, `sqrt` for safety.
Also the operators from D4. Evaluation currently re-parses argument strings
on every call (`eval_action_args`); pre-compile expressions at load into a
small bytecode / AST per state.

### 3.8 Weapons and input (see also `docs/30`, `docs/31` MA4)
- `A_WeaponReady(flags)`: BD passes `WRF_NOFIRE` 758, `WRF_ALLOWZOOM` 702,
  `WRF_NOBOB` 664, `WRF_ALLOWRELOAD` 197, `WRF_NOSWITCH` 94,
  `WRF_NOSECONDARY` 93, `WRF_NOPRIMARY` 13, `WRF_DISABLESWITCH` 1. Room
  ignores the argument. Implement all of them (bob freeze, allowed buttons,
  switch lock).
- State labels: `AltFire` 35, `AltHold` 3, `Reload` 29, `Zoom` 1,
  `User1`–`User4` (2/17/12/0), `Hold` 8, `Flash` 5. Buttons: `ALTATTACK`,
  `RELOAD`, `ZOOM`, `USER1`–`USER4` in `TicCmd.buttons` (free bits; the wire
  field is already `u32` → no protocol change). BD's KEYCONF binds kick to
  `+zoom` (Q), reload to `+reload` (R), grenade `+user1` (G), unload `+user2`,
  dual-wield `+user3` (X), ammo drop `+user4`.
- `Weapon.AmmoType1/2`, `AmmoUse1/2`, `AmmoGive1/2` (primary/secondary ammo;
  BD shows magazine + reserve via SBARINFO `ammo1`/`ammo2`), `A_JumpIfNoAmmo`,
  `A_ZoomFactor` (FOV change, render-only per player), `A_SetCrosshair`.
- 10 slots (`setslot 0`–`9`) with many weapons per slot (slot cycling from
  `docs/30` W9).
- `A_FireCustomMissile` / `A_FireBullets` ammo use per `docs/30` N2.

### 3.9 ACS (`crates/room-acs`)
The three objects load (`BD_Main.o`: `AcseCompressed`, 652 scripts, 540 named,
imports `zancmpat`; `BD_Hash.o`: `Acse`, 4 scripts; `zancmpat.o`: library), but
`load.rs` reads only `LOAD`, `SNAM`, `SPTR`, `STRE`, `STRL`. Chunks present in
BD and ignored: **`FUNC`, `FNAM`** (functions), **`ARAY`, `AINI`, `SARY`,
`FARY`** (map/script/function arrays), **`MINI`** (map var initialisers),
**`MEXP`, `ALIB`** (library exports, cross-library linking), **`ASTR`, `ATAG`**
(string tagging), **`SFLG`, `SVCT`** (script flags, script variable counts).
`pcd.rs` defines ~180 PCDs; BD's compiled code needs the rest of GZDoom's list:
`CALL`, `CALLDISCARD`, `RETURNVOID`, `RETURNVAL`, `CALLSTACK`/function
pointers, map/world/global/script/function **array** ops, `PUSHFUNCTION`,
string ops (`STRCPY…`, `PRINTMAPCHARRAY`, `SAVESTRING` for `StrParam`), HUD
messages (`SETHUDSIZE`, `ENDHUDMESSAGE`, `ENDHUDMESSAGEBOLD`,
`OPTHUDMESSAGE`, `MOREHUDMESSAGE`, `SETHUDCLIPRECT`, `SETHUDWRAPWIDTH`),
`PLAYERINPUT`-family, etc. Port the full PCD table and chunk loader from
GZDoom `src/playsim/p_acs.cpp`.

ACS functions BD calls (from the sources) that have no constant in `pcd.rs`
(may need PCD + ACSF + host work): `HudMessage`/`SetHudSize` (123 uses),
`GetPlayerInput` 19, `SetPlayerProperty` 18, `ConsolePlayerNumber` 15,
`IsNetworkGame` 10, `SetMugShotState` 6, `SetActivatorToPlayer` 5,
`SpawnProjectile` 5, `Get/SetUserVariable` 6, `StrParam`/`StrFormat`,
`ConsoleCommand`, `NamedSendNetworkString`, `GetLanguageKey`. Zandronum-only
functions (`ConsoleCommand`, `NamedSendNetworkString`, `PlayerIsLoggedIn`,
`GetPlayerAccountName`) should return safe defaults; BD guards most of them
with `IsZandronum()` checks.

Also: console `puke <n> [args]` and `pukename <name> [always] [args]` (BD's
KEYCONF aliases: flashlight, dodge roll, taunt, underbarrel grenade) must
send a **network-safe** request (a TicCmd-carried script request, not a
direct call) so netplay stays in sync.

### 3.10 ZScript — runs natively on the Room ZScript VM (`docs/34`)
Room will compile and execute ZScript itself (decision
`docs/decisions/0005-zscript-vm.md`, spec `docs/34-zscript-vm.md`), so the
pk3 is loaded **unmodified** — no per-mod compatibility module.

What BD's ZScript does, and therefore what the VM must cover first
(`docs/34` §6 lists the exact natives):
- `Script_DamageMult` (EventHandler, `DamageHandler.zc`): on `PlayerEntered`
  and `WorldThingSpawned` with `skill >= 6`, `GiveInventory("Damage_Control")`
  to the player and to every `bISMONSTER` actor (`gamestate`, `players[]`).
- `ZC_IntStrMap` (plain class: `Array<int>`, `Array<string>`, `Insert`,
  `Delete`, `Size`, `String.Format`, `ToInt`) and `ZC_TMapHack`
  (EventHandler used as a per-session store: `EventHandler.Find`, `new`, casts).
- `ZandroCompat` static `play` functions called from ACS through `ScriptCall`:
  `LumpOpen/LumpRead/LumpReadString/LumpClose/LumpGetInfoInt/LumpGetInfoStr`
  (`Wads.CheckNumForFullName`, `CheckNumForName`, `FindLump`, `ReadLump`,
  `String.Mid/Length/CharCodeAt`), used by `BD_Hash` to fingerprint the map
  for the Map Enhancement System; `IsClient`, `ConsolePlayerNumber`,
  `PlayerInGame`, `GetPlayerTID`/`SetPlayerTID` (`ChangeTID`), `GetGender`
  (`PlayerInfo.GetGender`, `CVar.GetCVar(name, player).GetInt()`,
  `mo.SoundClass`).
- MAPINFO `gameinfo { AddEventHandlers = "ZC_TMapHack", "Script_DamageMult";
  ForceDamageEventScripts = true }` registers both handlers.

**Status (audit 2026-11-02, details in `docs/34` §14).** The three real BD
files (8.4 KB) compile in Room with 0 errors, but executing them on
`room-zvm` gives wrong results. `for`, `while`, `switch`, compound
assignment and ternaries are not compiled. Call results are lost. Methods on
script objects (`m.insert`, `m.get`), `CharCodeAt` and local
`Array<int>`/`Array<string>` trap. `ScriptCall` passes ints only, so
`LumpOpen(name…)` and `(str)LumpReadString` cannot work. On the play side,
`WorldThingSpawned` gets a fake object instead of the actor, and
`GiveInventory` grants are only logged, never applied. `Damage_Control` is
not registered as a class, and `ModifyDamage` is never called from
`damage_mobj`. The existing green tests use `testdata/zscript/bd_like/`, a
734-byte reduction of the real code. Treat ZScript as **not working for
BD** until `docs/34` §14.6 passes on the real files. In the meantime, BD's
ZScript affects only skill 6 damage scaling (`Damage_Control`) and the
`BD_Hash` Map Enhancement System seed: the rest of BD (DECORATE/ACS) does
not depend on it.

The complete list of entry points, the signatures and the engine features BD
needs is in `docs/34` §14.2. The compiler/VM design with reference code is in
§14.3, the engine hooks (`ModifyDamage`, `ScriptCall`, events, `Wads.*`) in
§14.4, and the acceptance tests in §14.6.

Engine-side requirements: ZScript compiled before DECORATE in the definitions
pipeline; `ACSF_ScriptCall` (210) dispatching to ZScript statics (today a
bootstrap no-op in `room-acs/src/pcd.rs`); `Wads.*` reading through
`room-vfs`; MAPINFO `AddEventHandlers` resolving ZScript classes. BD needs only
`docs/34` milestones **ZS0–ZS4**; the rest of the VM (UI scope, JIT, coverage
sprint) serves heavier ZScript mods.

### 3.11 Other lumps and UI (verify with the harness)
- **KEYCONF**: `clearplayerclasses`, `addplayerclass` (5 classes: player class
  menu), `weaponsection`, `setslot 0–9`, `addkeysection`, `addmenukey`,
  `alias`, `defaultbind` (whitelist in `docs/05` §3).
- **CVARINFO**: 122 `server`/`user` cvars read by ACS (`GetCVar`,
  `GetUserCVar`, `SetCVar`, `SetUserCVar`); user cvars must be synced per
  player in netplay.
- **MENUDEF**: 1,196 lines of option menus bound to those cvars (BD options,
  key sections from KEYCONF).
- **SBARINFO**: custom bar (`drawswitchableimage weaponslot`, `InInventory`
  with `||`, `UsesSecondaryAmmo`, `ammo1`/`ammo2`, `forcescaled`).
- **MAPINFO**: `clearskills`, skills with `ReplaceActor`, `SpawnFilter`,
  `AmmoFactor`, `DamageFactor`, `EasyBossBrain`, `AutoUseHealth`,
  `mustconfirm` text, `TextColor`; `gameinfo` keys.
- **GLDEFS**: lights bound to frames, state `Light()` (D1), brightmaps via
  `#include "doomwalls.bm"` etc., flashlight spot lights.
- **MODELDEF**: MD3 bullet-decal / shockwave / blood models, MD2; voxels.
- **DECALDEF** (3 lumps) + actor `Decal` property (385 uses): blood and
  bullet decals; floor blood is done with actors, but wall decals need the
  property wired to missile/puff impacts.
- **TERRAIN** (2), **SNDINFO** (7: `$random`, `$limit`, `$volume`, `$alias`,
  player sounds `$playersound` for 5 classes), **LANGUAGE** (6 locales),
  **Fontdefs**, **TEXTURES.***, **ANIMDEFS**, **XHAIRS** (crosshair set used by
  `A_SetCrosshair`).

### 3.12 Rendering, audio, performance
- Formats are covered (PNG via `png`, OGG via `lewton`, FLAC via `claxon`,
  MP3 via `symphonia`, MD2/MD3, KVX). Verify sprite offsets for PNG sprites
  (`grAb` chunk) — BD's PNG sprites rely on it.
- **Gore volume**: `A_CustomMissile` is referenced 10,830 times and spawn
  loops like `TNT1 AAAAAAAAAAAAAAA 0 A_CustomMissile("Brutal_FlyingBloodTrail", …)`
  create 15–50 actors per state. A busy fight easily reaches thousands of live
  actors. Requirements:
  - fast path for `NOINTERACTION` actors (no collision, no blockmap links,
    no sight, simple movement) and `NOBLOCKMAP`/`THRUACTORS` ones;
  - actor storage and thinker iteration without per-tick allocation;
  - `A_QueueCorpse` / `sv_corpsequeuesize`, BD's own blood-amount cvars;
  - budget: 3,000 live gore actors at 35 Hz ≤ 4 ms/tic on desktop, ≤ 12 ms
    on web (wasm32); add a soak test.
- Optional later mode (ties to `docs/29` §8.J/K): convert pure-visual gore
  (`NOINTERACTION` + no collision + no gameplay effect) into render-only
  particles with the render RNG. Only possible where BD doesn't read them
  back (some gore is kickable — those must stay sim actors).

### 3.13 Netplay and demos
BD targets Zandronum (client/server, `CLIENTSIDEONLY` actors). Room is
lockstep: every actor, including gore, runs on every peer from the same
inputs, so determinism matters: all randomness through the playsim RNG
(DECORATE `random`/`frandom` already map to it), user cvars synced at join,
`puke`/`pukename` sent through the tic stream, menus changing `server` cvars
only through a network-safe path. The room-server verifier must run BD with
the same compiled ZScript program.

---

## 4. Compatibility harness (BD0)

Add `room-cli mod check <archive>... [--iwad <wad>] [--json out.json] [--md out.md]`:
1. Mount the IWAD + archives with the real VFS; list lumps per namespace,
   duplicates, embedded WADs, maps.
2. Parse every text lump Room supports, collecting **all** errors with
   original file:line (DECORATE through an `#include` source map).
3. DECORATE: parse per actor (lenient mode), build the registry with missing
   parents reported, then report unknown flags, properties, actions
   (case-folded), special-name actions, expression identifiers/functions,
   state keywords — each with counts and the first 3 locations.
4. ACS: load each object, list unknown chunks, scan bytecode (walk the code
   with the PCD operand table) for unknown PCDs and unknown `CALLFUNC` ACSF
   indices; list `LOADACS` libraries and named scripts referenced from
   DECORATE/KEYCONF that don't exist.
5. ZScript: detect, list classes, event handlers, `ScriptCall` targets from
   ACS, and the ZScript VM's native coverage for them (`room-cli zscript coverage`).
6. Optional `--run <map> --tics N --script <inputs>`: headless run with a
   scripted input file; report traps, unknown-action hits at runtime, actor
   counts and tic times.

CI: unit tests use small synthetic fixtures covering each feature (no BD
content). A gated integration test runs the harness on `ROOM_BRUTALDOOM_PK3`
with Freedoom 2 (`./scripts/fetch-freedoom.sh`) and asserts the report's
counters (parse failures, unknown flags/actions, unknown PCDs) are at or
below the milestone's targets, so regressions are visible.

---

## 5. Order of work and where

| Step | Crates / files |
|---|---|
| Harness | `crates/room-cli` (new `mod` subcommand), helpers in `room-game` (mount + load pipeline), `room-decorate` (source map, lenient mode) |
| DECORATE syntax D1–D8 | `crates/room-sc`, `crates/room-decorate` (`parse.rs`, `expr.rs`, `ir.rs`), `crates/room-sim/src/state.rs` (state lights/offsets, keep-sprite) |
| Base classes, flags, properties | `wadsrc/room/actors/*.txt`, `crates/room-sim/src/{class,inventory,pickup,damage,physics}.rs`, new `powerups.rs`, morph |
| Actions, specials-as-actions, expressions, 0-tic loop guard | `crates/room-sim/src/{action,think_player,deco_eval,world,specials}.rs` |
| Weapons/input | `crates/room-sim/src/{think_player,player}.rs`, `room-input`, shells (bindings), `room-ui` (crosshairs, zoom FOV) |
| ACS | `crates/room-acs/src/{load,pcd,vm}.rs`, ACS host in `room-sim/src/acs.rs`, `room-ui` (HUD messages) |
| ZScript | `docs/34`: new crates `room-zscript`, `room-zvm`; `room-sim`/`room-acs` (`ScriptCall`), `room-vfs` (`Wads.*`) |
| Lumps/UI | `room-defs`, `room-ui` (MENUDEF, SBARINFO), `room-console` (KEYCONF, CVARINFO, `puke`) |
| Performance | `room-sim` actor storage / thinker loop / blockmap, `room-render` sprite batching |
| Netplay | `room-net`, `room-server` verifier, user-cvar sync |

---

## 6. Milestones

| ID | Scope | Acceptance (with BD + Freedoom 2 unless noted) |
|---|---|---|
| **BD0** | `room-cli mod check` harness + gated BD integration test + synthetic fixtures | Gated `brutal_doom_pk3_gated`: DECORATE **4130/4130**, ZScript ok, 0 lump errors, 8 skills / 5 player classes |
| **BD1** | DECORATE syntax D1–D8, all-errors reporting with file:line, lenient mode | **4130 / 4130** actors parse on `brutal22test6.pk3` |
| **BD2** | Native base classes (§3.3), full flag table + storage (§3.4), properties (§3.5), unknown-name warnings | Registry / mod-check: **0** missing parents, **0** unknown flags, **0** unknown properties |
| **BD3** | Case-folded action dispatch, missing actions, specials-as-actions, expressions + precompilation, iterative 0-tic chains | **0** unknown actions; case-fold; MAP01 **10s** + **40s** soak green; full **10-min deferred** (thinker growth ~7/tic) |
| **BD4** | Weapons/input: `WRF_*`, `AltFire/Reload/Zoom/User1–4`, buttons, dual ammo, `A_ZoomFactor`, `A_SetCrosshair`, 10 slots | Fixture BdPistol e2e + commercial `BrutalPistol` Fire/Reload/Zoom + ATTACK leaves Ready |
| **BD5** | ACS: all chunks, full PCD table, functions/arrays/strings, HUD messages, missing ACSF, `puke`/`pukename` via tic stream | `BD_Main` OPEN + LOADACS + `pukename GetCVars`; HUD PCDs stubbed to host print |
| **BD6** | BD's ZScript on the Room ZScript VM: `docs/34` **ZS1b–ZS4b + ZS6b** (§14.7): typed compiler, call ABI, `ZHost`, real event structs, `ModifyDamage`, `ScriptCall` marshalling, `Wads.*` on the VFS, ZScript class registration | **Done (2026-11-02)**: `docs/34` §14.6 probe and every `bd_*` test pass on `testdata/zscript/bd_real`. Lump bytes are the in-memory wad map (`Wads.FindLump` still returns -1). `lower.rs` is the ZScript compiler; the runtime natives live in `room-zvm` `natives.rs` |
| **BD7** | KEYCONF/CVARINFO/MENUDEF/SBARINFO/MAPINFO skills/GLDEFS/MODELDEF/DECALDEF/TERRAIN/SNDINFO/LANGUAGE verified with BD; player class menu | Harness: **0** lump errors; fixture MENUDEF golden; GZDoom screenshots deferred |
| **BD8** | Performance: `NOINTERACTION` fast path, gore budget, corpse queue, streamed pk3 loading on web | Soft 3k budget + corpse queue green; **web streamed pk3 deferred** |
| **BD9** | Netplay: user-cvar sync, network-safe puke, verifier with BD (ZScript on the VM) | Commercial BD ZScript 2-peer checksum; **15-min room soak deferred** |

Track progress in `docs/21` (new BD table) and keep `docs/11` (DECORATE
coverage) and `docs/10` (ACS coverage) updated as each milestone lands.

---

## 7. Notes and risks

- **Licence / distribution:** BD ships its own licence file (`DDZLICENSE`) and
  is distributed by its author; Room must not bundle or redistribute it. The
  web client and room-server can only load it from a user-provided file.
- **Moving target:** v22 is a test/dev build; pin the harness counters to this
  exact file (store its SHA-256 in the test) and re-baseline deliberately for
  newer builds.
- **GZDoom version semantics:** BD's ZScript says `version "3.2.3"`; DECORATE
  features used are GZDoom 2.x–4.x era. Where GZDoom versions differ, follow
  the current GZDoom behaviour (AGENTS.md rule 5) unless BD visibly depends on
  the older one.
- **Zandronum paths:** BD branches on `IsZandronum()`; Room should report
  "not Zandronum" everywhere so BD takes its GZDoom paths.
- **Monsters-only variant** (mentioned on ModDB) uses the same systems and is
  a good intermediate target after BD3.

## 8. Open questions for the owner

1. Strict (GZDoom-like abort) or lenient (skip broken actors) DECORATE loading
   by default?
2. Is a render-only gore mode (§3.12) acceptable even though it changes
   behaviour slightly (no kicking pure-visual gore)?
3. Which BD build to target long-term: this v22 test 6, v21 Gold (stable), or
   follow the dev builds?

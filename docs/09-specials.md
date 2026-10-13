# 09 — Action specials (`room-sim::specials`)

GZDoom references: `src/playsim/actionspecials.h` (authoritative number→name→arg-count table),
`p_lnspec.cpp` (implementations), `p_spec.cpp` (activation, sector specials),
`p_switch.cpp`, `p_doors.cpp`, `p_floor.cpp`, `p_ceiling.cpp`, `p_plats.cpp`,
`p_pillar.cpp`, `p_lights.cpp`, `p_scroll.cpp`, `p_pusher.cpp`, `p_teleport.cpp`,
`wadsrc/static/xlat/*.txt` (Doom/Heretic/Strife translators), ZDoom wiki
*Action specials* and *Sector types*.

## 1. Model

Every special is `(number: u16, args: [i32; 5])`. Specials are the same
whether triggered by a line, a thing's death/pickup (`Thing.special`), a sector
action actor, ACS (`LSPEC` p-codes), DECORATE (calling a special by name as an
action function), or a WASM module (`world.execute_special`).

```rust
pub type SpecialFn = fn(&mut SpecialCtx, args: &[i32; 5]) -> i32;   // returns "success" (non-zero) like GZDoom
pub struct SpecialCtx<'w> {
    pub world: &'w mut WorldCtx<'w>,
    pub line: Option<LineId>, pub side: u8 /*0 front, 1 back*/,
    pub activator: Option<ActorId>,
    pub backside: bool,
    pub string_arg0: Option<Name>,       // UDMF arg0str / ACS named scripts
}
pub static SPECIALS: [Option<SpecialDef>; 512];  // { name, fn, min_args, max_args, flags }
```

Port `actionspecials.h` 1:1 into a generated Rust table (`build.rs` or a `macro_rules!`
list). Keep GZDoom names — ACS, DECORATE and UDMF refer to specials by name.

### 1.1 Categories (implementation checklist)

| Category | Examples (number) |
|---|---|
| Polyobjects | `Polyobj_StartLine` (1), `RotateLeft/Right` (2/3), `Move` (4), `ExplicitLine` (5), `MoveTimes8`, `DoorSwing` (7), `DoorSlide` (8), `OR_*` variants, `MoveTo`, `MoveToSpot`, `Stop` |
| Doors | `Door_Close` (10), `Door_Open` (11), `Door_Raise` (12), `Door_LockedRaise` (13), `Door_Animated` (14), `Door_WaitRaise`, `Door_WaitClose`, `Door_CloseWaitOpen`, `Generic_Door` |
| Floors | `Floor_LowerByValue` (20) … `Floor_RaiseAndCrush`, `Floor_MoveToValue(Times8)`, `Floor_Waggle`, `Floor_Donut`, `Floor_TransferTrigger/Numeric`, `Floor_RaiseByTexture`, `Floor_LowerToHighestEE`, `Floor_RaiseAndCrushDoom`, `Floor_*Instant`, `Generic_Floor` |
| Ceilings | `Ceiling_LowerByValue` (40) …, crushers (`Ceiling_CrushAndRaise(A/Dist/Silent…)`, `Ceiling_CrushStop`, `Generic_Crusher(2)`), `Ceiling_Waggle`, `Ceiling_MoveToValue(Times8)`, `Generic_Ceiling` |
| Floor+Ceiling | `FloorAndCeiling_LowerByValue`, `RaiseByValue`, `LowerRaise`, `Pillar_Build(AndCrush)`, `Pillar_Open`, `Elevator_*` |
| Platforms | `Plat_PerpetualRaise(Lip)`, `Plat_Stop`, `Plat_DownWaitUpStay(Lip)`, `Plat_UpWaitDownStay`, `Plat_DownByValue`, `Plat_UpByValue`, `Plat_UpNearestWaitDownStay`, `Plat_RaiseAndStayTx0`, `Plat_UpByValueStayTx`, `Plat_ToggleCeiling`, `Generic_Lift` |
| Stairs | `Stairs_BuildDown/Up(Sync)`, `Stairs_BuildUpDoom(Crush/Sync)`, `Generic_Stairs` |
| Lights | `Light_RaiseByValue`, `LowerByValue`, `ChangeToValue`, `Fade`, `Glow`, `Flicker`, `Strobe`, `StrobeDoom`, `Stop`, `MinNeighbor`, `MaxNeighbor`, `Light_ForceLightning` |
| Teleport / exit | `Teleport` (70), `Teleport_NoFog` (71), `Teleport_NewMap` (74), `Teleport_EndGame` (75), `TeleportOther`, `TeleportGroup`, `TeleportInSector`, `Teleport_Line`, `Teleport_NoStop`, `Teleport_ZombieChanger`, `Exit_Normal`, `Exit_Secret` |
| ACS | `ACS_Execute` (80), `ACS_Suspend`, `ACS_Terminate`, `ACS_LockedExecute(Door)`, `ACS_ExecuteWithResult`, `ACS_ExecuteAlways`, `FS_Execute` (FraggleScript – C) |
| Things | `Thing_Activate/Deactivate/Remove/Destroy/Damage/Raise/Stop/Move/SetSpecial/ChangeTID/Hate/SetGoal/SetTranslation/SetConversation/Spawn(NoFog/Facing)/Projectile(Gravity/Aimed/Intercept)`, `ThrustThing(Z)`, `DamageThing`, `HealThing`, `NoiseAlert`, `Radius_Quake` |
| Scrolling / pushing | `Scroll_Texture_Left/Right/Up/Down/Both/Model/Offsets`, `Scroll_Wall`, `Scroll_Floor`, `Scroll_Ceiling`, `Sector_SetWind`, `Sector_SetCurrent`, `PointPush_SetForce`, `Sector_SetFriction` |
| Sector properties | `Sector_SetColor`, `Sector_SetFade`, `Sector_SetDamage`, `Sector_SetGravity`, `Sector_SetTranslucent`, `Sector_SetPlaneReflection`, `Sector_SetRotation`, `Sector_SetFloor/CeilingPanning`, `Sector_SetFloor/CeilingScale(2)`, `Sector_ChangeSound`, `Sector_ChangeFlags`, `Sector_SetContents`, `Sector_SetLink`, `Sector_CopyScroller`, `SetGlobalFogParameter` |
| Geometry setup (load-time only) | `Plane_Align` (181), `Plane_Copy` (118), `Line_SetIdentification` (121), `Static_Init` (190), `Transfer_Heights` (209), `Transfer_FloorLight` (210), `Transfer_CeilingLight` (211), `Transfer_WallLight`, `Sector_Set3DFloor` (160), `Sector_Attach3dMidtex`, `Sector_SetPortal` (57), `Line_SetPortal` (156), `Line_QuickPortal`, `Line_Mirror` (182), `Line_Horizon` (9), `Line_AlignFloor/Ceiling`, `TranslucentLine` (208), `ExtraFloor_LightOnly`, `Polyobj_StartLine/ExplicitLine` |
| Lines (runtime) | `Line_SetTextureOffset`, `Line_SetTextureScale`, `Line_SetBlocking`, `Line_SetPortalTarget`, `Line_SetHealth`, `Sector_SetHealth` (destructible geometry), `ForceField`, `ClearForceField`, `GlassBreak` |
| Misc | `Autosave`, `ChangeCamera`, `ChangeSkill`, `SetPlayerProperty`, `UsePuzzleItem`, `StartConversation`, `SendToCommunicator`, `Team_Score/GivePoints`, `Player_SetTeam` |

Rule: if a special's name appears in GZDoom's `actionspecials.h` it must be in Room's table with identical number and argument count.

## 2. Activation

### 2.1 Activation flags

```
SPAC_Cross, SPAC_Use, SPAC_MCross, SPAC_Impact, SPAC_Push, SPAC_PCross,
SPAC_UseThrough, SPAC_AnyCross, SPAC_MUse, SPAC_MPush, SPAC_UseBack, SPAC_Damage, SPAC_Death
```

In Hexen-format maps the activation is encoded in linedef flags bits 10–12
(`GET_SPAC(flags)`): `0 Cross, 1 Use, 2 MCross, 3 Impact, 4 Push, 5 PCross, 6 UseThrough, 7 Impact|PCross`. Confirm against the translation table in GZDoom's map loader and pin it with a unit test. UDMF uses booleans (`playercross`, `playeruse`, `monstercross`, `impact`, `playerpush`, `missilecross`, `monsteruse`, `monsterpush`, `anycross`, `repeatspecial`, `firstsideonly`, `playeruseback`, `damagespecial`, `deathspecial`).

### 2.2 `P_ActivateLine` / `P_TestActivateLine` semantics

`activate_line(line, mo, side, activation_type, optpos) -> bool`:
1. Test permission (`P_TestActivateLine`): activation type matches; monsters only for `MCross/MUse/MPush` or legacy rules (`ML_MONSTERSCANACTIVATE`, Doom-format monster-activatable specials via XLAT flags); `MF2_NOTELEPORT`/`MF_MISSILE` rules for teleporters; `ML_FIRSTSIDEONLY`; locked lines (`lock` field → `P_CheckKeys(mo, lock, remote)` with LOCKDEFS messages/sounds); `ML_REPEAT_SPECIAL` vs one-shot.
2. Execute the special with `line` and `activator`.
3. If not repeatable and executed successfully (and not a ZDoom "keep special" case): clear the line special (Doom behaviour). GZDoom nuance: Hexen-format lines clear the special only if `!repeat`, regardless of success — port `P_ActivateLine` exactly.
4. Switch texture change (`P_ChangeSwitchTexture`) for `Use`/`Impact`(gunshot)/`Push` activations if the special returned success **or** per the `ML_*` / compat rules; repeatable switches spawn a button thinker that reverts after `BUTTONTIME` (35 tics) with sound.
5. `checkswitchrange` (MAPINFO / line flag): use-line reachability height test.

### 2.3 Sector specials

- **Load-time** (`P_InitSectorSpecial`): light effects spawn thinkers (Doom types 1–4, 8, 12, 13, 17; Hexen phased/sequence lights; ZDoom extended), secret sectors, damage (`Sector_SetDamage` equivalents with leakiness), exits, scroll/wind/current (Hexen `Scroll_*` sector types 201–244 and `Wind_*`), friction (Boom bit), push (Boom bit), Boom generalized damage bits, MBF21 instant-death/kill-all bits (`SECF_*`).
- **Per-tic** (`P_PlayerInSpecialSector` / `CheckEnvironment` / `sector_t::DoSectorDamage`): player-only damage every N tics (`level.time & damageinterval`), environment suit protection (`PowerIronFeet`, leak chance), secret discovery (once, message `"A secret is revealed!"` with `cl_showsecretmessage`), end-level sector (Doom type 11: god mode removed, damage, exit at health ≤ 10).
- **Sector actions** (`SecAct*` things): triggered by actor enter/exit/hit floor/use etc.; execute their own special.

## 3. Translation of Doom-format specials (XLAT)

Doom, Heretic and Strife format maps (and UDMF `doom`/`heretic`/`strife` namespaces) store game-specific line types. GZDoom translates them on load to Hexen-style specials using **XLAT** tables (`wadsrc/static/xlat/doom.txt`, `heretic.txt`, `strife.txt`, `base.txt`, `defines.i`). Room does the same:

- `room.pk3/xlat/*.txt` = copies of GZDoom's xlat tables (GPL data).
- Translator grammar (§`05-text-lumps.md §3.6`): fixed line type entries
  `1 = USE|REP|MONST, Door_Raise (0, D_SLOW, VDOORWAIT)`, tag placeholder `tag`, flags `WALK, USE, SHOOT, REP, MONST, PUSH, FIRSTSIDE …`, generalized ranges with bitfield decoding (Boom generalized lines: crushers 0x2F80, stairs 0x3000, lifts 0x3400, locked doors 0x3800, doors 0x3C00, ceilings 0x4000, floors 0x6000), sector type translation (`sector` bitmask rules mapping Boom bits: damage 0x60, secret 0x80, friction 0x100, push 0x200, MBF21 0x400/0x800), thing flag translation.
- MAPINFO `translator` key overrides per map/game.
- The translator also handles Boom's special handling of line tag 0 on certain specials (e.g. "use the back sector"), `compat_*`-dependent cases, and `Line_SetIdentification` from tags for Doom maps.

Translation output is cached per (translator, line type).

## 4. Mover thinkers — behavioural notes

Port each class from GZDoom; the following details are easy to get wrong:

- **Doors** (`DDoor`): types `doorOpen, doorClose, doorRaise, doorWaitRaise, doorCloseWaitOpen, doorWaitClose, doorRaiseIn5Mins`; blocked closing doors reverse (raise) unless `crush`; door sounds via sound sequences (`SNDSEQ` `Door` slots or `$seq` names) with `nostopcutoff`; "door lighting" (`lighttag` arg brightens tagged sectors proportionally — `EV_DoorLight`); vanilla's "door re-trigger reverses direction" for repeatable manual doors.
- **Floors**: Boom "change texture & special" models (`numeric` / `trigger` model, `Floor_TransferTrigger`), stair builders (texture-match rules, `Stairs_BuildUpDoom` vs Hexen `Stairs_Build*` with step delay and reset), donuts, `hexencrush` vs Doom crush, slopes moving (plane `d` change), `instant` moves.
- **Platforms** (`DPlat`): perpetual raise (stop/restart with `Plat_Stop` and in-stasis list), `plat_*` sounds, `PLATWAIT` = 3 s, `lip`.
- **Ceilings/crushers**: crusher speed halving while crushing (Doom), silent crushers, `Generic_Crusher2` Hexen-style, crush mode `crushDoom|crushHexen|crushSlowdown`.
- **Elevators**, **Pillars**, **Waggles** (`Floor_Waggle`/`Ceiling_Waggle`, sine with `FloatBobOffsets` table).
- **Lights**: `DFireFlicker`, `DFlicker`, `DLightFlash`, `DStrobe`, `DGlow`, `DGlow2`, `DPhased`, Hexen light sequences (`Light_Sequence*` sector types), lightning (MAPINFO `lightning`: sky flash, sector light boost, thunder sound; `Light_ForceLightning`).
- **Scrollers** (`DScroller`): `EScroll` types (side, floor, ceiling, carry), displacement-based (`Scroll_Texture_Model` with control sector delta), accelerative scrollers; carry/wind pushes actors on the sector (`P_Thrust` with `CARRYFACTOR`).
- **Pushers** (`DPusher`): wind, current, point push/pull (`PointPusher`/`PointPuller` things), `MF2_WINDTHRUST`.
- **Interpolation**: every mover registers plane/scroll/polyobject interpolations so the renderer can lerp (see `01-architecture.md §6`).

## 5. Tests

- Synthetic map per special category (generated UDMF) verifying end state after N tics.
- XLAT golden: translate every Doom line type 0–141, Boom 142–272, generalized samples; snapshot translation output.
- Doom 1.9 vanilla demo-sync tests in compat mode cover mover timing (see `20`).

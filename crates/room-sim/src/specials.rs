//! Line / sector specials (`P_ExecuteSpecial` subset).

use crate::actor::ActorId;
use crate::damage::damage_mobj;
use crate::world::World;
use room_core::{Angle, DVec3, Fixed, LineId, SectorId, SideId, VertexId, dmath};

/// Vanilla crush damage amount (`P_ChangeSector` / `PIT_ChangeSector`).
const CRUSH_DAMAGE: i32 = 10;

/// Hexen / ZDoom special numbers used in M5.
pub mod num {
    /// `Door_Raise` — tag, speed, delay, lighttag.
    pub const DOOR_RAISE: u16 = 12;
    /// `Door_Open`.
    pub const DOOR_OPEN: u16 = 11;
    /// `Door_Close`.
    pub const DOOR_CLOSE: u16 = 10;
    /// `Door_LockedRaise`.
    pub const DOOR_LOCKED_RAISE: u16 = 13;
    /// `Door_CloseWaitOpen`.
    pub const DOOR_CLOSE_WAIT_OPEN: u16 = 249;
    /// `Door_WaitRaise` — tag, speed, delay, wait, lighttag.
    pub const DOOR_WAIT_RAISE: u16 = 105;
    /// `Door_WaitClose` — tag, speed, wait, lighttag.
    pub const DOOR_WAIT_CLOSE: u16 = 106;
    /// `Elevator_RaiseToNearest`.
    pub const ELEVATOR_RAISE_TO_NEAREST: u16 = 245;
    /// `Elevator_MoveToFloor`.
    pub const ELEVATOR_MOVE_TO_FLOOR: u16 = 246;
    /// `Elevator_LowerToNearest`.
    pub const ELEVATOR_LOWER_TO_NEAREST: u16 = 247;
    /// `Light_ChangeToValue`.
    pub const LIGHT_CHANGE_TO_VALUE: u16 = 112;
    /// `Exit_Normal`.
    pub const EXIT_NORMAL: u16 = 243;
    /// `Floor_LowerByValue` — tag, speed, height.
    pub const FLOOR_LOWER_BY_VALUE: u16 = 20;
    /// `Floor_RaiseByValue` — tag, speed, height.
    pub const FLOOR_RAISE_BY_VALUE: u16 = 23;
    /// `Floor_RaiseByValueTimes8`.
    pub const FLOOR_RAISE_BY_VALUE_TIMES8: u16 = 35;
    /// `Floor_LowerByValueTimes8`.
    pub const FLOOR_LOWER_BY_VALUE_TIMES8: u16 = 36;
    /// `Floor_LowerToLowest` — tag, speed.
    pub const FLOOR_LOWER_TO_LOWEST: u16 = 21;
    /// `Floor_RaiseToNearest`.
    pub const FLOOR_RAISE_TO_NEAREST: u16 = 25;
    /// `Floor_RaiseAndCrush`.
    pub const FLOOR_RAISE_AND_CRUSH: u16 = 28;
    /// `Floor_LowerToHighest`.
    pub const FLOOR_LOWER_TO_HIGHEST: u16 = 22;
    /// `Floor_LowerToNearest`.
    pub const FLOOR_LOWER_TO_NEAREST: u16 = 242;
    /// `Floor_RaiseByTexture`.
    pub const FLOOR_RAISE_BY_TEXTURE: u16 = 240;
    /// `Floor_RaiseToHighest`.
    pub const FLOOR_RAISE_TO_HIGHEST: u16 = 24;
    /// `Floor_RaiseToLowestCeiling`.
    pub const FLOOR_RAISE_TO_LOWEST_CEILING: u16 = 238;
    /// `Stairs_BuildDown` — tag, speed, height, delay, reset.
    pub const STAIRS_BUILD_DOWN: u16 = 26;
    /// `Stairs_BuildUp` — tag, speed, height, delay, reset.
    pub const STAIRS_BUILD_UP: u16 = 27;
    /// `Stairs_BuildDownSync` — tag, speed, height, reset.
    pub const STAIRS_BUILD_DOWN_SYNC: u16 = 31;
    /// `Stairs_BuildUpSync` — tag, speed, height, reset.
    pub const STAIRS_BUILD_UP_SYNC: u16 = 32;
    /// `Ceiling_CrushAndRaiseA` — tag, dnspeed, upspeed, damage, crushtype.
    pub const CEILING_CRUSH_AND_RAISE_A: u16 = 196;
    /// `Ceiling_CrushAndRaiseSilentA` — tag, dnspeed, upspeed, damage, crushtype.
    pub const CEILING_CRUSH_AND_RAISE_SILENT_A: u16 = 197;
    /// `Generic_Crusher2` — tag, dnspeed, upspeed, silent, damage.
    pub const GENERIC_CRUSHER2: u16 = 169;
    /// `Door_Animated` — tag, speed, delay, lock (raise subset).
    pub const DOOR_ANIMATED: u16 = 14;
    /// `Door_AnimatedClose` — tag, speed.
    pub const DOOR_ANIMATED_CLOSE: u16 = 274;
    /// `Floor_LowerToHighestEE` — tag, speed, adjust.
    pub const FLOOR_LOWER_TO_HIGHEST_EE: u16 = 256;
    /// `Floor_RaiseToLowest` — tag [, speed].
    pub const FLOOR_RAISE_TO_LOWEST: u16 = 257;
    /// `Floor_LowerToLowestCeiling` — tag, speed.
    pub const FLOOR_LOWER_TO_LOWEST_CEILING: u16 = 258;
    /// `Floor_RaiseToCeiling` — tag, speed.
    pub const FLOOR_RAISE_TO_CEILING: u16 = 259;
    /// `Floor_ToCeilingInstant` — tag.
    pub const FLOOR_TO_CEILING_INSTANT: u16 = 260;
    /// `Ceiling_RaiseToHighest` — tag, speed.
    pub const CEILING_RAISE_TO_HIGHEST: u16 = 262;
    /// `Ceiling_ToHighestInstant` — tag.
    pub const CEILING_TO_HIGHEST_INSTANT: u16 = 263;
    /// `Ceiling_LowerToNearest` — tag, speed.
    pub const CEILING_LOWER_TO_NEAREST: u16 = 264;
    /// `Ceiling_RaiseToLowest` — tag, speed.
    pub const CEILING_RAISE_TO_LOWEST: u16 = 265;
    /// `Ceiling_RaiseToHighestFloor` — tag, speed.
    pub const CEILING_RAISE_TO_HIGHEST_FLOOR: u16 = 266;
    /// `Ceiling_ToFloorInstant` — tag.
    pub const CEILING_TO_FLOOR_INSTANT: u16 = 267;
    /// `Stairs_BuildDownDoom` — tag, speed, height, delay, reset.
    pub const STAIRS_BUILD_DOWN_DOOM: u16 = 270;
    /// `Stairs_BuildUpDoomSync` — tag, speed, height, reset.
    pub const STAIRS_BUILD_UP_DOOM_SYNC: u16 = 271;
    /// `Stairs_BuildDownDoomSync` — tag, speed, height, reset.
    pub const STAIRS_BUILD_DOWN_DOOM_SYNC: u16 = 272;
    /// `Stairs_BuildUpDoomCrush` — tag, speed, height, delay, reset.
    pub const STAIRS_BUILD_UP_DOOM_CRUSH: u16 = 273;
    /// `Floor_RaiseByValueTxTy` — tag, speed, height.
    pub const FLOOR_RAISE_BY_VALUE_TX_TY: u16 = 239;
    /// `Floor_LowerToLowestTxTy` — tag, speed.
    pub const FLOOR_LOWER_TO_LOWEST_TX_TY: u16 = 241;
    /// `Floor_LowerByTexture` — tag, speed.
    pub const FLOOR_LOWER_BY_TEXTURE: u16 = 261;
    /// `Ceiling_RaiseByTexture` — tag, speed.
    pub const CEILING_RAISE_BY_TEXTURE: u16 = 268;
    /// `Ceiling_LowerByTexture` — tag, speed.
    pub const CEILING_LOWER_BY_TEXTURE: u16 = 269;
    /// `Floor_MoveToValueAndCrush` — tag, speed, height, negative [, crush].
    pub const FLOOR_MOVE_TO_VALUE_AND_CRUSH: u16 = 279;
    /// `Ceiling_MoveToValueAndCrush` — tag, speed, height, negative [, crush].
    pub const CEILING_MOVE_TO_VALUE_AND_CRUSH: u16 = 280;
    /// `Polyobj_StopSound` — po.
    pub const POLYOBJ_STOP_SOUND: u16 = 283;
    /// Room: toggle a Duke `SE 20` stretch (`user_stretch`).
    pub const SECTOR_STRETCH: u16 = 284;
    /// `Line_Horizon` — paint one-sided wall as horizon.
    pub const LINE_HORIZON: u16 = 9;
    /// `Transfer_WallLight` — tag, side (front/back).
    pub const TRANSFER_WALL_LIGHT: u16 = 16;
    /// `Sector_CopyScroller` — tag, type.
    pub const SECTOR_COPY_SCROLLER: u16 = 58;
    /// `Light_ForceLightning` — mode.
    pub const LIGHT_FORCE_LIGHTNING: u16 = 109;
    /// `Plane_Copy` — tag, … (floor/ceiling copy from front).
    pub const PLANE_COPY: u16 = 118;
    /// `Line_SetIdentification` — id, … (runtime line id).
    pub const LINE_SET_IDENTIFICATION: u16 = 121;
    /// `Scroll_Texture_Model` — lineid, controlside.
    pub const SCROLL_TEXTURE_MODEL: u16 = 222;
    /// `Scroll_Texture_Offsets` — …
    pub const SCROLL_TEXTURE_OFFSETS: u16 = 225;
    /// `Teleport`.
    pub const TELEPORT: u16 = 70;
    /// `Teleport_NoFog`.
    pub const TELEPORT_NO_FOG: u16 = 71;
    /// `Teleport_Line` — Boom silent line-to-line teleport.
    pub const TELEPORT_LINE: u16 = 215;
    /// `Exit_Secret`.
    pub const EXIT_SECRET: u16 = 244;
    /// `Plat_DownWaitUpStay` (lip variant uses same thinker).
    pub const PLAT_DOWN_WAIT_UP_STAY: u16 = 62;
    /// `Ceiling_LowerToFloor`.
    pub const CEILING_LOWER_TO_FLOOR: u16 = 254;
    /// `Ceiling_RaiseToNearest`.
    pub const CEILING_RAISE_TO_NEAREST: u16 = 252;
    /// `Ceiling_LowerToLowest`.
    pub const CEILING_LOWER_TO_LOWEST: u16 = 253;
    /// `Ceiling_LowerToHighestFloor`.
    pub const CEILING_LOWER_TO_HIGHEST_FLOOR: u16 = 192;
    /// `FloorAndCeiling_LowerByValue`.
    pub const FLOOR_AND_CEILING_LOWER_BY_VALUE: u16 = 95;
    /// `FloorAndCeiling_RaiseByValue`.
    pub const FLOOR_AND_CEILING_RAISE_BY_VALUE: u16 = 96;
    /// `FloorAndCeiling_LowerRaise`.
    pub const FLOOR_AND_CEILING_LOWER_RAISE: u16 = 251;
    /// `Plat_ToggleCeiling`.
    pub const PLAT_TOGGLE_CEILING: u16 = 231;
    /// `Ceiling_CrushAndRaise` (dist variant).
    pub const CEILING_CRUSH_AND_RAISE: u16 = 42;
    /// `Ceiling_CrushAndRaiseSilentDist`.
    pub const CEILING_CRUSH_AND_RAISE_SILENT_DIST: u16 = 104;
    /// `Ceiling_LowerAndCrush`.
    pub const CEILING_LOWER_AND_CRUSH: u16 = 43;
    /// `Ceiling_LowerAndCrushDist`.
    pub const CEILING_LOWER_AND_CRUSH_DIST: u16 = 97;
    /// `Sector_SetTranslucent`.
    pub const SECTOR_SET_TRANSLUCENT: u16 = 98;
    /// `Line_SetBlocking`.
    pub const LINE_SET_BLOCKING: u16 = 55;
    /// `TranslucentLine`.
    pub const TRANSLUCENT_LINE: u16 = 208;
    /// `NoiseAlert`.
    pub const NOISE_ALERT: u16 = 173;
    /// `Ceiling_CrushStop`.
    pub const CEILING_CRUSH_STOP: u16 = 44;
    /// `Floor_CrushStop`.
    pub const FLOOR_CRUSH_STOP: u16 = 46;
    /// `Floor_Donut`.
    pub const FLOOR_DONUT: u16 = 250;
    /// `Plat_PerpetualRaise`.
    pub const PLAT_PERPETUAL_RAISE: u16 = 60;
    /// `Plat_Stop`.
    pub const PLAT_STOP: u16 = 61;
    /// `Floor_RaiseAndCrushDoom`.
    pub const FLOOR_RAISE_AND_CRUSH_DOOM: u16 = 99;
    /// `Stairs_BuildUpDoom` (and crush variant share the number in XLAT).
    pub const STAIRS_BUILD_UP_DOOM: u16 = 217;
    /// `Light_RaiseByValue`.
    pub const LIGHT_RAISE_BY_VALUE: u16 = 110;
    /// `Light_LowerByValue`.
    pub const LIGHT_LOWER_BY_VALUE: u16 = 111;
    /// `Light_Fade`.
    pub const LIGHT_FADE: u16 = 113;
    /// `Light_Glow`.
    pub const LIGHT_GLOW: u16 = 114;
    /// `Light_Flicker` — tag, upper, lower (GZDoom).
    pub const LIGHT_FLICKER: u16 = 115;
    /// `Light_Strobe` — tag, upper, lower, u-tics, l-tics.
    pub const LIGHT_STROBE: u16 = 116;
    /// `Light_Stop` — tag (clears light thinkers on tagged sectors).
    pub const LIGHT_STOP: u16 = 117;
    /// `Ceiling_LowerByValue`.
    pub const CEILING_LOWER_BY_VALUE: u16 = 40;
    /// `Ceiling_RaiseByValue`.
    pub const CEILING_RAISE_BY_VALUE: u16 = 41;
    /// `Light_StrobeDoom`.
    pub const LIGHT_STROBE_DOOM: u16 = 232;
    /// `Light_MinNeighbor`.
    pub const LIGHT_MIN_NEIGHBOR: u16 = 233;
    /// `Light_MaxNeighbor`.
    pub const LIGHT_MAX_NEIGHBOR: u16 = 234;
    /// `Floor_TransferTrigger`.
    pub const FLOOR_TRANSFER_TRIGGER: u16 = 235;
    /// `Floor_TransferNumeric`.
    pub const FLOOR_TRANSFER_NUMERIC: u16 = 236;
    /// `Scroll_Texture_Left`.
    pub const SCROLL_TEXTURE_LEFT: u16 = 100;
    /// `Scroll_Texture_Right`.
    pub const SCROLL_TEXTURE_RIGHT: u16 = 101;
    /// `Scroll_Texture_Up`.
    pub const SCROLL_TEXTURE_UP: u16 = 102;
    /// `Scroll_Texture_Down`.
    pub const SCROLL_TEXTURE_DOWN: u16 = 103;
    /// `Scroll_Floor` — tag, flags, x-move, y-move (Hexen units / 32 ≈ map units/tic).
    pub const SCROLL_FLOOR: u16 = 223;
    /// `Scroll_Ceiling` — tag, flags, x-move, y-move (same units as floor).
    pub const SCROLL_CEILING: u16 = 224;
    /// `Plat_RaiseAndStayTx0` — raise to next higher floor, copy trigger floor tex, clear special.
    pub const PLAT_RAISE_AND_STAY_TX0: u16 = 228;
    /// `Plat_DownByValue` — tag, speed, delay, height (×8).
    pub const PLAT_DOWN_BY_VALUE: u16 = 63;
    /// `Plat_UpWaitDownStay` — tag, speed, delay.
    pub const PLAT_UP_WAIT_DOWN_STAY: u16 = 64;
    /// `Plat_UpByValue` — tag, speed, delay, height (×8).
    pub const PLAT_UP_BY_VALUE: u16 = 65;
    /// `Floor_LowerInstant` — tag, unused, height (×8).
    pub const FLOOR_LOWER_INSTANT: u16 = 66;
    /// `Floor_RaiseInstant` — tag, unused, height (×8).
    pub const FLOOR_RAISE_INSTANT: u16 = 67;
    /// `Floor_MoveToValue` — tag, speed, height, negative.
    pub const FLOOR_MOVE_TO_VALUE: u16 = 37;
    /// `Floor_MoveToValueTimes8` — tag, speed, height, negative.
    pub const FLOOR_MOVE_TO_VALUE_TIMES8: u16 = 68;
    /// `Ceiling_MoveToValue` — tag, speed, height, negative.
    pub const CEILING_MOVE_TO_VALUE: u16 = 47;
    /// `Ceiling_MoveToValueTimes8` — tag, speed, height, negative.
    pub const CEILING_MOVE_TO_VALUE_TIMES8: u16 = 69;
    /// `Ceiling_LowerInstant` — tag, unused, height (×8).
    pub const CEILING_LOWER_INSTANT: u16 = 193;
    /// `Ceiling_RaiseInstant` — tag, unused, height (×8).
    pub const CEILING_RAISE_INSTANT: u16 = 194;
    /// `Scroll_Texture_Both` — id, left, right, up, down.
    pub const SCROLL_TEXTURE_BOTH: u16 = 221;
    /// `TeleportOther` — other_tid, dest_tid, fog.
    pub const TELEPORT_OTHER: u16 = 76;
    /// `TeleportGroup` — group_tid, source_tid, dest_tid, move_source, fog.
    pub const TELEPORT_GROUP: u16 = 77;
    /// `Ceiling_RaiseByValueTimes8` — tag, speed, height.
    pub const CEILING_RAISE_BY_VALUE_TIMES8: u16 = 198;
    /// `Ceiling_LowerByValueTimes8` — tag, speed, height.
    pub const CEILING_LOWER_BY_VALUE_TIMES8: u16 = 199;
    /// `Plat_DownWaitUpStayLip` — tag, speed, delay, lip.
    pub const PLAT_DOWN_WAIT_UP_STAY_LIP: u16 = 206;
    /// `Plat_PerpetualRaiseLip` — tag, speed, delay, lip.
    pub const PLAT_PERPETUAL_RAISE_LIP: u16 = 207;
    /// `Floor_Waggle` — tag, amp, freq, delay, time.
    pub const FLOOR_WAGGLE: u16 = 138;
    /// `Ceiling_Waggle` — tag, amp, freq, delay, time.
    pub const CEILING_WAGGLE: u16 = 38;
    /// `TeleportInSector` — tag, source_tid, dest_tid, fog, group_tid.
    pub const TELEPORT_IN_SECTOR: u16 = 78;
    /// `Ceiling_CrushRaiseAndStay` — tag, speed, crush, crushtype.
    pub const CEILING_CRUSH_RAISE_AND_STAY: u16 = 45;
    /// `Ceiling_CrushRaiseAndStayA` — tag, dnspeed, upspeed, damage, crushtype.
    pub const CEILING_CRUSH_RAISE_AND_STAY_A: u16 = 195;
    /// `Ceiling_CrushRaiseAndStaySilA` — tag, dnspeed, upspeed, damage, crushtype.
    pub const CEILING_CRUSH_RAISE_AND_STAY_SIL_A: u16 = 255;
    /// `Teleport_NoStop` — tid, tag, nosourcefog.
    pub const TELEPORT_NO_STOP: u16 = 154;
    /// `Plat_UpByValueStayTx` — raise by value×8, copy trigger floor tex, clear special.
    pub const PLAT_UP_BY_VALUE_STAY_TX: u16 = 230;
    /// `Generic_Floor` — Boom generalized floor (tag, speed, height, target, flags).
    pub const GENERIC_FLOOR: u16 = 200;
    /// `Generic_Ceiling` — Boom generalized ceiling (tag, speed, height, target, flags).
    pub const GENERIC_CEILING: u16 = 201;
    /// `Generic_Door` — Boom generalized door (tag, speed, kind, delay, lock).
    pub const GENERIC_DOOR: u16 = 202;
    /// `Generic_Lift` — Boom generalized lift (tag, speed, delay, type, height).
    pub const GENERIC_LIFT: u16 = 203;
    /// `Generic_Stairs` — Boom generalized stairs (tag, speed, step, flags, reset).
    pub const GENERIC_STAIRS: u16 = 204;
    /// `Generic_Crusher` — Boom generalized crusher (tag, dspeed, uspeed, silent, crush).
    pub const GENERIC_CRUSHER: u16 = 205;
    /// `Transfer_Heights` — Boom fake floor/ceiling from control sector.
    pub const TRANSFER_HEIGHTS: u16 = 209;
    /// `Transfer_FloorLight`.
    pub const TRANSFER_FLOOR_LIGHT: u16 = 210;
    /// `Transfer_CeilingLight`.
    pub const TRANSFER_CEILING_LIGHT: u16 = 211;
    /// `Sector_SetWind`.
    pub const SECTOR_SET_WIND: u16 = 218;
    /// `Sector_SetFriction`.
    pub const SECTOR_SET_FRICTION: u16 = 219;
    /// `Sector_SetCurrent`.
    pub const SECTOR_SET_CURRENT: u16 = 220;
    /// `PointPush_SetForce`.
    pub const POINT_PUSH_SET_FORCE: u16 = 227;
    /// `Polyobj_RotateLeft`.
    pub const POLYOBJ_ROTATE_LEFT: u16 = 2;
    /// `Polyobj_RotateRight`.
    pub const POLYOBJ_ROTATE_RIGHT: u16 = 3;
    /// `Polyobj_Move`.
    pub const POLYOBJ_MOVE: u16 = 4;
    /// `Polyobj_MoveTimes8`.
    pub const POLYOBJ_MOVE_TIMES8: u16 = 6;
    /// `Polyobj_DoorSwing`.
    pub const POLYOBJ_DOOR_SWING: u16 = 7;
    /// `Polyobj_DoorSlide`.
    pub const POLYOBJ_DOOR_SLIDE: u16 = 8;
    /// `Polyobj_Stop`.
    pub const POLYOBJ_STOP: u16 = 87;
    /// `Polyobj_OR_RotateLeft`.
    pub const POLYOBJ_OR_ROTATE_LEFT: u16 = 90;
    /// `Polyobj_OR_RotateRight`.
    pub const POLYOBJ_OR_ROTATE_RIGHT: u16 = 91;
    /// `Polyobj_OR_Move`.
    pub const POLYOBJ_OR_MOVE: u16 = 92;
    /// `Polyobj_OR_MoveTimes8`.
    pub const POLYOBJ_OR_MOVE_TIMES8: u16 = 93;
    /// `Polyobj_MoveToSpot`.
    pub const POLYOBJ_MOVE_TO_SPOT: u16 = 86;
    /// `Polyobj_MoveTo`.
    pub const POLYOBJ_MOVE_TO: u16 = 88;
    /// `Polyobj_OR_MoveTo`.
    pub const POLYOBJ_OR_MOVE_TO: u16 = 89;
    /// `Polyobj_OR_MoveToSpot`.
    pub const POLYOBJ_OR_MOVE_TO_SPOT: u16 = 59;
    /// `Teleport_NewMap` — map, pos, face.
    pub const TELEPORT_NEW_MAP: u16 = 74;
    /// `Teleport_EndGame`.
    pub const TELEPORT_END_GAME: u16 = 75;
    /// `Radius_Quake` — intensity, duration, damrad, tremrad, tid.
    pub const RADIUS_QUAKE: u16 = 120;
    /// `ThrustThing` — angle, force, nolimit, tid.
    pub const THRUST_THING: u16 = 72;
    /// `ACS_Execute` — script, map, s_arg1, s_arg2, s_arg3.
    pub const ACS_EXECUTE: u16 = 80;
    /// `ACS_Suspend` — script, map.
    pub const ACS_SUSPEND: u16 = 81;
    /// `ACS_Terminate` — script, map.
    pub const ACS_TERMINATE: u16 = 82;
    /// `ACS_LockedExecute` — script, map, s_arg1, s_arg2, lock (key check later).
    pub const ACS_LOCKED_EXECUTE: u16 = 83;
    /// `ACS_ExecuteWithResult` — script, s_arg1, s_arg2, s_arg3, s_arg4.
    pub const ACS_EXECUTE_WITH_RESULT: u16 = 84;
    /// `ACS_LockedExecuteDoor` — script, map, s_arg1, s_arg2, lock.
    pub const ACS_LOCKED_EXECUTE_DOOR: u16 = 85;
    /// `ACS_ExecuteAlways` — script, map, s_arg1, s_arg2, s_arg3.
    pub const ACS_EXECUTE_ALWAYS: u16 = 226;
    /// `Thing_ProjectileIntercept` — tid, type, speed, target, newtid.
    pub const THING_PROJECTILE_INTERCEPT: u16 = 175;
    /// `Thing_ProjectileAimed` — tid, type, speed, target, newtid.
    pub const THING_PROJECTILE_AIMED: u16 = 178;
    /// `Sector_SetPlaneReflection` — tag, floor, ceiling (0–255).
    pub const SECTOR_SET_PLANE_REFLECTION: u16 = 159;
    /// `Line_SetHealth` — id, health.
    pub const LINE_SET_HEALTH: u16 = 150;
    /// `Sector_SetHealth` — tag, part, health.
    pub const SECTOR_SET_HEALTH: u16 = 151;
    /// `Sector_SetFloorGlow` — tag, height, r, g, b.
    pub const SECTOR_SET_FLOOR_GLOW: u16 = 277;
    /// `Sector_SetCeilingGlow` — tag, height, r, g, b.
    pub const SECTOR_SET_CEILING_GLOW: u16 = 278;
    /// `Floor_Stop` — tag.
    pub const FLOOR_STOP: u16 = 275;
    /// `Ceiling_Stop` — tag.
    pub const CEILING_STOP: u16 = 276;
    /// `Ceiling_CrushAndRaiseDist` — tag, dist, speed, damage, crushtype.
    pub const CEILING_CRUSH_AND_RAISE_DIST: u16 = 168;
    /// `Plat_UpNearestWaitDownStay` — tag, speed, delay.
    pub const PLAT_UP_NEAREST_WAIT_DOWN_STAY: u16 = 172;
    /// `Line_SetAutomapFlags` — id, setflags, clearflags.
    pub const LINE_SET_AUTOMAP_FLAGS: u16 = 281;
    /// `Line_SetAutomapStyle` — id, style.
    pub const LINE_SET_AUTOMAP_STYLE: u16 = 282;
    /// `Pillar_Build` — tag, speed, height.
    pub const PILLAR_BUILD: u16 = 29;
    /// `Pillar_Open` — tag, speed, fdist, cdist.
    pub const PILLAR_OPEN: u16 = 30;
    /// `Pillar_BuildAndCrush` — tag, speed, height, crush.
    pub const PILLAR_BUILD_AND_CRUSH: u16 = 94;
    /// `Thing_Activate` — tid.
    pub const THING_ACTIVATE: u16 = 130;
    /// `Thing_Deactivate` — tid.
    pub const THING_DEACTIVATE: u16 = 131;
    /// `Thing_Remove` — tid.
    pub const THING_REMOVE: u16 = 132;
    /// `Thing_Destroy` — tid, extreme, zone_tid.
    pub const THING_DESTROY: u16 = 133;
    /// `Thing_Projectile` — tid, type, angle, speed, vspeed.
    pub const THING_PROJECTILE: u16 = 134;
    /// `Thing_Spawn` — tid, type, angle, newtid.
    pub const THING_SPAWN: u16 = 135;
    /// `Thing_ProjectileGravity` — tid, type, angle, speed, vspeed.
    pub const THING_PROJECTILE_GRAVITY: u16 = 136;
    /// `Thing_SpawnNoFog` — tid, type, angle, newtid.
    pub const THING_SPAWN_NO_FOG: u16 = 137;
    /// `Thing_SpawnFacing` — tid, type, nofog, newtid.
    pub const THING_SPAWN_FACING: u16 = 139;
    /// `Sector_ChangeSound` — tag, sound.
    pub const SECTOR_CHANGE_SOUND: u16 = 140;
    /// `ChangeCamera` — tid, who, revert.
    pub const CHANGE_CAMERA: u16 = 237;
    /// `Scroll_Wall` — id, x, y, side, flags.
    pub const SCROLL_WALL: u16 = 52;
    /// `SetGlobalFogParameter` — type, value.
    pub const SET_GLOBAL_FOG_PARAMETER: u16 = 157;
    /// `Sector_SetCeilingScale2` — tag, x-factor, y-factor (fixed).
    pub const SECTOR_SET_CEILING_SCALE2: u16 = 170;
    /// `Sector_SetFloorScale2` — tag, x-factor, y-factor (fixed).
    pub const SECTOR_SET_FLOOR_SCALE2: u16 = 171;
    /// `Thing_SetGoal` — tid, goal, delay, chasegoal.
    pub const THING_SET_GOAL: u16 = 229;
    /// `Thing_ChangeTID` — oldtid, newtid.
    pub const THING_CHANGE_TID: u16 = 176;
    /// `ChangeSkill` — skill (0–4 → Doom 1–5).
    pub const CHANGE_SKILL: u16 = 179;
    /// `StartConversation` — talker tid, face_talker.
    pub const START_CONVERSATION: u16 = 18;
    /// `Thing_Stop` — tid.
    pub const THING_STOP: u16 = 19;
    /// `HealThing` — amount, max.
    pub const HEAL_THING: u16 = 248;
    /// `Thing_Hate` — hater, hatee, type.
    pub const THING_HATE: u16 = 177;
    /// `Thing_SetSpecial` — tid, special, arg1, arg2, arg3.
    pub const THING_SET_SPECIAL: u16 = 127;
    /// `Thing_SetConversation` — tid, conversationid.
    pub const THING_SET_CONVERSATION: u16 = 79;
    /// `ThrustThingZ` — tid, speed, up/down, add/set.
    pub const THRUST_THING_Z: u16 = 128;
    /// `Autosave`.
    pub const AUTOSAVE: u16 = 15;
    /// `Thing_Raise` — tid.
    pub const THING_RAISE: u16 = 17;
    /// `Line_SetTextureOffset` — lineid, x, y, side, flags.
    pub const LINE_SET_TEXTURE_OFFSET: u16 = 53;
    /// `Line_SetTextureScale` — lineid, x, y, side, flags.
    pub const LINE_SET_TEXTURE_SCALE: u16 = 56;
    /// `SetPlayerProperty` — who, set, which.
    pub const SET_PLAYER_PROPERTY: u16 = 191;
    /// `Sector_SetCeilingPanning` — tag, u-int, u-frac, v-int, v-frac.
    pub const SECTOR_SET_CEILING_PANNING: u16 = 186;
    /// `Sector_SetFloorPanning` — tag, u-int, u-frac, v-int, v-frac.
    pub const SECTOR_SET_FLOOR_PANNING: u16 = 187;
    /// `Sector_SetCeilingScale` — tag, x-int, x-frac, y-int, y-frac.
    pub const SECTOR_SET_CEILING_SCALE: u16 = 188;
    /// `Sector_SetFloorScale` — tag, x-int, x-frac, y-int, y-frac.
    pub const SECTOR_SET_FLOOR_SCALE: u16 = 189;
    /// `Sector_SetRotation` — tag, floor-angle, ceiling-angle (degrees).
    pub const SECTOR_SET_ROTATION: u16 = 185;
    /// `DamageThing` — damage [, mod].
    pub const DAMAGE_THING: u16 = 73;
    /// `Thing_Damage` — tid, amount, mod.
    pub const THING_DAMAGE: u16 = 119;
    /// `Thing_Move` — tid, mapspot, nofog.
    pub const THING_MOVE: u16 = 125;
    /// `Sector_ChangeFlags` — tag, set, clear.
    pub const SECTOR_CHANGE_FLAGS: u16 = 54;
    /// `GlassBreak` — nojunk [, junk].
    pub const GLASS_BREAK: u16 = 49;
    /// `Thing_SetTranslation` — tid, range.
    pub const THING_SET_TRANSLATION: u16 = 180;
    /// `Sector_SetColor` — tag, r, g, b, desaturate.
    pub const SECTOR_SET_COLOR: u16 = 212;
    /// `Sector_SetFade` — tag, r, g, b.
    pub const SECTOR_SET_FADE: u16 = 213;
    /// `Sector_SetDamage` — tag, amount, mod, interval, leaky.
    pub const SECTOR_SET_DAMAGE: u16 = 214;
    /// `Sector_SetGravity` — tag, intpart, fracpart.
    pub const SECTOR_SET_GRAVITY: u16 = 216;
}

/// Boom point push/pull source (`DPusher::p_push`).
#[derive(Clone, Debug)]
pub struct PointForce {
    /// Source X.
    pub x: f64,
    /// Source Y.
    pub y: f64,
    /// Full magnitude (force reaches 0 at `2 * magnitude` distance).
    pub magnitude: f64,
    /// `true` = pull toward source (`PointPuller`); `false` = push away.
    pub pull: bool,
}

/// Doom teleport landing thing type.
const THING_TELEPORT_DEST: i16 = 14;

/// How a line was activated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationType {
    /// Player crossed.
    Cross,
    /// Player used.
    Use,
    /// Impact (projectile).
    Impact,
    /// Push.
    Push,
    /// Destructible line damaged (`SPAC_Damage`).
    Damage,
    /// Destructible line destroyed (`SPAC_Death`).
    Death,
}

/// Floor plane mover (`T_MoveFloor` subset).
#[derive(Clone, Debug)]
pub struct FloorMover {
    /// Target sector.
    pub sector: SectorId,
    /// Direction: 1 up, -1 down.
    pub direction: i8,
    /// Speed (map units / tic).
    pub speed: f64,
    /// Destination floor height.
    pub dest_z: f64,
    /// Deal crush damage while raising into actors (`Floor_RaiseAndCrush*`).
    pub crush: bool,
}

/// Platform mover (`T_PlatRaise` DWUS / perpetual subset).
#[derive(Clone, Debug)]
pub struct PlatMover {
    /// Target sector.
    pub sector: SectorId,
    /// -1 down, 0 wait, 1 up.
    pub status: i8,
    /// Speed.
    pub speed: f64,
    /// Low floor height.
    pub low: f64,
    /// High (original) floor height.
    pub high: f64,
    /// Wait countdown at bottom.
    pub wait: i32,
    /// Wait duration.
    pub wait_max: i32,
    /// Keep cycling after reaching high.
    pub perpetual: bool,
    /// When true, arriving at `low` while descending finishes the plat (up→wait→down).
    pub stop_at_low: bool,
}

/// Strobe light thinker (`T_StrobeFlash` subset).
#[derive(Clone, Debug)]
pub struct LightStrobe {
    /// Target sector.
    pub sector: SectorId,
    /// Bright level.
    pub max_light: i16,
    /// Dim level.
    pub min_light: i16,
    /// Countdown until next toggle.
    pub count: i32,
    /// Duration at bright.
    pub bright_time: i32,
    /// Duration at dim.
    pub dark_time: i32,
    /// Currently bright.
    pub bright: bool,
}

/// Gradual light fade (`T_LightFade` / `Light_Fade`).
#[derive(Clone, Debug)]
pub struct LightFade {
    /// Target sector.
    pub sector: SectorId,
    /// Destination light 0–255.
    pub dest: i16,
    /// Remaining tics.
    pub tics: i32,
    /// Per-tic delta (can be fractional via accumulator).
    pub step: f64,
    /// Current light as f64 for smooth steps.
    pub current: f64,
}

/// Random fire / blink flicker (`T_FireFlicker` / `T_LightFlash` subset).
#[derive(Clone, Debug)]
pub struct LightFlicker {
    /// Target sector.
    pub sector: SectorId,
    /// Bright level.
    pub max_light: i16,
    /// Dim floor.
    pub min_light: i16,
    /// Countdown until next change.
    pub count: i32,
    /// `true` = fire flicker (every 4 tics); `false` = random blink flash.
    pub fire: bool,
    /// Currently at bright (blink mode only).
    pub bright: bool,
    /// Doom `lightflash_t.mintime` (mask while dim); unused for fire.
    pub min_time: i32,
    /// Doom `lightflash_t.maxtime` (mask while bright); unused for fire.
    pub max_time: i32,
}

/// Oscillating glow light (`T_Glow` / `Light_Glow`).
#[derive(Clone, Debug)]
pub struct LightGlow {
    /// Target sector.
    pub sector: SectorId,
    /// Upper light.
    pub upper: i16,
    /// Lower light.
    pub lower: i16,
    /// Half-cycle length in tics.
    pub tics_max: i32,
    /// Countdown within half-cycle.
    pub tics: i32,
    /// Direction: 1 toward upper, -1 toward lower.
    pub dir: i8,
    /// Current light as f64.
    pub current: f64,
    /// Step per tic toward the current target.
    pub step: f64,
}

/// Permanent floor scroll / carry (`Scroll_Floor` subset).
#[derive(Clone, Debug)]
pub struct FloorScroller {
    /// Target sector.
    pub sector: SectorId,
    /// Carry velocity X (map units / tic).
    pub dx: f64,
    /// Carry velocity Y.
    pub dy: f64,
    /// Accumulated U offset (texture sync).
    pub u: f64,
    /// Accumulated V offset.
    pub v: f64,
}

/// Walkable rotating floor (`user_spin` / `user_spinx` / `user_spiny`).
#[derive(Clone, Debug)]
pub struct FloorSpinner {
    /// Target sector.
    pub sector: SectorId,
    /// Pivot X (map units).
    pub cx: f64,
    /// Pivot Y (map units).
    pub cy: f64,
    /// Radians per tic (signed).
    pub omega: f64,
}

/// Locator-following subway / train car (`user_subway`).
#[derive(Clone, Debug)]
pub struct FloorSubway {
    /// Car sector (boarding).
    pub sector: SectorId,
    /// Waypoints in visit order.
    pub points: Vec<(f64, f64)>,
    /// Index of the waypoint the car is moving toward.
    pub i: usize,
    /// Virtual car X.
    pub x: f64,
    /// Virtual car Y.
    pub y: f64,
    /// Map units per tic.
    pub speed: f64,
    /// Moving-wall polyobject, when the car converted as a slab.
    pub poly: Option<i32>,
}

/// Duke `SE 20` stretch / shrink (`user_stretch`).
#[derive(Clone, Debug)]
pub struct FloorStretch {
    /// Sector whose vertices move.
    pub sector: SectorId,
    /// Linedefs that bound the sector (blockmap refresh).
    pub lines: Vec<LineId>,
    /// Unique vertices of those lines.
    pub verts: Vec<VertexId>,
    /// Unit direction X.
    pub dx: f64,
    /// Unit direction Y.
    pub dy: f64,
    /// Travel limit (map units).
    pub dist: f64,
    /// Map units per tic.
    pub speed: f64,
    /// Distance travelled from rest (0..=dist).
    pub pos: f64,
    /// +1 stretching, −1 shrinking, 0 idle.
    pub sign: i8,
}

/// Duke `SE 22` teeth prongs (`user_teeth`) that follow a lotag-29 door.
#[derive(Clone, Debug)]
pub struct FloorTeeth {
    /// Sector whose vertices retract.
    pub sector: SectorId,
    /// Lotag-29 door whose ceiling drives the prongs.
    pub door: SectorId,
    /// Linedefs that bound the sector (blockmap refresh).
    pub lines: Vec<LineId>,
    /// Unique vertices of those lines.
    pub verts: Vec<VertexId>,
    /// Unit direction X.
    pub dx: f64,
    /// Unit direction Y.
    pub dy: f64,
    /// Travel limit (map units).
    pub dist: f64,
    /// Map units per tic.
    pub speed: f64,
    /// Distance travelled from rest (0..=dist).
    pub pos: f64,
    /// Door ceiling height when the map started (closed).
    pub rest_ceil: f64,
}

/// Duke `SE 16` reactor skew (`user_skew`): vertices oscillate along an axis.
#[derive(Clone, Debug)]
pub struct FloorSkew {
    /// Sector whose vertices shear.
    pub sector: SectorId,
    /// Linedefs that bound the sector (blockmap refresh).
    pub lines: Vec<LineId>,
    /// Unique vertices of those lines.
    pub verts: Vec<VertexId>,
    /// Unit direction X.
    pub dx: f64,
    /// Unit direction Y.
    pub dy: f64,
    /// Travel limit (map units).
    pub dist: f64,
    /// Map units per tic.
    pub speed: f64,
    /// Distance travelled from rest (0..=dist).
    pub pos: f64,
    /// +1 or −1; reverses at each end.
    pub sign: i8,
}

/// Ceiling flat scroller (`Scroll_Ceiling` — texture only, no carry).
#[derive(Clone, Debug)]
pub struct CeilingScroller {
    /// Target sector.
    pub sector: SectorId,
    /// Scroll velocity X (map units / tic).
    pub dx: f64,
    /// Scroll velocity Y.
    pub dy: f64,
    /// Accumulated U offset.
    pub u: f64,
    /// Accumulated V offset.
    pub v: f64,
}

/// Wall texture scroller (`Scroll_Texture_*` subset).
#[derive(Clone, Debug)]
pub struct TextureScroller {
    /// Target sidedef.
    pub side: SideId,
    /// Offset delta X (texels / tic).
    pub dx: f64,
    /// Offset delta Y (texels / tic).
    pub dy: f64,
    /// Accumulated X (fractional).
    pub x: f64,
    /// Accumulated Y (fractional).
    pub y: f64,
}

/// Ceiling plane mover / crusher (`T_MoveCeiling` subset).
#[derive(Clone, Debug)]
pub struct CeilingMover {
    /// Target sector.
    pub sector: SectorId,
    /// -1 down, 1 up, 0 stopped.
    pub direction: i8,
    /// Speed while lowering.
    pub speed: f64,
    /// Speed while raising (defaults to `speed` for non-generic movers).
    pub speed_up: f64,
    /// Bottom (dest when lowering).
    pub bottom_z: f64,
    /// Top (original / raise target).
    pub top_z: f64,
    /// Crusher: reverse at bottom instead of removing.
    pub crush: bool,
    /// One-shot lower (no raise).
    pub once: bool,
    /// Crusher that raises once then stops at top (`Ceiling_CrushRaiseAndStay*`).
    pub stop_at_high: bool,
}

/// Plane waggle thinker (`T_FloorWaggle` / `T_CeilingWaggle` subset).
#[derive(Clone, Debug)]
pub struct PlaneWaggle {
    /// Target sector.
    pub sector: SectorId,
    /// `true` = floor, `false` = ceiling.
    pub floor: bool,
    /// Rest height when amp×sin = 0.
    pub base_z: f64,
    /// Peak displacement (map units).
    pub amplitude: f64,
    /// Phase advance per tic (radians).
    pub freq: f64,
    /// Current phase.
    pub phase: f64,
    /// Tics before motion starts.
    pub delay: i32,
    /// Remaining tics (`0` = forever).
    pub time_left: i32,
}

/// Ceiling door thinker (raise → wait → close, or close → wait → open).
#[derive(Clone, Debug)]
pub struct DoorMover {
    /// Target sector.
    pub sector: SectorId,
    /// Direction: 1 up, -1 down, 0 wait.
    pub direction: i8,
    /// Speed (map units / tic).
    pub speed: f64,
    /// Top height to reach.
    pub top_z: f64,
    /// Bottom (floor) height.
    pub bottom_z: f64,
    /// Countdown while waiting at top.
    pub wait: i32,
    /// Initial wait at top (`VDOORWAIT` = 150).
    pub wait_max: i32,
    /// Close → wait → open (otherwise raise → wait → close).
    pub close_wait_open: bool,
    /// Initial wait then raise (`Door_WaitRaise`).
    pub wait_then_raise: bool,
}

/// Elevator mover (floor + ceiling together).
#[derive(Clone, Debug)]
pub struct ElevatorMover {
    /// Target sector.
    pub sector: SectorId,
    /// Direction: 1 up, -1 down.
    pub direction: i8,
    /// Speed.
    pub speed: f64,
    /// Destination floor Z.
    pub dest_floor: f64,
    /// Ceiling offset from floor (preserved height).
    pub height: f64,
}

/// Execute a numbered special. Returns non-zero on success (GZDoom convention).
pub fn execute_special(
    world: &mut World,
    special: u16,
    args: &[i32; 5],
    activator: Option<ActorId>,
    line: Option<LineId>,
) -> i32 {
    match special {
        num::DOOR_RAISE | num::DOOR_ANIMATED => door_raise(world, args, line),
        num::DOOR_OPEN => door_open(world, args, line),
        num::DOOR_CLOSE | num::DOOR_ANIMATED_CLOSE => door_close(world, args, line),
        num::DOOR_LOCKED_RAISE => door_locked_raise(world, args, line, activator),
        num::DOOR_CLOSE_WAIT_OPEN => door_close_wait_open(world, args, line),
        num::DOOR_WAIT_RAISE => door_wait_raise(world, args, line),
        num::DOOR_WAIT_CLOSE => door_wait_close(world, args, line),
        num::ELEVATOR_RAISE_TO_NEAREST => elevator_move(world, args, line, 1),
        num::ELEVATOR_LOWER_TO_NEAREST => elevator_move(world, args, line, -1),
        num::ELEVATOR_MOVE_TO_FLOOR => elevator_move_to_floor(world, args, line),
        num::LIGHT_CHANGE_TO_VALUE => light_change(world, args),
        num::LIGHT_RAISE_BY_VALUE => light_adjust(world, args, 1),
        num::LIGHT_LOWER_BY_VALUE => light_adjust(world, args, -1),
        num::LIGHT_FADE => light_fade(world, args, line),
        num::LIGHT_GLOW => light_glow(world, args, line),
        num::LIGHT_FLICKER => light_flicker(world, args, line),
        num::LIGHT_STROBE => light_strobe(world, args, line),
        num::LIGHT_STOP => light_stop(world, args, line),
        num::LIGHT_STROBE_DOOM => light_strobe_doom(world, args, line),
        num::CEILING_RAISE_BY_VALUE => ceiling_by_value(world, args, line, 1),
        num::CEILING_LOWER_BY_VALUE => ceiling_by_value(world, args, line, -1),
        num::LIGHT_MAX_NEIGHBOR => light_neighbor(world, args, true),
        num::LIGHT_MIN_NEIGHBOR => light_neighbor(world, args, false),
        num::EXIT_NORMAL | num::EXIT_SECRET => {
            world.level.exiting = true;
            if !cfg!(test) {
                eprintln!(
                    "room: level exit: Exit_Normal/Exit_Secret special (tic {})",
                    world.level.time
                );
            }
            world.level.secret_exit = special == num::EXIT_SECRET;
            // Vanilla exit switch cue (`sfx_swtchx`).
            if let Some(mo) = activator {
                if let Some(a) = world.actors.get(mo) {
                    world.play_sound("switches/exitbutn", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
                }
            } else if let Some(lid) = line {
                if let Some(l) = world.map.lines.get(lid.raw() as usize) {
                    if let Some(v1) = world.map.vertices.get(l.v1.raw() as usize) {
                        world.play_sound("switches/exitbutn", v1.pos.x(), v1.pos.y(), 0.0, 1.0);
                    }
                }
            }
            1
        }
        num::FLOOR_LOWER_BY_VALUE => floor_lower_by_value(world, args, line, 1.0),
        num::FLOOR_RAISE_BY_VALUE => floor_raise_by_value(world, args, line),
        num::FLOOR_RAISE_BY_VALUE_TIMES8 => {
            let mut a = *args;
            a[2] = a[2].saturating_mul(8);
            floor_raise_by_value(world, &a, line)
        }
        num::FLOOR_LOWER_BY_VALUE_TIMES8 => floor_lower_by_value(world, args, line, 8.0),
        num::FLOOR_LOWER_INSTANT => floor_instant(world, args, line, -1),
        num::FLOOR_RAISE_INSTANT => floor_instant(world, args, line, 1),
        num::FLOOR_MOVE_TO_VALUE => floor_move_to_value(world, args, line, 1.0, false),
        num::FLOOR_MOVE_TO_VALUE_TIMES8 => floor_move_to_value(world, args, line, 8.0, false),
        num::FLOOR_MOVE_TO_VALUE_AND_CRUSH => floor_move_to_value(world, args, line, 1.0, true),
        num::CEILING_MOVE_TO_VALUE => ceiling_move_to_value(world, args, line, 1.0, false),
        num::CEILING_MOVE_TO_VALUE_TIMES8 => ceiling_move_to_value(world, args, line, 8.0, false),
        num::CEILING_MOVE_TO_VALUE_AND_CRUSH => ceiling_move_to_value(world, args, line, 1.0, true),
        num::FLOOR_RAISE_BY_VALUE_TX_TY => {
            let r = floor_raise_by_value(world, args, line);
            if r != 0 {
                for sid in sectors_for_tag(world, args[0], line) {
                    apply_tx0_change(world, sid, line);
                }
            }
            r
        }
        num::FLOOR_LOWER_TO_LOWEST_TX_TY => {
            let r = floor_lower_to_lowest(world, args, line);
            if r != 0 {
                for sid in sectors_for_tag(world, args[0], line) {
                    apply_tx0_change(world, sid, line);
                }
            }
            r
        }
        num::FLOOR_LOWER_BY_TEXTURE => floor_lower_by_texture(world, args, line),
        num::CEILING_RAISE_BY_TEXTURE => ceiling_by_texture(world, args, line, 1),
        num::CEILING_LOWER_BY_TEXTURE => ceiling_by_texture(world, args, line, -1),
        num::CEILING_LOWER_INSTANT => ceiling_instant(world, args, line, -1),
        num::CEILING_RAISE_INSTANT => ceiling_instant(world, args, line, 1),
        num::SCROLL_TEXTURE_BOTH => scroll_texture_both(world, args),
        num::FLOOR_LOWER_TO_LOWEST => floor_lower_to_lowest(world, args, line),
        num::FLOOR_LOWER_TO_HIGHEST | num::FLOOR_LOWER_TO_NEAREST => {
            floor_lower_by(world, args, line, 24.0)
        }
        num::FLOOR_RAISE_TO_NEAREST => floor_raise_to_nearest(world, args, line),
        num::FLOOR_RAISE_BY_TEXTURE => floor_raise_by_texture(world, args, line),
        num::FLOOR_RAISE_AND_CRUSH | num::FLOOR_RAISE_AND_CRUSH_DOOM => {
            floor_raise_and_crush(world, args, line)
        }
        num::FLOOR_RAISE_TO_LOWEST_CEILING => floor_raise_to_lowest_ceiling(world, args, line),
        num::FLOOR_RAISE_TO_HIGHEST => floor_raise_to_highest(world, args, line),
        num::TELEPORT => teleport(world, args, activator, true, true, false),
        num::TELEPORT_NO_FOG => teleport(world, args, activator, false, false, false),
        num::TELEPORT_NO_STOP => teleport(world, args, activator, args[2] == 0, true, true),
        num::TELEPORT_OTHER => teleport_other(world, args),
        num::TELEPORT_GROUP => teleport_group(world, args),
        num::TELEPORT_IN_SECTOR => teleport_in_sector(world, args),
        num::TELEPORT_LINE => teleport_line(world, args, activator, line),
        num::FLOOR_WAGGLE => start_waggle(world, args, line, true),
        num::CEILING_WAGGLE => start_waggle(world, args, line, false),
        num::CEILING_CRUSH_RAISE_AND_STAY => {
            let speed = hexen_speed(args[1]);
            start_crusher(world, args[0], speed, speed, 8.0, line, false, true)
        }
        num::CEILING_CRUSH_RAISE_AND_STAY_A => {
            let down = hexen_speed(args[1]);
            let up = if args[2] != 0 {
                hexen_speed(args[2])
            } else {
                down
            };
            start_crusher(world, args[0], down, up, 8.0, line, false, true)
        }
        num::CEILING_CRUSH_RAISE_AND_STAY_SIL_A => {
            let down = hexen_speed(args[1]);
            let up = if args[2] != 0 {
                hexen_speed(args[2])
            } else {
                down
            };
            start_crusher(world, args[0], down, up, 8.0, line, true, true)
        }
        // Doom `LIFT_LIP` = 8; lower target is lowest surrounding floor + lip.
        num::PLAT_DOWN_WAIT_UP_STAY => plat_dwus(world, args, line, false, 8.0),
        num::PLAT_PERPETUAL_RAISE => plat_dwus(world, args, line, true, 8.0),
        num::PLAT_DOWN_WAIT_UP_STAY_LIP => {
            // XLAT may pass lip `0` (Doom type 88) — that is intentional.
            let lip = f64::from(args[3]).max(0.0);
            plat_dwus(world, args, line, false, lip)
        }
        num::PLAT_PERPETUAL_RAISE_LIP => {
            let lip = f64::from(args[3]).max(0.0);
            plat_dwus(world, args, line, true, lip)
        }
        num::PLAT_DOWN_BY_VALUE => plat_down_by_value(world, args, line),
        num::PLAT_UP_WAIT_DOWN_STAY => plat_up_wait_down_stay(world, args, line),
        num::PLAT_UP_BY_VALUE => plat_up_by_value(world, args, line),
        num::PLAT_UP_NEAREST_WAIT_DOWN_STAY => plat_up_nearest_wait_down_stay(world, args, line),
        num::PLAT_STOP => plat_stop(world, args),
        num::CEILING_RAISE_BY_VALUE_TIMES8 => {
            let mut a = *args;
            a[2] = a[2].saturating_mul(8);
            ceiling_by_value(world, &a, line, 1)
        }
        num::CEILING_LOWER_BY_VALUE_TIMES8 => {
            let mut a = *args;
            a[2] = a[2].saturating_mul(8);
            ceiling_by_value(world, &a, line, -1)
        }
        num::LINE_SET_AUTOMAP_FLAGS => line_set_automap_flags(world, args),
        num::LINE_SET_AUTOMAP_STYLE => line_set_automap_style(world, args),
        num::CEILING_LOWER_TO_FLOOR => ceiling_lower_to_floor(world, args, line),
        num::CEILING_RAISE_TO_NEAREST => ceiling_raise_to_nearest(world, args, line),
        num::CEILING_LOWER_TO_LOWEST => ceiling_lower_to_lowest(world, args, line),
        num::CEILING_LOWER_TO_HIGHEST_FLOOR => ceiling_lower_to_highest_floor(world, args, line),
        num::FLOOR_AND_CEILING_LOWER_RAISE => floor_and_ceiling_lower_raise(world, args, line),
        num::FLOOR_AND_CEILING_LOWER_BY_VALUE => floor_and_ceiling_by_value(world, args, line, -1),
        num::FLOOR_AND_CEILING_RAISE_BY_VALUE => floor_and_ceiling_by_value(world, args, line, 1),
        num::PLAT_TOGGLE_CEILING => plat_toggle_ceiling(world, args, line),
        num::FLOOR_DONUT => floor_donut(world, args, line),
        num::FLOOR_CRUSH_STOP | num::FLOOR_STOP => floor_crush_stop(world, args),
        num::CEILING_CRUSH_AND_RAISE | num::CEILING_CRUSH_AND_RAISE_SILENT_DIST => {
            ceiling_crush_and_raise(world, args, line)
        }
        num::CEILING_CRUSH_AND_RAISE_DIST => ceiling_crush_and_raise_dist(world, args, line),
        num::CEILING_LOWER_AND_CRUSH => ceiling_lower_and_crush(world, args, line, 8.0),
        num::CEILING_LOWER_AND_CRUSH_DIST => {
            let dist = if args[3] != 0 {
                f64::from(args[3].abs())
            } else {
                8.0
            };
            ceiling_lower_and_crush(world, args, line, dist)
        }
        num::SECTOR_SET_TRANSLUCENT => sector_set_translucent(world, args, line),
        num::LINE_SET_BLOCKING => line_set_blocking(world, args),
        num::TRANSLUCENT_LINE => translucent_line(world, args, line),
        num::NOISE_ALERT => noise_alert_special(world, args, activator),
        num::CEILING_CRUSH_STOP | num::CEILING_STOP => ceiling_crush_stop(world, args),
        num::STAIRS_BUILD_UP_DOOM | num::STAIRS_BUILD_UP => stairs_build_up(world, args, line),
        num::STAIRS_BUILD_UP_DOOM_CRUSH => {
            let mut a = *args;
            a[3] = 1;
            stairs_build_up(world, &a, line)
        }
        num::STAIRS_BUILD_DOWN | num::STAIRS_BUILD_DOWN_DOOM => {
            stairs_build_dir(world, args, line, false, false)
        }
        num::STAIRS_BUILD_UP_SYNC | num::STAIRS_BUILD_UP_DOOM_SYNC => {
            stairs_build_dir(world, args, line, true, true)
        }
        num::STAIRS_BUILD_DOWN_SYNC | num::STAIRS_BUILD_DOWN_DOOM_SYNC => {
            stairs_build_dir(world, args, line, false, true)
        }
        num::FLOOR_LOWER_TO_HIGHEST_EE => floor_lower_to_highest_ee(world, args, line),
        num::FLOOR_RAISE_TO_LOWEST => floor_raise_to_lowest(world, args, line),
        num::FLOOR_LOWER_TO_LOWEST_CEILING => floor_lower_to_lowest_ceiling(world, args, line),
        num::FLOOR_RAISE_TO_CEILING => floor_raise_to_ceiling(world, args, line, false),
        num::FLOOR_TO_CEILING_INSTANT => floor_raise_to_ceiling(world, args, line, true),
        num::CEILING_RAISE_TO_HIGHEST => ceiling_to_neighbor(world, args, line, 1, true, false),
        num::CEILING_TO_HIGHEST_INSTANT => ceiling_to_neighbor(world, args, line, 1, true, true),
        num::CEILING_LOWER_TO_NEAREST => ceiling_to_neighbor(world, args, line, -1, false, false),
        num::CEILING_RAISE_TO_LOWEST => ceiling_raise_to_lowest(world, args, line),
        num::CEILING_RAISE_TO_HIGHEST_FLOOR => ceiling_raise_to_highest_floor(world, args, line),
        num::CEILING_TO_FLOOR_INSTANT => ceiling_to_floor_instant(world, args, line),
        num::CEILING_CRUSH_AND_RAISE_A => {
            let down = hexen_speed(args[1]);
            let up = if args[2] != 0 {
                hexen_speed(args[2])
            } else {
                down
            };
            start_crusher(world, args[0], down, up, 8.0, line, false, false)
        }
        num::CEILING_CRUSH_AND_RAISE_SILENT_A => {
            let down = hexen_speed(args[1]);
            let up = if args[2] != 0 {
                hexen_speed(args[2])
            } else {
                down
            };
            start_crusher(world, args[0], down, up, 8.0, line, true, false)
        }
        num::GENERIC_CRUSHER2 => {
            let down = hexen_speed(args[1]);
            let up = if args[2] != 0 {
                hexen_speed(args[2])
            } else {
                down
            };
            start_crusher(world, args[0], down, up, 8.0, line, args[3] != 0, false)
        }
        num::SCROLL_TEXTURE_LEFT => scroll_texture(world, args, line, -1.0, 0.0),
        num::SCROLL_TEXTURE_RIGHT => scroll_texture(world, args, line, 1.0, 0.0),
        num::SCROLL_TEXTURE_UP => scroll_texture(world, args, line, 0.0, -1.0),
        num::SCROLL_TEXTURE_DOWN => scroll_texture(world, args, line, 0.0, 1.0),
        num::SCROLL_TEXTURE_OFFSETS => scroll_texture_offsets(world, args, line),
        num::SCROLL_TEXTURE_MODEL => scroll_texture_model(world, args),
        num::SCROLL_FLOOR => scroll_floor(world, args, line),
        num::SECTOR_STRETCH => toggle_stretch(world, args, line),
        num::SCROLL_CEILING => scroll_ceiling(world, args, line),
        num::SECTOR_COPY_SCROLLER => sector_copy_scroller(world, args, line),
        num::LINE_HORIZON => line_horizon(world, line),
        num::TRANSFER_WALL_LIGHT => transfer_wall_light(world, args, line),
        num::LIGHT_FORCE_LIGHTNING => light_force_lightning(world, args),
        num::PLANE_COPY => plane_copy(world, args, line),
        num::LINE_SET_IDENTIFICATION => line_set_identification(world, args, line),
        num::TRANSFER_HEIGHTS => transfer_heights(world, args, line),
        num::TRANSFER_FLOOR_LIGHT => transfer_floor_light(world, args, line),
        num::TRANSFER_CEILING_LIGHT => transfer_ceiling_light(world, args, line),
        num::SECTOR_SET_FRICTION => sector_set_friction(world, args, line),
        num::SECTOR_SET_WIND => sector_set_pusher(world, args, line, false),
        num::SECTOR_SET_CURRENT => sector_set_pusher(world, args, line, true),
        num::POINT_PUSH_SET_FORCE => point_push_set_force(world, args, line),
        num::PLAT_RAISE_AND_STAY_TX0 => plat_raise_and_stay_tx0(world, args, line),
        num::PLAT_UP_BY_VALUE_STAY_TX => plat_up_by_value_stay_tx(world, args, line),
        num::FLOOR_TRANSFER_TRIGGER => floor_transfer_trigger(world, args, line),
        num::FLOOR_TRANSFER_NUMERIC => floor_transfer_numeric(world, args, line),
        num::GENERIC_FLOOR => generic_floor(world, args, line),
        num::GENERIC_CEILING => generic_ceiling(world, args, line),
        num::GENERIC_DOOR => generic_door(world, args, line, activator),
        num::GENERIC_LIFT => generic_lift(world, args, line),
        num::GENERIC_STAIRS => generic_stairs(world, args, line),
        num::GENERIC_CRUSHER => generic_crusher(world, args, line),
        num::POLYOBJ_MOVE => crate::polyobj::polyobj_move(world, args),
        num::POLYOBJ_MOVE_TIMES8 => crate::polyobj::polyobj_move_times8(world, args),
        num::POLYOBJ_ROTATE_LEFT => crate::polyobj::polyobj_rotate(world, args, true),
        num::POLYOBJ_ROTATE_RIGHT => crate::polyobj::polyobj_rotate(world, args, false),
        num::POLYOBJ_STOP | num::POLYOBJ_STOP_SOUND => crate::polyobj::polyobj_stop(world, args),
        num::POLYOBJ_DOOR_SLIDE => crate::polyobj::polyobj_door_slide(world, args),
        num::POLYOBJ_DOOR_SWING => crate::polyobj::polyobj_door_swing(world, args),
        num::POLYOBJ_OR_ROTATE_LEFT => crate::polyobj::polyobj_or_rotate(world, args, true),
        num::POLYOBJ_OR_ROTATE_RIGHT => crate::polyobj::polyobj_or_rotate(world, args, false),
        num::POLYOBJ_OR_MOVE => crate::polyobj::polyobj_or_move(world, args),
        num::POLYOBJ_OR_MOVE_TIMES8 => crate::polyobj::polyobj_or_move_times8(world, args),
        num::POLYOBJ_MOVE_TO => crate::polyobj::polyobj_move_to(world, args, false),
        num::POLYOBJ_OR_MOVE_TO => crate::polyobj::polyobj_move_to(world, args, true),
        num::POLYOBJ_MOVE_TO_SPOT => crate::polyobj::polyobj_move_to_spot(world, args, false),
        num::POLYOBJ_OR_MOVE_TO_SPOT => crate::polyobj::polyobj_move_to_spot(world, args, true),
        num::TELEPORT_NEW_MAP => teleport_new_map(world, args),
        num::TELEPORT_END_GAME => teleport_end_game(world),
        num::RADIUS_QUAKE => radius_quake(world, args, activator),
        num::THRUST_THING => thrust_thing(world, args, activator),
        num::THRUST_THING_Z => thrust_thing_z(world, args, activator),
        num::AUTOSAVE => {
            world.autosave_request = true;
            1
        }
        num::ACS_EXECUTE | num::ACS_LOCKED_EXECUTE | num::ACS_LOCKED_EXECUTE_DOOR => {
            crate::acs::acs_execute(world, args, false, activator, line)
        }
        num::ACS_EXECUTE_ALWAYS | num::ACS_EXECUTE_WITH_RESULT => {
            crate::acs::acs_execute(world, args, true, activator, line)
        }
        num::ACS_SUSPEND => crate::acs::acs_suspend(world, args),
        num::ACS_TERMINATE => crate::acs::acs_terminate(world, args),
        num::PILLAR_BUILD => pillar_build(world, args, line, false),
        num::PILLAR_BUILD_AND_CRUSH => pillar_build(world, args, line, true),
        num::PILLAR_OPEN => pillar_open(world, args, line),
        num::THING_ACTIVATE => thing_set_dormant(world, args, false),
        num::THING_DEACTIVATE => thing_set_dormant(world, args, true),
        num::THING_REMOVE => thing_remove(world, args),
        num::THING_DESTROY => thing_destroy(world, args),
        num::THING_SPAWN => thing_spawn(world, args, true),
        num::THING_SPAWN_NO_FOG => thing_spawn(world, args, false),
        num::THING_SPAWN_FACING => thing_spawn_facing(world, args),
        num::SECTOR_CHANGE_SOUND => sector_change_sound(world, args),
        num::CHANGE_CAMERA => change_camera(world, args, activator),
        num::SCROLL_WALL => scroll_wall(world, args),
        num::SET_GLOBAL_FOG_PARAMETER => set_global_fog_parameter(world, args),
        num::SECTOR_SET_FLOOR_SCALE2 => sector_set_scale2(world, args, line, true),
        num::SECTOR_SET_CEILING_SCALE2 => sector_set_scale2(world, args, line, false),
        num::THING_SET_GOAL => thing_set_goal(world, args),
        num::THING_PROJECTILE => thing_projectile(world, args, false, activator),
        num::THING_PROJECTILE_GRAVITY => thing_projectile(world, args, true, activator),
        num::THING_PROJECTILE_AIMED | num::THING_PROJECTILE_INTERCEPT => {
            thing_projectile_aimed(world, args)
        }
        num::SECTOR_SET_PLANE_REFLECTION => sector_set_plane_reflection(world, args, line),
        num::LINE_SET_HEALTH => line_set_health(world, args),
        num::SECTOR_SET_HEALTH => sector_set_health(world, args, line),
        num::SECTOR_SET_FLOOR_GLOW => sector_set_glow(world, args, line, true),
        num::SECTOR_SET_CEILING_GLOW => sector_set_glow(world, args, line, false),
        num::THING_CHANGE_TID => thing_change_tid(world, args, activator),
        num::CHANGE_SKILL => change_skill(world, args),
        num::THING_STOP => thing_stop(world, args, activator),
        num::HEAL_THING => heal_thing(world, args, activator),
        num::THING_HATE => thing_hate(world, args, activator),
        num::THING_SET_SPECIAL => thing_set_special(world, args),
        num::THING_SET_CONVERSATION => thing_set_conversation(world, args, activator),
        num::START_CONVERSATION => start_conversation(world, args, activator),
        num::THING_RAISE => thing_raise(world, args),
        num::LINE_SET_TEXTURE_OFFSET => line_set_texture_offset(world, args),
        num::LINE_SET_TEXTURE_SCALE => line_set_texture_scale(world, args),
        num::SET_PLAYER_PROPERTY => set_player_property(world, args, activator),
        num::SECTOR_SET_FLOOR_PANNING => sector_set_panning(world, args, line, true),
        num::SECTOR_SET_CEILING_PANNING => sector_set_panning(world, args, line, false),
        num::SECTOR_SET_FLOOR_SCALE => sector_set_scale(world, args, line, true),
        num::SECTOR_SET_CEILING_SCALE => sector_set_scale(world, args, line, false),
        num::SECTOR_SET_ROTATION => sector_set_rotation(world, args, line),
        num::DAMAGE_THING => damage_thing(world, args, activator),
        num::THING_DAMAGE => thing_damage(world, args),
        num::THING_MOVE => thing_move(world, args, activator),
        num::SECTOR_CHANGE_FLAGS => sector_change_flags(world, args),
        num::GLASS_BREAK => glass_break(world, line),
        num::THING_SET_TRANSLATION => thing_set_translation(world, args, activator),
        num::SECTOR_SET_COLOR => sector_set_color(world, args, line),
        num::SECTOR_SET_FADE => sector_set_fade(world, args, line),
        num::SECTOR_SET_DAMAGE => sector_set_damage(world, args, line),
        num::SECTOR_SET_GRAVITY => sector_set_gravity(world, args, line),
        _ => 0,
    }
}

/// `Teleport_NewMap(map, pos, face)` — exit to Doom2-style `MAPxx` (Hexen map numbers later).
fn teleport_new_map(world: &mut World, args: &[i32; 5]) -> i32 {
    let map = args[0].clamp(1, 99);
    world.level.exiting = true;
    if !cfg!(test) {
        eprintln!(
            "room: level exit: Teleport_NewMap (tic {})",
            world.level.time
        );
    }
    world.level.secret_exit = false;
    world.level.next_map_override = Some(format!("MAP{map:02}"));
    let _ = (args[1], args[2]); // pos / face — hub start spot later
    1
}

/// `Teleport_EndGame` — leave the episode (no next map).
fn teleport_end_game(world: &mut World) -> i32 {
    world.level.exiting = true;
    if !cfg!(test) {
        eprintln!(
            "room: level exit: Teleport_EndGame (tic {})",
            world.level.time
        );
    }
    world.level.secret_exit = false;
    // Empty override = no next map (consumed by the game layer).
    world.level.next_map_override = Some(String::new());
    1
}

/// `Radius_Quake(intensity, duration, damrad, tremrad, tid)`.
fn radius_quake(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let intensity = f64::from(args[0]).clamp(0.0, 9.0);
    let duration = args[1].max(0);
    let damage_radius = f64::from(args[2]).abs();
    let tremor_radius = f64::from(args[3]).abs();
    let tid = args[4];
    if duration == 0 {
        return 0;
    }
    let spot = if tid != 0 {
        world.find_by_tid(tid)
    } else {
        activator
    };
    let Some(spot) = spot else {
        return 0;
    };
    world.quakes.push(crate::world::Quake {
        spot,
        intensity,
        countdown: duration,
        damage_radius,
        tremor_radius,
        sound: "world/quake".into(),
        thrust_multiplier: 0.5,
    });
    1
}

/// `Pillar_Build` / `Pillar_BuildAndCrush` — floor up + ceiling down.
fn pillar_build(world: &mut World, args: &[i32; 5], line: Option<LineId>, crush: bool) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let height = f64::from(args[2]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let gap = sec.ceiling_z - sec.floor_z;
        if gap <= 0.0 {
            continue;
        }
        let (floor_dest, ceil_dest) = if height <= 0.0 {
            let mid = (sec.floor_z + sec.ceiling_z) * 0.5;
            (mid, mid)
        } else {
            let rise = height.min(gap);
            (sec.floor_z + rise, sec.ceiling_z - rise)
        };
        if floor_dest > sec.floor_z {
            world.floors.push(FloorMover {
                sector: sid,
                direction: 1,
                speed,
                dest_z: floor_dest,
                crush,
            });
        }
        if ceil_dest < sec.ceiling_z {
            world.ceilings.push(CeilingMover {
                sector: sid,
                direction: -1,
                speed,
                speed_up: speed,
                bottom_z: ceil_dest,
                top_z: sec.ceiling_z,
                crush,
                once: true,
                stop_at_high: false,
            });
        }
        started += 1;
    }
    i32::from(started > 0)
}

/// `Pillar_Open` — floor down by `fdist`, ceiling up by `cdist`.
fn pillar_open(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let fdist = f64::from(args[2]);
    let cdist = f64::from(args[3]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        if fdist > 0.0 {
            world.floors.push(FloorMover {
                sector: sid,
                direction: -1,
                speed,
                dest_z: sec.floor_z - fdist,
                crush: false,
            });
        }
        if cdist > 0.0 {
            let top = sec.ceiling_z + cdist;
            world.ceilings.push(CeilingMover {
                sector: sid,
                direction: 1,
                speed,
                speed_up: speed,
                bottom_z: sec.ceiling_z,
                top_z: top,
                crush: false,
                once: true,
                stop_at_high: false,
            });
        }
        started += 1;
    }
    i32::from(started > 0)
}

fn actors_with_tid(world: &World, tid: i32) -> Vec<ActorId> {
    if tid == 0 {
        return Vec::new();
    }
    world
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            world
                .actors
                .get(id)
                .is_some_and(|a| !a.destroyed && a.tid == tid)
        })
        .collect()
}

fn thing_set_dormant(world: &mut World, args: &[i32; 5], dormant: bool) -> i32 {
    // `SoundEnvironment` things live in the zone state, not as actors.
    let envs = world.activate_sound_environments(args[0], !dormant);
    let ids = actors_with_tid(world, args[0]);
    if ids.is_empty() {
        return i32::from(envs);
    }
    for id in ids {
        if let Some(a) = world.actors.get_mut(id) {
            if dormant {
                a.flags |= crate::class::FLAG_DORMANT;
            } else {
                a.flags &= !crate::class::FLAG_DORMANT;
            }
        }
    }
    1
}

fn thing_remove(world: &mut World, args: &[i32; 5]) -> i32 {
    let ids = actors_with_tid(world, args[0]);
    if ids.is_empty() {
        return 0;
    }
    for id in ids {
        if let Some(a) = world.actors.get_mut(id) {
            a.destroyed = true;
        }
    }
    1
}

/// `Thing_ChangeTID(oldtid, newtid)` — retag actors (0 = activator).
fn thing_change_tid(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let old = args[0];
    let new_tid = args[1];
    let ids = if old == 0 {
        activator.into_iter().collect::<Vec<_>>()
    } else {
        actors_with_tid(world, old)
    };
    if ids.is_empty() {
        return 0;
    }
    for id in ids {
        if let Some(a) = world.actors.get_mut(id) {
            a.tid = new_tid;
        }
        if new_tid == 261 {
            let actor = world.actors.get(id);
            let class = actor
                .and_then(|a| world.classes.get(a.class))
                .map_or("<missing>", |c| c.name.as_str());
            eprintln!(
                "room: VEHICLE TID tic={} change old={} new={} actor=#{} class={} hp={}",
                world.level.time,
                old,
                new_tid,
                id.index(),
                class,
                actor.map_or(0, |a| a.health),
            );
        }
    }
    1
}

/// `ChangeSkill(skill)` — ACS/Hexen skill index 0–4 → Doom skill 1–5.
fn change_skill(world: &mut World, args: &[i32; 5]) -> i32 {
    let skill = (args[0].clamp(0, 4) + 1) as u8;
    world.skill = skill;
    world.fast_monsters = skill >= 5;
    // Do not auto-enable respawn; Nightmare maps set that separately.
    1
}

/// `Thing_Stop(tid)` — zero velocity (0 = activator).
fn thing_stop(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let ids = if args[0] == 0 {
        activator.into_iter().collect::<Vec<_>>()
    } else {
        actors_with_tid(world, args[0])
    };
    if ids.is_empty() {
        return 0;
    }
    for id in ids {
        if let Some(a) = world.actors.get_mut(id) {
            a.vel = DVec3::ZERO;
        }
    }
    1
}

/// `HealThing(amount, max)` — heal activator; `max` 0 = class spawn health.
fn heal_thing(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let Some(id) = activator else {
        return 0;
    };
    let amount = args[0].max(0);
    if amount == 0 {
        return 0;
    }
    let Some(a) = world.actors.get(id) else {
        return 0;
    };
    let class_hp = world
        .classes
        .get(a.class)
        .map(|c| c.health)
        .unwrap_or(a.health);
    let cap = if args[1] > 0 {
        args[1]
    } else {
        class_hp.max(a.health)
    };
    let new_hp = (a.health + amount).min(cap);
    if new_hp <= a.health {
        return 0;
    }
    if let Some(a) = world.actors.get_mut(id) {
        a.health = new_hp;
    }
    1
}

/// `Thing_Hate(hater, hatee, type)` — set chase target (`hatee` 0 = activator).
///
/// `type` 0 = always retarget; 1 = only if hater has no target.
fn thing_hate(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let haters = actors_with_tid(world, args[0]);
    if haters.is_empty() {
        return 0;
    }
    let hatees: Vec<ActorId> = if args[1] == 0 {
        activator.into_iter().collect()
    } else {
        actors_with_tid(world, args[1])
    };
    let Some(&hatee) = hatees.first() else {
        return 0;
    };
    let only_if_none = args[2] != 0;
    let mut n = 0;
    for id in haters {
        if id == hatee {
            continue;
        }
        let Some(a) = world.actors.get_mut(id) else {
            continue;
        };
        if only_if_none && a.target.is_some() {
            continue;
        }
        a.target = Some(hatee);
        a.flags &= !crate::class::FLAG_FRIENDLY;
        n += 1;
    }
    i32::from(n > 0)
}

/// `Thing_SetSpecial(tid, special, arg1, arg2, arg3)`.
fn thing_set_special(world: &mut World, args: &[i32; 5]) -> i32 {
    let ids = actors_with_tid(world, args[0]);
    if ids.is_empty() {
        return 0;
    }
    let special = args[1].clamp(0, i32::from(u16::MAX)) as u16;
    for id in ids {
        if let Some(a) = world.actors.get_mut(id) {
            a.special = special;
            a.args = [args[2], args[3], args[4], 0, 0];
        }
    }
    1
}

/// `Thing_SetConversation(tid, conversationid)` — set dialogue ID (`tid` 0 = activator).
fn thing_set_conversation(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let ids = if args[0] == 0 {
        activator.into_iter().collect::<Vec<_>>()
    } else {
        actors_with_tid(world, args[0])
    };
    if ids.is_empty() {
        return 0;
    }
    let conv = args[1].max(0);
    for id in ids {
        if let Some(a) = world.actors.get_mut(id) {
            a.conversation = conv;
        }
    }
    1
}

/// `StartConversation(tid, facetalker)` — queue dialogue with first living talker.
fn start_conversation(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let Some(player) = activator else {
        return 0;
    };
    let is_player = world.players.iter().flatten().any(|p| p.mo == Some(player));
    let alive = world
        .actors
        .get(player)
        .is_some_and(|a| !a.destroyed && a.health > 0);
    if !is_player || !alive {
        return 0;
    }
    let Some(talker) = actors_with_tid(world, args[0]).into_iter().next() else {
        return 0;
    };
    if talker == player {
        return 0;
    }
    let face = args[1] != 0;
    if face {
        if let (Some(p), Some(t)) = (world.actors.get(player), world.actors.get(talker)) {
            let dx = t.pos.x() - p.pos.x();
            let dy = t.pos.y() - p.pos.y();
            let ang = Angle::from_radians(room_core::dmath::atan2(dy, dx));
            if let Some(a) = world.actors.get_mut(player) {
                a.angle = ang;
            }
        }
    }
    world.conversation_request = Some((talker, face));
    1
}

/// `Thing_Raise(tid)` — revive corpses that have a `Raise` state (Arch-vile path).
fn thing_raise(world: &mut World, args: &[i32; 5]) -> i32 {
    let ids = actors_with_tid(world, args[0]);
    if ids.is_empty() {
        return 0;
    }
    let mut n = 0;
    for id in ids {
        let Some(a) = world.actors.get(id).cloned() else {
            continue;
        };
        if a.destroyed || (a.health > 0 && (a.flags & crate::class::FLAG_CORPSE) == 0) {
            continue;
        }
        let Some(raise) = world.states.find_label(a.class, "Raise") else {
            continue;
        };
        let spawn_hp = world
            .classes
            .get(a.class)
            .map(|c| c.health.max(1))
            .unwrap_or(1);
        let flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        if (flags & crate::class::FLAG_COUNTKILL) != 0 {
            world.level.found_kills = world.level.found_kills.saturating_sub(1);
        }
        if let Some(m) = world.actors.get_mut(id) {
            m.health = spawn_hp;
            m.destroyed = false;
            m.flags = flags;
            m.vel = DVec3::ZERO;
            m.reaction_time = 0;
        }
        world.dequeue_corpse(id);
        world.set_state(id, Some(raise), true);
        n += 1;
    }
    i32::from(n > 0)
}

fn thing_destroy(world: &mut World, args: &[i32; 5]) -> i32 {
    let tid = args[0];
    let extreme = args[1] != 0;
    let ids = actors_with_tid(world, tid);
    if ids.is_empty() {
        return 0;
    }
    for id in ids {
        let hp = world.actors.get(id).map(|a| a.health).unwrap_or(0);
        let dmg = if extreme { hp.max(1000) } else { hp.max(1) };
        crate::damage::damage_mobj(world, id, None, None, dmg);
    }
    1
}

/// Resolve Hexen spawn `type` — bootstrap: treat as DoomEdNum (`spawnnums` later).
/// GZDoom `SpawnableThings`: specials name classes by `SpawnID`; Room falls
/// back to the DoomEdNum for maps written against older ports.
pub(crate) fn class_for_spawn_type(world: &World, type_id: i32) -> Option<room_core::ClassId> {
    if type_id > 0 {
        if let Some(c) = world.classes.iter().find(|c| c.spawn_id == type_id) {
            return world
                .classes
                .find_name(&c.name)
                .map(|id| world.classes.replacement(id));
        }
    }
    world.classes.find_doomednum(type_id)
}

fn byte_angle(byte: i32) -> Angle {
    let rad = (byte.rem_euclid(256) as f64) * (std::f64::consts::TAU / 256.0);
    Angle::from_radians(rad)
}

/// `Thing_Spawn` / `Thing_SpawnNoFog` — tid, type, angle, newtid.
fn thing_spawn(world: &mut World, args: &[i32; 5], fog: bool) -> i32 {
    let spot_tid = args[0];
    let type_id = args[1];
    let angle = byte_angle(args[2]);
    let newtid = args[3];
    let Some(class) = class_for_spawn_type(world, type_id) else {
        return 0;
    };
    let spots = actors_with_tid(world, spot_tid);
    if spots.is_empty() {
        // Fall back to map things with matching tid (not yet spawned as actors).
        let map_spots: Vec<(f64, f64, f64)> = world
            .map
            .things
            .iter()
            .filter(|t| i32::from(t.tid) == spot_tid)
            .map(|t| {
                let z = world.sectors.first().map(|s| s.floor_z).unwrap_or(0.0);
                (f64::from(t.x), f64::from(t.y), z)
            })
            .collect();
        if map_spots.is_empty() {
            return 0;
        }
        let mut n = 0;
        for (x, y, z) in map_spots {
            let pos = DVec3::new(x, y, z);
            if let Ok(id) = world.spawn_at(class, pos, angle) {
                if let Some(a) = world.actors.get_mut(id) {
                    a.tid = newtid;
                }
                if fog {
                    spawn_teleport_fog(world, pos);
                }
                n = 1;
            }
        }
        return n;
    }
    let mut n = 0;
    for spot in spots {
        let Some(pos) = world.actors.get(spot).map(|a| a.pos) else {
            continue;
        };
        if let Ok(id) = world.spawn_at(class, pos, angle) {
            if let Some(a) = world.actors.get_mut(id) {
                a.tid = newtid;
            }
            if fog {
                spawn_teleport_fog(world, pos);
            }
            n = 1;
        }
    }
    n
}

/// `Thing_SpawnFacing(tid, type, nofog, newtid)` — spawn using spot angle.
fn thing_spawn_facing(world: &mut World, args: &[i32; 5]) -> i32 {
    let spot_tid = args[0];
    let type_id = args[1];
    let fog = args[2] == 0;
    let newtid = args[3];
    let Some(class) = class_for_spawn_type(world, type_id) else {
        return 0;
    };
    let spots = actors_with_tid(world, spot_tid);
    if spots.is_empty() {
        return 0;
    }
    let mut n = 0;
    for spot in spots {
        let Some((pos, angle)) = world.actors.get(spot).map(|a| (a.pos, a.angle)) else {
            continue;
        };
        if let Ok(id) = world.spawn_at(class, pos, angle) {
            if let Some(a) = world.actors.get_mut(id) {
                a.tid = newtid;
            }
            if fog {
                spawn_teleport_fog(world, pos);
            }
            n = 1;
        }
    }
    n
}

/// `Sector_ChangeSound(tag, sound)` — SNDSEQ type index per sector.
fn sector_change_sound(world: &mut World, args: &[i32; 5]) -> i32 {
    let tag = args[0];
    if tag == 0 {
        return 0;
    }
    let sectors = sectors_for_tag(world, tag, None);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        world.sector_seq_type.insert(sid.raw(), args[1]);
    }
    1
}

/// `ChangeCamera(tid, who, revert)` — `tid` 0 = player eye; `who` 0 = activator only.
fn change_camera(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let camera = if args[0] == 0 {
        None
    } else {
        world.find_by_tid(args[0])
    };
    let all = args[1] != 0;
    let revert = args[2] != 0;
    let slots: Vec<usize> = if all {
        (0..world.players.len())
            .filter(|&i| world.players[i].as_ref().is_some_and(|p| p.mo.is_some()))
            .collect()
    } else {
        let Some(mo) = activator else {
            return 0;
        };
        match world
            .players
            .iter()
            .position(|p| p.as_ref().is_some_and(|pl| pl.mo == Some(mo)))
        {
            Some(i) => vec![i],
            None => return 0,
        }
    };
    if slots.is_empty() {
        return 0;
    }
    for i in slots {
        if let Some(p) = world.players[i].as_mut() {
            p.camera = camera;
            p.camera_revert = camera.is_some() && revert;
        }
    }
    1
}

/// `Scroll_Wall(id, x, y, side, flags)` — fixed-point scroll rates on tagged lines.
fn scroll_wall(world: &mut World, args: &[i32; 5]) -> i32 {
    let line_id = args[0];
    if line_id == 0 {
        return 0;
    }
    let dx = Fixed(args[1]).to_f64();
    let dy = Fixed(args[2]).to_f64();
    let back = args[3] != 0;
    let mut n = 0;
    for ld in &world.map.lines {
        if i32::from(ld.tag) != line_id {
            continue;
        }
        let Some(sid) = (if back { ld.sides[1] } else { ld.sides[0] }) else {
            continue;
        };
        let base_x = world
            .map
            .sides
            .get(sid.raw() as usize)
            .map(|s| f64::from(s.x_offset))
            .unwrap_or(0.0);
        let base_y = world
            .map
            .sides
            .get(sid.raw() as usize)
            .map(|s| f64::from(s.y_offset))
            .unwrap_or(0.0);
        world.tex_scrollers.retain(|s| s.side != sid);
        world.tex_scrollers.push(TextureScroller {
            side: sid,
            dx,
            dy,
            x: base_x,
            y: base_y,
        });
        n += 1;
    }
    i32::from(n > 0)
}

/// `SetGlobalFogParameter(type, value)`.
fn set_global_fog_parameter(world: &mut World, args: &[i32; 5]) -> i32 {
    match args[0] {
        0 => {
            world.fog_density = (args[1] >> 1).clamp(0, 255) as f32 / 255.0;
            1
        }
        1 => {
            world.outside_fog_density = (args[1] >> 1).clamp(0, 255) as f32 / 255.0;
            1
        }
        2 => {
            world.sky_fog = args[1].max(0) as f32;
            1
        }
        _ => 0,
    }
}

/// `Sector_SetFloor/CeilingScale2(tag, x-factor, y-factor)` — fixed → reciprocal.
fn sector_set_scale2(world: &mut World, args: &[i32; 5], line: Option<LineId>, floor: bool) -> i32 {
    let sx = if args[1] != 0 {
        Some(1.0 / Fixed(args[1]).to_f64())
    } else {
        None
    };
    let sy = if args[2] != 0 {
        Some(1.0 / Fixed(args[2]).to_f64())
    } else {
        None
    };
    if sx.is_none() && sy.is_none() {
        return 0;
    }
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let i = sid.raw();
        let map = if floor {
            &mut world.floor_scales
        } else {
            &mut world.ceiling_scales
        };
        let (cx, cy) = map.get(&i).copied().unwrap_or((1.0, 1.0));
        map.insert(i, (sx.unwrap_or(cx), sy.unwrap_or(cy)));
    }
    1
}

/// `Thing_SetGoal(tid, goal, delay, chasegoal)` — goal tid of patrol spot.
fn thing_set_goal(world: &mut World, args: &[i32; 5]) -> i32 {
    let movers = actors_with_tid(world, args[0]);
    if movers.is_empty() {
        return 0;
    }
    let goal = if args[1] == 0 {
        None
    } else {
        world.find_by_tid(args[1])
    };
    let delay = args[2].saturating_mul(35);
    let chase = args[3] != 0;
    let mut n = 0;
    for id in movers {
        let Some(a) = world.actors.get_mut(id) else {
            continue;
        };
        if (a.flags & crate::class::FLAG_SHOOTABLE) == 0 {
            continue;
        }
        if a.target.is_some() && a.target == a.goal {
            a.target = None;
        }
        a.goal = goal;
        a.chase_goal = chase;
        if a.target.is_none() {
            a.reaction_time = delay;
        }
        n += 1;
    }
    i32::from(n > 0)
}

/// `Thing_Projectile` / `Thing_ProjectileGravity` — tid, type, angle, speed, vspeed.
fn thing_projectile(
    world: &mut World,
    args: &[i32; 5],
    gravity: bool,
    activator: Option<ActorId>,
) -> i32 {
    let Some(class) = class_for_spawn_type(world, args[1]) else {
        return 0;
    };
    // GZDoom `SPEED(a)` = a / 8.
    spawn_projectiles_at(
        world,
        args[0],
        class,
        args[2],
        f64::from(args[3]) / 8.0,
        f64::from(args[4]) / 8.0,
        gravity,
        0,
        activator,
    )
}

/// GZDoom `EV_Thing_Projectile` (no aiming): spawn `class` at every actor with
/// `tid` (the activator when `tid` is 0), angle in byte units, speeds in map
/// units per tic. Returns 1 when anything was spawned.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_projectiles_at(
    world: &mut World,
    tid: i32,
    class: room_core::ClassId,
    angle_byte: i32,
    speed: f64,
    vspeed: f64,
    gravity: bool,
    newtid: i32,
    activator: Option<ActorId>,
) -> i32 {
    let spots = if tid == 0 {
        activator.into_iter().collect()
    } else {
        actors_with_tid(world, tid)
    };
    let angle = byte_angle(angle_byte);
    let yaw = angle.radians();
    let (dx, dy) = (yaw.cos() * speed, yaw.sin() * speed);
    let mut n = 0;
    for spot in spots {
        let Some(pos) = world.actors.get(spot).map(|a| a.pos) else {
            continue;
        };
        if let Ok(id) = world.spawn_at(class, pos, angle) {
            if let Some(a) = world.actors.get_mut(id) {
                a.vel = DVec3::new(dx, dy, vspeed);
                a.flags |= crate::class::FLAG_MISSILE;
                if gravity {
                    a.flags &= !crate::class::FLAG_NOGRAVITY;
                } else {
                    a.flags |= crate::class::FLAG_NOGRAVITY;
                }
                if newtid != 0 {
                    a.tid = newtid;
                }
                a.target = Some(spot);
            }
            n = 1;
        }
    }
    n
}

/// `Thing_ProjectileAimed/Intercept(tid, type, speed, target, newtid)`.
fn thing_projectile_aimed(world: &mut World, args: &[i32; 5]) -> i32 {
    let spot_tid = args[0];
    let type_id = args[1];
    let speed = f64::from(args[2]).max(0.0);
    let target_tid = args[3];
    let newtid = args[4];
    let Some(class) = class_for_spawn_type(world, type_id) else {
        return 0;
    };
    let spots = actors_with_tid(world, spot_tid);
    if spots.is_empty() {
        return 0;
    }
    let target_pos = if target_tid != 0 {
        world
            .find_by_tid(target_tid)
            .and_then(|id| world.actors.get(id).map(|a| a.pos))
    } else {
        None
    };
    let mut n = 0;
    for spot in spots {
        let Some(pos) = world.actors.get(spot).map(|a| a.pos) else {
            continue;
        };
        let (angle, vel) = if let Some(tp) = target_pos {
            let dx = tp.x() - pos.x();
            let dy = tp.y() - pos.y();
            let dz = tp.z() - pos.z();
            let len = room_core::dmath::sqrt(dx * dx + dy * dy + dz * dz).max(1e-6);
            let ang = Angle::from_radians(room_core::dmath::atan2(dy, dx));
            (
                ang,
                DVec3::new(dx / len * speed, dy / len * speed, dz / len * speed),
            )
        } else {
            let ang = world
                .actors
                .get(spot)
                .map(|a| a.angle)
                .unwrap_or(Angle::ZERO);
            let yaw = ang.radians();
            (
                ang,
                DVec3::new(
                    room_core::dmath::cos(yaw) * speed,
                    room_core::dmath::sin(yaw) * speed,
                    0.0,
                ),
            )
        };
        let aim = if target_tid != 0 {
            world.find_by_tid(target_tid)
        } else {
            None
        };
        if let Ok(id) = world.spawn_at(class, pos, angle) {
            if let Some(a) = world.actors.get_mut(id) {
                a.vel = vel;
                a.tid = newtid;
                a.flags |= crate::class::FLAG_MISSILE | crate::class::FLAG_NOGRAVITY;
                a.target = aim;
            }
            n = 1;
        }
    }
    n
}

/// `Sector_SetPlaneReflection(tag, floor, ceiling)` — 0–255 → 0..=1.
fn sector_set_plane_reflection(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let floor_r = (args[1].clamp(0, 255) as f32) / 255.0;
    let ceil_r = (args[2].clamp(0, 255) as f32) / 255.0;
    for sid in sectors {
        world.plane_reflection.insert(sid.raw(), (floor_r, ceil_r));
    }
    1
}

/// `Line_SetHealth(id, health)`.
fn line_set_health(world: &mut World, args: &[i32; 5]) -> i32 {
    let line_id = args[0];
    let health = args[1].max(0);
    let mut n = 0;
    let map = std::sync::Arc::make_mut(&mut world.map);
    for (i, ld) in map.lines.iter_mut().enumerate() {
        let matches = if line_id == 0 {
            i == 0
        } else {
            ld.has_id(line_id)
        };
        if !matches {
            continue;
        }
        ld.health = health;
        world.line_health.insert(i as u32, health);
        n += 1;
    }
    i32::from(n > 0)
}

/// `Sector_SetHealth(tag, part, health)` — part: 0 floor, 1 ceiling, 2 3D.
fn sector_set_health(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let part = args[1];
    let health = args[2].max(0);
    for sid in sectors {
        if let Some(s) = world.sectors.get_mut(sid.raw() as usize) {
            match part {
                1 => s.health_ceiling = health,
                2 => s.health_3d = health,
                _ => s.health_floor = health,
            }
        }
    }
    1
}

/// `Sector_SetFloor/CeilingGlow(tag, height, r, g, b)`.
fn sector_set_glow(world: &mut World, args: &[i32; 5], line: Option<LineId>, floor: bool) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let height = args[1];
    let glow = if height < 0 {
        None
    } else {
        Some(crate::world::SectorPlaneGlow {
            color: [
                args[2].clamp(0, 255) as u8,
                args[3].clamp(0, 255) as u8,
                args[4].clamp(0, 255) as u8,
            ],
            height: height as f32,
        })
    };
    for sid in sectors {
        if let Some(s) = world.sectors.get_mut(sid.raw() as usize) {
            if floor {
                s.glow_floor = glow;
            } else {
                s.glow_ceiling = glow;
            }
        }
    }
    1
}

/// `ThrustThing(angle, force, nolimit, tid)` — horizontal thrust (byte angle).
fn thrust_thing(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let angle_byte = args[0];
    let force = f64::from(args[1]);
    let _nolimit = args[2] != 0;
    let tid = args[3];
    if force == 0.0 {
        return 0;
    }
    let rad = (angle_byte.rem_euclid(256) as f64) * (std::f64::consts::TAU / 256.0);
    let (dx, dy) = (rad.cos() * force, rad.sin() * force);
    let targets: Vec<ActorId> = if tid != 0 {
        world
            .thinkers
            .default
            .iter()
            .copied()
            .filter(|&id| {
                world
                    .actors
                    .get(id)
                    .is_some_and(|a| !a.destroyed && a.tid == tid)
            })
            .collect()
    } else if let Some(a) = activator {
        vec![a]
    } else {
        return 0;
    };
    let mut n = 0;
    for id in targets {
        if let Some(a) = world.actors.get_mut(id) {
            a.vel = room_core::DVec3::new(a.vel.x() + dx, a.vel.y() + dy, a.vel.z());
            n = 1;
        }
    }
    n
}

/// `ThrustThingZ(tid, speed, up/down, add/set)` — vertical thrust (`speed` in 1/4 mu/tic).
fn thrust_thing_z(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let tid = args[0];
    let force = f64::from(args[1]) * 0.25;
    if force == 0.0 {
        return 0;
    }
    let down = args[2] != 0;
    let set = args[3] != 0;
    let dz = if down { -force } else { force };
    let targets: Vec<ActorId> = if tid != 0 {
        actors_with_tid(world, tid)
    } else if let Some(a) = activator {
        vec![a]
    } else {
        return 0;
    };
    if targets.is_empty() {
        return 0;
    }
    for id in targets {
        if let Some(a) = world.actors.get_mut(id) {
            let z = if set { dz } else { a.vel.z() + dz };
            a.vel = DVec3::new(a.vel.x(), a.vel.y(), z);
        }
    }
    1
}

/// Hexen/ZDoom mover speed: special args are eighths of a map-unit per tic
/// (`SPEED(a)` = `a * FRACUNIT/8` in GZDoom `p_lnspec.cpp`). Doom `F_SLOW`/`FLOORSPEED`
/// is arg `8` → 1 mu/tic; `D_SLOW` is `16` → 2 mu/tic.
fn hexen_speed_or(arg: i32, default_hexen: i32) -> f64 {
    let a = if arg != 0 { arg } else { default_hexen };
    f64::from(a) / 8.0
}

fn hexen_speed(arg: i32) -> f64 {
    hexen_speed_or(arg, 8)
}

/// Activate a linedef's special for `activation`.
pub fn activate_line(
    world: &mut World,
    line_id: LineId,
    activator: Option<ActorId>,
    activation: ActivationType,
) -> bool {
    let special = world.line_special(line_id) as u16;
    if special == 0 {
        return false;
    }
    if world.map.lines.get(line_id.raw() as usize).is_none() {
        return false;
    }
    if let Some(id) = activator {
        if world
            .actors
            .get(id)
            .is_some_and(|a| (a.flags & crate::class::FLAG_NOTRIGGER) != 0)
        {
            return false;
        }
    }
    let mask = world.line_activation(line_id);
    let flags = world.line_flags(line_id);
    let args = world.line_args(line_id);
    let allowed = match activation {
        ActivationType::Cross => room_map::spac::allows_cross(mask),
        ActivationType::Use => room_map::spac::allows_use(mask),
        ActivationType::Impact => room_map::spac::allows_impact(mask),
        ActivationType::Push => room_map::spac::allows_push(mask),
        ActivationType::Damage => room_map::spac::allows_damage(mask),
        ActivationType::Death => room_map::spac::allows_death(mask),
    };
    if !allowed {
        return false;
    }
    // Doom `P_UseSpecialLine`: non-players may only *use* manual doors (not
    // walk-specials like teleport). Chase currently approximates spechit via
    // `use_lines`; without this whitelist monsters "use" distant teleporters.
    if activation == ActivationType::Use {
        let is_player =
            activator.is_some_and(|id| world.players.iter().flatten().any(|p| p.mo == Some(id)));
        // ZDoom `ML_MONSTERSCANACTIVATE` (UDMF `monsteractivate`) widens it.
        if !is_player && !monster_can_use_special(special) && (flags & 0x2000) == 0 {
            return false;
        }
    }
    // UDMF `firstsideonly` / `checkswitchrange` (Use and Push).
    if matches!(activation, ActivationType::Use | ActivationType::Push) {
        if let Some(id) = activator {
            let (first_only, check_range) = world
                .map
                .lines
                .get(line_id.raw() as usize)
                .map(|l| (l.first_side_only, l.check_switch_range))
                .unwrap_or((false, false));
            if first_only && !actor_on_front_side(world, line_id, id) {
                return false;
            }
            if activation == ActivationType::Use
                && check_range
                && !switch_range_reachable(world, line_id, id)
            {
                return false;
            }
        }
    }
    // `Plane_Align` (181) is consumed at load (GZDoom `P_SetSlopes` zeroes the
    // line special), so it never fires when crossed or used.
    if special == 181 {
        return false;
    }
    // Doom-format maps already translated into Hexen numbers at load for
    // some paths; raw Doom specials may still be present — treat as Hexen id.
    let ok = execute_special(world, special, &args, activator, Some(line_id)) != 0;
    if !cfg!(test) {
        let act = match activation {
            ActivationType::Use => "USE",
            ActivationType::Cross => "WALK",
            ActivationType::Impact => "SHOOT",
            ActivationType::Push => "PUSH",
            ActivationType::Damage => "DAMAGE",
            ActivationType::Death => "DEATH",
        };
        eprintln!(
            "room: SPECIAL @{tic} {act} line={} special={special} args={args:?} ok={ok}",
            line_id.raw(),
            tic = world.level.time,
        );
    }
    if ok && activation == ActivationType::Use {
        flip_switch_texture(world, line_id);
    }
    // One-shot: clear special when not `ML_REPEAT_SPECIAL` (Doom / Boom W1/S1/G1).
    // Destructible Damage/Death keep the special unless Death cleared it below.
    if ok
        && (flags & crate::world::ML_REPEAT_SPECIAL) == 0
        && !matches!(activation, ActivationType::Damage | ActivationType::Death)
    {
        world.line_specials.insert(line_id.raw(), 0);
    }
    ok
}

/// Apply damage to a destructible sector plane (`healthfloor` / `healthceiling`).
///
/// `part`: `0` floor, `1` ceiling, `2` 3D. Returns remaining health (`0` if none).
pub fn damage_sector_plane(world: &mut World, sector: SectorId, part: i32, damage: i32) -> i32 {
    if damage <= 0 {
        return 0;
    }
    let Some(s) = world.sectors.get_mut(sector.raw() as usize) else {
        return 0;
    };
    let hp = match part {
        1 => &mut s.health_ceiling,
        2 => &mut s.health_3d,
        _ => &mut s.health_floor,
    };
    if *hp <= 0 {
        return 0;
    }
    *hp = (*hp - damage).max(0);
    // Keep static map fields in sync for GetSectorUDMFInt bootstrap.
    if let Some(ss) = std::sync::Arc::make_mut(&mut world.map)
        .sectors
        .get_mut(sector.raw() as usize)
    {
        match part {
            1 => ss.health_ceiling = *hp,
            2 => ss.health_3d = *hp,
            _ => ss.health_floor = *hp,
        }
    }
    *hp
}

/// Apply damage to a destructible linedef (`P_DamageLinedef` subset).
///
/// Returns remaining health after the hit (`0` if destroyed or not destructible).
pub fn damage_linedef(
    world: &mut World,
    line_id: LineId,
    source: Option<ActorId>,
    damage: i32,
) -> i32 {
    if damage <= 0 {
        return world.line_health.get(&line_id.raw()).copied().unwrap_or(0);
    }
    let idx = line_id.raw() as usize;
    let Some(line) = world.map.lines.get(idx) else {
        return 0;
    };
    let group = line.health_group;
    let mut hp = world
        .line_health
        .get(&line_id.raw())
        .copied()
        .unwrap_or(line.health);
    if hp <= 0 {
        return 0;
    }
    let before = hp;
    hp = (hp - damage).max(0);
    // Sync healthgroup peers.
    let targets: Vec<u32> = if group != 0 {
        world
            .map
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.health_group == group)
            .map(|(i, _)| i as u32)
            .collect()
    } else {
        vec![line_id.raw()]
    };
    for ti in &targets {
        world.line_health.insert(*ti, hp);
        if let Some(l) = std::sync::Arc::make_mut(&mut world.map)
            .lines
            .get_mut(*ti as usize)
        {
            l.health = hp;
        }
    }
    // Activate while health was > 0 before this hit (GZDoom SPAC_Damage).
    if before > 0 {
        for &ti in &targets {
            let _ = activate_line(world, LineId::from_raw(ti), source, ActivationType::Damage);
        }
    }
    if hp == 0 {
        for &ti in &targets {
            clear_dead_health_line(world, ti);
            if let Some(bag) = std::sync::Arc::make_mut(&mut world.map)
                .udmf_user
                .lines
                .get_mut(ti as usize)
            {
                bag.ints.remove("user_contactdamage");
            }
            let _ = activate_line(world, LineId::from_raw(ti), source, ActivationType::Death);
        }
    }
    hp
}

/// A dead health wall stays in the BSP but stops blocking and drawing its mid.
fn clear_dead_health_line(world: &mut World, ti: u32) {
    const CLEAR: i32 = crate::world::ML_BLOCKING
        | crate::world::ML_BLOCKMONSTERS
        | crate::world::ML_BLOCK_PLAYERS
        | crate::world::ML_BLOCKEVERYTHING
        | crate::world::ML_BLOCKHITSCAN;
    let map = std::sync::Arc::make_mut(&mut world.map);
    let (sides, flags) = {
        let Some(l) = map.lines.get_mut(ti as usize) else {
            return;
        };
        l.block_projectiles = false;
        l.block_hitscan = false;
        l.block_sight = false;
        l.midtex3d = false;
        l.midtex3d_impassible = false;
        l.flags &= !(CLEAR as i16);
        (l.sides, crate::world::base_line_flags(l) & !CLEAR)
    };
    for sid in sides.into_iter().flatten() {
        if let Some(side) = map.sides.get_mut(sid.raw() as usize) {
            if !side.middle.is_empty() && side.middle != "-" {
                side.middle.clear();
            }
        }
    }
    world.line_flags.insert(ti, flags);
}

/// UDMF `locknumber` overrides the special's lock argument when non-zero.
fn effective_lock_number(world: &World, line: Option<LineId>, arg_lock: i32) -> i32 {
    if let Some(lid) = line {
        if let Some(n) = world
            .map
            .lines
            .get(lid.raw() as usize)
            .map(|l| l.lock_number)
            .filter(|&n| n != 0)
        {
            return n;
        }
    }
    arg_lock
}

/// Doom front side is to the right of directed linedef v1→v2.
fn actor_on_front_side(world: &World, line_id: LineId, mo: ActorId) -> bool {
    let Some(actor) = world.actors.get(mo) else {
        return false;
    };
    let Some(line) = world.map.lines.get(line_id.raw() as usize) else {
        return false;
    };
    let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
        return false;
    };
    let cross = line.delta.x() * (actor.pos.y() - v1.pos.y())
        - line.delta.y() * (actor.pos.x() - v1.pos.x());
    cross <= 0.0
}

/// Eye Z must lie in the linedef opening (two-sided) or front sector span.
fn switch_range_reachable(world: &World, line_id: LineId, mo: ActorId) -> bool {
    let Some(actor) = world.actors.get(mo) else {
        return false;
    };
    let Some(line) = world.map.lines.get(line_id.raw() as usize) else {
        return false;
    };
    let eye = actor.pos.z() + actor.view_height.max(1.0);
    let mut bottom = f64::NEG_INFINITY;
    let mut top = f64::INFINITY;
    if let Some(fs) = line.front_sector {
        if let Some(s) = world.sectors.get(fs.raw() as usize) {
            bottom = s.floor_z;
            top = s.ceiling_z;
        }
    }
    if let Some(bs) = line.back_sector {
        if let Some(s) = world.sectors.get(bs.raw() as usize) {
            bottom = bottom.max(s.floor_z);
            top = top.min(s.ceiling_z);
        }
    }
    if !bottom.is_finite() || !top.is_finite() || bottom > top {
        return true;
    }
    const SLACK: f64 = 16.0;
    eye >= bottom - SLACK && eye <= top + SLACK
}

/// Swap `SW1*` → `SW2*` on the front side (mid, then lower, then upper).
fn flip_switch_texture(world: &mut World, line_id: LineId) {
    let (idx, mid, lower, upper, sound_xy) = {
        let Some(line) = world.map.lines.get(line_id.raw() as usize) else {
            return;
        };
        let Some(side_id) = line.sides[0] else {
            return;
        };
        let idx = side_id.raw();
        let Some(side) = world.map.sides.get(idx as usize) else {
            return;
        };
        let mid = world
            .switch_mids
            .get(&idx)
            .cloned()
            .unwrap_or_else(|| side.middle.clone());
        let lower = world
            .switch_lowers
            .get(&idx)
            .cloned()
            .unwrap_or_else(|| side.lower.clone());
        let upper = world
            .switch_uppers
            .get(&idx)
            .cloned()
            .unwrap_or_else(|| side.upper.clone());
        let sound_xy = match (
            world.map.vertices.get(line.v1.raw() as usize),
            world.map.vertices.get(line.v2.raw() as usize),
        ) {
            (Some(v1), Some(v2)) => Some((
                (v1.pos.x() + v2.pos.x()) * 0.5,
                (v1.pos.y() + v2.pos.y()) * 0.5,
            )),
            _ => None,
        };
        (idx, mid, lower, upper, sound_xy)
    };
    let flipped = flip_sw1(&mid)
        .map(|n| (0u8, n))
        .or_else(|| flip_sw1(&lower).map(|n| (1u8, n)))
        .or_else(|| flip_sw1(&upper).map(|n| (2u8, n)));
    let Some((part, name)) = flipped else {
        if !cfg!(test) {
            eprintln!(
                "room: SWITCH line={} side={idx} mid={mid:?} low={lower:?} up={upper:?} (no SW1* to flip)",
                line_id.raw()
            );
        }
        return;
    };
    let part_name = match part {
        0 => "mid",
        1 => "lower",
        _ => "upper",
    };
    let from = match part {
        0 => mid.as_str(),
        1 => lower.as_str(),
        _ => upper.as_str(),
    };
    if !cfg!(test) {
        eprintln!(
            "room: SWITCH line={} side={idx} {part_name} {from} → {name}",
            line_id.raw()
        );
    }
    // Keep runtime maps + static sidedef in sync (renderer reads either path).
    match part {
        0 => {
            world.switch_mids.insert(idx, name.clone());
            if let Some(side) = std::sync::Arc::make_mut(&mut world.map)
                .sides
                .get_mut(idx as usize)
            {
                side.middle = name;
            }
        }
        1 => {
            world.switch_lowers.insert(idx, name.clone());
            if let Some(side) = std::sync::Arc::make_mut(&mut world.map)
                .sides
                .get_mut(idx as usize)
            {
                side.lower = name;
            }
        }
        _ => {
            world.switch_uppers.insert(idx, name.clone());
            if let Some(side) = std::sync::Arc::make_mut(&mut world.map)
                .sides
                .get_mut(idx as usize)
            {
                side.upper = name;
            }
        }
    }
    if let Some((x, y)) = sound_xy {
        world.play_sound("switches/normbutn", x, y, 0.0, 1.0);
    }
}

fn flip_sw1(tex: &str) -> Option<String> {
    let upper = tex.to_ascii_uppercase();
    upper.strip_prefix("SW1").map(|rest| format!("SW2{rest}"))
}

fn sectors_for_tag(world: &World, tag: i32, line: Option<LineId>) -> Vec<SectorId> {
    if tag != 0 {
        return world
            .map
            .sectors
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tags.contains(&tag))
            .map(|(i, _)| SectorId::from_raw(i as u32))
            .collect();
    }
    // Tag 0 → back sector of the activating line.
    if let Some(lid) = line {
        if let Some(line) = world.map.lines.get(lid.raw() as usize) {
            if let Some(sid) = line.back_sector {
                return vec![sid];
            }
        }
    }
    Vec::new()
}

/// GZDoom `P_CheckKeys` failure: only the console player's own body gets
/// the "you need a key" message and `*usefail` sound — a monster bumping a
/// locked door (or a voodoo doll) fails quietly.
fn locked_door_denied(world: &mut World, mo: ActorId, lock: i32) {
    let console_mo = world
        .players
        .first()
        .and_then(|p| p.as_ref())
        .and_then(|p| p.mo);
    if console_mo != Some(mo) {
        return;
    }
    let (msg, sound) = crate::locks::lock_deny_info(world, lock);
    world.play_sound(&sound, 0.0, 0.0, 0.0, 1.0);
    world.push_message(msg);
}

fn door_locked_raise(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    activator: Option<ActorId>,
) -> i32 {
    let lock = effective_lock_number(world, line, args[3]);
    if lock != 0 {
        let Some(mo) = activator else {
            return 0;
        };
        if !crate::locks::actor_has_lock(world, mo, lock) {
            locked_door_denied(world, mo, lock);
            return 0;
        }
    }
    // Tag: Hexen arg0, or Doom XLAT manual form stores it in arg4.
    let tag = if args[0] != 0 { args[0] } else { args[4] };
    let speed = hexen_speed_or(args[1], 16);
    let delay = if args[2] != 0 { args[2] } else { 150 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        spawn_door(world, sid, speed, delay, false);
    }
    1
}

fn door_raise(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let delay = if args[2] != 0 { args[2] } else { 150 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        spawn_door(world, sid, speed, delay, /*open_only*/ false);
    }
    1
}

fn door_open(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let sectors = sectors_for_tag(world, tag, line);
    for sid in sectors {
        spawn_door(world, sid, speed, 0, true);
    }
    1
}

fn door_close(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let sectors = sectors_for_tag(world, tag, line);
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let bottom = sec.floor_z;
        let top = sec.ceiling_z;
        play_sector_sound(world, sid, "doors/close");
        world.doors.push(DoorMover {
            sector: sid,
            direction: -1,
            speed,
            top_z: top,
            bottom_z: bottom,
            wait: 0,
            wait_max: 0,
            close_wait_open: false,
            wait_then_raise: false,
        });
        apply_door_sector_lights(world, sid, false);
    }
    1
}

fn door_close_wait_open(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let delay = if args[2] != 0 { args[2] } else { 240 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        world.doors.push(DoorMover {
            sector: sid,
            direction: -1,
            speed,
            top_z: sec.ceiling_z,
            bottom_z: sec.floor_z,
            wait: 0,
            wait_max: delay,
            close_wait_open: true,
            wait_then_raise: false,
        });
        apply_door_sector_lights(world, sid, false);
    }
    1
}

/// `Door_WaitRaise(tag, speed, delay, wait, lighttag)`.
fn door_wait_raise(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let delay = if args[2] != 0 { args[2] } else { 150 };
    let wait = args[3].max(0);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let top = door_top(world, sid, sec.floor_z).max(sec.ceiling_z);
        world.doors.push(DoorMover {
            sector: sid,
            direction: 0,
            speed,
            top_z: top,
            bottom_z: sec.floor_z,
            wait,
            wait_max: delay,
            close_wait_open: false,
            wait_then_raise: true,
        });
    }
    1
}

/// `Door_WaitClose(tag, speed, wait, lighttag)`.
fn door_wait_close(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let wait = args[2].max(0);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        world.doors.push(DoorMover {
            sector: sid,
            direction: 0,
            speed,
            top_z: sec.ceiling_z,
            bottom_z: sec.floor_z,
            wait,
            wait_max: 0,
            close_wait_open: false,
            wait_then_raise: false,
        });
    }
    1
}

fn generic_door(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    activator: Option<ActorId>,
) -> i32 {
    // Boom XLAT sometimes encodes kind as 64..=67; normalize to 0..=3.
    let kind = {
        let k = args[2];
        if (64..68).contains(&k) { k - 64 } else { k }
    };
    let lock = effective_lock_number(world, line, args[4]);
    if lock != 0 {
        let Some(mo) = activator else {
            return 0;
        };
        if !crate::locks::actor_has_lock(world, mo, lock) {
            locked_door_denied(world, mo, lock);
            return 0;
        }
    }
    let speed = hexen_speed_or(args[1], 16);
    let delay = if args[3] != 0 { args[3] } else { 150 };
    let tag = args[0];
    match kind {
        0 => door_raise(world, &[tag, args[1], delay, 0, 0], line),
        1 => door_open(world, &[tag, args[1], 0, 0, 0], line),
        2 => door_close_wait_open(world, &[tag, args[1], delay, 0, 0], line),
        3 => door_close(world, &[tag, args[1], 0, 0, 0], line),
        _ => {
            let _ = speed;
            0
        }
    }
}

fn elevator_move(world: &mut World, args: &[i32; 5], line: Option<LineId>, direction: i8) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 32);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let height = sec.ceiling_z - sec.floor_z;
        let dest = if direction > 0 {
            sec.floor_z + 64.0 // nearest higher — bootstrap
        } else {
            sec.floor_z - 64.0
        };
        world.elevators.push(ElevatorMover {
            sector: sid,
            direction,
            speed,
            dest_floor: dest,
            height,
        });
    }
    1
}

fn elevator_move_to_floor(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Move elevator so its floor matches the activating line's front sector floor.
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 32);
    let dest = line
        .and_then(|lid| world.map.lines.get(lid.raw() as usize))
        .and_then(|l| l.front_sector)
        .and_then(|sid| world.sectors.get(sid.raw() as usize))
        .map(|s| s.floor_z)
        .unwrap_or(0.0);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let height = sec.ceiling_z - sec.floor_z;
        let direction = if dest >= sec.floor_z { 1 } else { -1 };
        world.elevators.push(ElevatorMover {
            sector: sid,
            direction,
            speed,
            dest_floor: dest,
            height,
        });
    }
    1
}

/// Tick elevator movers (floor+ceiling together).
pub fn tick_elevators(world: &mut World) {
    let mut i = 0;
    while i < world.elevators.len() {
        let sector = world.elevators[i].sector;
        let Some(sec) = world.sectors.get_mut(sector.raw() as usize) else {
            world.elevators.swap_remove(i);
            continue;
        };
        let elev = &mut world.elevators[i];
        let done = if elev.direction > 0 {
            sec.floor_z += elev.speed;
            if sec.floor_z >= elev.dest_floor {
                sec.floor_z = elev.dest_floor;
                sec.ceiling_z = elev.dest_floor + elev.height;
                true
            } else {
                sec.ceiling_z = sec.floor_z + elev.height;
                false
            }
        } else {
            sec.floor_z -= elev.speed;
            if sec.floor_z <= elev.dest_floor {
                sec.floor_z = elev.dest_floor;
                sec.ceiling_z = elev.dest_floor + elev.height;
                true
            } else {
                sec.ceiling_z = sec.floor_z + elev.height;
                false
            }
        };
        if done {
            world.elevators.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

/// Door open height: `P_FindLowestCeilingSurrounding - 4`, or 120 above the
/// floor for a sector with no neighbours.
fn door_top(world: &World, sid: SectorId, floor_z: f64) -> f64 {
    let (_, _, min_ceil, _) = neighbor_plane_extrema(world, sid);
    min_ceil.map_or(floor_z + 120.0, |c| c - 4.0)
}

fn spawn_door(world: &mut World, sid: SectorId, speed: f64, delay: i32, open_only: bool) {
    // Vanilla `EV_VerticalDoor`: if a door thinker already owns the sector,
    // re-trigger reverses it (closing → open; raising/waiting → close now).
    // Open-stay (`doorOpen` / D1) never reverses — it only raises once.
    if let Some(d) = world.doors.iter_mut().find(|d| d.sector == sid) {
        if open_only || d.wait_max < 0 {
            return;
        }
        if d.direction == -1 {
            d.direction = 1;
            d.wait = 0;
            play_sector_sound(world, sid, "doors/open");
            apply_door_sector_lights(world, sid, true);
        } else {
            // Raising or waiting at top → start closing immediately.
            d.direction = -1;
            d.wait = 0;
            play_sector_sound(world, sid, "doors/close");
            apply_door_sector_lights(world, sid, false);
        }
        return;
    }
    let idx = sid.raw() as usize;
    let Some(sec) = world.sectors.get(idx) else {
        return;
    };
    let top = door_top(world, sid, sec.floor_z);
    let bottom = sec.floor_z;
    world.doors.push(DoorMover {
        sector: sid,
        direction: 1,
        speed,
        top_z: top.max(sec.ceiling_z),
        bottom_z: bottom,
        wait: 0,
        wait_max: if open_only { -1 } else { delay },
        close_wait_open: false,
        wait_then_raise: false,
    });
    play_sector_sound(world, sid, "doors/open");
    apply_door_sector_lights(world, sid, true);
}

/// Duke `SE 8` / `SE 9`: a ceiling door lights same-tag rooms.
fn door_light_spec(world: &World, sid: SectorId) -> (i32, i16, bool) {
    let i = sid.raw() as usize;
    let Some(bag) = world.map.udmf_user.sectors.get(i) else {
        return (0, 0, true);
    };
    let tag = bag.ints.get("user_doorlight").copied().unwrap_or(0);
    let bright = bag
        .ints
        .get("user_doorbright")
        .copied()
        .unwrap_or(0)
        .clamp(0, 255) as i16;
    let on_open = bag.ints.get("user_doorlightopen").copied().unwrap_or(1) != 0;
    (tag, bright, on_open)
}

fn apply_door_sector_lights(world: &mut World, sid: SectorId, opening: bool) {
    let (tag, bright, on_open) = door_light_spec(world, sid);
    if tag == 0 {
        return;
    }
    let use_bright = opening == on_open;
    for (i, s) in world.map.sectors.iter().enumerate() {
        if !s.tags.contains(&tag) {
            continue;
        }
        if let Some(dyn_s) = world.sectors.get_mut(i) {
            dyn_s.light = if use_bright { bright } else { s.light };
        }
    }
}

/// Rough sector origin for positional SFX (average of linedef endpoints).
fn sector_sound_origin(world: &World, sid: SectorId) -> (f64, f64, f64) {
    let idx = sid.raw() as usize;
    let z = world.sectors.get(idx).map(|s| s.floor_z).unwrap_or(0.0);
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut n = 0.0;
    for line in &world.map.lines {
        if line.front_sector == Some(sid) || line.back_sector == Some(sid) {
            if let (Some(v1), Some(v2)) = (
                world.map.vertices.get(line.v1.raw() as usize),
                world.map.vertices.get(line.v2.raw() as usize),
            ) {
                sx += v1.pos.x() + v2.pos.x();
                sy += v1.pos.y() + v2.pos.y();
                n += 2.0;
            }
        }
    }
    if n > 0.0 {
        (sx / n, sy / n, z)
    } else {
        (0.0, 0.0, z)
    }
}

/// Mover sound `name` from sector `sid`, honouring UDMF `silent` and
/// `soundsequence` (choice 0 when starting / opening, 1 when closing; stop
/// sounds belong to the sequence).
fn play_sector_sound(world: &mut World, sid: SectorId, name: &str) {
    let Some(geom) = world.map.sectors.get(sid.raw() as usize) else {
        return;
    };
    if geom.silent {
        return;
    }
    let (x, y, z) = sector_sound_origin(world, sid);
    if !geom.sound_sequence.is_empty() {
        let choice = match name {
            "doors/close" => 1,
            "doors/open" | "platforms/start" | "floors/move" => 0,
            _ => return,
        };
        let seq = world
            .sndseq
            .choose(&geom.sound_sequence, choice)
            .unwrap_or_else(|| geom.sound_sequence.clone());
        if crate::sndseq::start_sequence(world, &seq, x, y, z) {
            return;
        }
    }
    world.play_sound(name, x, y, z, 1.0);
}

fn light_change(world: &mut World, args: &[i32; 5]) -> i32 {
    let tag = args[0];
    let value = args[1].clamp(0, 255) as i16;
    let mut n = 0;
    for (i, s) in world.map.sectors.iter().enumerate() {
        if tag != 0 && !s.tags.contains(&tag) {
            continue;
        }
        if tag == 0 {
            break;
        }
        if let Some(dyn_s) = world.sectors.get_mut(i) {
            dyn_s.light = value;
            n += 1;
        }
    }
    i32::from(n > 0)
}

fn light_adjust(world: &mut World, args: &[i32; 5], sign: i32) -> i32 {
    let tag = args[0];
    let delta = args[1].saturating_mul(sign);
    if tag == 0 {
        return 0;
    }
    let mut n = 0;
    for (i, s) in world.map.sectors.iter().enumerate() {
        if !s.tags.contains(&tag) {
            continue;
        }
        if let Some(dyn_s) = world.sectors.get_mut(i) {
            let next = i32::from(dyn_s.light).saturating_add(delta).clamp(0, 255);
            dyn_s.light = next as i16;
            n += 1;
        }
    }
    i32::from(n > 0)
}

fn light_flicker(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let upper = args[1].clamp(0, 255) as i16;
    let lower = args[2].clamp(0, 255) as i16;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let max_light = if upper > 0 {
            upper
        } else {
            world.sectors.get(idx).map(|s| s.light).unwrap_or(255)
        };
        let min_light = if lower > 0 {
            lower.min(max_light)
        } else {
            (max_light / 3).min(max_light)
        };
        world.flickers.push(LightFlicker {
            sector: sid,
            max_light,
            min_light,
            count: 4,
            fire: true,
            bright: true,
            min_time: 7,
            max_time: 64,
        });
    }
    1
}

fn light_glow(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let upper = args[1].clamp(0, 255) as i16;
    let lower = args[2].clamp(0, 255) as i16;
    let tics = if args[3] > 0 { args[3] } else { 8 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let current = f64::from(sec.light);
        let dest = f64::from(upper);
        let step = if tics > 0 {
            (dest - current) / f64::from(tics)
        } else {
            0.0
        };
        world.glows.push(LightGlow {
            sector: sid,
            upper,
            lower,
            tics_max: tics,
            tics,
            dir: 1,
            current,
            step,
        });
    }
    1
}

fn light_fade(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let dest = args[1].clamp(0, 255) as i16;
    let tics = if args[2] > 0 { args[2] } else { 35 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let current = f64::from(sec.light);
        let dest_f = f64::from(dest);
        let step = (dest_f - current) / f64::from(tics);
        world.fades.push(LightFade {
            sector: sid,
            dest,
            tics,
            step,
            current,
        });
    }
    1
}

fn light_strobe(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let upper = args[1].clamp(0, 255) as i16;
    let lower = args[2].clamp(0, 255) as i16;
    let bright = if args[3] > 0 { args[3] } else { 5 };
    let dark = if args[4] > 0 { args[4] } else { 5 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let max_light = if upper > 0 {
            upper
        } else {
            world.sectors.get(idx).map(|s| s.light).unwrap_or(255)
        };
        let min_light = lower.min(max_light);
        world.strobes.push(LightStrobe {
            sector: sid,
            max_light,
            min_light,
            count: bright,
            bright_time: bright,
            dark_time: dark,
            bright: true,
        });
    }
    1
}

fn light_stop(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let mut removed = 0usize;
    for sid in &sectors {
        let before =
            world.strobes.len() + world.flickers.len() + world.glows.len() + world.fades.len();
        world.strobes.retain(|s| s.sector != *sid);
        world.flickers.retain(|s| s.sector != *sid);
        world.glows.retain(|s| s.sector != *sid);
        world.fades.retain(|s| s.sector != *sid);
        let after =
            world.strobes.len() + world.flickers.len() + world.glows.len() + world.fades.len();
        removed += before.saturating_sub(after);
    }
    i32::from(removed > 0)
}

fn light_strobe_doom(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let dark = if args[1] != 0 { args[1] } else { 5 };
    let bright = if args[2] != 0 { args[2] } else { 35 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let max_light = sec.light;
        let min_light = 0;
        world.strobes.push(LightStrobe {
            sector: sid,
            max_light,
            min_light,
            count: bright,
            bright_time: bright,
            dark_time: dark,
            bright: true,
        });
    }
    1
}

fn light_neighbor(world: &mut World, args: &[i32; 5], take_max: bool) -> i32 {
    let tag = args[0];
    if tag == 0 {
        return 0;
    }
    let mut n = 0;
    let lights: Vec<(usize, i16)> = world
        .map
        .sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| s.tags.contains(&tag))
        .filter_map(|(i, _)| world.sectors.get(i).map(|s| (i, s.light)))
        .collect();
    for (i, _) in &lights {
        let mut extreme = if take_max { 0i16 } else { 255i16 };
        // Bootstrap: compare against all other sectors' lights (neighbor walk later).
        for (j, sec) in world.sectors.iter().enumerate() {
            if j == *i {
                continue;
            }
            if take_max {
                extreme = extreme.max(sec.light);
            } else {
                extreme = extreme.min(sec.light);
            }
        }
        if let Some(dyn_s) = world.sectors.get_mut(*i) {
            dyn_s.light = extreme;
            n += 1;
        }
    }
    i32::from(n > 0)
}

/// Tick strobe light thinkers.
pub fn tick_strobes(world: &mut World) {
    for strobe in &mut world.strobes {
        strobe.count -= 1;
        if strobe.count > 0 {
            continue;
        }
        strobe.bright = !strobe.bright;
        strobe.count = if strobe.bright {
            strobe.bright_time
        } else {
            strobe.dark_time
        };
        if let Some(sec) = world.sectors.get_mut(strobe.sector.raw() as usize) {
            sec.light = if strobe.bright {
                strobe.max_light
            } else {
                strobe.min_light
            };
        }
    }
}

/// Tick fire / blink flicker thinkers (Doom `T_FireFlicker` / `T_LightFlash`).
pub fn tick_flickers(world: &mut World) {
    for i in 0..world.flickers.len() {
        world.flickers[i].count -= 1;
        if world.flickers[i].count > 0 {
            continue;
        }
        let roll = world.p_random("pr_lights");
        let flick = &mut world.flickers[i];
        let light = if flick.fire {
            // Doom `T_FireFlicker`: amount then clamp against minlight.
            flick.count = 4;
            let amount = i16::from(roll & 3) * 16;
            if flick.max_light - amount < flick.min_light {
                flick.min_light
            } else {
                flick.max_light - amount
            }
        } else {
            // Doom `T_LightFlash`: dim uses mintime mask, bright uses maxtime.
            if flick.bright {
                flick.bright = false;
                flick.count = (i32::from(roll) & flick.min_time) + 1;
                flick.min_light
            } else {
                flick.bright = true;
                flick.count = (i32::from(roll) & flick.max_time) + 1;
                flick.max_light
            }
        };
        if let Some(sec) = world.sectors.get_mut(flick.sector.raw() as usize) {
            sec.light = light;
        }
    }
}

fn scroll_texture(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    sx: f64,
    sy: f64,
) -> i32 {
    let Some(line_id) = line else {
        return 0;
    };
    let Some(ld) = world.map.lines.get(line_id.raw() as usize) else {
        return 0;
    };
    let Some(side) = ld.sides[0] else {
        return 0;
    };
    let speed = if args[0] != 0 {
        f64::from(args[0])
    } else {
        1.0
    };
    // Doom/Hexen: speed is often in 1/64 texel units per tic; treat as texels/tic.
    let dx = sx * speed;
    let dy = sy * speed;
    let base_x = world
        .map
        .sides
        .get(side.raw() as usize)
        .map(|s| f64::from(s.x_offset))
        .unwrap_or(0.0);
    let base_y = world
        .map
        .sides
        .get(side.raw() as usize)
        .map(|s| f64::from(s.y_offset))
        .unwrap_or(0.0);
    // Replace existing scroller on the same side.
    world.tex_scrollers.retain(|s| s.side != side);
    world.tex_scrollers.push(TextureScroller {
        side,
        dx,
        dy,
        x: base_x,
        y: base_y,
    });
    1
}

/// During a Duke `.DMO`, lock the view to a `DukeCamera` (`SE 27` / `CAMERA1`).
/// Prefers a camera that can see the player; otherwise the nearest. Cycles every
/// 70 tics. Move does not drop the view (`camera_revert` stays off).
pub fn tick_demo_cameras(world: &mut World) {
    if !world.demo_cameras {
        return;
    }
    let Some(cid) = world.classes.find_name("DukeCamera") else {
        return;
    };
    let Some(mo) = world.players.iter().flatten().find_map(|p| p.mo) else {
        return;
    };
    let cams: Vec<ActorId> = world
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            world
                .actors
                .get(id)
                .is_some_and(|a| !a.destroyed && a.health > 0 && a.class == cid)
        })
        .collect();
    if cams.is_empty() {
        return;
    }
    let seen: Vec<ActorId> = cams
        .iter()
        .copied()
        .filter(|&id| crate::sight::check_sight(world, id, mo))
        .collect();
    let pool = if seen.is_empty() { &cams } else { &seen };
    let idx = (world.level.time as usize / 70) % pool.len();
    let cam = pool[idx];
    for p in world.players.iter_mut().flatten() {
        p.camera = Some(cam);
        p.camera_revert = false;
    }
    world.camera_textures.insert(
        "VCAMA0".into(),
        crate::world::CameraTextureBind {
            camera: cam,
            fov: 90.0,
        },
    );
}

/// Bind the first living `DukeCamera` to the viewscreen canvas (`VCAMA0`).
pub fn init_security_camera_textures(world: &mut World) {
    let camera = world.thinkers.default.iter().copied().find(|&id| {
        world.actors.get(id).is_some_and(|a| {
            !a.destroyed
                && world
                    .classes
                    .get(a.class)
                    .is_some_and(|c| c.name == "DukeCamera")
        })
    });
    let Some(camera) = camera else {
        return;
    };
    world
        .camera_textures
        .entry("VCAMA0".into())
        .or_insert(crate::world::CameraTextureBind { camera, fov: 90.0 });
}

/// Seed walkable rotators from UDMF `user_spin` / `user_spinx` / `user_spiny`.
pub fn init_udmf_floor_spinners(world: &mut World) {
    for (i, bag) in world.map.udmf_user.sectors.iter().enumerate() {
        let Some(&speed) = bag.ints.get("user_spin") else {
            continue;
        };
        if speed == 0 {
            continue;
        }
        let cx = f64::from(bag.ints.get("user_spinx").copied().unwrap_or(0));
        let cy = f64::from(bag.ints.get("user_spiny").copied().unwrap_or(0));
        world.spinners.push(FloorSpinner {
            sector: SectorId::from_raw(i as u32),
            cx,
            cy,
            omega: f64::from(speed) / 256.0 * std::f64::consts::TAU,
        });
    }
}

/// Seed locator-following cars from UDMF `user_subway` / `user_sx*` / `user_sy*`.
pub fn init_udmf_floor_subways(world: &mut World) {
    for (i, bag) in world.map.udmf_user.sectors.iter().enumerate() {
        if bag.ints.get("user_subway").copied().unwrap_or(0) == 0 {
            continue;
        }
        let n = bag.ints.get("user_subn").copied().unwrap_or(0).clamp(0, 32) as usize;
        if n < 2 {
            continue;
        }
        let mut points = Vec::with_capacity(n);
        for k in 0..n {
            let Some(&x) = bag.ints.get(&format!("user_sx{k}")) else {
                continue;
            };
            let Some(&y) = bag.ints.get(&format!("user_sy{k}")) else {
                continue;
            };
            points.push((f64::from(x), f64::from(y)));
        }
        if points.len() < 2 {
            continue;
        }
        let speed = f64::from(bag.ints.get("user_subspeed").copied().unwrap_or(64)) / 32.0;
        let x = bag
            .ints
            .get("user_subx")
            .map(|&v| f64::from(v))
            .unwrap_or(points[0].0);
        let y = bag
            .ints
            .get("user_suby")
            .map(|&v| f64::from(v))
            .unwrap_or(points[0].1);
        world.subways.push(FloorSubway {
            sector: SectorId::from_raw(i as u32),
            points,
            i: 0,
            x,
            y,
            speed: speed.max(0.25),
            poly: bag.ints.get("user_subpo").copied().filter(|&id| id > 0),
        });
    }
}

/// Seed `SE 20` stretch sectors from UDMF `user_stretch`.
pub fn init_udmf_floor_stretches(world: &mut World) {
    for (i, bag) in world.map.udmf_user.sectors.iter().enumerate() {
        if bag.ints.get("user_stretch").copied().unwrap_or(0) == 0 {
            continue;
        }
        let dx8 = bag.ints.get("user_stdx").copied().unwrap_or(256);
        let dy8 = bag.ints.get("user_stdy").copied().unwrap_or(0);
        let dist = f64::from(bag.ints.get("user_stdist").copied().unwrap_or(16)).max(1.0);
        let speed = f64::from(bag.ints.get("user_stspd").copied().unwrap_or(32)) / 32.0;
        let sid = SectorId::from_raw(i as u32);
        let mut lines = Vec::new();
        let mut verts = Vec::new();
        if let Some(geom) = world.map.sectors.get(i) {
            for &lid in &geom.lines {
                lines.push(lid);
                if let Some(line) = world.map.lines.get(lid.raw() as usize) {
                    for v in [line.v1, line.v2] {
                        if !verts.contains(&v) {
                            verts.push(v);
                        }
                    }
                }
            }
        }
        if verts.is_empty() {
            continue;
        }
        let len =
            dmath::sqrt(f64::from(dx8) * f64::from(dx8) + f64::from(dy8) * f64::from(dy8)).max(1.0);
        world.stretches.push(FloorStretch {
            sector: sid,
            lines,
            verts,
            dx: f64::from(dx8) / len,
            dy: f64::from(dy8) / len,
            dist,
            speed: speed.max(0.25),
            pos: 0.0,
            sign: 0,
        });
    }
}

/// Seed `SE 22` teeth sectors from UDMF `user_teeth`.
pub fn init_udmf_floor_teeth(world: &mut World) {
    for (i, bag) in world.map.udmf_user.sectors.iter().enumerate() {
        if bag.ints.get("user_teeth").copied().unwrap_or(0) == 0 {
            continue;
        }
        let door_i = bag.ints.get("user_thdoor").copied().unwrap_or(-1);
        if door_i < 0 || door_i as usize >= world.sectors.len() {
            continue;
        }
        let dx8 = bag.ints.get("user_thdx").copied().unwrap_or(256);
        let dy8 = bag.ints.get("user_thdy").copied().unwrap_or(0);
        let dist = f64::from(bag.ints.get("user_thdist").copied().unwrap_or(16)).max(1.0);
        let speed = f64::from(bag.ints.get("user_thspd").copied().unwrap_or(32)) / 32.0;
        let sid = SectorId::from_raw(i as u32);
        let mut lines = Vec::new();
        let mut verts = Vec::new();
        if let Some(geom) = world.map.sectors.get(i) {
            for &lid in &geom.lines {
                lines.push(lid);
                if let Some(line) = world.map.lines.get(lid.raw() as usize) {
                    for v in [line.v1, line.v2] {
                        if !verts.contains(&v) {
                            verts.push(v);
                        }
                    }
                }
            }
        }
        if verts.is_empty() {
            continue;
        }
        let len =
            dmath::sqrt(f64::from(dx8) * f64::from(dx8) + f64::from(dy8) * f64::from(dy8)).max(1.0);
        let rest_ceil = world
            .sectors
            .get(door_i as usize)
            .map(|s| s.ceiling_z)
            .unwrap_or(0.0);
        world.teeth.push(FloorTeeth {
            sector: sid,
            door: SectorId::from_raw(door_i as u32),
            lines,
            verts,
            dx: f64::from(dx8) / len,
            dy: f64::from(dy8) / len,
            dist,
            speed: speed.max(0.25),
            pos: 0.0,
            rest_ceil,
        });
    }
}

/// Seed `SE 16` reactor-skew sectors from UDMF `user_skew`.
pub fn init_udmf_floor_skews(world: &mut World) {
    for (i, bag) in world.map.udmf_user.sectors.iter().enumerate() {
        if bag.ints.get("user_skew").copied().unwrap_or(0) == 0 {
            continue;
        }
        let dx8 = bag.ints.get("user_skdx").copied().unwrap_or(256);
        let dy8 = bag.ints.get("user_skdy").copied().unwrap_or(0);
        let dist = f64::from(bag.ints.get("user_skdist").copied().unwrap_or(16)).max(1.0);
        let speed = f64::from(bag.ints.get("user_skspd").copied().unwrap_or(32)) / 32.0;
        let sid = SectorId::from_raw(i as u32);
        let mut lines = Vec::new();
        let mut verts = Vec::new();
        if let Some(geom) = world.map.sectors.get(i) {
            for &lid in &geom.lines {
                lines.push(lid);
                if let Some(line) = world.map.lines.get(lid.raw() as usize) {
                    for v in [line.v1, line.v2] {
                        if !verts.contains(&v) {
                            verts.push(v);
                        }
                    }
                }
            }
        }
        if verts.is_empty() {
            continue;
        }
        let len =
            dmath::sqrt(f64::from(dx8) * f64::from(dx8) + f64::from(dy8) * f64::from(dy8)).max(1.0);
        world.skews.push(FloorSkew {
            sector: sid,
            lines,
            verts,
            dx: f64::from(dx8) / len,
            dy: f64::from(dy8) / len,
            dist,
            speed: speed.max(0.25),
            pos: 0.0,
            sign: 1,
        });
    }
}

fn toggle_stretch(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    let mut n = 0;
    for sid in sectors {
        if let Some(st) = world.stretches.iter_mut().find(|s| s.sector == sid) {
            st.sign = if st.sign == 0 {
                if st.pos > st.dist * 0.5 { -1 } else { 1 }
            } else {
                -st.sign
            };
            n += 1;
        }
    }
    n
}

/// Seed wall scrollers from UDMF sidedef `xscroll` / `yscroll`.
pub fn init_udmf_side_scrollers(world: &mut World) {
    let sides: Vec<(SideId, f64, f64, f64, f64)> = world
        .map
        .sides
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            if s.scroll_x.abs() < 1e-9 && s.scroll_y.abs() < 1e-9 {
                return None;
            }
            Some((
                SideId::from_raw(i as u32),
                f64::from(s.scroll_x),
                f64::from(s.scroll_y),
                f64::from(s.x_offset),
                f64::from(s.y_offset),
            ))
        })
        .collect();
    for (side, dx, dy, base_x, base_y) in sides {
        // UDMF scroll stacks with / replaces an existing special scroller on the side.
        if let Some(sc) = world.tex_scrollers.iter_mut().find(|s| s.side == side) {
            sc.dx += dx;
            sc.dy += dy;
        } else {
            world.tex_scrollers.push(TextureScroller {
                side,
                dx,
                dy,
                x: base_x,
                y: base_y,
            });
        }
    }
}

/// Spawn permanent `Scroll_Texture_*` thinkers from linedef specials at map start.
pub fn init_line_scrollers(world: &mut World) {
    let lines: Vec<_> = world
        .map
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if l.special < 0 {
                return None;
            }
            #[allow(clippy::cast_sign_loss)]
            let special = l.special as u16;
            match special {
                num::SCROLL_TEXTURE_LEFT
                | num::SCROLL_TEXTURE_RIGHT
                | num::SCROLL_TEXTURE_UP
                | num::SCROLL_TEXTURE_DOWN
                | num::SCROLL_TEXTURE_OFFSETS
                | num::SCROLL_TEXTURE_MODEL
                | num::LINE_HORIZON
                | num::TRANSFER_WALL_LIGHT => Some((LineId::from_raw(i as u32), special, l.args)),
                _ => None,
            }
        })
        .collect();
    for (line, special, args) in lines {
        let _ = execute_special(world, special, &args, None, Some(line));
        // Load-time scrollers are not use/walk triggers — clear so `use_lines`
        // does not prefer a SCROLL wall over a nearby SR switch.
        world.line_specials.insert(line.raw(), 0);
    }
}

/// Apply load-time Boom transfer specials (`Transfer_Heights` / light).
pub fn init_transfers(world: &mut World) {
    let lines: Vec<_> = world
        .map
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if l.special < 0 {
                return None;
            }
            #[allow(clippy::cast_sign_loss)]
            let special = l.special as u16;
            match special {
                num::TRANSFER_HEIGHTS
                | num::TRANSFER_FLOOR_LIGHT
                | num::TRANSFER_CEILING_LIGHT
                | num::SECTOR_SET_FRICTION
                | num::SECTOR_SET_WIND
                | num::SECTOR_SET_CURRENT => Some((LineId::from_raw(i as u32), special, l.args)),
                _ => None,
            }
        })
        .collect();
    for (line, special, args) in lines {
        let _ = execute_special(world, special, &args, None, Some(line));
    }
    apply_transfers(world);
}

/// Init `PointPush_SetForce` after map things exist (needs PointPusher/Puller).
pub fn init_point_pushers(world: &mut World) {
    let lines: Vec<_> = world
        .map
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if l.special < 0 {
                return None;
            }
            #[allow(clippy::cast_sign_loss)]
            let special = l.special as u16;
            if special == num::POINT_PUSH_SET_FORCE {
                Some((LineId::from_raw(i as u32), l.args))
            } else {
                None
            }
        })
        .collect();
    for (line, args) in lines {
        let _ = execute_special(world, num::POINT_PUSH_SET_FORCE, &args, None, Some(line));
    }
}

fn point_push_set_force(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // PointPush_SetForce(tag, tid, magnitude, useline)
    let use_line = args[3] != 0;
    let magnitude = if use_line {
        if let Some(lid) = line {
            if let Some(ln) = world.map.lines.get(lid.raw() as usize) {
                room_core::dmath::sqrt(ln.delta.x() * ln.delta.x() + ln.delta.y() * ln.delta.y())
            } else {
                0.0
            }
        } else {
            0.0
        }
    } else {
        f64::from(args[2]).abs()
    };
    if magnitude <= 0.0 {
        return 0;
    }
    let tag = args[0];
    let tid_filter = args[1];
    let mut added = 0;
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(actor) = world.actors.get(id).cloned() else {
            continue;
        };
        if actor.destroyed {
            continue;
        }
        if tid_filter != 0 && actor.tid != tid_filter {
            continue;
        }
        let Some(class) = world.classes.get(actor.class) else {
            continue;
        };
        let pull = match class.name.as_str() {
            "PointPuller" => true,
            "PointPusher" => false,
            _ => continue,
        };
        if tag != 0 {
            let Some(sid) =
                room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
            else {
                continue;
            };
            let Some(geom) = world.map.sectors.get(sid.raw() as usize) else {
                continue;
            };
            if !geom.tags.contains(&tag) {
                continue;
            }
        }
        world.point_forces.push(PointForce {
            x: actor.pos.x(),
            y: actor.pos.y(),
            magnitude,
            pull,
        });
        added += 1;
    }
    added
}

fn sector_set_friction(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Sector_SetFriction(tag, amount) — amount 0 → linedef length (Boom).
    let mut amount = args[1];
    if amount == 0 {
        if let Some(lid) = line {
            if let Some(ln) = world.map.lines.get(lid.raw() as usize) {
                let len = room_core::dmath::sqrt(
                    ln.delta.x() * ln.delta.x() + ln.delta.y() * ln.delta.y(),
                );
                #[allow(clippy::cast_possible_truncation)]
                {
                    amount = len.round().clamp(1.0, 200.0) as i32;
                }
            }
        }
    }
    amount = amount.clamp(1, 200);
    // GZDoom: friction = ((0x1EB8 * amount) / 0x80 + 0xD001) / 65536
    let friction = (f64::from(0x1EB8 * amount) / f64::from(0x80) + f64::from(0xD001)) / 65536.0;
    let friction = friction.clamp(0.0, 1.0);
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        if let Some(sec) = world.sectors.get_mut(sid.raw() as usize) {
            sec.friction = friction;
        }
    }
    1
}

fn sector_set_pusher(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    current: bool,
) -> i32 {
    // Sector_SetWind/Current(tag, amount, angle, useline)
    let use_line = args[3] != 0;
    let (dx, dy) = if use_line {
        if let Some(lid) = line {
            if let Some(ln) = world.map.lines.get(lid.raw() as usize) {
                // Boom: push ≈ linedef delta / 32 per tic.
                (ln.delta.x() / 32.0, ln.delta.y() / 32.0)
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        }
    } else {
        let amount = f64::from(args[1]) / 32.0;
        // Hexen arg2 is a byte angle (0–255 → full circle).
        let ang = Angle::from_degrees(f64::from(args[2] & 0xFF) * (360.0 / 256.0));
        (amount * ang.cos(), amount * ang.sin())
    };
    if dx == 0.0 && dy == 0.0 {
        return 0;
    }
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        if let Some(sec) = world.sectors.get_mut(sid.raw() as usize) {
            if current {
                sec.current_x = dx;
                sec.current_y = dy;
            } else {
                sec.wind_x = dx;
                sec.wind_y = dy;
            }
        }
    }
    1
}

/// Apply sector wind / current thrust to actors (Boom pusher subset).
pub fn tick_sector_pushers(world: &mut World) {
    let has_sector = world
        .sectors
        .iter()
        .any(|s| s.wind_x != 0.0 || s.wind_y != 0.0 || s.current_x != 0.0 || s.current_y != 0.0);
    let has_point = !world.point_forces.is_empty();
    if !has_sector && !has_point {
        return;
    }
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(actor) = world.actors.get(id).cloned() else {
            continue;
        };
        if actor.destroyed {
            continue;
        }
        // Skip PointPusher / PointPuller sources.
        if world
            .classes
            .get(actor.class)
            .is_some_and(|c| c.name == "PointPusher" || c.name == "PointPuller")
        {
            continue;
        }
        let mut dx = 0.0;
        let mut dy = 0.0;
        if has_sector {
            if let Some(sid) =
                room_nodebuild::point_in_sector(&world.map, actor.pos.x(), actor.pos.y())
            {
                let idx = sid.raw() as usize;
                if let Some(sec) = world.sectors.get(idx) {
                    let on_floor = (actor.pos.z() - sec.floor_z).abs() <= 1.0;
                    let mut wx = sec.wind_x;
                    let mut wy = sec.wind_y;
                    if !on_floor {
                        wx *= 0.5;
                        wy *= 0.5;
                    }
                    dx += wx;
                    dy += wy;
                    if on_floor {
                        dx += sec.current_x;
                        dy += sec.current_y;
                    }
                }
            }
        }
        for pf in &world.point_forces {
            let ox = actor.pos.x() - pf.x;
            let oy = actor.pos.y() - pf.y;
            let dist = room_core::dmath::sqrt(ox * ox + oy * oy);
            let reach = pf.magnitude * 2.0;
            if dist <= 0.0 || dist >= reach {
                continue;
            }
            // Linear falloff to zero at 2L; thrust scale ~ magnitude/128 per tic.
            let strength = (pf.magnitude * (1.0 - dist / reach)) / 128.0;
            let inv = strength / dist;
            if pf.pull {
                dx -= ox * inv;
                dy -= oy * inv;
            } else {
                dx += ox * inv;
                dy += oy * inv;
            }
        }
        if dx == 0.0 && dy == 0.0 {
            continue;
        }
        if let Some(a) = world.actors.get_mut(id) {
            a.vel = DVec3::new(a.vel.x() + dx, a.vel.y() + dy, a.vel.z());
        }
    }
}

fn control_sector(world: &World, line: Option<LineId>) -> Option<SectorId> {
    let lid = line?;
    world
        .map
        .lines
        .get(lid.raw() as usize)
        .and_then(|l| l.front_sector)
}

fn transfer_heights(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let Some(control) = control_sector(world, line) else {
        return 0;
    };
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        if sid == control {
            continue;
        }
        world.height_transfers.retain(|t| t.affected != sid);
        world.height_transfers.push(HeightTransfer {
            control,
            affected: sid,
        });
        n += 1;
    }
    i32::from(n > 0)
}

fn transfer_floor_light(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    transfer_light(world, args, line, true)
}

fn transfer_ceiling_light(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    transfer_light(world, args, line, false)
}

fn transfer_light(world: &mut World, args: &[i32; 5], line: Option<LineId>, floor: bool) -> i32 {
    let Some(control) = control_sector(world, line) else {
        return 0;
    };
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let list = if floor {
        &mut world.floor_light_transfers
    } else {
        &mut world.ceiling_light_transfers
    };
    let mut n = 0;
    for sid in sectors {
        if sid == control {
            continue;
        }
        list.retain(|t| t.affected != sid);
        list.push(LightTransfer {
            control,
            affected: sid,
        });
        n += 1;
    }
    i32::from(n > 0)
}

/// Boom light / height transfer link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightTransfer {
    /// Donor sector (linedef front).
    pub control: SectorId,
    /// Tagged recipient.
    pub affected: SectorId,
}

/// Boom `Transfer_Heights` link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeightTransfer {
    /// Control sector providing fake planes.
    pub control: SectorId,
    /// Tagged sector receiving fake floor/ceiling.
    pub affected: SectorId,
}

/// Refresh fake planes and transferred lights from control sectors.
pub fn apply_transfers(world: &mut World) {
    let height_pairs: Vec<_> = world.height_transfers.clone();
    for t in height_pairs {
        let Some(ctrl) = world.sectors.get(t.control.raw() as usize).cloned() else {
            continue;
        };
        if let Some(sec) = world.sectors.get_mut(t.affected.raw() as usize) {
            sec.fake_floor_z = Some(ctrl.floor_z);
            sec.fake_ceiling_z = Some(ctrl.ceiling_z);
        }
    }
    let floor_lights: Vec<_> = world.floor_light_transfers.clone();
    for t in floor_lights {
        let light = world
            .sectors
            .get(t.control.raw() as usize)
            .map(|s| s.light)
            .unwrap_or(0);
        if let Some(sec) = world.sectors.get_mut(t.affected.raw() as usize) {
            sec.light = light;
        }
    }
    let ceil_lights: Vec<_> = world.ceiling_light_transfers.clone();
    for t in ceil_lights {
        let light = world
            .sectors
            .get(t.control.raw() as usize)
            .map(|s| s.light)
            .unwrap_or(0);
        if let Some(sec) = world.sectors.get_mut(t.affected.raw() as usize) {
            sec.light = light;
        }
    }
}

/// Tick wall texture scrollers into [`World::side_offsets`].
pub fn tick_tex_scrollers(world: &mut World) {
    for sc in &mut world.tex_scrollers {
        sc.x += sc.dx;
        sc.y += sc.dy;
        // Keep offsets in i16 range (wrap like Doom; via i32 so 65535 → -1).
        let ox = (sc.x.rem_euclid(65_536.0) as i32) as i16;
        let oy = (sc.y.rem_euclid(65_536.0) as i32) as i16;
        world.side_offsets.insert(sc.side.raw(), (ox, oy));
    }
}

fn scroll_floor(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let (dx, dy) = scroll_velocity(args, line, world);
    if dx == 0.0 && dy == 0.0 {
        return 0;
    }
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        // Replace existing scroller on same sector.
        world.scrollers.retain(|s| s.sector != sid);
        world.scrollers.push(FloorScroller {
            sector: sid,
            dx,
            dy,
            u: 0.0,
            v: 0.0,
        });
    }
    1
}

fn scroll_ceiling(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let (dx, dy) = scroll_velocity(args, line, world);
    if dx == 0.0 && dy == 0.0 {
        return 0;
    }
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        world.ceiling_scrollers.retain(|s| s.sector != sid);
        world.ceiling_scrollers.push(CeilingScroller {
            sector: sid,
            dx,
            dy,
            u: 0.0,
            v: 0.0,
        });
    }
    1
}

/// Hexen movex/movey in 1/32 units, or Boom linedef-vector when both are zero.
fn scroll_velocity(args: &[i32; 5], line: Option<LineId>, world: &World) -> (f64, f64) {
    let mut dx = f64::from(args[2]) / 32.0;
    let mut dy = f64::from(args[3]) / 32.0;
    if dx == 0.0 && dy == 0.0 {
        if let Some(lid) = line {
            if let Some(ln) = world.map.lines.get(lid.raw() as usize) {
                // Boom: scroll speed from linedef length / 32 per tic.
                dx = ln.delta.x() / 32.0;
                dy = ln.delta.y() / 32.0;
            }
        }
    }
    (dx, dy)
}

/// Tick ceiling flat scroll offsets (for renderer sync).
pub fn tick_ceiling_scrollers(world: &mut World) {
    for sc in &mut world.ceiling_scrollers {
        sc.u += sc.dx;
        sc.v += sc.dy;
        world
            .ceiling_scroll_offsets
            .insert(sc.sector.raw(), (sc.u, sc.v));
    }
}

/// Carry mobjs standing on scrolling floors; accumulate flat UV offsets.
pub fn tick_scrollers(world: &mut World) {
    if world.scrollers.is_empty() {
        return;
    }
    for sc in &mut world.scrollers {
        sc.u += sc.dx;
        sc.v += sc.dy;
        world
            .floor_scroll_offsets
            .insert(sc.sector.raw(), (sc.u, sc.v));
    }
    let carries: Vec<(SectorId, f64, f64)> = world
        .scrollers
        .iter()
        .map(|s| (s.sector, s.dx, s.dy))
        .collect();
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(actor) = world.actors.get(id).cloned() else {
            continue;
        };
        if actor.destroyed {
            continue;
        }
        // Crude sector membership: actor XY inside any tagged scrolling sector's bbox via floor_z match.
        for &(sid, dx, dy) in &carries {
            let idx = sid.raw() as usize;
            let Some(sec) = world.sectors.get(idx) else {
                continue;
            };
            // On-floor check.
            if (actor.pos.z() - sec.floor_z).abs() > 1.0 {
                continue;
            }
            let Some(geom) = world.map.sectors.get(idx) else {
                continue;
            };
            // Point-in-sector via bounding box of sector linedef endpoints (bootstrap).
            let mut min_x = f64::MAX;
            let mut max_x = f64::MIN;
            let mut min_y = f64::MAX;
            let mut max_y = f64::MIN;
            for &lid in &geom.lines {
                let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                    continue;
                };
                for vid in [line.v1, line.v2] {
                    if let Some(v) = world.map.vertices.get(vid.raw() as usize) {
                        min_x = min_x.min(v.pos.x());
                        max_x = max_x.max(v.pos.x());
                        min_y = min_y.min(v.pos.y());
                        max_y = max_y.max(v.pos.y());
                    }
                }
            }
            let x = actor.pos.x();
            let y = actor.pos.y();
            if x < min_x || x > max_x || y < min_y || y > max_y {
                continue;
            }
            let _ = crate::physics::try_move(
                world,
                id,
                room_core::DVec3::new(x + dx, y + dy, actor.pos.z()),
            );
            break;
        }
    }
}

/// Carry on-floor actors around a lotag-30 / `SE 1` pivot.
pub fn tick_spinners(world: &mut World) {
    if world.spinners.is_empty() {
        return;
    }
    let spins: Vec<FloorSpinner> = world.spinners.clone();
    for spin in &spins {
        let deg = Angle::from_radians(spin.omega).degrees();
        let idx = spin.sector.raw();
        let (floor_a, ceil_a) = world.plane_angles.get(&idx).copied().unwrap_or((0.0, 0.0));
        world
            .plane_angles
            .insert(idx, ((floor_a + deg).rem_euclid(360.0), ceil_a));
    }
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(actor) = world.actors.get(id).cloned() else {
            continue;
        };
        if actor.destroyed {
            continue;
        }
        for spin in &spins {
            let idx = spin.sector.raw() as usize;
            let Some(sec) = world.sectors.get(idx) else {
                continue;
            };
            if (actor.pos.z() - sec.floor_z).abs() > 1.0 {
                continue;
            }
            let Some(geom) = world.map.sectors.get(idx) else {
                continue;
            };
            let mut min_x = f64::MAX;
            let mut max_x = f64::MIN;
            let mut min_y = f64::MAX;
            let mut max_y = f64::MIN;
            for &lid in &geom.lines {
                let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                    continue;
                };
                for vid in [line.v1, line.v2] {
                    if let Some(v) = world.map.vertices.get(vid.raw() as usize) {
                        min_x = min_x.min(v.pos.x());
                        max_x = max_x.max(v.pos.x());
                        min_y = min_y.min(v.pos.y());
                        max_y = max_y.max(v.pos.y());
                    }
                }
            }
            let x = actor.pos.x();
            let y = actor.pos.y();
            if x < min_x || x > max_x || y < min_y || y > max_y {
                continue;
            }
            let s = dmath::sin(spin.omega);
            let c = dmath::cos(spin.omega);
            let dx = x - spin.cx;
            let dy = y - spin.cy;
            let nx = spin.cx + dx * c - dy * s;
            let ny = spin.cy + dx * s + dy * c;
            if let Some(a) = world.actors.get_mut(id) {
                a.pos = DVec3::new(nx, ny, a.pos.z());
                let vx = a.vel.x();
                let vy = a.vel.y();
                a.vel = DVec3::new(vx * c - vy * s, vx * s + vy * c, a.vel.z());
                a.angle = Angle::from_degrees(
                    a.angle.degrees() + Angle::from_radians(spin.omega).degrees(),
                );
            }
            break;
        }
    }
}

/// How close an actor must be to the virtual car to ride it.
const SUBWAY_CARRY: f64 = 96.0;

/// Carry nearby actors along locator waypoints (`SE 6` / `14` / `30`).
pub fn tick_subways(world: &mut World) {
    if world.subways.is_empty() {
        return;
    }
    let mut cars = world.subways.clone();
    let mut deltas: Vec<(f64, f64, f64, f64)> = Vec::with_capacity(cars.len());
    for car in &mut cars {
        if car.points.len() < 2 || car.speed <= 0.0 {
            deltas.push((car.x, car.y, 0.0, 0.0));
            continue;
        }
        let mut remain = car.speed;
        let mut dx = 0.0;
        let mut dy = 0.0;
        while remain > 1e-9 {
            let (px, py) = car.points[car.i % car.points.len()];
            let vx = px - car.x;
            let vy = py - car.y;
            let dist = dmath::sqrt(vx * vx + vy * vy);
            if dist <= 8.0 {
                car.i = (car.i + 1) % car.points.len();
                continue;
            }
            let step = remain.min(dist);
            let ux = vx / dist * step;
            let uy = vy / dist * step;
            car.x += ux;
            car.y += uy;
            dx += ux;
            dy += uy;
            remain -= step;
            if dist - step <= 8.0 {
                car.i = (car.i + 1) % car.points.len();
            }
        }
        world
            .floor_scroll_offsets
            .insert(car.sector.raw(), (car.x, car.y));
        if let Some(id) = car.poly {
            let speed = ((car.speed * 8.0).ceil() as i32).max(1);
            let dest = [id, speed, car.x.round() as i32, car.y.round() as i32, 0];
            crate::polyobj::polyobj_move_to(world, &dest, true);
        }
        deltas.push((car.x, car.y, dx, dy));
    }
    world.subways = cars;
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(actor) = world.actors.get(id).cloned() else {
            continue;
        };
        if actor.destroyed {
            continue;
        }
        for &(cx, cy, dx, dy) in &deltas {
            if dx == 0.0 && dy == 0.0 {
                continue;
            }
            let ax = actor.pos.x() - cx;
            let ay = actor.pos.y() - cy;
            if dmath::sqrt(ax * ax + ay * ay) > SUBWAY_CARRY {
                continue;
            }
            if let Some(a) = world.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z());
            }
            break;
        }
    }
}

/// Slide `SE 20` stretch vertices and carry anyone on that floor.
pub fn tick_stretches(world: &mut World) {
    if world.stretches.is_empty() {
        return;
    }
    let mut cars = world.stretches.clone();
    let mut deltas: Vec<(SectorId, f64, f64)> = Vec::new();
    for st in &mut cars {
        if st.sign == 0 || st.speed <= 0.0 {
            continue;
        }
        let mut step = st.speed * f64::from(st.sign);
        let next = st.pos + step;
        if next >= st.dist {
            step = st.dist - st.pos;
            st.pos = st.dist;
            st.sign = 0;
        } else if next <= 0.0 {
            step = -st.pos;
            st.pos = 0.0;
            st.sign = 0;
        } else {
            st.pos = next;
        }
        if step.abs() <= 1e-9 {
            continue;
        }
        let dx = st.dx * step;
        let dy = st.dy * step;
        crate::polyobj::translate_vertices(world, &st.verts, &st.lines, dx, dy);
        deltas.push((st.sector, dx, dy));
    }
    world.stretches = cars;
    if deltas.is_empty() {
        return;
    }
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(actor) = world.actors.get(id).cloned() else {
            continue;
        };
        if actor.destroyed {
            continue;
        }
        for &(sid, dx, dy) in &deltas {
            let idx = sid.raw() as usize;
            let Some(sec) = world.sectors.get(idx) else {
                continue;
            };
            if (actor.pos.z() - sec.floor_z).abs() > 1.0 {
                continue;
            }
            let Some(geom) = world.map.sectors.get(idx) else {
                continue;
            };
            let mut min_x = f64::MAX;
            let mut max_x = f64::MIN;
            let mut min_y = f64::MAX;
            let mut max_y = f64::MIN;
            for &lid in &geom.lines {
                let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                    continue;
                };
                for vid in [line.v1, line.v2] {
                    if let Some(v) = world.map.vertices.get(vid.raw() as usize) {
                        min_x = min_x.min(v.pos.x());
                        max_x = max_x.max(v.pos.x());
                        min_y = min_y.min(v.pos.y());
                        max_y = max_y.max(v.pos.y());
                    }
                }
            }
            let x = actor.pos.x();
            let y = actor.pos.y();
            if x < min_x || x > max_x || y < min_y || y > max_y {
                continue;
            }
            if let Some(a) = world.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z());
            }
            break;
        }
    }
}

/// Retract `SE 22` teeth when the linked door ceiling has left its rest height.
pub fn tick_teeth(world: &mut World) {
    if world.teeth.is_empty() {
        return;
    }
    let mut cars = world.teeth.clone();
    let mut deltas: Vec<(SectorId, f64, f64)> = Vec::new();
    for th in &mut cars {
        let ceil = world
            .sectors
            .get(th.door.raw() as usize)
            .map(|s| s.ceiling_z)
            .unwrap_or(th.rest_ceil);
        let target = if ceil > th.rest_ceil + 1.0 {
            th.dist
        } else {
            0.0
        };
        let mut step = (target - th.pos).clamp(-th.speed, th.speed);
        let next = th.pos + step;
        if next >= th.dist {
            step = th.dist - th.pos;
            th.pos = th.dist;
        } else if next <= 0.0 {
            step = -th.pos;
            th.pos = 0.0;
        } else {
            th.pos = next;
        }
        if step.abs() <= 1e-9 {
            continue;
        }
        let dx = th.dx * step;
        let dy = th.dy * step;
        crate::polyobj::translate_vertices(world, &th.verts, &th.lines, dx, dy);
        deltas.push((th.sector, dx, dy));
    }
    world.teeth = cars;
    if deltas.is_empty() {
        return;
    }
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(actor) = world.actors.get(id).cloned() else {
            continue;
        };
        if actor.destroyed {
            continue;
        }
        for &(sid, dx, dy) in &deltas {
            let idx = sid.raw() as usize;
            let Some(sec) = world.sectors.get(idx) else {
                continue;
            };
            if (actor.pos.z() - sec.floor_z).abs() > 1.0 {
                continue;
            }
            let Some(geom) = world.map.sectors.get(idx) else {
                continue;
            };
            let mut min_x = f64::MAX;
            let mut max_x = f64::MIN;
            let mut min_y = f64::MAX;
            let mut max_y = f64::MIN;
            for &lid in &geom.lines {
                let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                    continue;
                };
                for vid in [line.v1, line.v2] {
                    if let Some(v) = world.map.vertices.get(vid.raw() as usize) {
                        min_x = min_x.min(v.pos.x());
                        max_x = max_x.max(v.pos.x());
                        min_y = min_y.min(v.pos.y());
                        max_y = max_y.max(v.pos.y());
                    }
                }
            }
            let x = actor.pos.x();
            let y = actor.pos.y();
            if x < min_x || x > max_x || y < min_y || y > max_y {
                continue;
            }
            if let Some(a) = world.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z());
            }
            break;
        }
    }
}

/// Oscillate `SE 16` reactor-skew vertices and carry anyone on that floor.
pub fn tick_skews(world: &mut World) {
    if world.skews.is_empty() {
        return;
    }
    let mut cars = world.skews.clone();
    let mut deltas: Vec<(SectorId, f64, f64)> = Vec::new();
    for sk in &mut cars {
        if sk.speed <= 0.0 {
            continue;
        }
        let mut step = sk.speed * f64::from(sk.sign);
        let next = sk.pos + step;
        if next >= sk.dist {
            step = sk.dist - sk.pos;
            sk.pos = sk.dist;
            sk.sign = -1;
        } else if next <= 0.0 {
            step = -sk.pos;
            sk.pos = 0.0;
            sk.sign = 1;
        } else {
            sk.pos = next;
        }
        if step.abs() <= 1e-9 {
            continue;
        }
        let dx = sk.dx * step;
        let dy = sk.dy * step;
        crate::polyobj::translate_vertices(world, &sk.verts, &sk.lines, dx, dy);
        deltas.push((sk.sector, dx, dy));
    }
    world.skews = cars;
    if deltas.is_empty() {
        return;
    }
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(actor) = world.actors.get(id).cloned() else {
            continue;
        };
        if actor.destroyed {
            continue;
        }
        for &(sid, dx, dy) in &deltas {
            let idx = sid.raw() as usize;
            let Some(sec) = world.sectors.get(idx) else {
                continue;
            };
            if (actor.pos.z() - sec.floor_z).abs() > 1.0 {
                continue;
            }
            let Some(geom) = world.map.sectors.get(idx) else {
                continue;
            };
            let mut min_x = f64::MAX;
            let mut max_x = f64::MIN;
            let mut min_y = f64::MAX;
            let mut max_y = f64::MIN;
            for &lid in &geom.lines {
                let Some(line) = world.map.lines.get(lid.raw() as usize) else {
                    continue;
                };
                for vid in [line.v1, line.v2] {
                    if let Some(v) = world.map.vertices.get(vid.raw() as usize) {
                        min_x = min_x.min(v.pos.x());
                        max_x = max_x.max(v.pos.x());
                        min_y = min_y.min(v.pos.y());
                        max_y = max_y.max(v.pos.y());
                    }
                }
            }
            let x = actor.pos.x();
            let y = actor.pos.y();
            if x < min_x || x > max_x || y < min_y || y > max_y {
                continue;
            }
            if let Some(a) = world.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z());
            }
            break;
        }
    }
}

/// Tick gradual light fades.
pub fn tick_fades(world: &mut World) {
    let mut i = 0;
    while i < world.fades.len() {
        let fade = &mut world.fades[i];
        fade.tics -= 1;
        fade.current += fade.step;
        let light = fade.current.round().clamp(0.0, 255.0) as i16;
        let sid = fade.sector.raw() as usize;
        let done = fade.tics <= 0;
        let dest = fade.dest;
        if let Some(sec) = world.sectors.get_mut(sid) {
            sec.light = if done { dest } else { light };
        }
        if done {
            world.fades.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

/// Tick oscillating glow lights.
pub fn tick_glows(world: &mut World) {
    for glow in &mut world.glows {
        glow.tics -= 1;
        glow.current += glow.step;
        if glow.tics <= 0 {
            glow.dir = -glow.dir;
            glow.tics = glow.tics_max;
            let dest = if glow.dir > 0 {
                f64::from(glow.upper)
            } else {
                f64::from(glow.lower)
            };
            glow.step = if glow.tics_max > 0 {
                (dest - glow.current) / f64::from(glow.tics_max)
            } else {
                0.0
            };
        }
        if let Some(sec) = world.sectors.get_mut(glow.sector.raw() as usize) {
            sec.light = glow.current.round().clamp(0.0, 255.0) as i16;
        }
    }
}

/// Kill shootable actors overlapping `mo` after a teleport. `+NOTELEFRAG` is spared (UZ1).
fn telefrag_occupants(world: &mut World, mo: ActorId) {
    let Some(mover) = world.actors.get(mo).cloned() else {
        return;
    };
    let victims: Vec<ActorId> = world
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            if id == mo {
                return false;
            }
            let Some(a) = world.actors.get(id) else {
                return false;
            };
            if a.destroyed || a.health <= 0 || (a.flags & crate::class::FLAG_SHOOTABLE) == 0 {
                return false;
            }
            if a.extra_flags.get_name("NOTELEFRAG") {
                return false;
            }
            let dx = a.pos.x() - mover.pos.x();
            let dy = a.pos.y() - mover.pos.y();
            let reach = a.radius + mover.radius;
            if dx * dx + dy * dy > reach * reach {
                return false;
            }
            let top = (a.pos.z() + a.height).min(mover.pos.z() + mover.height);
            let bot = a.pos.z().max(mover.pos.z());
            top > bot
        })
        .collect();
    for id in victims {
        let _ = damage_mobj(world, id, Some(mo), Some(mo), 10_000);
    }
}

fn teleport(
    world: &mut World,
    args: &[i32; 5],
    activator: Option<ActorId>,
    source_fog: bool,
    dest_fog: bool,
    keep_velocity: bool,
) -> i32 {
    let Some(mo) = activator else {
        return 0;
    };
    // Missiles never teleport (Doom `EV_Teleport`).
    if world
        .actors
        .get(mo)
        .is_some_and(|a| (a.flags & crate::class::FLAG_MISSILE) != 0)
    {
        return 0;
    }
    // XLAT Doom Teleport: args = [tid, tag, …].
    // tid≠0 → Hexen-style TID land; tid=0 → Doom `EV_Teleport` (TeleportMan in tagged sector).
    let tid = args[0];
    let tag = args[1];
    let Some((pos, ang)) = find_teleport_dest(world, tid, tag) else {
        return 0;
    };
    let old_pos = world.actors.get(mo).map(|a| a.pos).unwrap_or(pos);
    let old_vel = world.actors.get(mo).map(|a| a.vel).unwrap_or(DVec3::ZERO);
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = pos;
        a.vel = if keep_velocity { old_vel } else { DVec3::ZERO };
        a.angle = ang;
    }
    // Doom: `thing->z = thing->floorz` after `P_TeleportMove` (non-Final Doom).
    let floor_z = if world.math_mode == crate::vanilla::SimMathMode::VanillaFixed {
        world
            .actors
            .get(mo)
            .map(|a| crate::vanilla::check_position_fixed(world, a, a.pos, Some(mo)).floor_z)
            .unwrap_or(pos.z())
    } else {
        world
            .actors
            .get(mo)
            .map(|a| crate::physics::check_position(world, a, a.pos, Some(mo)).floor_z)
            .unwrap_or(pos.z())
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = DVec3::new(a.pos.x(), a.pos.y(), floor_z);
    }
    telefrag_occupants(world, mo);
    if source_fog {
        spawn_teleport_fog_of(world, mo, old_pos, false);
    }
    if dest_fog {
        // Doom: dest fog at `m->x + 20*finecosine[an]`, `m->y + 20*finesine[an]`.
        let fine = room_core::math::bam_to_fine(ang.to_bam().0);
        let fog_x = pos.x() + f64::from(20 * room_core::math::finecosine(fine)) / 65536.0;
        let fog_y = pos.y() + f64::from(20 * room_core::math::finesine(fine)) / 65536.0;
        spawn_teleport_fog_of(world, mo, DVec3::new(fog_x, fog_y, floor_z), true);
    }
    // Player reactiontime stall — monsters move immediately (`Teleport_NoStop` skips).
    if !keep_velocity && world.players.iter().flatten().any(|p| p.mo == Some(mo)) {
        // `Player.TeleportFreezeTime` (default 18).
        let freeze = world
            .actors
            .get(mo)
            .and_then(|a| world.classes.get(a.class))
            .map(|c| c.ext.teleport_freeze_time)
            .filter(|t| *t >= 0)
            .unwrap_or(18);
        if let Some(a) = world.actors.get_mut(mo) {
            a.reaction_time = freeze;
        }
    }
    1
}

/// Resolve teleport landing: TID, else Doom tag→TeleportMan, else first dest.
fn find_teleport_dest(world: &World, tid: i32, tag: i32) -> Option<(DVec3, Angle)> {
    if tid != 0 {
        if let Some(dest_id) = world.find_by_tid(tid) {
            let dest = world.actors.get(dest_id)?;
            return Some((dest.pos, dest.angle));
        }
        let dest = world
            .map
            .things
            .iter()
            .find(|t| t.type_id == THING_TELEPORT_DEST && i32::from(t.tid) == tid)?;
        return Some((
            DVec3::new(f64::from(dest.x), f64::from(dest.y), f64::from(dest.z)),
            Angle::from_degrees(f64::from(dest.angle)),
        ));
    }
    if tag != 0 {
        // Doom `EV_Teleport`: live `MT_TELEPORTMAN` whose subsector sector has `tag`.
        let tele_class = world.classes.find_doomednum(i32::from(THING_TELEPORT_DEST));
        for &id in &world.thinkers.default {
            let Some(a) = world.actors.get(id) else {
                continue;
            };
            if a.destroyed {
                continue;
            }
            let is_dest = tele_class.is_some_and(|c| a.class == c)
                || world
                    .classes
                    .get(a.class)
                    .is_some_and(|c| c.name.eq_ignore_ascii_case("TeleportDest"));
            if !is_dest {
                continue;
            }
            let Some(sid) = room_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y())
            else {
                continue;
            };
            let Some(sec) = world.map.sectors.get(sid.raw() as usize) else {
                continue;
            };
            if sec.tags.contains(&tag) {
                return Some((a.pos, a.angle));
            }
        }
        // Fallback: map things (pre-spawn / missing class).
        for t in &world.map.things {
            if t.type_id != THING_TELEPORT_DEST {
                continue;
            }
            let Some(sid) =
                room_nodebuild::point_in_sector(&world.map, f64::from(t.x), f64::from(t.y))
            else {
                continue;
            };
            let Some(sec) = world.map.sectors.get(sid.raw() as usize) else {
                continue;
            };
            if sec.tags.contains(&tag) {
                return Some((
                    DVec3::new(f64::from(t.x), f64::from(t.y), f64::from(t.z)),
                    Angle::from_degrees(f64::from(t.angle)),
                ));
            }
        }
        return None;
    }
    let dest = world
        .map
        .things
        .iter()
        .find(|t| t.type_id == THING_TELEPORT_DEST)?;
    Some((
        DVec3::new(f64::from(dest.x), f64::from(dest.y), f64::from(dest.z)),
        Angle::from_degrees(f64::from(dest.angle)),
    ))
}

/// Spawn `TeleportFog` (when defined) and play `misc/teleport` at `pos`.
pub(crate) fn spawn_teleport_fog(world: &mut World, pos: DVec3) {
    world.play_sound("misc/teleport", pos.x(), pos.y(), pos.z(), 1.0);
    if let Some(class) = world.classes.find_name("TeleportFog") {
        let _ = world.spawn_at(class, pos, Angle::ZERO);
    }
}

/// Teleport fog for `mo`: `A_SetTeleFog`'s class, else the class
/// `TelefogSourceType` / `TelefogDestType`, else `TeleportFog`. `"None"`
/// spawns no fog (the sound still plays).
pub(crate) fn spawn_teleport_fog_of(world: &mut World, mo: ActorId, pos: DVec3, dest: bool) {
    let name = world.actors.get(mo).and_then(|a| {
        let own = if dest {
            &a.telefog_dest
        } else {
            &a.telefog_source
        };
        if !own.is_empty() {
            return Some(own.clone());
        }
        let c = world.classes.get(a.class)?;
        let t = if dest {
            &c.ext.telefog_dest_type
        } else {
            &c.ext.telefog_source_type
        };
        (!t.is_empty()).then(|| t.clone())
    });
    let Some(name) = name else {
        spawn_teleport_fog(world, pos);
        return;
    };
    world.play_sound("misc/teleport", pos.x(), pos.y(), pos.z(), 1.0);
    if name.eq_ignore_ascii_case("None") {
        return;
    }
    if let Some(class) = world.classes.find_name(&name) {
        let _ = world.spawn_at(class, pos, Angle::ZERO);
    }
}

/// Hexen specials a non-player may activate via Use (`P_UseSpecialLine` door set).
fn monster_can_use_special(special: u16) -> bool {
    matches!(
        special,
        num::DOOR_RAISE
            | num::DOOR_OPEN
            | num::DOOR_CLOSE
            | num::DOOR_LOCKED_RAISE
            | num::DOOR_CLOSE_WAIT_OPEN
    )
}

fn teleport_line(
    world: &mut World,
    args: &[i32; 5],
    activator: Option<ActorId>,
    source: Option<LineId>,
) -> i32 {
    // Teleport_Line(thisid, destid, flip) — silent Boom line teleport.
    let Some(mo) = activator else {
        return 0;
    };
    let dest_tag = args[1];
    if dest_tag == 0 {
        return 0;
    }
    let flip = args[2] != 0;
    let dest_li = world.map.lines.iter().enumerate().find_map(|(i, ln)| {
        let id = LineId::from_raw(i as u32);
        if source == Some(id) {
            return None;
        }
        if i32::from(ln.tag) == dest_tag {
            Some(id)
        } else {
            None
        }
    });
    let Some(dest_li) = dest_li else {
        return 0;
    };
    let Some(dest) = world.map.lines.get(dest_li.raw() as usize) else {
        return 0;
    };
    let Some(v1) = world.map.vertices.get(dest.v1.raw() as usize) else {
        return 0;
    };
    let Some(v2) = world.map.vertices.get(dest.v2.raw() as usize) else {
        return 0;
    };
    let mid_x = (v1.pos.x() + v2.pos.x()) * 0.5;
    let mid_y = (v1.pos.y() + v2.pos.y()) * 0.5;
    // Nudge onto the front side so the activator isn't stuck on the line.
    let len =
        room_core::dmath::sqrt(dest.delta.x() * dest.delta.x() + dest.delta.y() * dest.delta.y());
    let (nx, ny) = if len > 1e-6 {
        (-dest.delta.y() / len, dest.delta.x() / len)
    } else {
        (0.0, 0.0)
    };
    let z = dest
        .front_sector
        .and_then(|s| world.sectors.get(s.raw() as usize))
        .map(|s| s.floor_z)
        .unwrap_or(0.0);
    let pos = DVec3::new(mid_x + nx * 16.0, mid_y + ny * 16.0, z);
    let mut ang = Angle::from_radians(room_core::dmath::atan2(dest.delta.y(), dest.delta.x()));
    if flip {
        ang = Angle::from_degrees(ang.degrees() + 180.0).normalized();
    }
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = pos;
        a.vel = DVec3::ZERO;
        a.angle = ang;
    }
    1
}

fn plat_dwus(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    perpetual: bool,
    lip: f64,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let wait = if args[2] != 0 { args[2] } else { 105 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        // Vanilla `EV_DoPlat`: sector already busy → skip (WR lifts must not stack).
        if world.plats.iter().any(|p| p.sector == sid)
            || world.floors.iter().any(|f| f.sector == sid)
        {
            continue;
        }
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let high = sec.floor_z;
        // Doom `P_FindLowestFloorSurrounding` + lip (not `high - lip`).
        let (min_f, _, _, _) = neighbor_plane_extrema(world, sid);
        let low = min_f.unwrap_or(high) + lip;
        if low >= high - 0.001 {
            continue;
        }
        world.plats.push(PlatMover {
            sector: sid,
            status: -1,
            speed,
            low,
            high,
            wait: 0,
            wait_max: wait,
            perpetual,
            stop_at_low: false,
        });
        play_sector_sound(world, sid, "platforms/start");
        started += 1;
    }
    i32::from(started > 0)
}

/// `Floor_Waggle` / `Ceiling_Waggle` — `(tag, amp, freq, delay, time)`.
fn start_waggle(world: &mut World, args: &[i32; 5], line: Option<LineId>, floor: bool) -> i32 {
    let tag = args[0];
    // Hexen: amp << (FRACBITS-3) → map units = amp/8; freq similarly scaled.
    let amplitude = f64::from(if args[1] != 0 { args[1] } else { 24 }) / 8.0;
    let freq =
        f64::from(if args[2] != 0 { args[2] } else { 32 }) * (std::f64::consts::TAU / 256.0) / 8.0;
    let delay = args[3].max(0);
    // GZDoom: time<<2 tics when non-zero.
    let time_left = if args[4] != 0 {
        args[4].saturating_mul(4)
    } else {
        0
    };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let base_z = if floor { sec.floor_z } else { sec.ceiling_z };
        world.waggles.push(PlaneWaggle {
            sector: sid,
            floor,
            base_z,
            amplitude,
            freq,
            phase: 0.0,
            delay,
            time_left,
        });
        n += 1;
    }
    i32::from(n > 0)
}

/// Tick plane waggles.
pub fn tick_waggles(world: &mut World) {
    let mut i = 0;
    while i < world.waggles.len() {
        if world.waggles[i].delay > 0 {
            world.waggles[i].delay -= 1;
            i += 1;
            continue;
        }
        let sector = world.waggles[i].sector;
        let floor = world.waggles[i].floor;
        let base_z = world.waggles[i].base_z;
        let amplitude = world.waggles[i].amplitude;
        let freq = world.waggles[i].freq;
        world.waggles[i].phase += freq;
        let phase = world.waggles[i].phase;
        let z = base_z + amplitude * room_core::dmath::sin(phase);
        let Some(sec) = world.sectors.get_mut(sector.raw() as usize) else {
            world.waggles.swap_remove(i);
            continue;
        };
        if floor {
            sec.floor_z = z;
        } else {
            sec.ceiling_z = z.max(sec.floor_z + 1.0);
        }
        if world.waggles[i].time_left > 0 {
            world.waggles[i].time_left -= 1;
            if world.waggles[i].time_left == 0 {
                if floor {
                    sec.floor_z = base_z;
                } else {
                    sec.ceiling_z = base_z;
                }
                world.waggles.swap_remove(i);
                continue;
            }
        }
        i += 1;
    }
}

/// `TeleportInSector(tag, source_tid, dest_tid, fog, group_tid)`.
fn teleport_in_sector(world: &mut World, args: &[i32; 5]) -> i32 {
    let tag = args[0];
    if tag == 0 {
        return 0;
    }
    let sources = actors_with_tid(world, args[1]);
    let dests = actors_with_tid(world, args[2]);
    let Some(&src_id) = sources.first() else {
        return 0;
    };
    let Some(&dst_id) = dests.first() else {
        return 0;
    };
    let Some(src_pos) = world.actors.get(src_id).map(|a| a.pos) else {
        return 0;
    };
    let Some((dst_pos, dst_ang)) = world.actors.get(dst_id).map(|a| (a.pos, a.angle)) else {
        return 0;
    };
    let dx = dst_pos.x() - src_pos.x();
    let dy = dst_pos.y() - src_pos.y();
    let dz = dst_pos.z() - src_pos.z();
    let fog = args[3] != 0;
    let group_tid = args[4];
    let tagged: std::collections::BTreeSet<u32> = world
        .map
        .sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| s.tags.contains(&tag))
        .map(|(i, _)| i as u32)
        .collect();
    if tagged.is_empty() {
        return 0;
    }
    let ids: Vec<_> = world.thinkers.default.iter().copied().collect();
    let mut n = 0;
    for id in ids {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed {
            continue;
        }
        if group_tid != 0 && a.tid != group_tid {
            continue;
        }
        let Some(sid) = room_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y()) else {
            continue;
        };
        if !tagged.contains(&sid.raw()) {
            continue;
        }
        let old = a.pos;
        if let Some(a) = world.actors.get_mut(id) {
            a.pos = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z() + dz);
            a.angle = dst_ang;
            a.vel = DVec3::ZERO;
        }
        let new_pos = world.actors.get(id).map(|a| a.pos).unwrap_or(old);
        if fog {
            spawn_teleport_fog_of(world, id, old, false);
            spawn_teleport_fog_of(world, id, new_pos, true);
        }
        n += 1;
    }
    i32::from(n > 0)
}

/// `TeleportOther(other_tid, dest_tid, fog)`.
fn teleport_other(world: &mut World, args: &[i32; 5]) -> i32 {
    let movers = actors_with_tid(world, args[0]);
    let dests = actors_with_tid(world, args[1]);
    let Some(&dest_id) = dests.first() else {
        return 0;
    };
    let Some(dest) = world.actors.get(dest_id).map(|a| (a.pos, a.angle)) else {
        return 0;
    };
    if movers.is_empty() {
        return 0;
    }
    let fog = args[2] != 0;
    for id in movers {
        let old = world.actors.get(id).map(|a| a.pos).unwrap_or(dest.0);
        if let Some(a) = world.actors.get_mut(id) {
            a.pos = dest.0;
            a.angle = dest.1;
            a.vel = DVec3::ZERO;
        }
        if fog {
            spawn_teleport_fog_of(world, id, old, false);
            spawn_teleport_fog_of(world, id, dest.0, true);
        }
    }
    1
}

/// `TeleportGroup(group_tid, source_tid, dest_tid, move_source, fog)`.
///
/// Moves every actor with `group_tid` by the vector from the first `source_tid`
/// actor to the first `dest_tid` actor (GZDoom group-relative teleport subset).
fn teleport_group(world: &mut World, args: &[i32; 5]) -> i32 {
    let source = actors_with_tid(world, args[1]);
    let dests = actors_with_tid(world, args[2]);
    let Some(&src_id) = source.first() else {
        return 0;
    };
    let Some(&dst_id) = dests.first() else {
        return 0;
    };
    let Some(src_pos) = world.actors.get(src_id).map(|a| a.pos) else {
        return 0;
    };
    let Some((dst_pos, dst_ang)) = world.actors.get(dst_id).map(|a| (a.pos, a.angle)) else {
        return 0;
    };
    let dx = dst_pos.x() - src_pos.x();
    let dy = dst_pos.y() - src_pos.y();
    let dz = dst_pos.z() - src_pos.z();
    let mut movers = actors_with_tid(world, args[0]);
    if args[3] != 0 {
        movers.push(src_id);
    }
    if movers.is_empty() {
        return 0;
    }
    let fog = args[4] != 0;
    let mut n = 0;
    for id in movers {
        let Some(a) = world.actors.get_mut(id) else {
            continue;
        };
        let old = a.pos;
        a.pos = DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z() + dz);
        a.angle = dst_ang;
        a.vel = DVec3::ZERO;
        let new_pos = a.pos;
        if fog {
            spawn_teleport_fog_of(world, id, old, false);
            spawn_teleport_fog_of(world, id, new_pos, true);
        }
        n += 1;
    }
    i32::from(n > 0)
}

/// `Plat_DownByValue(tag, speed, delay, height)` — lower by height×8, wait, rise.
fn plat_down_by_value(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let wait = if args[2] != 0 { args[2] } else { 105 };
    let units = if args[3] != 0 { args[3] } else { 1 };
    let drop = f64::from(units) * 8.0;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let high = sec.floor_z;
        world.plats.push(PlatMover {
            sector: sid,
            status: -1,
            speed,
            low: high - drop,
            high,
            wait: 0,
            wait_max: wait,
            perpetual: false,
            stop_at_low: false,
        });
        play_sector_sound(world, sid, "platforms/start");
    }
    1
}

/// `Plat_UpWaitDownStay(tag, speed, delay)` — raise to next higher floor, wait, lower.
fn plat_up_wait_down_stay(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let wait = if args[2] != 0 { args[2] } else { 105 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let low = sec.floor_z;
        let Some(high) = nearest_higher_floor(world, sid) else {
            continue;
        };
        if high <= low + 0.001 {
            continue;
        }
        world.plats.push(PlatMover {
            sector: sid,
            status: 1,
            speed,
            low,
            high,
            wait: 0,
            wait_max: wait,
            perpetual: true,
            stop_at_low: true,
        });
        play_sector_sound(world, sid, "platforms/start");
        n += 1;
    }
    i32::from(n > 0)
}

/// `Plat_UpByValue(tag, speed, delay, height)` — raise by height×8, wait, lower.
fn plat_up_by_value(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let wait = if args[2] != 0 { args[2] } else { 105 };
    let units = if args[3] != 0 { args[3] } else { 1 };
    let raise = f64::from(units) * 8.0;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let low = sec.floor_z;
        world.plats.push(PlatMover {
            sector: sid,
            status: 1,
            speed,
            low,
            high: low + raise,
            wait: 0,
            wait_max: wait,
            perpetual: true,
            stop_at_low: true,
        });
        play_sector_sound(world, sid, "platforms/start");
    }
    1
}

/// `Plat_UpNearestWaitDownStay(tag, speed, delay)`.
fn plat_up_nearest_wait_down_stay(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let wait = if args[2] != 0 { args[2] } else { 105 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let low = sec.floor_z;
        let Some(high) = nearest_higher_floor(world, sid) else {
            continue;
        };
        if high <= low + 0.001 {
            continue;
        }
        world.plats.push(PlatMover {
            sector: sid,
            status: 1,
            speed,
            low,
            high,
            wait: 0,
            wait_max: wait,
            perpetual: true,
            stop_at_low: false,
        });
        play_sector_sound(world, sid, "platforms/start");
        n += 1;
    }
    i32::from(n > 0)
}

/// `Line_SetAutomapFlags(id, setbits, clearbits)` — bit0 SECRET,1 DONTDRAW,2 MAPPED,3 REVEALED.
fn line_set_automap_flags(world: &mut World, args: &[i32; 5]) -> i32 {
    let line_id = args[0];
    if line_id == 0 {
        return 0;
    }
    const TRANS: [i32; 4] = [
        crate::world::ML_SECRET,
        crate::world::ML_DONTDRAW,
        crate::world::ML_MAPPED,
        crate::world::ML_REVEALED,
    ];
    let mut set_ml = 0i32;
    let mut clear_ml = 0i32;
    let mut set_bits = args[1];
    let mut clear_bits = args[2];
    for &flag in &TRANS {
        if set_bits & 1 != 0 {
            set_ml |= flag;
        }
        if clear_bits & 1 != 0 {
            clear_ml |= flag;
        }
        set_bits >>= 1;
        clear_bits >>= 1;
    }
    let mut n = 0;
    for (i, ld) in world.map.lines.iter().enumerate() {
        if i32::from(ld.tag) != line_id {
            continue;
        }
        let li = i as u32;
        let base = world
            .line_flags
            .get(&li)
            .copied()
            .unwrap_or_else(|| crate::world::base_line_flags(ld));
        world.line_flags.insert(li, (base | set_ml) & !clear_ml);
        n += 1;
    }
    i32::from(n > 0)
}

/// `Line_SetAutomapStyle(id, style)`.
fn line_set_automap_style(world: &mut World, args: &[i32; 5]) -> i32 {
    let line_id = args[0];
    if line_id == 0 {
        return 0;
    }
    let style = args[1];
    let mut n = 0;
    let map = std::sync::Arc::make_mut(&mut world.map);
    for (i, ld) in map.lines.iter_mut().enumerate() {
        if !ld.has_id(line_id) {
            continue;
        }
        ld.automap_style = style;
        world.line_automap_style.insert(i as u32, style);
        n += 1;
    }
    i32::from(n > 0)
}

fn generic_lift(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Generic_Lift(tag, speed, delay, type, height)
    // type: 0 up-by-value×8; 1 DWUS lip8; 2 down-to-nearest; 3 down-to-lowest-ceiling; 4 perpetual
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 16);
    let wait = if args[2] != 0 { args[2] } else { 105 };
    let lift_type = args[3];
    let height_units = if args[4] != 0 { args[4] } else { 1 };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx).cloned() else {
            continue;
        };
        let (min_f, max_f, min_c, _) = neighbor_plane_extrema(world, sid);
        match lift_type {
            0 => {
                // Raise by value×8 and stay (Boom "up by value" subset).
                let raise = f64::from(height_units) * 8.0;
                world.floors.push(FloorMover {
                    sector: sid,
                    direction: 1,
                    speed,
                    dest_z: sec.floor_z + raise,
                    crush: false,
                });
                play_sector_sound(world, sid, "platforms/start");
                started += 1;
            }
            1..=4 => {
                let high = sec.floor_z;
                let low = match lift_type {
                    1 => high - 8.0, // lip 8
                    2 => min_f.filter(|z| *z < high - 0.001).unwrap_or(high - 64.0),
                    3 => min_c
                        .map(|c| c.min(high))
                        .filter(|z| *z < high - 0.001)
                        .unwrap_or(high - 64.0),
                    _ => min_f.unwrap_or(high - 64.0).min(high), // perpetual uses lowest neighbor
                };
                // Perpetual also needs a real high above current when already at lowest.
                let high = if lift_type == 4 {
                    max_f.filter(|z| *z > high + 0.001).unwrap_or(high + 64.0)
                } else {
                    high
                };
                let low = if lift_type == 4 {
                    min_f.filter(|z| *z < high - 0.001).unwrap_or(high - 64.0)
                } else {
                    low
                };
                world.plats.push(PlatMover {
                    sector: sid,
                    status: -1,
                    speed,
                    low: low.min(high),
                    high: high.max(low),
                    wait: 0,
                    wait_max: wait,
                    perpetual: lift_type == 4,
                    stop_at_low: false,
                });
                play_sector_sound(world, sid, "platforms/start");
                started += 1;
            }
            _ => {}
        }
    }
    i32::from(started > 0)
}

/// Copy floor flat from the activating line's front sector (trigger model).
fn apply_floor_tex_change(world: &mut World, sector: SectorId, line: Option<LineId>) {
    let idx = sector.raw();
    let pic = line
        .and_then(|lid| world.map.lines.get(lid.raw() as usize))
        .and_then(|l| l.front_sector)
        .and_then(|fs| floor_pic_of(world, fs));
    if let Some(pic) = pic {
        world.sector_floor_pics.insert(idx, pic);
    }
}

fn floor_pic_of(world: &World, sector: SectorId) -> Option<String> {
    let fi = sector.raw();
    world.sector_floor_pics.get(&fi).cloned().or_else(|| {
        world
            .map
            .sectors
            .get(fi as usize)
            .map(|s| s.floor_pic.clone())
    })
}

/// Boom numeric model: copy floor pic (and optionally special) from a neighbor
/// whose floor height matches `model_z`.
fn apply_floor_tex_change_numeric(
    world: &mut World,
    sector: SectorId,
    model_z: f64,
    copy_special: bool,
) {
    let idx = sector.raw() as usize;
    let Some(geom) = world.map.sectors.get(idx) else {
        return;
    };
    let mut best: Option<(f64, SectorId)> = None;
    for &lid in &geom.lines {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        for other in [line.front_sector, line.back_sector].into_iter().flatten() {
            if other == sector {
                continue;
            }
            let Some(sec) = world.sectors.get(other.raw() as usize) else {
                continue;
            };
            let d = (sec.floor_z - model_z).abs();
            best = Some(match best {
                Some((bd, sid)) if d >= bd => (bd, sid),
                _ => (d, other),
            });
        }
    }
    let Some((_, model)) = best else {
        return;
    };
    if let Some(pic) = floor_pic_of(world, model) {
        world.sector_floor_pics.insert(sector.raw(), pic);
    }
    if copy_special {
        let sp = world
            .sector_specials
            .get(&model.raw())
            .copied()
            .or_else(|| {
                world
                    .map
                    .sectors
                    .get(model.raw() as usize)
                    .map(|s| s.special)
            })
            .unwrap_or(0);
        world.sector_specials.insert(sector.raw(), sp);
    }
}

/// Copy floor flat + clear special from the activating line's front sector (Tx0 model).
fn apply_tx0_change(world: &mut World, sector: SectorId, line: Option<LineId>) {
    apply_floor_tex_change(world, sector, line);
    world.sector_specials.insert(sector.raw(), 0);
}

/// Next higher neighboring floor height, or `None` if already highest.
fn nearest_higher_floor(world: &World, sector: SectorId) -> Option<f64> {
    let idx = sector.raw() as usize;
    let cur = world.sectors.get(idx)?.floor_z;
    let geom = world.map.sectors.get(idx)?;
    let mut best: Option<f64> = None;
    for &lid in &geom.lines {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        for other in [line.front_sector, line.back_sector].into_iter().flatten() {
            if other == sector {
                continue;
            }
            let Some(sec) = world.sectors.get(other.raw() as usize) else {
                continue;
            };
            if sec.floor_z > cur + 0.001 {
                best = Some(match best {
                    Some(b) => b.min(sec.floor_z),
                    None => sec.floor_z,
                });
            }
        }
    }
    best
}

/// Neighbor floor/ceiling extremes for Boom generic movers.
fn neighbor_plane_extrema(
    world: &World,
    sector: SectorId,
) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    let idx = sector.raw() as usize;
    let Some(geom) = world.map.sectors.get(idx) else {
        return (None, None, None, None);
    };
    let mut min_floor = None;
    let mut max_floor = None;
    let mut min_ceil = None;
    let mut max_ceil = None;
    for &lid in &geom.lines {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        for other in [line.front_sector, line.back_sector].into_iter().flatten() {
            if other == sector {
                continue;
            }
            let Some(sec) = world.sectors.get(other.raw() as usize) else {
                continue;
            };
            min_floor = Some(min_floor.map_or(sec.floor_z, |v: f64| v.min(sec.floor_z)));
            max_floor = Some(max_floor.map_or(sec.floor_z, |v: f64| v.max(sec.floor_z)));
            min_ceil = Some(min_ceil.map_or(sec.ceiling_z, |v: f64| v.min(sec.ceiling_z)));
            max_ceil = Some(max_ceil.map_or(sec.ceiling_z, |v: f64| v.max(sec.ceiling_z)));
        }
    }
    (min_floor, max_floor, min_ceil, max_ceil)
}

fn generic_floor(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let height = f64::from(args[2]);
    let target = args[3];
    let flags = args[4];
    let raise = (flags & 8) != 0;
    let crush = (flags & 16) != 0;
    let change_bits = flags & 3;
    let change_tex = change_bits != 0;
    let clear_special = change_bits == 1;
    let copy_special = change_bits == 3;
    let numeric_model = (flags & 4) != 0;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx).cloned() else {
            continue;
        };
        let (min_f, max_f, min_c, _max_c) = neighbor_plane_extrema(world, sid);
        let dest = match target {
            0 => {
                if raise {
                    sec.floor_z + height
                } else {
                    sec.floor_z - height
                }
            }
            1 => max_f.unwrap_or(sec.floor_z),
            2 => min_f.unwrap_or(sec.floor_z),
            3 => {
                // Nearest surrounding floor.
                let mut best = None;
                for cand in [min_f, max_f].into_iter().flatten() {
                    let d = (cand - sec.floor_z).abs();
                    best = Some(match best {
                        Some((bd, bz)) if d >= bd => (bd, bz),
                        _ => (d, cand),
                    });
                }
                best.map(|(_, z)| z).unwrap_or(sec.floor_z)
            }
            4 => min_c.unwrap_or(sec.ceiling_z),
            5 => sec.ceiling_z,
            6 => {
                let delta = shortest_lower_wall_height(world, sid).unwrap_or(0.0);
                if raise {
                    sec.floor_z + delta
                } else {
                    sec.floor_z - delta
                }
            }
            _ => sec.floor_z,
        };
        if (dest - sec.floor_z).abs() < 0.001 {
            continue;
        }
        let direction = if dest > sec.floor_z { 1 } else { -1 };
        world.floors.push(FloorMover {
            sector: sid,
            direction,
            speed,
            dest_z: dest,
            crush: crush && direction > 0,
        });
        if change_tex {
            if numeric_model {
                apply_floor_tex_change_numeric(world, sid, dest, copy_special);
            } else {
                apply_floor_tex_change(world, sid, line);
            }
        }
        if clear_special {
            world.sector_specials.insert(idx as u32, 0);
        } else if copy_special && !numeric_model {
            // Trigger model: copy special from line front sector.
            let sp = line
                .and_then(|lid| world.map.lines.get(lid.raw() as usize))
                .and_then(|l| l.front_sector)
                .map(|fs| {
                    world
                        .sector_specials
                        .get(&fs.raw())
                        .copied()
                        .or_else(|| world.map.sectors.get(fs.raw() as usize).map(|s| s.special))
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            world.sector_specials.insert(idx as u32, sp);
        }
        started += 1;
    }
    i32::from(started > 0)
}

fn generic_ceiling(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let height = f64::from(args[2]);
    let target = args[3];
    let flags = args[4];
    let raise = (flags & 8) != 0;
    let crush = (flags & 16) != 0;
    let clear_special = (flags & 1) != 0;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx).cloned() else {
            continue;
        };
        let (_min_f, max_f, min_c, max_c) = neighbor_plane_extrema(world, sid);
        let dest = match target {
            0 => {
                if raise {
                    sec.ceiling_z + height
                } else {
                    (sec.ceiling_z - height).max(sec.floor_z)
                }
            }
            1 => {
                if raise {
                    max_c.unwrap_or(sec.ceiling_z)
                } else {
                    max_c.unwrap_or(sec.ceiling_z).min(sec.ceiling_z)
                }
            }
            2 => min_c.unwrap_or(sec.ceiling_z),
            3 => {
                // Nearest surrounding ceiling (closest absolute delta).
                let mut best = None;
                for cand in [min_c, max_c].into_iter().flatten() {
                    let d = (cand - sec.ceiling_z).abs();
                    best = Some(match best {
                        Some((bd, bz)) if d >= bd => (bd, bz),
                        _ => (d, cand),
                    });
                }
                best.map(|(_, z)| z).unwrap_or(sec.ceiling_z)
            }
            4 => max_f.unwrap_or(sec.floor_z),
            5 => sec.floor_z,
            6 => {
                let delta = shortest_lower_wall_height(world, sid).unwrap_or(0.0);
                if raise {
                    sec.ceiling_z + delta
                } else {
                    (sec.ceiling_z - delta).max(sec.floor_z)
                }
            }
            _ => sec.ceiling_z,
        };
        if (dest - sec.ceiling_z).abs() < 0.001 {
            continue;
        }
        let direction = if dest > sec.ceiling_z { 1 } else { -1 };
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction,
            speed,
            speed_up: speed,
            bottom_z: dest.min(sec.ceiling_z),
            top_z: dest.max(sec.ceiling_z),
            crush,
            once: true,
            stop_at_high: false,
        });
        if clear_special {
            world.sector_specials.insert(idx as u32, 0);
        }
        started += 1;
    }
    i32::from(started > 0)
}

fn plat_raise_and_stay_tx0(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 4);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let Some(dest) = nearest_higher_floor(world, sid) else {
            continue;
        };
        apply_tx0_change(world, sid, line);
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: dest,
            crush: false,
        });
        play_sector_sound(world, sid, "platforms/start");
        started += 1;
    }
    i32::from(started > 0)
}

fn plat_up_by_value_stay_tx(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed_or(args[1], 4);
    // Height is in units of 8 (Doom `Plat_UpByValueStayTx`).
    let units = if args[2] != 0 { args[2] } else { 1 };
    let raise = f64::from(units) * 8.0;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let dest = sec.floor_z + raise;
        apply_tx0_change(world, sid, line);
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: dest,
            crush: false,
        });
        play_sector_sound(world, sid, "platforms/start");
    }
    1
}

fn plat_stop(world: &mut World, args: &[i32; 5]) -> i32 {
    let tag = args[0];
    let keep: Vec<_> = world
        .plats
        .iter()
        .filter(|p| {
            world
                .map
                .sectors
                .get(p.sector.raw() as usize)
                .is_none_or(|s| tag != 0 && !s.tags.contains(&tag))
        })
        .cloned()
        .collect();
    let removed = world.plats.len() != keep.len();
    world.plats = keep;
    i32::from(removed)
}

fn floor_crush_stop(world: &mut World, args: &[i32; 5]) -> i32 {
    let tag = args[0];
    let keep: Vec<_> = world
        .floors
        .iter()
        .filter(|f| {
            world
                .map
                .sectors
                .get(f.sector.raw() as usize)
                .is_none_or(|s| tag != 0 && !s.tags.contains(&tag))
        })
        .cloned()
        .collect();
    let removed = world.floors.len() != keep.len();
    world.floors = keep;
    i32::from(removed)
}

fn floor_donut(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Doom `EV_DoDonut`: tagged pillar (s1) lowers to outer floor (s3);
    // adjacent pool (s2) moves to s3 and copies its floor flat / clears special.
    let tag = args[0];
    let pillar_speed = hexen_speed_or(args[1], 4);
    let pool_speed = if args[2] != 0 {
        hexen_speed(args[2])
    } else {
        pillar_speed
    };
    let pillars = sectors_for_tag(world, tag, line);
    if pillars.is_empty() {
        return 0;
    }
    let mut started = 0;
    for s1 in pillars {
        let Some(s2) = donut_pool_sector(world, s1) else {
            continue;
        };
        let Some((s3_z, s3_pic)) = donut_outer_model(world, s2, s1) else {
            continue;
        };
        // Rising/lowering pool copies outer floor.
        world.sector_floor_pics.insert(s2.raw(), s3_pic);
        world.sector_specials.insert(s2.raw(), 0);
        let pool_z = world
            .sectors
            .get(s2.raw() as usize)
            .map(|s| s.floor_z)
            .unwrap_or(s3_z);
        let pool_dir = if s3_z >= pool_z { 1 } else { -1 };
        world.floors.push(FloorMover {
            sector: s2,
            direction: pool_dir,
            speed: pool_speed,
            dest_z: s3_z,
            crush: false,
        });
        // Lowering pillar (donut hole).
        world.floors.push(FloorMover {
            sector: s1,
            direction: -1,
            speed: pillar_speed,
            dest_z: s3_z,
            crush: false,
        });
        started += 1;
    }
    i32::from(started > 0)
}

/// Pool sector adjacent to the pillar (first two-sided neighbor).
fn donut_pool_sector(world: &World, pillar: SectorId) -> Option<SectorId> {
    let geom = world.map.sectors.get(pillar.raw() as usize)?;
    for &lid in &geom.lines {
        let line = world.map.lines.get(lid.raw() as usize)?;
        for other in [line.front_sector, line.back_sector].into_iter().flatten() {
            if other != pillar {
                return Some(other);
            }
        }
    }
    None
}

/// Outer model sector (s3): two-sided neighbor of the pool that is not the pillar.
fn donut_outer_model(world: &World, pool: SectorId, pillar: SectorId) -> Option<(f64, String)> {
    let geom = world.map.sectors.get(pool.raw() as usize)?;
    for &lid in &geom.lines {
        let line = world.map.lines.get(lid.raw() as usize)?;
        if line.back_sector.is_none() {
            continue;
        }
        for other in [line.front_sector, line.back_sector].into_iter().flatten() {
            if other == pool || other == pillar {
                continue;
            }
            let z = world.sectors.get(other.raw() as usize)?.floor_z;
            let pic = world
                .sector_floor_pics
                .get(&other.raw())
                .cloned()
                .or_else(|| {
                    world
                        .map
                        .sectors
                        .get(other.raw() as usize)
                        .map(|s| s.floor_pic.clone())
                })
                .unwrap_or_default();
            return Some((z, pic));
        }
    }
    None
}

fn stairs_build_up(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Doom `EV_BuildStairs`: raise tagged sector, then walk adjacent same-height /
    // same-floorpic sectors, each raised one more step.
    stairs_build_dir(world, args, line, true, false)
}

fn stairs_build_dir(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    up: bool,
    sync: bool,
) -> i32 {
    build_stairs(
        world,
        StairBuild {
            tag: args[0],
            speed_arg: args[1],
            step_arg: args[2],
            up,
            sync,
            ignore_tex: false,
            crush: args[3] != 0,
            line,
        },
    )
}

fn generic_stairs(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Generic_Stairs(tag, speed, height, flags, reset)
    // flags bit0 = up; bit1 = ignore floor texture. `reset` deferred.
    let flags = args[3];
    build_stairs(
        world,
        StairBuild {
            tag: args[0],
            speed_arg: args[1],
            step_arg: args[2],
            up: (flags & 1) != 0,
            sync: false,
            ignore_tex: (flags & 2) != 0,
            crush: false,
            line,
        },
    )
}

struct StairBuild {
    tag: i32,
    speed_arg: i32,
    step_arg: i32,
    up: bool,
    /// Scale speed by step index so later stairs catch up (`*Sync`).
    sync: bool,
    ignore_tex: bool,
    crush: bool,
    line: Option<LineId>,
}

fn build_stairs(world: &mut World, p: StairBuild) -> i32 {
    let base_speed = if p.speed_arg != 0 {
        f64::from(p.speed_arg)
    } else {
        8.0
    };
    let step = if p.step_arg != 0 {
        f64::from(p.step_arg)
    } else {
        8.0
    };
    let starts = sectors_for_tag(world, p.tag, p.line);
    if starts.is_empty() {
        return 0;
    }
    let mut started = 0;
    let mut visited = std::collections::BTreeSet::new();
    for start in starts {
        if !visited.insert(start.raw()) {
            continue;
        }
        let idx = start.raw() as usize;
        let Some(sec0) = world.sectors.get(idx) else {
            continue;
        };
        let base_z = sec0.floor_z;
        let floor_pic = world
            .sector_floor_pics
            .get(&(start.raw()))
            .cloned()
            .unwrap_or_else(|| {
                world
                    .map
                    .sectors
                    .get(idx)
                    .map(|s| s.floor_pic.clone())
                    .unwrap_or_default()
            });
        let mut cur = start;
        let mut dest = if p.up { base_z + step } else { base_z - step };
        let mut step_i = 1u32;
        loop {
            let speed = if p.sync {
                base_speed * f64::from(step_i)
            } else {
                base_speed
            };
            world.floors.push(FloorMover {
                sector: cur,
                direction: if p.up { 1 } else { -1 },
                speed,
                dest_z: dest,
                crush: p.crush,
            });
            started += 1;
            let next = stair_next_sector(world, cur, base_z, &floor_pic, p.ignore_tex, &visited);
            let Some(next) = next else {
                break;
            };
            visited.insert(next.raw());
            cur = next;
            step_i = step_i.saturating_add(1);
            if p.up {
                dest += step;
            } else {
                dest -= step;
            }
        }
    }
    i32::from(started > 0)
}

/// Next Doom stair sector: adjacent, same original floor height (+ floor flat unless ignored).
fn stair_next_sector(
    world: &World,
    sector: SectorId,
    base_z: f64,
    floor_pic: &str,
    ignore_tex: bool,
    visited: &std::collections::BTreeSet<u32>,
) -> Option<SectorId> {
    let geom = world.map.sectors.get(sector.raw() as usize)?;
    for &lid in &geom.lines {
        let line = world.map.lines.get(lid.raw() as usize)?;
        if line.back_sector.is_none() {
            continue;
        }
        for other in [line.front_sector, line.back_sector].into_iter().flatten() {
            if other == sector || visited.contains(&other.raw()) {
                continue;
            }
            let oidx = other.raw() as usize;
            let sec = world.sectors.get(oidx)?;
            if (sec.floor_z - base_z).abs() > 0.001 {
                continue;
            }
            if !ignore_tex {
                let pic = world
                    .sector_floor_pics
                    .get(&other.raw())
                    .map(String::as_str)
                    .or_else(|| world.map.sectors.get(oidx).map(|s| s.floor_pic.as_str()))
                    .unwrap_or("");
                if !pic.eq_ignore_ascii_case(floor_pic) {
                    continue;
                }
            }
            return Some(other);
        }
    }
    None
}

fn ceiling_lower_to_floor(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction: -1,
            speed,
            speed_up: speed,
            bottom_z: sec.floor_z,
            top_z: sec.ceiling_z,
            crush: false,
            once: true,
            stop_at_high: false,
        });
    }
    1
}

fn ceiling_raise_to_nearest(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let (_, _, _, max_c) = neighbor_plane_extrema(world, sid);
        let top = max_c.unwrap_or(sec.ceiling_z + 64.0);
        if top <= sec.ceiling_z + 0.001 {
            continue;
        }
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction: 1,
            speed,
            speed_up: speed,
            bottom_z: sec.ceiling_z,
            top_z: top,
            crush: false,
            once: true,
            stop_at_high: false,
        });
        started += 1;
    }
    i32::from(started > 0)
}

fn ceiling_by_value(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    direction: i8,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let height = f64::from(if args[2] != 0 { args[2] } else { 24 });
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let dest = if direction > 0 {
            sec.ceiling_z + height
        } else {
            (sec.ceiling_z - height).max(sec.floor_z)
        };
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction,
            speed,
            speed_up: speed,
            bottom_z: dest.min(sec.ceiling_z),
            top_z: dest.max(sec.ceiling_z),
            crush: false,
            once: true,
            stop_at_high: false,
        });
    }
    1
}

fn ceiling_lower_to_lowest(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        // Bootstrap: drop 64 toward lowest surrounding ceiling.
        let bottom = (sec.ceiling_z - 64.0).max(sec.floor_z);
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction: -1,
            speed,
            speed_up: speed,
            bottom_z: bottom,
            top_z: sec.ceiling_z,
            crush: false,
            once: true,
            stop_at_high: false,
        });
    }
    1
}

fn ceiling_lower_to_highest_floor(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx).cloned() else {
            continue;
        };
        let (_min_f, max_f, _min_c, _max_c) = neighbor_plane_extrema(world, sid);
        let dest = max_f
            .unwrap_or(sec.floor_z)
            .max(sec.floor_z)
            .min(sec.ceiling_z);
        if dest >= sec.ceiling_z - 0.001 {
            continue;
        }
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction: -1,
            speed,
            speed_up: speed,
            bottom_z: dest,
            top_z: sec.ceiling_z,
            crush: false,
            once: true,
            stop_at_high: false,
        });
        started += 1;
    }
    i32::from(started > 0)
}

/// `FloorAndCeiling_LowerByValue` / `RaiseByValue` (tag, speed, height).
fn floor_and_ceiling_by_value(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    direction: i8,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let height = f64::from(args[2].abs());
    if speed <= 0.0 || height <= 0.0 {
        return 0;
    }
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let dir = f64::from(direction.signum());
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx).cloned() else {
            continue;
        };
        let floor_dest = sec.floor_z + dir * height;
        let ceil_dest = sec.ceiling_z + dir * height;
        // Keep a minimum gap so the planes do not cross.
        if ceil_dest <= floor_dest + 1.0 {
            continue;
        }
        world.floors.push(FloorMover {
            sector: sid,
            direction,
            speed,
            dest_z: floor_dest,
            crush: false,
        });
        let (bottom_z, top_z) = if direction < 0 {
            (ceil_dest, sec.ceiling_z)
        } else {
            (sec.ceiling_z, ceil_dest)
        };
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction,
            speed,
            speed_up: speed,
            bottom_z,
            top_z,
            crush: false,
            once: true,
            stop_at_high: false,
        });
        play_sector_sound(world, sid, "floors/move");
        started += 1;
    }
    i32::from(started > 0)
}

fn floor_and_ceiling_lower_raise(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // FloorAndCeiling_LowerRaise(tag, fspeed, cspeed[, boomemu])
    // Lower floor to lowest neighbor; raise ceiling to highest neighbor.
    let tag = args[0];
    let fspeed = hexen_speed(args[1]);
    let cspeed = if args[2] != 0 {
        hexen_speed(args[2])
    } else {
        fspeed
    };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx).cloned() else {
            continue;
        };
        let (min_f, _max_f, _min_c, max_c) = neighbor_plane_extrema(world, sid);
        let floor_dest = min_f.unwrap_or(sec.floor_z);
        if floor_dest < sec.floor_z - 0.001 {
            world.floors.push(FloorMover {
                sector: sid,
                direction: -1,
                speed: fspeed,
                dest_z: floor_dest,
                crush: false,
            });
            started += 1;
        }
        let ceil_dest = max_c.unwrap_or(sec.ceiling_z);
        if ceil_dest > sec.ceiling_z + 0.001 {
            world.ceilings.push(CeilingMover {
                sector: sid,
                direction: 1,
                speed: cspeed,
                speed_up: cspeed,
                bottom_z: sec.ceiling_z,
                top_z: ceil_dest,
                crush: false,
                once: true,
                stop_at_high: false,
            });
            started += 1;
        }
    }
    i32::from(started > 0)
}

fn plat_toggle_ceiling(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Plat_ToggleCeiling: instant floor ↔ ceiling-8 toggle (Boom 211/212).
    let tag = args[0];
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx).cloned() else {
            continue;
        };
        let high = sec.ceiling_z - 8.0;
        let low = sec.floor_z;
        let at_high = (sec.floor_z - high).abs() < 1.0;
        let (dest, dir) = if at_high {
            let base = world
                .map
                .sectors
                .get(idx)
                .map(|s| s.floor_height)
                .unwrap_or(low);
            (base.min(high), -1i8)
        } else {
            (high.max(low), 1i8)
        };
        world.floors.push(FloorMover {
            sector: sid,
            direction: dir,
            speed: 512.0,
            dest_z: dest,
            crush: false,
        });
        started += 1;
    }
    i32::from(started > 0)
}

fn floor_transfer_trigger(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Copy floor flat + special from the activating line's front sector.
    let tag = args[0];
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut changed = 0;
    for sid in sectors {
        apply_tx0_change(world, sid, line);
        changed += 1;
    }
    i32::from(changed > 0)
}

fn floor_transfer_numeric(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Numeric model: copy flat from the sector on the other side of the lowest-
    // numbered two-sided line of the target (approx: first neighbor).
    let tag = args[0];
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut changed = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(geom) = world.map.sectors.get(idx) else {
            continue;
        };
        let mut donor = None;
        for &lid in &geom.lines {
            let Some(ln) = world.map.lines.get(lid.raw() as usize) else {
                continue;
            };
            for other in [ln.front_sector, ln.back_sector].into_iter().flatten() {
                if other != sid {
                    donor = Some(other);
                    break;
                }
            }
            if donor.is_some() {
                break;
            }
        }
        let Some(donor) = donor else {
            continue;
        };
        let pic = world
            .sector_floor_pics
            .get(&donor.raw())
            .cloned()
            .or_else(|| {
                world
                    .map
                    .sectors
                    .get(donor.raw() as usize)
                    .map(|s| s.floor_pic.clone())
            });
        if let Some(pic) = pic {
            world.sector_floor_pics.insert(sid.raw(), pic);
        }
        let special = world
            .sector_specials
            .get(&donor.raw())
            .copied()
            .or_else(|| {
                world
                    .map
                    .sectors
                    .get(donor.raw() as usize)
                    .map(|s| s.special)
            })
            .unwrap_or(0);
        world.sector_specials.insert(sid.raw(), special);
        changed += 1;
    }
    i32::from(changed > 0)
}

fn ceiling_crush_and_raise(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    // args: tag, dist, speed, damage, …
    let speed = if args[2] != 0 {
        hexen_speed(args[2])
    } else {
        hexen_speed(args[1])
    };
    start_crusher(world, tag, speed, speed, 8.0, line, false, false)
}

/// `Ceiling_CrushAndRaiseDist(tag, dist, speed, damage, crushtype)`.
fn ceiling_crush_and_raise_dist(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let dist = if args[1] != 0 {
        f64::from(args[1].abs())
    } else {
        8.0
    };
    let speed = hexen_speed(args[2]);
    start_crusher(world, tag, speed, speed, dist, line, false, false)
}

fn generic_crusher(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    // Generic_Crusher(tag, dspeed, uspeed, silent, crush)
    let tag = args[0];
    let down = hexen_speed(args[1]);
    let up = if args[2] != 0 {
        hexen_speed(args[2])
    } else {
        down
    };
    let silent = args[3] != 0;
    start_crusher(world, tag, down, up, 8.0, line, silent, false)
}

fn start_crusher(
    world: &mut World,
    tag: i32,
    speed_down: f64,
    speed_up: f64,
    gap: f64,
    line: Option<LineId>,
    silent: bool,
    stop_at_high: bool,
) -> i32 {
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let bottom = sec.floor_z + gap.max(0.0);
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction: -1,
            speed: speed_down,
            speed_up,
            bottom_z: bottom,
            top_z: sec.ceiling_z,
            crush: true,
            once: false,
            stop_at_high,
        });
        if !silent {
            play_sector_sound(world, sid, "floors/move");
        }
    }
    1
}

fn ceiling_lower_and_crush(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    dist: f64,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let gap = dist.max(0.0);
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let bottom = (sec.floor_z + gap).min(sec.ceiling_z);
        if bottom >= sec.ceiling_z - 0.001 {
            continue;
        }
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction: -1,
            speed,
            speed_up: speed,
            bottom_z: bottom,
            top_z: sec.ceiling_z,
            crush: true,
            once: true,
            stop_at_high: false,
        });
        started += 1;
    }
    i32::from(started > 0)
}

/// Map `BLOCKF_*` bits onto `ML_*` flags we track.
fn blockf_to_ml(bits: i32) -> i32 {
    use crate::world::{
        ML_BLOCK_PLAYERS, ML_BLOCKEVERYTHING, ML_BLOCKING, ML_BLOCKMONSTERS, ML_RAILING, blockf,
    };
    let mut ml = 0;
    if bits & blockf::CREATURES != 0 {
        ml |= ML_BLOCKING;
    }
    if bits & blockf::MONSTERS != 0 {
        ml |= ML_BLOCKMONSTERS;
    }
    if bits & blockf::PLAYERS != 0 {
        ml |= ML_BLOCK_PLAYERS;
    }
    if bits & blockf::EVERYTHING != 0 {
        ml |= ML_BLOCKEVERYTHING;
    }
    if bits & blockf::RAILING != 0 {
        ml |= ML_RAILING;
    }
    if bits & blockf::HITSCAN != 0 {
        ml |= crate::world::ML_BLOCKHITSCAN;
    }
    ml
}

/// `Line_SetBlocking(lineid, setflags, clearflags)`.
fn line_set_blocking(world: &mut World, args: &[i32; 5]) -> i32 {
    let line_id = args[0];
    let set_ml = blockf_to_ml(args[1]);
    let clear_ml = blockf_to_ml(args[2]);
    if set_ml == 0 && clear_ml == 0 {
        return 0;
    }
    let mut n = 0;
    for (i, ld) in world.map.lines.iter().enumerate() {
        let matches = if line_id == 0 {
            // Activating line when id is 0 — bootstrap: first linedef.
            i == 0
        } else {
            ld.has_id(line_id)
        };
        if !matches {
            continue;
        }
        let li = i as u32;
        let base = world
            .line_flags
            .get(&li)
            .copied()
            .unwrap_or_else(|| crate::world::base_line_flags(ld));
        world.line_flags.insert(li, (base | set_ml) & !clear_ml);
        n += 1;
    }
    i32::from(n > 0)
}

/// Combine GZDoom panning int + frac args into map-unit UV offset.
fn panning_uv(int_part: i32, frac_part: i32) -> f64 {
    f64::from(int_part) + Fixed(frac_part).to_f64()
}

/// `Sector_SetFloor/CeilingPanning(tag, u-int, u-frac, v-int, v-frac)`.
fn sector_set_panning(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    floor: bool,
) -> i32 {
    let tag = args[0];
    let u = panning_uv(args[1], args[2]);
    let v = panning_uv(args[3], args[4]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let i = sid.raw();
        if floor {
            world.floor_scroll_offsets.insert(i, (u, v));
        } else {
            world.ceiling_scroll_offsets.insert(i, (u, v));
        }
    }
    1
}

/// GZDoom scale args: `int + frac/100`, stored as reciprocal (0 = leave).
fn scale_from_parts(int_part: i32, frac_part: i32) -> Option<f64> {
    let s = f64::from(int_part) + f64::from(frac_part) / 100.0;
    if s == 0.0 { None } else { Some(1.0 / s) }
}

/// `Sector_SetFloor/CeilingScale(tag, x-int, x-frac, y-int, y-frac)`.
fn sector_set_scale(world: &mut World, args: &[i32; 5], line: Option<LineId>, floor: bool) -> i32 {
    let tag = args[0];
    let sx = scale_from_parts(args[1], args[2]);
    let sy = scale_from_parts(args[3], args[4]);
    if sx.is_none() && sy.is_none() {
        return 0;
    }
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let i = sid.raw();
        let map = if floor {
            &mut world.floor_scales
        } else {
            &mut world.ceiling_scales
        };
        let (cx, cy) = map.get(&i).copied().unwrap_or((1.0, 1.0));
        map.insert(i, (sx.unwrap_or(cx), sy.unwrap_or(cy)));
    }
    1
}

/// `Sector_SetRotation(tag, floor-angle, ceiling-angle)` — degrees.
fn sector_set_rotation(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let floor_a = f64::from(args[1]);
    let ceil_a = f64::from(args[2]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        world.plane_angles.insert(sid.raw(), (floor_a, ceil_a));
    }
    1
}

/// `Thing_Damage(tid, amount, mod)` — damage all actors with tid.
fn thing_damage(world: &mut World, args: &[i32; 5]) -> i32 {
    let ids = actors_with_tid(world, args[0]);
    if ids.is_empty() {
        return 0;
    }
    let amount = args[1].max(0);
    if amount == 0 {
        return 0;
    }
    let mut n = 0;
    for id in ids {
        let shootable = world
            .actors
            .get(id)
            .is_some_and(|a| (a.flags & crate::class::FLAG_SHOOTABLE) != 0);
        if shootable {
            if damage_mobj(world, id, None, None, amount) > 0 {
                n += 1;
            }
        } else if let Some(a) = world.actors.get_mut(id) {
            if !a.destroyed {
                a.health = a.health.saturating_sub(amount);
                n += 1;
            }
        }
    }
    i32::from(n > 0)
}

/// `Thing_Move(tid, mapspot, nofog)` — teleport tid actors to mapspot tid.
fn thing_move(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    // ACS uses TID 0 to mean the script activator. Vehicle entry scripts
    // rely on Thing_Move(0, vehicle_tid, ...) to seat the player on the vehicle.
    let movers = if args[0] == 0 {
        activator.into_iter().collect()
    } else {
        actors_with_tid(world, args[0])
    };
    let spots = actors_with_tid(world, args[1]);
    let Some(&spot) = spots.first() else {
        if args[1] == 261 {
            eprintln!(
                "room: VEHICLE Thing_Move tic={} source_tid={} activator={:?} destination_tid=261 not found",
                world.level.time,
                args[0],
                activator.map(|id| id.index()),
            );
        }
        return 0;
    };
    let Some(dest) = world.actors.get(spot).map(|a| (a.pos, a.angle)) else {
        return 0;
    };
    if movers.is_empty() {
        if args[1] == 261 {
            eprintln!(
                "room: VEHICLE Thing_Move tic={} source_tid={} destination_tid=261 found=#{} but no movers",
                world.level.time,
                args[0],
                spot.index(),
            );
        }
        return 0;
    }
    if args[1] == 261 {
        let movers = movers.iter().map(|id| id.index()).collect::<Vec<_>>();
        let dest_actor = world.actors.get(spot);
        eprintln!(
            "room: VEHICLE Thing_Move tic={} source_tid={} movers={:?} destination=#{} pos=({:.1},{:.1},{:.1})",
            world.level.time,
            args[0],
            movers,
            spot.index(),
            dest_actor.map_or(0.0, |a| a.pos.x()),
            dest_actor.map_or(0.0, |a| a.pos.y()),
            dest_actor.map_or(0.0, |a| a.pos.z()),
        );
    }
    let fog = args[2] == 0;
    for id in movers {
        if let Some(a) = world.actors.get_mut(id) {
            a.pos = dest.0;
            a.angle = dest.1;
            a.vel = DVec3::ZERO;
        }
        if fog {
            spawn_teleport_fog(world, dest.0);
        }
    }
    1
}

/// `Sector_ChangeFlags(tag, set, clear)`.
fn sector_change_flags(world: &mut World, args: &[i32; 5]) -> i32 {
    let tag = args[0];
    if tag == 0 {
        return 0;
    }
    let set = args[1] as u32;
    let clear = args[2] as u32;
    let sectors = sectors_for_tag(world, tag, None);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let i = sid.raw();
        let base = world.sector_flags.get(&i).copied().unwrap_or(0);
        world.sector_flags.insert(i, (base | set) & !clear);
    }
    1
}

/// `GlassBreak` — clear blocking on activating linedef and its special.
fn glass_break(world: &mut World, line: Option<LineId>) -> i32 {
    let Some(lid) = line else {
        return 0;
    };
    let li = lid.raw();
    let Some(ld) = world.map.lines.get(li as usize) else {
        return 0;
    };
    let base = world
        .line_flags
        .get(&li)
        .copied()
        .unwrap_or_else(|| crate::world::base_line_flags(ld));
    world.line_flags.insert(
        li,
        base & !(crate::world::ML_BLOCKING | crate::world::ML_BLOCKEVERYTHING),
    );
    // Clear runtime special so the line won't re-fire.
    world.line_args.insert(li, [0, 0, 0, 0, 0]);
    world.line_specials.insert(li, 0);
    1
}

/// `Thing_SetTranslation(tid, range)` — bootstrap string tags (`0` clear, `-1` copy activator).
fn thing_set_translation(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let range = args[1];
    let tr = if range == 0 {
        String::new()
    } else if range < 0 {
        activator
            .and_then(|id| world.actors.get(id))
            .map(|a| a.translation.clone())
            .unwrap_or_default()
    } else if range == 0x100007 {
        "Ice".to_owned()
    } else {
        format!("ACS{range}")
    };
    let ids = if args[0] == 0 {
        activator.into_iter().collect::<Vec<_>>()
    } else {
        actors_with_tid(world, args[0])
    };
    if ids.is_empty() {
        return 0;
    }
    for id in ids {
        if let Some(a) = world.actors.get_mut(id) {
            a.translation = tr.clone();
        }
    }
    1
}

/// `Sector_SetColor(tag, r, g, b, desaturate)`.
fn sector_set_color(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let rgb = [
        args[1].clamp(0, 255) as u8,
        args[2].clamp(0, 255) as u8,
        args[3].clamp(0, 255) as u8,
    ];
    let desat = args[4].clamp(0, 255) as u8;
    for sid in sectors {
        if let Some(s) = world.sectors.get_mut(sid.raw() as usize) {
            s.color = rgb;
            s.color_desaturate = desat;
        }
    }
    1
}

/// `Sector_SetFade(tag, r, g, b)`.
fn sector_set_fade(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let rgb = [
        args[1].clamp(0, 255) as u8,
        args[2].clamp(0, 255) as u8,
        args[3].clamp(0, 255) as u8,
    ];
    for sid in sectors {
        if let Some(s) = world.sectors.get_mut(sid.raw() as usize) {
            s.fade = rgb;
        }
    }
    1
}

/// `Sector_SetDamage(tag, amount, mod, interval, leaky)`.
fn sector_set_damage(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let amount = args[1];
    let mut interval = args[3];
    if interval <= 0 {
        interval = if amount < 20 {
            32
        } else if amount < 50 {
            32
        } else {
            1
        };
    }
    let dtype = if args[2] == 0 {
        String::new()
    } else {
        format!("mod{}", args[2])
    };
    let leaky = args[4].clamp(0, 256);
    for sid in sectors {
        if let Some(s) = world.sectors.get_mut(sid.raw() as usize) {
            s.damage_amount = amount;
            s.damage_type = dtype.clone();
            s.damage_interval = interval;
            s.leakiness = leaky;
        }
    }
    1
}

/// `Sector_SetGravity(tag, intpart, fracpart)`.
fn sector_set_gravity(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let sectors = sectors_for_tag(world, args[0], line);
    if sectors.is_empty() {
        return 0;
    }
    let frac = args[2].clamp(0, 99);
    let g = f64::from(args[1]) + f64::from(frac) * 0.01;
    for sid in sectors {
        if let Some(s) = world.sectors.get_mut(sid.raw() as usize) {
            s.gravity = g;
        }
    }
    1
}

/// `DamageThing(damage [, mod])` — damage activator; `0` = kill.
fn damage_thing(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let Some(id) = activator else {
        return 0;
    };
    let Some(a) = world.actors.get(id) else {
        return 0;
    };
    if a.destroyed {
        return 0;
    }
    let amount = if args[0] == 0 {
        a.health.max(1)
    } else {
        args[0].max(0)
    };
    if amount == 0 {
        return 0;
    }
    // Prefer `damage_mobj` when shootable; else apply raw (players may lack SHOOTABLE).
    if (a.flags & crate::class::FLAG_SHOOTABLE) != 0 {
        let dealt = damage_mobj(world, id, None, None, amount);
        return i32::from(dealt > 0);
    }
    if let Some(a) = world.actors.get_mut(id) {
        a.health = a.health.saturating_sub(amount);
    }
    1
}

/// `Line_SetTextureOffset(lineid, x, y, side, flags)` — `x`/`y` fixed → map units.
fn line_set_texture_offset(world: &mut World, args: &[i32; 5]) -> i32 {
    let line_id = args[0];
    let ox = Fixed(args[1]).to_f64().round() as i16;
    let oy = Fixed(args[2]).to_f64().round() as i16;
    let back = args[3] != 0;
    let mut n = 0;
    for (i, ld) in world.map.lines.iter().enumerate() {
        let matches = if line_id == 0 {
            i == 0
        } else {
            ld.has_id(line_id)
        };
        if !matches {
            continue;
        }
        let side_opt = if back { ld.sides[1] } else { ld.sides[0] };
        let Some(sid) = side_opt else {
            continue;
        };
        world.side_offsets.insert(sid.raw(), (ox, oy));
        n += 1;
    }
    i32::from(n > 0)
}

/// `Line_SetTextureScale(lineid, x, y, side, flags)` — `x`/`y` fixed scale factors.
fn line_set_texture_scale(world: &mut World, args: &[i32; 5]) -> i32 {
    let line_id = args[0];
    let sx = Fixed(args[1]).to_f64() as f32;
    let sy = Fixed(args[2]).to_f64() as f32;
    let back = args[3] != 0;
    let mut n = 0;
    for (i, ld) in world.map.lines.iter().enumerate() {
        let matches = if line_id == 0 {
            i == 0
        } else {
            ld.has_id(line_id)
        };
        if !matches {
            continue;
        }
        let side_opt = if back { ld.sides[1] } else { ld.sides[0] };
        let Some(sid) = side_opt else {
            continue;
        };
        world.side_scales.insert(sid.raw(), (sx, sy));
        n += 1;
    }
    i32::from(n > 0)
}

/// `SetPlayerProperty(who, set, which)` — PROP_FROZEN/NOTARGET/FLY/TOTALLYFROZEN.
fn set_player_property(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    const PROP_FROZEN: i32 = 0;
    const PROP_NOTARGET: i32 = 1;
    const PROP_FLY: i32 = 3;
    const PROP_TOTALLY_FROZEN: i32 = 4;
    let all = args[0] != 0;
    let set = args[1] != 0;
    let which = args[2];
    let slots: Vec<usize> = if all {
        (0..world.players.len())
            .filter(|&i| world.players[i].as_ref().is_some_and(|p| p.mo.is_some()))
            .collect()
    } else {
        let Some(mo) = activator else {
            return 0;
        };
        match world
            .players
            .iter()
            .position(|p| p.as_ref().is_some_and(|pl| pl.mo == Some(mo)))
        {
            Some(i) => vec![i],
            None => return 0,
        }
    };
    if slots.is_empty() {
        return 0;
    }
    for i in slots {
        let Some(p) = world.players[i].as_mut() else {
            continue;
        };
        match which {
            PROP_FROZEN | PROP_TOTALLY_FROZEN => p.frozen = set,
            PROP_NOTARGET => p.notarget = set,
            PROP_FLY => {
                if let Some(mo) = p.mo {
                    if let Some(a) = world.actors.get_mut(mo) {
                        if set {
                            a.flags |= crate::class::FLAG_NOGRAVITY;
                        } else {
                            a.flags &= !crate::class::FLAG_NOGRAVITY;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    1
}

/// `NoiseAlert(target_tid, emitter_tid)` — flood `soundtarget` from emitter.
fn noise_alert_special(world: &mut World, args: &[i32; 5], activator: Option<ActorId>) -> i32 {
    let target = resolve_tid_or_activator(world, args[0], activator);
    let emitter = resolve_tid_or_activator(world, args[1], activator).or(target);
    let (Some(t), Some(e)) = (target, emitter) else {
        return 0;
    };
    crate::action::noise_alert_ex(world, t, e);
    1
}

fn resolve_tid_or_activator(
    world: &World,
    tid: i32,
    activator: Option<ActorId>,
) -> Option<ActorId> {
    if tid == 0 {
        return activator;
    }
    world.find_by_tid(tid)
}

/// `TranslucentLine(lineid, amount [, additive])` — amount 0–255 opacity.
fn translucent_line(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let line_id = args[0];
    let alpha = (args[1].clamp(0, 255) as u8) as f32 / 255.0;
    let additive = args[2] != 0;
    let mut n = 0;
    for (i, ld) in world.map.lines.iter().enumerate() {
        let matches = if line_id == 0 {
            line.is_some_and(|l| l.raw() as usize == i) || (line.is_none() && i == 0)
        } else {
            ld.has_id(line_id)
        };
        if !matches {
            continue;
        }
        let li = i as u32;
        world.line_alpha.insert(li, alpha);
        world.line_additive.insert(li, additive);
        n += 1;
    }
    i32::from(n > 0)
}

/// `Sector_SetTranslucent(tag, plane, amount, type)` — plane 0=floor, 1=ceiling; amount 0–255.
fn sector_set_translucent(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let plane = args[1];
    let amount = (args[2].clamp(0, 255) as u8) as f32 / 255.0;
    let additive = args[3] != 0;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get_mut(idx) else {
            continue;
        };
        match plane {
            0 => {
                sec.floor_alpha = amount;
                sec.floor_additive = additive;
                started += 1;
            }
            1 => {
                sec.ceiling_alpha = amount;
                sec.ceiling_additive = additive;
                started += 1;
            }
            _ => {
                // Bootstrap: treat other values as both planes.
                sec.floor_alpha = amount;
                sec.ceiling_alpha = amount;
                sec.floor_additive = additive;
                sec.ceiling_additive = additive;
                started += 1;
            }
        }
    }
    i32::from(started > 0)
}

fn ceiling_crush_stop(world: &mut World, args: &[i32; 5]) -> i32 {
    let tag = args[0];
    let keep: Vec<_> = world
        .ceilings
        .iter()
        .filter(|c| {
            world
                .map
                .sectors
                .get(c.sector.raw() as usize)
                .is_none_or(|s| tag != 0 && !s.tags.contains(&tag))
        })
        .cloned()
        .collect();
    let removed = world.ceilings.len() != keep.len();
    world.ceilings = keep;
    i32::from(removed)
}

/// Axis-aligned bounds of a sector's lines.
fn sector_bbox(world: &World, sector: SectorId) -> Option<(f64, f64, f64, f64)> {
    let ms = world.map.sectors.get(sector.raw() as usize)?;
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for lid in &ms.lines {
        let Some(l) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        for v in [l.v1, l.v2] {
            if let Some(v) = world.map.vertices.get(v.raw() as usize) {
                x0 = x0.min(v.pos.x());
                y0 = y0.min(v.pos.y());
                x1 = x1.max(v.pos.x());
                y1 = y1.max(v.pos.y());
            }
        }
    }
    (x0 <= x1).then_some((x0, y0, x1, y1))
}

/// Vanilla `P_ChangeSector` "nofit" for a lowering ceiling: some living
/// shootable thing touching `sector` no longer fits between its floor and the
/// (new) ceiling. Corpses and non-shootable things never hold a door.
fn door_squeezes_actor(world: &World, sector: SectorId) -> bool {
    let Some((x0, y0, x1, y1)) = sector_bbox(world, sector) else {
        return false;
    };
    world.thinkers.default.iter().any(|&id| {
        let Some(a) = world.actors.get(id) else {
            return false;
        };
        if a.destroyed
            || a.health <= 0
            || a.flags & crate::class::FLAG_SHOOTABLE == 0
            || a.flags & crate::class::FLAG_NOBLOCKMAP != 0
        {
            return false;
        }
        let r = a.radius;
        if a.pos.x() + r <= x0 || a.pos.x() - r >= x1 || a.pos.y() + r <= y0 || a.pos.y() - r >= y1
        {
            return false;
        }
        let check = crate::physics::check_position(world, a, a.pos, Some(id));
        // Only this sector's ceiling matters: the thing must be touching it.
        let ceil = world.sectors[sector.raw() as usize].ceiling_z;
        (check.ceiling_z - ceil).abs() < 1e-6 && check.ceiling_z - check.floor_z < a.height
    })
}

/// Apply vanilla-style crush damage to actors whose height exceeds the sector gap.
///
/// Only actors whose centre lies in `sector` are affected (Doom `PIT_ChangeSector`
/// walks the sector's blockmap, not the whole thinker list).
fn crush_sector_actors(world: &mut World, sector: SectorId) {
    // Doom deals crush damage every 4 tics while the victim is pinned.
    if world.level.time % 4 != 0 {
        return;
    }
    let idx = sector.raw() as usize;
    let Some(sec) = world.sectors.get(idx) else {
        return;
    };
    let gap = sec.ceiling_z - sec.floor_z;
    let victims: Vec<ActorId> = world
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            world.actors.get(id).is_some_and(|a| {
                if a.destroyed || a.height <= gap {
                    return false;
                }
                let in_sec = room_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y())
                    .or_else(|| {
                        room_nodebuild::point_in_sector_vanilla(&world.map, a.pos.x(), a.pos.y())
                    })
                    .is_some_and(|s| s == sector);
                if !in_sec {
                    return false;
                }
                // Living shootables take crush damage; corpses gib when squashed.
                (a.flags & crate::class::FLAG_SHOOTABLE) != 0 || a.health <= 0
            })
        })
        .collect();
    for id in victims {
        let health = world.actors.get(id).map(|a| a.health).unwrap_or(0);
        if health <= 0 {
            crush_corpse(world, id);
            continue;
        }
        let cue = world
            .actors
            .get(id)
            .and_then(|a| world.classes.get(a.class))
            .map(|c| c.crush_pain_sound.clone())
            .filter(|s| !s.is_empty());
        let pos = world
            .actors
            .get(id)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()));
        damage_mobj(world, id, None, None, CRUSH_DAMAGE);
        if let Some(ld) = world.last_damage.as_mut() {
            if ld.target == Some(id) && ld.tic == world.level.time {
                ld.cause = format!("crush sector {} gap={gap:.0}", sector.raw());
            }
        }
        if let (Some(name), Some((x, y, z))) = (cue, pos) {
            world.play_sound(&name, x, y, z, 1.0);
        }
    }
}

/// Squish a dead actor into gibs (`PIT_ChangeSector` corpse path).
fn crush_corpse(world: &mut World, id: ActorId) {
    let Some(a) = world.actors.get(id).cloned() else {
        return;
    };
    if a.destroyed || a.health > 0 {
        return;
    }
    let flags = a.flags | world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
    let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
    world.play_sound("misc/gibbed", x, y, z, 1.0);
    if (flags & crate::class::FLAG_NOBLOOD) == 0 {
        let blood_name = world
            .classes
            .get(a.class)
            .and_then(|c| c.blood_type.first().cloned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Blood".into());
        if let Some(blood) = world.classes.find_name(&blood_name) {
            // Vanilla sprays several blood hunks; three is enough for M5 visuals.
            for i in 0..3 {
                let ox = f64::from(i) * 4.0 - 4.0;
                if let Ok(bid) = world.spawn_at(blood, DVec3::new(x + ox, y, z), Angle::ZERO) {
                    if let Some(b) = world.actors.get_mut(bid) {
                        b.vel = DVec3::new(ox * 0.5, 0.0, 2.0);
                    }
                }
            }
        }
    }
    if let Some(a) = world.actors.get_mut(id) {
        a.height = 0.0;
        a.flags &= !crate::class::FLAG_SOLID;
        a.flags &= !crate::class::FLAG_SHOOTABLE;
        a.destroyed = true;
        a.state = None;
    }
}

/// Tick ceiling / crusher movers.
pub fn tick_ceilings(world: &mut World) {
    let mut i = 0;
    while i < world.ceilings.len() {
        let sector = world.ceilings[i].sector;
        let direction = world.ceilings[i].direction;
        let speed_down = world.ceilings[i].speed;
        let speed_up = world.ceilings[i].speed_up;
        let bottom_z = world.ceilings[i].bottom_z;
        let top_z = world.ceilings[i].top_z;
        let crush = world.ceilings[i].crush;
        let once = world.ceilings[i].once;
        let stop_at_high = world.ceilings[i].stop_at_high;
        let Some(sec) = world.sectors.get_mut(sector.raw() as usize) else {
            world.ceilings.swap_remove(i);
            continue;
        };
        let mut crushing = false;
        let mut new_dir = direction;
        let done = match direction {
            -1 => {
                crushing = true;
                sec.ceiling_z -= speed_down;
                if sec.ceiling_z <= bottom_z {
                    sec.ceiling_z = bottom_z;
                    if crush && !once {
                        new_dir = 1;
                        false
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            1 => {
                sec.ceiling_z += speed_up;
                if sec.ceiling_z >= top_z {
                    sec.ceiling_z = top_z;
                    if crush && !once && !stop_at_high {
                        new_dir = -1;
                        false
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            _ => true,
        };
        world.ceilings[i].direction = new_dir;
        if crushing {
            crush_sector_actors(world, sector);
        }
        if done {
            world.ceilings.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

/// Tick platform movers.
pub fn tick_plats(world: &mut World) {
    let mut i = 0;
    while i < world.plats.len() {
        let sector = world.plats[i].sector;
        let Some(sec) = world.sectors.get_mut(sector.raw() as usize) else {
            world.plats.swap_remove(i);
            continue;
        };
        let old_floor = sec.floor_z;
        let plat = &mut world.plats[i];
        let prev = plat.status;
        let done = match plat.status {
            -1 => {
                sec.floor_z -= plat.speed;
                if sec.floor_z <= plat.low {
                    sec.floor_z = plat.low;
                    if plat.stop_at_low {
                        true
                    } else {
                        plat.status = 0;
                        plat.wait = plat.wait_max;
                        false
                    }
                } else {
                    false
                }
            }
            0 => {
                if plat.wait > 0 {
                    plat.wait -= 1;
                } else if (sec.floor_z - plat.low).abs() < 1e-6 {
                    plat.status = 1;
                } else {
                    // Waiting at high (perpetual).
                    plat.status = -1;
                }
                false
            }
            1 => {
                sec.floor_z += plat.speed;
                if sec.floor_z >= plat.high {
                    sec.floor_z = plat.high;
                    if plat.perpetual {
                        plat.status = 0;
                        plat.wait = plat.wait_max;
                        false
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            _ => true,
        };
        let delta = sec.floor_z - old_floor;
        let status = plat.status;
        let arrived = (prev == -1 || prev == 1) && (status == 0 || done);
        let departing = prev == 0 && (status == 1 || status == -1);
        if done {
            world.plats.swap_remove(i);
        } else {
            i += 1;
        }
        if delta.abs() > 1e-9 {
            carry_floor_actors(world, sector, old_floor, delta);
        }
        if arrived {
            play_sector_sound(world, sector, "platforms/stop");
        } else if departing {
            play_sector_sound(world, sector, "platforms/start");
        }
    }
}

fn floor_raise_to_lowest_ceiling(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let dest = sec.ceiling_z - 8.0;
        if dest <= sec.floor_z {
            continue;
        }
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: dest,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
    }
    1
}

/// `Floor_RaiseToHighest(tag, speed)`.
fn floor_raise_to_highest(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let (_, max_f, _, _) = neighbor_plane_extrema(world, sid);
        let Some(dest) = max_f.filter(|z| *z > sec.floor_z + 0.001) else {
            continue;
        };
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: dest,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
        n += 1;
    }
    i32::from(n > 0)
}

/// `Floor_LowerToHighestEE(tag, speed, adjust)` — lower to highest neighbor + (adjust-128).
fn floor_lower_to_highest_ee(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let adjust = f64::from(args[2] - 128);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let (_, max_f, _, _) = neighbor_plane_extrema(world, sid);
        let Some(dest) = max_f
            .map(|z| z + adjust)
            .filter(|z| *z < sec.floor_z - 0.001)
        else {
            continue;
        };
        world.floors.push(FloorMover {
            sector: sid,
            direction: -1,
            speed,
            dest_z: dest,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
        n += 1;
    }
    i32::from(n > 0)
}

/// `Floor_RaiseToLowest(tag)` — raise floor to lowest neighboring floor.
fn floor_raise_to_lowest(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(if args[1] != 0 { args[1] } else { 8 });
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let (min_f, _, _, _) = neighbor_plane_extrema(world, sid);
        let Some(dest) = min_f.filter(|z| *z > sec.floor_z + 0.001) else {
            continue;
        };
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: dest,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
        n += 1;
    }
    i32::from(n > 0)
}

/// `Floor_LowerToLowestCeiling(tag, speed)`.
fn floor_lower_to_lowest_ceiling(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let (_, _, min_c, _) = neighbor_plane_extrema(world, sid);
        let Some(dest) = min_c.filter(|z| *z < sec.floor_z - 0.001) else {
            continue;
        };
        world.floors.push(FloorMover {
            sector: sid,
            direction: -1,
            speed,
            dest_z: dest,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
        n += 1;
    }
    i32::from(n > 0)
}

/// `Floor_RaiseToCeiling` / `Floor_ToCeilingInstant`.
fn floor_raise_to_ceiling(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    instant: bool,
) -> i32 {
    let tag = args[0];
    let speed = if instant { 1.0e9 } else { hexen_speed(args[1]) };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let dest = sec.ceiling_z;
        if dest <= sec.floor_z + 0.001 {
            continue;
        }
        if instant {
            if let Some(sec) = world.sectors.get_mut(idx) {
                sec.floor_z = dest;
            }
        } else {
            world.floors.push(FloorMover {
                sector: sid,
                direction: 1,
                speed,
                dest_z: dest,
                crush: false,
            });
        }
        play_sector_sound(world, sid, "floors/move");
        n += 1;
    }
    i32::from(n > 0)
}

/// Raise/lower ceiling toward neighboring extrema (`Ceiling_RaiseToHighest` / `LowerToNearest`).
fn ceiling_to_neighbor(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    direction: i8,
    highest: bool,
    instant: bool,
) -> i32 {
    let tag = args[0];
    let speed = if instant { 1.0e9 } else { hexen_speed(args[1]) };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let ceil_z = sec.ceiling_z;
        let floor_z = sec.floor_z;
        let (_, _, min_c, max_c) = neighbor_plane_extrema(world, sid);
        let dest = if highest { max_c } else { min_c };
        let Some(dest) = dest else {
            continue;
        };
        if direction > 0 && dest <= ceil_z + 0.001 {
            continue;
        }
        if direction < 0 && dest >= ceil_z - 0.001 {
            continue;
        }
        if instant {
            if let Some(sec) = world.sectors.get_mut(idx) {
                sec.ceiling_z = dest.max(floor_z);
            }
        } else {
            world.ceilings.push(CeilingMover {
                sector: sid,
                direction,
                speed,
                speed_up: speed,
                bottom_z: dest.min(ceil_z),
                top_z: dest.max(ceil_z),
                crush: false,
                once: true,
                stop_at_high: false,
            });
        }
        n += 1;
    }
    i32::from(n > 0)
}

/// `Ceiling_RaiseToLowest` — raise ceiling up to the lowest neighboring ceiling.
fn ceiling_raise_to_lowest(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    ceiling_to_neighbor(world, args, line, 1, false, false)
}

/// `Ceiling_RaiseToHighestFloor` — raise ceiling to highest neighboring floor.
fn ceiling_raise_to_highest_floor(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let (_, max_f, _, _) = neighbor_plane_extrema(world, sid);
        let Some(dest) = max_f.filter(|z| *z > sec.ceiling_z + 0.001) else {
            continue;
        };
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction: 1,
            speed,
            speed_up: speed,
            bottom_z: sec.ceiling_z,
            top_z: dest,
            crush: false,
            once: true,
            stop_at_high: false,
        });
        n += 1;
    }
    i32::from(n > 0)
}

/// `Ceiling_ToFloorInstant`.
fn ceiling_to_floor_instant(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get_mut(idx) else {
            continue;
        };
        sec.ceiling_z = sec.floor_z;
        n += 1;
    }
    i32::from(n > 0)
}

fn floor_raise_and_crush(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        // Crush gap of 8 units under ceiling (Doom convention).
        let dest = sec.ceiling_z - 8.0;
        if dest <= sec.floor_z {
            continue;
        }
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: dest,
            crush: true,
        });
    }
    1
}

fn floor_raise_by_value(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let height = f64::from(if args[2] != 0 { args[2] } else { 24 });
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: sec.floor_z + height,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
    }
    1
}

/// `Floor_LowerInstant` / `Floor_RaiseInstant` — `(tag, unused, height)` moves by height×8 now.
fn floor_instant(world: &mut World, args: &[i32; 5], line: Option<LineId>, dir: i8) -> i32 {
    let tag = args[0];
    let units = if args[2] != 0 { args[2] } else { 1 };
    let delta = f64::from(units) * 8.0 * f64::from(dir);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get_mut(idx) else {
            continue;
        };
        sec.floor_z += delta;
        play_sector_sound(world, sid, "floors/move");
        n += 1;
    }
    i32::from(n > 0)
}

/// `Floor_MoveToValue[Times8|AndCrush](tag, speed, height, negative)`.
fn floor_move_to_value(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    scale: f64,
    crush: bool,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sign = if args[3] != 0 { -1.0 } else { 1.0 };
    let dest = f64::from(args[2]) * scale * sign;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let dir = if dest >= sec.floor_z { 1 } else { -1 };
        world.floors.push(FloorMover {
            sector: sid,
            direction: dir,
            speed,
            dest_z: dest,
            crush,
        });
        play_sector_sound(world, sid, "floors/move");
        n += 1;
    }
    i32::from(n > 0)
}

/// `Ceiling_MoveToValue[Times8|AndCrush](tag, speed, height, negative)`.
fn ceiling_move_to_value(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    scale: f64,
    crush: bool,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sign = if args[3] != 0 { -1.0 } else { 1.0 };
    let dest = f64::from(args[2]) * scale * sign;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let direction = if dest >= sec.ceiling_z { 1 } else { -1 };
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction,
            speed,
            speed_up: speed,
            bottom_z: dest.min(sec.ceiling_z),
            top_z: dest.max(sec.ceiling_z),
            crush,
            once: true,
            stop_at_high: false,
        });
        n += 1;
    }
    i32::from(n > 0)
}

/// `Ceiling_LowerInstant` / `Ceiling_RaiseInstant` — `(tag, unused, height)` by height×8.
fn ceiling_instant(world: &mut World, args: &[i32; 5], line: Option<LineId>, dir: i8) -> i32 {
    let tag = args[0];
    let units = if args[2] != 0 { args[2] } else { 1 };
    let delta = f64::from(units) * 8.0 * f64::from(dir);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get_mut(idx) else {
            continue;
        };
        sec.ceiling_z = (sec.ceiling_z + delta).max(sec.floor_z);
        n += 1;
    }
    i32::from(n > 0)
}

/// `Line_Horizon` — mark activating linedef for horizon rendering.
fn line_horizon(world: &mut World, line: Option<LineId>) -> i32 {
    let Some(lid) = line else {
        return 0;
    };
    let li = lid.raw();
    let base = world.line_flags.get(&li).copied().unwrap_or_else(|| {
        world
            .map
            .lines
            .get(li as usize)
            .map(crate::world::base_line_flags)
            .unwrap_or(0)
    });
    world.line_flags.insert(li, base | crate::world::ML_HORIZON);
    1
}

/// `Transfer_WallLight(tag, side)` — copy front-sector light onto matching sides.
fn transfer_wall_light(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let back = args[1] != 0;
    let light = line
        .and_then(|lid| world.map.lines.get(lid.raw() as usize))
        .and_then(|l| l.front_sector)
        .and_then(|s| world.sectors.get(s.raw() as usize))
        .map(|s| s.light)
        .unwrap_or(255);
    let mut n = 0;
    for (i, ld) in world.map.lines.iter().enumerate() {
        if tag != 0 && i32::from(ld.tag) != tag {
            continue;
        }
        if tag == 0 && line != Some(LineId::from_raw(i as u32)) {
            continue;
        }
        let Some(sid) = (if back { ld.sides[1] } else { ld.sides[0] }) else {
            continue;
        };
        world.side_lights.insert(sid.raw(), light);
        n += 1;
    }
    i32::from(n > 0)
}

/// `Light_ForceLightning(mode)` — flash all sector lights, then fade back.
fn light_force_lightning(world: &mut World, _args: &[i32; 5]) -> i32 {
    let targets: Vec<_> = world
        .sectors
        .iter()
        .enumerate()
        .map(|(i, s)| (SectorId::from_raw(i as u32), s.light))
        .collect();
    if targets.is_empty() {
        return 0;
    }
    for (sid, orig) in targets {
        if let Some(sec) = world.sectors.get_mut(sid.raw() as usize) {
            sec.light = 255;
        }
        world.fades.retain(|f| f.sector != sid);
        world.fades.push(LightFade {
            sector: sid,
            dest: orig,
            tics: 8,
            step: f64::from(i32::from(orig) - 255) / 8.0,
            current: 255.0,
        });
    }
    1
}

/// `Plane_Copy` — copy front-sector floor/ceiling Z + flats onto tagged sectors.
fn plane_copy(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let src = line
        .and_then(|lid| world.map.lines.get(lid.raw() as usize))
        .and_then(|l| l.front_sector);
    let Some(src) = src else {
        return 0;
    };
    let Some(src_sec) = world.sectors.get(src.raw() as usize).cloned() else {
        return 0;
    };
    let floor_pic = floor_pic_of(world, src).unwrap_or_default();
    let ceil_pic = world
        .sector_ceiling_pics
        .get(&src.raw())
        .cloned()
        .or_else(|| {
            world
                .map
                .sectors
                .get(src.raw() as usize)
                .map(|s| s.ceiling_pic.clone())
        })
        .unwrap_or_default();
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in &sectors {
        if let Some(sec) = world.sectors.get_mut(sid.raw() as usize) {
            sec.floor_z = src_sec.floor_z;
            sec.ceiling_z = src_sec.ceiling_z;
        }
        world.sector_floor_pics.insert(sid.raw(), floor_pic.clone());
        world
            .sector_ceiling_pics
            .insert(sid.raw(), ceil_pic.clone());
    }
    1
}

/// `Line_SetIdentification(id, …)` — store runtime line id on the activator.
fn line_set_identification(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let Some(lid) = line else {
        return 0;
    };
    world.line_ids.insert(lid.raw(), args[0]);
    1
}

/// `Scroll_Texture_Offsets` — scroll using the sidedef's current offset as per-tic delta.
fn scroll_texture_offsets(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let Some(line_id) = line else {
        return 0;
    };
    let Some(ld) = world.map.lines.get(line_id.raw() as usize) else {
        return 0;
    };
    let Some(side) = ld.sides[0] else {
        return 0;
    };
    let (ox, oy) = world
        .side_offsets
        .get(&side.raw())
        .copied()
        .unwrap_or_else(|| {
            world
                .map
                .sides
                .get(side.raw() as usize)
                .map(|s| (s.x_offset, s.y_offset))
                .unwrap_or((0, 0))
        });
    let scale = if args[0] != 0 {
        f64::from(args[0])
    } else {
        1.0
    };
    let dx = f64::from(ox) * scale / 64.0;
    let dy = f64::from(oy) * scale / 64.0;
    world.tex_scrollers.retain(|s| s.side != side);
    world.tex_scrollers.push(TextureScroller {
        side,
        dx,
        dy,
        x: f64::from(ox),
        y: f64::from(oy),
    });
    1
}

/// `Scroll_Texture_Model(lineid, controlside)` — copy scroll rate from a control linedef's side.
fn scroll_texture_model(world: &mut World, args: &[i32; 5]) -> i32 {
    let line_id = args[0];
    if line_id == 0 {
        return 0;
    }
    let control_back = args[1] != 0;
    // Find first line with matching tag as control; scroll all sides of that id.
    let mut control_dx = 0.0;
    let mut control_dy = 0.0;
    let mut found = false;
    for ld in &world.map.lines {
        if i32::from(ld.tag) != line_id {
            continue;
        }
        let Some(sid) = (if control_back {
            ld.sides[1]
        } else {
            ld.sides[0]
        }) else {
            continue;
        };
        if let Some(sc) = world.tex_scrollers.iter().find(|s| s.side == sid) {
            control_dx = sc.dx;
            control_dy = sc.dy;
            found = true;
            break;
        }
        let side = world.map.sides.get(sid.raw() as usize);
        if let Some(side) = side {
            control_dx = f64::from(side.x_offset) / 64.0;
            control_dy = f64::from(side.y_offset) / 64.0;
            found = true;
            break;
        }
    }
    if !found {
        return 0;
    }
    let mut n = 0;
    for ld in &world.map.lines {
        if i32::from(ld.tag) != line_id {
            continue;
        }
        for &opt in &ld.sides {
            let Some(sid) = opt else {
                continue;
            };
            let base_x = world
                .map
                .sides
                .get(sid.raw() as usize)
                .map(|s| f64::from(s.x_offset))
                .unwrap_or(0.0);
            let base_y = world
                .map
                .sides
                .get(sid.raw() as usize)
                .map(|s| f64::from(s.y_offset))
                .unwrap_or(0.0);
            world.tex_scrollers.retain(|s| s.side != sid);
            world.tex_scrollers.push(TextureScroller {
                side: sid,
                dx: control_dx,
                dy: control_dy,
                x: base_x,
                y: base_y,
            });
            n += 1;
        }
    }
    i32::from(n > 0)
}

/// `Sector_CopyScroller(tag, type)` — copy floor/ceiling scroll from the front sector.
fn sector_copy_scroller(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let copy_ceil = args[1] != 0;
    let src = line
        .and_then(|lid| world.map.lines.get(lid.raw() as usize))
        .and_then(|l| l.front_sector);
    let Some(src) = src else {
        return 0;
    };
    let floor_scroll = world.scrollers.iter().find(|s| s.sector == src).cloned();
    let ceil_scroll = world
        .ceiling_scrollers
        .iter()
        .find(|s| s.sector == src)
        .cloned();
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut n = 0;
    for sid in sectors {
        if sid == src {
            continue;
        }
        if let Some(ref s) = floor_scroll {
            world.scrollers.retain(|x| x.sector != sid);
            let mut c = s.clone();
            c.sector = sid;
            world.scrollers.push(c);
            n += 1;
        }
        if copy_ceil {
            if let Some(ref s) = ceil_scroll {
                world.ceiling_scrollers.retain(|x| x.sector != sid);
                let mut c = s.clone();
                c.sector = sid;
                world.ceiling_scrollers.push(c);
                n += 1;
            }
        }
    }
    i32::from(n > 0)
}

/// `Scroll_Texture_Both(id, left, right, up, down)` — negative id selects back side.
fn scroll_texture_both(world: &mut World, args: &[i32; 5]) -> i32 {
    let mut line_id = args[0];
    if line_id == 0 {
        return 0;
    }
    let back = line_id < 0;
    if back {
        line_id = -line_id;
    }
    // GZDoom: dx = (left-right)/64, dy = (down-up)/64 texel units/tic.
    let dx = f64::from(args[1] - args[2]) / 64.0;
    let dy = f64::from(args[4] - args[3]) / 64.0;
    let mut n = 0;
    for ld in &world.map.lines {
        if i32::from(ld.tag) != line_id {
            continue;
        }
        let Some(sid) = (if back { ld.sides[1] } else { ld.sides[0] }) else {
            continue;
        };
        let base_x = world
            .map
            .sides
            .get(sid.raw() as usize)
            .map(|s| f64::from(s.x_offset))
            .unwrap_or(0.0);
        let base_y = world
            .map
            .sides
            .get(sid.raw() as usize)
            .map(|s| f64::from(s.y_offset))
            .unwrap_or(0.0);
        world.tex_scrollers.retain(|s| s.side != sid);
        world.tex_scrollers.push(TextureScroller {
            side: sid,
            dx,
            dy,
            x: base_x,
            y: base_y,
        });
        n += 1;
    }
    i32::from(n > 0)
}

/// Doom `raiseToTexture`: raise by the shortest lower-wall span around the sector.
///
/// Without a TEXTUREX height table in the sim, approximate texture height as the
/// absolute floor delta across two-sided lines that have a non-blank lower tex.
fn floor_raise_by_texture(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let raise = shortest_lower_wall_height(world, sid).unwrap_or(64.0);
        if raise <= 0.0 {
            continue;
        }
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: sec.floor_z + raise,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
        started += 1;
    }
    i32::from(started > 0)
}

/// `Floor_LowerByTexture(tag, speed)` — lower by shortest lower-wall span.
fn floor_lower_by_texture(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let drop = shortest_lower_wall_height(world, sid).unwrap_or(64.0);
        if drop <= 0.0 {
            continue;
        }
        world.floors.push(FloorMover {
            sector: sid,
            direction: -1,
            speed,
            dest_z: sec.floor_z - drop,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
        started += 1;
    }
    i32::from(started > 0)
}

/// `Ceiling_Raise/LowerByTexture(tag, speed)` — move by shortest upper/lower wall span.
fn ceiling_by_texture(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    direction: i8,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let delta = shortest_lower_wall_height(world, sid).unwrap_or(64.0);
        if delta <= 0.0 {
            continue;
        }
        let dest = if direction > 0 {
            sec.ceiling_z + delta
        } else {
            (sec.ceiling_z - delta).max(sec.floor_z)
        };
        world.ceilings.push(CeilingMover {
            sector: sid,
            direction,
            speed,
            speed_up: speed,
            bottom_z: dest.min(sec.ceiling_z),
            top_z: dest.max(sec.ceiling_z),
            crush: false,
            once: true,
            stop_at_high: false,
        });
        started += 1;
    }
    i32::from(started > 0)
}

fn shortest_lower_wall_height(world: &World, sector: SectorId) -> Option<f64> {
    let idx = sector.raw() as usize;
    let cur_z = world.sectors.get(idx)?.floor_z;
    let geom = world.map.sectors.get(idx)?;
    let mut best: Option<f64> = None;
    for &lid in &geom.lines {
        let Some(line) = world.map.lines.get(lid.raw() as usize) else {
            continue;
        };
        if line.back_sector.is_none() {
            continue;
        }
        // Prefer the side belonging to this sector.
        let side_id = if line.front_sector == Some(sector) {
            line.sides[0]
        } else if line.back_sector == Some(sector) {
            line.sides[1]
        } else {
            None
        };
        let Some(side_id) = side_id else {
            continue;
        };
        let Some(side) = world.map.sides.get(side_id.raw() as usize) else {
            continue;
        };
        let lower = side.lower.trim();
        if lower.is_empty() || lower == "-" {
            continue;
        }
        let other = if line.front_sector == Some(sector) {
            line.back_sector
        } else {
            line.front_sector
        };
        let Some(other) = other else {
            continue;
        };
        let Some(osec) = world.sectors.get(other.raw() as usize) else {
            continue;
        };
        let gap = (osec.floor_z - cur_z).abs();
        if gap < 0.001 {
            continue;
        }
        best = Some(match best {
            Some(b) => b.min(gap),
            None => gap,
        });
    }
    best
}

fn floor_lower_by_value(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    height_scale: f64,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let height = f64::from(if args[2] != 0 { args[2] } else { 24 }) * height_scale;
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        world.floors.push(FloorMover {
            sector: sid,
            direction: -1,
            speed,
            dest_z: sec.floor_z - height,
            crush: false,
        });
    }
    1
}

fn floor_lower_to_lowest(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    let mut started = 0;
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        let (min_f, _, _, _) = neighbor_plane_extrema(world, sid);
        let dest = min_f.unwrap_or(sec.floor_z - 64.0);
        if dest >= sec.floor_z - 0.001 {
            continue;
        }
        world.floors.push(FloorMover {
            sector: sid,
            direction: -1,
            speed,
            dest_z: dest,
            crush: false,
        });
        started += 1;
    }
    if started > 0 {
        1
    } else {
        // No lower neighbor: fall back to fixed drop (legacy tests / open rooms).
        floor_lower_by(world, args, line, 64.0)
    }
}

fn floor_lower_by(
    world: &mut World,
    args: &[i32; 5],
    line: Option<LineId>,
    default_drop: f64,
) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    // LowerToHighest XLAT often passes lip/height in arg2 (e.g. 128).
    let amount = if args[2] != 0 {
        f64::from(args[2])
    } else {
        default_drop
    };
    let sectors = sectors_for_tag(world, tag, line);
    if sectors.is_empty() {
        return 0;
    }
    for sid in sectors {
        let idx = sid.raw() as usize;
        let Some(sec) = world.sectors.get(idx) else {
            continue;
        };
        world.floors.push(FloorMover {
            sector: sid,
            direction: -1,
            speed,
            dest_z: sec.floor_z - amount,
            crush: false,
        });
    }
    1
}

fn floor_raise_to_nearest(world: &mut World, args: &[i32; 5], line: Option<LineId>) -> i32 {
    let tag = args[0];
    let speed = hexen_speed(args[1]);
    let sectors = sectors_for_tag(world, tag, line);
    let mut started = 0;
    for sid in sectors {
        let dest = nearest_higher_floor(world, sid).unwrap_or_else(|| {
            world
                .sectors
                .get(sid.raw() as usize)
                .map(|s| s.floor_z + 24.0)
                .unwrap_or(24.0)
        });
        world.floors.push(FloorMover {
            sector: sid,
            direction: 1,
            speed,
            dest_z: dest,
            crush: false,
        });
        play_sector_sound(world, sid, "floors/move");
        started += 1;
    }
    i32::from(started > 0)
}

/// Tick floor movers.
pub fn tick_floors(world: &mut World) {
    let mut i = 0;
    while i < world.floors.len() {
        let sector = world.floors[i].sector;
        let crush = world.floors[i].crush;
        let direction = world.floors[i].direction;
        let speed = world.floors[i].speed;
        let dest_z = world.floors[i].dest_z;
        let Some(sec) = world.sectors.get_mut(sector.raw() as usize) else {
            world.floors.swap_remove(i);
            continue;
        };
        let old_floor = sec.floor_z;
        let mut crushing = false;
        let done = match direction {
            1 => {
                crushing = crush;
                sec.floor_z += speed;
                if sec.floor_z >= dest_z {
                    sec.floor_z = dest_z;
                    true
                } else {
                    false
                }
            }
            -1 => {
                sec.floor_z -= speed;
                if sec.floor_z <= dest_z {
                    sec.floor_z = dest_z;
                    true
                } else {
                    false
                }
            }
            _ => true,
        };
        let delta = sec.floor_z - old_floor;
        if crushing {
            crush_sector_actors(world, sector);
        }
        if delta.abs() > 1e-9 {
            carry_floor_actors(world, sector, old_floor, delta);
        }
        if done {
            world.floors.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

/// Ride actors standing on a moving floor (`dropactors` gates downward carry).
fn carry_floor_actors(world: &mut World, sector: SectorId, old_floor: f64, delta: f64) {
    let drop = world
        .map
        .sectors
        .get(sector.raw() as usize)
        .is_some_and(|s| s.drop_actors);
    // Downward floors leave actors behind unless `dropactors` or `+STAYONLIFT`.
    let downward = delta < 0.0 && !drop;
    let bbox = if delta > 0.0 {
        sector_bbox(world, sector)
    } else {
        None
    };
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed {
            continue;
        }
        if downward && !a.extra_flags.get_name("STAYONLIFT") {
            continue;
        }
        let Some(sid) = room_nodebuild::point_in_sector(&world.map, a.pos.x(), a.pos.y())
            .or_else(|| room_nodebuild::point_in_sector_vanilla(&world.map, a.pos.x(), a.pos.y()))
        else {
            continue;
        };
        if sid != sector {
            // Vanilla `P_ChangeSector` → `P_ThingHeightClip`: a rising floor
            // also lifts things whose box only overlaps the sector edge (the
            // lift picks up a player straddling it instead of rising through).
            let touches = bbox.is_some_and(|(x0, y0, x1, y1)| {
                let r = a.radius;
                a.pos.x() + r > x0 && a.pos.x() - r < x1 && a.pos.y() + r > y0 && a.pos.y() - r < y1
            });
            if delta > 0.0 && touches && (a.pos.z() - old_floor).abs() <= 1.5 {
                let check = crate::physics::check_position(world, a, a.pos, Some(id));
                let new_floor = old_floor + delta;
                if (check.floor_z - new_floor).abs() < 1e-6
                    && check.ceiling_z - new_floor >= a.height
                {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.pos = DVec3::new(a.pos.x(), a.pos.y(), new_floor);
                    }
                }
            }
            continue;
        }
        // Standing on the floor before this step (small tolerance for fixed→f64).
        if (a.pos.z() - old_floor).abs() > 1.5 {
            continue;
        }
        if let Some(a) = world.actors.get_mut(id) {
            a.pos = DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + delta);
        }
    }
}

/// Tick all door movers (`T_VerticalDoor` subset).
pub fn tick_doors(world: &mut World) {
    let mut i = 0;
    while i < world.doors.len() {
        let sector = world.doors[i].sector;
        let direction = world.doors[i].direction;
        let speed = world.doors[i].speed;
        let top_z = world.doors[i].top_z;
        let bottom_z = world.doors[i].bottom_z;
        let cwo = world.doors[i].close_wait_open;
        let wait_then_raise = world.doors[i].wait_then_raise;
        let wait_max = world.doors[i].wait_max;
        let Some(sec) = world.sectors.get_mut(sector.raw() as usize) else {
            world.doors.swap_remove(i);
            continue;
        };
        let mut crushing = false;
        let mut old_ceiling = sec.ceiling_z;
        let mut new_dir = direction;
        let mut new_wait = world.doors[i].wait;
        let mut clear_wait_raise = false;
        let done = match direction {
            1 => {
                sec.ceiling_z += speed;
                if sec.ceiling_z >= top_z {
                    sec.ceiling_z = top_z;
                    if cwo || wait_max < 0 {
                        true
                    } else {
                        new_dir = 0;
                        new_wait = wait_max;
                        false
                    }
                } else {
                    false
                }
            }
            -1 => {
                crushing = true;
                old_ceiling = sec.ceiling_z;
                sec.ceiling_z -= speed;
                if sec.ceiling_z <= bottom_z {
                    sec.ceiling_z = bottom_z;
                    if cwo {
                        new_dir = 0;
                        new_wait = wait_max;
                        false
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            0 => {
                if new_wait > 0 {
                    new_wait -= 1;
                }
                if new_wait == 0 {
                    if wait_then_raise {
                        new_dir = 1;
                        clear_wait_raise = true;
                    } else if cwo {
                        new_dir = 1;
                    } else {
                        new_dir = -1;
                    }
                }
                false
            }
            _ => true,
        };
        let mut done = done;
        if crushing && door_squeezes_actor(world, sector) {
            // Vanilla `T_MovePlane(crush = false)` / GZDoom `DDoor::Tick`: a
            // closing door never moves into a living thing — the ceiling is
            // put back, and every door except a plain close goes back up.
            if let Some(sec) = world.sectors.get_mut(sector.raw() as usize) {
                sec.ceiling_z = old_ceiling;
            }
            done = false;
            crushing = false;
            let plain_close = !cwo && wait_max == 0;
            if !plain_close {
                new_dir = 1;
                new_wait = world.doors[i].wait;
                play_sector_sound(world, sector, "doors/open");
                apply_door_sector_lights(world, sector, true);
            } else {
                new_dir = -1;
            }
        }
        let started_move = direction == 0 && new_dir != 0;
        world.doors[i].direction = new_dir;
        world.doors[i].wait = new_wait;
        if clear_wait_raise {
            world.doors[i].wait_then_raise = false;
        }
        // Doors are not crushers (vanilla `T_VerticalDoor` passes crush=false).
        let _ = crushing;
        if started_move {
            let name = if new_dir < 0 {
                "doors/close"
            } else {
                "doors/open"
            };
            play_sector_sound(world, sector, name);
            apply_door_sector_lights(world, sector, new_dir > 0);
        }
        if done {
            world.doors.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::World;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn locked_door_requires_key() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = room_decorate::parse_decorate(src).unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let args = [1, 16, 150, 2 | 128, 0]; // blue card lock
        assert_eq!(
            execute_special(&mut w, num::DOOR_LOCKED_RAISE, &args, Some(mo), None),
            0
        );
        assert!(w.doors.is_empty());
        assert!(
            w.drain_messages().iter().any(|m| m.contains("blue key")),
            "expected locked-door HUD message"
        );
        let blue = w.classes.find_name("BlueCard").unwrap();
        assert!(crate::inventory::give_inventory(&mut w, mo, blue, 1));
        assert_eq!(
            execute_special(&mut w, num::DOOR_LOCKED_RAISE, &args, Some(mo), None),
            1
        );
        assert!(!w.doors.is_empty());
    }

    #[test]
    fn udmf_locknumber_overrides_door_lock_arg() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = room_decorate::parse_decorate(src).unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        apply_nodes(&mut map);
        map.lines[0].lock_number = 2 | 128; // blue card
        map.lines[0].tag = 9;
        let lid = LineId::from_raw(0);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        // Arg lock is red (5|128); UDMF locknumber must win.
        let args = [1, 16, 150, 5 | 128, 0];
        assert_eq!(
            execute_special(&mut w, num::DOOR_LOCKED_RAISE, &args, Some(mo), Some(lid)),
            0
        );
        let blue = w.classes.find_name("BlueCard").unwrap();
        assert!(crate::inventory::give_inventory(&mut w, mo, blue, 1));
        assert_eq!(
            execute_special(&mut w, num::DOOR_LOCKED_RAISE, &args, Some(mo), Some(lid)),
            1
        );
        assert!(!w.doors.is_empty());
    }

    #[test]
    fn teleport_new_map_sets_override() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::TELEPORT_NEW_MAP, &[5, 0, 0, 0, 0], None, None),
            1
        );
        assert!(w.level.exiting);
        assert_eq!(w.level.next_map_override.as_deref(), Some("MAP05"));
    }

    #[test]
    fn thrust_thing_adds_velocity() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        // angle 0 = east, force 10
        assert_eq!(
            execute_special(&mut w, num::THRUST_THING, &[0, 10, 0, 0, 0], Some(mo), None),
            1
        );
        let a = w.actors.get(mo).unwrap();
        assert!(a.vel.x() > 9.0, "vx={}", a.vel.x());
        assert!(a.vel.y().abs() < 1e-6);
    }

    #[test]
    fn pillar_build_starts_floor_and_ceiling() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(3);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::PILLAR_BUILD, &[3, 8, 0, 0, 0], None, None),
            1
        );
        assert!(!w.floors.is_empty());
        assert!(!w.ceilings.is_empty());
        let mid = 64.0;
        assert!((w.floors[0].dest_z - mid).abs() < 1e-6);
        assert!((w.ceilings[0].bottom_z - mid).abs() < 1e-6);
    }

    #[test]
    fn thing_spawn_at_tid_spot() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Spot : Actor 9001
{
  +NOBLOCKMAP
  +NOSECTOR
  States { Spawn: TNT1 A -1 Stop }
}
actor Imp : Actor 3001
{
  Health 60
  Radius 20
  Height 56
  States { Spawn: TROO A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let spot = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(100.0, 120.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(spot).unwrap().tid = 7;
        assert_eq!(
            execute_special(
                &mut w,
                num::THING_SPAWN_NO_FOG,
                &[7, 3001, 0, 42, 0],
                None,
                None
            ),
            1
        );
        let spawned = w
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                w.actors
                    .get(id)
                    .is_some_and(|a| !a.destroyed && a.tid == 42)
            })
            .expect("spawned tid 42");
        let a = w.actors.get(spawned).unwrap();
        assert!((a.pos.x() - 100.0).abs() < 1e-6);
        assert!((a.pos.y() - 120.0).abs() < 1e-6);
    }

    #[test]
    fn thing_remove_and_deactivate() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9901
{
  Health 10
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let id = w
            .spawn_at(
                classes.find_name("Dummy").unwrap(),
                DVec3::new(80.0, 80.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(id).unwrap().tid = 9;
        assert_eq!(
            execute_special(&mut w, num::THING_DEACTIVATE, &[9, 0, 0, 0, 0], None, None),
            1
        );
        assert_ne!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_DORMANT,
            0
        );
        assert_eq!(
            execute_special(&mut w, num::THING_REMOVE, &[9, 0, 0, 0, 0], None, None),
            1
        );
        assert!(w.actors.get(id).unwrap().destroyed);
    }

    #[test]
    fn door_close_wait_open_cycle() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(2);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let top = w.sectors[0].ceiling_z;
        assert_eq!(
            execute_special(
                &mut w,
                num::DOOR_CLOSE_WAIT_OPEN,
                &[2, 16, 2, 0, 0],
                None,
                None
            ),
            1
        );
        // Close 72 mu @ hexen speed 16 → 2 mu/tic ⇒ 36 tics.
        for _ in 0..36 {
            tick_doors(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - w.sectors[0].floor_z).abs() < 1e-6);
        assert_eq!(w.doors[0].direction, 0);
        // Wait 2 tics then open.
        tick_doors(&mut w);
        tick_doors(&mut w);
        assert_eq!(w.doors[0].direction, 1);
        for _ in 0..36 {
            tick_doors(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - top).abs() < 1e-6);
        assert!(w.doors.is_empty());
    }

    #[test]
    fn elevator_raise_moves_floor_and_ceiling() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(8);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let height = w.sectors[0].ceiling_z - w.sectors[0].floor_z;
        assert_eq!(
            execute_special(
                &mut w,
                num::ELEVATOR_RAISE_TO_NEAREST,
                &[8, 32, 0, 0, 0],
                None,
                None
            ),
            1
        );
        // Hexen speed 32 → 4 mu/tic; raise 0→64 needs 16 tics.
        for _ in 0..16 {
            tick_elevators(&mut w);
        }
        assert!((w.sectors[0].floor_z - 64.0).abs() < 1e-6);
        assert!((w.sectors[0].ceiling_z - (64.0 + height)).abs() < 1e-6);
        assert!(w.elevators.is_empty());
    }

    #[test]
    fn door_closing_never_crushes_player() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Height 56
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(3);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        // Start a closing door from current ceiling.
        w.doors.push(DoorMover {
            sector: room_core::SectorId::from_raw(0),
            direction: -1,
            speed: 16.0,
            top_z: 72.0,
            bottom_z: 0.0,
            wait: 0,
            wait_max: 0,
            close_wait_open: false,
            wait_then_raise: false,
        });
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(mo).unwrap().health;
        for _ in 0..5 {
            w.level.time = 0;
            tick_doors(&mut w);
        }
        // Vanilla `T_VerticalDoor`: a door never crushes — a plain close
        // stops above the player's head, without damage.
        assert!(w.sectors[0].ceiling_z >= 56.0);
        assert_eq!(w.actors.get(mo).unwrap().health, hp0);
        // A raise door (wait > 0) bounces back up instead.
        w.doors[0].wait_max = 150;
        w.doors[0].direction = -1;
        tick_doors(&mut w);
        assert_eq!(w.doors[0].direction, 1);
    }

    #[test]
    fn door_raise_plays_open_sound() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None),
            1
        );
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "doors/open"),
            "door raise should cue doors/open"
        );
    }

    #[test]
    fn sector_sound_sequence_picks_open_and_close_choices() {
        use crate::sndseq::{SndSeqDef, SndSeqStep};
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        map.sectors[0].sound_sequence = "Hatch".into();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        for (name, sound) in [("UP", "hatch/up"), ("DOWN", "hatch/down")] {
            w.sndseq.sequences.insert(
                name.into(),
                SndSeqDef {
                    name: name.into(),
                    steps: vec![SndSeqStep::Play(sound.into()), SndSeqStep::End],
                    stopsound: None,
                },
            );
        }
        w.sndseq
            .choices
            .insert("HATCH".into(), vec![(0, "UP".into()), (1, "DOWN".into())]);
        let heard = |w: &mut World| {
            crate::sndseq::tick_sequences(w);
            w.drain_sound_events()
                .into_iter()
                .map(|e| e.name)
                .collect::<Vec<_>>()
        };
        execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None);
        assert_eq!(heard(&mut w), ["hatch/up"]);
        execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None);
        assert_eq!(heard(&mut w), ["hatch/down"]);
    }

    #[test]
    fn udmf_silent_sector_skips_door_sound() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        map.sectors[0].silent = true;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None),
            1
        );
        assert!(
            w.drain_sound_events().is_empty(),
            "UDMF silent sector suppresses doors/open"
        );
    }

    #[test]
    fn door_raise_lifts_ceiling() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        // Tag sector 0.
        map.sectors[0].tags.push(1);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let args = [1, 16, 10, 0, 0];
        assert_eq!(
            execute_special(&mut w, num::DOOR_RAISE, &args, None, None),
            1
        );
        assert_eq!(w.doors.len(), 1);
        let z0 = w.sectors[0].ceiling_z;
        tick_doors(&mut w);
        assert!(
            w.sectors[0].ceiling_z > z0,
            "ceiling {z0} -> {}",
            w.sectors[0].ceiling_z
        );
    }

    #[test]
    fn door_raise_brightens_se8_rooms_and_restores_them() {
        let mut map = MapBuilder::new()
            .room("door", rect(0.0, 0.0, 128.0, 128.0), 0.0, 8.0)
            .room("room", rect(128.0, 0.0, 256.0, 128.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        map.sectors[0].light = 80;
        map.sectors[1].tags.push(3);
        map.sectors[1].light = 80;
        if map.udmf_user.sectors.len() < 2 {
            map.udmf_user.sectors.resize(2, Default::default());
        }
        map.udmf_user.sectors[0]
            .ints
            .insert("user_doorlight".into(), 3);
        map.udmf_user.sectors[0]
            .ints
            .insert("user_doorbright".into(), 200);
        map.udmf_user.sectors[0]
            .ints
            .insert("user_doorlightopen".into(), 1);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.sectors[1].light, 80);
        assert_eq!(
            execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None),
            1
        );
        assert_eq!(w.sectors[1].light, 200);
        assert_eq!(
            execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None),
            1
        );
        assert_eq!(w.sectors[1].light, 80);
    }

    #[test]
    fn door_close_brightens_se9_rooms() {
        let mut map = MapBuilder::new()
            .room("door", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .room("room", rect(128.0, 0.0, 256.0, 128.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        map.sectors[0].light = 80;
        map.sectors[1].tags.push(3);
        map.sectors[1].light = 80;
        if map.udmf_user.sectors.len() < 2 {
            map.udmf_user.sectors.resize(2, Default::default());
        }
        map.udmf_user.sectors[0]
            .ints
            .insert("user_doorlight".into(), 3);
        map.udmf_user.sectors[0]
            .ints
            .insert("user_doorbright".into(), 200);
        map.udmf_user.sectors[0]
            .ints
            .insert("user_doorlightopen".into(), 0);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None),
            1
        );
        assert_eq!(w.sectors[1].light, 80);
        assert_eq!(
            execute_special(&mut w, num::DOOR_CLOSE, &[1, 16, 0, 0, 0], None, None),
            1
        );
        assert_eq!(w.sectors[1].light, 200);
    }

    #[test]
    fn rotating_floor_carries_an_actor_around_the_pivot() {
        let mut map = MapBuilder::new()
            .room("gear", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(192.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if map.udmf_user.sectors.is_empty() {
            map.udmf_user.sectors.resize(1, Default::default());
        }
        map.udmf_user.sectors[0].ints.insert("user_spin".into(), 27);
        map.udmf_user.sectors[0]
            .ints
            .insert("user_spinx".into(), 128);
        map.udmf_user.sectors[0]
            .ints
            .insert("user_spiny".into(), 128);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.spinners.len(), 1);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let (x0, y0) = {
            let a = w.actors.get(mo).unwrap();
            (a.pos.x(), a.pos.y())
        };
        for _ in 0..35 {
            tick_spinners(&mut w);
        }
        let a = w.actors.get(mo).unwrap();
        let (x1, y1) = (a.pos.x(), a.pos.y());
        let d0 = room_core::dmath::sqrt((x0 - 128.0) * (x0 - 128.0) + (y0 - 128.0) * (y0 - 128.0));
        let d1 = room_core::dmath::sqrt((x1 - 128.0) * (x1 - 128.0) + (y1 - 128.0) * (y1 - 128.0));
        assert!(
            (d0 - d1).abs() < 4.0,
            "stayed on the circle {d0} -> {d1} at ({x1},{y1})"
        );
        assert!(
            (x1 - x0).hypot(y1 - y0) > 8.0,
            "moved around the pivot ({x0},{y0}) -> ({x1},{y1})"
        );
        let (floor_a, _) = w.plane_angles[&0];
        assert!(floor_a.abs() > 1.0, "floor texture should spin ({floor_a})");
    }

    #[test]
    fn subway_carries_an_actor_along_locators() {
        let mut map = MapBuilder::new()
            .room("car", rect(0.0, 0.0, 256.0, 256.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if map.udmf_user.sectors.is_empty() {
            map.udmf_user.sectors.resize(1, Default::default());
        }
        let bag = &mut map.udmf_user.sectors[0].ints;
        bag.insert("user_subway".into(), 1);
        bag.insert("user_subspeed".into(), 64);
        bag.insert("user_subx".into(), 32);
        bag.insert("user_suby".into(), 32);
        bag.insert("user_subn".into(), 2);
        bag.insert("user_sx0".into(), 200);
        bag.insert("user_sy0".into(), 32);
        bag.insert("user_sx1".into(), 200);
        bag.insert("user_sy1".into(), 200);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.subways.len(), 1);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let (x0, y0) = {
            let a = w.actors.get(mo).unwrap();
            (a.pos.x(), a.pos.y())
        };
        for _ in 0..35 {
            tick_subways(&mut w);
        }
        let a = w.actors.get(mo).unwrap();
        let (x1, y1) = (a.pos.x(), a.pos.y());
        assert!(
            x1 > x0 + 16.0,
            "rode toward the first locator ({x0},{y0}) -> ({x1},{y1})"
        );
        assert!(
            (y1 - y0).abs() < 8.0,
            "stayed on the first leg {y0} -> {y1}"
        );
    }

    #[test]
    fn stretch_moves_sector_vertices() {
        let mut map = MapBuilder::new()
            .room("bridge", rect(0.0, 0.0, 128.0, 64.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(3);
        if map.udmf_user.sectors.is_empty() {
            map.udmf_user.sectors.resize(1, Default::default());
        }
        let bag = &mut map.udmf_user.sectors[0].ints;
        bag.insert("user_stretch".into(), 1);
        bag.insert("user_stdx".into(), 256);
        bag.insert("user_stdy".into(), 0);
        bag.insert("user_stdist".into(), 32);
        bag.insert("user_stspd".into(), 32);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.stretches.len(), 1);
        let vid = w.stretches[0].verts[0];
        let x0 = w.map.vertices[vid.raw() as usize].pos.x();
        assert_eq!(
            execute_special(&mut w, num::SECTOR_STRETCH, &[3, 0, 0, 0, 0], None, None),
            1
        );
        for _ in 0..20 {
            tick_stretches(&mut w);
        }
        let x1 = w.map.vertices[vid.raw() as usize].pos.x();
        assert!(x1 > x0 + 8.0, "stretched {x0} -> {x1}");
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let a = w.actors.get(mo).unwrap();
        assert!(a.pos.x() > 32.0, "carried with the stretch {}", a.pos.x());
    }

    #[test]
    fn teeth_retract_when_the_door_ceiling_opens() {
        let mut map = MapBuilder::new()
            .room("door", rect(0.0, 0.0, 128.0, 128.0), 0.0, 16.0)
            .room("teeth", rect(128.0, 0.0, 192.0, 64.0), 0.0, 72.0)
            .player_start(140.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if map.udmf_user.sectors.len() < 2 {
            map.udmf_user.sectors.resize(2, Default::default());
        }
        let bag = &mut map.udmf_user.sectors[1].ints;
        bag.insert("user_teeth".into(), 1);
        bag.insert("user_thdx".into(), 256);
        bag.insert("user_thdy".into(), 0);
        bag.insert("user_thdist".into(), 32);
        bag.insert("user_thspd".into(), 32);
        bag.insert("user_thdoor".into(), 0);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.teeth.len(), 1);
        let vid = w.teeth[0].verts[0];
        let x0 = w.map.vertices[vid.raw() as usize].pos.x();
        w.sectors[0].ceiling_z = 80.0;
        for _ in 0..20 {
            tick_teeth(&mut w);
        }
        let x1 = w.map.vertices[vid.raw() as usize].pos.x();
        assert!(x1 > x0 + 8.0, "teeth retracted {x0} -> {x1}");
    }

    #[test]
    fn skew_oscillates_sector_vertices() {
        let mut map = MapBuilder::new()
            .room("reactor", rect(0.0, 0.0, 128.0, 64.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if map.udmf_user.sectors.is_empty() {
            map.udmf_user.sectors.resize(1, Default::default());
        }
        let bag = &mut map.udmf_user.sectors[0].ints;
        bag.insert("user_skew".into(), 1);
        bag.insert("user_skdx".into(), 256);
        bag.insert("user_skdy".into(), 0);
        bag.insert("user_skdist".into(), 32);
        bag.insert("user_skspd".into(), 32);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.skews.len(), 1);
        let vid = w.skews[0].verts[0];
        let x0 = w.map.vertices[vid.raw() as usize].pos.x();
        for _ in 0..20 {
            tick_skews(&mut w);
        }
        let x1 = w.map.vertices[vid.raw() as usize].pos.x();
        assert!(x1 > x0 + 8.0, "skewed {x0} -> {x1}");
        for _ in 0..40 {
            tick_skews(&mut w);
        }
        let x2 = w.map.vertices[vid.raw() as usize].pos.x();
        assert!(x2 < x1 - 4.0, "skew reversed {x1} -> {x2}");
    }

    #[test]
    fn demo_cameras_lock_the_view() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = r#"
actor DukeCamera : Actor 9942
{
  Health 1
  States { Spawn: POSS A -1 Stop }
}
"#;
        let ir = room_decorate::parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let cam_c = w.classes.find_name("DukeCamera").unwrap();
        let cam = w
            .spawn_at(
                cam_c,
                room_core::DVec3::new(400.0, 400.0, 32.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        w.set_demo_cameras(true);
        tick_demo_cameras(&mut w);
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.camera, Some(cam));
        assert!(!p.camera_revert);
        assert_eq!(w.camera_textures.get("VCAMA0").map(|b| b.camera), Some(cam));
    }

    #[test]
    fn floor_raise_and_crush_hexen_number() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(7);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_AND_CRUSH,
                &[7, 8, 10, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(w.floors[0].crush);
        assert!((w.floors[0].dest_z - 120.0).abs() < 1e-6);
    }

    #[test]
    fn floor_raise_and_crush_stops_under_ceiling() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(13);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_AND_CRUSH_DOOM,
                &[13, 16, 10, 2, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..160 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - (w.sectors[0].ceiling_z - 8.0)).abs() < 1e-6);
    }

    #[test]
    fn floor_raise_and_crush_damages_player() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Height 56
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(14);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(mo).unwrap().health;
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_AND_CRUSH_DOOM,
                &[14, 16, 10, 2, 0],
                None,
                None
            ),
            1
        );
        // Dest is ceiling-8 = 64; floor rises 0→64 @ hexen 16 → 2 mu/tic ⇒ 32 tics.
        for _ in 0..32 {
            w.level.time = 0;
            tick_floors(&mut w);
        }
        let gap = w.sectors[0].ceiling_z - w.sectors[0].floor_z;
        assert!(gap < 56.0, "gap {gap}");
        let hp1 = w.actors.get(mo).unwrap().health;
        assert!(hp1 < hp0, "floor crush should deal damage ({hp1} vs {hp0})");
    }

    #[test]
    fn floor_lower_to_highest_drops() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(15);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let z0 = w.sectors[0].floor_z;
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_LOWER_TO_HIGHEST,
                &[15, 16, 32, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..32 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - (z0 - 32.0)).abs() < 1e-6);
    }

    #[test]
    fn floor_lower_dropactors_false_leaves_actor_z() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(16);
        map.sectors[0].drop_actors = false;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let z0 = w.actors.get(mo).unwrap().pos.z();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_LOWER_BY_VALUE,
                &[16, 16, 32, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..32 {
            tick_floors(&mut w);
        }
        assert!(
            (w.actors.get(mo).unwrap().pos.z() - z0).abs() < 1e-6,
            "dropactors=false should not ride the floor down"
        );
        assert!((w.sectors[0].floor_z + 32.0).abs() < 1e-6);
    }

    #[test]
    fn stayonlift_rides_a_floor_that_does_not_drop_actors() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(16);
        map.sectors[0].drop_actors = false;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors
            .get_mut(mo)
            .unwrap()
            .extra_flags
            .set_name("STAYONLIFT", true);
        let z0 = w.actors.get(mo).unwrap().pos.z();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_LOWER_BY_VALUE,
                &[16, 16, 32, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..32 {
            tick_floors(&mut w);
        }
        assert!(
            (w.actors.get(mo).unwrap().pos.z() - (z0 - 32.0)).abs() < 1e-6,
            "STAYONLIFT rides the floor down"
        );
    }

    #[test]
    fn notelefrag_survives_an_overlapping_teleport() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = room_decorate::parse_decorate(src).unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let pos = w.actors.get(mo).unwrap().pos;
        let imp = classes.find_name("DoomImp").unwrap();
        let victim = w.spawn_at(imp, pos, room_core::Angle::ZERO).unwrap();
        w.actors.get_mut(victim).unwrap().flags |= crate::class::FLAG_SHOOTABLE;
        telefrag_occupants(&mut w, mo);
        assert!(w.actors.get(victim).unwrap().health <= 0);

        let spared = w.spawn_at(imp, pos, room_core::Angle::ZERO).unwrap();
        {
            let a = w.actors.get_mut(spared).unwrap();
            a.flags |= crate::class::FLAG_SHOOTABLE;
            a.extra_flags.set_name("NOTELEFRAG", true);
            a.health = 60;
        }
        telefrag_occupants(&mut w, mo);
        assert!(w.actors.get(spared).unwrap().health > 0);
    }

    #[test]
    fn floor_lower_dropactors_true_rides_actor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(17);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let z0 = w.actors.get(mo).unwrap().pos.z();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_LOWER_BY_VALUE,
                &[17, 16, 32, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..32 {
            tick_floors(&mut w);
        }
        assert!(
            (w.actors.get(mo).unwrap().pos.z() - (z0 - 32.0)).abs() < 1e-6,
            "default dropactors rides the floor down"
        );
    }

    #[test]
    fn plat_down_by_value_rides_actor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(18);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let z0 = w.actors.get(mo).unwrap().pos.z();
        // height arg ×8 → 32mu drop; long wait so we stay at low.
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_DOWN_BY_VALUE,
                &[18, 16, 105, 4, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..64 {
            tick_plats(&mut w);
        }
        assert!(
            (w.sectors[0].floor_z - (z0 - 32.0)).abs() < 1e-6,
            "plat should reach low"
        );
        assert!(
            (w.actors.get(mo).unwrap().pos.z() - (z0 - 32.0)).abs() < 1e-6,
            "actors ride plats like floors"
        );
    }

    #[test]
    fn plat_down_dropactors_false_leaves_actor_z() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(19);
        map.sectors[0].drop_actors = false;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let z0 = w.actors.get(mo).unwrap().pos.z();
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_DOWN_BY_VALUE,
                &[19, 16, 105, 4, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..64 {
            tick_plats(&mut w);
        }
        assert!(
            (w.actors.get(mo).unwrap().pos.z() - z0).abs() < 1e-6,
            "dropactors=false should not ride the plat down"
        );
    }

    #[test]
    fn floor_raise_by_texture_uses_shortest_lower_gap() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 40.0, 168.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let low = map
            .sectors
            .iter()
            .position(|s| s.floor_height.abs() < 1e-6)
            .unwrap();
        map.sectors[low].tags.push(30);
        // Put a lower texture on the shared wall side facing the low sector.
        for line in &map.lines {
            if matches!(
                (line.front_sector.map(|s| s.raw() as usize), line.back_sector.map(|s| s.raw() as usize)),
                (Some(f), Some(b)) if f == low || b == low
            ) {
                if let Some(sid) = line.sides[0] {
                    if let Some(side) = map.sides.get_mut(sid.raw() as usize) {
                        if side.sector.raw() as usize == low {
                            side.lower = "STEP2".into();
                        }
                    }
                }
                if let Some(sid) = line.sides[1] {
                    if let Some(side) = map.sides.get_mut(sid.raw() as usize) {
                        if side.sector.raw() as usize == low {
                            side.lower = "STEP2".into();
                        }
                    }
                }
            }
        }
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_BY_TEXTURE,
                &[30, 8, 0, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..128 {
            tick_floors(&mut w);
        }
        assert!(
            (w.sectors[low].floor_z - 40.0).abs() < 1e-6,
            "expected raise by 40, got {}",
            w.sectors[low].floor_z
        );
    }

    #[test]
    fn generic_door_kind0_raises_like_door_raise() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(2);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let z0 = w.sectors[0].ceiling_z;
        assert_eq!(
            execute_special(&mut w, num::GENERIC_DOOR, &[2, 16, 0, 150, 0], None, None),
            1
        );
        tick_doors(&mut w);
        assert!(w.sectors[0].ceiling_z > z0);
    }

    #[test]
    fn floor_raise_by_value_lifts_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(7);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let args = [7, 8, 24, 0, 0];
        assert_eq!(
            execute_special(&mut w, num::FLOOR_RAISE_BY_VALUE, &args, None, None),
            1
        );
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "floors/move"),
            "floor raise should cue floors/move"
        );
        let z0 = w.sectors[0].floor_z;
        tick_floors(&mut w);
        assert!(w.sectors[0].floor_z > z0);
        for _ in 0..80 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - (z0 + 24.0)).abs() < 1e-6);
    }

    #[test]
    fn teleport_line_moves_to_tagged_line() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 384.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        // Tag a line in room b as dest id 7; source is a different line.
        let dest_line = map
            .lines
            .iter()
            .enumerate()
            .find(|(_, l)| l.front_sector == Some(room_core::SectorId::from_raw(1)))
            .map(|(i, _)| i)
            .expect("dest line");
        map.lines[dest_line].tag = 7;
        let src_line = map
            .lines
            .iter()
            .enumerate()
            .find(|(i, l)| {
                *i != dest_line && l.front_sector == Some(room_core::SectorId::from_raw(0))
            })
            .map(|(i, _)| i)
            .expect("src line");
        map.lines[src_line].tag = 7;
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::TELEPORT_LINE,
                &[7, 7, 1, 0, 0],
                Some(mo),
                Some(LineId::from_raw(src_line as u32)),
            ),
            1
        );
        let a = w.actors.get(mo).unwrap();
        assert!(
            a.pos.x() > 200.0,
            "expected teleport into room b, x={}",
            a.pos.x()
        );
        assert!(w.drain_sound_events().is_empty(), "silent teleport");
    }

    #[test]
    fn teleport_moves_activator() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 200,
            y: 200,
            z: 0,
            angle: 90,
            type_id: 14,
            flags: 0,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: -2147483648,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: -32768,
            roll: -32768,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
            count_secret: false,
            no_count: false,
            arg0_str: String::new(),
            conversation: 0,
            accuracy: i32::MIN,
            stamina: i32::MIN,
            species: String::new(),
            melee_range: -1.0,
            view_height: -1.0,
            mass: i32::MIN,
            speed: -1.0,
            friction: -1.0,
            damage_factor: -1.0,
            max_step_height: -1.0,
            max_dropoff_height: -1.0,
            float_speed: -1.0,
            fast_speed: -1.0,
            jump_z: -1.0,
            attack_z_offset: -1.0,
        });
        apply_nodes(&mut map);
        // Doom EV_Teleport: tag selects the sector holding TeleportDest.
        if let Some(sid) = room_nodebuild::point_in_sector(&map, 200.0, 200.0) {
            map.sectors[sid.raw() as usize].tags.push(7);
        }
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let args = [0, 7, 0, 0, 0];
        assert_eq!(
            execute_special(&mut w, num::TELEPORT, &args, Some(mo), None),
            1
        );
        let a = w.actors.get(mo).unwrap();
        assert!((a.pos.x() - 200.0).abs() < 1e-6);
        assert!((a.pos.y() - 200.0).abs() < 1e-6);
        assert!(!w.drain_sound_events().is_empty());
    }

    #[test]
    fn teleport_spawns_fog_at_origin_and_dest() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = room_decorate::parse_decorate(src).unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&file).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 200,
            y: 200,
            z: 0,
            angle: 90,
            type_id: 14,
            flags: 0,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: -2147483648,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: -32768,
            roll: -32768,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
            count_secret: false,
            no_count: false,
            arg0_str: String::new(),
            conversation: 0,
            accuracy: i32::MIN,
            stamina: i32::MIN,
            species: String::new(),
            melee_range: -1.0,
            view_height: -1.0,
            mass: i32::MIN,
            speed: -1.0,
            friction: -1.0,
            damage_factor: -1.0,
            max_step_height: -1.0,
            max_dropoff_height: -1.0,
            float_speed: -1.0,
            fast_speed: -1.0,
            jump_z: -1.0,
            attack_z_offset: -1.0,
        });
        apply_nodes(&mut map);
        if let Some(sid) = room_nodebuild::point_in_sector(&map, 200.0, 200.0) {
            map.sectors[sid.raw() as usize].tags.push(1);
        }
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let fog_class = classes.find_name("TeleportFog").unwrap();
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == fog_class))
            .count();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            execute_special(&mut w, num::TELEPORT, &[0, 1, 0, 0, 0], Some(mo), None),
            1
        );
        let after = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == fog_class))
            .count();
        assert_eq!(after, before + 2, "fog at leave + arrive");
        let teleports = w
            .drain_sound_events()
            .into_iter()
            .filter(|e| e.name == "misc/teleport")
            .count();
        assert_eq!(teleports, 2);
    }

    #[test]
    fn generic_lift_type1_dwus_lip() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(5);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let high = w.sectors[0].floor_z;
        assert_eq!(
            execute_special(&mut w, num::GENERIC_LIFT, &[5, 8, 2, 1, 0], None, None),
            1
        );
        assert_eq!(w.plats.len(), 1);
        assert!((w.plats[0].low - (high - 8.0)).abs() < 1e-6);
        for _ in 0..160 {
            tick_plats(&mut w);
        }
        assert!((w.sectors[0].floor_z - high).abs() < 1e-6);
    }

    #[test]
    fn generic_lift_type0_raises_by_value() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(6);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::GENERIC_LIFT, &[6, 8, 0, 0, 3], None, None),
            1
        );
        for _ in 0..64 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - 24.0).abs() < 1e-6);
    }

    #[test]
    fn plat_dwus_lowers_then_rises() {
        // Raised platform next to a lower neighbor (Doom lowest-surrounding target).
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 64.0, 192.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(3);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let args = [3, 16, 2, 0, 0];
        assert_eq!(
            execute_special(&mut w, num::PLAT_DOWN_WAIT_UP_STAY, &args, None, None),
            1
        );
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "platforms/start"),
            "plat start should cue platforms/start"
        );
        let high = w.sectors[0].floor_z;
        assert!(
            (w.plats[0].low - (0.0 + 8.0)).abs() < 1e-6,
            "low=neighbor+lip"
        );
        tick_plats(&mut w);
        assert!(w.sectors[0].floor_z < high);
        for _ in 0..160 {
            tick_plats(&mut w);
        }
        assert!((w.sectors[0].floor_z - high).abs() < 1e-6);
        assert!(w.plats.is_empty());
    }

    #[test]
    fn ceiling_raise_to_nearest_lifts() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(6);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let z0 = w.sectors[0].ceiling_z;
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_RAISE_TO_NEAREST,
                &[6, 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..64 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - (z0 + 64.0)).abs() < 1e-6);
        assert!(w.ceilings.is_empty());
    }

    #[test]
    fn ceiling_lower_to_floor_drops() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(5);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let floor = w.sectors[0].floor_z;
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_LOWER_TO_FLOOR,
                &[5, 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..128 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - floor).abs() < 1e-6);
        assert!(w.ceilings.is_empty());
    }

    #[test]
    fn ceiling_crush_damages_player_when_gap_too_small() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Height 56
  CrushPainSound "player/crush"
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(11);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(mo).unwrap().health;
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_CRUSH_AND_RAISE,
                &[11, 8, 16, 10, 0],
                None,
                None
            ),
            1
        );
        // Lower until gap < player height (56); hexen speed 16 → 2 mu/tic.
        // Keep `level.time` a multiple of 4 so crush damage applies.
        for _ in 0..40 {
            w.level.time = 0;
            tick_ceilings(&mut w);
        }
        let gap = w.sectors[0].ceiling_z - w.sectors[0].floor_z;
        assert!(gap < 56.0, "gap {gap}");
        let hp1 = w.actors.get(mo).unwrap().health;
        assert!(hp1 < hp0, "crush should deal damage ({hp1} vs {hp0})");
        let sounds = w.drain_sound_events();
        assert!(
            sounds.iter().any(|e| e.name == "player/crush"),
            "expected CrushPainSound, got {sounds:?}"
        );
        assert!(
            sounds.iter().any(|e| e.name == "floors/move"),
            "noisy crusher should cue floors/move"
        );
    }

    #[test]
    fn crush_does_not_hurt_actors_outside_sector() {
        // Two rooms: close a door in room B must not crush the player in room A.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(256.0, 0.0, 384.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[1].tags.push(22);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hp0 = w.actors.get(mo).unwrap().health;
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_LOWER_AND_CRUSH,
                &[22, 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..80 {
            w.level.time = 0;
            tick_ceilings(&mut w);
        }
        let gap_b = w.sectors[1].ceiling_z - w.sectors[1].floor_z;
        assert!(gap_b < 56.0, "door B should be low (gap {gap_b})");
        let hp1 = w.actors.get(mo).unwrap().health;
        assert_eq!(
            hp1, hp0,
            "player in sector A must not take crush damage from sector B"
        );
    }

    #[test]
    fn generic_crusher_silent_skips_move_sound() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(13);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::GENERIC_CRUSHER, &[13, 8, 8, 1, 0], None, None),
            1
        );
        assert!(
            !w.drain_sound_events()
                .iter()
                .any(|e| e.name == "floors/move"),
            "silent generic crusher should not play floors/move"
        );
    }

    #[test]
    fn crusher_gibs_corpse_with_blood() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = room_decorate::parse_decorate(src).unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&file).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(12);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let blood = classes.find_name("Blood").unwrap();
        let zid = w
            .spawn_at(zombie, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        let _ = damage_mobj(&mut w, zid, None, None, 9999);
        assert!(w.actors.get(zid).unwrap().health <= 0);
        // Corpse after A_NoBlocking: clear shootable so only the gib path runs.
        if let Some(a) = w.actors.get_mut(zid) {
            a.flags &= !crate::class::FLAG_SHOOTABLE;
            a.height = 16.0;
        }
        let blood0 = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == blood))
            .count();
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_CRUSH_AND_RAISE,
                &[12, 8, 16, 10, 0],
                None,
                None
            ),
            1
        );
        // Hexen speed 16 → 2 mu/tic from 128 → floor+8 (120 mu) ⇒ 60 tics.
        for _ in 0..60 {
            w.level.time = 0;
            tick_ceilings(&mut w);
        }
        let gap = w.sectors[0].ceiling_z - w.sectors[0].floor_z;
        assert!(gap <= 8.0, "gap {gap}");
        assert!(w.actors.get(zid).unwrap().destroyed);
        let blood1 = w
            .thinkers
            .default
            .iter()
            .filter(|&&id| w.actors.get(id).is_some_and(|a| a.class == blood))
            .count();
        assert!(blood1 >= blood0 + 3, "expected gib blood spray");
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "misc/gibbed")
        );
    }

    #[test]
    fn ceiling_lower_to_highest_floor_uses_neighbor() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 192.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 64.0, 192.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(20);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_LOWER_TO_HIGHEST_FLOOR,
                &[20, 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..128 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - 64.0).abs() < 1e-6);
    }

    #[test]
    fn floor_and_ceiling_raise_by_value_moves_both() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(22);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_AND_CEILING_RAISE_BY_VALUE,
                &[22, 8, 16, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..20 {
            tick_floors(&mut w);
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].floor_z - 16.0).abs() < 1e-6);
        assert!((w.sectors[0].ceiling_z - 144.0).abs() < 1e-6);
    }

    #[test]
    fn floor_and_ceiling_lower_raise_opens_gap() {
        let mut map = MapBuilder::new()
            .room("mid", rect(0.0, 0.0, 128.0, 128.0), 32.0, 96.0)
            .room("low", rect(128.0, 0.0, 256.0, 128.0), 0.0, 96.0)
            .room("high", rect(256.0, 0.0, 384.0, 128.0), 32.0, 160.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(21);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_AND_CEILING_LOWER_RAISE,
                &[21, 8, 8, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..160 {
            tick_floors(&mut w);
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].floor_z - 0.0).abs() < 1e-6);
        assert!((w.sectors[0].ceiling_z - 160.0).abs() < 1e-6);
    }

    #[test]
    fn generic_crusher_uses_asymmetric_speeds() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(15);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let top = w.sectors[0].ceiling_z;
        // Down 16, up 8 → 120 units: 8 tics down, 15 tics up.
        assert_eq!(
            execute_special(
                &mut w,
                num::GENERIC_CRUSHER,
                &[15, 16, 8, 0, 10],
                None,
                None
            ),
            1
        );
        assert_eq!(w.ceilings[0].speed, 2.0);
        assert_eq!(w.ceilings[0].speed_up, 1.0);
        // 120 mu down @ 2 ⇒ 60 tics; 120 mu up @ 1 ⇒ 120 tics.
        for _ in 0..60 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - (w.sectors[0].floor_z + 8.0)).abs() < 1e-6);
        assert_eq!(w.ceilings[0].direction, 1);
        for _ in 0..120 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - top).abs() < 1e-6);
        assert_eq!(w.ceilings[0].direction, -1);
    }

    #[test]
    fn ceiling_crush_and_raise_cycles() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(9);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let top = w.sectors[0].ceiling_z;
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_CRUSH_AND_RAISE,
                &[9, 8, 16, 10, 0],
                None,
                None
            ),
            1
        );
        // Lower toward floor+8 (120 mu @ hexen 16 → 2 mu/tic ⇒ 60 tics).
        for _ in 0..60 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - (w.sectors[0].floor_z + 8.0)).abs() < 1e-6);
        assert_eq!(w.ceilings[0].direction, 1);
        // Rise back to original top.
        for _ in 0..60 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - top).abs() < 1e-6);
        assert_eq!(w.ceilings[0].direction, -1);
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_CRUSH_STOP,
                &[9, 0, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(w.ceilings.is_empty());
    }

    #[test]
    fn light_strobe_toggles() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(4);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.sectors[0].light = 200;
        assert_eq!(
            execute_special(&mut w, num::LIGHT_STROBE_DOOM, &[4, 2, 3, 0, 0], None, None),
            1
        );
        assert_eq!(w.strobes.len(), 1);
        for _ in 0..3 {
            tick_strobes(&mut w);
        }
        assert_eq!(w.sectors[0].light, 0);
        for _ in 0..2 {
            tick_strobes(&mut w);
        }
        assert_eq!(w.sectors[0].light, 200);
    }

    #[test]
    fn floor_donut_lowers_pillar_and_raises_pool() {
        // Adjacent strip: outer (z=0) | pool (z=-24) | pillar (z=0, tagged).
        // Topology matches EV_DoDonut: pillar→pool→outer.
        let mut map = MapBuilder::new()
            .room("outer", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .room("pool", rect(64.0, 0.0, 128.0, 64.0), -24.0, 128.0)
            .room("pillar", rect(128.0, 0.0, 192.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let pool = map
            .sectors
            .iter()
            .position(|s| (s.floor_height + 24.0).abs() < 1e-6)
            .unwrap();
        let sector_min_x = |i: usize| {
            map.sectors[i]
                .lines
                .iter()
                .flat_map(|lid| {
                    let line = &map.lines[lid.raw() as usize];
                    [line.v1, line.v2]
                })
                .map(|v| map.vertices[v.raw() as usize].pos.x())
                .fold(f64::MAX, f64::min)
        };
        // Rightmost z=0 sector is the pillar.
        let pillar = (0..map.sectors.len())
            .filter(|&i| i != pool && map.sectors[i].floor_height.abs() < 1e-6)
            .max_by(|a, b| sector_min_x(*a).partial_cmp(&sector_min_x(*b)).unwrap())
            .unwrap();
        let outer = (0..map.sectors.len())
            .find(|&i| i != pillar && i != pool)
            .unwrap();
        map.sectors[pillar].tags.push(9);
        map.sectors[outer].floor_pic = "FLOOR0_6".into();
        map.sectors[pool].floor_pic = "NUKAGE1".into();
        map.sectors[pool].special = 7;
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::FLOOR_DONUT, &[9, 4, 4, 0, 0], None, None),
            1
        );
        assert_eq!(
            w.sector_floor_pics.get(&(pool as u32)).map(String::as_str),
            Some("FLOOR0_6")
        );
        assert_eq!(w.sector_specials.get(&(pool as u32)).copied(), Some(0));
        for _ in 0..128 {
            tick_floors(&mut w);
        }
        assert!(
            (w.sectors[pillar].floor_z - 0.0).abs() < 1e-6,
            "pillar should lower to outer floor, got {}",
            w.sectors[pillar].floor_z
        );
        assert!(
            (w.sectors[pool].floor_z - 0.0).abs() < 1e-6,
            "pool should rise to outer floor, got {}",
            w.sectors[pool].floor_z
        );
    }

    #[test]
    fn stairs_build_up_raises_tagged_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(11);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let z0 = w.sectors[0].floor_z;
        assert_eq!(
            execute_special(
                &mut w,
                num::STAIRS_BUILD_UP_DOOM,
                &[11, 8, 16, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..64 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - (z0 + 16.0)).abs() < 1e-6);
    }

    #[test]
    fn generic_stairs_builds_up_with_flag() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(12);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::GENERIC_STAIRS, &[12, 8, 16, 1, 0], None, None),
            1
        );
        for _ in 0..64 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - 16.0).abs() < 1e-6);
    }

    #[test]
    fn generic_stairs_ignore_tex_chains_different_flats() {
        let mut map = MapBuilder::new()
            .room("s0", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .room("s1", rect(64.0, 0.0, 128.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].floor_pic = "STEP1".into();
        map.sectors[1].floor_pic = "FLAT1".into();
        map.sectors[0].tags.push(13);
        let mut w = World::from_map(map).unwrap();
        // flags=3 → up + ignore texture
        assert_eq!(
            execute_special(&mut w, num::GENERIC_STAIRS, &[13, 8, 8, 3, 0], None, None),
            1
        );
        assert!(
            w.floors.len() >= 2,
            "expected ignore-tex chain, got {}",
            w.floors.len()
        );
    }

    #[test]
    fn stairs_build_up_chains_adjacent_same_flat() {
        let mut map = MapBuilder::new()
            .room("s0", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .room("s1", rect(64.0, 0.0, 128.0, 64.0), 0.0, 128.0)
            .room("s2", rect(128.0, 0.0, 192.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        for s in &mut map.sectors {
            s.floor_pic = "STEP1".into();
        }
        // Tag only the first stair tread.
        map.sectors[0].tags.push(7);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::STAIRS_BUILD_UP_DOOM,
                &[7, 8, 8, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(
            w.floors.len() >= 3,
            "expected chained stair movers, got {}",
            w.floors.len()
        );
        for _ in 0..128 {
            tick_floors(&mut w);
        }
        let mut heights: Vec<_> = w.sectors.iter().map(|s| s.floor_z).collect();
        heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((heights[0] - 8.0).abs() < 1e-6);
        assert!((heights[1] - 16.0).abs() < 1e-6);
        assert!((heights[2] - 24.0).abs() < 1e-6);
    }

    #[test]
    fn exit_plays_switch_sound() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let _ = w.drain_sound_events();
        assert_eq!(
            execute_special(&mut w, num::EXIT_NORMAL, &[0; 5], Some(mo), None),
            1
        );
        assert!(w.level.exiting);
        assert!(
            w.drain_sound_events()
                .iter()
                .any(|e| e.name == "switches/exitbutn"),
            "exit should play switches/exitbutn"
        );
    }

    #[test]
    fn exit_secret_sets_flag() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::EXIT_SECRET, &[0; 5], None, None),
            1
        );
        assert!(w.level.exiting);
        assert!(w.level.secret_exit);
    }

    #[test]
    fn transfer_floor_light_copies_control_light() {
        let mut map = MapBuilder::new()
            .room("ctrl", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .room("dark", rect(64.0, 0.0, 128.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].light = 200;
        map.sectors[1].light = 40;
        map.sectors[1].tags.push(3);
        // Control line: front in sector 0.
        let src = map
            .lines
            .iter()
            .enumerate()
            .find(|(_, l)| l.front_sector == Some(SectorId::from_raw(0)))
            .map(|(i, _)| i)
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::TRANSFER_FLOOR_LIGHT,
                &[3, 0, 0, 0, 0],
                None,
                Some(LineId::from_raw(src as u32)),
            ),
            1
        );
        apply_transfers(&mut w);
        assert_eq!(w.sectors[1].light, 200);
    }

    #[test]
    fn transfer_heights_sets_fake_planes() {
        let mut map = MapBuilder::new()
            .room("ctrl", rect(0.0, 0.0, 64.0, 64.0), 24.0, 96.0)
            .room("pool", rect(64.0, 0.0, 128.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[1].tags.push(4);
        let src = map
            .lines
            .iter()
            .enumerate()
            .find(|(_, l)| l.front_sector == Some(SectorId::from_raw(0)))
            .map(|(i, _)| i)
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::TRANSFER_HEIGHTS,
                &[4, 0, 0, 0, 0],
                None,
                Some(LineId::from_raw(src as u32)),
            ),
            1
        );
        apply_transfers(&mut w);
        assert_eq!(w.sectors[1].fake_floor_z, Some(24.0));
        assert_eq!(w.sectors[1].fake_ceiling_z, Some(96.0));
        assert_eq!(w.sectors[1].floor_z, 0.0);
    }

    #[test]
    fn scroll_floor_accumulates_uv_offsets() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(9);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::SCROLL_FLOOR, &[9, 0, 32, 0, 0], None, None),
            1
        );
        for _ in 0..5 {
            tick_scrollers(&mut w);
        }
        let (u, v) = w.floor_scroll_offsets[&0];
        assert!((u - 5.0).abs() < 1e-6, "u={u}");
        assert!(v.abs() < 1e-6);
    }

    #[test]
    fn scroll_ceiling_accumulates_offsets() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(8);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::SCROLL_CEILING, &[8, 0, 32, 16, 0], None, None),
            1
        );
        for _ in 0..4 {
            tick_ceiling_scrollers(&mut w);
        }
        let (u, v) = w.ceiling_scroll_offsets[&0];
        assert!((u - 4.0).abs() < 1e-6, "u={u}");
        assert!((v - 2.0).abs() < 1e-6, "v={v}");
    }

    #[test]
    fn scroll_floor_carries_player() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(5);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let x0 = w.actors.get(mo).unwrap().pos.x();
        assert_eq!(
            execute_special(
                &mut w,
                num::SCROLL_FLOOR,
                &[5, 0, 32, 0, 0], // 32/32 = 1 unit/tic in X
                None,
                None
            ),
            1
        );
        for _ in 0..10 {
            tick_scrollers(&mut w);
        }
        let x1 = w.actors.get(mo).unwrap().pos.x();
        assert!(x1 > x0 + 5.0, "x0={x0} x1={x1}");
    }

    #[test]
    fn floor_lower_by_value_drops() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 64.0, 192.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(3);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_LOWER_BY_VALUE,
                &[3, 8, 24, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..64 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - 40.0).abs() < 1e-6);
    }

    #[test]
    fn generic_floor_lowers_to_lowest_and_changes_tex() {
        let mut map = MapBuilder::new()
            .room("high", rect(0.0, 0.0, 128.0, 128.0), 64.0, 192.0)
            .room("low", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let high = map
            .sectors
            .iter()
            .position(|s| (s.floor_height - 64.0).abs() < 1e-6)
            .unwrap();
        let low = map
            .sectors
            .iter()
            .position(|s| (s.floor_height - 0.0).abs() < 1e-6)
            .unwrap();
        map.sectors[high].tags.push(37);
        map.sectors[high].special = 5;
        map.sectors[low].floor_pic = "NUKAGE1".into();
        let line = map
            .lines
            .iter()
            .position(|l| {
                matches!(
                    (l.front_sector.map(|s| s.raw() as usize), l.back_sector.map(|s| s.raw() as usize)),
                    (Some(f), Some(b)) if (f == high && b == low) || (f == low && b == high)
                )
            })
            .unwrap();
        if map.lines[line].front_sector.map(|s| s.raw() as usize) != Some(low) {
            map.lines[line].front_sector = Some(SectorId::from_raw(low as u32));
            map.lines[line].back_sector = Some(SectorId::from_raw(high as u32));
        }
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::GENERIC_FLOOR,
                &[37, 8, 0, 2, 3],
                None,
                Some(LineId::from_raw(line as u32)),
            ),
            1
        );
        assert_eq!(
            w.sector_floor_pics.get(&(high as u32)).map(String::as_str),
            Some("NUKAGE1")
        );
        assert_eq!(w.sector_specials.get(&(high as u32)).copied(), Some(0));
        for _ in 0..128 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[high].floor_z - 0.0).abs() < 1e-6);
    }

    #[test]
    fn damage_sector_plane_reduces_healthfloor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].health_floor = 50;
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.sectors[0].health_floor, 50);
        let left = damage_sector_plane(&mut w, SectorId::from_raw(0), 0, 20);
        assert_eq!(left, 30);
        assert_eq!(w.sectors[0].health_floor, 30);
        assert_eq!(w.map.sectors[0].health_floor, 30);
        let left = damage_sector_plane(&mut w, SectorId::from_raw(0), 0, 40);
        assert_eq!(left, 0);
        assert_eq!(w.sectors[0].health_floor, 0);
    }

    #[test]
    fn damage_linedef_triggers_damage_and_death_specials() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        if let Some(l) = map.lines.first_mut() {
            l.tag = 22;
            l.health = 40;
            l.activation |= room_map::spac::DAMAGE | room_map::spac::DEATH;
            // Door_Open — harmless for the test; success clears nothing for Damage/Death.
            l.special = num::DOOR_OPEN as i16;
            l.args = [22, 16, 0, 0, 0];
        }
        let mut w = World::from_map(map).unwrap();
        let lid = LineId::from_raw(0);
        assert_eq!(w.line_health.get(&0), Some(&40));
        let left = damage_linedef(&mut w, lid, None, 15);
        assert_eq!(left, 25);
        assert_eq!(w.line_health.get(&0), Some(&25));
        let left = damage_linedef(&mut w, lid, None, 25);
        assert_eq!(left, 0);
        assert_eq!(w.line_health.get(&0), Some(&0));
    }

    #[test]
    fn damage_linedef_destroys_healthgroup_peers() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        assert!(map.lines.len() >= 2);
        for i in 0..2 {
            let l = &mut map.lines[i];
            l.health = 10;
            l.health_group = 7;
            l.activation |= room_map::spac::DEATH;
            l.special = num::DOOR_OPEN as i16;
            l.args = [22, 16, 0, 0, 0];
        }
        map.sectors[0].tags.push(22);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(damage_linedef(&mut w, LineId::from_raw(0), None, 10), 0);
        assert_eq!(w.line_health.get(&0), Some(&0));
        assert_eq!(w.line_health.get(&1), Some(&0));
        assert_eq!(w.map.lines[1].health, 0);
    }

    #[test]
    fn dead_health_line_stops_blocking_and_hides_mid() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let lid = map
            .lines
            .iter()
            .position(|l| l.front_sector.is_some() && l.back_sector.is_some())
            .expect("shared wall");
        {
            let l = &mut map.lines[lid];
            l.health = 1;
            l.flags |= crate::world::ML_BLOCKING as i16;
            for sid in l.sides.into_iter().flatten() {
                map.sides[sid.raw() as usize].middle = "GLASS".into();
            }
        }
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let actor = w.actors.get(mo).unwrap().clone();
        let blocked =
            crate::physics::check_position(&w, &actor, DVec3::new(128.0, 64.0, 0.0), Some(mo));
        assert!(!blocked.ok, "health wall should block");
        assert_eq!(
            damage_linedef(&mut w, LineId::from_raw(lid as u32), None, 1),
            0
        );
        let flags = w.line_flags(LineId::from_raw(lid as u32));
        assert_eq!(flags & crate::world::ML_BLOCKING, 0);
        let actor = w.actors.get(mo).unwrap().clone();
        let open =
            crate::physics::check_position(&w, &actor, DVec3::new(128.0, 64.0, 0.0), Some(mo));
        assert!(open.ok, "dead health wall should be walk-through");
        let line = &w.map.lines[lid];
        for sid in line.sides.into_iter().flatten() {
            let mid = &w.map.sides[sid.raw() as usize].middle;
            assert!(mid.is_empty() || mid == "-", "{mid}");
        }
    }

    #[test]
    fn activate_line_respects_spac_mask() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(1);
        map.lines[0].special = num::DOOR_RAISE as i16;
        map.lines[0].args = [1, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        map.lines[0].flags |= crate::world::ML_REPEAT_SPECIAL as i16;
        let mut w = World::from_map(map).unwrap();
        let lid = LineId::from_raw(0);
        assert!(!activate_line(&mut w, lid, None, ActivationType::Cross));
        assert!(activate_line(&mut w, lid, None, ActivationType::Use));
        assert!(!w.doors.is_empty());
        assert_eq!(w.line_special(lid), num::DOOR_RAISE as i16);
    }

    #[test]
    fn monster_use_rejects_teleport_allows_door() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = room_decorate::parse_decorate(src).unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&file).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.lines[0].special = num::TELEPORT as i16;
        map.lines[0].args = [0, 1, 0, 0, 0];
        map.lines[0].activation = 0;
        map.lines[1].special = num::DOOR_RAISE as i16;
        map.lines[1].args = [1, 16, 150, 0, 0];
        map.lines[1].activation = 0;
        map.sectors[0].tags.push(1);
        let mut w =
            World::from_map_with_classes(map, std::sync::Arc::clone(&classes), states).unwrap();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let mon = w
            .spawn_at(zombie, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        assert!(
            !activate_line(&mut w, LineId::from_raw(0), Some(mon), ActivationType::Use),
            "monsters must not Use-activate teleports"
        );
        assert!(activate_line(
            &mut w,
            LineId::from_raw(1),
            Some(mon),
            ActivationType::Use
        ));
        assert!(!w.doors.is_empty());
    }

    #[test]
    fn door_opens_to_lowest_neighbour_ceiling() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 0.0)
            .room("c", rect(256.0, 0.0, 384.0, 128.0), 0.0, 200.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[1].tags.push(5);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::DOOR_OPEN, &[5, 16, 0, 0, 0], None, None),
            1
        );
        assert_eq!(w.doors[0].top_z, 68.0);
    }

    #[test]
    fn door_raise_retrigger_reverses_while_open() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 8.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None),
            1
        );
        // Raise to top and enter wait.
        for _ in 0..80 {
            tick_doors(&mut w);
        }
        assert_eq!(w.doors.len(), 1);
        assert_eq!(w.doors[0].direction, 0);
        // Re-trigger while waiting → start closing (vanilla manual door).
        assert_eq!(
            execute_special(&mut w, num::DOOR_RAISE, &[1, 16, 150, 0, 0], None, None),
            1
        );
        assert_eq!(w.doors.len(), 1);
        assert_eq!(w.doors[0].direction, -1);
    }

    #[test]
    fn doom_dr_door_keeps_special_after_use() {
        // Doom linedef type 1 (DR Door_Raise) → XLAT USE|MONST|REP; special must survive.
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 8.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let xlat = {
            let raw = room_map::raw::RawLinedef {
                v1: 0,
                v2: 1,
                flags: 4,
                special: 1,
                tag: 0,
                side_front: 0,
                side_back: 0xffff,
                args: [0; 5],
                hexen: false,
                activation: 0,
                arg0_str: String::new(),
                midtex3d: false,
                midtex3d_impassible: false,
                check_switch_range: false,
                first_side_only: false,
                block_floaters: false,
                lock_number: 0,
                block_projectiles: false,
                block_sight: false,
                block_hitscan: false,
                extra_ids: Vec::new(),
                alpha: 1.0,
                mid_additive: false,
                health: 0,
                health_group: 0,
                automap_style: 0,
                clip_midtex: false,
                wrap_midtex: false,
            };
            room_map::xlat::translate_doom_linedef(&raw)
        };
        assert_ne!(xlat.flags & crate::world::ML_REPEAT_SPECIAL as i16, 0);
        {
            let line = &mut map.lines[0];
            line.special = xlat.special;
            line.args = xlat.args;
            line.flags = xlat.flags;
            line.activation = xlat.activation;
            if line.back_sector.is_none() {
                line.back_sector = Some(room_core::SectorId::from_raw(0));
            }
        }
        let mut w = World::from_map(map).unwrap();
        let lid = LineId::from_raw(0);
        assert!(activate_line(&mut w, lid, None, ActivationType::Use));
        assert_ne!(
            w.line_special(lid),
            0,
            "DR door must keep its special (REP)"
        );
        assert!(
            activate_line(&mut w, lid, None, ActivationType::Use),
            "DR door must be usable again while the thinker is active"
        );
    }

    #[test]
    fn activate_line_clears_one_shot_special() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(1);
        map.lines[0].special = num::DOOR_RAISE as i16;
        map.lines[0].args = [1, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        map.lines[0].flags &= !(crate::world::ML_REPEAT_SPECIAL as i16);
        let mut w = World::from_map(map).unwrap();
        let lid = LineId::from_raw(0);
        assert!(activate_line(&mut w, lid, None, ActivationType::Use));
        assert_eq!(w.line_special(lid), 0);
        assert!(!activate_line(&mut w, lid, None, ActivationType::Use));
    }

    #[test]
    fn firstsideonly_rejects_back_side_use() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(1);
        // Pick a line with a known direction and place the player on its back.
        let lid = LineId::from_raw(0);
        {
            let line = &mut map.lines[0];
            line.special = num::DOOR_RAISE as i16;
            line.args = [1, 16, 150, 0, 0];
            line.activation = room_map::spac::USE;
            line.flags |= crate::world::ML_REPEAT_SPECIAL as i16;
            line.first_side_only = true;
        }
        let v1 = map.vertices[map.lines[0].v1.raw() as usize].pos;
        let delta = map.lines[0].delta;
        // Point on the left of v1→v2 is the back side (cross > 0).
        let back = DVec3::new(v1.x() - delta.y() * 0.25, v1.y() + delta.x() * 0.25, 0.0);
        let front = DVec3::new(v1.x() + delta.y() * 0.25, v1.y() - delta.x() * 0.25, 0.0);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = back;
        }
        assert!(!activate_line(&mut w, lid, Some(mo), ActivationType::Use));
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = front;
        }
        assert!(activate_line(&mut w, lid, Some(mo), ActivationType::Use));
    }

    #[test]
    fn checkswitchrange_rejects_unreachable_eye() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(1);
        map.lines[0].special = num::DOOR_RAISE as i16;
        map.lines[0].args = [1, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        map.lines[0].flags |= crate::world::ML_REPEAT_SPECIAL as i16;
        map.lines[0].check_switch_range = true;
        let mut w = World::from_map(map).unwrap();
        let lid = LineId::from_raw(0);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(32.0, 32.0, -200.0);
            a.view_height = 41.0;
        }
        assert!(!activate_line(&mut w, lid, Some(mo), ActivationType::Use));
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(32.0, 32.0, 0.0);
        }
        assert!(activate_line(&mut w, lid, Some(mo), ActivationType::Use));
    }

    #[test]
    fn generic_floor_by_texture_and_numeric_model() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 64.0, 192.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let low = map
            .sectors
            .iter()
            .position(|s| (s.floor_height - 0.0).abs() < 1e-6)
            .unwrap();
        let high = map
            .sectors
            .iter()
            .position(|s| (s.floor_height - 64.0).abs() < 1e-6)
            .unwrap();
        map.sectors[low].tags.push(50);
        map.sectors[high].floor_pic = "FLAT20".into();
        map.sectors[high].special = 9;
        // Shared wall: lower texture on low side so shortest gap = 64.
        let line = map
            .lines
            .iter()
            .position(|l| {
                matches!(
                    (l.front_sector.map(|s| s.raw() as usize), l.back_sector.map(|s| s.raw() as usize)),
                    (Some(f), Some(b)) if (f == low && b == high) || (f == high && b == low)
                )
            })
            .unwrap();
        if let Some(sid) = map.lines[line].sides[0] {
            if map.lines[line].front_sector.map(|s| s.raw() as usize) == Some(low) {
                map.sides[sid.raw() as usize].lower = "DOORTRAK".into();
            }
        }
        if let Some(sid) = map.lines[line].sides[1] {
            if map.lines[line].back_sector.map(|s| s.raw() as usize) == Some(low) {
                map.sides[sid.raw() as usize].lower = "DOORTRAK".into();
            }
        }
        let mut w = World::from_map(map).unwrap();
        // Target 6 raise by texture; flags: change tex (2) + numeric model (4) + raise (8).
        assert_eq!(
            execute_special(
                &mut w,
                num::GENERIC_FLOOR,
                &[50, 8, 0, 6, 2 | 4 | 8],
                None,
                Some(LineId::from_raw(line as u32)),
            ),
            1
        );
        assert_eq!(
            w.sector_floor_pics.get(&(low as u32)).map(String::as_str),
            Some("FLAT20")
        );
        for _ in 0..128 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[low].floor_z - 64.0).abs() < 1e-6);
    }

    #[test]
    fn generic_floor_absolute_height_raise_24() {
        // Boom gen 0x0300 → Generic_Floor(tag, speed, 24, 0, flags|8).
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(8);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::GENERIC_FLOOR,
                &[8, 8, 24, 0, 8], // raise by 24
                None,
                None,
            ),
            1
        );
        for _ in 0..64 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - 24.0).abs() < 1e-6);
    }

    #[test]
    fn generic_ceiling_raises_to_highest_neighbor() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 72.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 0.0, 192.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let low = map
            .sectors
            .iter()
            .position(|s| (s.ceiling_height - 72.0).abs() < 1e-6)
            .unwrap();
        map.sectors[low].tags.push(40);
        let mut w = World::from_map(map).unwrap();
        // Doom type 40 → Generic_Ceiling(tag, C_SLOW, 0, 1, 8).
        assert_eq!(
            execute_special(&mut w, num::GENERIC_CEILING, &[40, 8, 0, 1, 8], None, None),
            1
        );
        for _ in 0..256 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[low].ceiling_z - 192.0).abs() < 1e-6);
    }

    #[test]
    fn plat_raise_and_stay_tx0_copies_floor_and_raises() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 64.0, 192.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        // Tag the low sector; front of the shared wall faces high (trigger model uses front).
        let low = map
            .sectors
            .iter()
            .position(|s| (s.floor_height - 0.0).abs() < 1e-6)
            .unwrap();
        let high = map
            .sectors
            .iter()
            .position(|s| (s.floor_height - 64.0).abs() < 1e-6)
            .unwrap();
        map.sectors[low].tags.push(3);
        map.sectors[low].special = 9;
        map.sectors[high].floor_pic = "FLAT14".into();
        let line = map
            .lines
            .iter()
            .position(|l| {
                matches!(
                    (l.front_sector.map(|s| s.raw() as usize), l.back_sector.map(|s| s.raw() as usize)),
                    (Some(f), Some(b)) if (f == high && b == low) || (f == low && b == high)
                )
            })
            .unwrap();
        // Prefer front = high so Tx0 copies FLAT14.
        if map.lines[line].front_sector.map(|s| s.raw() as usize) != Some(high) {
            map.lines[line].front_sector = Some(SectorId::from_raw(high as u32));
            map.lines[line].back_sector = Some(SectorId::from_raw(low as u32));
        }
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_RAISE_AND_STAY_TX0,
                &[3, 8, 0, 0, 0],
                None,
                Some(LineId::from_raw(line as u32)),
            ),
            1
        );
        assert_eq!(
            w.sector_floor_pics.get(&(low as u32)).map(String::as_str),
            Some("FLAT14")
        );
        assert_eq!(w.sector_specials.get(&(low as u32)).copied(), Some(0));
        for _ in 0..128 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[low].floor_z - 64.0).abs() < 1e-6);
    }

    #[test]
    fn plat_up_by_value_stay_tx_raises_by_units() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(5);
        map.sectors[0].floor_pic = "FLOOR0_1".into();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        // Use line 0 front sector as texture source (same sector).
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_UP_BY_VALUE_STAY_TX,
                &[5, 8, 3, 0, 0],
                None,
                Some(LineId::from_raw(0)),
            ),
            1
        );
        for _ in 0..64 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - 24.0).abs() < 1e-6);
        assert_eq!(w.sector_specials.get(&0).copied(), Some(0));
    }

    #[test]
    fn udmf_xscroll_yscroll_seeds_tex_scroller() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let side = map.lines[0].sides[0].unwrap();
        map.sides[side.raw() as usize].scroll_x = 2.0;
        map.sides[side.raw() as usize].scroll_y = -1.0;
        let mut w = World::from_map(map).unwrap();
        let sc = w
            .tex_scrollers
            .iter()
            .find(|s| s.side == side)
            .expect("udmf scroller");
        assert!((sc.dx - 2.0).abs() < 1e-6);
        assert!((sc.dy - (-1.0)).abs() < 1e-6);
        tick_tex_scrollers(&mut w);
        assert_eq!(w.side_offsets.get(&side.raw()), Some(&(2, -1)));
    }

    #[test]
    fn scroll_texture_right_updates_side_offset() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let line = LineId::from_raw(0);
        let side = map.lines[0].sides[0].unwrap();
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::SCROLL_TEXTURE_RIGHT,
                &[2, 0, 0, 0, 0],
                None,
                Some(line)
            ),
            1
        );
        assert_eq!(w.tex_scrollers.len(), 1);
        tick_tex_scrollers(&mut w);
        tick_tex_scrollers(&mut w);
        let (x, _) = w.side_offsets.get(&side.raw()).copied().unwrap();
        assert_eq!(x, 4);
    }

    #[test]
    fn scroll_texture_down_updates_y_offset() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let line = LineId::from_raw(0);
        let side = map.lines[0].sides[0].unwrap();
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::SCROLL_TEXTURE_DOWN,
                &[3, 0, 0, 0, 0],
                None,
                Some(line)
            ),
            1
        );
        tick_tex_scrollers(&mut w);
        let (_, y) = w.side_offsets.get(&side.raw()).copied().unwrap();
        assert_eq!(y, 3);
    }

    #[test]
    fn init_line_scrollers_spawns_from_map() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.lines[0].special = num::SCROLL_TEXTURE_LEFT as i16;
        map.lines[0].args = [4, 0, 0, 0, 0];
        let w = World::from_map(map).unwrap();
        assert_eq!(w.tex_scrollers.len(), 1);
        assert!(w.tex_scrollers[0].dx < 0.0);
    }

    #[test]
    fn horizon_lightning_plane_copy_and_line_id() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 64.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 16.0, 96.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(28);
        map.sectors[0].light = 100;
        map.lines[0].tag = 28;
        apply_nodes(&mut map);
        let lid = LineId::from_raw(0);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::LINE_HORIZON, &[0; 5], None, Some(lid)),
            1
        );
        assert_ne!(w.line_flags[&0] & crate::world::ML_HORIZON, 0);
        assert_eq!(
            execute_special(
                &mut w,
                num::LINE_SET_IDENTIFICATION,
                &[99, 0, 0, 0, 0],
                None,
                Some(lid)
            ),
            1
        );
        assert_eq!(w.line_ids[&0], 99);
        assert_eq!(
            execute_special(
                &mut w,
                num::LIGHT_FORCE_LIGHTNING,
                &[0, 0, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.sectors[0].light, 255);
        assert!(!w.fades.is_empty());
        assert_eq!(
            execute_special(&mut w, num::PLANE_COPY, &[28, 0, 0, 0, 0], None, Some(lid)),
            1
        );
        // Front sector of line 0 copied onto tagged sector(s).
        assert!(
            w.sectors
                .iter()
                .any(|s| (s.floor_z - w.sectors[0].floor_z).abs() < 1e-6)
        );
        assert_eq!(
            execute_special(
                &mut w,
                num::TRANSFER_WALL_LIGHT,
                &[28, 0, 0, 0, 0],
                None,
                Some(lid)
            ),
            1
        );
        assert!(!w.side_lights.is_empty());
    }

    #[test]
    fn by_texture_tx_ty_and_move_crush() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 32.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let low = map
            .sectors
            .iter()
            .position(|s| s.floor_height.abs() < 1e-6)
            .unwrap();
        map.sectors[low].tags.push(27);
        // Give the shared wall a lower texture so ByTexture can measure the gap.
        for ld in &mut map.lines {
            if ld.front_sector.is_some() && ld.back_sector.is_some() {
                if let Some(sid) = ld.sides[0] {
                    map.sides[sid.raw() as usize].lower = "STEP1".into();
                }
            }
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_LOWER_BY_TEXTURE,
                &[27, 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.floors[0].dest_z + 32.0).abs() < 1e-6 || w.floors[0].dest_z < 0.0);
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_MOVE_TO_VALUE_AND_CRUSH,
                &[27, 16, 8, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(
            w.floors
                .iter()
                .any(|f| f.crush && (f.dest_z - 8.0).abs() < 1e-6)
        );
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_BY_VALUE_TX_TY,
                &[27, 16, 16, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(
            w.sector_specials.get(&(low as u32)).copied().unwrap_or(1) == 0
                || w.sector_floor_pics.contains_key(&(low as u32))
        );
    }

    #[test]
    fn ee_plane_movers_and_door_animated() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 16.0, 96.0)
            .room("b", rect(128.0, 0.0, 256.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let a = map
            .sectors
            .iter()
            .position(|s| (s.floor_height - 16.0).abs() < 1e-6)
            .unwrap();
        map.sectors[a].tags.push(26);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_TO_CEILING,
                &[26, 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.floors[0].dest_z - 96.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_RAISE_TO_HIGHEST,
                &[26, 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.ceilings[0].top_z - 128.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_TO_CEILING_INSTANT,
                &[26, 0, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.sectors[a].floor_z - w.sectors[a].ceiling_z).abs() < 1e-6);
        assert_eq!(
            execute_special(&mut w, num::DOOR_ANIMATED, &[26, 16, 35, 0, 0], None, None),
            1
        );
        assert!(!w.doors.is_empty());
    }

    #[test]
    fn floor_raise_to_highest_and_crush_a() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 48.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let low = map
            .sectors
            .iter()
            .position(|s| s.floor_height.abs() < 1e-6)
            .unwrap();
        map.sectors[low].tags.push(25);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_TO_HIGHEST,
                &[25, 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.floors[0].dest_z - 48.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_CRUSH_AND_RAISE_A,
                &[25, 16, 8, 10, 0],
                None,
                None
            ),
            1
        );
        assert!(!w.ceilings[0].stop_at_high);
        assert!(w.ceilings[0].speed_up > 0.0);
        assert!(
            w.ceilings[0].speed_up < w.ceilings[0].speed
                || (w.ceilings[0].speed - w.ceilings[0].speed_up).abs() < 1e-9
        );
        assert_eq!(
            execute_special(
                &mut w,
                num::STAIRS_BUILD_UP_SYNC,
                &[25, 2, 8, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(w.floors.iter().any(|f| (f.speed - 2.0).abs() < 1e-9));
    }

    #[test]
    fn waggle_teleport_in_sector_and_crush_stay() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9901
{
  Health 10
  States { Spawn: POSS A -1 Stop }
}
actor Spot : Actor 9001
{
  +NOBLOCKMAP
  States { Spawn: TNT1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(24);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let z0 = w.sectors[0].floor_z;
        assert_eq!(
            execute_special(&mut w, num::FLOOR_WAGGLE, &[24, 32, 64, 0, 2,], None, None),
            1
        );
        // time=2 → 8 tics; advance past delay-less start.
        for _ in 0..4 {
            tick_waggles(&mut w);
        }
        assert!((w.sectors[0].floor_z - z0).abs() > 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_CRUSH_RAISE_AND_STAY_A,
                &[24, 16, 8, 10, 0],
                None,
                None
            ),
            1
        );
        assert!(w.ceilings[0].stop_at_high);
        let dummy = w
            .spawn_at(
                classes.find_name("Dummy").unwrap(),
                DVec3::new(40.0, 40.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        let src = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(0.0, 0.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(src).unwrap().tid = 60;
        let dst = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(16.0, 8.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(dst).unwrap().tid = 61;
        assert_eq!(
            execute_special(
                &mut w,
                num::TELEPORT_IN_SECTOR,
                &[24, 60, 61, 0, 0],
                None,
                None
            ),
            1
        );
        let a = w.actors.get(dummy).unwrap();
        assert!((a.pos.x() - 56.0).abs() < 1e-6);
        assert!((a.pos.y() - 48.0).abs() < 1e-6);
    }

    #[test]
    fn teleport_other_group_and_plat_lip() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9901
{
  Health 10
  States { Spawn: POSS A -1 Stop }
}
actor Spot : Actor 9001
{
  +NOBLOCKMAP
  States { Spawn: TNT1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(23);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_DOWN_WAIT_UP_STAY_LIP,
                &[23, 16, 2, 16, 0],
                None,
                None
            ),
            1
        );
        assert!(
            (w.plats[0].low - -16.0).abs() < 1e-6
                || (w.plats[0].high - w.plats[0].low - 16.0).abs() < 1e-6
        );
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_RAISE_BY_VALUE_TIMES8,
                &[23, 16, 2, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.ceilings[0].top_z - 144.0).abs() < 1e-6);
        let dummy = w
            .spawn_at(
                classes.find_name("Dummy").unwrap(),
                DVec3::new(40.0, 40.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(dummy).unwrap().tid = 50;
        let spot = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(180.0, 180.0, 0.0),
                Angle::from_degrees(90.0),
            )
            .unwrap();
        w.actors.get_mut(spot).unwrap().tid = 51;
        assert_eq!(
            execute_special(&mut w, num::TELEPORT_OTHER, &[50, 51, 0, 0, 0], None, None),
            1
        );
        let a = w.actors.get(dummy).unwrap();
        assert!((a.pos.x() - 180.0).abs() < 1e-6);
        assert!((a.pos.y() - 180.0).abs() < 1e-6);
        let g1 = w
            .spawn_at(
                classes.find_name("Dummy").unwrap(),
                DVec3::new(60.0, 60.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(g1).unwrap().tid = 52;
        let src = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(0.0, 0.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(src).unwrap().tid = 53;
        let dst = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(20.0, 10.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(dst).unwrap().tid = 54;
        assert_eq!(
            execute_special(&mut w, num::TELEPORT_GROUP, &[52, 53, 54, 0, 0], None, None),
            1
        );
        let g = w.actors.get(g1).unwrap();
        assert!((g.pos.x() - 80.0).abs() < 1e-6);
        assert!((g.pos.y() - 70.0).abs() < 1e-6);
    }

    #[test]
    fn floor_ceiling_move_to_value_and_scroll_both() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(22);
        map.lines[0].tag = 22;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_MOVE_TO_VALUE,
                &[22, 16, 40, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.floors[0].dest_z - 40.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_MOVE_TO_VALUE_TIMES8,
                &[22, 16, 10, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(
            (w.ceilings[0].top_z - 80.0).abs() < 1e-6
                || (w.ceilings[0].bottom_z - 80.0).abs() < 1e-6
        );
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_LOWER_INSTANT,
                &[22, 0, 4, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.sectors[0].ceiling_z - 96.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::SCROLL_TEXTURE_BOTH,
                &[22, 64, 0, 0, 64],
                None,
                None
            ),
            1
        );
        assert_eq!(w.tex_scrollers.len(), 1);
        assert!((w.tex_scrollers[0].dx - 1.0).abs() < 1e-6);
        assert!((w.tex_scrollers[0].dy - 1.0).abs() < 1e-6);
    }

    #[test]
    fn floor_lower_raise_instant() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(21);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_INSTANT,
                &[21, 0, 3, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.sectors[0].floor_z - 24.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_LOWER_INSTANT,
                &[21, 0, 2, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.sectors[0].floor_z - 8.0).abs() < 1e-6);
    }

    #[test]
    fn plat_up_by_value_and_teleport_no_stop() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 32.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let low = map
            .sectors
            .iter()
            .position(|s| s.floor_height.abs() < 1e-6)
            .unwrap();
        map.sectors[low].tags.push(19);
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 200,
            y: 64,
            z: 0,
            angle: 0,
            type_id: 14,
            flags: 0,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: -2147483648,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: -32768,
            roll: -32768,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
            count_secret: false,
            no_count: false,
            arg0_str: String::new(),
            conversation: 0,
            accuracy: i32::MIN,
            stamina: i32::MIN,
            species: String::new(),
            melee_range: -1.0,
            view_height: -1.0,
            mass: i32::MIN,
            speed: -1.0,
            friction: -1.0,
            damage_factor: -1.0,
            max_step_height: -1.0,
            max_dropoff_height: -1.0,
            float_speed: -1.0,
            fast_speed: -1.0,
            jump_z: -1.0,
            attack_z_offset: -1.0,
        });
        apply_nodes(&mut map);
        if let Some(sid) = room_nodebuild::point_in_sector(&map, 200.0, 64.0) {
            map.sectors[sid.raw() as usize].tags.push(20);
        }
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_UP_BY_VALUE,
                &[19, 16, 2, 2, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.plats[0].status, 1);
        assert!((w.plats[0].high - 16.0).abs() < 1e-6);
        assert!(w.plats[0].stop_at_low);
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_DOWN_BY_VALUE,
                &[19, 16, 2, 1, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.plats[1].status, -1);
        assert!((w.plats[1].low - -8.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_UP_WAIT_DOWN_STAY,
                &[19, 16, 2, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.plats[2].high - 32.0).abs() < 1e-6);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.vel = DVec3::new(8.0, 0.0, 0.0);
        }
        assert_eq!(
            execute_special(
                &mut w,
                num::TELEPORT_NO_STOP,
                &[0, 20, 1, 0, 0],
                Some(mo),
                None
            ),
            1
        );
        let a = w.actors.get(mo).unwrap();
        assert!((a.pos.x() - 200.0).abs() < 1e-6);
        assert!((a.vel.x() - 8.0).abs() < 1e-6);
        // Keep-velocity path must not apply the usual 18-tic stall.
        assert_ne!(a.reaction_time, 18);
    }

    #[test]
    fn plat_up_nearest_and_automap_flags() {
        let mut map = MapBuilder::new()
            .room("low", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .room("high", rect(128.0, 0.0, 256.0, 128.0), 32.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let low = map
            .sectors
            .iter()
            .position(|s| s.floor_height.abs() < 1e-6)
            .unwrap();
        map.sectors[low].tags.push(18);
        map.lines[0].tag = 18;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::PLAT_UP_NEAREST_WAIT_DOWN_STAY,
                &[18, 16, 10, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.plats[0].status, 1);
        assert!((w.plats[0].high - 32.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::LINE_SET_AUTOMAP_FLAGS,
                &[18, 0b0101, 0, 0, 0],
                None,
                None
            ),
            1
        );
        let f = w.line_flags[&0];
        assert_ne!(f & crate::world::ML_SECRET, 0);
        assert_ne!(f & crate::world::ML_MAPPED, 0);
        assert_eq!(
            execute_special(
                &mut w,
                num::LINE_SET_AUTOMAP_STYLE,
                &[18, 3, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.line_automap_style[&0], 3);
    }

    #[test]
    fn floor_ceiling_stop_and_crush_dist() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(17);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_CRUSH_AND_RAISE_DIST,
                &[17, 16, 16, 10, 0],
                None,
                None
            ),
            1
        );
        assert!((w.ceilings[0].bottom_z - (w.sectors[0].floor_z + 16.0)).abs() < 1e-6);
        assert_eq!(
            execute_special(&mut w, num::CEILING_STOP, &[17, 0, 0, 0, 0], None, None),
            1
        );
        assert!(w.ceilings.is_empty());
        w.floors.push(FloorMover {
            sector: room_core::SectorId::from_raw(0),
            direction: 1,
            speed: 1.0,
            dest_z: 64.0,
            crush: false,
        });
        assert_eq!(
            execute_special(&mut w, num::FLOOR_STOP, &[17, 0, 0, 0, 0], None, None),
            1
        );
        assert!(w.floors.is_empty());
    }

    #[test]
    fn line_sector_health_and_glow() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.lines[0].tag = 16;
        map.sectors[0].tags.push(16);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::LINE_SET_HEALTH, &[16, 50, 0, 0, 0], None, None),
            1
        );
        assert_eq!(w.line_health[&0], 50);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_HEALTH,
                &[16, 0, 80, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.sectors[0].health_floor, 80);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_FLOOR_GLOW,
                &[16, 32, 255, 128, 0],
                None,
                None
            ),
            1
        );
        let g = w.sectors[0].glow_floor.unwrap();
        assert_eq!(g.color, [255, 128, 0]);
        assert!((g.height - 32.0).abs() < 1e-5);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_CEILING_GLOW,
                &[16, -1, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(w.sectors[0].glow_ceiling.is_none());
    }

    #[test]
    fn projectile_aimed_and_plane_reflection() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Bolt : Actor 9902
{
  +MISSILE
  +NOGRAVITY
  States { Spawn: BAL1 A -1 Stop }
}
actor Spot : Actor 9001
{
  +NOBLOCKMAP
  States { Spawn: TNT1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(15);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let spot = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(64.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(spot).unwrap().tid = 40;
        let tgt = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(164.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(tgt).unwrap().tid = 41;
        assert_eq!(
            execute_special(
                &mut w,
                num::THING_PROJECTILE_AIMED,
                &[40, 9902, 10, 41, 42],
                None,
                None
            ),
            1
        );
        let bolt = w.find_by_tid(42).unwrap();
        assert!(w.actors.get(bolt).unwrap().vel.x() > 9.0);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_PLANE_REFLECTION,
                &[15, 128, 64, 0, 0],
                None,
                None
            ),
            1
        );
        let (fr, cr) = w.plane_reflection[&0];
        assert!((fr - 128.0 / 255.0).abs() < 1e-5);
        assert!((cr - 64.0 / 255.0).abs() < 1e-5);
    }

    #[test]
    fn scroll_wall_fog_scale2_and_set_goal() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9901
{
  Health 10
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor Spot : Actor 9001
{
  +NOBLOCKMAP
  States { Spawn: TNT1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.lines[0].tag = 14;
        map.sectors[0].tags.push(14);
        apply_nodes(&mut map);
        let side = map.lines[0].sides[0].unwrap();
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::SCROLL_WALL,
                &[14, 1 << 16, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(w.tex_scrollers.iter().any(|s| s.side == side && s.dx > 0.0));
        assert_eq!(
            execute_special(
                &mut w,
                num::SET_GLOBAL_FOG_PARAMETER,
                &[0, 200, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(w.fog_density > 0.3);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_FLOOR_SCALE2,
                &[14, 2 << 16, 4 << 16, 0, 0],
                None,
                None
            ),
            1
        );
        let (sx, sy) = w.floor_scales[&0];
        assert!((sx - 0.5).abs() < 1e-9 && (sy - 0.25).abs() < 1e-9);
        let dummy = w
            .spawn_at(
                classes.find_name("Dummy").unwrap(),
                DVec3::new(80.0, 80.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(dummy).unwrap().tid = 30;
        let spot = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(120.0, 120.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(spot).unwrap().tid = 31;
        assert_eq!(
            execute_special(&mut w, num::THING_SET_GOAL, &[30, 31, 2, 1, 0], None, None),
            1
        );
        let a = w.actors.get(dummy).unwrap();
        assert_eq!(a.goal, Some(spot));
        assert!(a.chase_goal);
        assert_eq!(a.reaction_time, 70);
    }

    #[test]
    fn door_wait_raise_and_wait_close() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 8.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(11);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::DOOR_WAIT_RAISE,
                &[11, 16, 150, 2, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.doors[0].direction, 0);
        assert!(w.doors[0].wait_then_raise);
        tick_doors(&mut w);
        tick_doors(&mut w);
        assert_eq!(w.doors[0].direction, 1);
        assert!(!w.doors[0].wait_then_raise);
        // Open ceiling a bit then wait-close.
        w.sectors[0].ceiling_z = 64.0;
        w.doors.clear();
        assert_eq!(
            execute_special(&mut w, num::DOOR_WAIT_CLOSE, &[11, 16, 1, 0, 0], None, None),
            1
        );
        tick_doors(&mut w);
        assert_eq!(w.doors[0].direction, -1);
    }

    #[test]
    fn spawn_facing_change_sound_and_camera() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9901
{
  Health 10
  States { Spawn: POSS A -1 Stop }
}
actor Spot : Actor 9001
{
  +NOBLOCKMAP
  States { Spawn: TNT1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(9);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let spot = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(100.0, 100.0, 0.0),
                Angle::from_degrees(90.0),
            )
            .unwrap();
        w.actors.get_mut(spot).unwrap().tid = 20;
        assert_eq!(
            execute_special(
                &mut w,
                num::THING_SPAWN_FACING,
                &[20, 9901, 1, 21, 0],
                None,
                None
            ),
            1
        );
        let spawned = w.find_by_tid(21).unwrap();
        assert!((w.actors.get(spawned).unwrap().angle.degrees() - 90.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_CHANGE_SOUND,
                &[9, 3, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.sector_seq_type[&0], 3);
        w.actors.get_mut(spot).unwrap().tid = 22;
        assert_eq!(
            execute_special(
                &mut w,
                num::CHANGE_CAMERA,
                &[22, 0, 1, 0, 0],
                Some(mo),
                None
            ),
            1
        );
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.camera, Some(spot));
        assert!(p.camera_revert);
    }

    #[test]
    fn sector_color_fade_damage_gravity_and_translation() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(8);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_COLOR,
                &[8, 10, 20, 30, 40],
                None,
                None
            ),
            1
        );
        assert_eq!(w.sectors[0].color, [10, 20, 30]);
        assert_eq!(w.sectors[0].color_desaturate, 40);
        assert_eq!(
            execute_special(&mut w, num::SECTOR_SET_FADE, &[8, 1, 2, 3, 0], None, None),
            1
        );
        assert_eq!(w.sectors[0].fade, [1, 2, 3]);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_DAMAGE,
                &[8, 5, 0, 16, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.sectors[0].damage_amount, 5);
        assert_eq!(w.sectors[0].damage_interval, 16);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_GRAVITY,
                &[8, 1, 50, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.sectors[0].gravity - 1.5).abs() < 1e-9);
        w.actors.get_mut(mo).unwrap().tid = 12;
        assert_eq!(
            execute_special(
                &mut w,
                num::THING_SET_TRANSLATION,
                &[12, 3, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.actors.get(mo).unwrap().translation, "ACS3");
    }

    #[test]
    fn thing_damage_move_glass_and_sector_flags() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9901
{
  Health 40
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor Spot : Actor 9001
{
  +NOBLOCKMAP
  States { Spawn: TNT1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(6);
        map.lines[0].flags |= crate::world::ML_BLOCKING as i16;
        map.lines[0].special = 49;
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let dummy = w
            .spawn_at(
                classes.find_name("Dummy").unwrap(),
                DVec3::new(64.0, 64.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(dummy).unwrap().tid = 10;
        let spot = w
            .spawn_at(
                classes.find_name("Spot").unwrap(),
                DVec3::new(200.0, 200.0, 0.0),
                Angle::from_degrees(90.0),
            )
            .unwrap();
        w.actors.get_mut(spot).unwrap().tid = 11;
        assert_eq!(
            execute_special(&mut w, num::THING_DAMAGE, &[10, 15, 0, 0, 0], None, None),
            1
        );
        assert_eq!(w.actors.get(dummy).unwrap().health, 25);
        assert_eq!(
            execute_special(&mut w, num::THING_MOVE, &[10, 11, 1, 0, 0], None, None),
            1
        );
        let p = w.actors.get(dummy).unwrap().pos;
        assert!((p.x() - 200.0).abs() < 1e-6 && (p.y() - 200.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_CHANGE_FLAGS,
                &[6, 0x10, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.sector_flags[&0], 0x10);
        let lid = room_core::LineId::from_raw(0);
        assert_eq!(
            execute_special(&mut w, num::GLASS_BREAK, &[0, 0, 0, 0, 0], None, Some(lid)),
            1
        );
        assert_eq!(
            w.line_flags.get(&0).copied().unwrap() & crate::world::ML_BLOCKING,
            0
        );
        assert_eq!(w.line_special(lid), 0);
    }

    #[test]
    fn sector_scale_and_rotation() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(5);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        // x=2 → scale 0.5; y=4 → 0.25
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_FLOOR_SCALE,
                &[5, 2, 0, 4, 0],
                None,
                None
            ),
            1
        );
        let (sx, sy) = w.floor_scales[&0];
        assert!((sx - 0.5).abs() < 1e-9 && (sy - 0.25).abs() < 1e-9);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_CEILING_SCALE,
                &[5, 1, 0, 2, 0],
                None,
                None
            ),
            1
        );
        let (cx, cy) = w.ceiling_scales[&0];
        assert!((cx - 1.0).abs() < 1e-9 && (cy - 0.5).abs() < 1e-9);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_ROTATION,
                &[5, 45, 90, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.plane_angles[&0], (45.0, 90.0));
    }

    #[test]
    fn sector_panning_and_damage_thing() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(2);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_FLOOR_PANNING,
                &[2, 16, 0, 8, 0],
                None,
                None
            ),
            1
        );
        let (u, v) = w.floor_scroll_offsets[&0];
        assert!((u - 16.0).abs() < 1e-6 && (v - 8.0).abs() < 1e-6);
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_CEILING_PANNING,
                &[2, 4, 0, 2, 0],
                None,
                None
            ),
            1
        );
        let (cu, cv) = w.ceiling_scroll_offsets[&0];
        assert!((cu - 4.0).abs() < 1e-6 && (cv - 2.0).abs() < 1e-6);
        w.actors.get_mut(mo).unwrap().health = 50;
        assert_eq!(
            execute_special(&mut w, num::DAMAGE_THING, &[10, 0, 0, 0, 0], Some(mo), None),
            1
        );
        assert_eq!(w.actors.get(mo).unwrap().health, 40);
    }

    #[test]
    fn line_scale_and_player_property() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.lines[0].tag = 9;
        apply_nodes(&mut map);
        let side = map.lines[0].sides[0].unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::LINE_SET_TEXTURE_SCALE,
                &[9, 2 << 16, 1 << 16, 0, 0],
                None,
                None
            ),
            1
        );
        let (sx, sy) = w.side_scales.get(&side.raw()).copied().unwrap();
        assert!((sx - 2.0).abs() < 1e-5 && (sy - 1.0).abs() < 1e-5);
        assert_eq!(
            execute_special(
                &mut w,
                num::SET_PLAYER_PROPERTY,
                &[0, 1, 0, 0, 0],
                Some(mo),
                None
            ),
            1
        );
        assert!(w.players[0].as_ref().unwrap().frozen);
        assert_eq!(
            execute_special(
                &mut w,
                num::SET_PLAYER_PROPERTY,
                &[0, 1, 3, 0, 0],
                Some(mo),
                None
            ),
            1
        );
        assert_ne!(
            w.actors.get(mo).unwrap().flags & crate::class::FLAG_NOGRAVITY,
            0
        );
    }

    #[test]
    fn line_set_texture_offset_stores() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.lines[0].tag = 7;
        apply_nodes(&mut map);
        let side = map.lines[0].sides[0].unwrap();
        let mut w = World::from_map(map).unwrap();
        // 16<<16 fixed → 16 map units
        assert_eq!(
            execute_special(
                &mut w,
                num::LINE_SET_TEXTURE_OFFSET,
                &[7, 16 << 16, 8 << 16, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.side_offsets.get(&side.raw()).copied(), Some((16, 8)));
    }

    #[test]
    fn thing_raise_revives_corpse() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9901
{
  Health 10
  +COUNTKILL
  States
  {
  Spawn: POSS A -1 Stop
  Raise: POSS A 1 A_Look Goto Spawn
  }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let id = w
            .spawn_at(
                classes.find_name("Dummy").unwrap(),
                DVec3::new(80.0, 80.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(id).unwrap().tid = 4;
        w.actors.get_mut(id).unwrap().health = 0;
        w.actors.get_mut(id).unwrap().flags |= crate::class::FLAG_CORPSE;
        w.level.found_kills = 1;
        assert_eq!(
            execute_special(&mut w, num::THING_RAISE, &[4, 0, 0, 0, 0], None, None),
            1
        );
        let a = w.actors.get(id).unwrap();
        assert_eq!(a.health, 10);
        assert_eq!(a.flags & crate::class::FLAG_CORPSE, 0);
        assert_eq!(w.level.found_kills, 0);
    }

    #[test]
    fn thrust_thing_z_and_autosave() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        // speed 4 → 1.0 mu/tic up, add
        assert_eq!(
            execute_special(
                &mut w,
                num::THRUST_THING_Z,
                &[0, 4, 0, 0, 0],
                Some(mo),
                None
            ),
            1
        );
        assert!((w.actors.get(mo).unwrap().vel.z() - 1.0).abs() < 1e-6);
        assert_eq!(
            execute_special(&mut w, num::AUTOSAVE, &[0, 0, 0, 0, 0], None, None),
            1
        );
        assert!(w.autosave_request);
    }

    #[test]
    fn thing_hate_and_set_special() {
        let ir = room_decorate::parse_decorate(
            r#"
actor Dummy : Actor 9901
{
  Health 10
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes.clone(), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let hater = w
            .spawn_at(
                classes.find_name("Dummy").unwrap(),
                DVec3::new(80.0, 80.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(hater).unwrap().tid = 3;
        w.actors.get_mut(hater).unwrap().flags |= crate::class::FLAG_FRIENDLY;
        assert_eq!(
            execute_special(&mut w, num::THING_HATE, &[3, 0, 0, 0, 0], Some(mo), None),
            1
        );
        assert_eq!(w.actors.get(hater).unwrap().target, Some(mo));
        assert_eq!(
            w.actors.get(hater).unwrap().flags & crate::class::FLAG_FRIENDLY,
            0
        );
        assert_eq!(
            execute_special(
                &mut w,
                num::THING_SET_SPECIAL,
                &[3, 80, 1, 2, 3],
                None,
                None
            ),
            1
        );
        let a = w.actors.get(hater).unwrap();
        assert_eq!(a.special, 80);
        assert_eq!(a.args, [1, 2, 3, 0, 0]);
    }

    #[test]
    fn thing_stop_and_heal_thing() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().vel = DVec3::new(10.0, 5.0, 0.0);
        w.actors.get_mut(mo).unwrap().health = 50;
        w.actors.get_mut(mo).unwrap().tid = 2;
        assert_eq!(
            execute_special(&mut w, num::THING_STOP, &[2, 0, 0, 0, 0], None, None),
            1
        );
        let v = w.actors.get(mo).unwrap().vel;
        assert_eq!((v.x(), v.y(), v.z()), (0.0, 0.0, 0.0));
        assert_eq!(
            execute_special(&mut w, num::HEAL_THING, &[25, 0, 0, 0, 0], Some(mo), None),
            1
        );
        assert_eq!(w.actors.get(mo).unwrap().health, 75);
    }

    #[test]
    fn thing_change_tid_and_change_skill() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 3;
        assert_eq!(
            execute_special(&mut w, num::THING_CHANGE_TID, &[3, 11, 0, 0, 0], None, None),
            1
        );
        assert_eq!(w.actors.get(mo).unwrap().tid, 11);
        assert_eq!(
            execute_special(&mut w, num::CHANGE_SKILL, &[4, 0, 0, 0, 0], None, None),
            1
        );
        assert_eq!(w.skill, 5);
        assert!(w.fast_monsters);
        assert_eq!(
            execute_special(
                &mut w,
                num::THING_SET_CONVERSATION,
                &[11, 42, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.actors.get(mo).unwrap().conversation, 42);
        let npc_tid = 22;
        w.actors.get_mut(mo).unwrap().tid = 0;
        // Spawn a stand-in talker by retargeting a clone of the player pawn fields.
        let talker = w
            .spawn_at(
                w.actors.get(mo).unwrap().class,
                DVec3::new(80.0, 32.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(talker).unwrap().tid = npc_tid;
        w.actors.get_mut(talker).unwrap().conversation = 7;
        assert_eq!(
            execute_special(
                &mut w,
                num::START_CONVERSATION,
                &[npc_tid, 1, 0, 0, 0],
                Some(mo),
                None
            ),
            1
        );
        assert_eq!(w.conversation_request, Some((talker, true)));
    }

    #[test]
    fn noise_alert_special_sets_soundtarget() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 5;
        assert_eq!(
            execute_special(&mut w, num::NOISE_ALERT, &[5, 5, 0, 0, 0], Some(mo), None),
            1
        );
        assert_eq!(w.sectors[0].soundtarget, Some(mo));
    }

    #[test]
    fn translucent_line_stores_alpha() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.lines[0].tag = 77;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::TRANSLUCENT_LINE,
                &[77, 64, 1, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.line_alpha[&0] - 64.0 / 255.0).abs() < 1e-5);
        assert_eq!(w.line_additive.get(&0), Some(&true));
    }

    #[test]
    fn line_set_blocking_sets_and_clears() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.lines[0].tag = 1000;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::LINE_SET_BLOCKING,
                &[1000, crate::world::blockf::EVERYTHING, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert_ne!(
            w.line_flags(LineId::from_raw(0)) & crate::world::ML_BLOCKEVERYTHING,
            0
        );
        assert_eq!(
            execute_special(
                &mut w,
                num::LINE_SET_BLOCKING,
                &[1000, 0, crate::world::blockf::EVERYTHING, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(
            w.line_flags(LineId::from_raw(0)) & crate::world::ML_BLOCKEVERYTHING,
            0
        );
    }

    #[test]
    fn line_set_blocking_finds_moreids_and_clears_hitscan() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.lines[0].extra_ids.push(430_000);
        map.lines[0].block_hitscan = true;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let hitscan = |w: &World| w.line_flags(LineId::from_raw(0)) & crate::world::ML_BLOCKHITSCAN;
        assert_ne!(hitscan(&w), 0);
        let args = [430_000, 0, crate::world::blockf::HITSCAN, 0, 0];
        assert_eq!(
            execute_special(&mut w, num::LINE_SET_BLOCKING, &args, None, None),
            1
        );
        assert_eq!(hitscan(&w), 0);
    }

    #[test]
    fn sector_set_translucent_stores_alpha() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(44);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_TRANSLUCENT,
                &[44, 0, 128, 1, 0],
                None,
                None
            ),
            1
        );
        assert!((w.sectors[0].floor_alpha - 128.0 / 255.0).abs() < 1e-5);
        assert!(w.sectors[0].floor_additive);
        assert_eq!(w.sectors[0].ceiling_alpha, 1.0);
    }

    #[test]
    fn ceiling_lower_and_crush_dist_stops_above_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(33);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_LOWER_AND_CRUSH_DIST,
                &[33, 16, 10, 24, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..80 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - 24.0).abs() < 1e-6);
        assert!(w.ceilings.is_empty());
    }

    #[test]
    fn ceiling_lower_and_crush_sets_crush_flag() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(7);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_LOWER_AND_CRUSH,
                &[7, 8, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!(w.ceilings.iter().any(|c| c.crush && c.once));
    }

    #[test]
    fn light_strobe_and_stop() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(7);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(&mut w, num::LIGHT_STROBE, &[7, 200, 40, 3, 7], None, None),
            1
        );
        assert_eq!(w.strobes.len(), 1);
        assert_eq!(w.strobes[0].bright_time, 3);
        assert_eq!(w.strobes[0].dark_time, 7);
        assert_eq!(
            execute_special(&mut w, num::LIGHT_STOP, &[7, 0, 0, 0, 0], None, None),
            1
        );
        assert!(w.strobes.is_empty());
    }

    #[test]
    fn light_flicker_special_spawns_thinker() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(3);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.sectors[0].light = 200;
        assert_eq!(
            execute_special(&mut w, num::LIGHT_FLICKER, &[3, 200, 50, 0, 0], None, None),
            1
        );
        assert_eq!(w.flickers.len(), 1);
        assert!(w.flickers[0].fire);
        assert_eq!(w.flickers[0].max_light, 200);
        assert_eq!(w.flickers[0].min_light, 50);
        for _ in 0..8 {
            tick_flickers(&mut w);
        }
        assert!(w.sectors[0].light <= 200);
    }

    #[test]
    fn light_glow_oscillates() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(2);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.sectors[0].light = 100;
        assert_eq!(
            execute_special(&mut w, num::LIGHT_GLOW, &[2, 200, 50, 5, 0], None, None),
            1
        );
        for _ in 0..5 {
            tick_glows(&mut w);
        }
        assert!(w.sectors[0].light >= 180);
        for _ in 0..5 {
            tick_glows(&mut w);
        }
        assert!(w.sectors[0].light <= 80);
    }

    #[test]
    fn ceiling_raise_by_value() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(8);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let z0 = w.sectors[0].ceiling_z;
        assert_eq!(
            execute_special(
                &mut w,
                num::CEILING_RAISE_BY_VALUE,
                &[8, 8, 32, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..64 {
            tick_ceilings(&mut w);
        }
        assert!((w.sectors[0].ceiling_z - (z0 + 32.0)).abs() < 1e-6);
    }

    #[test]
    fn light_fade_reaches_dest() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 64.0, 64.0), 0.0, 128.0)
            .player_start(16.0, 16.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(9);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        w.sectors[0].light = 200;
        assert_eq!(
            execute_special(&mut w, num::LIGHT_FADE, &[9, 50, 10, 0, 0], None, None),
            1
        );
        assert_eq!(w.fades.len(), 1);
        for _ in 0..10 {
            tick_fades(&mut w);
        }
        assert!(w.fades.is_empty());
        assert_eq!(w.sectors[0].light, 50);
    }

    #[test]
    fn floor_raise_to_lowest_ceiling() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(7);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::FLOOR_RAISE_TO_LOWEST_CEILING,
                &[7, 8, 0, 0, 0],
                None,
                None
            ),
            1
        );
        for _ in 0..160 {
            tick_floors(&mut w);
        }
        assert!((w.sectors[0].floor_z - 120.0).abs() < 1e-6);
    }

    #[test]
    fn sector_set_friction_amount_100_is_orig() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(3);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_FRICTION,
                &[3, 100, 0, 0, 0],
                None,
                None
            ),
            1
        );
        assert!((w.sectors[0].friction - 0.90625).abs() < 1e-4);
    }

    #[test]
    fn sector_set_wind_thrusts_player_vel() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(5);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        // amount=32 → 1 unit/tic, angle=0 (east), useline=0
        assert_eq!(
            execute_special(&mut w, num::SECTOR_SET_WIND, &[5, 32, 0, 0, 0], None, None),
            1
        );
        assert!((w.sectors[0].wind_x - 1.0).abs() < 1e-6);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        tick_sector_pushers(&mut w);
        let vx = w.actors.get(mo).unwrap().vel.x();
        assert!(vx > 0.5, "vx={vx}");
    }

    #[test]
    fn point_push_set_force_pushes_player() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  States { Spawn: PLAY A -1 Stop }
}
actor PointPusher : Actor 5001
{
  +NOGRAVITY
  States { Spawn: TNT1 A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(200.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(1);
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let pusher = w.classes.find_doomednum(5001).unwrap();
        let src = w
            .spawn_at(pusher, DVec3::new(128.0, 128.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(src) {
            a.tid = 7;
        }
        assert_eq!(
            execute_special(
                &mut w,
                num::POINT_PUSH_SET_FORCE,
                &[1, 7, 128, 0, 0],
                None,
                None
            ),
            1
        );
        assert_eq!(w.point_forces.len(), 1);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        if let Some(a) = w.actors.get_mut(mo) {
            a.vel = DVec3::new(0.0, 0.0, 0.0);
        }
        tick_sector_pushers(&mut w);
        let vx = w.actors.get(mo).unwrap().vel.x();
        assert!(vx > 0.0, "expected push away from source, vx={vx}");
    }

    #[test]
    fn sector_set_current_floor_only() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].tags.push(6);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(
            execute_special(
                &mut w,
                num::SECTOR_SET_CURRENT,
                &[6, 64, 64, 0, 0], // byte angle 64 = 90°
                None,
                None
            ),
            1
        );
        assert!(w.sectors[0].current_y.abs() > 1.0);
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        // Airborne: current should not apply.
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(a.pos.x(), a.pos.y(), 40.0);
            a.vel = DVec3::new(0.0, 0.0, 0.0);
        }
        tick_sector_pushers(&mut w);
        let vy_air = w.actors.get(mo).unwrap().vel.y();
        assert!(vy_air.abs() < 1e-9, "vy_air={vy_air}");
        if let Some(a) = w.actors.get_mut(mo) {
            a.pos = DVec3::new(a.pos.x(), a.pos.y(), 0.0);
            a.vel = DVec3::new(0.0, 0.0, 0.0);
        }
        tick_sector_pushers(&mut w);
        let vy = w.actors.get(mo).unwrap().vel.y();
        assert!(vy > 1.0, "vy={vy}");
    }
}

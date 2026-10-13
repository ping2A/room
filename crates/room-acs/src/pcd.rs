//! Hexen / ZDoom PCD opcode numbers (prefix of the full enum).
//!
//! Numbering matches GZDoom `p_acs.h` / ACC `pcdefs.h`.

#![allow(dead_code)]

/// `PCD_NOP`
pub const NOP: u32 = 0;
/// `PCD_TERMINATE`
pub const TERMINATE: u32 = 1;
/// `PCD_SUSPEND`
pub const SUSPEND: u32 = 2;
/// `PCD_PUSHNUMBER`
pub const PUSHNUMBER: u32 = 3;
/// `PCD_LSPEC1`
pub const LSPEC1: u32 = 4;
/// `PCD_LSPEC2`
pub const LSPEC2: u32 = 5;
/// `PCD_LSPEC3`
pub const LSPEC3: u32 = 6;
/// `PCD_LSPEC4`
pub const LSPEC4: u32 = 7;
/// `PCD_LSPEC5`
pub const LSPEC5: u32 = 8;
/// `PCD_LSPEC1DIRECT`
pub const LSPEC1DIRECT: u32 = 9;
/// `PCD_LSPEC2DIRECT`
pub const LSPEC2DIRECT: u32 = 10;
/// `PCD_LSPEC3DIRECT`
pub const LSPEC3DIRECT: u32 = 11;
/// `PCD_LSPEC4DIRECT`
pub const LSPEC4DIRECT: u32 = 12;
/// `PCD_LSPEC5DIRECT`
pub const LSPEC5DIRECT: u32 = 13;
/// `PCD_ADD`
pub const ADD: u32 = 14;
/// `PCD_SUBTRACT`
pub const SUBTRACT: u32 = 15;
/// `PCD_MULTIPLY`
pub const MULTIPLY: u32 = 16;
/// `PCD_DIVIDE`
pub const DIVIDE: u32 = 17;
/// `PCD_MODULUS`
pub const MODULUS: u32 = 18;
/// `PCD_EQ`
pub const EQ: u32 = 19;
/// `PCD_NE`
pub const NE: u32 = 20;
/// `PCD_LT`
pub const LT: u32 = 21;
/// `PCD_GT`
pub const GT: u32 = 22;
/// `PCD_LE`
pub const LE: u32 = 23;
/// `PCD_GE`
pub const GE: u32 = 24;
/// `PCD_ASSIGNSCRIPTVAR`
pub const ASSIGNSCRIPTVAR: u32 = 25;
/// `PCD_ASSIGNMAPVAR`
pub const ASSIGNMAPVAR: u32 = 26;
/// `PCD_ASSIGNWORLDVAR`
pub const ASSIGNWORLDVAR: u32 = 27;
/// `PCD_PUSHSCRIPTVAR`
pub const PUSHSCRIPTVAR: u32 = 28;
/// `PCD_PUSHMAPVAR`
pub const PUSHMAPVAR: u32 = 29;
/// `PCD_PUSHWORLDVAR`
pub const PUSHWORLDVAR: u32 = 30;
/// `PCD_ADDSCRIPTVAR`
pub const ADDSCRIPTVAR: u32 = 31;
/// `PCD_ADDMAPVAR`
pub const ADDMAPVAR: u32 = 32;
/// `PCD_ADDWORLDVAR`
pub const ADDWORLDVAR: u32 = 33;
/// `PCD_SUBSCRIPTVAR`
pub const SUBSCRIPTVAR: u32 = 34;
/// `PCD_SUBMAPVAR`
pub const SUBMAPVAR: u32 = 35;
/// `PCD_SUBWORLDVAR`
pub const SUBWORLDVAR: u32 = 36;
/// `PCD_MULSCRIPTVAR`
pub const MULSCRIPTVAR: u32 = 37;
/// `PCD_MULMAPVAR`
pub const MULMAPVAR: u32 = 38;
/// `PCD_MULWORLDVAR`
pub const MULWORLDVAR: u32 = 39;
/// `PCD_DIVSCRIPTVAR`
pub const DIVSCRIPTVAR: u32 = 40;
/// `PCD_DIVMAPVAR`
pub const DIVMAPVAR: u32 = 41;
/// `PCD_DIVWORLDVAR`
pub const DIVWORLDVAR: u32 = 42;
/// `PCD_MODSCRIPTVAR`
pub const MODSCRIPTVAR: u32 = 43;
/// `PCD_MODMAPVAR`
pub const MODMAPVAR: u32 = 44;
/// `PCD_MODWORLDVAR`
pub const MODWORLDVAR: u32 = 45;
/// `PCD_INCSCRIPTVAR`
pub const INCSCRIPTVAR: u32 = 46;
/// `PCD_INCMAPVAR`
pub const INCMAPVAR: u32 = 47;
/// `PCD_INCWORLDVAR`
pub const INCWORLDVAR: u32 = 48;
/// `PCD_DECSCRIPTVAR`
pub const DECSCRIPTVAR: u32 = 49;
/// `PCD_DECMAPVAR`
pub const DECMAPVAR: u32 = 50;
/// `PCD_DECWORLDVAR`
pub const DECWORLDVAR: u32 = 51;
/// `PCD_GOTO`
pub const GOTO: u32 = 52;
/// `PCD_IFGOTO`
pub const IFGOTO: u32 = 53;
/// `PCD_DROP`
pub const DROP: u32 = 54;
/// `PCD_DELAY`
pub const DELAY: u32 = 55;
/// `PCD_DELAYDIRECT`
pub const DELAYDIRECT: u32 = 56;
/// `PCD_RANDOM`
pub const RANDOM: u32 = 57;
/// `PCD_RANDOMDIRECT`
pub const RANDOMDIRECT: u32 = 58;
/// `PCD_THINGCOUNT`
pub const THINGCOUNT: u32 = 59;
/// `PCD_THINGCOUNTDIRECT`
pub const THINGCOUNTDIRECT: u32 = 60;
/// `PCD_TAGWAIT`
pub const TAGWAIT: u32 = 61;
/// `PCD_TAGWAITDIRECT`
pub const TAGWAITDIRECT: u32 = 62;
/// `PCD_POLYWAIT`
pub const POLYWAIT: u32 = 63;
/// `PCD_POLYWAITDIRECT`
pub const POLYWAITDIRECT: u32 = 64;
/// `PCD_CHANGEFLOOR`
pub const CHANGEFLOOR: u32 = 65;
/// `PCD_CHANGEFLOORDIRECT`
pub const CHANGEFLOORDIRECT: u32 = 66;
/// `PCD_CHANGECEILING`
pub const CHANGECEILING: u32 = 67;
/// `PCD_CHANGECEILINGDIRECT`
pub const CHANGECEILINGDIRECT: u32 = 68;
/// `PCD_RESTART`
pub const RESTART: u32 = 69;
/// `PCD_ANDLOGICAL`
pub const ANDLOGICAL: u32 = 70;
/// `PCD_ORLOGICAL`
pub const ORLOGICAL: u32 = 71;
/// `PCD_ANDBITWISE`
pub const ANDBITWISE: u32 = 72;
/// `PCD_ORBITWISE`
pub const ORBITWISE: u32 = 73;
/// `PCD_EORBITWISE`
pub const EORBITWISE: u32 = 74;
/// `PCD_NEGATELOGICAL`
pub const NEGATELOGICAL: u32 = 75;
/// `PCD_LSHIFT`
pub const LSHIFT: u32 = 76;
/// `PCD_RSHIFT`
pub const RSHIFT: u32 = 77;
/// `PCD_UNARYMINUS`
pub const UNARYMINUS: u32 = 78;
/// `PCD_IFNOTGOTO`
pub const IFNOTGOTO: u32 = 79;
/// `PCD_LINESIDE`
pub const LINESIDE: u32 = 80;
/// `PCD_SCRIPTWAIT`
pub const SCRIPTWAIT: u32 = 81;
/// `PCD_SCRIPTWAITDIRECT`
pub const SCRIPTWAITDIRECT: u32 = 82;
/// `PCD_CLEARLINESPECIAL`
pub const CLEARLINESPECIAL: u32 = 83;
/// `PCD_CASEGOTO`
pub const CASEGOTO: u32 = 84;
/// `PCD_BEGINPRINT`
pub const BEGINPRINT: u32 = 85;
/// `PCD_ENDPRINT`
pub const ENDPRINT: u32 = 86;
/// `PCD_PRINTSTRING`
pub const PRINTSTRING: u32 = 87;
/// `PCD_PRINTNUMBER`
pub const PRINTNUMBER: u32 = 88;
/// `PCD_PRINTCHARACTER`
pub const PRINTCHARACTER: u32 = 89;
/// `PCD_PLAYERCOUNT`
pub const PLAYERCOUNT: u32 = 90;
/// `PCD_GAMETYPE`
pub const GAMETYPE: u32 = 91;
/// `PCD_GAMESKILL`
pub const GAMESKILL: u32 = 92;
/// `PCD_TIMER`
pub const TIMER: u32 = 93;
/// `PCD_SECTORSOUND`
pub const SECTORSOUND: u32 = 94;
/// `PCD_AMBIENTSOUND`
pub const AMBIENTSOUND: u32 = 95;
/// `PCD_SOUNDSEQUENCE`
pub const SOUNDSEQUENCE: u32 = 96;
/// `PCD_SETLINETEXTURE`
pub const SETLINETEXTURE: u32 = 97;
/// `PCD_SETLINEBLOCKING`
pub const SETLINEBLOCKING: u32 = 98;
/// `PCD_SETLINESPECIAL`
pub const SETLINESPECIAL: u32 = 99;
/// `PCD_THINGSOUND`
pub const THINGSOUND: u32 = 100;
/// `PCD_ENDPRINTBOLD`
pub const ENDPRINTBOLD: u32 = 101;
/// `PCD_ACTIVATORSOUND`
pub const ACTIVATORSOUND: u32 = 102;
/// `PCD_LOCALAMBIENTSOUND`
pub const LOCALAMBIENTSOUND: u32 = 103;
/// `PCD_SETFONT`
pub const SETFONT: u32 = 165;
/// `PCD_SETFONTDIRECT`
pub const SETFONTDIRECT: u32 = 166;
/// `PCD_PUSHBYTE` — push one unsigned byte (ACSe packed).
pub const PUSHBYTE: u32 = 167;
/// `PCD_FADETO`
pub const FADETO: u32 = 190;
/// `PCD_FADERANGE`
pub const FADERANGE: u32 = 191;
/// `PCD_CANCELFADE`
pub const CANCELFADE: u32 = 192;
/// `PCD_SIN` — BAM angle → fixed sine.
pub const SIN: u32 = 220;
/// `PCD_COS`
pub const COS: u32 = 221;
/// `PCD_VECTORANGLE`
pub const VECTORANGLE: u32 = 222;
/// `PCD_CHECKWEAPON`
pub const CHECKWEAPON: u32 = 223;
/// `PCD_SETWEAPON`
pub const SETWEAPON: u32 = 224;
/// `PCD_PLAYERNUMBER`
pub const PLAYERNUMBER: u32 = 247;
/// `PCD_GETSCREENWIDTH`
pub const GETSCREENWIDTH: u32 = 250;
/// `PCD_GETSCREENHEIGHT`
pub const GETSCREENHEIGHT: u32 = 251;
/// `PCD_STRLEN`
pub const STRLEN: u32 = 253;
/// `PCD_GETCVAR`
pub const GETCVAR: u32 = 255;
/// `PCD_USEINVENTORY` (ACC / Hexen; was briefly misnumbered 335).
pub const USEINVENTORY: u32 = 336;
/// `PCD_USEACTORINVENTORY`
pub const USEACTORINVENTORY: u32 = 337;
/// `PCD_CHECKACTORCEILINGTEXTURE`
pub const CHECKACTORCEILINGTEXTURE: u32 = 338;
/// `PCD_CHECKACTORFLOORTEXTURE`
pub const CHECKACTORFLOORTEXTURE: u32 = 339;
/// `PCD_LSPEC1DIRECTB`
pub const LSPEC1DIRECTB: u32 = 168;
/// `PCD_LSPEC2DIRECTB`
pub const LSPEC2DIRECTB: u32 = 169;
/// `PCD_LSPEC3DIRECTB`
pub const LSPEC3DIRECTB: u32 = 170;
/// `PCD_LSPEC4DIRECTB`
pub const LSPEC4DIRECTB: u32 = 171;
/// `PCD_LSPEC5DIRECTB`
pub const LSPEC5DIRECTB: u32 = 172;
/// `PCD_DELAYDIRECTB`
pub const DELAYDIRECTB: u32 = 173;
/// `PCD_RANDOMDIRECTB`
pub const RANDOMDIRECTB: u32 = 174;
/// `PCD_PUSHBYTES` — count byte then that many bytes pushed.
pub const PUSHBYTES: u32 = 175;
/// `PCD_PUSH2BYTES`
pub const PUSH2BYTES: u32 = 176;
/// `PCD_PUSH3BYTES`
pub const PUSH3BYTES: u32 = 177;
/// `PCD_PUSH4BYTES`
pub const PUSH4BYTES: u32 = 178;
/// `PCD_PUSH5BYTES`
pub const PUSH5BYTES: u32 = 179;
/// `PCD_CLEARINVENTORY` — clear activator inventory.
pub const CLEARINVENTORY: u32 = 142;
/// `PCD_GIVEINVENTORY`
pub const GIVEINVENTORY: u32 = 143;
/// `PCD_GIVEINVENTORYDIRECT`
pub const GIVEINVENTORYDIRECT: u32 = 144;
/// `PCD_TAKEINVENTORY`
pub const TAKEINVENTORY: u32 = 145;
/// `PCD_TAKEINVENTORYDIRECT`
pub const TAKEINVENTORYDIRECT: u32 = 146;
/// `PCD_CHECKINVENTORY`
pub const CHECKINVENTORY: u32 = 147;
/// `PCD_CHECKINVENTORYDIRECT`
pub const CHECKINVENTORYDIRECT: u32 = 148;
/// `PCD_SPAWN`
pub const SPAWN: u32 = 149;
/// `PCD_SPAWNDIRECT`
pub const SPAWNDIRECT: u32 = 150;
/// `PCD_SPAWNSPOT`
pub const SPAWNSPOT: u32 = 151;
/// `PCD_SPAWNSPOTDIRECT`
pub const SPAWNSPOTDIRECT: u32 = 152;
/// `PCD_SETMUSIC`
pub const SETMUSIC: u32 = 153;
/// `PCD_SETMUSICDIRECT`
pub const SETMUSICDIRECT: u32 = 154;
/// `PCD_LOCALSETMUSIC`
pub const LOCALSETMUSIC: u32 = 155;
/// `PCD_LOCALSETMUSICDIRECT`
pub const LOCALSETMUSICDIRECT: u32 = 156;
/// `PCD_FIXEDMUL`
pub const FIXEDMUL: u32 = 136;
/// `PCD_FIXEDDIV`
pub const FIXEDDIV: u32 = 137;
/// `PCD_SETGRAVITY` — level gravity (16.16).
pub const SETGRAVITY: u32 = 138;
/// `PCD_SETGRAVITYDIRECT`
pub const SETGRAVITYDIRECT: u32 = 139;
/// `PCD_SETAIRCONTROL` — level air control (16.16).
pub const SETAIRCONTROL: u32 = 140;
/// `PCD_SETAIRCONTROLDIRECT`
pub const SETAIRCONTROLDIRECT: u32 = 141;
/// `PCD_SINGLEPLAYER` — 1 in single-player bootstrap.
pub const SINGLEPLAYER: u32 = 135;
/// `PCD_GETACTORX`
pub const GETACTORX: u32 = 196;
/// `PCD_GETACTORY`
pub const GETACTORY: u32 = 197;
/// `PCD_GETACTORZ`
pub const GETACTORZ: u32 = 198;
/// `PCD_SETACTORPROPERTY`
pub const SETACTORPROPERTY: u32 = 245;
/// `PCD_GETACTORPROPERTY`
pub const GETACTORPROPERTY: u32 = 246;
/// `PCD_ACTIVATORTID`
pub const ACTIVATORTID: u32 = 248;
/// `PCD_SETRESULTVALUE`
pub const SETRESULTVALUE: u32 = 257;
/// `PCD_GETACTORFLOORZ`
pub const GETACTORFLOORZ: u32 = 259;
/// `PCD_GETACTORANGLE`
pub const GETACTORANGLE: u32 = 260;
/// `PCD_GETLEVELINFO` — `LEVELINFO_*` selector on stack.
pub const GETLEVELINFO: u32 = 265;
/// `PCD_CHANGESKY` — sky1, sky2 texture names.
pub const CHANGESKY: u32 = 266;
/// `PCD_PLAYERINGAME`
pub const PLAYERINGAME: u32 = 267;
/// `PCD_SETCAMERATOTEXTURE` — tid, texture string, fov.
pub const SETCAMERATOTEXTURE: u32 = 269;
/// `PCD_ENDLOG` — flush print buffer to host log.
pub const ENDLOG: u32 = 270;
/// `PCD_GETAMMOCAPACITY`
pub const GETAMMOCAPACITY: u32 = 271;
/// `PCD_SETAMMOCAPACITY`
pub const SETAMMOCAPACITY: u32 = 272;
/// `PCD_GETPLAYERINFO` — playernum, infotype.
pub const GETPLAYERINFO: u32 = 326;
/// `PCD_CHANGELEVEL` — map, pos, flags, skill.
pub const CHANGELEVEL: u32 = 327;
/// `PCD_SECTORDAMAGE` — tag, amount, type, protection, flags.
pub const SECTORDAMAGE: u32 = 328;
/// `PCD_REPLACETEXTURES` — from, to, flags.
pub const REPLACETEXTURES: u32 = 329;
/// `PCD_NEGATEBINARY` — bitwise not.
pub const NEGATEBINARY: u32 = 330;
/// `PCD_SETACTORPITCH`
pub const SETACTORPITCH: u32 = 332;
/// `PCD_SETACTORSTATE` — tid, state label string, [, exact].
pub const SETACTORSTATE: u32 = 334;
/// `PCD_THINGDAMAGE2` — tid, amount, mod string.
pub const THINGDAMAGE2: u32 = 335;
/// `PCD_GETSECTORFLOORZ`
pub const GETSECTORFLOORZ: u32 = 261;
/// `PCD_GETSECTORCEILINGZ`
pub const GETSECTORCEILINGZ: u32 = 262;
/// `PCD_GETSECTORLIGHTLEVEL`
pub const GETSECTORLIGHTLEVEL: u32 = 281;
/// `PCD_SETACTORANGLE`
pub const SETACTORANGLE: u32 = 276;
/// `PCD_GETACTORCEILINGZ`
pub const GETACTORCEILINGZ: u32 = 282;
/// `PCD_SETACTORPOSITION`
pub const SETACTORPOSITION: u32 = 283;
/// `PCD_CLEARACTORINVENTORY`
pub const CLEARACTORINVENTORY: u32 = 284;
/// `PCD_GIVEACTORINVENTORY`
pub const GIVEACTORINVENTORY: u32 = 285;
/// `PCD_TAKEACTORINVENTORY`
pub const TAKEACTORINVENTORY: u32 = 286;
/// `PCD_CHECKACTORINVENTORY`
pub const CHECKACTORINVENTORY: u32 = 287;
/// `PCD_GETACTORPITCH`
pub const GETACTORPITCH: u32 = 331;
/// `PCD_CALLFUNC` — ACSF table dispatch.
pub const CALLFUNC: u32 = 351;
/// `PCD_THINGCOUNTNAME` — count by class name string.
pub const THINGCOUNTNAME: u32 = 288;
/// `PCD_SPAWNSPOTFACING` — spawn at spots using each spot's angle.
pub const SPAWNSPOTFACING: u32 = 289;
/// `PCD_CLASSIFYACTOR` — actor category bitmask.
pub const CLASSIFYACTOR: u32 = 346;
/// `PCD_GETACTORLIGHTLEVEL`
pub const GETACTORLIGHTLEVEL: u32 = 340;
/// `PCD_THINGCOUNTSECTOR` — count by DoomEdNum + optional tid in tagged sectors.
pub const THINGCOUNTSECTOR: u32 = 342;
/// `PCD_THINGCOUNTNAMESECTOR` — count by class name in tagged sectors.
pub const THINGCOUNTNAMESECTOR: u32 = 343;
/// `PCD_SCRIPTWAITNAMED` — wait for a named script (SNAM).
pub const SCRIPTWAITNAMED: u32 = 361;

/// ACSF indices (GZDoom `EACSFunctions`; starts at 1).
pub mod acsf {
    /// `ACSF_GetLineUDMFInt`
    pub const GET_LINE_UDMF_INT: i32 = 1;
    /// `ACSF_GetLineUDMFFixed`
    pub const GET_LINE_UDMF_FIXED: i32 = 2;
    /// `ACSF_GetThingUDMFInt`
    pub const GET_THING_UDMF_INT: i32 = 3;
    /// `ACSF_GetThingUDMFFixed`
    pub const GET_THING_UDMF_FIXED: i32 = 4;
    /// `ACSF_GetSectorUDMFInt`
    pub const GET_SECTOR_UDMF_INT: i32 = 5;
    /// `ACSF_GetSectorUDMFFixed`
    pub const GET_SECTOR_UDMF_FIXED: i32 = 6;
    /// `ACSF_GetSideUDMFInt`
    pub const GET_SIDE_UDMF_INT: i32 = 7;
    /// `ACSF_GetSideUDMFFixed`
    pub const GET_SIDE_UDMF_FIXED: i32 = 8;
    /// `ACSF_GetActorVelX`
    pub const GET_ACTOR_VEL_X: i32 = 9;
    /// `ACSF_GetActorVelY`
    pub const GET_ACTOR_VEL_Y: i32 = 10;
    /// `ACSF_GetActorVelZ`
    pub const GET_ACTOR_VEL_Z: i32 = 11;
    /// `ACSF_SetActivator`
    pub const SET_ACTIVATOR: i32 = 12;
    /// `ACSF_SetActivatorToTarget`
    pub const SET_ACTIVATOR_TO_TARGET: i32 = 13;
    /// `ACSF_GetActorViewHeight`
    pub const GET_ACTOR_VIEW_HEIGHT: i32 = 14;
    /// `ACSF_GetChar`
    pub const GET_CHAR: i32 = 15;
    /// `ACSF_GetAirSupply`
    pub const GET_AIR_SUPPLY: i32 = 16;
    /// `ACSF_SetAirSupply`
    pub const SET_AIR_SUPPLY: i32 = 17;
    /// `ACSF_SetSkyScrollSpeed`
    pub const SET_SKY_SCROLL_SPEED: i32 = 18;
    /// `ACSF_GetArmorType`
    pub const GET_ARMOR_TYPE: i32 = 19;
    /// `ACSF_SpawnSpotForced`
    pub const SPAWN_SPOT_FORCED: i32 = 20;
    /// `ACSF_SpawnSpotFacingForced`
    pub const SPAWN_SPOT_FACING_FORCED: i32 = 21;
    /// `ACSF_CheckActorProperty`
    pub const CHECK_ACTOR_PROPERTY: i32 = 22;
    /// `ACSF_SetActorVelocity`
    pub const SET_ACTOR_VELOCITY: i32 = 23;
    /// `ACSF_SetUserVariable`
    pub const SET_USER_VAR: i32 = 24;
    /// `ACSF_GetUserVariable`
    pub const GET_USER_VAR: i32 = 25;
    /// `ACSF_Radius_Quake2`
    pub const RADIUS_QUAKE2: i32 = 26;
    /// `ACSF_CheckActorClass`
    pub const CHECK_ACTOR_CLASS: i32 = 27;
    /// `ACSF_SetUserArray`
    pub const SET_USER_ARRAY: i32 = 28;
    /// `ACSF_GetUserArray`
    pub const GET_USER_ARRAY: i32 = 29;
    /// `ACSF_SoundSequenceOnActor`
    pub const SOUND_SEQUENCE_ON_ACTOR: i32 = 30;
    /// `ACSF_SoundSequenceOnSector`
    pub const SOUND_SEQUENCE_ON_SECTOR: i32 = 31;
    /// `ACSF_SoundSequenceOnPolyobj`
    pub const SOUND_SEQUENCE_ON_POLYOBJ: i32 = 32;
    /// `ACSF_GetPolyobjX`
    pub const GET_POLYOBJ_X: i32 = 33;
    /// `ACSF_GetPolyobjY`
    pub const GET_POLYOBJ_Y: i32 = 34;
    /// `ACSF_CheckSight`
    pub const CHECK_SIGHT: i32 = 35;
    /// `ACSF_SpawnForced`
    pub const SPAWN_FORCED: i32 = 36;
    /// `ACSF_AnnouncerSound` (Skulltag)
    pub const ANNOUNCER_SOUND: i32 = 37;
    /// `ACSF_SetPointer`
    pub const SET_POINTER: i32 = 38;
    /// `ACSF_ACS_NamedExecute`
    pub const ACS_NAMED_EXECUTE: i32 = 39;
    /// `ACSF_ACS_NamedSuspend`
    pub const ACS_NAMED_SUSPEND: i32 = 40;
    /// `ACSF_ACS_NamedTerminate`
    pub const ACS_NAMED_TERMINATE: i32 = 41;
    /// `ACSF_ACS_NamedLockedExecute` (lock check deferred — same as NamedExecute).
    pub const ACS_NAMED_LOCKED_EXECUTE: i32 = 42;
    /// `ACSF_ACS_NamedLockedExecuteDoor` (lock check deferred — same as NamedExecute).
    pub const ACS_NAMED_LOCKED_EXECUTE_DOOR: i32 = 43;
    /// `ACSF_ACS_NamedExecuteWithResult`
    pub const ACS_NAMED_EXECUTE_WITH_RESULT: i32 = 44;
    /// `ACSF_ACS_NamedExecuteAlways`
    pub const ACS_NAMED_EXECUTE_ALWAYS: i32 = 45;
    /// `ACSF_UniqueTID`
    pub const UNIQUE_TID: i32 = 46;
    /// `ACSF_IsTIDUsed`
    pub const IS_TID_USED: i32 = 47;
    /// `ACSF_Sqrt`
    pub const SQRT: i32 = 48;
    /// `ACSF_FixedSqrt`
    pub const FIXED_SQRT: i32 = 49;
    /// `ACSF_VectorLength`
    pub const VECTOR_LENGTH: i32 = 50;
    /// `ACSF_SetHUDClipRect`
    pub const SET_HUD_CLIP_RECT: i32 = 51;
    /// `ACSF_SetHUDWrapWidth`
    pub const SET_HUD_WRAP_WIDTH: i32 = 52;
    /// `ACSF_SetCVar`
    pub const SET_CVAR: i32 = 53;
    /// `ACSF_GetUserCVar`
    pub const GET_USER_CVAR: i32 = 54;
    /// `ACSF_SetUserCVar`
    pub const SET_USER_CVAR: i32 = 55;
    /// `ACSF_GetCVarString`
    pub const GET_CVAR_STRING: i32 = 56;
    /// `ACSF_SetCVarString`
    pub const SET_CVAR_STRING: i32 = 57;
    /// `ACSF_GetUserCVarString`
    pub const GET_USER_CVAR_STRING: i32 = 58;
    /// `ACSF_SetUserCVarString`
    pub const SET_USER_CVAR_STRING: i32 = 59;
    /// `ACSF_LineAttack`
    pub const LINE_ATTACK: i32 = 60;
    /// `ACSF_PlaySound`
    pub const PLAY_SOUND: i32 = 61;
    /// `ACSF_StopSound`
    pub const STOP_SOUND: i32 = 62;
    /// `ACSF_SoundVolume`
    pub const SOUND_VOLUME: i32 = 70;
    /// `ACSF_PlayActorSound`
    pub const PLAY_ACTOR_SOUND: i32 = 71;
    /// `ACSF_SpawnDecal`
    pub const SPAWN_DECAL: i32 = 72;
    /// `ACSF_GetArmorInfo`
    pub const GET_ARMOR_INFO: i32 = 81;
    /// `ACSF_DropInventory`
    pub const DROP_INVENTORY: i32 = 82;
    /// `ACSF_strcmp`
    pub const STRCMP: i32 = 63;
    /// `ACSF_stricmp`
    pub const STRICMP: i32 = 64;
    /// `ACSF_StrLeft`
    pub const STR_LEFT: i32 = 65;
    /// `ACSF_StrRight`
    pub const STR_RIGHT: i32 = 66;
    /// `ACSF_StrMid`
    pub const STR_MID: i32 = 67;
    /// `ACSF_GetActorClass`
    pub const GET_ACTOR_CLASS: i32 = 68;
    /// `ACSF_GetWeapon`
    pub const GET_WEAPON: i32 = 69;
    /// `ACSF_CheckFont`
    pub const CHECK_FONT: i32 = 73;
    /// `ACSF_DropItem`
    pub const DROP_ITEM: i32 = 74;
    /// `ACSF_CheckFlag`
    pub const CHECK_FLAG: i32 = 75;
    /// `ACSF_SetLineActivation`
    pub const SET_LINE_ACTIVATION: i32 = 76;
    /// `ACSF_GetLineActivation`
    pub const GET_LINE_ACTIVATION: i32 = 77;
    /// `ACSF_GetActorPowerupTics`
    pub const GET_ACTOR_POWERUP_TICS: i32 = 78;
    /// `ACSF_ChangeActorAngle`
    pub const CHANGE_ACTOR_ANGLE: i32 = 79;
    /// `ACSF_ChangeActorPitch`
    pub const CHANGE_ACTOR_PITCH: i32 = 80;
    /// `ACSF_PickActor`
    pub const PICK_ACTOR: i32 = 83;
    /// `ACSF_IsPointerEqual`
    pub const IS_POINTER_EQUAL: i32 = 84;
    /// `ACSF_CanRaiseActor`
    pub const CAN_RAISE_ACTOR: i32 = 85;
    /// `ACSF_SetActorTeleFog`
    pub const SET_ACTOR_TELE_FOG: i32 = 86;
    /// `ACSF_SwapActorTeleFog`
    pub const SWAP_ACTOR_TELE_FOG: i32 = 87;
    /// `ACSF_SetActorRoll`
    pub const SET_ACTOR_ROLL: i32 = 88;
    /// `ACSF_ChangeActorRoll`
    pub const CHANGE_ACTOR_ROLL: i32 = 89;
    /// `ACSF_GetActorRoll`
    pub const GET_ACTOR_ROLL: i32 = 90;
    /// `ACSF_QuakeEx`
    pub const QUAKE_EX: i32 = 91;
    /// `ACSF_Warp`
    pub const WARP: i32 = 92;
    /// `ACSF_GetMaxInventory`
    pub const GET_MAX_INVENTORY: i32 = 93;
    /// `ACSF_SetSectorDamage`
    pub const SET_SECTOR_DAMAGE: i32 = 94;
    /// `ACSF_SetSectorTerrain`
    pub const SET_SECTOR_TERRAIN: i32 = 95;
    /// `ACSF_SpawnParticle`
    pub const SPAWN_PARTICLE: i32 = 96;
    /// `ACSF_SetMusicVolume`
    pub const SET_MUSIC_VOLUME: i32 = 97;
    /// `ACSF_CheckProximity`
    pub const CHECK_PROXIMITY: i32 = 98;
    /// `ACSF_CheckActorState`
    pub const CHECK_ACTOR_STATE: i32 = 99;
    /// `ACSF_CheckClass`
    pub const CHECK_CLASS: i32 = 200;
    /// `ACSF_DamageActor`
    pub const DAMAGE_ACTOR: i32 = 201;
    /// `ACSF_SetActorFlag`
    pub const SET_ACTOR_FLAG: i32 = 202;
    /// `ACSF_SetTranslation`
    pub const SET_TRANSLATION: i32 = 203;
    /// `ACSF_GetActorFloorTexture`
    pub const GET_ACTOR_FLOOR_TEXTURE: i32 = 204;
    /// `ACSF_GetActorFloorTerrain`
    pub const GET_ACTOR_FLOOR_TERRAIN: i32 = 205;
    /// `ACSF_StrArg`
    pub const STR_ARG: i32 = 206;
    /// `ACSF_Floor` (fixed → int toward −∞)
    pub const FLOOR: i32 = 207;
    /// `ACSF_Round`
    pub const ROUND: i32 = 208;
    /// `ACSF_Ceil`
    pub const CEIL: i32 = 209;
    /// `ACSF_ScriptCall` — ZScript bridge (bootstrap no-op).
    pub const SCRIPT_CALL: i32 = 210;
    /// `ACSF_StartSlideshow`
    pub const START_SLIDESHOW: i32 = 211;
    /// `ACSF_GetSectorHealth`
    pub const GET_SECTOR_HEALTH: i32 = 212;
    /// `ACSF_GetLineHealth`
    pub const GET_LINE_HEALTH: i32 = 213;
    /// `ACSF_SetSubtitleNumber`
    pub const SET_SUBTITLE_NUMBER: i32 = 214;
    /// `ACSF_GetNetID`
    pub const GET_NET_ID: i32 = 215;
    /// `ACSF_SetActivatorByNetID`
    pub const SET_ACTIVATOR_BY_NET_ID: i32 = 216;
    /// `ACSF_GetLineX` (Eternity)
    pub const GET_LINE_X: i32 = 300;
    /// `ACSF_GetLineY` (Eternity)
    pub const GET_LINE_Y: i32 = 301;
    /// `ACSF_SetSectorGlow`
    pub const SET_SECTOR_GLOW: i32 = 400;
    /// `ACSF_SetFogDensity`
    pub const SET_FOG_DENSITY: i32 = 401;
    /// `ACSF_GetTeamScore` (ZDaemon)
    pub const GET_TEAM_SCORE: i32 = 19620;
    /// `ACSF_SetTeamScore` (ZDaemon)
    pub const SET_TEAM_SCORE: i32 = 19621;
}

/// `GetSectorHealth` part selectors (`SECPART_*`).
pub mod secpart {
    /// Floor.
    pub const FLOOR: i32 = 0;
    /// Ceiling.
    pub const CEILING: i32 = 1;
    /// 3D floor.
    pub const THREE_D: i32 = 2;
}

/// `GetLevelInfo` selectors (`LEVELINFO_*`).
pub mod levelinfo {
    /// Par time (seconds); 0 if unset.
    pub const PAR_TIME: i32 = 0;
    /// Suck time; 0 if unset.
    pub const SUCK_TIME: i32 = 1;
    /// Cluster number; 0 if unset.
    pub const CLUSTERNUM: i32 = 2;
    /// Level number; 0 if unset.
    pub const LEVELNUM: i32 = 3;
    /// Total secrets.
    pub const TOTAL_SECRETS: i32 = 4;
    /// Found secrets.
    pub const FOUND_SECRETS: i32 = 5;
    /// Total items.
    pub const TOTAL_ITEMS: i32 = 6;
    /// Found items.
    pub const FOUND_ITEMS: i32 = 7;
    /// Total monsters.
    pub const TOTAL_MONSTERS: i32 = 8;
    /// Killed monsters.
    pub const KILLED_MONSTERS: i32 = 9;
}

/// `APROP_*` for `GetActorProperty` / `SetActorProperty`.
pub mod aprop {
    /// Health.
    pub const HEALTH: i32 = 0;
    /// Speed (fixed).
    pub const SPEED: i32 = 1;
    /// Damage.
    pub const DAMAGE: i32 = 2;
    /// Alpha (fixed).
    pub const ALPHA: i32 = 3;
    /// Render style (`STYLE_*` ACS constants).
    pub const RENDERSTYLE: i32 = 4;
    /// See sound (set-only string).
    pub const SEESOUND: i32 = 5;
    /// Attack sound (set-only string).
    pub const ATTACKSOUND: i32 = 6;
    /// Pain sound (set-only string).
    pub const PAINSOUND: i32 = 7;
    /// Death sound (set-only string).
    pub const DEATHSOUND: i32 = 8;
    /// Active sound (set-only string).
    pub const ACTIVESOUND: i32 = 9;
    /// Ambush flag.
    pub const AMBUSH: i32 = 10;
    /// Invulnerable flag.
    pub const INVULNERABLE: i32 = 11;
    /// Jump Z impulse (fixed).
    pub const JUMPZ: i32 = 12;
    /// Chase goal instead of target.
    pub const CHASEGOAL: i32 = 13;
    /// Frightened (flee) flag.
    pub const FRIGHTENED: i32 = 14;
    /// Gravity (fixed).
    pub const GRAVITY: i32 = 15;
    /// Friendly flag.
    pub const FRIENDLY: i32 = 16;
    /// Spawn health (class default; read-only).
    pub const SPAWNHEALTH: i32 = 17;
    /// Dropped flag.
    pub const DROPPED: i32 = 18;
    /// Notarget flag.
    pub const NOTARGET: i32 = 19;
    /// Species name (string).
    pub const SPECIES: i32 = 20;
    /// Display / tag name (string).
    pub const NAMETAG: i32 = 21;
    /// Score.
    pub const SCORE: i32 = 22;
    /// NoTrigger — ignore line activation from this actor.
    pub const NOTRIGGER: i32 = 23;
    /// Damage factor (fixed).
    pub const DAMAGEFACTOR: i32 = 24;
    /// Damage multiplier (fixed; same field as damage factor for bootstrap).
    pub const DAMAGEMULTIPLIER: i32 = 43;
    /// Scale X (fixed).
    pub const SCALEX: i32 = 29;
    /// Scale Y (fixed).
    pub const SCALEY: i32 = 30;
    /// Master TID.
    pub const MASTER_TID: i32 = 25;
    /// Target TID.
    pub const TARGET_TID: i32 = 26;
    /// Tracer TID.
    pub const TRACER_TID: i32 = 27;
    /// Water immersion level 0..=3 (read-only; bootstrap returns 0).
    pub const WATERLEVEL: i32 = 28;
    /// Dormant flag.
    pub const DORMANT: i32 = 31;
    /// Mass.
    pub const MASS: i32 = 32;
    /// Accuracy.
    pub const ACCURACY: i32 = 33;
    /// Stamina.
    pub const STAMINA: i32 = 34;
    /// Height (fixed).
    pub const HEIGHT: i32 = 35;
    /// Radius (fixed).
    pub const RADIUS: i32 = 36;
    /// Reaction time.
    pub const REACTIONTIME: i32 = 37;
    /// Melee range (fixed).
    pub const MELEERANGE: i32 = 38;
    /// View height (fixed).
    pub const VIEWHEIGHT: i32 = 39;
    /// Attack Z offset (fixed).
    pub const ATTACKZOFFSET: i32 = 40;
    /// Friction (fixed).
    pub const FRICTION: i32 = 42;
    /// Max step height (fixed).
    pub const MAXSTEPHEIGHT: i32 = 44;
    /// Max dropoff height (fixed).
    pub const MAXDROPOFFHEIGHT: i32 = 45;
    /// Damage type name (string).
    pub const DAMAGETYPE: i32 = 46;
    /// Sound class prefix (string).
    pub const SOUNDCLASS: i32 = 47;
    /// Friendly see distance in blockmap blocks (default 10).
    pub const FRIENDLYSEEBLOCKS: i32 = 48;
    /// Stencil / fill colour as 0xRRGGBB (0 = unset).
    pub const STENCILCOLOR: i32 = 41;
}

/// Runaway instruction limit per tic (GZDoom `p_acs.cpp`).
pub const RUNAWAY_LIMIT: u32 = 2_000_000;

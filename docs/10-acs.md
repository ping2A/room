# 10 — ACS virtual machine (`room-acs`)

GZDoom references: `src/playsim/p_acs.cpp` (≈ 10k lines; VM, loader, ACSF functions),
`p_acs.h`, ZDoom wiki *ACS*, *Built-in ACS functions*, ACC source (`acc` repo:
`pcdefs.c`, `pcdefs.h` for p-code numbering), ZDoom wiki *ACS Script types*.

ACS is compiled bytecode (`BEHAVIOR` lumps in maps, libraries in the `A_START/A_END`
namespace / `acs/` directory loaded via `LOADACS` or `#import`). Room runs
existing bytecode; it does not need an ACS compiler (ACC/GDCC remain external tools).

## 1. Object formats

| Format | Header | Notes |
|---|---|---|
| ACS0 | `"ACS\0"`, `u32 dir_offset` | Hexen. Directory: `u32 numscripts`, entries `(u32 number_and_type, u32 offset, u32 argc)` (number ≥ 1000 → OPEN script = number − 1000 with type Open), then `u32 numstrings`, string offsets |
| ACSE | `"ACSE"`, `u32 dir_offset` | ZDoom chunked format, 4-byte p-codes |
| ACSe | `"ACSe"` | Same chunks, **compressed p-codes**: opcode < 240 is 1 byte; ≥ 240 is two bytes (`240 + (b2)`); some arguments packed into bytes (`PushByte`, `LSPEC1Direct` byte args) |
| Hybrid | ACS0 header whose directory points into a chunk area (`ACSE`/`ACSe` marker before `dir_offset`) | old ACC output for Hexen-compatible + ZDoom features |

Chunks (all `[u8;4] id, u32 len, data`):

| Chunk | Meaning |
|---|---|
| `SPTR` | script pointers: `(u16 number, u8 type, u8 argc, u32 offset)` (newer) or the 12-byte variant (older ACC) — detect by size |
| `SFLG` | script flags (`NET`, `CLIENTSIDE`) |
| `SVCT` | script local var counts (default 20) |
| `SNAM` | named scripts (negative numbers: −1, −2 … map to names) |
| `STRL` / `STRE` | string table (STRE = "encrypted": XOR with `offset * 157135 + i/2`) |
| `LOAD` | library names to load |
| `FUNC` / `FNAM` | functions (`u8 argc, u8 localcount, u8 hasreturn, u8 importnum, u32 address`), names |
| `MINI` | map var initializers |
| `ARAY` / `AINI` | map arrays and initializers |
| `AIMP` / `MIMP` | imported arrays / imported map vars |
| `MEXP` | exported map var names |
| `ASTR` / `MSTR` | map arrays / map vars that hold strings (for string-pool tagging) |
| `ATAG` | array element tagging (strings / functions) |
| `JUMP` | dynamic jump targets |

## 2. Runtime model

```rust
pub struct AcsState {                  // lives in World, serializable
    pub modules: Vec<AcsModuleState>,  // loaded BEHAVIOR + libraries for this map (bytecode is in Definitions / MapGeometry, shared)
    pub scripts: Vec<RunningScript>,   // DACSThinker's list; ordering is significant (run order)
    pub map_vars: per module [i32; 128] + arrays,
    pub world_vars: [i32; 256], pub world_arrays: [SparseArray; 256],   // persist across a hub
    pub global_vars: [i32; 64], pub global_arrays: [SparseArray; 64],   // persist across the whole game
    pub strings: AcsStringPool,        // dynamic strings with refcount + "lock" semantics (GZDoom ACSStringPool)
    pub queued_starts: Vec<AcsStart>,  // for maps not yet loaded (ACS_Execute with map arg → stored per map, run on enter)
}

pub struct RunningScript {
    pub module: AcsModuleId, pub number: ScriptNumber, pub pc: u32,
    pub state: ScriptState,            // Running, Suspended, Delayed(tics), TagWait(tag), PolyWait(po), ScriptWait(n), Terminated, PleaseRemove
    pub stack: SmallVec<[i32; 64]>, pub locals: SmallVec<[i32; 20]>, pub local_arrays: …,
    pub frames: Vec<CallFrame>,        // CALL/RETURN for ACS functions
    pub activator: Option<ActorId>, pub line: Option<LineId>, pub side: u8, pub backside: bool,
    pub font: Option<FontId>, pub hud_size: (i32, i32, bool), pub print_buf: String, pub client_side: bool,
}
```

### 2.1 Script types

`Closed (0), Open (1), Respawn (2), Death (3), Enter (4), Pickup (5), BlueReturn (6), RedReturn (7), WhiteReturn (8), Lightning (12), Unloading (13), Disconnect (14), Return (15), Event (16), Kill (17), Reopen (18)`. Verify values against GZDoom's `EACSScriptType` / ACC `zdefs.acs`. Event dispatch:

| Event | Trigger |
|---|---|
| Open | map start (first tic, after `PostBeginPlay`) |
| Enter | each player entering the map (after Open), and on respawn in coop for new players |
| Return | player returning to a hub map |
| Respawn / Death | player respawn/death |
| Lightning | each lightning flash |
| Unloading | map exit (runs immediately, can't delay) |
| Disconnect | player leaves (arg: player number) |
| Kill | monster death when `MF4_…`/`sv_` setting or MAPINFO enables it |
| Event | ACS `EVENT` script for `GAMEEVENT_*` |
| Reopen | when a map is re-entered (GZDoom 4.x) |

### 2.2 Execution

- ACS runs as one thinker at `STAT_SCRIPTS`; each tic it iterates running scripts in list order and executes each until it yields (delay, wait, suspend, terminate).
- **Runaway guard**: if a script executes more than 2,000,000 instructions (take the exact limit from `p_acs.cpp` and keep it a named constant) in one tic without yielding, terminate it with the "Runaway script N terminated" message.
- `ACS_Execute` on an already running script does nothing (unless `ACS_ExecuteAlways`); on a suspended script it resumes it.
- `ACS_ExecuteWithResult` runs the script synchronously until it ends or yields; result is `SetResultValue`.
- Scripts started during a tic begin executing **in the same tic** if started by a line/thing (GZDoom runs them immediately up to the first delay — `P_StartScript` with `always`/`wantresultcode` semantics; check `DLevelScript::DLevelScript` and `RunScript` placement). Port exactly; a subtle source of map behaviour differences.
- Waiting: `TagWait(tag)` waits until no sector with that tag has active floor/ceiling/light movers; `PolyWait(po)` until polyobject stops; `ScriptWait(n)` until script n terminates (including not-yet-started bookkeeping).

### 2.3 P-codes

Port the `PCD_*` enum exactly (numbering is part of the binary format; ~380 entries in GZDoom). Implementation is a `match` in a tight loop over a pre-decoded instruction stream: at load, decode ACSE/ACSe bytes into `Vec<Insn>` (`enum Insn { PushNumber(i32), LSpec{special:u8, nargs:u8}, LSpecDirect{special, args:[i32;5]}, Add, … }`) and remap jump targets from byte offsets to instruction indices. Keep a byte-offset→index map for `JUMP` tables and saves (saves store byte offsets for compatibility of the save format across decoder changes).

Major groups:

- Stack/arith: push/drop/dup/swap, `ADD SUBTRACT MULTIPLY DIVIDE MODULUS` (division by zero → terminate script with error "Division by zero", like GZDoom), unary/binary bitwise and logical ops, `FIXEDMUL`, `FIXEDDIV`, increments/decrements on all var scopes, array ops (`PUSHMAPARRAY`, `ASSIGNWORLDARRAY` …).
- Control: `GOTO`, `IFGOTO`, `IFNOTGOTO`, `CASEGOTO`, `CASEGOTOSORTED` (binary search table), `CALL`, `CALLDISCARD`, `RETURNVOID`, `RETURNVAL`, `TERMINATE`, `SUSPEND`, `RESTART`, `DELAY`, `DELAYDIRECT(B)`, `TAGWAIT(DIRECT)`, `POLYWAIT(DIRECT)`, `SCRIPTWAIT(DIRECT|NAMED)`.
- Specials: `LSPEC1..5`, `LSPEC1..5DIRECT(B)`, `LSPEC5RESULT`, `LSPEC5EX`, `LSPEC5EXRESULT`.
- Printing/HUD: `BEGINPRINT`, `PRINTSTRING/NUMBER/CHARACTER/FIXED/NAME/LOCALIZED/BINARY/HEX/MAPCHARARRAY…`, `ENDPRINT`, `ENDPRINTBOLD`, `ENDLOG`, `MOREHUDMESSAGE`, `OPTHUDMESSAGE`, `ENDHUDMESSAGE(BOLD)`, `SETFONT(DIRECT)`, `SAVESTRING`, `STRCPYTO*`.
- World queries: `PLAYERCOUNT`, `GAMETYPE`, `GAMESKILL`, `TIMER`, `SECTORSOUND`, `AMBIENTSOUND`, `SOUNDSEQUENCE`, `SETLINETEXTURE`, `SETLINEBLOCKING`, `SETLINESPECIAL`, `THINGSOUND`, `ACTIVATORSOUND`, `LOCALAMBIENTSOUND`, `SETLINEMONSTERBLOCKING`, `THINGCOUNT(DIRECT|NAME)`, `SPAWN(SPOT)(DIRECT)`, `CLEARINVENTORY`, `GIVEINVENTORY(DIRECT)`, `TAKEINVENTORY(DIRECT)`, `CHECKINVENTORY(DIRECT)`, `SETMUSIC`, `LOCALSETMUSIC`, `SETSTYLE`, `FADETO`, `FADERANGE`, `CANCELFADE`, `PLAYMOVIE`, `SETFLOORTRIGGER`, `SETCEILINGTRIGGER`, `GETACTORX/Y/Z`, `GETACTORANGLE/PITCH`, `GETACTORFLOORZ/CEILINGZ`, `GETACTORLIGHTLEVEL`, `GETSECTORFLOORZ/CEILINGZ`, `GETSECTORLIGHTLEVEL`, `SETACTORPOSITION`, `GETACTORPROPERTY`, `SETACTORPROPERTY`, `SETACTORSTATE`, `CHECKACTORPROPERTY`, `PLAYERNUMBER`, `ACTIVATORTID`, `GETSCREENWIDTH/HEIGHT`, `THING_PROJECTILE2`, `STRLEN`, `SETHUDSIZE`, `GETCVAR`, `SETRESULTVALUE`, `GETLINEROWOFFSET`, `CLASSIFYACTOR`, `CHECKWEAPON`, `SETWEAPON`, `TAGSTRING`, `SETMARINEWEAPON`, `SETMARINESPRITE`, `PLAYERINGAME`, `PLAYERISBOT`, `SETCAMERATOTEXTURE`, `SETTHINGSPECIAL`, `SETAIRCONTROL`, `GETAIRSUPPLY`, `SETAIRSUPPLY`, `SETGRAVITY`, `CHANGESKY`, … — the exact list is the enum.
- `CALLFUNC(argc, funcindex)`: the **ACSF** extension table (~250 functions): `GetLineUDMFInt/Fixed`, `GetThingUDMFInt/Fixed`, `GetSectorUDMFInt/Fixed`, `GetSideUDMFInt/Fixed`, `GetActorVelX/Y/Z`, `SetActivator`, `SetActivatorToTarget`, `GetActorViewHeight`, `GetChar`, `GetAirSupply`, `SetAirSupply`, `SetSkyScrollSpeed`, `GetArmorType`, `SpawnSpotForced`, `SpawnSpotFacingForced`, `CheckActorProperty`, `SetActorVelocity`, `SetUserVariable`, `GetUserVariable`, `Radius_Quake2`, `CheckActorClass`, `SetUserArray`, `GetUserArray`, `SoundSequenceOnActor/Sector/Polyobj`, `GetPolyobjX/Y`, `CheckSight`, `SpawnForced`, `AnnouncerSound`, `SetPointer`, `ACS_NamedExecute*` family, `UniqueTID`, `IsTIDUsed`, `Sqrt`, `FixedSqrt`, `VectorLength`, `SetHUDClipRect`, `SetHUDWrapWidth`, `SetCVar`, `GetUserCVar`, `SetUserCVar`, `GetCVarString`, `SetCVarString`, `GetUserCVarString`, `SetUserCVarString`, `LineAttack`, `PlaySound`, `StopSound`, `strcmp`, `stricmp`, `StrLeft`, `StrRight`, `StrMid`, `GetActorClass`, `GetWeapon`, `SoundVolume`, `PlayActorSound`, `SpawnDecal`, `CheckFont`, `DropItem`, `CheckFlag`, `SetLineActivation`, `GetLineActivation`, `GetActorPowerupTics`, `ChangeActorAngle/Pitch/Roll`, `GetArmorInfo`, `DropInventory`, `PickActor`, `IsPointerEqual`, `CanRaiseActor`, `SetActorTeleFog`, `SwapActorTeleFog`, `SetActorRoll`, `GetActorRoll`, `QuakeEx`, `Warp`, `GetMaxInventory`, `SetSectorDamage`, `SetSectorTerrain`, `SpawnParticle`, `SetMusicVolume`, `CheckProximity`, `CheckActorState`, `CheckClass`, `GetActorFloorTexture`, `GetActorFloorTerrain`, `StrArg`, `Floor`, `Round`, `Ceil`, `ScriptCall`, `StartSlideshow`, `GetSectorHealth`, `GetLineHealth`, `SetSubtitleNumber`, `GetLevelInfo`, `GetPlayerInfo`, `ChangeLevel`, `SectorDamage`, `ReplaceTextures`, `NamedScriptWait`, `SetMugShotState`, `SetPlayerProperty`, `PlayerClass`, `GetPlayerInput`, `ClassifyActor`, `SetFont`, `Thing_Damage2`, … — port `ACSF_*` enum and implementations from `p_acs.cpp`.

`GetActorProperty`/`SetActorProperty`/`CheckActorProperty` support every `APROP_*` constant (Health, Speed, Damage, Alpha, RenderStyle, SeeSound, AttackSound, PainSound, DeathSound, ActiveSound, Ambush, Invulnerable, JumpZ, ChaseGoal, Frightened, Gravity, Friendly, SpawnHealth, Dropped, Notarget, Species, NameTag, Score, Notrigger, DamageFactor, MasterTID, TargetTID, TracerTID, WaterLevel, ScaleX, ScaleY, Dormant, Mass, Accuracy, Stamina, Height, Radius, ReactionTime, MeleeRange, ViewHeight, AttackZOffset, StencilColor, Friction, DamageMultiplier, MaxStepHeight, MaxDropOffHeight, DamageType, SoundClass, FriendlySeeBlocks, …).

### 2.4 Strings

ACS strings are integers. Module strings (`STRL`) are encoded as indices tagged with the module number in the high bits (GZDoom `(module_index << 16) | string_index`). Dynamic strings (from `StrParam`, `StrLeft`, etc.) live in `AcsStringPool` with reference tracking: strings stored in map/world/global vars and arrays are "locked" (tagged by `ASTR`/`MSTR`/`ATAG`), others are collected at end of each tic if unreferenced by any running script stack/locals. Port GZDoom's string-pool algorithm (`ACSStringPool::LockStringForScript`, `PurgeStrings`, `MarkStringArray`).

### 2.5 Fixed-point

ACS fixed numbers are 16.16 `i32`. Conversions to engine `f64` use exact `x / 65536.0`. ACS math functions (`sin`, `cos`, `VectorAngle`) use the deterministic tables/routines GZDoom uses (`finesine`-based or `g_sin` on doubles then converted — check each; determinism rules apply).

## 3. Integration with modules

- `ScriptCall("ClassName", "FunctionName", args…)` (ACSF) calls a **static exported function** of a WASM module whose class is `ClassName` (see `12 §6` `reg_static_function` and `12 §9`). Arguments and return are `i32`; strings are passed as ACS string handles converted through the host.
- Modules can start ACS scripts (`acs_execute`, `acs_named_execute`, `acs_execute_with_result`) and read/write map/world/global vars by index.

## 4. Printing and HUD messages

ACS print p-codes produce `UiEvent::Print { text, bold, log, player_target }` and `UiEvent::HudMessage { id, type (Plain, FadeOut, TypeOn, FadeInOut), x, y, hold, extra…, font, color, alpha, layer, wrap_width, clip_rect, hud_size, flags (HUDMSG_LOG, HUDMSG_COLORSTRING, HUDMSG_ADDBLEND, HUDMSG_ALPHA, HUDMSG_NOWRAP, HUDMSG_LAYER_*, HUDMSG_NOTWITHFULLMAP, HUDMSG_NOTWITHOVERLAYMAP, HUDMSG_NOTWITH3DVIEW) }`. These are sim outputs consumed by the UI (and recorded in the save game, since HUD messages persist in GZDoom saves).

## 5. Persistence and netplay

- Serialize `AcsState` fully. Bytecode isn't saved; saves store module identity (lump name + MD5).
- `NET` scripts may be triggered from console (`puke`, `pukename`) through `NetCommand::Puke`; non-NET scripts are rejected in multiplayer.
- `CLIENTSIDE` scripts (Zandronum) — ignored flag, run normally.

## 6. Tests

- Don't compile fixtures with ACC/BCC in CI: ACC is C and must not become a build dependency. Instead commit pre-compiled `.o` fixtures plus their `.acs` sources under `testdata/acs/`, regenerated by a documented manual script.
- One fixture per p-code group and per ACSF group, asserting results via `SetResultValue` and `Log` capture.
- String pool stress test; save/load mid-script test; runaway termination test.

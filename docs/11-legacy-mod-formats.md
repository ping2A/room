# 11 — Legacy modding formats: DECORATE, DeHackEd, ZScript policy

## 1. DECORATE (`ferrite-decorate`)

GZDoom references: `src/scripting/decorate/thingdef_parse.cpp`, `thingdef_states.cpp`,
`thingdef_exp.cpp`, `src/scripting/thingdef_properties.cpp` (property table),
`src/scripting/thingdef_data.cpp` (flag table), ZDoom wiki *DECORATE*,
*DECORATE expressions*, *Actor properties*, *Actor flags*, *Actor states*.

### 1.1 Output

DECORATE compiles to a `ClassDefIr` list that `ferrite-defs` merges into the
`ClassRegistry`. Built-in classes (`ferrite.pk3`) use the same path.

### 1.2 Grammar (summary)

```
decorate      := (include | const_decl | enum_decl | actor_def | damagetype_def)*
include       := '#include' STRING
const_decl    := 'const' ('int'|'float') IDENT '=' expr ';'
enum_decl     := 'enum' '{' IDENT ('=' expr)? (',' IDENT ('=' expr)?)* ','? '}' ';'?
actor_def     := 'actor' NAME (':' NAME)? ('replaces' NAME)? INT? 'native'? '{' actor_body* '}'
actor_body    := property | flag | 'states' states_block | var_decl | const_decl | enum_decl
               | action_decl /* only 'action native' in ferrite.pk3 */
               | 'skip_super' | 'clearflags' | 'monster' | 'projectile' | 'game' NAME | 'spawnid' INT | 'conversationid' INT (',' INT ',' INT)?
flag          := ('+'|'-') FLAGNAME            // FLAGNAME may be qualified: INVENTORY.AUTOACTIVATE, WEAPON.NOAUTOFIRE
property      := PROPNAME args                 // PROPNAME may be qualified: Inventory.Amount, Player.ColorRange …
var_decl      := 'var' ('int'|'float') IDENT ('[' INT ']')? ';'    // name must start with user_
states_block  := '{' (label ':' | state_line | flow)* '}'
label         := IDENT ('.' IDENT)*            // Death.Fire, Pain.Ice
state_line    := SPRITE FRAMES DURATION modifiers* action? ';'?
SPRITE        := 4 chars or '####' / '----'    FRAMES := letters or '#'  (quoted "frames" allowed)
DURATION      := INT | 'random' '(' INT ',' INT ')' | expr-in-parentheses (constant)
modifiers     := 'bright' | 'fast' | 'slow' | 'nodelay' | 'canraise' | 'light' '(' STRING (',' STRING)* ')' | 'offset' '(' expr ',' expr ')'
action        := IDENT ('(' args ')')? | '{' anon_statements '}'
flow          := 'loop' | 'stop' | 'wait' | 'fail' | 'goto' label_ref ('+' INT)?
label_ref     := ('Super' '::' | CLASSNAME '::')? label
```

- Properties: each has a typed signature (table ported from `thingdef_properties.cpp`), e.g. `DropItem "Clip" [, probability [, amount]]` (accumulates; a child's first `DropItem` clears inherited drops), `DamageFactor ["type",] f`, `PainChance ["type",] n`, `Player.WeaponSlot n, "A", "B"`, `Player.ColorRange a, b`, `Player.ColorSet n, "name", start, end, color …`, `Player.StartItem "class" [, amount]`, `Inventory.PickupMessage "s"`, `Weapon.AmmoType "c"`, `Powerup.Color r g b [, a]` or `"name"`, `RenderStyle Add|Translucent|…`, `Translation "a:b=c:d" | n`, `BloodColor`, `BloodType "a" [,"b" [,"c"]]`, `Damage n | (expr)` (expression damage evaluated each hit), `Health`, `Mass`, `Speed`, `Tag "$LOCALIZED"`, `Obituary`, `HitObituary`, `Scale`, `XScale/YScale`, `Alpha`, `MaxTargetRange`, `MeleeRange`, `MeleeThreshold`, `MinMissileChance`, `Gravity`, `Friction`, `DesignatedTeam`, `VisibleAngles`, `VisiblePitch`, `DefThreshold`, `Threshold`, `FloatBobStrength`, `CameraHeight`, `CameraFOV`, `Radius`, `Height`, `DeathHeight`, `BurnHeight`, `ProjectilePassHeight`, `Stamina`, `Accuracy`, `RipperLevel`, `RipLevelMin/Max`, `StencilColor`, `TeleFogSourceType/DestType`, `Species`, `Activation`, `DamageType`, `PoisonDamage`, `PoisonDamageType`, `RadiusDamageFactor`, `SelfDamageFactor`, `StealthAlpha`, `WoundHealth`, `GibHealth`, `ReactionTime`, `PainThreshold`, `DontHurtShooter`, `ExplosionDamage`, `ExplosionRadius`, `FastSpeed`, `FloatSpeed`, `BounceType`, `BounceFactor`, `WallBounceFactor`, `BounceCount`, `BounceSound`, `WallBounceSound`, `CrushPainSound`, `ActiveSound`, `AttackSound`, `DeathSound`, `PainSound`, `SeeSound`, `HowlSound`, `Args`, `Monster`, `Projectile`, `ClearFlags`, `DistanceCheck "cvar"`, `MaxStepHeight`, `MaxDropoffHeight`, `JumpZ` (player), `Health.LowMessage`, `Armor.*`, `Ammo.BackpackAmount/MaxAmount/DropAmount`, `Morph.*`, `PowerSpeed.NoTrail`, `PuzzleItem.Number`, `PuzzleItem.FailMessage`, `HealthPickup.AutoUse`, `MapRevealer`, `FakeInventory.Respawns`, `Mass`, `AutomapOffsets`, `LightLevel`, `FriendlySeeBlocks`, `ShadowAimFactor`/`ShadowPenaltyFactor` (4.12+, S)… The complete list = everything in GZDoom's DECORATE property table. Unsupported properties produce a load error naming the property.
- Flags: table ported from `thingdef_data.cpp` (`ActorFlagDefs`, `InventoryFlagDefs`, `WeaponFlagDefs`, `PlayerPawnFlagDefs`, deprecated aliases like `+LOWGRAVITY`, `+FIREDAMAGE`, `+ICEDAMAGE`, `+QUARTERGRAVITY`, `+SHORTMISSILERANGE`, `+LONGMELEERANGE`, `+NONETID` – deprecated flags must still be accepted and mapped as GZDoom does).
- Expressions: C-like precedence, int/float/bool/string/name/state/class/sound/color types, built-in variables (`x, y, z, angle, pitch, roll, alpha, args[], health, velx/vely/velz, momx/momy/momz, scalex/scaley, mass, accuracy, stamina, height, radius, reactiontime, meleerange, speed, special, tid, tidtohate, waterlevel, threshold, damage, floorz, ceilingz, user_*`), functions (`random, random2, frandom, randompick, frandompick, sin, cos, sqrt, abs, min, max, clamp, CallACS, CountInv, GetDistance, GetAngle, GetZAt, GetCrouchFactor, GetCVar, GetPlayerInput, CheckClass, IsPointerEqual, GetSpawnHealth, GetGibHealth, GetMissileDamage, OverlayID, OverlayX, OverlayY, …`).
- Constant folding at compile time; non-constant expressions become `DecoExpr` trees (see `08 §4.1`).
- `action native` and `native` class declarations are **only permitted in `ferrite.pk3`**; in user mods they're errors (as in GZDoom DECORATE).

### 1.3 Errors

Collect all errors in all DECORATE lumps, then abort startup with a report if any (GZDoom: "Script error" list then fatal). Warnings (unknown sprite, missing state label targets) are non-fatal.

## 2. DeHackEd / BEX / MBF21 (`ferrite-dehacked`)

GZDoom reference: `src/gamedata/d_dehacked.cpp`; specs: Doom Wiki *DeHackEd*, *BEX*, *MBF21* spec (`mbf21.md` in DSDA repo), *DSDHacked*.

### 2.1 Sources

`DEHACKED` lumps (every container, load order), `-deh`/`-bex` files, `.deh`/`.bex` loose files passed with `-file`. Applied **after** all DECORATE and module class registration so patches override.

### 2.2 Mapping data (`ferrite.pk3/dehacked/`)

DeHackEd addresses things/frames/sounds/sprites by vanilla index. Ship mapping tables as data files:

- `things.txt` — vanilla `mobjinfo` index (1-based in DeHackEd) → class name (`1 DoomPlayer`, `2 ZombieMan`, …, plus `MBF` dogs/beta things, `Deh_Actor_145…249` placeholder classes for DSDHacked free slots — GZDoom provides `Deh_Actor_150`… classes).
- `states.txt` — vanilla `states[]` index (0..1088 + MBF/MBF21 extras + DSDHacked dynamic range) → (owning class, label, offset) so that Frame N edits modify the right `State`.
- `sprites.txt`, `sounds.txt` — vanilla tables.
- `codepointers.txt` — DeHackEd mnemonic → action function name (e.g. `Chase → A_Chase`, `FireShotgun → A_FireShotgun`, MBF/MBF21 pointers).
- `ammo.txt`, `weapons.txt` — vanilla ammo/weapon indices → classes.
- `misc.txt` — `Misc` field targets (initial health/bullets, max health/armor, green/blue armor class, max soulsphere, soulsphere health, megasphere health, god mode health, IDFA/IDKFA armor and armor class, BFG cells/shot, monsters infight).

### 2.3 Patch syntax

- Header: `Patch File for DeHackEd v3.0`, `Doom version = 19|21`, `Patch format = 6`.
- Blocks: `Thing N (name)`, `Frame N`, `Pointer N (Frame M)`, `Sound N`, `Sprite N`, `Ammo N`, `Weapon N`, `Cheat 0`, `Misc 0`, `Text <oldlen> <newlen>` followed by raw text (the replace-string semantics: matches sprite names, music names, sound names and string table entries by original text).
- BEX extensions: `[STRINGS]` (mnemonic = value, `\` line continuation), `[PARS]`, `[CODEPTR]` (`FRAME n = Mnemonic`), `[HELPER]`, `[SPRITES]`, `[SOUNDS]`, `[MUSIC]`, `INCLUDE [NOTEXT] file`.
- Thing fields: `ID #`, `Initial frame`, `Hit points`, `First moving frame`, `Alert sound`, `Reaction time`, `Attack sound`, `Injury frame`, `Pain chance`, `Pain sound`, `Close attack frame`, `Far attack frame`, `Death frame`, `Exploding frame`, `Death sound`, `Speed`, `Width`, `Height`, `Mass`, `Missile damage`, `Action sound`, `Bits` (numeric or `+`/`|`-separated mnemonics incl. Boom/MBF `TRANSLUCENT, TOUCHY, BOUNCES, FRIEND`), `Respawn frame`, `Dropped item` (DSDA), MBF21: `MBF21 Bits` (`LOGRAV, SHORTMRANGE, DMGIGNORED, NORADIUSDMG, FORCERADIUSDMG, HIGHERMPROB, RANGEHALF, NOTHRESHOLD, LONGMELEE, BOSS, MAP07BOSS1/2, E1M8BOSS, E2M8BOSS, E3M8BOSS, E4M6BOSS, E4M8BOSS, RIP, FULLVOLSOUNDS`), `Infighting group`, `Projectile group`, `Splash group`, `Rip sound`, `Fast speed`, `Melee range`.
- Frame fields: `Sprite number`, `Sprite subnumber` (with fullbright bit 0x8000), `Duration`, `Next frame`, `Unknown 1/2` (misc1/misc2), `Codep Frame`, MBF21 `MBF21 Bits` (`SKILL5FAST`), `Args1..Args8`.
- Weapon fields: `Ammo type`, `Deselect frame`, `Select frame`, `Bobbing frame`, `Shooting frame`, `Firing frame`, MBF21 `Ammo per shot`, `MBF21 Bits` (`NOTHRUST, SILENT, NOAUTOFIRE, FLEEMELEE, AUTOSWITCHFROM, NOAUTOSWITCHTO`).
- DSDHacked: thing/frame/sprite/sound indices beyond vanilla allocate new entries dynamically.

Semantics to replicate:
- Vanilla codepointer arg conventions (`misc1/misc2` for `A_Spawn`, `A_Turn`, `A_Face`, `A_Scratch`, `A_PlaySound`, `A_RandomJump`, `A_LineEffect`, …), MBF21 `Args`.
- Editing a frame's codepointer to a weapon function on a monster frame (and vice versa) works as in GZDoom (it wraps them with compatible calling conventions).
- `Thing` speed for missiles is fixed-point in DeHackEd (values ≥ 65536 interpreted as fixed).
- Doom version 1.2/1.666 patches: GZDoom rejects/limits; match.

## 3. ZScript policy

### 3.1 Decision

ZScript source is **not executed** by Ferrite. Rationale: ZScript is a large, GZDoom-internal language tightly coupled to C++ class layouts; reproducing its VM and ~3000 native bindings would duplicate GZDoom without improving the modding experience. WASM modules replace it with a smaller, versioned, language-agnostic API.

### 3.2 Behaviour on load

- A `ZSCRIPT` lump (or `zscript.*` file at archive root) in a mod produces a single clear diagnostic:
  `"<mod.pk3> contains ZScript, which Ferrite does not run. The mod may not work. See https://<docs>/migrating-from-zscript"`.
- If the same container also provides a Ferrite module manifest (`ferrite.toml` / `modules/`), the ZScript is silently ignored (dual-target mods).
- Engine option `zscript_policy = warn | error | ignore` (default `warn`).

### 3.3 Migration support

1. **Migration guide** (`docs/guides/migrating-from-zscript.md`, written alongside the SDK): side-by-side table of ZScript constructs → SDK equivalents (see `12 §9`).
2. **API name parity**: the WASM host API mirrors ZScript's names (`A_SpawnItemEx`, `Actor.Spawn`, `FindInventory`, `CountInv`, `LevelLocals` → `level()`, `players[]` → `players()`, `EventHandler` → `#[event_handler]`), so translations are mechanical.
3. **Stretch (C): `ferrite-zsc` transpiler** — a separate tool that parses ZScript (grammar reference: GZDoom `src/common/scripting/frontend/zcc-parse.lemon`), type-checks against an API description generated from Ferrite's ABI (`ferrite-script-abi` exports a machine-readable API manifest), and emits Rust source using `ferrite-script-sdk`, which is then compiled to WASM. Unsupported features (direct native class field access with no ABI equivalent, `ui`/`play` scope violations, `Object` reflection) produce diagnostics. This is a separate repository/milestone; nothing in the engine depends on it.

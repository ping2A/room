# 08 — Actors (`room-sim::actor`, `room-defs::classes`)

GZDoom references: `src/playsim/actor.h`, `p_mobj.cpp`, `p_actionfunctions.cpp`,
`p_enemy.cpp`, `wadsrc/static/zscript/actors/**` (ZScript sources of every
built-in class — the authoritative behaviour reference for porting to Rust),
ZDoom wiki pages *Actor properties*, *Actor flags*, *Actor states*,
*Action functions*, *Classes*.

## 1. Where built-in actors come from

GZDoom implements built-in classes in ZScript inside `gzdoom.pk3`. Room has
no ZScript, so built-ins are split into:

1. **Declarations in DECORATE** inside `room.pk3` (`actors/shared/*.txt`,
   `actors/doom/*.txt`, `actors/heretic/*`, `actors/hexen/*`, `actors/strife/*`,
   `actors/chex/*`): properties, flags, states, DoomEdNums — written in DECORATE
   with Room's `native` extensions:
   ```
   ACTOR Inventory native                      // behaviour implemented in Rust
   {
       Inventory.Amount 1
       Inventory.MaxAmount 1
       ...
       action native A_JumpIfNoAmmo(state label);
   }
   ACTOR DoomImp 3001
   {
       Health 60  Radius 20  Height 56  Speed 8  PainChance 200
       Monster  +FLOORCLIP
       SeeSound "imp/sight" ...
       States { Spawn: TROO AB 10 A_Look  Loop  ... }
   }
   ```
   This is exactly how ZDoom 2.x shipped its actors (`wadsrc/static/actors/` in ZDoom 2.8 history) — use those files as a structural reference, updated to match current GZDoom ZScript defaults.
2. **Behaviour in Rust** (`room_sim::builtins`): native base classes (§5) and
   native action functions (§4). The registry validates at load that every
   `native` class and `action native` declaration has a Rust implementation,
   and vice versa (unit test).

The ZScript of current GZDoom is the behaviour reference: each Rust port of a
ZScript function cites the `.zs` file and function name.

## 2. Class registry

```rust
pub struct ClassDef {
    pub name: Name, pub id: ClassId,
    pub parent: Option<ClassId>,
    pub replaces: Option<ClassId>,           // DECORATE `replaces` / module replacement; resolved via `get_replacement` chain
    pub replaced_by: Option<ClassId>,
    pub native: NativeKind,                  // which Rust behaviour vtable (Actor, Inventory, Weapon, PlayerPawn, …)
    pub origin: ClassOrigin,                 // Builtin | Decorate(lump) | DeHackEd | Module(ModuleId)
    pub defaults: Box<ActorDefaults>,        // full default property block (the "default instance")
    pub state_labels: StateLabelTree,        // "Spawn", "Death.Fire", "Pain.Ice", custom labels
    pub states: Range<StateId>,              // this class's own states in the global StateTable
    pub user_vars: UserVarLayout,            // DECORATE `var int user_x[4];` + module fields stored host-side (see 12)
    pub virtuals: VirtualTable,              // overrides provided by modules (12)
    pub doomednum: Option<i32>, pub spawn_id: Option<u16>, pub conversation_id: Option<u32>,
    pub game_filter: GameMask,               // DECORATE `game Doom`
    pub sprite_names: SmallVec<[SpriteId; 4]>,
    pub abstract_: bool,
}
```

- Inheritance copies the parent's `defaults` then applies the child's properties (GZDoom `PClassActor::InitializeDefaults`).
- State labels inherit; `Goto Super::Label` resolves in the parent's label tree.
- Replacement: `spawn("ZombieMan")` spawns `get_replacement(ZombieMan)`; `get_replacee` used by `A_BossDeath`-type checks. Modules can also replace at runtime through the `CheckReplacement` event (`12`).
- Class lookup is case-insensitive by `Name`.

## 3. States

```rust
pub struct State {
    pub sprite: SpriteId, pub frame: u8,           // frame letter index ('A'..'Z','[','\\',']'); '#' = keep previous frame (see wiki *Actor states*)
    pub tics: i16, pub tic_range: Option<(i16, i16)>, // `random(a,b)` duration
    pub misc1: i32, pub misc2: i32,               // DeHackEd/MBF21 args (`Args1..8` for MBF21 states)
    pub action: Option<ActionRef>,                // native fn, module fn, DECORATE anonymous function, or DeHackEd codepointer
    pub next: Option<StateId>,                    // None = Stop
    pub flags: StateFlags,                        // BRIGHT, FAST, SLOW, NODELAY, CANRAISE, OFFSET, LIGHT, MBF21 `SKILL5FAST`, …
    pub offset: (i16, i16),                       // `Offset(x, y)` for weapon sprites
    pub lights: SmallVec<[Name; 1]>,              // `Light("name")` bindings
    pub owner: ClassId,
    pub source: SourceSpan,
}
```

State machine semantics (port `AActor::SetState`, `DPSprite::SetState`):

- `set_state(actor, state, nofunction)`: loop: if `None` → destroy actor (return false); set `tics` (apply `FAST` for fast monsters/`nightmare`, `SLOW`, random range using `pr_statetics`); set sprite/frame (sprite `TNT1` / frame `#`-`----` rules: frame `#` keeps the previous frame, sprite `####` keeps previous sprite); call action; if the action changed state (jump), continue with the new one; if `tics != 0` stop; else follow `next`. Guard: 1000 zero-tic iterations → error "infinite state loop" and freeze the actor (GZDoom).
- Spawn state: first call of `set_state` on spawn does **not** run the action unless the state has `NODELAY`.
- `tics == -1`: infinite; `Wait` = stay on the last state re-running nothing; `Loop` = jump to label start; `Fail` = only for `CustomInventory` Pickup/Use; `Stop` = `None`; `Goto Label[+N]`.
- Actions return values only for `CustomInventory` success/fail and `A_Jump*` style state results (anonymous functions may `return state`).

## 4. Action functions

### 4.1 Calling convention

```rust
pub type NativeAction = fn(&mut ActionCtx, &ActionArgs) -> ActionResult;
pub struct ActionCtx<'w> {
    pub world: &'w mut WorldCtx<'w>,
    pub self_: ActorId,           // actor running the state (for weapons: the player pawn)
    pub invoker: ActorId,         // weapon/CustomInventory item that owns the state (GZDoom "invoker")
    pub state_kind: StateCaller,  // Actor | PSprite(layer) | CustomInventory
}
pub enum ActionResult { Continue, Jump(StateId), Success(bool) }
```

Arguments are typed: `Int, Float, Bool, Name, String, Sound, Class, State(label path or relative offset), Color, Angle, Flags(u32), Expr(ExprId)`. DECORATE arguments may be **expressions** evaluated at call time (`random(1,8)*3`, `user_count + 1`, `CallACS("x")`, `args[0]`, `health`, `velx`, `GetDistance(true)`, `CountInv("X")`). Compile them into a tiny typed expression IR (`DecoExpr`) evaluated by the host — not WASM.

### 4.2 Required native action set

Implement every action function listed on the ZDoom wiki *Action functions* page that is not marked deprecated-and-removed. Grouped checklist (≈ 400 functions):

- **Generic**: `A_Jump*` (Jump, JumpIf, JumpIfCloser, JumpIfHealthLower, JumpIfInventory, JumpIfInTargetInventory, JumpIfArmorType, JumpIfNoAmmo, JumpIfTargetInLOS, JumpIfInTargetLOS, JumpIfTracerCloser, JumpIfMasterCloser, CheckFloor, CheckCeiling, CheckSight, CheckSightOrRange, CheckRange, CheckFlag, CheckProximity, CheckBlock, CheckSpecies, CheckLOF), `A_SetTics`, `A_Stop`, `A_SetAngle/Pitch/Roll`, `A_SetScale`, `A_SetRenderStyle`, `A_SetTranslucent`, `A_FadeIn/Out/To`, `A_SetTranslation`, `A_ChangeFlag`/`A_ChangeLinkFlags`, `A_SetSpecial`, `A_SetArg`, `A_SetUserVar/Array/VarFloat/ArrayFloat`, `A_SetHealth`, `A_SetMass`, `A_SetSize`, `A_SetSpeed`, `A_SetFloatSpeed`, `A_SetGravity`, `A_SetFloorClip/UnsetFloorClip`, `A_SetDamageType`, `A_SetPainThreshold`, `A_SetRipperLevel/RipMin/RipMax`, `A_SetChaseThreshold`, `A_SetViewPitch/Angle/Roll`, `A_SetVisibleRotation`, `A_SprayDecal`, `A_SetMugshotState`, `A_SetBlend`, `A_SetInventory`, `A_SetFloatBobPhase`, `A_CopySpriteFrame`, `A_SetTeleFog`, `A_SwapTeleFog`, `A_ChangeCountFlags`, `A_ChangeVelocity`, `A_ScaleVelocity`, `A_Stop`, `A_Warp`, `A_Teleport`, `A_QuakeEx`, `A_Quake`, `A_SetGravity`, `A_LogInt/Float`, `A_Log`, `A_Print`, `A_PrintBold`, `A_CheckTerrain`, `A_DropItem`, `A_SpawnItemEx`, `A_SpawnItem`, `A_SpawnDebris`, `A_SpawnParticle(Ex)`, `A_RadiusThrust`, `A_RadiusGive`, `A_RadiusDamageSelf`, `A_Explode`, `A_Blast`, `A_DamageSelf/Target/Master/Children/Siblings/Tracer`, `A_KillMaster/Children/Siblings/Target/Tracer`, `A_RemoveMaster/Children/Siblings/Target/Tracer`, `A_Remove`, `A_RaiseMaster/Children/Siblings/Self`, `A_GiveInventory`, `A_TakeInventory`, `A_GiveToTarget/Children/Siblings`, `A_TakeFromTarget/...`, `A_SelectWeapon`, `A_SetInventory`, `A_DropInventory`, `A_TransferPointer`, `A_RearrangePointers`, `A_CopyFriendliness`, `A_ResetHealth`, `A_FaceTarget/Master/Tracer/Movement`, `A_Face`, `A_Recoil`, `A_ClearTarget`, `A_ClearLastHeard`, `A_ClearSoundTarget`, `A_LookEx`, `A_Wander`, `A_Chase` (+ flags), `A_ExtChase`, `A_VileChase`, `A_Fall`/`A_NoBlocking`, `A_Scream`, `A_XScream`, `A_Pain`, `A_PlaySound`/`A_StartSound`/`A_StartSoundIfNotPlaying`, `A_StopSound(s)`, `A_SoundPitch`, `A_SoundVolume`, `A_PlaySoundEx`, `A_PlayWeaponSound`, `A_ActiveSound`, `A_Countdown`, `A_CountdownArg`, `A_SetShootable/Unset`, `A_SetSolid/Unset`, `A_SetFloat/Unset`, `A_SetReflective` …, `A_HideThing/UnHideThing`, `A_FreezeDeath`, `A_GenericFreezeDeath`, `A_FreezeDeathChunks`, `A_IceGuyDie`, `A_BossDeath`, `A_KeenDie`, `A_BrainDie`, `A_Burst`, `A_SkullPop`, `A_PlayerSkinCheck`, `A_CheckPlayerDone`, `A_Respawn`, `A_ResetReloadCounter`, `A_SeekerMissile`, `A_Tracer`, `A_Tracer2`, `A_Fire`, `A_FireCrackle`, `A_LightInverse`, `A_Light0/1/2`, `A_Light`, `A_ZoomFactor`, `A_SetCrosshair`, `A_Overlay`, `A_OverlayOffset`, `A_OverlayFlags`, `A_OverlayAlpha`, `A_OverlayRenderStyle`, `A_OverlayTranslation`, `A_OverlayPivot(Align)`, `A_OverlayScale`, `A_OverlayRotate`, `A_OverlayVertexOffset`, `A_ClearOverlays`, `A_MonsterRail`, `A_CustomRailgun`, `A_RailAttack`, `A_CustomBulletAttack`, `A_CustomMissile`/`A_SpawnProjectile`, `A_CustomMeleeAttack`, `A_CustomComboAttack`, `A_BasicAttack`, `A_MonsterRefire`, `A_ThrowGrenade`, `A_Weave`, `A_Morph`, `A_CheckForReload`, `A_CheckForResurrection`, `A_DeQueueCorpse`, `A_QueueCorpse`, `A_Detonate`, `A_Mushroom`, `A_KlaxonBlare`, `A_BishopMissileWeave`, `A_CStaffMissileSlither` …
- **Weapon**: `A_WeaponReady(flags)`, `A_Raise`, `A_Lower`, `A_ReFire`, `A_ClearReFire`, `A_GunFlash`, `A_CheckReload`, `A_Light*`, `A_FireBullets`, `A_FireProjectile`/`A_FireCustomMissile`, `A_CustomPunch`, `A_Saw`, `A_RailAttack`, `A_FireSTGrenade`, `A_WeaponOffset`, `A_ZoomFactor`, `A_SetCrosshair`, `A_Recoil`, `A_ResetReloadCounter`, `A_WeaponOffset`, plus game-specific (`A_FirePistol`, `A_FireShotgun`, `A_FireShotgun2`, `A_FireCGun`, `A_FireMissile`, `A_FirePlasma`, `A_FireBFG`, `A_BFGSpray`, `A_FireOldBFG`, `A_Punch`, `A_CheckReload`, `A_OpenShotgun2`, `A_LoadShotgun2`, `A_CloseShotgun2`, Heretic/Hexen/Strife weapon actions).
- **Doom monsters**: `A_PosAttack`, `A_SPosAttack`, `A_CPosAttack`, `A_CPosRefire`, `A_SpidRefire`, `A_TroopAttack`, `A_SargAttack`, `A_HeadAttack`, `A_BruisAttack`, `A_SkullAttack`, `A_BspiAttack`, `A_Hoof`, `A_Metal`, `A_BabyMetal`, `A_CyberAttack`, `A_PainAttack`, `A_PainDie`, `A_FatRaise`, `A_FatAttack1/2/3`, `A_SkelWhoosh`, `A_SkelFist`, `A_SkelMissile`, `A_Tracer`, `A_VileStart`, `A_VileTarget`, `A_VileAttack`, `A_StartFire`, `A_FireCrackle`, `A_BrainAwake`, `A_BrainPain`, `A_BrainScream`, `A_BrainExplode`, `A_BrainSpit`, `A_SpawnFly`, `A_SpawnSound`, `A_KeenDie`, `A_BossDeath`, `A_SkullPop`, MBF (`A_Detonate`, `A_Mushroom`, `A_Spawn`, `A_Turn`, `A_Face`, `A_Scratch`, `A_PlaySound`, `A_RandomJump`, `A_LineEffect`, `A_Die`, `A_FireOldBFG`, `A_BetaSkullAttack`, `A_Stop`), MBF21 (`A_SpawnObject`, `A_MonsterProjectile`, `A_MonsterBulletAttack`, `A_MonsterMeleeAttack`, `A_RadiusDamage`, `A_NoiseAlert`, `A_HealChase`, `A_SeekTracer`, `A_FindTracer`, `A_ClearTracer`, `A_JumpIfHealthBelow`, `A_JumpIfTargetInSight`, `A_JumpIfTargetCloser`, `A_JumpIfTracerInSight`, `A_JumpIfTracerCloser`, `A_JumpIfFlagsSet`, `A_AddFlags`, `A_RemoveFlags`, `A_WeaponProjectile`, `A_WeaponBulletAttack`, `A_WeaponMeleeAttack`, `A_WeaponSound`, `A_WeaponAlert`, `A_WeaponJump`, `A_ConsumeAmmo`, `A_CheckAmmo`, `A_RefireTo`, `A_GunFlashTo`).
- **Heretic / Hexen / Strife** monster & weapon actions: full list from the wiki's per-game action function categories (≈ 250). Implement per game milestone (roadmap).

### 4.3 Anonymous functions in DECORATE

DECORATE allows `{ … }` blocks in state definitions with `if/else`, `while`/`for` (limited), local variables, assignments to user vars, `return state("Label")`, and calls. Compile to the same `DecoExpr` IR extended with statements (`DecoBlock`). Keep it a small tree-walking interpreter; heavy logic belongs in WASM modules.

## 5. Native base classes (Rust behaviour)

Each `NativeKind` provides a Rust vtable mirroring the ZScript virtuals of the
corresponding GZDoom class. Modules can subclass any of these and override
virtuals (`12-wasm-scripting.md §Class model`).

```rust
pub trait ActorBehaviour {   // dispatched on NativeKind with a static match, not dyn
    fn begin_play(..); fn post_begin_play(..); fn tick(..); fn on_destroy(..);
    fn activate(.., activator); fn deactivate(.., activator);
    fn damage_mobj(.., inflictor, source, damage, mod_, flags, angle) -> i32;
    fn die(.., source, inflictor, dmgflags, mod_);
    fn special_missile_hit(.., victim) -> i32; fn touch(.., toucher); fn can_collide_with(.., other, passive) -> bool;
    fn collided_with(..); fn special_bounce_hit(..); fn slam_into(..); fn get_death_height(..) -> f64; fn get_gib_health(..);
    fn morphed_death(..); fn use_(..) /* Inventory */; fn try_pickup(..); fn handle_pickup(..); fn create_copy(..);
    fn create_tossable(..); fn attach_to_owner(..); fn detach_from_owner(..); fn do_effect(..); fn absorb_damage(..);
    fn modify_damage(..); fn travelled(..); fn owner_died(..); fn get_speed_factor(..); fn get_no_teleport_freeze(..);
    fn check_replacement / check_replacee (class-level, handled by registry)
    // PlayerPawn: player_think, move_player, calc_height, check_jump, check_crouch, give_default_inventory, …
    // Weapon: use_ammo, check_ammo, deplete_ammo, get_up_state/down/ready/atk/flash, end_powerup, …
}
```

Required native classes (port from `wadsrc/static/zscript/actors/**`):

- `Actor`; `Inventory` family: `CustomInventory`, `Ammo`, `BackpackItem`, `Armor`, `BasicArmor`, `BasicArmorPickup`, `BasicArmorBonus`, `HexenArmor`, `Health`, `HealthPickup`, `MegasphereHealth`, `Key`, `PuzzleItem`, `MapRevealer`, `ScoreItem`, `WeaponGiver`, `WeaponHolder`, `WeaponPiece`, `Weapon`, `PowerupGiver`, `Powerup` and every subclass (`PowerInvulnerable, PowerStrength, PowerInvisibility, PowerGhost, PowerShadow, PowerIronFeet, PowerMask, PowerLightAmp, PowerTorch, PowerFlight, PowerWeaponLevel2, PowerSpeed, PowerMinotaur, PowerTargeter, PowerFrightener, PowerBuddha, PowerScanner, PowerTimeFreezer, PowerDamage, PowerProtection, PowerDrain, PowerRegeneration, PowerHighJump, PowerDoubleFiringSpeed, PowerInfiniteAmmo, PowerMorph, PowerReflection, …`), `Coin`, `Sigil`, `RaiseAlarm`… (Strife), `ArtiTeleport`, `ArtiPoisonBag`, `ArtiBoostArmor`, `ArtiBlastRadius`, `ArtiHealingRadius`, `ArtiDarkServant`, `ArtiTeleportOther`, `ArtiEgg`, `ArtiPork`, …
- `PlayerPawn` (+ `PlayerChunk`, `MorphedMonster`, per-game player classes are DECORATE).
- Map helpers: `MapSpot`, `MapSpotGravity`, `PatrolPoint`, `PatrolSpecial`, `InterpolationPoint`, `InterpolationSpecial`, `PathFollower`, `ActorMover`, `MovingCamera`, `AimingCamera`, `SecurityCamera`, `SkyViewpoint`, `SkyPicker`, `StackPoint`, `UpperStackLookOnly`, `LowerStackLookOnly`, `SectorSilencer`, `SectorFlagSetter`, `SoundSequence`, `SoundSequenceSlot`, `AmbientSound`, `AmbientSoundNoGravity`, `SoundEnvironment`, `WaterZone`, `SecretTrigger`, `TeleportDest(2/3)`, `TeleportFog`, `Spark`, `ParticleFountain`, `DynamicLight` family (`PointLight`, `PointLightPulse`, `PointLightFlicker`, `SectorPointLight`, `PointLightFlickerRandom`, `SpotLight*`, attenuated/additive/subtractive variants, things 9800–9885), `VavoomLight*`, `ColorSetter`, `FadeSetter`, `SectorAction` family (`SecActEnter, SecActExit, SecActHitFloor, SecActHitCeiling, SecActUse, SecActUseWall, SecActEyesDive, SecActEyesSurface, SecActEyesBelowC, SecActEyesAboveC, SecActHitFakeFloor, SecActDamage`, MusicChanger), `Decal` (thing 9200), `Bridge`, `InvisibleBridge`, `CustomBridge`, `HateTarget`, `RandomSpawner`, `FastProjectile`, `SwitchableDecoration`, `SwitchingDecoration`, `PointPusher/Puller`, `Bot`-related (C), `SpecialSpot`, `BossBrain`/`BossEye`/`BossTarget`/`SpawnShot`/`SpawnFire`, `MorphProjectile`, `CustomSprite`, `VisualThinker` (GZDoom 4.12+, S).

## 6. Actor instance data

Group fields to keep the hot loop cache-friendly. Every DECORATE property and
flag maps to one field; keep GZDoom names (snake_case) for traceability.

```rust
pub struct Actor {
    // identity / class
    pub class: ClassId, pub state: Option<StateId>, pub tics: i32, pub sprite: SpriteId, pub frame: u8,
    // spatial
    pub pos: DVec3, pub prev: DVec3, pub vel: DVec3, pub angles: Rotator /*yaw,pitch,roll*/, pub prev_angles: Rotator,
    pub radius: f64, pub height: f64, pub floorz: f64, pub ceilingz: f64, pub dropoffz: f64,
    pub floorsector: SectorId, pub ceilingsector: SectorId, pub sector: SectorId, pub subsector: SubsectorId, pub group: PortalGroup,
    pub floorpic: TextureId, pub ceilingpic: TextureId, pub floorterrain: TerrainId, pub blockingline: Option<LineId>, pub blockingmobj: Option<ActorId>,
    pub speed: f64, pub float_speed: f64, pub gravity: f64, pub friction: f64, pub mass: i32, pub max_step_height: f64, pub max_dropoff_height: f64,
    pub floor_clip: f64, pub float_bob_phase: i32, pub float_bob_strength: f64, pub water_level: u8, pub water_depth: f64,
    // flags
    pub flags: ActorFlags,           // MF..MF8 combined bitflags (u64 × N)
    pub render_flags: RenderFlags,   // RF_*
    pub bounce: BounceFlags, pub bounce_factor: f64, pub wall_bounce_factor: f64, pub bounce_count: i32,
    // render
    pub render_style: RenderStyle, pub alpha: f64, pub fill_color: Rgb, pub translation: TranslationId, pub scale: DVec2,
    pub visible_angles/pitch: …, pub sprite_offset: DVec2, pub world_offset: DVec3, pub sprite_angle/rotation: f64,
    pub light_level: i16, pub stealth_alpha: f64, pub fov?, pub view_pos: Option<ViewPosition>,
    // gameplay
    pub health: i32, pub spawn_health: i32 (via defaults), pub pain_chances: PainChanceTable, pub damage_factors: DamageFactorTable,
    pub damage: DamageValue /* fixed or expr */, pub damage_type: Name, pub damage_multiply: f64, pub projectile_kickback: i32,
    pub reaction_time: i32, pub threshold: i32, pub def_threshold: i32, pub move_dir: u8, pub move_count: i32, pub strafe_count: i32,
    pub min_missile_chance: i32, pub melee_range: f64, pub max_target_range: f64, pub melee_threshold: f64,
    pub pain_threshold: i32, pub ripper_level: i32, pub rip_level_min: i32, pub rip_level_max: i32,
    pub species: Name, pub designated_team: u8, pub friend_player: u8, pub skill_respawn_count: i32, pub tele_fog_source/dest: ClassId,
    pub activation_type: ActivationFlags, pub special: Special, pub args: [i32; 5], pub tid: i32, pub special1: i32, pub special2: i32,
    pub special_f1: f64, pub special_f2: f64, pub score: i32, pub accuracy: i32, pub stamina: i32, pub weave_index_xy: u8, pub weave_index_z: u8,
    pub kill_count/item_count via flags; pub last_look_player: i32, pub visdir: i32, pub conversation_root: i32, pub conversation: Option<..>,
    // relationships
    pub target, tracer, master, last_enemy, last_heard, goal, inventory_owner: Option<ActorId>, pub inventory: Option<ActorId> /*linked list head*/,
    pub poisoner: Option<ActorId>,
    pub player: Option<PlayerIndex>,
    // sounds
    pub sounds: ActorSounds { see, attack, pain, death, active, use_, bounce, wall_bounce, crush_pain, howl, push },
    // inventory-specific, weapon-specific and player-pawn-specific data in enum side-tables keyed by ActorId:
    //   InventoryData, WeaponData, PlayerPawnData, PowerupData… (stored in separate arenas to keep Actor small)
    // links
    pub links: ActorLinks,           // sector list prev/next, block list prev/next, touching-sector node head, tid hash next
    pub user_vars: UserVarStorage,   // DECORATE user_* vars
    pub module_slot: Option<ModuleObjRef>, // per-instance module data handle (12)
    pub obj_flags: ObjFlags,         // destroyed, travelling, …
}
```

## 7. Inventory and weapons

- Inventory is a singly-linked list via `inventory` (preserve GZDoom ordering: newest first; behaviour of `FindInventory` and `UseInventory` depends on it).
- Pickup pipeline: `Touch` → `CallTryPickup` → `TryPickup` → owner's existing items' `HandlePickup` chain → `AttachToOwner` / amount merge → `PlayPickupSound`, `PrintPickupMessage`, `DoPickupSpecial`, flash, auto-activate (`INVENTORY.AUTOACTIVATE`), respawn handling (`sv_itemrespawn`, `INVENTORY.BIGPOWERUP`), `ShouldStay` in coop (weapons stay).
- `Inventory` flags: `QUIET, AUTOACTIVATE, UNDROPPABLE, INVBAR, HUBPOWER, PERSISTENTPOWER, INTERHUBSTRIP, ALWAYSPICKUP, FANCYPICKUPSOUND, BIGPOWERUP, KEEPDEPLETED, IGNORESKILL, ADDITIVETIME, UNTOSSABLE, RESTRICTABSOLUTELY, NOSCREENFLASH, TOSSED, ALWAYSRESPAWN, TRANSFER, NOTELEPORTFREEZE, NOSCREENBLINK, ISHEALTH, ISARMOR, UNCLEARABLE, NEVERRESPAWN, NOATTENPICKUPSOUND`.
- Weapons: `PSprite` layers (`PSP_WEAPON=1`, `PSP_FLASH=1000`, `PSP_STRIFEHANDS`, `PSP_TARGETCENTER/LEFT/RIGHT`, user overlays any int), layer flags (`PSPF_ADDWEAPON, ADDBOB, POWDOUBLE, CVARFAST, ALPHA, RENDERSTYLE, FLIP, FORCEALPHA, FORCESTYLE, MIRROR, PLAYERTRANSLATED, PIVOTPERCENT, INTERPOLATE`), bobbing (`BobStyle` Normal/Inverse/Alpha/InverseAlpha/Smooth/InverseSmooth, `BobSpeed`, `BobRangeX/Y`), weapon states `Ready/Deselect/Select/Fire/AltFire/Hold/AltHold/Flash/AltFlash/Reload/Zoom/User1-4/DeadLowered`, `A_WeaponReady` flags `WRF_*`, `Weapon.*` properties (AmmoType1/2, AmmoUse1/2, AmmoGive1/2, MinSelectionAmmo1/2, SelectionOrder, SlotNumber, SlotPriority, SisterWeapon, YAdjust, UpSound, ReadySound, KickBack, BobStyle…, LookScale, DefaultKickback), flags (`NOAUTOFIRE, READYSNDHALF, DONTBOB, AXEBLOOD, NOALERT, AMMO_OPTIONAL, ALT_AMMO_OPTIONAL, AMMO_CHECKBOTH, PRIMARY_USES_BOTH, ALT_USES_BOTH, WIMPY_WEAPON, POWERED_UP, STAFF2_KICKBACK, EXPLOSIVE, MELEEWEAPON, BFG, CHEATNOTWEAPON, NO_AUTO_SWITCH, NOAUTOAIM, NODEATHDESELECT, NODEATHINPUT, …`).
- Weapon slots: priority from (1) user KEYCONF/`setslot` (2) player class `Player.WeaponSlot` (3) weapon `SlotNumber`. Port `FWeaponSlots`.

## 8. Monster AI

Port `p_enemy.cpp` (+ ZScript `actors/attacks.zs`, `actors/actor_*.zs`):
- `look` (`A_Look`, `A_LookEx` with `LOF_*`), `P_LookForPlayers` (FOV 180°, `allaround`, `MF_AMBUSH`, sound target, `MF3_NOSIGHTCHECK`, `MF4_LOOKALLAROUND`), `P_LookForMonsters` (deathmatch/friendly), `P_LookForTID`, `P_LookForEnemies` (MBF friendlies).
- `chase` (`A_Chase` with `CHF_*`): reaction time, threshold & infighting retargeting, melee/missile range checks (`P_CheckMeleeRange`, `P_CheckMissileRange` incl. `MinMissileChance`, `MF4_MISSILEMORE/EVENMORE`, `MaxTargetRange`), `P_Move` (dir-based walking, door opening by `MF_CANUSEWALLS`/`MF4_CANUSEWALLS`, float AI adjusting z), `P_TryWalk`, `P_NewChaseDir` (`P_DoNewChaseDir`, `P_RandomChaseDir`, MBF `P_SmartMove`, dropoff avoidance `P_AvoidDropoff` (MBF)), strafing (`CHF_` + `MF5_` flags), fleeing (`FRIGHTENED`, `MF4_FRIGHTENED`, `frighten` power), active sounds, `MF5_ALWAYSFAST`, `fastmonsters` skill.
- `P_NoiseAlert` / `P_RecursiveSound` (sound propagation through two-sided lines, `ML_SOUNDBLOCK` count ≥ 2 stops, `Sector.soundtarget`), `A_AlertMonsters`.
- Friendly monsters (MBF): `MF_FRIENDLY`, `FRIENDLYSEEBLOCKS`, following player, `Thing_SetGoal` patrol, `Thing_Hate` modes.
- Resurrection: `A_VileChase` / `A_CheckForResurrection`, `CanRaise` flag rules, corpse checks with `P_CheckPosition`, `compat_vileghosts`.
- Bosses: `A_BossDeath` checks (`MF2_BOSS`, `MF4_BOSSDEATH`, MAPINFO special actions), `A_BrainSpit` targets (`BossTarget` list in spawn order), Icon of Sin `easy` alternation.

## 9. Morphing, conversations, bots

- Morph: `P_MorphPlayer`/`P_MorphMonster`, `MorphStyle` flags (`MRF_*`), undo, `PowerMorph`, `MorphProjectile`. Port from `morph.zs`.
- Strife conversations: dialogue IR from USDF/binary `DIALOGUE` lumps; the sim handles choices via a `NetCommand::ConversationReply` (GZDoom `DEM_CONVREPLY`) so it is deterministic in netplay; the UI shows the dialogue (`16-ui.md`).
- Bots (GZDoom `b_*.cpp`): **C priority**. If implemented, bots produce `TicCmd`s from sim state and run on every peer deterministically.

## 10. Tests

- Each native action function has a micro-scenario test (synthetic UDMF map + spawn + N tics + assertions).
- Golden behaviour tests: record Room demos on Freedoom maps; checksums per tic snapshotted; any change in behaviour must update goldens deliberately.
- Cross-check scripts (manual, documented in `20`): play the same input in GZDoom with `-record` and compare high-level outcomes (kills, health over time) — not bit-exact.

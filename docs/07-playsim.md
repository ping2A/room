# 07 — Playsim (`ferrite-sim`)

GZDoom references: `src/playsim/` (`p_tick.cpp`, `p_mobj.cpp`, `p_map.cpp`,
`p_maputl.cpp`, `p_sight.cpp`, `p_interaction.cpp`, `p_enemy.cpp`, `p_user.cpp`,
`p_spec.cpp`, `p_lnspec.cpp`, `mapthinkers/*`, `p_trace.cpp`, `p_secnodes.cpp`),
`src/g_game.cpp`, `src/g_level.cpp`, `src/m_random.cpp`.

## 1. World layout

```rust
pub struct World {
    pub defs: Arc<Definitions>,
    pub map: Arc<MapGeometry>,
    pub level: LevelState,                 // time, maptime, totaltime, stats, flags, sky, fade, gravity, aircontrol, music, …
    pub sectors: Vec<SectorDynamic>,
    pub sides: Vec<SideDynamic>,           // texture/offset/scale/light changes at runtime
    pub lines: Vec<LineDynamic>,           // flags, special, args, activation changes (SetLineSpecial, Line_SetBlocking…)
    pub polyobjs: Vec<PolyobjDynamic>,
    pub ffloors: Vec<FFloor>,
    pub actors: ActorArena,                // generational arena of Actor (see 08)
    pub thinkers: ThinkerLists,            // statnum-ordered lists (§3)
    pub players: [Option<Player>; MAX_PLAYERS], // MAX_PLAYERS = 16 (storage); netplay 1.0 targets up to 8 peers
    pub blockmap_things: BlockThings,
    pub sector_links: SectorThingLinks,    // msecnode equivalent: actor ↔ touching sectors
    pub rng: RngSet,                       // §6
    pub tid_hash: TidHash,                 // tid → actor list (insertion-ordered)
    pub interpolations: Interpolations,    // prev/cur for planes, scrollers, polyobjects
    pub sound_events: Vec<SoundEvent>,     // drained by audio each tic
    pub fx_events: Vec<FxEvent>,           // decals, particles spawn requests, screen flashes, quakes (render-only effects)
    pub acs: AcsState,                     // owned state of the ACS VM (serializable)
    pub script_state: ScriptStateHandle,   // module instances are owned by ScriptHost, referenced here
    pub corpse_queue: VecDeque<ActorId>,
    pub body_queue: …,                     // player corpses (bodyqueue size 32)
    pub stamp: QueryStamps,                // validcount replacements
    pub frozen: bool,                      // "freeze" cheat / level freeze
}
```

Rules:

- All containers are `Vec`/arenas; no `HashMap` iteration (`02-conventions.md §4.2`).
- `World` derives `Serialize/Deserialize` via a custom `SaveContext` that stores IDs, not pointers.
- `World::checksum() -> u64`: a stable hash (e.g. `xxh3` over a canonical byte serialization) of actor positions/velocities/health/state ids, sector plane `d` values, RNG states, and player states. Used for desync detection and determinism CI.

## 2. The game tic

### 2.1 `Game::tick` (equivalent of `G_Ticker`)

```
1. Process pending GameAction (new game, load, save, level completed, world done, victory, screenshot, quit-to-title, …).
2. For each player in game: copy this tic's TicCmd; process net commands (weapon select, inventory use, cheats, puke, netevent, …) in player order.
3. Consistency check (netplay): compare world checksum of tic N-k with peers (17).
4. match game_state:
     Level        => world.tick(); automap.tick();
     Intermission => intermission.tick();
     Finale       => finale.tick();
     DemoScreen / TitleLevel => page/title-map tick;
```

### 2.2 `World::tick` (equivalent of `P_Ticker`)

Order is behaviourally significant; port from `p_tick.cpp` and keep a comment per step with the GZDoom line it corresponds to:

```
if paused or menu-paused (single player) => return
interpolations.begin_tic()                 // copy cur → prev
psprites.new_tic()
every 4 tics: apply pending freeze change
think_particles()
for each player (index order): player_think(p)      // P_PlayerThink (movement from cmd, weapons, inventory, view)
events.world_tick()                                  // WASM WorldTick handlers
status_bar.tick()                                    // through UI hook (sim-visible state only)
level.tick()                                         // level-wide effects (fade, sky scroll, lightning)
thinkers.run_all()                                   // §3
if !frozen { update_specials(); run_effects(); }     // animated textures/switch timers, particle effects
level.time += 1; level.maptime += 1; level.totaltime += 1
events.world_tick_post()
gc_destroyed_actors()                                // §4
```

## 3. Thinkers

### 3.1 Status lists

Thinkers are grouped by **statnum** and run in ascending statnum, insertion
order within a list (GZDoom `FThinkerCollection`). Constants mirror GZDoom:

```
STAT_INFO=0, STAT_DECAL=1, STAT_AUTODECAL=2, STAT_CORPSEPOINTER=3, STAT_TRAVELLING=4, STAT_STATIC=5,
STAT_FIRST_THINKING=32, STAT_SCROLLER=32, STAT_PLAYER, STAT_BOSSTARGET, STAT_LIGHTNING, STAT_DECALTHINKER,
STAT_INVENTORY, STAT_LIGHT, STAT_LIGHTTRANSFER, STAT_EARTHQUAKE, STAT_MAPMARKER, STAT_DLIGHT,
STAT_USER=70, STAT_USER_MAX=90,
STAT_DEFAULT=100, STAT_SECTOREFFECT, STAT_ACTORMOVER, STAT_SCRIPTS, STAT_BOT, STAT_VISUALTHINKER (GZDoom 4.12+)
```

Thinkers below `STAT_FIRST_THINKING` never tick. Verify exact numeric values against `statnums.h` and keep them identical (modules can call `ChangeStatNum`).

### 3.2 Thinker kinds

```rust
pub enum Thinker {
    Actor(ActorId),
    Floor(FloorMover), Ceiling(CeilingMover), Door(DoorMover), Plat(PlatMover), Elevator(ElevatorMover),
    Pillar(PillarMover), Waggle(WaggleMover), Stairs/Donut handled as Floor variants,
    PolyMove(..), PolyRotate(..), PolyDoor(..), PolyMoveTo(..),
    Scroller(Scroller), Pusher(Pusher),
    LightFlash, Strobe, Glow, Glow2, FireFlicker, Flicker, Phased, LightTransfer/WallLightTransfer,
    Lightning(LightningController), Earthquake(Quake), PlaneWatcher, SectorEffect…,
    AcsScript(AcsScriptId),              // ACS runs as thinker STAT_SCRIPTS (DACSThinker)
    Module(ModuleThinkerId),             // WASM-defined non-actor thinkers (ZScript `Thinker` subclasses)
}
```

Sector movers store `SectorId` and are registered in the sector's `floor_data/ceiling_data/lighting_data` slots exactly like GZDoom (a sector can have at most one active floor mover and one ceiling mover; specials check `sector.floor_data.is_some()` to refuse activation).

`T_MovePlane` semantics (`DMover::MovePlane` in GZDoom): returns `Ok | Crushed | PastDest`; handles `crush` value, `hexencrush` mode, `P_ChangeSector` call after every move and restoring position on blocked movement. Port exactly, including the slopes-aware `MovePlane` used for sloped sectors.

## 4. Object lifecycle and deferred mutation

- `world.spawn(class, pos, flags) -> ActorId` is allowed at any time during a tic; new actors are appended to their statnum list and will think **this tic** if their list hasn't been passed yet (GZDoom behaviour: newly spawned actors in a later list do think in the same tic; actors spawned into a list currently being iterated are appended and also reached). Implement thinker lists as `Vec<ThinkerSlot>` with tombstones and iterate by index until the current length (re-read each step).
- `world.destroy(actor)`: runs `OnDestroy` hooks, unlinks from blockmap/sector lists immediately, marks the slot destroyed and **bumps the generation immediately** (so every stale `ActorId` reads as `None`, matching GZDoom's `TObjPtr` null-on-destroy semantic). The slot index is reclaimed at `gc_destroyed_actors()` at end of tic.
- Actor references stored inside actors (`target`, `tracer`, `master`, `lastenemy`, `goal`, …) are `Option<ActorId>`.

## 5. Movement, collision, and interaction

Port these GZDoom functions faithfully. The table is the implementation checklist; each becomes a Rust function in `ferrite_sim::physics`.

| GZDoom | Ferrite | Notes |
|---|---|---|
| `P_CheckPosition` + `PIT_CheckLine` + `PIT_CheckThing` | `check_position(actor, pos) -> CheckResult` | outputs floorz, ceilingz, dropoffz, floorsector, floorpic, floorterrain, ceilingline, blocking line/thing, portal info, 3D floor aware (`P_LineOpening` with ffloors), `MF2_PASSMOBJ` vertical checks, `MF3_ISMONSTER`… |
| `P_TryMove` | `try_move(actor, newpos, dropoff, &mut tm) -> bool` | step height, `MaxDropOffHeight`, dropoff rules (`compat_dropoff`), crossing special lines (`SPAC_Cross/MCross/PCross/AnyCross`), portal crossing (linked & teleporting), `PushLine` |
| `P_XYMovement` | `xy_movement(actor)` | max move subdivision (`MAXMOVE`), sliding (`P_SlideMove`), bouncing (`P_BounceWall`, `BounceFlags`), missile explosion (`P_ExplodeMissile`), friction (`P_GetFriction`, `MF2_SLIDE`, ice), `MF_SKULLFLY` stop, stopping thresholds (`STOPSPEED`) |
| `P_ZMovement` | `z_movement(actor, oldfloorz)` | gravity (`Gravity`, level gravity, `MF_NOGRAVITY`, water levels), float AI (`MF_FLOAT`), `FloatBob`, hitting floor/ceiling (`P_HitFloor` terrain splash), falling damage, bouncing on planes, landing sounds/squash |
| `P_FakeZMovement`, `P_TestMobjZ` | | |
| `P_SlideMove`, `P_BounceWall`, `P_BounceActor` | | |
| `P_ChangeSector` / `PIT_ChangeSector` | `change_sector(sector, crunch, amount, floor_or_ceiling, is_reset)` | crushing (`DMG_CRUSH`), gibbing corpses (`A_Crush` behavior: `CrushPainSound`, gib state), `MF2_DORMANT`… |
| `P_TeleportMove`, `P_Teleport` | `teleport_move`, `teleport` | telefrag (`MF2_TELESTOMP`), fog spawning, `TELF_*` flags |
| `P_CheckSight` (+ `SightCheck` class) | `check_sight(a, b, flags) -> bool` | reject early-out, blockmap trace, 3D floors, linked portals, `SF_IGNOREVISIBILITY`, `SF_SEEPASTSHOOTABLELINES`… |
| `P_AimLineAttack` | `aim_line_attack(...)` | autoaim, `ALF_*` flags, friendly/target filtering, 3D floors |
| `P_LineAttack` / `P_TraceBullet` / `Trace()` | `line_attack(...)` / `trace(...)` | hitscan via `FTraceInfo`: puffs (`PUFF_*`), blood, decals (as `FxEvent`), hitting 3D floors/planes, portals, water splash, `LAF_*` flags |
| `P_RailAttack` | `rail_attack` | piercing, `RAF_*`, rail trail effect as FxEvent |
| `P_UseLines` / `PTR_UseTraverse` | `use_lines(player)` | use range (`UseRange`), `UseThrough`, `BLOCKUSE`, `checkswitchrange` |
| `P_UsePuzzleItem` | | Hexen puzzle items |
| `P_RadiusAttack` | `radius_attack(spot, source, damage, distance, mod, flags, fulldamagedistance, species)` | `RADF_*` flags, `compat_explode1/2`, sight check via `P_CheckSight` variant |
| `P_DamageMobj` | `damage_mobj(target, inflictor, source, damage, mod, flags, angle) -> i32` | full logic: invulnerability, buddha, god, `DamageFactor`s (per type, per skill), `ApplyDamageFactors`, armor absorption (`AbsorbDamage` on inventory), `ModifyDamage` (powerups), pain chance, thrust (`P_ThrustMobj` w/ `ProjectileKickback`), infighting rules, `DMG_*` flags, death, obituaries |
| `AActor::Die` | `die(actor, source, inflictor, dmgflags, mod)` | `XDeath`, `Death.<type>` states, drops (`DropItem` lists, `P_DropItem` w/ skill ammo factor), kill counts, `MF_COUNTKILL`, `MF4_BOSSDEATH`, `A_BossDeath` map specials (MAPINFO specialaction) |
| `P_SpawnPuff`, `P_SpawnBlood`, `P_BloodSplatter`, `P_RipperBlood` | | |
| `P_SpawnMissile*` family | `spawn_missile(source, dest, class)` etc. | `P_CheckMissileSpawn`, `MF2_SEEKERMISSILE` tracking via `A_SeekerMissile` |
| `P_ExplodeMissile` | | |
| `P_HitFloor`, `P_HitWater` | | terrain splashes |
| `P_CheckFor3DFloorHit` / `P_CheckFor3DCeilingHit` | | |
| `P_ThingOnFloor`, `P_ActivateThingSpecial` | | |
| Pushers (`DPusher`, things 5001/5002), wind/current sectors | `Pusher` thinker | Boom semantics incl. `MF2_WINDTHRUST`, `compat_*` |
| Friction (Boom 223, sector `friction`) | | `MoveFactor` |

### 5.1 Linking

- `link_to_world(actor, ctx)` / `unlink_from_world(actor)`: maintain sector `thinglist` (for `MF_NOSECTOR` exclusion), blockmap cell list (excluded with `MF_NOBLOCKMAP`), touching-sector nodes (`P_CreateSecNodeList`), render-only "sprite-touching" sectors (`touching_rendersectors`) and portal group.
- Use intrusive index-linked lists stored in arrays (`prev/next: Option<ActorId>`); never `Vec::remove` in hot paths.

## 6. Random numbers

GZDoom's `FRandom` (`m_random.cpp`):

- Named generators (`pr_chase`, `pr_damagemobj`, … ~300 names). Each is an **SFMT** (SIMD-oriented Fast Mersenne Twister, MEXP 607) instance. Port SFMT-607 to pure scalar Rust (the SIMD version is just an optimization; output is identical).
- Seeding: all generators seeded from the global `rngseed` combined with the CRC32 of the generator's name (verify the exact combination in `FRandom::Init`/`StaticClearRandom`). Same seed + same names ⇒ same streams.
- API: `rng.random(name) -> u8` (0..=255, `operator()`), `random2(mask)` (difference of two calls), `hit_dice(count)`, `gen_rand32`, `gen_rand_real1/2`.
- Vanilla compat: the classic `rndtable[256]` with `prndindex`/`rndindex` used by `M_Random`/`P_Random`, active only in the strict vanilla mode used for vanilla demo playback. GZDoom has no such mode; it's a Ferrite addition (see `19`).
- Modules and ACS get their own named generators (`pr_acs`, `pr_script_<module>`); a module may create named generators which are added to the set deterministically by name.
- Save: every generator's full state. Net checksum: includes RNG indices.
- Non-sim randomness (menu effects, UI, audio pitch variation for non-sim sounds) uses `ferrite_core::rng::UiRng`, never `World::rng`.

## 7. Players

```rust
pub struct Player {
    pub mo: Option<ActorId>,               // the PlayerPawn actor
    pub cmd: TicCmd, pub original_cmd: TicCmd,
    pub player_state: PlayerState,         // Live, Dead, Reborn, Enter, Gone
    pub cls: ClassId,                      // player class
    pub view_z: f64, pub view_height: f64, pub delta_view_height: f64, pub bob: f64, pub vel: DVec2,
    pub centering: bool, pub turn_ticks: i32, pub on_ground: bool, pub crouch_*: …, pub jump_tics: i32,
    pub ready_weapon: Option<ActorId>, pub pending_weapon: PendingWeapon, pub psprites: PSpriteStack,
    pub weapons: WeaponSlots,              // KEYCONF/class/module slots
    pub inventory_selection: Option<ActorId>,
    pub cheats: CheatFlags,                // CF_NOCLIP, CF_GODMODE, CF_NOTARGET, CF_FROZEN, CF_BUDDHA, CF_INSTANTWEAPSWITCH, …
    pub frags: [i32; MAX_PLAYERS], pub fragcount: i32, pub kill_count, item_count, secret_count: i32,
    pub damage_count, bonus_count, poison_count, hazard_count: i32, pub poisoner, attacker: Option<ActorId>,
    pub extra_light: i32, pub fixed_colormap, fixed_lightlevel: i32,
    pub morph_tics: i32, pub morph_style: u32, pub premorph_weapon: Option<ClassId>,
    pub chicken_peck: i32, pub jump_tics: i32, pub respawn_time: i32, pub camera: Option<ActorId>,
    pub air_finished: i32, pub user_info: UserInfo,  // name, team, colour, skin, gender, autoaim, neverswitch, movebob, stillbob, fov, handicap, …
    pub settings_controller: bool, pub conversation_*: …, pub log_text: String, pub sound_class: Name,
    pub last_bump/use, pub mute_until: …, pub cheat_history: …
}
```

`player_think` ports `P_PlayerThink` + `PlayerPawn::PlayerThink` (which in GZDoom is ZScript in `player.zs` — port its logic to Rust in `ferrite_sim::player`): crouching, `MovePlayer` (forward/side scaling with `ForwardMove1/2`, `SideMove1/2`, `run`), jump, fly (`MF_FLY` via inventory), `CalcHeight` (bob, `MoveBob`, `StillBob`), `CheckUse`, `CheckWeaponChange`, `TickPSprites`, `CheckPoison`, `CheckDegeneration`, `CheckAirSupply`, `CheckEnvironment` (sector damage/specials), `CheckUndoMorph`, death think (`DeathThink`: view lowering, look at killer, respawn on use), `CheckFOV`, cheat effects.

### 7.1 TicCmd

```rust
#[repr(C)] #[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicCmd {
    pub buttons: Buttons,  // u32: ATTACK, USE, JUMP, CROUCH, TURN180, ALTATTACK, RELOAD, ZOOM, SPEED, STRAFE,
                           // MOVERIGHT, MOVELEFT, BACK, FORWARD, RIGHT, LEFT, LOOKUP, LOOKDOWN, MOVEUP, MOVEDOWN,
                           // SHOWSCORES, USER1..USER4, RUN
    pub pitch: i16, pub yaw: i16, pub roll: i16,   // angle deltas in 1/65536 turn units (GZDoom usercmd_t)
    pub forward_move: i16, pub side_move: i16, pub up_move: i16,
}
pub enum NetCommand { SelectWeapon(ClassId), UseInventory(ActorRef), DropInventory(..), Cheat(CheatCmd), Give(..),
    Summon(..), Puke{script, args, always}, NetEvent{name, args:[i32;3], manual}, Say(String), ChangeUserInfo(..), Kill, … }
```

## 8. Specials, ACS, scripts hooks

- Line/sector specials: `09-specials.md`. Entry points: `world.activate_line(line, actor, side, activation_type) -> bool`, `world.execute_special(special, args, activator, line, side) -> i32` (the `LineSpecials[]` table, `P_ExecuteSpecial`).
- `update_specials()`: animated textures (`AnimDefs` timers), switch button timers (`DActiveButton` thinkers in GZDoom), level-wide timers (`timelimit` in DM).
- ACS `run_tic` happens as a thinker (STAT_SCRIPTS).
- Module hooks via `ScriptHost` (see `12`).

## 9. Level flow

- `start_map(name, skill, flags)`: load MapGeometry (cached), create World, spawn things (`P_SpawnMapThing` semantics: skill bits, mode flags (single/coop/dm), class filter, `MTF_*`, DoomEdNum → class (MAPINFO `DoomEdNums` + game defaults), `filterstarts`, player starts 1–8 + deathmatch starts 11 + Hexen "player start N" args, polyobject spots skipped, ambient sound things 14001–14064, sound sequence overrides 1400–1409/1411, 9001 map spots, 9024 patrol points, 9025 security camera, 9026 spark, 9027–9033 particle fountains, 9070 interpolation points, 9048 sound environment, 9045 water zone, 9046 secret trigger, … — the authoritative list is GZDoom's `mapinfo/doomednums` engine data and the ZDoom wiki "Editor number" page; reproduce it in `ferrite.pk3` MAPINFO `DoomEdNums`), run `OPEN` ACS scripts and `WorldLoaded` events, `PostBeginPlay` for every actor on first tic.
- Exit: `G_ExitLevel(position, keep_facing)` semantics, secret exit, `Teleport_NewMap`, `Teleport_EndGame`, `Exit_Normal/Secret`, MAPINFO `next` resolution (`EndSequence`, `endgame1`…, `EndTitle`), cluster texts, intermission.
- **Hubs**: when leaving a map inside a hub cluster, serialize the World (minus players) into an in-memory "hub snapshot" keyed by map name; on return, restore it and run `RETURN` ACS scripts. Players travel with inventory (`STAT_TRAVELLING`). Hub snapshots are included in save games.
- `resethealth`, `resetinventory`, `keepfullinventory`, pistol start logic (`pistolstart` CVar), `ClearInventory` on death in single player (`sv_*`).

## 10. Visual-only effects inside the sim

- Particles (GZDoom `particle_t`, `P_ThinkParticles`) live in `World` because some are spawned by sim logic (`A_SpawnParticle`), but they never affect gameplay. They are stored in a bounded pool (`r_maxparticles`) and **excluded from `World::checksum()` and from saves** (GZDoom does not save particles). They use their own RNG `pr_particles` which is also excluded.
- Decals: the sim emits `FxEvent::Decal { … }`; the renderer owns decal storage. Decals **are** saved (GZDoom saves `DBaseDecal` thinkers) — the renderer's decal list is passed into the save system through `ferrite-game`.
- Screen-shake quakes (`DEarthquake`) are sim thinkers (they can damage); the view offset is computed from them in `SimView`.

## 11. Performance targets

- nuts.wad (10k+ monsters awake) ≥ 35 tics/s single-threaded on a 2020 desktop CPU (release).
- Allocation-free steady-state tics (pre-size arenas; reuse scratch buffers in `WorldScratch`).
- Profile with `tracing` spans compiled out in release unless `profile` feature.

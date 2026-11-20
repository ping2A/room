# 36 — UZDoom feature parity: gap analysis and milestones

Status: in progress (2026-11-03). Audience: an engineer or LLM implementing
Room. Reference engine: **UZDoom** (the GZDoom continuation, GPLv3+). The
snapshot is `UZDoom/UZDoom` trunk at commit date 2026-10-28, which includes
the 5.0 feature set (released Aug 2026). The Room tree compared is from
2026-11-02. Companion docs: `docs/19` (compatibility), `docs/21`
(tracker), `docs/33` (Brutal Doom), `docs/34` §14 (ZScript audit).

> **Revalidation note (2026-11-05):** the parity tables below are a useful
> baseline, not a current generated diff. Some entries became stale as Room
> work landed (notably actor-flag storage and weapon overlays). Use the
> current deltas in §0.1 before treating any `❌` or count in §3/§6 as live.

---

## 0. TL;DR

1. Room already covers most of the Doom-family **content pipeline**:
   VFS, Doom/Hexen/UDMF maps, DECORATE, DeHackEd/BEX/MBF21/DSDHacked,
   UMAPINFO/ZMAPINFO, ACS, the GL-era renderer extras, MD2/MD3/OBJ/IQM/UE1/KVX,
   and lockstep netplay plus a server. In places it is **ahead** of UZDoom
   (§2): vanilla demo compatibility, cross-arch determinism, the browser
   build, WASM mods, the room server with SFU, and Duke 3D.
2. The big gaps are structural, not cosmetic:
   - **G1: general ZScript compatibility is incomplete** (`docs/34` §14).
     Room executes its VM and BD-focused scripts; UZDoom's own game logic is ~94.7 k lines of ZScript (295 files). About 590 of its
     ~700 `A_*` action functions are *written in ZScript*, not native.
   - **G2: the actor flag set is full.** `ClassDef.flags` and `Actor.flags`
     are a single `u64` with exactly 64 `FLAG_*` constants in
     `room-sim/src/class.rs`. UZDoom defines **315** actor flags, and about
     **180** are not recognised by Room. New flags cannot be added without
     changing the storage.
   - **G3: actions.** About **458** of UZDoom's 699 `A_*` names are unknown
     to Room. ~120 are generic or Doom-relevant (overlays, model/animation,
     view, sound, particles, MBF21/ID24 extras); the rest are Heretic, Hexen
     and Strife (M8/M9 deferred).
   - **G4: games.** Heretic, Hexen and Strife (M8, M9) are not implemented,
     and Strife needs conversations (USDF/`DIALOGUE`) and a UI.
   - **G5: input, audio and multiplayer basics** that UZDoom has:
     - game controllers now work through `gilrs` (round 3, §0.1); haptics
       and per-button rebinding in a menu remain;
     - reverb now works: `REVERBS`, `SoundEnvironment` zones and the
       stock UZDoom environments (§0.1, round 4), rendered by a port of
       OpenAL Soft's EAX reverb, on desktop and web (round 10);
     - 64 players and client-side view prediction landed in UZ12 (§0.1,
       round 9; `38-network-state.md`);
     - no ZScript menus; the accessibility options are a Graphics-menu
       section (flash reduction, extra light), not UZDoom's separate menu.
3. **Strategy (§4).** Finish the ZScript VM first (`docs/34` ZS1b–ZS4b),
   then run **UZDoom's own `wadsrc/static/zscript`**, which is GPLv3+ like
   Room. Room then implements **natives** (~1,830 declarations, used
   incrementally) instead of hand-porting ~590 ZScript actions and all the
   Raven/Strife actors. Doom's vanilla actors stay native Rust, because demo
   bit-compatibility is a Room feature UZDoom does not have.
4. Milestones **UZ0–UZ14** are in §5. The critical path is UZ0 → UZ1 (flags)
   → UZ2 (ZScript VM) → UZ3 (stdlib boot) → UZ8/UZ9 (Heretic/Hexen) → UZ10
   (Strife).

### 0.1 Current deltas from the 2026-11-02 baseline

These were checked against the current Room source and `docs/21` on
2026-11-05. They supersede conflicting statements in the baseline tables;
the unverified feature counts remain labelled as baseline numbers.

| Area | Current Room status | Evidence / remaining work |
|---|---|---|
| Portals (2026-11-19) | Line portals (visual, teleport, interactive, linked, Eternity pairs, `Line_QuickPortal`), stacked sectors (things and `Sector_SetPortal` 0), linked plane portals (6), copies, Eternity plane / horizon portals, sky viewpoints (`SkyViewpoint`, `SkyPicker`, `SkyCamCompat`), horizons and mirrors, set up by UZDoom's rules (`room-map/src/portals.rs`). The playsim moves, collides, sees, shoots, hears and aims through them (`room-sim/src/portal.rs`, `sight_portal.rs`); both shells draw each view at full resolution every frame (`room-render/src/portal_view.rs`); the automap overlays linked groups; `Line_SetPortalTarget` works; `LevelLocals.GetPortalGroupCount`. Round 2: position queries (collision, blasts, `A_RadiusThrust` / `A_RadiusGive`) see the groups they reach; polyobject portals (`UpdateLinks`, no rotation for linked ones); ZScript `Sector` portal queries, `LevelLocals.Vec2Diff` / `Vec3Diff` / `Vec2Offset` / `Vec3Offset` / `GetDisplacement` and portal-aware actor vectors; decals, particles, models and voxels inside portal views; translucent flats over portal planes. | `06-maps-geometry.md` §5, `13-renderer.md` §6. Round 3: ZScript `Line` portal natives on the new map objects (doc 34). |
| Actor flags | Room imports the full UZDoom name catalogue (315 known names); the generated name diff reports no missing UZDoom flags. | `room-sim/src/flags.rs`; this confirms recognition/storage only. Each flag's effect across movement, damage, AI, rendering, and serialization still needs behavioural verification (`UZ1`, code complete locally, CI pending). |
| Weapon overlays | Multi-layer psprites with UZDoom `PSPF_*` bits (round 7 in §0.1) and drawn vertex offsets (round 8); the blanket `❌` in §3.2 is stale. | `room-sim/src/player.rs`, `think_player.rs`, `room-ui/src/canvas_bridge.rs`. |
| ZScript stdlib/native bindings | The vendored stdlib type-checks and all 968 native imports bind with no return-zero stubs (round 12). | Several bindings are approximations or queued ops not applied yet (round 12 row); host natives trap outside their host. |
| ZScript object arrays and map objects (2026-11-19) | `Array<Actor>`-style arrays hold objects (`ObjKind::ArrayA`, traced); `level.Sectors` / `Lines` / `Sides` / `Vertexes` are real `Sector` / `Line` / `Side` / `Vertex` objects with their main fields, `CurSector` and `PointInSector` return them, and the `Sector` light / texture / plane and `Line` portal natives work on them. | `34-zscript-vm.md` "Object arrays and map objects". Not done: direct field writes reaching the playsim, `thinglist`, `LineTrace` data, float arrays. |
| ZScript completeness | BD-focused VM paths and native coverage are materially ahead of the original audit, but general GZDoom ZScript behaviour is not yet demonstrated. | `docs/21` ZS0–ZS10; custom ZScript menus and Cranelift remain deferred; GZDoom/BD end-to-end compatibility is still open. |
| Audio channel semantics | `A_StartSound` carries its full ZScript signature and channel flags through to the mixer; LOOP, UI, NOPAUSE, NOSTOP, OVERLAP, LOCAL, LISTENERZ and TRANSIENT are represented. `A_StartSoundIfNotSame` compares resolved sound identities against the active same-actor/same-channel voice and carries volume, attenuation, pitch, flags, and start offset in both the ZScript native and action dispatch. SNDINFO `$playersound`, `$playersounddup` and `$playeralias` now resolve `*name` sounds by actor sound class. | `room-sim/action.rs`, `room-game/sound.rs`, `room-audio/{event,mixer,reverb}.rs`; gender, pause, local-seat routing, listener-Z, saved loops and reverb landed in round 4 (row below). |
| MBF21 weapon actions | Added `A_CheckAmmo`, `A_RefireTo` and `A_GunFlashTo` to DECORATE action recognition and psprite dispatch; matching ZScript native imports now route through the same actions. | `room-sim/action.rs`, `action_known.rs`, `room-zvm/natives.rs`; weapon mode edge cases and ID24 end-to-end gameplay remain unverified. |
| Generic playsim actions | Added actor paths for visibility/collision/gravity flags, target/tracer removal and kill, count/link flags, active sounds, overlay clearing, view/weapon transforms, `A_CheckSpecies`, `A_ClearFlash`, per-weapon reload actions, particle spawn motion/fade actions, the UZDoom-style `A_BulletAttack` pellet loop, parameterized weapon light via `A_Light` (clamped to −20…20), and per-actor `LastHeard` propagation/clearing for noise alerts and look actions. `A_WeaponAlert` and `A_ClearSoundTarget` use the playsim soundtarget path. Count flag changes adjust level totals. | Generated diff as of 2026-11-08: 329 of 700 UZDoom action names present (see the counts below the table). Remaining approximations include custom `Damage` functions in `A_BulletAttack`, textured particle styles, and portal-aware sector linking (portals themselves landed 2026-11-19). Water splash alerts now honor `FSplashDef::NoAlert`, player-only alert generation, `DONTSPLASH`, and receiver-side `NOSPLASHALERT`; portal-aware sector membership remains. |
| Reflective actors | `REFLECTIVE` now deflects missiles; `THRUREFLECT` lets missiles pass through and retarget; `AIMREFLECT` and `MIRRORREFLECT` affect redirection. | Wired into missile collision; other reflectable attacks and all edge cases still need comparison. |
| ZScript Wads natives | `FindLump` and `FindLumpFullName` are bound against the play host's ordered in-memory lump view. | Namespace filtering and exact archive ordering are unavailable in that host view; partial compatibility. |
| ZScript native linkage | Lookup and coverage reporting now follow UZDoom's case-insensitive native-name resolution; actor `ChangeTID` now applies the queued TID update back to the live playsim actor. `Object.New` now allocates compiled script classes with their true VM field layout; unknown DECORATE/native classes retain the generic fallback. The vendored stdlib coverage command finds bindings for every referenced native name. `LevelLocals.GetChecksum` returns the map's lowercase MD5; `Actor.A_SpawnParticleEx` queues particle simulation/render data. | The focused sample still contains five explicitly marked `Stub` entries; it proves names link, not full semantic parity. Round 12 bound the rest of the UZ3 inventory (968 of 968). |
| Brutal Doom runtime | BIKERACE now has a working entry path; vehicle movement/audio/exit still need a full in-game comparison run. | `docs/33 §3.3`, recent `docs/21` log entries. |
| Game families | Heretic/Hexen (M8) and Strife (M9) remain deferred. | This is a deliberate scope gap, not a small feature-count discrepancy. |
| Armor model (2026-11-08) | Armor follows GZDoom `BasicArmor`. Damage absorbs `full + int((damage − full) × SavePercent)`, capped per tic by `Armor.MaxAbsorb` and fully absorbed up to `Armor.MaxFullAbsorb`. `BasicArmorPickup` / `BasicArmorBonus` subclasses read `Armor.SaveAmount`, `SavePercent`, `MaxSaveAmount`, `MaxAbsorb` and `MaxFullAbsorb`. Room's Doom armors keep GZDoom's defaults (100 @ 33.335 %, 200 @ 50 %, bonus 1 up to 200). An armor bonus is always taken, even at the cap (vanilla). `A_GiveInventory("GreenArmor", n)` wears the armor (`n` multiplies `SaveAmount`) instead of carrying it. ACS `CheckArmorType` and `GetArmorInfo` and the new `A_JumpIfArmorType` read the worn armor class. | **Behaviour change:** vanilla absorption now rounds down (`damage / 3`, `damage / 2`, as in `P_DamageMobj`). Room used to round up. Recorded Room bundles with armor hits may need re-recording. `Armor.MaxBonus` / `MaxBonusMax`, skill `ArmorFactor`, MAPINFO `NoArmor` damage types and Hexen armor landed in round 5 (row below). |
| Backpack (2026-11-08) | A backpack fills every base ammo type (directly under `Ammo`, or Room's Doom ammo). Each gets `Ammo.BackpackAmount`, doubled on skills 1 and 5, and its cap is raised to `Ammo.BackpackMaxAmount`. Room's Doom ammo has no values set, so it uses vanilla `P_GiveAmmo(…, 1)`: one clip each (10 / 4 / 1 / 20) with the cap doubled. Room used to give 1 of each. | Subclasses of an ammo type are not filled (GZDoom `GetParentAmmo`). An ammo type the player has never owned gets its cap raised only when the backpack gives some of it. |
| Property semantics (2026-11-08) | Now parsed with behaviour: the `Player.ViewHeight`, `Player.JumpZ` and `Player.AttackZOffset` spellings (only the bare names worked before; Brutal Doom uses the qualified ones). Also `Player.SideMove`, `Player.RunHealth` (at or below it, movement clamps to walk speed), `Inventory.Icon` and `Ammo.BackpackAmount` / `BackpackMaxAmount` from DECORATE. `Weapon.SlotNumber` and `SlotPriority` place weapons that KEYCONF does not list (`AddExtraWeapons`). `MissileHeight`, `RadiusDamageFactor`, `SelfDamageFactor`, `DamageMultiply` and `VSpeed` (spawn vertical speed) work. `TelefogSourceType` / `TelefogDestType` work, and teleports now honour `A_SetTeleFog`, which was stored before but never used. `Health.LowMessage` works. | Stored only: `Player.DisplayName`, `Player.MaxHealth`, `Player.DamageScreenColor`, `Stamina`, `StealthAlpha`. When `Player.SideMove` is unset, strafing keeps Room's old `ForwardMove` scale. GZDoom scales only forward movement there. |
| New actions (2026-11-08) | `A_PainShootSkull` (class, absolute angle, `PAF_*` flags, limit), `A_DualPainAttack`, and `A_PainAttack` with arguments. `A_BasicAttack` now reads GZDoom's order (melee damage, sound, missile, height); before it was routed to `A_CustomComboAttack`'s order. `A_ComboAttack` and `A_MissileAttack` use the class `MeleeDamage` / `MeleeSound` / `MissileType` / `MissileHeight`. Also new: `A_BetaSkullAttack`, `A_JumpIfMasterCloser`, `A_JumpIfArmorType`, `A_Tracer2`, `A_CountdownArg`, `A_Countdown`, `A_SetShadow`, `A_ClearShadow`, `A_LoopActiveSound`, `A_FLoopActiveSound`, `A_SetUserVarFloat`, `A_SetUserArrayFloat`, `A_CheckPlayerDone`, `A_FireOldBFG` and `A_CheckForResurrection` (custom state and sound, native path). `A_FaceConsolePlayer` is a no-op, as in UZDoom. `A_JumpIfTracerCloser` now takes GZDoom's `(distance, state [, noz])` and checks the height gap; Room's old `(state, distance)` order still works. | Approximations: the looping sounds play once per call, because Room has no looping actor voice. Float user variables are truncated to Room's integer user variables. |
| Parity tool (2026-11-08) | `room parity` now writes ZScript properties as DECORATE does: bare on `Actor`, else `Class.Name` or `property prefix:`. Room's side counts the names the class builder matches, not blanket prefixes such as `weapon.`. It previously counted only `is_known_property`, so `Inventory.MaxAmount` and other handled names were reported missing while unhandled `Armor.*` names were hidden. | Regenerated `docs/qa/uzdoom-parity.md`. |
| Action ports (2026-11-08, pass 2) | Every UZDoom action name now dispatches. `room-sim/src/uzact/` ports 369 bodies from the UZDoom ZScript to Rust: `generic.rs`, `heretic.rs`, `hexen.rs`, `strife.rs`. They use helpers that mirror `Spawn`, `SpawnMissile`, `LineAttack` and the other ZScript engine calls. Smoke test: `uzact::tests` runs each port three times on a monster with a live target, then on a dead one, and ticks the world. None panics. 234 of the 369 visibly act in that generic setup; the rest need a specific state (a corpse, a weapon owner, a counter). Lights from `A_AttachLight` / `A_AttachLightDef` are now drawn. `A_ChangeModel`'s MODELDEF swap is drawn. | These are line-by-line ports without per-game playtests. Heretic, Hexen and Strife still lack their IWAD flow (M8/M9), so most ports only run in mods that use those actors. Model animation (`A_SetAnimation*`, model flags) is stored but not drawn. |
| Property semantics (2026-11-08, pass 2) | Behaviour, not just parsing, for: `LightLevel` (with `+ADDLIGHTLEVEL`); `ShadowAimFactor` / `ShadowPenaltyFactor`, plus Doom's `+SHADOW` aim spread in `A_FaceTarget`, also for a player with a BlurSphere; `MinRespawnTics` and `RespawnDice` (nightmare respawn); `Inventory.RespawnTics`, `FakeInventory.Respawns`, `+INVENTORY.NEVERRESPAWN` / `ALWAYSRESPAWN` (item respawn when `respawn_items` is on); `Inventory.GiveQuest`; `Inventory.InterHubAmount`; `HealthPickup.AutoUse` 1–3 and manual use. On players: `AirCapacity` (drowning after 20 s × capacity at `waterlevel` 3, `*gasp` on surfacing), `FallingScreamSpeed`, `ViewBobSpeed`, `FlyBob`, `WaterClimbSpeed`, `MaxHealth` (stimpacks, medikits, mugshot), `MugshotMaxHealth`, `DisplayName` (SBARINFO `PlayerClass`), `Portrait` (the new class menu), `FlechetteType`, `HealRadiusType` and `InvulnerabilityMode` (reflective / ghost). Also: `PuzzleItem.Number` / `FailMessage` / `FailSound` with the `UsePuzzleItem` special on lines and things; `WeaponPiece.Number` / `Weapon` (assemble after `Health` pieces); `Weapon.SisterWeapon` (`PowerWeaponLevel2` swap); `Weapon.YAdjust`; `Armor.MaxBonus` / `MaxBonusMax`; `StealthAlpha` (`+STEALTH` fade in on attack and pain, out on chase); `PoisonDamage damage, duration, period` with `PoisonDamageType` and `+ADDITIVEPOISON*`, plus Hexen `poisoncount`; `MaxSlopeSteepness` (only for classes that set it); `SpectatorCamera.LagDistance` / `ChaseMode` / `ChasingTracer`. In rendering: `RenderRequired` / `RenderHidden`, `VisibleAngles` / `VisiblePitch` with `+ABSVIEWANGLES`, `SpriteRotation` (sprite quads roll; `+ROLLSPRITE` adds the actor roll), `+SPRITEANGLE`, `DynamicLight.SpotInnerAngle` / `SpotOuterAngle` (map-placed `DynamicLight` actors are drawn) and `AutomapOffsets` (things on the second `iddt`). | Stored but not drawn: `Weapon.BobPivot3D` (Room draws no first-person 3D weapon models) and `Powerup.Colormap`. `RenderRadius` has no effect because Room has no per-sector sprite culling. Drowning and poison damage are typed (`Drowning`, `PoisonDamageType`); poison uses the poisoner's class type rather than the dart's. |
| Generic powerups (2026-11-08) | A `PowerupGiver` whose `Powerup.Type` names any `Power*` class now works (`room-sim/src/powerups.rs`). It honours `Powerup.Duration` (negative = seconds), `Strength`, `Mode` and `Color` (screen blend), plus `Player.DamageScreenColor` for the pain flash. Handled powers: Invulnerable, Invisibility, Ghost, Shadow (`Mode` / `Strength` alpha), IronFeet, Mask, LightAmp, Torch, Strength, Flight, Speed (×1.5), Damage (×4), Protection (¼), Drain, Regeneration, HighJump (`Strength` × `JumpZ`), InfiniteAmmo, Buddha, Frightener, Reflection (with `ReflectType`) and WeaponLevel2. Picking a running power up again refreshes its time. Auto-activate givers are used up on pickup; others work from the inventory bar. | DoubleFiringSpeed, TimeFreezer, Targeter and Scanner landed in round 5 (row below). Not modelled: Morph (the existing vehicle path still handles `PowerMorph`). Doom's own spheres keep their dedicated code. |
| Player travel and pistol start (2026-11-08) | Leaving a finished level now carries each live player's inventory, ready weapon, armor, backpack and health into the next map, as `G_PlayerFinishLevel(FINISH_NoHub)` does. Keys and anything with `Inventory.InterHubAmount` below 1 are dropped. Inventory-bar items are cut to their `InterHubAmount`. Moving inside a MAPINFO hub keeps everything. MAPINFO `ResetInventory` / `ResetHealth` are honoured. `GameOptions.pistol_start` (UZDoom `sv_pistolstart`) turns travel off. Before this, Room pistol-started every map. | **Behaviour change** for normal play. Recording a demo bundle (`record_demo`) still pistol-starts each map, because a bundle replays every map from a fresh world: it stores tics and a seed, not the carried inventory. The console's `changemap [map] [-1]` now goes through the same travel; `-1` pistol-starts (round 3). |
| Maps, console, input and accessibility (2026-11-08, round 3) | **UDMF:** vertex heights (`zfloor` / `zceiling`) slope triangular sectors; an explicit `floorplane` / `ceilingplane` still wins. New keys with behaviour: linedef `blockuse` (stops the use trace), `passuse` (the use goes on to lines behind), `blocklandmonsters`, `monsteractivate` (sets `ML_MONSTERSCANACTIVATE`, which monsters' use now honours), `jumpover` (Strife railing: a 32-unit fence over the higher floor; `ML_RAILING` from `Line_SetBlocking` now works the same way), `revealed` (as `mapped`), `drawfullheight` (automap draws the line as a wall), Strife `translucent` / `transparent` (alpha 0.75 / 0.25). Sector `noattack` (monsters in it never start an attack), `hurtmonsters`, `harminair` (UDMF sector damage now skips airborne actors otherwise), `damagehazard` (Strife hazard meter: fills while standing in it, hurts 5 every 32 tics past 16 s), `hidden` (no automap lines). Thing `standing`, `strifeally`, `invisible`, `translucent`, and `coop` / `dm` = false for multiplayer-only things. The automap now also hides `ML_DONTDRAW` lines in play. **Console:** `changemap [map] [-1]` (no map = MAPINFO `next` or the stock order; keeps inventory unless `-1`), `changeepisode <n>`, `doublebind` / `undoublebind`. **Input:** game controllers via `gilrs` (`gamepad` feature, on by default): analog move and look with a radial dead zone and response curve, triggers, a default button layout, D-pad / A / B / Start in menus, and the `use_joystick`, `joy_deadzone`, `joy_curve`, `joy_sensitivity`, `joy_speedmultiplier`, `joy_invertpitch` CVars. Double-tap binds fire on a second press within `m_doubleclicktime` (250 ms). **Rendering / UI:** `+INTERPOLATESCALE` / `+INTERPOLATEALPHA` lerp scale and alpha between tics; `+CASTSPRITESHADOW` / `+NOSPRITESHADOW` choose which actors get `r_actorspriteshadow` shadows; `gl_texture_filter_anisotropic` (1–16, switches world textures to trilinear filtering); accessibility `r_reduce_flashing` (pain / pickup / powerup flashes at 35 %) and `r_extralight` (0–4, +8 light per step), in a new Graphics-menu section; `showendoom` prints ENDOOM / ENDTEXT / ENDSTRF in ANSI colour on quit. | `norespawn`, `noskywalls` and `zoneboundary` now have behaviour (round 4 row). Controller buttons cannot be rebound yet (fixed layout), and there is no haptics. `doublebind` lines in ini files are not loaded. The 5.0 post uniforms wait for custom post-process shaders (SH3/SH4), which have no uniform block yet. UDMF has no side-alpha or "don't draw side" key in UZDoom's spec, so that row only applies to UZDoom's own 5.0 sidedef extensions. |
| Sound, reverb, particles and sky walls (2026-11-08, round 4) | **Reverb:** `room-audio/src/reverb.rs` holds UZDoom's built-in environments (`s_environment.cpp`, incl. "DSP Water") and its stock `reverbs.txt` (vendored, GPL-3.0-or-later); `REVERBS` / `SNDEAX` lumps add or override them, with unset fields filled from the `Environment` base as `S_ReadReverbDef` does. Sectors flood into sound zones across two-sided lines, stopped by UDMF `zoneboundary` (`room-map/src/sound_zones.rs`); non-dormant `SoundEnvironment` things (9048, `args[0] << 8 \| args[1]`) set their zone (`room-sim/src/sound_env.rs`); MAPINFO `DefaultEnvironment` (name or `id1, id2`) fills the rest. The listener's zone picks the environment each frame; under water (`waterlevel` 3) "DSP Water" muffles world sounds. The mixer renders it with early taps plus 8 damped combs and 4 allpasses per side, tuned from decay time, HF ratio, room size, diffusion and the room / reflections / reverb levels. UI sounds stay dry. CVars `snd_reverb`, `snd_waterreverb`. **Sound control:** pause holds world voices in place and drops one-shots started while paused (`CHANF_NOPAUSE`, `UI`, `FORCE` and loops are exempt); `CHANF_LISTENERZ` plays at the listener's height; `CHANF_LOCAL` plays only when its source is the local pawn or camera, and seat-addressed sounds only for that seat (`Headless::drain_audio_for`); `CHANF_SINGULAR`; CHAN_AUTO from an actor no longer replaces the actor's last CHAN_AUTO sound (UZDoom sets `CHANF_OVERLAP`); `A_StopSound(slot = CHAN_VOICE)`, `A_StopSoundEx(slot)` and `A_StopSounds(min, max)` stop only those channels of the calling actor, and `A_StopAllSounds` is `A_StopSounds(0, 0)` on the caller (it used to silence the whole level). Gender: the `gender` CVar (default `neutral`, as UZDoom) and per-player `Player::gender` pick `$playersound` sets with UZDoom's names (male, female, neutral, other) and fallback order. Saves store looping voices in `sounds.txt`; a load stops the old world's voices and restarts those loops. **Particles:** `A_SpawnParticleEx` textures (a name from DECORATE; a `TextureID` from ZScript, now that `TexMan.CheckForTexture` / `GetName` and `TextureID.IsValid` / `IsNull` / `Exists` / `SetNull` / `SetInvalid` work) draw untinted; particles face the camera, centred (`SPF_FACECAMERA` faces the eye, `SPF_NO_XY_BILLBOARD` stays upright, `SPF_STRETCHPIXELS` stretches 1.2×); styles Add / AddStencil / AddShaded, Normal / Translucent / Stencil / TranslucentStencil / Shaded, SoulTrans (⅔ alpha), Fuzzy / OptFuzzy / Shadow (dark, 30 %) and Subtract (approximated by a dark tint). **Maps:** sky walls: geometry behind a sky-capped wall is hidden, as GZDoom's `SkyTop` and vanilla do (depth-only quads from the sky ceiling up; the sky is painted first). UDMF `noskywalls` on the line or sector turns them off. Respawning players use their own start (deathmatch: a deathmatch start) and skip starts in UDMF `norespawn` sectors. | **Behaviour changes:** sky walls hide things that used to show above sky-capped walls; untextured particles are now centred camera-facing quads (they used to be Z-only billboards standing on their position); a dead coop player respawns at their own start instead of player 1's. The reverb approximation, the missing `SPF_*` flags, subtractive blending, `SetEnvironmentID` after map start and web reverb were finished in round 10 (row below). |
| Armor, powerups, chase and ZScript 5.0 (2026-11-09, round 5) | **Armor:** `Armor.MaxBonus` / `MaxBonusMax` raise the bonus cap; skill `ArmorFactor` scales `SaveAmount` (not with `+INVENTORY.IGNORESKILL`); MAPINFO `damagetype` blocks (`NoArmor`, `Factor`, `ReplaceFactor`) apply when the actor has no own factor for that type, and `NoArmor` skips armor. Hexen armor: `Player.HexenArmor` base and slot increments, `HexenArmor` pieces (slot = `Health`), `ArtiBoostArmor` (+1 per slot), absorption and wear as UZDoom `AbsorbDamage`, and the HUD armor class. **Powerups:** `PowerDoubleFiringSpeed`; `PowerTimeFreezer` (non-players and other players stop, the owner and teammates keep moving, `+NOTIMEFREEZE` exempts, the last seconds blink, level time keeps running, world audio pauses); `PowerTargeter` (TRGT pieces around the crosshair, spread by accuracy, blinking at the end); `PowerScanner` (full automap while it lasts). **Chase:** DECORATE `A_Chase` with arguments, `A_ExtChase` and the ZScript native run UZDoom's chase order; fast-chase strafe lives on the actor. **ZScript 5.0:** const-method mutability, trailing `unsafe(const)`, deprecated classes, the rest of Dictionary / DictionaryIterator. | **Behaviour changes:** the freeze cheat now also stops non-player actors (UZDoom `isFrozen`); `A_Chase(args)` in DECORATE now honours its arguments (they were ignored); UZDoom-path chasers go to their Idle (else Spawn) state when they lose their target. Bare `A_Chase` is unchanged. A 4.15.1+ script that writes fields from a `const` method no longer compiles, as in UZDoom. Not done: `PowerMorph` outside the vehicle path. (Chase targeting across linked portals followed on 2026-11-19.) |
| ZScript object model (2026-11-09, round 6) | **VM:** script objects have a float bank (`fields_f`, `LoadFieldF` / `StoreFieldF`). `double` / `float` fields keep fractions (they were stored as truncated ints), and `vector2` / `vector3` / `vector4` / `quat` fields take 2–4 float slots, so `pos.x`, `pos.y` and `pos.z` are separate values (they all read one int slot before). **Layouts:** a subclass's fields now start after its parent's (they used to reuse the parent's slots from 0, so a mod field overwrote an inherited one), and `extend class` fields are appended to the class. **Lowering:** vector locals, tuples `(x, y, z)`, `+ - * /` on vectors, dot product, `.xy` / `.xyz`, `Length()`, `Unit()`, `AngleToVector`, swizzle assignment (`vel.xy *= f`), compound float assignment, float `++` / `--`, chained `a = b = 0`, float `%` and float comparisons (int-truncated before). `class<T>` values are carried as names. Native calls through a field (`target.CountInv(…)`) call the native (they called function 0); vector parameters pass their components and natives can return vectors (`CallNat` b = 4). **Actor fields:** before a ZScript call (`Tick`, `PostBeginPlay`, ZScript state actions) the playsim copies the actor's native fields into its script object — `pos`, `vel`, `angle`, `pitch`, `roll`, `health`, `speed`, `alpha`, `scale`, `height`, `radius`, `floorz`, `ceilingz`, `gravity`, `special1/2`, `tid`, `target`, `tracer`, `master`, `lastenemy`, `goal`, … — and the same for the actors those pointers name; after the call it writes changed values back. **World natives** (`room-zvm/src/world_natives.rs`): `Distance2D/3D(Squared)`, `AngleTo`, `Vec2To/Vec3To`, `Vec3Offset`, `Vec2Offset(Z)`, `Vec2Angle/Vec3Angle`, `SetOrigin`, `SetZ`, `AddZ`, `GetZ`, `GetPointer`, `CheckSight` (playsim line of sight), `CheckMeleeRange` (playsim melee check), `CountInv`, `FindInventory`; `math.sqrt`, `exp`, `log`, `fmin/fmax`, `deltaangle`, `absangle`. | **Behaviour change:** ZScript code that relied on the old int storage of `double` fields now sees fractions. Not done: actors reached other than through a pointer of the caller (iterators, `Spawn` results) are not mirrored; `CheckMissileRange` still approximates (the playsim check draws random numbers, so it is not precomputed); script functions cannot return vectors yet; `super.Tick()` stays a no-op because the native tick always runs first. Play sessions still compile mods against Room's small `base.zs`, not the vendored UZDoom stdlib, so mods that use most of the stdlib do not compile in play yet. |
| Weapon overlays and ZScript status bars (2026-11-09, round 7) | **Overlays:** any psprite layer id now works, not just the gun (1) and one extra (2). Each layer (`Player.psp_layers`) has its own state, tics, sprite, offset, alpha, scale, pivot, rotation, translation, render style, flags and vertex offsets. `A_Overlay(id, state, nooverride)` starts one; it runs its own state actions (`OverlayID()` returns its id, and `0` as a layer argument means the calling layer) and ends when its state ends. Layers below 0 draw under the gun, layers between 1 and 1000 above it, and layers above 1000 above the flash. `A_ClearOverlays(start, stop, safety)` clears a range; with `safety` it keeps the gun and the flash. A layer goes away when the weapon that started it is no longer the ready weapon, and a new layer does not tick in the tic it was created. Overlay actions called from an actor's own state (custom inventory, `CallStateChain`) are queued and run on the player's layers. **`PSPF_*` bits now match UZDoom** (`ADDWEAPON` 1, `ADDBOB` 2, `POWDOUBLE` 4, `CVARFAST` 8, `ALPHA` 16, `RENDERSTYLE` 32, `FLIP` 64, `FORCEALPHA` 128, `FORCESTYLE` 256, `MIRROR` 512, `PLAYERTRANSLATED` 1024, `PIVOTPERCENT` 2048, `INTERPOLATE` 4096); a new layer starts with `ADDWEAPON`, `ADDBOB`, `CVARFAST`, `POWDOUBLE` and `PIVOTPERCENT`. As in UZDoom, a layer's own alpha only counts with `PSPF_ALPHA` (with `FORCEALPHA` it replaces the player's) and its own style only with `PSPF_RENDERSTYLE` / `FORCESTYLE`. **Status bars:** `GameInfo { StatusBarClass = "…" }` in MAPINFO (or `ROOM_STATUSBAR=<class>`) compiles the vendored UZDoom ZScript library with the mod's own ZScript (`room_game::load_status_bar_program`) and runs that class's `Init` once and `Draw(state, ticfrac)` every frame (`room-ui/src/script_sbar.rs`). The library's `BaseStatusBar` / `DoomStatusBar` code runs as written: `BeginStatusBar` / `BeginHUD`, `DrawImage`, `DrawTexture`, `DrawString` with `HUDFont` (Doom's big digits, small digits and console font), `Fill`, `FormatNumber`, `GetMugShot`, `GetInventoryIcon`, `HasWeaponsInSlot`, `CheckKeys`, `FindInventory`, `CountInv`, and `DI_SCREEN_*` / `DI_ITEM_*` alignment. The player, pawn, ready weapon, its ammo and the inventory chain are copied in each frame from `World::status_bar_items`. Without a status bar class the native bar draws as before. **ZScript:** functions can return several values (`return a, b;`, `[x, y] = F();`), fixed-size local arrays (`bool locks[6]`) have their size, a local shadows a constant or class of the same name, enum names resolve within the calling class first, and calls on `TextureID` locals reach the type's natives. | Not done (vertex offsets, fullscreen toggle, inventory bar and scoreboard: see round 8): multi-returns do not carry vectors; natives get at most 4 float arguments; play sessions still compile mods against `base.zs`, so the status bar is a separate program that does not share state with the playsim's ZScript. |
| Overlay corners, HUD toggle, inventory bar and scoreboard (2026-11-09, round 8) | **Vertex offsets:** `A_OverlayVertexOffset` now draws, on the gun, layer 2 and every other layer: corners in UZDoom's order (top-left, bottom-left, top-right, bottom-right) are moved, scaled with the layer, mirrored with `PSPF_FLIP`, turned with the layer's rotation and mapped as two triangles like UZDoom's hardware renderer (`room-ui/src/canvas_bridge.rs` `warp_rgba`). `WOF_KEEPX` / `WOF_KEEPY` / `WOF_ADD` are honoured on layers 1 and 2 too. **Fullscreen HUD:** F5 now switches a ZScript status bar between `Draw(HUD_StatusBar)` and `Draw(HUD_Fullscreen)`, as it already did for the native bar. **Inventory bar:** the playsim keeps a selected item (`InvSel`) among `+INVENTORY.INVBAR` items in pickup order (`room-sim/src/invbar.rs`). `invnext` / `invprev` (PgDn / PgUp by default) move it and open the bar for 5 seconds; `invuse` (End, or the existing `'` key) uses it with the bar closed and only closes the bar when it is open, as UZDoom's `CCMD(invuse)`. The commands travel as one-tic buttons (`Buttons::INVNEXT` / `INVPREV`), so they are deterministic in netplay and demos. Both bars draw it: the selection takes the face's place, and the open bar shows seven boxes from (48, 169) with counts (centred at the bottom in fullscreen). The ZScript bar runs the library's own `DrawInventoryBar` (`InvSel`, `InvFirst` kept between frames, `NextInv`, `inventorytics`, `TexMan.GetScaledSize`). Items without `Inventory.Icon` fall back to their spawn frame. **Scoreboard:** hold `\` (`+showscores`), or die in deathmatch, for a list of players: name, frags and deaths in deathmatch (sorted by frags), kills / items / secrets as percentages of the level totals in co-op (`World::scoreboard`, `room-ui/src/scoreboard.rs`); per-player counts are new (`World::player_stats`). **ZScript compiler:** vector parameters of script functions now arrive as their components (they were one int register, so `DrawInventoryIcon(item, (160, 198))` lost its position); arguments left out of a script call get their declared defaults (`alpha = 1.`, `scale = (1, 1)`) instead of zeros; a statement's temporary registers are reused by the next statement, so long functions no longer pass 255 registers (seven stdlib functions did, `LevelCompatibility.Apply` needed 4606, and silently wrapped); a function that still needs more than 255 is reported as an error; locals declared without a value start at zero. | Natives still read their own defaults for left-out arguments (they are shared with DECORATE). Per-player counts are not saved in savegames. The scoreboard has no ping or time columns and no team totals. Short `+INVBAR` (without `INVENTORY.`) does not mark an item for the bar. |
| Multiplayer to 64 players (UZ12, 2026-11-09, round 9) | **64 seats:** `MAX_PLAYERS` is 64 (UZDoom `MAXPLAYERS`). `World::players` is a 64-slot `Vec`, `Player::frags` a 64-entry `Frags`, bot and `PowerTimeFreezer` seat masks are `u64`. Seats 5–8 use ZDoom's Player 5–8 starts (4001–4004); seats past the map's starts look for free floor on rings around Player 1's start (walkable from it, clear of walls and pawns). The world checksum always covers the first 8 seats and covers a higher seat only while it is in the game, so games of up to 8 players hash exactly as before (every committed checksum test still passes). **Protocol v3** (`room_proto::PROTOCOL_VERSION`): `TicFrameMsg.seat_mask` is `u64`; a frame whose seats all fit in 8 bits is sent in the unchanged v2 layout (`MSG_TIC_FRAME`), a wider one as `MSG_TIC_FRAME_WIDE` (8-byte mask, up to 1100 bytes) and batches as `MSG_TIC_BATCH_WIDE`. Server Doom rooms take 2–64 seats (Duke stays 8); the tic ring grows with the room instead of a fixed 8-seat array; spectator logs, the verifier and `room-load` handle 64. The native TCP relay takes 64; the UDP and TCP meshes keep 4 and 8. **Client prediction:** `cl_predict 1` draws the local player where its unreleased netplay input puts it (`World::predict_player_view`, `room-sim/src/predict.rs`). It walks a copy of the pawn through `LockstepSession::pending_local` with `move_player`'s thrust rules and the read-only `check_position`, and never touches the world, so with it off (the default) play is bit-identical. `ROOM_NET_DELAY=N` (same on every peer, 1–8) raises the lockstep input delay so latency stops stalling; prediction hides that delay. **Intermission ready-vote:** in netplay the intermission ends when every seat has pressed use or fire (counted from the lockstep commands, so all peers go on the same tic; fire held from the level does not count; bots are ready). The screen shows `READY n/m`. **Net saves:** only the host can save or load a net game. The request rides in the host's next command (`Buttons::NETSAVE` / `NETLOAD`, slot in `weapon_select`, stripped before the playsim, as vanilla's `BTS_SAVEGAME`), and every peer saves or loads its own `net<N>` copy on that tic. Saves now carry every seat (`players.bin`: pawn, health, armor, powers, weapon, inventory, flags); single-player saves are unchanged. | Prediction (native and, since the same day, web — see `38 §5`) models walking, and since 2026-11-18 jumps, gravity and ledges; not flying, swimming height, pushers or conveyors. It is on by default since 2026-11-18. Joiners use the host's `ROOM_NET_DELAY` (sent in Welcome) since 2026-11-18. A peer without the host's net save file desyncs on load. Players past 8 have no colour of their own and respawn on Player 1's start. |
| EFX reverb, particle flags and live environments (UZ5, 2026-11-09, round 10) | **Reverb:** `room-audio/src/efx.rs` is a port of OpenAL Soft's EAX reverb (`alc/effects/reverb.cpp`, LGPL-2.1-or-later, used under the GPL-3.0-or-later), the device UZDoom renders through. Each `REVERBS` entry maps to `AL_EAXREVERB_*` as UZDoom's `LoadReverb` does (density = size³/16, mB → gain, clamped to the EFX ranges). The network has a B-format input, a 4-line early stage (decorrelated taps, all-pass, reflected secondary delay), a 4-line late FDN with a cubic-interpolated modulated read, 3-band T60 shelves (`DecayHFRatio` / `DecayLFRatio` / `DecayHFLimit` with air absorption), a vector all-pass and a scattering feedback matrix, then `ReflectionsPan` / `LateReverbPan` focus and OpenAL Soft's first-order stereo decode. `GainHF` / `GainLF` are the master shelves at `HFReference` / `LFReference`; a change of decay, density, diffusion or modulation cross-fades to a fresh pipeline while the old tail rings out, and smaller changes glide over 256 samples. As in current OpenAL Soft, `EchoTime` / `EchoDepth` do nothing. The mixer feeds the reverb a first-order B-format send per voice (its direction from the listener), shaped by OpenAL's `WetGainAuto` distance rule with UZDoom's manual rolloff (`0.001^(d·(1−g)/(T·343.3))·(1−g) + g`). **Under water** (`waterlevel` 3 or a "DSP Water" zone): world voices play ×0.7937 lower (UZDoom `PITCH_MULT`, even with `snd_waterreverb` off); with `snd_waterreverb` the "Underwater" preset replaces the zone and the direct path gets OpenAL's 5 kHz shelf at `GAINHF` 0.125; UI voices are left alone. `snd_reverb` off is UZDoom without an effect slot. **Web:** `room-web` `pump_audio` now sets the environment, pause and under-water state like the native shell (`?reverb=0`, `?waterreverb=0`). **Particles:** `SPF_LOCAL_ANIM` runs a textured particle's ANIMDEFS animation from its first frame on its own clock, held during a level freeze (`SwitchTic++`); without it the particle shows the level animation frame (textured particles were not animated before). Without `SPF_ALLOWSHADERS` a particle draws with the default shader (no GLDEFS brightmap), as UZDoom's `OverrideShader = 0`. `STYLE_Subtract` particles draw with a reverse-subtract pipeline (`dst − src`, destination alpha kept) in back-to-front order with the other sprites. **Environments after map start:** dormant `SoundEnvironment` things are kept with their tid; `Thing_Activate` applies their environment to the zone (also when not dormant, as UZDoom's `Activate`), `Thing_Deactivate` only marks them dormant. ZScript `Sector.SetEnvironmentID(id)` and `Sector.SetEnvironment(name)` act on the calling actor's sector (`CurSector`, the only form UZDoom's stdlib and mods use; REVERBS names come from the shell). Zone environments and thing dormancy are saved (`soundenv.bin`; older saves keep the map's initial ones). **Checksum:** particle requests, FX cues and zone environments stay out of `World::checksum` (test). | **Tests:** impulse responses measure T60 within 15 % of Generic / Cave / Arena at flat ratios, the first output arrives at `ReflectionsDelay`, "Off" is silent, pan vectors steer, HF / LF ratios shape the tail, modulation changes it, a mid-tail environment switch has no click, the shelf reaches `gain²` (OpenAL Soft's RBJ form, so an HF ratio above 1 lengthens the broadband tail more than the ratio says, as on an OpenAL device); a left source's early reflections are louder on the left; under-water pitch and muffling skip UI voices; particle golden (fade-in-out, fade-in-hold-out, roll, growth, gravity, freeze); `SPF_LOCAL_ANIM`, `SPF_ALLOWSHADERS`, subtract range flags; dormant thing activation by tid, name lookup and `soundenv.bin` round trip; `CurSector.SetEnvironmentID` from ZScript against the UZDoom stdlib. **Behaviour changes:** the reverb sounds different (and denser) than the old Freeverb network; textured particles now animate; Subtract particles darken the scene instead of drawing a dark tint. **Limits:** other sector references (`Level.Sectors[i].SetEnvironment…`) still have no sector objects in Room's ZScript; ANIMDEFS `rand` tics use their mean for particles as for walls. *(2026-11-18: `level.PointInSector(pos).SetEnvironment…` now reaches the sector at that point, since `PointInSector` returns a sector reference that keeps the point; `Level.Sectors[i]` still has no sector objects. ANIMDEFS `rand min max` frames now get seeded random durations, for walls, flats and particles, from a 32-pass schedule per animation instead of the mean.)* Web particles followed on 2026-11-10: `room-web` now drains the sim's particle requests every tic into the same `ParticleStore` (cleared on a new level, not spawned during spectator catch-up), uploads the soft-disc textures, and draws them with `append_particles_animated`, so `LOCAL_ANIM`, `ALLOWSHADERS`, subtract and `r_particlefx` stretch match desktop. Before this the web build never drained the requests, so they piled up for the whole level. The desktop-only gore and weapon FX (blood sprays, casings, tracers…) stay desktop-only. |
| Alt HUD and renderer extras (UZ4 + UZ6, 2026-11-10, round 11) | **Alt HUD:** `hud_althud` (with `hud_scalefactor`, `hud_aspectscale` and UZDoom's `hud_*` defaults) runs the vendored `AltHud` class in the fullscreen HUD. Its natives: `Screen.DrawText` / `DrawTexture` with DTA tags, `TexMan.GetSize`, `TextureID.Exists` / `IsNull`, `WeaponSlots`, `Key.GetKeyType*`, `GetDefaultByType`, `FindInventory`, `GetClass`, `StatusBar`, `GameInfo`. **ZScript compiler:** `vararg` natives get their extra arguments with a bank layout (`String.Format` does full printf, `%i` too); `in out` parameters; string inference for ternaries, members and string-array indexes; name / class arrays are string arrays; `Array.Find` returns `Size()` when missing (and `FindS` for strings); `TextureID` locals keep their type; `Level.X()` binds to `LevelLocals`. **Sprites:** `+ROLLSPRITE` / `+ROLLCENTER` roll, `SpriteAngle` (now a property too), `SpriteRotation` picks the rotation frame only, `SPRITEFLIP`, the square-pixel scale, `+NODYNAMICLIGHTING`, isometric sprites under an orthographic camera; shadows are the sprite itself squashed onto the floor (`r_actorspriteshadow` 0/1/2, `r_actorspriteshadowalpha`, `r_actorspriteshadowfadeheight`). **Camera:** `SetViewPos` out-of-bounds and orthographic flags. **Fog:** MAPINFO `skyfog`, `skymist`, `skymistyscale`, `useskymist`, `thickfogdistance`, `thickfogmultiplier` (floats in MAPINFO values now parse) and `Level.SetSkyFog` / `SetThickFog` / `ChangeSkyMist`; the unlit shader thickens fog past the distance and mixes the sky toward the fog colour. **UDMF:** sidedef `alpha` and `blockrendering`. **Post:** GLDEFS post shaders (`Name`, `Shader "…" 330`, `Uniform`, `Texture`, `Enabled`) compile against UZDoom's prolog with `InputTimeDelta` / `InputTime` / `InputTimeGame` and `LastInputTexture`, and `Enabled` ones run after the scene. Goldens: `uz4_*`, `uz6_*`. | The mist texture layer is not drawn; `PPShader` natives and the other post targets are missing; the web build has no fog; a blocking side culls less than UZDoom's clipper because Room's BSP walk only stops on a closed view. |
| Stdlib natives and model bones (UZ3 + UZ7, 2026-11-10, round 12) | **UZ3:** all 968 native imports of the vendored stdlib bind, and none is a return-zero stub (`uz_stdlib.rs` asserts it). New `room-zvm/src/uz3_natives.rs`, in four groups. (1) Computed: `GetClass`, `Class.GetClassName` / `IsAbstract` / `GetParentClass`, `GetReplacement` / `GetReplacee`, `Color(...)`, `Quat.FromAngles`, `CheckFOV`, `CheckIfTargetInLOS`, `CheckIfInTargetLOS`, `CheckLOF`, `CheckMissileRange`, `CheckKeys`, `CheckProximity`, `GetFriction`, `PitchFromVel`, `HasWeaponsInSlot`, `SlotSize`, `InStateSequence`, `String.GetNextCodePoint` / `AppendFormat`. (2) Queued for the playsim (`room-sim/src/zs_native_ops.rs`), applied after the action or world event: `Activate` / `Deactivate`, `DestroyAllInventory`, `StartSoundSequence`, `DisplayNameTag`, `A_SetAnimation*`, model flags, all bone setters and `ClearBoneOffsets`, `PoisonPlayer`, subtitles and logs, `Level.ReplaceTextures`, `PPShader.SetEnabled` / `SetUniform*` (into `World::post_shader_ops`). (3) Engine-managed (the playsim already does the work): `BeginPlay`, `Tick`, `PostBeginPlay`, floor clip, water level, interpolation clears. (4) Host natives (2D `Screen`, status bar, menus, `LevelPostProcessor`): when no host VM replaces them, they trap with the host's name instead of returning zero. **Compiler:** `Super.F()` is a static call to the parent body (it was dispatched virtually); `dot` / `cross` vector operators; `Quat(...)` / `Vector3(...)` literals; class- and string-typed locals call `Class.` / `String.` natives. **UZ7:** bone setters keep UZDoom's per-bone state (`BoneOverrideComponent`: add or replace, previous and current value, interpolation over the duration) on the actor (`ActorModelState::bones`). `A_SetAnimation` keeps the previous animation so the renderer blends from its pose over `interpolateTics` (`SAF_INSTANT`, `SAF_LOOP`, absolute start / loop / end frames as in `SetAnimationInternal`). `A_ChangeModel` stores its animation file. `room-render/src/iqm_anim.rs` reads IQM animations, samples a frame pose (lerp of two frames, shortest-arc normalized quaternion lerp), blends two poses, applies bone overrides and builds the joint matrices; the model catalog CPU-skins IQM actors with them. Tests: `golden_uz7_bones.rs` (bone-override golden `uz7_bones.txt`, blend = lerp of frames), `queued_bone_setter_records_its_arguments`, `uz3_super_calls_run_the_parent_body`. | Queued but not applied yet: `SetFullbrightMode`, `SetSafeFlash`, `SpotState`, `DynamicLight` toggles, `Sector.SetColor` / `SetFade` / `RemoveForceField`. `post_shader_ops` is not drained into the desktop post runners yet. The bone getters (`GetBoneIndex`, `GetBoneOffset`, `GetBoneBaseTRS`…) return "no bone" defaults, because the playsim has no model data; `TransformByBone` approximates with the actor's yaw frame. `CheckMissileRange` is deterministic (no RNG roll). The vendored UZDoom has no `AnimateBones` virtual, so there is nothing to call. Animated IQMs are drawn in the desktop app only (not the web build). |
| Brutal Doom HUD and gore fixes (2026-11-10, round 13) | **SBARINFO** is drawn (it was parsed only): a mod's lump replaces the stock bar in `room-app` and `room-web`. `SbarInfoBar` places the `normal` bar like UZDoom's `forcescaled` `VirtualToRealCoords` (full height, 4:3 wide, so Brutal Doom's magazine panel at x −49 sits in the left margin) and `fullscreenoffsets` HUDs from the screen edges. Parser: brace-less bodies (`else InInventory X, 1 { … }`, `alpha 0.75` + one command) and `monospacefonts true, "0"`. `World::status_bar_items` reports running powerups (`powerups::effect_tics_for_item`, UZDoom `GetEffectTicsForItem`) for `PowerupTime`. New `DrawCmd::Clip` crops `DrawBar` foregrounds. **Gore:** `try_move` compares the floors under the box (`tmfloorz − tmdropoffz`), not the floor with the actor's z, so gibs spawned in the air fly instead of exploding at spawn; Doom bounce (`+DOOMBOUNCE`) gets `BounceCount`, auto-off and whole-velocity `BounceFactor`, so limbs stop tumbling. **Weapons:** a psprite zero-tic chain may run 10,000 states in a tic (was 64), so Brutal Doom's per-round `InsertBullets` fills the magazine in one tic. Tests: `sbarinfo_draw` unit tests, gated `sbarinfo_brutal_doom.rs` (real lumps, PNGs with `ROOM_SBAR_PNG`), `airborne_actor_over_flat_floor_is_not_at_a_dropoff`, `doom_bounce_limb_settles_into_its_death_state`, `long_zero_tic_reload_chain_finishes_in_one_tic`. **ACS `ScriptCall`** from a running script never reached ZScript: the host resolved the class / function strings through `world.acs`, which is empty while scripts run. The VM now resolves them (`AcsHost::script_call_resolved`); the call's `GiveInventory` / `TakeInventory` grants and queued natives are applied, with `players[n].mo` mapped to the player's pawn. Brutal Doom's `GetGender` now gives `GenderMale`, which its status bar needs to draw the face (test `script_call_hands_the_host_resolved_names`). | SBARINFO gaps in §3 (inventory bar, gems, number interpolation, mugshot states). The stock bar still uses Room's square-pixel 320×200 mapping. |
| Class menu and second `iddt` (2026-11-08) | New game shows a "Choose Class" page when KEYCONF lists two or more `addplayerclass` entries without `nomenu`. Each class shows its `Player.DisplayName` and `Player.Portrait`. The pick sets the pawn class for the game (`Headless::set_player_class`). `iddt` now cycles map, then map + things (triangles, `AutomapOffsets` applied), then off, as in Doom. | |

The current generated report is `docs/qa/uzdoom-parity.md` (UZDoom commit
`238f4584749ac4d913bf1092ca840854cc996035`, regenerated 2026-11-08 after the
second pass): **697 of 697** UZDoom action names dispatch (none missing; the
UZDoom count dropped from 700 because names inside comments are no longer
counted). There are 339 recognized flag names (none missing) and
**148 of 149** DECORATE-qualified properties with behaviour. The one left is
`Weapon.BobPivot3D`, which needs first-person 3D weapon models. ACSF names are
122/122. These are still name-level counts: the per-row notes in §0.1 say
where an implementation is an approximation. Next, in order:
- Heretic, Hexen and Strife IWAD flow (M8/M9), so the ported family actions
  run in their own games, with playtests per family.
- Model flags at draw time and first-person 3D weapon models
  (`Weapon.BobPivot3D`); IQM animation landed in round 12.
- Validation of ZScript natives and the stdlib in end-to-end content runs.
- Controller rebinding, haptics, and the rest of the post-process path:
  running the queued `PPShader` ops, the `beforebloom` / `screen` targets and material
  shaders at draw time (the 5.0 post uniforms themselves landed in round 11).

---

## 1. Method (reproducible)

| Input | How |
|---|---|
| UZDoom | `git clone --depth 1 --filter=blob:none --sparse https://github.com/UZDoom/UZDoom` then sparse-checkout `wadsrc/static/zscript/`, `wadsrc/static/mapinfo/`, `src/playsim/p_acs.cpp`, `src/scripting/thingdef_data.cpp`, `src/playsim/actionspecials.h` |
| UZDoom 5.0 feature list | zdoom.org wiki "UZDoom version history/5.x" (release notes) |
| Room actions | `room-cli parity` gathers action names from the recognition/dispatch tables (current report: 329 of 700 UZDoom names present) |
| Room flags | `room-sim/src/flags.rs` full UZDoom flag catalogue plus Room aliases (315 UZDoom names recognized) |
| Room ACSF | `ACSF_*` doc names in `room-acs/src/pcd.rs` (116) |
| Feature presence | targeted `grep -ril` over `crates/` (non-test files) per feature keyword, then manual spot checks |

Add the comparison as a tool: `room-cli parity --uzdoom <checkout>` prints
the §6 tables (action, flag, property, ACSF and native diffs) as JSON plus
Markdown, and is re-run each milestone. This is part of UZ0.

Status legend: ✅ present, ◐ partial (some form exists, gaps listed),
❌ absent, — not applicable or intentionally different.

---

## 2. Where Room is already ahead (keep these)

| Room feature | UZDoom |
|---|---|
| Vanilla/Boom/MBF demo playback and recording, bit-exact | ❌ ("Demo record/playback of classic and Boom demos is not supported") |
| Deterministic playsim across x86-64 / aarch64 / wasm32 (`room_core::dmath`, no HashMap iteration) | float-dependent, platform-specific |
| Browser build (WebGPU/WebGL2, `room-web`) | ❌ |
| WASM mods with a stable append-only ABI (`docs/12`, `docs/31`) | ❌ (ZScript only) |
| Multi-room server, verifier, SFU voice/video, BR stats (`room-server`, `room-sfu`) | packet server inside the game exe |
| Duke Nukem 3D native runtime (`room-duke`) | ❌ |
| Face-swap pipeline (`room-faceswap`) | ❌ |
| Library-first crates (headless sim, `room-cli` render/inspect) | monolith |

None of the milestones below may regress these. In particular, Doom
vanilla actors and codepointers stay native so demos keep passing (§4.2).

---

## 3. Comparison by area

### 3.1 Games and content formats

| Feature | UZDoom | Room | Evidence / gap |
|---|---|---|---|
| Doom / Doom II / Final Doom / Freedoom / Chex | ✅ | ✅ | `room-vfs/src/iwad.rs` |
| Doom II Unity/KEX re-release IWADs, Legacy of Rust, `id24` actors | ✅ (`mapinfo/doom2unity.txt`, `actors/doom/id24/*`) | ◐ | calamity blade actions (`A_CheckAmmo`, `A_GunFlashTo`, `A_RefireTo`) dispatch (§0.1); the `id24` actor set and lumps still need an end-to-end run |
| Heretic | ✅ | ❌ (M8 deferred) | ~70 Heretic actions missing (§6.1) |
| Hexen (+ DD) | ✅ | ❌ (M8 deferred) | ~200 actions; classes, armor, weapon pieces, puzzle items, hubs work in Room engine-side |
| Strife | ✅ | ❌ (M9 deferred) | ~100 actions; conversations, quests, Sigil, merchants |
| DeHackEd / BEX | ✅ | ✅ | `room-dehacked` |
| MBF21 | ✅ | ✅/◐ | `A_WeaponAlert` and `A_ClearSoundTarget` have playsim and ZScript bindings; other action semantics remain tracked in §6.1 |
| DSDHacked | ✅ | ✅ | `room-dehacked/src/apply.rs` `Deh_Actor_N` slots |
| DEHEXTRA | ✅ | ◐ | no explicit reference; DSDHacked tables cover most cases. Verify the frame/sprite/sound ranges with a fixture |
| ID24 lumps (`SBARDEF`, `SKYDEFS`, `GAMECONF`, `INTERLEVEL`, `FINALE`, `DEMOLOOP`, `TRAKINFO`) | ◐→✅ (UZDoom 5.0 has id24 fixes) | ◐ | `SBARDEF` recognised in `room-defs`, `GAMECONF` mentioned in UI; the others are absent |
| UMAPINFO | ✅ | ✅ | `room-defs/src/umapinfo.rs` |
| ZMAPINFO / MAPINFO / EMAPINFO | ✅ | ◐ | ZMAPINFO yes; Eternity `EMAPINFO` not referenced (UZDoom ships `mapinfo/eternity.txt` for compat) |
| Level compatibility fixes (`level_compatibility.zs`, `compatibility.zs`) | ✅ | ◐ | `room-map/src/level_fixes.rs` (subset); UZDoom's list is ZScript and comes for free after UZ3 |
| "Mikoportals" emulation, auto gameplay compat (5.0) | ✅ | ❌ | — |
| Pistol-start option / `changemap -1` | ✅ | ✅ | `GameOptions.pistol_start`, player travel, console `changemap [map] [-1]` and `changeepisode` (§0.1 round 3) |

### 3.2 Playsim and actors

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| Actor flags | 315 | 315 recognized | **G2 is no longer a storage/name gap.** Behavioural parity remains to be checked per flag; see §0.1 and `docs/qa/uzdoom-parity.md` |
| Action functions | 697 | 697 dispatched | **G3.** Native Doom code pointers plus the `uzact/` ports (§0.1). Family actions still need their IWAD flow to be exercised |
| Properties (DECORATE spelling) | 149 | 148 with behaviour | `Weapon.BobPivot3D` remains. The parity tool counts builder matches and stored properties that have a reader, not blanket prefixes |
| Armor (`BasicArmorPickup` / `BasicArmorBonus`, `SavePercent`, `MaxAbsorb`, `MaxFullAbsorb`) | ✅ | ✅ | `room-sim/src/{pickup,damage}.rs`. `MaxBonus` / `MaxBonusMax`, skill `ArmorFactor` (skipped by `+INVENTORY.IGNORESKILL`), MAPINFO `damagetype` `NoArmor` / `Factor` / `ReplaceFactor`, and Hexen armor (`Player.HexenArmor`, `HexenArmor` pieces, `ArtiBoostArmor`, `AbsorbDamage` wear) (§0.1 round 5). Hexen/Heretic game support itself is still deferred |
| Backpack (`Ammo.BackpackAmount` / `BackpackMaxAmount`) | ✅ | ✅ | `pickup.rs::backpack_fill` |
| Generic `PowerupGiver` (`Powerup.Type/Duration/Color/Mode/Strength`) | ✅ | ✅ | `room-sim/src/powerups.rs`; DoubleFiringSpeed, TimeFreezer (with `+NOTIMEFREEZE` and the end blink), Targeter (HUD pieces) and Scanner (automap) as of round 5. `PowerMorph` still goes through the vehicle path |
| `A_ExtraLightOff` | ✅ | ✅ | Clears the targeted player's weapon extralight; matches Strife's scripted PowerCrystal behavior |
| `A_RadiusDamageSelf` | ✅ | ✅/◐ | Applies 3D distance-scaled damage, `RDSF_BFGDAMAGE`, and still spawns the optional flash when falloff reaches zero damage; flash handling includes `PUFFGETSOWNER`, damage type, `FOILINVUL`, and `FOILBUDDHA`. ZScript direct class literals are carried by name; dynamic class handles and wall-traced `P_TraceBleed` decals remain |
| `A_Chase` ZScript overload | ✅ | ✅ | DECORATE `A_Chase(melee, missile, flags)` and the ZScript native share `chase_with_labels`: `null` / default state labels, `CHF_RESURRECT`, `CHF_FASTCHASE` (strafe count kept on the actor, as UZDoom's `FastChaseStrafeCount`), `CHF_DONTIDLE`, `CHF_DONTLOOKALLAROUND`, `CHF_STOPIFBLOCKED`, sound/tic/turn controls, and UZDoom's body order (INCHASE guard, invisible-target drop, threshold, idle without a target, JUSTATTACKED, scared monsters). Bare `A_Chase` stays on the vanilla path for demo sync. Targets across linked portals are aimed at through `PosRelative` (2026-11-19, `06-maps-geometry.md` §5) |
| `A_ExtChase` | ✅ | ✅ | `A_ExtChase(usemelee, usemissile, playactive, nightmarefast)` runs the same UZDoom chase body as `A_Chase` with arguments; it has no chase-flag parameter in UZDoom |
| Arch-vile resurrection (`A_VileChase`, `A_CheckForResurrection`) | ✅ | ◐ | `A_VileChase`, `A_Chase(CHF_RESURRECT)`, and the `A_CheckForResurrection()` default ZScript native share the physical corpse selection and raise checks; the native returns a synchronous bool and queues the raise for post-VM application. `A_CheckForResurrection` conservatively returns false when either class overrides `CanResurrect`, and custom state/sound arguments remain unsupported. 3D-floor-group matching and compatibility ghost effects also remain |
| Overlays (`A_Overlay*`: alpha, flags, offset, pivot, rotate, scale, translation, render style, vertex offset) | ✅ | ✅ | Any layer id, own state chains, `OverlayID`, `A_ClearOverlays` ranges, UZDoom `PSPF_*` bits and alpha/style gating (§0.1 round 7); vertex offsets drawn (round 8) |
| `A_WeaponOffset`, `A_SpriteOffset`, `A_SetViewAngle/Pitch/Roll/Pos` | ✅ | ◐ | Implemented paths exist; verify argument/psprite contexts against UZDoom |
| `A_SetTics` caller context | ✅ | ✅/◐ | Actor calls update actor tics, weapon calls update weapon psprite tics, flash states update flash psprite tics, and inventory state chains are ignored. Other action-context edge cases still need parity checks |
| Sound control: `A_StopSounds`, `A_StopAllSounds`, `A_SoundPitch/Volume`, `A_StartSoundIfNotSame` | ✅ | ✅ | Channel-scoped stops (`A_StopSound` CHAN_VOICE default, `A_StopSoundEx`, `A_StopSounds(min, max)`, `A_StopAllSounds` = caller only), pitch, volume, `A_StartSoundIfNotSame`, CHAN_AUTO overlap, `CHANF_SINGULAR` / `FORCE` / `LOCAL` / `LISTENERZ` / `NOPAUSE` / `UI`, pause, seat routing, gender with UZDoom's names and fallback, and looping voices across save / load (§0.1 round 4) |
| `A_SpawnParticleEx`, textured particles, `SPF_*` | ✅ | ✅/◐ | Textures (DECORATE name or ZScript `TextureID`), every legacy render style (Subtract with true reverse-subtract blending), camera-facing / `NO_XY_BILLBOARD` / `FACECAMERA` / `STRETCHPIXELS`, `LOCAL_ANIM`, `ALLOWSHADERS`, plus the motion, fade, size, roll, `REPLACE`, `NOTIMEFREEZE` and `NODYNAMICLIGHTING` support from before (§0.1 round 10); desktop and web |
| Visual thinkers, client-side thinkers (5.0) | ✅ | ❌ | needs ZScript + a render-only thinker list (Room's FX events, `docs/29` K, are the native analogue) |
| Behaviors (5.0 pseudo-interfaces) | ✅ | ❌ | ZScript-only |
| Hexen actor shattering (`A_Burst`) | ✅ | ◐ | DECORATE and ZScript class literals, runtime `class<Actor>` ids, and dynamic string/name values spawn chunks with randomized velocity, copy render, friendship, target, and `LastHeard` fields, handle `BOSSDEATH`, clear blocking, and destroy the source. UZDoom does not spawn `DropItem`s here. Exact spawn/collision/RNG parity remains |
| Morphing (`A_Morph`, `MorphProjectile`, Hexen pig/Heretic chicken) | ✅ | ◐ | `A_Morph` and the morph actions are ported (`uzact/generic.rs`); Heretic/Hexen playtests wait for M8 |
| Freeze / ice deaths (`A_FreezeDeath*`, `ICECORPSE`) | ✅ | ◐ | `A_FreezeDeath`, `A_FreezeDeathChunks`, `A_GenericFreezeDeath` and `A_IceGuyDie` are ported; ice translation and Hexen playtests remain |
| Railguns (`A_FireRailgun*`, `A_MonsterRail`) | ✅ | ◐ | Rail damage, side offsets, primary/alt ammo depletion, two-way `Flash` selection, and caller-selected `pufftype` with `ALWAYSPUFF`/`BLOODLESSIMPACT` behavior are implemented for the actor action path. Monster target lead/basic fire are present. Full rail trails, puff inflictor damage semantics, portals and some aiming edge cases remain |
| Scripted marine (`A_M_*`) | ✅ | ◐ | The vendored MarinePistol can fire and damage targets in headless probes; the full scripted marine behavior and actor action surface still need parity runs |
| Map thinkers exposed (door, plat, elevator, ceiling) | ✅ (5.0) | — | Room's are native; expose them through the ZScript natives in UZ3 |
| Level travel (`AddToTravellingList`, `PostTravelled`) | ✅ (5.0) | ◐ | hub snapshots exist; the ZScript API is missing |
| Bots (ZCajun) | ✅ | — | Room has its own deterministic bots (`docs/28` BE8); ZCajun is not planned |

### 3.3 Scripting

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| ZScript language | ✅ | ◐ executable VM + compiler | `docs/34` and `docs/21`: BD-focused compiler, VM, hooks, save image and native host work; general GZDoom compatibility and edge syntax remain open. The `A_Chase` native now honors state labels and selected chase flags |
| ZScript stdlib (gzdoom.pk3 `zscript/`) | ✅ 94.7 k lines | ◐ vendored UZDoom stdlib type-checks | All 968 native imports bind (round 12); some are approximations or queued ops not applied yet |
| ZScript natives | ~1,830 declarations | ◐ every stdlib import bound | 968/968, 0 stubs (round 12); coverage-driven (`docs/34` §6) |
| ZScript DAP debugger (VS Code) | ✅ (5.0) | ❌ | `room-zvm` already has line tables; add DAP in UZ13 |
| ZScript 5.0 language: `unsafe(const)`, `unsafe(clearscope)`, `norollback`, `<=>`, deprecated classes | ✅ | ✅ | `<=>` / `<>=` lower to -1/0/1; `<=>`, `unsafe(...)` and `norollback` are gated at 4.15.1. From 4.15.1, a `const` method is read-only (`VARF_SafeConst`): writing a field of `self` (assignment, compound assignment, `++` / `--`, element of a field array) is an error, except through an object pointer field; trailing `unsafe(const)` lifts it. Classes keep `deprecated("version", "msg")` and a use at or above that version warns (not inside another deprecated class). Dictionary and DictionaryIterator natives are complete (Insert, At, Remove, ToString / FromString as JSON, Create / Next / Key / Value). `unsafe(clearscope)` is plain-data scope, the same as `clearscope`, because Room has no scope barrier checks. `norollback` (and the implicit one on `ui` / `native` fields) is metadata only: Room has no client-side prediction, so there is nothing to roll back |
| ACS | ✅ | ✅ | Current generated name diff: 122/122 `ACSF_*`; this does not score opcode or runtime semantic parity |
| DECORATE | ✅ | ✅ | BD 4,130/4,130 classes parse (`docs/33`) |
| CVARINFO / KEYCONF / MENUDEF / SBARINFO / LANGUAGE / GLDEFS / TEXTURES / ANIMDEFS / DECALDEF / TERRAIN / LOCKDEFS / SNDINFO / SNDSEQ / MODELDEF | ✅ | ✅ | `room-defs` |
| ZScript `EventHandler` / `StaticEventHandler` lifecycle and tick events | ✅ | ◐ | MAPINFO handlers lazily activate and receive `WorldTick`; map exit dispatches `WorldUnloaded` and `OnUnregister` before ACS UNLOADING scripts. `WorldLoaded`, `PlayerEntered`, `PlayerSpawned`, `PlayerRespawning`, `PlayerRespawned`, `PlayerDied`, `WorldThingSpawned`, `WorldThingDied`, `WorldThingDamaged`, and `WorldThingRevived` are wired; `WorldThingSpawned` now receives the actual actor snapshot (class, health, TID, `bIsMonster`) on normal spawn and late handler activation. `PlayerRespawning` can veto a respawn, and `PlayerEntered` carries `IsReturn` when returning through a hub. Player death/respawn callbacks run before their ACS scripts, with queued actor operations and inventory applied. Actor resurrection also invokes inherited `Actor.OnRevive`. `WorldThingDestroyed` covers explicit removal actions, terminal/looping state chains, corpse cleanup/respawn, spawn cubes, and death without a death state. Queued actor-native operations from actor `PostBeginPlay`/`Tick` and lifecycle callbacks are applied; recursive destroyed-event dispatch is guarded. Actor payloads are class-layout-aware proxies; live actor identity/mutation, full field coverage, teardown paths, and arbitrary engine-side destruction callbacks remain open. Player callbacks carry the correct seat number; static/global registration, other callbacks, UI events, and `SendNetworkEvent` remain open |

**CVar native detail (UZ3):** `CVar` handles carry integer, float, or string current/default values and the CVar type. `GetFloat`, `GetString`, typed default getters, `SetInt/Float/String`, `GetRealType`, and `ResetToDefault` are linked. Setters convert into the CVar's existing type, matching UZDoom's generic-representation behavior. `ZscriptRuntime` exposes typed default injection for float and string CVars; engine configuration still needs to populate those maps. CVar flags, menu-only mutation permissions, bool/color/redirect types, and dynamic CVARINFO registration remain gaps.

`PlayerInfo.GetUserName(charLimit)` now falls back to the `name` CVar and applies UZDoom's character-limit truncation with an ellipsis. The host still stores one menu-selected name rather than per-player userinfo.

### 3.4 Renderer

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| Hardware renderer (Vulkan/GL/GLES) | ✅ | ✅ wgpu (Full/Downlevel) | — |
| Software renderer (truecolor + 8-bit) | ✅ | ❌ | `room-render-soft` is a stub (classic look via palette post: `docs/29` exists) |
| Dynamic lights, attenuated, spot, shadowmaps | ✅ | ✅ | `lights.rs`, `scene.rs` |
| Lightmaps (ZDRay `LIGHTMAP` lump) | ✅ | ✅ | `lightmap.rs` |
| SSAO, bloom, tonemap, FXAA, lens | ✅ | ✅/◐ | `docs/21` SH0/SH1 at 85% |
| `HardwareShader` (texture + PostProcess) | ✅ | ◐ | `hardware_shader.rs` translates material GLSL. Post shaders: GLDEFS `HardwareShader PostProcess <target> { Name … Shader "…" 330 Uniform <type> <name> Texture … Enabled }` parses, and `room-render/src/hw_post.rs` compiles the body against UZDoom's prolog and draws it (§0.1 round 11). `Enabled` shaders run on desktop after the scene; `PPShader.SetEnabled` / `SetUniform*` natives, the `beforebloom` / `screen` targets and material shaders at draw time remain |
| 5.0 post uniforms (`InputTime`, `InputTimeGame`, `InputTimeDelta`, `LastInputTexture`) | ✅ | ✅ | `hw_post.rs`: automatic uniform block (delta, time, game seconds) and the previous frame's input as `LastInputTexture`; golden `uz6_post_inputs.txt` (§0.1 round 11) |
| Skymist, thick fog (`ThickFogDistance/Multiplier`, `SetThickFog`, `SetSkyFog`) | ✅ (5.0) | ✅/◐ | MAPINFO `skyfog`, `skymist`, `skymistyscale`, `useskymist`, `thickfogdistance`, `thickfogmultiplier` and `Level.SetSkyFog` / `SetThickFog` / `ChangeSkyMist` reach the world; thick fog thickens distance fog and light diminishing past the distance; sky fog mixes the sky toward the fog colour (desktop; goldens `uz6_fog.txt`, `uz6_fog_gpu.txt`). The mist texture layer is not drawn (Room has no per-sector fade colour), and the web build has no fog |
| Sky boxes, horizons, line/sector/stacked portals, mirrors, camera textures | ✅ | ✅ | M7/M10 |
| Canvas textures (ZScript `Canvas`), ANIMDEFS canvas `offset` | ✅ | ❌ | needs ZScript UI/canvas natives |
| 3D floors, slopes, colormaps, fog | ✅ | ✅ | — |
| UDMF vertex heights (`zfloor`/`zceiling`) | ✅ | ✅ | `room-map/src/udmf.rs` `apply_vertex_heights`: triangular sectors get a plane through their vertices (§0.1 round 3) |
| Side alpha + "don't render side" flag (5.0) | ✅ | ✅/◐ | UDMF sidedef `alpha` overrides the line's mid-texture alpha and `blockrendering` (`WALLF_BLOCKRENDERING`) makes the viewed side occlude in the BSP walk; both round-trip through the UDMF writer (golden `uz6_sides.txt`). Room's walk only stops once the view is closed, so a blocking side culls less than UZDoom's clipper |
| Flat sprites | ✅ | ✅ | `FLATSPRITE` |
| Roll sprites (`ROLLSPRITE`, `ROLLCENTER`), `SPRITEANGLE`, `SPRITEFLIP`, `SQUAREPIXELS`/`STRETCHPIXELS` | ✅ | ✅ | `room-sim/src/render_pose.rs` `sprite_view`: roll only with `+ROLLSPRITE` (about the origin, or the centre with `+ROLLCENTER`), `SpriteAngle` (property and `A_SetSpriteAngle`), `SpriteRotation` as a rotation-frame offset, `SPRITEFLIP` / `XFLIP` / `YFLIP`, the `√1.2` square-pixel scale; desktop and web draw them (golden `uz6_sprite_view.txt`) |
| Sprite shadows (`CASTSPRITESHADOW`, `NOSPRITESHADOW`) | ✅ | ✅ | UZDoom's rule (`r_actorspriteshadow` 0/1/2, blend-compatible styles) in `casts_sprite_shadow`; the shadow is the same frame squashed to 15 % on the floor, black at `r_actorspriteshadowalpha` and faded by `r_actorspriteshadowfadeheight` (desktop; golden `uz6_sprite_shadow.txt`) |
| Orthographic / OOB camera, FOV-scaled ortho | ✅ (4.14) | ✅ | `SetViewPos` `VPSF_ALLOWOUTOFBOUNDS` skips the 4-unit floor/ceiling clamp and `VPSF_ORTHOGRAPHIC` gives an orthographic projection sized by the offset length and FOV (`view_eye`, `Camera::ortho`; desktop and web; golden `uz6_ortho.txt`) |
| Isometric sprites (`ISOMETRICSPRITES`) | ✅ | ✅ | UZDoom's `isoscaleY` / `isotheta` (`isometric_sprite`, golden `uz6_isometric.txt`) applied to flagged sprites under an orthographic camera (desktop and web) |
| Texture upscaling (HQnX/xBRZ), anisotropic filtering | ✅ | ◐ | `gl_texture_filter_anisotropic` sets the sampler (trilinear + anisotropic), now on the web too. `gl_texture_hqresizemode` / `mult` / `maxinputsize` / `targets` upscale textures, flats, sprites and model skins at upload (`room-render/src/hq_resize.rs`; desktop and web; changing it reloads the textures): Scale2x / 3x / 4x and NormalNx are implemented; hqNx and both xBRZ modes fall back to Scale*x |
| Stereo 3D modes | ✅ | ◐ | references exist (verify scope) |
| `+NODYNAMICLIGHTING`, `SPF_NODYNAMICLIGHTING` (5.0) | ✅ | ✅ | flagged sprites skip dynamic lights (`MaterialRange::NO_DYNLIGHT`); particles already honoured `SPF_NODYNAMICLIGHTING` |
| `+INTERPOLATESCALE`, `+INTERPOLATEALPHA` (5.0) | ✅ | ✅ | scale and alpha lerp between tics for flagged actors (`room-app` sprite poses) |

### 3.5 Models

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| MD2, MD3, OBJ, UE1, voxels (KVX) | ✅ | ✅ | `models.rs`, `ue1.rs`, `kvx.rs` |
| IQM skeletal playback | ✅ | ✅ | `skinning.rs` |
| IQM bone API (5.0: `Set(Named)Bone*`, getters, `TransformByBone`, `AnimateBones`, blending, layers) | ✅ | ◐ | setters, `ClearBoneOffsets` and animation blending are drawn (`iqm_anim.rs`, golden `uz7_bones.txt`, §0.1 round 12); getters return defaults; `TransformByBone` is approximate; no `AnimateBones` in the vendored UZDoom |
| `A_ChangeModel`, `A_SetAnimation`, `A_SetAnimationFrameRate`, `A_SetModelFlag`/`ClearModelFlag`/`ResetModelFlags` | ✅ | ◐ | all dispatch (`uzact/generic.rs`); `A_ChangeModel`'s MODELDEF swap is drawn; IQM `A_SetAnimation*` is drawn with blending (round 12); model flags are drawn: MODELDEF flags and `A_SetModelFlag` overrides drive `PITCHFROMMOMENTUM`, `USEACTORPITCH` / `ROLL` (and the `INHERIT…` sign), `ROTATING` (`Rotation-Speed` / `-Vector` / `-Center`), `USEROTATIONCENTER`, `NOINTERPOLATION`, `INTERPOLATEDOUBLEDFRAMES`; `IGNORETRANSLATION`, back-face culling, per-pixel lighting and pixel-stretch flags are parsed but have no effect yet |
| MODELDEF drawing (frames, slots, blocks) | ✅ | ✅/◐ | `room-render/src/model_catalog.rs` (2026-11-20): every `Model` block kept (`ModelDefs::blocks`), `FindModelFrame` per sprite frame (frames without a binding stay sprites, and modelled actors no longer also draw their sprite), every model slot with its skin path, MD2 / MD3 / UE1 keyframes by number or name blended toward the next state's frame (`room_sim::model_frame_lerp`), IQM keyframes blended too, UZDoom's model matrix (actor scale, offsets, angle / pitch / roll offsets, `SpriteRotation`); desktop and, new, the browser. MD3 surfaces past the first and `SurfaceSkin` beyond surface 0 are not drawn |
| MODELDEF `ViewModelFOV` (5.0) | ✅ | ◐ | parsed; half the player FOV draws the first-person gun at 2×. No separate view-model projection |
| Quaternion model rotation (4.10) | ✅ | verify | — |

### 3.6 Audio and music

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| SFX mixer, positional audio | ✅ (OpenAL) | ✅ | `room-audio` |
| Reverb environments (`REVERBS`, `SoundEnvironment` sector property, EAX-style presets) | ✅ | ✅/◐ | `room-audio/src/reverb.rs` + `room-sim/src/sound_env.rs`: built-ins, stock `reverbs.txt`, `REVERBS` lumps, `SoundEnvironment` zones split by `zoneboundary`, MAPINFO `DefaultEnvironment`, "DSP Water" / "Underwater" under water, `Thing_Activate` and ZScript `SetEnvironmentID` after map start; rendered by a port of OpenAL Soft's EAX reverb, desktop and web (§0.1 round 10) |
| HRTF | ✅ (OpenAL Soft) | ❌ | optional |
| MUS/MIDI, SoundFont (FluidSynth-like), OPL3, trackers, OGG/FLAC/MP3 streams | ✅ (ZMusic) | ✅ | `room-music`, `room-formats` (lewton/claxon/symphonia) |
| GME (VGM/NSF/SPC…), ADL/OPN MIDI, Timidity/WildMidi | ✅ | ◐ | verify GME; ADL/OPN banks absent |
| SNDSEQ, ambient sounds, MUSINFO | ✅ | ✅/◐ | MUSINFO 1 reference (verify) |
| Quit sounds up to 5 s, multi-splash fix (5.0) | ✅ | — | trivial |

### 3.7 Input

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| Keyboard/mouse bindings, KEYCONF | ✅ | ✅ | `room-input`, `room-console` |
| Game controllers (sticks, triggers, paddles, curves) | ✅ | ◐ | `room-input/src/gamepad.rs` + `room-app/src/gamepad.rs` (gilrs): sticks, triggers, curves, menu navigation; fixed button layout, no paddles mapping, no web Gamepad API yet |
| Haptic feedback (5.0) | ✅ | ❌ | — |
| Double-tap binds (`DoubleTapControl`), double-click threshold | ✅ | ✅/◐ | `doublebind` / `undoublebind` and `m_doubleclicktime` (`room-input/src/double_tap.rs`); not saved to or loaded from ini |

### 3.8 UI, menus, HUD, localization, accessibility

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| MENUDEF menus | ✅ | ✅ | `room-ui/src/menudef_runtime.rs` |
| ZScript menus (`OptionMenu`/`ListMenu` subclasses, custom items) | ✅ | ❌ | "ZScript custom menus deferred" (`docs/21` ZS7) |
| Tooltips, gray-check modes, `DoubleControl` (5.0 MENUDEF) | ✅ | ❌ | — |
| SBARINFO | ✅ | ◐ | A mod's SBARINFO replaces the stock bar on desktop and web (`room-ui/src/sbarinfo_draw.rs`, §0.1 round 13): UZDoom placement (`forcescaled` 4:3, `fullscreenoffsets`, `N + center`), all flow controls Brutal Doom uses, images, switchable images, numbers, strings, clipped bars, mugshot prefixes, selected inventory, key bar, FONTDEFS fonts. Missing: `drawinventorybar`, `drawgem`, `drawshader`, number `interpolate`, the popups, `mugshot` state blocks (the face follows the stock mugshot) |
| ZScript status bars (`BaseStatusBar`), full HUD `Draw` export, scoreboard drawing | ✅ | ◐ | `GameInfo.StatusBarClass` runs on the vendored library (`room-ui/src/script_sbar.rs`, §0.1 round 7); stock `DoomStatusBar` matches the native bar; F5 fullscreen toggle, inventory bar and a native scoreboard (round 8). No ZScript scoreboard override |
| Alternative HUD (`althud`) | ✅ | ✅ | `hud_althud` runs the vendored UZDoom `AltHud` ZScript in the fullscreen HUD (`ScriptStatusBar::draw_althud`) with the `hud_*` CVars, `hud_scalefactor` and `hud_aspectscale`; golden `uz4_althud.txt` (§0.1 round 11) |
| Automap: textured floors, sector fill colour (5.0) | ✅ | ◐ | `am_textured` (desktop, 2026-11-20): explored, non-hidden sectors filled with their floor flat at the sector light (`DrawCmd::FlatPoly`, `room-ui/src/automap.rs`); the browser automap (drawn in JS) and the 5.0 sector fill colour are not done |
| Intermission scripts, finales, ENDOOM | ✅ | ✅/◐ | ENDOOM / ENDTEXT / ENDSTRF print to the terminal in ANSI colour with `showendoom` (`room-ui/src/endoom.rs`); no window |
| LANGUAGE + many locales, BCP 47 tags (5.0) | ✅ | ✅/◐ | StringTable i18n present; check tag handling |
| Strife conversations (USDF / `DIALOGUE` lumps, menu) | ✅ | ❌ | only a `StartConversation` request field in `world.rs`; no parser or UI |
| Accessibility menu (extra light, light radius, flash reduction, double-click threshold) | ✅ (5.0) | ◐ | `r_reduce_flashing` and `r_extralight` in the Graphics menu's Accessibility section; light radius and a separate menu remain |
| Gamma/black compensation with calibration image (5.0) | ✅ | ◐ | gamma in post; no calibration screen |
| Console commands (5.0: `changeepisode`, `acs`/`acsn`, `mute*`, `profilecsthinkers`…) | ✅ | ◐ | `room-console`: `changemap [map] [-1]`, `changeepisode`, `doublebind` added; `acs`/`acsn`, `mute*`, `profilecsthinkers` remain |

### 3.9 Multiplayer

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| Model | packet server (5.0 default), P2P fallback | lockstep P2P + room-server relay/verifier | different by design |
| Max players | 64 | **8** (`room-sim/src/player.rs`) | **G5** |
| Client-side prediction, connection stabiliser | ✅ | ❌ | Room is pure lockstep: input delay grows with RTT |
| Lobby (guests, host management, IP bans), ready/vote at intermission, mute commands, chat cooldowns | ✅ (5.0) | ◐ | room-server lobby + bans + SFU mute; no intermission ready-vote |
| Net saves (host-only, `NetGame` folder) | ✅ | ❌ | — |
| Matching file versions enforced | ✅ | ✅ | mod fingerprints (`docs/22`) |

### 3.10 Front end and tooling

| Feature | UZDoom | Room | Gap |
|---|---|---|---|
| Launcher (IWAD picker, extra params, themes, drag-and-drop) | ✅ | ◐ | launcher exists (`docs/21` M13) |
| Updater (5.0) | ✅ | ❌ | web build updates itself; desktop updater optional |
| `-help`, `-help-all`, `-episode`, `-optfile`, `-coop`, `-password` (5.0) | ✅ | ◐ | `room-app` CLI |
| Save/load speedups, UUID save slots | ✅ | ◐ | `save_world_bin` |

---

## 4. Strategy

### 4.1 Run UZDoom's ZScript instead of re-implementing it

UZDoom's game logic is ZScript in `wadsrc/static/zscript` (295 files,
94.7 k lines, GPLv3+), and Room is GPL as well. Of ~700 `A_*` functions,
only **~134 are `native`**; the other ~590, and every Heretic/Hexen/Strife
monster, weapon and item, are ZScript bodies. Once the VM is correct
(`docs/34` ZS1b–ZS4b), Room can:

1. ship `wadsrc/uzdoom/zscript/` (vendored at a pinned commit, with a
   `VENDOR.md` noting the commit hash and licence; Room-only patches go into
   `wadsrc/room/zscript/overrides/`);
2. compile it at startup (cached by content hash: `room-zscript` cache →
   `CompiledProgram` bincode);
3. implement the **natives** it calls, driven by `room-zvm` coverage reports
   on real content (§5 UZ3).

This turns G3 and G4 from "port ~450 functions plus ~600 actor classes" into
"bind natives plus engine features". It also brings 5.0 ZScript features
(behaviors, map thinker exports, level travel API) at the same time.

### 4.2 What stays native Rust

- **Doom vanilla actors and codepointers** (`A_Chase`, `A_Look`, `A_FireShotgun`…):
  demo bit-compatibility (vanilla/Boom/MBF complevels) depends on Room's
  exact RNG call order and fixed-point paths. The ZScript versions in UZDoom
  are float-based and not demo-compatible. Rule: when a class has a native
  Room implementation, the native wins and the stdlib's ZScript body for the
  same class is shadowed. This needs a `native_override` table in
  `room-sim/src/class.rs`.
- **Hot paths** that are cheaper native (`P_TryMove`, line traces,
  `A_Explode` radius) are exposed as natives, as in UZDoom.
- **WASM mods**, which are unaffected (`docs/34` §5.7).

### 4.3 Determinism

UZDoom's ZScript uses doubles. Room's VM already uses deterministic float
ops (`docs/34` §4.4: no fused multiply-add, `dmath` transcendental
functions). Netplay and the verifier stay deterministic across
architectures. Classic demo compatibility is only guaranteed for native
Doom paths (§4.2), which matches UZDoom's own stance of having no demo
compat.

### 4.4 Flag storage (prerequisite for almost everything)

Replace `flags: u64` with a fixed-size bitset sized from a generated table:

```rust
// room-sim/src/flags.rs (generated by build.rs from flags.ron)
pub const N_FLAGS: usize = 384;
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ActorFlags([u64; N_FLAGS / 64]);
impl ActorFlags {
    #[inline] pub fn get(&self, f: Flag) -> bool { self.0[f as usize >> 6] >> (f as usize & 63) & 1 != 0 }
    #[inline] pub fn set(&mut self, f: Flag, v: bool) { let (w, b) = (f as usize >> 6, f as usize & 63);
        if v { self.0[w] |= 1 << b } else { self.0[w] &= !(1 << b) } }
}
#[repr(u16)] pub enum Flag { Solid, Shootable, /* … all 315 UZDoom names + Room-only … */ }
```

- `flags.ron` lists each flag with its name, aliases, the class it belongs
  to (`AActor`, `INVENTORY`, `WEAPON`, `PLAYERPAWN`), its DECORATE/ZScript
  spelling, and its ZScript `bFOO` field name, so `apply_flag`, ZScript
  natives, DeHackEd/MBF21 bit maps and the WASM ABI (`class_flag::*` stays
  append-only) are all generated from one source.
- Save format and checksum: bump the save version and hash the full
  bitset; old saves map the 64 legacy bits.
- Inventory, weapon and player flags currently "recognised, stored later"
  get real storage in the same bitset (separate words).

---

## 5. Milestones

Size estimates are lines of Rust (or WGSL) unless noted. Each milestone has
an acceptance test that fails today.

| ID | Scope | Depends | Size | Acceptance |
|---|---|---|---:|---|
| **UZ0** | `room-cli parity` tool (§1) + checked-in report `docs/qa/uzdoom-parity.md`; vendor UZDoom `wadsrc/static/{zscript,mapinfo}` at a pinned commit (`wadsrc/uzdoom/`, `VENDOR.md`) | — | 0.6 k | report regenerates identically; CI fails if action/flag counts regress |
| **UZ1** | Flag storage (§4.4): `flags.ron` → generated `Flag`/`ActorFlags`; all 315 UZDoom flags parsed; behaviour for the Doom-relevant subset: `NOFEAR`, `ALWAYSFAST`, `NOTELEFRAG`, `CANJUMP`, `FALLDAMAGE`, `SKULLFLY` state, `MBFBOUNCER`, `STAYONLIFT`, `NOFRICTION`, `DONTINTERPOLATE`, `INTERPOLATEANGLES`, `BOSSSPAWNED`, `NOSECTORDAMAGE`, `FORCESECTORDAMAGE`, `NOSPLASHALERT`, `HARMFRIENDS`, `FOILINVUL`, `FOILBUDDHA` | — | 2.0 k | `room mod check` on BD reports 0 unknown flags; old saves load; demo suite still bit-exact |
| **UZ2** | ZScript VM completion = `docs/34` **ZS1b–ZS4b + ZS6b** | — | 7.8 k | `docs/34` §14.6 green |
| **UZ3** | **Stdlib boot**: compile vendored UZDoom `zscript/` with Room natives; `native_override` table for native Doom classes; native binding sprint #1: Actor fields/flags (generated from `flags.ron`), `Thinker`, `ThinkerIterator`, `BlockThingsIterator`, `LineTrace`, `Spawn`, `A_*` natives (~134), `LevelLocals`, `PlayerInfo`, `Inventory`/`Weapon` core | UZ1, UZ2 | 6.0 k | stdlib compiles with 0 unbound natives (stubs counted, trap on call); Doom E1M1–E1M9 / MAP01–MAP32 attract demos still bit-exact; a stdlib-only actor (`ScriptedMarine`) fights correctly in a headless test |
| **UZ4** | Weapons and HUD layer: overlays (`A_Overlay*` full set, `PSPF_*`), `A_WeaponOffset`, `A_SpriteOffset`, `A_SetView*`, `A_ReFire` `autoSwitch`, `ViewModelFOV`; ZScript `BaseStatusBar` + alt HUD | UZ3 | 2.5 k | a weapon using overlays + pivot/rotate renders identically to a UZDoom reference screenshot (manual) and matches a golden; alt HUD golden |
| **UZ5** | Sound and particles: `A_StopSounds/AllSounds`, `A_SoundPitch/Volume`, `A_StartSoundIfNotSame`; **reverb** (`REVERBS` lump, `SoundEnvironment`, per-sector env, presets) in `room-audio` (render-side, not in sim); `A_SpawnParticleEx` + textured particles + `SPF_*`; visual/client-side thinkers on a render-only list (link to `docs/29` K FX events) | UZ3 | 2.2 k | reverb unit tests (impulse response); particle golden; client-side thinkers excluded from the checksum |
| **UZ6** | Renderer 5.0 + 4.x extras: roll sprites (`ROLLSPRITE`, `ROLLCENTER`, `SPRITEANGLE`, `SPRITEFLIP`, `SQUAREPIXELS`/`STRETCHPIXELS`), `INTERPOLATESCALE/ALPHA`, `NODYNAMICLIGHTING`, per-actor sprite shadows, skymist + thick fog + `SetSkyFog`, side alpha + hide-side flag, UDMF vertex heights, post uniforms `InputTime*`/`LastInputTexture`, anisotropic filtering option, ortho/OOB camera, isometric sprites | UZ1 | 3.0 k | goldens per feature in `testdata/goldens/uz6_*`; WGSL validated by naga |
| **UZ7** | Models 5.0: IQM bone API (setters/getters/transform), animation layers and blending, `AnimateBones` UI virtual, `A_ChangeModel`, `A_SetAnimation*`, model flags | UZ3 | 2.0 k | bone-override golden; animation blend unit test (pose = lerp of frames) |
| **UZ8** | **Heretic** (M8 part 1): IWAD detection, `mapinfo/heretic.txt`, Heretic actors via stdlib ZScript, natives they need (tome of power, `A_Feathers`/chicken morph, mace/phoenix/skull rod/hellstaff rain), status bar, intermission, inventory bar | UZ3, UZ4 | 2.5 k | E1M1–E1M3 completable headless by scripted inputs; Heretic demo playback (vanilla) for native paths optional |
| **UZ9** | **Hexen** (M8 part 2): classes, weapon pieces, armor (AC), puzzle items, `A_Morph`, ice/freeze deaths, polyobject cases, `A_Blast`, Korax/Heresiarch scripts, hubs, Deathkings | UZ8 | 3.0 k | MAP01 hub loop + return; Korax fight runs; ACS-heavy maps no traps |
| **UZ10** | **Strife** (M9): USDF + binary `DIALOGUE` parser (`room-defs`), conversation UI (`room-ui`), quests/objectives log, merchants, Sigil, Strife status bar, voices | UZ3 | 3.5 k | MAP02→MAP33 dialogue flow; buying/selling; Sigil pieces |
| **UZ11** | Input: game controllers through `gilrs` (desktop) + Gamepad API (web), stick curves, paddles/back buttons, haptics (`gilrs` force feedback / `vibrationActuator`), double-tap binds | — | 1.5 k | input mapping tests; rumble on damage on a real pad (manual) |
| **UZ12** | Multiplayer: `MAX_PLAYERS` → 64 (players array, TicCmd framing, proto v3, server limits), optional **client prediction** for the local player (rollback of own pawn only; sim stays lockstep-authoritative), intermission ready-vote, net saves (host-only) | — | 3.0 k | 16-seat headless soak; 64-seat proto test; prediction off = bit-identical to today |
| **UZ13** | UI and tooling: ZScript menus (`OptionMenu`/`ListMenu` classes), MENUDEF 5.0 keywords (tooltips, gray modes, `DoubleControl`), accessibility menu (extra light, light radius, flash reduction), gamma calibration screen, automap textured + sector fill, ENDOOM, ZScript **DAP debugger** (`room-zvm` line tables + `dap` crate over stdio/TCP) | UZ2 | 3.0 k | menu goldens; DAP: breakpoint hit + step in VS Code (manual) + protocol unit tests |
| **UZ14** | Content standards: `id24` lumps (`SKYDEFS`, `INTERLEVEL`, `FINALE`, `DEMOLOOP`, `TRAKINFO`, full `SBARDEF`, `GAMECONF`), id24 actors via stdlib, Unity/KEX IWAD set, DEHEXTRA fixture, EMAPINFO compat, pistol-start option + `changemap -1`, auto gameplay compat (`compatibility.zs` from the stdlib) | UZ3 | 2.0 k | ID24 test WAD (Legacy of Rust-style) loads; DEHEXTRA fixture green |

Critical path: **UZ1 ∥ UZ2 → UZ3 → (UZ4, UZ5, UZ7) → UZ8 → UZ9**; UZ10 needs UZ3 + UZ13's conversation UI pieces. UZ6, UZ11 and UZ12 are independent and can run in parallel at any time.

Rough total: ~45 k lines. Track in `docs/21` as a new **UZ** table, and mark
M8/M9 as superseded by UZ8–UZ10.

**Out of scope for the current pass (2026-11-03):** UZ8 (Heretic), UZ9 (Hexen),
UZ10 (Strife), UZ11 (game controllers). Leave them `deferred`. Do the rest in
order: UZ0, UZ1, UZ2, UZ3, UZ4, UZ5, UZ6, UZ7, UZ12, UZ13, UZ14.

---

## 7. Execution status

Living checklist for this document. Flip a box when its acceptance test is
green. Measured gaps live in [`qa/uzdoom-parity.md`](qa/uzdoom-parity.md)
(`room parity`); do not hand-edit that file.

| ID | Status | Notes |
|---|---|---|
| **UZ0** | `done` | `room parity`; report regenerates identically; `room-cli` test fails if Room action/flag/property/ACSF counts drop. Vendored tree: `wadsrc/uzdoom/` at `67bdb4b7` (2026-10-28). |
| **UZ1** | `code_complete` | All 315 flag names are stored and the Doom-relevant subset behaves, including sprite lerp: `+DONTINTERPOLATE` snaps, `+INTERPOLATEANGLES` takes the short yaw turn (`sprite_pose`, used by the play shell). Legacy `u64` is unchanged. Brutal Doom v22 test 6 `room mod check`: 0 unknown flags, properties, and actions. Saves round-trip the extra flags; a blob without the trailer still loads. Freedoom smoke and map bundles were re-recorded from the current playsim and verify. Status is not `done` until CI runs. Next task is **UZ2**. |
| **UZ2** | `code_complete` | `docs/34` §14.6 re-checked 2026-11-03: probe, `bd_compiles_and_links`, lump API, damage control/scaling, player TID, gender, save/load, and two-run checksum all pass. Not `done` until CI. Next task is **UZ3**. |
| **UZ3** | `code_complete` (2026-11-10) | Round 12 (§0.1): 968 of 968 stdlib native imports bind, 0 return-zero stubs; host natives trap with their host name. `Super` calls run the parent body. A headless `MarinePistol` still fights (rounds 6–9). Freedoom2 bundles replay; the Freedoom1 E1–E4 committed-bundle tests (`freedoom_smoke.rs`) already failed before round 12 (E1M1 desyncs at tic 35, same `freedoom1.wad`), so the demo-sync criterion is open: that regression predates this round and needs its own bisect. Gaps: see the round 12 row. |
| **UZ4** | `done` (2026-11-10) | Without `PowerDoubleFiringSpeed`, a 4-tic second layer is at 3 after it starts and the gun frame stays at 4. Holding the power halves that layer to 1 after it starts, even with fast weapons on, and the gun stays at 4. `A_OverlayFlags(2, 4, false)` keeps the layer at 3 while the power is still held. The gun stays off the power. With fast weapons at 1, a 4-tic second layer is down to 1 tic after it starts and the gun frame stays at 4. At level 2 that layer ends in the same tic and the gun stays at 4. `A_OverlayFlags(2, 8, false)` keeps the 4-tic layer at 3 after it starts. The gun stays off the fast-weapon switch. A fuzzy player fuzzes the second weapon layer and the gun. `A_OverlayFlags(2, 512, true)` draws that layer solid and leaves the gun fuzzy. `A_OverlayRenderStyle(2, 2)` fuzzes the second layer again. `A_OverlayRenderStyle(2, 1)` clears it while the flag stays on. `A_OverlayFlags(2, 512, false)` lets the player fuzz the second layer again. `A_OverlayFlags(1, 512, true)` then draws the gun solid and leaves the second layer fuzzy. A player at alpha 0.25 with second-layer alpha 0.5 draws that layer at 0.125 and the gun at 0.25. `A_OverlayFlags(2, 1024, true)` keeps the second layer at 0.5 and leaves the gun at 0.25. `A_OverlayFlags(1, 1024, true)` then draws the gun solid and leaves the second layer at 0.5. `A_OverlayFlags(2, 1024, false)` returns the second layer to 0.125 and leaves the gun solid. `A_OverlayFlags(2, 256, false)` snaps the second weapon layer between tics and leaves the gun easing. `A_OverlayFlags(1, 256, false)` then snaps the gun and leaves the second layer snapped. `A_OverlayFlags(2, 256, true)` eases only the second layer again. `A_OverlayFlags(2, 1, false)` drops the gun offset from the second layer. Sway 10 plus offset 3 still draws the gun at 13, and the second layer uses 10. `A_OverlayFlags(1, 1, false)` then drops the gun offset and leaves the second layer without it. `A_OverlayFlags(2, 1, true)` returns the offset to the second layer only. `A_OverlayFlags(2, 2, false)` drops weapon sway from the second layer while the gun still draws sway 10 plus offset 3. `A_OverlayFlags(1, 2, false)` then drops the gun sway and leaves the second layer still. `A_OverlayFlags(2, 2, true)` returns sway to the second layer only. `A_OverlayFlags(2, 128, false)` makes `A_OverlayPivot(2, 4, 4)` a pixel pivot on the second weapon layer. The gun stays in fraction mode. `A_OverlayFlags(1, 128, false)` then switches the gun to pixels and leaves the second layer in pixels. `A_OverlayFlags(2, 128, true)` returns only the second layer to fractions. `A_OverlayRenderStyle(2, 2)` fuzzes the second weapon layer and leaves the gun solid. `A_OverlayRenderStyle(1, 2)` then fuzzes the gun and leaves the second layer fuzzy. `A_OverlayRenderStyle(2, 1)` clears only the second layer. `A_OverlayTranslation(2, "Green")` tints the second weapon layer green: its green channel stays and the red channel drops to a quarter. The gun stays white. `A_OverlayTranslation(1, "Red")` tints the gun red and leaves the second layer green. `A_OverlayTranslation(2, "None")` clears only the second layer. `A_OverlayPivotAlign(2, 2, 0)` pins the second weapon layer's pivot to the right edge and the top. The gun keeps the center pivot. `A_OverlayPivotAlign(1, 0, 2)` pins the gun to the left edge and the bottom and leaves the second layer on the right and top. `A_OverlayPivot(2, 0, 0)` scales the second weapon layer from its top-left corner. The gun keeps the center pivot. `A_OverlayPivot(1, 0.25, 0.75)` moves the gun's pivot and leaves the second layer at the corner. `A_OverlayRotate(2, 90)` turns the second weapon layer 90° counter-clockwise. The gun stays upright. `A_OverlayRotate(1, 15)` turns the gun 15° and leaves the second layer at 90°. `A_OverlayFlags(2, 32, true)` flips the second weapon layer top to bottom. The gun stays upright. `A_OverlayFlags(1, 32, true)` then flips the gun and leaves the second layer flipped. `A_OverlayFlags(2, 32, false)` clears only the second layer. `A_OverlayFlags(2, 16, true)` mirrors the second weapon layer left to right. The gun stays unflipped. `A_OverlayFlags(1, 16, true)` then mirrors the gun and leaves the second layer mirrored. `A_OverlayFlags(2, 16, false)` clears only the second layer. `A_OverlayScale(2, 2)` draws the second weapon layer at 2×2. `A_OverlayScale(2, 2, 3)` makes it 2× wide and 3× tall. The gun stays at 1× until `A_OverlayScale(1, 4)`, which draws the gun at 4× and leaves the second layer at 2×3. `A_OverlayAlpha(2, 0.25)` draws the second weapon layer at a quarter opacity. The gun stays fully opaque. A later `A_OverlayAlpha(1, 0.5)` draws the gun at half opacity and leaves the second layer at a quarter. `A_OverlayOffset(2, 6, 40)` moves the second weapon layer 6 right and 8 down. The gun stays put. A later `A_OverlayOffset(1, 3, 36)` moves the gun by 3 and leaves the second layer at 6. `A_Overlay(2, "Flash")` draws that state's sprite on a second weapon layer. `PISF A` becomes patch `PISFA0`. `A_Overlay(1, "Flash")` leaves the second layer empty. The layer clears when the state ends. `A_OverlayFlags(1, 16, true)` mirrors the weapon left to right (`PSPF_FLIP`). The HUD flip flag turns on, layer 2 does not clear it, and `A_OverlayFlags(1, 16, false)` turns it off. Bit 64 still flips the same way. `A_OverlayFlags(1, 256, false)` snaps the gun offset each tic (`PSPF_INTERPOLATE` off). With the flag on, an offset moving from 0 to 10 is drawn at 5 halfway through the tic. The same call on layer 2 leaves snapping on. `A_OverlayFlags(1, 256, true)` eases again. `A_OverlayFlags(1, 512, true)` keeps the overlay style when the player is fuzzy (`PSPF_FORCESTYLE`). Blur fuzzes the gun, the flag with no overlay style draws it solid, and `A_OverlayRenderStyle(1, 2)` fuzzes it again. The same flag call on layer 2 leaves the flag set. Clearing it after a Normal style lets the blur fuzz the gun again. `A_OverlayFlags(1, 1024, true)` keeps the overlay alpha when the player is translucent (`PSPF_FORCEALPHA`). A player at alpha 0.25 draws the gun at 0.25, then at the overlay's 0.5 after the flag is set. The same call on layer 2 leaves the flag set. Clearing it multiplies again, so 0.5 becomes 0.125. `A_OverlayFlags(1, 128, false)` makes `A_OverlayPivot(1, 4, 4)` a pixel pivot (`PSPF_PIVOTPERCENT` off). On an 8×8 sprite scaled to 2× that lands on the same spot as a center fraction of 0.5. The same flag call on layer 2 leaves pixel mode on. `A_OverlayFlags(1, 128, true)` returns to fractions. `A_OverlayFlags(1, 32, true)` flips the weapon sprite top to bottom (`PSPF_MIRROR`). The top pixel becomes the bottom pixel. The same call on layer 2 leaves the sprite upright. `A_OverlayFlags(1, 32, false)` clears the flip. `A_OverlayFlags(1, 4, true)` with `PowerDoubleFiringSpeed` in inventory turns a 4-tic weapon frame into 2 (`PSPF_POWDOUBLE`). Without that power the frame stays 4 tics. The same call on layer 2 leaves the 4-tic frame. `A_OverlayFlags(1, 8, true)` with `sv_fastweapons` 1 turns a 4-tic weapon frame into 2 (`PSPF_CVARFAST`). Level 2 makes that frame 1 tic. The same call on layer 2 leaves the 4-tic frame. `A_OverlayFlags(1, 1, false)` drops the gun offset (`PSPF_ADDWEAPON`). Sway 10 plus an offset of 3 draws at 13, then at 10 after the flag clears. The same call on layer 2 leaves the offset off. `A_OverlayFlags(1, 1, true)` puts the offset back. `A_OverlayFlags(1, 2, false)` drops weapon sway (`PSPF_ADDBOB`). A gun offset of 3 with sway 10 draws at 13, then at 3 after the flag clears. The same call on layer 2 leaves the sway off. `A_OverlayFlags(1, 2, true)` puts the sway back. `A_OverlayTranslation(1, "Green")` tints the weapon green: a white pixel's red channel drops to a quarter and the green channel stays. The same call on layer 2 leaves that tint. `A_OverlayTranslation(1, "None")` clears it. `A_SetCrosshair(2)` draws a wider center mark than index 1. Index 0 draws nothing, and a later `A_SetCrosshair(0)` clears the mark. `ViewModelFOV 45` on a pistol model draws that gun at 2× when the player field of view is 90°. A model without `ViewModelFOV` stays at 1×. `A_ZoomFactor(2)` halves the view: a 90° field becomes 45°. `A_ZoomFactor(1)` restores it. A scripted camera keeps its own field of view. `A_SpriteOffset(4, 6)` shifts the sprite 4 to the camera's right and 6 up. The actor stays put. A later `A_SpriteOffset(1, 2)` replaces that shift. Looking toward -Y, the 4-unit shift is along +X. `SetViewPos(8, 0, 2)` moves the eye 8 forward and 2 up while the actor stays put. After the body turns to 90°, that same offset is 8 along the new aim. Flag 2 adds `(0, 6, 0)` in world axes. Flag 4 puts the eye at `(10, 20, 30)`. `A_SetViewRoll(15)` banks the camera 15° while the sprite stays upright. Looking along +X, a quarter turn points the camera's up toward -Y. `A_SetRoll(40)` rolls the sprite to 40° and the camera stays at 15°. A pointer other than the caller leaves that bank. `A_SetViewAngle(30)` turns the camera 30° while the body still faces ahead, so shots still leave along the aim. `A_SetAngle(90)` then turns the body to 90° and the camera stays at 30°. A pointer other than the caller leaves that camera angle. `A_SetViewPitch(20)` looks 20° up while the aim stays level, so shots still leave flat. `A_SetPitch(5)` then aims those shots at 5° and the camera stays at 20°. `A_SetViewPitch(100, 1)` clamps the camera to 89°. `A_OverlayRenderStyle(1, 2)` draws the weapon layer with the spectre fuzz (`STYLE_Fuzzy`). The same call on layer 2 leaves that fuzz. `A_OverlayRenderStyle(1, 1)` (`STYLE_Normal`) clears it. `A_OverlayRotate(1, 90)` turns the weapon layer a quarter turn counter-clockwise, so the right-hand pixel ends up on top. The same call on layer 2 leaves the weapon at 90°. `A_OverlayPivotAlign(1, 2, 0)` puts the scale pivot on the right edge and the top, so a 2×3 scale grows left and down. The same call on layer 2 leaves that pivot. `A_OverlayPivot(1, 0, 0)` scales the weapon layer from its top-left corner. The same call on layer 2 leaves the pivot at the corner. Until a pivot is set, scale still grows from the center. `A_OverlayFlags(1, 64, true)` flips the weapon layer left to right. The same flag on layer 2 leaves the weapon layer flipped. `A_OverlayFlags(1, 64, false)` clears the flip. `A_OverlayScale(1, 2)` draws the weapon layer at twice its size. `A_OverlayScale(1, 2, 3)` is 2× wide and 3× tall, grown about the patch center. The same call on layer 2 leaves that scale. `A_OverlayAlpha(1, 0.25)` draws the weapon layer at a quarter of its light. The same call on layer 2 leaves the weapon layer at 0.25. `A_OverlayOffset(1, 4, 40)` shifts the weapon layer by 4 pixels right and 8 down. The same call on layer 2 leaves that layer where it is. `A_WeaponOffset(10, 48)` shifts the gun by 10 pixels right and 16 down. A follow-up `A_WeaponOffset(1, 2, 5)` keeps that X and adds 2 to Y, so the HUD draws the gun at (10, 18). `A_ReFire(null, false)` on an empty pistol leaves that pistol ready. The same refire with the flag omitted queues the Fist. Round 7 (§0.1) added every other layer id and the ZScript status bar; flag numbers in the older notes above use Room's previous bit order (now UZDoom's: `FLIP` 64, `MIRROR` 512, `FORCEALPHA` 128, `FORCESTYLE` 256, `PIVOTPERCENT` 2048, `INTERPOLATE` 4096). Round 8 draws vertex offsets and adds the fullscreen toggle for the ZScript bar. Round 11 adds the alt HUD (UZDoom's `AltHud` ZScript, `hud_althud`) and the two goldens: `uz4_overlay_pivot_rotate.txt` (a gun scaled 1.5× and turned 15° under an overlay pivoted at its corner, scaled 2×1 and turned 90°) and `uz4_althud.txt`. The manual screenshot comparison against UZDoom is not done. |
| **UZ5** | `done` (2026-11-09) | Round 4: reverb (REVERBS, zones, presets), channel-scoped stops, pause, local routing, gender, saved loops and textured particles (§0.1). Round 10: OpenAL Soft EAX reverb port with B-format sends and UZDoom under-water handling, web reverb, `SPF_LOCAL_ANIM`, `SPF_ALLOWSHADERS`, reverse-subtract particles, `SetEnvironmentID` after map start (`Thing_Activate`, ZScript `CurSector`), saved zone environments. Acceptance: impulse-response tests, particle golden, render-only state out of the checksum (§0.1 round 10). `A_StopAllSounds` drops the caller's queued pistol and shotgun and leaves one channel-stop request for that actor. `A_SoundPitch(0, 2)` plays a body-channel pistol shot at double rate and leaves its volume at 1. `A_SoundPitch(5, 2)` leaves that shot at normal rate. A playing voice on channel 1 goes to pitch 2 and a voice on channel 2 stays at 1. `A_SoundVolume(0, 0.25)` turns a body-channel pistol shot from full volume down to a quarter. `A_SoundVolume(5, 0.25)` leaves that shot at full volume. A playing voice on channel 1 drops to 0.25 and a voice on channel 2 stays at 1. |
| **UZ6** | `done` (2026-11-10) | Round 11 (§0.1): roll / angle / flip / square-pixel sprites, sprite shadows as squashed sprites, `NODYNAMICLIGHTING`, thick fog + sky fog + MAPINFO skymist keys and their `Level` natives, sidedef `alpha` / `blockrendering`, GLDEFS post shaders with the 5.0 automatic uniforms and `LastInputTexture`, ortho / out-of-bounds camera, isometric sprites. Goldens `testdata/goldens/uz6_*`; shaders validated by naga (`build.rs` for the built-in WGSL, `glsl_to_wgsl` for post shaders). Open: the skymist texture layer, PPShader natives, web fog. |
| **UZ7** | `code_complete` (2026-11-10) | Round 12 (§0.1): bone setters with UZDoom interpolation, `A_SetAnimation*` blending, `A_ChangeModel` animation file, IQM animation sampling and CPU skinning; bone-override golden and blend unit test. Bone getters return defaults; no `AnimateBones` virtual in the vendored UZDoom. |
| **UZ8** | `deferred` | Heretic. Out of scope for this pass. |
| **UZ9** | `deferred` | Hexen. Out of scope for this pass. |
| **UZ10** | `deferred` | Strife. Out of scope for this pass. |
| **UZ11** | `in progress` | Game controllers: gilrs backend, analog move/look, menus and `doublebind` landed in round 3 (§0.1); rebinding, haptics and the web Gamepad API remain. |
| **UZ12** | `done` (2026-11-09) | 64 players, proto v3, client prediction (native netplay), intermission ready-vote, host-only net saves — see §0.1 round 9 and `38-network-state.md`. Web clients predict too (on by default, `?predict=0` off). |
| **UZ13** | `not_started` | Menus, accessibility, DAP. |
| **UZ14** | `not_started` | id24 lumps and DEHEXTRA. |

2026-11-03 tool counts (commit `67bdb4b7`): actions 345 / 700 (458 missing),
flags 158 / 284 (165 missing), properties 103 / 150 (108 missing), ACSF 122 / 122
(0 missing — the §6.3 "8 missing" list is already named in `room-acs`).

---

## 6. Appendix — measured gaps (UZDoom trunk 2026-10-28 vs Room 2026-11-02)

The lists below are the 2026-11-02 hand count. The tool output in
[`qa/uzdoom-parity.md`](qa/uzdoom-parity.md) is the source of truth from
2026-11-03 on (`room parity --uzdoom <checkout>`). Names are case-insensitive.
A name in these lists may still be handled by a differently named Room code
path; the tool checks `action_known.rs` aliases.

### 6.1 Missing action functions (457 of 699, historical)

All names below now dispatch (§0.1, 2026-11-08 pass 2). The list stays as
the record of what the ports cover.

Generic and Doom-relevant (do these first, many are free after UZ3):

- `actors/actor.zs` (79):
  - **Activation and special state:** `A_ActiveAndUnblock`, `A_ActiveSound`,
    `A_CheckForResurrection`, `A_CheckPlayerDone`, `A_CheckTerrain`,
    `A_ClassBossHealth`, `A_CountdownArg`, `A_RestoreSpecialPosition`,
    `A_ScreamAndUnblock`.
  - **Lights:** `A_AttachLight`, `A_AttachLightDef`, `A_RemoveLight`,
    `A_Light`, `A_LightInverse`.
  - **Attacks and chase:** `A_BulletAttack`, `A_DoChase`,
    `A_ExtChase`, `A_Look2`, `A_MonsterRail`, `A_RadiusDamageSelf`,
    `A_SetChaseThreshold`, `A_WolfAttack`.
  - **Flags and properties:** `A_ChangeCountFlags`, `A_ChangeLinkFlags`,
    `A_HideThing`, `A_UnHideThing`, `A_LowGravity`, `A_ResetHealth`,
    `A_SetFloat`, `A_UnsetFloat`, `A_SetReflective`, `A_UnSetReflective`,
    `A_SetReflectiveInvulnerable`, `A_UnSetReflectiveInvulnerable`,
    `A_SetShootable`, `A_UnSetShootable`, `A_SetSolid`, `A_UnsetSolid`.
  - **Pointers:** `A_KillTarget`, `A_KillTracer`, `A_RemoveTarget`,
    `A_RemoveTracer`.
  - **Models and animation:** `A_ChangeModel`, `A_ClearModelFlag`,
    `A_ResetModelFlags`, `A_SetAnimation`, `A_SetAnimationFrameRate`,
    `A_SetModelFlag`.
  - **Overlays:** `A_ClearOverlays`, `A_Overlay`, `A_OverlayAlpha`,
    `A_OverlayFlags`, `A_OverlayOffset`, `A_OverlayPivot`,
    `A_OverlayPivotAlign`, `A_OverlayRenderStyle`, `A_OverlayRotate`,
    `A_OverlayScale`, `A_OverlayTranslation`, `A_OverlayVertexOffset`.
  - **View and sprite:** `A_FaceConsolePlayer`, `A_SetMugshotState`,
    `A_SetSpriteAngle`, `A_SetSpriteRotation`, `A_SetViewAngle`,
    `A_SetViewPitch`, `A_SetViewPos`, `A_SetViewRoll`,
    `A_SetVisibleRotation`, `A_WeaponOffset`.
  - **Sound:** `A_SoundPitch`, `A_SoundVolume`, `A_StartSoundIfNotSame`,
    `A_StopAllSounds`, `A_StopSounds`.
  - **User variables and particles:** `A_SetUserArrayFloat`,
    `A_SetUserVarFloat`, `A_SpawnParticleEx`.
- `actors/actions.zs`: `A_SpriteOffset`.
- `actors/attacks.zs`: `A_ComboAttack`, `A_MissileAttack`, `A_NailBomb`.
- `actors/checks.zs`: `A_CheckSpecies`, `A_JumpIfArmorType`,
  `A_JumpIfMasterCloser`, `A_PlayerSkinCheck`.
- **Doom:**
  - railguns: `A_FireRailgun`, `A_FireRailgunLeft`, `A_FireRailgunRight`,
    `A_RailWait`;
  - id24 calamity blade: `A_CheckAmmo`, `A_GunFlashTo`, `A_RefireTo`;
  - monsters: `A_BetaSkullAttack`, `A_DualPainAttack`, `A_PainShootSkull`,
    `A_CPosAttackDehacked`, `A_CPosAttackInternal`, `A_SPosAttackInternal`,
    `A_Tracer2`;
  - scripted marine: `A_M_*` ×14, `A_MarineChase`, `A_MarineLook`,
    `A_MarineNoise`;
  - beta and grenade weapons: `A_FireOldBFG`, `A_FireSTGrenade`.
- **Shared:** `A_BridgeInit`, `A_BridgeOrbit`, `A_FreezeDeath`,
  `A_FreezeDeathChunks`, `A_GenericFreezeDeath`, `A_IceSetTics`, `A_Morph`,
  `A_RestoreSpecialDoomThing`, `A_RestoreSpecialThing1`/`2`,
  `A_CheckForReload`, `A_ResetReloadCounter` (MBF21),
  `A_Damage`, `A_Kill` (constants).

Game-specific totals: Heretic ~70 (`heretic/*`, `raven/minotaur`), Hexen
~200 (`hexen/*`), Strife ~100 (`strife/*`). These all come from stdlib
ZScript in UZ8–UZ10.

### 6.2 Actor flag catalogue imported into Room (315 names)

The names below were missing from an older one-word flag report. They are now
recognized by the imported flag catalogue; that does **not** mean every flag
has its UZDoom runtime effect. See §3.2 and the renderer table for known
behavioral gaps.

`ABSMASKANGLE ABSMASKPITCH ABSVIEWANGLES ADDITIVEPOISONDAMAGE
ADDITIVEPOISONDURATION ADDLIGHTLEVEL AIMREFLECT ALLOWBOUNCEONACTORS
ALLOWPAIN ALLOWTHRUBITS ALLOWTHRUFLAGS ALWAYSFAST ALWAYSRESPAWN ANGLEDROLL
ARGSDEFINED ARMED AVOIDHAZARDS AVOIDINGDROPOFF AVOIDMELEE
BILLBOARDFACECAMERA BILLBOARDNOFACECAMERA BLASTED BLOCKASPLAYER
BLOCKEDBYSOLIDACTORS BOSSCUBE BOSSSPAWNED BOUNCEAUTOOFF
BOUNCEAUTOOFFFLOORONLY BOUNCELIKEHERETIC BOUNCEMODIFIESPITCH
BOUNCEONUNRIPPABLES BUMPSPECIAL CAMFOLLOWSPLAYER CANBLAST CANJUMP
CANTLEAVEFLOORPIC CASTSPRITESHADOW CAUSEPAIN CHASEGOAL CRASHED
CROSSLINECHECK DECOUPLEDANIMATIONS DEFLECT DONTBOUNCEONSHOOTABLES
DONTBOUNCEONSKY DONTDRAIN DONTFACETALKER DONTFLIP DONTFOLLOWPLAYERS
DONTINTERPOLATE DONTOVERLAP DONTSEEKINVISIBLE DOSHADOWBLOCK E1M8BOSS
E2M8BOSS E3M8BOSS E4M6BOSS E4M8BOSS EXPLOCOUNT FALLDAMAGE FALLING
FLIPSPRITEOFFSETX FLIPSPRITEOFFSETY FLY FLYCHEAT FOILBUDDHA FOILINVUL
FORCEINFIGHTING FORCESECTORDAMAGE FORCEZERORADIUSDMG FRIGHTENING
FULLVOLDEATH FULLVOLSEE GETOWNER GRENADETRAIL HANDLENODELAY HARMFRIENDS
HITMASTER HITOWNER HITTARGET HITTRACER HUNTPLAYERS ICECORPSE ICESHATTER
INCHASE INCOMBAT INCONVERSATION INFLOAT INTERPOLATEALPHA INTERPOLATEANGLES
INTERPOLATESCALE INTRYMOVE INVISIBLEINMIRRORS ISOMETRICSPRITES JUMPDOWN
JUSTATTACKED JUSTHIT KEEPBOUNCEANGLE KILLED LAXTELEFRAGDMG LIGHTMULTALPHA
LINEDONE MAP07BOSS1 MAP07BOSS2 MASKROTATION MASTERNOSEE MBFBOUNCER
MINVISIBLE MIRRORREFLECT MVISBLOCKED NOAUTOOFFSKULLFLY NOBOSSRIP
NOBOUNCESOUND NODYNAMICLIGHTING NOEXPLODEFLOOR NOFEAR NOFORWARDFALL
NOFRICTION NOFRICTIONBOUNCE NOHATEPLAYERS NOINFIGHTSPECIES NOKILLSCRIPTS
NOMENU NONSHOOTABLE NOSAVEGAME NOSECTORDAMAGE NOSHIELDREFLECT NOSIGHTCHECK
NOSPLASHALERT NOSPRITESHADOW NOTARGETSWITCH NOTELEFRAG NOTELESTOMP
NOVERTICALMELEERANGE NOWALLBOUNCESND ONLYSLAMSOLID ONLYVISIBLEINMIRRORS
ONMOBJ POISONALWAYS RELATIVETOFLOOR RESPAWNINVUL RETARGETAFTERSLAM
ROLLCENTER ROLLSPRITE SCROLLMOVE SEEFRIENDLYMONSTERS SEESDAGGERS SHADOWAIM
SHADOWAIMVERT SHADOWBLOCK SHATTERING SHIELDREFLECT SKULLFLY SMASHABLE
SPAWNFLOAT SPAWNSOUNDSOURCE SPECIAL SPECIALFIREDAMAGE SPECIALFLOORCLIP
SPRITEANGLE SPRITEFLIP SQUAREPIXELS STANDSTILL STAYMORPHED STAYONLIFT
STOPRAILS STRETCHPIXELS STRIFEDAMAGE SUMMONEDMONSTER TELEPORT THRUREFLECT
UNMORPHED VFRICTION WARNBOT WEAPONSPAWN XFLIP YFLIP ZDOOMTRANS`

Several of these are internal state flags in UZDoom (`INCHASE`, `JUSTHIT`,
`INFLOAT`, `KILLED`…). Room may keep them as actor state rather than class
flags, but they must still be readable and settable as ZScript `bFOO` fields
after UZ3. `BUDDHA` is matched by a different path in Room (`cf::BUDDHA`)
and was excluded by hand.

### 6.3 ACS functions not declared in `room-acs/src/pcd.rs` (8 of 122)

`Radius_Quake2`, `ACS_NamedExecute`, `ACS_NamedSuspend`,
`ACS_NamedTerminate`, `ACS_NamedLockedExecute`,
`ACS_NamedLockedExecuteDoor`, `ACS_NamedExecuteWithResult`,
`ACS_NamedExecuteAlways`. Named scripts may be handled through another path:
check the BD ACS corpus (`docs/33` §3.9) before implementing them.

**2026-11-03:** `room parity` finds all 122 `ACSF_*` names from `p_acs.cpp`
already written in `pcd.rs` (0 missing names). That does not mean each
function's behaviour is implemented; verify the eight named-script opcodes
before calling ACS done.

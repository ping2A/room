//! ACS script runtime (Hexen PCD subset).

use crate::load::{AcsModule, ScriptType};
use crate::pcd;

/// Host callbacks used while a script runs (line specials, RNG, world queries).
#[allow(clippy::too_many_arguments)] // ACSF host mirrors GZDoom arity.
pub trait AcsHost {
    /// Execute a Hexen/ZDoom line special with five args; return activation result.
    fn execute_special(&mut self, special: i32, args: [i32; 5]) -> i32;
    /// Inclusive random in `[lo, hi]` (ACS `Random`).
    fn random(&mut self, lo: i32, hi: i32) -> i32;
    /// Level time in tics (`Timer()`).
    fn timer(&self) -> i32;
    /// Player count (stub-friendly).
    fn player_count(&self) -> i32;
    /// Game skill 1..=5.
    fn game_skill(&self) -> i32;
    /// Append a print / bold print string (ENDPRINT / ENDPRINTBOLD).
    fn print(&mut self, text: &str, bold: bool);
    /// `SectorSound(name, volume)` — volume 0..=127; at activator / line sector.
    fn sector_sound(&mut self, name: &str, volume: i32, activator: u64) {
        let _ = (name, volume, activator);
    }
    /// `AmbientSound(name, volume)` — no falloff.
    fn ambient_sound(&mut self, name: &str, volume: i32) {
        let _ = (name, volume);
    }
    /// `LocalAmbientSound(name, volume)` — activator-local (bootstrap = ambient).
    fn local_ambient_sound(&mut self, name: &str, volume: i32, activator: u64) {
        let _ = activator;
        self.ambient_sound(name, volume);
    }
    /// `ActivatorSound(name, volume)`.
    fn activator_sound(&mut self, name: &str, volume: i32, activator: u64) {
        let _ = (name, volume, activator);
    }
    /// `ThingSound(tid, name, volume)`.
    fn thing_sound(&mut self, tid: i32, name: &str, volume: i32, activator: u64) {
        let _ = (tid, name, volume, activator);
    }
    /// ACSF `PlaySound(tid, name [, channel, volume, looping, attenuation, local])`.
    fn play_sound_acs(
        &mut self,
        tid: i32,
        name: &str,
        volume: i32,
        attenuation: i32,
        activator: u64,
    ) {
        let _ = (tid, name, volume, attenuation, activator);
    }
    /// ACSF `StopSound(tid [, channel])` — bootstrap no-op until channels are tracked.
    fn stop_sound_acs(&mut self, tid: i32, channel: i32, activator: u64) {
        let _ = (tid, channel, activator);
    }
    /// ACSF `GetAirSupply(playernum)` — remaining air tics.
    fn get_air_supply(&self, player: i32) -> i32 {
        let _ = player;
        0
    }
    /// ACSF `SetAirSupply(playernum, tics)` — 1 on success.
    fn set_air_supply(&mut self, player: i32, tics: i32) -> i32 {
        let _ = (player, tics);
        0
    }
    /// `SetGravity(amount)` — level gravity as 16.16.
    fn set_gravity(&mut self, amount: i32) {
        let _ = amount;
    }
    /// `GetLevelInfo(selector)` — `LEVELINFO_*`.
    fn level_info(&self, selector: i32) -> i32 {
        let _ = selector;
        0
    }
    /// `ChangeSky(sky1, sky2)`.
    fn change_sky(&mut self, sky1: &str, sky2: &str) {
        let _ = (sky1, sky2);
    }
    /// `ReplaceTextures(from, to, flags)`.
    fn replace_textures(&mut self, from: &str, to: &str, flags: i32) {
        let _ = (from, to, flags);
    }
    /// ACSF `GetArmorType(typename, playernum)`.
    fn get_armor_type(&self, type_name: &str, player: i32) -> i32 {
        let _ = (type_name, player);
        0
    }
    /// ACSF `DropInventory(tid, type)`.
    fn drop_inventory(&mut self, tid: i32, item: &str, activator: u64) -> i32 {
        let _ = (tid, item, activator);
        0
    }
    /// ACSF `LineAttack(tid, angle, pitch, damage, …)`.
    fn line_attack_acs(
        &mut self,
        tid: i32,
        angle: i32,
        pitch: i32,
        damage: i32,
        range: i32,
        activator: u64,
    ) -> i32 {
        let _ = (tid, angle, pitch, damage, range, activator);
        0
    }
    /// `SetAirControl(amount)` — 16.16.
    fn set_air_control(&mut self, amount: i32) {
        let _ = amount;
    }
    /// `PlayerInGame(playernum)`.
    fn player_in_game(&self, player: i32) -> i32 {
        let _ = player;
        0
    }
    /// `GetAmmoCapacity(classname)` for activator (class default / bag max).
    fn get_ammo_capacity(&self, class_name: &str, activator: u64) -> i32 {
        let _ = (class_name, activator);
        0
    }
    /// `SetAmmoCapacity(classname, amount)`.
    fn set_ammo_capacity(&mut self, class_name: &str, amount: i32, activator: u64) {
        let _ = (class_name, amount, activator);
    }
    /// `SetActorState(tid, state [, exact])` — 1 on success.
    fn set_actor_state(&mut self, tid: i32, state: &str, exact: bool, activator: u64) -> i32 {
        let _ = (tid, state, exact, activator);
        0
    }
    /// `Thing_Damage2(tid, amount, mod)` — returns actors damaged.
    fn thing_damage2(&mut self, tid: i32, amount: i32, _mod: &str, activator: u64) -> i32 {
        let _ = (tid, amount, activator);
        0
    }
    /// `UseActorInventory(tid, classname)`.
    fn use_actor_inventory(&mut self, tid: i32, item: &str, activator: u64) -> i32 {
        let _ = (tid, item, activator);
        0
    }
    /// ACSF `SpawnDecal(tid, name, …)` — place DECALDEF at actor.
    fn spawn_decal_acs(&mut self, tid: i32, name: &str, activator: u64) -> i32 {
        let _ = (tid, name, activator);
        0
    }
    /// ACSF `GetArmorInfo(infotype)` for activator player.
    fn get_armor_info(&self, info: i32, activator: u64) -> i32 {
        let _ = (info, activator);
        0
    }
    /// ACSF `SoundVolume` — bootstrap no-op.
    fn sound_volume(&mut self, tid: i32, channel: i32, volume: i32, activator: u64) {
        let _ = (tid, channel, volume, activator);
    }
    /// ACSF `GetActorPowerupTics(tid, powerup)`.
    fn get_actor_powerup_tics(&self, tid: i32, powerup: &str, activator: u64) -> i32 {
        let _ = (tid, powerup, activator);
        0
    }
    /// `ChangeLevel(map, pos, flags, skill)`.
    fn change_level(&mut self, map: &str, _pos: i32, _flags: i32, _skill: i32) {
        let _ = map;
    }
    /// `GetPlayerInfo(playernum, infotype)`.
    fn get_player_info(&self, player: i32, info: i32) -> i32 {
        let _ = (player, info);
        -1
    }
    /// `SectorDamage(tag, amount, type, protection, flags)`.
    fn sector_damage(
        &mut self,
        tag: i32,
        amount: i32,
        _dtype: &str,
        _protection: &str,
        _flags: i32,
    ) {
        let _ = (tag, amount);
    }
    /// `CheckActorFloorTexture` / `CheckActorCeilingTexture`.
    fn check_actor_texture(&self, tid: i32, texture: &str, floor: bool, activator: u64) -> i32 {
        let _ = (tid, texture, floor, activator);
        0
    }
    /// `SetMusic` / `LocalSetMusic` — request a music lump / SNDINFO name.
    fn set_music(&mut self, song: &str, _order: i32, _unused: i32) {
        let _ = song;
    }
    /// `SetFont(name)`.
    fn set_font(&mut self, name: &str) {
        let _ = name;
    }
    /// `FadeTo(r,g,b,a,seconds)` — fixed 16.16 colour / duration.
    fn fade_to(&mut self, r: i32, g: i32, b: i32, a: i32, seconds: i32) {
        let _ = (r, g, b, a, seconds);
    }
    /// `CancelFade`.
    fn cancel_fade(&mut self) {}
    /// `CheckWeapon(name)` — 1 if activator ready weapon matches.
    fn check_weapon(&self, name: &str, activator: u64) -> i32 {
        let _ = (name, activator);
        0
    }
    /// `SetWeapon(name)` — pending weapon switch; 1 on success.
    fn set_weapon(&mut self, name: &str, activator: u64) -> i32 {
        let _ = (name, activator);
        0
    }
    /// `UseInventory(name)` — consume one; 1 on success.
    fn use_inventory(&mut self, name: &str, activator: u64) -> i32 {
        let _ = (name, activator);
        0
    }
    /// `PlayerNumber()` — player slot of activator, or `-1`.
    fn player_number(&self, activator: u64) -> i32 {
        let _ = activator;
        -1
    }
    /// `GetScreenWidth` / `GetScreenHeight`.
    fn screen_size(&self) -> (i32, i32) {
        (320, 200)
    }
    /// `GetCVar(name)` — integer / bool as 0/1; float truncated.
    fn get_cvar(&self, name: &str) -> i32 {
        let _ = name;
        0
    }
    /// Whether any sector with `tag` has an active floor/ceiling/plat mover.
    fn tag_busy(&self, tag: i32) -> bool;
    /// Whether polyobject `po` is still moving.
    fn poly_busy(&self, po: i32) -> bool;
    /// `ClearLineSpecial` — zero the special on linedef `line` (`-1` = no-op).
    fn clear_line_special(&mut self, line: i32) {
        let _ = line;
    }
    /// ACSF `CALLFUNC` dispatch; return value pushed on the ACS stack.
    ///
    /// Pure math ACSF (`Sqrt`, …) and UDMF key resolution are handled in the VM;
    /// the host covers world-touching indices. Unknown indices return `0`.
    fn call_func(&mut self, func_index: i32, args: &[i32], activator: u64) -> i32 {
        let _ = (func_index, args, activator);
        0
    }
    /// `Get*UDMFInt` — `kind`: 0=line, 1=thing, 2=sector, 3=side.
    fn udmf_int(&self, kind: u8, id: i32, key: &str) -> i32 {
        let _ = (kind, id, key);
        0
    }
    /// `Get*UDMFFixed`.
    fn udmf_fixed(&self, kind: u8, id: i32, key: &str) -> i32 {
        let _ = (kind, id, key);
        0
    }
    /// Actor query (`GetActorX` family). `tid == 0` means the script activator.
    /// `activator` is an opaque host key (`0` = none). Values are 16.16 fixed.
    fn actor_x(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `GetActorY`.
    fn actor_y(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `GetActorZ`.
    fn actor_z(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `GetActorFloorZ`.
    fn actor_floor_z(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `GetActorCeilingZ`.
    fn actor_ceiling_z(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `GetActorAngle` — turns as 16.16 (`1.0` = 360°).
    fn actor_angle(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `GetActorPitch` — turns as 16.16.
    fn actor_pitch(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// ACSF `GetActorRoll` — turns as 16.16.
    fn actor_roll(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `ActivatorTID`.
    fn activator_tid(&self, activator: u64) -> i32 {
        let _ = activator;
        0
    }
    /// `GetSectorFloorZ(tag, x, y)` — `x`/`y` are 16.16 map units.
    fn sector_floor_z(&self, tag: i32, x: i32, y: i32) -> i32 {
        let _ = (tag, x, y);
        0
    }
    /// `GetSectorCeilingZ(tag, x, y)`.
    fn sector_ceiling_z(&self, tag: i32, x: i32, y: i32) -> i32 {
        let _ = (tag, x, y);
        0
    }
    /// `GetSectorLightLevel(tag)` — `-1` if no sector.
    fn sector_light_level(&self, tag: i32) -> i32 {
        let _ = tag;
        -1
    }
    /// `GetActorLightLevel(tid)` — sector light of the actor; `0` if missing.
    fn actor_light_level(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `ThingCount(type, tid)` — Hexen spawn type (`0` = any); `tid == 0` = any.
    fn thing_count(&self, type_id: i32, tid: i32) -> i32 {
        let _ = (type_id, tid);
        0
    }
    /// `ThingCountName(classname, tid)`.
    fn thing_count_name(&self, class_name: &str, tid: i32) -> i32 {
        let _ = (class_name, tid);
        0
    }
    /// `ThingCountSector(type, tid, tag)`.
    fn thing_count_sector(&self, type_id: i32, tid: i32, tag: i32) -> i32 {
        let _ = (type_id, tid, tag);
        0
    }
    /// `ThingCountNameSector(classname, tid, tag)`.
    fn thing_count_name_sector(&self, class_name: &str, tid: i32, tag: i32) -> i32 {
        let _ = (class_name, tid, tag);
        0
    }
    /// `ClearActorInventory(tid)`.
    fn clear_actor_inventory(&mut self, tid: i32, activator: u64) {
        let _ = (tid, activator);
    }
    /// `GiveActorInventory(tid, type, amount)` — returns actors that received.
    fn give_actor_inventory(&mut self, tid: i32, item: &str, amount: i32, activator: u64) -> i32 {
        let _ = (tid, item, amount, activator);
        0
    }
    /// `TakeActorInventory(tid, type, amount)` — returns actors that lost items.
    fn take_actor_inventory(&mut self, tid: i32, item: &str, amount: i32, activator: u64) -> i32 {
        let _ = (tid, item, amount, activator);
        0
    }
    /// `CheckActorInventory(tid, type)` — inventory count on first matching actor.
    fn check_actor_inventory(&self, tid: i32, item: &str, activator: u64) -> i32 {
        let _ = (tid, item, activator);
        0
    }
    /// `Spawn(type, x, y, z, angle, tid)` — coords 16.16; angle byte 0..=255.
    /// Returns number of things spawned (0/1 bootstrap).
    fn spawn(&mut self, type_id: i32, x: i32, y: i32, z: i32, angle: i32, tid: i32) -> i32 {
        let _ = (type_id, x, y, z, angle, tid);
        0
    }
    /// `SpawnSpot(type, spot_tid, tid, angle)`. `spot_tid == 0` uses `activator`.
    fn spawn_spot(
        &mut self,
        type_id: i32,
        spot_tid: i32,
        tid: i32,
        angle: i32,
        activator: u64,
    ) -> i32 {
        let _ = (type_id, spot_tid, tid, angle, activator);
        0
    }
    /// `SpawnSpotFacing(type, spot_tid, tid)` — angle taken from each spot.
    fn spawn_spot_facing(&mut self, type_id: i32, spot_tid: i32, tid: i32, activator: u64) -> i32 {
        let _ = (type_id, spot_tid, tid, activator);
        0
    }
    /// `ClassifyActor(tid)` — GZDoom category bitmask (`ACTOR_*`).
    fn classify_actor(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `SetActivator(tid)` — new opaque activator key (`0` = failed / none).
    fn set_activator(&self, tid: i32, current: u64) -> u64 {
        let _ = (tid, current);
        0
    }
    /// `SetActivatorToTarget(tid)`.
    fn set_activator_to_target(&self, tid: i32, current: u64) -> u64 {
        let _ = (tid, current);
        0
    }
    /// `SetActorVelocity(tid, vx, vy, vz, add)` — velocities 16.16.
    fn set_actor_velocity(
        &mut self,
        tid: i32,
        vx: i32,
        vy: i32,
        vz: i32,
        add: bool,
        activator: u64,
    ) -> i32 {
        let _ = (tid, vx, vy, vz, add, activator);
        0
    }
    /// `GetActorProperty(tid, prop)`.
    fn actor_property(&self, tid: i32, prop: i32, activator: u64) -> i32 {
        let _ = (tid, prop, activator);
        0
    }
    /// `SetActorProperty(tid, prop, value)`.
    fn set_actor_property(&mut self, tid: i32, prop: i32, value: i32, activator: u64) {
        let _ = (tid, prop, value, activator);
    }
    /// `SetActorPosition(tid, x, y, z, fog)` — coords 16.16.
    fn set_actor_position(
        &mut self,
        tid: i32,
        x: i32,
        y: i32,
        z: i32,
        fog: bool,
        activator: u64,
    ) -> i32 {
        let _ = (tid, x, y, z, fog, activator);
        0
    }
    /// `SetActorAngle(tid, angle)` — angle in 16.16 turns.
    fn set_actor_angle(&mut self, tid: i32, angle: i32, activator: u64) {
        let _ = (tid, angle, activator);
    }
    /// `SetActorPitch` / `ChangeActorPitch` — pitch in 16.16 turns.
    fn set_actor_pitch(&mut self, tid: i32, pitch: i32, activator: u64) {
        let _ = (tid, pitch, activator);
    }
    /// ACSF `SetActorRoll` / `ChangeActorRoll` — roll in 16.16 turns.
    fn set_actor_roll(&mut self, tid: i32, roll: i32, activator: u64) {
        let _ = (tid, roll, activator);
    }
    /// ACSF `GetMaxInventory(tid, classname)`.
    fn get_max_inventory(&self, tid: i32, class_name: &str, activator: u64) -> i32 {
        let _ = (tid, class_name, activator);
        0
    }
    /// ACSF `CheckActorState(tid, state [, exact])` — 1 if class defines the label.
    fn check_actor_state(&self, tid: i32, state: &str, exact: bool, activator: u64) -> i32 {
        let _ = (tid, state, exact, activator);
        0
    }
    /// ACSF `CheckClass(classname)` — 1 if the actor class exists.
    fn check_class(&self, class_name: &str) -> i32 {
        let _ = class_name;
        0
    }
    /// ACSF `DamageActor(target, ptr1, inflictor, ptr2, amount, damagetype)`.
    fn damage_actor(
        &mut self,
        target_tid: i32,
        inflictor_tid: i32,
        amount: i32,
        _dtype: &str,
        activator: u64,
    ) -> i32 {
        let _ = (target_tid, inflictor_tid, amount, activator);
        0
    }
    /// ACSF `PlayActorSound(tid, soundtype [, channel, volume, looping, attenuation])`.
    fn play_actor_sound(
        &mut self,
        tid: i32,
        sound_type: i32,
        volume: i32,
        attenuation: i32,
        activator: u64,
    ) {
        let _ = (tid, sound_type, volume, attenuation, activator);
    }
    /// ACSF `SetSkyScrollSpeed(sky, speed)` — sky 1/2/3; speed 16.16.
    fn set_sky_scroll_speed(&mut self, sky: i32, speed: i32) {
        let _ = (sky, speed);
    }
    /// `SetUserVariable(tid, name, value)` — returns actors written.
    fn set_user_var(&mut self, tid: i32, name: &str, value: i32, activator: u64) -> i32 {
        let _ = (tid, name, value, activator);
        0
    }
    /// `GetUserVariable(tid, name)`.
    fn get_user_var(&self, tid: i32, name: &str, activator: u64) -> i32 {
        let _ = (tid, name, activator);
        0
    }
    /// `SetUserArray(tid, name, index, value)` — returns actors written.
    fn set_user_array(
        &mut self,
        tid: i32,
        name: &str,
        index: i32,
        value: i32,
        activator: u64,
    ) -> i32 {
        let _ = (tid, name, index, value, activator);
        0
    }
    /// `GetUserArray(tid, name, index)`.
    fn get_user_array(&self, tid: i32, name: &str, index: i32, activator: u64) -> i32 {
        let _ = (tid, name, index, activator);
        0
    }
    /// `CheckActorClass(tid, classname)`.
    fn check_actor_class(&self, tid: i32, class_name: &str, activator: u64) -> i32 {
        let _ = (tid, class_name, activator);
        0
    }
    /// `GetActorClass(tid)` — class name string (empty if missing).
    fn actor_class_name(&self, tid: i32, activator: u64) -> String {
        let _ = (tid, activator);
        String::new()
    }
    /// `APROP_Species` get — runtime override or class `Species`.
    fn actor_species(&self, tid: i32, activator: u64) -> String {
        let _ = (tid, activator);
        String::new()
    }
    /// `APROP_Species` set — empty / `"None"` clears the runtime override.
    fn set_actor_species(&mut self, tid: i32, species: &str, activator: u64) {
        let _ = (tid, species, activator);
    }
    /// `APROP_Nametag` get — runtime tag or class `Tag`.
    fn actor_nametag(&self, tid: i32, activator: u64) -> String {
        let _ = (tid, activator);
        String::new()
    }
    /// `APROP_Nametag` set.
    fn set_actor_nametag(&mut self, tid: i32, tag: &str, activator: u64) {
        let _ = (tid, tag, activator);
    }
    /// `APROP_DamageType` get — runtime or class damage type.
    fn actor_damage_type(&self, tid: i32, activator: u64) -> String {
        let _ = (tid, activator);
        String::new()
    }
    /// `APROP_DamageType` set — empty / `"None"` clears.
    fn set_actor_damage_type(&mut self, tid: i32, dtype: &str, activator: u64) {
        let _ = (tid, dtype, activator);
    }
    /// `APROP_Waterlevel` — 0..=3 immersion (bootstrap: 0 until water sim).
    fn actor_waterlevel(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `APROP_SoundClass` get.
    fn actor_sound_class(&self, tid: i32, activator: u64) -> String {
        let _ = (tid, activator);
        String::new()
    }
    /// `APROP_SoundClass` set.
    fn set_actor_sound_class(&mut self, tid: i32, class: &str, activator: u64) {
        let _ = (tid, class, activator);
    }
    /// Set `APROP_SeeSound` / `AttackSound` / `PainSound` / `DeathSound` / `ActiveSound`.
    fn set_actor_sound_prop(&mut self, tid: i32, prop: i32, sound: &str, activator: u64) {
        let _ = (tid, prop, sound, activator);
    }
    /// `GetActorFloorTexture(tid)` — floor flat name under the actor.
    fn actor_floor_texture(&self, tid: i32, activator: u64) -> String {
        let _ = (tid, activator);
        String::new()
    }
    /// `GetActorFloorTerrain(tid)` — TERRAIN name under the actor.
    fn actor_floor_terrain(&self, tid: i32, activator: u64) -> String {
        let _ = (tid, activator);
        String::new()
    }
    /// `GetWeapon()` — ready weapon class name for the activator player.
    fn weapon_class_name(&self, activator: u64) -> String {
        let _ = activator;
        String::new()
    }
    /// `DropItem(tid, classname [, amount [, chance]])` — spawn a dropped actor.
    fn drop_item(
        &mut self,
        tid: i32,
        class_name: &str,
        amount: i32,
        chance: i32,
        activator: u64,
    ) -> i32 {
        let _ = (tid, class_name, amount, chance, activator);
        0
    }
    /// `CheckProximity(tid, classname, distance, count [, flags])` — 1 if enough in range.
    fn check_proximity(
        &self,
        tid: i32,
        class_name: &str,
        distance: i32,
        count: i32,
        activator: u64,
    ) -> i32 {
        let _ = (tid, class_name, distance, count, activator);
        0
    }
    /// `CheckFlag(tid, flagname)` — 1 if set.
    fn check_flag(&self, tid: i32, flag: &str, activator: u64) -> i32 {
        let _ = (tid, flag, activator);
        0
    }
    /// `SetActorFlag(tid, flagname, value)`.
    fn set_actor_flag(&mut self, tid: i32, flag: &str, value: i32, activator: u64) -> i32 {
        let _ = (tid, flag, value, activator);
        0
    }
    /// `GetActorViewHeight(tid)` — 16.16 fixed.
    fn actor_view_height(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// `CheckActorProperty(tid, prop, value)`.
    fn check_actor_property(&self, tid: i32, prop: i32, value: i32, activator: u64) -> i32 {
        i32::from(self.actor_property(tid, prop, activator) == value)
    }
    /// `Radius_Quake2(tid, intensity, duration, damrad, tremrad, sound)`.
    #[allow(clippy::too_many_arguments)]
    fn radius_quake2(
        &mut self,
        tid: i32,
        intensity: i32,
        duration: i32,
        damrad: i32,
        tremrad: i32,
        sound: &str,
        activator: u64,
    ) -> i32 {
        let _ = (tid, intensity, duration, damrad, tremrad, sound, activator);
        0
    }
    /// `SetCameraToTexture(tid, texture, fov)` — `tid == 0` uses activator.
    fn set_camera_to_texture(&mut self, tid: i32, texture: &str, fov: i32, activator: u64) {
        let _ = (tid, texture, fov, activator);
    }
    /// `ChangeFloor` / `ChangeCeiling` — `ceiling == true` → ceiling flat.
    fn change_flat(&mut self, tag: i32, texture: &str, ceiling: bool) {
        let _ = (tag, texture, ceiling);
    }
    /// `SetLineTexture(lineid, side, position, texture)` — position 0=top,1=mid,2=bottom.
    fn set_line_texture(&mut self, line_id: i32, side: i32, position: i32, texture: &str) {
        let _ = (line_id, side, position, texture);
    }
    /// `SetLineBlocking(lineid, setting)` — ACS `BLOCK_*` modes.
    fn set_line_blocking(&mut self, line_id: i32, setting: i32) {
        let _ = (line_id, setting);
    }
    /// `SetLineSpecial(lineid, special, arg0..arg4)`.
    fn set_line_special(&mut self, line_id: i32, special: i32, args: [i32; 5]) {
        let _ = (line_id, special, args);
    }
    /// `SoundSequence(name)` — SNDSEQ bootstrap (plays name as ambient for now).
    fn sound_sequence(&mut self, name: &str) {
        let _ = name;
    }
    /// ACSF `SoundSequenceOnActor(tid, name)`.
    fn sound_sequence_on_actor(&mut self, tid: i32, name: &str, activator: u64) {
        let _ = (tid, name, activator);
    }
    /// ACSF `SoundSequenceOnSector(tag, name [, location])`.
    fn sound_sequence_on_sector(&mut self, tag: i32, name: &str) {
        let _ = (tag, name);
    }
    /// ACSF `SoundSequenceOnPolyobj(po, name)`.
    fn sound_sequence_on_polyobj(&mut self, po: i32, name: &str) {
        let _ = (po, name);
    }
    /// ACSF `CanRaiseActor(tid)`.
    fn can_raise_actor(&self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// ACSF `SetMusicVolume(volume)` — 16.16 fixed.
    fn set_music_volume(&mut self, volume: i32) {
        let _ = volume;
    }
    /// ACSF `SetTranslation(tid, name)`.
    fn set_translation_acs(&mut self, tid: i32, name: &str, activator: u64) {
        let _ = (tid, name, activator);
    }
    /// ACSF `PickActor(tid, angle, pitch, distance, tid_new [, …])`.
    fn pick_actor(
        &mut self,
        tid: i32,
        angle: i32,
        _pitch: i32,
        distance: i32,
        new_tid: i32,
        return_tid: bool,
        activator: u64,
    ) -> i32 {
        let _ = (tid, angle, distance, new_tid, return_tid, activator);
        0
    }
    /// ACSF `IsPointerEqual(ptr1, ptr2 [, tid1, tid2])` — bootstrap compares actors.
    fn is_pointer_equal(&self, tid1: i32, tid2: i32, activator: u64) -> i32 {
        let _ = (tid1, tid2, activator);
        0
    }
    /// ACSF `SetActorTeleFog(tid, source, dest)`.
    fn set_actor_tele_fog(&mut self, tid: i32, source: &str, dest: &str, activator: u64) {
        let _ = (tid, source, dest, activator);
    }
    /// ACSF `SwapActorTeleFog(tid)`.
    fn swap_actor_tele_fog(&mut self, tid: i32, activator: u64) -> i32 {
        let _ = (tid, activator);
        0
    }
    /// ACSF `SetSectorDamage(tag, amount [, type, interval, leaky])`.
    fn set_sector_damage(&mut self, tag: i32, amount: i32, dtype: &str, interval: i32, leaky: i32) {
        let _ = (tag, amount, dtype, interval, leaky);
    }
    /// ACSF `SetSectorTerrain(tag, position, terrain)` — position 0=floor, 1=ceiling.
    fn set_sector_terrain(&mut self, tag: i32, position: i32, terrain: &str) {
        let _ = (tag, position, terrain);
    }
    /// ACSF `QuakeEx(tid, intensX, intensY, intensZ, duration, damrad, tremrad, sound, …)`.
    fn quake_ex(
        &mut self,
        tid: i32,
        intensity: i32,
        duration: i32,
        damrad: i32,
        tremrad: i32,
        sound: &str,
        activator: u64,
    ) -> i32 {
        self.radius_quake2(tid, intensity, duration, damrad, tremrad, sound, activator)
    }
    /// ACSF `Warp(tid, xofs, yofs, zofs, angle, flags [, state…])` — warp activator to tid.
    fn warp_acs(
        &mut self,
        dest_tid: i32,
        xofs: i32,
        yofs: i32,
        zofs: i32,
        angle: i32,
        _flags: i32,
        activator: u64,
    ) -> i32 {
        let _ = (dest_tid, xofs, yofs, zofs, angle, activator);
        0
    }
    /// ACSF `SetHUDClipRect(x, y, w, h [, …])`.
    fn set_hud_clip_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let _ = (x, y, w, h);
    }
    /// ACSF `SetHUDWrapWidth(w)`.
    fn set_hud_wrap_width(&mut self, w: i32) {
        let _ = w;
    }
    /// `SetHudSize(w, h [, statusbar])` — coordinate space for HudMessage.
    fn set_hud_size(&mut self, w: i32, h: i32, statusbar: i32) {
        let _ = (w, h, statusbar);
    }
    /// `HudMessage` / `ENDHUDMESSAGE` — show `text` (bold when requested).
    fn hud_message(&mut self, text: &str, bold: bool) {
        self.print(text, bold);
    }
    /// `GetPlayerInput(playernum, input)` — button / mouse axes (0 stub).
    fn get_player_input(&self, player: i32, input: i32) -> i32 {
        let _ = (player, input);
        0
    }
    /// `SetMugShotState(state)`.
    fn set_mugshot_state(&mut self, state: &str) {
        let _ = state;
    }
    /// Console player number (−1 when dedicated / no local seat).
    fn console_player_number(&self) -> i32 {
        0
    }
    /// Network game? (0 = single-player bootstrap).
    fn is_network_game(&self) -> i32 {
        0
    }
    /// ACSF `SetCVar(name, value)` — integer/bool bootstrap.
    fn set_cvar(&mut self, name: &str, value: i32) -> i32 {
        let _ = (name, value);
        0
    }
    /// ACSF `GetUserCVar(player, name)`.
    fn get_user_cvar(&self, player: i32, name: &str) -> i32 {
        let _ = (player, name);
        0
    }
    /// ACSF `SetUserCVar(player, name, value)`.
    fn set_user_cvar(&mut self, player: i32, name: &str, value: i32) -> i32 {
        let _ = (player, name, value);
        0
    }
    /// ACSF `GetCVarString(name)` — host returns the string body.
    fn get_cvar_string(&self, name: &str) -> String {
        let _ = name;
        String::new()
    }
    /// ACSF `SetCVarString(name, value)`.
    fn set_cvar_string(&mut self, name: &str, value: &str) -> i32 {
        let _ = (name, value);
        0
    }
    /// ACSF `GetUserCVarString(player, name)`.
    fn get_user_cvar_string(&self, player: i32, name: &str) -> String {
        let _ = (player, name);
        String::new()
    }
    /// ACSF `SetUserCVarString(player, name, value)`.
    fn set_user_cvar_string(&mut self, player: i32, name: &str, value: &str) -> i32 {
        let _ = (player, name, value);
        0
    }
    /// ACSF `StartSlideshow(name)`.
    fn start_slideshow(&mut self, name: &str) {
        let _ = name;
    }
    /// ACSF `GetSectorHealth(tag, part)`.
    fn get_sector_health(&self, tag: i32, part: i32) -> i32 {
        let _ = (tag, part);
        0
    }
    /// ACSF `GetLineHealth(lineid)`.
    fn get_line_health(&self, line_id: i32) -> i32 {
        let _ = line_id;
        0
    }
    /// ACSF `GetLineX(lineid, frac [, offset])` — fixed map X along the line.
    fn get_line_x(&self, line_id: i32, frac: i32, offset: i32) -> i32 {
        let _ = (line_id, frac, offset);
        0
    }
    /// ACSF `GetLineY(lineid, frac [, offset])` — fixed map Y along the line.
    fn get_line_y(&self, line_id: i32, frac: i32, offset: i32) -> i32 {
        let _ = (line_id, frac, offset);
        0
    }
    /// ACSF `SetSubtitleNumber(log, sound)`.
    fn set_subtitle_number(&mut self, log: i32, sound: &str, activator: u64) {
        let _ = (log, sound, activator);
    }
    /// ACSF `GetNetID(tid, ptr [, aaptr])` — bootstrap network id.
    fn get_net_id(&self, tid: i32, ptr: i32, aaptr: i32, activator: u64) -> i32 {
        let _ = (tid, ptr, aaptr, activator);
        0
    }
    /// ACSF `SetActivatorByNetID(netid [, aaptr])` — returns new activator key.
    fn set_activator_by_net_id(&self, net_id: i32, aaptr: i32) -> u64 {
        let _ = (net_id, aaptr);
        0
    }
    /// ACSF `SetSectorGlow(tag, which, r, g, b, height)`.
    fn set_sector_glow(&mut self, tag: i32, which: i32, r: i32, g: i32, b: i32, height: i32) {
        let _ = (tag, which, r, g, b, height);
    }
    /// ACSF `SetFogDensity(tag, density)`.
    fn set_fog_density(&mut self, tag: i32, density: i32) {
        let _ = (tag, density);
    }
    /// ACSF `GetTeamScore(team)`.
    fn get_team_score(&self, team: i32) -> i32 {
        let _ = team;
        0
    }
    /// ACSF `SetTeamScore(team, value)`.
    fn set_team_score(&mut self, team: i32, value: i32) {
        let _ = (team, value);
    }
    /// ACSF `AnnouncerSound(sound [, flags])` — global cue (Skulltag).
    fn announcer_sound(&mut self, name: &str) {
        let _ = name;
    }
    /// ACSF `SetPointer(assign_slot, tid [, pointer_selector])`.
    fn set_pointer_acs(
        &mut self,
        assign_slot: i32,
        tid: i32,
        pointer_selector: i32,
        activator: u64,
    ) -> i32 {
        let _ = (assign_slot, tid, pointer_selector, activator);
        0
    }
    /// ACSF `ScriptCall(...)` — ZScript bridge; bootstrap returns 0.
    fn script_call(&mut self, args: &[i32], activator: u64) -> i32 {
        let _ = (args, activator);
        0
    }
    /// ACSF `SpawnParticle(color [, fullbright, lifetime, size, x, y, z, …])`.
    fn spawn_particle(
        &mut self,
        color: i32,
        fullbright: bool,
        lifetime: i32,
        size: i32,
        x: i32,
        y: i32,
        z: i32,
    ) {
        let _ = (color, fullbright, lifetime, size, x, y, z);
    }
    /// `SetLineActivation(lineid, activation [, repeat])`.
    fn set_line_activation(&mut self, line_id: i32, activation: i32, repeat: i32) {
        let _ = (line_id, activation, repeat);
    }
    /// `GetLineActivation(lineid)`.
    fn get_line_activation(&self, line_id: i32) -> i32 {
        let _ = line_id;
        0
    }
}

/// Per-script run state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptRunState {
    /// Interpreting (or about to).
    Running,
    /// Waiting `n` tics (`DELAY`).
    Delayed(i32),
    /// Waiting for sector movers on tag.
    TagWait(i32),
    /// Waiting for polyobject motion.
    PolyWait(i32),
    /// Waiting for another script number to finish.
    ScriptWait(i32),
    /// Suspended (`SUSPEND` / `ACS_Suspend`); resumable.
    Suspended,
    /// Finished; remove next sweep.
    Terminated,
}

/// Saved state for an ACS `CALL` / `CALLDISCARD` frame.
#[derive(Clone, Debug)]
pub struct CallFrame {
    /// PC to resume after `RETURN*`.
    pub return_pc: u32,
    /// Caller locals restored on return.
    pub locals: Vec<i32>,
    /// When true, discard the function return value (`CALLDISCARD`).
    pub discard_result: bool,
}

/// One live script instance (`DLevelScript` subset).
#[derive(Clone, Debug)]
pub struct RunningScript {
    /// Script number.
    pub number: i32,
    /// Bytecode PC.
    pub pc: u32,
    /// Entry address (for `RESTART`).
    pub entry: u32,
    /// Run state.
    pub state: ScriptRunState,
    /// Expression stack.
    pub stack: Vec<i32>,
    /// Script-local variables (Hexen default 20).
    pub locals: Vec<i32>,
    /// `CALL` / `RETURN` frame stack.
    pub frames: Vec<CallFrame>,
    /// Print buffer between BEGINPRINT / ENDPRINT.
    pub print_buf: String,
    /// Activator side (0 = front).
    pub side: u8,
    /// Activating linedef index (`-1` if none / OPEN / manual start).
    pub line: i32,
    /// Opaque activator key for host actor queries (`0` = none).
    pub activator: u64,
}

/// High bit marks a dynamic string index from [`AcsState::dyn_strings`].
const DYN_STRING_TAG: i32 = 0x4000_0000;

/// Level ACS state (lives on `World`).
#[derive(Clone, Debug)]
pub struct AcsState {
    /// Loaded modules (map BEHAVIOR first).
    pub modules: Vec<AcsModule>,
    /// Running / delayed / suspended scripts (order = run order).
    pub scripts: Vec<RunningScript>,
    /// Map variables (module 0; 128 slots).
    pub map_vars: [i32; 128],
    /// Map arrays from `ARAY`/`AINI` (module 0).
    pub map_arrays: Vec<Vec<i32>>,
    /// World variables (hub-persistent; 256).
    pub world_vars: [i32; 256],
    /// Global variables (64).
    pub global_vars: [i32; 64],
    /// `SetResultValue` / `ACS_ExecuteWithResult` return (GZDoom script result).
    pub result_value: i32,
    /// Dynamic strings (`StrLeft`/`StrMid`/…); bootstrap pool (no GC yet).
    pub dyn_strings: Vec<String>,
}

impl Default for AcsState {
    fn default() -> Self {
        Self {
            modules: Vec::new(),
            scripts: Vec::new(),
            map_vars: [0; 128],
            map_arrays: Vec::new(),
            world_vars: [0; 256],
            global_vars: [0; 64],
            result_value: 0,
            dyn_strings: Vec::new(),
        }
    }
}

impl AcsState {
    /// Empty state (no BEHAVIOR).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve an ACS string index (library or dynamic pool).
    #[must_use]
    pub fn resolve_string(&self, index: i32) -> String {
        acs_string(self, index)
    }

    /// Attach a parsed module (typically map `BEHAVIOR`).
    ///
    /// Applies `MINI` map-var defaults and clones `ARAY`/`AINI` arrays when this
    /// is the first (map) module.
    pub fn push_module(&mut self, module: AcsModule) {
        if self.modules.is_empty() {
            self.map_vars = module.map_var_defaults;
            self.map_arrays = module
                .map_arrays
                .iter()
                .map(|a| a.elements.clone())
                .collect();
        }
        self.modules.push(module);
    }

    /// Whether script `number` is currently running / delayed / waiting (not suspended/terminated).
    #[must_use]
    pub fn is_active(&self, number: i32) -> bool {
        self.scripts.iter().any(|s| {
            s.number == number
                && !matches!(
                    s.state,
                    ScriptRunState::Terminated | ScriptRunState::Suspended
                )
        })
    }

    /// Find a suspended instance of `number`.
    fn find_suspended_mut(&mut self, number: i32) -> Option<&mut RunningScript> {
        self.scripts
            .iter_mut()
            .find(|s| s.number == number && matches!(s.state, ScriptRunState::Suspended))
    }
}

/// Start all scripts of `script_type` in loaded modules.
pub fn start_scripts_of_type(
    state: &mut AcsState,
    host: &mut dyn AcsHost,
    script_type: ScriptType,
    activator: u64,
) {
    let batch: Vec<(i32, u32, u8, u16)> = state
        .modules
        .iter()
        .flat_map(|m| {
            m.scripts
                .iter()
                .filter(|s| s.script_type == script_type)
                .map(|s| (s.number, s.address, s.argc, s.local_count))
                .collect::<Vec<_>>()
        })
        .collect();
    for (number, address, argc, local_count) in batch {
        let _ = start_script_inner(
            state,
            host,
            number,
            address,
            argc,
            local_count,
            &[0; 3],
            activator,
            -1,
            0,
        );
    }
}

/// Start all OPEN scripts in loaded modules (map enter).
pub fn start_open_scripts(state: &mut AcsState, host: &mut dyn AcsHost) {
    start_scripts_of_type(state, host, ScriptType::Open, 0);
}

/// Start ENTER scripts for a player activator (after OPEN).
pub fn start_enter_scripts(state: &mut AcsState, host: &mut dyn AcsHost, activator: u64) {
    start_scripts_of_type(state, host, ScriptType::Enter, activator);
}

/// Start RETURN scripts (hub re-entry).
pub fn start_return_scripts(state: &mut AcsState, host: &mut dyn AcsHost, activator: u64) {
    start_scripts_of_type(state, host, ScriptType::Return, activator);
}

/// Start DEATH scripts for a player activator.
pub fn start_death_scripts(state: &mut AcsState, host: &mut dyn AcsHost, activator: u64) {
    start_scripts_of_type(state, host, ScriptType::Death, activator);
}

/// Start RESPAWN scripts for a player activator.
pub fn start_respawn_scripts(state: &mut AcsState, host: &mut dyn AcsHost, activator: u64) {
    start_scripts_of_type(state, host, ScriptType::Respawn, activator);
}

/// Start UNLOADING scripts (map exit; no delay expected).
pub fn start_unloading_scripts(state: &mut AcsState, host: &mut dyn AcsHost) {
    start_scripts_of_type(state, host, ScriptType::Unloading, 0);
}

/// `ACS_Execute` / `ACS_ExecuteAlways`.
///
/// Returns `1` if a script was started or resumed, else `0`.
pub fn start_script(
    state: &mut AcsState,
    host: &mut dyn AcsHost,
    number: i32,
    args: &[i32; 3],
    always: bool,
) -> i32 {
    start_script_with_activator(state, host, number, args, always, 0)
}

/// Like [`start_script`], with an opaque activator key for actor queries.
pub fn start_script_with_activator(
    state: &mut AcsState,
    host: &mut dyn AcsHost,
    number: i32,
    args: &[i32; 3],
    always: bool,
    activator: u64,
) -> i32 {
    start_script_with_line(state, host, number, args, always, activator, -1, 0)
}

/// Like [`start_script_with_activator`], also recording the activating linedef.
#[allow(clippy::too_many_arguments)]
pub fn start_script_with_line(
    state: &mut AcsState,
    host: &mut dyn AcsHost,
    number: i32,
    args: &[i32; 3],
    always: bool,
    activator: u64,
    line: i32,
    side: u8,
) -> i32 {
    // Resume suspended first.
    if let Some(s) = state.find_suspended_mut(number) {
        s.state = ScriptRunState::Running;
        if line >= 0 {
            s.line = line;
            s.side = side;
        }
        if activator != 0 {
            s.activator = activator;
        }
        let idx = state
            .scripts
            .iter()
            .position(|x| x.number == number && matches!(x.state, ScriptRunState::Running))
            .unwrap_or(0);
        run_one(state, host, idx);
        return 1;
    }
    if !always && state.is_active(number) {
        return 0;
    }
    let Some((address, argc, local_count)) = state.modules.iter().find_map(|m| {
        m.find_script(number)
            .map(|s| (s.address, s.argc, s.local_count))
    }) else {
        return 0;
    };
    start_script_inner(
        state,
        host,
        number,
        address,
        argc,
        local_count,
        args,
        activator,
        line,
        side,
    )
}

#[allow(clippy::too_many_arguments)]
fn start_script_inner(
    state: &mut AcsState,
    host: &mut dyn AcsHost,
    number: i32,
    address: u32,
    argc: u8,
    local_count: u16,
    args: &[i32; 3],
    activator: u64,
    line: i32,
    side: u8,
) -> i32 {
    let n_locals = if local_count == 0 {
        20
    } else {
        usize::from(local_count).max(20)
    };
    let mut locals = vec![0i32; n_locals];
    let n = usize::from(argc.min(3)).min(locals.len());
    locals[..n].copy_from_slice(&args[..n]);
    let script = RunningScript {
        number,
        pc: address,
        entry: address,
        state: ScriptRunState::Running,
        stack: Vec::new(),
        locals,
        frames: Vec::new(),
        print_buf: String::new(),
        side,
        line,
        activator,
    };
    state.scripts.push(script);
    let idx = state.scripts.len() - 1;
    run_one(state, host, idx);
    1
}

/// `ACS_Suspend(script, map)`.
pub fn suspend_script(state: &mut AcsState, number: i32) -> i32 {
    let mut n = 0;
    for s in &mut state.scripts {
        if s.number == number
            && !matches!(
                s.state,
                ScriptRunState::Terminated | ScriptRunState::Suspended
            )
        {
            s.state = ScriptRunState::Suspended;
            n = 1;
        }
    }
    n
}

/// `ACS_Terminate(script, map)`.
pub fn terminate_script(state: &mut AcsState, number: i32) -> i32 {
    let mut n = 0;
    for s in &mut state.scripts {
        if s.number == number && !matches!(s.state, ScriptRunState::Terminated) {
            s.state = ScriptRunState::Terminated;
            n = 1;
        }
    }
    n
}

/// Advance all scripts one tic (delays, waits, then run until yield).
pub fn tick_scripts(state: &mut AcsState, host: &mut dyn AcsHost) {
    // Numbers that still have a live (non-suspended) instance.
    let live_numbers: Vec<i32> = state
        .scripts
        .iter()
        .filter(|s| {
            !matches!(
                s.state,
                ScriptRunState::Terminated
                    | ScriptRunState::Suspended
                    | ScriptRunState::ScriptWait(_)
            )
        })
        .map(|s| s.number)
        .collect();

    // Phase 1: decrement delays / resolve waits.
    for s in &mut state.scripts {
        match s.state {
            ScriptRunState::Delayed(t) => {
                if t <= 1 {
                    s.state = ScriptRunState::Running;
                } else {
                    s.state = ScriptRunState::Delayed(t - 1);
                }
            }
            ScriptRunState::TagWait(tag) => {
                if !host.tag_busy(tag) {
                    s.state = ScriptRunState::Running;
                }
            }
            ScriptRunState::PolyWait(po) => {
                if !host.poly_busy(po) {
                    s.state = ScriptRunState::Running;
                }
            }
            // Wake when no non-waiting instance of `num` remains.
            ScriptRunState::ScriptWait(num) if !live_numbers.contains(&num) => {
                s.state = ScriptRunState::Running;
            }
            _ => {}
        }
    }

    // Phase 2: run Running scripts in list order (indices may grow — only run
    // scripts that existed at start of the phase, matching GZDoom's list walk).
    let n = state.scripts.len();
    for idx in 0..n {
        if matches!(
            state.scripts.get(idx).map(|s| &s.state),
            Some(ScriptRunState::Running)
        ) {
            run_one(state, host, idx);
        }
    }
    // Sweep terminated.
    state
        .scripts
        .retain(|s| !matches!(s.state, ScriptRunState::Terminated));
}

fn run_one(state: &mut AcsState, host: &mut dyn AcsHost, idx: usize) {
    let mut instr = 0u32;
    loop {
        if idx >= state.scripts.len() {
            return;
        }
        if !matches!(state.scripts[idx].state, ScriptRunState::Running) {
            return;
        }
        instr += 1;
        if instr > pcd::RUNAWAY_LIMIT {
            state.scripts[idx].state = ScriptRunState::Terminated;
            host.print(
                &format!("Runaway script {} terminated", state.scripts[idx].number),
                false,
            );
            return;
        }
        if matches!(step(state, host, idx), Step::Break) {
            return;
        }
    }
}

enum Step {
    Continue,
    Break,
}

fn take_op(state: &mut AcsState, idx: usize) -> Option<u32> {
    let mut pc = state.scripts[idx].pc;
    let op = state.modules.first()?.fetch_op(&mut pc)?;
    state.scripts[idx].pc = pc;
    Some(op)
}

fn take_byte_or_word(state: &mut AcsState, idx: usize) -> Option<i32> {
    let mut pc = state.scripts[idx].pc;
    let v = state.modules.first()?.fetch_byte_or_word(&mut pc)?;
    state.scripts[idx].pc = pc;
    Some(v)
}

fn take_i32(state: &mut AcsState, idx: usize) -> Option<i32> {
    let mut pc = state.scripts[idx].pc;
    let v = state.modules.first()?.fetch_i32(&mut pc)?;
    state.scripts[idx].pc = pc;
    Some(v)
}

fn take_u32(state: &mut AcsState, idx: usize) -> Option<u32> {
    let mut pc = state.scripts[idx].pc;
    let v = state.modules.first()?.fetch_u32(&mut pc)?;
    state.scripts[idx].pc = pc;
    Some(v)
}

fn take_u8(state: &mut AcsState, idx: usize) -> Option<u8> {
    let mut pc = state.scripts[idx].pc;
    let v = state.modules.first()?.fetch_u8(&mut pc)?;
    state.scripts[idx].pc = pc;
    Some(v)
}

fn take_short_or_word(state: &mut AcsState, idx: usize) -> Option<i32> {
    let mut pc = state.scripts[idx].pc;
    let v = state.modules.first()?.fetch_short_or_word(&mut pc)?;
    state.scripts[idx].pc = pc;
    Some(v)
}

fn acs_string(state: &AcsState, index: i32) -> String {
    if index < 0 {
        return String::new();
    }
    if index & DYN_STRING_TAG != 0 {
        let slot = (index & !DYN_STRING_TAG) as usize;
        return state.dyn_strings.get(slot).cloned().unwrap_or_default();
    }
    let idx = index as usize;
    state
        .modules
        .first()
        .map(|m| m.string(idx).to_owned())
        .unwrap_or_default()
}

/// ACS angle (16.16 turns, `1.0` = 360°) → radians for [`room_core::dmath`].
fn acs_angle_radians(acs: i32) -> f64 {
    let turns = room_core::Fixed(acs).to_f64();
    turns * std::f64::consts::TAU
}

fn acs_sin(acs_angle: i32) -> i32 {
    let r = acs_angle_radians(acs_angle);
    room_core::Fixed::from_f64(room_core::dmath::sin(r)).0
}

fn acs_cos(acs_angle: i32) -> i32 {
    let r = acs_angle_radians(acs_angle);
    room_core::Fixed::from_f64(room_core::dmath::cos(r)).0
}

fn acs_vector_angle(x: i32, y: i32) -> i32 {
    let xf = room_core::Fixed(x).to_f64();
    let yf = room_core::Fixed(y).to_f64();
    if xf == 0.0 && yf == 0.0 {
        return 0;
    }
    let rad = room_core::dmath::atan2(yf, xf);
    let turns = rad / std::f64::consts::TAU;
    room_core::Fixed::from_f64(turns).0
}

fn add_dyn_string(state: &mut AcsState, s: String) -> i32 {
    // Cap runaway growth from hostile bytecode.
    if state.dyn_strings.len() >= 65_536 {
        return 0;
    }
    let slot = state.dyn_strings.len() as i32;
    state.dyn_strings.push(s);
    DYN_STRING_TAG | slot
}

fn resolve_named_script_number(state: &AcsState, name: &str) -> Option<i32> {
    state
        .modules
        .iter()
        .find_map(|m| m.script_number_by_name(name))
}

/// `ACS_NamedExecute*` / Suspend / Terminate — needs `&mut AcsState`.
fn dispatch_named_script_acs(
    state: &mut AcsState,
    host: &mut dyn AcsHost,
    func_index: i32,
    args: &[i32],
    activator: u64,
) -> i32 {
    let name = acs_string(state, args.first().copied().unwrap_or(0));
    let Some(number) = resolve_named_script_number(state, &name) else {
        return 0;
    };
    // `map` (args[1]) non-zero = cross-map deferred start — not yet supported.
    let map = args.get(1).copied().unwrap_or(0);
    if map != 0
        && matches!(
            func_index,
            pcd::acsf::ACS_NAMED_EXECUTE
                | pcd::acsf::ACS_NAMED_EXECUTE_ALWAYS
                | pcd::acsf::ACS_NAMED_EXECUTE_WITH_RESULT
                | pcd::acsf::ACS_NAMED_LOCKED_EXECUTE
                | pcd::acsf::ACS_NAMED_LOCKED_EXECUTE_DOOR
        )
    {
        return 0;
    }
    let script_args = [
        args.get(2).copied().unwrap_or(0),
        args.get(3).copied().unwrap_or(0),
        args.get(4).copied().unwrap_or(0),
    ];
    match func_index {
        pcd::acsf::ACS_NAMED_SUSPEND => suspend_script(state, number),
        pcd::acsf::ACS_NAMED_TERMINATE => terminate_script(state, number),
        pcd::acsf::ACS_NAMED_EXECUTE_ALWAYS => {
            start_script_with_activator(state, host, number, &script_args, true, activator)
        }
        pcd::acsf::ACS_NAMED_EXECUTE_WITH_RESULT => {
            let _ =
                start_script_with_activator(state, host, number, &script_args, false, activator);
            state.result_value
        }
        pcd::acsf::ACS_NAMED_EXECUTE
        | pcd::acsf::ACS_NAMED_LOCKED_EXECUTE
        | pcd::acsf::ACS_NAMED_LOCKED_EXECUTE_DOOR => {
            start_script_with_activator(state, host, number, &script_args, false, activator)
        }
        _ => 0,
    }
}

fn is_named_script_acs(func_index: i32) -> bool {
    matches!(
        func_index,
        pcd::acsf::ACS_NAMED_EXECUTE
            | pcd::acsf::ACS_NAMED_SUSPEND
            | pcd::acsf::ACS_NAMED_TERMINATE
            | pcd::acsf::ACS_NAMED_LOCKED_EXECUTE
            | pcd::acsf::ACS_NAMED_LOCKED_EXECUTE_DOOR
            | pcd::acsf::ACS_NAMED_EXECUTE_WITH_RESULT
            | pcd::acsf::ACS_NAMED_EXECUTE_ALWAYS
    )
}

/// Pure-math / UDMF-key ACSF in the VM; world queries go to [`AcsHost::call_func`].
fn dispatch_acs_func(
    state: &mut AcsState,
    host: &mut dyn AcsHost,
    func_index: i32,
    args: &[i32],
    activator: u64,
) -> i32 {
    match func_index {
        pcd::acsf::SQRT => {
            let n = args.first().copied().unwrap_or(0);
            room_core::conv::floor_to_int(room_core::dmath::sqrt(f64::from(n)))
        }
        pcd::acsf::FIXED_SQRT => {
            let n = args.first().copied().unwrap_or(0);
            let v = f64::from(n) / f64::from(room_core::FRACUNIT);
            room_core::Fixed::from_f64(room_core::dmath::sqrt(v)).0
        }
        pcd::acsf::VECTOR_LENGTH => {
            let x = f64::from(args.first().copied().unwrap_or(0)) / f64::from(room_core::FRACUNIT);
            let y = f64::from(args.get(1).copied().unwrap_or(0)) / f64::from(room_core::FRACUNIT);
            let len = room_core::dmath::sqrt(x * x + y * y);
            room_core::Fixed::from_f64(len).0
        }
        pcd::acsf::GET_LINE_UDMF_INT => {
            let id = args.first().copied().unwrap_or(0);
            let key = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.udmf_int(0, id, &key)
        }
        pcd::acsf::GET_LINE_UDMF_FIXED => {
            let id = args.first().copied().unwrap_or(0);
            let key = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.udmf_fixed(0, id, &key)
        }
        pcd::acsf::GET_THING_UDMF_INT => {
            let id = args.first().copied().unwrap_or(0);
            let key = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.udmf_int(1, id, &key)
        }
        pcd::acsf::GET_THING_UDMF_FIXED => {
            let id = args.first().copied().unwrap_or(0);
            let key = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.udmf_fixed(1, id, &key)
        }
        pcd::acsf::GET_SECTOR_UDMF_INT => {
            let id = args.first().copied().unwrap_or(0);
            let key = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.udmf_int(2, id, &key)
        }
        pcd::acsf::GET_SECTOR_UDMF_FIXED => {
            let id = args.first().copied().unwrap_or(0);
            let key = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.udmf_fixed(2, id, &key)
        }
        pcd::acsf::GET_SIDE_UDMF_INT => {
            // args: lineid, side, key
            let line_id = args.first().copied().unwrap_or(0);
            let side = args.get(1).copied().unwrap_or(0);
            let key = acs_string(state, args.get(2).copied().unwrap_or(0));
            // Pack side into high bit of id for host (line_id | side<<31).
            let id = line_id | (side.clamp(0, 1) << 30);
            host.udmf_int(3, id, &key)
        }
        pcd::acsf::GET_SIDE_UDMF_FIXED => {
            let line_id = args.first().copied().unwrap_or(0);
            let side = args.get(1).copied().unwrap_or(0);
            let key = acs_string(state, args.get(2).copied().unwrap_or(0));
            let id = line_id | (side.clamp(0, 1) << 30);
            host.udmf_fixed(3, id, &key)
        }
        pcd::acsf::SET_USER_VAR => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            let value = args.get(2).copied().unwrap_or(0);
            host.set_user_var(tid, &name, value, activator)
        }
        pcd::acsf::GET_USER_VAR => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.get_user_var(tid, &name, activator)
        }
        pcd::acsf::SET_USER_ARRAY => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            let index = args.get(2).copied().unwrap_or(0);
            let value = args.get(3).copied().unwrap_or(0);
            host.set_user_array(tid, &name, index, value, activator)
        }
        pcd::acsf::GET_USER_ARRAY => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            let index = args.get(2).copied().unwrap_or(0);
            host.get_user_array(tid, &name, index, activator)
        }
        pcd::acsf::CHECK_ACTOR_CLASS => {
            let tid = args.first().copied().unwrap_or(0);
            let class_name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.check_actor_class(tid, &class_name, activator)
        }
        pcd::acsf::GET_CHAR => {
            let s = acs_string(state, args.first().copied().unwrap_or(0));
            let i = args.get(1).copied().unwrap_or(0);
            if i < 0 {
                0
            } else {
                s.as_bytes()
                    .get(i as usize)
                    .copied()
                    .map(i32::from)
                    .unwrap_or(0)
            }
        }
        pcd::acsf::GET_ACTOR_VIEW_HEIGHT => {
            let tid = args.first().copied().unwrap_or(0);
            host.actor_view_height(tid, activator)
        }
        pcd::acsf::CHECK_ACTOR_PROPERTY => {
            let tid = args.first().copied().unwrap_or(0);
            let prop = args.get(1).copied().unwrap_or(0);
            let value = args.get(2).copied().unwrap_or(0);
            if prop == pcd::aprop::SPECIES {
                let want = acs_string(state, value);
                let got = host.actor_species(tid, activator);
                i32::from(got.eq_ignore_ascii_case(&want))
            } else if prop == pcd::aprop::NAMETAG {
                let want = acs_string(state, value);
                let got = host.actor_nametag(tid, activator);
                i32::from(got.eq_ignore_ascii_case(&want))
            } else if prop == pcd::aprop::DAMAGETYPE {
                let want = acs_string(state, value);
                let got = host.actor_damage_type(tid, activator);
                i32::from(got.eq_ignore_ascii_case(&want))
            } else if prop == pcd::aprop::SOUNDCLASS {
                let want = acs_string(state, value);
                let got = host.actor_sound_class(tid, activator);
                i32::from(got.eq_ignore_ascii_case(&want))
            } else {
                host.check_actor_property(tid, prop, value, activator)
            }
        }
        pcd::acsf::RADIUS_QUAKE2 => {
            let tid = args.first().copied().unwrap_or(0);
            let intensity = args.get(1).copied().unwrap_or(0);
            let duration = args.get(2).copied().unwrap_or(0);
            let damrad = args.get(3).copied().unwrap_or(0);
            let tremrad = args.get(4).copied().unwrap_or(0);
            let sound = acs_string(state, args.get(5).copied().unwrap_or(0));
            host.radius_quake2(tid, intensity, duration, damrad, tremrad, &sound, activator)
        }
        pcd::acsf::SPAWN_SPOT_FORCED => {
            // Bootstrap: same as SpawnSpot (no "forced" collision bypass yet).
            let type_id = args.first().copied().unwrap_or(0);
            let spot = args.get(1).copied().unwrap_or(0);
            let tid = args.get(2).copied().unwrap_or(0);
            let angle = args.get(3).copied().unwrap_or(0);
            host.spawn_spot(type_id, spot, tid, angle, activator)
        }
        pcd::acsf::SPAWN_SPOT_FACING_FORCED => {
            let type_id = args.first().copied().unwrap_or(0);
            let spot = args.get(1).copied().unwrap_or(0);
            let tid = args.get(2).copied().unwrap_or(0);
            host.spawn_spot_facing(type_id, spot, tid, activator)
        }
        pcd::acsf::SPAWN_FORCED => {
            let type_id = args.first().copied().unwrap_or(0);
            let x = args.get(1).copied().unwrap_or(0);
            let y = args.get(2).copied().unwrap_or(0);
            let z = args.get(3).copied().unwrap_or(0);
            let tid = args.get(4).copied().unwrap_or(0);
            let angle = args.get(5).copied().unwrap_or(0);
            host.spawn(type_id, x, y, z, angle, tid)
        }
        pcd::acsf::ANNOUNCER_SOUND => {
            let name = acs_string(state, args.first().copied().unwrap_or(0));
            host.announcer_sound(&name);
            0
        }
        pcd::acsf::SET_POINTER => {
            let assign = args.first().copied().unwrap_or(0);
            let tid = args.get(1).copied().unwrap_or(0);
            let selector = args.get(2).copied().unwrap_or(0);
            host.set_pointer_acs(assign, tid, selector, activator)
        }
        pcd::acsf::PLAY_SOUND => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            // channel (2) / looping (4) / local (6) ignored for bootstrap.
            let volume = args.get(3).copied().unwrap_or(room_core::FRACUNIT);
            let attenuation = args.get(5).copied().unwrap_or(room_core::FRACUNIT);
            host.play_sound_acs(tid, &name, volume, attenuation, activator);
            0
        }
        pcd::acsf::STOP_SOUND => {
            let tid = args.first().copied().unwrap_or(0);
            let channel = args.get(1).copied().unwrap_or(0);
            host.stop_sound_acs(tid, channel, activator);
            0
        }
        pcd::acsf::GET_AIR_SUPPLY => {
            let player = args.first().copied().unwrap_or(0);
            host.get_air_supply(player)
        }
        pcd::acsf::SET_AIR_SUPPLY => {
            let player = args.first().copied().unwrap_or(0);
            let tics = args.get(1).copied().unwrap_or(0);
            host.set_air_supply(player, tics)
        }
        pcd::acsf::GET_ARMOR_TYPE => {
            let type_name = acs_string(state, args.first().copied().unwrap_or(0));
            let player = args.get(1).copied().unwrap_or(0);
            host.get_armor_type(&type_name, player)
        }
        pcd::acsf::DROP_INVENTORY => {
            let tid = args.first().copied().unwrap_or(0);
            let item = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.drop_inventory(tid, &item, activator)
        }
        pcd::acsf::LINE_ATTACK => {
            let tid = args.first().copied().unwrap_or(0);
            let angle = args.get(1).copied().unwrap_or(0);
            let pitch = args.get(2).copied().unwrap_or(0);
            let damage = args.get(3).copied().unwrap_or(0);
            // puff / damagetype / flags / pufftid ignored for bootstrap.
            let range = args.get(6).copied().unwrap_or(0);
            host.line_attack_acs(tid, angle, pitch, damage, range, activator)
        }
        pcd::acsf::SPAWN_DECAL => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.spawn_decal_acs(tid, &name, activator)
        }
        pcd::acsf::GET_ARMOR_INFO => {
            let info = args.first().copied().unwrap_or(0);
            host.get_armor_info(info, activator)
        }
        pcd::acsf::SOUND_VOLUME => {
            let tid = args.first().copied().unwrap_or(0);
            let channel = args.get(1).copied().unwrap_or(0);
            let volume = args.get(2).copied().unwrap_or(0);
            host.sound_volume(tid, channel, volume, activator);
            0
        }
        pcd::acsf::STRCMP => {
            let a = acs_string(state, args.first().copied().unwrap_or(0));
            let b = acs_string(state, args.get(1).copied().unwrap_or(0));
            let n = args.get(2).copied();
            acs_strcmp(&a, &b, false, n)
        }
        pcd::acsf::STRICMP => {
            let a = acs_string(state, args.first().copied().unwrap_or(0));
            let b = acs_string(state, args.get(1).copied().unwrap_or(0));
            let n = args.get(2).copied();
            acs_strcmp(&a, &b, true, n)
        }
        pcd::acsf::STR_LEFT => {
            let s = acs_string(state, args.first().copied().unwrap_or(0));
            let n = args.get(1).copied().unwrap_or(0).max(0) as usize;
            let bytes = s.as_bytes();
            let take = n.min(bytes.len());
            add_dyn_string(state, String::from_utf8_lossy(&bytes[..take]).into_owned())
        }
        pcd::acsf::STR_RIGHT => {
            let s = acs_string(state, args.first().copied().unwrap_or(0));
            let n = args.get(1).copied().unwrap_or(0).max(0) as usize;
            let bytes = s.as_bytes();
            let take = n.min(bytes.len());
            let start = bytes.len() - take;
            add_dyn_string(state, String::from_utf8_lossy(&bytes[start..]).into_owned())
        }
        pcd::acsf::STR_MID => {
            let s = acs_string(state, args.first().copied().unwrap_or(0));
            let pos = args.get(1).copied().unwrap_or(0).max(0) as usize;
            let n = args.get(2).copied().unwrap_or(0).max(0) as usize;
            let bytes = s.as_bytes();
            if pos >= bytes.len() {
                add_dyn_string(state, String::new())
            } else {
                let take = n.min(bytes.len() - pos);
                add_dyn_string(
                    state,
                    String::from_utf8_lossy(&bytes[pos..pos + take]).into_owned(),
                )
            }
        }
        pcd::acsf::GET_ACTOR_CLASS => {
            let tid = args.first().copied().unwrap_or(0);
            let name = host.actor_class_name(tid, activator);
            add_dyn_string(state, name)
        }
        pcd::acsf::GET_ACTOR_FLOOR_TEXTURE => {
            let tid = args.first().copied().unwrap_or(0);
            let name = host.actor_floor_texture(tid, activator);
            add_dyn_string(state, name)
        }
        pcd::acsf::GET_ACTOR_FLOOR_TERRAIN => {
            let tid = args.first().copied().unwrap_or(0);
            let name = host.actor_floor_terrain(tid, activator);
            add_dyn_string(state, name)
        }
        pcd::acsf::GET_WEAPON => {
            let name = host.weapon_class_name(activator);
            add_dyn_string(state, name)
        }
        pcd::acsf::CHECK_FONT => {
            // Bootstrap: any non-empty font name is "available".
            let name = acs_string(state, args.first().copied().unwrap_or(0));
            i32::from(!name.is_empty())
        }
        pcd::acsf::DROP_ITEM => {
            let tid = args.first().copied().unwrap_or(0);
            let class_name = acs_string(state, args.get(1).copied().unwrap_or(0));
            let amount = args.get(2).copied().unwrap_or(1);
            let chance = args.get(3).copied().unwrap_or(256);
            host.drop_item(tid, &class_name, amount, chance, activator)
        }
        pcd::acsf::CHECK_PROXIMITY => {
            let tid = args.first().copied().unwrap_or(0);
            let class_name = acs_string(state, args.get(1).copied().unwrap_or(0));
            let distance = args.get(2).copied().unwrap_or(0);
            let count = args.get(3).copied().unwrap_or(1);
            host.check_proximity(tid, &class_name, distance, count, activator)
        }
        pcd::acsf::CHECK_FLAG => {
            let tid = args.first().copied().unwrap_or(0);
            let flag = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.check_flag(tid, &flag, activator)
        }
        pcd::acsf::SET_LINE_ACTIVATION => {
            let line_id = args.first().copied().unwrap_or(0);
            let activation = args.get(1).copied().unwrap_or(0);
            let repeat = args.get(2).copied().unwrap_or(-1);
            host.set_line_activation(line_id, activation, repeat);
            0
        }
        pcd::acsf::GET_LINE_ACTIVATION => {
            let line_id = args.first().copied().unwrap_or(0);
            host.get_line_activation(line_id)
        }
        pcd::acsf::GET_ACTOR_POWERUP_TICS => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.get_actor_powerup_tics(tid, &name, activator)
        }
        pcd::acsf::CHANGE_ACTOR_ANGLE => {
            // args: tid, angle [, interpolate] — interpolate ignored for now.
            let tid = args.first().copied().unwrap_or(0);
            let angle = args.get(1).copied().unwrap_or(0);
            host.set_actor_angle(tid, angle, activator);
            0
        }
        pcd::acsf::CHANGE_ACTOR_PITCH => {
            let tid = args.first().copied().unwrap_or(0);
            let pitch = args.get(1).copied().unwrap_or(0);
            host.set_actor_pitch(tid, pitch, activator);
            0
        }
        pcd::acsf::SET_ACTOR_ROLL | pcd::acsf::CHANGE_ACTOR_ROLL => {
            // interpolate (args[2]) ignored for bootstrap.
            let tid = args.first().copied().unwrap_or(0);
            let roll = args.get(1).copied().unwrap_or(0);
            host.set_actor_roll(tid, roll, activator);
            0
        }
        pcd::acsf::GET_ACTOR_ROLL => {
            let tid = args.first().copied().unwrap_or(0);
            host.actor_roll(tid, activator)
        }
        pcd::acsf::GET_MAX_INVENTORY => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.get_max_inventory(tid, &name, activator)
        }
        pcd::acsf::CHECK_ACTOR_STATE => {
            let tid = args.first().copied().unwrap_or(0);
            let state_name = acs_string(state, args.get(1).copied().unwrap_or(0));
            let exact = args.get(2).copied().unwrap_or(0) != 0;
            host.check_actor_state(tid, &state_name, exact, activator)
        }
        pcd::acsf::CHECK_CLASS => {
            let name = acs_string(state, args.first().copied().unwrap_or(0));
            host.check_class(&name)
        }
        pcd::acsf::DAMAGE_ACTOR => {
            // ptr_select args ignored — use tid / activator resolution.
            let target = args.first().copied().unwrap_or(0);
            let inflictor = args.get(2).copied().unwrap_or(0);
            let amount = args.get(4).copied().unwrap_or(0);
            let dtype = acs_string(state, args.get(5).copied().unwrap_or(0));
            host.damage_actor(target, inflictor, amount, &dtype, activator)
        }
        pcd::acsf::PLAY_ACTOR_SOUND => {
            let tid = args.first().copied().unwrap_or(0);
            let sound_type = args.get(1).copied().unwrap_or(0);
            let volume = args.get(3).copied().unwrap_or(room_core::FRACUNIT);
            let attenuation = args.get(5).copied().unwrap_or(room_core::FRACUNIT);
            host.play_actor_sound(tid, sound_type, volume, attenuation, activator);
            0
        }
        pcd::acsf::SET_SKY_SCROLL_SPEED => {
            let sky = args.first().copied().unwrap_or(0);
            let speed = args.get(1).copied().unwrap_or(0);
            host.set_sky_scroll_speed(sky, speed);
            1
        }
        pcd::acsf::SOUND_SEQUENCE_ON_ACTOR => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.sound_sequence_on_actor(tid, &name, activator);
            0
        }
        pcd::acsf::SOUND_SEQUENCE_ON_SECTOR => {
            let tag = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.sound_sequence_on_sector(tag, &name);
            0
        }
        pcd::acsf::SOUND_SEQUENCE_ON_POLYOBJ => {
            let po = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.sound_sequence_on_polyobj(po, &name);
            0
        }
        pcd::acsf::PICK_ACTOR => {
            let tid = args.first().copied().unwrap_or(0);
            let angle = args.get(1).copied().unwrap_or(0);
            let pitch = args.get(2).copied().unwrap_or(0);
            let distance = args.get(3).copied().unwrap_or(0);
            let new_tid = args.get(4).copied().unwrap_or(0);
            // flags: bit0 FORCETID, bit1 RETURNTID (bootstrap: any non-zero = return tid)
            let flags = args.get(7).copied().unwrap_or(0);
            host.pick_actor(tid, angle, pitch, distance, new_tid, flags != 0, activator)
        }
        pcd::acsf::IS_POINTER_EQUAL => {
            // ptr selectors ignored — compare tid1/tid2 (default activator).
            let tid1 = args.get(2).copied().unwrap_or(0);
            let tid2 = args.get(3).copied().unwrap_or(0);
            host.is_pointer_equal(tid1, tid2, activator)
        }
        pcd::acsf::CAN_RAISE_ACTOR => {
            let tid = args.first().copied().unwrap_or(0);
            host.can_raise_actor(tid, activator)
        }
        pcd::acsf::SET_HUD_CLIP_RECT => {
            let x = args.first().copied().unwrap_or(0);
            let y = args.get(1).copied().unwrap_or(0);
            let w = args.get(2).copied().unwrap_or(0);
            let h = args.get(3).copied().unwrap_or(0);
            host.set_hud_clip_rect(x, y, w, h);
            0
        }
        pcd::acsf::SET_HUD_WRAP_WIDTH => {
            let w = args.first().copied().unwrap_or(0);
            host.set_hud_wrap_width(w);
            0
        }
        pcd::acsf::SET_CVAR => {
            let name = acs_string(state, args.first().copied().unwrap_or(0));
            let value = args.get(1).copied().unwrap_or(0);
            host.set_cvar(&name, value)
        }
        pcd::acsf::GET_USER_CVAR => {
            let player = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.get_user_cvar(player, &name)
        }
        pcd::acsf::SET_USER_CVAR => {
            let player = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            let value = args.get(2).copied().unwrap_or(0);
            host.set_user_cvar(player, &name, value)
        }
        pcd::acsf::GET_CVAR_STRING => {
            let name = acs_string(state, args.first().copied().unwrap_or(0));
            add_dyn_string(state, host.get_cvar_string(&name))
        }
        pcd::acsf::SET_CVAR_STRING => {
            let name = acs_string(state, args.first().copied().unwrap_or(0));
            let value = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.set_cvar_string(&name, &value)
        }
        pcd::acsf::GET_USER_CVAR_STRING => {
            let player = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            add_dyn_string(state, host.get_user_cvar_string(player, &name))
        }
        pcd::acsf::SET_USER_CVAR_STRING => {
            let player = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            let value = acs_string(state, args.get(2).copied().unwrap_or(0));
            host.set_user_cvar_string(player, &name, &value)
        }
        pcd::acsf::SCRIPT_CALL => host.script_call(args, activator),
        pcd::acsf::START_SLIDESHOW => {
            let name = acs_string(state, args.first().copied().unwrap_or(0));
            host.start_slideshow(&name);
            0
        }
        pcd::acsf::GET_SECTOR_HEALTH => {
            let tag = args.first().copied().unwrap_or(0);
            let part = args.get(1).copied().unwrap_or(0);
            host.get_sector_health(tag, part)
        }
        pcd::acsf::GET_LINE_HEALTH => {
            let line_id = args.first().copied().unwrap_or(0);
            host.get_line_health(line_id)
        }
        pcd::acsf::GET_LINE_X => {
            let line_id = args.first().copied().unwrap_or(0);
            let frac = args.get(1).copied().unwrap_or(0);
            let offset = args.get(2).copied().unwrap_or(0);
            host.get_line_x(line_id, frac, offset)
        }
        pcd::acsf::GET_LINE_Y => {
            let line_id = args.first().copied().unwrap_or(0);
            let frac = args.get(1).copied().unwrap_or(0);
            let offset = args.get(2).copied().unwrap_or(0);
            host.get_line_y(line_id, frac, offset)
        }
        pcd::acsf::SET_SUBTITLE_NUMBER => {
            let log = args.first().copied().unwrap_or(0);
            let sound = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.set_subtitle_number(log, &sound, activator);
            0
        }
        pcd::acsf::GET_NET_ID => {
            let tid = args.first().copied().unwrap_or(0);
            let ptr = args.get(1).copied().unwrap_or(0);
            let aaptr = args.get(2).copied().unwrap_or(0);
            host.get_net_id(tid, ptr, aaptr, activator)
        }
        pcd::acsf::SET_ACTIVATOR_BY_NET_ID => {
            let net_id = args.first().copied().unwrap_or(0);
            let aaptr = args.get(1).copied().unwrap_or(0);
            // CALLFUNC result is int; activator swap is host-side for bootstrap.
            let key = host.set_activator_by_net_id(net_id, aaptr);
            i32::from(key != 0)
        }
        pcd::acsf::SET_SECTOR_GLOW => {
            let tag = args.first().copied().unwrap_or(0);
            let which = args.get(1).copied().unwrap_or(0);
            let r = args.get(2).copied().unwrap_or(0);
            let g = args.get(3).copied().unwrap_or(0);
            let b = args.get(4).copied().unwrap_or(0);
            let height = args.get(5).copied().unwrap_or(0);
            host.set_sector_glow(tag, which, r, g, b, height);
            0
        }
        pcd::acsf::SET_FOG_DENSITY => {
            let tag = args.first().copied().unwrap_or(0);
            let density = args.get(1).copied().unwrap_or(0);
            host.set_fog_density(tag, density);
            0
        }
        pcd::acsf::GET_TEAM_SCORE => {
            let team = args.first().copied().unwrap_or(0);
            host.get_team_score(team)
        }
        pcd::acsf::SET_TEAM_SCORE => {
            let team = args.first().copied().unwrap_or(0);
            let value = args.get(1).copied().unwrap_or(0);
            host.set_team_score(team, value);
            0
        }
        pcd::acsf::SPAWN_PARTICLE => {
            let color = args.first().copied().unwrap_or(0);
            let fullbright = args.get(1).copied().unwrap_or(0) != 0;
            let lifetime = args.get(2).copied().unwrap_or(35);
            let size = args.get(3).copied().unwrap_or(room_core::FRACUNIT);
            let x = args.get(4).copied().unwrap_or(0);
            let y = args.get(5).copied().unwrap_or(0);
            let z = args.get(6).copied().unwrap_or(0);
            host.spawn_particle(color, fullbright, lifetime, size, x, y, z);
            0
        }
        pcd::acsf::QUAKE_EX => {
            let tid = args.first().copied().unwrap_or(0);
            let ix = args.get(1).copied().unwrap_or(0);
            let iy = args.get(2).copied().unwrap_or(0);
            let iz = args.get(3).copied().unwrap_or(0);
            let intensity = ix.abs().max(iy.abs()).max(iz.abs());
            let duration = args.get(4).copied().unwrap_or(0);
            let damrad = args.get(5).copied().unwrap_or(0);
            let tremrad = args.get(6).copied().unwrap_or(0);
            let sound = acs_string(state, args.get(7).copied().unwrap_or(0));
            host.quake_ex(tid, intensity, duration, damrad, tremrad, &sound, activator)
        }
        pcd::acsf::WARP => {
            let dest = args.first().copied().unwrap_or(0);
            let xofs = args.get(1).copied().unwrap_or(0);
            let yofs = args.get(2).copied().unwrap_or(0);
            let zofs = args.get(3).copied().unwrap_or(0);
            let angle = args.get(4).copied().unwrap_or(0);
            let flags = args.get(5).copied().unwrap_or(0);
            host.warp_acs(dest, xofs, yofs, zofs, angle, flags, activator)
        }
        pcd::acsf::SET_ACTOR_TELE_FOG => {
            let tid = args.first().copied().unwrap_or(0);
            let src = acs_string(state, args.get(1).copied().unwrap_or(0));
            let dst = acs_string(state, args.get(2).copied().unwrap_or(0));
            host.set_actor_tele_fog(tid, &src, &dst, activator);
            0
        }
        pcd::acsf::SWAP_ACTOR_TELE_FOG => {
            let tid = args.first().copied().unwrap_or(0);
            host.swap_actor_tele_fog(tid, activator)
        }
        pcd::acsf::SET_SECTOR_DAMAGE => {
            let tag = args.first().copied().unwrap_or(0);
            let amount = args.get(1).copied().unwrap_or(0);
            let dtype = acs_string(state, args.get(2).copied().unwrap_or(0));
            let interval = args.get(3).copied().unwrap_or(32);
            let leaky = args.get(4).copied().unwrap_or(0);
            host.set_sector_damage(tag, amount, &dtype, interval, leaky);
            0
        }
        pcd::acsf::SET_SECTOR_TERRAIN => {
            let tag = args.first().copied().unwrap_or(0);
            let position = args.get(1).copied().unwrap_or(0);
            let terrain = acs_string(state, args.get(2).copied().unwrap_or(0));
            host.set_sector_terrain(tag, position, &terrain);
            0
        }
        pcd::acsf::SET_MUSIC_VOLUME => {
            let vol = args.first().copied().unwrap_or(room_core::FRACUNIT);
            host.set_music_volume(vol);
            0
        }
        pcd::acsf::SET_TRANSLATION => {
            let tid = args.first().copied().unwrap_or(0);
            let name = acs_string(state, args.get(1).copied().unwrap_or(0));
            host.set_translation_acs(tid, &name, activator);
            0
        }
        pcd::acsf::STR_ARG => {
            let n = args.first().copied().unwrap_or(0);
            add_dyn_string(state, n.to_string())
        }
        pcd::acsf::SET_ACTOR_FLAG => {
            let tid = args.first().copied().unwrap_or(0);
            let flag = acs_string(state, args.get(1).copied().unwrap_or(0));
            let value = args.get(2).copied().unwrap_or(0);
            host.set_actor_flag(tid, &flag, value, activator)
        }
        pcd::acsf::FLOOR => {
            let n = args.first().copied().unwrap_or(0);
            // 16.16 fixed → floor toward −∞
            if n >= 0 {
                n >> 16
            } else {
                -(((-n) + 0xFFFF) >> 16)
            }
        }
        pcd::acsf::CEIL => {
            let n = args.first().copied().unwrap_or(0);
            if n >= 0 {
                (n + 0xFFFF) >> 16
            } else {
                -((-n) >> 16)
            }
        }
        pcd::acsf::ROUND => {
            let n = args.first().copied().unwrap_or(0);
            if n >= 0 {
                (n + 0x8000) >> 16
            } else {
                -(((-n) + 0x8000) >> 16)
            }
        }
        // Zandronum-only ACSFs BD guards with `IsZandronum()` — safe defaults.
        pcd::acsf::CONSOLE_PLAYER_NUMBER => host.console_player_number(),
        pcd::acsf::PLAYER_IS_SPECTATOR => 0,
        pcd::acsf::RESET_MAP => 0,
        _ => host.call_func(func_index, args, activator),
    }
}

/// C `strcmp` / `strncmp` / `stricmp` / `strnicmp` over ACS string bytes.
fn acs_strcmp(a: &str, b: &str, case_insensitive: bool, n: Option<i32>) -> i32 {
    let limit = n.map_or(usize::MAX, |v| v.max(0) as usize);
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let mut i = 0usize;
    while i < limit {
        let mut ca = ab.get(i).copied().unwrap_or(0);
        let mut cb = bb.get(i).copied().unwrap_or(0);
        if case_insensitive {
            ca = ca.to_ascii_lowercase();
            cb = cb.to_ascii_lowercase();
        }
        if ca != cb {
            return i32::from(ca) - i32::from(cb);
        }
        if ca == 0 {
            return 0;
        }
        i += 1;
    }
    0
}

fn step(state: &mut AcsState, host: &mut dyn AcsHost, idx: usize) -> Step {
    let Some(op) = take_op(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };

    match op {
        pcd::NOP => Step::Continue,
        pcd::TERMINATE => {
            state.scripts[idx].state = ScriptRunState::Terminated;
            Step::Break
        }
        pcd::SUSPEND => {
            state.scripts[idx].state = ScriptRunState::Suspended;
            Step::Break
        }
        pcd::PUSHNUMBER => {
            let Some(v) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].stack.push(v);
            Step::Continue
        }
        pcd::PUSHBYTE => {
            let Some(v) = take_u8(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].stack.push(i32::from(v));
            Step::Continue
        }
        pcd::PUSH2BYTES | pcd::PUSH3BYTES | pcd::PUSH4BYTES | pcd::PUSH5BYTES => {
            let n = match op {
                pcd::PUSH2BYTES => 2,
                pcd::PUSH3BYTES => 3,
                pcd::PUSH4BYTES => 4,
                _ => 5,
            };
            for _ in 0..n {
                let Some(v) = take_u8(state, idx) else {
                    state.scripts[idx].state = ScriptRunState::Terminated;
                    return Step::Break;
                };
                state.scripts[idx].stack.push(i32::from(v));
            }
            Step::Continue
        }
        pcd::PUSHBYTES => {
            let Some(n) = take_u8(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            for _ in 0..n {
                let Some(v) = take_u8(state, idx) else {
                    state.scripts[idx].state = ScriptRunState::Terminated;
                    return Step::Break;
                };
                state.scripts[idx].stack.push(i32::from(v));
            }
            Step::Continue
        }
        pcd::LSPEC1DIRECTB..=pcd::LSPEC5DIRECTB => {
            let nargs = (op - pcd::LSPEC1DIRECTB + 1) as usize;
            let Some(special) = take_u8(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let mut args = [0i32; 5];
            for arg in args.iter_mut().take(nargs) {
                let Some(v) = take_u8(state, idx) else {
                    state.scripts[idx].state = ScriptRunState::Terminated;
                    return Step::Break;
                };
                *arg = i32::from(v);
            }
            let _ = host.execute_special(i32::from(special), args);
            Step::Continue
        }
        pcd::DELAYDIRECTB => {
            let Some(t) = take_u8(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let t = i32::from(t);
            if t > 0 {
                state.scripts[idx].state = ScriptRunState::Delayed(t);
                Step::Break
            } else {
                Step::Continue
            }
        }
        pcd::RANDOMDIRECTB => {
            let Some(lo) = take_u8(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(hi) = take_u8(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx]
                .stack
                .push(host.random(i32::from(lo), i32::from(hi)));
            Step::Continue
        }
        pcd::LSPEC1..=pcd::LSPEC5 => {
            let nargs = (op - pcd::LSPEC1 + 1) as usize;
            let special = pop(state, idx);
            let mut args = [0i32; 5];
            for i in (0..nargs).rev() {
                args[i] = pop(state, idx);
            }
            let _ = host.execute_special(special, args);
            Step::Continue
        }
        pcd::LSPEC1DIRECT..=pcd::LSPEC5DIRECT => {
            let nargs = (op - pcd::LSPEC1DIRECT + 1) as usize;
            let Some(special) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let mut args = [0i32; 5];
            for arg in args.iter_mut().take(nargs) {
                let Some(v) = take_i32(state, idx) else {
                    state.scripts[idx].state = ScriptRunState::Terminated;
                    return Step::Break;
                };
                *arg = v;
            }
            let _ = host.execute_special(special, args);
            Step::Continue
        }
        pcd::ADD => binop(state, idx, |a, b| a.wrapping_add(b)),
        pcd::SUBTRACT => binop(state, idx, |a, b| a.wrapping_sub(b)),
        pcd::MULTIPLY => binop(state, idx, |a, b| a.wrapping_mul(b)),
        pcd::DIVIDE => {
            let b = pop(state, idx);
            let a = pop(state, idx);
            if b == 0 {
                host.print("Division by zero", false);
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            }
            state.scripts[idx].stack.push(a / b);
            Step::Continue
        }
        pcd::MODULUS => {
            let b = pop(state, idx);
            let a = pop(state, idx);
            if b == 0 {
                host.print("Division by zero", false);
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            }
            state.scripts[idx].stack.push(a % b);
            Step::Continue
        }
        pcd::EQ => binop(state, idx, |a, b| i32::from(a == b)),
        pcd::NE => binop(state, idx, |a, b| i32::from(a != b)),
        pcd::LT => binop(state, idx, |a, b| i32::from(a < b)),
        pcd::GT => binop(state, idx, |a, b| i32::from(a > b)),
        pcd::LE => binop(state, idx, |a, b| i32::from(a <= b)),
        pcd::GE => binop(state, idx, |a, b| i32::from(a >= b)),
        pcd::ASSIGNSCRIPTVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let v = pop(state, idx);
            set_local(state, idx, i as usize, v);
            Step::Continue
        }
        pcd::ASSIGNMAPVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let v = pop(state, idx);
            if let Some(slot) = state.map_vars.get_mut(i as usize) {
                *slot = v;
            }
            Step::Continue
        }
        pcd::ASSIGNWORLDVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let v = pop(state, idx);
            if let Some(slot) = state.world_vars.get_mut(i as usize) {
                *slot = v;
            }
            Step::Continue
        }
        pcd::PUSHSCRIPTVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let v = get_local(state, idx, i as usize);
            state.scripts[idx].stack.push(v);
            Step::Continue
        }
        pcd::PUSHMAPVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let v = state.map_vars.get(i as usize).copied().unwrap_or(0);
            state.scripts[idx].stack.push(v);
            Step::Continue
        }
        pcd::PUSHWORLDVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let v = state.world_vars.get(i as usize).copied().unwrap_or(0);
            state.scripts[idx].stack.push(v);
            Step::Continue
        }
        pcd::ADDSCRIPTVAR => var_op(state, idx, |l, v| l.wrapping_add(v)),
        pcd::ADDMAPVAR => map_var_op(state, idx, |l, v| l.wrapping_add(v)),
        pcd::ADDWORLDVAR => world_var_op(state, idx, |l, v| l.wrapping_add(v)),
        pcd::SUBSCRIPTVAR => var_op(state, idx, |l, v| l.wrapping_sub(v)),
        pcd::SUBMAPVAR => map_var_op(state, idx, |l, v| l.wrapping_sub(v)),
        pcd::SUBWORLDVAR => world_var_op(state, idx, |l, v| l.wrapping_sub(v)),
        pcd::MULSCRIPTVAR => var_op(state, idx, |l, v| l.wrapping_mul(v)),
        pcd::MULMAPVAR => map_var_op(state, idx, |l, v| l.wrapping_mul(v)),
        pcd::MULWORLDVAR => world_var_op(state, idx, |l, v| l.wrapping_mul(v)),
        pcd::DIVSCRIPTVAR => var_div(state, host, idx, true),
        pcd::DIVMAPVAR => map_div(state, host, idx),
        pcd::DIVWORLDVAR => world_div(state, host, idx),
        pcd::MODSCRIPTVAR => var_mod(state, host, idx),
        pcd::MODMAPVAR => map_mod(state, host, idx),
        pcd::MODWORLDVAR => world_mod(state, host, idx),
        pcd::INCSCRIPTVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let v = get_local(state, idx, i as usize).wrapping_add(1);
            set_local(state, idx, i as usize, v);
            Step::Continue
        }
        pcd::INCMAPVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            if let Some(slot) = state.map_vars.get_mut(i as usize) {
                *slot = slot.wrapping_add(1);
            }
            Step::Continue
        }
        pcd::INCWORLDVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            if let Some(slot) = state.world_vars.get_mut(i as usize) {
                *slot = slot.wrapping_add(1);
            }
            Step::Continue
        }
        pcd::DECSCRIPTVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let v = get_local(state, idx, i as usize).wrapping_sub(1);
            set_local(state, idx, i as usize, v);
            Step::Continue
        }
        pcd::DECMAPVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            if let Some(slot) = state.map_vars.get_mut(i as usize) {
                *slot = slot.wrapping_sub(1);
            }
            Step::Continue
        }
        pcd::DECWORLDVAR => {
            let Some(i) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            if let Some(slot) = state.world_vars.get_mut(i as usize) {
                *slot = slot.wrapping_sub(1);
            }
            Step::Continue
        }
        pcd::GOTO => {
            let Some(dest) = take_u32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].pc = dest;
            Step::Continue
        }
        pcd::CASEGOTO => {
            // Immediates: case value, jump address. Stack top is switch expr
            // (left in place on miss for the next CASEGOTO).
            let Some(case_v) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(dest) = take_u32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let top = state.scripts[idx].stack.last().copied().unwrap_or(0);
            if top == case_v {
                let _ = pop(state, idx);
                state.scripts[idx].pc = dest;
            }
            Step::Continue
        }
        pcd::IFGOTO => {
            let Some(dest) = take_u32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            if pop(state, idx) != 0 {
                state.scripts[idx].pc = dest;
            }
            Step::Continue
        }
        pcd::IFNOTGOTO => {
            let Some(dest) = take_u32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            if pop(state, idx) == 0 {
                state.scripts[idx].pc = dest;
            }
            Step::Continue
        }
        pcd::DROP => {
            let _ = pop(state, idx);
            Step::Continue
        }
        pcd::DELAY => {
            let t = pop(state, idx);
            state.scripts[idx].state = ScriptRunState::Delayed(t.max(1));
            Step::Break
        }
        pcd::DELAYDIRECT => {
            let Some(t) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].state = ScriptRunState::Delayed(t.max(1));
            Step::Break
        }
        pcd::RANDOM => {
            let hi = pop(state, idx);
            let lo = pop(state, idx);
            state.scripts[idx].stack.push(host.random(lo, hi));
            Step::Continue
        }
        pcd::RANDOMDIRECT => {
            let Some(lo) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(hi) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].stack.push(host.random(lo, hi));
            Step::Continue
        }
        pcd::TAGWAIT => {
            let tag = pop(state, idx);
            state.scripts[idx].state = ScriptRunState::TagWait(tag);
            Step::Break
        }
        pcd::TAGWAITDIRECT => {
            let Some(tag) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].state = ScriptRunState::TagWait(tag);
            Step::Break
        }
        pcd::POLYWAIT => {
            let po = pop(state, idx);
            state.scripts[idx].state = ScriptRunState::PolyWait(po);
            Step::Break
        }
        pcd::POLYWAITDIRECT => {
            let Some(po) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].state = ScriptRunState::PolyWait(po);
            Step::Break
        }
        pcd::SCRIPTWAIT => {
            let num = pop(state, idx);
            state.scripts[idx].state = ScriptRunState::ScriptWait(num);
            Step::Break
        }
        pcd::SCRIPTWAITDIRECT => {
            let Some(num) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].state = ScriptRunState::ScriptWait(num);
            Step::Break
        }
        pcd::SCRIPTWAITNAMED => {
            let str_idx = pop(state, idx);
            let name = acs_string(state, str_idx);
            let Some(num) = resolve_named_script_number(state, &name) else {
                // Unknown name: do not wait forever — continue.
                return Step::Continue;
            };
            state.scripts[idx].state = ScriptRunState::ScriptWait(num);
            Step::Break
        }
        pcd::RESTART => {
            state.scripts[idx].pc = state.scripts[idx].entry;
            Step::Continue
        }
        pcd::ANDLOGICAL => binop(state, idx, |a, b| i32::from(a != 0 && b != 0)),
        pcd::ORLOGICAL => binop(state, idx, |a, b| i32::from(a != 0 || b != 0)),
        pcd::ANDBITWISE => binop(state, idx, |a, b| a & b),
        pcd::ORBITWISE => binop(state, idx, |a, b| a | b),
        pcd::EORBITWISE => binop(state, idx, |a, b| a ^ b),
        pcd::NEGATELOGICAL => {
            let a = pop(state, idx);
            state.scripts[idx].stack.push(i32::from(a == 0));
            Step::Continue
        }
        pcd::LSHIFT => binop(state, idx, |a, b| a.wrapping_shl(b as u32)),
        pcd::RSHIFT => binop(state, idx, |a, b| a.wrapping_shr(b as u32)),
        pcd::UNARYMINUS => {
            let a = pop(state, idx);
            state.scripts[idx].stack.push(a.wrapping_neg());
            Step::Continue
        }
        pcd::LINESIDE => {
            let side = i32::from(state.scripts[idx].side);
            state.scripts[idx].stack.push(side);
            Step::Continue
        }
        pcd::CHANGEFLOOR => {
            let tex_i = pop(state, idx) as usize;
            let tag = pop(state, idx);
            let tex = module_string(state, tex_i);
            host.change_flat(tag, &tex, false);
            Step::Continue
        }
        pcd::CHANGEFLOORDIRECT => {
            let Some(tag) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(tex_i) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let tex = module_string(state, tex_i as usize);
            host.change_flat(tag, &tex, false);
            Step::Continue
        }
        pcd::CHANGECEILING => {
            let tex_i = pop(state, idx) as usize;
            let tag = pop(state, idx);
            let tex = module_string(state, tex_i);
            host.change_flat(tag, &tex, true);
            Step::Continue
        }
        pcd::CHANGECEILINGDIRECT => {
            let Some(tag) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(tex_i) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let tex = module_string(state, tex_i as usize);
            host.change_flat(tag, &tex, true);
            Step::Continue
        }
        pcd::SETLINETEXTURE => {
            let tex_i = pop(state, idx) as usize;
            let position = pop(state, idx);
            let side = pop(state, idx);
            let line_id = pop(state, idx);
            let tex = module_string(state, tex_i);
            host.set_line_texture(line_id, side, position, &tex);
            Step::Continue
        }
        pcd::SETLINEBLOCKING => {
            let setting = pop(state, idx);
            let line_id = pop(state, idx);
            host.set_line_blocking(line_id, setting);
            Step::Continue
        }
        pcd::SETLINESPECIAL => {
            // Stack (bottom→top): lineid, special, arg0..arg4
            let arg4 = pop(state, idx);
            let arg3 = pop(state, idx);
            let arg2 = pop(state, idx);
            let arg1 = pop(state, idx);
            let arg0 = pop(state, idx);
            let special = pop(state, idx);
            let line_id = pop(state, idx);
            host.set_line_special(line_id, special, [arg0, arg1, arg2, arg3, arg4]);
            Step::Continue
        }
        pcd::SOUNDSEQUENCE => {
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            host.sound_sequence(&name);
            Step::Continue
        }
        pcd::SECTORSOUND => {
            let volume = pop(state, idx);
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            host.sector_sound(&name, volume, act);
            Step::Continue
        }
        pcd::AMBIENTSOUND => {
            let volume = pop(state, idx);
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            host.ambient_sound(&name, volume);
            Step::Continue
        }
        pcd::LOCALAMBIENTSOUND => {
            let volume = pop(state, idx);
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            host.local_ambient_sound(&name, volume, act);
            Step::Continue
        }
        pcd::ACTIVATORSOUND => {
            let volume = pop(state, idx);
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            host.activator_sound(&name, volume, act);
            Step::Continue
        }
        pcd::THINGSOUND => {
            let volume = pop(state, idx);
            let str_i = pop(state, idx);
            let tid = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            host.thing_sound(tid, &name, volume, act);
            Step::Continue
        }
        pcd::CLEARINVENTORY => {
            let act = state.scripts[idx].activator;
            host.clear_actor_inventory(0, act);
            Step::Continue
        }
        pcd::GIVEINVENTORY => {
            let amount = pop(state, idx);
            let str_i = pop(state, idx);
            let item = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            let _ = host.give_actor_inventory(0, &item, amount, act);
            Step::Continue
        }
        pcd::TAKEINVENTORY => {
            let amount = pop(state, idx);
            let str_i = pop(state, idx);
            let item = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            let _ = host.take_actor_inventory(0, &item, amount, act);
            Step::Continue
        }
        pcd::CHECKINVENTORY => {
            let str_i = pop(state, idx);
            let item = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.check_actor_inventory(0, &item, act));
            Step::Continue
        }
        pcd::GIVEINVENTORYDIRECT => {
            let Some(str_i) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(amount) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let item = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            let _ = host.give_actor_inventory(0, &item, amount, act);
            Step::Continue
        }
        pcd::TAKEINVENTORYDIRECT => {
            let Some(str_i) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(amount) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let item = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            let _ = host.take_actor_inventory(0, &item, amount, act);
            Step::Continue
        }
        pcd::CHECKINVENTORYDIRECT => {
            let Some(str_i) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let item = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.check_actor_inventory(0, &item, act));
            Step::Continue
        }
        pcd::SETMUSIC | pcd::LOCALSETMUSIC => {
            let unused = pop(state, idx);
            let order = pop(state, idx);
            let str_i = pop(state, idx);
            let song = acs_string(state, str_i);
            host.set_music(&song, order, unused);
            Step::Continue
        }
        pcd::SETMUSICDIRECT | pcd::LOCALSETMUSICDIRECT => {
            let Some(str_i) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(order) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(unused) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let song = acs_string(state, str_i);
            host.set_music(&song, order, unused);
            Step::Continue
        }
        pcd::SETFONT => {
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            host.set_font(&name);
            Step::Continue
        }
        pcd::SETFONTDIRECT => {
            let Some(str_i) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let name = acs_string(state, str_i);
            host.set_font(&name);
            Step::Continue
        }
        pcd::FADETO => {
            let seconds = pop(state, idx);
            let a = pop(state, idx);
            let b = pop(state, idx);
            let g = pop(state, idx);
            let r = pop(state, idx);
            host.fade_to(r, g, b, a, seconds);
            Step::Continue
        }
        pcd::FADERANGE => {
            // Bootstrap: treat end colour as FadeTo target; ignore start + position.
            let seconds = pop(state, idx);
            let a2 = pop(state, idx);
            let b2 = pop(state, idx);
            let g2 = pop(state, idx);
            let r2 = pop(state, idx);
            let _a1 = pop(state, idx);
            let _b1 = pop(state, idx);
            let _g1 = pop(state, idx);
            let _r1 = pop(state, idx);
            host.fade_to(r2, g2, b2, a2, seconds);
            Step::Continue
        }
        pcd::CANCELFADE => {
            host.cancel_fade();
            Step::Continue
        }
        pcd::SIN => {
            let ang = pop(state, idx);
            state.scripts[idx].stack.push(acs_sin(ang));
            Step::Continue
        }
        pcd::COS => {
            let ang = pop(state, idx);
            state.scripts[idx].stack.push(acs_cos(ang));
            Step::Continue
        }
        pcd::VECTORANGLE => {
            let y = pop(state, idx);
            let x = pop(state, idx);
            state.scripts[idx].stack.push(acs_vector_angle(x, y));
            Step::Continue
        }
        pcd::PLAYERNUMBER => {
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.player_number(act));
            Step::Continue
        }
        pcd::GETSCREENWIDTH => {
            state.scripts[idx].stack.push(host.screen_size().0);
            Step::Continue
        }
        pcd::GETSCREENHEIGHT => {
            state.scripts[idx].stack.push(host.screen_size().1);
            Step::Continue
        }
        pcd::STRLEN => {
            let str_i = pop(state, idx);
            let s = acs_string(state, str_i);
            state.scripts[idx]
                .stack
                .push(i32::try_from(s.len()).unwrap_or(i32::MAX));
            Step::Continue
        }
        pcd::GETCVAR => {
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            state.scripts[idx].stack.push(host.get_cvar(&name));
            Step::Continue
        }
        pcd::CHECKWEAPON => {
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.check_weapon(&name, act));
            Step::Continue
        }
        pcd::SETWEAPON => {
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.set_weapon(&name, act));
            Step::Continue
        }
        pcd::USEINVENTORY => {
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.use_inventory(&name, act));
            Step::Continue
        }
        pcd::CLEARLINESPECIAL => {
            let line = state.scripts[idx].line;
            host.clear_line_special(line);
            Step::Continue
        }
        pcd::BEGINPRINT => {
            state.scripts[idx].print_buf.clear();
            Step::Continue
        }
        pcd::ENDPRINT => {
            let text = std::mem::take(&mut state.scripts[idx].print_buf);
            host.print(&text, false);
            Step::Continue
        }
        pcd::ENDPRINTBOLD => {
            let text = std::mem::take(&mut state.scripts[idx].print_buf);
            host.print(&text, true);
            Step::Continue
        }
        pcd::MOREHUDMESSAGE => {
            // GZDoom keeps the print buffer; subsequent PRINT* continue it.
            Step::Continue
        }
        pcd::OPTHUDMESSAGE => {
            // type, id, color, x, y, hold [, …] — consume stack for BD5 bootstrap.
            for _ in 0..6 {
                let _ = pop(state, idx);
            }
            Step::Continue
        }
        pcd::ENDHUDMESSAGE | pcd::ENDHUDMESSAGEBOLD => {
            let bold = op == pcd::ENDHUDMESSAGEBOLD;
            let text = std::mem::take(&mut state.scripts[idx].print_buf);
            host.hud_message(&text, bold);
            Step::Continue
        }
        pcd::SETHUDSIZE => {
            // Stack: width, height [, statusbar] — GZDoom pops 3 after SetHudSize.
            let statusbar = pop(state, idx);
            let h = pop(state, idx);
            let w = pop(state, idx);
            host.set_hud_size(w, h, statusbar);
            Step::Continue
        }
        pcd::GETPLAYERINPUT => {
            let input = pop(state, idx);
            let player = pop(state, idx);
            state.scripts[idx]
                .stack
                .push(host.get_player_input(player, input));
            Step::Continue
        }
        pcd::SETMUGSHOTSTATE => {
            let i = pop(state, idx);
            let name = acs_string(state, i);
            host.set_mugshot_state(&name);
            Step::Continue
        }
        pcd::SAVESTRING => {
            // Intern the print buffer as a dynamic string id (GZDoom StrParam).
            let text = std::mem::take(&mut state.scripts[idx].print_buf);
            let id = add_dyn_string(state, text);
            state.scripts[idx].stack.push(id);
            Step::Continue
        }
        pcd::PRINTMAPCHARARRAY
        | pcd::PRINTWORLDCHARARRAY
        | pcd::PRINTGLOBALCHARARRAY
        | pcd::PRINTSCRIPTCHARARRAY => {
            // Stack: array id, index — bootstrap: append nothing, pop 2.
            let _index = pop(state, idx);
            let _array = pop(state, idx);
            Step::Continue
        }
        pcd::PRINTMAPCHRANGE
        | pcd::PRINTWORLDCHRANGE
        | pcd::PRINTGLOBALCHRANGE
        | pcd::PRINTSCRIPTCHRANGE => {
            // Stack: array, index, length (approx.) — consume 3.
            let _ = pop(state, idx);
            let _ = pop(state, idx);
            let _ = pop(state, idx);
            Step::Continue
        }
        pcd::STRCPYTOMAPCHRANGE
        | pcd::STRCPYTOWORLDCHRANGE
        | pcd::STRCPYTOGLOBALCHRANGE
        | pcd::STRCPYTOSCRIPTCHRANGE => {
            // GZDoom pops 5 operands and pushes success (1).
            for _ in 0..5 {
                let _ = pop(state, idx);
            }
            state.scripts[idx].stack.push(1);
            Step::Continue
        }
        pcd::PRINTSTRING => {
            let i = pop(state, idx) as usize;
            let s = state
                .modules
                .first()
                .map(|m| m.string(i).to_owned())
                .unwrap_or_default();
            state.scripts[idx].print_buf.push_str(&s);
            Step::Continue
        }
        pcd::PRINTNUMBER => {
            let n = pop(state, idx);
            state.scripts[idx].print_buf.push_str(&n.to_string());
            Step::Continue
        }
        pcd::PRINTCHARACTER => {
            let c = pop(state, idx) as u8 as char;
            state.scripts[idx].print_buf.push(c);
            Step::Continue
        }
        pcd::PLAYERCOUNT => {
            state.scripts[idx].stack.push(host.player_count());
            Step::Continue
        }
        pcd::GAMETYPE => {
            // 0 = single player stub
            state.scripts[idx].stack.push(0);
            Step::Continue
        }
        pcd::GAMESKILL => {
            state.scripts[idx].stack.push(host.game_skill());
            Step::Continue
        }
        pcd::TIMER => {
            state.scripts[idx].stack.push(host.timer());
            Step::Continue
        }
        pcd::FIXEDMUL => {
            let b = pop(state, idx);
            let a = pop(state, idx);
            let r = room_core::Fixed(a).mul(room_core::Fixed(b)).0;
            state.scripts[idx].stack.push(r);
            Step::Continue
        }
        pcd::FIXEDDIV => {
            let b = pop(state, idx);
            let a = pop(state, idx);
            let r = (room_core::Fixed(a) / room_core::Fixed(b)).0;
            state.scripts[idx].stack.push(r);
            Step::Continue
        }
        pcd::GETACTORX => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.actor_x(tid, act));
            Step::Continue
        }
        pcd::GETACTORY => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.actor_y(tid, act));
            Step::Continue
        }
        pcd::GETACTORZ => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.actor_z(tid, act));
            Step::Continue
        }
        pcd::GETACTORFLOORZ => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.actor_floor_z(tid, act));
            Step::Continue
        }
        pcd::GETACTORCEILINGZ => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.actor_ceiling_z(tid, act));
            Step::Continue
        }
        pcd::GETACTORANGLE => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.actor_angle(tid, act));
            Step::Continue
        }
        pcd::GETACTORPITCH => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.actor_pitch(tid, act));
            Step::Continue
        }
        pcd::ACTIVATORTID => {
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.activator_tid(act));
            Step::Continue
        }
        pcd::GETACTORPROPERTY => {
            let prop = pop(state, idx);
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            let v = if prop == pcd::aprop::SPECIES {
                add_dyn_string(state, host.actor_species(tid, act))
            } else if prop == pcd::aprop::NAMETAG {
                add_dyn_string(state, host.actor_nametag(tid, act))
            } else if prop == pcd::aprop::DAMAGETYPE {
                add_dyn_string(state, host.actor_damage_type(tid, act))
            } else if prop == pcd::aprop::SOUNDCLASS {
                add_dyn_string(state, host.actor_sound_class(tid, act))
            } else if prop == pcd::aprop::WATERLEVEL {
                host.actor_waterlevel(tid, act)
            } else {
                // See/Attack/Pain/Death/Active sounds are set-only → 0 on get.
                host.actor_property(tid, prop, act)
            };
            state.scripts[idx].stack.push(v);
            Step::Continue
        }
        pcd::SETACTORPROPERTY => {
            let value = pop(state, idx);
            let prop = pop(state, idx);
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            if prop == pcd::aprop::SPECIES {
                let s = acs_string(state, value);
                host.set_actor_species(tid, &s, act);
            } else if prop == pcd::aprop::NAMETAG {
                let s = acs_string(state, value);
                host.set_actor_nametag(tid, &s, act);
            } else if prop == pcd::aprop::DAMAGETYPE {
                let s = acs_string(state, value);
                host.set_actor_damage_type(tid, &s, act);
            } else if prop == pcd::aprop::SOUNDCLASS {
                let s = acs_string(state, value);
                host.set_actor_sound_class(tid, &s, act);
            } else if matches!(
                prop,
                pcd::aprop::SEESOUND
                    | pcd::aprop::ATTACKSOUND
                    | pcd::aprop::PAINSOUND
                    | pcd::aprop::DEATHSOUND
                    | pcd::aprop::ACTIVESOUND
            ) {
                let s = acs_string(state, value);
                host.set_actor_sound_prop(tid, prop, &s, act);
            } else if prop != pcd::aprop::WATERLEVEL {
                // Waterlevel is read-only.
                host.set_actor_property(tid, prop, value, act);
            }
            Step::Continue
        }
        pcd::SETACTORANGLE => {
            let angle = pop(state, idx);
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            host.set_actor_angle(tid, angle, act);
            Step::Continue
        }
        pcd::SETACTORPITCH => {
            let pitch = pop(state, idx);
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            host.set_actor_pitch(tid, pitch, act);
            Step::Continue
        }
        pcd::SETGRAVITY => {
            let amount = pop(state, idx);
            host.set_gravity(amount);
            Step::Continue
        }
        pcd::SETGRAVITYDIRECT => {
            let Some(amount) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            host.set_gravity(amount);
            Step::Continue
        }
        pcd::SETAIRCONTROL => {
            let amount = pop(state, idx);
            host.set_air_control(amount);
            Step::Continue
        }
        pcd::SETAIRCONTROLDIRECT => {
            let Some(amount) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            host.set_air_control(amount);
            Step::Continue
        }
        pcd::SINGLEPLAYER => {
            state.scripts[idx].stack.push(1);
            Step::Continue
        }
        pcd::PLAYERINGAME => {
            let p = pop(state, idx);
            state.scripts[idx].stack.push(host.player_in_game(p));
            Step::Continue
        }
        pcd::GETAMMOCAPACITY => {
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.get_ammo_capacity(&name, act));
            Step::Continue
        }
        pcd::SETAMMOCAPACITY => {
            let amount = pop(state, idx);
            let str_i = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            host.set_ammo_capacity(&name, amount, act);
            Step::Continue
        }
        pcd::SETACTORSTATE => {
            // Stack: tid, state [, exact] — exact optional (default 0).
            // ACC always pushes 3 args for the PCD.
            let exact = pop(state, idx);
            let str_i = pop(state, idx);
            let tid = pop(state, idx);
            let label = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.set_actor_state(tid, &label, exact != 0, act));
            Step::Continue
        }
        pcd::THINGDAMAGE2 => {
            let mod_i = pop(state, idx);
            let amount = pop(state, idx);
            let tid = pop(state, idx);
            let mod_name = acs_string(state, mod_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.thing_damage2(tid, amount, &mod_name, act));
            Step::Continue
        }
        pcd::USEACTORINVENTORY => {
            let str_i = pop(state, idx);
            let tid = pop(state, idx);
            let name = acs_string(state, str_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.use_actor_inventory(tid, &name, act));
            Step::Continue
        }
        pcd::NEGATEBINARY => {
            let a = pop(state, idx);
            state.scripts[idx].stack.push(!a);
            Step::Continue
        }
        pcd::ENDLOG => {
            let text = std::mem::take(&mut state.scripts[idx].print_buf);
            host.print(&text, false);
            Step::Continue
        }
        pcd::GETLEVELINFO => {
            let sel = pop(state, idx);
            state.scripts[idx].stack.push(host.level_info(sel));
            Step::Continue
        }
        pcd::CHANGESKY => {
            let sky2_i = pop(state, idx);
            let sky1_i = pop(state, idx);
            let sky1 = acs_string(state, sky1_i);
            let sky2 = acs_string(state, sky2_i);
            host.change_sky(&sky1, &sky2);
            Step::Continue
        }
        pcd::REPLACETEXTURES => {
            let flags = pop(state, idx);
            let to_i = pop(state, idx);
            let from_i = pop(state, idx);
            let from = acs_string(state, from_i);
            let to = acs_string(state, to_i);
            host.replace_textures(&from, &to, flags);
            Step::Continue
        }
        pcd::CHANGELEVEL => {
            let skill = pop(state, idx);
            let flags = pop(state, idx);
            let pos = pop(state, idx);
            let map_i = pop(state, idx);
            let map = acs_string(state, map_i);
            host.change_level(&map, pos, flags, skill);
            Step::Continue
        }
        pcd::GETPLAYERINFO => {
            let info = pop(state, idx);
            let player = pop(state, idx);
            state.scripts[idx]
                .stack
                .push(host.get_player_info(player, info));
            Step::Continue
        }
        pcd::SECTORDAMAGE => {
            let flags = pop(state, idx);
            let prot_i = pop(state, idx);
            let type_i = pop(state, idx);
            let amount = pop(state, idx);
            let tag = pop(state, idx);
            let dtype = acs_string(state, type_i);
            let prot = acs_string(state, prot_i);
            host.sector_damage(tag, amount, &dtype, &prot, flags);
            Step::Continue
        }
        pcd::CHECKACTORFLOORTEXTURE | pcd::CHECKACTORCEILINGTEXTURE => {
            let floor = op == pcd::CHECKACTORFLOORTEXTURE;
            let tex_i = pop(state, idx);
            let tid = pop(state, idx);
            let tex = acs_string(state, tex_i);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.check_actor_texture(tid, &tex, floor, act));
            Step::Continue
        }
        pcd::SETCAMERATOTEXTURE => {
            // Stack: tid, texture string index, fov (top).
            let fov = pop(state, idx);
            let str_i = pop(state, idx) as usize;
            let tid = pop(state, idx);
            let tex = state
                .modules
                .first()
                .map(|m| m.string(str_i).to_owned())
                .unwrap_or_default();
            let act = state.scripts[idx].activator;
            host.set_camera_to_texture(tid, &tex, fov, act);
            Step::Continue
        }
        pcd::SETACTORPOSITION => {
            let fog = pop(state, idx);
            let z = pop(state, idx);
            let y = pop(state, idx);
            let x = pop(state, idx);
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.set_actor_position(tid, x, y, z, fog != 0, act));
            Step::Continue
        }
        pcd::CLEARACTORINVENTORY => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            host.clear_actor_inventory(tid, act);
            Step::Continue
        }
        pcd::GIVEACTORINVENTORY => {
            let amount = pop(state, idx);
            let str_i = pop(state, idx) as usize;
            let tid = pop(state, idx);
            let item = state
                .modules
                .first()
                .map(|m| m.string(str_i).to_owned())
                .unwrap_or_default();
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.give_actor_inventory(tid, &item, amount, act));
            Step::Continue
        }
        pcd::TAKEACTORINVENTORY => {
            let amount = pop(state, idx);
            let str_i = pop(state, idx) as usize;
            let tid = pop(state, idx);
            let item = state
                .modules
                .first()
                .map(|m| m.string(str_i).to_owned())
                .unwrap_or_default();
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.take_actor_inventory(tid, &item, amount, act));
            Step::Continue
        }
        pcd::CHECKACTORINVENTORY => {
            let str_i = pop(state, idx) as usize;
            let tid = pop(state, idx);
            let item = state
                .modules
                .first()
                .map(|m| m.string(str_i).to_owned())
                .unwrap_or_default();
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.check_actor_inventory(tid, &item, act));
            Step::Continue
        }
        pcd::GETSECTORFLOORZ => {
            let y = pop(state, idx);
            let x = pop(state, idx);
            let tag = pop(state, idx);
            state.scripts[idx]
                .stack
                .push(host.sector_floor_z(tag, x, y));
            Step::Continue
        }
        pcd::GETSECTORCEILINGZ => {
            let y = pop(state, idx);
            let x = pop(state, idx);
            let tag = pop(state, idx);
            state.scripts[idx]
                .stack
                .push(host.sector_ceiling_z(tag, x, y));
            Step::Continue
        }
        pcd::GETSECTORLIGHTLEVEL => {
            let tag = pop(state, idx);
            state.scripts[idx].stack.push(host.sector_light_level(tag));
            Step::Continue
        }
        pcd::GETACTORLIGHTLEVEL => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.actor_light_level(tid, act));
            Step::Continue
        }
        pcd::THINGCOUNT => {
            let tid = pop(state, idx);
            let type_id = pop(state, idx);
            state.scripts[idx]
                .stack
                .push(host.thing_count(type_id, tid));
            Step::Continue
        }
        pcd::THINGCOUNTNAME => {
            let tid = pop(state, idx);
            let str_i = pop(state, idx) as usize;
            let name = state
                .modules
                .first()
                .map(|m| m.string(str_i).to_owned())
                .unwrap_or_default();
            state.scripts[idx]
                .stack
                .push(host.thing_count_name(&name, tid));
            Step::Continue
        }
        pcd::THINGCOUNTSECTOR => {
            let tag = pop(state, idx);
            let tid = pop(state, idx);
            let type_id = pop(state, idx);
            state.scripts[idx]
                .stack
                .push(host.thing_count_sector(type_id, tid, tag));
            Step::Continue
        }
        pcd::THINGCOUNTNAMESECTOR => {
            let tag = pop(state, idx);
            let tid = pop(state, idx);
            let str_i = pop(state, idx) as usize;
            let name = state
                .modules
                .first()
                .map(|m| m.string(str_i).to_owned())
                .unwrap_or_default();
            state.scripts[idx]
                .stack
                .push(host.thing_count_name_sector(&name, tid, tag));
            Step::Continue
        }
        pcd::THINGCOUNTDIRECT => {
            // Two word immediates (ACS0/ACSE); packed ACSe still uses word-sized NEXT.
            let Some(type_id) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(tid) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx]
                .stack
                .push(host.thing_count(type_id, tid));
            Step::Continue
        }
        pcd::SPAWN => {
            // STACK: type,x,y,z,tid,angle (top) → DoSpawn(type,x,y,z,tid,angle)
            let angle = pop(state, idx);
            let tid = pop(state, idx);
            let z = pop(state, idx);
            let y = pop(state, idx);
            let x = pop(state, idx);
            let type_id = pop(state, idx);
            let n = host.spawn(type_id, x, y, z, angle, tid);
            state.scripts[idx].stack.push(n);
            Step::Continue
        }
        pcd::SPAWNDIRECT => {
            let Some(type_id) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(x) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(y) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(z) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(tid) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(angle) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx]
                .stack
                .push(host.spawn(type_id, x, y, z, angle, tid));
            Step::Continue
        }
        pcd::SPAWNSPOT => {
            let angle = pop(state, idx);
            let tid = pop(state, idx);
            let spot = pop(state, idx);
            let type_id = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.spawn_spot(type_id, spot, tid, angle, act));
            Step::Continue
        }
        pcd::SPAWNSPOTFACING => {
            let tid = pop(state, idx);
            let spot = pop(state, idx);
            let type_id = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.spawn_spot_facing(type_id, spot, tid, act));
            Step::Continue
        }
        pcd::CLASSIFYACTOR => {
            let tid = pop(state, idx);
            let act = state.scripts[idx].activator;
            state.scripts[idx].stack.push(host.classify_actor(tid, act));
            Step::Continue
        }
        pcd::SPAWNSPOTDIRECT => {
            let Some(type_id) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(spot) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(tid) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(angle) = take_i32(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let act = state.scripts[idx].activator;
            state.scripts[idx]
                .stack
                .push(host.spawn_spot(type_id, spot, tid, angle, act));
            Step::Continue
        }
        pcd::SETRESULTVALUE => {
            state.result_value = pop(state, idx);
            Step::Continue
        }
        pcd::PUSHFUNCTION => {
            let Some(funcnum) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            // Tag like a string id so CALLSTACK can clear the high bits.
            state.scripts[idx]
                .stack
                .push(DYN_STRING_TAG | (funcnum & 0xffff));
            Step::Continue
        }
        pcd::CALL | pcd::CALLDISCARD | pcd::CALLSTACK => {
            let discard = op == pcd::CALLDISCARD;
            let funcnum: u32 = if op == pcd::CALLSTACK {
                let tagged = pop(state, idx);
                (tagged & 0xffff) as u32
            } else {
                let Some(n) = take_byte_or_word(state, idx) else {
                    state.scripts[idx].state = ScriptRunState::Terminated;
                    return Step::Break;
                };
                n as u32
            };
            let Some(func) = state
                .modules
                .first()
                .and_then(|m| m.functions.get(funcnum as usize).cloned())
            else {
                host.print(&format!("Function {funcnum} out of range"), false);
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            if func.import_num != 0 || func.address == 0 {
                // Cross-library resolve deferred; CALLSTACK always pushes 0,
                // CALL pushes 0 when the result is kept.
                if !discard || op == pcd::CALLSTACK {
                    state.scripts[idx].stack.push(0);
                }
                return Step::Continue;
            }
            let argc = usize::from(func.argc);
            let local_extra = usize::from(func.local_count);
            let mut new_locals = vec![0i32; argc + local_extra];
            for i in (0..argc).rev() {
                new_locals[i] = pop(state, idx);
            }
            let old_locals = std::mem::replace(&mut state.scripts[idx].locals, new_locals);
            let return_pc = state.scripts[idx].pc;
            state.scripts[idx].frames.push(CallFrame {
                return_pc,
                locals: old_locals,
                discard_result: discard,
            });
            state.scripts[idx].pc = func.address;
            Step::Continue
        }
        pcd::RETURNVOID | pcd::RETURNVAL => {
            let value = if op == pcd::RETURNVAL {
                pop(state, idx)
            } else {
                0
            };
            let Some(frame) = state.scripts[idx].frames.pop() else {
                // Return from top-level script ≈ terminate.
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            state.scripts[idx].locals = frame.locals;
            state.scripts[idx].pc = frame.return_pc;
            if !frame.discard_result {
                state.scripts[idx].stack.push(value);
            }
            Step::Continue
        }
        pcd::PUSHMAPARRAY => {
            let Some(mapvar) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let index = pop(state, idx);
            let arr_idx = state.map_vars.get(mapvar as usize).copied().unwrap_or(0);
            let v = state
                .map_arrays
                .get(arr_idx as usize)
                .and_then(|a| a.get(index as usize))
                .copied()
                .unwrap_or(0);
            state.scripts[idx].stack.push(v);
            Step::Continue
        }
        pcd::ASSIGNMAPARRAY => {
            let Some(mapvar) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let value = pop(state, idx);
            let index = pop(state, idx);
            let arr_idx = state.map_vars.get(mapvar as usize).copied().unwrap_or(0);
            if let Some(slot) = state
                .map_arrays
                .get_mut(arr_idx as usize)
                .and_then(|a| a.get_mut(index as usize))
            {
                *slot = value;
            }
            Step::Continue
        }
        pcd::ADDMAPARRAY
        | pcd::SUBMAPARRAY
        | pcd::MULMAPARRAY
        | pcd::DIVMAPARRAY
        | pcd::MODMAPARRAY => {
            let Some(mapvar) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let value = pop(state, idx);
            let index = pop(state, idx);
            let arr_idx = state.map_vars.get(mapvar as usize).copied().unwrap_or(0);
            if let Some(slot) = state
                .map_arrays
                .get_mut(arr_idx as usize)
                .and_then(|a| a.get_mut(index as usize))
            {
                *slot = match op {
                    pcd::ADDMAPARRAY => slot.wrapping_add(value),
                    pcd::SUBMAPARRAY => slot.wrapping_sub(value),
                    pcd::MULMAPARRAY => slot.wrapping_mul(value),
                    pcd::DIVMAPARRAY => {
                        if value == 0 {
                            host.print("Division by zero", false);
                            state.scripts[idx].state = ScriptRunState::Terminated;
                            return Step::Break;
                        }
                        *slot / value
                    }
                    pcd::MODMAPARRAY => {
                        if value == 0 {
                            host.print("Division by zero", false);
                            state.scripts[idx].state = ScriptRunState::Terminated;
                            return Step::Break;
                        }
                        *slot % value
                    }
                    _ => *slot,
                };
            }
            Step::Continue
        }
        pcd::INCMAPARRAY | pcd::DECMAPARRAY => {
            let Some(mapvar) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let index = pop(state, idx);
            let arr_idx = state.map_vars.get(mapvar as usize).copied().unwrap_or(0);
            if let Some(slot) = state
                .map_arrays
                .get_mut(arr_idx as usize)
                .and_then(|a| a.get_mut(index as usize))
            {
                if op == pcd::INCMAPARRAY {
                    *slot = slot.wrapping_add(1);
                } else {
                    *slot = slot.wrapping_sub(1);
                }
            }
            Step::Continue
        }
        pcd::CALLFUNC => {
            // NEXTBYTE argc, NEXTSHORT index; pop argc args; push retval.
            let Some(argc) = take_byte_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let Some(func_index) = take_short_or_word(state, idx) else {
                state.scripts[idx].state = ScriptRunState::Terminated;
                return Step::Break;
            };
            let argc = argc.max(0) as usize;
            let mut args = vec![0i32; argc];
            for slot in args.iter_mut().rev() {
                *slot = pop(state, idx);
            }
            let act = state.scripts[idx].activator;
            if func_index == pcd::acsf::SET_ACTIVATOR {
                let tid = args.first().copied().unwrap_or(0);
                let new_act = host.set_activator(tid, act);
                if new_act != 0 {
                    state.scripts[idx].activator = new_act;
                }
                state.scripts[idx].stack.push(i32::from(new_act != 0));
                return Step::Continue;
            }
            if func_index == pcd::acsf::SET_ACTIVATOR_TO_TARGET {
                let tid = args.first().copied().unwrap_or(0);
                let new_act = host.set_activator_to_target(tid, act);
                if new_act != 0 {
                    state.scripts[idx].activator = new_act;
                }
                state.scripts[idx].stack.push(i32::from(new_act != 0));
                return Step::Continue;
            }
            if func_index == pcd::acsf::SET_ACTIVATOR_BY_NET_ID {
                let net_id = args.first().copied().unwrap_or(0);
                let aaptr = args.get(1).copied().unwrap_or(0);
                let new_act = host.set_activator_by_net_id(net_id, aaptr);
                if new_act != 0 {
                    state.scripts[idx].activator = new_act;
                }
                state.scripts[idx].stack.push(i32::from(new_act != 0));
                return Step::Continue;
            }
            if is_named_script_acs(func_index) {
                let retval = dispatch_named_script_acs(state, host, func_index, &args, act);
                state.scripts[idx].stack.push(retval);
                return Step::Continue;
            }
            let retval = dispatch_acs_func(state, host, func_index, &args, act);
            state.scripts[idx].stack.push(retval);
            Step::Continue
        }
        // Unimplemented PCD — terminate safely rather than runaway.
        _ => {
            state.scripts[idx].state = ScriptRunState::Terminated;
            Step::Break
        }
    }
}

fn pop(state: &mut AcsState, idx: usize) -> i32 {
    state.scripts[idx].stack.pop().unwrap_or(0)
}

fn module_string(state: &AcsState, index: usize) -> String {
    state
        .modules
        .first()
        .map(|m| m.string(index).to_owned())
        .unwrap_or_default()
}

fn binop(state: &mut AcsState, idx: usize, f: impl FnOnce(i32, i32) -> i32) -> Step {
    let b = pop(state, idx);
    let a = pop(state, idx);
    state.scripts[idx].stack.push(f(a, b));
    Step::Continue
}

fn get_local(state: &AcsState, idx: usize, i: usize) -> i32 {
    state.scripts[idx].locals.get(i).copied().unwrap_or(0)
}

fn set_local(state: &mut AcsState, idx: usize, i: usize, v: i32) {
    if i >= state.scripts[idx].locals.len() {
        state.scripts[idx].locals.resize(i + 1, 0);
    }
    state.scripts[idx].locals[i] = v;
}

fn var_op(state: &mut AcsState, idx: usize, f: impl FnOnce(i32, i32) -> i32) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    let cur = get_local(state, idx, i as usize);
    set_local(state, idx, i as usize, f(cur, v));
    Step::Continue
}

fn map_var_op(state: &mut AcsState, idx: usize, f: impl FnOnce(i32, i32) -> i32) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    if let Some(slot) = state.map_vars.get_mut(i as usize) {
        *slot = f(*slot, v);
    }
    Step::Continue
}

fn world_var_op(state: &mut AcsState, idx: usize, f: impl FnOnce(i32, i32) -> i32) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    if let Some(slot) = state.world_vars.get_mut(i as usize) {
        *slot = f(*slot, v);
    }
    Step::Continue
}

fn var_div(state: &mut AcsState, host: &mut dyn AcsHost, idx: usize, _script: bool) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    if v == 0 {
        host.print("Division by zero", false);
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    }
    let cur = get_local(state, idx, i as usize);
    set_local(state, idx, i as usize, cur / v);
    Step::Continue
}

fn map_div(state: &mut AcsState, host: &mut dyn AcsHost, idx: usize) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    if v == 0 {
        host.print("Division by zero", false);
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    }
    if let Some(slot) = state.map_vars.get_mut(i as usize) {
        *slot /= v;
    }
    Step::Continue
}

fn world_div(state: &mut AcsState, host: &mut dyn AcsHost, idx: usize) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    if v == 0 {
        host.print("Division by zero", false);
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    }
    if let Some(slot) = state.world_vars.get_mut(i as usize) {
        *slot /= v;
    }
    Step::Continue
}

fn var_mod(state: &mut AcsState, host: &mut dyn AcsHost, idx: usize) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    if v == 0 {
        host.print("Division by zero", false);
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    }
    let cur = get_local(state, idx, i as usize);
    set_local(state, idx, i as usize, cur % v);
    Step::Continue
}

fn map_mod(state: &mut AcsState, host: &mut dyn AcsHost, idx: usize) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    if v == 0 {
        host.print("Division by zero", false);
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    }
    if let Some(slot) = state.map_vars.get_mut(i as usize) {
        *slot %= v;
    }
    Step::Continue
}

fn world_mod(state: &mut AcsState, host: &mut dyn AcsHost, idx: usize) -> Step {
    let Some(i) = take_byte_or_word(state, idx) else {
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    };
    let v = pop(state, idx);
    if v == 0 {
        host.print("Division by zero", false);
        state.scripts[idx].state = ScriptRunState::Terminated;
        return Step::Break;
    }
    if let Some(slot) = state.world_vars.get_mut(i as usize) {
        *slot %= v;
    }
    Step::Continue
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::load::load_behavior;

    struct TestHost {
        specials: Vec<(i32, [i32; 5])>,
        prints: Vec<String>,
        time: i32,
        callfuncs: Vec<(i32, Vec<i32>)>,
        cleared_lines: Vec<i32>,
        line_blocking: Vec<(i32, i32)>,
        line_specials: Vec<(i32, i32, [i32; 5])>,
        sound_sequences: Vec<String>,
        species: String,
        nametag: String,
        damage_type: String,
        sound_class: String,
        see_sound: String,
    }

    impl TestHost {
        fn new() -> Self {
            Self {
                specials: Vec::new(),
                prints: Vec::new(),
                time: 0,
                callfuncs: Vec::new(),
                cleared_lines: Vec::new(),
                line_blocking: Vec::new(),
                line_specials: Vec::new(),
                sound_sequences: Vec::new(),
                species: String::new(),
                nametag: String::new(),
                damage_type: String::new(),
                sound_class: String::new(),
                see_sound: String::new(),
            }
        }
    }

    impl AcsHost for TestHost {
        fn execute_special(&mut self, special: i32, args: [i32; 5]) -> i32 {
            self.specials.push((special, args));
            1
        }
        fn random(&mut self, lo: i32, hi: i32) -> i32 {
            lo.saturating_add(hi.saturating_sub(lo) / 2)
        }
        fn timer(&self) -> i32 {
            self.time
        }
        fn player_count(&self) -> i32 {
            1
        }
        fn game_skill(&self) -> i32 {
            3
        }
        fn print(&mut self, text: &str, _bold: bool) {
            self.prints.push(text.to_owned());
        }
        fn tag_busy(&self, _tag: i32) -> bool {
            false
        }
        fn poly_busy(&self, _po: i32) -> bool {
            false
        }
        fn clear_line_special(&mut self, line: i32) {
            self.cleared_lines.push(line);
        }
        fn set_line_blocking(&mut self, line_id: i32, setting: i32) {
            self.line_blocking.push((line_id, setting));
        }
        fn set_line_special(&mut self, line_id: i32, special: i32, args: [i32; 5]) {
            self.line_specials.push((line_id, special, args));
        }
        fn sound_sequence(&mut self, name: &str) {
            self.sound_sequences.push(name.to_owned());
        }
        fn call_func(&mut self, func_index: i32, args: &[i32], _activator: u64) -> i32 {
            self.callfuncs.push((func_index, args.to_vec()));
            0
        }
        fn actor_species(&self, _tid: i32, _activator: u64) -> String {
            self.species.clone()
        }
        fn set_actor_species(&mut self, _tid: i32, species: &str, _activator: u64) {
            if species.is_empty() || species.eq_ignore_ascii_case("None") {
                self.species.clear();
            } else {
                self.species = species.to_owned();
            }
        }
        fn actor_nametag(&self, _tid: i32, _activator: u64) -> String {
            self.nametag.clone()
        }
        fn set_actor_nametag(&mut self, _tid: i32, tag: &str, _activator: u64) {
            self.nametag = tag.to_owned();
        }
        fn actor_damage_type(&self, _tid: i32, _activator: u64) -> String {
            self.damage_type.clone()
        }
        fn set_actor_damage_type(&mut self, _tid: i32, dtype: &str, _activator: u64) {
            if dtype.is_empty() || dtype.eq_ignore_ascii_case("None") {
                self.damage_type.clear();
            } else {
                self.damage_type = dtype.to_owned();
            }
        }
        fn actor_sound_class(&self, _tid: i32, _activator: u64) -> String {
            self.sound_class.clone()
        }
        fn set_actor_sound_class(&mut self, _tid: i32, class: &str, _activator: u64) {
            self.sound_class = class.to_owned();
        }
        fn set_actor_sound_prop(&mut self, _tid: i32, prop: i32, sound: &str, _activator: u64) {
            if prop == pcd::aprop::SEESOUND {
                self.see_sound = sound.to_owned();
            }
        }
    }

    fn module_with_code(code: &[u8], script_number: i32) -> AcsModule {
        let dir_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACS\0");
        out.extend_from_slice(&(dir_off as u32).to_le_bytes());
        out.extend_from_slice(code);
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&script_number.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        load_behavior(&out).expect("module")
    }

    #[test]
    fn delay_then_terminate() {
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::DELAYDIRECT.to_le_bytes());
        code.extend_from_slice(&2i32.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 1));
        let mut host = TestHost::new();
        assert_eq!(start_script(&mut state, &mut host, 1, &[0; 3], false), 1);
        assert_eq!(state.scripts.len(), 1);
        assert!(matches!(state.scripts[0].state, ScriptRunState::Delayed(2)));
        tick_scripts(&mut state, &mut host);
        assert!(matches!(state.scripts[0].state, ScriptRunState::Delayed(1)));
        tick_scripts(&mut state, &mut host);
        // delay done → run TERMINATE → swept
        assert!(state.scripts.is_empty());
    }

    #[test]
    fn lspec_direct_invokes_host() {
        // LSPEC1DIRECT special=80 arg=7 ; TERMINATE
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::LSPEC1DIRECT.to_le_bytes());
        code.extend_from_slice(&80i32.to_le_bytes());
        code.extend_from_slice(&7i32.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 2));
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 2, &[0; 3], false);
        assert_eq!(host.specials.len(), 1);
        assert_eq!(host.specials[0].0, 80);
        assert_eq!(host.specials[0].1[0], 7);
        assert!(matches!(state.scripts[0].state, ScriptRunState::Terminated));
        tick_scripts(&mut state, &mut host); // sweep
        assert!(state.scripts.is_empty());
    }

    #[test]
    fn acse_pushbyte_and_directb() {
        use crate::load::{AcsFormat, ScriptDef, ScriptType};
        // PUSHBYTE 7; DELAYDIRECTB 1; LSPEC1DIRECTB special=80 arg=3; TERMINATE
        let mut data = vec![0u8; 48];
        data[0..4].copy_from_slice(b"ACSe");
        data[4..8].copy_from_slice(&48u32.to_le_bytes());
        let mut pc = 8usize;
        data[pc] = pcd::PUSHBYTE as u8;
        pc += 1;
        data[pc] = 7;
        pc += 1;
        data[pc] = pcd::DELAYDIRECTB as u8;
        pc += 1;
        data[pc] = 1;
        pc += 1;
        data[pc] = pcd::LSPEC1DIRECTB as u8;
        pc += 1;
        data[pc] = 80;
        pc += 1;
        data[pc] = 3;
        pc += 1;
        data[pc] = pcd::TERMINATE as u8;
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: AcsFormat::AcseCompressed,
            scripts: vec![ScriptDef {
                number: 1,
                address: 8,
                argc: 0,
                script_type: ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: Vec::new(),
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 1, &[0; 3], false);
        assert!(matches!(state.scripts[0].state, ScriptRunState::Delayed(1)));
        assert_eq!(state.scripts[0].stack, vec![7]);
        tick_scripts(&mut state, &mut host);
        assert_eq!(host.specials.len(), 1);
        assert_eq!(host.specials[0].0, 80);
        assert_eq!(host.specials[0].1[0], 3);
        assert!(state.scripts.is_empty());
    }

    /// ACSe: 1-byte opcodes + 4-byte DELAYDIRECT arg.
    #[test]
    fn acse_compressed_delay_terminate() {
        use crate::load::{AcsFormat, ScriptDef, ScriptType};
        // bytes: DELAYDIRECT(56) as u8, i32=2, TERMINATE(1) as u8
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(b"ACSe");
        data[4..8].copy_from_slice(&32u32.to_le_bytes()); // chunks at EOF (empty)
        data[8] = 56; // DELAYDIRECT
        data[9..13].copy_from_slice(&2i32.to_le_bytes());
        data[13] = 1; // TERMINATE
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: AcsFormat::AcseCompressed,
            scripts: vec![ScriptDef {
                number: 1,
                address: 8,
                argc: 0,
                script_type: ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: Vec::new(),
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        assert_eq!(start_script(&mut state, &mut host, 1, &[0; 3], false), 1);
        assert!(matches!(state.scripts[0].state, ScriptRunState::Delayed(2)));
        tick_scripts(&mut state, &mut host);
        tick_scripts(&mut state, &mut host);
        assert!(state.scripts.is_empty());
    }

    #[test]
    fn map_var_assign_and_push() {
        // PUSHNUMBER 42; ASSIGNMAPVAR 0; PUSHMAPVAR 0; ASSIGNWORLDVAR 1; TERMINATE
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&42i32.to_le_bytes());
        code.extend_from_slice(&pcd::ASSIGNMAPVAR.to_le_bytes());
        code.extend_from_slice(&0u32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHMAPVAR.to_le_bytes());
        code.extend_from_slice(&0u32.to_le_bytes());
        code.extend_from_slice(&pcd::ASSIGNWORLDVAR.to_le_bytes());
        code.extend_from_slice(&1u32.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 3));
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 3, &[0; 3], false);
        assert_eq!(state.map_vars[0], 42);
        assert_eq!(state.world_vars[1], 42);
    }

    #[test]
    fn callfunc_sqrt_and_setresult() {
        // PUSHNUMBER 144; CALLFUNC argc=1 index=Sqrt; SETRESULTVALUE; TERMINATE
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&144i32.to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes()); // argc
        code.extend_from_slice(&pcd::acsf::SQRT.to_le_bytes()); // index as word
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 4));
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 4, &[0; 3], false);
        assert_eq!(state.result_value, 12);
        tick_scripts(&mut state, &mut host);
        assert!(state.scripts.is_empty());
    }

    #[test]
    fn callfunc_vector_length() {
        // 3.0 and 4.0 in 16.16 → length 5.0
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&(3 * room_core::FRACUNIT).to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&(4 * room_core::FRACUNIT).to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&2i32.to_le_bytes());
        code.extend_from_slice(&pcd::acsf::VECTOR_LENGTH.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 5));
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 5, &[0; 3], false);
        assert_eq!(state.result_value, 5 * room_core::FRACUNIT);
    }

    #[test]
    fn sin_cos_vector_angle() {
        // PUSHNUMBER 0; SIN; SETRESULTVALUE; TERMINATE — sin(0)=0
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::SIN.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 11));
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 11, &[0; 3], false);
        assert_eq!(state.result_value, 0);

        // 0.25 turns = 90° → cos ≈ 0
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&(room_core::FRACUNIT / 4).to_le_bytes());
        code.extend_from_slice(&pcd::COS.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 12));
        start_script(&mut state, &mut host, 12, &[0; 3], false);
        assert!(state.result_value.abs() < 64);

        // VECTORANGLE(1, 0) → 0 turns
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&room_core::FRACUNIT.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::VECTORANGLE.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 13));
        start_script(&mut state, &mut host, 13, &[0; 3], false);
        assert_eq!(state.result_value, 0);
    }

    #[test]
    fn fixedmul_and_getactor_x() {
        // PUSHNUMBER 2.0; PUSHNUMBER 3.0; FIXEDMUL; SETRESULTVALUE;
        // PUSHNUMBER 0; GETACTORX; DROP; TERMINATE
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&(2 * room_core::FRACUNIT).to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&(3 * room_core::FRACUNIT).to_le_bytes());
        code.extend_from_slice(&pcd::FIXEDMUL.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::GETACTORX.to_le_bytes());
        code.extend_from_slice(&pcd::DROP.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 7));
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 7, &[0; 3], false);
        assert_eq!(state.result_value, 6 * room_core::FRACUNIT);
    }

    #[test]
    fn callfunc_unknown_goes_to_host() {
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&7i32.to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        code.extend_from_slice(&19_999i32.to_le_bytes()); // unknown ACSF
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 6));
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 6, &[0; 3], false);
        assert_eq!(host.callfuncs, vec![(19_999, vec![7])]);
        assert_eq!(state.result_value, 0);
    }

    #[test]
    fn clear_line_special_notifies_host() {
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::CLEARLINESPECIAL.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 8));
        let mut host = TestHost::new();
        start_script_with_line(&mut state, &mut host, 8, &[0; 3], false, 0, 42, 0);
        assert_eq!(host.cleared_lines, vec![42]);
    }

    #[test]
    fn callfunc_floor_ceil_round() {
        let mut state = AcsState::new();
        let mut host = TestHost::new();
        // 1.5 → floor 1, ceil 2, round 2
        let one_half = 0x1_8000;
        assert_eq!(
            dispatch_acs_func(&mut state, &mut host, pcd::acsf::FLOOR, &[one_half], 0),
            1
        );
        assert_eq!(
            dispatch_acs_func(&mut state, &mut host, pcd::acsf::CEIL, &[one_half], 0),
            2
        );
        assert_eq!(
            dispatch_acs_func(&mut state, &mut host, pcd::acsf::ROUND, &[one_half], 0),
            2
        );
        // −1.25 → floor −2, ceil −1
        let neg = -0x1_4000;
        assert_eq!(
            dispatch_acs_func(&mut state, &mut host, pcd::acsf::FLOOR, &[neg], 0),
            -2
        );
        assert_eq!(
            dispatch_acs_func(&mut state, &mut host, pcd::acsf::CEIL, &[neg], 0),
            -1
        );
    }

    #[test]
    fn callfunc_strcmp_stricmp() {
        assert!(acs_strcmp("Ab", "ac", false, None) < 0);
        assert_eq!(acs_strcmp("Ab", "ab", true, None), 0);
        assert_eq!(acs_strcmp("ab", "ac", true, Some(1)), 0);
        assert!(acs_strcmp("ab", "ac", true, Some(2)) < 0);

        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&2i32.to_le_bytes());
        code.extend_from_slice(&pcd::acsf::STRICMP.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut data = vec![0u8; 16 + code.len()];
        data[8..11].copy_from_slice(b"Go\0");
        data[11..14].copy_from_slice(b"go\0");
        data[16..16 + code.len()].copy_from_slice(&code);
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: crate::load::AcsFormat::Acs0,
            scripts: vec![crate::load::ScriptDef {
                number: 1,
                address: 16,
                argc: 0,
                script_type: crate::load::ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: vec![8, 11],
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 1, &[0; 3], false);
        assert_eq!(state.result_value, 0);
    }

    #[test]
    fn get_set_actor_property_species_string() {
        // SetActorProperty(0, APROP_Species, "Pack"); GetActorProperty → dyn "Pack"
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // tid
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::SPECIES.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // "Pack"
        code.extend_from_slice(&pcd::SETACTORPROPERTY.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // tid
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::SPECIES.to_le_bytes());
        code.extend_from_slice(&pcd::GETACTORPROPERTY.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // "Pack"
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&2i32.to_le_bytes());
        code.extend_from_slice(&pcd::acsf::STRCMP.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let code_at = 16usize;
        let mut data = vec![0u8; code_at + code.len()];
        data[8..13].copy_from_slice(b"Pack\0");
        data[code_at..code_at + code.len()].copy_from_slice(&code);
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: crate::load::AcsFormat::Acs0,
            scripts: vec![crate::load::ScriptDef {
                number: 1,
                address: code_at as u32,
                argc: 0,
                script_type: crate::load::ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: vec![8],
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 1, &[0; 3], false);
        assert_eq!(host.species, "Pack");
        assert_eq!(state.dyn_strings, vec!["Pack".to_owned()]);
        assert_eq!(state.result_value, 0);
    }

    #[test]
    fn set_actor_property_seesound_and_soundclass() {
        let mut code = Vec::new();
        // Set SeeSound "chat"
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::SEESOUND.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // "chat"
        code.extend_from_slice(&pcd::SETACTORPROPERTY.to_le_bytes());
        // Set SoundClass "marine"
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::SOUNDCLASS.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes()); // "marine"
        code.extend_from_slice(&pcd::SETACTORPROPERTY.to_le_bytes());
        // Get SoundClass; strcmp "marine"
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::SOUNDCLASS.to_le_bytes());
        code.extend_from_slice(&pcd::GETACTORPROPERTY.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&2i32.to_le_bytes());
        code.extend_from_slice(&pcd::acsf::STRCMP.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let code_at = 24usize;
        let mut data = vec![0u8; code_at + code.len()];
        data[8..13].copy_from_slice(b"chat\0");
        data[14..21].copy_from_slice(b"marine\0");
        data[code_at..code_at + code.len()].copy_from_slice(&code);
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: crate::load::AcsFormat::Acs0,
            scripts: vec![crate::load::ScriptDef {
                number: 1,
                address: code_at as u32,
                argc: 0,
                script_type: crate::load::ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: vec![8, 14],
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 1, &[0; 3], false);
        assert_eq!(host.see_sound, "chat");
        assert_eq!(host.sound_class, "marine");
        assert_eq!(state.result_value, 0);
    }

    #[test]
    fn get_set_actor_property_damagetype_string() {
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::DAMAGETYPE.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // "Fire"
        code.extend_from_slice(&pcd::SETACTORPROPERTY.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::DAMAGETYPE.to_le_bytes());
        code.extend_from_slice(&pcd::GETACTORPROPERTY.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&2i32.to_le_bytes());
        code.extend_from_slice(&pcd::acsf::STRCMP.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let code_at = 16usize;
        let mut data = vec![0u8; code_at + code.len()];
        data[8..13].copy_from_slice(b"Fire\0");
        data[code_at..code_at + code.len()].copy_from_slice(&code);
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: crate::load::AcsFormat::Acs0,
            scripts: vec![crate::load::ScriptDef {
                number: 1,
                address: code_at as u32,
                argc: 0,
                script_type: crate::load::ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: vec![8],
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 1, &[0; 3], false);
        assert_eq!(host.damage_type, "Fire");
        assert_eq!(state.dyn_strings, vec!["Fire".to_owned()]);
        assert_eq!(state.result_value, 0);
    }

    #[test]
    fn get_set_actor_property_nametag_string() {
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::NAMETAG.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // "Hero"
        code.extend_from_slice(&pcd::SETACTORPROPERTY.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&pcd::aprop::NAMETAG.to_le_bytes());
        code.extend_from_slice(&pcd::GETACTORPROPERTY.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&2i32.to_le_bytes());
        code.extend_from_slice(&pcd::acsf::STRCMP.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let code_at = 16usize;
        let mut data = vec![0u8; code_at + code.len()];
        data[8..13].copy_from_slice(b"Hero\0");
        data[code_at..code_at + code.len()].copy_from_slice(&code);
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: crate::load::AcsFormat::Acs0,
            scripts: vec![crate::load::ScriptDef {
                number: 1,
                address: code_at as u32,
                argc: 0,
                script_type: crate::load::ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: vec![8],
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 1, &[0; 3], false);
        assert_eq!(host.nametag, "Hero");
        assert_eq!(state.dyn_strings, vec!["Hero".to_owned()]);
        assert_eq!(state.result_value, 0);
    }

    #[test]
    fn callfunc_str_mid_dyn_string() {
        // StrMid("Hello", 1, 3) → "ell"; strcmp with module "ell" → 0.
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // "Hello"
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&3i32.to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&3i32.to_le_bytes());
        code.extend_from_slice(&pcd::acsf::STR_MID.to_le_bytes());
        // stack: dyn "ell"; push module str 1 "ell"; strcmp
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        code.extend_from_slice(&pcd::CALLFUNC.to_le_bytes());
        code.extend_from_slice(&2i32.to_le_bytes());
        code.extend_from_slice(&pcd::acsf::STRCMP.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let code_at = 32usize;
        let mut data = vec![0u8; code_at + code.len()];
        data[8..14].copy_from_slice(b"Hello\0");
        data[20..24].copy_from_slice(b"ell\0");
        data[code_at..code_at + code.len()].copy_from_slice(&code);
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: crate::load::AcsFormat::Acs0,
            scripts: vec![crate::load::ScriptDef {
                number: 1,
                address: code_at as u32,
                argc: 0,
                script_type: crate::load::ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: vec![8, 20],
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 1, &[0; 3], false);
        assert_eq!(state.dyn_strings, vec!["ell".to_owned()]);
        assert_eq!(state.result_value, 0);
    }

    #[test]
    fn savestring_interns_print_buffer() {
        use crate::load::{AcsFormat, ScriptDef, ScriptType};
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::BEGINPRINT.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes()); // "Hi"
        code.extend_from_slice(&pcd::PRINTSTRING.to_le_bytes());
        code.extend_from_slice(&pcd::SAVESTRING.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let code_at = 16usize;
        let mut data = vec![0u8; code_at + code.len()];
        data[8..11].copy_from_slice(b"Hi\0");
        data[code_at..code_at + code.len()].copy_from_slice(&code);
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: AcsFormat::Acs0,
            scripts: vec![ScriptDef {
                number: 1,
                address: code_at as u32,
                argc: 0,
                script_type: ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: vec![8],
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        start_script(&mut state, &mut host, 1, &[0; 3], false);
        assert_eq!(state.dyn_strings, vec!["Hi".to_owned()]);
        assert_eq!(state.result_value, DYN_STRING_TAG);
        assert!(state.scripts[0].print_buf.is_empty());
    }

    #[test]
    fn named_execute_via_callfunc() {
        use crate::load::{AcsFormat, ScriptDef, ScriptType};
        // data: "Go\0" at 4; named script (−1) at 16; caller (1) at 40.
        let mut data = vec![0u8; 128];
        data[4..7].copy_from_slice(b"Go\0");

        // Script −1: LSPEC1DIRECT special=80 arg=9; TERMINATE
        let mut pc = 16usize;
        data[pc..pc + 4].copy_from_slice(&pcd::LSPEC1DIRECT.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&80i32.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&9i32.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::TERMINATE.to_le_bytes());

        // Script 1: push str=0, map=0, a1..a3=0; CALLFUNC NamedExecute; TERMINATE
        pc = 40;
        for v in [0i32, 0, 0, 0, 0] {
            data[pc..pc + 4].copy_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
            pc += 4;
            data[pc..pc + 4].copy_from_slice(&v.to_le_bytes());
            pc += 4;
        }
        data[pc..pc + 4].copy_from_slice(&pcd::CALLFUNC.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&5i32.to_le_bytes()); // argc
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::acsf::ACS_NAMED_EXECUTE.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::TERMINATE.to_le_bytes());

        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: AcsFormat::Acse,
            scripts: vec![
                ScriptDef {
                    number: -1,
                    address: 16,
                    argc: 0,
                    script_type: ScriptType::Closed,
                    flags: 0,
                    local_count: 0,
                },
                ScriptDef {
                    number: 1,
                    address: 40,
                    argc: 0,
                    script_type: ScriptType::Closed,
                    flags: 0,
                    local_count: 0,
                },
            ],
            string_offsets: vec![4],
            libraries: Vec::new(),
            script_names: vec!["Go".into()],
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
        });
        let mut host = TestHost::new();
        assert_eq!(start_script(&mut state, &mut host, 1, &[0; 3], false), 1);
        assert_eq!(host.specials.len(), 1);
        assert_eq!(host.specials[0].0, 80);
        assert_eq!(host.specials[0].1[0], 9);
    }

    #[test]
    fn casegoto_matches_and_jumps() {
        // PUSHNUMBER 2; CASEGOTO 1 (miss); CASEGOTO 2 → ok; fail path; ok path.
        let start = 8u32;
        let fail_len = 4 + 4 + 4 + 4; // PUSHNUMBER 0; SETRESULTVALUE; TERMINATE
        let mut head = Vec::new();
        head.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        head.extend_from_slice(&2i32.to_le_bytes());
        head.extend_from_slice(&pcd::CASEGOTO.to_le_bytes());
        head.extend_from_slice(&1i32.to_le_bytes());
        head.extend_from_slice(&0u32.to_le_bytes());
        head.extend_from_slice(&pcd::CASEGOTO.to_le_bytes());
        head.extend_from_slice(&2i32.to_le_bytes());
        let ok_addr = start + (head.len() + 4 + fail_len) as u32;
        head.extend_from_slice(&ok_addr.to_le_bytes());
        let mut code = head;
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        assert_eq!(start + code.len() as u32, ok_addr);
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        code.extend_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 1));
        let mut host = TestHost::new();
        assert_eq!(start_script(&mut state, &mut host, 1, &[0; 3], false), 1);
        assert_eq!(state.result_value, 1);
    }

    #[test]
    fn set_line_blocking_and_special_pcods() {
        // PUSHNUMBER line=3; PUSHNUMBER BLOCK_NOTHING=0; SETLINEBLOCKING;
        // PUSHNUMBER 3; PUSHNUMBER 80; five arg 0s; SETLINESPECIAL; TERMINATE
        let mut code = Vec::new();
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&3i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&pcd::SETLINEBLOCKING.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&3i32.to_le_bytes());
        code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        code.extend_from_slice(&80i32.to_le_bytes());
        for _ in 0..5 {
            code.extend_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
            code.extend_from_slice(&0i32.to_le_bytes());
        }
        code.extend_from_slice(&pcd::SETLINESPECIAL.to_le_bytes());
        code.extend_from_slice(&pcd::TERMINATE.to_le_bytes());
        let mut state = AcsState::new();
        state.push_module(module_with_code(&code, 1));
        let mut host = TestHost::new();
        assert_eq!(start_script(&mut state, &mut host, 1, &[0; 3], false), 1);
        assert_eq!(host.line_blocking, vec![(3, 0)]);
        assert_eq!(host.line_specials.len(), 1);
        assert_eq!(host.line_specials[0].0, 3);
        assert_eq!(host.line_specials[0].1, 80);
    }

    #[test]
    fn call_return_and_map_array() {
        // Script @8: PUSHNUMBER 5; CALL 0; SETRESULTVALUE; TERMINATE
        // Func 0 @40: PUSHSCRIPTVAR 0; PUSHNUMBER 1; ADD; RETURNVAL
        // Also: map array[0]=9 via PUSHMAPARRAY after MINI/ARAY applied by push_module.
        use crate::load::{AcsFormat, AcsFunction, AcsMapArray, ScriptDef, ScriptType};

        let mut data = vec![0u8; 80];
        data[0..4].copy_from_slice(b"ACSE");
        data[4..8].copy_from_slice(&80u32.to_le_bytes()); // chunks at EOF (empty)
        // script body at 8
        let mut pc = 8usize;
        data[pc..pc + 4].copy_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&5i32.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::CALL.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&0i32.to_le_bytes()); // func 0
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::SETRESULTVALUE.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::TERMINATE.to_le_bytes());
        // function body at 40
        let func_pc = 40usize;
        pc = func_pc;
        data[pc..pc + 4].copy_from_slice(&pcd::PUSHSCRIPTVAR.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&0i32.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::PUSHNUMBER.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&1i32.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::ADD.to_le_bytes());
        pc += 4;
        data[pc..pc + 4].copy_from_slice(&pcd::RETURNVAL.to_le_bytes());

        let mut map_var_defaults = [0i32; 128];
        map_var_defaults[0] = 0; // array slot
        let mut state = AcsState::new();
        state.push_module(crate::load::AcsModule {
            data,
            format: AcsFormat::Acse,
            scripts: vec![ScriptDef {
                number: 1,
                address: 8,
                argc: 0,
                script_type: ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: Vec::new(),
            libraries: Vec::new(),
            script_names: Vec::new(),
            functions: vec![AcsFunction {
                argc: 1,
                local_count: 0,
                has_return: 1,
                import_num: 0,
                address: func_pc as u32,
            }],
            function_names: vec!["inc".into()],
            map_var_defaults,
            map_arrays: vec![AcsMapArray {
                map_var: 0,
                elements: vec![9, 0, 0],
            }],
            map_exports: Vec::new(),
        });
        assert_eq!(state.map_arrays[0][0], 9);
        let mut host = TestHost::new();
        assert_eq!(start_script(&mut state, &mut host, 1, &[0; 3], false), 1);
        assert_eq!(state.result_value, 6); // 5+1
    }

    /// BD5: load commercial `acs/BD_Main.o` and run OPEN scripts for a few ticks.
    #[test]
    fn bd_main_open_scripts_gated() {
        let Ok(pk3) = std::env::var("ROOM_BRUTALDOOM_PK3") else {
            return;
        };
        if !std::path::Path::new(&pk3).is_file() {
            return;
        }
        let out = std::process::Command::new("unzip")
            .args(["-p", &pk3, "acs/BD_Main.o"])
            .output()
            .expect("unzip BD_Main.o");
        assert!(
            out.status.success() && !out.stdout.is_empty(),
            "failed to extract acs/BD_Main.o"
        );
        let module = load_behavior(&out.stdout).expect("load BD_Main");
        let open_n = module
            .scripts
            .iter()
            .filter(|s| s.script_type == ScriptType::Open)
            .count();
        assert!(open_n > 0, "BD_Main should declare OPEN scripts");
        let mut state = AcsState::default();
        state.push_module(module);
        let mut host = TestHost::new();
        start_open_scripts(&mut state, &mut host);
        for _ in 0..35 {
            tick_scripts(&mut state, &mut host);
            host.time = host.time.saturating_add(1);
        }
        eprintln!(
            "BD_Main OPEN smoke: open_scripts={open_n} active={} prints={} callfuncs={}",
            state.scripts.len(),
            host.prints.len(),
            host.callfuncs.len()
        );
    }
}

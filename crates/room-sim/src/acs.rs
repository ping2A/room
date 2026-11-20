//! ACS specials and host bridge (`ACS_Execute*` family).

use crate::actor::ActorId;
use crate::world::World;
use room_acs::{
    AcsHost, AcsState, acsf, aprop, start_script_with_line, suspend_script, terminate_script,
};
use room_core::{Angle, DVec3, Fixed};
use room_nodebuild::{point_in_sector, point_in_sector_vanilla};

/// Room `Buttons` bits → GZDoom `BT_*` (`GetPlayerInput` / `BT_RELOAD` = 1<<6).
pub(crate) fn gzdoom_button_bits(room: u32) -> i32 {
    use crate::player::Buttons as B;
    let mut g = 0i32;
    let map = [
        (B::ATTACK, 1 << 0),
        (B::USE, 1 << 1),
        (B::JUMP, 1 << 2),
        (B::ALTATTACK, 1 << 5),
        (B::RELOAD, 1 << 6),
        (B::ZOOM, 1 << 7),
        (B::SPEED, 1 << 8),
        (B::STRAFE, 1 << 9),
        (B::USER1, 1 << 21),
        (B::USER2, 1 << 22),
        (B::USER3, 1 << 23),
        (B::USER4, 1 << 24),
    ];
    for (bit, gz) in map {
        if room & bit != 0 {
            g |= gz;
        }
    }
    g
}

/// Log an ACS opcode / ACSF index with no implementation, once per process.
pub(crate) fn report_unsupported_acs(kind: &str, index: i32) {
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<std::collections::BTreeSet<(String, i32)>>> = OnceLock::new();
    if cfg!(test) {
        return;
    }
    let seen = SEEN.get_or_init(|| Mutex::new(std::collections::BTreeSet::new()));
    if let Ok(mut s) = seen.lock() {
        if s.insert((kind.to_owned(), index)) {
            eprintln!("room: unsupported {kind} {index} (returns 0)");
        }
    }
}

thread_local! {
    /// String result of the WASM `ScriptCall` that just returned.
    static PENDING_CALL_STRING: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// World as ACS host (line specials, RNG, queries).
pub(crate) struct WorldAcsHost<'a> {
    pub world: &'a mut World,
}

impl AcsHost for WorldAcsHost<'_> {
    fn execute_special(&mut self, special: i32, args: [i32; 5], activator: u64) -> i32 {
        if special <= 0 {
            return 0;
        }
        crate::specials::execute_special(
            self.world,
            special as u16,
            &args,
            unpack_actor(activator),
            None,
        )
    }

    fn random(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        let span = (hi - lo + 1) as u32;
        let r = u32::from(self.world.p_random("acs"));
        lo.wrapping_add((r % span) as i32)
    }

    fn timer(&self) -> i32 {
        self.world.level.maptime as i32
    }

    fn player_count(&self) -> i32 {
        self.world.players.iter().filter(|p| p.is_some()).count() as i32
    }

    fn get_player_input(&self, player: i32, input: i32) -> i32 {
        // `-1` is the script activator / console player, not "nobody".
        let seat = if player < 0 { 0 } else { player as usize };
        let Some(p) = self.world.players.get(seat).and_then(|s| s.as_ref()) else {
            return 0;
        };
        let mut bits = match input {
            // GZDoom `EInputFlags`: OLDBUTTONS, BUTTONS, then axes.
            0 | 8 => gzdoom_button_bits(p.acs_prev_buttons),
            1 | 9 => gzdoom_button_bits(p.cmd.buttons.0),
            5 | 13 => i32::from(p.cmd.forward_move),
            6 | 14 => i32::from(p.cmd.side_move),
            7 | 15 => i32::from(p.cmd.up_move),
            _ => 0,
        };
        if p.acs_use_latched && matches!(input, 0 | 1 | 8 | 9) {
            // The same edge mounts the vehicle. Hide it from vehicle-control
            // ACS until release, otherwise BDGeneralInput treats it as exit.
            bits &= !(1 << 1); // GZDoom BT_USE
        }
        bits
    }

    fn console_player_number(&self) -> i32 {
        // Local seat 0 offline; dedicated would return −1.
        if self.world.players[0].is_some() {
            0
        } else {
            -1
        }
    }

    fn set_mugshot_state(&mut self, state: &str) {
        let state = state.trim();
        if state.is_empty() {
            return;
        }
        // Bootstrap: force evil-grin priority face via existing mugshot fields.
        if let Some(p) = self.world.players[0].as_mut() {
            let _ = state;
            p.mugshot = crate::mugshot::MugShot::default();
        }
    }

    fn game_skill(&self) -> i32 {
        i32::from(self.world.skill)
    }

    fn sector_sound(&mut self, name: &str, volume: i32, activator: u64) {
        if name.is_empty() {
            return;
        }
        let vol = acs_volume(volume);
        if let Some(id) = unpack_actor(activator) {
            if let Some(a) = self.world.actors.get(id) {
                self.world
                    .play_sound(name, a.pos.x(), a.pos.y(), a.pos.z(), vol);
                return;
            }
        }
        self.world.play_sound_attn(name, 0.0, 0.0, 0.0, vol, 0.0);
    }

    fn ambient_sound(&mut self, name: &str, volume: i32) {
        if name.is_empty() {
            return;
        }
        self.world
            .play_sound_attn(name, 0.0, 0.0, 0.0, acs_volume(volume), 0.0);
    }

    fn activator_sound(&mut self, name: &str, volume: i32, activator: u64) {
        if name.is_empty() {
            return;
        }
        let vol = acs_volume(volume);
        if let Some(id) = unpack_actor(activator) {
            if let Some(a) = self.world.actors.get(id) {
                self.world
                    .play_sound(name, a.pos.x(), a.pos.y(), a.pos.z(), vol);
                return;
            }
        }
        self.world.play_sound_attn(name, 0.0, 0.0, 0.0, vol, 0.0);
    }

    fn thing_sound(&mut self, tid: i32, name: &str, volume: i32, activator: u64) {
        if name.is_empty() {
            return;
        }
        let vol = acs_volume(volume);
        for id in collect_tid_ids(self.world, tid, activator) {
            if let Some(a) = self.world.actors.get(id) {
                self.world
                    .play_sound(name, a.pos.x(), a.pos.y(), a.pos.z(), vol);
            }
        }
    }

    fn play_sound_acs(
        &mut self,
        tid: i32,
        name: &str,
        volume: i32,
        attenuation: i32,
        activator: u64,
    ) {
        if name.is_empty() {
            return;
        }
        let vol = Fixed(volume).to_f64().clamp(0.0, 1.0) as f32;
        let atten = Fixed(attenuation).to_f64().max(0.0) as f32;
        for id in collect_tid_ids(self.world, tid, activator) {
            if let Some(a) = self.world.actors.get(id) {
                self.world
                    .play_sound_attn(name, a.pos.x(), a.pos.y(), a.pos.z(), vol, atten);
            }
        }
    }

    fn stop_sound_acs(&mut self, tid: i32, _channel: i32, activator: u64) {
        // Channel tracking still open — drop queued events near matching actors.
        for id in collect_tid_ids(self.world, tid, activator) {
            let Some(a) = self.world.actors.get(id) else {
                continue;
            };
            let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
            self.world.sound_events.retain(|ev| {
                let dx = ev.x - x;
                let dy = ev.y - y;
                let dz = ev.z - z;
                dx * dx + dy * dy + dz * dz >= 1.0
            });
        }
    }

    fn sound_volume(&mut self, tid: i32, _channel: i32, volume: i32, activator: u64) {
        // Channel mixer still open — rescale queued events from this actor.
        let vol = Fixed(volume).to_f64().clamp(0.0, 1.0) as f32;
        for id in collect_tid_ids(self.world, tid, activator) {
            let Some(a) = self.world.actors.get(id) else {
                continue;
            };
            let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
            for ev in &mut self.world.sound_events {
                let dx = ev.x - x;
                let dy = ev.y - y;
                let dz = ev.z - z;
                if dx * dx + dy * dy + dz * dz < 1.0 {
                    ev.volume = vol;
                }
            }
        }
    }

    fn get_actor_powerup_tics(&self, tid: i32, powerup: &str, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(p) = self
            .world
            .players
            .iter()
            .flatten()
            .find(|pl| pl.mo == Some(id))
        else {
            return 0;
        };
        let key = powerup.trim();
        if key.is_empty() {
            return 0;
        }
        let k = key.to_ascii_lowercase();
        if k.contains("invul") {
            return p.invul_tics;
        }
        if k.contains("invis") || k.contains("blur") {
            return p.blur_tics;
        }
        if k.contains("iron") || k.contains("rad") {
            return p.radsuit_tics;
        }
        if k.contains("light") || k.contains("infra") || k.contains("torch") {
            return p.infrared_tics;
        }
        if k.contains("strength") || k.contains("berserk") {
            return i32::from(p.berserk);
        }
        0
    }

    fn get_air_supply(&self, player: i32) -> i32 {
        if player < 0 {
            return 0;
        }
        let Some(p) = self
            .world
            .players
            .get(player as usize)
            .and_then(|s| s.as_ref())
        else {
            return 0;
        };
        (p.air_finished - self.world.level.maptime as i32).max(0)
    }

    fn set_air_supply(&mut self, player: i32, tics: i32) -> i32 {
        if player < 0 {
            return 0;
        }
        let Some(slot) = self.world.players.get_mut(player as usize) else {
            return 0;
        };
        let Some(p) = slot.as_mut() else {
            return 0;
        };
        p.air_finished = tics.saturating_add(self.world.level.maptime as i32);
        1
    }

    fn set_gravity(&mut self, amount: i32) {
        self.world.level.gravity = Fixed(amount).to_f64();
    }

    fn set_air_control(&mut self, amount: i32) {
        self.world.level.air_control = Fixed(amount).to_f64();
    }

    fn player_in_game(&self, player: i32) -> i32 {
        if player < 0 {
            return 0;
        }
        i32::from(
            self.world
                .players
                .get(player as usize)
                .and_then(|s| s.as_ref())
                .is_some(),
        )
    }

    fn get_ammo_capacity(&self, class_name: &str, activator: u64) -> i32 {
        let Some(class) = self.world.classes.find_name(class_name) else {
            return 0;
        };
        if let Some(id) = unpack_actor(activator) {
            if let Some(item) = self.world.inventory.find(id, class) {
                return item.max_amount;
            }
        }
        self.world
            .classes
            .get(class)
            .map(|c| c.inventory_max_amount)
            .unwrap_or(0)
    }

    fn set_ammo_capacity(&mut self, class_name: &str, amount: i32, activator: u64) {
        let Some(class) = self.world.classes.find_name(class_name) else {
            return;
        };
        let Some(id) = unpack_actor(activator) else {
            return;
        };
        self.world
            .inventory
            .set_max_amount(id, class, amount.max(1));
    }

    fn set_actor_state(&mut self, tid: i32, state: &str, _exact: bool, activator: u64) -> i32 {
        if state.is_empty() {
            return 0;
        }
        // Labels may be "Spawn", "See", "Missile.Flash", etc. — take last segment.
        let label = state.rsplit('.').next().unwrap_or(state);
        let mut n = 0i32;
        for id in collect_tid_ids(self.world, tid, activator) {
            let Some(class) = self.world.actors.get(id).map(|a| a.class) else {
                continue;
            };
            let Some(sid) = self.world.states.find_label(class, label) else {
                continue;
            };
            self.world.set_state(id, Some(sid), true);
            n += 1;
        }
        i32::from(n > 0)
    }

    fn thing_damage2(&mut self, tid: i32, amount: i32, _mod: &str, activator: u64) -> i32 {
        let mut n = 0i32;
        for id in collect_tid_ids(self.world, tid, activator) {
            let dealt =
                crate::damage::damage_mobj(self.world, id, None, unpack_actor(activator), amount);
            if dealt != 0 {
                n += 1;
            }
        }
        n
    }

    fn use_actor_inventory(&mut self, tid: i32, item: &str, activator: u64) -> i32 {
        if item.is_empty() {
            return 0;
        }
        let Some(class) = self.world.classes.find_name(item) else {
            return 0;
        };
        let mut n = 0i32;
        for id in collect_tid_ids(self.world, tid, activator) {
            if self.world.inventory.take(id, class, 1) > 0 {
                n += 1;
            }
        }
        n
    }

    fn spawn_decal_acs(&mut self, tid: i32, name: &str, activator: u64) -> i32 {
        if name.is_empty() {
            return 0;
        }
        let mut n = 0i32;
        for id in collect_tid_ids(self.world, tid, activator) {
            let Some(a) = self.world.actors.get(id) else {
                continue;
            };
            self.world
                .spawn_decal(name, a.pos.x(), a.pos.y(), a.pos.z(), None);
            n += 1;
        }
        n
    }

    fn change_level(&mut self, map: &str, _pos: i32, _flags: i32, _skill: i32) {
        let map = map.trim();
        if map.is_empty() {
            return;
        }
        self.world.level.next_map_override = Some(map.to_ascii_uppercase());
        self.world.level.exiting = true;
        if !cfg!(test) {
            eprintln!(
                "room: level exit: ACS ChangeLevel (tic {})",
                self.world.level.time
            );
        }
        self.world.level.secret_exit = false;
    }

    fn get_player_info(&self, player: i32, info: i32) -> i32 {
        if player < 0 {
            return -1;
        }
        let Some(p) = self
            .world
            .players
            .get(player as usize)
            .and_then(|s| s.as_ref())
        else {
            return -1;
        };
        match info {
            // PLAYERINFO_PLAYERCLASS
            7 => p.cls.raw() as i32,
            // PLAYERINFO_FOV / DESIREDFOV bootstrap
            8 | 9 => 90,
            // health via mo
            100 => {
                p.mo.and_then(|id| self.world.actors.get(id))
                    .map(|a| a.health)
                    .unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn sector_damage(
        &mut self,
        tag: i32,
        amount: i32,
        _dtype: &str,
        protection: &str,
        _flags: i32,
    ) {
        if tag == 0 || amount <= 0 {
            return;
        }
        let prot = if protection.is_empty() {
            None
        } else {
            self.world.classes.find_name(protection)
        };
        let secs: Vec<u32> = self
            .world
            .map
            .sectors
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tags.contains(&tag))
            .map(|(i, _)| i as u32)
            .collect();
        if secs.is_empty() {
            return;
        }
        let victims: Vec<_> = self
            .world
            .thinkers
            .default
            .iter()
            .copied()
            .filter(|&id| {
                let Some(a) = self.world.actors.get(id) else {
                    return false;
                };
                if a.destroyed {
                    return false;
                }
                point_in_sector(&self.world.map, a.pos.x(), a.pos.y())
                    .or_else(|| point_in_sector_vanilla(&self.world.map, a.pos.x(), a.pos.y()))
                    .is_some_and(|si| secs.contains(&si.raw()))
            })
            .collect();
        for id in victims {
            if let Some(pclass) = prot {
                if self.world.inventory.count(id, pclass) > 0 {
                    continue;
                }
            }
            let _ = crate::damage::damage_mobj(self.world, id, None, None, amount);
        }
    }

    fn check_actor_texture(&self, tid: i32, texture: &str, floor: bool, activator: u64) -> i32 {
        if texture.is_empty() {
            return 0;
        }
        let name = actor_plane_texture(self.world, tid, activator, !floor);
        i32::from(name.eq_ignore_ascii_case(texture))
    }

    fn get_armor_info(&self, info: i32, activator: u64) -> i32 {
        let Some(id) = unpack_actor(activator) else {
            return 0;
        };
        let Some(p) = self
            .world
            .players
            .iter()
            .flatten()
            .find(|pl| pl.mo == Some(id))
        else {
            return 0;
        };
        match info {
            0 => p.armor,                  // ARMORINFO_AMOUNT
            1 => i32::from(p.armor_class), // ARMORINFO_CLASS / save type
            // ARMORINFO_SAVEPERCENT (whole percent).
            2 => (p.armor_fraction() * 100.0) as i32,
            3 => p.armor_max_absorb,      // ARMORINFO_MAXABSORB
            4 => p.armor_max_full_absorb, // ARMORINFO_MAXFULLABSORB
            _ => 0,
        }
    }

    fn level_info(&self, selector: i32) -> i32 {
        use room_acs::levelinfo;
        let l = &self.world.level;
        match selector {
            levelinfo::PAR_TIME => l.par_time as i32,
            levelinfo::SUCK_TIME => l.suck_time as i32,
            levelinfo::CLUSTERNUM => l.cluster,
            levelinfo::LEVELNUM => l.levelnum,
            levelinfo::TOTAL_SECRETS => l.total_secrets as i32,
            levelinfo::FOUND_SECRETS => l.found_secrets as i32,
            levelinfo::TOTAL_ITEMS => l.total_items as i32,
            levelinfo::FOUND_ITEMS => l.found_items as i32,
            levelinfo::TOTAL_MONSTERS => l.total_kills as i32,
            levelinfo::KILLED_MONSTERS => l.found_kills as i32,
            _ => 0,
        }
    }

    fn change_sky(&mut self, sky1: &str, sky2: &str) {
        if !sky1.is_empty() {
            self.world.sky1_override = Some(sky1.to_ascii_uppercase());
        }
        if !sky2.is_empty() {
            self.world.sky2_override = Some(sky2.to_ascii_uppercase());
        }
    }

    fn replace_textures(&mut self, from: &str, to: &str, _flags: i32) {
        if from.is_empty() || to.is_empty() {
            return;
        }
        let from_u = from.to_ascii_uppercase();
        let to_u = to.to_ascii_uppercase();
        let rewrite = |map: &mut std::collections::BTreeMap<u32, String>| {
            for v in map.values_mut() {
                if v.eq_ignore_ascii_case(&from_u) {
                    *v = to_u.clone();
                }
            }
        };
        rewrite(&mut self.world.sector_floor_pics);
        rewrite(&mut self.world.sector_ceiling_pics);
        rewrite(&mut self.world.switch_mids);
        rewrite(&mut self.world.switch_uppers);
        rewrite(&mut self.world.switch_lowers);
        // Seed floor/ceiling overrides from static map flats when matching.
        for (i, sec) in self.world.map.sectors.iter().enumerate() {
            let si = i as u32;
            if sec.floor_pic.eq_ignore_ascii_case(&from_u)
                && !self.world.sector_floor_pics.contains_key(&si)
            {
                self.world.sector_floor_pics.insert(si, to_u.clone());
            }
            if sec.ceiling_pic.eq_ignore_ascii_case(&from_u)
                && !self.world.sector_ceiling_pics.contains_key(&si)
            {
                self.world.sector_ceiling_pics.insert(si, to_u.clone());
            }
        }
    }

    fn get_armor_type(&self, type_name: &str, player: i32) -> i32 {
        if player < 0 || type_name.is_empty() {
            return 0;
        }
        let Some(p) = self
            .world
            .players
            .get(player as usize)
            .and_then(|s| s.as_ref())
        else {
            return 0;
        };
        if p.armor <= 0 {
            return 0;
        }
        let want = type_name.to_ascii_lowercase();
        // `BasicArmor.ArmorType` is the class that gave the armor.
        if let Some(name) = p
            .armor_type
            .and_then(|c| self.world.classes.get(c))
            .map(|c| c.name.to_ascii_lowercase())
        {
            return if name == want { p.armor } else { 0 };
        }
        let class_ok = match p.armor_class {
            1 => want.contains("green") || want == "greenarmor" || want == "armorbonus",
            2 => want.contains("blue") || want == "bluearmor" || want == "armor",
            _ => false,
        };
        if class_ok { p.armor } else { 0 }
    }

    fn drop_inventory(&mut self, tid: i32, item: &str, activator: u64) -> i32 {
        if item.is_empty() {
            return 0;
        }
        let Some(class) = self.world.classes.find_name(item) else {
            return 0;
        };
        let mut n = 0i32;
        for id in collect_tid_ids(self.world, tid, activator) {
            let count = self.world.inventory.count(id, class);
            if count <= 0 {
                continue;
            }
            let _ = self.world.inventory.take(id, class, count);
            let Some(src) = self.world.actors.get(id).map(|a| (a.pos, a.angle)) else {
                continue;
            };
            if let Ok(drop) = self.world.spawn_at(class, src.0, src.1) {
                if let Some(a) = self.world.actors.get_mut(drop) {
                    a.flags |= crate::class::FLAG_DROPPED;
                }
                n += 1;
            }
        }
        n
    }

    fn line_attack_acs(
        &mut self,
        tid: i32,
        angle: i32,
        pitch: i32,
        damage: i32,
        range: i32,
        activator: u64,
    ) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let ang = acs_to_angle(angle);
        let pit = acs_to_angle(pitch);
        let range_u = if range != 0 {
            Fixed(range).to_f64().abs()
        } else {
            2048.0 // Doom `MISSILERANGE`
        };
        let slope = room_core::dmath::tan(pit.radians());
        let hit = crate::hitscan::line_attack_with_slope(
            self.world,
            id,
            ang,
            range_u,
            damage.max(0),
            slope,
        );
        i32::from(hit.actor.is_some())
    }

    fn set_music(&mut self, song: &str, _order: i32, _unused: i32) {
        let song = song.trim();
        if song.is_empty() || song == "*" || song.eq_ignore_ascii_case("*") {
            return;
        }
        self.world.music_request = Some(song.to_owned());
    }

    fn set_font(&mut self, name: &str) {
        self.world.acs_font = name.to_owned();
    }

    fn fade_to(&mut self, r: i32, g: i32, b: i32, a: i32, seconds: i32) {
        let to_f = |v: i32| Fixed(v).to_f64().clamp(0.0, 1.0) as f32;
        let secs = Fixed(seconds).to_f64().max(0.0);
        let tics = if secs <= 0.0 {
            0
        } else {
            (secs * 35.0).ceil() as i32
        };
        let (tr, tg, tb, ta) = (to_f(r), to_f(g), to_f(b), to_f(a));
        self.world.push_debug(format!(
            "FADE acs ({tr:.2},{tg:.2},{tb:.2}) a={ta:.2} tics={tics}"
        ));
        let fade = &mut self.world.screen_fade;
        fade.target_r = tr;
        fade.target_g = tg;
        fade.target_b = tb;
        fade.target_a = ta;
        if tics <= 0 {
            fade.r = fade.target_r;
            fade.g = fade.target_g;
            fade.b = fade.target_b;
            fade.a = fade.target_a;
            fade.tics_left = 0;
        } else {
            fade.tics_left = tics;
        }
    }

    fn cancel_fade(&mut self) {
        self.world.screen_fade.cancel();
    }

    fn check_weapon(&self, name: &str, activator: u64) -> i32 {
        let Some(id) = unpack_actor(activator) else {
            return 0;
        };
        let Some(class) = self.world.classes.find_name(name) else {
            return 0;
        };
        i32::from(
            self.world
                .players
                .iter()
                .flatten()
                .any(|p| p.mo == Some(id) && p.ready_weapon == Some(class)),
        )
    }

    fn set_weapon(&mut self, name: &str, activator: u64) -> i32 {
        let Some(id) = unpack_actor(activator) else {
            return 0;
        };
        let Some(class) = self.world.classes.find_name(name) else {
            return 0;
        };
        if self.world.inventory.count(id, class) <= 0 {
            return 0;
        }
        for p in self.world.players.iter_mut().flatten() {
            if p.mo == Some(id) {
                p.pending_weapon = Some(class);
                return 1;
            }
        }
        0
    }

    fn use_inventory(&mut self, name: &str, activator: u64) -> i32 {
        let Some(id) = unpack_actor(activator) else {
            return 0;
        };
        let Some(class) = self.world.classes.find_name(name) else {
            return 0;
        };
        if crate::pickup::use_inventory_item(self.world, id, class) {
            return 1;
        }
        i32::from(self.world.inventory.take(id, class, 1) > 0)
    }

    fn player_number(&self, activator: u64) -> i32 {
        let Some(id) = unpack_actor(activator) else {
            return -1;
        };
        self.world
            .players
            .iter()
            .enumerate()
            .find_map(|(i, p)| p.as_ref().filter(|pl| pl.mo == Some(id)).map(|_| i as i32))
            .unwrap_or(-1)
    }

    fn get_cvar(&self, name: &str) -> i32 {
        match name.to_ascii_lowercase().as_str() {
            "skill" | "gameskill" => i32::from(self.world.skill),
            "sv_fastmonsters" | "fastmonsters" => i32::from(self.world.fast_monsters),
            "sv_respawnmonsters" | "respawn" => i32::from(self.world.respawn_monsters),
            "sv_nomapfixes" => 0,
            other => self.world.mod_cvar_int(other).unwrap_or(0),
        }
    }

    fn set_cvar(&mut self, name: &str, value: i32) -> i32 {
        match name.to_ascii_lowercase().as_str() {
            "skill" | "gameskill" => {
                self.world.skill = value.clamp(1, 5) as u8;
                1
            }
            "sv_fastmonsters" | "fastmonsters" => {
                self.world.fast_monsters = value != 0;
                1
            }
            "sv_respawnmonsters" | "respawn" => {
                self.world.respawn_monsters = value != 0;
                1
            }
            other if self.world.mod_cvars.contains_key(other) => {
                self.world.set_mod_cvar(other, &value.to_string());
                1
            }
            _ => 0,
        }
    }

    fn get_user_cvar(&self, player: i32, name: &str) -> i32 {
        let key = user_cvar_key(player, name);
        self.world.acs_user_cvars.get(&key).copied().unwrap_or(0)
    }

    fn set_user_cvar(&mut self, player: i32, name: &str, value: i32) -> i32 {
        if name.is_empty() {
            return 0;
        }
        let key = user_cvar_key(player, name);
        self.world.acs_user_cvars.insert(key, value);
        1
    }

    fn get_cvar_string(&self, name: &str) -> String {
        let lower = name.to_ascii_lowercase();
        if let Some(s) = self.world.acs_cvar_strings.get(&lower) {
            return s.clone();
        }
        if let Some(s) = self.world.mod_cvars.get(&lower) {
            return s.clone();
        }
        match lower.as_str() {
            "skill" | "gameskill" => self.world.skill.to_string(),
            "sv_fastmonsters" | "fastmonsters" => {
                if self.world.fast_monsters {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            _ => String::new(),
        }
    }

    fn set_cvar_string(&mut self, name: &str, value: &str) -> i32 {
        if name.is_empty() {
            return 0;
        }
        self.world
            .acs_cvar_strings
            .insert(name.to_ascii_lowercase(), value.to_owned());
        1
    }

    fn get_user_cvar_string(&self, player: i32, name: &str) -> String {
        let key = user_cvar_key(player, name);
        self.world
            .acs_user_cvar_strings
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    fn set_user_cvar_string(&mut self, player: i32, name: &str, value: &str) -> i32 {
        if name.is_empty() {
            return 0;
        }
        let key = user_cvar_key(player, name);
        self.world
            .acs_user_cvar_strings
            .insert(key, value.to_owned());
        1
    }

    fn start_slideshow(&mut self, name: &str) {
        if !name.is_empty() {
            self.world.slideshow_request = Some(name.to_owned());
        }
    }

    fn get_sector_health(&self, tag: i32, part: i32) -> i32 {
        for (i, sec) in self.world.map.sectors.iter().enumerate() {
            if !sec.tags.contains(&tag) {
                continue;
            }
            let Some(dyn_s) = self.world.sectors.get(i) else {
                return 0;
            };
            return match part {
                room_acs::secpart::CEILING => dyn_s.health_ceiling,
                room_acs::secpart::THREE_D => dyn_s.health_3d,
                _ => dyn_s.health_floor,
            };
        }
        0
    }

    fn get_line_health(&self, line_id: i32) -> i32 {
        let Some(li) = lines_for_acs_id(self.world, line_id).into_iter().next() else {
            return 0;
        };
        self.world.line_health.get(&li).copied().unwrap_or(0)
    }

    fn get_line_x(&self, line_id: i32, frac: i32, offset: i32) -> i32 {
        line_point_acs(self.world, line_id, frac, offset, true)
    }

    fn get_line_y(&self, line_id: i32, frac: i32, offset: i32) -> i32 {
        line_point_acs(self.world, line_id, frac, offset, false)
    }

    fn set_subtitle_number(&mut self, log: i32, sound: &str, activator: u64) {
        if unpack_actor(activator).is_none() {
            return;
        }
        self.world.subtitle_request = Some((log, sound.to_owned()));
    }

    fn get_net_id(&self, tid: i32, _ptr: i32, aaptr: i32, activator: u64) -> i32 {
        let Some(mut id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        if aaptr != 0 {
            id = match copy_aaptr(self.world, id, aaptr) {
                Some(p) => p,
                None => return 0,
            };
        }
        // Bootstrap net-id = arena slot index (full network entity IDs later).
        id.index() as i32
    }

    fn set_activator_by_net_id(&self, net_id: i32, aaptr: i32) -> u64 {
        if net_id < 0 {
            return 0;
        }
        let idx = net_id as u32;
        let Some(id) = self
            .world
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| id.index() == idx)
            .filter(|&id| self.world.actors.get(id).is_some_and(|a| !a.destroyed))
        else {
            return 0;
        };
        let id = if aaptr != 0 {
            match copy_aaptr(self.world, id, aaptr) {
                Some(p) => p,
                None => return 0,
            }
        } else {
            id
        };
        pack_actor(id)
    }

    fn set_sector_glow(&mut self, tag: i32, which: i32, r: i32, g: i32, b: i32, height: i32) {
        let glow = if r < 0 {
            None
        } else {
            Some(crate::world::SectorPlaneGlow {
                color: [
                    r.clamp(0, 255) as u8,
                    g.clamp(0, 255) as u8,
                    b.clamp(0, 255) as u8,
                ],
                height: height as f32,
            })
        };
        for (i, sec) in self.world.map.sectors.iter().enumerate() {
            if !sec.tags.contains(&tag) {
                continue;
            }
            if let Some(dyn_s) = self.world.sectors.get_mut(i) {
                if which != 0 {
                    dyn_s.glow_ceiling = glow;
                } else {
                    dyn_s.glow_floor = glow;
                }
            }
        }
    }

    fn set_fog_density(&mut self, tag: i32, density: i32) {
        let d = (density / 2).clamp(0, 255) as u8;
        for (i, sec) in self.world.map.sectors.iter().enumerate() {
            if !sec.tags.contains(&tag) {
                continue;
            }
            if let Some(dyn_s) = self.world.sectors.get_mut(i) {
                dyn_s.fog_density = d;
            }
        }
        self.world.fog_density = (f32::from(d) / 255.0).max(self.world.fog_density);
    }

    fn get_team_score(&self, team: i32) -> i32 {
        self.world.team_scores.get(&team).copied().unwrap_or(0)
    }

    fn set_team_score(&mut self, team: i32, value: i32) {
        self.world.team_scores.insert(team, value);
    }

    fn announcer_sound(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        // Global cue — play at origin with no attenuation.
        self.world.play_sound_attn(name, 0.0, 0.0, 0.0, 1.0, 0.0);
    }

    fn set_pointer_acs(
        &mut self,
        assign_slot: i32,
        tid: i32,
        pointer_selector: i32,
        activator: u64,
    ) -> i32 {
        let Some(act) = unpack_actor(activator)
            .filter(|&id| self.world.actors.get(id).is_some_and(|a| !a.destroyed))
        else {
            return 0;
        };
        let Some(mut ptr) = resolve_actor(self.world, tid, activator) else {
            // Still clear assign slot when tid missing? GZDoom leaves ptr null.
            assign_aaptr_slots(self.world, act, assign_slot, None);
            return 0;
        };
        if pointer_selector != 0 {
            ptr = match copy_aaptr(self.world, ptr, pointer_selector) {
                Some(p) => p,
                None => {
                    assign_aaptr_slots(self.world, act, assign_slot, None);
                    return 0;
                }
            };
        }
        if ptr == act {
            assign_aaptr_slots(self.world, act, assign_slot, None);
            return 0;
        }
        if self.world.actors.get(ptr).is_none_or(|a| a.destroyed) {
            assign_aaptr_slots(self.world, act, assign_slot, None);
            return 0;
        }
        assign_aaptr_slots(self.world, act, assign_slot, Some(ptr));
        1
    }

    fn script_call(&mut self, args: &[i32], activator: u64) -> i32 {
        // Direct calls (tests, puke) while the ACS state is still in the world.
        let strings: Vec<Option<String>> = args
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                (i < 2 || (v > 0 && (v & 0x4000_0000) != 0 && (v & 0x0FFF_FFFF) < 4096))
                    .then(|| self.world.acs.resolve_string(v))
            })
            .collect();
        self.script_call_resolved(args, &strings, activator)
    }

    fn script_call_resolved(
        &mut self,
        args: &[i32],
        strings: &[Option<String>],
        activator: u64,
    ) -> i32 {
        let name = |i: usize| strings.get(i).cloned().flatten().unwrap_or_default();
        let (class, func) = (name(0), name(1));
        if class.is_empty() || func.is_empty() {
            return 0;
        }
        let rest = args.get(2..).unwrap_or(&[]);
        // Prefer ZScript VM (`docs/34` ZS4 / BD6) when a program is loaded.
        if self.world.zscript.is_active() {
            let qname = format!("{class}.{func}");
            self.world.zscript.sync_host(self.world.skill, 0, 0);
            let mut i_args = Vec::new();
            let mut s_owned = Vec::new();
            for (k, &v) in rest.iter().enumerate() {
                // String handles were resolved by the VM; other values,
                // including negative ones such as `-2`, are ints.
                match strings.get(k + 2).cloned().flatten() {
                    Some(text) => s_owned.push(text),
                    None => i_args.push(i64::from(v)),
                }
            }
            let s_refs: Vec<&str> = s_owned.iter().map(String::as_str).collect();
            let result = self
                .world
                .zscript
                .script_call_with(&qname, &i_args, &s_refs);
            // What the function did to the playsim: `GiveInventory` /
            // `TakeInventory` on any actor (Brutal Doom's `GetGender` gives
            // the `GenderMale` token its status bar draws the face for) and
            // the other queued natives.
            let grants = self.world.zscript.take_grants();
            let fallback = resolve_actor(self.world, 0, activator)
                .or_else(|| self.world.players.iter().flatten().find_map(|p| p.mo));
            if let Some(owner) = fallback {
                for (cookie, item, amount) in grants {
                    // `players[n].mo` handles name the player's pawn.
                    let pawn = self
                        .world
                        .zscript
                        .player_of_cookie(cookie)
                        .and_then(|pn| usize::try_from(pn).ok())
                        .and_then(|pn| self.world.players.get(pn).and_then(|p| p.as_ref()))
                        .and_then(|p| p.mo);
                    match pawn {
                        Some(mo) => self
                            .world
                            .apply_zscript_inventory_change(mo, -1, &item, amount),
                        None => self
                            .world
                            .apply_zscript_inventory_change(owner, cookie, &item, amount),
                    }
                }
            }
            self.world.apply_zscript_native_ops(None);
            if let Ok(v) = result {
                return i32::try_from(v).unwrap_or(i32::MAX);
            }
        }
        // Content-loaded WASM mods (`reg_static_function`, `docs/39` §2):
        // ints plus the strings the VM resolved, so a Rust port of a
        // ZScript `ScriptCall` target sees the same arguments.
        let rest_strings: Vec<Option<String>> = (0..rest.len())
            .map(|k| strings.get(k + 2).cloned().flatten())
            .collect();
        if let Some(r) =
            crate::script_mods::dispatch_static_call(self.world, &class, &func, rest, &rest_strings)
        {
            return match r {
                crate::script_mods::StaticCallResult::Int(v) => {
                    i32::try_from(v).unwrap_or(i32::MAX)
                }
                crate::script_mods::StaticCallResult::Str(text) => {
                    PENDING_CALL_STRING.with(|c| *c.borrow_mut() = Some(text));
                    0
                }
            };
        }
        match self.world.scripts.call_static(&class, &func, rest) {
            Ok(v) => i32::try_from(v).unwrap_or(i32::MAX),
            Err(_) => 0,
        }
    }

    fn take_script_call_string(&mut self) -> Option<String> {
        PENDING_CALL_STRING.with(|c| c.borrow_mut().take())
    }

    fn set_hud_clip_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.world.hud_clip = crate::world::HudClipRect { x, y, w, h };
    }

    fn set_hud_wrap_width(&mut self, w: i32) {
        self.world.hud_wrap_width = w.max(0);
    }

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
        self.world.spawn_particle(
            color as u32,
            fullbright,
            lifetime,
            Fixed(size).to_f64(),
            Fixed(x).to_f64(),
            Fixed(y).to_f64(),
            Fixed(z).to_f64(),
        );
    }

    fn print(&mut self, text: &str, _bold: bool) {
        if !text.is_empty() {
            self.world
                .push_message_kind(crate::world::MessageKind::Script, text);
        }
    }

    fn hud_message_ex(&mut self, msg: room_acs::HudMessageArgs) {
        // GZDoom: a missile activator shows the message to its shooter; a
        // non-bold message with a player activator is only for that player.
        let seat = if msg.bold {
            None
        } else {
            unpack_actor(msg.activator).and_then(|id| {
                let a = self.world.actors.get(id)?;
                let screen = if (a.flags & crate::class::FLAG_MISSILE) != 0 {
                    a.target.unwrap_or(id)
                } else {
                    id
                };
                self.world
                    .players
                    .iter()
                    .position(|p| p.as_ref().is_some_and(|p| p.mo == Some(screen)))
            })
        };
        let hud = crate::hud_msg::AcsHudMessage::from_args(&msg, seat);
        self.world.attach_hud_message(hud);
    }

    fn clear_line_special(&mut self, line: i32) {
        if line < 0 {
            return;
        }
        self.world.line_specials.insert(line as u32, 0);
    }

    fn tag_busy(&self, tag: i32) -> bool {
        if tag == 0 {
            return false;
        }
        let secs: Vec<u32> = self
            .world
            .map
            .sectors
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tags.contains(&tag))
            .map(|(i, _)| i as u32)
            .collect();
        if secs.is_empty() {
            return false;
        }
        let busy_floor = self
            .world
            .floors
            .iter()
            .any(|f| secs.contains(&f.sector.raw()));
        let busy_plat = self
            .world
            .plats
            .iter()
            .any(|p| secs.contains(&p.sector.raw()));
        let busy_ceil = self
            .world
            .ceilings
            .iter()
            .any(|c| secs.contains(&c.sector.raw()));
        let busy_door = self
            .world
            .doors
            .iter()
            .any(|d| secs.contains(&d.sector.raw()));
        busy_floor || busy_plat || busy_ceil || busy_door
    }

    fn poly_busy(&self, po: i32) -> bool {
        self.world
            .polyobjs
            .iter()
            .any(|p| p.id == po && p.motion.is_some())
    }

    fn unsupported_pcode(&mut self, op: u32) {
        report_unsupported_acs("ACS p-code", op as i32);
    }

    fn print_name(&self, which: i32, activator: u64) -> String {
        match which {
            -1 | -2 | -3 => String::new(),
            _ => {
                let seat = if which == 0 {
                    let me = unpack_actor(activator);
                    self.world
                        .players
                        .iter()
                        .position(|p| p.as_ref().is_some_and(|p| p.mo.is_some() && p.mo == me))
                        .unwrap_or(0)
                } else {
                    (which - 1).max(0) as usize
                };
                let raw = self.world.player_names.stored(seat);
                let name = String::from_utf8_lossy(raw).into_owned();
                if name.is_empty() {
                    "Player".into()
                } else {
                    name
                }
            }
        }
    }

    fn set_thing_special(&mut self, tid: i32, special: i32, args: [i32; 5], activator: u64) {
        for id in collect_tid_ids(self.world, tid, activator) {
            if let Some(a) = self.world.actors.get_mut(id) {
                a.special = special.clamp(0, i32::from(u16::MAX)) as u16;
                a.args = args;
            }
        }
    }

    fn spawn_projectile(
        &mut self,
        tid: i32,
        type_id: i32,
        class: &str,
        angle: i32,
        speed: i32,
        vspeed: i32,
        gravity: bool,
        newtid: i32,
        activator: u64,
    ) {
        let cls = if class.is_empty() {
            crate::specials::class_for_spawn_type(self.world, type_id)
        } else {
            self.world
                .classes
                .find_name(class)
                .map(|id| self.world.classes.replacement(id))
        };
        let Some(cls) = cls else {
            return;
        };
        crate::specials::spawn_projectiles_at(
            self.world,
            tid,
            cls,
            angle,
            f64::from(speed) / 8.0,
            f64::from(vspeed) / 8.0,
            gravity,
            newtid,
            unpack_actor(activator),
        );
    }

    fn call_func(&mut self, func_index: i32, args: &[i32], activator: u64) -> i32 {
        match func_index {
            acsf::GET_POLYOBJ_X => {
                let id = args.first().copied().unwrap_or(0);
                match self.world.polyobjs.iter().find(|p| p.id == id) {
                    Some(p) => Fixed::from_f64(p.start_spot.x()).0,
                    None => i32::MAX,
                }
            }
            acsf::GET_POLYOBJ_Y => {
                let id = args.first().copied().unwrap_or(0);
                match self.world.polyobjs.iter().find(|p| p.id == id) {
                    Some(p) => Fixed::from_f64(p.start_spot.y()).0,
                    None => i32::MAX,
                }
            }
            acsf::IS_TID_USED => {
                let tid = args.first().copied().unwrap_or(0);
                i32::from(is_tid_used(self.world, tid))
            }
            acsf::UNIQUE_TID => {
                let start = args.first().copied().unwrap_or(0);
                let limit = args.get(1).copied().filter(|&l| l >= 0).unwrap_or(0);
                find_unique_tid(self.world, start, limit)
            }
            acsf::GET_ACTOR_VEL_X | acsf::GET_ACTOR_VEL_Y | acsf::GET_ACTOR_VEL_Z => {
                let tid = args.first().copied().unwrap_or(0);
                let Some(id) = resolve_actor(self.world, tid, activator) else {
                    return 0;
                };
                let Some(a) = self.world.actors.get(id) else {
                    return 0;
                };
                let v = match func_index {
                    acsf::GET_ACTOR_VEL_X => a.vel.x(),
                    acsf::GET_ACTOR_VEL_Y => a.vel.y(),
                    _ => a.vel.z(),
                };
                Fixed::from_f64(v).0
            }
            acsf::CHECK_SIGHT => {
                let tid1 = args.first().copied().unwrap_or(0);
                let tid2 = args.get(1).copied().unwrap_or(0);
                // flags (args[2]) ignored for bootstrap — uses Doom P_CheckSight.
                i32::from(acs_check_sight(self.world, tid1, tid2, activator))
            }
            acsf::SET_ACTOR_VELOCITY => {
                let tid = args.first().copied().unwrap_or(0);
                let vx = args.get(1).copied().unwrap_or(0);
                let vy = args.get(2).copied().unwrap_or(0);
                let vz = args.get(3).copied().unwrap_or(0);
                let add = args.get(4).copied().unwrap_or(0) != 0;
                self.set_actor_velocity(tid, vx, vy, vz, add, activator)
            }
            // Zandronum extensions (ResetMap, PlayerIsSpectator,
            // ConsolePlayerNumber, …): GZDoom's CallFunction falls through to
            // `return 0` for these. In single player 0 is also the right
            // answer (console player 0, not a spectator, no lives counter).
            100..=199 => 0,
            other => {
                report_unsupported_acs("ACSF function", other);
                0
            }
        }
    }

    fn udmf_int(&self, kind: u8, id: i32, key: &str) -> i32 {
        udmf_builtin_int(self.world, kind, id, key)
    }

    fn udmf_fixed(&self, kind: u8, id: i32, key: &str) -> i32 {
        udmf_builtin_fixed(self.world, kind, id, key)
    }

    fn actor_x(&self, tid: i32, activator: u64) -> i32 {
        resolve_actor(self.world, tid, activator)
            .and_then(|id| self.world.actors.get(id))
            .map(|a| Fixed::from_f64(a.pos.x()).0)
            .unwrap_or(0)
    }

    fn actor_y(&self, tid: i32, activator: u64) -> i32 {
        resolve_actor(self.world, tid, activator)
            .and_then(|id| self.world.actors.get(id))
            .map(|a| Fixed::from_f64(a.pos.y()).0)
            .unwrap_or(0)
    }

    fn actor_z(&self, tid: i32, activator: u64) -> i32 {
        resolve_actor(self.world, tid, activator)
            .and_then(|id| self.world.actors.get(id))
            .map(|a| Fixed::from_f64(a.pos.z()).0)
            .unwrap_or(0)
    }

    fn actor_floor_z(&self, tid: i32, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        let z = point_in_sector(&self.world.map, a.pos.x(), a.pos.y())
            .and_then(|si| self.world.sectors.get(si.raw() as usize))
            .map(|s| s.floor_z)
            .unwrap_or(a.pos.z());
        Fixed::from_f64(z).0
    }

    fn actor_ceiling_z(&self, tid: i32, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        let z = point_in_sector(&self.world.map, a.pos.x(), a.pos.y())
            .and_then(|si| self.world.sectors.get(si.raw() as usize))
            .map(|s| s.ceiling_z)
            .unwrap_or(a.pos.z() + a.height);
        Fixed::from_f64(z).0
    }

    fn actor_angle(&self, tid: i32, activator: u64) -> i32 {
        resolve_actor(self.world, tid, activator)
            .and_then(|id| self.world.actors.get(id))
            .map(|a| angle_to_acs(a.angle))
            .unwrap_or(0)
    }

    fn actor_pitch(&self, tid: i32, activator: u64) -> i32 {
        resolve_actor(self.world, tid, activator)
            .and_then(|id| self.world.actors.get(id))
            // ACS pitch is GZDoom-signed (positive = down); Room stores up.
            .map(|a| angle_to_acs(Angle::from_degrees(-a.pitch.degrees())))
            .unwrap_or(0)
    }

    fn activator_tid(&self, activator: u64) -> i32 {
        unpack_actor(activator)
            .and_then(|id| self.world.actors.get(id))
            .map(|a| a.tid)
            .unwrap_or(0)
    }

    fn sector_floor_z(&self, tag: i32, x: i32, y: i32) -> i32 {
        sector_plane_z(self.world, tag, x, y, true)
    }

    fn sector_ceiling_z(&self, tag: i32, x: i32, y: i32) -> i32 {
        sector_plane_z(self.world, tag, x, y, false)
    }

    fn sector_light_level(&self, tag: i32) -> i32 {
        first_sector_from_tag(self.world, tag)
            .and_then(|i| self.world.sectors.get(i))
            .map(|s| i32::from(s.light))
            .unwrap_or(-1)
    }

    fn actor_light_level(&self, tid: i32, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        crate::physics::actor_light_level(self.world, a)
    }

    fn thing_count(&self, type_id: i32, tid: i32) -> i32 {
        count_things(self.world, ThingCountFilter::DoomedNum(type_id), tid, None)
    }

    fn thing_count_name(&self, class_name: &str, tid: i32) -> i32 {
        count_things(
            self.world,
            ThingCountFilter::ClassName(class_name),
            tid,
            None,
        )
    }

    fn thing_count_sector(&self, type_id: i32, tid: i32, tag: i32) -> i32 {
        count_things(
            self.world,
            ThingCountFilter::DoomedNum(type_id),
            tid,
            Some(tag),
        )
    }

    fn thing_count_name_sector(&self, class_name: &str, tid: i32, tag: i32) -> i32 {
        count_things(
            self.world,
            ThingCountFilter::ClassName(class_name),
            tid,
            Some(tag),
        )
    }

    fn clear_actor_inventory(&mut self, tid: i32, activator: u64) {
        for id in collect_tid_ids(self.world, tid, activator) {
            self.world.inventory.clear(id);
        }
    }

    fn give_actor_inventory(&mut self, tid: i32, item: &str, amount: i32, activator: u64) -> i32 {
        if amount <= 0 || item.is_empty() {
            return 0;
        }
        let Some(class) = self.world.classes.find_name(item) else {
            return 0;
        };
        let mut n = 0i32;
        for id in collect_tid_ids(self.world, tid, activator) {
            if crate::inventory::give_inventory(self.world, id, class, amount) {
                n += 1;
            }
        }
        n
    }

    fn take_actor_inventory(&mut self, tid: i32, item: &str, amount: i32, activator: u64) -> i32 {
        if amount <= 0 || item.is_empty() {
            return 0;
        }
        let Some(class) = self.world.classes.find_name(item) else {
            return 0;
        };
        let mut n = 0i32;
        for id in collect_tid_ids(self.world, tid, activator) {
            if self.world.inventory.take(id, class, amount) > 0 {
                n += 1;
            }
        }
        n
    }

    fn morph_actor(
        &mut self,
        tid: i32,
        player_class: &str,
        monster_class: &str,
        _duration: i32,
        _style: i32,
        _enter_flash: &str,
        _exit_flash: &str,
        activator: u64,
    ) -> i32 {
        let class_name = if !player_class.is_empty() {
            player_class
        } else {
            monster_class
        };
        let Some(class) = self.world.classes.find_name(class_name) else {
            return 0;
        };
        let ids = collect_tid_ids(self.world, tid, activator);
        ids.into_iter()
            .filter(|&id| self.world.morph_actor_class(id, class))
            .count() as i32
    }

    fn unmorph_actor(&mut self, tid: i32, _force: bool, activator: u64) -> i32 {
        let ids = collect_tid_ids(self.world, tid, activator);
        ids.into_iter()
            .filter(|&id| self.world.unmorph_actor_class(id))
            .count() as i32
    }

    fn check_actor_inventory(&self, tid: i32, item: &str, activator: u64) -> i32 {
        if item.is_empty() {
            return 0;
        }
        let Some(class) = self.world.classes.find_name(item) else {
            return 0;
        };
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        self.world.inventory.count(id, class)
    }

    fn spawn(&mut self, type_id: i32, x: i32, y: i32, z: i32, angle: i32, tid: i32) -> i32 {
        let Some(class) = self.world.classes.find_doomednum(type_id) else {
            return 0;
        };
        let pos = DVec3::new(Fixed(x).to_f64(), Fixed(y).to_f64(), Fixed(z).to_f64());
        let ang = byte_angle(angle);
        match self.world.spawn_at(class, pos, ang) {
            Ok(id) => {
                if let Some(a) = self.world.actors.get_mut(id) {
                    a.tid = tid;
                }
                1
            }
            Err(_) => 0,
        }
    }

    fn set_activator(&self, tid: i32, current: u64) -> u64 {
        resolve_actor(self.world, tid, current)
            .filter(|&id| self.world.actors.get(id).is_some_and(|a| !a.destroyed))
            .map(pack_actor)
            .unwrap_or(0)
    }

    fn set_activator_to_target(&self, tid: i32, current: u64) -> u64 {
        let Some(id) = resolve_actor(self.world, tid, current) else {
            return 0;
        };
        self.world
            .actors
            .get(id)
            .and_then(|a| a.target)
            .filter(|&t| self.world.actors.get(t).is_some_and(|a| !a.destroyed))
            .map(pack_actor)
            .unwrap_or(0)
    }

    fn set_actor_velocity(
        &mut self,
        tid: i32,
        vx: i32,
        vy: i32,
        vz: i32,
        add: bool,
        activator: u64,
    ) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get_mut(id) else {
            return 0;
        };
        let dv = DVec3::new(Fixed(vx).to_f64(), Fixed(vy).to_f64(), Fixed(vz).to_f64());
        a.vel = if add {
            DVec3::new(a.vel.x() + dv.x(), a.vel.y() + dv.y(), a.vel.z() + dv.z())
        } else {
            dv
        };
        1
    }

    fn actor_property(&self, tid: i32, prop: i32, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        let value = match prop {
            aprop::HEALTH => a.health,
            aprop::SPEED => Fixed::from_f64(a.speed).0,
            aprop::DAMAGE => a.damage,
            aprop::ALPHA => Fixed::from_f64(a.alpha).0,
            aprop::RENDERSTYLE => a.render_style.to_acs_style(),
            aprop::AMBUSH => i32::from((a.flags & crate::class::FLAG_AMBUSH) != 0),
            aprop::INVULNERABLE => i32::from((a.flags & crate::class::FLAG_INVULNERABLE) != 0),
            aprop::GRAVITY => Fixed::from_f64(a.gravity).0,
            aprop::FRIENDLY => i32::from((a.flags & crate::class::FLAG_FRIENDLY) != 0),
            aprop::SPAWNHEALTH => self
                .world
                .classes
                .get(a.class)
                .map(|c| c.health)
                .unwrap_or(0),
            aprop::DROPPED => i32::from((a.flags & crate::class::FLAG_DROPPED) != 0),
            aprop::NOTARGET => i32::from((a.flags & crate::class::FLAG_NOTARGET) != 0),
            aprop::SCORE => a.score,
            aprop::NOTRIGGER => i32::from((a.flags & crate::class::FLAG_NOTRIGGER) != 0),
            aprop::DAMAGEFACTOR | aprop::DAMAGEMULTIPLIER => Fixed::from_f64(a.damage_factor).0,
            aprop::SCALEX => Fixed::from_f64(a.scale_x).0,
            aprop::SCALEY => Fixed::from_f64(a.scale_y).0,
            aprop::MASTER_TID => a
                .master
                .and_then(|m| self.world.actors.get(m))
                .map(|m| m.tid)
                .unwrap_or(0),
            aprop::TARGET_TID => a
                .target
                .and_then(|t| self.world.actors.get(t))
                .map(|t| t.tid)
                .unwrap_or(0),
            aprop::TRACER_TID => a
                .tracer
                .and_then(|t| self.world.actors.get(t))
                .map(|t| t.tid)
                .unwrap_or(0),
            aprop::DORMANT => i32::from((a.flags & crate::class::FLAG_DORMANT) != 0),
            aprop::MASS => a.mass,
            aprop::ACCURACY => a.accuracy,
            aprop::STAMINA => a.stamina,
            aprop::HEIGHT => Fixed::from_f64(a.height).0,
            aprop::RADIUS => Fixed::from_f64(a.radius).0,
            aprop::REACTIONTIME => a.reaction_time,
            aprop::MELEERANGE => Fixed::from_f64(a.melee_range).0,
            aprop::JUMPZ => Fixed::from_f64(a.jump_z).0,
            aprop::CHASEGOAL => i32::from(a.chase_goal),
            aprop::FRIGHTENED => {
                i32::from(a.frightened || (a.flags & crate::class::FLAG_FRIGHTENED) != 0)
            }
            aprop::VIEWHEIGHT => Fixed::from_f64(a.view_height).0,
            aprop::ATTACKZOFFSET => Fixed::from_f64(a.attack_z_offset).0,
            aprop::FRICTION => Fixed::from_f64(a.friction).0,
            aprop::MAXSTEPHEIGHT => Fixed::from_f64(a.max_step_height).0,
            aprop::MAXDROPOFFHEIGHT => Fixed::from_f64(a.max_dropoff_height).0,
            aprop::STENCILCOLOR => a
                .fill_color
                .map(|[r, g, b]| (i32::from(r) << 16) | (i32::from(g) << 8) | i32::from(b))
                .unwrap_or(0),
            aprop::FRIENDLYSEEBLOCKS => a.friendly_see_blocks,
            _ => 0,
        };
        if tid == 261 && prop == aprop::HEALTH {
            let class = self
                .world
                .classes
                .get(a.class)
                .map_or("<missing>", |c| c.name.as_str());
            eprintln!(
                "room: VEHICLE ACS GetActorProperty tic={} tid=261 actor=#{} class={} health={}",
                self.world.level.time,
                id.index(),
                class,
                value,
            );
        }
        value
    }

    fn set_actor_property(&mut self, tid: i32, prop: i32, value: i32, activator: u64) {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return;
        };
        if prop == aprop::HEALTH {
            let actor = self.world.actors.get(id);
            let class = actor
                .and_then(|a| self.world.classes.get(a.class))
                .map_or("<missing>", |c| c.name.as_str());
            eprintln!(
                "room: VEHICLE ACS SetActorProperty tic={} tid={} actor=#{} class={} health={} -> {}",
                self.world.level.time,
                tid,
                id.index(),
                class,
                actor.map_or(0, |a| a.health),
                value,
            );
        }
        // Pointer props: resolve TID → actor before taking a mut borrow.
        if matches!(
            prop,
            aprop::MASTER_TID | aprop::TARGET_TID | aprop::TRACER_TID
        ) {
            let ptr = if value == 0 {
                None
            } else {
                self.world.find_by_tid(value)
            };
            if let Some(a) = self.world.actors.get_mut(id) {
                match prop {
                    aprop::MASTER_TID => a.master = ptr,
                    aprop::TARGET_TID => a.target = ptr,
                    aprop::TRACER_TID => a.tracer = ptr,
                    _ => {}
                }
            }
            return;
        }
        let Some(a) = self.world.actors.get_mut(id) else {
            return;
        };
        let set_flag = |flags: &mut u64, bit: u64, on: bool| {
            if on {
                *flags |= bit;
            } else {
                *flags &= !bit;
            }
        };
        match prop {
            aprop::HEALTH => a.health = value,
            aprop::SPEED => a.speed = Fixed(value).to_f64(),
            aprop::DAMAGE => a.damage = value,
            aprop::ALPHA => a.alpha = Fixed(value).to_f64().clamp(0.0, 1.0),
            aprop::RENDERSTYLE => {
                if let Some(style) = crate::class::RenderStyle::from_acs_style(value) {
                    a.render_style = style;
                }
            }
            aprop::AMBUSH => set_flag(&mut a.flags, crate::class::FLAG_AMBUSH, value != 0),
            aprop::INVULNERABLE => {
                set_flag(&mut a.flags, crate::class::FLAG_INVULNERABLE, value != 0)
            }
            aprop::GRAVITY => a.gravity = Fixed(value).to_f64(),
            aprop::FRIENDLY => set_flag(&mut a.flags, crate::class::FLAG_FRIENDLY, value != 0),
            aprop::DROPPED => set_flag(&mut a.flags, crate::class::FLAG_DROPPED, value != 0),
            aprop::NOTARGET => set_flag(&mut a.flags, crate::class::FLAG_NOTARGET, value != 0),
            aprop::SCORE => a.score = value,
            aprop::NOTRIGGER => set_flag(&mut a.flags, crate::class::FLAG_NOTRIGGER, value != 0),
            aprop::DAMAGEFACTOR | aprop::DAMAGEMULTIPLIER => {
                a.damage_factor = Fixed(value).to_f64().max(0.0)
            }
            aprop::SCALEX => a.scale_x = Fixed(value).to_f64(),
            aprop::SCALEY => a.scale_y = Fixed(value).to_f64(),
            aprop::DORMANT => set_flag(&mut a.flags, crate::class::FLAG_DORMANT, value != 0),
            aprop::MASS => a.mass = value,
            aprop::ACCURACY => a.accuracy = value,
            aprop::STAMINA => a.stamina = value,
            aprop::HEIGHT => a.height = Fixed(value).to_f64(),
            aprop::RADIUS => a.radius = Fixed(value).to_f64(),
            aprop::REACTIONTIME => a.reaction_time = value.max(0),
            aprop::MELEERANGE => a.melee_range = Fixed(value).to_f64().max(0.0),
            aprop::JUMPZ => a.jump_z = Fixed(value).to_f64().max(0.0),
            aprop::CHASEGOAL => a.chase_goal = value != 0,
            aprop::FRIGHTENED => {
                a.frightened = value != 0;
                set_flag(&mut a.flags, crate::class::FLAG_FRIGHTENED, value != 0);
            }
            aprop::VIEWHEIGHT => a.view_height = Fixed(value).to_f64().max(0.0),
            aprop::ATTACKZOFFSET => a.attack_z_offset = Fixed(value).to_f64(),
            aprop::FRICTION => a.friction = Fixed(value).to_f64().max(0.0),
            aprop::MAXSTEPHEIGHT => a.max_step_height = Fixed(value).to_f64().max(0.0),
            aprop::MAXDROPOFFHEIGHT => a.max_dropoff_height = Fixed(value).to_f64().max(0.0),
            aprop::STENCILCOLOR => {
                if value == 0 {
                    a.fill_color = None;
                } else {
                    a.fill_color = Some([
                        ((value >> 16) & 0xff) as u8,
                        ((value >> 8) & 0xff) as u8,
                        (value & 0xff) as u8,
                    ]);
                }
            }
            aprop::FRIENDLYSEEBLOCKS => a.friendly_see_blocks = value.max(0),
            _ => {}
        }
    }

    fn set_actor_position(
        &mut self,
        tid: i32,
        x: i32,
        y: i32,
        z: i32,
        _fog: bool,
        activator: u64,
    ) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get_mut(id) else {
            return 0;
        };
        a.pos = DVec3::new(Fixed(x).to_f64(), Fixed(y).to_f64(), Fixed(z).to_f64());
        1
    }

    fn set_actor_angle(&mut self, tid: i32, angle: i32, activator: u64) {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return;
        };
        if let Some(a) = self.world.actors.get_mut(id) {
            a.angle = acs_to_angle(angle);
        }
    }

    fn set_actor_pitch(&mut self, tid: i32, pitch: i32, activator: u64) {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return;
        };
        if let Some(a) = self.world.actors.get_mut(id) {
            a.pitch = Angle::from_degrees(-acs_to_angle(pitch).degrees());
        }
    }

    fn actor_roll(&self, tid: i32, activator: u64) -> i32 {
        resolve_actor(self.world, tid, activator)
            .and_then(|id| self.world.actors.get(id))
            .map(|a| angle_to_acs(a.roll))
            .unwrap_or(0)
    }

    fn set_actor_roll(&mut self, tid: i32, roll: i32, activator: u64) {
        let ang = acs_to_angle(roll);
        for id in collect_tid_ids(self.world, tid, activator) {
            if let Some(a) = self.world.actors.get_mut(id) {
                a.roll = ang;
            }
        }
    }

    fn get_max_inventory(&self, tid: i32, class_name: &str, activator: u64) -> i32 {
        if class_name.is_empty() {
            return 0;
        }
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        if class_name.eq_ignore_ascii_case("Health") {
            return self
                .world
                .actors
                .get(id)
                .and_then(|a| self.world.classes.get(a.class))
                .map(|c| c.health)
                .unwrap_or(0);
        }
        let Some(class) = self.world.classes.find_name(class_name) else {
            return 0;
        };
        if let Some(item) = self.world.inventory.find(id, class) {
            return item.max_amount;
        }
        self.world
            .classes
            .get(class)
            .map(|c| c.inventory_max_amount)
            .unwrap_or(0)
    }

    fn check_actor_state(&self, tid: i32, state: &str, _exact: bool, activator: u64) -> i32 {
        if state.is_empty() {
            return 0;
        }
        let label = state.rsplit('.').next().unwrap_or(state);
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(class) = self.world.actors.get(id).map(|a| a.class) else {
            return 0;
        };
        i32::from(self.world.states.find_label(class, label).is_some())
    }

    fn check_class(&self, class_name: &str) -> i32 {
        if class_name.is_empty() {
            return 0;
        }
        i32::from(self.world.classes.find_name(class_name).is_some())
    }

    fn damage_actor(
        &mut self,
        target_tid: i32,
        inflictor_tid: i32,
        amount: i32,
        _dtype: &str,
        activator: u64,
    ) -> i32 {
        let Some(target) = resolve_actor(self.world, target_tid, activator) else {
            return 0;
        };
        let inflictor = resolve_actor(self.world, inflictor_tid, activator);
        crate::damage::damage_mobj(
            self.world,
            target,
            inflictor,
            unpack_actor(activator),
            amount,
        )
    }

    fn play_actor_sound(
        &mut self,
        tid: i32,
        sound_type: i32,
        volume: i32,
        attenuation: i32,
        activator: u64,
    ) {
        let vol = Fixed(volume).to_f64().clamp(0.0, 1.0) as f32;
        let atten = Fixed(attenuation).to_f64().max(0.0) as f32;
        for id in collect_tid_ids(self.world, tid, activator) {
            let Some(a) = self.world.actors.get(id) else {
                continue;
            };
            let Some(def) = self.world.classes.get(a.class) else {
                continue;
            };
            let name = match sound_type {
                0 if !a.see_sound.is_empty() => a.see_sound.as_str(),
                0 => def.see_sound.as_str(),
                1 if !a.attack_sound.is_empty() => a.attack_sound.as_str(),
                1 => def.attack_sound.as_str(),
                2 if !a.pain_sound.is_empty() => a.pain_sound.as_str(),
                2 => def.pain_sound.as_str(),
                3 if !a.death_sound.is_empty() => a.death_sound.as_str(),
                3 => def.death_sound.as_str(),
                4 if !a.active_sound.is_empty() => a.active_sound.as_str(),
                4 => def.active_sound.as_str(),
                8 => def.crush_pain_sound.as_str(),
                _ => "",
            };
            if name.is_empty() {
                continue;
            }
            let name = name.to_owned();
            let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
            self.world.play_sound_attn(&name, x, y, z, vol, atten);
        }
    }

    fn set_sky_scroll_speed(&mut self, sky: i32, speed: i32) {
        let s = Fixed(speed).to_f64();
        match sky {
            1 => self.world.sky_speed1 = s,
            2 => self.world.sky_speed2 = s,
            3 => self.world.sky_mist_speed = s,
            _ => {}
        }
    }

    fn set_user_var(&mut self, tid: i32, name: &str, value: i32, activator: u64) -> i32 {
        if name.is_empty() {
            return 0;
        }
        let ids = collect_tid_ids(self.world, tid, activator);
        let mut n = 0i32;
        for id in ids {
            if let Some(a) = self.world.actors.get_mut(id) {
                a.user_vars.insert(name.to_owned(), value);
                n += 1;
            }
        }
        n
    }

    fn get_user_var(&self, tid: i32, name: &str, activator: u64) -> i32 {
        resolve_actor(self.world, tid, activator)
            .and_then(|id| self.world.actors.get(id))
            .and_then(|a| a.user_vars.get(name).copied())
            .unwrap_or(0)
    }

    fn set_user_array(
        &mut self,
        tid: i32,
        name: &str,
        index: i32,
        value: i32,
        activator: u64,
    ) -> i32 {
        if name.is_empty() {
            return 0;
        }
        let ids = collect_tid_ids(self.world, tid, activator);
        let mut n = 0i32;
        for id in ids {
            if let Some(a) = self.world.actors.get_mut(id) {
                a.user_arrays
                    .entry(name.to_owned())
                    .or_default()
                    .insert(index, value);
                n += 1;
            }
        }
        n
    }

    fn get_user_array(&self, tid: i32, name: &str, index: i32, activator: u64) -> i32 {
        resolve_actor(self.world, tid, activator)
            .and_then(|id| self.world.actors.get(id))
            .and_then(|a| a.user_arrays.get(name)?.get(&index).copied())
            .unwrap_or(0)
    }

    fn check_actor_class(&self, tid: i32, class_name: &str, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        let Some(def) = self.world.classes.get(a.class) else {
            return 0;
        };
        i32::from(def.name.eq_ignore_ascii_case(class_name))
    }

    fn actor_class_name(&self, tid: i32, activator: u64) -> String {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return String::new();
        };
        let Some(a) = self.world.actors.get(id) else {
            return String::new();
        };
        self.world
            .classes
            .get(a.class)
            .map(|d| d.name.clone())
            .unwrap_or_default()
    }

    fn actor_species(&self, tid: i32, activator: u64) -> String {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return String::new();
        };
        let Some(a) = self.world.actors.get(id) else {
            return String::new();
        };
        if !a.species.is_empty() {
            return a.species.clone();
        }
        self.world
            .classes
            .get(a.class)
            .map(|c| c.species.clone())
            .unwrap_or_default()
    }

    fn set_actor_species(&mut self, tid: i32, species: &str, activator: u64) {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return;
        };
        let Some(a) = self.world.actors.get_mut(id) else {
            return;
        };
        if species.is_empty() || species.eq_ignore_ascii_case("None") {
            a.species.clear();
        } else {
            a.species = species.to_owned();
        }
    }

    fn actor_nametag(&self, tid: i32, activator: u64) -> String {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return String::new();
        };
        let Some(a) = self.world.actors.get(id) else {
            return String::new();
        };
        if !a.nametag.is_empty() {
            return a.nametag.clone();
        }
        self.world
            .classes
            .get(a.class)
            .map(|c| {
                if c.tag.is_empty() {
                    c.name.clone()
                } else {
                    c.tag.clone()
                }
            })
            .unwrap_or_default()
    }

    fn set_actor_nametag(&mut self, tid: i32, tag: &str, activator: u64) {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return;
        };
        let Some(a) = self.world.actors.get_mut(id) else {
            return;
        };
        a.nametag = tag.to_owned();
    }

    fn actor_damage_type(&self, tid: i32, activator: u64) -> String {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return String::new();
        };
        let Some(a) = self.world.actors.get(id) else {
            return String::new();
        };
        if !a.damage_type.is_empty() {
            return a.damage_type.clone();
        }
        self.world
            .classes
            .get(a.class)
            .map(|c| c.damage_type.clone())
            .unwrap_or_default()
    }

    fn set_actor_damage_type(&mut self, tid: i32, dtype: &str, activator: u64) {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return;
        };
        let Some(a) = self.world.actors.get_mut(id) else {
            return;
        };
        if dtype.is_empty() || dtype.eq_ignore_ascii_case("None") {
            a.damage_type.clear();
        } else {
            a.damage_type = dtype.to_owned();
        }
    }

    fn actor_waterlevel(&self, tid: i32, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        crate::physics::actor_waterlevel(self.world, a)
    }

    fn actor_sound_class(&self, tid: i32, activator: u64) -> String {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return String::new();
        };
        let Some(a) = self.world.actors.get(id) else {
            return String::new();
        };
        if !a.sound_class.is_empty() {
            return a.sound_class.clone();
        }
        self.world
            .classes
            .get(a.class)
            .map(|c| {
                if c.sound_class.is_empty() {
                    c.name.clone()
                } else {
                    c.sound_class.clone()
                }
            })
            .unwrap_or_default()
    }

    fn set_actor_sound_class(&mut self, tid: i32, class: &str, activator: u64) {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return;
        };
        let Some(a) = self.world.actors.get_mut(id) else {
            return;
        };
        a.sound_class = class.to_owned();
    }

    fn set_actor_sound_prop(&mut self, tid: i32, prop: i32, sound: &str, activator: u64) {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return;
        };
        let Some(a) = self.world.actors.get_mut(id) else {
            return;
        };
        let s = sound.to_owned();
        match prop {
            aprop::SEESOUND => a.see_sound = s,
            aprop::ATTACKSOUND => a.attack_sound = s,
            aprop::PAINSOUND => a.pain_sound = s,
            aprop::DEATHSOUND => a.death_sound = s,
            aprop::ACTIVESOUND => a.active_sound = s,
            _ => {}
        }
    }

    fn actor_floor_texture(&self, tid: i32, activator: u64) -> String {
        actor_plane_texture(self.world, tid, activator, false)
    }

    fn actor_floor_terrain(&self, tid: i32, activator: u64) -> String {
        let flat = self.actor_floor_texture(tid, activator);
        if flat.is_empty() {
            return String::new();
        }
        let key = flat.to_ascii_uppercase();
        self.world
            .terrain
            .floors
            .get(&key)
            .cloned()
            .or_else(|| self.world.terrain.default_terrain.clone())
            .unwrap_or_default()
    }

    fn weapon_class_name(&self, activator: u64) -> String {
        let mo = unpack_actor(activator)
            .or_else(|| self.world.players.iter().flatten().find_map(|p| p.mo));
        let Some(mo) = mo else {
            return String::new();
        };
        let Some(slot) = self
            .world
            .players
            .iter()
            .flatten()
            .find(|p| p.mo == Some(mo))
        else {
            return String::new();
        };
        let Some(wid) = slot.ready_weapon else {
            return String::new();
        };
        self.world
            .classes
            .get(wid)
            .map(|d| d.name.clone())
            .unwrap_or_default()
    }

    fn check_proximity(
        &self,
        tid: i32,
        class_name: &str,
        distance: i32,
        count: i32,
        activator: u64,
    ) -> i32 {
        if class_name.is_empty() {
            return 0;
        }
        let Some(want) = self.world.classes.find_name(class_name) else {
            return 0;
        };
        let Some(origin_id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(origin) = self.world.actors.get(origin_id) else {
            return 0;
        };
        let dist = Fixed(distance).to_f64();
        let d2 = dist * dist;
        let need = count.max(1);
        let mut n = 0i32;
        for &id in &self.world.thinkers.default {
            if id == origin_id {
                continue;
            }
            let Some(o) = self.world.actors.get(id) else {
                continue;
            };
            if o.destroyed || o.health <= 0 || o.class != want {
                continue;
            }
            let dx = o.pos.x() - origin.pos.x();
            let dy = o.pos.y() - origin.pos.y();
            let dz = o.pos.z() - origin.pos.z();
            if dx * dx + dy * dy + dz * dz <= d2 {
                n += 1;
                if n >= need {
                    return 1;
                }
            }
        }
        0
    }

    fn drop_item(
        &mut self,
        tid: i32,
        class_name: &str,
        amount: i32,
        chance: i32,
        activator: u64,
    ) -> i32 {
        if class_name.is_empty() {
            return 0;
        }
        // GZDoom: chance is 0..=255 style; 256 = always.
        if chance < 256 {
            let roll = i32::from(self.world.p_random("acs_dropitem"));
            if roll >= chance {
                return 0;
            }
        }
        let Some(drop_class) = self.world.classes.find_name(class_name) else {
            return 0;
        };
        let Some(src_id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(src) = self.world.actors.get(src_id).cloned() else {
            return 0;
        };
        let n = amount.clamp(1, 16);
        let mut spawned = 0i32;
        for _ in 0..n {
            if let Ok(id) = self.world.spawn_at(drop_class, src.pos, src.angle) {
                if let Some(a) = self.world.actors.get_mut(id) {
                    a.flags |= crate::class::FLAG_DROPPED;
                }
                spawned += 1;
            }
        }
        spawned
    }

    fn check_flag(&self, tid: i32, flag: &str, activator: u64) -> i32 {
        let Some(bit) = flag_bit(flag) else {
            return 0;
        };
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        i32::from((a.flags & bit) != 0)
    }

    fn set_actor_flag(&mut self, tid: i32, flag: &str, value: i32, activator: u64) -> i32 {
        let Some(bit) = flag_bit(flag) else {
            return 0;
        };
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get_mut(id) else {
            return 0;
        };
        if value != 0 {
            a.flags |= bit;
        } else {
            a.flags &= !bit;
        }
        1
    }

    fn actor_view_height(&self, tid: i32, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        for slot in self.world.players.iter().flatten() {
            if slot.mo == Some(id) {
                return Fixed::from_f64(slot.view_height).0;
            }
        }
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        let vh = self
            .world
            .classes
            .get(a.class)
            .map(|c| c.view_height)
            .filter(|&v| v > 0.0)
            .unwrap_or(0.0);
        Fixed::from_f64(vh).0
    }

    fn set_camera_to_texture(&mut self, tid: i32, texture: &str, fov: i32, activator: u64) {
        if texture.is_empty() {
            return;
        }
        let camera = if tid != 0 {
            self.world.find_by_tid(tid)
        } else {
            unpack_actor(activator)
        };
        let Some(camera) = camera else {
            return;
        };
        self.world.camera_textures.insert(
            texture.to_ascii_uppercase(),
            crate::world::CameraTextureBind {
                camera,
                fov: f64::from(fov).abs().max(1.0),
            },
        );
    }

    fn change_flat(&mut self, tag: i32, texture: &str, ceiling: bool) {
        if tag == 0 || texture.is_empty() {
            return;
        }
        let name = texture.to_ascii_uppercase();
        let secs: Vec<u32> = self
            .world
            .map
            .sectors
            .iter()
            .enumerate()
            .filter(|(_, s)| s.tags.contains(&tag))
            .map(|(i, _)| i as u32)
            .collect();
        for sid in secs {
            if ceiling {
                self.world.sector_ceiling_pics.insert(sid, name.clone());
            } else {
                self.world.sector_floor_pics.insert(sid, name.clone());
            }
        }
    }

    fn set_line_blocking(&mut self, line_id: i32, setting: i32) {
        use crate::world::{
            ML_BLOCK_PLAYERS, ML_BLOCKEVERYTHING, ML_BLOCKING, ML_RAILING, block_mode,
        };
        let mask = ML_BLOCKING | ML_BLOCKEVERYTHING | ML_RAILING | ML_BLOCK_PLAYERS;
        for li in lines_for_acs_id(self.world, line_id) {
            let base = self
                .world
                .map
                .lines
                .get(li as usize)
                .map(crate::world::base_line_flags)
                .unwrap_or(0);
            let mut flags = self.world.line_flags.get(&li).copied().unwrap_or(base);
            flags &= !mask;
            match setting {
                block_mode::NOTHING => {}
                block_mode::EVERYTHING => flags |= ML_BLOCKING | ML_BLOCKEVERYTHING,
                block_mode::RAILING => flags |= ML_BLOCKING | ML_RAILING,
                block_mode::PLAYERS => flags |= ML_BLOCK_PLAYERS,
                block_mode::CREATURES | _ => flags |= ML_BLOCKING,
            }
            self.world.line_flags.insert(li, flags);
        }
    }

    fn set_line_special(&mut self, line_id: i32, special: i32, args: [i32; 5]) {
        let special = special.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        for li in lines_for_acs_id(self.world, line_id) {
            self.world.line_specials.insert(li, special);
            self.world.line_args.insert(li, args);
        }
    }

    fn sound_sequence(&mut self, name: &str) {
        if crate::sndseq::start_sequence(self.world, name, 0.0, 0.0, 0.0) {
            return;
        }
        // Unknown sequence — fall back to ambient cue by name.
        self.ambient_sound(name, 127);
    }

    fn sound_sequence_on_actor(&mut self, tid: i32, name: &str, activator: u64) {
        let ids = collect_tid_ids(self.world, tid, activator);
        if ids.is_empty() {
            if !crate::sndseq::start_sequence(self.world, name, 0.0, 0.0, 0.0) {
                self.thing_sound(tid, name, 127, activator);
            }
            return;
        }
        for id in ids {
            if let Some(a) = self.world.actors.get(id) {
                let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
                if !crate::sndseq::start_sequence(self.world, name, x, y, z) {
                    self.world.play_sound(name, x, y, z, 1.0);
                }
            }
        }
    }

    fn sound_sequence_on_sector(&mut self, tag: i32, name: &str) {
        if name.is_empty() {
            return;
        }
        let Some(si) = first_sector_from_tag(self.world, tag) else {
            if !crate::sndseq::start_sequence(self.world, name, 0.0, 0.0, 0.0) {
                self.ambient_sound(name, 127);
            }
            return;
        };
        let z = self.world.sectors.get(si).map(|s| s.floor_z).unwrap_or(0.0);
        // Rough centre: first linedef midpoint of the sector.
        let mut x = 0.0;
        let mut y = 0.0;
        for line in &self.world.map.lines {
            let front = line.front_sector.map(|s| s.raw() as usize) == Some(si);
            let back = line.back_sector.map(|s| s.raw() as usize) == Some(si);
            if !(front || back) {
                continue;
            }
            if let (Some(v1), Some(v2)) = (
                self.world.map.vertices.get(line.v1.raw() as usize),
                self.world.map.vertices.get(line.v2.raw() as usize),
            ) {
                x = 0.5 * (v1.pos.x() + v2.pos.x());
                y = 0.5 * (v1.pos.y() + v2.pos.y());
                break;
            }
        }
        if !crate::sndseq::start_sequence(self.world, name, x, y, z) {
            self.world.play_sound(name, x, y, z, 1.0);
        }
    }

    fn sound_sequence_on_polyobj(&mut self, po: i32, name: &str) {
        if name.is_empty() {
            return;
        }
        if let Some(p) = self.world.polyobjs.iter().find(|p| p.id == po) {
            let s = p.start_spot;
            if !crate::sndseq::start_sequence(self.world, name, s.x(), s.y(), 0.0) {
                self.world.play_sound(name, s.x(), s.y(), 0.0, 1.0);
            }
        } else if !crate::sndseq::start_sequence(self.world, name, 0.0, 0.0, 0.0) {
            self.ambient_sound(name, 127);
        }
    }

    fn can_raise_actor(&self, tid: i32, activator: u64) -> i32 {
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let Some(a) = self.world.actors.get(id) else {
            return 0;
        };
        // Bootstrap: corpses (or non-positive health) can be raised.
        i32::from(!a.destroyed && ((a.flags & crate::class::FLAG_CORPSE) != 0 || a.health <= 0))
    }

    fn set_music_volume(&mut self, volume: i32) {
        self.world.music_volume = Fixed(volume).to_f64().clamp(0.0, 1.0) as f32;
    }

    fn set_translation_acs(&mut self, tid: i32, name: &str, activator: u64) {
        let tr = if name.eq_ignore_ascii_case("None") || name.is_empty() {
            String::new()
        } else {
            name.to_owned()
        };
        for id in collect_tid_ids(self.world, tid, activator) {
            if let Some(a) = self.world.actors.get_mut(id) {
                a.translation = tr.clone();
            }
        }
    }

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
        let Some(source) = resolve_actor(self.world, tid, activator) else {
            return 0;
        };
        let range = Fixed(distance).to_f64().abs().max(1.0);
        let Some(picked) =
            crate::hitscan::line_pick_actor(self.world, source, acs_to_angle(angle), range)
        else {
            return 0;
        };
        if new_tid != 0 {
            if let Some(a) = self.world.actors.get_mut(picked) {
                if a.tid == 0 || return_tid {
                    a.tid = new_tid;
                }
            }
        }
        if return_tid {
            self.world.actors.get(picked).map(|a| a.tid).unwrap_or(0)
        } else {
            1
        }
    }

    fn is_pointer_equal(&self, tid1: i32, tid2: i32, activator: u64) -> i32 {
        let a = resolve_actor(self.world, tid1, activator);
        let b = resolve_actor(self.world, tid2, activator);
        i32::from(a.is_some() && a == b)
    }

    fn set_actor_tele_fog(&mut self, tid: i32, source: &str, dest: &str, activator: u64) {
        for id in collect_tid_ids(self.world, tid, activator) {
            if let Some(a) = self.world.actors.get_mut(id) {
                a.telefog_source = source.to_owned();
                a.telefog_dest = dest.to_owned();
            }
        }
    }

    fn swap_actor_tele_fog(&mut self, tid: i32, activator: u64) -> i32 {
        let mut n = 0i32;
        for id in collect_tid_ids(self.world, tid, activator) {
            if let Some(a) = self.world.actors.get_mut(id) {
                std::mem::swap(&mut a.telefog_source, &mut a.telefog_dest);
                n += 1;
            }
        }
        n
    }

    fn set_sector_damage(&mut self, tag: i32, amount: i32, dtype: &str, interval: i32, leaky: i32) {
        let iv = interval.max(1);
        let leak = leaky.clamp(0, 256);
        for (i, sec) in self.world.map.sectors.iter().enumerate() {
            if !sec.tags.contains(&tag) {
                continue;
            }
            if let Some(dyn_s) = self.world.sectors.get_mut(i) {
                dyn_s.damage_amount = amount;
                dyn_s.damage_type = dtype.to_owned();
                dyn_s.damage_interval = iv;
                dyn_s.leakiness = leak;
            }
        }
    }

    fn set_sector_terrain(&mut self, tag: i32, position: i32, terrain: &str) {
        if terrain.is_empty() {
            return;
        }
        let name = terrain.to_ascii_uppercase();
        let ceiling = position != 0;
        for (i, sec) in self.world.map.sectors.iter().enumerate() {
            if !sec.tags.contains(&tag) {
                continue;
            }
            if ceiling {
                let flat = self
                    .world
                    .sector_ceiling_pics
                    .get(&(i as u32))
                    .cloned()
                    .unwrap_or_else(|| sec.ceiling_pic.to_ascii_uppercase());
                self.world.terrain.ceilings.insert(flat, name.clone());
            } else {
                let flat = self
                    .world
                    .sector_floor_pics
                    .get(&(i as u32))
                    .cloned()
                    .unwrap_or_else(|| sec.floor_pic.to_ascii_uppercase());
                self.world.terrain.floors.insert(flat, name.clone());
            }
        }
    }

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
        let Some(self_id) = unpack_actor(activator) else {
            return 0;
        };
        let Some(dest_id) = resolve_actor(self.world, dest_tid, activator) else {
            return 0;
        };
        if dest_id == self_id {
            return 0;
        }
        let Some(dest) = self.world.actors.get(dest_id).cloned() else {
            return 0;
        };
        let ox = Fixed(xofs).to_f64();
        let oy = Fixed(yofs).to_f64();
        let oz = Fixed(zofs).to_f64();
        let rad = dest.angle.radians();
        let c = room_core::dmath::cos(rad);
        let s = room_core::dmath::sin(rad);
        let pos = DVec3::new(
            dest.pos.x() + ox * c - oy * s,
            dest.pos.y() + ox * s + oy * c,
            dest.pos.z() + oz,
        );
        let ang = if angle != 0 {
            Angle::from_degrees(dest.angle.degrees() + acs_to_angle(angle).degrees())
        } else {
            dest.angle
        };
        if let Some(a) = self.world.actors.get_mut(self_id) {
            a.pos = pos;
            a.angle = ang;
        }
        1
    }

    fn set_line_activation(&mut self, line_id: i32, activation: i32, repeat: i32) {
        for li in lines_for_acs_id(self.world, line_id) {
            self.world
                .line_activations
                .insert(li, activation.max(0) as u32);
            if repeat < 0 {
                continue;
            }
            let base = self
                .world
                .map
                .lines
                .get(li as usize)
                .map(crate::world::base_line_flags)
                .unwrap_or(0);
            let cur = self.world.line_flags.get(&li).copied().unwrap_or(base);
            let next = if repeat > 0 {
                cur | crate::world::ML_REPEAT_SPECIAL
            } else {
                cur & !crate::world::ML_REPEAT_SPECIAL
            };
            self.world.line_flags.insert(li, next);
        }
    }

    fn get_line_activation(&self, line_id: i32) -> i32 {
        let Some(li) = lines_for_acs_id(self.world, line_id).into_iter().next() else {
            return 0;
        };
        self.world.line_activation(room_core::LineId::from_raw(li)) as i32
    }

    fn set_line_texture(&mut self, line_id: i32, side: i32, position: i32, texture: &str) {
        if texture.is_empty() {
            return;
        }
        let name = texture.to_ascii_uppercase();
        for li in lines_for_acs_id(self.world, line_id) {
            let Some(line) = self.world.map.lines.get(li as usize) else {
                continue;
            };
            let side_i = if side != 0 { 1usize } else { 0usize };
            let Some(sid) = line.sides.get(side_i).copied().flatten() else {
                continue;
            };
            let si = sid.raw();
            match position {
                0 => {
                    self.world.switch_uppers.insert(si, name.clone());
                }
                2 => {
                    self.world.switch_lowers.insert(si, name.clone());
                }
                _ => {
                    self.world.switch_mids.insert(si, name.clone());
                }
            }
        }
    }

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
        if duration <= 0 {
            return 0;
        }
        let spot = if tid != 0 {
            self.world.find_by_tid(tid)
        } else {
            unpack_actor(activator)
        };
        let Some(spot) = spot else {
            return 0;
        };
        let snd = if sound.is_empty() {
            "world/quake".to_owned()
        } else {
            sound.to_owned()
        };
        self.world.quakes.push(crate::world::Quake {
            spot,
            intensity: f64::from(intensity).clamp(0.0, 9.0),
            countdown: duration,
            damage_radius: f64::from(damrad).abs(),
            tremor_radius: f64::from(tremrad).abs(),
            sound: snd,
            thrust_multiplier: 0.5,
        });
        1
    }

    fn spawn_spot(
        &mut self,
        type_id: i32,
        spot_tid: i32,
        tid: i32,
        angle: i32,
        activator: u64,
    ) -> i32 {
        spawn_at_spots(self.world, type_id, spot_tid, tid, activator, Some(angle))
    }

    fn spawn_spot_facing(&mut self, type_id: i32, spot_tid: i32, tid: i32, activator: u64) -> i32 {
        spawn_at_spots(self.world, type_id, spot_tid, tid, activator, None)
    }

    fn classify_actor(&self, tid: i32, activator: u64) -> i32 {
        const ACTOR_NONE: i32 = 0;
        const ACTOR_WORLD: i32 = 1;
        const ACTOR_PLAYER: i32 = 2;
        const ACTOR_OBSTACLE: i32 = 4;
        const ACTOR_MONSTER: i32 = 8;
        const ACTOR_MISSILE: i32 = 16;
        const ACTOR_ALIVE: i32 = 64;
        const ACTOR_DEAD: i32 = 128;
        if tid == 0 && activator == 0 {
            return ACTOR_WORLD;
        }
        let Some(id) = resolve_actor(self.world, tid, activator) else {
            return ACTOR_NONE;
        };
        let Some(a) = self.world.actors.get(id) else {
            return ACTOR_NONE;
        };
        let mut bits = 0i32;
        if self
            .world
            .players
            .iter()
            .flatten()
            .any(|p| p.mo == Some(id))
        {
            bits |= ACTOR_PLAYER;
        } else if (a.flags & crate::class::FLAG_MISSILE) != 0 {
            bits |= ACTOR_MISSILE;
        } else if (a.flags & crate::class::FLAG_COUNTKILL) != 0 {
            bits |= ACTOR_MONSTER;
        } else if (a.flags & crate::class::FLAG_SOLID) != 0 {
            bits |= ACTOR_OBSTACLE;
        }
        if a.health > 0 && !a.destroyed {
            bits |= ACTOR_ALIVE;
        } else {
            bits |= ACTOR_DEAD;
        }
        bits
    }
}

fn actor_plane_texture(world: &World, tid: i32, activator: u64, ceiling: bool) -> String {
    let Some(id) = resolve_actor(world, tid, activator) else {
        return String::new();
    };
    let Some(a) = world.actors.get(id) else {
        return String::new();
    };
    let si = match point_in_sector(&world.map, a.pos.x(), a.pos.y())
        .or_else(|| point_in_sector_vanilla(&world.map, a.pos.x(), a.pos.y()))
    {
        Some(si) => si,
        None if world.map.sectors.len() == 1 => room_core::ids::SectorId::from_raw(0),
        None => return String::new(),
    };
    let idx = si.raw();
    if ceiling {
        if let Some(name) = world.sector_ceiling_pics.get(&idx) {
            return name.clone();
        }
        world
            .map
            .sectors
            .get(idx as usize)
            .map(|s| s.ceiling_pic.clone())
            .unwrap_or_default()
    } else if let Some(name) = world.sector_floor_pics.get(&idx) {
        name.clone()
    } else {
        world
            .map
            .sectors
            .get(idx as usize)
            .map(|s| s.floor_pic.clone())
            .unwrap_or_default()
    }
}

/// Linedefs matching ACS line id (tag) or raw index fallback.
/// Point on linedef: `v1 + delta * frac + normal * offset` (frac/offset fixed).
fn line_point_acs(world: &World, line_id: i32, frac: i32, offset: i32, want_x: bool) -> i32 {
    let Some(li) = lines_for_acs_id(world, line_id).into_iter().next() else {
        return 0;
    };
    let Some(line) = world.map.lines.get(li as usize) else {
        return 0;
    };
    let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
        return 0;
    };
    let t = Fixed(frac).to_f64();
    let off = Fixed(offset).to_f64();
    let dx = line.delta.x();
    let dy = line.delta.y();
    let len = (dx * dx + dy * dy).sqrt();
    let (nx, ny) = if len > 1e-9 {
        (dy / len, -dx / len)
    } else {
        (0.0, 0.0)
    };
    let x = v1.pos.x() + dx * t + nx * off;
    let y = v1.pos.y() + dy * t + ny * off;
    Fixed::from_f64(if want_x { x } else { y }).0
}

fn lines_for_acs_id(world: &World, line_id: i32) -> Vec<u32> {
    let mut idxs: Vec<u32> = world
        .map
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.has_id(line_id))
        .map(|(i, _)| i as u32)
        .collect();
    if idxs.is_empty() && line_id >= 0 && (line_id as usize) < world.map.lines.len() {
        idxs.push(line_id as u32);
    }
    idxs
}

fn spawn_at_spots(
    world: &mut World,
    type_id: i32,
    spot_tid: i32,
    tid: i32,
    activator: u64,
    fixed_angle: Option<i32>,
) -> i32 {
    let Some(class) = world.classes.find_doomednum(type_id) else {
        return 0;
    };
    let spots: Vec<(DVec3, Angle)> = if spot_tid != 0 {
        world
            .thinkers
            .default
            .iter()
            .copied()
            .filter_map(|id| {
                let a = world.actors.get(id)?;
                if a.destroyed || a.tid != spot_tid {
                    return None;
                }
                let ang = fixed_angle.map_or(a.angle, byte_angle);
                Some((a.pos, ang))
            })
            .collect()
    } else if let Some(id) = unpack_actor(activator) {
        world
            .actors
            .get(id)
            .filter(|a| !a.destroyed)
            .map(|a| {
                let ang = fixed_angle.map_or(a.angle, byte_angle);
                vec![(a.pos, ang)]
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut n = 0;
    for (pos, ang) in spots {
        if let Ok(id) = world.spawn_at(class, pos, ang) {
            if let Some(a) = world.actors.get_mut(id) {
                a.tid = tid;
            }
            n += 1;
        }
    }
    n
}

fn byte_angle(byte: i32) -> Angle {
    let deg = f64::from(byte.rem_euclid(256)) * (360.0 / 256.0);
    Angle::from_degrees(deg)
}

fn acs_volume(volume: i32) -> f32 {
    (f32::from(volume.clamp(0, 127) as u8) / 127.0).clamp(0.0, 1.0)
}

fn acs_check_sight(world: &World, tid1: i32, tid2: i32, activator: u64) -> bool {
    if tid1 == 0 && tid2 == 0 {
        return true;
    }
    let sources: Vec<ActorId> = if tid1 == 0 {
        unpack_actor(activator).into_iter().collect()
    } else {
        world
            .thinkers
            .default
            .iter()
            .copied()
            .filter(|&id| {
                world
                    .actors
                    .get(id)
                    .is_some_and(|a| !a.destroyed && a.tid == tid1)
            })
            .collect()
    };
    let dests: Vec<ActorId> = if tid2 == 0 {
        unpack_actor(activator).into_iter().collect()
    } else {
        world
            .thinkers
            .default
            .iter()
            .copied()
            .filter(|&id| {
                world
                    .actors
                    .get(id)
                    .is_some_and(|a| !a.destroyed && a.tid == tid2)
            })
            .collect()
    };
    for &src in &sources {
        for &dst in &dests {
            if crate::sight::check_sight(world, src, dst) {
                return true;
            }
        }
    }
    false
}

/// Built-in UDMF keys from linked geometry (+ `user_*` from [`MapGeometry::udmf_user`]).
fn udmf_builtin_int(world: &World, kind: u8, id: i32, key: &str) -> i32 {
    let key = key.to_ascii_lowercase();
    match kind {
        0 => {
            // Line by id/tag.
            let Some((idx, line)) = world
                .map
                .lines
                .iter()
                .enumerate()
                .find(|(_, l)| l.has_id(id))
            else {
                return 0;
            };
            match key.as_str() {
                "special" => i32::from(line.special),
                "id" => i32::from(line.tag),
                "arg0" => line.args[0],
                "arg1" => line.args[1],
                "arg2" => line.args[2],
                "arg3" => line.args[3],
                "arg4" => line.args[4],
                "blocking" => i32::from(line.flags & 1 != 0),
                "blockmonsters" => i32::from(line.flags & 2 != 0),
                "twosided" => i32::from(line.flags & 4 != 0),
                "dontpegtop" => i32::from(line.flags & 8 != 0),
                "dontpegbottom" => i32::from(line.flags & 16 != 0),
                "secret" => i32::from(line.flags & 32 != 0),
                "blocksound" => i32::from(line.flags & 64 != 0),
                "dontdraw" => i32::from(line.flags & 128 != 0),
                "mapped" => i32::from(line.flags & 256 != 0),
                "blockplayers" => {
                    i32::from((i32::from(line.flags) & crate::world::ML_BLOCK_PLAYERS) != 0)
                }
                "blockeverything" => {
                    i32::from((i32::from(line.flags) & crate::world::ML_BLOCKEVERYTHING) != 0)
                }
                "locknumber" => line.lock_number,
                "midtex3d" => i32::from(line.midtex3d),
                "midtex3dimpassible" => i32::from(line.midtex3d_impassible),
                "checkswitchrange" => i32::from(line.check_switch_range),
                "firstsideonly" => i32::from(line.first_side_only),
                "blockfloaters" => i32::from(line.block_floaters),
                "blockprojectiles" => i32::from(line.block_projectiles),
                "blocksight" => i32::from(line.block_sight),
                "blockhitscan" => i32::from(line.block_hitscan),
                "health" => world
                    .line_health
                    .get(&(idx as u32))
                    .copied()
                    .unwrap_or(line.health),
                "automapstyle" => world
                    .line_automap_style
                    .get(&(idx as u32))
                    .copied()
                    .unwrap_or(line.automap_style),
                "clipmidtex" => i32::from(line.clip_midtex),
                "wrapmidtex" => i32::from(line.wrap_midtex),
                "healthgroup" => line.health_group,
                "playercross" | "playeruse" | "monstercross" | "impact" | "playerpush"
                | "missilecross" | "monsteruse" | "monsterpush" | "anycross" | "playeruseback"
                | "damagespecial" | "deathspecial" => {
                    let act = world.line_activation(room_core::ids::LineId::from_raw(idx as u32));
                    let bit = match key.as_str() {
                        "playercross" => room_map::spac::CROSS,
                        "playeruse" => room_map::spac::USE,
                        "monstercross" => room_map::spac::MCROSS,
                        "impact" => room_map::spac::IMPACT,
                        "playerpush" => room_map::spac::PUSH,
                        "missilecross" => room_map::spac::PCROSS,
                        "monsteruse" => room_map::spac::MUSE,
                        "monsterpush" => room_map::spac::MPUSH,
                        "anycross" => room_map::spac::ANYCROSS,
                        "playeruseback" => room_map::spac::USEBACK,
                        "damagespecial" => room_map::spac::DAMAGE,
                        "deathspecial" => room_map::spac::DEATH,
                        _ => 0,
                    };
                    i32::from((act & bit) != 0)
                }
                "repeatspecial" => i32::from(
                    (world.line_flags(room_core::ids::LineId::from_raw(idx as u32))
                        & crate::world::ML_REPEAT_SPECIAL)
                        != 0,
                ),
                "alpha" => {
                    // Integer opacity 0..=255 (matches TranslucentLine amount).
                    (f64::from(line.mid_alpha) * 255.0).round() as i32
                }
                // ACS STYLE_* constants: Normal=0, Add=65 (same as APROP_RenderStyle).
                "renderstyle" => {
                    if line.mid_additive {
                        65
                    } else {
                        0
                    }
                }
                k if k.starts_with("user_") => world
                    .map
                    .udmf_user
                    .lines
                    .get(idx)
                    .and_then(|u| u.ints.get(k).copied())
                    .unwrap_or(0),
                _ => 0,
            }
        }
        1 => {
            // Thing by TID.
            let Some((idx, th)) = world
                .map
                .things
                .iter()
                .enumerate()
                .find(|(_, t)| i32::from(t.tid) == id)
            else {
                return 0;
            };
            match key.as_str() {
                "type" | "doomednum" => i32::from(th.type_id),
                "angle" => i32::from(th.angle),
                "id" | "tid" => i32::from(th.tid),
                "health" => th.health,
                "score" => {
                    if th.score == i32::MIN {
                        0
                    } else {
                        th.score
                    }
                }
                "friendlyseeblocks" => i32::from(th.friendly_see_blocks),
                "fillcolor" => th.fill_color.max(0),
                "floatbobphase" => i32::from(th.float_bob_phase),
                "renderstyle" => crate::class::RenderStyle::parse(&th.render_style)
                    .map(|r| r.to_acs_style())
                    .unwrap_or(0),
                "countsecret" => i32::from(th.count_secret),
                "nocount" => i32::from(th.no_count),
                "conversation" => th.conversation,
                "accuracy" => {
                    if th.accuracy == i32::MIN {
                        0
                    } else {
                        th.accuracy
                    }
                }
                "stamina" => {
                    if th.stamina == i32::MIN {
                        0
                    } else {
                        th.stamina
                    }
                }
                "mass" => {
                    if th.mass == i32::MIN {
                        0
                    } else {
                        th.mass
                    }
                }
                "pitch" => {
                    if th.pitch == i16::MIN {
                        0
                    } else {
                        i32::from(th.pitch)
                    }
                }
                "roll" => {
                    if th.roll == i16::MIN {
                        0
                    } else {
                        i32::from(th.roll)
                    }
                }
                "special" => i32::from(th.special),
                "arg0" => th.args[0],
                "arg1" => th.args[1],
                "arg2" => th.args[2],
                "arg3" => th.args[3],
                "arg4" => th.args[4],
                k if k.starts_with("user_") => world
                    .map
                    .udmf_user
                    .things
                    .get(idx)
                    .and_then(|u| u.ints.get(k).copied())
                    .unwrap_or(0),
                _ => 0,
            }
        }
        2 => {
            let Some(si) = first_sector_from_tag(world, id) else {
                return 0;
            };
            let Some(sec) = world.map.sectors.get(si) else {
                return 0;
            };
            let dyn_s = world.sectors.get(si);
            match key.as_str() {
                "lightlevel" => i32::from(dyn_s.map(|s| s.light).unwrap_or(sec.light)),
                "special" => i32::from(sec.special),
                "id" | "tag" => id,
                "heightfloor" => room_core::conv::floor_to_int(
                    dyn_s.map(|s| s.floor_z).unwrap_or(sec.floor_height),
                ),
                "heightceiling" => room_core::conv::floor_to_int(
                    dyn_s.map(|s| s.ceiling_z).unwrap_or(sec.ceiling_height),
                ),
                "friction" => sec.friction,
                "damageamount" => dyn_s.map(|s| s.damage_amount).unwrap_or(sec.damage_amount),
                "damageinterval" => {
                    dyn_s
                        .map(|s| s.damage_interval)
                        .unwrap_or(if sec.damage_interval > 0 {
                            sec.damage_interval
                        } else {
                            32
                        })
                }
                "leakiness" => dyn_s.map(|s| s.leakiness).unwrap_or(sec.leakiness),
                "silent" => i32::from(sec.silent),
                "waterzone" => i32::from(sec.waterzone),
                "dropactors" => i32::from(sec.drop_actors),
                "nofallingdamage" => i32::from(sec.no_falling_damage),
                "lightcolor" => {
                    if sec.light_color >= 0 {
                        sec.light_color
                    } else {
                        dyn_s
                            .map(|s| {
                                i32::from(s.color[0]) << 16
                                    | i32::from(s.color[1]) << 8
                                    | i32::from(s.color[2])
                            })
                            .unwrap_or(0xFF_FF_FF)
                    }
                }
                "fadecolor" => {
                    if sec.fade_color >= 0 {
                        sec.fade_color
                    } else {
                        dyn_s
                            .map(|s| {
                                i32::from(s.fade[0]) << 16
                                    | i32::from(s.fade[1]) << 8
                                    | i32::from(s.fade[2])
                            })
                            .unwrap_or(0)
                    }
                }
                "fogdensity" => {
                    i32::from(dyn_s.map(|s| s.fog_density).unwrap_or(0)).max(sec.fog_density)
                }
                "desaturation" => i32::from(
                    dyn_s
                        .map(|s| s.color_desaturate)
                        .unwrap_or(sec.desaturation.clamp(0, 255) as u8),
                ),
                "color_floor" => sec.color_floor.max(0),
                "color_ceiling" => sec.color_ceiling.max(0),
                "color_walltop" => sec.color_walltop.max(0),
                "color_wallbottom" => sec.color_wallbottom.max(0),
                "color_sprites" => sec.color_sprites.max(0),
                "healthfloor" => dyn_s.map(|s| s.health_floor).unwrap_or(sec.health_floor),
                "healthceiling" => dyn_s
                    .map(|s| s.health_ceiling)
                    .unwrap_or(sec.health_ceiling),
                "health3d" => dyn_s.map(|s| s.health_3d).unwrap_or(sec.health_3d),
                // Integer opacity 0..=255 (matches Sector_SetTranslucent amount).
                "alphafloor" | "alpha" => {
                    let a = dyn_s.map(|s| s.floor_alpha).unwrap_or(sec.floor_alpha);
                    (f64::from(a) * 255.0).round() as i32
                }
                "alphaceiling" => {
                    let a = dyn_s.map(|s| s.ceiling_alpha).unwrap_or(sec.ceiling_alpha);
                    (f64::from(a) * 255.0).round() as i32
                }
                "renderstyle" => {
                    let add = dyn_s
                        .map(|s| s.floor_additive || s.ceiling_additive)
                        .unwrap_or(sec.floor_additive || sec.ceiling_additive);
                    if add { 65 } else { 0 }
                }
                k if k.starts_with("user_") => world
                    .map
                    .udmf_user
                    .sectors
                    .get(si)
                    .and_then(|u| u.ints.get(k).copied())
                    .unwrap_or(0),
                _ => 0,
            }
        }
        3 => {
            let side_sel = (id >> 30) & 1;
            let line_id = id & 0x3fff_ffff;
            let Some(line) = world.map.lines.iter().find(|l| l.has_id(line_id)) else {
                return 0;
            };
            let Some(side_id) = line.sides[side_sel as usize] else {
                return 0;
            };
            let side_idx = side_id.raw() as usize;
            let Some(side) = world.map.sides.get(side_idx) else {
                return 0;
            };
            match key.as_str() {
                "offsetx" | "offsetx_mid" => i32::from(side.x_offset),
                "offsety" | "offsety_mid" => i32::from(side.y_offset),
                "offsetx_top" => i32::from(side.x_offset_top),
                "offsety_top" => i32::from(side.y_offset_top),
                "offsetx_bottom" => i32::from(side.x_offset_bottom),
                "offsety_bottom" => i32::from(side.y_offset_bottom),
                "sector" => side.sector.raw() as i32,
                "light" => udmf_side_light(side.light),
                "lightabsolute" => i32::from(side.light_absolute),
                "light_top" => udmf_side_light(side.light_top),
                "light_mid" => udmf_side_light(side.light_mid),
                "light_bottom" => udmf_side_light(side.light_bottom),
                "lightabsolute_top" => i32::from(side.light_absolute_top),
                "lightabsolute_mid" => i32::from(side.light_absolute_mid),
                "lightabsolute_bottom" => i32::from(side.light_absolute_bottom),
                "nofakecontrast" => i32::from(side.no_fake_contrast),
                "smoothlighting" => i32::from(side.smooth_lighting),
                "clipmidtex" => i32::from(side.clip_midtex),
                "wrapmidtex" => i32::from(side.wrap_midtex),
                "nodecals" => i32::from(side.no_decals),
                k if k.starts_with("user_") => world
                    .map
                    .udmf_user
                    .sides
                    .get(side_idx)
                    .and_then(|u| u.ints.get(k).copied())
                    .unwrap_or(0),
                _ => 0,
            }
        }
        _ => 0,
    }
}

fn udmf_side_light(v: i16) -> i32 {
    if v == i16::MIN { 0 } else { i32::from(v) }
}

fn udmf_user_fixed(world: &World, kind: u8, index: usize, key: &str) -> Option<i32> {
    let bag = match kind {
        0 => world.map.udmf_user.lines.get(index),
        1 => world.map.udmf_user.things.get(index),
        2 => world.map.udmf_user.sectors.get(index),
        3 => world.map.udmf_user.sides.get(index),
        _ => None,
    }?;
    bag.fixed
        .get(key)
        .copied()
        .or_else(|| bag.ints.get(key).map(|v| Fixed::from_int(*v).0))
}

fn udmf_builtin_fixed(world: &World, kind: u8, id: i32, key: &str) -> i32 {
    let key = key.to_ascii_lowercase();
    if key.starts_with("user_") {
        let idx = match kind {
            0 => world.map.lines.iter().position(|l| l.has_id(id)),
            1 => world.map.things.iter().position(|t| i32::from(t.tid) == id),
            2 => first_sector_from_tag(world, id),
            3 => {
                let side_sel = (id >> 30) & 1;
                let line_id = id & 0x3fff_ffff;
                world
                    .map
                    .lines
                    .iter()
                    .find(|l| l.has_id(line_id))
                    .and_then(|l| l.sides[side_sel as usize])
                    .map(|s| s.raw() as usize)
            }
            _ => None,
        };
        if let Some(i) = idx {
            if let Some(v) = udmf_user_fixed(world, kind, i, &key) {
                return v;
            }
        }
        return 0;
    }
    match kind {
        2 => {
            let Some(si) = first_sector_from_tag(world, id) else {
                return 0;
            };
            let Some(sec) = world.map.sectors.get(si) else {
                return 0;
            };
            let dyn_s = world.sectors.get(si);
            match key.as_str() {
                "heightfloor" => {
                    Fixed::from_f64(dyn_s.map(|s| s.floor_z).unwrap_or(sec.floor_height)).0
                }
                "heightceiling" => {
                    Fixed::from_f64(dyn_s.map(|s| s.ceiling_z).unwrap_or(sec.ceiling_height)).0
                }
                "xpanningfloor" => Fixed::from_f64(sec.floor_u_off).0,
                "ypanningfloor" => Fixed::from_f64(sec.floor_v_off).0,
                "xpanningceiling" => Fixed::from_f64(sec.ceiling_u_off).0,
                "ypanningceiling" => Fixed::from_f64(sec.ceiling_v_off).0,
                "rotationfloor" => Fixed::from_f64(sec.floor_angle).0,
                "rotationceiling" => Fixed::from_f64(sec.ceiling_angle).0,
                "xscalefloor" => Fixed::from_f64(sec.floor_scale_u).0,
                "yscalefloor" => Fixed::from_f64(sec.floor_scale_v).0,
                "xscaleceiling" => Fixed::from_f64(sec.ceiling_scale_u).0,
                "yscaleceiling" => Fixed::from_f64(sec.ceiling_scale_v).0,
                "alphafloor" | "alpha" => {
                    Fixed::from_f64(f64::from(
                        dyn_s.map(|s| s.floor_alpha).unwrap_or(sec.floor_alpha),
                    ))
                    .0
                }
                "alphaceiling" => {
                    Fixed::from_f64(f64::from(
                        dyn_s.map(|s| s.ceiling_alpha).unwrap_or(sec.ceiling_alpha),
                    ))
                    .0
                }
                "gravity" => {
                    let g = dyn_s.map(|s| s.gravity).unwrap_or(if sec.gravity >= 0.0 {
                        sec.gravity
                    } else {
                        1.0
                    });
                    Fixed::from_f64(g).0
                }
                _ => Fixed::from_int(udmf_builtin_int(world, kind, id, &key)).0,
            }
        }
        1 => {
            let Some((_, th)) = world
                .map
                .things
                .iter()
                .enumerate()
                .find(|(_, t)| i32::from(t.tid) == id)
            else {
                return 0;
            };
            match key.as_str() {
                "x" => Fixed::from_f64(f64::from(th.x)).0,
                "y" => Fixed::from_f64(f64::from(th.y)).0,
                "height" | "z" => Fixed::from_f64(f64::from(th.z)).0,
                "gravity" => {
                    if th.gravity < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.gravity).0
                    }
                }
                "scale" | "scalex" => {
                    if th.scale_x <= 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.scale_x).0
                    }
                }
                "scaley" => {
                    if th.scale_y <= 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.scale_y).0
                    }
                }
                "alpha" => {
                    if th.alpha < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.alpha).0
                    }
                }
                "meleerange" => {
                    if th.melee_range < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.melee_range).0
                    }
                }
                "viewheight" => {
                    if th.view_height < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.view_height).0
                    }
                }
                "speed" => {
                    if th.speed < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.speed).0
                    }
                }
                "friction" => {
                    if th.friction < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.friction).0
                    }
                }
                "damagefactor" => {
                    if th.damage_factor < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.damage_factor).0
                    }
                }
                "maxstepheight" => {
                    if th.max_step_height < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.max_step_height).0
                    }
                }
                "maxdropoffheight" => {
                    if th.max_dropoff_height < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.max_dropoff_height).0
                    }
                }
                "floatspeed" => {
                    if th.float_speed < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.float_speed).0
                    }
                }
                "fastspeed" => {
                    if th.fast_speed < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.fast_speed).0
                    }
                }
                "jumpz" => {
                    if th.jump_z < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.jump_z).0
                    }
                }
                "attackzoffset" => {
                    if th.attack_z_offset < 0.0 {
                        0
                    } else {
                        Fixed::from_f64(th.attack_z_offset).0
                    }
                }
                _ => Fixed::from_int(udmf_builtin_int(world, kind, id, &key)).0,
            }
        }
        3 => {
            let side_sel = (id >> 30) & 1;
            let line_id = id & 0x3fff_ffff;
            let Some(line) = world.map.lines.iter().find(|l| l.has_id(line_id)) else {
                return 0;
            };
            let Some(side_id) = line.sides[side_sel as usize] else {
                return 0;
            };
            let Some(side) = world.map.sides.get(side_id.raw() as usize) else {
                return 0;
            };
            match key.as_str() {
                "scalex" | "scalex_mid" => Fixed::from_f64(f64::from(side.scale_x)).0,
                "scaley" | "scaley_mid" => Fixed::from_f64(f64::from(side.scale_y)).0,
                "scalex_top" => Fixed::from_f64(f64::from(side.scale_x_top)).0,
                "scaley_top" => Fixed::from_f64(f64::from(side.scale_y_top)).0,
                "scalex_bottom" => Fixed::from_f64(f64::from(side.scale_x_bottom)).0,
                "scaley_bottom" => Fixed::from_f64(f64::from(side.scale_y_bottom)).0,
                "offsetx" | "offsetx_mid" => Fixed::from_int(i32::from(side.x_offset)).0,
                "offsety" | "offsety_mid" => Fixed::from_int(i32::from(side.y_offset)).0,
                "offsetx_top" => Fixed::from_int(i32::from(side.x_offset_top)).0,
                "offsety_top" => Fixed::from_int(i32::from(side.y_offset_top)).0,
                "offsetx_bottom" => Fixed::from_int(i32::from(side.x_offset_bottom)).0,
                "offsety_bottom" => Fixed::from_int(i32::from(side.y_offset_bottom)).0,
                "xscroll" => Fixed::from_f64(f64::from(side.scroll_x)).0,
                "yscroll" => Fixed::from_f64(f64::from(side.scroll_y)).0,
                _ => Fixed::from_int(udmf_builtin_int(world, kind, id, &key)).0,
            }
        }
        0 => {
            if key == "alpha" {
                let Some((_, line)) = world
                    .map
                    .lines
                    .iter()
                    .enumerate()
                    .find(|(_, l)| l.has_id(id))
                else {
                    return 0;
                };
                return Fixed::from_f64(f64::from(line.mid_alpha)).0;
            }
            // Integers as fixed (GZDoom often returns int*FRACUNIT for int-typed keys).
            let v = udmf_builtin_int(world, kind, id, &key);
            Fixed::from_int(v).0
        }
        _ => 0,
    }
}

fn pack_actor(id: ActorId) -> u64 {
    (u64::from(id.generation()) << 32) | u64::from(id.index())
}

fn unpack_actor(key: u64) -> Option<ActorId> {
    if key == 0 {
        return None;
    }
    Some(ActorId::from_raw(
        (key & 0xffff_ffff) as u32,
        (key >> 32) as u32,
    ))
}

fn resolve_actor(world: &World, tid: i32, activator: u64) -> Option<ActorId> {
    if tid == 0 {
        return unpack_actor(activator)
            .filter(|&id| world.actors.get(id).is_some_and(|a| !a.destroyed));
    }
    world.find_by_tid(tid)
}

fn user_cvar_key(player: i32, name: &str) -> String {
    format!("{}:{}", player, name.to_ascii_lowercase())
}

/// AAPTR bitmasks (GZDoom `constants.zs`).
const AAPTR_TARGET: i32 = 0x2;
const AAPTR_MASTER: i32 = 0x4;
const AAPTR_TRACER: i32 = 0x8;

fn copy_aaptr(world: &World, from: ActorId, selector: i32) -> Option<ActorId> {
    let a = world.actors.get(from)?;
    if selector & AAPTR_TARGET != 0 {
        return a
            .target
            .filter(|&id| world.actors.get(id).is_some_and(|t| !t.destroyed));
    }
    if selector & AAPTR_MASTER != 0 {
        return a
            .master
            .filter(|&id| world.actors.get(id).is_some_and(|t| !t.destroyed));
    }
    if selector & AAPTR_TRACER != 0 {
        return a
            .tracer
            .filter(|&id| world.actors.get(id).is_some_and(|t| !t.destroyed));
    }
    None
}

fn assign_aaptr_slots(world: &mut World, act: ActorId, slot: i32, value: Option<ActorId>) {
    let Some(a) = world.actors.get_mut(act) else {
        return;
    };
    if slot & AAPTR_TARGET != 0 {
        a.target = value;
    }
    if slot & AAPTR_MASTER != 0 {
        a.master = value;
    }
    if slot & AAPTR_TRACER != 0 {
        a.tracer = value;
    }
}

fn collect_tid_ids(world: &World, tid: i32, activator: u64) -> Vec<ActorId> {
    if tid == 0 {
        return unpack_actor(activator)
            .filter(|&id| world.actors.get(id).is_some_and(|a| !a.destroyed))
            .into_iter()
            .collect();
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

fn angle_to_acs(angle: Angle) -> i32 {
    // GZDoom `AngleToACS`: turns in 16.16 (`1.0` = 360°).
    let turns = angle.normalized().degrees() / 360.0;
    Fixed::from_f64(turns).0
}

fn acs_to_angle(acs: i32) -> Angle {
    let turns = Fixed(acs).to_f64();
    Angle::from_degrees(turns * 360.0)
}

fn first_sector_from_tag(world: &World, tag: i32) -> Option<usize> {
    if tag == 0 {
        return None;
    }
    world.map.sectors.iter().position(|s| s.tags.contains(&tag))
}

enum ThingCountFilter<'a> {
    DoomedNum(i32),
    ClassName(&'a str),
}

fn count_things(world: &World, filter: ThingCountFilter<'_>, tid: i32, tag: Option<i32>) -> i32 {
    let kind = match filter {
        ThingCountFilter::DoomedNum(type_id) if type_id > 0 => {
            let Some(k) = world.classes.find_doomednum(type_id) else {
                return 0;
            };
            Some(k)
        }
        ThingCountFilter::DoomedNum(_) => None,
        ThingCountFilter::ClassName(name) => {
            if name.is_empty() {
                None
            } else {
                let Some(k) = world.classes.find_name(name) else {
                    return 0;
                };
                Some(k)
            }
        }
    };
    let mut count = 0i32;
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || a.health <= 0 {
            continue;
        }
        if tid != 0 && a.tid != tid {
            continue;
        }
        if let Some(k) = kind {
            if a.class != k {
                continue;
            }
        }
        if let Some(tag) = tag {
            if tag == 0 {
                continue;
            }
            let in_tag = point_in_sector(&world.map, a.pos.x(), a.pos.y())
                .and_then(|si| world.map.sectors.get(si.raw() as usize))
                .is_some_and(|s| s.tags.contains(&tag));
            if !in_tag {
                continue;
            }
        }
        count += 1;
    }
    count
}

fn sector_plane_z(world: &World, tag: i32, x_fixed: i32, y_fixed: i32, floor: bool) -> i32 {
    let x = Fixed(x_fixed).to_f64();
    let y = Fixed(y_fixed).to_f64();
    let sec_i = if tag != 0 {
        first_sector_from_tag(world, tag)
    } else {
        point_in_sector(&world.map, x, y).map(|si| si.raw() as usize)
    };
    let Some(i) = sec_i else {
        return 0;
    };
    let Some(dyn_s) = world.sectors.get(i) else {
        return 0;
    };
    let static_h = world
        .map
        .sectors
        .get(i)
        .map(|s| {
            if floor {
                s.floor_height
            } else {
                s.ceiling_height
            }
        })
        .unwrap_or(0.0);
    let plane_z = world
        .map
        .setup
        .planes
        .get(i)
        .map(|p| {
            if floor {
                p.floor.z_at(x, y)
            } else {
                p.ceiling.z_at(x, y)
            }
        })
        .unwrap_or(static_h);
    Fixed::from_f64(
        plane_z
            + (if floor {
                dyn_s.floor_z
            } else {
                dyn_s.ceiling_z
            } - static_h),
    )
    .0
}

/// Map ACS / DECORATE flag names onto our `FLAG_*` bits.
fn flag_bit(name: &str) -> Option<u64> {
    let mut n = name.trim();
    if let Some(rest) = n.strip_prefix('+') {
        n = rest;
    }
    if let Some(rest) = n.strip_prefix("MF_") {
        n = rest;
    } else if let Some(rest) = n.strip_prefix("mf_") {
        n = rest;
    }
    let bit = match n.to_ascii_uppercase().as_str() {
        "SOLID" => crate::class::FLAG_SOLID,
        "SHOOTABLE" => crate::class::FLAG_SHOOTABLE,
        "COUNTKILL" => crate::class::FLAG_COUNTKILL,
        "COUNTITEM" => crate::class::FLAG_COUNTITEM,
        "MISSILE" => crate::class::FLAG_MISSILE,
        "NOGRAVITY" => crate::class::FLAG_NOGRAVITY,
        "AMBUSH" => crate::class::FLAG_AMBUSH,
        "FLOAT" => crate::class::FLAG_FLOAT,
        "DROPOFF" => crate::class::FLAG_DROPOFF,
        "NORADIUSDMG" => crate::class::FLAG_NORADIUSDMG,
        "DONTTHRUST" => crate::class::FLAG_DONTTHRUST,
        "NODAMAGE" => crate::class::FLAG_NODAMAGE,
        "INVULNERABLE" => crate::class::FLAG_INVULNERABLE,
        "DORMANT" => crate::class::FLAG_DORMANT,
        "NOTARGET" => crate::class::FLAG_NOTARGET,
        "FRIENDLY" => crate::class::FLAG_FRIENDLY,
        "NOBLOOD" => crate::class::FLAG_NOBLOOD,
        "FLOATBOB" => crate::class::FLAG_FLOATBOB,
        "DONTHARMSPECIES" => crate::class::FLAG_DONTHARMSPECIES,
        "DONTHARMCLASS" => crate::class::FLAG_DONTHARMCLASS,
        "BOSS" => crate::class::FLAG_BOSS,
        "SPAWNCEILING" => crate::class::FLAG_SPAWNCEILING,
        "NOBLOCKMAP" => crate::class::FLAG_NOBLOCKMAP,
        "FLOORCLIP" => crate::class::FLAG_FLOORCLIP,
        "DROPPED" => crate::class::FLAG_DROPPED,
        "CANUSEWALLS" => crate::class::FLAG_CANUSEWALLS,
        "JUSTHIT" => crate::class::FLAG_JUSTHIT,
        "JUSTATTACKED" => crate::class::FLAG_JUSTATTACKED,
        "CORPSE" => crate::class::FLAG_CORPSE,
        "DONTDRAW" | "INVISIBLE" => crate::class::FLAG_DONTDRAW,
        "FRIGHTENED" => crate::class::FLAG_FRIGHTENED,
        "NOTRIGGER" => crate::class::FLAG_NOTRIGGER,
        "DONTRIP" => crate::class::FLAG_DONTRIP,
        "RIPPER" => crate::class::FLAG_RIPPER,
        _ => return None,
    };
    Some(bit)
}

fn is_tid_used(world: &World, tid: i32) -> bool {
    if tid == 0 {
        return false;
    }
    world.thinkers.default.iter().copied().any(|id| {
        world
            .actors
            .get(id)
            .is_some_and(|a| !a.destroyed && a.tid == tid)
    })
}

/// Linear scan for a free TID (`start_tid != 0`).
fn find_unique_tid_linear(world: &World, start_tid: i32, limit: i32) -> i32 {
    // `end = start + limit - 1`, clamped on overflow (GZDoom FindUniqueTID).
    let end = match i64::from(start_tid).checked_add(i64::from(limit) - 1) {
        Some(e) if e <= i64::from(i32::MAX) => e as i32,
        _ => i32::MAX,
    };
    let mut tid = start_tid;
    while tid <= end {
        if tid != 0 && !is_tid_used(world, tid) {
            return tid;
        }
        if tid == i32::MAX {
            break;
        }
        tid += 1;
    }
    0
}

/// GZDoom `FLevelLocals::FindUniqueTID` subset.
fn find_unique_tid(world: &mut World, start_tid: i32, limit: i32) -> i32 {
    if start_tid != 0 {
        return find_unique_tid_linear(world, start_tid, limit);
    }
    // Random search in windows of 5 (uses ACS RNG stream).
    let attempts = if limit == 0 { i32::MAX } else { limit };
    let mut tried = 0i32;
    while tried < attempts {
        let r = u32::from(world.p_random("acs_uniquetid"));
        let r2 = u32::from(world.p_random("acs_uniquetid"));
        let seed = (((u64::from(r) << 8) | u64::from(r2)) & 0x7fff_ffff) as i32;
        let seed = if seed == 0 { 1 } else { seed };
        let found = find_unique_tid_linear(world, seed, 5);
        if found != 0 {
            return found;
        }
        tried = tried.saturating_add(5);
    }
    0
}

/// Load `BEHAVIOR` into `acs` and start OPEN scripts.
pub(crate) fn init_acs(world: &mut World) {
    world.acs = AcsState::new();
    let Some(bytes) = world.map.behavior.clone() else {
        return;
    };
    // GZDoom ignores a BEHAVIOR lump too small to hold an ACS header: Hexen-format
    // maps whose scripts come from LOADACS libraries ship it empty.
    if bytes.len() < 32 {
        return;
    }
    match room_acs::load_behavior(&bytes) {
        Ok(module) => {
            let name = world.map.name.clone();
            world.acs.push_module_named(module, &name);
            resolve_actor_arg0_str(world);
            resolve_line_arg0_str(world);
            start_open_on(world);
        }
        Err(e) => {
            world.push_message_kind(
                crate::world::MessageKind::System,
                format!("ACS load failed: {e}"),
            );
        }
    }
}

/// Map UDMF `arg0str` → negative SNAM script number on actors with ACS specials.
fn resolve_actor_arg0_str(world: &mut World) {
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.arg0_str.is_empty() || !is_acs_special(a.special) {
            continue;
        }
        let name = a.arg0_str.clone();
        let num = world
            .acs
            .modules
            .iter()
            .find_map(|m| m.script_number_by_name(&name));
        if let Some(a) = world.actors.get_mut(id) {
            if let Some(n) = num {
                a.args[0] = n;
            }
            a.arg0_str.clear();
        }
    }
}

fn is_acs_special(special: u16) -> bool {
    matches!(special, 80..=86 | 226)
}

/// Map UDMF linedef `arg0str` → negative SNAM script number in runtime `line_args`.
fn resolve_line_arg0_str(world: &mut World) {
    for (i, line) in world.map.lines.iter().enumerate() {
        if line.arg0_str.is_empty() || !is_acs_special(line.special as u16) {
            continue;
        }
        let Some(n) = world
            .acs
            .modules
            .iter()
            .find_map(|m| m.script_number_by_name(&line.arg0_str))
        else {
            continue;
        };
        let mut args = world.line_args(room_core::LineId::from_raw(i as u32));
        args[0] = n;
        world.line_args.insert(i as u32, args);
    }
}

/// Attach library `name` from `pool` and, depth-first, the libraries its `LOAD`
/// chunk names (GZDoom `FBehaviorContainer::LoadModule`). Already-attached
/// modules are skipped. Returns false if `name` is not in `pool`.
pub fn push_acs_library_tree(world: &mut World, name: &str, pool: &[(String, Vec<u8>)]) -> bool {
    if world
        .acs
        .modules
        .iter()
        .any(|m| m.name.eq_ignore_ascii_case(name))
    {
        return true;
    }
    let Some((_, bytes)) = pool.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) else {
        world.push_message_kind(
            crate::world::MessageKind::System,
            format!("Could not find ACS library {name}."),
        );
        return false;
    };
    let module = match room_acs::load_behavior(bytes) {
        Ok(m) => m,
        Err(e) => {
            world.push_message_kind(
                crate::world::MessageKind::System,
                format!("ACS library {name} load failed: {e}"),
            );
            return false;
        }
    };
    let deps = module.libraries.clone();
    world.acs.push_module_named(module, name);
    for dep in deps {
        let _ = push_acs_library_tree(world, &dep, pool);
    }
    true
}

/// Start OPEN scripts of modules attached after the first `from` (libraries).
pub fn start_library_open_scripts(world: &mut World, from: usize) {
    let mut acs = std::mem::take(&mut world.acs);
    {
        let mut host = WorldAcsHost { world };
        room_acs::start_open_scripts_from(&mut acs, &mut host, from);
    }
    world.acs = acs;
}

/// Append an ACS library module (LOADACS / `LOAD` chunk). Does not re-run OPEN.
pub fn push_acs_library(world: &mut World, bytes: &[u8]) -> bool {
    match room_acs::load_behavior(bytes) {
        Ok(module) => {
            world.acs.push_module(module);
            true
        }
        Err(e) => {
            world.push_message_kind(
                crate::world::MessageKind::System,
                format!("ACS library load failed: {e}"),
            );
            false
        }
    }
}

fn start_open_on(world: &mut World) {
    let mut acs = std::mem::take(&mut world.acs);
    {
        let mut host = WorldAcsHost { world };
        room_acs::start_open_scripts(&mut acs, &mut host);
    }
    world.acs = acs;
}

/// Fire ENTER scripts for every spawned player (after OPEN / map start).
pub fn start_enter_scripts(world: &mut World) {
    let acts: Vec<u64> = world
        .players
        .iter()
        .flatten()
        .filter_map(|p| p.mo.map(pack_actor))
        .collect();
    if acts.is_empty() {
        return;
    }
    let mut acs = std::mem::take(&mut world.acs);
    {
        let mut host = WorldAcsHost { world };
        for act in acts {
            room_acs::start_enter_scripts(&mut acs, &mut host, act);
        }
    }
    world.acs = acs;
}

/// Fire RETURN scripts for every player (hub re-entry).
pub fn start_return_scripts(world: &mut World) {
    let acts: Vec<u64> = world
        .players
        .iter()
        .flatten()
        .filter_map(|p| p.mo.map(pack_actor))
        .collect();
    if acts.is_empty() {
        return;
    }
    let mut acs = std::mem::take(&mut world.acs);
    {
        let mut host = WorldAcsHost { world };
        for act in acts {
            room_acs::start_return_scripts(&mut acs, &mut host, act);
        }
    }
    world.acs = acs;
}

/// Fire DEATH scripts for `player` pawn.
pub fn start_death_scripts(world: &mut World, mo: ActorId) {
    let mut acs = std::mem::take(&mut world.acs);
    {
        let mut host = WorldAcsHost { world };
        room_acs::start_death_scripts(&mut acs, &mut host, pack_actor(mo));
    }
    world.acs = acs;
}

/// Fire RESPAWN scripts for `player` pawn.
pub fn start_respawn_scripts(world: &mut World, mo: ActorId) {
    let mut acs = std::mem::take(&mut world.acs);
    {
        let mut host = WorldAcsHost { world };
        room_acs::start_respawn_scripts(&mut acs, &mut host, pack_actor(mo));
    }
    world.acs = acs;
}

/// Fire UNLOADING scripts (map exit).
pub fn start_unloading_scripts(world: &mut World) {
    let mut acs = std::mem::take(&mut world.acs);
    {
        let mut host = WorldAcsHost { world };
        room_acs::start_unloading_scripts(&mut acs, &mut host);
    }
    world.acs = acs;
}

/// One tic of ACS thinkers.
pub(crate) fn tick_acs(world: &mut World) {
    if world.acs.modules.is_empty() && world.acs.scripts.is_empty() {
        return;
    }
    let mut acs = std::mem::take(&mut world.acs);
    {
        let mut host = WorldAcsHost { world };
        room_acs::tick_scripts(&mut acs, &mut host);
    }
    world.acs = acs;
}

/// `ACS_Execute(script, map, s_arg1, s_arg2, s_arg3)`.
pub(crate) fn acs_execute(
    world: &mut World,
    args: &[i32; 5],
    always: bool,
    activator: Option<ActorId>,
    line: Option<room_core::LineId>,
) -> i32 {
    let script = args[0];
    let map = args[1];
    if map != 0 {
        // Deferred map start — store for hub enter (stub: ignore other maps).
        return 0;
    }
    let script_args = [args[2], args[3], args[4]];
    let act_key = activator.map(pack_actor).unwrap_or(0);
    let line_i = line.map(|l| l.raw() as i32).unwrap_or(-1);
    let mut acs = std::mem::take(&mut world.acs);
    let result = {
        let mut host = WorldAcsHost { world };
        start_script_with_line(
            &mut acs,
            &mut host,
            script,
            &script_args,
            always,
            act_key,
            line_i,
            0,
        )
    };
    world.acs = acs;
    result
}

/// Console `puke <n> [args]` — start script by number with the local player as activator.
pub fn puke_script(world: &mut World, script: i32, args: [i32; 3], always: bool) -> i32 {
    let activator = world.players[0].as_ref().and_then(|p| p.mo);
    let a = [script, 0, args[0], args[1], args[2]];
    acs_execute(world, &a, always, activator, None)
}

/// Console `pukename <name> [always] [args]`.
pub fn puke_named(world: &mut World, name: &str, args: [i32; 3], always: bool) -> i32 {
    let activator = world.players[0].as_ref().and_then(|p| p.mo);
    acs_named_execute_by_name(world, name, 0, args, always, activator)
}

/// Resolve a script name across loaded ACS modules and start it (BD3).
pub(crate) fn acs_named_execute_by_name(
    world: &mut World,
    name: &str,
    map: i32,
    script_args: [i32; 3],
    always: bool,
    activator: Option<ActorId>,
) -> i32 {
    if map != 0 {
        return 0;
    }
    let Some(number) = world
        .acs
        .modules
        .iter()
        .find_map(|m| m.script_number_by_name(name))
    else {
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "getvehicleprevioushealth" | "bdentertank" | "bdleavetank"
        ) {
            eprintln!(
                "room: VEHICLE ACS missing tic={} script={} modules={:?}",
                world.level.time,
                name,
                world
                    .acs
                    .modules
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>(),
            );
        }
        return 0;
    };
    let args = [number, 0, script_args[0], script_args[1], script_args[2]];
    let trace = matches!(
        name.to_ascii_lowercase().as_str(),
        "getvehicleprevioushealth" | "bdentertank" | "bdleavetank"
    );
    if trace {
        let activator_text = activator
            .and_then(|id| {
                world
                    .actors
                    .get(id)
                    .map(|a| (id.index(), a.tid, a.health, a.class))
            })
            .map(|(id, tid, hp, class)| {
                format!(
                    "#{}:{} tid={} hp={}",
                    id,
                    world
                        .classes
                        .get(class)
                        .map_or("<missing>", |c| c.name.as_str()),
                    tid,
                    hp
                )
            })
            .unwrap_or_else(|| "none".to_owned());
        eprintln!(
            "room: VEHICLE ACS start tic={} script={} number={} always={} activator={}",
            world.level.time, name, number, always, activator_text,
        );
    }
    let result = acs_execute(world, &args, always, activator, None);
    if trace {
        eprintln!(
            "room: VEHICLE ACS queued tic={} script={} result={} running={}",
            world.level.time,
            name,
            result,
            world.acs.scripts.len(),
        );
    }
    result
}

/// `ACS_Suspend(script, map)`.
pub(crate) fn acs_suspend(world: &mut World, args: &[i32; 5]) -> i32 {
    if args[1] != 0 {
        return 0;
    }
    suspend_script(&mut world.acs, args[0])
}

/// `ACS_Terminate(script, map)`.
pub(crate) fn acs_terminate(world: &mut World, args: &[i32; 5]) -> i32 {
    if args[1] != 0 {
        return 0;
    }
    terminate_script(&mut world.acs, args[0])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::specials::num;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn reload_bit_matches_gzdoom_bt_reload() {
        use crate::player::Buttons;
        assert_eq!(gzdoom_button_bits(Buttons::RELOAD), 1 << 6);
        assert_eq!(gzdoom_button_bits(Buttons::ZOOM), 1 << 7);
        assert_eq!(gzdoom_button_bits(Buttons::USER1), 1 << 21);
        assert_eq!(
            gzdoom_button_bits(Buttons::ATTACK | Buttons::RELOAD),
            (1 << 0) | (1 << 6)
        );
    }

    fn behavior_delay_open() -> Vec<u8> {
        // OPEN script 1: DELAYDIRECT 1; PUSHNUMBER 9; ASSIGNMAPVAR 0; TERMINATE
        let mut code = Vec::new();
        code.extend_from_slice(&56u32.to_le_bytes()); // DELAYDIRECT
        code.extend_from_slice(&1i32.to_le_bytes());
        code.extend_from_slice(&3u32.to_le_bytes()); // PUSHNUMBER
        code.extend_from_slice(&9i32.to_le_bytes());
        code.extend_from_slice(&26u32.to_le_bytes()); // ASSIGNMAPVAR
        code.extend_from_slice(&0u32.to_le_bytes());
        code.extend_from_slice(&1u32.to_le_bytes()); // TERMINATE
        let dir_off = 8 + code.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ACS\0");
        out.extend_from_slice(&(dir_off as u32).to_le_bytes());
        out.extend_from_slice(&code);
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&1001i32.to_le_bytes()); // OPEN #1
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    #[test]
    fn open_script_runs_after_delay() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.behavior = Some(behavior_delay_open());
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.acs.map_vars[0], 0);
        assert_eq!(w.acs.scripts.len(), 1);
        w.tick();
        assert_eq!(w.acs.map_vars[0], 9);
        assert!(w.acs.scripts.is_empty());
    }

    #[test]
    fn acs_execute_special() {
        // Closed script 2: PUSHNUMBER 42; ASSIGNMAPVAR 1; TERMINATE
        let mut code = Vec::new();
        code.extend_from_slice(&3u32.to_le_bytes());
        code.extend_from_slice(&42i32.to_le_bytes());
        code.extend_from_slice(&26u32.to_le_bytes());
        code.extend_from_slice(&1u32.to_le_bytes());
        code.extend_from_slice(&1u32.to_le_bytes());
        let dir_off = 8 + code.len();
        let mut beh = Vec::new();
        beh.extend_from_slice(b"ACS\0");
        beh.extend_from_slice(&(dir_off as u32).to_le_bytes());
        beh.extend_from_slice(&code);
        beh.extend_from_slice(&1u32.to_le_bytes());
        beh.extend_from_slice(&2i32.to_le_bytes());
        beh.extend_from_slice(&8u32.to_le_bytes());
        beh.extend_from_slice(&0u32.to_le_bytes());
        beh.extend_from_slice(&0u32.to_le_bytes());

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.behavior = Some(beh);
        let mut w = World::from_map(map).unwrap();
        let r = crate::specials::execute_special(
            &mut w,
            num::ACS_EXECUTE,
            &[2, 0, 0, 0, 0],
            None,
            None,
        );
        assert_eq!(r, 1);
        assert_eq!(w.acs.map_vars[1], 42);
    }

    #[test]
    fn actor_light_level_from_sector() {
        use room_core::SectorId;
        use room_map::setup::FFloorStatic;

        let mut map = MapBuilder::new()
            .room("play", rect(0.0, 0.0, 256.0, 256.0), 0.0, 192.0)
            .room("ctrl", rect(300.0, 0.0, 364.0, 64.0), 64.0, 80.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 9;
        let si = point_in_sector(&w.map, 128.0, 128.0)
            .or_else(|| point_in_sector_vanilla(&w.map, 128.0, 128.0))
            .unwrap()
            .raw() as usize;
        let ctrl = if si == 0 { 1 } else { 0 };
        w.sectors[si].light = 176;
        w.sectors[ctrl].light = 240;
        w.sectors[ctrl].floor_z = 64.0;
        w.sectors[ctrl].ceiling_z = 80.0;
        {
            let host = WorldAcsHost { world: &mut w };
            assert_eq!(host.actor_light_level(9, 0), 176);
            assert_eq!(host.actor_light_level(999, 0), 0);
        }
        std::sync::Arc::make_mut(&mut w.map)
            .setup
            .ffloors
            .push(FFloorStatic {
                target: SectorId::from_raw(si as u32),
                control: SectorId::from_raw(ctrl as u32),
                typ: 1,
                flags: 0,
                alpha: 255,
            });
        w.actors.get_mut(mo).unwrap().pos = DVec3::new(128.0, 128.0, 0.0);
        w.actors.get_mut(mo).unwrap().height = 56.0;
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(
            host.actor_light_level(9, 0),
            240,
            "under solid 3D floor should use control light"
        );
        std::sync::Arc::make_mut(&mut w.map).setup.ffloors.clear();
        std::sync::Arc::make_mut(&mut w.map)
            .setup
            .ffloors
            .push(FFloorStatic {
                target: SectorId::from_raw(si as u32),
                control: SectorId::from_raw(ctrl as u32),
                typ: 3,
                flags: 512 + 2048,
                alpha: 0,
            });
        w.sectors[si].light = 176;
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(
            host.actor_light_level(9, 0),
            240,
            "ExtraFloor_LightOnly below top plane uses control light"
        );
    }

    #[test]
    fn udmf_sector_lightlevel() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .sector_tag("a", 5)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.sectors[0].light = 192;
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.udmf_int(2, 5, "lightlevel"), 192);
        assert_eq!(
            host.udmf_fixed(2, 5, "heightfloor"),
            room_core::Fixed::from_f64(0.0).0
        );
        assert_eq!(
            host.udmf_fixed(2, 5, "heightceiling"),
            room_core::Fixed::from_f64(128.0).0
        );
    }

    #[test]
    fn udmf_line_ml_flags() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(l) = map.lines.first_mut() {
            l.tag = 8;
            l.flags |= 0x0001 | 0x0002 | 0x4000;
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.udmf_int(0, 8, "blocking"), 1);
        assert_eq!(host.udmf_int(0, 8, "blockmonsters"), 1);
        assert_eq!(host.udmf_int(0, 8, "blockplayers"), 1);
    }

    #[test]
    fn udmf_line_spac_and_repeatspecial() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(l) = map.lines.first_mut() {
            l.tag = 9;
            l.activation = room_map::spac::USE | room_map::spac::IMPACT;
            l.flags |= crate::world::ML_REPEAT_SPECIAL as i16;
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        {
            let host = WorldAcsHost { world: &mut w };
            assert_eq!(host.udmf_int(0, 9, "playeruse"), 1);
            assert_eq!(host.udmf_int(0, 9, "impact"), 1);
            assert_eq!(host.udmf_int(0, 9, "playercross"), 0);
            assert_eq!(host.udmf_int(0, 9, "repeatspecial"), 1);
        }
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_line_activation(9, room_map::spac::CROSS as i32, 0);
        }
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.udmf_int(0, 9, "playercross"), 1);
        assert_eq!(host.udmf_int(0, 9, "playeruse"), 0);
        assert_eq!(host.udmf_int(0, 9, "repeatspecial"), 0);
    }

    #[test]
    fn udmf_line_alpha() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(l) = map.lines.first_mut() {
            l.tag = 7;
            l.mid_alpha = 0.5;
            l.mid_additive = true;
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.line_additive.get(&0), Some(&true));
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(
            host.udmf_fixed(0, 7, "alpha"),
            room_core::Fixed::from_f64(0.5).0
        );
        assert_eq!(host.udmf_int(0, 7, "alpha"), 128);
        assert_eq!(host.udmf_int(0, 7, "renderstyle"), 65);
    }

    #[test]
    fn udmf_sector_light_fade_fog() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .sector_tag("a", 6)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].light_color = 0xFF_80_40;
        map.sectors[0].fade_color = 0x10_20_30;
        map.sectors[0].fog_density = 200;
        map.sectors[0].desaturation = 40;
        map.sectors[0].health_floor = 90;
        map.sectors[0].health_ceiling = 45;
        map.sectors[0].health_3d = 12;
        map.sectors[0].floor_alpha = 0.5;
        map.sectors[0].ceiling_alpha = 0.25;
        map.sectors[0].floor_additive = true;
        map.sectors[0].ceiling_additive = true;
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.sectors[0].color, [0xFF, 0x80, 0x40]);
        assert_eq!(w.sectors[0].fade, [0x10, 0x20, 0x30]);
        assert_eq!(w.sectors[0].fog_density, 200);
        assert_eq!(w.sectors[0].color_desaturate, 40);
        assert_eq!(w.sectors[0].health_floor, 90);
        assert_eq!(w.sectors[0].health_ceiling, 45);
        assert_eq!(w.sectors[0].health_3d, 12);
        assert!((w.sectors[0].floor_alpha - 0.5).abs() < 1e-5);
        assert!((w.sectors[0].ceiling_alpha - 0.25).abs() < 1e-5);
        assert!(w.sectors[0].floor_additive);
        assert!((w.fog_density - 200.0 / 255.0).abs() < 1e-4);
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.udmf_int(2, 6, "lightcolor"), 0xFF_80_40);
        assert_eq!(host.udmf_int(2, 6, "fadecolor"), 0x10_20_30);
        assert_eq!(host.udmf_int(2, 6, "fogdensity"), 200);
        assert_eq!(host.udmf_int(2, 6, "desaturation"), 40);
        assert_eq!(host.udmf_int(2, 6, "healthfloor"), 90);
        assert_eq!(host.udmf_int(2, 6, "healthceiling"), 45);
        assert_eq!(host.udmf_int(2, 6, "health3d"), 12);
        assert_eq!(host.udmf_int(2, 6, "alphafloor"), 128);
        assert_eq!(host.udmf_int(2, 6, "alphaceiling"), 64);
        assert_eq!(host.udmf_int(2, 6, "renderstyle"), 65);
        assert_eq!(
            host.udmf_fixed(2, 6, "alphafloor"),
            room_core::Fixed::from_f64(0.5).0
        );
    }

    #[test]
    fn udmf_clip_wrap_midtex() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(l) = map.lines.first_mut() {
            l.tag = 14;
            l.clip_midtex = true;
            l.wrap_midtex = true;
        }
        if let Some(s) = map.sides.first_mut() {
            s.clip_midtex = true;
            s.wrap_midtex = false;
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.udmf_int(0, 14, "clipmidtex"), 1);
        assert_eq!(host.udmf_int(0, 14, "wrapmidtex"), 1);
        assert_eq!(host.udmf_int(3, 14, "clipmidtex"), 1);
        assert_eq!(host.udmf_int(3, 14, "wrapmidtex"), 0);
    }

    #[test]
    fn udmf_line_health_and_automapstyle() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(l) = map.lines.first_mut() {
            l.tag = 12;
            l.health = 40;
            l.automap_style = 8;
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.line_health.get(&0), Some(&40));
        assert_eq!(w.line_automap_style.get(&0), Some(&8));
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.udmf_int(0, 12, "health"), 40);
        assert_eq!(host.udmf_int(0, 12, "automapstyle"), 8);
    }

    #[test]
    fn udmf_side_light_and_scale() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(l) = map.lines.first_mut() {
            l.tag = 11;
        }
        if let Some(s) = map.sides.first_mut() {
            s.light = 40;
            s.light_absolute = true;
            s.light_top = 220;
            s.light_absolute_top = true;
            s.light_bottom = -16;
            s.scale_x = 2.0;
            s.scale_y = 0.5;
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let host = WorldAcsHost { world: &mut w };
        // Front side: line id in low bits, side select in bit 30.
        let front = 11;
        assert_eq!(host.udmf_int(3, front, "light"), 40);
        assert_eq!(host.udmf_int(3, front, "lightabsolute"), 1);
        assert_eq!(host.udmf_int(3, front, "light_top"), 220);
        assert_eq!(host.udmf_int(3, front, "lightabsolute_top"), 1);
        assert_eq!(host.udmf_int(3, front, "light_bottom"), -16);
        assert_eq!(
            host.udmf_fixed(3, front, "scalex_mid"),
            room_core::Fixed::from_f64(2.0).0
        );
        assert_eq!(
            host.udmf_fixed(3, front, "scaley"),
            room_core::Fixed::from_f64(0.5).0
        );
    }

    #[test]
    fn udmf_user_props_via_acs() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .sector_tag("a", 5)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        if let Some(l) = map.lines.first_mut() {
            l.tag = 9;
        }
        map.udmf_user
            .lines
            .resize(map.lines.len(), Default::default());
        map.udmf_user.lines[0].ints.insert("user_score".into(), 42);
        map.udmf_user.lines[0]
            .fixed
            .insert("user_scale".into(), (1.5_f64 * 65536.0).round() as i32);
        map.udmf_user
            .sectors
            .resize(map.sectors.len(), Default::default());
        map.udmf_user.sectors[0].ints.insert("user_wet".into(), 1);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.udmf_int(0, 9, "user_score"), 42);
        assert_eq!(host.udmf_int(2, 5, "user_wet"), 1);
        assert_eq!(
            host.udmf_fixed(0, 9, "user_scale"),
            (1.5_f64 * 65536.0).round() as i32
        );
    }

    #[test]
    fn udmf_arg0str_resolves_named_script() {
        use crate::{ClassRegistry, StateTable};
        use room_acs::{AcsFormat, AcsModule, ScriptDef, ScriptType};
        use room_decorate::parse_decorate;
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Marker : Actor 3004
{
  Health 1
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.things.push(room_map::RawThing {
            tid: 22,
            x: 64,
            y: 64,
            z: 0,
            angle: 0,
            type_id: 3004,
            flags: 7,
            special: 80,
            args: [0; 5],
            hexen: true,
            friendly_see_blocks: -1,
            health: 0,
            score: i32::MIN,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: i16::MIN,
            roll: i16::MIN,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
            count_secret: false,
            no_count: false,
            arg0_str: "Go".into(),
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
            ext: 0,
            user: Default::default(),
        });
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        assert_eq!(
            w.actors.get(w.find_by_tid(22).unwrap()).unwrap().arg0_str,
            "Go"
        );
        w.acs = AcsState::new();
        w.acs.push_module(AcsModule {
            data: vec![0u8; 16],
            format: AcsFormat::Acse,
            scripts: vec![ScriptDef {
                number: -1,
                address: 0,
                argc: 0,
                script_type: ScriptType::Closed,
                flags: 0,
                local_count: 0,
            }],
            string_offsets: Vec::new(),
            libraries: Vec::new(),
            script_names: vec!["Go".into()],
            functions: Vec::new(),
            function_names: Vec::new(),
            map_var_defaults: [0; 128],
            map_arrays: Vec::new(),
            map_exports: Vec::new(),
            name: String::new(),
            named_base: 0,
            map_imports: Vec::new(),
            array_imports: Vec::new(),
            mstr_vars: Vec::new(),
            astr_vars: Vec::new(),
            atags: Vec::new(),
            script_arrays: Vec::new(),
            function_arrays: Vec::new(),
            jump_points: Vec::new(),
        });
        resolve_actor_arg0_str(&mut w);
        let a = w.actors.get(w.find_by_tid(22).unwrap()).unwrap();
        assert_eq!(a.args[0], -1);
        assert!(a.arg0_str.is_empty());
    }

    #[test]
    fn udmf_thing_health_score_gravity() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.things.push(room_map::RawThing {
            tid: 11,
            x: 64,
            y: 96,
            z: 8,
            angle: 90,
            type_id: 3004,
            flags: 7,
            special: 0,
            args: [0; 5],
            hexen: true,
            friendly_see_blocks: 4,
            health: 77,
            score: 12,
            gravity: 0.5,
            scale_x: 2.0,
            scale_y: 1.5,
            alpha: 0.25,
            pitch: 10,
            roll: -5,
            fill_color: 0x00_11_22,
            render_style: "Add".into(),
            float_bob_phase: 3,
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
            ext: 0,
            user: Default::default(),
        });
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.udmf_int(1, 11, "health"), 77);
        assert_eq!(host.udmf_int(1, 11, "score"), 12);
        assert_eq!(host.udmf_int(1, 11, "friendlyseeblocks"), 4);
        assert_eq!(host.udmf_int(1, 11, "fillcolor"), 0x00_11_22);
        assert_eq!(host.udmf_int(1, 11, "renderstyle"), 65);
        assert_eq!(host.udmf_int(1, 11, "pitch"), 10);
        assert_eq!(host.udmf_int(1, 11, "roll"), -5);
        assert_eq!(
            host.udmf_fixed(1, 11, "gravity"),
            room_core::Fixed::from_f64(0.5).0
        );
        assert_eq!(
            host.udmf_fixed(1, 11, "scalex"),
            room_core::Fixed::from_f64(2.0).0
        );
        assert_eq!(
            host.udmf_fixed(1, 11, "alpha"),
            room_core::Fixed::from_f64(0.25).0
        );
        assert_eq!(
            host.udmf_fixed(1, 11, "x"),
            room_core::Fixed::from_f64(64.0).0
        );
    }

    #[test]
    fn thing_count_living() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        // Tag the player pawn and count by tid.
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 7;
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.thing_count(0, 7), 1);
        assert_eq!(host.thing_count(99999, 0), 0); // unknown DoomEdNum
        assert!(host.thing_count(0, 0) >= 1);
    }

    #[test]
    fn thing_count_name_and_sector() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .sector_tag("a", 3)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let class_name = w
            .classes
            .get(w.actors.get(mo).unwrap().class)
            .unwrap()
            .name
            .clone();
        w.actors.get_mut(mo).unwrap().tid = 4;
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.thing_count_name(&class_name, 4), 1);
        assert_eq!(host.thing_count_name("NopeClass", 0), 0);
        assert_eq!(host.thing_count_name_sector(&class_name, 0, 3), 1);
        assert_eq!(host.thing_count_sector(0, 4, 3), 1);
        assert_eq!(host.thing_count_sector(0, 4, 99), 0);
    }

    #[test]
    fn actor_property_position_velocity() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 7;
        w.actors.get_mut(mo).unwrap().health = 80;
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.actor_property(7, aprop::HEALTH, 0), 80);
            host.set_actor_property(7, aprop::HEALTH, 50, 0);
            host.set_actor_property(7, aprop::SCORE, 25, 0);
            host.set_actor_property(7, aprop::DAMAGE, 9, 0);
            host.set_actor_property(7, aprop::FRIENDLY, 1, 0);
            assert_eq!(host.actor_property(7, aprop::SCORE, 0), 25);
            assert_eq!(host.actor_property(7, aprop::DAMAGE, 0), 9);
            assert_eq!(host.actor_property(7, aprop::FRIENDLY, 0), 1);
            assert!(host.actor_property(7, aprop::SPAWNHEALTH, 0) > 0);
            host.set_actor_property(7, aprop::MELEERANGE, Fixed::from_f64(96.0).0, 0);
            assert_eq!(
                host.actor_property(7, aprop::MELEERANGE, 0),
                Fixed::from_f64(96.0).0
            );
            host.set_actor_property(7, aprop::JUMPZ, Fixed::from_f64(12.0).0, 0);
            assert_eq!(
                host.actor_property(7, aprop::JUMPZ, 0),
                Fixed::from_f64(12.0).0
            );
            host.set_actor_property(7, aprop::CHASEGOAL, 1, 0);
            host.set_actor_property(7, aprop::FRIGHTENED, 1, 0);
            assert_eq!(host.actor_property(7, aprop::CHASEGOAL, 0), 1);
            assert_eq!(host.actor_property(7, aprop::FRIGHTENED, 0), 1);
            host.set_actor_property(7, aprop::NOTRIGGER, 1, 0);
            assert_eq!(host.actor_property(7, aprop::NOTRIGGER, 0), 1);
            host.set_actor_property(7, aprop::DAMAGEMULTIPLIER, Fixed::from_f64(2.0).0, 0);
            assert_eq!(
                host.actor_property(7, aprop::DAMAGEFACTOR, 0),
                Fixed::from_f64(2.0).0
            );
            host.set_actor_property(7, aprop::MAXSTEPHEIGHT, Fixed::from_f64(16.0).0, 0);
            host.set_actor_property(7, aprop::MAXDROPOFFHEIGHT, Fixed::from_f64(32.0).0, 0);
            assert_eq!(
                host.actor_property(7, aprop::MAXSTEPHEIGHT, 0),
                Fixed::from_f64(16.0).0
            );
            host.set_actor_property(7, aprop::STENCILCOLOR, 0x00ff_0000, 0);
            assert_eq!(host.actor_property(7, aprop::STENCILCOLOR, 0), 0x00ff_0000);
            host.set_actor_property(7, aprop::RENDERSTYLE, 65, 0); // STYLE_Add
            assert_eq!(host.actor_property(7, aprop::RENDERSTYLE, 0), 65);
            host.set_actor_species(7, "Pack", 0);
            assert_eq!(host.actor_species(7, 0), "Pack");
            host.set_actor_species(7, "None", 0);
            assert_eq!(host.actor_species(7, 0), "");
            host.set_actor_nametag(7, "Hero", 0);
            assert_eq!(host.actor_nametag(7, 0), "Hero");
            host.set_actor_damage_type(7, "Fire", 0);
            assert_eq!(host.actor_damage_type(7, 0), "Fire");
            host.set_actor_damage_type(7, "None", 0);
            assert_eq!(host.actor_damage_type(7, 0), "");
            assert_eq!(host.actor_waterlevel(7, 0), 0);
            host.set_actor_sound_prop(7, aprop::SEESOUND, "misc/chat", 0);
            host.set_actor_sound_class(7, "marine", 0);
            assert_eq!(host.actor_sound_class(7, 0), "marine");
            host.set_actor_property(7, aprop::FRIENDLYSEEBLOCKS, 5, 0);
            assert_eq!(host.actor_property(7, aprop::FRIENDLYSEEBLOCKS, 0), 5);
            assert_eq!(
                host.set_actor_position(
                    7,
                    Fixed::from_f64(40.0).0,
                    Fixed::from_f64(64.0).0,
                    0,
                    false,
                    0
                ),
                1
            );
            assert_eq!(
                host.set_actor_velocity(7, Fixed::from_f64(3.0).0, 0, 0, false, 0),
                1
            );
            let key = host.set_activator(7, 0);
            assert_ne!(key, 0);
            assert_eq!(host.activator_tid(key), 7);
        }
        let a = w.actors.get(mo).unwrap();
        assert_eq!(a.health, 50);
        assert_eq!(a.score, 25);
        assert_eq!(a.damage, 9);
        assert!((a.melee_range - 96.0).abs() < 1e-6);
        assert!((a.jump_z - 12.0).abs() < 1e-6);
        assert!(a.chase_goal);
        assert!(a.frightened);
        assert_ne!(a.flags & crate::class::FLAG_NOTRIGGER, 0);
        assert!((a.damage_factor - 2.0).abs() < 1e-6);
        assert!((a.max_step_height - 16.0).abs() < 1e-6);
        assert!((a.max_dropoff_height - 32.0).abs() < 1e-6);
        assert_eq!(a.render_style, crate::class::RenderStyle::Add);
        assert_eq!(a.fill_color, Some([255, 0, 0]));
        assert_ne!(a.flags & crate::class::FLAG_FRIENDLY, 0);
        assert!((a.pos.x() - 40.0).abs() < 1e-3);
        assert!((a.vel.x() - 3.0).abs() < 1e-3);
    }

    #[test]
    fn set_actor_property_master_target_tracer_tid() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = room_decorate::parse_decorate(src).unwrap();
        let classes = std::sync::Arc::new(crate::class::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let zombieman = classes.find_name("ZombieMan").unwrap();
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 1;
        let other = w
            .spawn_at(
                zombieman,
                room_core::DVec3::new(128.0, 64.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(other).unwrap().tid = 2;
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_actor_property(1, aprop::MASTER_TID, 2, 0);
            host.set_actor_property(1, aprop::TARGET_TID, 2, 0);
            host.set_actor_property(1, aprop::TRACER_TID, 2, 0);
            assert_eq!(host.actor_property(1, aprop::MASTER_TID, 0), 2);
            assert_eq!(host.actor_property(1, aprop::TARGET_TID, 0), 2);
            assert_eq!(host.actor_property(1, aprop::TRACER_TID, 0), 2);
            host.set_actor_property(1, aprop::TARGET_TID, 0, 0);
            assert_eq!(host.actor_property(1, aprop::TARGET_TID, 0), 0);
        }
        let a = w.actors.get(mo).unwrap();
        assert_eq!(a.master, Some(other));
        assert_eq!(a.target, None);
        assert_eq!(a.tracer, Some(other));
    }

    #[test]
    fn user_var_array_and_check_class() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::class::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 7;
        let key = pack_actor(mo);
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.set_user_var(7, "user_hp", 42, 0), 1);
            assert_eq!(host.get_user_var(7, "user_hp", 0), 42);
            assert_eq!(host.set_user_array(0, "user_hits", 3, 11, key), 1);
            assert_eq!(host.get_user_array(0, "user_hits", 3, key), 11);
            assert_eq!(host.check_actor_class(7, "PlayerPawn", 0), 1);
            assert_eq!(host.check_actor_class(7, "ZombieMan", 0), 0);
            assert_eq!(host.set_actor_flag(7, "SOLID", 1, 0), 1);
            assert_eq!(host.check_flag(7, "SOLID", 0), 1);
            assert_eq!(host.set_actor_flag(7, "+INVULNERABLE", 1, 0), 1);
            assert_eq!(host.check_flag(7, "MF_INVULNERABLE", 0), 1);
            assert_eq!(host.set_actor_flag(7, "SOLID", 0, 0), 1);
            assert_eq!(host.check_flag(7, "SOLID", 0), 0);
            assert!(host.actor_view_height(7, 0) > 0);
            assert_eq!(host.check_actor_property(7, aprop::HEALTH, 100, 0), 1);
            assert_eq!(host.check_actor_property(7, aprop::HEALTH, 50, 0), 0);
            assert_eq!(host.radius_quake2(7, 5, 10, 64, 128, "world/quake", 0), 1);
        }
        assert_eq!(w.quakes.len(), 1);
        assert_eq!(w.quakes[0].countdown, 10);
        assert!((w.quakes[0].intensity - 5.0).abs() < 1e-9);
    }

    #[test]
    fn fade_to_and_set_font() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_font("BIGFONT");
            // Instant fade to half-black (0 seconds).
            host.fade_to(
                Fixed::from_f64(0.0).0,
                Fixed::from_f64(0.0).0,
                Fixed::from_f64(0.0).0,
                Fixed::from_f64(0.5).0,
                0,
            );
        }
        assert_eq!(w.acs_font, "BIGFONT");
        assert!((w.screen_fade.a - 0.5).abs() < 1e-4);
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.cancel_fade();
        }
        assert_eq!(w.screen_fade.a, 0.0);
    }

    #[test]
    fn set_music_stores_request() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_music("D_RUNNIN", 0, 0);
            host.set_music_volume(Fixed::from_f64(0.5).0);
        }
        assert_eq!(w.music_request.as_deref(), Some("D_RUNNIN"));
        assert!((w.music_volume - 0.5).abs() < 1e-4);
    }

    #[test]
    fn particle_hud_and_cvar_string() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_hud_clip_rect(10, 20, 100, 50);
            host.set_hud_wrap_width(320);
            assert_eq!(host.set_cvar("skill", 3), 1);
            assert_eq!(host.get_cvar_string("skill"), "3");
            assert_eq!(host.set_cvar_string("message", "hi"), 1);
            assert_eq!(host.get_cvar_string("message"), "hi");
            assert_eq!(host.set_user_cvar(0, "myvar", 42), 1);
            assert_eq!(host.get_user_cvar(0, "myvar"), 42);
            assert_eq!(host.set_user_cvar_string(0, "nick", "bob"), 1);
            assert_eq!(host.get_user_cvar_string(0, "nick"), "bob");
            host.start_slideshow("Intro");
            assert_eq!(host.script_call(&[], 0), 0);
            host.announcer_sound("announce");
            host.spawn_particle(
                0x00_ff_00,
                true,
                35,
                Fixed::from_f64(2.0).0,
                Fixed::from_f64(10.0).0,
                Fixed::from_f64(20.0).0,
                0,
            );
        }
        assert_eq!(w.hud_clip.w, 100);
        assert_eq!(w.hud_wrap_width, 320);
        assert_eq!(w.skill, 3);
        assert_eq!(
            w.acs_cvar_strings.get("message").map(String::as_str),
            Some("hi")
        );
        assert_eq!(w.acs_user_cvars.get("0:myvar").copied(), Some(42));
        assert_eq!(
            w.acs_user_cvar_strings.get("0:nick").map(String::as_str),
            Some("bob")
        );
        assert_eq!(w.slideshow_request.as_deref(), Some("Intro"));
        assert!(!w.sound_events.is_empty());
        assert_eq!(w.particle_events.len(), 1);
        assert!(w.particle_events[0].fullbright);
        assert!((w.particle_events[0].x - 10.0).abs() < 1e-6);
    }

    #[test]
    fn set_pointer_assigns_target() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0 { Health 100 States { Spawn: PLAY A -1 Stop } }
actor Spot : Actor 9001 { States { Spawn: TNT1 A -1 Stop } }
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::class::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        let spot = w.classes.find_name("Spot").unwrap();
        let sid = w
            .spawn_at(
                spot,
                room_core::DVec3::new(200.0, 100.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(sid).unwrap().tid = 7;
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.set_pointer_acs(0x2, 7, 0, key), 1);
        }
        assert_eq!(w.actors.get(mo).unwrap().target, Some(sid));
    }

    #[test]
    fn sector_and_line_health() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .sector_tag("a", 9)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.sectors[0].health_floor = 100;
        w.sectors[0].health_ceiling = 50;
        w.line_health.insert(0, 25);
        let host = WorldAcsHost { world: &mut w };
        assert_eq!(host.get_sector_health(9, room_acs::secpart::FLOOR), 100);
        assert_eq!(host.get_sector_health(9, room_acs::secpart::CEILING), 50);
        assert_eq!(host.get_line_health(0), 25);
    }

    #[test]
    fn get_line_xy_midpoint() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let host = WorldAcsHost { world: &mut w };
        let half = Fixed::from_f64(0.5).0;
        let x = host.get_line_x(0, half, 0);
        let y = host.get_line_y(0, half, 0);
        let line = &w.map.lines[0];
        let v1 = &w.map.vertices[line.v1.raw() as usize];
        let expect_x = Fixed::from_f64(v1.pos.x() + line.delta.x() * 0.5).0;
        let expect_y = Fixed::from_f64(v1.pos.y() + line.delta.y() * 0.5).0;
        assert_eq!(x, expect_x);
        assert_eq!(y, expect_y);
    }

    #[test]
    fn fog_glow_subtitle_net_id() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .sector_tag("a", 4)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_fog_density(4, 200);
            host.set_sector_glow(4, 0, 255, 0, 0, 32);
            host.set_subtitle_number(3, "DSPISTOL", key);
            let nid = host.get_net_id(0, 0, 0, key);
            assert_eq!(nid, mo.index() as i32);
            assert_eq!(host.set_activator_by_net_id(nid, 0), key);
        }
        assert!(w.fog_density > 0.0);
        assert_eq!(w.sectors[0].fog_density, 100);
        assert_eq!(w.sectors[0].glow_floor.map(|g| g.color), Some([255, 0, 0]));
        assert_eq!(w.subtitle_request.as_ref().map(|s| s.0), Some(3));
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_team_score(1, 5);
            assert_eq!(host.get_team_score(1), 5);
        }
    }

    #[test]
    fn warp_and_quake_ex() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0 { Health 100 States { Spawn: PLAY A -1 Stop } }
actor Spot : Actor 9001 { States { Spawn: TNT1 A -1 Stop } }
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::class::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        let spot = w.classes.find_name("Spot").unwrap();
        let sid = w
            .spawn_at(
                spot,
                room_core::DVec3::new(200.0, 100.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(sid).unwrap().tid = 5;
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(
                host.warp_acs(5, Fixed::from_f64(16.0).0, 0, 0, 0, 0, key),
                1
            );
            assert_eq!(host.quake_ex(5, 4, 20, 64, 128, "world/quake", key), 1);
        }
        let p = w.actors.get(mo).unwrap().pos;
        assert!((p.x() - 216.0).abs() < 1e-6);
        assert!((p.y() - 100.0).abs() < 1e-6);
        assert_eq!(w.quakes.len(), 1);
        assert_eq!(w.quakes[0].countdown, 20);
    }

    #[test]
    fn telefog_sector_damage_and_pointer_equal() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .sector_tag("a", 7)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_actor_tele_fog(0, "TeleportFog", "TeleportFog", key);
            assert_eq!(host.swap_actor_tele_fog(0, key), 1);
            assert_eq!(host.is_pointer_equal(0, 0, key), 1);
            host.set_sector_damage(7, 5, "Fire", 16, 128);
            host.set_sector_terrain(7, 0, "Water");
            host.set_sector_terrain(7, 1, "Lava");
        }
        let a = w.actors.get(mo).unwrap();
        assert_eq!(a.telefog_source, "TeleportFog");
        assert_eq!(a.telefog_dest, "TeleportFog");
        assert_eq!(w.sectors[0].damage_amount, 5);
        assert_eq!(w.sectors[0].damage_interval, 16);
        assert_eq!(w.sectors[0].leakiness, 128);
        assert!(w.terrain.floors.values().any(|v| v == "WATER"));
        assert!(w.terrain.ceilings.values().any(|v| v == "LAVA"));
    }

    #[test]
    fn sound_seq_translation_and_can_raise() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        w.actors.get_mut(mo).unwrap().tid = 3;
        w.actors.get_mut(mo).unwrap().flags |= crate::class::FLAG_CORPSE;
        w.actors.get_mut(mo).unwrap().health = 0;
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.can_raise_actor(3, 0), 1);
            host.set_translation_acs(3, "Rainy", 0);
            host.sound_sequence("DoorOpen");
            host.sound_sequence_on_actor(3, "Platform", key);
        }
        assert_eq!(w.actors.get(mo).unwrap().translation, "Rainy");
        assert!(w.sound_events.iter().any(|e| e.name == "DoorOpen"));
        assert!(w.sound_events.iter().any(|e| e.name == "Platform"));
    }

    #[test]
    fn acs_sound_pcods_queue_events() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 2;
        let key = pack_actor(mo);
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.ambient_sound("world/quake", 127);
            host.activator_sound("weapons/pistol", 64, key);
            host.thing_sound(2, "misc/chat", 127, 0);
            host.sector_sound("switches/normbutn", 100, key);
        }
        let ev = w.drain_sound_events();
        assert_eq!(ev.len(), 4);
        assert_eq!(ev[0].name, "world/quake");
        assert!((ev[0].attenuation - 0.0).abs() < 1e-6);
        assert!((ev[1].volume - 64.0 / 127.0).abs() < 1e-4);
        assert_eq!(ev[2].name, "misc/chat");
    }

    #[test]
    fn classify_actor_player_alive() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 1;
        let host = WorldAcsHost { world: &mut w };
        let bits = host.classify_actor(1, 0);
        assert_eq!(bits & 2, 2); // ACTOR_PLAYER
        assert_eq!(bits & 64, 64); // ACTOR_ALIVE
        assert_eq!(host.classify_actor(999, 0), 0);
        assert_eq!(host.classify_actor(0, 0), 1); // ACTOR_WORLD
    }

    #[test]
    fn change_level_and_check_floor_texture() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        w.sector_floor_pics.insert(0, "FLAT5_1".into());
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.check_actor_texture(0, "FLAT5_1", true, key), 1);
            assert_eq!(host.check_actor_texture(0, "NOTATEX", true, key), 0);
            host.change_level("MAP02", 0, 0, -1);
        }
        assert!(w.level.exiting);
        assert_eq!(w.level.next_map_override.as_deref(), Some("MAP02"));
    }

    #[test]
    fn waterlevel_from_transfer_heights_and_swimmable() {
        use room_core::SectorId;
        use room_map::setup::FFloorStatic;

        let mut map = MapBuilder::new()
            .room("pool", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .room("ctrl", rect(300.0, 0.0, 364.0, 64.0), 0.0, 40.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.height = 56.0;
            a.pos = DVec3::new(128.0, 128.0, 0.0);
        }
        let key = pack_actor(mo);

        let si = point_in_sector(&w.map, 128.0, 128.0)
            .or_else(|| point_in_sector_vanilla(&w.map, 128.0, 128.0))
            .expect("player sector")
            .raw() as usize;
        let ctrl = if si == 0 { 1 } else { 0 };

        assert_eq!(WorldAcsHost { world: &mut w }.actor_waterlevel(0, key), 0);

        w.sectors[si].fake_floor_z = Some(24.0);
        assert_eq!(WorldAcsHost { world: &mut w }.actor_waterlevel(0, key), 1);

        w.sectors[si].fake_floor_z = Some(40.0);
        assert_eq!(WorldAcsHost { world: &mut w }.actor_waterlevel(0, key), 2);

        w.sectors[si].fake_floor_z = Some(64.0);
        assert_eq!(WorldAcsHost { world: &mut w }.actor_waterlevel(0, key), 3);

        w.sectors[si].fake_floor_z = None;
        std::sync::Arc::make_mut(&mut w.map)
            .setup
            .ffloors
            .push(FFloorStatic {
                target: SectorId::from_raw(si as u32),
                control: SectorId::from_raw(ctrl as u32),
                typ: 2,
                flags: 0,
                alpha: 255,
            });
        assert_eq!(WorldAcsHost { world: &mut w }.actor_waterlevel(0, key), 2);
    }

    #[test]
    fn waterlevel_from_udmf_waterzone() {
        let mut map = MapBuilder::new()
            .room("pool", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.sectors[0].waterzone = true;
        map.sectors[0].tags.push(5);
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        {
            let a = w.actors.get_mut(mo).unwrap();
            a.height = 56.0;
            a.pos = DVec3::new(128.0, 128.0, 0.0);
        }
        let key = pack_actor(mo);
        // Ceiling 128 >> head → fully submerged.
        assert_eq!(WorldAcsHost { world: &mut w }.actor_waterlevel(0, key), 3);
        assert_eq!(
            WorldAcsHost { world: &mut w }.udmf_int(2, 5, "waterzone"),
            1
        );
    }

    #[test]
    fn powerup_tics_and_sound_volume() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        w.players[0].as_mut().unwrap().invul_tics = 42;
        w.players[0].as_mut().unwrap().radsuit_tics = 17;
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.get_actor_powerup_tics(0, "PowerInvulnerable", key), 42);
            assert_eq!(host.get_actor_powerup_tics(0, "PowerIronFeet", key), 17);
            assert_eq!(host.get_actor_powerup_tics(0, "Nope", key), 0);
            host.play_sound_acs(0, "weapons/pistol", Fixed::from_f64(1.0).0, 0, key);
            host.sound_volume(0, 0, Fixed::from_f64(0.25).0, key);
        }
        assert_eq!(w.sound_events.len(), 1);
        assert!((w.sound_events[0].volume - 0.25).abs() < 1e-4);
    }

    #[test]
    fn roll_max_inventory_check_class_and_sky_scroll() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  SeeSound "player/male/sight"
  States { Spawn: PLAY A -1 Stop See: PLAY A 1 Loop }
}
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::class::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_actor_roll(0, Fixed::from_f64(0.25).0, key);
            assert_eq!(host.actor_roll(0, key), Fixed::from_f64(0.25).0);
            assert_eq!(host.get_max_inventory(0, "Clip", key), 200);
            assert_eq!(host.get_max_inventory(0, "Health", key), 100);
            assert_eq!(host.check_actor_state(0, "See", false, key), 1);
            assert_eq!(host.check_actor_state(0, "Missile", false, key), 0);
            assert_eq!(host.check_class("Clip"), 1);
            assert_eq!(host.check_class("Nope"), 0);
            host.set_sky_scroll_speed(1, Fixed::from_f64(2.0).0);
            host.play_actor_sound(0, 0, Fixed::from_f64(1.0).0, Fixed::from_f64(1.0).0, key);
        }
        assert!((w.sky_speed1 - 2.0).abs() < 1e-9);
        assert_eq!(w.sound_events.len(), 1);
        assert_eq!(w.sound_events[0].name, "player/male/sight");
        assert!((w.actors.get(mo).unwrap().roll.degrees() - 90.0).abs() < 1e-6);
    }

    #[test]
    fn ammo_capacity_actor_state_and_damage2() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop See: PLAY A 1 Loop }
}
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
actor Zombie : Actor 3004
{
  Health 20
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::class::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        let zom = w.classes.find_name("Zombie").unwrap();
        let zid = w
            .spawn_at(
                zom,
                room_core::DVec3::new(140.0, 128.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        w.actors.get_mut(zid).unwrap().tid = 7;
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.player_in_game(0), 1);
            assert_eq!(host.get_ammo_capacity("Clip", key), 200);
            host.set_ammo_capacity("Clip", 400, key);
            assert_eq!(host.give_actor_inventory(0, "Clip", 1, key), 1);
            assert_eq!(host.get_ammo_capacity("Clip", key), 400);
            assert_eq!(host.set_actor_state(0, "See", false, key), 1);
            assert_eq!(host.thing_damage2(7, 5, "None", key), 1);
            assert_eq!(host.use_actor_inventory(0, "Clip", key), 1);
            host.set_air_control(room_core::FRACUNIT / 4);
        }
        assert!((w.level.air_control - 0.25).abs() < 1e-6);
        assert_eq!(w.actors.get(zid).unwrap().health, 15);
    }

    #[test]
    fn gravity_levelinfo_sky_replace_textures() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.level.total_secrets = 3;
        w.level.found_secrets = 1;
        w.level.total_kills = 10;
        w.level.found_kills = 4;
        w.level.par_time = 30;
        w.level.suck_time = 90;
        w.level.cluster = 2;
        w.level.levelnum = 1;
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_gravity(room_core::FRACUNIT / 2);
            assert_eq!(host.level_info(room_acs::levelinfo::PAR_TIME), 30);
            assert_eq!(host.level_info(room_acs::levelinfo::SUCK_TIME), 90);
            assert_eq!(host.level_info(room_acs::levelinfo::CLUSTERNUM), 2);
            assert_eq!(host.level_info(room_acs::levelinfo::LEVELNUM), 1);
            assert_eq!(host.level_info(room_acs::levelinfo::TOTAL_SECRETS), 3);
            assert_eq!(host.level_info(room_acs::levelinfo::FOUND_SECRETS), 1);
            assert_eq!(host.level_info(room_acs::levelinfo::KILLED_MONSTERS), 4);
            host.change_sky("RSKY1", "RSKY2");
            host.replace_textures("FLOOR0_1", "FLAT5_1", 0);
        }
        assert!((w.level.gravity - 0.5).abs() < 1e-6);
        assert_eq!(w.sky1_override.as_deref(), Some("RSKY1"));
        assert_eq!(w.sky2_override.as_deref(), Some("RSKY2"));
        assert!(
            w.sector_floor_pics
                .values()
                .any(|v| v.eq_ignore_ascii_case("FLAT5_1"))
        );
    }

    #[test]
    fn air_supply_and_play_sound_acs() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.level.maptime = 100;
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let key = pack_actor(mo);
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.set_air_supply(0, 35), 1);
            assert_eq!(host.get_air_supply(0), 35);
            assert_eq!(host.get_air_supply(7), 0);
            host.play_sound_acs(
                0,
                "misc/chat",
                room_core::FRACUNIT,
                room_core::FRACUNIT,
                key,
            );
        }
        let ev = w.drain_sound_events();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].name, "misc/chat");
        assert!((ev[0].volume - 1.0).abs() < 1e-4);
        assert!((ev[0].attenuation - 1.0).abs() < 1e-4);
    }

    #[test]
    fn set_line_blocking_and_special_host() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        assert!(!w.map.lines.is_empty());
        let lid = room_core::LineId::from_raw(0);
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_line_blocking(0, crate::world::block_mode::NOTHING);
            host.set_line_special(0, 80, [4, 0, 0, 0, 0]);
        }
        assert_eq!(w.line_special(lid), 80);
        assert_eq!(w.line_args(lid), [4, 0, 0, 0, 0]);
        assert_eq!(w.line_flags(lid) & crate::world::ML_BLOCKING, 0);
        {
            let mut host = WorldAcsHost { world: &mut w };
            host.set_line_blocking(0, crate::world::block_mode::EVERYTHING);
        }
        assert_ne!(w.line_flags(lid) & crate::world::ML_BLOCKING, 0);
        assert_ne!(w.line_flags(lid) & crate::world::ML_BLOCKEVERYTHING, 0);
    }

    #[test]
    fn actor_inventory_acs_pcods() {
        let ir = room_decorate::parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
actor Clip : Actor 2007
{
  Inventory.Amount 10
  Inventory.MaxAmount 200
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::class::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(crate::StateTable::from_decorate(&ir, &classes));
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().tid = 5;
        {
            let mut host = WorldAcsHost { world: &mut w };
            assert_eq!(host.give_actor_inventory(5, "Clip", 7, 0), 1);
            assert_eq!(host.check_actor_inventory(5, "Clip", 0), 7);
            assert_eq!(host.take_actor_inventory(5, "Clip", 3, 0), 1);
            assert_eq!(host.check_actor_inventory(5, "Clip", 0), 4);
            host.clear_actor_inventory(5, 0);
            assert_eq!(host.check_actor_inventory(5, "Clip", 0), 0);
        }
    }

    #[test]
    fn bd5_acs_script_call_dispatches_to_zscript() {
        // docs/33 BD5/BD6: ACSF ScriptCall → ZandroCompat.* on the Room ZVM.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/zscript/bd_like");
        let mut sources = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                e.eq_ignore_ascii_case("zs")
                    || e.eq_ignore_ascii_case("zc")
                    || e.eq_ignore_ascii_case("zsc")
            }) {
                sources.push((
                    p.display().to_string(),
                    std::fs::read_to_string(&p).unwrap(),
                ));
            }
        }
        sources.sort_by(|a, b| {
            let score = |n: &str| {
                let base = n.rsplit(['/', '\\']).next().unwrap_or(n);
                if base.eq_ignore_ascii_case("zscript.zc") {
                    0
                } else {
                    1
                }
            };
            score(&a.0).cmp(&score(&b.0)).then_with(|| a.0.cmp(&b.0))
        });
        let result = room_zscript::check(&sources);
        assert!(result.ok(), "{:?}", result.diagnostics);
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.load_zscript_program(result.program.unwrap());
        w.acs.dyn_strings.push("ZandroCompat".into());
        w.acs.dyn_strings.push("ConsolePlayerNumber".into());
        const DYN: i32 = 0x4000_0000;
        let mut host = WorldAcsHost { world: &mut w };
        let v = host.script_call(&[DYN, DYN | 1], 0);
        assert_eq!(v, 0, "ConsolePlayerNumber via ACS ScriptCall; got {v}");
    }

    #[test]
    fn bd_lump_api_via_acs() {
        let src = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata/zscript/bd_real/BD_WCS.zc"),
        )
        .expect("BD_WCS.zc");
        let result = room_zscript::check(&[("BD_WCS.zc".into(), src)]);
        assert!(result.ok(), "{:?}", result.diagnostics);
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.load_zscript_program(result.program.unwrap());
        w.zscript.set_event_handlers(["ZC_TMapHack".into()]);
        w.zscript.activate_map_handlers().unwrap();
        let body: String = ['M', 'A', 'P', '\u{80}'].into_iter().collect();
        w.zscript.set_wad_lumps(std::collections::BTreeMap::from([(
            "MAP01".into(),
            body.clone(),
        )]));
        for name in [
            "ZandroCompat",
            "LumpOpen",
            "MAP01",
            "LumpRead",
            "LumpReadString",
            "LumpGetInfoInt",
            "LumpClose",
        ] {
            w.acs.dyn_strings.push(name.into());
        }
        const DYN: i32 = 0x4000_0000;
        let mut host = WorldAcsHost { world: &mut w };
        let lump = host.script_call(&[DYN, DYN | 1, DYN | 2, -2, 0], 0);
        assert_eq!(lump, 0, "LumpOpen MAP01");
        assert_eq!(
            host.script_call(&[DYN, DYN | 3, lump, 0, 1], 0),
            b'M' as i32
        );
        let _ = host.script_call(&[DYN, DYN | 4, lump, 0, 4], 0);
        assert_eq!(host.world.zscript.vm.as_ref().unwrap().last_s, body);
        assert_eq!(host.script_call(&[DYN, DYN | 5, lump, 0], 0), 4);
        let _ = host.script_call(&[DYN, DYN | 6, lump], 0);
        let _ = host.world.zscript.take_printf();
        assert_eq!(host.script_call(&[DYN, DYN | 3, lump, 0, 1], 0), 0);
        let msgs = host.world.zscript.take_printf();
        assert!(
            msgs.iter().any(|m| m.contains("non-existent")),
            "LumpRead after close prints, got {msgs:?}"
        );
    }

    #[test]
    fn bd_save_load_mid_map() {
        let src = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata/zscript/bd_real/BD_WCS.zc"),
        )
        .unwrap();
        let result = room_zscript::check(&[("BD_WCS.zc".into(), src)]);
        assert!(result.ok(), "{:?}", result.diagnostics);
        let prog = result.program.unwrap();
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let mut w = World::from_map(map).unwrap();
        w.load_zscript_program(prog.clone());
        w.zscript.set_event_handlers(["ZC_TMapHack".into()]);
        w.zscript.activate_map_handlers().unwrap();
        let body: String = ['M', 'A', 'P', '\u{80}'].into_iter().collect();
        w.zscript.set_wad_lumps(std::collections::BTreeMap::from([(
            "MAP01".into(),
            body.clone(),
        )]));
        w.zscript
            .script_call_with("ZandroCompat.LumpOpen", &[-2, 0], &["MAP01"])
            .unwrap();
        let checksum = w.zscript.checksum_bytes().unwrap();
        let blob = w.zscript.save_blob().unwrap();

        let mut w2 = World::from_map(
            MapBuilder::new()
                .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
                .player_start(64.0, 64.0, 0)
                .build_udmf("MAP01")
                .unwrap(),
        )
        .unwrap();
        w2.load_zscript_program(prog);
        w2.zscript.set_wad_lumps(std::collections::BTreeMap::from([(
            "MAP01".into(),
            body.clone(),
        )]));
        w2.zscript.load_blob(&blob, false).unwrap();
        assert_eq!(w2.zscript.checksum_bytes().unwrap(), checksum);
        w2.zscript
            .script_call_with("ZandroCompat.LumpReadString", &[0, 0, 4], &[])
            .unwrap();
        assert_eq!(w2.zscript.vm.as_ref().unwrap().last_s, body);
    }
}

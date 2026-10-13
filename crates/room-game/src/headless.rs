//! Headless simulation driver (`room_game::Headless`).

use crate::demo::{DemoBundle, DemoRecorder};
use crate::save::{self, SaveError};
use crate::sound::SoundSystem;
use room_audio::SoundEvent;
use room_defs::Complevel;
use room_map::MapGeometry;
use room_sim::{
    BossAction, ClassRegistry, CompatMode, MAX_PLAYERS, SimError, StateTable, TicCmd, World,
    cheat_give, math_mode_for_vanilla_demo,
};
use room_ui::HudState;
use room_vfs::Vfs;
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;

/// Skill level 1..=5 (Doom ITYTD…Nightmare).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skill(pub u8);

impl Skill {
    /// Construct, clamping to 1..=5.
    #[must_use]
    pub fn new(level: u8) -> Self {
        Self(level.clamp(1, 5))
    }
}

impl Default for Skill {
    fn default() -> Self {
        Self(3)
    }
}

/// Startup options for a headless session.
#[derive(Clone, Debug, Default)]
pub struct GameOptions {
    /// Skill.
    pub skill: Skill,
    /// RNG seed (`rngseed`).
    pub rng_seed: u32,
    /// Record a demo while ticking.
    pub record_demo: bool,
    /// `COMPLVL` / `compatmode` preset applied when a map starts.
    pub complevel: Complevel,
    /// MBF `-dogs N` — spawn friendly helper dogs at Player 2–4 starts (0..=3).
    pub helper_dogs: u8,
    /// Coop player count 1..=4 (spawns seats at Player 2–4 starts when >1).
    pub coop_players: u8,
}

/// Headless errors.
#[derive(Debug, Error)]
pub enum GameError {
    /// Sim / map setup.
    #[error("{0}")]
    Message(String),
    /// Wrapped sim error.
    #[error(transparent)]
    Sim(#[from] SimError),
    /// Save / load failure.
    #[error(transparent)]
    Save(#[from] SaveError),
}

/// High-level game state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    /// No map loaded.
    Title,
    /// Playing a level.
    Level,
    /// Intermission (stub).
    Intermission,
    /// End-of-episode / cast finale text screen (MAPINFO sequences later).
    Finale,
}

/// Headless game: load map → tick with TicCmds → checksum / demo.
pub struct Headless {
    /// Options.
    pub options: GameOptions,
    /// Current state.
    pub state: GameState,
    /// Active world (when in Level).
    world: Option<World>,
    /// Map lump name.
    map_name: String,
    /// Optional demo recorder (current map).
    demo: Option<DemoRecorder>,
    /// Finished map demos when recording a multi-map session.
    demo_bundle: DemoBundle,
    /// Class registry shared with the world.
    classes: Arc<ClassRegistry>,
    /// State table.
    states: Arc<StateTable>,
    /// Tallies captured when entering Intermission.
    pub intermission: Option<IntermissionTallies>,
    /// Map kept for title → start / intermission → restart.
    pending_map: Option<MapGeometry>,
    /// Optional SNDINFO + sample bank for draining sim sound events.
    pub sounds: Option<SoundSystem>,
    /// Optional MAPINFO for next/secretnext/par (falls back to Doom II heuristics).
    mapinfo: Option<Arc<room_defs::MapInfo>>,
    /// Optional TERRAIN table for hit-floor splash.
    terrain: room_sim::TerrainTable,
    /// Optional SNDSEQ table for sequence playback.
    sndseq: room_sim::SndSeqTable,
    /// Optional LOCKDEFS table for locked doors.
    locks: room_sim::LockTable,
    /// DECALDEF generator class → decal name.
    decal_generators: BTreeMap<String, String>,
    /// KEYCONF weapon slots / player classes.
    weapon_slots: room_sim::WeaponSlots,
    /// Full KEYCONF (shell applies alias / defaultbind).
    keyconf: Option<room_defs::KeyConf>,
    /// MENUDEF registry (for UI shells).
    menudef: Option<room_defs::MenuDef>,
    /// Preloaded ACS library blobs (`LOADACS` / resolved `LOAD` chunk).
    acs_libraries: Vec<Vec<u8>>,
    /// Per-map `compatibility.txt` database (`sv_nomapfixes` clears this).
    compat_db: Option<Arc<room_map::CompatDatabase>>,
    /// Per-map `levelfixes.ron` geometry edits (`sv_nomapfixes` skips).
    level_fixes: Option<Arc<room_map::LevelFixesDatabase>>,
    /// When true, skip applying [`Self::compat_db`] / [`Self::level_fixes`] (`sv_nomapfixes`).
    pub sv_nomapfixes: bool,
    /// In-memory hub map snapshots (keyed by uppercase map lump name).
    hub_snapshots: BTreeMap<String, HubSnapshot>,
}

/// Per-map hub state preserved while travelling within a hub cluster.
#[derive(Clone, Debug)]
struct HubSnapshot {
    /// ACS map variables.
    map_vars: [i32; 128],
    /// Dynamic sector floor heights.
    sector_floors: Vec<f64>,
    /// Dynamic sector ceiling heights.
    sector_ceils: Vec<f64>,
}

fn apply_keyconf(slots: &mut room_sim::WeaponSlots, conf: &room_defs::KeyConf) {
    for cmd in &conf.commands {
        match cmd {
            room_defs::KeyConfCmd::WeaponSection(name) => slots.set_section(name),
            room_defs::KeyConfCmd::SetSlot { slot, weapons } => {
                slots.set_slot(*slot, weapons);
            }
            room_defs::KeyConfCmd::AddSlot { slot, weapon } => {
                slots.add_slot(*slot, weapon);
            }
            room_defs::KeyConfCmd::AddSlotDefault { slot, weapon } => {
                slots.add_slot_default(*slot, weapon);
            }
            room_defs::KeyConfCmd::ClearPlayerClasses => slots.clear_player_classes(),
            room_defs::KeyConfCmd::AddPlayerClass { class, nomenu } => {
                slots.add_player_class(class, *nomenu);
            }
            // Binds / aliases / menu keys are applied by the shell (`apply_keyconf_shell`).
            room_defs::KeyConfCmd::Alias { .. }
            | room_defs::KeyConfCmd::DefaultBind { .. }
            | room_defs::KeyConfCmd::AddKeySection { .. }
            | room_defs::KeyConfCmd::AddMenuKey { .. } => {}
        }
    }
}

/// Console `alias` lines from KEYCONF (shell applies via [`room_console::Console::exec`]).
#[must_use]
pub fn keyconf_alias_lines(conf: &room_defs::KeyConf) -> Vec<String> {
    conf.commands
        .iter()
        .filter_map(|cmd| match cmd {
            room_defs::KeyConfCmd::Alias { name, body } => {
                let body = body.trim().trim_matches('"');
                Some(format!("alias {name} {body}"))
            }
            _ => None,
        })
        .collect()
}

/// Default key→command pairs from KEYCONF (`defaultbind`).
#[must_use]
pub fn keyconf_default_binds(conf: &room_defs::KeyConf) -> Vec<(String, String)> {
    conf.commands
        .iter()
        .filter_map(|cmd| match cmd {
            room_defs::KeyConfCmd::DefaultBind { key, cmd } => Some((key.clone(), cmd.clone())),
            _ => None,
        })
        .collect()
}

fn locks_from_defs(info: &room_defs::LockDefs, game: Option<&str>) -> room_sim::LockTable {
    let mut table = room_sim::LockTable {
        locks: Vec::with_capacity(info.locks.len()),
        game: game.map(str::to_owned),
    };
    for lock in &info.locks {
        table.locks.push(room_sim::LockSpec {
            number: lock.number,
            game: lock.game.clone(),
            keys: lock
                .keys
                .iter()
                .map(|k| match k {
                    room_defs::LockKeyReq::Class(c) => room_sim::LockKeyReq::Class(c.clone()),
                    room_defs::LockKeyReq::Any(v) => room_sim::LockKeyReq::Any(v.clone()),
                })
                .collect(),
            message: lock.message.clone(),
            locked_sound: lock.locked_sound.clone(),
        });
    }
    table
}

fn sndseq_from_defs(reg: &room_defs::SoundSeqRegistry) -> room_sim::SndSeqTable {
    use room_defs::SeqCommand;
    use room_sim::{SndSeqDef, SndSeqStep};
    let mut table = room_sim::SndSeqTable::default();
    for (name, seq) in &reg.sequences {
        let mut steps = Vec::new();
        let mut stopsound = None;
        for cmd in &seq.commands {
            match cmd {
                SeqCommand::Play(s) | SeqCommand::PlayUntilDone(s) => {
                    steps.push(SndSeqStep::Play(s.clone()));
                }
                SeqCommand::PlayRepeat(s) => {
                    steps.push(SndSeqStep::PlayRepeat(s.clone()));
                }
                SeqCommand::PlayTime { sound, tics } => {
                    steps.push(SndSeqStep::Play(sound.clone()));
                    steps.push(SndSeqStep::Delay(
                        (*tics).clamp(0, i64::from(i32::MAX)) as i32
                    ));
                }
                SeqCommand::PlayLoop { sound, tics } => {
                    steps.push(SndSeqStep::PlayLoop {
                        sound: sound.clone(),
                        tics: (*tics).clamp(0, i64::from(i32::MAX)) as i32,
                    });
                }
                SeqCommand::Delay(t) => {
                    steps.push(SndSeqStep::Delay((*t).clamp(0, i64::from(i32::MAX)) as i32));
                }
                SeqCommand::DelayRand { min, .. } => {
                    steps.push(SndSeqStep::Delay(
                        (*min).clamp(0, i64::from(i32::MAX)) as i32
                    ));
                }
                SeqCommand::Volume(v) => {
                    steps.push(SndSeqStep::Volume(((*v) / 100.0).clamp(0.0, 1.0) as f32));
                }
                SeqCommand::VolumeRel(v) => {
                    // SNDSEQ volumerel is typically 0–100 scale → multiply factor.
                    steps.push(SndSeqStep::VolumeRel(((*v) / 100.0).clamp(0.0, 4.0) as f32));
                }
                SeqCommand::Attenuation(v) => {
                    steps.push(SndSeqStep::Attenuation((*v).max(0.0) as f32));
                }
                SeqCommand::VolumeRand { min, .. } => {
                    steps.push(SndSeqStep::Volume(((*min) / 100.0).clamp(0.0, 1.0) as f32));
                }
                SeqCommand::StopSound(s) => {
                    stopsound = Some(s.clone());
                }
                SeqCommand::NoStopCutoff
                | SeqCommand::Door(_)
                | SeqCommand::Platform(_)
                | SeqCommand::Environment(_)
                | SeqCommand::Slot(_) => {}
            }
        }
        steps.push(SndSeqStep::End);
        let key = name.to_ascii_uppercase();
        table.sequences.insert(
            key.clone(),
            SndSeqDef {
                name: key,
                steps,
                stopsound,
            },
        );
    }
    for (&slot, seq) in &reg.door_slots {
        table
            .door_slots
            .insert(slot as i32, seq.to_ascii_uppercase());
    }
    for (&slot, seq) in &reg.platform_slots {
        table
            .platform_slots
            .insert(slot as i32, seq.to_ascii_uppercase());
    }
    table
}

fn terrain_from_defs(info: &room_defs::TerrainInfo) -> room_sim::TerrainTable {
    let mut table = room_sim::TerrainTable::default();
    table.floors = info.floors.clone();
    table.default_terrain = info.default_terrain.clone();
    for (name, t) in &info.terrains {
        table.terrains.insert(
            name.clone(),
            room_sim::TerrainSpec {
                splash: t.splash.clone(),
                liquid: t.liquid,
                damage_on_land: if t.damage_on_land {
                    t.damage_amount.unwrap_or(0)
                } else {
                    0
                },
            },
        );
    }
    for (name, s) in &info.splashes {
        table.splashes.insert(
            name.clone(),
            room_sim::SplashSpec {
                small_class: s.small_class.clone(),
                base_class: s.base_class.clone(),
                chunk_class: s.chunk_class.clone(),
                sound: s.sound.clone(),
                small_sound: s.small_sound.clone(),
                small_threshold: 12,
            },
        );
    }
    table
}

fn complevel_to_compat(c: Complevel) -> CompatMode {
    match c {
        Complevel::Default => CompatMode::Default,
        Complevel::Vanilla => CompatMode::Doom,
        Complevel::Boom => CompatMode::Boom,
        Complevel::Mbf => CompatMode::Mbf,
        Complevel::Mbf21 => CompatMode::Mbf21,
    }
}

/// Apply `COMPLVL` → compatflags + math mode on a fresh world.
pub fn apply_complevel_to_world(world: &mut World, complevel: Complevel) {
    world.compat = complevel_to_compat(complevel).flags();
    world.set_vanilla_demo_mode(matches!(complevel, Complevel::Vanilla));
}

/// Parse UMAPINFO `bossaction` extras (`Class,special,tag|…`).
fn parse_boss_actions(raw: Option<&String>) -> Vec<BossAction> {
    let Some(s) = raw else {
        return Vec::new();
    };
    if s.eq_ignore_ascii_case("clear") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for entry in s.split('|') {
        let mut parts = entry.split(',').map(str::trim);
        let Some(class) = parts.next() else {
            continue;
        };
        if class.is_empty() || class.eq_ignore_ascii_case("clear") {
            continue;
        }
        let Some(special) = parts.next().and_then(|p| p.parse::<i32>().ok()) else {
            continue;
        };
        let tag = parts
            .next()
            .and_then(|p| p.parse::<i32>().ok())
            .unwrap_or(0);
        out.push(BossAction {
            class: class.to_ascii_lowercase(),
            special,
            tag,
        });
    }
    out
}

impl Headless {
    /// Create a session with empty registries (call [`Self::set_defs`] or use defaults).
    #[must_use]
    pub fn new(options: GameOptions) -> Self {
        Self {
            options,
            state: GameState::Title,
            world: None,
            map_name: String::new(),
            demo: None,
            demo_bundle: DemoBundle::new(),
            classes: Arc::new(ClassRegistry::new()),
            states: Arc::new(StateTable::new()),
            intermission: None,
            pending_map: None,
            sounds: None,
            mapinfo: None,
            terrain: room_sim::TerrainTable::default(),
            sndseq: room_sim::SndSeqTable::default(),
            locks: room_sim::LockTable::default(),
            decal_generators: BTreeMap::new(),
            weapon_slots: room_sim::WeaponSlots::with_doom_defaults(),
            keyconf: None,
            menudef: None,
            acs_libraries: Vec::new(),
            compat_db: None,
            level_fixes: None,
            sv_nomapfixes: false,
            hub_snapshots: BTreeMap::new(),
        }
    }

    /// Install class / state tables (from DECORATE).
    pub fn set_defs(&mut self, classes: Arc<ClassRegistry>, states: Arc<StateTable>) {
        self.classes = classes;
        self.states = states;
    }

    /// Install MAPINFO (next / secretnext / par overrides).
    pub fn set_mapinfo(&mut self, mapinfo: Arc<room_defs::MapInfo>) {
        self.mapinfo = Some(mapinfo);
    }

    /// Install TERRAIN (from compiled [`room_defs::TerrainInfo`]).
    pub fn set_terrain(&mut self, info: &room_defs::TerrainInfo) {
        self.terrain = terrain_from_defs(info);
    }

    /// Install SNDSEQ (from compiled [`room_defs::SoundSeqRegistry`]).
    pub fn set_sndseq(&mut self, reg: &room_defs::SoundSeqRegistry) {
        self.sndseq = sndseq_from_defs(reg);
        if let Some(w) = self.world.as_mut() {
            w.sndseq = self.sndseq.clone();
        }
    }

    /// Install LOCKDEFS (from compiled [`room_defs::LockDefs`]).
    pub fn set_lockdefs(&mut self, info: &room_defs::LockDefs, game: Option<&str>) {
        self.locks = locks_from_defs(info, game);
    }

    /// Install DECALDEF generators (class → decal name).
    pub fn set_decaldef(&mut self, info: &room_defs::DecalInfo) {
        self.decal_generators.clear();
        for (class, decal) in &info.generators {
            self.decal_generators
                .insert(class.to_ascii_lowercase(), decal.clone());
        }
    }

    /// Apply KEYCONF weapon / playerclass commands onto the session slots table.
    pub fn set_keyconf(&mut self, conf: &room_defs::KeyConf) {
        apply_keyconf(&mut self.weapon_slots, conf);
        self.keyconf = Some(conf.clone());
    }

    /// Borrow KEYCONF for shell alias / bind apply.
    #[must_use]
    pub fn keyconf(&self) -> Option<&room_defs::KeyConf> {
        self.keyconf.as_ref()
    }

    /// Install MENUDEF for UI shells.
    pub fn set_menudef(&mut self, defs: room_defs::MenuDef) {
        self.menudef = Some(defs);
    }

    /// Borrow compiled MENUDEF, if any.
    #[must_use]
    pub fn menudef(&self) -> Option<&room_defs::MenuDef> {
        self.menudef.as_ref()
    }

    /// Install ACS library bytecode blobs (from `LOADACS` / VFS `acs/` lumps).
    pub fn set_acs_libraries(&mut self, libs: Vec<Vec<u8>>) {
        self.acs_libraries = libs;
    }

    /// Append one ACS library blob for subsequent map starts.
    pub fn push_acs_library_bytes(&mut self, bytes: Vec<u8>) {
        self.acs_libraries.push(bytes);
    }

    /// Apply a `COMPLVL` / `compatmode` preset for subsequent map starts.
    pub fn set_complevel(&mut self, complevel: Complevel) {
        self.options.complevel = complevel;
    }

    /// Install per-map compatibility flag database (`compat/flags.txt`).
    pub fn set_compat_db(&mut self, db: Arc<room_map::CompatDatabase>) {
        self.compat_db = Some(db);
    }

    /// Install per-map geometry/special fixes (`compat/levelfixes.ron`).
    pub fn set_level_fixes(&mut self, db: Arc<room_map::LevelFixesDatabase>) {
        self.level_fixes = Some(db);
    }

    /// Install a [`SoundSystem`] (SNDINFO + PCM bank).
    pub fn set_sounds(&mut self, sounds: SoundSystem) {
        self.sounds = Some(sounds);
    }

    /// Drain sim sound events through the sound system (unknown names skipped).
    pub fn drain_audio(&mut self, vfs: &Vfs) -> Vec<SoundEvent> {
        let Some(world) = self.world.as_mut() else {
            return Vec::new();
        };
        let events = world.drain_sound_events();
        let Some(sounds) = self.sounds.as_mut() else {
            return Vec::new();
        };
        sounds.translate(vfs, &events, world.level.maptime)
    }

    /// Take a pending ACS `SetMusic` request, if any.
    pub fn take_music_request(&mut self) -> Option<String> {
        self.world.as_mut()?.music_request.take()
    }

    /// Status-bar numbers for player `slot` (None if empty / not in level).
    #[must_use]
    pub fn hud_state(&self, slot: usize, fullscreen: bool) -> Option<HudState> {
        let hud = self.world.as_ref()?.player_hud(slot)?;
        Some(
            HudState::from_player(
                hud.health,
                hud.armor,
                hud.ammo,
                hud.keys,
                hud.face,
                hud.weapon_patch,
                fullscreen,
            )
            .with_flash(hud.flash_patch)
            .with_bob(hud.bob_x, hud.bob_y)
            .with_ammo_max(hud.ammo_max),
        )
    }

    /// Queue a map and stay on the title screen until [`Self::begin_pending_map`].
    pub fn set_pending_map(&mut self, map: MapGeometry) {
        self.pending_map = Some(map);
        self.state = GameState::Title;
        self.world = None;
        self.intermission = None;
    }

    /// Start the queued map (title / intermission → level).
    pub fn begin_pending_map(&mut self) -> Result<(), GameError> {
        let map = self
            .pending_map
            .clone()
            .ok_or_else(|| GameError::Message("no pending map".into()))?;
        self.start_custom_map(map, self.options.skill)
    }

    /// Start a custom map geometry (testkit / tools).
    pub fn start_custom_map(&mut self, map: MapGeometry, skill: Skill) -> Result<(), GameError> {
        self.options.skill = skill;
        self.map_name = map.name.clone();
        let map_key = self.map_name.to_ascii_uppercase();
        // Carry ACS world/global vars across hub maps; restore full snapshot on return.
        let prev_acs = self.world.as_ref().map(|w| {
            (
                w.acs.world_vars,
                w.acs.global_vars,
                w.players[0]
                    .as_ref()
                    .and_then(|p| p.mo)
                    .and_then(|mo| w.actors.get(mo))
                    .map(|a| a.health)
                    .unwrap_or(100),
            )
        });
        let hub_return = self.hub_snapshots.remove(&map_key);
        let hub_returning = hub_return.is_some();
        let mut map = map;
        if !self.sv_nomapfixes {
            if let Some(db) = self.level_fixes.as_ref() {
                db.apply_to_map(&mut map);
            }
        }
        self.pending_map = Some(map.clone());
        let math_mode =
            math_mode_for_vanilla_demo(matches!(self.options.complevel, Complevel::Vanilla));
        let mut world = World::from_map_with_skill_mode(
            map,
            Arc::clone(&self.classes),
            Arc::clone(&self.states),
            skill.0,
            math_mode,
        )?;
        apply_complevel_to_world(&mut world, self.options.complevel);
        world.terrain = self.terrain.clone();
        world.sndseq = self.sndseq.clone();
        world.locks = self.locks.clone();
        world.decal_generators = self.decal_generators.clone();
        world.weapon_slots = self.weapon_slots.clone();
        if !self.sv_nomapfixes {
            if let Some(db) = self.compat_db.as_ref() {
                let hex = room_map::format_md5(&world.map.md5);
                let (f1, f2) = db.bits_for(&hex);
                world.compat.merge(f1, f2);
            }
        }
        if let Some(info) = self.mapinfo.as_ref() {
            if let Some(def) = info.maps.get(&map_key) {
                world.boss_actions = parse_boss_actions(def.extras.get("bossaction"));
                world.level.par_time = def.par_seconds.unwrap_or(0);
                world.level.cluster = def.cluster.unwrap_or(0);
                if let Some(fd) = def.falling_damage {
                    world.level.falling_damage = fd;
                }
                if def.clip_midtextures == Some(true) {
                    let map = std::sync::Arc::make_mut(&mut world.map);
                    for line in &mut map.lines {
                        line.clip_midtex = true;
                    }
                    for side in &mut map.sides {
                        side.clip_midtex = true;
                    }
                }
                if def.wrap_midtextures == Some(true) {
                    let map = std::sync::Arc::make_mut(&mut world.map);
                    for line in &mut map.lines {
                        line.wrap_midtex = true;
                    }
                    for side in &mut map.sides {
                        side.wrap_midtex = true;
                    }
                }
                if let Some(n) = def
                    .extras
                    .get("levelnum")
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    world.level.levelnum = n;
                }
                if let Some(n) = def
                    .extras
                    .get("sucktime")
                    .or_else(|| def.extras.get("suck"))
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    world.level.suck_time = n;
                }
            }
        }
        if world.level.par_time == 0 {
            if let Some(p) = default_par_seconds(&map_key) {
                world.level.par_time = p;
            }
        }
        world.rng.set_seed(self.options.rng_seed);
        let coop = self.options.coop_players.clamp(1, 4);
        if coop > 1 {
            let _ = world.spawn_coop_players(coop)?;
        }
        if self.options.helper_dogs > 0 {
            let _ = world.spawn_helper_dogs(self.options.helper_dogs)?;
        }
        if let Some(snap) = hub_return {
            world.acs.map_vars = snap.map_vars;
            for (i, s) in world.sectors.iter_mut().enumerate() {
                if let Some(&z) = snap.sector_floors.get(i) {
                    s.floor_z = z;
                }
                if let Some(&z) = snap.sector_ceils.get(i) {
                    s.ceiling_z = z;
                }
            }
        }
        if let Some((world_vars, global_vars, travel_hp)) = prev_acs {
            // Hub-persistent ACS vars.
            if self
                .mapinfo
                .as_ref()
                .and_then(|info| info.cluster_of(&map_key))
                .is_some_and(|id| {
                    self.mapinfo
                        .as_ref()
                        .is_some_and(|info| info.is_hub_cluster(id))
                })
            {
                world.acs.world_vars = world_vars;
                world.acs.global_vars = global_vars;
                // Bootstrap player travel: keep health across hub maps.
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.health = travel_hp;
                    }
                }
            }
        }
        if self.options.record_demo {
            if let Some(prev) = self.demo.take() {
                if !prev.tics.is_empty() {
                    self.demo_bundle.push(prev);
                }
            }
            self.demo = Some(DemoRecorder::start_with_complevel(
                self.map_name.clone(),
                self.options.rng_seed,
                self.options.complevel,
            ));
        }
        // LOADACS / pre-resolved library blobs (after BEHAVIOR; OPEN already ran).
        for blob in &self.acs_libraries {
            let _ = room_sim::push_acs_library(&mut world, blob);
        }
        // ENTER on first visit; RETURN when restoring a hub snapshot.
        if hub_returning {
            room_sim::start_return_scripts(&mut world);
        } else {
            room_sim::start_enter_scripts(&mut world);
        }
        self.world = Some(world);
        self.intermission = None;
        self.state = GameState::Level;
        Ok(())
    }

    /// Whether a hub snapshot is stored for `map` (uppercase lump name).
    #[must_use]
    pub fn has_hub_snapshot(&self, map: &str) -> bool {
        self.hub_snapshots.contains_key(&map.to_ascii_uppercase())
    }

    /// Leave intermission back to title (keeps pending map).
    pub fn leave_intermission(&mut self) {
        if self.state == GameState::Intermission {
            self.state = GameState::Title;
            self.world = None;
            self.intermission = None;
        }
    }

    /// Abandon the current level / intermission and return to the title screen.
    ///
    /// Keeps `pending_map` so New Game can restart without remounting the IWAD.
    pub fn return_to_title(&mut self) {
        self.state = GameState::Title;
        self.world = None;
        self.intermission = None;
    }

    /// Intermission → finale when there is no next map (episode / game end).
    pub fn enter_finale(&mut self) {
        if self.state == GameState::Intermission {
            self.state = GameState::Finale;
            self.world = None;
        }
    }

    /// Leave finale back to the title screen.
    pub fn leave_finale(&mut self) {
        if self.state == GameState::Finale {
            self.state = GameState::Title;
            self.world = None;
            self.intermission = None;
        }
    }

    /// After intermission, load `map` as the next level (same skill / defs).
    pub fn continue_to_map(&mut self, map: MapGeometry) -> Result<(), GameError> {
        if self.state != GameState::Intermission && self.state != GameState::Title {
            return Err(GameError::Message(
                "continue_to_map only from Intermission or Title".into(),
            ));
        }
        self.start_custom_map(map, self.options.skill)
    }

    /// Suggested next map lump for Doom II / FreeDoom / Doom 1 episode naming.
    ///
    /// Secret exits: MAP15→MAP31, MAP31→MAP32; ExM3→ExM9. Secret cluster
    /// returns to MAP16 after MAP31/MAP32.
    #[must_use]
    pub fn default_next_map(current: &str, secret_exit: bool) -> Option<String> {
        default_next_map_name(current, secret_exit)
    }

    /// Current map lump name (empty before the first level).
    #[must_use]
    pub fn map_name(&self) -> &str {
        &self.map_name
    }

    /// Borrow the world (Level only).
    #[must_use]
    pub fn world(&self) -> Option<&World> {
        self.world.as_ref()
    }

    /// Mutable world.
    pub fn world_mut(&mut self) -> Option<&mut World> {
        self.world.as_mut()
    }

    /// One game tic. `cmds` are copied into player slots (pad/truncate to [`MAX_PLAYERS`]).
    pub fn tick(&mut self, cmds: &[TicCmd]) {
        if self.state != GameState::Level {
            return;
        }
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let mut full = [TicCmd::default(); MAX_PLAYERS];
        for (i, c) in cmds.iter().enumerate().take(MAX_PLAYERS) {
            full[i] = *c;
        }
        world.set_ticcmds(&full);
        world.tick();
        if world.level.exiting {
            room_sim::start_unloading_scripts(world);
            let key = self.map_name.to_ascii_uppercase();
            let par = self
                .mapinfo
                .as_ref()
                .and_then(|m| m.maps.get(&key))
                .and_then(|d| d.par_seconds);
            let mut tallies =
                IntermissionTallies::from_level_with_par(self.map_name.clone(), &world.level, par);
            // `Teleport_NewMap` / `Teleport_EndGame` win over MAPINFO next.
            let forced = world
                .level
                .next_map_override
                .as_ref()
                .is_some_and(|s| !s.is_empty())
                || world.level.next_map_override.as_deref() == Some("");
            if !forced {
                if let Some(info) = self.mapinfo.as_ref() {
                    if let Some(def) = info.maps.get(&key) {
                        tallies.next_map = if tallies.secret_exit {
                            def.secret_next
                                .clone()
                                .or_else(|| def.next.clone())
                                .or(tallies.next_map)
                        } else {
                            def.next.clone().or(tallies.next_map)
                        };
                    }
                }
            }
            // Hub travel: snapshot the leaving map when the next map shares a hub cluster.
            if let Some(next) = tallies.next_map.as_deref() {
                if self
                    .mapinfo
                    .as_ref()
                    .is_some_and(|info| info.same_hub(&self.map_name, next))
                {
                    let key = self.map_name.to_ascii_uppercase();
                    self.hub_snapshots.insert(
                        key,
                        HubSnapshot {
                            map_vars: world.acs.map_vars,
                            sector_floors: world.sectors.iter().map(|s| s.floor_z).collect(),
                            sector_ceils: world.sectors.iter().map(|s| s.ceiling_z).collect(),
                        },
                    );
                }
            }
            self.intermission = Some(tallies);
            self.state = GameState::Intermission;
        }
        if let Some(demo) = &mut self.demo {
            demo.record_tic(world, &full[..cmds.len().clamp(1, MAX_PLAYERS)]);
        }
    }

    /// Run `n` idle tics.
    pub fn run_tics(&mut self, n: u32) {
        let cmd = [TicCmd::default()];
        for _ in 0..n {
            self.tick(&cmd);
        }
    }

    /// Serialize the current level to a `.fsave` blob.
    pub fn save_game(&mut self) -> Result<Vec<u8>, GameError> {
        if self.state != GameState::Level {
            return Err(GameError::Message("can only save during Level".into()));
        }
        let map = self.map_name.clone();
        let skill = self.options.skill.0;
        let world = self
            .world
            .as_mut()
            .ok_or_else(|| GameError::Message("no world to save".into()))?;
        Ok(save::save_world(world, &map, skill)?)
    }

    /// Restore player/sector state from a `.fsave` into the current world.
    pub fn load_game(&mut self, bytes: &[u8]) -> Result<(), GameError> {
        let _info = save::read_save_info(bytes)?;
        let world = self
            .world
            .as_mut()
            .ok_or_else(|| GameError::Message("no world to load into".into()))?;
        let _ = save::load_world(world, bytes)?;
        self.state = GameState::Level;
        self.intermission = None;
        Ok(())
    }

    /// Toggle god mode for player `slot` (console `god` / `iddqd`).
    pub fn cheat_god(&mut self, slot: usize) -> bool {
        self.world_mut()
            .map(|w| w.toggle_god(slot))
            .unwrap_or(false)
    }

    /// Toggle noclip for player `slot`.
    pub fn cheat_noclip(&mut self, slot: usize) -> bool {
        self.world_mut()
            .map(|w| w.toggle_noclip(slot))
            .unwrap_or(false)
    }

    /// Toggle level freeze (`freeze` console cheat).
    pub fn cheat_freeze(&mut self) -> bool {
        self.world_mut().map(World::toggle_freeze).unwrap_or(false)
    }

    /// Toggle automap reveal (`iddt`).
    pub fn cheat_iddt(&mut self, slot: usize) -> bool {
        self.world_mut()
            .map(|w| w.toggle_iddt(slot))
            .unwrap_or(false)
    }

    /// Print player position (`idmypos`).
    pub fn cheat_idmypos(&mut self, slot: usize) -> bool {
        self.world_mut()
            .map(|w| w.report_idmypos(slot))
            .unwrap_or(false)
    }

    /// Toggle pause.
    pub fn cheat_pause(&mut self) -> bool {
        self.world_mut().map(World::toggle_pause).unwrap_or(false)
    }

    /// `summon <class>` in front of player `slot`.
    pub fn cheat_summon(&mut self, slot: usize, class_name: &str) -> bool {
        self.world_mut()
            .and_then(|w| w.summon_in_front(slot, class_name))
            .is_some()
    }

    /// Console `give` for player `slot` (`all`, `weapons`, class name, …).
    pub fn cheat_give(&mut self, slot: usize, what: &str) -> u32 {
        let Some(world) = self.world.as_mut() else {
            return 0;
        };
        let Some(mo) = world
            .players
            .get(slot)
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return 0;
        };
        cheat_give(world, mo, what)
    }

    /// Console `kill monsters` (or bare `kill` = monsters in M5).
    pub fn cheat_kill_monsters(&mut self) -> u32 {
        self.world_mut().map(World::kill_monsters).unwrap_or(0)
    }

    /// Console `kill` / `kill self` — fatal damage to local player pawn.
    pub fn cheat_kill_self(&mut self, slot: usize) -> bool {
        let Some(world) = self.world.as_mut() else {
            return false;
        };
        let Some(mo) = world
            .players
            .get(slot)
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return false;
        };
        let hp = world.actors.get(mo).map(|a| a.health).unwrap_or(0);
        let dealt = room_sim::damage_mobj(world, mo, None, None, hp.max(1) + 10_000);
        dealt > 0
            || world
                .players
                .get(slot)
                .and_then(|p| p.as_ref())
                .is_some_and(|p| p.player_state == room_sim::PlayerState::Dead)
    }

    /// World checksum if a level is loaded.
    #[must_use]
    pub fn checksum(&self) -> Option<u64> {
        self.world.as_ref().map(World::checksum)
    }

    /// Take the demo recording for the current map (if any).
    pub fn take_demo(&mut self) -> Option<DemoRecorder> {
        self.demo.take()
    }

    /// Take the multi-map demo bundle (flushes the current map first).
    pub fn take_demo_bundle(&mut self) -> DemoBundle {
        if let Some(cur) = self.demo.take() {
            if !cur.tics.is_empty() {
                self.demo_bundle.push(cur);
            }
        }
        std::mem::take(&mut self.demo_bundle)
    }

    /// Snapshot end-of-level tallies (kills/items/secrets percents + time).
    #[must_use]
    pub fn intermission_stats(&self) -> Option<IntermissionTallies> {
        let world = self.world.as_ref()?;
        Some(IntermissionTallies::from_level(
            self.map_name.clone(),
            &world.level,
        ))
    }
}

/// End-of-level tally values for UI / demo metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntermissionTallies {
    /// Map lump name.
    pub map_name: String,
    /// Kill percent.
    pub kills: u8,
    /// Item percent.
    pub items: u8,
    /// Secret percent.
    pub secrets: u8,
    /// Level time in tics.
    pub time_tics: u32,
    /// Par time in tics (`0` = unknown / omit).
    pub par_tics: u32,
    /// Secret exit used.
    pub secret_exit: bool,
    /// Suggested next map lump (`None` = end of episode / unknown).
    pub next_map: Option<String>,
}

impl IntermissionTallies {
    /// Fill percents from a live [`room_sim::LevelState`].
    #[must_use]
    pub fn from_level(map_name: impl Into<String>, lvl: &room_sim::LevelState) -> Self {
        Self::from_level_with_par(map_name, lvl, None)
    }

    /// Like [`Self::from_level`], optionally overriding par from MAPINFO seconds.
    #[must_use]
    pub fn from_level_with_par(
        map_name: impl Into<String>,
        lvl: &room_sim::LevelState,
        par_seconds: Option<u32>,
    ) -> Self {
        let map_name = map_name.into();
        let next_map = match lvl.next_map_override.as_deref() {
            Some("") => None, // `Teleport_EndGame`
            Some(name) => Some(name.to_owned()),
            None => default_next_map_name(&map_name, lvl.secret_exit),
        };
        let par_secs = par_seconds.or_else(|| default_par_seconds(&map_name));
        Self {
            map_name,
            kills: lvl.kills_percent(),
            items: lvl.items_percent(),
            secrets: lvl.secrets_percent(),
            time_tics: lvl.maptime,
            par_tics: par_secs.map(|s| s.saturating_mul(35)).unwrap_or(0),
            secret_exit: lvl.secret_exit,
            next_map,
        }
    }
}

/// Doom II / Freedoom Phase 2 default par times (seconds).
#[must_use]
pub fn default_par_seconds(map: &str) -> Option<u32> {
    let name = map.trim().to_ascii_uppercase();
    let n = name.strip_prefix("MAP")?.parse::<u32>().ok()?;
    Some(match n {
        1 => 30,
        2 => 90,
        3 | 4 | 7 | 8 | 31 => 120,
        5 | 10 => 90,
        6 | 12 | 13 | 14 | 16 | 18 | 20 | 22 | 24 | 25 => 150,
        9 => 270,
        11 | 15 | 19 => 210,
        17 | 28 => 420,
        21 => 240,
        23 => 180,
        26 | 29 => 300,
        27 => 330,
        30 => 180,
        32 => 30,
        _ => return None,
    })
}

/// Doom II `MAPxx` / Doom 1 `ExMy` next-map heuristic (full MAPINFO later).
#[must_use]
pub fn default_next_map_name(current: &str, secret_exit: bool) -> Option<String> {
    let name = current.trim().to_ascii_uppercase();
    if let Some(rest) = name.strip_prefix("MAP") {
        if let Ok(n) = rest.parse::<u32>() {
            if secret_exit {
                return match n {
                    15 => Some("MAP31".into()),
                    31 => Some("MAP32".into()),
                    _ => Some(format!("MAP{:02}", n + 1)),
                };
            }
            return match n {
                31 | 32 => Some("MAP16".into()),
                1..=29 => Some(format!("MAP{:02}", n + 1)),
                _ => None,
            };
        }
    }
    // ExMy
    let b = name.as_bytes();
    if b.len() >= 4 && b[0] == b'E' && b[2] == b'M' {
        let ep = b[1].wrapping_sub(b'0');
        let map = b[3].wrapping_sub(b'0');
        if (1..=4).contains(&ep) && (1..=9).contains(&map) {
            if secret_exit && map == 3 {
                return Some(format!("E{ep}M9"));
            }
            if map == 9 {
                return Some(format!("E{ep}M4"));
            }
            if map < 8 {
                return Some(format!("E{ep}M{}", map + 1));
            }
            if ep < 4 {
                return Some(format!("E{}M1", ep + 1));
            }
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn title_to_level_via_pending_map() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_pending_map(map);
        assert_eq!(game.state, GameState::Title);
        game.begin_pending_map().unwrap();
        assert_eq!(game.state, GameState::Level);
        assert!(game.world().is_some());
    }

    #[test]
    fn cheat_god_toggles() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.cheat_god(0));
        assert!(game.world().unwrap().players[0].as_ref().unwrap().god);
        assert!(!game.cheat_god(0));
    }

    #[test]
    fn cheat_summon_spawns_class() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes.clone(), states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let before = game.world().unwrap().thinkers.default.len();
        assert!(game.cheat_summon(0, "ZombieMan"));
        assert_eq!(game.world().unwrap().thinkers.default.len(), before + 1);
        let zombie = classes.find_name("ZombieMan").unwrap();
        assert!(game.world().unwrap().thinkers.default.iter().any(|&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == zombie)
        }));
    }

    #[test]
    fn cheat_pause_toggles() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.cheat_pause());
        assert!(game.world().unwrap().paused);
        assert!(!game.cheat_pause());
    }

    #[test]
    fn cheat_iddt_toggles_allmap() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.cheat_iddt(0));
        assert!(game.world().unwrap().players[0].as_ref().unwrap().allmap);
        assert!(!game.cheat_iddt(0));
        assert!(!game.world().unwrap().players[0].as_ref().unwrap().allmap);
    }

    #[test]
    fn cheat_idmypos_pushes_coords() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.cheat_idmypos(0));
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(
            msgs.iter().any(|m| m.contains("x,y=")),
            "expected idmypos message, got {msgs:?}"
        );
    }

    #[test]
    fn cheat_freeze_toggles_level_frozen() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.cheat_freeze());
        assert!(game.world().unwrap().level.frozen);
        let mt = game.world().unwrap().level.maptime;
        game.tick(&[TicCmd::default()]);
        assert_eq!(game.world().unwrap().level.maptime, mt);
        assert!(!game.cheat_freeze());
        assert!(!game.world().unwrap().level.frozen);
    }

    #[test]
    fn cheat_kill_self_marks_player_dead() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, PlayerState, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  Death:
    PLAY H 10
    PLAY N -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.cheat_kill_self(0));
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .unwrap()
                .player_state,
            PlayerState::Dead
        );
    }

    #[test]
    fn hud_state_tracks_god_face() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use room_ui::{DoomStatusBar, StatusBar, UiDrawList};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let hud = game.hud_state(0, false).unwrap();
        assert_eq!(hud.health, 100);
        assert_eq!(hud.face, "STFST00");
        assert!(game.cheat_god(0));
        let hud = game.hud_state(0, false).unwrap();
        assert_eq!(hud.face, "STFGOD0");
        let mut list = UiDrawList::new();
        DoomStatusBar.draw(&hud, &mut list);
        assert!(list.cmds.len() >= 4);
    }

    #[test]
    fn headless_ticks_and_checksums() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            skill: Skill::new(3),
            rng_seed: 1993,
            record_demo: true,
            ..Default::default()
        });
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let c0 = game.checksum().unwrap();
        game.run_tics(10);
        let c1 = game.checksum().unwrap();
        assert_ne!(c0, c1);
        assert_eq!(game.world().unwrap().level.time, 10);
        let demo = game.take_demo().unwrap();
        assert_eq!(demo.tics.len(), 10);
    }

    #[test]
    fn exit_transitions_to_intermission_with_tallies() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            skill: Skill::new(3),
            rng_seed: 1,
            record_demo: false,
            ..Default::default()
        });
        game.start_custom_map(map, Skill::new(3)).unwrap();
        {
            let w = game.world_mut().unwrap();
            w.level.exiting = true;
        }
        game.tick(&[]);
        assert_eq!(game.state, GameState::Intermission);
        let tallies = game.intermission.clone().unwrap();
        assert_eq!(tallies.map_name, "MAP01");
        assert_eq!(tallies.kills, 100); // no monsters → 100%
        assert_eq!(tallies.time_tics, 1);
        assert_eq!(tallies.par_tics, 30 * 35); // Doom II MAP01 par
        assert_eq!(tallies.next_map.as_deref(), Some("MAP02"));

        let mut map2 = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP02")
            .unwrap();
        apply_nodes(&mut map2);
        game.continue_to_map(map2).unwrap();
        assert_eq!(game.state, GameState::Level);
        assert_eq!(game.world().unwrap().map.name, "MAP02");
    }

    #[test]
    fn default_par_seconds_doom2() {
        assert_eq!(default_par_seconds("MAP01"), Some(30));
        assert_eq!(default_par_seconds("map30"), Some(180));
        assert_eq!(default_par_seconds("E1M1"), None);
    }

    #[test]
    fn hub_snapshot_on_exit_and_restore_on_return() {
        use room_defs::{ClusterDef, MapDef, MapInfo};

        let mut map1 = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map1);
        let mut map2 = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP02")
            .unwrap();
        apply_nodes(&mut map2);

        let mut info = MapInfo::default();
        info.maps.insert(
            "MAP01".into(),
            MapDef {
                name: "MAP01".into(),
                next: Some("MAP02".into()),
                cluster: Some(1),
                ..MapDef::default()
            },
        );
        info.maps.insert(
            "MAP02".into(),
            MapDef {
                name: "MAP02".into(),
                next: Some("MAP01".into()),
                cluster: Some(1),
                ..MapDef::default()
            },
        );
        info.clusters.insert(
            1,
            ClusterDef {
                id: 1,
                hub: true,
                ..ClusterDef::default()
            },
        );

        let mut game = Headless::new(GameOptions::default());
        game.set_mapinfo(Arc::new(info));
        game.start_custom_map(map1, Skill::new(3)).unwrap();
        {
            let w = game.world_mut().unwrap();
            w.acs.map_vars[0] = 77;
            w.acs.world_vars[0] = 42;
            if let Some(mo) = w.players[0].as_ref().and_then(|p| p.mo) {
                w.actors.get_mut(mo).unwrap().health = 55;
            }
            w.level.exiting = true;
        }
        game.tick(&[]);
        assert_eq!(game.state, GameState::Intermission);
        assert!(game.has_hub_snapshot("MAP01"));

        game.continue_to_map(map2).unwrap();
        assert_eq!(game.map_name(), "MAP02");
        assert_eq!(game.world().unwrap().acs.world_vars[0], 42);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .and_then(|p| p.mo)
                .and_then(|mo| game.world().unwrap().actors.get(mo))
                .map(|a| a.health),
            Some(55)
        );

        // Return to MAP01 — hub snapshot restores map_vars.
        let mut map1b = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map1b);
        {
            let w = game.world_mut().unwrap();
            w.level.exiting = true;
        }
        game.tick(&[]);
        game.continue_to_map(map1b).unwrap();
        assert!(!game.has_hub_snapshot("MAP01"));
        assert_eq!(game.world().unwrap().acs.map_vars[0], 77);
        assert_eq!(game.world().unwrap().acs.world_vars[0], 42);
    }

    #[test]
    fn mapinfo_falling_damage_enables_level_flag() {
        use room_defs::{MapDef, MapInfo};

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);

        let mut info = MapInfo::default();
        info.maps.insert(
            "MAP01".into(),
            MapDef {
                name: "MAP01".into(),
                falling_damage: Some(true),
                ..MapDef::default()
            },
        );

        let mut game = Headless::new(GameOptions::default());
        game.set_mapinfo(Arc::new(info));
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.world().unwrap().level.falling_damage);
    }

    #[test]
    fn mapinfo_clipmidtextures_stamps_lines() {
        use room_defs::{MapDef, MapInfo};

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        assert!(map.lines.iter().all(|l| !l.clip_midtex));

        let mut info = MapInfo::default();
        info.maps.insert(
            "MAP01".into(),
            MapDef {
                name: "MAP01".into(),
                clip_midtextures: Some(true),
                wrap_midtextures: Some(true),
                ..MapDef::default()
            },
        );

        let mut game = Headless::new(GameOptions::default());
        game.set_mapinfo(Arc::new(info));
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let w = game.world().unwrap();
        assert!(w.map.lines.iter().all(|l| l.clip_midtex && l.wrap_midtex));
        assert!(w.map.sides.iter().all(|s| s.clip_midtex && s.wrap_midtex));
    }

    #[test]
    fn mapinfo_overrides_next_and_par_on_exit() {
        use room_core::LineId;
        use room_defs::{MapDef, MapInfo};
        use room_sim::ActivationType;

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        // Exit_Normal on a one-sided wall for activate_line.
        if let Some(line) = map.lines.first_mut() {
            line.special = 243;
            line.args = [0; 5];
        }
        apply_nodes(&mut map);

        let mut info = MapInfo::default();
        info.maps.insert(
            "MAP01".into(),
            MapDef {
                name: "MAP01".into(),
                next: Some("MAP99".into()),
                secret_next: Some("MAP31".into()),
                par_seconds: Some(42),
                ..MapDef::default()
            },
        );

        let mut game = Headless::new(GameOptions::default());
        game.set_mapinfo(Arc::new(info));
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .unwrap()
            .mo
            .unwrap();
        assert!(room_sim::activate_line(
            game.world_mut().unwrap(),
            LineId::from_raw(0),
            Some(mo),
            ActivationType::Use,
        ));
        game.tick(&[]);
        assert_eq!(game.state, GameState::Intermission);
        let t = game.intermission.as_ref().unwrap();
        assert_eq!(t.next_map.as_deref(), Some("MAP99"));
        assert_eq!(t.par_tics, 42 * 35);
    }

    #[test]
    fn default_next_map_doom2_and_doom1() {
        assert_eq!(
            default_next_map_name("map01", false).as_deref(),
            Some("MAP02")
        );
        assert_eq!(
            default_next_map_name("MAP15", true).as_deref(),
            Some("MAP31")
        );
        assert_eq!(
            default_next_map_name("MAP31", true).as_deref(),
            Some("MAP32")
        );
        assert_eq!(
            default_next_map_name("MAP31", false).as_deref(),
            Some("MAP16")
        );
        assert_eq!(
            default_next_map_name("MAP32", false).as_deref(),
            Some("MAP16")
        );
        assert_eq!(
            default_next_map_name("E1M1", false).as_deref(),
            Some("E1M2")
        );
        assert_eq!(default_next_map_name("E1M3", true).as_deref(), Some("E1M9"));
        assert_eq!(
            default_next_map_name("E1M9", false).as_deref(),
            Some("E1M4")
        );
        assert_eq!(default_next_map_name("MAP30", false), None);
    }

    #[test]
    fn map30_exit_intermission_then_finale() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP30")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            skill: Skill::new(3),
            rng_seed: 1,
            record_demo: false,
            ..Default::default()
        });
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().level.exiting = true;
        game.tick(&[]);
        assert_eq!(game.state, GameState::Intermission);
        assert_eq!(game.intermission.as_ref().unwrap().next_map, None);
        game.enter_finale();
        assert_eq!(game.state, GameState::Finale);
        game.leave_finale();
        assert_eq!(game.state, GameState::Title);
        assert!(game.intermission.is_none());
    }

    #[test]
    fn compat_db_merges_per_map_flags() {
        use room_sim::COMPATF_TRACE;

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        // MD5 of `hello` (matches unit fixture / wadsrc sample).
        map.md5 = [
            0x5d, 0x41, 0x40, 0x2a, 0xbc, 0x4b, 0x2a, 0x76, 0xb9, 0x71, 0x9d, 0x91, 0x10, 0x17,
            0xc5, 0x92,
        ];
        let db = room_map::CompatDatabase::parse(
            r#"
5D41402ABC4B2A76B9719D911017C592
{
	trace
	useblocking
}
"#,
        )
        .unwrap();
        let mut game = Headless::new(GameOptions::default());
        game.set_compat_db(Arc::new(db));
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.world().unwrap().compat.has(COMPATF_TRACE));
    }

    #[test]
    fn level_fixes_apply_on_map_start() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.md5 = [0xab; 16];
        map.lines[0].special = 99;
        let hex = room_map::format_md5(&map.md5);
        let ron = format!(
            r#"{{
                "{hex}": [
                    ClearLineSpecial(line: 0),
                    OffsetSectorPlane(sector: 0, floor: true, delta: 8.0),
                ],
            }}"#
        );
        let db = room_map::LevelFixesDatabase::parse(&ron).unwrap();
        let mut game = Headless::new(GameOptions::default());
        game.set_level_fixes(Arc::new(db));
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let w = game.world().unwrap();
        assert_eq!(w.map.lines[0].special, 0);
        assert_eq!(w.map.sectors[0].floor_height, 8.0);
    }

    #[test]
    fn complevel_applies_compatflags_on_map_start() {
        use room_sim::{COMPATF_BOOMSCROLL, COMPATF2_NOMBF21, SimMathMode};

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_complevel(Complevel::Boom);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let c = game.world().unwrap().compat;
        assert!(c.has(COMPATF_BOOMSCROLL));
        assert!(c.has2(COMPATF2_NOMBF21));
        assert!(!c.mbf21_enabled());
        assert_eq!(game.world().unwrap().math_mode, SimMathMode::Double);

        let mut map2 = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map2);
        game.set_complevel(Complevel::Vanilla);
        game.start_custom_map(map2, Skill::new(3)).unwrap();
        assert_eq!(game.world().unwrap().math_mode, SimMathMode::VanillaFixed);
    }

    #[test]
    fn save_and_load_restores_player_health() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        {
            let w = game.world_mut().unwrap();
            let mo = w.players[0].as_ref().unwrap().mo.unwrap();
            if let Some(a) = w.actors.get_mut(mo) {
                a.health = 42;
            }
            w.level.time = 99;
        }
        let blob = game.save_game().unwrap();
        // Mutate, then load.
        {
            let w = game.world_mut().unwrap();
            let mo = w.players[0].as_ref().unwrap().mo.unwrap();
            if let Some(a) = w.actors.get_mut(mo) {
                a.health = 10;
            }
            w.level.time = 0;
        }
        game.load_game(&blob).unwrap();
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        assert_eq!(w.actors.get(mo).unwrap().health, 42);
        assert_eq!(w.level.time, 99);
    }
}

//! Headless simulation driver (`room_game::Headless`).

use crate::demo::{DemoBundle, DemoRecorder};
use crate::save::{self, SaveError};
use crate::sound::SoundSystem;
use room_audio::SoundEvent;
use room_defs::Complevel;
use room_map::MapGeometry;
use room_sim::{
    BossAction, Buttons, ClassRegistry, CompatMode, MAX_PLAYERS, PlayerState, SimError, StateTable,
    TicCmd, World, cheat_give, math_mode_for_vanilla_demo,
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
    /// Coop player count 1..=[`MAX_PLAYERS`] (BE12: DM starts + P1 offset fallback).
    pub coop_players: u8,
    /// When true, seats from [`Self::bot_first_seat`]..`coop_players` are sim bots (BE8).
    pub fill_bots: bool,
    /// First seat index filled by bots when [`Self::fill_bots`] (`0` = all seats can be bots).
    pub bot_first_seat: u8,
    /// Match time limit in tics across the maps of one game (`0` = none);
    /// [`World::match_over`] is set when it runs out.
    pub time_limit_tics: u32,
    /// Deathmatch / DukeMatch: PvP, DM starts, no monsters on Duke.
    pub deathmatch: bool,
    /// Session display names per seat (BE3); empty → sim fallback `Player N`.
    pub player_names: [String; MAX_PLAYERS],
}

impl GameOptions {
    /// Apply roster / lobby names (seat index → UTF-8 display name).
    pub fn set_session_player_names(
        &mut self,
        names: impl IntoIterator<Item = (u8, impl AsRef<str>)>,
    ) {
        for (seat, name) in names {
            let i = usize::from(seat);
            if i < MAX_PLAYERS {
                self.player_names[i] = name.as_ref().to_owned();
            }
        }
    }
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
    /// [`Self::set_defs`] tables before mod classes were merged in.
    base_classes: Arc<ClassRegistry>,
    base_states: Arc<StateTable>,
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
    /// Play-scope WASM mod modules (rebuilt per new game).
    mod_sources: Vec<room_sim::script_mods::ModSource>,
    /// Match time left across maps of this game (tics; `None` = no limit).
    time_left: Option<u32>,
    /// Live mod runtime for the current game session.
    mods: Option<room_sim::script_mods::ModRuntime>,
    /// Mod load / runtime errors not yet drained by the shell.
    mod_errors: Vec<String>,
    /// Duke `CONEVT` lump (`ENTERLEVEL` / `EGS` / `GETSHOT` / `RESETWEAPONS` / …).
    conevt: Option<String>,
    /// Player-0 health after the last CON hurt check.
    conevt_last_hp: Option<i32>,
    /// `KILLIT` already ran for this map.
    conevt_killit: bool,
    /// Use button was held last tic (`EVENT_USE` edge).
    conevt_use_down: bool,
    /// Attack button was held last tic (`EVENT_FIRE` edge).
    conevt_attack_down: bool,
    /// Weapon was lowering last tic (`EVENT_HOLSTER` edge).
    conevt_holster_down: bool,
    /// Player-0 was dead last tic (`EVENT_RESPAWN`).
    conevt_was_dead: bool,
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
    for (name, list) in &reg.choices {
        table.choices.insert(
            name.clone(),
            list.iter()
                .map(|(n, s)| ((*n).clamp(0, i64::from(i32::MAX)) as i32, s.clone()))
                .collect(),
        );
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
            base_classes: Arc::new(ClassRegistry::new()),
            base_states: Arc::new(StateTable::new()),
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
            mod_sources: Vec::new(),
            time_left: None,
            mods: None,
            mod_errors: Vec::new(),
            conevt: None,
            conevt_last_hp: None,
            conevt_killit: false,
            conevt_use_down: false,
            conevt_attack_down: false,
            conevt_holster_down: false,
            conevt_was_dead: false,
        }
    }

    /// Install class / state tables (from DECORATE).
    /// Installed mod classes are merged in again.
    pub fn set_defs(&mut self, classes: Arc<ClassRegistry>, states: Arc<StateTable>) {
        self.base_classes = Arc::clone(&classes);
        self.base_states = Arc::clone(&states);
        self.classes = classes;
        self.states = states;
        if !self.mod_sources.is_empty() {
            let sources = std::mem::take(&mut self.mod_sources);
            if let Err(e) = self.set_script_mods(sources) {
                self.mod_errors.push(e.to_string());
            }
        }
    }

    /// Session class registry (DECORATE plus merged mod classes).
    #[must_use]
    pub fn classes(&self) -> &Arc<ClassRegistry> {
        &self.classes
    }

    /// Install WASM mod modules (after [`Self::set_defs`]): registers them once,
    /// merges their classes into the session tables, and runs them every tic
    /// of every following game. Returns the loaded module names.
    ///
    /// # Errors
    ///
    /// Compile / link / registration failure, or a class that cannot be merged.
    /// On error no mod is installed.
    pub fn set_script_mods(
        &mut self,
        sources: Vec<room_sim::script_mods::ModSource>,
    ) -> Result<Vec<String>, GameError> {
        self.mods = None;
        self.mod_sources.clear();
        self.classes = Arc::clone(&self.base_classes);
        self.states = Arc::clone(&self.base_states);
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        let rt = room_sim::script_mods::ModRuntime::new(&sources, Arc::clone(&self.classes))
            .map_err(|e| GameError::Message(format!("mod: {e}")))?;
        let (classes, states) = rt
            .merge_classes(&self.classes, &self.states)
            .map_err(|e| GameError::Message(format!("mod: {e}")))?;
        self.classes = Arc::new(classes);
        self.states = Arc::new(states);
        self.mod_errors.extend(rt.drain_log());
        let names = rt.module_names().iter().map(|s| (*s).to_owned()).collect();
        self.mod_sources = sources;
        Ok(names)
    }

    /// Swap the mod set while a level is running (live editing): the old
    /// modules get `WorldUnloaded`, the new ones `room_init` + `WorldLoaded` /
    /// `PlayerEntered` on the current map. Class property changes apply to new
    /// spawns; added / removed mod classes need a level restart. Outside a
    /// level this is [`Self::set_script_mods`].
    ///
    /// # Errors
    ///
    /// Compile / registration / merge failure; the running mods are kept.
    pub fn hot_swap_script_mods(
        &mut self,
        sources: Vec<room_sim::script_mods::ModSource>,
    ) -> Result<Vec<String>, GameError> {
        if self.state != GameState::Level || self.world.is_none() {
            return self.set_script_mods(sources);
        }
        let err = |e: room_script::ScriptError| GameError::Message(format!("mod: {e}"));
        let (mut classes, mut states) = (
            Arc::clone(&self.base_classes),
            Arc::clone(&self.base_states),
        );
        let mut next = None;
        if !sources.is_empty() {
            let rt = room_sim::script_mods::ModRuntime::new(&sources, Arc::clone(&classes))
                .map_err(err)?;
            let (c, s) = rt.merge_classes(&classes, &states).map_err(err)?;
            classes = Arc::new(c);
            states = Arc::new(s);
            next = Some(rt);
        }
        if let Some(old) = self.mods.as_mut() {
            old.end_map(&mut self.world);
            self.mod_errors.extend(old.drain_log());
            // Hot-swap: drop every retained widget from the outgoing modules.
            if let Some(w) = self.world.as_mut() {
                w.mod_hud.clear_all();
                w.mod_zones.clear_all();
            }
        }
        let same_layout = classes.len() == self.classes.len() && states.len() == self.states.len();
        if let Some(w) = self.world.as_mut() {
            if same_layout {
                w.classes = Arc::clone(&classes);
                w.states = Arc::clone(&states);
            } else {
                self.mod_errors.push(
                    "mod classes were added or removed - restart the level to use them".into(),
                );
            }
        }
        self.classes = classes;
        self.states = states;
        self.mod_sources = sources;
        self.mods = next;
        if let Some(rt) = self.mods.as_mut() {
            self.mod_errors.extend(rt.drain_log());
            rt.begin_map(&mut self.world);
        }
        Ok(self.script_mod_names())
    }

    /// Names of the installed mod modules.
    #[must_use]
    pub fn script_mod_names(&self) -> Vec<String> {
        self.mod_sources.iter().map(|s| s.name.clone()).collect()
    }

    /// Identity of the installed mod set (0 when none) for net / demo checks.
    #[must_use]
    pub fn script_mods_fingerprint(&self) -> u64 {
        self.mods
            .as_ref()
            .map_or(0, room_sim::script_mods::ModRuntime::fingerprint)
    }

    /// Take mod log lines and errors since the last call.
    pub fn drain_script_log(&mut self) -> Vec<String> {
        let mut out = std::mem::take(&mut self.mod_errors);
        if let Some(rt) = self.mods.as_ref() {
            out.extend(rt.drain_log());
        }
        out
    }

    /// Fresh mod session for a new game (`room_init(NewGame)` runs again).
    fn restart_mods(&mut self) {
        self.mods = None;
        if self.mod_sources.is_empty() {
            return;
        }
        match room_sim::script_mods::ModRuntime::new(&self.mod_sources, Arc::clone(&self.classes)) {
            Ok(rt) => self.mods = Some(rt),
            Err(e) => self.mod_errors.push(format!("mod: {e}")),
        }
    }

    /// Install MAPINFO (next / secretnext / par overrides).
    pub fn set_mapinfo(&mut self, mapinfo: Arc<room_defs::MapInfo>) {
        self.mapinfo = Some(mapinfo);
    }

    /// Install a Duke `CONEVT` lump (`onevent` quote / sound on enter / exit).
    pub fn set_conevt(&mut self, text: impl Into<String>) {
        let text = text.into();
        if !text.is_empty() {
            self.conevt = Some(text);
        }
    }

    /// Installed MAPINFO, if any.
    #[must_use]
    pub fn mapinfo(&self) -> Option<&room_defs::MapInfo> {
        self.mapinfo.as_deref()
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

    /// Remove slotted weapons whose `Ready` sprite has no graphics in `vfs`
    /// (the super shotgun on Doom 1 / shareware IWADs), so slot keys and
    /// `give weapons` never pick an invisible gun. Call after
    /// [`Self::set_defs`] and [`Self::set_keyconf`].
    pub fn drop_weapons_without_sprites(&mut self, vfs: &room_vfs::Vfs) {
        let sprites: std::collections::BTreeSet<String> = vfs
            .iter()
            .filter(|(_, e)| e.namespace == room_vfs::Namespace::Sprites)
            .filter_map(|(_, e)| e.short_name.as_str().get(..4).map(str::to_ascii_uppercase))
            .collect();
        let (classes, states) = (&self.classes, &self.states);
        self.weapon_slots.retain_weapons(|name| {
            let Some(cid) = classes.find_name(name) else {
                return true;
            };
            let Some(st) = states
                .find_label(cid, "Ready")
                .and_then(|sid| states.get(sid))
            else {
                return true;
            };
            let sprite = st.sprite.to_ascii_uppercase();
            sprite.len() != 4
                || matches!(sprite.as_str(), "TNT1" | "----" | "####")
                || sprites.contains(&sprite)
        });
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
            .with_ammo_max(hud.ammo_max)
            .with_inventory(hud.arms, hud.ammo_table)
            .with_palette(hud.palette)
            .with_colormap(hud.colormap)
            .with_netgame(hud.deathmatch, hud.frags, hud.face_back),
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
        if self.state == GameState::Intermission {
            if let Some(rt) = self.mods.as_mut() {
                rt.end_map(&mut self.world);
            }
        } else {
            self.restart_mods();
            self.time_left =
                (self.options.time_limit_tics > 0).then_some(self.options.time_limit_tics);
        }
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
        let mut world = World::from_map_with_slots(
            map,
            Arc::clone(&self.classes),
            Arc::clone(&self.states),
            skill.0,
            math_mode,
            self.weapon_slots.clone(),
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
        match self.time_left {
            Some(0) => world.end_match(None, true),
            Some(t) => world.match_tics_left = t,
            None => {}
        }
        let coop = self.options.coop_players.clamp(1, MAX_PLAYERS as u8);
        for (i, name) in self.options.player_names.iter().enumerate().take(MAX_PLAYERS) {
            if !name.is_empty() {
                world.set_player_display_name(i, name);
            }
        }
        if coop > 1 {
            let _ = world.spawn_coop_players(coop)?;
        }
        if self.options.fill_bots && coop > 0 {
            let first = self.options.bot_first_seat.min(coop.saturating_sub(1));
            world.fill_bot_seats(first, coop - 1);
        }
        world.deathmatch = self.options.deathmatch;
        if world.deathmatch {
            world.apply_duke_match();
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
        if let Some(evt) = self.conevt.clone() {
            apply_conevt(&mut world, &evt, "ENTERLEVEL");
            if !hub_returning {
                apply_conevt(&mut world, &evt, "RESETWEAPONS");
                apply_conevt(&mut world, &evt, "RESETINVENTORY");
            }
        }
        self.conevt_last_hp = player0_health(&world);
        self.conevt_killit = false;
        self.conevt_use_down = false;
        self.conevt_attack_down = false;
        self.conevt_holster_down = false;
        self.conevt_was_dead = false;
        self.world = Some(world);
        if let Some(rt) = self.mods.as_mut() {
            rt.begin_map(&mut self.world);
        }
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

    /// Session display name for `seat` (BE3); updates options and live world when loaded.
    pub fn set_player_display_name(&mut self, seat: usize, name: &str) {
        if seat >= MAX_PLAYERS {
            return;
        }
        self.options.player_names[seat] = name.to_owned();
        if let Some(w) = self.world.as_mut() {
            w.set_player_display_name(seat, name);
        }
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
        if let Some(evt) = self.conevt.clone() {
            let hp = player0_health(world);
            if let (Some(prev), Some(now)) = (self.conevt_last_hp, hp) {
                if now < prev {
                    apply_conevt(world, &evt, "GETSHOT");
                    apply_conevt(world, &evt, "INCURDAMAGE");
                }
                if now <= 0 && !self.conevt_killit {
                    self.conevt_killit = true;
                    apply_conevt(world, &evt, "KILLIT");
                }
            }
            self.conevt_last_hp = hp;
            let (use_now, atk_now) = world.players[0]
                .as_ref()
                .map(|p| {
                    (
                        p.cmd.buttons.any(Buttons::USE),
                        p.cmd.buttons.any(Buttons::ATTACK),
                    )
                })
                .unwrap_or((false, false));
            if use_now && !self.conevt_use_down {
                apply_conevt(world, &evt, "USE");
            }
            if atk_now && !self.conevt_attack_down {
                apply_conevt(world, &evt, "FIRE");
            }
            self.conevt_use_down = use_now;
            self.conevt_attack_down = atk_now;
            let weap = world.players[0].as_ref().is_some_and(|p| {
                p.cmd.weapon_select != 0
                    || p.cmd.buttons.any(Buttons::WEAPONNEXT | Buttons::WEAPONPREV)
            });
            if weap {
                apply_conevt(world, &evt, "CHANGEWEAPON");
                apply_conevt(world, &evt, "HOLSTER");
            }
            let holster = world.players[0]
                .as_ref()
                .is_some_and(|p| p.pending_weapon.is_some());
            if holster && !self.conevt_holster_down {
                apply_conevt(world, &evt, "HOLSTER");
            }
            self.conevt_holster_down = holster;
            let live = world.players[0]
                .as_ref()
                .is_some_and(|p| p.player_state == PlayerState::Live);
            if self.conevt_was_dead && live {
                apply_conevt(world, &evt, "RESPAWN");
            }
            self.conevt_was_dead = !live;
            apply_useractor(world, &evt);
        }
        if self.time_left.is_some() {
            self.time_left = Some(world.match_tics_left);
        }
        if let Some(rt) = self.mods.as_mut() {
            rt.tick(&mut self.world);
        }
        let Some(world) = self.world.as_mut() else {
            return;
        };
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
            if let Some(evt) = self.conevt.clone() {
                apply_conevt(world, &evt, "EGS");
                apply_conevt(world, &evt, "ENDLEVEL");
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

    /// Start recording tics from the current level (`record` console).
    pub fn begin_demo_record(&mut self) {
        self.options.record_demo = true;
        if self.state != GameState::Level {
            return;
        }
        let mut rec = DemoRecorder::start_with_complevel(
            self.map_name.clone(),
            self.options.rng_seed,
            self.options.complevel,
        );
        rec.skill = self.options.skill.0;
        self.demo = Some(rec);
    }

    /// Whether a demo is being recorded.
    #[must_use]
    pub fn recording_demo(&self) -> bool {
        self.demo.is_some()
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

fn is_sky_pic(pic: &str) -> bool {
    ["F_SKY1", "F_SKY", "F_SKY001"]
        .iter()
        .any(|s| pic.eq_ignore_ascii_case(s))
}

fn player_sector_index(world: &World) -> Option<usize> {
    let mo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let a = world.actors.get(mo)?;
    room_sim::sector_at_xy(world, a.pos.x(), a.pos.y())
}

fn player_under_sky(world: &World) -> bool {
    let Some(si) = player_sector_index(world) else {
        return false;
    };
    let pic = world
        .sector_ceiling_pics
        .get(&(si as u32))
        .map(String::as_str)
        .or_else(|| world.map.sectors.get(si).map(|s| s.ceiling_pic.as_str()))
        .unwrap_or("");
    is_sky_pic(pic)
}

/// Duke `ifinspace`: sky floor and sky ceiling.
fn player_in_space(world: &World) -> bool {
    let Some(si) = player_sector_index(world) else {
        return false;
    };
    let ceil = world
        .sector_ceiling_pics
        .get(&(si as u32))
        .map(String::as_str)
        .or_else(|| world.map.sectors.get(si).map(|s| s.ceiling_pic.as_str()))
        .unwrap_or("");
    let floor = world
        .sector_floor_pics
        .get(&(si as u32))
        .map(String::as_str)
        .or_else(|| world.map.sectors.get(si).map(|s| s.floor_pic.as_str()))
        .unwrap_or("");
    is_sky_pic(ceil) && is_sky_pic(floor)
}

/// Duke `ifinouterspace`: sky floor (`G_CheckForSpaceFloor`).
fn player_in_outerspace(world: &World) -> bool {
    let Some(si) = player_sector_index(world) else {
        return false;
    };
    let floor = world
        .sector_floor_pics
        .get(&(si as u32))
        .map(String::as_str)
        .or_else(|| world.map.sectors.get(si).map(|s| s.floor_pic.as_str()))
        .unwrap_or("");
    is_sky_pic(floor)
}

/// Feet-to-floor in Build z (Room × 256).
fn player_floor_build_dist(world: &World) -> Option<f64> {
    let mo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let a = world.actors.get(mo)?;
    let floor = room_sim::floor_at_xy(world, a.pos.x(), a.pos.y())?;
    Some((a.pos.z() - floor).max(0.0) * 256.0)
}

/// Head-to-ceiling in Build z (Room × 256).
fn player_ceil_build_dist(world: &World) -> Option<f64> {
    let mo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let a = world.actors.get(mo)?;
    let ceil = room_sim::ceiling_at_xy(world, a.pos.x(), a.pos.y())?;
    Some((ceil - (a.pos.z() + a.height)).max(0.0) * 256.0)
}

/// Floor-to-ceiling gap in Build z (Room × 256). Duke `ifgapzl`.
fn player_gap_build(world: &World) -> Option<f64> {
    let mo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let a = world.actors.get(mo)?;
    let floor = room_sim::floor_at_xy(world, a.pos.x(), a.pos.y())?;
    let ceil = room_sim::ceiling_at_xy(world, a.pos.x(), a.pos.y())?;
    Some((ceil - floor).max(0.0) * 256.0)
}

/// Duke `ifawayfromwall`: more than 8 Room units (128 Build) from a blocker.
const AWAY_FROM_WALL: f64 = 8.0;

fn point_seg_dist2(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 <= 1e-12 {
        let ex = px - ax;
        let ey = py - ay;
        return ex * ex + ey * ey;
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    let ex = px - (ax + t * dx);
    let ey = py - (ay + t * dy);
    ex * ex + ey * ey
}

fn player_away_from_wall(world: &World) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    let (px, py) = (a.pos.x(), a.pos.y());
    let min2 = AWAY_FROM_WALL * AWAY_FROM_WALL;
    for line in &world.map.lines {
        let blocking = line.back_sector.is_none() || (i32::from(line.flags) & 0x0001) != 0;
        if !blocking {
            continue;
        }
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        if point_seg_dist2(px, py, v1.pos.x(), v1.pos.y(), v2.pos.x(), v2.pos.y()) < min2 {
            return false;
        }
    }
    true
}

fn player0_health(world: &World) -> Option<i32> {
    world.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .and_then(|mo| world.actors.get(mo))
        .map(|a| a.health)
}

/// Duke `ifsquished`: last crush, or floor/ceiling gap tighter than 32
/// (Duke `(fz-cz) < (32<<8)` after `/256`) or the pawn's own height.
fn player_pal_swap(world: &World) -> bool {
    world
        .players
        .first()
        .and_then(|p| p.as_ref())
        .is_some_and(|p| p.damage_count > 0 || p.bonus_count > 0)
}

fn player_something_on(world: &World) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    let ax = a.pos.x();
    let ay = a.pos.y();
    let az = a.pos.z();
    let ar = a.radius;
    let ah = a.height;
    world.thinkers.default.iter().any(|&id| {
        if id == mo {
            return false;
        }
        let Some(o) = world.actors.get(id) else {
            return false;
        };
        if (o.flags & room_sim::FLAG_SOLID) == 0 || o.health <= 0 {
            return false;
        }
        let dx = o.pos.x() - ax;
        let dy = o.pos.y() - ay;
        let r = ar + o.radius;
        if dx * dx + dy * dy >= r * r {
            return false;
        }
        let oz = o.pos.z();
        oz < az + ah && oz + o.height > az
    })
}

fn player_squished(world: &World) -> bool {
    if world
        .last_damage
        .as_ref()
        .is_some_and(|d| d.cause.starts_with("crush"))
    {
        return true;
    }
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    let Some(floor) = room_sim::floor_at_xy(world, a.pos.x(), a.pos.y()) else {
        return false;
    };
    let Some(ceil) = room_sim::ceiling_at_xy(world, a.pos.x(), a.pos.y()) else {
        return false;
    };
    let gap = ceil - floor;
    gap < 32.0 || (a.height > 0.0 && a.height > gap)
}

/// CON `ifpinventory NAME` → Duke class (FIRSTAID → DukeFirstAid) or the token.
fn find_con_inventory(world: &World, name: &str) -> Option<room_core::ClassId> {
    let mapped = duke_inventory_class(name);
    if !mapped.is_empty() {
        if let Some(id) = world.classes.find_name(mapped) {
            return Some(id);
        }
    }
    world
        .classes
        .find_name(name)
        .or_else(|| world.classes.find_name(&format!("Duke{name}")))
        .or_else(|| {
            let u = name.trim().to_ascii_uppercase();
            world
                .classes
                .iter()
                .find(|c| {
                    let n = c.name.to_ascii_uppercase();
                    n == u || n.contains(&u)
                })
                .map(|c| c.id)
        })
}

fn duke_inventory_class(name: &str) -> &'static str {
    match name.to_ascii_uppercase().as_str() {
        "FIRSTAID" => "DukeFirstAid",
        "STEROIDS" => "DukeSteroids",
        "AIRTANK" => "DukeScubaGear",
        "BOOTS" => "DukeBoots",
        "HEATSENSOR" => "DukeNightVision",
        "HOLODUKE" => "DukeHoloduke",
        "JETPACK" => "DukeJetpack",
        "SHIELD" => "DukeArmor",
        _ => "",
    }
}

fn duke_weapon_aliases(name: &str) -> Vec<String> {
    let u = name.trim().to_ascii_uppercase();
    let mut out = vec![u.clone(), format!("DUKE{u}")];
    match u.as_str() {
        "RPG" | "RPGSPRITE" | "RPG_WEAPON" => {
            out.extend(["ROCKET".into(), "DUKEROCKET".into(), "DUKERPG".into()]);
        }
        "KNEE" | "KICK" => out.extend(["KNEE".into(), "KICK".into(), "DUKEKICK".into()]),
        "SHOTSPARK1" | "FIRSTGUN" | "PISTOL" | "PISTOL_WEAPON" => {
            out.extend(["PISTOL".into(), "SHOTSPARK".into(), "DUKEPISTOL".into()]);
        }
        "SHOTGUN" | "SHOTGUN_WEAPON" => out.push("DUKESHOTGUN".into()),
        "CHAINGUN" | "CHAINGUNSPRITE" => {
            out.extend(["RIPPER".into(), "DUKERIPPER".into(), "CHAINGUN".into()]);
        }
        "FREEZEBLAST" | "FREEZE" => {
            out.extend(["FREEZE".into(), "ICE".into(), "DUKEFREEZE".into()]);
        }
        "SHRINKSPARK" | "SHRINKER" => out.extend(["SHRINK".into(), "DUKESHRINKER".into()]),
        "GROWSPARK" | "GROW" => out.extend(["GROW".into(), "EXPAND".into(), "DUKEEXPANDER".into()]),
        _ => {}
    }
    out
}

fn duke_ammo_class(name: &str) -> &'static str {
    match name.trim().to_ascii_uppercase().as_str() {
        "PISTOL" | "PISTOL_WEAPON" | "FIRSTGUN" | "CLIP" | "CHAINGUN" | "CHAINGUNSPRITE"
        | "RIPPER" => "Clip",
        "SHOTGUN" | "SHOTGUN_WEAPON" | "SHELL" => "Shell",
        "RPG" | "RPG_WEAPON" | "ROCKET" | "DEVASTATOR" => "RocketAmmo",
        "FREEZE" | "SHRINKER" | "GROW" | "CELL" | "TRIPBOMB" => "Cell",
        _ => "",
    }
}

fn find_con_weapon(world: &World, name: &str) -> Option<room_core::ClassId> {
    for alias in duke_weapon_aliases(name) {
        if let Some(id) = world.classes.find_name(&alias) {
            if world.classes.get(id).is_some_and(|c| c.is_weapon) {
                return Some(id);
            }
        }
    }
    world
        .classes
        .iter()
        .find(|c| c.is_weapon && class_matches_weapon(&c.name, name))
        .map(|c| c.id)
}

fn class_matches_weapon(class: &str, want: &str) -> bool {
    if class.is_empty() {
        return false;
    }
    let c = class.to_ascii_uppercase();
    duke_weapon_aliases(want)
        .iter()
        .any(|a| c == *a || c.contains(a.as_str()))
}

/// Duke `ifwasweapon NAME`: last player hit's inflictor / source class.
fn player_was_weapon(world: &World, name: &str) -> bool {
    let Some(d) = world.last_damage.as_ref() else {
        return false;
    };
    class_matches_weapon(&d.inflictor_class, name) || class_matches_weapon(&d.source_class, name)
}

fn useractor_was_weapon(world: &World, id: room_sim::ActorId, name: &str) -> bool {
    let Some(d) = world.last_damage.as_ref() else {
        return false;
    };
    if d.target != Some(id) {
        return false;
    }
    class_matches_weapon(&d.inflictor_class, name) || class_matches_weapon(&d.source_class, name)
}

fn useractor_hit_weapon(world: &World, id: room_sim::ActorId) -> bool {
    world
        .last_damage
        .as_ref()
        .is_some_and(|d| d.target == Some(id))
}

fn useractor_not_moving(world: &World, id: room_sim::ActorId) -> bool {
    world
        .actors
        .get(id)
        .is_some_and(|a| a.vel.x().abs() < 1e-6 && a.vel.y().abs() < 1e-6 && a.vel.z().abs() < 1e-6)
}

fn useractor_squished(world: &World, id: room_sim::ActorId) -> bool {
    if world
        .last_damage
        .as_ref()
        .is_some_and(|d| d.target == Some(id) && d.cause.starts_with("crush"))
    {
        return true;
    }
    let Some(a) = world.actors.get(id) else {
        return false;
    };
    let Some(floor) = room_sim::floor_at_xy(world, a.pos.x(), a.pos.y()) else {
        return false;
    };
    let Some(ceil) = room_sim::ceiling_at_xy(world, a.pos.x(), a.pos.y()) else {
        return false;
    };
    let gap = ceil - floor;
    gap < 32.0 || (a.height > 0.0 && a.height > gap)
}

fn sound_name_matches(got: &str, want: &str) -> bool {
    let got = got.to_ascii_lowercase();
    let want = want.trim().to_ascii_lowercase();
    let want_stem = want.rsplit('/').next().unwrap_or(want.as_str());
    let stem = got.rsplit('/').next().unwrap_or(got.as_str());
    got == want || stem == want || stem == want_stem || got == format!("duke/{want_stem}")
}

fn sound_is_queued(world: &World, name: &str) -> bool {
    world
        .sound_events
        .iter()
        .any(|e| sound_name_matches(&e.name, name))
        || world
            .sound_heard
            .iter()
            .any(|n| sound_name_matches(n, name))
}

fn conevt_stopsound(world: &mut World, name: &str) {
    world
        .sound_events
        .retain(|e| !sound_name_matches(&e.name, name));
    world.sound_heard.retain(|n| !sound_name_matches(n, name));
}

fn player_ifp(world: &World, flag: &str) -> bool {
    let hp = world.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .and_then(|mo| world.actors.get(mo))
        .map(|a| a.health)
        .unwrap_or(0);
    let on_ground = world.players[0].as_ref().is_some_and(|p| p.on_ground);
    let vel_z = world.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .and_then(|mo| world.actors.get(mo))
        .map(|a| a.vel.z())
        .unwrap_or(0.0);
    let moving = world.players[0]
        .as_ref()
        .is_some_and(|p| p.cmd.forward_move != 0 || p.cmd.side_move != 0);
    match flag {
        "palive" => hp > 0,
        "pdead" => hp <= 0,
        "ponground" => on_ground,
        "pfalling" => !on_ground && vel_z < 0.0,
        "pjumping" => !on_ground && vel_z > 0.0,
        "pstanding" => on_ground && !moving,
        "pwalking" | "prunning" => moving,
        "pducking" => world.players[0].as_ref().is_some_and(|p| p.crouching),
        "pjetpack" => world.players[0].as_ref().is_some_and(|p| p.fly_tics > 0),
        "ponsteroids" => world.players[0]
            .as_ref()
            .is_some_and(|p| p.steroid_tics > 0),
        "pshrunk" => world.players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .and_then(|mo| world.actors.get(mo))
            .and_then(|a| a.user_vars.get("user_dukeshink").copied())
            .is_some_and(|t| t > 0),
        "pkicking" => world.players[0]
            .as_ref()
            .is_some_and(|p| p.quick_kick_restore.is_some()),
        "phigher" => player_is_higher(world),
        "pwalkingback" | "prunningback" => world.players[0]
            .as_ref()
            .is_some_and(|p| p.cmd.forward_move < 0),
        "paiming" | "plooking" => world.players[0].as_ref().is_some_and(|p| p.cmd.pitch != 0),
        "pweapon" => player_has_weapon(world),
        "pweaponout" => world.players[0]
            .as_ref()
            .is_some_and(|p| p.ready_weapon.is_some()),
        _ => false,
    }
}

fn player_has_weapon(world: &World) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    world
        .inventory
        .iter(mo)
        .any(|item| item.amount > 0 && world.classes.get(item.class).is_some_and(|c| c.is_weapon))
}

fn player_has_inventory(world: &World, name: &str) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let mut names = vec![name.to_owned()];
    let mapped = duke_inventory_class(name);
    if !mapped.is_empty() {
        names.push(mapped.to_owned());
    }
    if !name.eq_ignore_ascii_case("duke") && !name.to_ascii_lowercase().starts_with("duke") {
        names.push(format!("Duke{name}"));
    }
    for n in names {
        if let Some(class) = world.classes.find_name(&n) {
            if room_sim::count_inventory(world, mo, class) > 0 {
                return true;
            }
        }
    }
    false
}

/// Duke `ifcansee` in a player event: line of sight to the last attacker
/// or a living `COUNTKILL`.
fn player_can_see(world: &World) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    if let Some(src) = world.last_damage.as_ref().and_then(|d| d.source) {
        if src != mo && room_sim::check_sight(world, mo, src) {
            return true;
        }
    }
    for &id in &world.thinkers.default {
        if id == mo {
            continue;
        }
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || a.health <= 0 {
            continue;
        }
        if world.players.iter().flatten().any(|p| p.mo == Some(id)) {
            continue;
        }
        let flags = a.flags | world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_COUNTKILL) == 0 {
            continue;
        }
        if room_sim::check_sight(world, mo, id) {
            return true;
        }
    }
    false
}

/// Duke `ifcanseetarget`: line of sight to the pawn's current target
/// (else the last attacker).
fn player_can_see_target(world: &World) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let tid = world
        .actors
        .get(mo)
        .and_then(|a| a.target)
        .or_else(|| world.last_damage.as_ref().and_then(|d| d.source));
    let Some(tid) = tid else {
        return false;
    };
    if tid == mo {
        return false;
    }
    let Some(t) = world.actors.get(tid) else {
        return false;
    };
    if t.destroyed || t.health <= 0 {
        return false;
    }
    room_sim::check_sight(world, mo, tid)
}

/// Duke `ifcanshoottarget`: a living `COUNTKILL` in sight and within 90° (512).
const CAN_SHOOT_ANG: f64 = 512.0;

fn player_can_shoot_target(world: &World) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    for &id in &world.thinkers.default {
        if id == mo {
            continue;
        }
        let Some(b) = world.actors.get(id) else {
            continue;
        };
        if b.destroyed || b.health <= 0 {
            continue;
        }
        if world.players.iter().flatten().any(|p| p.mo == Some(id)) {
            continue;
        }
        let flags = b.flags | world.classes.get(b.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_COUNTKILL) == 0 {
            continue;
        }
        if !room_sim::check_sight(world, mo, id) {
            continue;
        }
        let dx = b.pos.x() - a.pos.x();
        let dy = b.pos.y() - a.pos.y();
        if dx * dx + dy * dy <= 1e-12 {
            return true;
        }
        let to = room_core::dmath::atan2(dy, dx) * (180.0 / core::f64::consts::PI);
        let fa = a.angle.degrees().rem_euclid(360.0) * (2048.0 / 360.0);
        let ta = to.rem_euclid(360.0) * (2048.0 / 360.0);
        let d = (fa - ta + 1024.0).rem_euclid(2048.0) - 1024.0;
        if d.abs() < CAN_SHOOT_ANG {
            return true;
        }
    }
    false
}

/// Duke `ifspritepal`: empty translation is pal 0; otherwise trailing digits.
fn player_sprite_pal(world: &World) -> i32 {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return 0;
    };
    let Some(a) = world.actors.get(mo) else {
        return 0;
    };
    parse_sprite_pal(&a.translation)
}

fn parse_sprite_pal(tr: &str) -> i32 {
    let t = tr.trim();
    if t.is_empty() {
        return 0;
    }
    if let Ok(n) = t.parse::<i32>() {
        return n;
    }
    let digits: String = t
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse().unwrap_or(0)
}

/// Duke `ifactor NAME`: pawn class matches that CON actor (`APLAYER` → player).
fn player_is_actor(world: &World, name: &str) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    let Some(class) = world.classes.get(a.class) else {
        return false;
    };
    class_matches_actor(&class.name, name)
}

fn find_con_actor(world: &World, name: &str) -> Option<room_core::ClassId> {
    let mapped = duke_spawn_class(name);
    if !mapped.is_empty() {
        if let Some(id) = world.classes.find_name(mapped) {
            return Some(id);
        }
    }
    world
        .classes
        .find_name(name)
        .or_else(|| world.classes.find_name(&format!("Duke{name}")))
        .or_else(|| {
            let u = name.trim().to_ascii_uppercase();
            world
                .classes
                .iter()
                .find(|c| class_matches_actor(&c.name, &u))
                .map(|c| c.id)
        })
}

fn duke_spawn_class(name: &str) -> &'static str {
    match name.trim().to_ascii_uppercase().as_str() {
        "LIZTROOP" | "LIZTROOPRUNNING" | "LIZTROOPSTAYPUT" | "LIZTROOPSHOOT"
        | "LIZTROOPJETPACK" | "LIZTROOPONTOILET" | "LIZTROOPJUSTSIT" | "LIZTROOPDUCKING" => {
            "DukeTrooper"
        }
        "PIGCOP" | "PIGCOPSTAYPUT" | "PIGCOPDIVE" => "DukePigCop",
        "LIZMAN" | "LIZMANSTAYPUT" | "LIZMANSPITTING" | "LIZMANFEEDING" | "LIZMANJUMP" => {
            "DukeEnforcer"
        }
        "OCTABRAIN" => "DukeOctabrain",
        "COMMANDER" => "DukeCommander",
        "DRONE" => "DukeDrone",
        "BLOOD" => "DukeBlood",
        "SHELL" => "DukeShell",
        "RPG" | "RPGSPRITE" => "DukeRocket",
        "FIRELASER" => "DukeLaser",
        "FREEZEBLAST" => "DukeFreezeBlast",
        "SHRINKSPARK" => "DukeShrinkSpark",
        "SPIT" => "DukeSpit",
        "MONEY" => "DukeMoney",
        "MAIL" => "DukeMail",
        "PAPER" => "DukePaper",
        "SCRAP" | "SCRAP6" | "JIBS1" | "JIBS2" | "JIBS3" | "JIBS4" | "JIBS5" | "JIBS6"
        | "HEADJIB" | "LEGJIB" | "ARMJIB" => "DukeScrap",
        "SMALLSMOKE" => "DukeSmallSmoke",
        "GLASSPIECES" => "DukeGlassPiece",
        _ => "",
    }
}

fn conevt_spawn(world: &mut World, name: &str, count: i32, throw: bool) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    conevt_spawn_from(world, mo, name, count, throw);
}

fn conevt_spawn_from(
    world: &mut World,
    mo: room_sim::ActorId,
    name: &str,
    count: i32,
    throw: bool,
) {
    let Some(cid) = find_con_actor(world, name) else {
        return;
    };
    let (pos, ang) = world
        .actors
        .get(mo)
        .map(|a| (a.pos, a.angle))
        .unwrap_or((room_core::DVec3::ZERO, room_core::Angle::ZERO));
    for _ in 0..count.max(0) {
        let vel = if throw {
            let rx = i32::from(world.p_random("pr_dukeguts")) - 128;
            let ry = i32::from(world.p_random("pr_dukeguts")) - 128;
            let rz = i32::from(world.p_random("pr_dukeguts") & 127);
            room_core::DVec3::new(
                f64::from(rx) / 32.0,
                f64::from(ry) / 32.0,
                f64::from(rz) / 16.0,
            )
        } else {
            room_core::DVec3::ZERO
        };
        if let Ok(id) = world.spawn_at(cid, pos, ang) {
            if let Some(a) = world.actors.get_mut(id) {
                a.master = Some(mo);
                if throw {
                    a.vel = vel;
                }
            }
        }
    }
}

fn conevt_lotsofcolour(world: &mut World, count: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    conevt_lotsofcolour_from(world, mo, count);
}

fn conevt_lotsofcolour_from(world: &mut World, mo: room_sim::ActorId, count: i32) {
    let Some(cid) = find_con_actor(world, "GLASSPIECES") else {
        return;
    };
    let (pos, ang) = world
        .actors
        .get(mo)
        .map(|a| (a.pos, a.angle))
        .unwrap_or((room_core::DVec3::ZERO, room_core::Angle::ZERO));
    for i in 0..count.max(0).min(16) {
        let rx = i32::from(world.p_random("pr_dukeguts")) - 128;
        let ry = i32::from(world.p_random("pr_dukeguts")) - 128;
        let rz = i32::from(world.p_random("pr_dukeguts") & 127);
        let vel = room_core::DVec3::new(
            f64::from(rx) / 32.0,
            f64::from(ry) / 32.0,
            f64::from(rz) / 16.0,
        );
        if let Ok(id) = world.spawn_at(cid, pos, ang) {
            if let Some(a) = world.actors.get_mut(id) {
                a.master = Some(mo);
                a.vel = vel;
                apply_duke_spritepal(a, (i % 6) + 1);
            }
        }
    }
}

/// Duke `hitradius R D`: blast of `D` within Build radius `R` (`/16` map units).
fn conevt_hitradius(world: &mut World, build_r: i32, dmg: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    conevt_hitradius_from(world, mo, build_r, dmg);
}

fn conevt_hitradius_from(world: &mut World, origin_id: room_sim::ActorId, build_r: i32, dmg: i32) {
    if dmg <= 0 || build_r <= 0 {
        return;
    }
    let Some(origin) = world.actors.get(origin_id).map(|a| a.pos) else {
        return;
    };
    let lim2 = {
        let r = f64::from(build_r) / 16.0;
        r * r
    };
    let ids: Vec<_> = world.thinkers.default.clone();
    for id in ids {
        if id == origin_id {
            continue;
        }
        let Some(other) = world.actors.get(id) else {
            continue;
        };
        if other.destroyed {
            continue;
        }
        let dx = other.pos.x() - origin.x();
        let dy = other.pos.y() - origin.y();
        if dx * dx + dy * dy >= lim2 {
            continue;
        }
        room_sim::damage_mobj(world, id, Some(origin_id), Some(origin_id), dmg);
    }
}

/// Duke `fall`: snap the pawn to the floor under its XY and kill vertical speed.
fn conevt_fall(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    conevt_fall_from(world, mo);
    if let Some(p) = world.players[0].as_mut() {
        p.on_ground = true;
    }
}

fn conevt_fall_from(world: &mut World, id: room_sim::ActorId) {
    let Some((x, y)) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let Some(floor) = room_sim::floor_at_xy(world, x, y) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(id) {
        a.pos = room_core::DVec3::new(x, y, floor);
        a.vel = room_core::DVec3::new(a.vel.x(), a.vel.y(), 0.0);
    }
}

/// Duke `resetplayer`: restore health, spawn pose, and clear velocity.
fn conevt_resetplayer(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some((class, spawn, ang)) = world
        .actors
        .get(mo)
        .map(|a| (a.class, a.spawn_point, a.spawn_angle))
    else {
        return;
    };
    let hp = world.classes.get(class).map(|c| c.health).unwrap_or(100);
    if let Some(a) = world.actors.get_mut(mo) {
        a.health = hp.max(1);
        a.pos = spawn;
        a.angle = ang;
        a.vel = room_core::DVec3::ZERO;
        a.destroyed = false;
    }
    if let Some(p) = world.players[0].as_mut() {
        p.on_ground = true;
    }
}

fn conevt_operateactivators(world: &mut World, tag: i32, activator: Option<room_sim::ActorId>) {
    if tag == 0 {
        return;
    }
    let ids: Vec<room_core::LineId> = world
        .map
        .lines
        .iter()
        .enumerate()
        .filter(|(_, ld)| ld.special != 0 && (i32::from(ld.tag) == tag || ld.args[0] == tag))
        .map(|(i, _)| room_core::LineId::from_raw(i as u32))
        .collect();
    for lid in ids {
        let _ = room_sim::activate_line(world, lid, activator, room_sim::ActivationType::Use);
    }
}

fn conevt_operatesectors(world: &mut World, id: room_sim::ActorId) {
    let Some((x, y)) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let Some(sid) = room_sim::sector_at_xy(world, x, y) else {
        return;
    };
    let tags: Vec<i32> = world
        .map
        .sectors
        .get(sid)
        .map(|s| s.tags.iter().copied().collect())
        .unwrap_or_default();
    for tag in tags {
        conevt_operateactivators(world, tag, Some(id));
    }
}

fn conevt_resetinventory(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    const ITEMS: &[&str] = &[
        "DukeFirstAid",
        "FirstAid",
        "DukeSteroids",
        "Steroids",
        "DukeScubaGear",
        "DukeBoots",
        "DukeNightVision",
        "DukeHoloduke",
        "DukeJetpack",
        "DukeArmor",
        "Armor",
        "DukeBombsOut",
    ];
    for name in ITEMS {
        if let Some(cid) = world.classes.find_name(name) {
            let _ = room_sim::take_inventory(world, mo, cid, i32::MAX);
        }
    }
    if let Some(p) = world.players[0].as_mut() {
        p.inven_icon = 0;
        p.invdisp_tics = 0;
    }
}

fn conevt_resetweapons(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let keep: Vec<_> = ["DukeKick", "Fist", "DukePistol", "Pistol"]
        .iter()
        .filter_map(|n| world.classes.find_name(n))
        .collect();
    let strip: Vec<_> = world
        .inventory
        .iter(mo)
        .filter(|i| {
            world
                .classes
                .get(i.class)
                .is_some_and(|c| c.is_weapon && !keep.contains(&i.class))
        })
        .map(|i| i.class)
        .collect();
    for cid in strip {
        let _ = room_sim::take_inventory(world, mo, cid, i32::MAX);
    }
    let pistol = keep.iter().copied().find(|&c| {
        world
            .classes
            .get(c)
            .is_some_and(|d| d.name == "DukePistol" || d.name == "Pistol")
    });
    if let Some(p) = world.players[0].as_mut() {
        p.ready_weapon = pistol.or(keep.first().copied());
        p.pending_weapon = None;
    }
}

fn conevt_tossweapon(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let wid = world.players[0]
        .as_ref()
        .and_then(|p| p.ready_weapon.or(p.pending_weapon));
    if let Some(wid) = wid {
        room_sim::take_inventory(world, mo, wid, 1);
    }
    if let Some(p) = world.players[0].as_mut() {
        p.ready_weapon = None;
        p.pending_weapon = None;
    }
}

fn conevt_addweapon(world: &mut World, name: &str, n: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(wid) = find_con_weapon(world, name) {
        room_sim::give_inventory(world, mo, wid, 1);
        if let Some(p) = world.players[0].as_mut() {
            p.pending_weapon = Some(wid);
        }
    }
    if n <= 0 {
        return;
    }
    let mapped = duke_ammo_class(name);
    let cid = if !mapped.is_empty() {
        world.classes.find_name(mapped)
    } else {
        None
    }
    .or_else(|| world.classes.find_name(name))
    .or_else(|| {
        find_con_weapon(world, name).and_then(|wid| {
            world
                .classes
                .get(wid)
                .and_then(|c| {
                    (!c.ammo_type.is_empty()).then(|| world.classes.find_name(&c.ammo_type))
                })
                .flatten()
        })
    });
    if let Some(cid) = cid {
        room_sim::give_inventory(world, mo, cid, n);
    }
}

fn conevt_addinventory(world: &mut World, name: &str, n: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(cid) = find_con_inventory(world, name) {
        room_sim::give_inventory(world, mo, cid, n.max(0));
    }
}

fn conevt_addammo(world: &mut World, name: &str, n: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let mapped = duke_ammo_class(name);
    let cid = if !mapped.is_empty() {
        world.classes.find_name(mapped)
    } else {
        None
    }
    .or_else(|| world.classes.find_name(name))
    .or_else(|| {
        world.classes.find_name(name).and_then(|wid| {
            world
                .classes
                .get(wid)
                .and_then(|c| {
                    (!c.ammo_type.is_empty()).then(|| world.classes.find_name(&c.ammo_type))
                })
                .flatten()
        })
    });
    if let Some(cid) = cid {
        room_sim::give_inventory(world, mo, cid, n.max(0));
    }
}

fn conevt_movesprite(world: &mut World, x: i32, y: i32, z: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(mo) {
        let dx = f64::from(x) / 16.0;
        let dy = -f64::from(y) / 16.0;
        let dz = -f64::from(z) / 256.0;
        a.pos = room_core::DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z() + dz);
    }
}

fn conevt_movesprite_from(world: &mut World, id: room_sim::ActorId, x: i32, y: i32, z: i32) {
    if let Some(a) = world.actors.get_mut(id) {
        let dx = f64::from(x) / 16.0;
        let dy = -f64::from(y) / 16.0;
        let dz = -f64::from(z) / 256.0;
        a.pos = room_core::DVec3::new(a.pos.x() + dx, a.pos.y() + dy, a.pos.z() + dz);
    }
}

const USER_CLIPMOVE: &str = "user_clipmove";

fn conevt_clipmove(world: &mut World, x: i32, y: i32, z: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some(a) = world.actors.get(mo) else {
        return;
    };
    let dest = room_core::DVec3::new(
        a.pos.x() + f64::from(x) / 16.0,
        a.pos.y() + -f64::from(y) / 16.0,
        a.pos.z() + -f64::from(z) / 256.0,
    );
    let ok = room_sim::try_move(world, mo, dest);
    if let Some(a) = world.actors.get_mut(mo) {
        a.user_vars.insert(USER_CLIPMOVE.to_owned(), i32::from(ok));
    }
}

fn conevt_clipmove_from(world: &mut World, id: room_sim::ActorId, x: i32, y: i32, z: i32) {
    let Some(a) = world.actors.get(id) else {
        return;
    };
    let dest = room_core::DVec3::new(
        a.pos.x() + f64::from(x) / 16.0,
        a.pos.y() + -f64::from(y) / 16.0,
        a.pos.z() + -f64::from(z) / 256.0,
    );
    let ok = room_sim::try_move(world, id, dest);
    if let Some(a) = world.actors.get_mut(id) {
        a.user_vars.insert(USER_CLIPMOVE.to_owned(), i32::from(ok));
    }
}

fn useractor_yaw_deg_to_player(world: &World, id: room_sim::ActorId) -> Option<f64> {
    let pmo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let (px, py) = world.actors.get(pmo).map(|a| (a.pos.x(), a.pos.y()))?;
    let (ax, ay) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y()))?;
    Some(room_core::dmath::atan2(py - ay, px - ax) * (180.0 / core::f64::consts::PI))
}

fn useractor_yaw_deg_to_player_lead(world: &World, id: room_sim::ActorId) -> Option<f64> {
    let pmo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let p = world.actors.get(pmo)?;
    let a = world.actors.get(id)?;
    let px = p.pos.x() + p.vel.x();
    let py = p.pos.y() + p.vel.y();
    Some(room_core::dmath::atan2(py - a.pos.y(), px - a.pos.x()) * (180.0 / core::f64::consts::PI))
}

fn furthest_dir_deg(ax: f64, ay: f64, px: f64, py: f64) -> f64 {
    let mut best_deg = 180.0;
    let mut best_d = -1.0;
    for i in 0..8 {
        let deg = f64::from(i) * 45.0;
        let rad = deg * (core::f64::consts::PI / 180.0);
        let nx = ax + 16.0 * room_core::dmath::cos(rad);
        let ny = ay + 16.0 * room_core::dmath::sin(rad);
        let dx = nx - px;
        let dy = ny - py;
        let d = dx * dx + dy * dy;
        if d > best_d {
            best_d = d;
            best_deg = deg;
        }
    }
    best_deg
}

fn turn_toward_deg(cur: f64, want: f64, max_deg: f64) -> f64 {
    let mut d = (want - cur) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    cur + d.clamp(-max_deg, max_deg)
}

fn useractor_yaw_deg_away_from_missile(world: &World, id: room_sim::ActorId) -> Option<f64> {
    let a = world.actors.get(id)?;
    let min2 = BULLET_NEAR * BULLET_NEAR;
    let mut best: Option<(f64, f64)> = None;
    for &oid in &world.thinkers.default {
        if oid == id {
            continue;
        }
        let Some(b) = world.actors.get(oid) else {
            continue;
        };
        if b.destroyed {
            continue;
        }
        let flags = b.flags | world.classes.get(b.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_MISSILE) == 0 {
            continue;
        }
        let dx = b.pos.x() - a.pos.x();
        let dy = b.pos.y() - a.pos.y();
        let d2 = dx * dx + dy * dy;
        if d2 >= min2 {
            continue;
        }
        let away = room_core::dmath::atan2(-dy, -dx) * (180.0 / core::f64::consts::PI);
        if best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, away));
        }
    }
    best.map(|(_, yaw)| yaw)
}

const MOVE_FACEPLAYER: i32 = 1;
const MOVE_FLEEENEMY: i32 = 2;
const MOVE_SEEKPLAYER: i32 = 4;
const MOVE_FACEPLAYERSLOW: i32 = 8;
const MOVE_SPIN: i32 = 16;
const MOVE_DODGEBULLET: i32 = 32;
const MOVE_JUMPTOPLAYER: i32 = 64;
const MOVE_GETH: i32 = 128;
const MOVE_GETV: i32 = 256;
const MOVE_RANDOMANGLE: i32 = 512;
const MOVE_FACEPLAYERSMART: i32 = 1024;
const MOVE_FURTHESTDIR: i32 = 2048;

/// Walk to `dest`, or slide along the blocking wall (left, then right).
fn try_move_or_slide(world: &mut World, id: room_sim::ActorId, dest: room_core::DVec3) -> bool {
    if room_sim::try_move(world, id, dest) {
        return true;
    }
    let Some(a) = world.actors.get(id) else {
        return false;
    };
    let ox = a.pos.x();
    let oy = a.pos.y();
    let oz = a.pos.z();
    let dx = dest.x() - ox;
    let dy = dest.y() - oy;
    let left = room_core::DVec3::new(ox - dy, oy + dx, oz);
    if room_sim::try_move(world, id, left) {
        return true;
    }
    let right = room_core::DVec3::new(ox + dy, oy - dx, oz);
    room_sim::try_move(world, id, right)
}

/// Face from CON move flags, walk `vel` along facing, then apply `angvel`.
fn conevt_movevel(world: &mut World, id: room_sim::ActorId, vel: i32, angvel: i32, flags: i32) {
    if vel != 0 || angvel != 0 || flags != 0 {
        if let Some(a) = world.actors.get_mut(id) {
            a.user_vars.insert("user_conchase".to_owned(), 1);
        }
    }
    if flags & MOVE_DODGEBULLET != 0 {
        if let Some(want) = useractor_yaw_deg_away_from_missile(world, id) {
            if let Some(a) = world.actors.get_mut(id) {
                a.angle = room_core::Angle::from_degrees(want);
            }
        }
    } else if flags & MOVE_FACEPLAYERSMART != 0 {
        if let Some(want) = useractor_yaw_deg_to_player_lead(world, id) {
            if let Some(a) = world.actors.get_mut(id) {
                a.angle = room_core::Angle::from_degrees(want);
            }
        }
    } else if flags & MOVE_FURTHESTDIR != 0 {
        if let (Some(a), Some(pmo)) = (
            world.actors.get(id),
            world.players[0].as_ref().and_then(|p| p.mo),
        ) {
            if let Some(p) = world.actors.get(pmo) {
                let deg = furthest_dir_deg(a.pos.x(), a.pos.y(), p.pos.x(), p.pos.y());
                if let Some(a) = world.actors.get_mut(id) {
                    a.angle = room_core::Angle::from_degrees(deg);
                }
            }
        }
    } else if flags & (MOVE_FACEPLAYER | MOVE_FLEEENEMY | MOVE_SEEKPLAYER | MOVE_FACEPLAYERSLOW)
        != 0
    {
        if let Some(mut want) = useractor_yaw_deg_to_player(world, id) {
            if flags & MOVE_FLEEENEMY != 0
                && flags & (MOVE_FACEPLAYER | MOVE_SEEKPLAYER | MOVE_FACEPLAYERSLOW) == 0
            {
                want += 180.0;
            }
            if let Some(a) = world.actors.get_mut(id) {
                let next = if flags & MOVE_FACEPLAYERSLOW != 0
                    && flags & (MOVE_FACEPLAYER | MOVE_SEEKPLAYER) == 0
                {
                    turn_toward_deg(a.angle.degrees(), want, 22.5)
                } else {
                    want
                };
                a.angle = room_core::Angle::from_degrees(next);
            }
        }
    } else if flags & MOVE_RANDOMANGLE != 0 {
        let roll = i32::from(world.p_random("pr_conmove"));
        if let Some(a) = world.actors.get_mut(id) {
            let deg = f64::from(1 + roll % 7) * 45.0;
            a.angle = room_core::Angle::from_degrees(deg);
        }
    }
    if vel != 0 {
        let Some(a) = world.actors.get(id) else {
            return;
        };
        let rad = a.angle.radians();
        let step = f64::from(vel) / 16.0;
        let dest = room_core::DVec3::new(
            a.pos.x() + step * room_core::dmath::cos(rad),
            a.pos.y() + step * room_core::dmath::sin(rad),
            a.pos.z(),
        );
        let _ = try_move_or_slide(world, id, dest);
    }
    if flags & MOVE_GETH != 0 {
        if let (Some(a), Some(pmo)) = (
            world.actors.get(id),
            world.players[0].as_ref().and_then(|p| p.mo),
        ) {
            if let Some(p) = world.actors.get(pmo) {
                let dx = p.pos.x() - a.pos.x();
                let dy = p.pos.y() - a.pos.y();
                let len = room_core::dmath::sqrt(dx * dx + dy * dy);
                if len > 1e-6 {
                    let dest = room_core::DVec3::new(
                        a.pos.x() + 8.0 * dx / len,
                        a.pos.y() + 8.0 * dy / len,
                        a.pos.z(),
                    );
                    let _ = try_move_or_slide(world, id, dest);
                }
            }
        }
    }
    if flags & MOVE_GETV != 0 {
        if let Some(pmo) = world.players[0].as_ref().and_then(|p| p.mo) {
            if let (Some(a), Some(p)) = (world.actors.get(id), world.actors.get(pmo)) {
                let dz = (p.pos.z() - a.pos.z()).clamp(-4.0, 4.0);
                let dest = room_core::DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + dz);
                if let Some(a) = world.actors.get_mut(id) {
                    a.pos = dest;
                }
            }
        }
    }
    if flags & MOVE_JUMPTOPLAYER != 0 {
        if let Some(a) = world.actors.get(id) {
            let on_floor = room_sim::floor_at_xy(world, a.pos.x(), a.pos.y())
                .is_some_and(|z| (a.pos.z() - z).abs() < 1.0);
            if on_floor {
                if let Some(a) = world.actors.get_mut(id) {
                    a.vel = room_core::DVec3::new(a.vel.x(), a.vel.y(), 8.0);
                }
            }
        }
    }
    if angvel != 0 {
        if let Some(a) = world.actors.get_mut(id) {
            let deg = f64::from(angvel) * 360.0 / 2048.0;
            a.angle = room_core::Angle::from_degrees(a.angle.degrees() + deg);
        }
    }
    if flags & MOVE_SPIN != 0 {
        if let Some(a) = world.actors.get_mut(id) {
            a.angle = room_core::Angle::from_degrees(a.angle.degrees() + 22.5);
        }
    }
}

const USER_SPRITESTAT: &str = "user_spritestat";

fn conevt_changespritestat(world: &mut World, n: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.user_vars.insert(USER_SPRITESTAT.to_owned(), n);
    }
}

fn conevt_changespritestat_from(world: &mut World, id: room_sim::ActorId, n: i32) {
    if let Some(a) = world.actors.get_mut(id) {
        a.user_vars.insert(USER_SPRITESTAT.to_owned(), n);
    }
}

const USER_WACKMOVE: &str = "user_wackmove";

fn conevt_wackmove(world: &mut World, x: i32, y: i32, z: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    conevt_wackmove_from(world, mo, x, y, z);
}

fn conevt_wackmove_from(world: &mut World, id: room_sim::ActorId, x: i32, y: i32, z: i32) {
    let Some(a) = world.actors.get(id) else {
        return;
    };
    let dest = room_core::DVec3::new(
        a.pos.x() + f64::from(x) / 16.0,
        a.pos.y() + -f64::from(y) / 16.0,
        a.pos.z() + -f64::from(z) / 256.0,
    );
    let rad = a.radius;
    let height = a.height;
    let ok = room_sim::try_move(world, id, dest);
    if !ok {
        let mut hits = Vec::new();
        for &oid in &world.thinkers.default {
            if oid == id {
                continue;
            }
            let Some(o) = world.actors.get(oid) else {
                continue;
            };
            if o.destroyed || o.health <= 0 {
                continue;
            }
            let flags = o.flags | world.classes.get(o.class).map(|c| c.flags).unwrap_or(0);
            if (flags & room_sim::FLAG_SOLID) == 0 {
                continue;
            }
            let dx = o.pos.x() - dest.x();
            let dy = o.pos.y() - dest.y();
            let r = o.radius + rad;
            if dx * dx + dy * dy >= r * r {
                continue;
            }
            if o.pos.z() >= dest.z() + height || dest.z() >= o.pos.z() + o.height {
                continue;
            }
            hits.push(oid);
        }
        for hid in hits {
            room_sim::damage_mobj(world, hid, Some(id), Some(id), 10);
        }
    }
    if let Some(a) = world.actors.get_mut(id) {
        a.user_vars.insert(USER_WACKMOVE.to_owned(), i32::from(ok));
    }
}

const USER_RESPAWNHITAG: &str = "user_respawnhitag";

fn conevt_respawnhitag(world: &mut World, n: i32) {
    let revived = duke_respawn_tid(world, n);
    if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
        if let Some(a) = world.actors.get_mut(mo) {
            a.user_vars.insert(USER_RESPAWNHITAG.to_owned(), revived);
        }
    }
}

fn conevt_respawnhitag_from(world: &mut World, id: room_sim::ActorId, n: i32) {
    let revived = duke_respawn_tid(world, n);
    if let Some(a) = world.actors.get_mut(id) {
        a.user_vars.insert(USER_RESPAWNHITAG.to_owned(), revived);
    }
}

fn duke_respawn_tid(world: &mut World, n: i32) -> i32 {
    let ids: Vec<_> = world.thinkers.default.clone();
    let mut revived = 0i32;
    for id in ids {
        let Some(a) = world.actors.get(id).cloned() else {
            continue;
        };
        if a.tid != n {
            continue;
        }
        if a.health > 0 && !a.destroyed {
            continue;
        }
        let spawn_hp = world
            .classes
            .get(a.class)
            .map(|c| c.health.max(1))
            .unwrap_or(1);
        let class_flags = world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        if let Some(m) = world.actors.get_mut(id) {
            m.pos = a.spawn_point;
            m.angle = a.spawn_angle;
            m.vel = room_core::DVec3::ZERO;
            m.health = spawn_hp;
            m.destroyed = false;
            m.flags = class_flags;
            m.target = None;
        }
        world.dequeue_corpse(id);
        if let Some(spawn) = world.states.find_label(a.class, "Spawn") {
            world.set_state(id, Some(spawn), false);
        }
        revived += 1;
    }
    revived
}

fn conevt_setsprite(world: &mut World, x: i32, y: i32, z: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.pos = room_core::DVec3::new(
            f64::from(x) / 16.0,
            -f64::from(y) / 16.0,
            -f64::from(z) / 256.0,
        );
    }
}

fn conevt_setsprite_from(world: &mut World, id: room_sim::ActorId, x: i32, y: i32, z: i32) {
    if let Some(a) = world.actors.get_mut(id) {
        a.pos = room_core::DVec3::new(
            f64::from(x) / 16.0,
            -f64::from(y) / 16.0,
            -f64::from(z) / 256.0,
        );
    }
}

const USER_LOCKPLAYER: &str = "user_lockplayer";

fn conevt_lockplayer(world: &mut World, n: i32) {
    if let Some(p) = world.players[0].as_mut() {
        p.frozen = n > 0;
    }
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.vel = room_core::DVec3::ZERO;
        if n > 0 {
            a.user_vars.insert(USER_LOCKPLAYER.to_owned(), n);
        } else {
            a.user_vars.remove(USER_LOCKPLAYER);
        }
    }
}

fn duke_yaw_deg(ang: i32) -> f64 {
    f64::from((-(ang & 2047) * 360 / 2048).rem_euclid(360))
}

fn conevt_setplayerangle(world: &mut World, ang: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.angle = room_core::Angle::from_degrees(duke_yaw_deg(ang));
    }
}

const USER_PLAYERANGLE: &str = "user_playerangle";

fn duke_ang_from_yaw(deg: f64) -> i32 {
    let deg = deg.round() as i32;
    (-deg * 2048 / 360).rem_euclid(2048)
}

fn conevt_getplayerangle(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(mo) {
        let ang = duke_ang_from_yaw(a.angle.degrees());
        a.user_vars.insert(USER_PLAYERANGLE.to_owned(), ang);
    }
}

const USER_ACTORANGLE: &str = "user_actorangle";

fn conevt_getactorangle(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    conevt_get_angle_on(world, mo, USER_ACTORANGLE);
}

fn conevt_set_angle_on(world: &mut World, id: room_sim::ActorId, ang: i32) {
    if let Some(a) = world.actors.get_mut(id) {
        a.angle = room_core::Angle::from_degrees(duke_yaw_deg(ang));
    }
}

fn conevt_get_angle_on(world: &mut World, id: room_sim::ActorId, key: &str) {
    if let Some(a) = world.actors.get_mut(id) {
        let ang = duke_ang_from_yaw(a.angle.degrees());
        a.user_vars.insert(key.to_owned(), ang);
    }
}

const USER_LASTPAL: &str = "user_lastpal";

fn duke_pal_index(translation: &str) -> i32 {
    translation
        .strip_prefix("DukePal")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn apply_duke_spritepal(a: &mut room_sim::Actor, n: i32) {
    let last = duke_pal_index(&a.translation);
    a.user_vars.insert(USER_LASTPAL.to_owned(), last);
    a.translation = if n == 0 {
        String::new()
    } else {
        format!("DukePal{n}")
    };
}

const USER_CONCOUNT: &str = "user_concount";

/// Duke `ifcount`: pawn `user_concount` when set, else map tics.
fn player_con_count(world: &World) -> u32 {
    if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
        if let Some(n) = world
            .actors
            .get(mo)
            .and_then(|a| a.user_vars.get(USER_CONCOUNT).copied())
        {
            return u32::try_from(n.max(0)).unwrap_or(0);
        }
    }
    world.level.time
}

fn set_player_con_count(world: &mut World, n: i32) {
    if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
        if let Some(a) = world.actors.get_mut(mo) {
            a.user_vars.insert(USER_CONCOUNT.to_owned(), n.max(0));
        }
    }
}

const USER_CONACTIONCOUNT: &str = "user_conactioncount";

/// Duke `ifactioncount`: pawn `user_conactioncount` when set, else map tics.
fn player_con_action_count(world: &World) -> u32 {
    if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
        if let Some(n) = world
            .actors
            .get(mo)
            .and_then(|a| a.user_vars.get(USER_CONACTIONCOUNT).copied())
        {
            return u32::try_from(n.max(0)).unwrap_or(0);
        }
    }
    world.level.time
}

fn set_player_con_action_count(world: &mut World, n: i32) {
    if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
        if let Some(a) = world.actors.get_mut(mo) {
            a.user_vars.insert(USER_CONACTIONCOUNT.to_owned(), n.max(0));
        }
    }
}

/// Duke `sizeto`: step Build repeat (`scale * 64`) one unit toward `target`.
fn step_duke_repeat(scale: f64, target: i32) -> f64 {
    let cur = (scale * 64.0).round() as i32;
    let next = if cur < target {
        cur + 1
    } else if cur > target {
        cur - 1
    } else {
        cur
    };
    f64::from(next) / 64.0
}

fn apply_duke_cstat(a: &mut room_sim::Actor, cstat: i32) {
    if cstat & 1 != 0 {
        a.flags |= room_sim::FLAG_SOLID;
    } else {
        a.flags &= !room_sim::FLAG_SOLID;
    }
    a.alpha = if cstat & 2 != 0 { 0.66 } else { 1.0 };
    if cstat & 32_768 != 0 {
        a.flags |= room_sim::FLAG_DONTDRAW;
    } else {
        a.flags &= !room_sim::FLAG_DONTDRAW;
    }
}

fn apply_duke_cstator(a: &mut room_sim::Actor, bits: i32) {
    if bits & 1 != 0 {
        a.flags |= room_sim::FLAG_SOLID;
    }
    if bits & 2 != 0 {
        a.alpha = 0.66;
    }
    if bits & 32_768 != 0 {
        a.flags |= room_sim::FLAG_DONTDRAW;
    }
}

fn conevt_shoot(world: &mut World, name: &str) {
    conevt_shoot_z(world, name, 0);
}

fn conevt_shoot_z(world: &mut World, name: &str, zvel: i32) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    conevt_shoot_from(world, mo, name, zvel);
}

fn conevt_shoot_from(world: &mut World, mo: room_sim::ActorId, name: &str, zvel: i32) {
    let Some(cid) = find_con_actor(world, name) else {
        return;
    };
    let Some(src) = world.actors.get(mo).cloned() else {
        return;
    };
    let speed = world
        .classes
        .get(cid)
        .map(|c| if c.speed > 0.0 { c.speed } else { 16.0 })
        .unwrap_or(16.0);
    let cp = src.pitch.cos();
    let vel = room_core::DVec3::new(
        src.angle.cos() * cp * speed,
        src.angle.sin() * cp * speed,
        src.pitch.sin() * speed - f64::from(zvel) / 256.0,
    );
    if let Ok(id) = world.spawn_at(cid, src.pos, src.angle) {
        if let Some(a) = world.actors.get_mut(id) {
            a.master = Some(mo);
            a.vel = vel;
        }
    }
}

fn conevt_pstomp(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some(pl) = world.actors.get(mo).cloned() else {
        return;
    };
    let mut hits = Vec::new();
    for &oid in &world.thinkers.default {
        if oid == mo {
            continue;
        }
        let Some(o) = world.actors.get(oid) else {
            continue;
        };
        if o.destroyed || o.health <= 0 {
            continue;
        }
        if world.players.iter().flatten().any(|p| p.mo == Some(oid)) {
            continue;
        }
        let flags = o.flags | world.classes.get(o.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_SHOOTABLE) == 0 {
            continue;
        }
        let dx = o.pos.x() - pl.pos.x();
        let dy = o.pos.y() - pl.pos.y();
        let rad = o.radius + pl.radius;
        if dx * dx + dy * dy >= rad * rad {
            continue;
        }
        if o.pos.z() >= pl.pos.z() + pl.height || pl.pos.z() >= o.pos.z() + o.height {
            continue;
        }
        hits.push(oid);
    }
    for id in hits {
        room_sim::damage_mobj(world, id, Some(mo), Some(mo), 50);
    }
}

fn conevt_pstomp_from(world: &mut World, id: room_sim::ActorId) {
    let Some(src) = world.actors.get(id).cloned() else {
        return;
    };
    let mut hits = Vec::new();
    for &oid in &world.thinkers.default {
        if oid == id {
            continue;
        }
        let Some(o) = world.actors.get(oid) else {
            continue;
        };
        if o.destroyed || o.health <= 0 {
            continue;
        }
        let flags = o.flags | world.classes.get(o.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_SHOOTABLE) == 0 {
            continue;
        }
        let dx = o.pos.x() - src.pos.x();
        let dy = o.pos.y() - src.pos.y();
        let rad = o.radius + src.radius;
        if dx * dx + dy * dy >= rad * rad {
            continue;
        }
        if o.pos.z() >= src.pos.z() + src.height || src.pos.z() >= o.pos.z() + o.height {
            continue;
        }
        hits.push(oid);
    }
    for hid in hits {
        room_sim::damage_mobj(world, hid, Some(id), Some(id), 50);
    }
}

const USER_FINDPLAYER: &str = "user_findplayer";

fn conevt_findplayer(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some(tid) = nearest_countkill_id(world) else {
        if let Some(a) = world.actors.get_mut(mo) {
            a.user_vars.insert(USER_FINDPLAYER.to_owned(), -1);
        }
        return;
    };
    let Some((px, py)) = world.actors.get(mo).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let Some((tx, ty)) = world.actors.get(tid).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let dx = (tx - px) * 16.0;
    let dy = (ty - py) * 16.0;
    let dist = (dx * dx + dy * dy).sqrt().round() as i32;
    if let Some(a) = world.actors.get_mut(mo) {
        a.target = Some(tid);
        a.user_vars.insert(USER_FINDPLAYER.to_owned(), dist);
    }
}

const USER_FINDOTHERPLAYER: &str = "user_findotherplayer";

fn conevt_findotherplayer(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some((px, py)) = world.actors.get(mo).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let mut best: Option<(f64, room_sim::ActorId)> = None;
    for p in world.players.iter().flatten() {
        let Some(oid) = p.mo else {
            continue;
        };
        if oid == mo {
            continue;
        }
        let Some(o) = world.actors.get(oid) else {
            continue;
        };
        if o.destroyed || o.health <= 0 {
            continue;
        }
        let dx = o.pos.x() - px;
        let dy = o.pos.y() - py;
        let r2 = dx * dx + dy * dy;
        best = Some(match best {
            Some((br, bid)) if br <= r2 => (br, bid),
            _ => (r2, oid),
        });
    }
    if let Some(a) = world.actors.get_mut(mo) {
        if let Some((r2, tid)) = best {
            let dist = (r2.sqrt() * 16.0).round() as i32;
            a.target = Some(tid);
            a.user_vars.insert(USER_FINDOTHERPLAYER.to_owned(), dist);
        } else {
            a.user_vars.insert(USER_FINDOTHERPLAYER.to_owned(), -1);
        }
    }
}

fn conevt_findotherplayer_from(world: &mut World, id: room_sim::ActorId) {
    let Some((px, py)) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let mut best: Option<(f64, room_sim::ActorId)> = None;
    for p in world.players.iter().flatten() {
        let Some(oid) = p.mo else {
            continue;
        };
        if oid == id {
            continue;
        }
        let Some(o) = world.actors.get(oid) else {
            continue;
        };
        if o.destroyed || o.health <= 0 {
            continue;
        }
        let dx = o.pos.x() - px;
        let dy = o.pos.y() - py;
        let r2 = dx * dx + dy * dy;
        best = Some(match best {
            Some((br, bid)) if br <= r2 => (br, bid),
            _ => (r2, oid),
        });
    }
    if let Some(a) = world.actors.get_mut(id) {
        if let Some((r2, tid)) = best {
            let dist = (room_core::dmath::sqrt(r2) * 16.0).round() as i32;
            a.target = Some(tid);
            a.user_vars.insert(USER_FINDOTHERPLAYER.to_owned(), dist);
        } else {
            a.user_vars.insert(USER_FINDOTHERPLAYER.to_owned(), -1);
        }
    }
}

const USER_FLOORZ: &str = "user_floorz";
const USER_CEILZ: &str = "user_ceilz";

fn room_to_build_z(z: f64) -> i32 {
    (-z * 256.0).round() as i32
}

fn conevt_getzrange(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some((x, y)) = world.actors.get(mo).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let floor = room_sim::floor_at_xy(world, x, y).unwrap_or(0.0);
    let ceil = room_sim::ceiling_at_xy(world, x, y).unwrap_or(floor);
    if let Some(a) = world.actors.get_mut(mo) {
        a.user_vars
            .insert(USER_FLOORZ.to_owned(), room_to_build_z(floor));
        a.user_vars
            .insert(USER_CEILZ.to_owned(), room_to_build_z(ceil));
    }
}

fn conevt_getzrange_from(world: &mut World, id: room_sim::ActorId) {
    let Some((x, y)) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let floor = room_sim::floor_at_xy(world, x, y).unwrap_or(0.0);
    let ceil = room_sim::ceiling_at_xy(world, x, y).unwrap_or(floor);
    if let Some(a) = world.actors.get_mut(id) {
        a.user_vars
            .insert(USER_FLOORZ.to_owned(), room_to_build_z(floor));
        a.user_vars
            .insert(USER_CEILZ.to_owned(), room_to_build_z(ceil));
    }
}

const USER_SECTOR: &str = "user_sector";

fn conevt_updatesector(world: &mut World) {
    let Some(si) = player_sector_index(world) else {
        return;
    };
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(mo) {
        a.user_vars
            .insert(USER_SECTOR.to_owned(), i32::try_from(si).unwrap_or(0));
    }
}

fn conevt_updatesector_from(world: &mut World, id: room_sim::ActorId) {
    let Some(si) = useractor_sector_index(world, id) else {
        return;
    };
    if let Some(a) = world.actors.get_mut(id) {
        a.user_vars
            .insert(USER_SECTOR.to_owned(), i32::try_from(si).unwrap_or(0));
    }
}

const USER_FLORZ: &str = "user_florz";

fn conevt_getflorzofslope(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some((x, y)) = world.actors.get(mo).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let floor = room_sim::floor_at_xy(world, x, y).unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(mo) {
        a.user_vars
            .insert(USER_FLORZ.to_owned(), room_to_build_z(floor));
    }
}

fn conevt_getflorzofslope_from(world: &mut World, id: room_sim::ActorId) {
    let Some((x, y)) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let floor = room_sim::floor_at_xy(world, x, y).unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(id) {
        a.user_vars
            .insert(USER_FLORZ.to_owned(), room_to_build_z(floor));
    }
}

const USER_CEILZOFSLOPE: &str = "user_ceilzofslope";

fn conevt_getceilzofslope(world: &mut World) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some((x, y)) = world.actors.get(mo).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let ceil = room_sim::ceiling_at_xy(world, x, y).unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(mo) {
        a.user_vars
            .insert(USER_CEILZOFSLOPE.to_owned(), room_to_build_z(ceil));
    }
}

fn conevt_getceilzofslope_from(world: &mut World, id: room_sim::ActorId) {
    let Some((x, y)) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y())) else {
        return;
    };
    let ceil = room_sim::ceiling_at_xy(world, x, y).unwrap_or(0.0);
    if let Some(a) = world.actors.get_mut(id) {
        a.user_vars
            .insert(USER_CEILZOFSLOPE.to_owned(), room_to_build_z(ceil));
    }
}

fn class_matches_actor(class: &str, want: &str) -> bool {
    let c = class.to_ascii_uppercase();
    let w = want.trim().to_ascii_uppercase();
    if c == w || c == format!("DUKE{w}") {
        return true;
    }
    match w.as_str() {
        // Player-event CONEVT: the pawn is the player.
        "APLAYER" | "PLAYER" | "DUKEPLAYER" => true,
        _ => c.contains(&w),
    }
}

/// Duke `ifspawnedby NAME`: master (else last attacker) class matches.
fn player_spawned_by(world: &World, name: &str) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let mid = world
        .actors
        .get(mo)
        .and_then(|a| a.master)
        .or_else(|| world.last_damage.as_ref().and_then(|d| d.source));
    let Some(mid) = mid else {
        return false;
    };
    if mid == mo {
        return false;
    }
    let Some(m) = world.actors.get(mid) else {
        return false;
    };
    let Some(class) = world.classes.get(m.class) else {
        return false;
    };
    let c = class.name.to_ascii_uppercase();
    let w = name.trim().to_ascii_uppercase();
    c == w || c == format!("DUKE{w}") || c.contains(&w)
}

/// Duke stay-put guards set `user_stayput`; the player is free by default.
fn player_is_stayput(world: &World) -> bool {
    world.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .and_then(|mo| world.actors.get(mo))
        .and_then(|a| a.user_vars.get("user_stayput").copied())
        .is_some_and(|t| t > 0)
}

/// Duke `ifaction NAME`: current state label matches that CON action.
fn player_has_action(world: &World, name: &str) -> bool {
    let want = name.trim().to_ascii_uppercase();
    if want.is_empty() {
        return false;
    }
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    let Some(sid) = a.state else {
        return false;
    };
    world
        .states
        .labels_of(a.class)
        .any(|(label, id)| id == sid && label.to_ascii_uppercase() == want)
}

/// Duke CON `state NAME`: jump the pawn to that DECORATE / CON label.
fn player_goto_state(world: &mut World, name: &str) {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return;
    };
    let Some(class) = world.actors.get(mo).map(|a| a.class) else {
        return;
    };
    let Some(sid) = world.states.find_label(class, name) else {
        return;
    };
    world.set_state(mo, Some(sid), false);
}

/// Duke `ifp phigher`: pawn Z is above the current target (else nearest `COUNTKILL`).
fn player_is_higher(world: &World) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    let tid = a.target.or_else(|| nearest_countkill_id(world));
    let Some(tid) = tid else {
        return false;
    };
    let Some(t) = world.actors.get(tid) else {
        return false;
    };
    a.pos.z() > t.pos.z() + 1.0
}

fn nearest_countkill_id(world: &World) -> Option<room_sim::ActorId> {
    let mo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let a = world.actors.get(mo)?;
    let mut best: Option<(f64, room_sim::ActorId)> = None;
    for &id in &world.thinkers.default {
        if id == mo {
            continue;
        }
        let Some(b) = world.actors.get(id) else {
            continue;
        };
        if b.destroyed || b.health <= 0 {
            continue;
        }
        if world.players.iter().flatten().any(|p| p.mo == Some(id)) {
            continue;
        }
        let flags = b.flags | world.classes.get(b.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_COUNTKILL) == 0 {
            continue;
        }
        let dx = b.pos.x() - a.pos.x();
        let dy = b.pos.y() - a.pos.y();
        let r2 = dx * dx + dy * dy;
        best = Some(match best {
            Some((br, bid)) if br <= r2 => (br, bid),
            _ => (r2, id),
        });
    }
    best.map(|(_, id)| id)
}

/// Nearest living `COUNTKILL` XY distance² in Room units.
fn nearest_countkill_room2(world: &World) -> Option<f64> {
    let mo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let a = world.actors.get(mo)?;
    let mut best: Option<f64> = None;
    for &id in &world.thinkers.default {
        if id == mo {
            continue;
        }
        let Some(b) = world.actors.get(id) else {
            continue;
        };
        if b.destroyed || b.health <= 0 {
            continue;
        }
        if world.players.iter().flatten().any(|p| p.mo == Some(id)) {
            continue;
        }
        let flags = b.flags | world.classes.get(b.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_COUNTKILL) == 0 {
            continue;
        }
        let dx = b.pos.x() - a.pos.x();
        let dy = b.pos.y() - a.pos.y();
        let r2 = dx * dx + dy * dy;
        best = Some(best.map_or(r2, |x| x.min(r2)));
    }
    best
}

/// Duke Build distance² = Room² × 256 (`x/16`).
fn build_dist2_from_room2(room2: f64) -> f64 {
    room2 * 256.0
}

/// Duke `ifangdiffl`: shortest facing delta to the nearest living `COUNTKILL`
/// in Build angle units (2048 per circle).
fn player_ang_diff_build(world: &World) -> Option<f64> {
    let mo = world.players[0].as_ref().and_then(|p| p.mo)?;
    let a = world.actors.get(mo)?;
    let mut best: Option<(f64, room_sim::ActorId)> = None;
    for &id in &world.thinkers.default {
        if id == mo {
            continue;
        }
        let Some(b) = world.actors.get(id) else {
            continue;
        };
        if b.destroyed || b.health <= 0 {
            continue;
        }
        if world.players.iter().flatten().any(|p| p.mo == Some(id)) {
            continue;
        }
        let flags = b.flags | world.classes.get(b.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_COUNTKILL) == 0 {
            continue;
        }
        let dx = b.pos.x() - a.pos.x();
        let dy = b.pos.y() - a.pos.y();
        let r2 = dx * dx + dy * dy;
        if best.is_none_or(|(best_r2, _)| r2 < best_r2) {
            best = Some((r2, id));
        }
    }
    let (_, tid) = best?;
    let b = world.actors.get(tid)?;
    let dx = b.pos.x() - a.pos.x();
    let dy = b.pos.y() - a.pos.y();
    if dx * dx + dy * dy <= 1e-12 {
        return Some(0.0);
    }
    let to = room_core::dmath::atan2(dy, dx) * (180.0 / core::f64::consts::PI);
    let fa = a.angle.degrees().rem_euclid(360.0) * (2048.0 / 360.0);
    let ta = to.rem_euclid(360.0) * (2048.0 / 360.0);
    let d = (fa - ta + 1024.0).rem_euclid(2048.0) - 1024.0;
    Some(d.abs())
}

/// Duke `ifbulletnear`: a `MISSILE` within 64 Room units (~1024 Build).
const BULLET_NEAR: f64 = 64.0;

fn player_bullet_near(world: &World) -> bool {
    let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    let Some(a) = world.actors.get(mo) else {
        return false;
    };
    let min2 = BULLET_NEAR * BULLET_NEAR;
    for &id in &world.thinkers.default {
        if id == mo {
            continue;
        }
        let Some(b) = world.actors.get(id) else {
            continue;
        };
        if b.destroyed {
            continue;
        }
        let flags = b.flags | world.classes.get(b.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_MISSILE) == 0 {
            continue;
        }
        let dx = b.pos.x() - a.pos.x();
        let dy = b.pos.y() - a.pos.y();
        if dx * dx + dy * dy < min2 {
            return true;
        }
    }
    false
}

/// Duke `useractor` think: `event USERACTOR` runs on living `COUNTKILL`s each tic
/// (`count` / `addstrength` / `strength` / `sleeptime` / `ai` / `move` / `state` /
/// `action` / `findplayer` / `shoot` / `ifcansee` / `ifpdistl` / `ifpdistg` /
/// `ifstrength` / `ifcount` / `killit` / `guts` / `sound` / `print` /
/// `spritepal` / `spawn` / `ifspritepal` / `soundonce` / `cstat` / `cstator` /
/// `sizeat` / `sizeto` / `clipdist` / `ifcanseetarget` / `cactor` /
/// `getlastpal` / `stopsound` / `actioncount` / `ifactioncount` /
/// `ifp` / `ifhitspace` / `ifgotweaponce` / `ifpinventory` /
/// `ifhealthg` / `break` / `setplayerangle` / `setactorangle` /
/// `getplayerangle` / `getactorangle`).
fn apply_useractor(world: &mut World, text: &str) {
    #[derive(Clone)]
    enum Op {
        AddStrength(i32),
        Strength(i32),
        Count(i32),
        ResetCount,
        ActionCount(i32),
        ResetActionCount,
        SleepTime(i32),
        Jump(String),
        FindPlayer,
        Shoot(String),
        ZShoot(String, i32),
        IfCanSee,
        IfPDistL(i32),
        IfPDistG(i32),
        IfStrength(i32),
        IfHealthG(i32),
        IfCount(i32),
        IfActionCount(i32),
        IfRnd(i32),
        IfActor(String),
        IfSpawnedBy(String),
        IfActorNotStayPut,
        IfAction(String),
        IfInWater,
        IfOnWater,
        IfOutside,
        IfInSpace,
        IfInOuterSpace,
        IfFloorDistL(i32),
        IfCeilDistL(i32),
        IfFloorDistG(i32),
        IfCeilDistG(i32),
        IfGapZL(i32),
        IfGapZG(i32),
        IfAwayFromWall,
        IfAngDiffL(i32),
        IfAngDiffG(i32),
        IfBulletNear,
        IfCanShootTarget,
        IfNoSounds,
        IfSound(String),
        IfWasWeapon(String),
        IfHitWeapon,
        IfPalSwap,
        IfSomethingOnPlayer,
        IfMultiplayer,
        IfSquished,
        IfRespawn,
        IfDead,
        IfNotMoving,
        IfP(String),
        IfHitSpace,
        IfGotWeapon,
        IfInventory(String),
        IfSpritePal(i32),
        IfCanSeeTarget,
        KillIt,
        Guts(String, i32),
        Sound(String),
        SoundOnce(String),
        SpriteSound(String),
        StopSound(String),
        Print(String),
        SpritePal(i32),
        Spawn(String),
        CStat(i32),
        CStatOr(i32),
        SizeAt(i32, i32),
        SizeTo(i32, i32),
        ClipDist(i32),
        CActor(String),
        GetLastPal,
        HitRadius(i32, i32),
        Fall,
        Operate,
        OperateActivators(i32),
        OperateSectors,
        LotsOfGlass(i32),
        Money(i32),
        Mail(i32),
        Paper(i32),
        LotsOfColour(i32),
        AddKills(i32),
        Tip,
        MikeSnd,
        Quake(i32),
        EndOfGame,
        Debug,
        PStomp,
        WackPlayer,
        PKick,
        LockPlayer(i32),
        PalFrom(i32),
        AddPHealth(i32),
        ResetPlayer,
        TossWeapon,
        ResetWeapons,
        ResetInventory,
        AddWeapon(String, i32),
        AddInventory(String, i32),
        AddAmmo(String, i32),
        MoveSprite(i32, i32, i32),
        SetSprite(i32, i32, i32),
        ClipMove(i32, i32, i32),
        ChangeSpriteStat(i32),
        WackMove(i32, i32, i32),
        GetZRange,
        UpdateSector,
        GetFlorZOfSlope,
        GetCeilZOfSlope,
        RespawnHitag(i32),
        FindOtherPlayer,
        SetPlayerAngle(i32),
        SetActorAngle(i32),
        GetPlayerAngle,
        GetActorAngle,
        MoveVel(i32, i32, i32),
        Break,
        Else,
    }
    let mut want = false;
    let mut ops = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        match words.next().map(str::to_ascii_lowercase).as_deref() {
            Some("event") => {
                let name = words.next().unwrap_or("").to_ascii_uppercase();
                want = name == "USERACTOR";
            }
            Some("end") => want = false,
            Some("addstrength") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::AddStrength(n));
            }
            Some("strength") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(0)
                    .max(0);
                ops.push(Op::Strength(n));
            }
            Some("count") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(0)
                    .max(0);
                ops.push(Op::Count(n));
            }
            Some("resetcount") if want => ops.push(Op::ResetCount),
            Some("actioncount") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(0)
                    .max(0);
                ops.push(Op::ActionCount(n));
            }
            Some("resetactioncount") if want => ops.push(Op::ResetActionCount),
            Some("sleeptime") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(0)
                    .max(0);
                ops.push(Op::SleepTime(n));
            }
            Some("state" | "ai" | "move" | "action") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::Jump(name.to_owned()));
                }
            }
            Some("findplayer") if want => ops.push(Op::FindPlayer),
            Some("shoot") | Some("eshoot") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::Shoot(name.to_owned()));
                }
            }
            Some("zshoot") | Some("ezshoot") if want => {
                if let Some(name) = words.next() {
                    let z = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                    ops.push(Op::ZShoot(name.to_owned(), z));
                }
            }
            Some("ifcansee") if want => ops.push(Op::IfCanSee),
            Some("ifpdistl") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfPDistL(n));
            }
            Some("ifpdistg") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfPDistG(n));
            }
            Some("ifstrength") | Some("ifhealthl") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfStrength(n));
            }
            Some("ifhealthg") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfHealthG(n));
            }
            Some("ifcount") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfCount(n));
            }
            Some("ifactioncount") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfActionCount(n));
            }
            Some("ifrand") | Some("ifrnd") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfRnd(n));
            }
            Some("ifactor") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::IfActor(name.to_owned()));
                }
            }
            Some("ifspawnedby") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::IfSpawnedBy(name.to_owned()));
                }
            }
            Some("ifactornotstayput") if want => ops.push(Op::IfActorNotStayPut),
            Some("ifaction") | Some("ifai") | Some("ifmove") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::IfAction(name.to_owned()));
                }
            }
            Some("ifinwater") if want => ops.push(Op::IfInWater),
            Some("ifonwater") if want => ops.push(Op::IfOnWater),
            Some("ifoutside") if want => ops.push(Op::IfOutside),
            Some("ifinspace") if want => ops.push(Op::IfInSpace),
            Some("ifinouterspace") if want => ops.push(Op::IfInOuterSpace),
            Some("iffloordistl") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfFloorDistL(n));
            }
            Some("ifceilingdistl") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfCeilDistL(n));
            }
            Some("iffloordistg") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfFloorDistG(n));
            }
            Some("ifceilingdistg") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfCeilDistG(n));
            }
            Some("ifgapzl") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfGapZL(n));
            }
            Some("ifgapzg") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfGapZG(n));
            }
            Some("ifawayfromwall") if want => ops.push(Op::IfAwayFromWall),
            Some("ifangdiffl") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfAngDiffL(n));
            }
            Some("ifangdiffg") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfAngDiffG(n));
            }
            Some("ifbulletnear") if want => ops.push(Op::IfBulletNear),
            Some("ifcanshoottarget") if want => ops.push(Op::IfCanShootTarget),
            Some("ifnosounds") if want => ops.push(Op::IfNoSounds),
            Some("ifsound") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::IfSound(name.to_owned()));
                }
            }
            Some("ifwasweapon") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::IfWasWeapon(name.to_owned()));
                }
            }
            Some("ifhitweapon") if want => ops.push(Op::IfHitWeapon),
            Some("ifpalswap") if want => ops.push(Op::IfPalSwap),
            Some("ifsomethingonplayer") if want => ops.push(Op::IfSomethingOnPlayer),
            Some("ifmultiplayer") if want => ops.push(Op::IfMultiplayer),
            Some("ifsquished") if want => ops.push(Op::IfSquished),
            Some("ifrespawn") if want => ops.push(Op::IfRespawn),
            Some("ifdead") if want => ops.push(Op::IfDead),
            Some("ifnotmoving") if want => ops.push(Op::IfNotMoving),
            Some("ifp") if want => {
                if let Some(flag) = words.next() {
                    ops.push(Op::IfP(flag.to_ascii_lowercase()));
                }
            }
            Some("ifhitspace") if want => ops.push(Op::IfHitSpace),
            Some("ifgotweaponce") if want => ops.push(Op::IfGotWeapon),
            Some("ifpinventory") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::IfInventory(name.to_owned()));
                }
            }
            Some("ifspritepal") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::IfSpritePal(n));
            }
            Some("ifcanseetarget") if want => ops.push(Op::IfCanSeeTarget),
            Some("killit") if want => ops.push(Op::KillIt),
            Some("guts") | Some("debris") if want => {
                let name = words.next().unwrap_or("").to_owned();
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                if !name.is_empty() {
                    ops.push(Op::Guts(name, n));
                }
            }
            Some("sound") | Some("globalsound") if want => {
                if let Some(s) = words.next() {
                    ops.push(Op::Sound(s.to_owned()));
                }
            }
            Some("soundonce") if want => {
                if let Some(s) = words.next() {
                    ops.push(Op::SoundOnce(s.to_owned()));
                }
            }
            Some("spritesound") if want => {
                if let Some(s) = words.next() {
                    ops.push(Op::SpriteSound(s.to_owned()));
                }
            }
            Some("stopsound") if want => {
                if let Some(s) = words.next() {
                    ops.push(Op::StopSound(s.to_owned()));
                }
            }
            Some("print") | Some("quote") if want => {
                let t = words.collect::<Vec<_>>().join(" ");
                if !t.is_empty() {
                    ops.push(Op::Print(t));
                }
            }
            Some("spritepal") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::SpritePal(n));
            }
            Some("spawn") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::Spawn(name.to_owned()));
                }
            }
            Some("cstat") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::CStat(n));
            }
            Some("cstator") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::CStatOr(n));
            }
            Some("sizeat") if want => {
                let x = words.next().and_then(|w| w.parse().ok()).unwrap_or(64);
                let y = words.next().and_then(|w| w.parse().ok()).unwrap_or(64);
                ops.push(Op::SizeAt(x, y));
            }
            Some("sizeto") if want => {
                let x = words.next().and_then(|w| w.parse().ok()).unwrap_or(64);
                let y = words.next().and_then(|w| w.parse().ok()).unwrap_or(64);
                ops.push(Op::SizeTo(x, y));
            }
            Some("clipdist") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::ClipDist(n));
            }
            Some("cactor") if want => {
                if let Some(name) = words.next() {
                    ops.push(Op::CActor(name.to_owned()));
                }
            }
            Some("getlastpal") if want => ops.push(Op::GetLastPal),
            Some("hitradius") if want => {
                let r = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let d = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::HitRadius(r, d));
            }
            Some("fall") if want => ops.push(Op::Fall),
            Some("operate") if want => ops.push(Op::Operate),
            Some("operateactivators") if want => {
                if let Some(n) = words.next().and_then(|w| w.parse().ok()) {
                    ops.push(Op::OperateActivators(n));
                }
            }
            Some("operatesectors") if want => ops.push(Op::OperateSectors),
            Some("lotsofglass") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                ops.push(Op::LotsOfGlass(n));
            }
            Some("money") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                ops.push(Op::Money(n));
            }
            Some("mail") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                ops.push(Op::Mail(n));
            }
            Some("paper") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                ops.push(Op::Paper(n));
            }
            Some("lotsofcolour") | Some("lotsofcolor") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                ops.push(Op::LotsOfColour(n));
            }
            Some("addkills") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::AddKills(n));
            }
            Some("tip") if want => ops.push(Op::Tip),
            Some("mikesnd") if want => ops.push(Op::MikeSnd),
            Some("quake") | Some("earthquake") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::Quake(n));
            }
            Some("endofgame") if want => ops.push(Op::EndOfGame),
            Some("debug") | Some("addlog") if want => ops.push(Op::Debug),
            Some("pstomp") if want => ops.push(Op::PStomp),
            Some("wackplayer") if want => ops.push(Op::WackPlayer),
            Some("pkick") if want => ops.push(Op::PKick),
            Some("lockplayer") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::LockPlayer(n));
            }
            Some("palfrom") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(32);
                ops.push(Op::PalFrom(n));
            }
            Some("addphealth") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::AddPHealth(n));
            }
            Some("resetplayer") if want => ops.push(Op::ResetPlayer),
            Some("tossweapon") if want => ops.push(Op::TossWeapon),
            Some("resetweapons") if want => ops.push(Op::ResetWeapons),
            Some("resetinventory") if want => ops.push(Op::ResetInventory),
            Some("addweapon") if want => {
                let name = words.next().unwrap_or("").to_owned();
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::AddWeapon(name, n));
            }
            Some("addinventory") if want => {
                let name = words.next().unwrap_or("").to_owned();
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(1);
                ops.push(Op::AddInventory(name, n));
            }
            Some("addammo") if want => {
                let name = words.next().unwrap_or("").to_owned();
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::AddAmmo(name, n));
            }
            Some("movesprite") if want => {
                let x = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let y = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let z = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::MoveSprite(x, y, z));
            }
            Some("setsprite") if want => {
                let x = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let y = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let z = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::SetSprite(x, y, z));
            }
            Some("clipmove") if want => {
                let x = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let y = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let z = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::ClipMove(x, y, z));
            }
            Some("changespritestat") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::ChangeSpriteStat(n));
            }
            Some("wackmove") if want => {
                let x = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let y = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let z = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::WackMove(x, y, z));
            }
            Some("getzrange") if want => ops.push(Op::GetZRange),
            Some("updatesector") | Some("updatesectorz") if want => ops.push(Op::UpdateSector),
            Some("getflorzofslope") if want => ops.push(Op::GetFlorZOfSlope),
            Some("getceilzofslope") if want => ops.push(Op::GetCeilZOfSlope),
            Some("respawnhitag") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::RespawnHitag(n));
            }
            Some("findotherplayer") if want => ops.push(Op::FindOtherPlayer),
            Some("else") if want => ops.push(Op::Else),
            Some("break") if want => ops.push(Op::Break),
            Some("setplayerangle") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::SetPlayerAngle(n));
            }
            Some("setactorangle") if want => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::SetActorAngle(n));
            }
            Some("getplayerangle") if want => ops.push(Op::GetPlayerAngle),
            Some("getactorangle") if want => ops.push(Op::GetActorAngle),
            Some("movevel") if want => {
                let vel = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let angvel = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                let flags = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                ops.push(Op::MoveVel(vel, angvel, flags));
            }
            _ => {}
        }
    }
    if ops.is_empty() {
        return;
    }
    let mut ids = Vec::new();
    for &id in &world.thinkers.default {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || a.health <= 0 {
            continue;
        }
        if world.players.iter().flatten().any(|p| p.mo == Some(id)) {
            continue;
        }
        let flags = a.flags | world.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_COUNTKILL) == 0 {
            continue;
        }
        ids.push(id);
    }
    for id in ids {
        if let Some(a) = world.actors.get_mut(id) {
            a.user_vars.remove("user_conchase");
        }
        let mut skip = false;
        for op in &ops {
            match op {
                Op::IfCanSee => skip = !useractor_can_see_player(world, id),
                Op::IfPDistL(n) => {
                    let lim = f64::from(*n) * f64::from(*n);
                    skip = useractor_build_dist2_to_player(world, id).is_none_or(|d2| d2 >= lim);
                }
                Op::IfPDistG(n) => {
                    let lim = f64::from(*n) * f64::from(*n);
                    skip = useractor_build_dist2_to_player(world, id).is_none_or(|d2| d2 <= lim);
                }
                Op::IfStrength(n) => {
                    let hp = world.actors.get(id).map(|a| a.health).unwrap_or(0);
                    skip = hp >= *n;
                }
                Op::IfHealthG(n) => {
                    let hp = world.actors.get(id).map(|a| a.health).unwrap_or(0);
                    skip = hp <= *n;
                }
                Op::IfCount(n) => {
                    let c = world
                        .actors
                        .get(id)
                        .and_then(|a| a.user_vars.get(USER_CONCOUNT).copied())
                        .unwrap_or(0)
                        .max(0);
                    skip = c < *n;
                }
                Op::IfActionCount(n) => {
                    let c = world
                        .actors
                        .get(id)
                        .and_then(|a| a.user_vars.get(USER_CONACTIONCOUNT).copied())
                        .unwrap_or(0)
                        .max(0);
                    skip = c < *n;
                }
                Op::IfRnd(n) => {
                    let roll = i32::from(world.p_random("pr_conifrnd"));
                    skip = roll >= (*n).clamp(0, 256);
                }
                Op::IfActor(name) => skip = !useractor_is_actor(world, id, name),
                Op::IfSpawnedBy(name) => skip = !useractor_spawned_by(world, id, name),
                Op::IfActorNotStayPut => skip = useractor_is_stayput(world, id),
                Op::IfAction(name) => skip = !useractor_has_action(world, id, name),
                Op::IfInWater => skip = useractor_waterlevel(world, id) < 2,
                Op::IfOnWater => skip = useractor_waterlevel(world, id) != 1,
                Op::IfOutside => skip = !useractor_under_sky(world, id),
                Op::IfInSpace => skip = !useractor_in_space(world, id),
                Op::IfInOuterSpace => skip = !useractor_in_outerspace(world, id),
                Op::IfFloorDistL(n) => {
                    skip = useractor_floor_build_dist(world, id).is_none_or(|d| d >= f64::from(*n));
                }
                Op::IfCeilDistL(n) => {
                    skip = useractor_ceil_build_dist(world, id).is_none_or(|d| d >= f64::from(*n));
                }
                Op::IfFloorDistG(n) => {
                    skip = useractor_floor_build_dist(world, id).is_none_or(|d| d < f64::from(*n));
                }
                Op::IfCeilDistG(n) => {
                    skip = useractor_ceil_build_dist(world, id).is_none_or(|d| d < f64::from(*n));
                }
                Op::IfGapZL(n) => {
                    skip = useractor_gap_build(world, id).is_none_or(|d| d >= f64::from(*n));
                }
                Op::IfGapZG(n) => {
                    skip = useractor_gap_build(world, id).is_none_or(|d| d < f64::from(*n));
                }
                Op::IfAwayFromWall => skip = !useractor_away_from_wall(world, id),
                Op::IfAngDiffL(n) => {
                    skip =
                        useractor_ang_diff_to_player(world, id).is_none_or(|d| d >= f64::from(*n));
                }
                Op::IfAngDiffG(n) => {
                    skip =
                        useractor_ang_diff_to_player(world, id).is_none_or(|d| d < f64::from(*n));
                }
                Op::IfBulletNear => skip = !useractor_bullet_near(world, id),
                Op::IfCanShootTarget => skip = !useractor_can_shoot_target(world, id),
                Op::IfNoSounds => {
                    skip = !world.sound_events.is_empty() || !world.sound_heard.is_empty();
                }
                Op::IfSound(name) => skip = !sound_is_queued(world, name),
                Op::IfWasWeapon(name) => skip = !useractor_was_weapon(world, id, name),
                Op::IfHitWeapon => skip = !useractor_hit_weapon(world, id),
                Op::IfPalSwap => skip = !player_pal_swap(world),
                Op::IfSomethingOnPlayer => skip = !player_something_on(world),
                Op::IfMultiplayer => {
                    skip = !world.deathmatch && world.players.iter().flatten().count() <= 1;
                }
                Op::IfSquished => skip = !useractor_squished(world, id),
                Op::IfRespawn => skip = !world.deathmatch && !world.respawn_monsters,
                Op::IfDead => {
                    skip = world.actors.get(id).is_some_and(|a| a.health > 0);
                }
                Op::IfNotMoving => skip = !useractor_not_moving(world, id),
                Op::IfP(flag) => skip = !player_ifp(world, flag),
                Op::IfHitSpace => {
                    skip = !world.players[0]
                        .as_ref()
                        .is_some_and(|p| p.cmd.buttons.any(Buttons::USE));
                }
                Op::IfGotWeapon => skip = !player_has_weapon(world),
                Op::IfInventory(name) => skip = !player_has_inventory(world, name),
                Op::IfSpritePal(n) => {
                    let pal = world
                        .actors
                        .get(id)
                        .map(|a| parse_sprite_pal(&a.translation))
                        .unwrap_or(0);
                    skip = pal != *n;
                }
                Op::IfCanSeeTarget => skip = !useractor_can_see_target(world, id),
                Op::Else => skip = !skip,
                _ if skip => {}
                Op::Break => break,
                Op::SetPlayerAngle(n) => conevt_setplayerangle(world, *n),
                Op::SetActorAngle(n) => conevt_set_angle_on(world, id, *n),
                Op::GetPlayerAngle => conevt_getplayerangle(world),
                Op::GetActorAngle => conevt_get_angle_on(world, id, USER_ACTORANGLE),
                Op::MoveVel(vel, angvel, flags) => conevt_movevel(world, id, *vel, *angvel, *flags),
                Op::AddStrength(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.health = a.health.saturating_add(*n).max(0);
                    }
                }
                Op::Strength(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.health = *n;
                    }
                }
                Op::Count(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.user_vars.insert(USER_CONCOUNT.to_owned(), *n);
                    }
                }
                Op::ResetCount => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.user_vars.insert(USER_CONCOUNT.to_owned(), 0);
                    }
                }
                Op::ActionCount(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.user_vars.insert(USER_CONACTIONCOUNT.to_owned(), *n);
                    }
                }
                Op::ResetActionCount => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.user_vars.insert(USER_CONACTIONCOUNT.to_owned(), 0);
                    }
                }
                Op::SleepTime(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.reaction_time = *n;
                    }
                }
                Op::Jump(name) => {
                    let class = world.actors.get(id).map(|a| a.class);
                    if let Some(class) = class {
                        if let Some(sid) = world.states.find_label(class, name) {
                            world.set_state(id, Some(sid), false);
                        }
                    }
                }
                Op::FindPlayer => {
                    let Some(pmo) = world.players[0].as_ref().and_then(|p| p.mo) else {
                        continue;
                    };
                    let Some((px, py)) = world.actors.get(pmo).map(|a| (a.pos.x(), a.pos.y()))
                    else {
                        continue;
                    };
                    let Some((ax, ay)) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y()))
                    else {
                        continue;
                    };
                    let dx = (px - ax) * 16.0;
                    let dy = (py - ay) * 16.0;
                    let dist = room_core::dmath::sqrt(dx * dx + dy * dy).round() as i32;
                    let deg =
                        room_core::dmath::atan2(py - ay, px - ax) * (180.0 / core::f64::consts::PI);
                    if let Some(a) = world.actors.get_mut(id) {
                        a.target = Some(pmo);
                        a.angle = room_core::Angle::from_degrees(deg);
                        a.user_vars.insert(USER_FINDPLAYER.to_owned(), dist);
                    }
                }
                Op::Shoot(name) => conevt_shoot_from(world, id, name, 0),
                Op::ZShoot(name, z) => conevt_shoot_from(world, id, name, *z),
                Op::KillIt => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.health = 0;
                    }
                }
                Op::Guts(name, n) => conevt_spawn_from(world, id, name, *n, true),
                Op::Sound(name) => {
                    if let Some((x, y, z)) = world
                        .actors
                        .get(id)
                        .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
                    {
                        world.play_sound(name, x, y, z, 1.0);
                    }
                }
                Op::SpriteSound(name) => {
                    if let Some((x, y, z)) = world
                        .actors
                        .get(id)
                        .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
                    {
                        world.play_sound(name, x, y, z, 1.0);
                    }
                }
                Op::SoundOnce(name) => {
                    if !sound_is_queued(world, name) {
                        if let Some((x, y, z)) = world
                            .actors
                            .get(id)
                            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
                        {
                            world.play_sound(name, x, y, z, 1.0);
                        }
                    }
                }
                Op::StopSound(name) => conevt_stopsound(world, name),
                Op::Print(t) => world.push_message(t),
                Op::SpritePal(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        apply_duke_spritepal(a, *n);
                    }
                }
                Op::Spawn(name) => conevt_spawn_from(world, id, name, 1, false),
                Op::CStat(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        apply_duke_cstat(a, *n);
                    }
                }
                Op::CStatOr(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        apply_duke_cstator(a, *n);
                    }
                }
                Op::SizeAt(x, y) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.scale_x = f64::from(*x) / 64.0;
                        a.scale_y = f64::from(*y) / 64.0;
                    }
                }
                Op::SizeTo(x, y) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.scale_x = step_duke_repeat(a.scale_x, *x);
                        a.scale_y = step_duke_repeat(a.scale_y, *y);
                    }
                }
                Op::ClipDist(n) => {
                    if let Some(a) = world.actors.get_mut(id) {
                        a.radius = (f64::from(*n) / 16.0).clamp(1.0, 64.0);
                    }
                }
                Op::CActor(name) => {
                    if let Some(cid) = find_con_actor(world, name) {
                        if let Some(a) = world.actors.get_mut(id) {
                            a.class = cid;
                        }
                    }
                }
                Op::GetLastPal => {
                    if let Some(a) = world.actors.get_mut(id) {
                        let n = a.user_vars.get(USER_LASTPAL).copied().unwrap_or(0);
                        apply_duke_spritepal(a, n);
                    }
                }
                Op::HitRadius(r, d) => conevt_hitradius_from(world, id, *r, *d),
                Op::Fall => conevt_fall_from(world, id),
                Op::Operate => {
                    let _ = room_sim::use_lines(world, id);
                }
                Op::OperateActivators(n) => conevt_operateactivators(world, *n, Some(id)),
                Op::OperateSectors => conevt_operatesectors(world, id),
                Op::LotsOfGlass(n) => conevt_spawn_from(world, id, "GLASSPIECES", *n, true),
                Op::Money(n) => conevt_spawn_from(world, id, "MONEY", *n, true),
                Op::Mail(n) => conevt_spawn_from(world, id, "MAIL", *n, true),
                Op::Paper(n) => conevt_spawn_from(world, id, "PAPER", *n, true),
                Op::LotsOfColour(n) => conevt_lotsofcolour_from(world, id, *n),
                Op::AddKills(n) => {
                    let add = u32::try_from((*n).max(0)).unwrap_or(0);
                    world.level.found_kills = world.level.found_kills.saturating_add(add);
                }
                Op::Tip => {
                    world.push_message("TIP");
                    if let Some((x, y, z)) = world
                        .actors
                        .get(id)
                        .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
                    {
                        world.play_sound("duke/duke_tip1", x, y, z, 1.0);
                    }
                }
                Op::MikeSnd => {
                    if let Some((x, y, z)) = world
                        .actors
                        .get(id)
                        .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
                    {
                        world.play_sound("duke/duke_use1", x, y, z, 1.0);
                    }
                }
                Op::Quake(n) => world.start_duke_quake(*n),
                Op::EndOfGame => world.level.exiting = true,
                Op::Debug => world.push_message("DEBUG"),
                Op::PStomp => conevt_pstomp_from(world, id),
                Op::WackPlayer => {
                    if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                        if let Some(a) = world.actors.get_mut(mo) {
                            a.health = (a.health - 10).max(0);
                        }
                    }
                    if let Some(p) = world.players[0].as_mut() {
                        p.damage_count = p.damage_count.max(16);
                    }
                }
                Op::PKick => world.player_quick_kick(),
                Op::LockPlayer(n) => conevt_lockplayer(world, *n),
                Op::PalFrom(n) => {
                    if let Some(p) = world.players[0].as_mut() {
                        p.damage_count = (*n).clamp(0, 100);
                    }
                }
                Op::AddPHealth(n) => {
                    if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                        let max = world
                            .actors
                            .get(mo)
                            .and_then(|a| world.classes.get(a.class))
                            .map(|c| c.health.max(100))
                            .unwrap_or(100);
                        if let Some(a) = world.actors.get_mut(mo) {
                            a.health = (a.health + *n).clamp(0, max.max(a.health));
                        }
                    }
                }
                Op::ResetPlayer => conevt_resetplayer(world),
                Op::TossWeapon => conevt_tossweapon(world),
                Op::ResetWeapons => conevt_resetweapons(world),
                Op::ResetInventory => conevt_resetinventory(world),
                Op::AddWeapon(name, n) => conevt_addweapon(world, name, *n),
                Op::AddInventory(name, n) => conevt_addinventory(world, name, *n),
                Op::AddAmmo(name, n) => conevt_addammo(world, name, *n),
                Op::MoveSprite(x, y, z) => conevt_movesprite_from(world, id, *x, *y, *z),
                Op::SetSprite(x, y, z) => conevt_setsprite_from(world, id, *x, *y, *z),
                Op::ClipMove(x, y, z) => conevt_clipmove_from(world, id, *x, *y, *z),
                Op::ChangeSpriteStat(n) => conevt_changespritestat_from(world, id, *n),
                Op::WackMove(x, y, z) => conevt_wackmove_from(world, id, *x, *y, *z),
                Op::GetZRange => conevt_getzrange_from(world, id),
                Op::UpdateSector => conevt_updatesector_from(world, id),
                Op::GetFlorZOfSlope => conevt_getflorzofslope_from(world, id),
                Op::GetCeilZOfSlope => conevt_getceilzofslope_from(world, id),
                Op::RespawnHitag(n) => conevt_respawnhitag_from(world, id, *n),
                Op::FindOtherPlayer => conevt_findotherplayer_from(world, id),
            }
        }
    }
}

fn useractor_can_see_target(world: &World, id: room_sim::ActorId) -> bool {
    let Some(tid) = world.actors.get(id).and_then(|a| a.target) else {
        return false;
    };
    if tid == id {
        return false;
    }
    room_sim::check_sight(world, id, tid)
}

/// Duke `ifactor NAME` on a `useractor`: class matches that CON name
/// (`Dummy` / `DUKEDUMMY`). Does not treat `APLAYER` as always true.
fn useractor_is_actor(world: &World, id: room_sim::ActorId, name: &str) -> bool {
    let Some(a) = world.actors.get(id) else {
        return false;
    };
    let Some(class) = world.classes.get(a.class) else {
        return false;
    };
    let c = class.name.to_ascii_uppercase();
    let w = name.trim().to_ascii_uppercase();
    if w.is_empty() {
        return false;
    }
    c == w || c == format!("DUKE{w}") || (w != "APLAYER" && w != "PLAYER" && c.contains(&w))
}

fn useractor_spawned_by(world: &World, id: room_sim::ActorId, name: &str) -> bool {
    let Some(mid) = world.actors.get(id).and_then(|a| a.master) else {
        return false;
    };
    if mid == id {
        return false;
    }
    useractor_is_actor(world, mid, name)
}

fn useractor_has_action(world: &World, id: room_sim::ActorId, name: &str) -> bool {
    let want = name.trim().to_ascii_uppercase();
    if want.is_empty() {
        return false;
    }
    let Some(a) = world.actors.get(id) else {
        return false;
    };
    let Some(sid) = a.state else {
        return false;
    };
    world
        .states
        .labels_of(a.class)
        .any(|(label, id)| id == sid && label.to_ascii_uppercase() == want)
}

fn useractor_sector_index(world: &World, id: room_sim::ActorId) -> Option<usize> {
    let a = world.actors.get(id)?;
    room_sim::sector_at_xy(world, a.pos.x(), a.pos.y())
}

fn useractor_ceiling_pic(world: &World, si: usize) -> &str {
    world
        .sector_ceiling_pics
        .get(&(si as u32))
        .map(String::as_str)
        .or_else(|| world.map.sectors.get(si).map(|s| s.ceiling_pic.as_str()))
        .unwrap_or("")
}

fn useractor_floor_pic(world: &World, si: usize) -> &str {
    world
        .sector_floor_pics
        .get(&(si as u32))
        .map(String::as_str)
        .or_else(|| world.map.sectors.get(si).map(|s| s.floor_pic.as_str()))
        .unwrap_or("")
}

fn useractor_under_sky(world: &World, id: room_sim::ActorId) -> bool {
    useractor_sector_index(world, id).is_some_and(|si| is_sky_pic(useractor_ceiling_pic(world, si)))
}

fn useractor_in_space(world: &World, id: room_sim::ActorId) -> bool {
    useractor_sector_index(world, id).is_some_and(|si| {
        is_sky_pic(useractor_ceiling_pic(world, si)) && is_sky_pic(useractor_floor_pic(world, si))
    })
}

fn useractor_floor_build_dist(world: &World, id: room_sim::ActorId) -> Option<f64> {
    let a = world.actors.get(id)?;
    let floor = room_sim::floor_at_xy(world, a.pos.x(), a.pos.y())?;
    Some((a.pos.z() - floor).max(0.0) * 256.0)
}

fn useractor_can_shoot_target(world: &World, id: room_sim::ActorId) -> bool {
    let tid = world
        .actors
        .get(id)
        .and_then(|a| a.target)
        .or_else(|| world.players[0].as_ref().and_then(|p| p.mo));
    let Some(tid) = tid else {
        return false;
    };
    if tid == id {
        return false;
    }
    if !room_sim::check_sight(world, id, tid) {
        return false;
    }
    useractor_ang_diff_to(world, id, tid).is_some_and(|d| d < CAN_SHOOT_ANG)
}

fn useractor_ang_diff_to(
    world: &World,
    id: room_sim::ActorId,
    tid: room_sim::ActorId,
) -> Option<f64> {
    let a = world.actors.get(id)?;
    let t = world.actors.get(tid)?;
    let dx = t.pos.x() - a.pos.x();
    let dy = t.pos.y() - a.pos.y();
    if dx * dx + dy * dy <= 1e-12 {
        return Some(0.0);
    }
    let to = room_core::dmath::atan2(dy, dx) * (180.0 / core::f64::consts::PI);
    let fa = a.angle.degrees().rem_euclid(360.0) * (2048.0 / 360.0);
    let ta = to.rem_euclid(360.0) * (2048.0 / 360.0);
    let d = (fa - ta + 1024.0).rem_euclid(2048.0) - 1024.0;
    Some(d.abs())
}

fn useractor_bullet_near(world: &World, id: room_sim::ActorId) -> bool {
    let Some(a) = world.actors.get(id) else {
        return false;
    };
    let min2 = BULLET_NEAR * BULLET_NEAR;
    for &oid in &world.thinkers.default {
        if oid == id {
            continue;
        }
        let Some(b) = world.actors.get(oid) else {
            continue;
        };
        if b.destroyed {
            continue;
        }
        let flags = b.flags | world.classes.get(b.class).map(|c| c.flags).unwrap_or(0);
        if (flags & room_sim::FLAG_MISSILE) == 0 {
            continue;
        }
        let dx = b.pos.x() - a.pos.x();
        let dy = b.pos.y() - a.pos.y();
        if dx * dx + dy * dy < min2 {
            return true;
        }
    }
    false
}

fn useractor_ang_diff_to_player(world: &World, id: room_sim::ActorId) -> Option<f64> {
    let a = world.actors.get(id)?;
    let pmo = world.players[0].as_ref().and_then(|p| p.mo)?;
    if pmo == id {
        return None;
    }
    let p = world.actors.get(pmo)?;
    let dx = p.pos.x() - a.pos.x();
    let dy = p.pos.y() - a.pos.y();
    if dx * dx + dy * dy <= 1e-12 {
        return Some(0.0);
    }
    let to = room_core::dmath::atan2(dy, dx) * (180.0 / core::f64::consts::PI);
    let fa = a.angle.degrees().rem_euclid(360.0) * (2048.0 / 360.0);
    let ta = to.rem_euclid(360.0) * (2048.0 / 360.0);
    let d = (fa - ta + 1024.0).rem_euclid(2048.0) - 1024.0;
    Some(d.abs())
}

fn useractor_away_from_wall(world: &World, id: room_sim::ActorId) -> bool {
    let Some(a) = world.actors.get(id) else {
        return false;
    };
    let (px, py) = (a.pos.x(), a.pos.y());
    let min2 = AWAY_FROM_WALL * AWAY_FROM_WALL;
    for line in &world.map.lines {
        let blocking = line.back_sector.is_none() || (i32::from(line.flags) & 0x0001) != 0;
        if !blocking {
            continue;
        }
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        if point_seg_dist2(px, py, v1.pos.x(), v1.pos.y(), v2.pos.x(), v2.pos.y()) < min2 {
            return false;
        }
    }
    true
}

fn useractor_gap_build(world: &World, id: room_sim::ActorId) -> Option<f64> {
    let a = world.actors.get(id)?;
    let floor = room_sim::floor_at_xy(world, a.pos.x(), a.pos.y())?;
    let ceil = room_sim::ceiling_at_xy(world, a.pos.x(), a.pos.y())?;
    Some((ceil - floor).max(0.0) * 256.0)
}

fn useractor_ceil_build_dist(world: &World, id: room_sim::ActorId) -> Option<f64> {
    let a = world.actors.get(id)?;
    let ceil = room_sim::ceiling_at_xy(world, a.pos.x(), a.pos.y())?;
    Some((ceil - (a.pos.z() + a.height)).max(0.0) * 256.0)
}

fn useractor_in_outerspace(world: &World, id: room_sim::ActorId) -> bool {
    useractor_sector_index(world, id).is_some_and(|si| is_sky_pic(useractor_floor_pic(world, si)))
}

fn useractor_waterlevel(world: &World, id: room_sim::ActorId) -> i32 {
    world
        .actors
        .get(id)
        .map(|a| room_sim::actor_waterlevel(world, a))
        .unwrap_or(0)
}

fn useractor_is_stayput(world: &World, id: room_sim::ActorId) -> bool {
    world
        .actors
        .get(id)
        .and_then(|a| a.user_vars.get("user_stayput").copied())
        .is_some_and(|t| t > 0)
}

fn useractor_can_see_player(world: &World, id: room_sim::ActorId) -> bool {
    let Some(pmo) = world.players[0].as_ref().and_then(|p| p.mo) else {
        return false;
    };
    if pmo == id {
        return false;
    }
    room_sim::check_sight(world, id, pmo)
}

fn useractor_build_dist2_to_player(world: &World, id: room_sim::ActorId) -> Option<f64> {
    let pmo = world.players[0].as_ref().and_then(|p| p.mo)?;
    if pmo == id {
        return None;
    }
    let (px, py) = world.actors.get(pmo).map(|a| (a.pos.x(), a.pos.y()))?;
    let (ax, ay) = world.actors.get(id).map(|a| (a.pos.x(), a.pos.y()))?;
    let dx = (px - ax) * 16.0;
    let dy = (py - ay) * 16.0;
    Some(dx * dx + dy * dy)
}

/// Duke `CONEVT`: `event NAME` / `print` / `sound` / `end`.
fn apply_conevt(world: &mut World, text: &str, event: &str) {
    let want_name = event.trim().to_ascii_uppercase();
    let (x, y, z) = world.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .and_then(|mo| world.actors.get(mo))
        .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
        .unwrap_or((0.0, 0.0, 0.0));
    let hp = player0_health(world).unwrap_or(0);
    let water = world.players[0]
        .as_ref()
        .and_then(|p| p.mo)
        .and_then(|mo| world.actors.get(mo))
        .map(|a| room_sim::actor_waterlevel(world, a))
        .unwrap_or(0);
    let mut want = false;
    let mut skip = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        match words.next().map(str::to_ascii_lowercase).as_deref() {
            Some("event") => {
                let name = words.next().unwrap_or("").to_ascii_uppercase();
                want = name == want_name;
                skip = false;
            }
            Some("ifrand") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let roll = i32::from(world.p_random("pr_conifrnd"));
                skip = roll >= n.clamp(0, 256);
            }
            Some("ifhealthl") | Some("ifstrength") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = hp >= n;
            }
            Some("ifhealthg") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = hp <= n;
            }
            Some("ifinwater") if want => skip = water < 2,
            Some("ifonwater") if want => skip = water != 1,
            Some("ifdead") if want => skip = hp > 0,
            Some("ifmultiplayer") if want => {
                let extra = world.players.iter().flatten().count() > 1;
                skip = !world.deathmatch && !extra;
            }
            Some("ifp") if want => {
                let flag = words.next().unwrap_or("").to_ascii_lowercase();
                let on_ground = world.players[0].as_ref().is_some_and(|p| p.on_ground);
                let vel_z = world.players[0]
                    .as_ref()
                    .and_then(|p| p.mo)
                    .and_then(|mo| world.actors.get(mo))
                    .map(|a| a.vel.z())
                    .unwrap_or(0.0);
                let moving = world.players[0]
                    .as_ref()
                    .is_some_and(|p| p.cmd.forward_move != 0 || p.cmd.side_move != 0);
                skip = !match flag.as_str() {
                    "palive" => hp > 0,
                    "pdead" => hp <= 0,
                    "ponground" => on_ground,
                    "pfalling" => !on_ground && vel_z < 0.0,
                    "pjumping" => !on_ground && vel_z > 0.0,
                    "pstanding" => on_ground && !moving,
                    "pwalking" | "prunning" => moving,
                    "pducking" => world.players[0].as_ref().is_some_and(|p| p.crouching),
                    "pjetpack" => world.players[0].as_ref().is_some_and(|p| p.fly_tics > 0),
                    "ponsteroids" => world.players[0]
                        .as_ref()
                        .is_some_and(|p| p.steroid_tics > 0),
                    "pshrunk" => world.players[0]
                        .as_ref()
                        .and_then(|p| p.mo)
                        .and_then(|mo| world.actors.get(mo))
                        .and_then(|a| a.user_vars.get("user_dukeshink").copied())
                        .is_some_and(|t| t > 0),
                    "pkicking" => world.players[0]
                        .as_ref()
                        .is_some_and(|p| p.quick_kick_restore.is_some()),
                    "phigher" => player_is_higher(world),
                    "pwalkingback" | "prunningback" => world.players[0]
                        .as_ref()
                        .is_some_and(|p| p.cmd.forward_move < 0),
                    "paiming" | "plooking" => {
                        world.players[0].as_ref().is_some_and(|p| p.cmd.pitch != 0)
                    }
                    "pweapon" => player_has_weapon(world),
                    "pweaponout" => world.players[0]
                        .as_ref()
                        .is_some_and(|p| p.ready_weapon.is_some()),
                    _ => false,
                };
            }
            Some("ifoutside") if want => skip = !player_under_sky(world),
            Some("ifhitweapon") if want => skip = world.last_damage.is_none(),
            Some("ifpalswap") if want => skip = !player_pal_swap(world),
            Some("ifsomethingonplayer") if want => skip = !player_something_on(world),
            Some("ifsquished") if want => skip = !player_squished(world),
            Some("ifpinventory") if want => {
                let item = words.next().unwrap_or("");
                skip = !player_has_inventory(world, item);
            }
            Some("ifcansee") if want => skip = !player_can_see(world),
            Some("ifcanseetarget") if want => skip = !player_can_see_target(world),
            Some("ifcanshoottarget") if want => skip = !player_can_shoot_target(world),
            Some("ifspritepal") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_sprite_pal(world) != n;
            }
            Some("ifactor") if want => {
                let name = words.next().unwrap_or("");
                skip = !player_is_actor(world, name);
            }
            Some("ifspawnedby") if want => {
                let name = words.next().unwrap_or("");
                skip = !player_spawned_by(world, name);
            }
            Some("ifactornotstayput") if want => skip = player_is_stayput(world),
            Some("ifaction") | Some("ifai") | Some("ifmove") if want => {
                let name = words.next().unwrap_or("");
                skip = !player_has_action(world, name);
            }
            Some("ifcount") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_con_count(world) < u32::try_from(n.max(0)).unwrap_or(0);
            }
            Some("ifactioncount") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_con_action_count(world) < u32::try_from(n.max(0)).unwrap_or(0);
            }
            Some("ifpdistl") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let lim = f64::from(n) * f64::from(n);
                skip = nearest_countkill_room2(world)
                    .is_none_or(|r2| build_dist2_from_room2(r2) >= lim);
            }
            Some("ifpdistg") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let lim = f64::from(n) * f64::from(n);
                skip = nearest_countkill_room2(world)
                    .is_none_or(|r2| build_dist2_from_room2(r2) <= lim);
            }
            Some("ifhitspace") if want => {
                skip = !world.players[0]
                    .as_ref()
                    .is_some_and(|p| p.cmd.buttons.any(Buttons::USE));
            }
            Some("ifgotweaponce") if want => skip = !player_has_weapon(world),
            Some("ifnotmoving") if want => {
                skip = world.players[0]
                    .as_ref()
                    .is_some_and(|p| p.cmd.forward_move != 0 || p.cmd.side_move != 0);
            }
            Some("ifrespawn") if want => {
                skip = !world.deathmatch && !world.respawn_monsters;
            }
            Some("iffloordistl") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_floor_build_dist(world).is_none_or(|d| d >= f64::from(n));
            }
            Some("ifceilingdistl") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_ceil_build_dist(world).is_none_or(|d| d >= f64::from(n));
            }
            Some("iffloordistg") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_floor_build_dist(world).is_none_or(|d| d < f64::from(n));
            }
            Some("ifceilingdistg") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_ceil_build_dist(world).is_none_or(|d| d < f64::from(n));
            }
            Some("ifawayfromwall") if want => skip = !player_away_from_wall(world),
            Some("ifinspace") if want => skip = !player_in_space(world),
            Some("ifinouterspace") if want => skip = !player_in_outerspace(world),
            Some("ifwasweapon") if want => {
                let name = words.next().unwrap_or("");
                skip = !player_was_weapon(world, name);
            }
            Some("ifnosounds") if want => {
                skip = !world.sound_events.is_empty() || !world.sound_heard.is_empty();
            }
            Some("ifsound") if want => {
                let name = words.next().unwrap_or("");
                skip = !sound_is_queued(world, name);
            }
            Some("ifgapzl") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_gap_build(world).is_none_or(|d| d >= f64::from(n));
            }
            Some("ifgapzg") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_gap_build(world).is_none_or(|d| d < f64::from(n));
            }
            Some("ifangdiffl") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_ang_diff_build(world).is_none_or(|d| d >= f64::from(n));
            }
            Some("ifangdiffg") if want => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                skip = player_ang_diff_build(world).is_none_or(|d| d < f64::from(n));
            }
            Some("ifbulletnear") if want => skip = !player_bullet_near(world),
            Some("print") if want && !skip => {
                let t = line.get(5..).unwrap_or("").trim();
                if !t.is_empty() {
                    world.push_message(t);
                }
            }
            Some("sound") if want && !skip => {
                if let Some(s) = words.next() {
                    world.play_sound(s, x, y, z, 1.0);
                }
            }
            Some("soundonce") if want && !skip => {
                if let Some(s) = words.next() {
                    if !sound_is_queued(world, s) {
                        world.play_sound(s, x, y, z, 1.0);
                    }
                }
            }
            Some("spritesound") if want && !skip => {
                if let Some(s) = words.next() {
                    world.play_sound(s, x, y, z, 1.0);
                }
            }
            Some("stopsound") if want && !skip => {
                if let Some(s) = words.next() {
                    conevt_stopsound(world, s);
                }
            }
            Some("state" | "ai" | "move" | "action") if want && !skip => {
                if let Some(name) = words.next() {
                    player_goto_state(world, name);
                }
            }
            Some("else") if want => skip = !skip,
            Some("break") if want && !skip => want = false,
            Some("palfrom") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(32)
                    .clamp(0, 100);
                if let Some(p) = world.players[0].as_mut() {
                    p.damage_count = n;
                }
            }
            Some("addphealth") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    let max = world
                        .actors
                        .get(mo)
                        .and_then(|a| world.classes.get(a.class))
                        .map(|c| c.health.max(100))
                        .unwrap_or(100);
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.health = (a.health + n).clamp(0, max.max(a.health));
                    }
                }
            }
            Some("addstrength") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    let max = world
                        .actors
                        .get(mo)
                        .and_then(|a| world.classes.get(a.class))
                        .map(|c| c.health.max(100))
                        .unwrap_or(100);
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.health = (a.health + n).clamp(0, max.max(a.health));
                    }
                }
            }
            Some("strength") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0)
                    .max(0);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.health = n;
                    }
                }
            }
            Some("clipdist") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.radius = (f64::from(n) / 16.0).clamp(1.0, 64.0);
                    }
                }
            }
            Some("cactor") if want && !skip => {
                let name = words.next().unwrap_or("");
                if let Some(cid) = find_con_actor(world, name) {
                    if let Some(p) = world.players[0].as_mut() {
                        p.cls = cid;
                        if let Some(mo) = p.mo {
                            if let Some(a) = world.actors.get_mut(mo) {
                                a.class = cid;
                            }
                        }
                    }
                }
            }
            Some("addammo") if want && !skip => {
                let name = words.next().unwrap_or("");
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_addammo(world, name, n);
            }
            Some("addweapon") if want && !skip => {
                let name = words.next().unwrap_or("");
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_addweapon(world, name, n);
            }
            Some("addinventory") if want && !skip => {
                let name = words.next().unwrap_or("");
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(1);
                conevt_addinventory(world, name, n);
            }
            Some("spawn") if want && !skip => {
                let name = words.next().unwrap_or("");
                conevt_spawn(world, name, 1, false);
            }
            Some("guts") | Some("debris") if want && !skip => {
                let name = words.next().unwrap_or("");
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                conevt_spawn(world, name, n, true);
            }
            Some("shoot") if want && !skip => {
                let name = words.next().unwrap_or("");
                conevt_shoot(world, name);
            }
            Some("zshoot") if want && !skip => {
                let name = words.next().unwrap_or("");
                let z = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_shoot_z(world, name, z);
            }
            Some("eshoot") if want && !skip => {
                let name = words.next().unwrap_or("");
                conevt_shoot(world, name);
            }
            Some("ezshoot") if want && !skip => {
                let name = words.next().unwrap_or("");
                let z = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_shoot_z(world, name, z);
            }
            Some("pstomp") if want && !skip => {
                conevt_pstomp(world);
            }
            Some("findplayer") if want && !skip => {
                conevt_findplayer(world);
            }
            Some("debug") | Some("addlog") if want && !skip => {
                world.push_message("DEBUG");
            }
            Some("findotherplayer") if want && !skip => {
                conevt_findotherplayer(world);
            }
            Some("getzrange") if want && !skip => {
                conevt_getzrange(world);
            }
            Some("updatesector") | Some("updatesectorz") if want && !skip => {
                conevt_updatesector(world);
            }
            Some("getflorzofslope") if want && !skip => {
                conevt_getflorzofslope(world);
            }
            Some("getceilzofslope") if want && !skip => {
                conevt_getceilzofslope(world);
            }
            Some("killit") if want && !skip => {
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.health = 0;
                    }
                }
            }
            Some("pkick") if want && !skip => {
                world.player_quick_kick();
            }
            Some("wackplayer") if want && !skip => {
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.health = (a.health - 10).max(0);
                    }
                }
                if let Some(p) = world.players[0].as_mut() {
                    p.damage_count = p.damage_count.max(16);
                }
            }
            Some("operate") if want && !skip => {
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    let _ = room_sim::use_lines(world, mo);
                }
            }
            Some("operateactivators") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let mo = world.players[0].as_ref().and_then(|p| p.mo);
                conevt_operateactivators(world, n, mo);
            }
            Some("operatesectors") if want && !skip => {
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    conevt_operatesectors(world, mo);
                }
            }
            Some("money") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                conevt_spawn(world, "MONEY", n, true);
            }
            Some("mail") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                conevt_spawn(world, "MAIL", n, true);
            }
            Some("paper") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                conevt_spawn(world, "PAPER", n, true);
            }
            Some("tip") if want && !skip => {
                world.push_message("TIP");
                world.play_sound("duke/duke_tip1", x, y, z, 1.0);
            }
            Some("mikesnd") if want && !skip => {
                world.play_sound("duke/duke_use1", x, y, z, 1.0);
            }
            Some("resetplayer") if want && !skip => {
                conevt_resetplayer(world);
            }
            Some("endofgame") if want && !skip => {
                world.level.exiting = true;
            }
            Some("tossweapon") if want && !skip => {
                conevt_tossweapon(world);
            }
            Some("resetweapons") if want && !skip => {
                conevt_resetweapons(world);
            }
            Some("resetinventory") if want && !skip => {
                conevt_resetinventory(world);
            }
            Some("movesprite") if want && !skip => {
                let x = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let y = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let z = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_movesprite(world, x, y, z);
            }
            Some("clipmove") if want && !skip => {
                let x = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let y = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let z = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_clipmove(world, x, y, z);
            }
            Some("changespritestat") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_changespritestat(world, n);
            }
            Some("wackmove") if want && !skip => {
                let x = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let y = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let z = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_wackmove(world, x, y, z);
            }
            Some("respawnhitag") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_respawnhitag(world, n);
            }
            Some("setsprite") if want && !skip => {
                let x = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let y = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let z = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_setsprite(world, x, y, z);
            }
            Some("lockplayer") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_lockplayer(world, n);
            }
            Some("setplayerangle") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_setplayerangle(world, n);
            }
            Some("setactorangle") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_setplayerangle(world, n);
            }
            Some("getplayerangle") if want && !skip => {
                conevt_getplayerangle(world);
            }
            Some("getactorangle") if want && !skip => {
                conevt_getactorangle(world);
            }
            Some("quake") | Some("earthquake") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                world.start_duke_quake(n);
            }
            Some("lotsofglass") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                conevt_spawn(world, "GLASSPIECES", n, true);
            }
            Some("lotsofcolour") | Some("lotsofcolor") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(1)
                    .clamp(0, 16);
                conevt_lotsofcolour(world, n);
            }
            Some("spritepal") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        apply_duke_spritepal(a, n);
                    }
                }
            }
            Some("getlastpal") if want && !skip => {
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        let n = a.user_vars.get(USER_LASTPAL).copied().unwrap_or(0);
                        apply_duke_spritepal(a, n);
                    }
                }
            }
            Some("sizeat") if want && !skip => {
                let x = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(64);
                let y = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(64);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.scale_x = f64::from(x) / 64.0;
                        a.scale_y = f64::from(y) / 64.0;
                    }
                }
            }
            Some("sizeto") if want && !skip => {
                let x = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(64);
                let y = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(64);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.scale_x = step_duke_repeat(a.scale_x, x);
                        a.scale_y = step_duke_repeat(a.scale_y, y);
                    }
                }
            }
            Some("count") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0)
                    .max(0);
                set_player_con_count(world, n);
            }
            Some("resetcount") if want && !skip => {
                set_player_con_count(world, 0);
            }
            Some("actioncount") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0)
                    .max(0);
                set_player_con_action_count(world, n);
            }
            Some("resetactioncount") if want && !skip => {
                set_player_con_action_count(world, 0);
            }
            Some("addkills") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let add = u32::try_from(n.max(0)).unwrap_or(0);
                world.level.found_kills = world.level.found_kills.saturating_add(add);
            }
            Some("hitradius") if want && !skip => {
                let r = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                let d = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                conevt_hitradius(world, r, d);
            }
            Some("fall") if want && !skip => {
                conevt_fall(world);
            }
            Some("sleeptime") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0)
                    .max(0);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        a.reaction_time = n;
                    }
                }
            }
            Some("cstat") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        apply_duke_cstat(a, n);
                    }
                }
            }
            Some("cstator") if want && !skip => {
                let n = words
                    .next()
                    .and_then(|w| w.parse::<i32>().ok())
                    .unwrap_or(0);
                if let Some(mo) = world.players[0].as_ref().and_then(|p| p.mo) {
                    if let Some(a) = world.actors.get_mut(mo) {
                        apply_duke_cstator(a, n);
                    }
                }
            }
            Some("end") => {
                want = false;
                skip = false;
            }
            _ => {}
        }
    }
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
    fn duke_sav_randomseed_becomes_the_room_rng() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            rng_seed: 0x1357_9bdf,
            ..GameOptions::default()
        });
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert_eq!(game.world().unwrap().rng.seed(), 0x1357_9bdf);
        assert_eq!(game.options.rng_seed, 0x1357_9bdf);
    }

    #[test]
    fn duke_sav_lockclock_becomes_map_time() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().level.time = 105;
        assert_eq!(game.world().unwrap().level.time, 105);
    }

    #[test]
    fn duke_sav_earthquaketime_becomes_a_quake() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().start_duke_quake(35);
        let w = game.world().unwrap();
        assert_eq!(w.quakes.len(), 1);
        assert_eq!(w.quakes[0].countdown, 35);
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert!(w.quake_intensity_at(mo) > 0.0);
    }

    #[test]
    fn duke_sav_respawn_monsters_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(!game.world().unwrap().respawn_monsters);
        game.world_mut().unwrap().respawn_monsters = true;
        assert!(game.world().unwrap().respawn_monsters);
    }

    #[test]
    fn duke_sav_respawn_items_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(!game.world().unwrap().respawn_items);
        assert!(!game.world().unwrap().respawn_inventory);
        game.world_mut().unwrap().respawn_items = true;
        game.world_mut().unwrap().respawn_inventory = true;
        assert!(game.world().unwrap().respawn_items);
        assert!(game.world().unwrap().respawn_inventory);
    }

    #[test]
    fn duke_sav_monsters_off_clears_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 20
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .thing(9001, 96.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let n = game.world_mut().unwrap().kill_monsters();
        assert!(n >= 1, "monsters_off kills COUNTKILL ({n})");
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let alive = game.world().unwrap().thinkers.default.iter().any(|&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == dummy && a.health > 0 && !a.destroyed)
        });
        assert!(!alive, "no living COUNTKILL after monsters_off");
    }

    #[test]
    fn duke_sav_secretlevel_eog_and_global_random() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        {
            let w = game.world_mut().unwrap();
            assert!(!w.from_bonus);
            assert_eq!(w.secretlevel, 0);
            assert!(!w.eog);
            assert_eq!(w.global_random, 0);
            w.from_bonus = true;
            w.secretlevel = 7;
            w.eog = true;
            w.global_random = 1234;
        }
        let w = game.world().unwrap();
        assert!(w.from_bonus);
        assert_eq!(w.secretlevel, 7);
        assert!(w.eog);
        assert_eq!(w.global_random, 1234);
    }

    #[test]
    fn duke_sav_coop_ffire_psky_and_camera() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        {
            let w = game.world_mut().unwrap();
            assert!(!w.coop);
            assert!(!w.friendly_fire);
            assert_eq!(w.camsprite, -1);
            assert_eq!(w.pskybits, 0);
            w.coop = true;
            w.friendly_fire = true;
            w.marker = true;
            w.last_level = 2;
            w.camsprite = 5;
            w.set_demo_cameras(true);
            w.pskybits = 3;
            w.pskyoff = [0, 2, 3, 0, 2, 0, 1, 0];
            w.parallaxyscale = 65536;
        }
        let w = game.world().unwrap();
        assert!(w.coop && w.friendly_fire && w.marker && w.demo_cameras);
        assert_eq!(w.last_level, 2);
        assert_eq!(w.camsprite, 5);
        assert_eq!(w.pskybits, 3);
        assert_eq!(w.pskyoff[1], 2);
        assert_eq!(w.parallaxyscale, 65536);
    }

    #[test]
    fn duke_sav_frags_connect_and_animate() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        {
            let w = game.world_mut().unwrap();
            assert_eq!(w.connecthead, -1);
            assert_eq!(w.animatecnt, 0);
            w.connecthead = 0;
            w.connectpoint2[0] = -1;
            w.numplayersprites = 1;
            w.duke_frags[0][1] = 3;
            w.animatecnt = 4;
            w.animategoal = vec![100, 200, 300, 400];
            w.numanimwalls = 5;
            w.spriteqamount = 7;
            w.numclouds = 3;
            w.numcyclers = 2;
            if let Some(p) = w.players[0].as_mut() {
                p.frags[1] = 3;
            }
        }
        let w = game.world().unwrap();
        assert_eq!(w.connecthead, 0);
        assert_eq!(w.numplayersprites, 1);
        assert_eq!(w.duke_frags[0][1], 3);
        assert_eq!(w.players[0].as_ref().unwrap().frags[1], 3);
        assert_eq!(w.animatecnt, 4);
        assert_eq!(w.animategoal, vec![100, 200, 300, 400]);
        assert_eq!(w.numanimwalls, 5);
        assert_eq!(w.spriteqamount, 7);
        assert_eq!(w.numclouds, 3);
        assert_eq!(w.numcyclers, 2);
    }

    #[test]
    fn duke_sav_gotpic_and_playerswhenstarted() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        {
            let w = game.world_mut().unwrap();
            assert!(w.gotpic.is_empty());
            assert_eq!(w.playerswhenstarted, 1);
            w.apply_duke_gotpic(&[1, 0, 0]);
            w.playerswhenstarted = 2;
        }
        let w = game.world().unwrap();
        assert_eq!(w.gotpic[0], 1);
        assert_eq!(w.playerswhenstarted, 2);
        assert!(w.players[0].as_ref().unwrap().allmap);
    }

    #[test]
    fn duke_sav_god_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(!game.world().unwrap().players[0].as_ref().unwrap().god);
        game.world_mut().unwrap().players[0].as_mut().unwrap().god = true;
        assert!(game.world().unwrap().players[0].as_ref().unwrap().god);
    }

    #[test]
    fn duke_sav_player_pose_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut()
            .unwrap()
            .set_player_pose(100.0, 200.0, 32.0, 270.0, 40.0);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let a = w.actors.get(mo).unwrap();
        assert!((a.pos.x() - 100.0).abs() < 1e-9);
        assert!((a.pos.y() - 200.0).abs() < 1e-9);
        assert!((a.pos.z() - 32.0).abs() < 1e-9);
        assert!((a.angle.degrees() - 270.0).abs() < 1e-9);
        assert!((a.pitch.degrees() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn duke_sav_player_health_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_health(75);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(w.actors.get(mo).unwrap().health, 75);
    }

    #[test]
    fn duke_sav_player_clip_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Inventory.Amount 1
  Inventory.MaxAmount 200
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_clip_ammo(48);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let clip = w.classes.find_name("Clip").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, clip), 48);
    }

    #[test]
    fn duke_sav_player_shell_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shell : Actor 2008
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_shell_ammo(10);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let shell = w.classes.find_name("Shell").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, shell), 10);
    }

    #[test]
    fn duke_sav_player_rocket_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor RocketAmmo : Actor 2010
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_rocket_ammo(5);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let rocket = w.classes.find_name("RocketAmmo").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, rocket), 5);
    }

    #[test]
    fn duke_sav_player_cell_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Cell : Actor 2047
{
  Inventory.Amount 1
  Inventory.MaxAmount 300
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_cell_ammo(20);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let cell = w.classes.find_name("Cell").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, cell), 20);
    }

    #[test]
    fn duke_sav_player_pipebomb_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukePipebombAmmo : Actor 2048
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_pipebomb_ammo(9);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let ammo = w.classes.find_name("DukePipebombAmmo").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, ammo), 9);
    }

    #[test]
    fn duke_sav_player_devastator_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeDevastatorAmmo : Actor 2049
{
  Inventory.Amount 1
  Inventory.MaxAmount 99
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_devastator_ammo(15);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let ammo = w.classes.find_name("DukeDevastatorAmmo").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, ammo), 15);
    }

    #[test]
    fn duke_sav_player_freeze_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeFreezeAmmo : Actor 2050
{
  Inventory.Amount 1
  Inventory.MaxAmount 99
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_freeze_ammo(25);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let ammo = w.classes.find_name("DukeFreezeAmmo").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, ammo), 25);
    }

    #[test]
    fn duke_sav_player_tripbomb_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeTripbombAmmo : Actor 2051
{
  Inventory.Amount 1
  Inventory.MaxAmount 10
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_tripbomb_ammo(6);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let ammo = w.classes.find_name("DukeTripbombAmmo").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, ammo), 6);
    }

    #[test]
    fn duke_sav_player_grow_ammo_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeGrowAmmo : Actor 2052
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_grow_ammo(12);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let ammo = w.classes.find_name("DukeGrowAmmo").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, ammo), 12);
    }

    #[test]
    fn duke_sav_player_weapon_slot_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_weapon_slot(2);
        let w = game.world().unwrap();
        let gun = w.classes.find_name("Shotgun").unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, gun), 1);
        assert_eq!(
            w.players[0].as_ref().and_then(|p| p.ready_weapon),
            Some(gun)
        );
    }

    #[test]
    fn duke_sav_player_last_full_weapon_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Pistol : Actor 0
{
  Weapon.SlotNumber 2
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let before = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.ready_weapon);
        game.world_mut().unwrap().give_player_weapon_slot(1);
        let w = game.world().unwrap();
        let gun = w.classes.find_name("Pistol").unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, gun), 1);
        assert_eq!(
            w.players[0].as_ref().and_then(|p| p.ready_weapon),
            before,
            "last_full_weapon should not change the ready gun"
        );
    }

    #[test]
    fn duke_sav_player_last_weapon_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let before = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.ready_weapon);
        game.world_mut().unwrap().give_player_weapon_slot(2);
        let w = game.world().unwrap();
        let gun = w.classes.find_name("Shotgun").unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, gun), 1);
        assert_eq!(
            w.players[0].as_ref().and_then(|p| p.ready_weapon),
            before,
            "last_weapon should not change the ready gun"
        );
    }

    #[test]
    fn duke_sav_player_gotweapon_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mut got = [0u8; 12];
        got[2] = 1;
        game.world_mut().unwrap().give_player_gotweapons(&got);
        let w = game.world().unwrap();
        let gun = w.classes.find_name("Shotgun").unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, gun), 1);
    }

    #[test]
    fn duke_sav_player_armor_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_armor(50);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert_eq!(p.armor, 50);
        assert_eq!(p.armor_class, 1);
    }

    #[test]
    fn duke_sav_player_firstaid_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeFirstAid : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 100
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_firstaid(80);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let aid = w.classes.find_name("DukeFirstAid").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, aid), 80);
    }

    #[test]
    fn duke_sav_player_steroids_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeSteroids : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 400
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_steroids(400);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let cid = w.classes.find_name("DukeSteroids").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, cid), 400);
    }

    #[test]
    fn duke_sav_player_jetpack_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeJetpack : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 1600
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_jetpack(600);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let cid = w.classes.find_name("DukeJetpack").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, cid), 600);
    }

    #[test]
    fn duke_sav_player_scuba_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeScubaGear : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 640
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_scuba(640);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let cid = w.classes.find_name("DukeScubaGear").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, cid), 640);
    }

    #[test]
    fn duke_sav_player_holoduke_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeHoloduke : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 2400
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_holoduke(2400);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let cid = w.classes.find_name("DukeHoloduke").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, cid), 2400);
    }

    #[test]
    fn duke_sav_player_boots_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeBoots : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 200
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_boots(200);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let cid = w.classes.find_name("DukeBoots").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, cid), 200);
    }

    #[test]
    fn duke_sav_player_heat_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeNightVision : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 1200
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_heat(1200);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let cid = w.classes.find_name("DukeNightVision").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, cid), 1200);
    }

    #[test]
    fn duke_sav_player_weapon_ang_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .unwrap()
            .angle
            .degrees();
        game.world_mut().unwrap().add_player_yaw(45.0);
        let after = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .unwrap()
            .angle
            .degrees();
        assert!(
            (after - (before + 45.0)).abs() < 1e-6,
            "weapon_ang should add look-aside yaw, before={before} after={after}"
        );
    }

    #[test]
    fn duke_sav_player_access_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor BlueCard : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor RedCard : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor YellowCard : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_got_access(5);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let blue = w.classes.find_name("BlueCard").unwrap();
        let red = w.classes.find_name("RedCard").unwrap();
        let yellow = w.classes.find_name("YellowCard").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, blue), 1);
        assert_eq!(room_sim::count_inventory(w, mo, red), 0);
        assert_eq!(room_sim::count_inventory(w, mo, yellow), 1);
    }

    #[test]
    fn duke_sav_player_holoduke_on_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeHolodukeDecoy : Actor 0
{
  Health 1
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().spawn_player_holoduke_decoy();
        let w = game.world().unwrap();
        let decoy = w.classes.find_name("DukeHolodukeDecoy").unwrap();
        assert!(
            w.thinkers
                .default
                .iter()
                .any(|&id| w.actors.get(id).is_some_and(|a| a.class == decoy)),
            "holoduke_on should spawn a decoy"
        );
    }

    #[test]
    fn duke_sav_player_jetpack_on_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_jetpack_on(true);
        let w = game.world().unwrap();
        assert!(
            w.players[0].as_ref().is_some_and(|p| p.fly_tics > 0),
            "jetpack_on should start flight"
        );
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_ne!(
            w.actors.get(mo).unwrap().flags & room_sim::FLAG_NOGRAVITY,
            0
        );
    }

    #[test]
    fn duke_sav_player_scuba_on_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_scuba_on(true);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert!(p.radsuit, "scuba_on should start the radsuit");
        assert!(p.radsuit_tics > 0);
        assert!(p.air_finished > 0);
    }

    #[test]
    fn duke_sav_player_walking_snd_toggle_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut()
            .unwrap()
            .set_player_walking_snd_toggle(true);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.walk_snd_toggle),
            Some(true)
        );
    }

    #[test]
    fn duke_sav_player_palookup_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_palookup(21);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert_eq!(
            w.actors.get(mo).map(|a| a.translation.as_str()),
            Some("DukePal21")
        );
    }

    #[test]
    fn duke_sav_player_pals_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_pals_flash(35);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.damage_count),
            Some(35)
        );
    }

    #[test]
    fn duke_sav_player_rapid_fire_hold_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_rapid_fire_hold(true);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert!(p.attack_down);
        assert!(p.refire >= 1);
    }

    #[test]
    fn duke_sav_player_show_empty_weapon_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Pistol : Actor 0
{
  States
  {
  Ready:
    PISG A 1
    Loop
  Fire:
    PISG B 4
    Goto Ready
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let gun = game.world().unwrap().classes.find_name("Pistol").unwrap();
        {
            let w = game.world_mut().unwrap();
            let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
            room_sim::give_inventory(w, mo, gun, 1);
            if let Some(p) = w.players[0].as_mut() {
                p.ready_weapon = Some(gun);
            }
        }
        game.world_mut().unwrap().set_player_show_empty_weapon(true);
        let w = game.world().unwrap();
        let p = w.players[0].as_ref().unwrap();
        let fire = w.states.find_label(gun, "Fire").unwrap();
        assert_eq!(
            p.pspr_state,
            Some(fire),
            "show_empty_weapon should enter Fire"
        );
        assert!(p.attack_down);
    }

    #[test]
    fn duke_sav_player_tipincs_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.world_mut().unwrap().set_player_tip_incs(35);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.tip_incs),
            Some(35)
        );
        let sounds: Vec<_> = game
            .world_mut()
            .unwrap()
            .drain_sound_events()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(
            sounds.iter().any(|n| n == "duke/duke_tip1"),
            "tipincs should play the tip, got {sounds:?}"
        );
    }

    #[test]
    fn duke_sav_player_return_to_center_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_return_to_center(35);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.return_to_center),
            Some(35)
        );
    }

    #[test]
    fn duke_sav_player_knuckle_incs_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.world_mut().unwrap().set_player_knuckle_incs(true);
        let sounds: Vec<_> = game
            .world_mut()
            .unwrap()
            .drain_sound_events()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(
            sounds.iter().any(|n| n == "duke/crack"),
            "knuckle_incs should play the crack, got {sounds:?}"
        );
    }

    #[test]
    fn duke_sav_player_jumping_toggle_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_jumping_toggle(true);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.jump_down),
            Some(true)
        );
    }

    #[test]
    fn duke_sav_player_rotscrnang_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_rotscrnang(256);
        let w = game.world().unwrap();
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.rotscrnang, 256);
        let mo = p.mo.unwrap();
        let roll = w.actors.get(mo).unwrap().roll.degrees();
        assert!((roll - (-45.0)).abs() < 1e-6, "roll={roll}");
    }

    #[test]
    fn duke_sav_player_hbomb_hold_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeThrowPower : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 6
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_hbomb_hold(4);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let power = w.classes.find_name("DukeThrowPower").unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, power), 4);
        assert!(w.players[0].as_ref().unwrap().attack_down);
    }

    #[test]
    fn duke_sav_player_hurt_delay_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_hurt_delay(35);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert_eq!(p.hurt_delay_tics, 35);
        assert!(p.damage_count >= 35);
    }

    #[test]
    fn duke_sav_player_newowner_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let cls = game.world().unwrap().players[0].as_ref().unwrap().cls;
        let id = game
            .world_mut()
            .unwrap()
            .spawn_at(
                cls,
                room_core::DVec3::new(80.0, 80.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        game.world_mut()
            .unwrap()
            .look_through_duke_sprite_at(80.0, 80.0);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert_eq!(p.camera, Some(id));
        assert!(p.camera_revert);
    }

    #[test]
    fn duke_sav_player_pycount_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_pycount(512);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .and_then(|p| p.pycount),
            Some(512)
        );
    }

    #[test]
    fn duke_sav_player_hard_landing_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_hard_landing(8);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.hard_landing),
            Some(8)
        );
    }

    #[test]
    fn duke_sav_player_access_spritenum_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor CardSwitch : Actor 0
{
  +USESPECIAL
  +SOLID
  Radius 16
  Height 32
  States
  {
  Spawn:
    SWT1 A -1
    Stop
  Use:
    SWT1 B -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let sw = game
            .world()
            .unwrap()
            .classes
            .find_name("CardSwitch")
            .unwrap();
        let use_st = game.world().unwrap().states.find_label(sw, "Use").unwrap();
        let id = game
            .world_mut()
            .unwrap()
            .spawn_at(
                sw,
                room_core::DVec3::new(80.0, 80.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        game.world_mut().unwrap().use_duke_sprite_at(80.0, 80.0);
        assert_eq!(
            game.world().unwrap().actors.get(id).and_then(|a| a.state),
            Some(use_st),
            "access_spritenum should use the sprite"
        );
    }

    #[test]
    fn duke_sav_player_access_wallnum_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(1);
        map.lines[0].special = 12;
        map.lines[0].args = [1, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        let v = map.vertices[map.lines[0].v1.raw() as usize].pos;
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.world().unwrap().doors.is_empty());
        game.world_mut().unwrap().use_duke_wall_at(v.x(), v.y());
        assert!(
            !game.world().unwrap().doors.is_empty(),
            "access_wallnum should use the wall"
        );
    }

    #[test]
    fn duke_sav_player_fta_quote_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_fta_quote(35, 61);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert_eq!(p.fta_tics, 35);
        assert_eq!(p.ftq, 61);
    }

    #[test]
    fn duke_sav_player_access_incs_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(1);
        map.lines[0].special = 12;
        map.lines[0].args = [1, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        let v1 = map.vertices[map.lines[0].v1.raw() as usize].pos;
        let v2 = map.vertices[map.lines[0].v2.raw() as usize].pos;
        let mid = room_core::DVec2::new((v1.x() + v2.x()) * 0.5, (v1.y() + v2.y()) * 0.5);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            let dx = v2.x() - v1.x();
            let dy = v2.y() - v1.y();
            let len = room_core::dmath::sqrt(dx * dx + dy * dy).max(1.0);
            let mut nx = -dy / len;
            let mut ny = dx / len;
            let mut px = mid.x() + nx * 32.0;
            let mut py = mid.y() + ny * 32.0;
            if !(0.0..=256.0).contains(&px) || !(0.0..=256.0).contains(&py) {
                nx = -nx;
                ny = -ny;
                px = mid.x() + nx * 32.0;
                py = mid.y() + ny * 32.0;
            }
            a.pos = room_core::DVec3::new(px, py, a.pos.z());
            a.angle = room_core::Angle::from_radians(room_core::dmath::atan2(-ny, -nx));
        }
        assert!(game.world().unwrap().doors.is_empty());
        game.world_mut().unwrap().set_player_access_incs(true);
        assert!(
            !game.world().unwrap().doors.is_empty(),
            "access_incs should use the door"
        );
    }

    #[test]
    fn duke_sav_player_knee_incs_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Fist : Actor 0
{
  States
  {
  Ready:
    PUNG A 1
    Loop
  Fire:
    PUNG B 4
    Goto Ready
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_knee_incs(8);
        let w = game.world().unwrap();
        let kick = w.classes.find_name("Fist").unwrap();
        let fire = w.states.find_label(kick, "Fire").unwrap();
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.ready_weapon, Some(kick));
        assert_eq!(p.pspr_state, Some(fire), "knee_incs should enter Fire");
    }

    #[test]
    fn duke_sav_player_frag_ps_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0].as_ref().and_then(|p| p.mo);
        game.world_mut().unwrap().set_player_frag_ps(0);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .and_then(|p| p.attacker),
            mo
        );
    }

    #[test]
    fn duke_sav_player_quick_kick_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Fist : Actor 0
{
  States
  {
  Ready:
    PUNG A 1
    Loop
  Fire:
    PUNG B 4
    Goto Ready
  }
}
actor Pistol : Actor 0
{
  States
  {
  Ready:
    PISG A 1
    Loop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let kick = game.world().unwrap().classes.find_name("Fist").unwrap();
        let gun = game.world().unwrap().classes.find_name("Pistol").unwrap();
        {
            let w = game.world_mut().unwrap();
            let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
            room_sim::give_inventory(w, mo, kick, 1);
            room_sim::give_inventory(w, mo, gun, 1);
            if let Some(p) = w.players[0].as_mut() {
                p.ready_weapon = Some(gun);
            }
        }
        game.world_mut().unwrap().player_quick_kick();
        let w = game.world().unwrap();
        let fire = w.states.find_label(kick, "Fire").unwrap();
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.ready_weapon, Some(kick));
        assert_eq!(p.pspr_state, Some(fire));
        assert_eq!(p.quick_kick_restore, Some(gun));
    }

    #[test]
    fn duke_sav_player_weapon_pos_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_pspr_y(80.0);
        assert_eq!(
            game.world().unwrap().players[0].as_ref().map(|p| p.pspr_y),
            Some(80.0)
        );
    }

    #[test]
    fn duke_sav_player_hbomb_on_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeBombsOut : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_hbomb_on(true);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let bomb = w.classes.find_name("DukeBombsOut").unwrap();
        assert!(
            room_sim::count_inventory(w, mo, bomb) > 0,
            "hbomb_on should give DukeBombsOut"
        );
    }

    #[test]
    fn duke_sav_player_transporter_hold_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_transporter_hold(35);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.transporter_hold_tics),
            Some(35)
        );
    }

    #[test]
    fn duke_sav_player_on_ground_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_on_ground(false);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.on_ground),
            Some(false)
        );
    }

    #[test]
    fn duke_sav_player_invdisp_prints_steroids() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_inven_icon(2);
        game.world_mut().unwrap().set_player_invdisp_tics(35);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("STEROIDS")), "{msgs:?}");
    }

    #[test]
    fn duke_sav_player_refresh_inventory_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_invdisp_tics(35);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.invdisp_tics),
            Some(35)
        );
    }

    #[test]
    fn duke_sav_player_toggle_key_flag_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_toggle_key_flag(true);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert!(p.toggle_key_flag);
        assert!(p.invuse_down);
    }

    #[test]
    fn duke_sav_player_footprintpal_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_footprint_pal(21);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.footprint_pal),
            Some(Some(21))
        );
    }

    #[test]
    fn duke_sav_player_footprintcount_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_footprint_count(7);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.footprint_count),
            Some(Some(7))
        );
    }

    #[test]
    fn duke_sav_player_footprintshade_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_footprint_shade(12);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.footprint_shade),
            Some(Some(12))
        );
    }

    #[test]
    fn duke_sav_player_ohoriz_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_prev_pitch(20);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.prev_pitch),
            Some(Some(20))
        );
    }

    #[test]
    fn duke_sav_player_invdisptime_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_invdisp_tics(35);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.invdisp_tics),
            Some(35)
        );
    }

    #[test]
    fn duke_sav_player_inven_icon_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_inven_icon(2);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.inven_icon),
            Some(2)
        );
    }

    #[test]
    fn duke_sav_player_dead_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_dead(true);
        let w = game.world().unwrap();
        let p = w.players[0].as_ref().unwrap();
        assert_eq!(p.player_state, room_sim::PlayerState::Dead);
        let mo = p.mo.unwrap();
        assert_eq!(w.actors.get(mo).map(|a| a.health), Some(0));
    }

    #[test]
    fn duke_sav_player_jumping_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_jumping(true);
        let w = game.world().unwrap();
        let p = w.players[0].as_ref().unwrap();
        assert!(!p.on_ground, "jumping_counter should leave the ground");
        let mo = p.mo.unwrap();
        assert!(
            w.actors.get(mo).is_some_and(|a| a.vel.z() > 0.0),
            "jumping_counter should apply JumpZ"
        );
    }

    #[test]
    fn duke_sav_player_airleft_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let t = i32::try_from(game.world().unwrap().level.time).unwrap_or(0);
        game.world_mut().unwrap().set_player_air_tics(35);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.air_finished),
            Some(t.saturating_add(35)),
            "airleft should set remaining air"
        );
    }

    #[test]
    fn duke_sav_player_heat_on_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_heat_on(true);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert!(p.infrared, "heat_on should start night vision");
        assert!(p.infrared_tics > 0);
    }

    #[test]
    fn duke_sav_player_falling_tics_are_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_falling_tics(35);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.falling_tics),
            Some(35)
        );
    }

    #[test]
    fn duke_sav_player_kickback_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Pistol : Actor 0
{
  States
  {
  Ready:
    PISG A 1
    Loop
  Fire:
    PISG B 4
    Goto Ready
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let gun = game.world().unwrap().classes.find_name("Pistol").unwrap();
        {
            let w = game.world_mut().unwrap();
            let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
            room_sim::give_inventory(w, mo, gun, 1);
            if let Some(p) = w.players[0].as_mut() {
                p.ready_weapon = Some(gun);
            }
        }
        game.world_mut().unwrap().set_player_kickback(15);
        let w = game.world().unwrap();
        let p = w.players[0].as_ref().unwrap();
        let fire = w.states.find_label(gun, "Fire").unwrap();
        assert_eq!(p.pspr_state, Some(fire), "kickback_pic should enter Fire");
        assert!(p.attack_down);
        assert_eq!(p.refire, 15);
    }

    #[test]
    fn duke_sav_player_wantweaponfire_is_applied() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Pistol : Actor 0
{
  States
  {
  Ready:
    PISG A 1
    Loop
  Fire:
    PISG B 4
    Goto Ready
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_wantweapon_fire(1);
        let w = game.world().unwrap();
        let gun = w.classes.find_name("Pistol").unwrap();
        let p = w.players[0].as_ref().unwrap();
        let fire = w.states.find_label(gun, "Fire").unwrap();
        assert_eq!(p.ready_weapon, Some(gun));
        assert_eq!(p.pspr_state, Some(fire), "wantweaponfire should enter Fire");
        assert!(p.attack_down);
    }

    #[test]
    fn duke_sav_player_holster_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().set_player_holster(true);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert!(
            p.ready_weapon.is_none(),
            "holster_weapon should put the gun away"
        );
        assert!(p.pending_weapon.is_none());
        assert!(p.pspr_state.is_none());
    }

    #[test]
    fn duke_sav_sector_zs_are_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sector_zs(&[(16.0, 80.0)]);
        let s = &game.world().unwrap().sectors[0];
        assert!((s.floor_z - 16.0).abs() < 1e-9);
        assert!((s.ceiling_z - 80.0).abs() < 1e-9);
        assert!((s.prev_floor_z - 16.0).abs() < 1e-9);
        assert!((s.prev_ceiling_z - 80.0).abs() < 1e-9);
    }

    #[test]
    fn duke_sav_sector_pics_are_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sector_pics(&[("BT0003".into(), "BT0002".into())]);
        let w = game.world().unwrap();
        assert_eq!(
            w.sector_floor_pics.get(&0).map(String::as_str),
            Some("BT0003")
        );
        assert_eq!(
            w.sector_ceiling_pics.get(&0).map(String::as_str),
            Some("BT0002")
        );
    }

    #[test]
    fn duke_sav_sector_lights_are_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().apply_duke_sector_lights(&[135]);
        assert_eq!(game.world().unwrap().sectors[0].light, 135);
    }

    #[test]
    fn duke_sav_sector_fogs_are_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut().unwrap().apply_duke_sector_fogs(&[32]);
        assert_eq!(game.world().unwrap().sectors[0].fog_density, 32);
    }

    #[test]
    fn duke_sav_sprite_health_is_applied() {
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
        assert!(game.cheat_summon(0, "ZombieMan"));
        let zombie = classes.find_name("ZombieMan").unwrap();
        let (id, x, y) = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .and_then(|a| (a.class == zombie).then_some((id, a.pos.x(), a.pos.y())))
            })
            .unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sprite_health(&[(x, y, 17)]);
        assert_eq!(game.world().unwrap().actors.get(id).unwrap().health, 17);
    }

    #[test]
    fn duke_sav_sprite_xy_is_applied() {
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
        assert!(game.cheat_summon(0, "ZombieMan"));
        let zombie = classes.find_name("ZombieMan").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == zombie)
            })
            .unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sprite_xy(&[(80.0, 90.0)]);
        let a = game.world().unwrap().actors.get(id).unwrap();
        assert!((a.pos.x() - 80.0).abs() < 1e-9);
        assert!((a.pos.y() - 90.0).abs() < 1e-9);
    }

    #[test]
    fn duke_sav_sprite_cstat_is_applied() {
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
        assert!(game.cheat_summon(0, "ZombieMan"));
        let zombie = classes.find_name("ZombieMan").unwrap();
        let (id, x, y) = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .and_then(|a| (a.class == zombie).then_some((id, a.pos.x(), a.pos.y())))
            })
            .unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sprite_cstat(&[(x, y, 32_768 | 2)]);
        let a = game.world().unwrap().actors.get(id).unwrap();
        assert_eq!(a.flags & room_sim::FLAG_SOLID, 0);
        assert_eq!(a.flags & room_sim::FLAG_DONTDRAW, room_sim::FLAG_DONTDRAW);
        assert!((a.alpha - 0.66).abs() < 1e-9);
    }

    #[test]
    fn duke_sav_sprite_scale_is_applied() {
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
        assert!(game.cheat_summon(0, "ZombieMan"));
        let zombie = classes.find_name("ZombieMan").unwrap();
        let (id, x, y) = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .and_then(|a| (a.class == zombie).then_some((id, a.pos.x(), a.pos.y())))
            })
            .unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sprite_scale(&[(x, y, 48, 16, 32)]);
        let a = game.world().unwrap().actors.get(id).unwrap();
        assert!((a.scale_x - 0.75).abs() < 1e-9);
        assert!((a.scale_y - 0.25).abs() < 1e-9);
        assert!((a.radius - 2.0).abs() < 1e-9);
    }

    #[test]
    fn duke_sav_sprite_pal_is_applied() {
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
        assert!(game.cheat_summon(0, "ZombieMan"));
        let zombie = classes.find_name("ZombieMan").unwrap();
        let (id, x, y) = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .and_then(|a| (a.class == zombie).then_some((id, a.pos.x(), a.pos.y())))
            })
            .unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sprite_pal(&[(x, y, 21)]);
        assert_eq!(
            game.world().unwrap().actors.get(id).unwrap().translation,
            "DukePal21"
        );
    }

    #[test]
    fn duke_sav_sprite_ang_is_applied() {
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
        assert!(game.cheat_summon(0, "ZombieMan"));
        let zombie = classes.find_name("ZombieMan").unwrap();
        let (id, x, y) = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .and_then(|a| (a.class == zombie).then_some((id, a.pos.x(), a.pos.y())))
            })
            .unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sprite_ang(&[(x, y, 270.0)]);
        assert!(
            (game
                .world()
                .unwrap()
                .actors
                .get(id)
                .unwrap()
                .angle
                .degrees()
                - 270.0)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn duke_sav_sprite_z_is_applied() {
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
        assert!(game.cheat_summon(0, "ZombieMan"));
        let zombie = classes.find_name("ZombieMan").unwrap();
        let (id, x, y) = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .and_then(|a| (a.class == zombie).then_some((id, a.pos.x(), a.pos.y())))
            })
            .unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_sprite_z(&[(x, y, 32.0)]);
        assert!((game.world().unwrap().actors.get(id).unwrap().pos.z() - 32.0).abs() < 1e-9);
    }

    #[test]
    fn duke_sav_wall_health_is_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        if let Some(l) = map.lines.first_mut() {
            l.health = 40;
        }
        let v = map.vertices[map.lines[0].v1.raw() as usize].pos;
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert_eq!(game.world().unwrap().line_health.get(&0), Some(&40));
        game.world_mut()
            .unwrap()
            .apply_duke_wall_health(&[(v.x(), v.y(), 0)]);
        assert_eq!(game.world().unwrap().line_health.get(&0), Some(&0));
    }

    #[test]
    fn duke_sav_wall_pics_are_applied() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        if let Some(l) = map.lines.first_mut() {
            l.health = 40;
        }
        let v = map.vertices[map.lines[0].v1.raw() as usize].pos;
        let mut game = Headless::new(GameOptions::default());
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.world_mut()
            .unwrap()
            .apply_duke_wall_pics(&[(v.x(), v.y(), "-".into())]);
        let w = game.world().unwrap();
        let side = w.map.lines[0].sides[0].unwrap().raw();
        assert_eq!(w.switch_mids.get(&side).map(String::as_str), Some("-"));
        assert_eq!(w.map.sides[side as usize].middle, "-");
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
        game.run_tics(1);
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
    fn conevt_enterlevel_prints() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nprint WELCOME TO L.A.\nsound duke/duke_use\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(
            msgs.iter().any(|m| m.contains("WELCOME TO L.A.")),
            "{msgs:?}"
        );
    }

    #[test]
    fn conevt_egs_prints_on_exit() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event EGS\nprint THAT'S ALL FOLKS\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.world_mut().unwrap().level.exiting = true;
        game.tick(&[]);
        assert_eq!(game.state, GameState::Intermission);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(
            msgs.iter().any(|m| m.contains("THAT'S ALL FOLKS")),
            "{msgs:?}"
        );
    }

    #[test]
    fn conevt_getshot_prints_on_damage() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event GETSHOT\nprint OUCH\nend\nevent KILLIT\nprint YOU'RE DEAD\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = a.health.saturating_sub(10).max(1);
        }
        game.tick(&[]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("OUCH")), "{msgs:?}");
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 0;
        }
        game.tick(&[]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("YOU'RE DEAD")), "{msgs:?}");
    }

    #[test]
    fn conevt_use_and_fire_print_on_edge() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nprint OPEN UP\nend\nevent FIRE\nprint LET'S ROCK\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("OPEN UP")), "{msgs:?}");
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let held = game.world_mut().unwrap().drain_messages();
        assert!(
            !held.iter().any(|m| m.contains("OPEN UP")),
            "use is edge-triggered {held:?}"
        );
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::ATTACK),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("LET'S ROCK")), "{msgs:?}");
    }

    #[test]
    fn conevt_changeweapon_and_holster_print() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event CHANGEWEAPON\nprint SWITCH\nend\nevent HOLSTER\nprint DOWN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            weapon_select: 2,
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("SWITCH")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("DOWN")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifrnd_zero_never_prints() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifrand 0\nprint SECRET\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("SECRET")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifrnd_always_prints() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifrand 256\nprint ALWAYS\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("ALWAYS")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifhealthl_prints_when_hurt_enough() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event GETSHOT\nifhealthl 50\nprint I'M HURT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 80;
        }
        game.tick(&[]);
        let high = game.world_mut().unwrap().drain_messages();
        assert!(!high.iter().any(|m| m.contains("I'M HURT")), "{high:?}");
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[]);
        let low = game.world_mut().unwrap().drain_messages();
        assert!(low.iter().any(|m| m.contains("I'M HURT")), "{low:?}");
    }

    #[test]
    fn conevt_ifinwater_prints_when_submerged() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].waterzone = true;
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifinwater\nprint GULP\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("GULP")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifinwater_stays_quiet_on_dry_land() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifinwater\nprint GULP\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("GULP")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifmultiplayer_prints_in_dukematch() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            deathmatch: true,
            ..Default::default()
        });
        game.set_conevt("event ENTERLEVEL\nifmultiplayer\nprint FRAG FEST\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("FRAG FEST")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifdead_prints_on_killit() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event KILLIT\nifdead\nprint YOU'RE DEAD\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 0;
        }
        game.tick(&[]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("YOU'RE DEAD")), "{msgs:?}");
    }

    #[test]
    fn conevt_else_prints_when_ifdead_fails() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifdead\nprint DEAD\nelse\nprint ALIVE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("ALIVE")), "{msgs:?}");
        assert!(!msgs.iter().any(|m| m.contains("DEAD")), "{msgs:?}");
    }

    #[test]
    fn conevt_else_stays_quiet_when_if_passes() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt(
            "event ENTERLEVEL\nifp palive\nprint STILL KICKING\nelse\nprint DEAD\nend\n",
        );
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("STILL KICKING")), "{msgs:?}");
        assert!(!msgs.iter().any(|m| m.contains("DEAD")), "{msgs:?}");
    }

    #[test]
    fn conevt_break_stops_the_rest_of_the_event() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nprint HI\nbreak\nprint BYE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("HI")), "{msgs:?}");
        assert!(!msgs.iter().any(|m| m.contains("BYE")), "{msgs:?}");
    }

    #[test]
    fn conevt_palfrom_sets_damage_count() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\npalfrom 32\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let flash = game.world().unwrap().players[0]
            .as_ref()
            .map(|p| p.damage_count)
            .unwrap_or(0);
        assert_eq!(flash, 32);
    }

    #[test]
    fn conevt_addphealth_heals_the_pawn() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\naddphealth 10\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.health)
            .unwrap_or(0);
        assert_eq!(hp, 30);
    }

    #[test]
    fn conevt_addstrength_heals_the_pawn() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\naddstrength 25\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.health)
            .unwrap_or(0);
        assert_eq!(hp, 45);
    }

    #[test]
    fn conevt_strength_sets_health() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.health)
            .unwrap_or(0);
        assert_eq!(hp, 40);
    }

    #[test]
    fn conevt_clipdist_sets_radius() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nclipdist 128\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let r = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.radius)
            .unwrap_or(0.0);
        assert!((r - 8.0).abs() < 1e-9, "radius={r}");
    }

    #[test]
    fn conevt_cactor_changes_class() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\ncactor Dummy\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.class)
            .unwrap();
        assert_ne!(before, dummy);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.class)
            .unwrap();
        assert_eq!(after, dummy);
    }

    #[test]
    fn conevt_addammo_gives_clip() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Inventory.Amount 1
  Inventory.MaxAmount 200
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\naddammo PISTOL_WEAPON 12\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let clip = game.world().unwrap().classes.find_name("Clip").unwrap();
        let before = room_sim::count_inventory(game.world().unwrap(), mo, clip);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = room_sim::count_inventory(game.world().unwrap(), mo, clip);
        assert_eq!(after, before + 12);
    }

    #[test]
    fn conevt_addweapon_gives_shotgun() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shell : Actor 2008
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
  Weapon.AmmoType "Shell"
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\naddweapon SHOTGUN 8\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        let shell = game.world().unwrap().classes.find_name("Shell").unwrap();
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, gun), 0);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, gun), 1);
        assert_eq!(
            room_sim::count_inventory(game.world().unwrap(), mo, shell),
            8
        );
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .and_then(|p| p.pending_weapon),
            Some(gun)
        );
    }

    #[test]
    fn conevt_addinventory_gives_firstaid() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeFirstAid : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 100
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\naddinventory FIRSTAID 25\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let kit = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeFirstAid")
            .unwrap();
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, kit), 0);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert_eq!(
            room_sim::count_inventory(game.world().unwrap(), mo, kit),
            25
        );
    }

    #[test]
    fn conevt_spawn_drops_an_actor() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nspawn Dummy\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn conevt_guts_throws_debris() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nguts Dummy 3\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        assert_eq!(after, before + 3);
        let thrown = game.world().unwrap().thinkers.default.iter().any(|&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == dummy && a.vel.length_squared() > 0.0)
        });
        assert!(thrown, "guts should throw");
    }

    #[test]
    fn conevt_debris_throws_pieces() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\ndebris Dummy 3\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        assert_eq!(after, before + 3);
        let thrown = game.world().unwrap().thinkers.default.iter().any(|&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == dummy && a.vel.length_squared() > 0.0)
        });
        assert!(thrown, "debris should throw");
    }

    #[test]
    fn conevt_shoot_fires_along_facing() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 1
  Speed 16
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nshoot Dummy\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        assert_eq!(after, before + 1);
        let fired = game.world().unwrap().thinkers.default.iter().any(|&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == dummy && a.vel.length_squared() > 0.0)
        });
        assert!(fired, "shoot should fire along facing");
    }

    #[test]
    fn conevt_killit_drops_health() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nkillit\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        assert!(
            game.world()
                .unwrap()
                .actors
                .get(mo)
                .is_some_and(|a| a.health > 0)
        );
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 0);
    }

    #[test]
    fn conevt_wackplayer_hurts_and_flashes() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nwackplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 50;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 40);
        let flash = game.world().unwrap().players[0]
            .as_ref()
            .map(|p| p.damage_count)
            .unwrap_or(0);
        assert!(
            flash > 0,
            "wackplayer should flash HUD, damage_count={flash}"
        );
    }

    #[test]
    fn conevt_operatesectors_uses_sector_door() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(7);
        map.lines[0].special = 12;
        map.lines[0].args = [7, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\noperatesectors\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.world().unwrap().doors.is_empty());
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(
            !game.world().unwrap().doors.is_empty(),
            "operatesectors should start the sector's tagged door"
        );
    }

    #[test]
    fn conevt_operateactivators_uses_tagged_door() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(7);
        map.lines[0].special = 12;
        map.lines[0].args = [7, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\noperateactivators 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.world().unwrap().doors.is_empty());
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(
            !game.world().unwrap().doors.is_empty(),
            "operateactivators should start the tagged door"
        );
    }

    #[test]
    fn conevt_operate_uses_the_door() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(1);
        map.lines[0].special = 12;
        map.lines[0].args = [1, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        let v1 = map.vertices[map.lines[0].v1.raw() as usize].pos;
        let v2 = map.vertices[map.lines[0].v2.raw() as usize].pos;
        let mid = room_core::DVec2::new((v1.x() + v2.x()) * 0.5, (v1.y() + v2.y()) * 0.5);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event FIRE\noperate\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            let dx = v2.x() - v1.x();
            let dy = v2.y() - v1.y();
            let len = room_core::dmath::sqrt(dx * dx + dy * dy).max(1.0);
            let mut nx = -dy / len;
            let mut ny = dx / len;
            let mut px = mid.x() + nx * 32.0;
            let mut py = mid.y() + ny * 32.0;
            if !(0.0..=256.0).contains(&px) || !(0.0..=256.0).contains(&py) {
                nx = -nx;
                ny = -ny;
                px = mid.x() + nx * 32.0;
                py = mid.y() + ny * 32.0;
            }
            a.pos = room_core::DVec3::new(px, py, a.pos.z());
            a.angle = room_core::Angle::from_radians(room_core::dmath::atan2(-ny, -nx));
        }
        assert!(game.world().unwrap().doors.is_empty());
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::ATTACK),
            ..TicCmd::default()
        }]);
        assert!(
            !game.world().unwrap().doors.is_empty(),
            "operate should raise the door"
        );
    }

    #[test]
    fn conevt_money_drops_bills() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeMoney : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nmoney 4\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let bill = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeMoney")
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == bill)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == bill)
            })
            .count();
        assert_eq!(after, before + 4);
    }

    #[test]
    fn conevt_mail_drops_letters() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeMail : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nmail 4\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let letter = game.world().unwrap().classes.find_name("DukeMail").unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == letter)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == letter)
            })
            .count();
        assert_eq!(after, before + 4);
    }

    #[test]
    fn conevt_paper_drops_sheets() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukePaper : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\npaper 4\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let sheet = game
            .world()
            .unwrap()
            .classes
            .find_name("DukePaper")
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == sheet)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == sheet)
            })
            .count();
        assert_eq!(after, before + 4);
    }

    #[test]
    fn conevt_tip_prints_and_plays() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ntip\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("TIP")), "{msgs:?}");
        let snds = game.world_mut().unwrap().drain_sound_events();
        assert!(snds.iter().any(|s| s.name == "duke/duke_tip1"), "{snds:?}");
    }

    #[test]
    fn conevt_mikesnd_plays_use_chatter() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nmikesnd\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let snds = game.world_mut().unwrap().drain_sound_events();
        assert!(snds.iter().any(|s| s.name == "duke/duke_use1"), "{snds:?}");
    }

    #[test]
    fn conevt_resetplayer_restores_spawn() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nresetplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let (spawn, want_hp) = {
            let w = game.world().unwrap();
            let a = w.actors.get(mo).unwrap();
            let hp = w.classes.get(a.class).map(|c| c.health).unwrap_or(100);
            (a.spawn_point, hp)
        };
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 10;
            a.pos = room_core::DVec3::new(a.pos.x() + 40.0, a.pos.y() + 16.0, a.pos.z() + 20.0);
            a.vel = room_core::DVec3::new(4.0, 0.0, 2.0);
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.health, want_hp.max(1));
        assert!((a.pos.x() - spawn.x()).abs() < 1e-6, "x={}", a.pos.x());
        assert!((a.pos.y() - spawn.y()).abs() < 1e-6, "y={}", a.pos.y());
        assert!(a.vel.length_squared() < 1e-9, "vel={:?}", a.vel);
    }

    #[test]
    fn conevt_endofgame_exits_the_map() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nendofgame\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(!game.world().unwrap().level.exiting);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(game.world().unwrap().level.exiting);
    }

    #[test]
    fn conevt_pkick_starts_quick_kick() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Fist : Actor 0
{
  States
  {
  Ready:
    PUNG A 1
    Loop
  Fire:
    PUNG B 4
    Goto Ready
  }
}
actor Pistol : Actor 0
{
  States
  {
  Ready:
    PISG A 1
    Loop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\npkick\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let fist = game.world().unwrap().classes.find_name("Fist").unwrap();
        let pistol = game.world().unwrap().classes.find_name("Pistol").unwrap();
        {
            let w = game.world_mut().unwrap();
            let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
            room_sim::give_inventory(w, mo, fist, 1);
            room_sim::give_inventory(w, mo, pistol, 1);
            if let Some(p) = w.players[0].as_mut() {
                p.ready_weapon = Some(pistol);
            }
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let p = game.world().unwrap().players[0].as_ref().unwrap();
        assert_eq!(p.ready_weapon, Some(fist), "pkick should switch to Fist");
        assert_eq!(p.quick_kick_restore, Some(pistol));
        let fire = game
            .world()
            .unwrap()
            .states
            .find_label(fist, "Fire")
            .unwrap();
        assert_eq!(p.pspr_state, Some(fire));
    }

    #[test]
    fn conevt_tossweapon_drops_the_ready_gun() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shell : Actor 2008
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
  Weapon.AmmoType "Shell"
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\naddweapon SHOTGUN 8\ntossweapon\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, gun), 0);
        assert!(
            game.world().unwrap().players[0]
                .as_ref()
                .is_some_and(|p| p.ready_weapon.is_none() && p.pending_weapon.is_none())
        );
    }

    #[test]
    fn conevt_resetweapons_keeps_pistol_strips_shotgun() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Inventory.Amount 1
  Inventory.MaxAmount 200
}
actor Shell : Actor 2008
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor Pistol : Actor 0
{
  Weapon.SlotNumber 2
  Weapon.AmmoType "Clip"
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
  Weapon.AmmoType "Shell"
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\naddweapon PISTOL 12\naddweapon SHOTGUN 8\nresetweapons\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let pistol = game.world().unwrap().classes.find_name("Pistol").unwrap();
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let w = game.world().unwrap();
        assert_eq!(room_sim::count_inventory(w, mo, gun), 0);
        assert!(room_sim::count_inventory(w, mo, pistol) > 0);
        assert_eq!(
            w.players[0].as_ref().and_then(|p| p.ready_weapon),
            Some(pistol)
        );
    }

    #[test]
    fn conevt_resetinventory_strips_firstaid() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeFirstAid : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 100
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\naddinventory FIRSTAID 25\nresetinventory\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let aid = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeFirstAid")
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, aid), 0);
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .map(|p| p.inven_icon),
            Some(0)
        );
    }

    #[test]
    fn conevt_movesprite_shifts_the_pawn() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nmovesprite 256 0 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
            .unwrap();
        assert!((after.0 - before.0 - 16.0).abs() < 1e-6);
        assert!((after.1 - before.1).abs() < 1e-6);
        assert!((after.2 - before.2).abs() < 1e-6);
    }

    #[test]
    fn conevt_clipmove_shifts_when_open() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nclipmove 256 0 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert!((a.pos.x() - before.0 - 16.0).abs() < 1e-6);
        assert!((a.pos.y() - before.1).abs() < 1e-6);
        assert_eq!(a.user_vars.get("user_clipmove").copied(), Some(1));
    }

    #[test]
    fn conevt_clipmove_stays_when_blocked() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nclipmove 256 0 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert!((a.pos.x() - before.0).abs() < 1e-6);
        assert!((a.pos.y() - before.1).abs() < 1e-6);
        assert_eq!(a.user_vars.get("user_clipmove").copied(), Some(0));
    }

    #[test]
    fn conevt_changespritestat_stores_stat() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nchangespritestat 1\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.user_vars.get("user_spritestat").copied(), Some(1));
    }

    #[test]
    fn conevt_wackmove_hurts_the_blocker() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nwackmove 256 0 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.user_vars.get("user_wackmove").copied(), Some(0));
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(90));
    }

    #[test]
    fn conevt_respawnhitag_revives_dead_tid() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nrespawnhitag 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(dummy_id) {
            a.tid = 7;
            a.health = 0;
            a.destroyed = true;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let a = game.world().unwrap().actors.get(dummy_id).unwrap();
        assert_eq!(a.health, 100);
        assert!(!a.destroyed);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let p = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(p.user_vars.get("user_respawnhitag").copied(), Some(1));
    }

    #[test]
    fn conevt_setsprite_snaps_the_pawn() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsetsprite 2304 -2048 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let pos = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
            .unwrap();
        assert!((pos.0 - 144.0).abs() < 1e-6);
        assert!((pos.1 - 128.0).abs() < 1e-6);
        assert!(pos.2.abs() < 1e-6);
    }

    #[test]
    fn conevt_lockplayer_freezes_the_pawn() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nlockplayer 30\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(
            !game.world().unwrap().players[0]
                .as_ref()
                .is_some_and(|p| p.frozen)
        );
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(
            game.world().unwrap().players[0]
                .as_ref()
                .is_some_and(|p| p.frozen)
        );
    }

    #[test]
    fn conevt_setplayerangle_sets_yaw() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsetplayerangle 512\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let yaw = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.angle.degrees())
            .unwrap();
        assert!((yaw - 270.0).abs() < 1e-9);
    }

    #[test]
    fn conevt_quake_starts_a_rumble() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nquake 35\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.world().unwrap().quakes.is_empty());
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let w = game.world().unwrap();
        assert_eq!(w.quakes.len(), 1);
        assert_eq!(w.quakes[0].countdown, 35);
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        assert!(w.quake_intensity_at(mo) > 0.0);
    }

    #[test]
    fn conevt_spritesound_plays_at_the_pawn() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nspritesound duke/pistol_fire\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let snds = game.world_mut().unwrap().drain_sound_events();
        assert!(
            snds.iter().any(|s| s.name == "duke/pistol_fire"),
            "{snds:?}"
        );
    }

    #[test]
    fn conevt_soundonce_plays_only_once() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsoundonce duke/pistol_fire\nsoundonce duke/pistol_fire\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let bangs = game
            .world_mut()
            .unwrap()
            .drain_sound_events()
            .into_iter()
            .filter(|e| e.name.contains("pistol_fire"))
            .count();
        assert_eq!(bangs, 1);
    }

    #[test]
    fn conevt_stopsound_drops_the_cue() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsound duke/pistol_fire\nstopsound duke/pistol_fire\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let bangs = game
            .world_mut()
            .unwrap()
            .drain_sound_events()
            .into_iter()
            .filter(|e| e.name.contains("pistol_fire"))
            .count();
        assert_eq!(bangs, 0);
        assert!(
            !game
                .world()
                .unwrap()
                .sound_heard
                .iter()
                .any(|n| n.contains("pistol_fire"))
        );
    }

    #[test]
    fn conevt_setactorangle_sets_yaw() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsetactorangle 1024\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let yaw = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.angle.degrees())
            .unwrap();
        assert!((yaw - 180.0).abs() < 1e-9);
    }

    #[test]
    fn conevt_getplayerangle_stores_build_ang() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsetplayerangle 512\ngetplayerangle\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let ang = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .and_then(|a| a.user_vars.get("user_playerangle").copied());
        assert_eq!(ang, Some(512));
    }

    #[test]
    fn conevt_getactorangle_stores_build_ang() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsetactorangle 1024\ngetactorangle\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let ang = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .and_then(|a| a.user_vars.get("user_actorangle").copied());
        assert_eq!(ang, Some(1024));
    }

    #[test]
    fn conevt_lotsofcolour_throws_tinted_shards() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeGlassPiece : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nlotsofcolour 4\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let shard = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeGlassPiece")
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let tinted = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == shard && a.translation.starts_with("DukePal"))
            })
            .count();
        assert_eq!(tinted, 4);
    }

    #[test]
    fn conevt_zshoot_adds_build_zvel() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 1
  Speed 16
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nzshoot Dummy 256\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let vz = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|a| a.class == dummy)
                    .map(|a| a.vel.z())
            });
        assert!(vz.is_some_and(|z| (z + 1.0).abs() < 1e-6), "{vz:?}");
    }

    #[test]
    fn conevt_eshoot_fires_along_facing() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 1
  Speed 16
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\neshoot Dummy\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .count();
        assert_eq!(after, before + 1);
        let fired = game.world().unwrap().thinkers.default.iter().any(|&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == dummy && a.vel.length_squared() > 0.0)
        });
        assert!(fired, "eshoot should fire along facing");
    }

    #[test]
    fn conevt_ezshoot_adds_build_zvel() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 1
  Speed 16
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nezshoot Dummy 256\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let vz = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|a| a.class == dummy)
                    .map(|a| a.vel.z())
            });
        assert!(vz.is_some_and(|z| (z + 1.0).abs() < 1e-6), "{vz:?}");
    }

    #[test]
    fn conevt_pstomp_hurts_an_actor_underfoot() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nspawn Dummy\npstomp\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|a| a.class == dummy)
                    .map(|a| a.health)
            });
        assert_eq!(hp, Some(50));
    }

    #[test]
    fn conevt_findplayer_stores_build_dist() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 100
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nfindplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let (px, py, pz, ang) = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z(), a.angle))
            .unwrap();
        let tid = game
            .world_mut()
            .unwrap()
            .spawn_at(dummy, room_core::DVec3::new(px + 16.0, py, pz), ang)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.user_vars.get("user_findplayer").copied(), Some(256));
        assert_eq!(a.target, Some(tid));
    }

    #[test]
    fn conevt_debug_prints_debug() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ndebug\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("DEBUG")), "{msgs:?}");
    }

    #[test]
    fn conevt_findotherplayer_is_minus_one_when_alone() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nfindotherplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let n = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .and_then(|a| a.user_vars.get("user_findotherplayer").copied());
        assert_eq!(n, Some(-1));
    }

    #[test]
    fn conevt_findotherplayer_stores_coop_dist() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(2, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            coop_players: 2,
            ..GameOptions::default()
        });
        game.set_conevt("event USE\nfindotherplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.world().unwrap().players[1].is_some());
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let n = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .and_then(|a| a.user_vars.get("user_findotherplayer").copied());
        assert_eq!(n, Some(256));
    }

    #[test]
    fn conevt_getzrange_stores_build_z() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ngetzrange\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.user_vars.get("user_floorz").copied(), Some(0));
        assert_eq!(a.user_vars.get("user_ceilz").copied(), Some(-32768));
    }

    #[test]
    fn conevt_updatesector_stores_sector_index() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nupdatesector\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.user_vars.get("user_sector").copied(), Some(0));
    }

    #[test]
    fn conevt_getflorzofslope_stores_build_z() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ngetflorzofslope\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.user_vars.get("user_florz").copied(), Some(0));
    }

    #[test]
    fn conevt_getceilzofslope_stores_build_z() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ngetceilzofslope\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.user_vars.get("user_ceilzofslope").copied(), Some(-32768));
    }

    #[test]
    fn conevt_useractor_runs_on_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ncount 5\nstrength 50\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 50);
        assert_eq!(a.user_vars.get("user_concount").copied(), Some(5));
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let p = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(p.user_vars.get("user_concount").copied(), None);
    }

    #[test]
    fn conevt_useractor_ai_jumps_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  See:
    PLAY B -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nai See\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        let sid = a.state.expect("Dummy should have a state");
        let in_see = game
            .world()
            .unwrap()
            .states
            .labels_of(dummy)
            .any(|(label, id)| id == sid && label.eq_ignore_ascii_case("see"));
        assert!(in_see, "USERACTOR ai See should jump the COUNTKILL");
    }

    #[test]
    fn conevt_useractor_findplayer_aims_at_pawn() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nfindplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.target, Some(mo));
        assert_eq!(a.user_vars.get("user_findplayer").copied(), Some(256));
    }

    #[test]
    fn conevt_useractor_shoot_fires_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Bolt : Actor 0
{
  Health 1
  Speed 16
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nshoot Bolt\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let bolt = game.world().unwrap().classes.find_name("Bolt").unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let fired = game.world().unwrap().thinkers.default.iter().any(|&id| {
            game.world()
                .unwrap()
                .actors
                .get(id)
                .is_some_and(|a| a.class == bolt && a.master == Some(dummy_id))
        });
        assert!(fired, "USERACTOR shoot Bolt should fire from the COUNTKILL");
    }

    #[test]
    fn conevt_useractor_ifcansee_runs_when_in_view() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifcansee\nstrength 50\nelse\ncount 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 50);
        assert_eq!(a.user_vars.get("user_concount").copied(), None);
    }

    #[test]
    fn conevt_useractor_ifcansee_else_when_blocked() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
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
            .room("b", rect(256.0, 0.0, 384.0, 128.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .thing(9001, 320.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifcansee\nstrength 50\nelse\ncount 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 100);
        assert_eq!(a.user_vars.get("user_concount").copied(), Some(7));
    }

    #[test]
    fn conevt_useractor_ifpdistl_runs_when_close() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifpdistl 512\nstrength 50\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 50);
    }

    #[test]
    fn conevt_useractor_ifp_palive_runs() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifp palive\nstrength 50\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 50);
    }

    #[test]
    fn conevt_useractor_ifhitspace_runs_when_use_is_held() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifhitspace\nstrength 50\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let health = |game: &Headless| {
            game.world()
                .unwrap()
                .thinkers
                .default
                .iter()
                .find_map(|&id| {
                    game.world()
                        .unwrap()
                        .actors
                        .get(id)
                        .filter(|o| o.class == dummy)
                        .map(|o| o.health)
                })
                .unwrap()
        };
        assert_eq!(health(&game), 100);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert_eq!(health(&game), 50);
    }

    #[test]
    fn conevt_useractor_ifgotweaponce_stays_quiet_unarmed() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt(
            "event USERACTOR\nifgotweaponce\nstrength 50\nelse\nifpinventory FIRSTAID\nstrength 40\nend\n",
        );
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 100);
    }

    #[test]
    fn conevt_useractor_break_stops_the_rest() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nstrength 50\nbreak\nstrength 10\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 50);
    }

    #[test]
    fn conevt_useractor_ifhealthg_and_setactorangle() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt(
            "event USERACTOR\nifhealthg 50\nsetactorangle 1024\ngetactorangle\nsetplayerangle 512\ngetplayerangle\nend\n",
        );
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .cloned()
            })
            .unwrap();
        assert!((a.angle.degrees() - 180.0).abs() < 1e-9);
        assert_eq!(a.user_vars.get("user_actorangle").copied(), Some(1024));
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let player = game.world().unwrap().actors.get(mo).unwrap();
        assert!((player.angle.degrees() - 270.0).abs() < 1e-9);
        assert_eq!(player.user_vars.get("user_playerangle").copied(), Some(512));
    }

    #[test]
    fn conevt_useractor_movevel_walks_along_facing() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 32.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 256 512\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        {
            let a = game.world_mut().unwrap().actors.get_mut(dummy_id).unwrap();
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        let x0 = game.world().unwrap().actors.get(dummy_id).unwrap().pos.x();
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(dummy_id).unwrap();
        assert!(
            (a.pos.x() - (x0 + 16.0)).abs() < 1.0,
            "walked east {x0} -> {}",
            a.pos.x()
        );
        assert!(
            (a.angle.degrees() - 90.0).abs() < 1e-9,
            "angvel 512 is 90° ({})",
            a.angle.degrees()
        );
        assert_eq!(
            a.user_vars.get("user_conchase").copied(),
            Some(1),
            "movevel marks CON chase so A_Chase skips XY"
        );
    }

    #[test]
    fn conevt_useractor_movevel_slides_along_a_wall() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .thing(9001, 232.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 256 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        {
            let a = game.world_mut().unwrap().actors.get_mut(dummy_id).unwrap();
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        let (x0, y0) = {
            let a = game.world().unwrap().actors.get(dummy_id).unwrap();
            (a.pos.x(), a.pos.y())
        };
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(dummy_id).unwrap();
        assert!(
            (a.pos.x() - x0).abs() < 1.0,
            "east wall blocks X {x0} -> {}",
            a.pos.x()
        );
        assert!(
            (a.pos.y() - (y0 + 16.0)).abs() < 1.0,
            "slides left along the wall {y0} -> {}",
            a.pos.y()
        );
    }

    #[test]
    fn conevt_useractor_faceplayer_turns_and_walks() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 32.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 256 0 1\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        {
            let a = game.world_mut().unwrap().actors.get_mut(dummy_id).unwrap();
            a.angle = room_core::Angle::from_degrees(180.0);
        }
        let x0 = game.world().unwrap().actors.get(dummy_id).unwrap().pos.x();
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(dummy_id).unwrap();
        assert!(
            (a.angle.degrees() - 0.0).abs() < 1.0,
            "faceplayer toward pawn ({})",
            a.angle.degrees()
        );
        assert!(
            a.pos.x() > x0 + 8.0,
            "walked toward pawn {x0} -> {}",
            a.pos.x()
        );
    }

    #[test]
    fn conevt_useractor_fleeenemy_turns_away() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 256 0 2\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        {
            let a = game.world_mut().unwrap().actors.get_mut(dummy_id).unwrap();
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        let x0 = game.world().unwrap().actors.get(dummy_id).unwrap().pos.x();
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(dummy_id).unwrap();
        let yaw = a.angle.degrees().rem_euclid(360.0);
        assert!(
            (yaw - 180.0).abs() < 1.0,
            "fleeenemy away from pawn ({yaw})"
        );
        assert!(a.pos.x() < x0 - 8.0, "walked away {x0} -> {}", a.pos.x());
    }

    #[test]
    fn conevt_useractor_spin_turns() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 32.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 0 0 16\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        {
            let a = game.world_mut().unwrap().actors.get_mut(dummy_id).unwrap();
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        game.tick(&[TicCmd::default()]);
        let yaw = game
            .world()
            .unwrap()
            .actors
            .get(dummy_id)
            .unwrap()
            .angle
            .degrees();
        assert!((yaw - 22.5).abs() < 1e-9, "spin is 22.5° per tic ({yaw})");
    }

    #[test]
    fn conevt_useractor_dodgebullet_turns_from_missile() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Bolt : Actor 0
{
  Health 1
  Radius 8
  Height 8
  +MISSILE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 64.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 0 0 32\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let bolt = game.world().unwrap().classes.find_name("Bolt").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        let pos = game.world().unwrap().actors.get(dummy_id).unwrap().pos;
        game.world_mut()
            .unwrap()
            .spawn_at(
                bolt,
                room_core::DVec3::new(pos.x() + 32.0, pos.y(), pos.z() + 32.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        {
            let a = game.world_mut().unwrap().actors.get_mut(dummy_id).unwrap();
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        game.tick(&[TicCmd::default()]);
        let yaw = game
            .world()
            .unwrap()
            .actors
            .get(dummy_id)
            .unwrap()
            .angle
            .degrees()
            .rem_euclid(360.0);
        assert!(
            (yaw - 180.0).abs() < 1.0,
            "dodgebullet faces away from the missile ({yaw})"
        );
    }

    #[test]
    fn conevt_useractor_jumptoplayer_hops() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 32.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 0 0 64\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        let z0 = game.world().unwrap().actors.get(dummy_id).unwrap().pos.z();
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(dummy_id).unwrap();
        assert!(
            a.pos.z() > z0 + 0.5 || a.vel.z() > 1.0,
            "jumptoplayer hops ({z0} -> {} velz {})",
            a.pos.z(),
            a.vel.z()
        );
    }

    #[test]
    fn conevt_useractor_geth_steps_toward_pawn() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 32.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 0 0 128\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        {
            let a = game.world_mut().unwrap().actors.get_mut(dummy_id).unwrap();
            a.angle = room_core::Angle::from_degrees(180.0);
        }
        let x0 = game.world().unwrap().actors.get(dummy_id).unwrap().pos.x();
        game.tick(&[TicCmd::default()]);
        let x1 = game.world().unwrap().actors.get(dummy_id).unwrap().pos.x();
        assert!(x1 > x0 + 4.0, "geth steps toward pawn {x0} -> {x1}");
    }

    #[test]
    fn conevt_useractor_getv_steps_toward_pawn_z() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  +NOGRAVITY
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 32.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 0 0 256\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        {
            let w = game.world_mut().unwrap();
            if let Some(p) = w.actors.get_mut(mo) {
                p.pos = room_core::DVec3::new(p.pos.x(), p.pos.y(), 40.0);
            }
            if let Some(a) = w.actors.get_mut(dummy_id) {
                a.pos = room_core::DVec3::new(a.pos.x(), a.pos.y(), 8.0);
            }
        }
        let z0 = game.world().unwrap().actors.get(dummy_id).unwrap().pos.z();
        game.tick(&[TicCmd::default()]);
        let z1 = game.world().unwrap().actors.get(dummy_id).unwrap().pos.z();
        assert!(z1 > z0 + 1.0, "getv steps toward pawn z {z0} -> {z1}");
    }

    #[test]
    fn conevt_useractor_randomangle_turns() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 32.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovevel 0 0 512\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|o| o.class == dummy)
            })
            .unwrap();
        {
            let a = game.world_mut().unwrap().actors.get_mut(dummy_id).unwrap();
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        game.tick(&[TicCmd::default()]);
        let yaw = game
            .world()
            .unwrap()
            .actors
            .get(dummy_id)
            .unwrap()
            .angle
            .degrees();
        assert!(yaw.abs() > 1.0, "randomangle leaves 0° ({yaw})");
    }

    #[test]
    fn conevt_useractor_ifpdistl_else_when_far() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifpdistl 100\nstrength 50\nelse\ncount 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 100);
        assert_eq!(a.user_vars.get("user_concount").copied(), Some(7));
    }

    #[test]
    fn conevt_useractor_ifpdistg_runs_when_far() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 144.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifpdistg 100\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 40);
    }

    #[test]
    fn conevt_useractor_ifstrength_runs_when_hurt() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifstrength 50\nstrength 10\nelse\ncount 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 100);
        assert_eq!(a.user_vars.get("user_concount").copied(), Some(7));
    }

    #[test]
    fn conevt_useractor_ifcount_runs_after_count() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ncount 5\nifcount 5\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 40);
        assert_eq!(a.user_vars.get("user_concount").copied(), Some(5));
    }

    #[test]
    fn conevt_useractor_killit_drops_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nkillit\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(0));
    }

    #[test]
    fn conevt_useractor_guts_throws_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Chunk : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nguts Chunk 3\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let chunk = game.world().unwrap().classes.find_name("Chunk").unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let n = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == chunk && a.master == Some(dummy_id))
            })
            .count();
        assert_eq!(n, 3);
    }

    #[test]
    fn conevt_useractor_sound_plays_at_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nsound duke/alert\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd::default()]);
        let snds = game.world_mut().unwrap().drain_sound_events();
        let hit = snds.iter().find(|s| s.name == "duke/alert");
        assert!(hit.is_some(), "{snds:?}");
        let s = hit.unwrap();
        assert!((s.x - 160.0).abs() < 1e-6);
        assert!((s.y - 128.0).abs() < 1e-6);
    }

    #[test]
    fn conevt_useractor_print_quotes_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nprint GOTCHA\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd::default()]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("GOTCHA")), "{msgs:?}");
    }

    #[test]
    fn conevt_useractor_spritepal_tints_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nspritepal 21\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let pal = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.translation.clone())
            });
        assert_eq!(pal.as_deref(), Some("DukePal21"));
    }

    #[test]
    fn conevt_useractor_spawn_drops_at_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Egg : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nspawn Egg\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let egg = game.world().unwrap().classes.find_name("Egg").unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let spawned = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|a| a.class == egg && a.master == Some(dummy_id))
            });
        let a = spawned.expect("USERACTOR spawn Egg");
        assert!((a.pos.x() - 160.0).abs() < 1e-6);
        assert!((a.pos.y() - 128.0).abs() < 1e-6);
        assert!((a.vel.x()).abs() < 1e-6);
        assert!((a.vel.y()).abs() < 1e-6);
    }

    #[test]
    fn conevt_useractor_ifspritepal_runs_when_pal_matches() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nspritepal 21\nifspritepal 21\nstrength 50\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 50);
        assert_eq!(a.translation, "DukePal21");
    }

    #[test]
    fn conevt_useractor_ifspritepal_else_on_default_pal() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifspritepal 21\nstrength 50\nelse\ncount 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 100);
        assert_eq!(a.user_vars.get("user_concount").copied(), Some(7));
    }

    #[test]
    fn conevt_useractor_soundonce_plays_only_once() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nsoundonce duke/alert\nsoundonce duke/alert\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd::default()]);
        game.tick(&[TicCmd::default()]);
        let bangs = game
            .world_mut()
            .unwrap()
            .drain_sound_events()
            .iter()
            .filter(|s| s.name == "duke/alert")
            .count();
        let heard = game
            .world()
            .unwrap()
            .sound_heard
            .iter()
            .any(|n| n == "duke/alert");
        assert_eq!(bangs, 0);
        assert!(
            heard,
            "soundonce should have played once, then stayed queued"
        );
    }

    #[test]
    fn conevt_useractor_cstat_hides_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, FLAG_DONTDRAW, FLAG_SOLID, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SOLID
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ncstat 32768\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_ne!(a.flags & FLAG_DONTDRAW, 0);
        assert_eq!(a.flags & FLAG_SOLID, 0);
    }

    #[test]
    fn conevt_useractor_cstator_ors_flags() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, FLAG_DONTDRAW, FLAG_SOLID, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SOLID
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ncstator 32768\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_ne!(a.flags & FLAG_DONTDRAW, 0);
        assert_ne!(a.flags & FLAG_SOLID, 0);
    }

    #[test]
    fn conevt_useractor_sizeat_sets_scale() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nsizeat 32 32\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert!((a.scale_x - 0.5).abs() < 1e-6);
        assert!((a.scale_y - 0.5).abs() < 1e-6);
    }

    #[test]
    fn conevt_useractor_sizeto_steps_scale() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nsizeto 32 32\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert!((a.scale_x - 63.0 / 64.0).abs() < 1e-6);
        assert!((a.scale_y - 63.0 / 64.0).abs() < 1e-6);
    }

    #[test]
    fn conevt_useractor_clipdist_sets_radius() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nclipdist 128\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert!((a.radius - 8.0).abs() < 1e-6);
    }

    #[test]
    fn conevt_useractor_ifcanseetarget_runs_when_aimed() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nfindplayer\nifcanseetarget\nstrength 50\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 50);
    }

    #[test]
    fn conevt_useractor_ifcanseetarget_else_without_target() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifcanseetarget\nstrength 50\nelse\ncount 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 100);
        assert_eq!(a.user_vars.get("user_concount").copied(), Some(7));
    }

    #[test]
    fn conevt_useractor_cactor_changes_class() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Other : Actor 0
{
  Health 50
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ncactor Other\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let other = game.world().unwrap().classes.find_name("Other").unwrap();
        let dummy_id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let class = game.world().unwrap().actors.get(dummy_id).map(|a| a.class);
        assert_eq!(class, Some(other));
    }

    #[test]
    fn conevt_useractor_getlastpal_restores_pal() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nspritepal 21\nspritepal 6\ngetlastpal\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let pal = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.translation.clone())
            });
        assert_eq!(pal.as_deref(), Some("DukePal21"));
    }

    #[test]
    fn conevt_useractor_stopsound_drops_the_cue() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nsound duke/alert\nstopsound duke/alert\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd::default()]);
        let bangs = game
            .world_mut()
            .unwrap()
            .drain_sound_events()
            .iter()
            .filter(|s| s.name == "duke/alert")
            .count();
        assert_eq!(bangs, 0);
    }

    #[test]
    fn conevt_useractor_hitradius_hurts_the_pawn() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nhitradius 1024 25\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 75);
    }

    #[test]
    fn conevt_useractor_fall_snaps_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nfall\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            a.pos = room_core::DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + 80.0);
            a.vel = room_core::DVec3::new(a.vel.x(), a.vel.y(), 10.0);
        }
        game.tick(&[TicCmd::default()]);
        let (z, vz) = game
            .world()
            .unwrap()
            .actors
            .get(id)
            .map(|a| (a.pos.z(), a.vel.z()))
            .unwrap();
        assert!(z.abs() < 1.0, "z={z}");
        assert_eq!(vz, 0.0);
    }

    #[test]
    fn conevt_useractor_operate_uses_the_door() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].tags.push(1);
        map.lines[0].special = 12;
        map.lines[0].args = [1, 16, 150, 0, 0];
        map.lines[0].activation = room_map::spac::USE;
        let v1 = map.vertices[map.lines[0].v1.raw() as usize].pos;
        let v2 = map.vertices[map.lines[0].v2.raw() as usize].pos;
        let mid = room_core::DVec2::new((v1.x() + v2.x()) * 0.5, (v1.y() + v2.y()) * 0.5);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\noperate\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            let dx = v2.x() - v1.x();
            let dy = v2.y() - v1.y();
            let len = room_core::dmath::sqrt(dx * dx + dy * dy).max(1.0);
            let mut nx = -dy / len;
            let mut ny = dx / len;
            let mut px = mid.x() + nx * 32.0;
            let mut py = mid.y() + ny * 32.0;
            if !(0.0..=256.0).contains(&px) || !(0.0..=256.0).contains(&py) {
                nx = -nx;
                ny = -ny;
                px = mid.x() + nx * 32.0;
                py = mid.y() + ny * 32.0;
            }
            a.pos = room_core::DVec3::new(px, py, a.pos.z());
            a.angle = room_core::Angle::from_radians(room_core::dmath::atan2(-ny, -nx));
        }
        assert!(game.world().unwrap().doors.is_empty());
        game.tick(&[TicCmd::default()]);
        assert!(
            !game.world().unwrap().doors.is_empty(),
            "USERACTOR operate should raise the door"
        );
    }

    #[test]
    fn conevt_useractor_lotsofglass_throws_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor DukeGlassPiece : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nlotsofglass 3\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let shard = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeGlassPiece")
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == shard)
            })
            .count();
        game.tick(&[TicCmd::default()]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == shard)
            })
            .count();
        assert_eq!(after, before + 3);
    }

    #[test]
    fn conevt_useractor_money_drops_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor DukeMoney : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmoney 4\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let bill = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeMoney")
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == bill)
            })
            .count();
        game.tick(&[TicCmd::default()]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == bill)
            })
            .count();
        assert_eq!(after, before + 4);
    }

    #[test]
    fn conevt_useractor_mail_drops_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor DukeMail : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmail 2\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let letter = game.world().unwrap().classes.find_name("DukeMail").unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == letter)
            })
            .count();
        game.tick(&[TicCmd::default()]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == letter)
            })
            .count();
        assert_eq!(after, before + 2);
    }

    #[test]
    fn conevt_useractor_paper_drops_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor DukePaper : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\npaper 5\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let sheet = game
            .world()
            .unwrap()
            .classes
            .find_name("DukePaper")
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == sheet)
            })
            .count();
        game.tick(&[TicCmd::default()]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == sheet)
            })
            .count();
        assert_eq!(after, before + 5);
    }

    #[test]
    fn conevt_useractor_ifactioncount_runs_after_actioncount() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nactioncount 8\nifactioncount 8\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 40);
        assert_eq!(a.user_vars.get("user_conactioncount").copied(), Some(8));
    }

    #[test]
    fn conevt_useractor_resetactioncount_clears_ifactioncount() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt(
            "event USERACTOR\nactioncount 8\nresetactioncount\nifactioncount 8\nstrength 40\nend\n",
        );
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.health, 100);
        assert_eq!(a.user_vars.get("user_conactioncount").copied(), Some(0));
    }

    #[test]
    fn conevt_useractor_ifrnd_zero_skips() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifrand 0\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(100));
    }

    #[test]
    fn conevt_useractor_ifrnd_always_runs() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifrand 256\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_lotsofcolour_throws_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor DukeGlassPiece : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nlotsofcolour 4\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let shard = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeGlassPiece")
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let tinted = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == shard && a.translation.starts_with("DukePal"))
            })
            .count();
        assert_eq!(tinted, 4);
    }

    #[test]
    fn conevt_useractor_zshoot_adds_build_zvel() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Bolt : Actor 0
{
  Health 1
  Speed 16
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nzshoot Bolt 256\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let bolt = game.world().unwrap().classes.find_name("Bolt").unwrap();
        game.tick(&[TicCmd::default()]);
        let vz = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|a| a.class == bolt)
                    .map(|a| a.vel.z())
            });
        assert!(vz.is_some_and(|z| (z + 1.0).abs() < 1e-6), "{vz:?}");
    }

    #[test]
    fn conevt_useractor_ifactor_runs_when_class_matches() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifactor Dummy\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifactor_else_when_class_differs() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifactor LIZTROOP\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifspawnedby_runs_when_master_matches() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifspawnedby PlayerPawn\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            a.master = Some(mo);
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(id)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 40);
    }

    #[test]
    fn conevt_useractor_ifspawnedby_else_without_master() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt(
            "event USERACTOR\nifspawnedby PlayerPawn\nstrength 40\nelse\nstrength 7\nend\n",
        );
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifactornotstayput_runs_when_free() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifactornotstayput\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifactornotstayput_else_when_stayput() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifactornotstayput\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            a.user_vars.insert("user_stayput".into(), 1);
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(id)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 7);
    }

    #[test]
    fn conevt_useractor_ifaction_runs_on_spawn_label() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  See:
    PLAY B -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifaction Spawn\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifaction_else_on_other_label() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  See:
    PLAY B -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifaction See\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_addkills_bumps_map_count() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\naddkills 3\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let before = game.world().unwrap().level.found_kills;
        game.tick(&[TicCmd::default()]);
        let after = game.world().unwrap().level.found_kills;
        assert_eq!(after, before + 3);
    }

    #[test]
    fn conevt_useractor_ifinwater_runs_when_submerged() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].waterzone = true;
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifinwater\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifinwater_else_on_dry_land() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifinwater\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_tip_prints_from_countkill() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ntip\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.tick(&[TicCmd::default()]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("TIP")), "{msgs:?}");
        let snds = game.world_mut().unwrap().drain_sound_events();
        assert!(snds.iter().any(|s| s.name == "duke/duke_tip1"), "{snds:?}");
    }

    #[test]
    fn conevt_useractor_ifoutside_runs_under_sky() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].ceiling_pic = "F_SKY1".into();
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifoutside\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifoutside_else_indoors() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifoutside\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifinspace_runs_when_both_planes_sky() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].ceiling_pic = "F_SKY1".into();
        map.sectors[0].floor_pic = "F_SKY1".into();
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifinspace\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifinouterspace_else_indoors() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifinouterspace\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_iffloordistl_runs_on_the_floor() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\niffloordistl 1024\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifceilingdistl_else_in_tall_room() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt(
            "event USERACTOR\nifceilingdistl 100\nstrength 40\nelse\nstrength 7\nend\n",
        );
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifgapzl_runs_in_a_low_room() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 64.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifgapzl 20000\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifgapzl_else_in_a_tall_room() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifgapzl 1024\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifawayfromwall_runs_in_the_open() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifawayfromwall\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifawayfromwall_else_near_a_wall() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 2.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifawayfromwall\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            a.radius = 1.0;
            a.pos = room_core::DVec3::new(2.0, 128.0, a.pos.z());
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifangdiffl_runs_when_facing_pawn() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifangdiffl 256\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            a.angle = room_core::Angle::from_degrees(180.0);
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(id)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 40);
    }

    #[test]
    fn conevt_useractor_ifangdiffl_else_when_looking_away() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifangdiffl 256\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(id)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 7);
    }

    #[test]
    fn conevt_useractor_ifbulletnear_runs_when_missile_close() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Bolt : Actor 0
{
  Health 1
  Radius 8
  Height 8
  +MISSILE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifbulletnear\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let bolt = game.world().unwrap().classes.find_name("Bolt").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        let pos = game.world().unwrap().actors.get(id).unwrap().pos;
        game.world_mut()
            .unwrap()
            .spawn_at(
                bolt,
                room_core::DVec3::new(pos.x() + 32.0, pos.y(), pos.z() + 32.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(id)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 40);
    }

    #[test]
    fn conevt_useractor_ifbulletnear_else_without_missile() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifbulletnear\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifcanshoottarget_runs_when_facing_pawn() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifcanshoottarget\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            a.angle = room_core::Angle::from_degrees(180.0);
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(id)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 40);
    }

    #[test]
    fn conevt_useractor_ifcanshoottarget_else_when_looking_away() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifcanshoottarget\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let id = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(id) {
            a.angle = room_core::Angle::from_degrees(0.0);
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(id)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 7);
    }

    #[test]
    fn conevt_useractor_ifnosounds_runs_when_quiet() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifnosounds\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifsound_runs_when_cue_is_queued() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifsound duke/alert\nstrength 40\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.world_mut()
            .unwrap()
            .play_sound("duke/alert", 160.0, 128.0, 0.0, 1.0);
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_quake_starts_a_rumble() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nquake 35\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(game.world().unwrap().quakes.is_empty());
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        assert_eq!(w.quakes.len(), 1);
        assert_eq!(w.quakes[0].countdown, 35);
    }

    #[test]
    fn conevt_useractor_endofgame_exits_the_map() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nendofgame\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(!game.world().unwrap().level.exiting);
        game.tick(&[TicCmd::default()]);
        assert!(game.world().unwrap().level.exiting);
    }

    #[test]
    fn conevt_useractor_debug_prints_debug() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ndebug\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd::default()]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("DEBUG")), "{msgs:?}");
    }

    #[test]
    fn conevt_useractor_pstomp_hurts_the_player() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 132.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\npstomp\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let hp = w.actors.get(mo).map(|a| a.health).unwrap_or(0);
        assert!(hp <= 50, "player health {hp}");
    }

    #[test]
    fn conevt_useractor_ifwasweapon_runs_when_hit_by_rpg() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, LastDamageInfo, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifwasweapon RPG\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        {
            let w = game.world_mut().unwrap();
            w.last_damage = Some(LastDamageInfo {
                tic: w.level.time,
                dealt: 10,
                raw: 10,
                target: Some(did),
                source: None,
                inflictor: None,
                source_class: "Rocket".into(),
                inflictor_class: "Rocket".into(),
                cause: "test".into(),
            });
        }
        game.tick(&[TicCmd::default()]);
        let hp = game.world().unwrap().actors.get(did).map(|a| a.health);
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifwasweapon_else_when_wrong_gun() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, LastDamageInfo, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifwasweapon RPG\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        {
            let w = game.world_mut().unwrap();
            w.last_damage = Some(LastDamageInfo {
                tic: w.level.time,
                dealt: 10,
                raw: 10,
                target: Some(did),
                source: None,
                inflictor: None,
                source_class: "BulletPuff".into(),
                inflictor_class: "BulletPuff".into(),
                cause: "test".into(),
            });
        }
        game.tick(&[TicCmd::default()]);
        let hp = game.world().unwrap().actors.get(did).map(|a| a.health);
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifhitweapon_runs_when_this_actor_was_hit() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, LastDamageInfo, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifhitweapon\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        {
            let w = game.world_mut().unwrap();
            w.last_damage = Some(LastDamageInfo {
                tic: w.level.time,
                dealt: 10,
                raw: 10,
                target: Some(did),
                source: None,
                inflictor: None,
                source_class: String::new(),
                inflictor_class: String::new(),
                cause: "test".into(),
            });
        }
        game.tick(&[TicCmd::default()]);
        let hp = game.world().unwrap().actors.get(did).map(|a| a.health);
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifhitweapon_else_when_unhurt() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifhitweapon\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifmultiplayer_runs_in_dukematch() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            deathmatch: true,
            ..Default::default()
        });
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifmultiplayer\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifmultiplayer_else_when_alone() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifmultiplayer\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifsquished_runs_when_crushed() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, LastDamageInfo, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifsquished\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        {
            let w = game.world_mut().unwrap();
            w.last_damage = Some(LastDamageInfo {
                tic: w.level.time,
                dealt: 10,
                raw: 10,
                target: Some(did),
                source: None,
                inflictor: None,
                source_class: String::new(),
                inflictor_class: String::new(),
                cause: "crush sector 0".into(),
            });
        }
        game.tick(&[TicCmd::default()]);
        let hp = game.world().unwrap().actors.get(did).map(|a| a.health);
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifsquished_else_in_open_room() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifsquished\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifrespawn_runs_in_dukematch() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            deathmatch: true,
            ..Default::default()
        });
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifrespawn\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifrespawn_else_in_sp() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifrespawn\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifdead_else_when_alive() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifdead\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_ifnotmoving_runs_when_still() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifnotmoving\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(40));
    }

    #[test]
    fn conevt_useractor_ifnotmoving_else_when_sliding() {
        use room_core::DVec3;
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  +NOGRAVITY
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nifnotmoving\nstrength 40\nelse\nstrength 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(did) {
            a.vel = DVec3::new(8.0, 0.0, 0.0);
        }
        game.tick(&[TicCmd::default()]);
        let hp = game.world().unwrap().actors.get(did).map(|a| a.health);
        assert_eq!(hp, Some(7));
    }

    #[test]
    fn conevt_useractor_wackplayer_hurts_and_flashes() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nwackplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 50;
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.health)
            .unwrap_or(-1);
        assert_eq!(hp, 40);
        let flash = game.world().unwrap().players[0]
            .as_ref()
            .map(|p| p.damage_count)
            .unwrap_or(0);
        assert!(
            flash > 0,
            "wackplayer should flash HUD, damage_count={flash}"
        );
    }

    #[test]
    fn conevt_useractor_lockplayer_freezes_the_pawn() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nlockplayer 32\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(!game.world().unwrap().players[0].as_ref().unwrap().frozen);
        game.tick(&[TicCmd::default()]);
        let w = game.world().unwrap();
        assert!(w.players[0].as_ref().unwrap().frozen);
        let mo = w.players[0].as_ref().and_then(|p| p.mo).unwrap();
        let n = w
            .actors
            .get(mo)
            .and_then(|a| a.user_vars.get("user_lockplayer").copied());
        assert_eq!(n, Some(32));
    }

    #[test]
    fn conevt_useractor_palfrom_sets_damage_count() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\npalfrom 32\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd::default()]);
        let flash = game.world().unwrap().players[0]
            .as_ref()
            .map(|p| p.damage_count)
            .unwrap_or(0);
        assert_eq!(flash, 32);
    }

    #[test]
    fn conevt_useractor_addphealth_heals_the_player() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\naddphealth 10\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[TicCmd::default()]);
        let hp = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.health)
            .unwrap_or(0);
        assert_eq!(hp, 30);
    }

    #[test]
    fn conevt_useractor_resetplayer_restores_spawn() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nresetplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let (spawn, want_hp) = {
            let w = game.world().unwrap();
            let a = w.actors.get(mo).unwrap();
            let hp = w.classes.get(a.class).map(|c| c.health).unwrap_or(100);
            (a.spawn_point, hp)
        };
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 10;
            a.pos = room_core::DVec3::new(a.pos.x() + 40.0, a.pos.y() + 16.0, a.pos.z() + 20.0);
            a.vel = room_core::DVec3::new(4.0, 0.0, 2.0);
        }
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(mo).unwrap();
        assert_eq!(a.health, want_hp.max(1));
        assert!((a.pos.x() - spawn.x()).abs() < 1e-6, "x={}", a.pos.x());
        assert!((a.pos.y() - spawn.y()).abs() < 1e-6, "y={}", a.pos.y());
        assert!(a.vel.length_squared() < 1e-9, "vel={:?}", a.vel);
    }

    #[test]
    fn conevt_useractor_tossweapon_drops_the_ready_gun() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Shell : Actor 2008
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
  Weapon.AmmoType "Shell"
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ntossweapon\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        {
            let w = game.world_mut().unwrap();
            room_sim::give_inventory(w, mo, gun, 1);
            if let Some(p) = w.players[0].as_mut() {
                p.ready_weapon = Some(gun);
            }
        }
        game.tick(&[TicCmd::default()]);
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, gun), 0);
        assert!(
            game.world().unwrap().players[0]
                .as_ref()
                .is_some_and(|p| p.ready_weapon.is_none() && p.pending_weapon.is_none())
        );
    }

    #[test]
    fn conevt_useractor_addweapon_gives_shotgun() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Shell : Actor 2008
{
  Inventory.Amount 1
  Inventory.MaxAmount 50
}
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
  Weapon.AmmoType "Shell"
  Inventory.Amount 1
  Inventory.MaxAmount 1
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\naddweapon SHOTGUN 8\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        let shell = game.world().unwrap().classes.find_name("Shell").unwrap();
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, gun), 0);
        game.tick(&[TicCmd::default()]);
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, gun), 1);
        assert_eq!(
            room_sim::count_inventory(game.world().unwrap(), mo, shell),
            8
        );
        assert_eq!(
            game.world().unwrap().players[0]
                .as_ref()
                .and_then(|p| p.pending_weapon),
            Some(gun)
        );
    }

    #[test]
    fn conevt_useractor_addinventory_gives_firstaid() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor DukeFirstAid : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 100
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\naddinventory FIRSTAID 25\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let kit = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeFirstAid")
            .unwrap();
        assert_eq!(room_sim::count_inventory(game.world().unwrap(), mo, kit), 0);
        game.tick(&[TicCmd::default()]);
        assert_eq!(
            room_sim::count_inventory(game.world().unwrap(), mo, kit),
            25
        );
    }

    #[test]
    fn conevt_useractor_addammo_gives_clip() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Clip : Actor 2007
{
  Inventory.Amount 1
  Inventory.MaxAmount 200
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\naddammo PISTOL_WEAPON 12\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let clip = game.world().unwrap().classes.find_name("Clip").unwrap();
        let before = room_sim::count_inventory(game.world().unwrap(), mo, clip);
        game.tick(&[TicCmd::default()]);
        let after = room_sim::count_inventory(game.world().unwrap(), mo, clip);
        assert_eq!(after, before + 12);
    }

    #[test]
    fn conevt_useractor_movesprite_shifts_the_actor() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  +NOGRAVITY
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nmovesprite 256 0 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(did)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let after = game
            .world()
            .unwrap()
            .actors
            .get(did)
            .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
            .unwrap();
        assert!(
            (after.0 - before.0 - 16.0).abs() < 1e-6,
            "{before:?} -> {after:?}"
        );
        assert!((after.1 - before.1).abs() < 1e-6);
        assert!((after.2 - before.2).abs() < 1e-6);
    }

    #[test]
    fn conevt_useractor_setsprite_snaps_the_actor() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  +NOGRAVITY
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nsetsprite 2304 -2048 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let pos = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|a| a.class == dummy)
                    .map(|a| (a.pos.x(), a.pos.y(), a.pos.z()))
            })
            .unwrap();
        assert!((pos.0 - 144.0).abs() < 1e-6, "{pos:?}");
        assert!((pos.1 - 128.0).abs() < 1e-6);
        assert!(pos.2.abs() < 1e-6);
    }

    #[test]
    fn conevt_useractor_clipmove_shifts_when_open() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  +NOGRAVITY
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nclipmove 256 0 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(did)
            .map(|a| (a.pos.x(), a.pos.y()))
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(did).unwrap();
        assert!(
            (a.pos.x() - before.0 - 16.0).abs() < 1e-6,
            "{before:?} -> {}",
            a.pos.x()
        );
        assert!((a.pos.y() - before.1).abs() < 1e-6);
        assert_eq!(a.user_vars.get("user_clipmove").copied(), Some(1));
    }

    #[test]
    fn conevt_useractor_changespritestat_stores_stat() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nchangespritestat 1\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let n = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|a| a.class == dummy)
                    .and_then(|a| a.user_vars.get("user_spritestat").copied())
            });
        assert_eq!(n, Some(1));
    }

    #[test]
    fn conevt_useractor_wackmove_hurts_the_blocker() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Pillar : Actor 9002
{
  Health 100
  Radius 16
  Height 56
  +SOLID
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .thing(9002, 192.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nwackmove 512 0 0\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let pillar = game.world().unwrap().classes.find_name("Pillar").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(did).unwrap();
        assert_eq!(a.user_vars.get("user_wackmove").copied(), Some(0));
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == pillar)
                    .map(|o| o.health)
            });
        assert_eq!(hp, Some(90));
    }

    #[test]
    fn conevt_useractor_getzrange_stores_build_z() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ngetzrange\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let a = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
            })
            .unwrap();
        assert_eq!(a.user_vars.get("user_floorz").copied(), Some(0));
        assert_eq!(a.user_vars.get("user_ceilz").copied(), Some(-32768));
    }

    #[test]
    fn conevt_useractor_updatesector_stores_sector_index() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nupdatesector\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let n = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .and_then(|a| a.user_vars.get("user_sector").copied())
            });
        assert_eq!(n, Some(0));
    }

    #[test]
    fn conevt_useractor_getflorzofslope_stores_build_z() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ngetflorzofslope\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let n = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .and_then(|a| a.user_vars.get("user_florz").copied())
            });
        assert_eq!(n, Some(0));
    }

    #[test]
    fn conevt_useractor_getceilzofslope_stores_build_z() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\ngetceilzofslope\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let n = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .and_then(|a| a.user_vars.get("user_ceilzofslope").copied())
            });
        assert_eq!(n, Some(-32768));
    }

    #[test]
    fn conevt_useractor_respawnhitag_revives_dead_tid() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor Corpse : Actor 9002
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .thing(9002, 192.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nrespawnhitag 7\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        let corpse = game.world().unwrap().classes.find_name("Corpse").unwrap();
        let did = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == dummy)
            })
            .unwrap();
        let cid = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == corpse)
            })
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(cid) {
            a.tid = 7;
            a.health = 0;
            a.destroyed = true;
        }
        game.tick(&[TicCmd::default()]);
        let a = game.world().unwrap().actors.get(cid).unwrap();
        assert_eq!(a.health, 100);
        assert!(!a.destroyed);
        let n = game
            .world()
            .unwrap()
            .actors
            .get(did)
            .and_then(|a| a.user_vars.get("user_respawnhitag").copied());
        assert_eq!(n, Some(1));
    }

    #[test]
    fn conevt_useractor_findotherplayer_stores_player_dist() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nfindotherplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let n = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .and_then(|a| a.user_vars.get("user_findotherplayer").copied())
            });
        assert_eq!(n, Some(512));
    }

    #[test]
    fn conevt_useractor_findotherplayer_prefers_closer_coop() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 9001
{
  Health 100
  Radius 16
  Height 56
  +COUNTKILL
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(2, 144.0, 128.0, 0)
            .thing(9001, 160.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            coop_players: 2,
            ..GameOptions::default()
        });
        game.set_defs(classes, states);
        game.set_conevt("event USERACTOR\nfindotherplayer\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd::default()]);
        let n = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|o| o.class == dummy)
                    .and_then(|a| a.user_vars.get("user_findotherplayer").copied())
            });
        assert_eq!(n, Some(256));
    }

    #[test]
    fn conevt_lotsofglass_throws_shards() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeGlassPiece : Actor 0
{
  Health 1
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nlotsofglass 8\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let shard = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeGlassPiece")
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == shard)
            })
            .count();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .filter(|&&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == shard)
            })
            .count();
        assert_eq!(after, before + 8);
    }

    #[test]
    fn conevt_spritepal_sets_translation() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nspritepal 21\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let pal = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.translation.as_str())
            .unwrap_or("");
        assert_eq!(pal, "DukePal21");
    }

    #[test]
    fn conevt_sizeat_sets_scale() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsizeat 32 32\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let (sx, sy) = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.scale_x, a.scale_y))
            .unwrap_or((0.0, 0.0));
        assert!((sx - 0.5).abs() < 1e-9, "scale_x={sx}");
        assert!((sy - 0.5).abs() < 1e-9, "scale_y={sy}");
    }

    #[test]
    fn conevt_sizeto_steps_scale() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsizeto 32 32\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.scale_x, a.scale_y))
            .unwrap_or((0.0, 0.0));
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let (sx, sy) = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.scale_x, a.scale_y))
            .unwrap_or((0.0, 0.0));
        let want = 63.0 / 64.0;
        assert!((before.0 - 1.0).abs() < 1e-9, "before_x={}", before.0);
        assert!((sx - want).abs() < 1e-9, "scale_x={sx}");
        assert!((sy - want).abs() < 1e-9, "scale_y={sy}");
    }

    #[test]
    fn conevt_count_sets_ifcount() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ncount 8\nifcount 8\nprint READY\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("READY")), "{msgs:?}");
    }

    #[test]
    fn conevt_resetcount_clears_ifcount() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ncount 8\nresetcount\nifcount 8\nprint READY\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("READY")), "{msgs:?}");
    }

    #[test]
    fn conevt_actioncount_sets_ifactioncount() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nactioncount 8\nifactioncount 8\nprint READY\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("READY")), "{msgs:?}");
    }

    #[test]
    fn conevt_resetactioncount_clears_ifactioncount() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt(
            "event USE\nactioncount 8\nresetactioncount\nifactioncount 8\nprint READY\nend\n",
        );
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("READY")), "{msgs:?}");
    }

    #[test]
    fn conevt_addkills_bumps_the_tally() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\naddkills 3\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let before = game.world().unwrap().level.found_kills;
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let after = game.world().unwrap().level.found_kills;
        assert_eq!(after, before + 3);
    }

    #[test]
    fn conevt_hitradius_hurts_a_nearby_actor() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Dummy : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nspawn Dummy\nhitradius 4096 50\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let dummy = game.world().unwrap().classes.find_name("Dummy").unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let hp = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .find_map(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .filter(|a| a.class == dummy)
                    .map(|a| a.health)
            });
        assert_eq!(hp, Some(50));
    }

    #[test]
    fn conevt_fall_snaps_to_the_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nfall\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.pos = room_core::DVec3::new(a.pos.x(), a.pos.y(), a.pos.z() + 80.0);
            a.vel = room_core::DVec3::new(a.vel.x(), a.vel.y(), 10.0);
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let (z, vz) = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.pos.z(), a.vel.z()))
            .unwrap();
        assert!(z.abs() < 1.0, "z={z}");
        assert!(vz.abs() < 1e-9, "vel.z={vz}");
    }

    #[test]
    fn conevt_sleeptime_sets_reaction() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nsleeptime 30\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let rt = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.reaction_time)
            .unwrap_or(0);
        assert_eq!(rt, 30);
    }

    #[test]
    fn conevt_getlastpal_restores_palette() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nspritepal 21\ngetlastpal\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let pal = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.translation.as_str())
            .unwrap_or("?");
        assert_eq!(pal, "");
    }

    #[test]
    fn conevt_cstat_hides_the_pawn() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ncstat 32768\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let flags = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.flags)
            .unwrap_or(0);
        assert_ne!(flags & room_sim::FLAG_DONTDRAW, 0);
    }

    #[test]
    fn conevt_cstator_ors_flags() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\ncstator 2\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let before = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| a.flags)
            .unwrap_or(0);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let (flags, alpha) = game
            .world()
            .unwrap()
            .actors
            .get(mo)
            .map(|a| (a.flags, a.alpha))
            .unwrap_or((0, 1.0));
        assert!((alpha - 0.66).abs() < 1e-9, "alpha={alpha}");
        assert_eq!(flags & room_sim::FLAG_SOLID, before & room_sim::FLAG_SOLID);
    }

    #[test]
    fn conevt_ifp_palive_prints_on_enter() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifp palive\nprint STILL KICKING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("STILL KICKING")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pfalling_stays_quiet_on_ground() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifp pfalling\nprint GERONIMO\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("GERONIMO")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifoutside_prints_under_sky() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].ceiling_pic = "F_SKY1".into();
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifoutside\nprint NICE VIEW\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("NICE VIEW")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifoutside_stays_quiet_indoors() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifoutside\nprint NICE VIEW\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("NICE VIEW")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifpalswap_prints_when_flashed() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifpalswap\nprint FLASHED\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        if let Some(p) = game.world_mut().unwrap().players[0].as_mut() {
            p.damage_count = 16;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("FLASHED")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifpalswap_stays_quiet_without_flash() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifpalswap\nprint FLASHED\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("FLASHED")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifsomethingonplayer_prints_when_overlapped() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeCrate : Actor 9302
{
  Health 100
  Radius 16
  Height 32
  +SOLID
  +SHOOTABLE
  States { Spawn: POSS A -1 Stop }
}
actor PlayerPawn : Actor 0
{
  Health 100
  Radius 16
  Height 56
  +SHOOTABLE
  States { Spawn: PLAY A -1 Stop }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nifsomethingonplayer\nprint RIDER\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let crate_c = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeCrate")
            .unwrap();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let (x, y, z) = {
            let a = game.world().unwrap().actors.get(mo).unwrap();
            (a.pos.x(), a.pos.y(), a.pos.z())
        };
        let _ = game.world_mut().unwrap().spawn_at(
            crate_c,
            room_core::DVec3::new(x, y, z),
            room_core::Angle::ZERO,
        );
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("RIDER")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifsomethingonplayer_stays_quiet_alone() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifsomethingonplayer\nprint RIDER\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("RIDER")), "{msgs:?}");
    }

    #[test]
    fn conevt_resetweapons_prints_on_fresh_start() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt(
            "event RESETWEAPONS\nprint GUNS READY\nend\nevent RESETINVENTORY\nprint PACKED\nend\n",
        );
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("GUNS READY")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("PACKED")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifsquished_prints_when_ceiling_pins() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event GETSHOT\nifsquished\nprint FLAT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 80;
        }
        game.tick(&[]);
        let open = game.world_mut().unwrap().drain_messages();
        assert!(!open.iter().any(|m| m.contains("FLAT")), "{open:?}");
        if let Some(sec) = game.world_mut().unwrap().sectors.get_mut(0) {
            sec.ceiling_z = sec.floor_z + 8.0;
        }
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[]);
        let pin = game.world_mut().unwrap().drain_messages();
        assert!(pin.iter().any(|m| m.contains("FLAT")), "{pin:?}");
    }

    #[test]
    fn conevt_ifsquished_stays_quiet_in_open_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifsquished\nprint FLAT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("FLAT")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifpinventory_prints_when_held() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor DukeFirstAid : Actor 0
{
  Inventory.Amount 1
  Inventory.MaxAmount 100
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event GETSHOT\nifpinventory FIRSTAID\nprint GOT KIT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 80;
        }
        game.tick(&[]);
        let empty = game.world_mut().unwrap().drain_messages();
        assert!(!empty.iter().any(|m| m.contains("GOT KIT")), "{empty:?}");
        let kit = game
            .world()
            .unwrap()
            .classes
            .find_name("DukeFirstAid")
            .unwrap();
        room_sim::give_inventory(game.world_mut().unwrap(), mo, kit, 1);
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[]);
        let held = game.world_mut().unwrap().drain_messages();
        assert!(held.iter().any(|m| m.contains("GOT KIT")), "{held:?}");
    }

    #[test]
    fn conevt_ifpinventory_stays_quiet_without_item() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifpinventory FIRSTAID\nprint GOT KIT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("GOT KIT")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifcansee_prints_when_a_trooper_is_in_view() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifcansee\nprint I SEE YOU\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("I SEE YOU")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifcansee_stays_quiet_when_alone() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifcansee\nprint I SEE YOU\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("I SEE YOU")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifpdistl_prints_when_a_trooper_is_close() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifpdistl 4096\nprint TOO CLOSE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("TOO CLOSE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifpdistl_stays_quiet_when_far_or_alone() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifpdistl 100\nprint TOO CLOSE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("TOO CLOSE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifhitspace_prints_when_use_is_held() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event GETSHOT\nifhitspace\nprint USING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 80;
        }
        game.tick(&[]);
        let open = game.world_mut().unwrap().drain_messages();
        assert!(!open.iter().any(|m| m.contains("USING")), "{open:?}");
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let held = game.world_mut().unwrap().drain_messages();
        assert!(held.iter().any(|m| m.contains("USING")), "{held:?}");
    }

    #[test]
    fn conevt_ifhitspace_stays_quiet_on_enter() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifhitspace\nprint USING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("USING")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifgotweaponce_prints_when_armed() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event GETSHOT\nifgotweaponce\nprint LOCK AND LOAD\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 80;
        }
        game.tick(&[]);
        let empty = game.world_mut().unwrap().drain_messages();
        assert!(
            !empty.iter().any(|m| m.contains("LOCK AND LOAD")),
            "{empty:?}"
        );
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        room_sim::give_inventory(game.world_mut().unwrap(), mo, gun, 1);
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[]);
        let armed = game.world_mut().unwrap().drain_messages();
        assert!(
            armed.iter().any(|m| m.contains("LOCK AND LOAD")),
            "{armed:?}"
        );
    }

    #[test]
    fn conevt_ifgotweaponce_stays_quiet_unarmed() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifgotweaponce\nprint LOCK AND LOAD\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(
            !msgs.iter().any(|m| m.contains("LOCK AND LOAD")),
            "{msgs:?}"
        );
    }

    #[test]
    fn conevt_ifnotmoving_prints_on_enter() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifnotmoving\nprint STANDING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("STANDING")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifnotmoving_stays_quiet_when_walking() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event GETSHOT\nifnotmoving\nprint STANDING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[TicCmd {
            forward_move: 50,
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("STANDING")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifrespawn_prints_in_dukematch() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            deathmatch: true,
            ..Default::default()
        });
        game.set_conevt("event ENTERLEVEL\nifrespawn\nprint AGAIN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("AGAIN")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifrespawn_stays_quiet_in_sp() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifrespawn\nprint AGAIN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("AGAIN")), "{msgs:?}");
    }

    #[test]
    fn conevt_respawn_prints_after_use_while_dead() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions {
            deathmatch: true,
            ..Default::default()
        });
        game.set_defs(classes, states);
        game.set_conevt("event RESPAWN\nifrespawn\nprint AGAIN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        assert!(game.cheat_kill_self(0));
        game.tick(&[]);
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("AGAIN")), "{msgs:?}");
    }

    #[test]
    fn conevt_iffloordistl_prints_on_the_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\niffloordistl 1024\nprint ON DECK\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("ON DECK")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifceilingdistl_stays_quiet_in_a_tall_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifceilingdistl 100\nprint LOW CEIL\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("LOW CEIL")), "{msgs:?}");
    }

    #[test]
    fn conevt_iffloordistg_stays_quiet_on_the_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\niffloordistg 1024\nprint AIR\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("AIR")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifceilingdistg_prints_in_a_tall_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifceilingdistg 100\nprint TALL\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("TALL")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifawayfromwall_prints_in_the_open() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifawayfromwall\nprint ROOM TO MOVE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("ROOM TO MOVE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifawayfromwall_stays_quiet_near_a_wall() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(2.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifawayfromwall\nprint ROOM TO MOVE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("ROOM TO MOVE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifinspace_prints_when_both_planes_are_sky() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].ceiling_pic = "F_SKY1".into();
        map.sectors[0].floor_pic = "F_SKY1".into();
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifinspace\nprint IN THE VOID\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("IN THE VOID")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifinspace_stays_quiet_with_only_a_sky_ceiling() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].ceiling_pic = "F_SKY1".into();
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifinspace\nprint IN THE VOID\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("IN THE VOID")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifgapzl_prints_in_a_low_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 64.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifgapzl 20000\nprint DUCK\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("DUCK")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifgapzl_stays_quiet_in_a_tall_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifgapzl 1024\nprint DUCK\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("DUCK")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifgapzg_prints_in_a_tall_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifgapzg 20000\nprint TALL\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("TALL")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifgapzg_stays_quiet_in_a_low_room() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 64.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifgapzg 20000\nprint TALL\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("TALL")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifangdiffl_prints_when_facing_a_trooper() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .thing(3004, 240.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifangdiffl 256\nprint LOOKING AT YOU\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(
            msgs.iter().any(|m| m.contains("LOOKING AT YOU")),
            "{msgs:?}"
        );
    }

    #[test]
    fn conevt_ifangdiffl_stays_quiet_when_looking_away() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .thing(3004, 64.0, 280.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifangdiffl 256\nprint LOOKING AT YOU\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(
            !msgs.iter().any(|m| m.contains("LOOKING AT YOU")),
            "{msgs:?}"
        );
    }

    #[test]
    fn conevt_ifangdiffg_prints_when_looking_away() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .thing(3004, 64.0, 280.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifangdiffg 256\nprint LOOK AWAY\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("LOOK AWAY")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifangdiffg_stays_quiet_when_facing_a_trooper() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .thing(3004, 240.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifangdiffg 256\nprint LOOK AWAY\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("LOOK AWAY")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifbulletnear_prints_when_a_rocket_is_close() {
        use room_core::{Angle, DVec3};
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let rocket = classes.find_name("Rocket").expect("Rocket");
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nifbulletnear\nprint INCOMING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let pos = {
            let w = game.world().unwrap();
            let mo = w.players[0].as_ref().unwrap().mo.unwrap();
            w.actors.get(mo).unwrap().pos
        };
        game.world_mut()
            .unwrap()
            .spawn_at(
                rocket,
                DVec3::new(pos.x() + 32.0, pos.y(), pos.z() + 32.0),
                Angle::ZERO,
            )
            .unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("INCOMING")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifbulletnear_stays_quiet_without_a_missile() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifbulletnear\nprint INCOMING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("INCOMING")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifinouterspace_prints_on_a_sky_floor() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].floor_pic = "F_SKY1".into();
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifinouterspace\nprint OPEN VOID\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("OPEN VOID")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifinouterspace_stays_quiet_with_only_a_sky_ceiling() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map.sectors[0].ceiling_pic = "F_SKY1".into();
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifinouterspace\nprint OPEN VOID\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("OPEN VOID")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifwasweapon_prints_when_hit_by_a_rocket() {
        use room_core::{Angle, DVec3};
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, LastDamageInfo, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let rocket = classes.find_name("Rocket").expect("Rocket");
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event GETSHOT\nifwasweapon RPG\nprint RPG HIT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let pos = game.world().unwrap().actors.get(mo).unwrap().pos;
        let rid = game
            .world_mut()
            .unwrap()
            .spawn_at(
                rocket,
                DVec3::new(pos.x() + 32.0, pos.y(), pos.z() + 32.0),
                Angle::ZERO,
            )
            .unwrap();
        {
            let w = game.world_mut().unwrap();
            w.last_damage = Some(LastDamageInfo {
                tic: w.level.time,
                dealt: 10,
                raw: 10,
                target: Some(mo),
                source: Some(rid),
                inflictor: Some(rid),
                source_class: "Rocket".into(),
                inflictor_class: "Rocket".into(),
                cause: "test".into(),
            });
            if let Some(a) = w.actors.get_mut(mo) {
                a.health = a.health.saturating_sub(10).max(1);
            }
        }
        game.tick(&[]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("RPG HIT")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifwasweapon_stays_quiet_for_the_wrong_gun() {
        use room_sim::LastDamageInfo;

        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event GETSHOT\nifwasweapon RPG\nprint RPG HIT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        {
            let w = game.world_mut().unwrap();
            w.last_damage = Some(LastDamageInfo {
                tic: w.level.time,
                dealt: 10,
                raw: 10,
                target: Some(mo),
                source: None,
                inflictor: None,
                source_class: String::new(),
                inflictor_class: String::new(),
                cause: "test".into(),
            });
            if let Some(a) = w.actors.get_mut(mo) {
                a.health = a.health.saturating_sub(10).max(1);
            }
        }
        game.tick(&[]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("RPG HIT")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifnosounds_prints_on_a_quiet_start() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifnosounds\nprint DEAD AIR\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("DEAD AIR")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifnosounds_stays_quiet_when_a_sound_is_queued() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifnosounds\nprint DEAD AIR\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.world_mut()
            .unwrap()
            .play_sound("duke/pistol_fire", 128.0, 128.0, 0.0, 1.0);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("DEAD AIR")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifsound_prints_when_that_sound_is_queued() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifsound PISTOL_FIRE\nprint BANG\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let _ = game.world_mut().unwrap().drain_sound_events();
        game.world_mut()
            .unwrap()
            .play_sound("duke/pistol_fire", 128.0, 128.0, 0.0, 1.0);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("BANG")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifsound_stays_quiet_when_that_sound_is_missing() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifsound PISTOL_FIRE\nprint BANG\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("BANG")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifcanshoottarget_prints_when_facing_a_trooper() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .thing(3004, 240.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifcanshoottarget\nprint ON TARGET\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("ON TARGET")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifcanshoottarget_stays_quiet_when_looking_away() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 128.0, 0)
            .thing(3004, 64.0, 280.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifcanshoottarget\nprint ON TARGET\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("ON TARGET")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifspritepal_prints_when_the_pawn_pal_matches() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifspritepal 21\nprint CAPTAIN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.translation = "DukePal21".into();
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("CAPTAIN")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifspritepal_stays_quiet_on_the_default_pal() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifspritepal 21\nprint CAPTAIN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("CAPTAIN")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifactor_prints_for_the_player() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifactor APLAYER\nprint THAT'S ME\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("THAT'S ME")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifactor_stays_quiet_for_a_trooper_name() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifactor LIZTROOP\nprint THAT'S ME\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("THAT'S ME")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifcount_prints_from_the_first_tic() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifcount 0\nprint NOW\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("NOW")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifcount_stays_quiet_before_the_counter() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifcount 32\nprint NOW\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("NOW")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pducking_prints_when_crouched() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifp pducking\nprint DUCKING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        if let Some(p) = game.world_mut().unwrap().players[0].as_mut() {
            p.crouching = true;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("DUCKING")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pducking_stays_quiet_when_standing() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifp pducking\nprint DUCKING\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("DUCKING")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pshrunk_prints_when_shrunk() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifp pshrunk\nprint TINY\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world_mut().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo);
        if let Some(mo) = mo {
            if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
                a.user_vars.insert("user_dukeshink".into(), 100);
            }
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("TINY")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pshrunk_stays_quiet_at_full_size() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifp pshrunk\nprint TINY\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("TINY")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifcanseetarget_prints_when_the_target_is_in_view() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes.clone(), states);
        game.set_conevt("event USE\nifcanseetarget\nprint LOCKED\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let tid = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == zombie)
            });
        let mo = game.world().unwrap().players[0].as_ref().and_then(|p| p.mo);
        if let (Some(mo), Some(tid)) = (mo, tid) {
            if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
                a.target = Some(tid);
            }
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("LOCKED")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifcanseetarget_stays_quiet_without_a_target() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifcanseetarget\nprint LOCKED\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("LOCKED")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifactioncount_prints_from_the_first_tic() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifactioncount 0\nprint NOW\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("NOW")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifactioncount_stays_quiet_before_the_counter() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifactioncount 32\nprint NOW\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("NOW")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifstrength_prints_when_health_is_below() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifstrength 50\nprint WEAK\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
            a.health = 20;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("WEAK")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifstrength_stays_quiet_when_health_holds() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifstrength 1\nprint WEAK\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("WEAK")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifspawnedby_prints_when_master_matches() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes.clone(), states);
        game.set_conevt("event USE\nifspawnedby ZOMBIEMAN\nprint FROM\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let zombie = classes.find_name("ZombieMan").unwrap();
        let tid = game
            .world()
            .unwrap()
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                game.world()
                    .unwrap()
                    .actors
                    .get(id)
                    .is_some_and(|a| a.class == zombie)
            });
        let mo = game.world().unwrap().players[0].as_ref().and_then(|p| p.mo);
        if let (Some(mo), Some(tid)) = (mo, tid) {
            if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
                a.master = Some(tid);
            }
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("FROM")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifspawnedby_stays_quiet_without_a_master() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifspawnedby ZOMBIEMAN\nprint FROM\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("FROM")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_phigher_prints_when_above_a_trooper() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nifp phigher\nprint ABOVE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0].as_ref().and_then(|p| p.mo);
        if let Some(mo) = mo {
            if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
                let z = a.pos.z() + 80.0;
                a.pos = room_core::DVec3::new(a.pos.x(), a.pos.y(), z);
            }
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("ABOVE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_phigher_stays_quiet_on_the_same_floor() {
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
            .thing(3004, 200.0, 200.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifp phigher\nprint ABOVE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("ABOVE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pwalkingback_prints_when_backing_up() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifp pwalkingback\nprint BACK\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            forward_move: -50 * 256,
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("BACK")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pwalkingback_stays_quiet_when_still() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifp pwalkingback\nprint BACK\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("BACK")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifactornotstayput_prints_for_the_player() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifactornotstayput\nprint FREE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("FREE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifactornotstayput_stays_quiet_when_stayput() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifactornotstayput\nprint FREE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0].as_ref().and_then(|p| p.mo);
        if let Some(mo) = mo {
            if let Some(a) = game.world_mut().unwrap().actors.get_mut(mo) {
                a.user_vars.insert("user_stayput".into(), 1);
            }
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("FREE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_paiming_prints_when_looking() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event USE\nifp paiming\nprint AIM\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            pitch: 200,
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("AIM")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_paiming_stays_quiet_when_level() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifp paiming\nprint AIM\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("AIM")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifaction_prints_on_the_spawn_pose() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifaction SPAWN\nprint POSE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("POSE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifaction_stays_quiet_on_the_wrong_pose() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifaction SEE\nprint POSE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("POSE")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifai_prints_on_the_spawn_pose() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifai SPAWN\nprint HUNT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("HUNT")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifai_stays_quiet_on_the_wrong_pose() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifai SEE\nprint HUNT\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("HUNT")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifmove_prints_on_the_spawn_pose() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifmove SPAWN\nprint GO\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("GO")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifmove_stays_quiet_on_the_wrong_pose() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event ENTERLEVEL\nifmove SEE\nprint GO\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("GO")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pweapon_prints_when_armed() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nifp pweapon\nprint GUN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        room_sim::give_inventory(game.world_mut().unwrap(), mo, gun, 1);
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("GUN")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pweapon_stays_quiet_unarmed() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_conevt("event ENTERLEVEL\nifp pweapon\nprint GUN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("GUN")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pweaponout_prints_when_drawn() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nifp pweaponout\nprint DRAWN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        room_sim::give_inventory(game.world_mut().unwrap(), mo, gun, 1);
        if let Some(p) = game.world_mut().unwrap().players[0].as_mut() {
            p.ready_weapon = Some(gun);
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(msgs.iter().any(|m| m.contains("DRAWN")), "{msgs:?}");
    }

    #[test]
    fn conevt_ifp_pweaponout_stays_quiet_when_holstered() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let ir = parse_decorate(
            r#"
actor Shotgun : Actor 0
{
  Weapon.SlotNumber 3
}
actor PlayerPawn : Actor 0
{
  Health 100
  +SHOOTABLE
  States
  {
  Spawn:
    PLAY A -1
    Stop
  }
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
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nifp pweaponout\nprint DRAWN\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        let _ = game.world_mut().unwrap().drain_messages();
        let mo = game.world().unwrap().players[0]
            .as_ref()
            .and_then(|p| p.mo)
            .unwrap();
        let gun = game.world().unwrap().classes.find_name("Shotgun").unwrap();
        room_sim::give_inventory(game.world_mut().unwrap(), mo, gun, 1);
        if let Some(p) = game.world_mut().unwrap().players[0].as_mut() {
            p.ready_weapon = None;
        }
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        let msgs = game.world_mut().unwrap().drain_messages();
        assert!(!msgs.iter().any(|m| m.contains("DRAWN")), "{msgs:?}");
    }

    #[test]
    fn conevt_state_jumps_the_pawn_to_see() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nstate SEE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        assert!(
            super::player_has_action(game.world().unwrap(), "SPAWN"),
            "start in Spawn"
        );
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(
            super::player_has_action(game.world().unwrap(), "SEE"),
            "CON state SEE should jump the pawn"
        );
    }

    #[test]
    fn conevt_state_stays_put_on_a_missing_label() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nstate NOPE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(
            super::player_has_action(game.world().unwrap(), "SPAWN"),
            "missing CON state should leave Spawn"
        );
    }

    #[test]
    fn conevt_ai_jumps_the_pawn_to_see() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nai SEE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(
            super::player_has_action(game.world().unwrap(), "SEE"),
            "CON ai SEE should jump the pawn"
        );
    }

    #[test]
    fn conevt_move_jumps_the_pawn_to_see() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\nmove SEE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(
            super::player_has_action(game.world().unwrap(), "SEE"),
            "CON move SEE should jump the pawn"
        );
    }

    #[test]
    fn conevt_action_jumps_the_pawn_to_see() {
        use room_decorate::parse_decorate;
        use room_sim::{ClassRegistry, StateTable};
        use std::sync::Arc;

        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let ir = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut game = Headless::new(GameOptions::default());
        game.set_defs(classes, states);
        game.set_conevt("event USE\naction SEE\nend\n");
        game.start_custom_map(map, Skill::new(3)).unwrap();
        game.tick(&[TicCmd {
            buttons: Buttons(Buttons::USE),
            ..TicCmd::default()
        }]);
        assert!(
            super::player_has_action(game.world().unwrap(), "SEE"),
            "CON action SEE should jump the pawn"
        );
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

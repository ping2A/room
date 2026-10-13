//! `World` — the playsim state container.

use crate::action::{self, ActionResult};
use crate::actor::{Actor, ActorArena, ActorId};
use crate::class::ClassRegistry;
use crate::compat::CompatFlags;
use crate::inventory::InventoryBags;
use crate::level::LevelState;
use crate::player::{MAX_PLAYERS, Player};
use crate::specials::{
    self, CeilingMover, CeilingScroller, DoorMover, ElevatorMover, FloorMover, FloorScroller,
    HeightTransfer, LightFade, LightGlow, LightStrobe, LightTransfer, PlaneWaggle, PlatMover,
};
use crate::state::StateTable;
use crate::think_player;
use crate::thinker::ThinkerLists;
use crate::vanilla::{SimMathMode, math_mode_for_vanilla_demo};
use room_core::rng::{RngSet, VanillaRng};
use room_core::{Angle, Bam, ClassId, DVec3};
use room_map::MapGeometry;
use std::sync::Arc;
use thiserror::Error;
use xxhash_rust::xxh3::xxh3_64;

/// Player 1 start DoomEdNum.
const THING_PLAYER1_START: i16 = 1;
/// Player 2–4 starts (MBF `-dogs` spawn sites).
const THING_PLAYER2_START: i16 = 2;
const THING_PLAYER3_START: i16 = 3;
const THING_PLAYER4_START: i16 = 4;
/// MBF helper dog DoomEdNum.
const THING_MBF_HELPER_DOG: i32 = 888;

/// Doom `mobj->angle = ANG45 * (mthing->angle / 45)` — exact BAM for 45° steps.
fn doom_thing_angle(degrees: i16) -> Angle {
    let steps = i32::from(degrees) / 45;
    Bam((steps as u32).wrapping_mul(0x2000_0000)).to_angle()
}

/// Boom / GZDoom `Sector_SetFriction` amount → ground friction factor.
/// Unpack UDMF `0xRRGGBB` (`-1` = use `default`).
fn rgb_from_udmf(packed: i32, default: [u8; 3]) -> [u8; 3] {
    if packed < 0 {
        return default;
    }
    let v = packed as u32;
    [
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    ]
}

fn boom_friction_factor(amount: i32) -> f64 {
    let friction = (f64::from(0x1EB8 * amount) / f64::from(0x80) + f64::from(0xD001)) / 65536.0;
    friction.clamp(0.0, 1.0)
}

/// Doom skill bits: easy=1, normal=2, hard=4.
/// Empty skill bits (common in incomplete UDMF) → spawn on all skills.
fn thing_fits_skill(flags: i16, skill: u8) -> bool {
    if flags & 7 == 0 {
        return true;
    }
    let mask: i16 = match skill {
        1 | 2 => 1,
        3 => 2,
        4 | 5 => 4,
        _ => 7,
    };
    (flags & mask) != 0
}

/// Sim errors.
#[derive(Debug, Error)]
pub enum SimError {
    /// Bad map / setup.
    #[error("{0}")]
    Message(String),
}

/// Most recent damage dealt to a player pawn (shell debug / HUD attribution).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LastDamageInfo {
    /// Level tic when damage was applied.
    pub tic: u32,
    /// Health points subtracted after armor.
    pub dealt: i32,
    /// Requested damage before armor / factors.
    pub raw: i32,
    /// Damaged player pawn.
    pub target: Option<ActorId>,
    /// `damage_mobj` source (monster that owns the attack), if any.
    pub source: Option<ActorId>,
    /// `damage_mobj` inflictor (missile / hitscan puff owner), if any.
    pub inflictor: Option<ActorId>,
    /// Class name of `source` (empty if none).
    pub source_class: String,
    /// Class name of `inflictor` (empty if none / same as source).
    pub inflictor_class: String,
    /// Human-readable cause (`hitscan`, `crush sector N`, …).
    pub cause: String,
}

/// Kind of recorded attack for HIT / shot debug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotKind {
    /// `P_LineAttack` / rail hitscan.
    Hitscan,
    /// Spawned projectile (`P_SpawnMissile`).
    Missile,
}

/// One hitscan or missile launch (ring-buffered on [`World`]).
#[derive(Clone, Debug)]
pub struct ShotTrace {
    /// Level tic.
    pub tic: u32,
    /// Hitscan vs missile spawn.
    pub kind: ShotKind,
    /// Shooter actor.
    pub source: ActorId,
    /// Shooter class name.
    pub source_class: String,
    /// Origin (map units).
    pub from: [f64; 3],
    /// Impact / aim end (map units).
    pub to: [f64; 3],
    /// Actor struck (hitscan) or intended target (missile), if any.
    pub hit: Option<ActorId>,
    /// Hit / target class name.
    pub hit_class: String,
    /// Damage for hitscan; 0 for missile spawn.
    pub damage: i32,
    /// Missile class name when `kind == Missile`.
    pub missile_class: String,
}

/// Per-sector runtime plane heights (static geometry stays in [`MapGeometry`]).
#[derive(Clone, Debug, PartialEq)]
pub struct SectorDynamic {
    /// Floor height.
    pub floor_z: f64,
    /// Ceiling height.
    pub ceiling_z: f64,
    /// Floor Z at previous tic start (for render lerp).
    pub prev_floor_z: f64,
    /// Ceiling Z at previous tic start.
    pub prev_ceiling_z: f64,
    /// Light level 0–255.
    pub light: i16,
    /// Boom `Transfer_Heights` fake floor (render / swim plane).
    pub fake_floor_z: Option<f64>,
    /// Boom `Transfer_Heights` fake ceiling.
    pub fake_ceiling_z: Option<f64>,
    /// Sector friction factor (Boom / `Sector_SetFriction`; default `ORIG_FRICTION`).
    pub friction: f64,
    /// Wind thrust per tic (`Sector_SetWind`).
    pub wind_x: f64,
    /// Wind thrust Y.
    pub wind_y: f64,
    /// Current thrust per tic (`Sector_SetCurrent`; floor-only).
    pub current_x: f64,
    /// Current thrust Y.
    pub current_y: f64,
    /// Boom sector type push/wind bit (`0x200`) — accept linedef wind/current.
    pub boom_push: bool,
    /// Doom `sector_t::soundtarget` — last noise emitter for `A_Look`.
    pub soundtarget: Option<ActorId>,
    /// ACS `SetSectorDamage` amount (0 = none).
    pub damage_amount: i32,
    /// ACS `SetSectorDamage` type name.
    pub damage_type: String,
    /// ACS `SetSectorDamage` interval in tics.
    pub damage_interval: i32,
    /// UDMF/ACS sector damage leakiness (0..=256; radsuit fail chance).
    pub leakiness: i32,
    /// Destructible floor health (`GetSectorHealth` floor part).
    pub health_floor: i32,
    /// Destructible ceiling health.
    pub health_ceiling: i32,
    /// Destructible 3D-floor health.
    pub health_3d: i32,
    /// ACS `SetSectorGlow` floor plane (RGB, height); `None` colour = off.
    pub glow_floor: Option<SectorPlaneGlow>,
    /// ACS `SetSectorGlow` ceiling plane.
    pub glow_ceiling: Option<SectorPlaneGlow>,
    /// ACS `SetFogDensity` (0–255).
    pub fog_density: u8,
    /// Floor plane alpha 0..=1 (`Sector_SetTranslucent`).
    pub floor_alpha: f32,
    /// Ceiling plane alpha 0..=1.
    pub ceiling_alpha: f32,
    /// Floor uses additive blend when translucent.
    pub floor_additive: bool,
    /// Ceiling uses additive blend when translucent.
    pub ceiling_additive: bool,
    /// `Sector_SetColor` RGB.
    pub color: [u8; 3],
    /// `Sector_SetColor` desaturation 0–255.
    pub color_desaturate: u8,
    /// `Sector_SetFade` RGB.
    pub fade: [u8; 3],
    /// Sector gravity multiplier (`Sector_SetGravity`; default 1.0).
    pub gravity: f64,
}

/// Plane glow from ACS `SetSectorGlow`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorPlaneGlow {
    /// RGB 0–255.
    pub color: [u8; 3],
    /// Glow height (map units).
    pub height: f32,
}

/// Playsim world (GZDoom `FLevelLocals` + thinkers subset).
pub struct World {
    /// Class registry (DECORATE / builtins).
    pub classes: Arc<ClassRegistry>,
    /// Compiled state table.
    pub states: Arc<StateTable>,
    /// Immutable map geometry.
    pub map: Arc<MapGeometry>,
    /// Level clocks / gravity.
    pub level: LevelState,
    /// Dynamic sector planes.
    pub sectors: Vec<SectorDynamic>,
    /// Actors.
    pub actors: ActorArena,
    /// Named RNG streams.
    pub rng: RngSet,
    /// Thinker lists.
    pub thinkers: ThinkerLists,
    /// Length of [`ThinkerLists::default`] after map-thing spawn (before mid-game
    /// missiles / drops). Doom runs levspec lights after these, before later spawns.
    map_thinker_count: usize,
    /// Player slots.
    pub players: [Option<Player>; MAX_PLAYERS],
    /// `player->refire` of the slot currently in `player_think` (slot is `take()`n).
    pub(crate) thinking_refire: i32,
    /// Active vertical door movers.
    pub doors: Vec<DoorMover>,
    /// Active floor movers.
    pub floors: Vec<FloorMover>,
    /// Active platform movers.
    pub plats: Vec<PlatMover>,
    /// Active ceiling / crusher movers.
    pub ceilings: Vec<CeilingMover>,
    /// Active floor / ceiling waggles.
    pub waggles: Vec<PlaneWaggle>,
    /// Runtime polyobjects (from map setup; movers in `polyobj`).
    pub polyobjs: Vec<crate::polyobj::PolyobjDynamic>,
    /// Active strobe lights.
    pub strobes: Vec<LightStrobe>,
    /// Active fire / blink flickers.
    pub flickers: Vec<crate::specials::LightFlicker>,
    /// Active light fades.
    pub fades: Vec<LightFade>,
    /// Active glow lights.
    pub glows: Vec<LightGlow>,
    /// Active floor scroll / carry thinkers.
    pub scrollers: Vec<FloorScroller>,
    /// Accumulated floor UV offsets (sector index → u/v).
    pub floor_scroll_offsets: std::collections::BTreeMap<u32, (f64, f64)>,
    /// Active ceiling flat scrollers.
    pub ceiling_scrollers: Vec<CeilingScroller>,
    /// Accumulated ceiling UV offsets (sector index → u/v).
    pub ceiling_scroll_offsets: std::collections::BTreeMap<u32, (f64, f64)>,
    /// Floor flat scale (sector → x/y; GZDoom inverted input).
    pub floor_scales: std::collections::BTreeMap<u32, (f64, f64)>,
    /// Ceiling flat scale (sector → x/y).
    pub ceiling_scales: std::collections::BTreeMap<u32, (f64, f64)>,
    /// Floor / ceiling flat rotation degrees (sector → (floor, ceiling)).
    pub plane_angles: std::collections::BTreeMap<u32, (f64, f64)>,
    /// Floor / ceiling plane reflection 0..=1 (`Sector_SetPlaneReflection`).
    pub plane_reflection: std::collections::BTreeMap<u32, (f32, f32)>,
    /// Runtime sector flags (`Sector_ChangeFlags`).
    pub sector_flags: std::collections::BTreeMap<u32, u32>,
    /// Runtime automap style per line (`Line_SetAutomapStyle`).
    pub line_automap_style: std::collections::BTreeMap<u32, i32>,
    /// Sector SNDSEQ type index (`Sector_ChangeSound`).
    pub sector_seq_type: std::collections::BTreeMap<u32, i32>,
    /// Boom `Transfer_Heights` links.
    pub height_transfers: Vec<HeightTransfer>,
    /// Boom `Transfer_FloorLight` links.
    pub floor_light_transfers: Vec<LightTransfer>,
    /// Boom `Transfer_CeilingLight` links.
    pub ceiling_light_transfers: Vec<LightTransfer>,
    /// Boom point push/pull sources (`PointPush_SetForce`).
    pub point_forces: Vec<crate::specials::PointForce>,
    /// Active wall texture scrollers.
    pub tex_scrollers: Vec<crate::specials::TextureScroller>,
    /// Runtime side texture offsets (side index → x/y).
    pub side_offsets: std::collections::BTreeMap<u32, (i16, i16)>,
    /// Runtime side texture scale (side index → x/y scale factors).
    pub side_scales: std::collections::BTreeMap<u32, (f32, f32)>,
    /// Active elevators.
    pub elevators: Vec<ElevatorMover>,
    /// Active screen-shake / damage quakes (`DEarthquake`).
    pub quakes: Vec<Quake>,
    /// Per-sector secret discovery flags.
    pub secret_found: Vec<bool>,
    /// Mid-texture overrides after switch flips (sidedef index → name).
    /// Overridden midtex after SW1→SW2 flip (side index → name).
    pub switch_mids: std::collections::BTreeMap<u32, String>,
    /// Overridden upper tex after switch flip.
    pub switch_uppers: std::collections::BTreeMap<u32, String>,
    /// Overridden lower tex after switch flip.
    pub switch_lowers: std::collections::BTreeMap<u32, String>,
    /// Runtime floor flat overrides (sector index → name).
    pub sector_floor_pics: std::collections::BTreeMap<u32, String>,
    /// Runtime ceiling flat overrides (ACS `ChangeCeiling`).
    pub sector_ceiling_pics: std::collections::BTreeMap<u32, String>,
    /// TERRAIN flat→splash registry (optional; filled by the shell).
    pub terrain: crate::terrain::TerrainTable,
    /// SNDSEQ registry (optional; filled by the shell).
    pub sndseq: crate::sndseq::SndSeqTable,
    /// Active SNDSEQ playback instances.
    pub active_sndseqs: Vec<crate::sndseq::ActiveSndSeq>,
    /// LOCKDEFS key requirements (optional; filled by the shell).
    pub locks: crate::locks::LockTable,
    /// KEYCONF weapon slots / player classes.
    pub weapon_slots: crate::weapon_slots::WeaponSlots,
    /// Runtime sector special overrides (sector index → special; 0 clears).
    pub sector_specials: std::collections::BTreeMap<u32, i16>,
    /// Runtime linedef special overrides (line index → special; 0 clears one-shots).
    pub line_specials: std::collections::BTreeMap<u32, i16>,
    /// Runtime linedef arg overrides (ACS `SetLineSpecial`).
    pub line_args: std::collections::BTreeMap<u32, [i32; 5]>,
    /// Runtime linedef activation (`SPAC_*`) overrides (ACS `SetLineActivation`).
    pub line_activations: std::collections::BTreeMap<u32, u32>,
    /// Runtime linedef flag overrides (`ML_REPEAT_SPECIAL`, ACS `SetLineBlocking`, …).
    pub line_flags: std::collections::BTreeMap<u32, i32>,
    /// Runtime mid-texture alpha 0..=1 (`TranslucentLine`).
    pub line_alpha: std::collections::BTreeMap<u32, f32>,
    /// Runtime additive mid-texture blend (`TranslucentLine` arg 3 / `ML_ADDTRANS`).
    pub line_additive: std::collections::BTreeMap<u32, bool>,
    /// Destructible linedef health (`GetLineHealth`; line index → HP).
    pub line_health: std::collections::BTreeMap<u32, i32>,
    /// Runtime line id overrides (`Line_SetIdentification`; line index → id).
    pub line_ids: std::collections::BTreeMap<u32, i32>,
    /// Runtime wall light levels (`Transfer_WallLight`; side index → light).
    pub side_lights: std::collections::BTreeMap<u32, i16>,
    /// Inventory bags keyed by owner actor.
    pub inventory: InventoryBags,
    /// Sound events produced this tic (drained by the game shell).
    pub sound_events: Vec<SimSoundEvent>,
    /// ACS `SetMusic` / `LocalSetMusic` request (lump or SNDINFO name; drained by shell).
    pub music_request: Option<String>,
    /// ACS `SetMusicVolume` (0..=1).
    pub music_volume: f32,
    /// ACS `FadeTo` / `CancelFade` screen overlay (RGBA 0..=1 + remaining tics).
    pub screen_fade: ScreenFade,
    /// ACS `SetFont` current font name (HUD / print bootstrap).
    pub acs_font: String,
    /// Decal events produced this tic (drained by the renderer / shell).
    pub decal_events: Vec<SimDecalEvent>,
    /// ACS `SpawnParticle` events this tic (drained by the shell / renderer).
    pub particle_events: Vec<SimParticleEvent>,
    /// ACS `SetHUDClipRect` (zero width disables).
    pub hud_clip: HudClipRect,
    /// ACS `SetHUDWrapWidth` (0 = default).
    pub hud_wrap_width: i32,
    /// ACS `SetCVarString` string store (name → value).
    pub acs_cvar_strings: std::collections::BTreeMap<String, String>,
    /// ACS `SetUserCVar` integer store (`player:name` → value).
    pub acs_user_cvars: std::collections::BTreeMap<String, i32>,
    /// ACS `SetUserCVarString` store (`player:name` → value).
    pub acs_user_cvar_strings: std::collections::BTreeMap<String, String>,
    /// ACS `StartSlideshow` request (consumed by shell / intermission).
    pub slideshow_request: Option<String>,
    /// Hexen `Autosave` special — shell should write an autosave when set.
    pub autosave_request: bool,
    /// `StartConversation` request — (talker, face_talker); consumed by UI shell.
    pub conversation_request: Option<(ActorId, bool)>,
    /// ACS `SetSubtitleNumber` (log index + optional sound name).
    pub subtitle_request: Option<(i32, String)>,
    /// Max ACS sector fog density seen this level (0–1 scale for renderer).
    pub fog_density: f32,
    /// `SetGlobalFogParameter` outside fog density (0–1).
    pub outside_fog_density: f32,
    /// `SetGlobalFogParameter` sky fog amount.
    pub sky_fog: f32,
    /// ACS `SetTeamScore` (team index → score).
    pub team_scores: std::collections::BTreeMap<i32, i32>,
    /// DECALDEF `generator` class → decal name (lowercase class keys).
    pub decal_generators: std::collections::BTreeMap<String, String>,
    /// ACS `SetCameraToTexture` bindings (texture name → camera actor + FOV).
    pub camera_textures: std::collections::BTreeMap<String, CameraTextureBind>,
    /// ACS `ChangeSky` sky1 override (empty = leave MAPINFO/default).
    pub sky1_override: Option<String>,
    /// ACS `ChangeSky` sky2 override.
    pub sky2_override: Option<String>,
    /// ACS `SetSkyScrollSpeed` sky1 scroll speed (map units / tic; render later).
    pub sky_speed1: f64,
    /// ACS `SetSkyScrollSpeed` sky2 scroll speed.
    pub sky_speed2: f64,
    /// ACS `SetSkyScrollSpeed` skymist scroll speed.
    pub sky_mist_speed: f64,
    /// HUD messages produced this tic (drained by the shell).
    pub messages: Vec<String>,
    /// Pause flag (menu / pause key).
    pub paused: bool,
    /// Doom skill 1..=5 (filters map things).
    pub skill: u8,
    /// Fast monsters (Nightmare or `-fast`); uses `FastSpeed` when set.
    pub fast_monsters: bool,
    /// Respawn `COUNTKILL` corpses (`-respawn` / Nightmare skill).
    pub respawn_monsters: bool,
    /// Round-robin index into live `BossTarget` actors for `A_BrainSpit`.
    pub boss_target_cursor: usize,
    /// Monster corpse FIFO (`sv_corpsequeuesize`, default 64). Oldest destroyed when full.
    pub corpse_queue: std::collections::VecDeque<ActorId>,
    /// Max retained monster corpses (`0` disables enqueue).
    pub corpse_queue_size: usize,
    /// Player body FIFO (vanilla `bodyque`, size 32).
    pub body_queue: std::collections::VecDeque<ActorId>,
    /// Max retained player corpses.
    pub body_queue_size: usize,
    /// Active `compatflags` / `compatflags2` (from `COMPLVL` / `compatmode`).
    pub compat: CompatFlags,
    /// UMAPINFO `bossaction` entries (class → line special + tag).
    pub boss_actions: Vec<BossAction>,
    /// Last damage applied to any player pawn (overwritten each hit; shell may `take`).
    pub last_damage: Option<LastDamageInfo>,
    /// Recent hitscans / missile launches (newest at back; capped).
    pub recent_shots: std::collections::VecDeque<ShotTrace>,
    /// Numeric strategy (`Double` default; `VanillaFixed` for strict demo sync).
    pub math_mode: SimMathMode,
    /// Classic `P_Random` / `M_Random` (used when [`SimMathMode::VanillaFixed`]).
    pub vanilla_rng: VanillaRng,
    /// ACS VM state (`BEHAVIOR` scripts for this map).
    pub acs: room_acs::AcsState,
    /// WASM play modules (M11 `room-script`).
    pub scripts: crate::script_host::ScriptHost,
}

/// One UMAPINFO `bossaction = Class, special, tag` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BossAction {
    /// Actor class name (case-insensitive match; stored lowercase).
    pub class: String,
    /// Hexen / ZDoom line special number.
    pub special: i32,
    /// Sector / line tag argument.
    pub tag: i32,
}

/// GZDoom `sv_corpsequeuesize` default.
pub const DEFAULT_CORPSE_QUEUE_SIZE: usize = 64;
/// Vanilla Doom `BODYQUESIZE`.
pub const DEFAULT_BODY_QUEUE_SIZE: usize = 32;

/// Active earthquake thinker (GZDoom `DEarthquake` subset).
#[derive(Clone, Debug)]
pub struct Quake {
    /// Origin actor (`m_Spot`).
    pub spot: ActorId,
    /// Intensity X/Y (Z unused for classic `A_Quake`).
    pub intensity: f64,
    /// Remaining tics.
    pub countdown: i32,
    /// Damage radius (map units).
    pub damage_radius: f64,
    /// Camera tremor radius (map units).
    pub tremor_radius: f64,
    /// Looping SFX name.
    pub sound: String,
    /// Thrust multiplier (classic `P_StartQuake` uses 0.5).
    pub thrust_multiplier: f64,
}

/// ACS full-screen colour fade (`FadeTo` / `CancelFade`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenFade {
    /// Current red 0..=1.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Current alpha 0..=1.
    pub a: f32,
    /// Target red.
    pub target_r: f32,
    /// Target green.
    pub target_g: f32,
    /// Target blue.
    pub target_b: f32,
    /// Target alpha.
    pub target_a: f32,
    /// Remaining blend tics (`0` = hold current / idle).
    pub tics_left: i32,
}

impl ScreenFade {
    /// Advance one tic toward the target colour.
    pub fn tick(&mut self) {
        if self.tics_left <= 0 {
            return;
        }
        let steps = self.tics_left as f32;
        self.r += (self.target_r - self.r) / steps;
        self.g += (self.target_g - self.g) / steps;
        self.b += (self.target_b - self.b) / steps;
        self.a += (self.target_a - self.a) / steps;
        self.tics_left -= 1;
        if self.tics_left == 0 {
            self.r = self.target_r;
            self.g = self.target_g;
            self.b = self.target_b;
            self.a = self.target_a;
        }
    }

    /// Snap to transparent and stop.
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
}

/// Lightweight sim sound request (mapped to `room-audio::SoundEvent` by the shell).
#[derive(Clone, Debug, PartialEq)]
pub struct SimSoundEvent {
    /// Logical sound name (SNDINFO key).
    pub name: String,
    /// World position.
    pub x: f64,
    /// Y.
    pub y: f64,
    /// Z.
    pub z: f64,
    /// Volume 0..=1.
    pub volume: f32,
    /// Attenuation scale (`1.0` = normal, `0.0` = full volume / no falloff).
    pub attenuation: f32,
}

/// Wall / floor decal request for the renderer (DECALDEF name).
#[derive(Clone, Debug, PartialEq)]
pub struct SimDecalEvent {
    /// DECALDEF decal or group name.
    pub name: String,
    /// World X.
    pub x: f64,
    /// Y.
    pub y: f64,
    /// Z.
    pub z: f64,
    /// Optional linedef index when known.
    pub line: Option<u32>,
}

/// ACS `SpawnParticle` request (render consumes later).
#[derive(Clone, Debug, PartialEq)]
pub struct SimParticleEvent {
    /// Packed RGB (`0xRRGGBB`).
    pub color: u32,
    /// Lifetime in tics.
    pub lifetime: i32,
    /// Size in map units.
    pub size: f64,
    /// World position.
    pub x: f64,
    /// Y.
    pub y: f64,
    /// Z.
    pub z: f64,
    /// Fullbright flag.
    pub fullbright: bool,
}

/// ACS `SetHUDClipRect` (HUD draw clip; shell applies).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HudClipRect {
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width (`0` = disabled).
    pub w: i32,
    /// Height.
    pub h: i32,
}

/// ACS `SetCameraToTexture` binding (render consumes later).
#[derive(Clone, Debug, PartialEq)]
pub struct CameraTextureBind {
    /// Viewpoint actor.
    pub camera: ActorId,
    /// Horizontal FOV in degrees.
    pub fov: f64,
}

/// Status-bar / HUD numbers for one player (shell-facing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerHud {
    /// Hit points.
    pub health: i32,
    /// Armor points.
    pub armor: i32,
    /// Ready ammo count (best-effort from Clip/Shell/RocketAmmo/Cell).
    pub ammo: i32,
    /// Max for the ready ammo type (backpack-aware).
    pub ammo_max: i32,
    /// Key bits: blue=1, yellow=2, red=4.
    pub keys: u8,
    /// Mugshot lump name.
    pub face: String,
    /// Ready weapon patch lump (`PISGA0`, …).
    pub weapon_patch: String,
    /// Muzzle flash patch (`PISFA0`, …); empty when inactive.
    pub flash_patch: String,
    /// Weapon bob X offset (screen pixels, Doom 320 space).
    pub bob_x: f32,
    /// Weapon bob Y offset.
    pub bob_y: f32,
    /// God mode active.
    pub god: bool,
}

/// Doom ready-frame patch for a weapon class (mirrors `room_ui::weapon_ready_patch`).
fn weapon_patch_for_class(class_name: &str) -> &'static str {
    match class_name {
        "Fist" => "PUNGA0",
        "Chainsaw" => "SAWGA0",
        "Pistol" => "PISGA0",
        "Shotgun" => "SHTGA0",
        "SuperShotgun" => "SGN2A0",
        "Chaingun" => "CHGGA0",
        "RocketLauncher" => "MISGA0",
        "PlasmaRifle" => "PLSGA0",
        "BFG9000" => "BFGGA0",
        _ => "PISGA0",
    }
}

/// Build a Doom sprite lump name from psprite sprite + frame (`PISG`/`B` → `PISGB0`).
fn weapon_patch_from_sprite_frame(sprite: &str, frame: u8) -> String {
    let s = sprite.trim().to_ascii_uppercase();
    if s.is_empty() {
        return String::new();
    }
    let letter = char::from(b'A' + frame.min(25));
    format!("{s}{letter}0")
}

/// ZDoom `ML_REPEAT_SPECIAL` — keep special after successful activation.
pub const ML_REPEAT_SPECIAL: i32 = 0x0200;
/// Doom / ZDoom `ML_BLOCKING`.
pub const ML_BLOCKING: i32 = 0x0001;
/// Doom `ML_BLOCKMONSTERS`.
pub const ML_BLOCKMONSTERS: i32 = 0x0002;
/// ZDoom `ML_BLOCK_PLAYERS`.
pub const ML_BLOCK_PLAYERS: i32 = 0x0000_4000;
/// ZDoom `ML_BLOCKEVERYTHING`.
pub const ML_BLOCKEVERYTHING: i32 = 0x0000_8000;
/// ZDoom `ML_RAILING`.
pub const ML_RAILING: i32 = 0x0002_0000;
/// Doom `ML_SECRET`.
pub const ML_SECRET: i32 = 0x0020;
/// Doom `ML_DONTDRAW`.
pub const ML_DONTDRAW: i32 = 0x0080;
/// Doom `ML_MAPPED`.
pub const ML_MAPPED: i32 = 0x0100;
/// ZDoom `ML_REVEALED` (automap revealed).
pub const ML_REVEALED: i32 = 0x0001_0000;
/// Runtime `Line_Horizon` marker (not a vanilla ML bit).
pub const ML_HORIZON: i32 = 0x4000_0000;

/// `Line_SetBlocking` `BLOCKF_*` bits (ACS / special args).
pub mod blockf {
    /// Blocks players and monsters (`ML_BLOCKING`).
    pub const CREATURES: i32 = 1;
    /// Blocks monsters (`ML_BLOCKMONSTERS`).
    pub const MONSTERS: i32 = 2;
    /// Blocks players (`ML_BLOCK_PLAYERS`).
    pub const PLAYERS: i32 = 4;
    /// Blocks everything (`ML_BLOCKEVERYTHING`).
    pub const EVERYTHING: i32 = 8;
    /// Railing (`ML_RAILING`).
    pub const RAILING: i32 = 32;
}

/// ACS `SetLineBlocking` blocking modes.
pub mod block_mode {
    /// Clear player/creature/everything/railing block bits.
    pub const NOTHING: i32 = 0;
    /// `ML_BLOCKING` only (default).
    pub const CREATURES: i32 = 1;
    /// `ML_BLOCKING | ML_BLOCKEVERYTHING`.
    pub const EVERYTHING: i32 = 2;
    /// `ML_BLOCKING | ML_RAILING`.
    pub const RAILING: i32 = 3;
    /// `ML_BLOCK_PLAYERS` only.
    pub const PLAYERS: i32 = 4;
}

impl World {
    /// Cap for [`Self::recent_shots`].
    const MAX_RECENT_SHOTS: usize = 48;

    /// Run `f` with `player` temporarily restored into an empty `players` slot.
    ///
    /// `World::tick` / `player_think` take the slot out so they can mutate both
    /// `World` and `Player`. Callers that need `players[].mo` matching (use-line
    /// activation, pickups) must park the in-hand player first.
    pub(crate) fn with_parked_player<R>(
        &mut self,
        player: &mut Player,
        f: impl FnOnce(&mut World) -> R,
    ) -> R {
        let slot = (0..MAX_PLAYERS).find(|&i| self.players[i].is_none());
        if let Some(i) = slot {
            self.players[i] = Some(std::mem::take(player));
            let r = f(self);
            *player = self.players[i].take().unwrap_or_default();
            r
        } else {
            f(self)
        }
    }

    /// Record a hitscan / missile for HIT debug (newest at back).
    pub fn push_shot_trace(&mut self, shot: ShotTrace) {
        // Terminal trace in play builds so player↔monster shots are visible without HIT.
        if !cfg!(test) {
            let kind = match shot.kind {
                ShotKind::Hitscan => "hitscan",
                ShotKind::Missile => "missile",
            };
            let src_seat = self
                .players
                .iter()
                .enumerate()
                .find(|(_, p)| p.as_ref().and_then(|pl| pl.mo) == Some(shot.source))
                .map(|(i, _)| format!("p{i}"))
                .unwrap_or_else(|| "ai".into());
            let dst = if !shot.hit_class.is_empty() {
                format!(
                    "{}#{}",
                    shot.hit_class,
                    shot.hit.map(|a| a.index()).unwrap_or(0)
                )
            } else if shot.hit.is_some() {
                format!("actor#{}", shot.hit.map(|a| a.index()).unwrap_or(0))
            } else if shot.kind == ShotKind::Missile {
                "aim".into()
            } else {
                "wall/miss".into()
            };
            let dmg = if shot.damage > 0 {
                format!(" dmg={}", shot.damage)
            } else {
                String::new()
            };
            let missile = if shot.missile_class.is_empty() {
                String::new()
            } else {
                format!(" {}", shot.missile_class)
            };
            let dist = {
                let dx = shot.to[0] - shot.from[0];
                let dy = shot.to[1] - shot.from[1];
                let dz = shot.to[2] - shot.from[2];
                (dx * dx + dy * dy + dz * dz).sqrt()
            };
            eprintln!(
                "room: SHOOT @{tic} {kind}{missile} {src_seat} {}#{} ({:.0},{:.0},{:.0}) → {dst} ({:.0},{:.0},{:.0}) dist={dist:.0}{dmg}",
                shot.source_class,
                shot.source.index(),
                shot.from[0],
                shot.from[1],
                shot.from[2],
                shot.to[0],
                shot.to[1],
                shot.to[2],
                tic = shot.tic,
            );
        }
        self.recent_shots.push_back(shot);
        while self.recent_shots.len() > Self::MAX_RECENT_SHOTS {
            self.recent_shots.pop_front();
        }
    }

    /// Effective linedef special (runtime override or map static).
    #[must_use]
    pub fn line_special(&self, line_id: room_core::LineId) -> i16 {
        let idx = line_id.raw();
        if let Some(&s) = self.line_specials.get(&idx) {
            return s;
        }
        self.map
            .lines
            .get(idx as usize)
            .map(|l| l.special)
            .unwrap_or(0)
    }

    /// Effective linedef activation mask (`SPAC_*`).
    #[must_use]
    pub fn line_activation(&self, line_id: room_core::LineId) -> u32 {
        let idx = line_id.raw();
        if let Some(&a) = self.line_activations.get(&idx) {
            return a;
        }
        self.map
            .lines
            .get(idx as usize)
            .map(|l| l.activation)
            .unwrap_or(0)
    }

    /// Effective linedef flags (runtime override or map static).
    #[must_use]
    pub fn line_flags(&self, line_id: room_core::LineId) -> i32 {
        let idx = line_id.raw();
        if let Some(&f) = self.line_flags.get(&idx) {
            return f;
        }
        self.map
            .lines
            .get(idx as usize)
            .map(|l| i32::from(l.flags))
            .unwrap_or(0)
    }

    /// Effective linedef args (runtime override or map static).
    #[must_use]
    pub fn line_args(&self, line_id: room_core::LineId) -> [i32; 5] {
        let idx = line_id.raw();
        if let Some(&a) = self.line_args.get(&idx) {
            return a;
        }
        self.map
            .lines
            .get(idx as usize)
            .map(|l| l.args)
            .unwrap_or([0; 5])
    }

    /// Build dynamic state from linked map geometry and an empty class registry.
    pub fn from_map(map: MapGeometry) -> Result<Self, SimError> {
        Self::from_map_with_classes(
            map,
            Arc::new(ClassRegistry::new()),
            Arc::new(StateTable::new()),
        )
    }

    /// Build dynamic state, spawning things that resolve via `classes` (skill 3).
    pub fn from_map_with_classes(
        map: MapGeometry,
        classes: Arc<ClassRegistry>,
        states: Arc<StateTable>,
    ) -> Result<Self, SimError> {
        Self::from_map_with_skill(map, classes, states, 3)
    }

    /// Build dynamic state at a Doom skill level (1..=5).
    pub fn from_map_with_skill(
        map: MapGeometry,
        classes: Arc<ClassRegistry>,
        states: Arc<StateTable>,
        skill: u8,
    ) -> Result<Self, SimError> {
        Self::from_map_with_skill_mode(map, classes, states, skill, SimMathMode::Double)
    }

    /// Like [`from_map_with_skill`], with an explicit math mode so vanilla demo
    /// `P_Random` runs during map-thing / light spawn (Doom: things then specials).
    pub fn from_map_with_skill_mode(
        map: MapGeometry,
        classes: Arc<ClassRegistry>,
        states: Arc<StateTable>,
        skill: u8,
        math_mode: SimMathMode,
    ) -> Result<Self, SimError> {
        let sectors: Vec<SectorDynamic> = map
            .sectors
            .iter()
            .map(|s| SectorDynamic {
                floor_z: s.floor_height,
                ceiling_z: s.ceiling_height,
                prev_floor_z: s.floor_height,
                prev_ceiling_z: s.ceiling_height,
                light: s.light,
                fake_floor_z: None,
                fake_ceiling_z: None,
                // Vanilla `ORIG_FRICTION` (~0xE800 / 65536); UDMF `friction` overrides.
                friction: if s.friction != 0 {
                    boom_friction_factor(s.friction)
                } else {
                    0.90625
                },
                wind_x: 0.0,
                wind_y: 0.0,
                current_x: 0.0,
                current_y: 0.0,
                boom_push: false,
                soundtarget: None,
                damage_amount: s.damage_amount,
                damage_type: s.damage_type.clone(),
                damage_interval: if s.damage_interval > 0 {
                    s.damage_interval
                } else {
                    32
                },
                leakiness: s.leakiness.clamp(0, 256),
                health_floor: s.health_floor.max(0),
                health_ceiling: s.health_ceiling.max(0),
                health_3d: s.health_3d.max(0),
                glow_floor: None,
                glow_ceiling: None,
                fog_density: s.fog_density.clamp(0, 255) as u8,
                floor_alpha: s.floor_alpha.clamp(0.0, 1.0),
                ceiling_alpha: s.ceiling_alpha.clamp(0.0, 1.0),
                floor_additive: s.floor_additive,
                ceiling_additive: s.ceiling_additive,
                color: rgb_from_udmf(s.light_color, [255, 255, 255]),
                color_desaturate: s.desaturation.clamp(0, 255) as u8,
                fade: rgb_from_udmf(s.fade_color, [0, 0, 0]),
                gravity: if s.gravity >= 0.0 { s.gravity } else { 1.0 },
            })
            .collect();
        let sector_count = sectors.len();
        let mut max_fog = 0.0f32;
        for s in &sectors {
            if s.fog_density > 0 {
                max_fog = max_fog.max(f32::from(s.fog_density) / 255.0);
            }
        }
        let mut line_alpha = std::collections::BTreeMap::new();
        let mut line_additive = std::collections::BTreeMap::new();
        let mut line_health = std::collections::BTreeMap::new();
        let mut line_automap_style = std::collections::BTreeMap::new();
        for (i, ld) in map.lines.iter().enumerate() {
            if (ld.mid_alpha - 1.0).abs() > 1e-6 {
                line_alpha.insert(i as u32, ld.mid_alpha);
            }
            if ld.mid_additive {
                line_additive.insert(i as u32, true);
            }
            if ld.health != 0 {
                line_health.insert(i as u32, ld.health);
            }
            if ld.automap_style != 0 {
                line_automap_style.insert(i as u32, ld.automap_style);
            }
        }
        let mut rng = RngSet::new();
        rng.set_seed(1993);
        rng.get_or_insert("world")
            .map_err(|e| SimError::Message(e.to_string()))?;
        let mut world = Self {
            classes,
            states,
            map: Arc::new(map),
            level: LevelState {
                gravity: 1.0,
                ..LevelState::default()
            },
            sectors,
            actors: ActorArena::new(),
            rng,
            thinkers: ThinkerLists::default(),
            map_thinker_count: 0,
            players: std::array::from_fn(|_| None),
            thinking_refire: 0,
            doors: Vec::new(),
            floors: Vec::new(),
            plats: Vec::new(),
            ceilings: Vec::new(),
            waggles: Vec::new(),
            polyobjs: Vec::new(),
            strobes: Vec::new(),
            flickers: Vec::new(),
            fades: Vec::new(),
            glows: Vec::new(),
            scrollers: Vec::new(),
            floor_scroll_offsets: std::collections::BTreeMap::new(),
            ceiling_scrollers: Vec::new(),
            ceiling_scroll_offsets: std::collections::BTreeMap::new(),
            floor_scales: std::collections::BTreeMap::new(),
            ceiling_scales: std::collections::BTreeMap::new(),
            plane_angles: std::collections::BTreeMap::new(),
            plane_reflection: std::collections::BTreeMap::new(),
            sector_flags: std::collections::BTreeMap::new(),
            line_automap_style,
            sector_seq_type: std::collections::BTreeMap::new(),
            height_transfers: Vec::new(),
            floor_light_transfers: Vec::new(),
            ceiling_light_transfers: Vec::new(),
            point_forces: Vec::new(),
            tex_scrollers: Vec::new(),
            side_offsets: std::collections::BTreeMap::new(),
            side_scales: std::collections::BTreeMap::new(),
            elevators: Vec::new(),
            quakes: Vec::new(),
            secret_found: vec![false; sector_count],
            switch_mids: std::collections::BTreeMap::new(),
            switch_uppers: std::collections::BTreeMap::new(),
            switch_lowers: std::collections::BTreeMap::new(),
            sector_floor_pics: std::collections::BTreeMap::new(),
            sector_ceiling_pics: std::collections::BTreeMap::new(),
            terrain: crate::terrain::TerrainTable::default(),
            sndseq: crate::sndseq::SndSeqTable::default(),
            active_sndseqs: Vec::new(),
            locks: crate::locks::LockTable::default(),
            weapon_slots: crate::weapon_slots::WeaponSlots::with_doom_defaults(),
            sector_specials: std::collections::BTreeMap::new(),
            line_specials: std::collections::BTreeMap::new(),
            line_args: std::collections::BTreeMap::new(),
            line_activations: std::collections::BTreeMap::new(),
            line_flags: std::collections::BTreeMap::new(),
            line_alpha,
            line_additive,
            line_health,
            line_ids: std::collections::BTreeMap::new(),
            side_lights: std::collections::BTreeMap::new(),
            inventory: InventoryBags::new(),
            sound_events: Vec::new(),
            music_request: None,
            music_volume: 1.0,
            screen_fade: ScreenFade::default(),
            acs_font: String::new(),
            decal_events: Vec::new(),
            particle_events: Vec::new(),
            hud_clip: HudClipRect::default(),
            hud_wrap_width: 0,
            acs_cvar_strings: std::collections::BTreeMap::new(),
            acs_user_cvars: std::collections::BTreeMap::new(),
            acs_user_cvar_strings: std::collections::BTreeMap::new(),
            slideshow_request: None,
            autosave_request: false,
            conversation_request: None,
            subtitle_request: None,
            fog_density: max_fog,
            outside_fog_density: 0.0,
            sky_fog: 0.0,
            team_scores: std::collections::BTreeMap::new(),
            decal_generators: std::collections::BTreeMap::new(),
            camera_textures: std::collections::BTreeMap::new(),
            sky1_override: None,
            sky2_override: None,
            sky_speed1: 0.0,
            sky_speed2: 0.0,
            sky_mist_speed: 0.0,
            messages: Vec::new(),
            paused: false,
            skill: skill.clamp(1, 5),
            fast_monsters: skill >= 5,
            respawn_monsters: skill >= 5,
            boss_target_cursor: 0,
            corpse_queue: std::collections::VecDeque::new(),
            corpse_queue_size: DEFAULT_CORPSE_QUEUE_SIZE,
            body_queue: std::collections::VecDeque::new(),
            body_queue_size: DEFAULT_BODY_QUEUE_SIZE,
            compat: CompatFlags::default(),
            boss_actions: Vec::new(),
            last_damage: None,
            recent_shots: std::collections::VecDeque::new(),
            math_mode,
            acs: room_acs::AcsState::new(),
            scripts: crate::script_host::ScriptHost::new(),
            vanilla_rng: VanillaRng::new(),
        };
        if math_mode == SimMathMode::VanillaFixed {
            // Doom `M_ClearRandom` before `P_LoadThings` / `P_SpawnSpecials`.
            world.vanilla_rng.clear();
        }
        // Doom: secrets/doors prep, then map things (`P_Random` lastlook/tics), then lights.
        crate::sector_specials::init_sector_specials(&mut world);
        crate::specials::init_line_scrollers(&mut world);
        crate::specials::init_udmf_side_scrollers(&mut world);
        crate::specials::init_transfers(&mut world);
        world.spawn_map_things()?;
        world.map_thinker_count = world.thinkers.default.len();
        crate::sector_specials::spawn_sector_light_thinkers(&mut world);
        crate::specials::init_point_pushers(&mut world);
        crate::polyobj::init_polyobjs(&mut world);
        crate::acs::init_acs(&mut world);
        Ok(world)
    }

    /// Merge WASM `reg_class_*` results into [`Self::classes`] (M11).
    ///
    /// # Errors
    ///
    /// Script / class registry failures.
    pub fn apply_script_classes(&mut self) -> Result<(), SimError> {
        let mut reg = (*self.classes).clone();
        self.scripts
            .merge_into_registry(&mut reg)
            .map_err(|e| SimError::Message(e.to_string()))?;
        self.classes = Arc::new(reg);
        Ok(())
    }

    fn spawn_map_things(&mut self) -> Result<(), SimError> {
        let things = self.map.things.clone();
        for thing in &things {
            // Skill filter (MTF_EASY=1, MTF_NORMAL=2, MTF_HARD=4). Player starts always spawn.
            if thing.type_id != THING_PLAYER1_START && !thing_fits_skill(thing.flags, self.skill) {
                continue;
            }
            // MTF_NOTSINGLE (16): skip in single-player bootstrap.
            if thing.type_id != THING_PLAYER1_START && (thing.flags & 16) != 0 {
                continue;
            }
            // Player 2–4 / deathmatch starts are spawn spots only (vanilla); coop
            // players are created in `spawn_coop_players`. Spawning the marker
            // actors (TNT1) left invisible thinkers that confused sprite probes.
            if thing.type_id == THING_PLAYER2_START
                || thing.type_id == THING_PLAYER3_START
                || thing.type_id == THING_PLAYER4_START
                || thing.type_id == 11
            {
                continue;
            }
            if thing.type_id == THING_PLAYER1_START {
                let class = self
                    .classes
                    .find_name("PlayerPawn")
                    .or_else(|| self.classes.find_name("Actor"))
                    .unwrap_or(ClassId::from_raw(0));
                let id = self.spawn_at(
                    class,
                    DVec3::new(f64::from(thing.x), f64::from(thing.y), f64::from(thing.z)),
                    doom_thing_angle(thing.angle),
                )?;
                self.randomize_mapthing_tics(id);
                if self.players[0].is_none() {
                    let mut p = Player::default();
                    p.possess(id, class);
                    if let Some(vh) = self
                        .classes
                        .get(class)
                        .map(|c| c.view_height)
                        .filter(|&v| v > 0.0)
                    {
                        p.view_height = vh;
                    }
                    if let Some(a) = self.actors.get(id) {
                        p.view_z = a.pos.z() + p.view_height;
                    }
                    self.players[0] = Some(p);
                    self.give_starting_loadout(id);
                }
                continue;
            }
            let Some(class) = self.classes.find_doomednum(i32::from(thing.type_id)) else {
                continue;
            };
            let id = self.spawn_at(
                class,
                DVec3::new(f64::from(thing.x), f64::from(thing.y), f64::from(thing.z)),
                doom_thing_angle(thing.angle),
            )?;
            self.randomize_mapthing_tics(id);
            // Doom `MTF_AMBUSH` (8): deaf — ignore sound wake.
            if (thing.flags & 8) != 0 {
                if let Some(a) = self.actors.get_mut(id) {
                    a.flags |= crate::class::FLAG_AMBUSH;
                }
            }
            // Hexen/UDMF dormant (32): skip thinker until woken.
            // (Doom-format bit 5 is MTF_NOTDM — only honour dormant when Hexen/UDMF.)
            if thing.hexen && (thing.flags & 32) != 0 {
                if let Some(a) = self.actors.get_mut(id) {
                    a.flags |= crate::class::FLAG_DORMANT;
                }
            }
            // MBF `MTF_FRIEND` / UDMF `friend` (128).
            if (thing.flags & 128) != 0 {
                if let Some(a) = self.actors.get_mut(id) {
                    a.flags |= crate::class::FLAG_FRIENDLY;
                }
            }
            if let Some(a) = self.actors.get_mut(id) {
                a.special = u16::from(thing.special);
                a.args = thing.args;
                a.tid = i32::from(thing.tid);
                if thing.friendly_see_blocks >= 0 {
                    a.friendly_see_blocks = i32::from(thing.friendly_see_blocks);
                }
                if thing.health > 0 {
                    a.health = thing.health;
                }
                if thing.score != i32::MIN {
                    a.score = thing.score;
                }
                if thing.gravity >= 0.0 {
                    a.gravity = thing.gravity;
                }
                if thing.scale_x > 0.0 || thing.scale_y > 0.0 {
                    if let Some(def) = self.classes.get(a.class) {
                        let sx = if thing.scale_x > 0.0 {
                            thing.scale_x
                        } else {
                            def.scale_x
                        };
                        let sy = if thing.scale_y > 0.0 {
                            thing.scale_y
                        } else {
                            def.scale_y
                        };
                        a.scale_x = sx;
                        a.scale_y = sy;
                        a.radius = def.radius * sx;
                        a.height = def.height * sy;
                    }
                }
                if thing.alpha >= 0.0 {
                    a.alpha = thing.alpha;
                }
                if thing.pitch != i16::MIN {
                    a.pitch = Angle::from_degrees(f64::from(thing.pitch));
                }
                if thing.roll != i16::MIN {
                    a.roll = Angle::from_degrees(f64::from(thing.roll));
                }
                if thing.fill_color >= 0 {
                    let rgb = thing.fill_color as u32;
                    a.fill_color = Some([
                        ((rgb >> 16) & 0xff) as u8,
                        ((rgb >> 8) & 0xff) as u8,
                        (rgb & 0xff) as u8,
                    ]);
                }
                if !thing.render_style.is_empty() {
                    if let Some(rs) = crate::class::RenderStyle::parse(&thing.render_style) {
                        a.render_style = rs;
                    }
                }
                if thing.float_bob_phase >= 0 {
                    a.float_bob_phase = (thing.float_bob_phase as u8) & 63;
                }
                if thing.count_secret {
                    a.count_secret = true;
                    self.level.total_secrets = self.level.total_secrets.saturating_add(1);
                }
                if thing.no_count {
                    let had_kill = (a.flags & crate::class::FLAG_COUNTKILL) != 0;
                    let had_item = (a.flags & crate::class::FLAG_COUNTITEM) != 0;
                    a.flags &= !(crate::class::FLAG_COUNTKILL | crate::class::FLAG_COUNTITEM);
                    if had_kill {
                        self.level.total_kills = self.level.total_kills.saturating_sub(1);
                    }
                    if had_item {
                        self.level.total_items = self.level.total_items.saturating_sub(1);
                    }
                }
                if !thing.arg0_str.is_empty() {
                    a.arg0_str = thing.arg0_str.clone();
                }
                if thing.conversation != 0 {
                    a.conversation = thing.conversation;
                }
                if thing.accuracy != i32::MIN {
                    a.accuracy = thing.accuracy;
                }
                if thing.stamina != i32::MIN {
                    a.stamina = thing.stamina;
                }
                if !thing.species.is_empty() {
                    a.species = thing.species.clone();
                }
                if thing.melee_range >= 0.0 {
                    a.melee_range = thing.melee_range;
                }
                if thing.view_height >= 0.0 {
                    a.view_height = thing.view_height;
                }
                if thing.mass != i32::MIN {
                    a.mass = thing.mass;
                }
                if thing.speed >= 0.0 {
                    a.speed = thing.speed;
                }
                if thing.friction >= 0.0 {
                    a.friction = thing.friction;
                }
                if thing.damage_factor >= 0.0 {
                    a.damage_factor = thing.damage_factor;
                }
                if thing.max_step_height >= 0.0 {
                    a.max_step_height = thing.max_step_height;
                }
                if thing.max_dropoff_height >= 0.0 {
                    a.max_dropoff_height = thing.max_dropoff_height;
                }
                if thing.float_speed >= 0.0 {
                    a.float_speed = thing.float_speed;
                }
                if thing.fast_speed >= 0.0 {
                    a.fast_speed = thing.fast_speed;
                }
                if thing.jump_z >= 0.0 {
                    a.jump_z = thing.jump_z;
                }
                if thing.attack_z_offset >= 0.0 {
                    a.attack_z_offset = thing.attack_z_offset;
                }
            }
        }
        Ok(())
    }

    /// Spawn coop players 2..=N at Player 2–4 starts (`total` clamped 1..=4).
    ///
    /// Player 1 is already created in [`Self::spawn_map_things`]. Missing starts are skipped.
    /// Also enables multiplayer-only map things (`MTF_NOTSINGLE`) when `total > 1`.
    pub fn spawn_coop_players(&mut self, total: u8) -> Result<u8, SimError> {
        let total = total.clamp(1, 4) as usize;
        if total <= 1 {
            return Ok(1);
        }
        self.spawn_multiplayer_only_things()?;
        let class = self
            .classes
            .find_name("PlayerPawn")
            .or_else(|| self.classes.find_name("Actor"))
            .unwrap_or(ClassId::from_raw(0));
        let starts: [i16; 3] = [
            THING_PLAYER2_START,
            THING_PLAYER3_START,
            THING_PLAYER4_START,
        ];
        let mut alive = 1u8;
        for (slot, &tid) in starts.iter().enumerate().take(total - 1) {
            let seat = slot + 1;
            if self.players[seat].is_some() {
                alive = alive.saturating_add(1);
                continue;
            }
            let Some(thing) = self.map.things.iter().find(|t| t.type_id == tid).cloned() else {
                continue;
            };
            let id = self.spawn_at(
                class,
                DVec3::new(f64::from(thing.x), f64::from(thing.y), f64::from(thing.z)),
                doom_thing_angle(thing.angle),
            )?;
            self.randomize_mapthing_tics(id);
            let mut p = Player::default();
            p.possess(id, class);
            if let Some(vh) = self
                .classes
                .get(class)
                .map(|c| c.view_height)
                .filter(|&v| v > 0.0)
            {
                p.view_height = vh;
            }
            if let Some(a) = self.actors.get(id) {
                p.view_z = a.pos.z() + p.view_height;
            }
            self.players[seat] = Some(p);
            self.give_starting_loadout(id);
            alive = alive.saturating_add(1);
        }
        Ok(alive)
    }

    /// Spawn map things flagged `MTF_NOTSINGLE` (bit 16) once multiplayer is active.
    fn spawn_multiplayer_only_things(&mut self) -> Result<(), SimError> {
        let things = self.map.things.clone();
        for thing in &things {
            if (thing.flags & 16) == 0 {
                continue;
            }
            if thing.type_id == THING_PLAYER1_START
                || thing.type_id == THING_PLAYER2_START
                || thing.type_id == THING_PLAYER3_START
                || thing.type_id == THING_PLAYER4_START
            {
                continue;
            }
            if !thing_fits_skill(thing.flags, self.skill) {
                continue;
            }
            let Some(class) = self.classes.find_doomednum(i32::from(thing.type_id)) else {
                continue;
            };
            let id = self.spawn_at(
                class,
                DVec3::new(f64::from(thing.x), f64::from(thing.y), f64::from(thing.z)),
                doom_thing_angle(thing.angle),
            )?;
            self.randomize_mapthing_tics(id);
            if (thing.flags & 8) != 0 {
                if let Some(a) = self.actors.get_mut(id) {
                    a.flags |= crate::class::FLAG_AMBUSH;
                }
            }
            if (thing.flags & 128) != 0 {
                if let Some(a) = self.actors.get_mut(id) {
                    a.flags |= crate::class::FLAG_FRIENDLY;
                }
            }
        }
        Ok(())
    }

    /// Spawn MBF helper dogs at Player 2–4 starts (`-dogs N`, clamped 0..=3).
    ///
    /// Dogs are always friendly. Missing starts or missing `MBFHelperDog` class → skipped.
    pub fn spawn_helper_dogs(&mut self, count: u8) -> Result<u8, SimError> {
        let n = count.min(3) as usize;
        if n == 0 {
            return Ok(0);
        }
        let Some(class) = self
            .classes
            .find_doomednum(THING_MBF_HELPER_DOG)
            .or_else(|| self.classes.find_name("MBFHelperDog"))
        else {
            return Ok(0);
        };
        let starts: [i16; 3] = [
            THING_PLAYER2_START,
            THING_PLAYER3_START,
            THING_PLAYER4_START,
        ];
        let mut spawned = 0u8;
        for &tid in starts.iter().take(n) {
            let Some(thing) = self.map.things.iter().find(|t| t.type_id == tid).cloned() else {
                continue;
            };
            let id = self.spawn_at(
                class,
                DVec3::new(f64::from(thing.x), f64::from(thing.y), f64::from(thing.z)),
                doom_thing_angle(thing.angle),
            )?;
            self.randomize_mapthing_tics(id);
            if let Some(a) = self.actors.get_mut(id) {
                a.flags |= crate::class::FLAG_FRIENDLY;
            }
            spawned += 1;
        }
        Ok(spawned)
    }

    /// Respawn a dead player at the first Player 1 start (keeps the same pawn id).
    pub fn respawn_player_at(&mut self, player: &mut Player) -> bool {
        if player.player_state != crate::player::PlayerState::Dead {
            return false;
        }
        let Some(mo) = player.mo else {
            return false;
        };
        let Some(start) = self
            .map
            .things
            .iter()
            .find(|t| t.type_id == THING_PLAYER1_START)
            .cloned()
        else {
            return false;
        };
        let class = player.cls;
        let health = self.classes.get(class).map(|c| c.health).unwrap_or(100);
        let (radius, height, flags) = self
            .classes
            .get(class)
            .map(|c| (c.radius, c.height, c.flags))
            .unwrap_or((
                16.0,
                56.0,
                crate::class::FLAG_SOLID | crate::class::FLAG_SHOOTABLE,
            ));
        let spawn = self.states.find_label(class, "Spawn");
        if let Some(a) = self.actors.get_mut(mo) {
            a.pos = DVec3::new(f64::from(start.x), f64::from(start.y), f64::from(start.z));
            a.angle = doom_thing_angle(start.angle);
            a.vel = DVec3::ZERO;
            a.health = health;
            a.radius = radius;
            a.height = height;
            a.flags = flags;
            a.destroyed = false;
            a.target = None;
        }
        if let Some(spawn) = spawn {
            self.set_state(mo, Some(spawn), true);
        }
        self.inventory.clear(mo);
        let view_h = self
            .classes
            .get(class)
            .map(|c| c.view_height)
            .filter(|&v| v > 0.0)
            .unwrap_or(41.0);
        player.possess(mo, class);
        player.view_height = view_h;
        if let Some(a) = self.actors.get(mo) {
            player.view_z = a.pos.z() + view_h;
        }
        self.give_starting_loadout(mo);
        // Re-apply ready weapon after loadout (give_starting_loadout may set players[0]).
        if let Some(pistol) = self.classes.find_name("Pistol") {
            player.ready_weapon = Some(pistol);
        }
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_TOP;
        true
    }

    /// Doom starting kit: Fist + Pistol + a clip of bullets (when those classes exist).
    fn give_starting_loadout(&mut self, mo: ActorId) {
        let fist = self.classes.find_name("Fist");
        let pistol = self.classes.find_name("Pistol");
        if fist.is_none() && pistol.is_none() {
            return;
        }
        if let Some(fist) = fist {
            let _ = crate::inventory::give_inventory(self, mo, fist, 1);
        }
        if let Some(pistol) = pistol {
            let _ = crate::inventory::give_inventory(self, mo, pistol, 1);
            // Bind ready weapon to the seat that owns `mo` (not always players[0]).
            if let Some(p) = self.players.iter_mut().flatten().find(|p| p.mo == Some(mo)) {
                p.ready_weapon = Some(pistol);
                p.pspr_state = None;
                p.pspr_y = crate::player::WEAPON_TOP;
            }
        }
        if let Some(clip) = self.classes.find_name("Clip") {
            let bullets = self
                .players
                .iter()
                .flatten()
                .find(|p| p.mo == Some(mo))
                .and_then(|p| self.classes.get(p.cls))
                .map(|c| c.start_bullets)
                .filter(|&n| n > 0)
                .or_else(|| {
                    self.classes
                        .find_name("PlayerPawn")
                        .or_else(|| self.classes.find_name("DoomPlayer"))
                        .and_then(|cid| self.classes.get(cid))
                        .map(|c| c.start_bullets)
                        .filter(|&n| n > 0)
                })
                .unwrap_or(50);
            let _ = crate::inventory::give_inventory(self, mo, clip, bullets);
        }
    }

    /// Doom `P_SpawnMapThing`: `tics = 1 + (P_Random() % tics)` when tics > 0.
    fn randomize_mapthing_tics(&mut self, id: ActorId) {
        if self.math_mode != SimMathMode::VanillaFixed {
            return;
        }
        let tics = self.actors.get(id).map(|a| a.tics).unwrap_or(0);
        if tics <= 0 {
            return;
        }
        let rolled = 1 + i32::from(self.p_random("pr_spawnmapthing")) % tics;
        if let Some(a) = self.actors.get_mut(id) {
            a.tics = rolled;
        }
    }

    /// Spawn an actor from class defaults at `pos`.
    pub fn spawn_at(
        &mut self,
        class: ClassId,
        pos: DVec3,
        angle: Angle,
    ) -> Result<ActorId, SimError> {
        let def = self
            .classes
            .get(class)
            .cloned()
            .unwrap_or_else(|| crate::class::ClassDef {
                id: class,
                name: "Actor".into(),
                tag: String::new(),
                parent: None,
                doomednum: None,
                health: 1000,
                gib_health: 0,
                wound_health: 0,
                death_height: 0.0,
                burn_height: 0.0,
                radius: 16.0,
                height: 56.0,
                speed: 0.0,
                melee_range: 64.0,
                melee_threshold: 0.0,
                mass: 100,
                reaction_time: 8,
                threshold: 100,
                def_threshold: 100,
                gravity: 1.0,
                friction: 1.0,
                jump_z: 8.0,
                ripper_level: 0,
                rip_level_min: 0,
                rip_level_max: 0,
                float_bob_strength: 1.0,
                max_target_range: 0.0,
                min_missile_chance: 200,
                damage_factor: 1.0,
                typed_damage_factors: std::collections::BTreeMap::new(),
                damage_type: String::new(),
                float_speed: 4.0,
                fast_speed: 0.0,
                max_step_height: 24.0,
                max_dropoff_height: 24.0,
                species: String::new(),
                infighting_group: -1,
                projectile_group: -1,
                splash_group: -1,
                designated_team: 255,
                view_height: 0.0,
                camera_height: 0.0,
                camera_fov: 0.0,
                attack_z_offset: 0.0,
                obituary: String::new(),
                hit_obituary: String::new(),
                blood_color: None,
                blood_type: Vec::new(),
                fill_color: None,
                scale_x: 1.0,
                scale_y: 1.0,
                alpha: 1.0,
                render_style: crate::class::RenderStyle::Normal,
                translation: String::new(),
                flags: 0,
                spawn_tics: -1,
                inventory_amount: 1,
                inventory_max_amount: 1,
                start_bullets: 50,
                pickup_message: String::new(),
                pickup_sound: String::new(),
                is_weapon: false,
                pain_chance: 0,
                pain_threshold: 0,
                typed_pain_chances: std::collections::BTreeMap::new(),
                damage: 0,
                ammo_type: String::new(),
                ammo_use: 0,
                ammo_give: 0,
                drop_items: Vec::new(),
                see_sound: String::new(),
                death_sound: String::new(),
                pain_sound: String::new(),
                crush_pain_sound: String::new(),
                howl_sound: String::new(),
                active_sound: String::new(),
                attack_sound: String::new(),
                sound_class: String::new(),
                friendly_see_blocks: 10,
                bounce_sound: String::new(),
                wall_bounce_sound: String::new(),
                push_sound: String::new(),
                use_sound: String::new(),
            });
        let spawn_state = self.states.find_label(class, "Spawn");
        let (sprite, frame, tics) = if let Some(sid) = spawn_state {
            if let Some(st) = self.states.get(sid) {
                (st.sprite.clone(), st.frame, i32::from(st.tics))
            } else {
                (String::new(), 0, def.spawn_tics)
            }
        } else {
            (String::new(), 0, def.spawn_tics)
        };
        let speed = if self.fast_monsters && def.fast_speed > 0.0 {
            def.fast_speed
        } else {
            def.speed
        };
        let radius = def.radius * def.scale_x;
        let height = def.height * def.scale_y;
        let mut spawn_pos = pos;
        // Doom things have no Z in the binary format (`z == 0`); snap feet to the
        // floor under XY (slope + sector height) so raised steps don't float items.
        if (def.flags & crate::class::FLAG_SPAWNCEILING) == 0 && pos.z().abs() < 1e-6 {
            if let Some(floor) = crate::physics::floor_at_xy(self, pos.x(), pos.y()) {
                spawn_pos = DVec3::new(pos.x(), pos.y(), floor);
            }
        }
        if (def.flags & crate::class::FLAG_SPAWNCEILING) != 0 {
            // Hang from ceiling: feet at `ceiling_z - height`.
            let probe = Actor {
                class,
                pos,
                vel: DVec3::ZERO,
                angle,
                pitch: Angle::ZERO,
                roll: Angle::ZERO,
                health: def.health,
                speed,
                gravity: def.gravity,
                mass: def.mass,
                alpha: def.alpha,
                render_style: def.render_style,
                translation: def.translation.clone(),
                telefog_source: String::new(),
                telefog_dest: String::new(),
                damage_type: def.damage_type.clone(),
                damage: def.damage,
                score: 0,
                radius,
                height,
                melee_range: def.melee_range,
                jump_z: def.jump_z,
                friction: def.friction,
                max_step_height: def.max_step_height,
                max_dropoff_height: def.max_dropoff_height,
                float_speed: def.float_speed,
                fast_speed: def.fast_speed,
                blood_color: def.blood_color,
                fill_color: def.fill_color,
                view_height: def.view_height,
                attack_z_offset: def.attack_z_offset,
                scale_x: def.scale_x,
                scale_y: def.scale_y,
                obituary: def.obituary.clone(),
                hit_obituary: def.hit_obituary.clone(),
                damage_factor: def.damage_factor,
                designated_team: def.designated_team,
                camera_height: def.camera_height,
                camera_fov: def.camera_fov,
                accuracy: 0,
                stamina: 0,
                ripper_level: def.ripper_level,
                rip_level_min: def.rip_level_min,
                rip_level_max: def.rip_level_max,
                state: spawn_state,
                tics,
                sprite: sprite.clone(),
                frame,
                flags: def.flags,
                reaction_time: def.reaction_time,
                threshold: def.threshold,
                pain_threshold: def.pain_threshold,
                target: None,
                master: None,
                tracer: None,
                goal: None,
                chase_goal: false,
                frightened: (def.flags & crate::class::FLAG_FRIGHTENED) != 0,
                special: 0,
                args: [0; 5],
                tid: 0,
                user_vars: std::collections::BTreeMap::new(),
                user_arrays: std::collections::BTreeMap::new(),
                spawn_point: pos,
                spawn_angle: angle,
                species: String::new(),
                nametag: def.tag.clone(),
                see_sound: String::new(),
                attack_sound: String::new(),
                pain_sound: String::new(),
                death_sound: String::new(),
                active_sound: String::new(),
                sound_class: def.sound_class.clone(),
                friendly_see_blocks: def.friendly_see_blocks,
                float_bob_phase: 0,
                count_secret: false,
                arg0_str: String::new(),
                conversation: 0,
                move_count: 0,
                move_dir: crate::actor::MOVE_DIR_NONE,
                destroyed: false,
                action_skip: 0,
            };
            let check = crate::physics::check_position(self, &probe, pos, None);
            spawn_pos = DVec3::new(pos.x(), pos.y(), check.ceiling_z - height);
        }
        // Doom `P_SpawnMobj`: `lastlook = P_Random() % MAXPLAYERS` (burn stream even if unused).
        // Map-thing-only `tics = 1 + P_Random()%tics` is applied in `spawn_map_things`.
        // Reuse the byte for `FloatBobPhase` in vanilla-fixed mode.
        let spawn_rnd = if self.math_mode == SimMathMode::VanillaFixed {
            self.p_random("pr_spawnmobj")
        } else {
            0
        };
        let _last_look = if self.math_mode == SimMathMode::VanillaFixed {
            spawn_rnd % 4
        } else {
            0
        };
        let float_bob_phase = spawn_rnd & 63;
        // Doom pickups are `MF_SPECIAL` and not `MF_SOLID` (weapons/ammo/items).
        let mut flags = def.flags;
        if def.is_weapon || def.inventory_max_amount > 1 {
            flags &= !crate::class::FLAG_SOLID;
        }
        let actor = Actor {
            class,
            pos: spawn_pos,
            vel: DVec3::ZERO,
            angle,
            pitch: Angle::ZERO,
            roll: Angle::ZERO,
            health: def.health,
            speed,
            gravity: def.gravity,
            mass: def.mass,
            alpha: def.alpha,
            render_style: def.render_style,
            translation: def.translation.clone(),
            telefog_source: String::new(),
            telefog_dest: String::new(),
            damage_type: def.damage_type.clone(),
            damage: def.damage,
            score: 0,
            radius,
            height,
            melee_range: def.melee_range,
            jump_z: def.jump_z,
            friction: def.friction,
            max_step_height: def.max_step_height,
            max_dropoff_height: def.max_dropoff_height,
            float_speed: def.float_speed,
            fast_speed: def.fast_speed,
            blood_color: def.blood_color,
            fill_color: def.fill_color,
            view_height: def.view_height,
            attack_z_offset: def.attack_z_offset,
            scale_x: def.scale_x,
            scale_y: def.scale_y,
            obituary: def.obituary.clone(),
            hit_obituary: def.hit_obituary.clone(),
            damage_factor: def.damage_factor,
            designated_team: def.designated_team,
            camera_height: def.camera_height,
            camera_fov: def.camera_fov,
            accuracy: 0,
            stamina: 0,
            ripper_level: def.ripper_level,
            rip_level_min: def.rip_level_min,
            rip_level_max: def.rip_level_max,
            state: spawn_state,
            tics,
            sprite,
            frame,
            flags,
            reaction_time: def.reaction_time,
            threshold: def.threshold,
            pain_threshold: def.pain_threshold,
            target: None,
            master: None,
            tracer: None,
            goal: None,
            chase_goal: false,
            frightened: (flags & crate::class::FLAG_FRIGHTENED) != 0,
            special: 0,
            args: [0; 5],
            tid: 0,
            user_vars: std::collections::BTreeMap::new(),
            user_arrays: std::collections::BTreeMap::new(),
            spawn_point: spawn_pos,
            spawn_angle: angle,
            species: String::new(),
            nametag: def.tag.clone(),
            see_sound: String::new(),
            attack_sound: String::new(),
            pain_sound: String::new(),
            death_sound: String::new(),
            active_sound: String::new(),
            sound_class: def.sound_class.clone(),
            friendly_see_blocks: def.friendly_see_blocks,
            float_bob_phase,
            count_secret: false,
            arg0_str: String::new(),
            conversation: 0,
            move_count: 0,
            move_dir: crate::actor::MOVE_DIR_NONE,
            destroyed: false,
            action_skip: 0,
        };
        let id = self
            .actors
            .insert(actor)
            .map_err(|e| SimError::Message(e.to_string()))?;
        self.thinkers.link_default(id);
        if (def.flags & crate::class::FLAG_COUNTKILL) != 0 {
            self.level.total_kills = self.level.total_kills.saturating_add(1);
        }
        if (def.flags & crate::class::FLAG_COUNTITEM) != 0 {
            self.level.total_items = self.level.total_items.saturating_add(1);
        }
        Ok(id)
    }

    /// First living actor with Hexen/UDMF `tid` (0 never matches).
    #[must_use]
    pub fn find_by_tid(&self, tid: i32) -> Option<ActorId> {
        if tid == 0 {
            return None;
        }
        self.thinkers.default.iter().copied().find(|&id| {
            self.actors
                .get(id)
                .is_some_and(|a| !a.destroyed && a.tid == tid)
        })
    }

    /// Playsim random byte: SFMT named stream, or classic `P_Random` in vanilla-fixed mode.
    #[must_use]
    pub fn p_random(&mut self, name: &str) -> u8 {
        if self.math_mode == SimMathMode::VanillaFixed {
            let v = self.vanilla_rng.p_random();
            if std::env::var_os("ROOM_TRACE_PRANDOM").is_some() {
                eprintln!(
                    "PRNG tic={} idx={} name={}",
                    self.level.time, self.vanilla_rng.prndindex, name
                );
            }
            v
        } else {
            self.rng.random(name).unwrap_or(0)
        }
    }

    /// Enable / disable strict vanilla-fixed math + `P_Random` (from `COMPLVL`).
    ///
    /// Does not re-clear the spawn stream — use [`Self::from_map_with_skill_mode`] so
    /// map-thing / light `P_Random` runs under `VanillaFixed` in Doom order.
    pub fn set_vanilla_demo_mode(&mut self, enabled: bool) {
        self.math_mode = math_mode_for_vanilla_demo(enabled);
    }

    /// Queue a positional sound (SFX name resolved by the shell).
    pub fn play_sound(&mut self, name: &str, x: f64, y: f64, z: f64, volume: f32) {
        self.play_sound_attn(name, x, y, z, volume, 1.0);
    }

    /// Queue a sound with explicit attenuation (`0.0` = full volume, vanilla boss cue).
    pub fn play_sound_attn(
        &mut self,
        name: &str,
        x: f64,
        y: f64,
        z: f64,
        volume: f32,
        attenuation: f32,
    ) {
        self.sound_events.push(SimSoundEvent {
            name: name.to_owned(),
            x,
            y,
            z,
            volume,
            attenuation,
        });
    }

    /// Peak tremor intensity at `viewer` (for `SimView` camera shake).
    #[must_use]
    pub fn quake_intensity_at(&self, viewer: ActorId) -> f64 {
        let Some(v) = self.actors.get(viewer) else {
            return 0.0;
        };
        let mut peak = 0.0_f64;
        for q in &self.quakes {
            let Some(spot) = self.actors.get(q.spot) else {
                continue;
            };
            let dx = v.pos.x() - spot.pos.x();
            let dy = v.pos.y() - spot.pos.y();
            let dist = room_core::dmath::sqrt(dx * dx + dy * dy);
            if dist < q.tremor_radius {
                // Classic intensity is doubled in `GetModIntensity`.
                peak = peak.max(q.intensity * 2.0);
            }
        }
        peak
    }

    /// Drain sound events since the last call.
    pub fn drain_sound_events(&mut self) -> Vec<SimSoundEvent> {
        std::mem::take(&mut self.sound_events)
    }

    /// Queue a wall/floor decal for the renderer.
    pub fn spawn_decal(&mut self, name: &str, x: f64, y: f64, z: f64, line: Option<u32>) {
        if name.is_empty() {
            return;
        }
        // UDMF `nodecals` on either side of the linedef suppresses wall decals.
        if let Some(li) = line {
            if let Some(ld) = self.map.lines.get(li as usize) {
                let blocked = ld.sides.iter().flatten().any(|sid| {
                    self.map
                        .sides
                        .get(sid.raw() as usize)
                        .is_some_and(|s| s.no_decals)
                });
                if blocked {
                    return;
                }
            }
        }
        self.decal_events.push(SimDecalEvent {
            name: name.to_owned(),
            x,
            y,
            z,
            line,
        });
    }

    /// Look up a DECALDEF generator for `class_name` and spawn that decal.
    pub fn spawn_generator_decal(
        &mut self,
        class_name: &str,
        x: f64,
        y: f64,
        z: f64,
        line: Option<u32>,
    ) {
        let key = class_name.to_ascii_lowercase();
        if let Some(decal) = self.decal_generators.get(&key).cloned() {
            self.spawn_decal(&decal, x, y, z, line);
        }
    }

    /// Drain decal events since the last call.
    pub fn drain_decal_events(&mut self) -> Vec<SimDecalEvent> {
        std::mem::take(&mut self.decal_events)
    }

    /// Queue an ACS/DECORATE particle for the renderer.
    pub fn spawn_particle(
        &mut self,
        color: u32,
        fullbright: bool,
        lifetime: i32,
        size: f64,
        x: f64,
        y: f64,
        z: f64,
    ) {
        if lifetime <= 0 {
            return;
        }
        self.particle_events.push(SimParticleEvent {
            color,
            lifetime: lifetime.clamp(1, 255),
            size: size.abs().max(0.1),
            x,
            y,
            z,
            fullbright,
        });
    }

    /// Drain ACS `SpawnParticle` events since the last call.
    pub fn drain_particle_events(&mut self) -> Vec<SimParticleEvent> {
        std::mem::take(&mut self.particle_events)
    }

    /// Queue a HUD message for the shell.
    pub fn push_message(&mut self, text: impl Into<String>) {
        self.messages.push(text.into());
    }

    /// Drain HUD messages since the last call.
    pub fn drain_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    /// Take ACS `StartSlideshow` request (shell shows / plays later).
    pub fn take_slideshow_request(&mut self) -> Option<String> {
        self.slideshow_request.take()
    }

    /// Take ACS `SetSubtitleNumber` request `(log, sound)`.
    pub fn take_subtitle_request(&mut self) -> Option<(i32, String)> {
        self.subtitle_request.take()
    }

    /// Snapshot numbers for the Doom status bar / HUD (player slot `p`).
    #[must_use]
    pub fn player_hud(&self, p: usize) -> Option<PlayerHud> {
        let player = self.players.get(p).and_then(|s| s.as_ref())?;
        let mo = player.mo?;
        let actor = self.actors.get(mo)?;
        let ammo_cid = player
            .ready_weapon
            .and_then(|w| self.classes.get(w))
            .filter(|w| !w.ammo_type.is_empty())
            .and_then(|w| self.classes.find_name(&w.ammo_type))
            .or_else(|| {
                ["Clip", "Shell", "RocketAmmo", "Cell"]
                    .iter()
                    .find_map(|name| {
                        let cid = self.classes.find_name(name)?;
                        (self.inventory.count(mo, cid) > 0).then_some(cid)
                    })
            });
        let ammo = ammo_cid
            .map(|cid| self.inventory.count(mo, cid))
            .unwrap_or(0);
        let ammo_max = ammo_cid
            .and_then(|cid| self.inventory.find(mo, cid))
            .map(|item| item.max_amount)
            .or_else(|| {
                ammo_cid
                    .and_then(|cid| self.classes.get(cid))
                    .map(|c| c.inventory_max_amount)
            })
            .unwrap_or(0);
        // Bits: blue/yellow/red card = 1/2/4; blue/yellow/red skull = 8/16/32.
        let mut keys = 0u8;
        if self
            .classes
            .find_name("BlueCard")
            .is_some_and(|c| self.inventory.count(mo, c) > 0)
        {
            keys |= 1;
        }
        if self
            .classes
            .find_name("YellowCard")
            .is_some_and(|c| self.inventory.count(mo, c) > 0)
        {
            keys |= 2;
        }
        if self
            .classes
            .find_name("RedCard")
            .is_some_and(|c| self.inventory.count(mo, c) > 0)
        {
            keys |= 4;
        }
        if self
            .classes
            .find_name("BlueSkull")
            .is_some_and(|c| self.inventory.count(mo, c) > 0)
        {
            keys |= 8;
        }
        if self
            .classes
            .find_name("YellowSkull")
            .is_some_and(|c| self.inventory.count(mo, c) > 0)
        {
            keys |= 16;
        }
        if self
            .classes
            .find_name("RedSkull")
            .is_some_and(|c| self.inventory.count(mo, c) > 0)
        {
            keys |= 32;
        }
        let face = if player.god || player.invulnerable {
            "STFGOD0".into()
        } else if actor.health <= 0 {
            "STFDEAD0".into()
        } else if actor.health < 20 {
            "STFST40".into()
        } else if actor.health < 40 {
            "STFST30".into()
        } else if actor.health < 60 {
            "STFST20".into()
        } else if actor.health < 80 {
            "STFST10".into()
        } else {
            "STFST00".into()
        };
        let weapon_patch = player
            .pspr_state
            .and_then(|sid| self.states.get(sid))
            .map(|st| weapon_patch_from_sprite_frame(&st.sprite, st.frame))
            .filter(|p| !p.is_empty())
            .or_else(|| {
                player
                    .ready_weapon
                    .and_then(|w| self.classes.get(w))
                    .map(|c| weapon_patch_for_class(&c.name).to_string())
            })
            .unwrap_or_default();
        let flash_patch = player
            .pspr_flash_state
            .and_then(|sid| self.states.get(sid))
            .map(|st| weapon_patch_from_sprite_frame(&st.sprite, st.frame))
            .filter(|p| !p.is_empty())
            .unwrap_or_default();
        // Classic fine-angle bob: cos(t) / |sin(t)| style in screen space.
        let t = f64::from(self.level.maptime) * (core::f64::consts::PI / 32.0);
        let bob = player.bob;
        #[allow(clippy::cast_possible_truncation)]
        let bob_x = (bob * room_core::dmath::cos(t)) as f32;
        #[allow(clippy::cast_possible_truncation)]
        let bob_y = (bob * room_core::dmath::sin(t).abs()
            + (player.pspr_y - crate::player::WEAPON_TOP)) as f32;
        Some(PlayerHud {
            health: actor.health,
            armor: player.armor,
            ammo,
            ammo_max,
            keys,
            face,
            weapon_patch,
            flash_patch,
            bob_x,
            bob_y,
            god: player.god,
        })
    }

    /// Toggle `god` on player slot `p` (no-op if empty). Returns new state.
    pub fn toggle_god(&mut self, p: usize) -> bool {
        let Some(player) = self.players.get_mut(p).and_then(|s| s.as_mut()) else {
            return false;
        };
        player.god = !player.god;
        let on = player.god;
        let mo = player.mo;
        // Vanilla `iddqd`: enabling god mode restores health to 100.
        if on {
            if let Some(mo) = mo {
                if let Some(a) = self.actors.get_mut(mo) {
                    a.health = 100;
                }
            }
        }
        self.push_message(if on {
            "Degreelessness Mode On"
        } else {
            "Degreelessness Mode Off"
        });
        on
    }

    /// Toggle `noclip` on player slot `p`. Returns new state.
    pub fn toggle_noclip(&mut self, p: usize) -> bool {
        let Some(player) = self.players.get_mut(p).and_then(|s| s.as_mut()) else {
            return false;
        };
        player.noclip = !player.noclip;
        let on = player.noclip;
        self.push_message(if on {
            "No clipping mode ON"
        } else {
            "No clipping mode OFF"
        });
        on
    }

    /// Toggle level `frozen` (console `freeze`). Returns new state.
    pub fn toggle_freeze(&mut self) -> bool {
        self.level.frozen = !self.level.frozen;
        let on = self.level.frozen;
        self.push_message(if on {
            "Freeze mode ON"
        } else {
            "Freeze mode OFF"
        });
        on
    }

    /// Mark linedefs of the actor's current sector as automap-explored (`ML_MAPPED`).
    ///
    /// Vanilla sets this when the software renderer draws a wall; Room approximates
    /// by revealing every linedef bounding the sector the player stands in.
    pub fn mark_automap_at(&mut self, mo: ActorId) {
        let Some(a) = self.actors.get(mo) else {
            return;
        };
        let Some(sid) = room_nodebuild::point_in_sector(&self.map, a.pos.x(), a.pos.y())
            .or_else(|| {
                room_nodebuild::point_in_sector_vanilla(&self.map, a.pos.x(), a.pos.y())
            })
        else {
            return;
        };
        let mut touched = Vec::new();
        for (i, line) in self.map.lines.iter().enumerate() {
            if line.front_sector == Some(sid) || line.back_sector == Some(sid) {
                if (i32::from(line.flags) & ML_MAPPED) == 0 {
                    touched.push(i as u32);
                }
            }
        }
        if touched.is_empty() {
            return;
        }
        let map = std::sync::Arc::make_mut(&mut self.map);
        for idx in touched {
            if let Some(line) = map.lines.get_mut(idx as usize) {
                line.flags |= ML_MAPPED as i16;
            }
            let base = self.line_flags.get(&idx).copied().unwrap_or(0);
            self.line_flags.insert(idx, base | ML_MAPPED);
        }
    }

    /// Toggle Computer Area Map (`iddt` bootstrap — full automap reveal).
    pub fn toggle_iddt(&mut self, p: usize) -> bool {
        let Some(player) = self.players.get_mut(p).and_then(|s| s.as_mut()) else {
            return false;
        };
        player.allmap = !player.allmap;
        let on = player.allmap;
        self.push_message(if on {
            "Computer Area Map ON"
        } else {
            "Computer Area Map OFF"
        });
        on
    }

    /// Console `idmypos` — print player map coordinates / angle.
    pub fn report_idmypos(&mut self, p: usize) -> bool {
        let Some(mo) = self
            .players
            .get(p)
            .and_then(|s| s.as_ref())
            .and_then(|pl| pl.mo)
        else {
            return false;
        };
        let Some(a) = self.actors.get(mo) else {
            return false;
        };
        let msg = format!(
            "ang={:.0};x,y=({:.0},{:.0})",
            a.angle.degrees(),
            a.pos.x(),
            a.pos.y()
        );
        self.push_message(msg);
        true
    }

    /// Toggle `paused` (console `pause`). Returns new state.
    pub fn toggle_pause(&mut self) -> bool {
        self.paused = !self.paused;
        self.paused
    }

    /// Spawn `class_name` a short distance in front of player `p` (`summon`).
    pub fn summon_in_front(&mut self, p: usize, class_name: &str) -> Option<ActorId> {
        let class = self.classes.find_name(class_name)?;
        let mo = self
            .players
            .get(p)
            .and_then(|s| s.as_ref())
            .and_then(|pl| pl.mo)?;
        let (pos, angle) = {
            let a = self.actors.get(mo)?;
            let rad = a.angle.radians();
            let dist = a.radius + 48.0;
            let pos = DVec3::new(
                a.pos.x() + room_core::dmath::cos(rad) * dist,
                a.pos.y() + room_core::dmath::sin(rad) * dist,
                a.pos.z(),
            );
            (pos, a.angle)
        };
        self.spawn_at(class, pos, angle).ok()
    }

    /// `kill monsters` — deal fatal damage to all `COUNTKILL` actors (not players).
    pub fn kill_monsters(&mut self) -> u32 {
        let player_mos: Vec<_> = self.players.iter().flatten().filter_map(|p| p.mo).collect();
        let targets: Vec<_> = self
            .thinkers
            .default
            .iter()
            .copied()
            .filter(|&id| {
                if player_mos.contains(&id) {
                    return false;
                }
                self.actors
                    .get(id)
                    .is_some_and(|a| !a.destroyed && (a.flags & crate::class::FLAG_COUNTKILL) != 0)
            })
            .collect();
        let mut n = 0;
        for id in targets {
            let hp = self.actors.get(id).map(|a| a.health).unwrap_or(0);
            if crate::damage::damage_mobj(self, id, None, None, hp.max(1) + 10_000) > 0 {
                n += 1;
            }
        }
        n
    }

    /// Copy this tic's commands into player slots (call before [`Self::tick`]).
    pub fn set_ticcmds(&mut self, cmds: &[crate::player::TicCmd]) {
        for (i, cmd) in cmds.iter().enumerate().take(MAX_PLAYERS) {
            if let Some(p) = &mut self.players[i] {
                p.cmd = *cmd;
            }
        }
    }

    /// One game tic (`P_Ticker` subset). See `docs/07-playsim.md §2.2`.
    pub fn tick(&mut self) {
        // GZDoom: if paused ⇒ return
        if self.paused {
            return;
        }
        self.scripts.sync_from_world(self);
        self.sound_events.clear();
        self.messages.clear();
        self.screen_fade.tick();
        // Snapshot plane Z for render interpolation (`interpolations.begin_tic`).
        for sec in &mut self.sectors {
            sec.prev_floor_z = sec.floor_z;
            sec.prev_ceiling_z = sec.ceiling_z;
        }
        for i in 0..MAX_PLAYERS {
            if self.players[i].is_some() {
                let mut p = self.players[i].take().unwrap_or_default();
                think_player::player_think(self, &mut p);
                self.players[i] = Some(p);
            }
        }
        // events.world_tick (WASM EventHandlers) + flush WASM→ACS starts
        let _ = self.scripts.dispatch_world_tick();
        for (script, map, args) in self.scripts.drain_acs_starts() {
            let mut a = [0i32; 5];
            a[0] = script;
            a[1] = map;
            for (i, v) in args.iter().take(3).enumerate() {
                a[2 + i] = *v;
            }
            let _ = crate::acs::acs_execute(self, &a, false, None, None);
        }
        if !self.level.frozen {
            specials::tick_sector_pushers(self);
        }
        // Doom `P_RunThinkers` order after `P_SpawnSpecials`: map things, then levspec
        // lights, then anything spawned later (missiles). Splitting only "this tic's"
        // spawns is not enough — in-flight missiles must also run *after* lights so
        // `P_ExplodeMissile`'s `P_Random` does not steal the flash count byte.
        let n_map = self.map_thinker_count.min(self.thinkers.default.len());
        let mut ti = 0;
        while ti < n_map {
            let id = self.thinkers.default[ti];
            ti += 1;
            if self
                .actors
                .get(id)
                .is_some_and(|a| (a.flags & crate::class::FLAG_DORMANT) != 0)
            {
                continue;
            }
            // Player pawns already applied cmd velocity this tic; still run XY for monsters.
            crate::physics::xy_movement(self, id);
            crate::physics::z_movement(self, id);
            // Doom touches specials inside `P_TryMove`; approximate post-move.
            if let Some(pi) = self
                .players
                .iter()
                .enumerate()
                .find_map(|(i, p)| p.as_ref().and_then(|pl| (pl.mo == Some(id)).then_some(i)))
            {
                let mut p = self.players[pi].take().unwrap_or_default();
                crate::pickup::touch_specials(self, &mut p, id);
                self.players[pi] = Some(p);
            }
            self.advance_actor_state(id);
        }
        if !self.level.frozen {
            specials::tick_strobes(self);
            specials::tick_flickers(self);
            specials::tick_glows(self);
        }
        // Missiles already in-flight + newly spawned this tic (list may grow).
        while ti < self.thinkers.default.len() {
            let id = self.thinkers.default[ti];
            ti += 1;
            if self
                .actors
                .get(id)
                .is_some_and(|a| (a.flags & crate::class::FLAG_DORMANT) != 0)
            {
                continue;
            }
            crate::physics::xy_movement(self, id);
            crate::physics::z_movement(self, id);
            self.advance_actor_state(id);
        }
        if !self.level.frozen {
            specials::tick_doors(self);
            specials::tick_floors(self);
            specials::tick_plats(self);
            specials::tick_ceilings(self);
            specials::tick_waggles(self);
            specials::tick_elevators(self);
            crate::polyobj::tick_polyobjs(self);
            crate::acs::tick_acs(self);
            crate::sndseq::tick_sequences(self);
            specials::tick_fades(self);
            specials::tick_scrollers(self);
            specials::tick_ceiling_scrollers(self);
            specials::tick_tex_scrollers(self);
            specials::apply_transfers(self);
            tick_quakes(self);
        }
        for i in 0..MAX_PLAYERS {
            if let Some(mo) = self.players[i].as_ref().and_then(|p| p.mo) {
                crate::sector_specials::player_in_special_sector(self, mo);
            }
        }
        crate::sector_specials::monsters_in_special_sectors(self);
        self.level.tick();
        // gc_destroyed_actors — later
    }

    /// Enter a state, optionally running its action (`AActor::SetState`).
    pub fn set_state(
        &mut self,
        id: ActorId,
        state: Option<room_core::StateId>,
        call_action: bool,
    ) {
        let Some(sid) = state else {
            if let Some(a) = self.actors.get_mut(id) {
                a.state = None;
                a.destroyed = true;
            }
            return;
        };
        let Some(st) = self.states.get(sid).cloned() else {
            return;
        };
        let action = st.action.clone();
        let action_args = st.action_args.clone();
        if let Some(a) = self.actors.get_mut(id) {
            a.state = Some(sid);
            a.tics = i32::from(st.tics);
            a.sprite = st.sprite;
            a.frame = st.frame;
        }
        if call_action {
            if let Some(name) = action.as_deref() {
                let evaluated = crate::deco_eval::eval_action_args(self, id, &action_args);
                match action::call_action_args(self, id, name, &evaluated) {
                    ActionResult::Continue => {}
                    ActionResult::Jump(j) => self.set_state(id, Some(j), true),
                }
            }
            // Zero-tic chain.
            if let Some(a) = self.actors.get(id) {
                if a.tics == 0 {
                    let next = a
                        .state
                        .and_then(|s| self.states.get(s))
                        .and_then(|s| s.next);
                    self.set_state(id, next, true);
                }
            }
        }
    }

    /// Decrement tics; when they expire, follow `next` and run the new state's action.
    fn advance_actor_state(&mut self, id: ActorId) {
        let (destroyed, tics) = match self.actors.get(id) {
            Some(a) => (a.destroyed, a.tics),
            None => return,
        };
        if destroyed {
            return;
        }
        // Final death frame (`tics == -1`): nightmare / `-respawn` revive.
        if tics < 0 {
            try_nightmare_respawn(self, id);
            return;
        }
        let Some(actor) = self.actors.get_mut(id) else {
            return;
        };
        actor.tick_state_timer();
        if actor.tics != 0 {
            return;
        }
        let next = actor
            .state
            .and_then(|sid| self.states.get(sid))
            .and_then(|st| st.next);
        self.set_state(id, next, true);
    }

    /// Enqueue a monster corpse; destroy the oldest when over `corpse_queue_size`.
    pub fn queue_corpse(&mut self, id: ActorId) {
        if self.corpse_queue_size == 0 {
            return;
        }
        if self.corpse_queue.iter().any(|&x| x == id) {
            return;
        }
        self.corpse_queue.push_back(id);
        while self.corpse_queue.len() > self.corpse_queue_size {
            if let Some(old) = self.corpse_queue.pop_front() {
                destroy_queued_corpse(self, old);
            }
        }
    }

    /// Enqueue a player body; destroy the oldest when over `body_queue_size`.
    pub fn queue_body(&mut self, id: ActorId) {
        if self.body_queue_size == 0 {
            return;
        }
        if self.body_queue.iter().any(|&x| x == id) {
            return;
        }
        self.body_queue.push_back(id);
        while self.body_queue.len() > self.body_queue_size {
            if let Some(old) = self.body_queue.pop_front() {
                destroy_queued_corpse(self, old);
            }
        }
    }

    /// Remove `id` from corpse/body queues without destroying it.
    pub fn dequeue_corpse(&mut self, id: ActorId) {
        self.corpse_queue.retain(|&x| x != id);
        self.body_queue.retain(|&x| x != id);
    }

    /// Stable checksum for determinism / netplay (`World::checksum`).
    #[must_use]
    pub fn checksum(&self) -> u64 {
        let mut bytes = Vec::with_capacity(64 + self.sectors.len() * 24);
        bytes.extend_from_slice(&self.level.time.to_le_bytes());
        bytes.extend_from_slice(&self.level.maptime.to_le_bytes());
        bytes.extend_from_slice(&self.level.totaltime.to_le_bytes());
        bytes.push(u8::from(self.level.frozen));
        bytes.push(u8::from(self.paused));
        for s in &self.sectors {
            bytes.extend_from_slice(&s.floor_z.to_bits().to_le_bytes());
            bytes.extend_from_slice(&s.ceiling_z.to_bits().to_le_bytes());
            bytes.extend_from_slice(&s.light.to_le_bytes());
        }
        bytes.extend_from_slice(&(self.actors.len() as u32).to_le_bytes());
        for &id in &self.thinkers.default {
            if let Some(a) = self.actors.get(id) {
                bytes.extend_from_slice(&a.class.raw().to_le_bytes());
                bytes.extend_from_slice(&a.pos.x().to_bits().to_le_bytes());
                bytes.extend_from_slice(&a.pos.y().to_bits().to_le_bytes());
                bytes.extend_from_slice(&a.pos.z().to_bits().to_le_bytes());
                bytes.extend_from_slice(&a.health.to_le_bytes());
                bytes.extend_from_slice(&a.tics.to_le_bytes());
            }
        }
        let n = self.rng.iter().count() as u32;
        bytes.extend_from_slice(&n.to_le_bytes());
        for named in self.rng.iter() {
            bytes.extend_from_slice(&named.seed_token().to_le_bytes());
        }
        bytes.extend_from_slice(&(self.quakes.len() as u32).to_le_bytes());
        for q in &self.quakes {
            bytes.extend_from_slice(&q.countdown.to_le_bytes());
            bytes.extend_from_slice(&q.intensity.to_bits().to_le_bytes());
            bytes.extend_from_slice(&q.damage_radius.to_bits().to_le_bytes());
        }
        // Player weapon / powerup state (netplay + demos).
        for slot in &self.players {
            let Some(p) = slot else {
                bytes.push(0);
                continue;
            };
            bytes.push(1);
            bytes.push(p.player_state as u8);
            bytes.extend_from_slice(&p.ready_weapon.map(|c| c.raw()).unwrap_or(0).to_le_bytes());
            bytes.extend_from_slice(&p.pending_weapon.map(|c| c.raw()).unwrap_or(0).to_le_bytes());
            bytes.extend_from_slice(&p.pspr_y.to_bits().to_le_bytes());
            bytes.extend_from_slice(
                &p.pspr_state
                    .map(|s| s.raw())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(&p.pspr_tics.to_le_bytes());
            bytes.extend_from_slice(
                &p.pspr_flash_state
                    .map(|s| s.raw())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(&p.pspr_flash_tics.to_le_bytes());
            bytes.extend_from_slice(&p.extralight.to_le_bytes());
            bytes.extend_from_slice(&p.bob.to_bits().to_le_bytes());
            bytes.push(p.armor_class);
            bytes.extend_from_slice(&p.armor.to_le_bytes());
            bytes.push(u8::from(p.god));
            bytes.push(u8::from(p.noclip));
            bytes.push(u8::from(p.berserk));
            bytes.push(u8::from(p.invulnerable));
            bytes.push(u8::from(p.blur));
            bytes.push(u8::from(p.radsuit));
            bytes.push(u8::from(p.infrared));
            bytes.push(u8::from(p.backpack));
            bytes.push(u8::from(p.allmap));
        }
        xxh3_64(&bytes)
    }
}

fn destroy_queued_corpse(world: &mut World, id: ActorId) {
    if let Some(a) = world.actors.get_mut(id) {
        a.destroyed = true;
        a.state = None;
        a.flags = 0;
        a.health = 0;
    }
}

/// Vanilla `P_NightmareRespawn`: revive a `COUNTKILL` corpse at its spawn point.
fn try_nightmare_respawn(world: &mut World, id: ActorId) {
    if !world.respawn_monsters {
        return;
    }
    let Some(a) = world.actors.get(id) else {
        return;
    };
    if (a.flags & crate::class::FLAG_COUNTKILL) == 0 || a.health > 0 {
        return;
    }
    if let Some(a) = world.actors.get_mut(id) {
        a.move_count = a.move_count.saturating_add(1);
    }
    let move_count = world.actors.get(id).map(|a| a.move_count).unwrap_or(0);
    // 12 seconds at TICRATE 35.
    if move_count < 12 * 35 {
        return;
    }
    if world.level.time & 31 != 0 {
        return;
    }
    let roll = world.p_random("pr_respawn");
    if roll > 4 {
        return;
    }
    let Some(a) = world.actors.get(id).cloned() else {
        return;
    };
    let class = a.class;
    let spawn_pos = a.spawn_point;
    let spawn_angle = a.spawn_angle;
    let old_pos = a.pos;
    // Spawn point must be clear (ignore the corpse itself).
    let probe = Actor {
        pos: spawn_pos,
        destroyed: false,
        ..a
    };
    let check = crate::physics::check_position(world, &probe, spawn_pos, Some(id));
    if !check.ok {
        return;
    }
    if let Some(corpse) = world.actors.get_mut(id) {
        corpse.destroyed = true;
        corpse.state = None;
        corpse.flags = 0;
    }
    world.dequeue_corpse(id);
    crate::specials::spawn_teleport_fog(world, old_pos);
    crate::specials::spawn_teleport_fog(world, spawn_pos);
    let _ = world.spawn_at(class, spawn_pos, spawn_angle);
}

/// Tick active earthquakes (damage + thrust + countdown).
fn tick_quakes(world: &mut World) {
    if world.quakes.is_empty() {
        return;
    }
    let mut i = 0;
    while i < world.quakes.len() {
        let spot = world.quakes[i].spot;
        let Some(origin) = world.actors.get(spot).map(|a| a.pos) else {
            world.quakes.swap_remove(i);
            continue;
        };
        let sound = world.quakes[i].sound.clone();
        if !sound.is_empty() {
            world.play_sound(&sound, origin.x(), origin.y(), origin.z(), 1.0);
        }
        let damage_radius = world.quakes[i].damage_radius;
        let intensity = world.quakes[i].intensity;
        let thrust_mul = world.quakes[i].thrust_multiplier;
        if damage_radius > 0.0 {
            let mut victims: Vec<ActorId> = Vec::new();
            for slot in world.players.iter().flatten() {
                if let Some(mo) = slot.mo {
                    victims.push(mo);
                }
            }
            for victim in victims {
                if victim == spot {
                    continue;
                }
                let Some(v) = world.actors.get(victim).cloned() else {
                    continue;
                };
                let dx = v.pos.x() - origin.x();
                let dy = v.pos.y() - origin.y();
                let dist = room_core::dmath::sqrt(dx * dx + dy * dy);
                if dist >= damage_radius {
                    continue;
                }
                let floor_z =
                    crate::physics::check_position(world, &v, v.pos, Some(victim)).floor_z;
                if v.pos.z() > floor_z + 1e-3 {
                    continue;
                }
                let roll = world.p_random("pr_quake");
                if roll < 50 {
                    let damage = world
                        .rng
                        .get_or_insert("pr_quake")
                        .map(|rng| rng.hit_dice(1).max(1))
                        .unwrap_or(1);
                    let _ = crate::damage::damage_mobj(world, victim, None, None, damage);
                }
                let flags = world
                    .actors
                    .get(victim)
                    .map(|a| a.flags | world.classes.get(a.class).map(|c| c.flags).unwrap_or(0))
                    .unwrap_or(0);
                if (flags & crate::class::FLAG_DONTTHRUST) == 0 && thrust_mul > 0.0 {
                    let ang =
                        f64::from(world.p_random("pr_quake")) * (core::f64::consts::TAU / 256.0);
                    if let Some(a) = world.actors.get_mut(victim) {
                        a.vel = DVec3::new(
                            a.vel.x() + intensity * room_core::dmath::cos(ang) * thrust_mul,
                            a.vel.y() + intensity * room_core::dmath::sin(ang) * thrust_mul,
                            a.vel.z(),
                        );
                    }
                }
            }
        }
        world.quakes[i].countdown -= 1;
        if world.quakes[i].countdown <= 0 {
            world.quakes.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::StateTable;
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    fn empty_room() -> MapGeometry {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        map
    }

    #[test]
    fn udmf_sector_gravity_friction_damage_applied() {
        let mut map = empty_room();
        map.sectors[0].gravity = 0.25;
        map.sectors[0].friction = 100; // ORIG
        map.sectors[0].damage_amount = 5;
        map.sectors[0].damage_type = "Slime".into();
        map.sectors[0].damage_interval = 8;
        let w = World::from_map(map).unwrap();
        assert!((w.sectors[0].gravity - 0.25).abs() < 1e-9);
        assert!((w.sectors[0].friction - 0.90625).abs() < 1e-4);
        assert_eq!(w.sectors[0].damage_amount, 5);
        assert_eq!(w.sectors[0].damage_type, "Slime");
        assert_eq!(w.sectors[0].damage_interval, 8);
    }

    #[test]
    fn nodecals_side_blocks_wall_decal_spawn() {
        let mut map = empty_room();
        if let Some(s) = map.sides.first_mut() {
            s.no_decals = true;
        }
        let mut w = World::from_map(map).unwrap();
        w.spawn_decal("BulletChip", 0.0, 0.0, 0.0, Some(0));
        assert!(w.drain_decal_events().is_empty());
        w.spawn_decal("BulletChip", 0.0, 0.0, 0.0, None);
        assert_eq!(w.drain_decal_events().len(), 1);
    }

    #[test]
    fn tick_advances_time_deterministically() {
        let mut w = World::from_map(empty_room()).unwrap();
        let c0 = w.checksum();
        w.tick();
        w.tick();
        assert_eq!(w.level.time, 2);
        assert_eq!(w.level.maptime, 2);
        let c2 = w.checksum();
        assert_ne!(c0, c2);

        let mut w2 = World::from_map(empty_room()).unwrap();
        w2.tick();
        w2.tick();
        assert_eq!(w.checksum(), w2.checksum());
    }

    #[test]
    fn pause_skips_clock() {
        let mut w = World::from_map(empty_room()).unwrap();
        w.paused = true;
        w.tick();
        assert_eq!(w.level.time, 0);
    }

    #[test]
    fn p_random_uses_vanilla_table_in_fixed_mode() {
        use room_core::rng::RNDTABLE;
        let mut w = World::from_map(empty_room()).unwrap();
        w.set_vanilla_demo_mode(true);
        assert_eq!(w.p_random("pr_chase"), RNDTABLE[1]);
        assert_eq!(w.p_random("pr_hitscan"), RNDTABLE[2]);
        // Named streams collapse to one classic index.
        assert_eq!(w.vanilla_rng.prndindex, 2);
    }

    #[test]
    fn spawn_applies_class_scale_to_size() {
        let ir = parse_decorate(
            r#"
actor BigGuy : Actor 9950
{
  Radius 20
  Height 56
  Scale 2
  States { Spawn: POSS A -1 Stop }
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
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let id = w
            .spawn_at(
                classes.find_name("BigGuy").unwrap(),
                DVec3::new(100.0, 100.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        let a = w.actors.get(id).unwrap();
        assert!((a.radius - 40.0).abs() < 1e-9);
        assert!((a.height - 112.0).abs() < 1e-9);
    }

    #[test]
    fn spawn_snaps_feet_to_raised_sector_floor() {
        let ir = parse_decorate(
            r#"
actor Clip : Actor 2007
{
  Radius 20
  Height 16
  States { Spawn: CLIP A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 48.0, 192.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let id = w
            .spawn_at(
                classes.find_name("Clip").unwrap(),
                DVec3::new(100.0, 100.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        let a = w.actors.get(id).unwrap();
        assert!(
            (a.pos.z() - 48.0).abs() < 1e-6,
            "expected floor snap z=48, got {}",
            a.pos.z()
        );
    }

    #[test]
    fn spawn_ceiling_hangs_from_sector_ceiling() {
        let ir = parse_decorate(
            r#"
actor HangMeat : Actor 9941
{
  Radius 16
  Height 64
  +SPAWNCEILING
  +NOGRAVITY
  States { Spawn: GOR2 A -1 Stop }
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
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        assert_eq!(
            classes
                .get(classes.find_name("HangMeat").unwrap())
                .unwrap()
                .flags
                & crate::class::FLAG_SPAWNCEILING,
            crate::class::FLAG_SPAWNCEILING
        );
        let id = w
            .spawn_at(
                classes.find_name("HangMeat").unwrap(),
                DVec3::new(100.0, 100.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        let a = w.actors.get(id).unwrap();
        // Ceiling 128, height 64 → feet at 64.
        assert!(
            (a.pos.z() - 64.0).abs() < 1e-6,
            "expected z=64, got {}",
            a.pos.z()
        );
    }

    #[test]
    fn mtf_friend_sets_friendly_flag_on_spawn() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  +SHOOTABLE
  +COUNTKILL
  States { Spawn: POSS A -1 Stop }
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
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 100,
            y: 100,
            z: 0,
            angle: 0,
            type_id: 3004,
            flags: 7 | 128, // skill all + MTF_FRIEND
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: 3,
            health: 50,
            score: 25,
            gravity: 0.5,
            scale_x: 2.0,
            scale_y: 2.0,
            alpha: 0.25,
            pitch: 15,
            roll: -10,
            fill_color: 0x00_ff_00,
            render_style: "stencil".into(),
            float_bob_phase: 7,
            count_secret: false,
            no_count: false,
            arg0_str: String::new(),
            conversation: 0,
            accuracy: 50,
            stamina: 25,
            species: "Zombie".into(),
            melee_range: 96.0,
            view_height: 48.0,
            mass: 400,
            speed: 12.0,
            friction: 0.75,
            damage_factor: 0.5,
            max_step_height: 32.0,
            max_dropoff_height: 64.0,
            float_speed: 4.0,
            fast_speed: 20.0,
            jump_z: 9.0,
            attack_z_offset: 8.0,
        });
        apply_nodes(&mut map);
        let w = World::from_map_with_classes(map, classes, states).unwrap();
        let friend = w.thinkers.default.iter().find_map(|&id| {
            let a = w.actors.get(id)?;
            (a.score == 25).then_some(a)
        });
        let friend = friend.unwrap();
        assert_ne!(friend.flags & crate::class::FLAG_FRIENDLY, 0);
        assert_eq!(friend.friendly_see_blocks, 3);
        assert_eq!(friend.health, 50);
        assert_eq!(friend.accuracy, 50);
        assert_eq!(friend.stamina, 25);
        assert_eq!(friend.species, "Zombie");
        assert!((friend.melee_range - 96.0).abs() < 1e-9);
        assert!((friend.view_height - 48.0).abs() < 1e-9);
        assert_eq!(friend.mass, 400);
        assert!((friend.speed - 12.0).abs() < 1e-9);
        assert!((friend.friction - 0.75).abs() < 1e-9);
        assert!((friend.damage_factor - 0.5).abs() < 1e-9);
        assert!((friend.max_step_height - 32.0).abs() < 1e-9);
        assert!((friend.max_dropoff_height - 64.0).abs() < 1e-9);
        assert!((friend.float_speed - 4.0).abs() < 1e-9);
        assert!((friend.fast_speed - 20.0).abs() < 1e-9);
        assert!((friend.jump_z - 9.0).abs() < 1e-9);
        assert!((friend.attack_z_offset - 8.0).abs() < 1e-9);
        assert_eq!(friend.score, 25);
        assert!((friend.gravity - 0.5).abs() < 1e-9);
        assert!((friend.scale_x - 2.0).abs() < 1e-9);
        assert!((friend.scale_y - 2.0).abs() < 1e-9);
        assert!((friend.radius - 40.0).abs() < 1e-9); // default radius 20 × scale
        assert!((friend.height - 32.0).abs() < 1e-9); // default height 16 × scale
        assert!((friend.alpha - 0.25).abs() < 1e-9);
        assert!((friend.pitch.degrees() - 15.0).abs() < 1e-9);
        assert!((friend.roll.degrees() - (-10.0)).abs() < 1e-9);
        assert_eq!(friend.fill_color, Some([0, 255, 0]));
        assert_eq!(friend.render_style, crate::class::RenderStyle::Stencil);
        assert_eq!(friend.float_bob_phase, 7);
    }

    #[test]
    fn udmf_countsecret_and_nocount_on_spawn() {
        let ir = parse_decorate(
            r#"
actor SecretOrb : Actor 2014
{
  Health 1
  +COUNTITEM
  States { Spawn: BON1 A -1 Stop }
}
actor QuietZombie : Actor 3004
{
  Health 20
  +SHOOTABLE
  +COUNTKILL
  States { Spawn: POSS A -1 Stop }
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
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 80,
            y: 80,
            z: 0,
            angle: 0,
            type_id: 2014,
            flags: 7,
            special: 0,
            args: [0; 5],
            hexen: false,
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
            count_secret: true,
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
        });
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 120,
            y: 80,
            z: 0,
            angle: 0,
            type_id: 3004,
            flags: 7,
            special: 0,
            args: [0; 5],
            hexen: false,
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
            no_count: true,
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
        });
        apply_nodes(&mut map);
        let w = World::from_map_with_classes(map, classes, states).unwrap();
        assert_eq!(w.level.total_secrets, 1);
        assert_eq!(w.level.total_kills, 0);
        assert_eq!(w.level.total_items, 1);
        let orb = w.thinkers.default.iter().find_map(|&id| {
            let a = w.actors.get(id)?;
            a.count_secret.then_some(a)
        });
        assert!(orb.is_some());
        let zombie = w.thinkers.default.iter().find_map(|&id| {
            let a = w.actors.get(id)?;
            (a.health == 20).then_some(a)
        });
        let zombie = zombie.unwrap();
        assert_eq!(zombie.flags & crate::class::FLAG_COUNTKILL, 0);
    }

    #[test]
    fn spawn_coop_players_fills_seats_at_starts() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        for (i, (x, y)) in [(96i16, 32i16), (160, 32), (224, 32)]
            .into_iter()
            .enumerate()
        {
            let mut t = map.things[0].clone();
            t.x = x;
            t.y = y;
            t.type_id = (2 + i) as i16;
            map.things.push(t);
        }
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert!(w.players[0].is_some());
        assert!(w.players[1].is_none());
        assert_eq!(w.spawn_coop_players(2).unwrap(), 2);
        assert!(w.players[1].is_some());
        let mo0 = w.players[0].as_ref().unwrap().mo.unwrap();
        let mo1 = w.players[1].as_ref().unwrap().mo.unwrap();
        assert_ne!(mo0, mo1);
        let a0 = w.actors.get(mo0).unwrap();
        let a1 = w.actors.get(mo1).unwrap();
        assert!((a0.pos.x() - a1.pos.x()).abs() > 8.0);
        assert_eq!(w.spawn_coop_players(4).unwrap(), 4);
        assert!(w.players[2].is_some());
        assert!(w.players[3].is_some());
    }

    #[test]
    fn coop_seats_each_get_ready_weapon() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        for (i, (x, y)) in [(96i16, 32i16), (160, 32)].into_iter().enumerate() {
            let mut t = map.things[0].clone();
            t.x = x;
            t.y = y;
            t.type_id = (2 + i) as i16;
            map.things.push(t);
        }
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        assert_eq!(w.spawn_coop_players(2).unwrap(), 2);
        let pistol = classes.find_name("Pistol").unwrap();
        assert_eq!(w.players[0].as_ref().unwrap().ready_weapon, Some(pistol));
        assert_eq!(
            w.players[1].as_ref().unwrap().ready_weapon,
            Some(pistol),
            "seat 1 must get a ready weapon (gun overlay)"
        );
        assert!(!w.player_hud(1).unwrap().weapon_patch.is_empty());
    }

    #[test]
    fn spawn_helper_dogs_at_player_starts() {
        let ir = parse_decorate(
            r#"
actor MBFHelperDog : Actor 888
{
  Health 500
  Radius 12
  Height 28
  +SHOOTABLE
  States { Spawn: DOGS A -1 Stop }
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
        for (i, (x, y)) in [(80i16, 80i16), (120, 80), (160, 80)]
            .into_iter()
            .enumerate()
        {
            map.things.push(room_map::RawThing {
                tid: 0,
                x,
                y,
                z: 0,
                angle: 90,
                type_id: (2 + i) as i16,
                flags: 7,
                special: 0,
                args: [0; 5],
                hexen: false,
                friendly_see_blocks: -1,
                health: 0,
                score: -2147483648,
                gravity: -1.0,
                scale_x: 0.0,
                scale_y: 0.0,
                alpha: -1.0,
                pitch: -32768,
                roll: -32768,
                fill_color: -1,
                render_style: String::new(),
                float_bob_phase: -1,
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
            });
        }
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        assert_eq!(w.spawn_helper_dogs(3).unwrap(), 3);
        let dogs: Vec<_> = w
            .thinkers
            .default
            .iter()
            .filter_map(|&id| {
                let a = w.actors.get(id)?;
                (a.health == 500 && a.flags & crate::class::FLAG_FRIENDLY != 0).then_some(id)
            })
            .collect();
        assert_eq!(dogs.len(), 3);
    }

    #[test]
    fn dormant_skips_state_advance_until_damage() {
        let ir = parse_decorate(
            r#"
actor Sleeper : Actor 9940
{
  Health 100
  Speed 8
  +SHOOTABLE
  +DORMANT
  States
  {
  Spawn:
    POSS A 1
    POSS B 1
    Loop
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
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let sleeper = classes.find_name("Sleeper").unwrap();
        assert_eq!(
            classes.get(sleeper).unwrap().flags & crate::class::FLAG_DORMANT,
            crate::class::FLAG_DORMANT
        );
        let id = w
            .spawn_at(sleeper, DVec3::new(100.0, 100.0, 0.0), Angle::ZERO)
            .unwrap();
        let frame0 = w.actors.get(id).unwrap().frame;
        let state0 = w.actors.get(id).unwrap().state;
        w.tick();
        w.tick();
        assert_eq!(w.actors.get(id).unwrap().frame, frame0);
        assert_eq!(w.actors.get(id).unwrap().state, state0);
        let _ = crate::damage::damage_mobj(&mut w, id, None, None, 1);
        assert_eq!(
            w.actors.get(id).unwrap().flags & crate::class::FLAG_DORMANT,
            0
        );
        w.tick();
        assert_ne!(w.actors.get(id).unwrap().frame, frame0);
    }

    #[test]
    fn skill_filters_map_things() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  +SOLID
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        // Hard-only zombie (MTF_HARD = 4).
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 100,
            y: 100,
            z: 0,
            angle: 0,
            type_id: 3004,
            flags: 4,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: -2147483648,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: -32768,
            roll: -32768,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
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
        });
        apply_nodes(&mut map);
        let easy =
            World::from_map_with_skill(map.clone(), Arc::clone(&classes), Arc::clone(&states), 2)
                .unwrap();
        assert_eq!(easy.actors.len(), 1, "skill 2 skips hard-only thing");
        let hard = World::from_map_with_skill(map, classes, states, 4).unwrap();
        assert_eq!(hard.actors.len(), 2, "skill 4 spawns hard-only thing");
    }

    #[test]
    fn spawns_player_start_and_decorate_thing() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Radius 20
  Height 56
  +SOLID
  States { Spawn: POSS A 10 Loop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 90)
            .build_udmf("MAP01")
            .unwrap();
        // Inject a zombie thing after build.
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 100,
            y: 100,
            z: 0,
            angle: 0,
            type_id: 3004,
            flags: 0,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: -2147483648,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: -32768,
            roll: -32768,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
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
        });
        apply_nodes(&mut map);
        let w = World::from_map_with_classes(map, classes, states).unwrap();
        assert_eq!(w.actors.len(), 2);
        let zombie = w.thinkers.default.iter().find_map(|&id| {
            let a = w.actors.get(id)?;
            (a.health == 20).then_some(a)
        });
        let zombie = zombie.unwrap();
        assert_eq!(zombie.tics, 10);
        assert_eq!(zombie.radius, 20.0);
    }

    #[test]
    fn player_starts_with_fist_pistol_ammo() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let pistol = classes.find_name("Pistol").unwrap();
        let fist = classes.find_name("Fist").unwrap();
        let clip = classes.find_name("Clip").unwrap();
        assert_eq!(w.players[0].as_ref().unwrap().ready_weapon, Some(pistol));
        assert!(crate::inventory::count_inventory(&w, mo, fist) >= 1);
        assert!(crate::inventory::count_inventory(&w, mo, pistol) >= 1);
        assert!(crate::inventory::count_inventory(&w, mo, clip) >= 50);
    }

    #[test]
    fn start_bullets_property_sets_loadout_ammo() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  StartBullets 12
  States { Spawn: PLAY A -1 Stop }
}
actor Fist : Actor { Inventory.Amount 1 Inventory.MaxAmount 1 States { Spawn: PUNG A -1 Stop } }
actor Pistol : Actor { Inventory.Amount 1 Inventory.MaxAmount 1 States { Spawn: PISG A -1 Stop } }
actor Clip : Actor 2007 { Inventory.Amount 10 Inventory.MaxAmount 200 States { Spawn: CLIP A -1 Stop } }
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let clip = classes.find_name("Clip").unwrap();
        assert_eq!(
            classes
                .get(classes.find_name("PlayerPawn").unwrap())
                .unwrap()
                .start_bullets,
            12
        );
        assert_eq!(crate::inventory::count_inventory(&w, mo, clip), 12);
    }

    #[test]
    fn player_hud_reflects_health_and_keys() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let clip = classes.find_name("Clip").unwrap();
        let blue = classes.find_name("BlueCard").unwrap();
        let skull = classes.find_name("BlueSkull").unwrap();
        let before = crate::inventory::count_inventory(&w, mo, clip);
        let _ = crate::inventory::give_inventory(&mut w, mo, clip, 40);
        let _ = crate::inventory::give_inventory(&mut w, mo, blue, 1);
        let _ = crate::inventory::give_inventory(&mut w, mo, skull, 1);
        if let Some(p) = w.players[0].as_mut() {
            p.armor = 50;
            p.god = true;
        }
        let hud = w.player_hud(0).unwrap();
        assert_eq!(hud.ammo, before + 40);
        assert!(hud.ammo_max >= hud.ammo, "ammo_max={}", hud.ammo_max);
        assert_eq!(hud.armor, 50);
        assert_eq!(hud.keys & 1, 1);
        assert_eq!(hud.keys & 8, 8, "blue skull bit");
        assert_eq!(hud.face, "STFGOD0");
        if let Some(p) = w.players[0].as_mut() {
            p.god = false;
            p.invulnerable = true;
        }
        assert_eq!(w.player_hud(0).unwrap().face, "STFGOD0");
    }

    #[test]
    fn player_ticcmd_moves_forward() {
        let mut w = World::from_map(empty_room()).unwrap();
        assert!(w.players[0].is_some());
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let x0 = w.actors.get(mo).unwrap().pos.x();
        let cmd = crate::player::TicCmd {
            forward_move: 8192, // Doom units (×1/2048 in think)
            ..Default::default()
        };
        w.set_ticcmds(&[cmd]);
        w.tick();
        let x1 = w.actors.get(mo).unwrap().pos.x();
        assert!(x1 > x0, "expected forward move {x0} -> {x1}");
    }

    #[test]
    fn player_view_height_from_classdef() {
        let ir = parse_decorate(
            r#"
actor PlayerPawn : Actor 0
{
  Health 100
  Height 56
  ViewHeight 32
  +SOLID
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
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        assert!(
            (classes
                .get(classes.find_name("PlayerPawn").unwrap())
                .unwrap()
                .view_height
                - 32.0)
                .abs()
                < 1e-9
        );
        assert!((w.players[0].as_ref().unwrap().view_height - 32.0).abs() < 1e-9);
    }

    #[test]
    fn kill_monsters_clears_countkill() {
        let ir = parse_decorate(
            r#"
actor ZombieMan : Actor 3004
{
  Health 20
  Monster
  States { Spawn: POSS A -1 Stop Death: POSS H -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        map.things.push(room_map::RawThing {
            tid: 0,
            x: 100,
            y: 100,
            z: 0,
            angle: 0,
            type_id: 3004,
            flags: 0,
            special: 0,
            args: [0; 5],
            hexen: false,
            friendly_see_blocks: -1,
            health: 0,
            score: -2147483648,
            gravity: -1.0,
            scale_x: 0.0,
            scale_y: 0.0,
            alpha: -1.0,
            pitch: -32768,
            roll: -32768,
            fill_color: -1,
            render_style: String::new(),
            float_bob_phase: -1,
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
        });
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        assert_eq!(w.kill_monsters(), 1);
        assert_eq!(w.level.found_kills, 1);
    }

    #[test]
    fn nightmare_respawns_countkill_corpse() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        w.respawn_monsters = true;
        let zombie = classes.find_name("ZombieMan").unwrap();
        let id = w
            .spawn_at(zombie, DVec3::new(200.0, 200.0, 0.0), Angle::ZERO)
            .unwrap();
        let spawn_pt = w.actors.get(id).unwrap().spawn_point;
        assert!((spawn_pt.x() - 200.0).abs() < 1e-9);
        let _ = crate::damage::damage_mobj(&mut w, id, None, None, 9999);
        assert!(w.actors.get(id).unwrap().health <= 0);
        // Jump to final death frame.
        if let Some(a) = w.actors.get_mut(id) {
            a.tics = -1;
            a.move_count = 12 * 35;
        }
        let before = w
            .thinkers
            .default
            .iter()
            .filter(|&&aid| {
                w.actors
                    .get(aid)
                    .is_some_and(|a| a.class == zombie && a.health > 0 && !a.destroyed)
            })
            .count();
        assert_eq!(before, 0);
        let mut revived = false;
        for attempt in 0..512 {
            w.level.time = 32 + attempt * 32;
            // Drive only the corpse thinker path.
            w.advance_actor_state(id);
            if w.actors.get(id).is_some_and(|a| a.destroyed) {
                revived = w.thinkers.default.iter().any(|&aid| {
                    w.actors
                        .get(aid)
                        .is_some_and(|a| a.class == zombie && a.health > 0 && !a.destroyed)
                });
                if revived {
                    break;
                }
            }
        }
        assert!(revived, "nightmare should eventually respawn the zombie");
    }

    #[test]
    fn iddqd_restores_health_to_100() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        w.actors.get_mut(mo).unwrap().health = 17;
        assert!(w.toggle_god(0));
        assert_eq!(w.actors.get(mo).unwrap().health, 100);
        assert!(w.players[0].as_ref().unwrap().god);
        // Toggling off does not change health.
        assert!(!w.toggle_god(0));
        assert_eq!(w.actors.get(mo).unwrap().health, 100);
    }
}

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
use room_core::{Angle, Bam, ClassId, DVec3, StateId};
use room_map::MapGeometry;
use std::sync::Arc;
use thiserror::Error;
use xxhash_rust::xxh3::xxh3_64;

/// Mutable world view handed to a ZScript call while the runtime is split out.
pub(crate) struct WorldZHost<'a> {
    /// World the script may touch (actors, inventory).
    pub world: &'a mut World,
}

/// Player 1 start DoomEdNum.
const THING_PLAYER1_START: i16 = 1;
/// Player 2–4 starts (MBF `-dogs` spawn sites).
const THING_PLAYER2_START: i16 = 2;
const THING_PLAYER3_START: i16 = 3;
const THING_PLAYER4_START: i16 = 4;
const THING_DEATHMATCH_START: i16 = 11;

fn p1_fallback_offset(seat: usize) -> (f64, f64) {
    let i = seat as f64;
    let ang = i * 2.399_963_229_728_653;
    (
        room_core::dmath::cos(ang) * 48.0,
        room_core::dmath::sin(ang) * 48.0,
    )
}

/// Wall-clock UTC `HH:MM:SS.mmmZ` for SHOOT / HIT terminal traces (debug only).
/// `std::time::SystemTime::now` panics on `wasm32-unknown-unknown`, so the
/// browser build gets a placeholder.
fn shoot_log_timestamp() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        "--:--:--.---Z".into()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        shoot_log_timestamp_native()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn shoot_log_timestamp_native() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let Ok(dur) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return "--:--:--.---Z".into();
    };
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    format!("{h:02}:{m:02}:{s:02}.{millis:03}Z")
}
/// MBF helper dog DoomEdNum.
const THING_MBF_HELPER_DOG: i32 = 888;

/// Duke `inven_icon` HUD name (1 kit … 7 boots).
fn duke_inven_name(icon: u8) -> Option<&'static str> {
    Some(match icon {
        1 => "MEDKIT",
        2 => "STEROIDS",
        3 => "HOLODUKE",
        4 => "JETPACK",
        5 => "NIGHT VISION",
        6 => "SCUBA",
        7 => "BOOTS",
        _ => return None,
    })
}

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
    /// Doom `sector_t::soundtarget` — compatibility fallback for `A_Look`.
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

/// Who a [`HudMessage`] is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageTo {
    /// Sim message without a recipient (pickups, ACS `Print`): solo shells
    /// show it; netplay shells cannot tell whose screen it belongs to.
    Unscoped,
    /// Every player.
    All,
    /// One seat (0-based).
    Seat(u8),
}

/// One HUD message for the shell (never read by the sim).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HudMessage {
    /// Recipient.
    pub to: MessageTo,
    /// Text.
    pub text: String,
    /// What kind of message it is (shells let players hide some kinds).
    pub kind: MessageKind,
}

/// Kind of a [`HudMessage`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MessageKind {
    /// Not classified.
    #[default]
    Other,
    /// Item / weapon / ammo pickup.
    Pickup,
    /// Power-up taking effect (berserk, invulnerability, …).
    Power,
    /// Secret found.
    Secret,
    /// Locked door / switch / puzzle item refused.
    Lock,
    /// Death message (obituary).
    Obituary,
    /// Printed by a map or mod script (ACS `Print`, `A_Print`, WASM mods).
    Script,
    /// Engine / cheat feedback (god mode, automap, load errors).
    System,
}

/// One death for shell kill feeds / scoreboards (never read by the sim).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KillEvent {
    /// Player seat of the victim (`None` = monster / object).
    pub victim_player: Option<u8>,
    /// Victim class name.
    pub victim_class: String,
    /// Victim had `COUNTKILL` (counts toward kill %).
    pub victim_countkill: bool,
    /// Player seat credited with the kill.
    pub killer_player: Option<u8>,
    /// Killer class name (empty = world / unknown).
    pub killer_class: String,
}

/// Cap on undrained [`KillEvent`]s (shells that never drain stay bounded).
pub const MAX_KILL_EVENTS: usize = 64;

/// Actors mirrored into script objects for one ZScript call.
pub(crate) type ZscriptSynced = Vec<(ActorId, room_zvm::ObjHandle, crate::zscript::ActorFieldSnap)>;

/// Playsim world (GZDoom `FLevelLocals` + thinkers subset).
///
/// `Clone` exists for rollback snapshots; use [`World::snapshot`], which
/// refuses worlds whose WASM play modules cannot be copied.
#[derive(Clone)]
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
    /// Non-destroyed subset of [`ThinkerLists::default`], same order: rebuilt at
    /// the start of every tic, extended on spawn. Destroyed actors stay on the
    /// thinker list (checksums and demos hash them), so per-move scans read
    /// this instead (`collision_candidates`). Brutal Doom destroys ~8 effect
    /// actors a tic; scanning the full list from every `check_position` made
    /// each tic cost O(moving × everything ever spawned).
    live_ids: Vec<ActorId>,
    /// Length of [`ThinkerLists::default`] after map-thing spawn (before mid-game
    /// missiles / drops). Doom runs levspec lights after these, before later spawns.
    map_thinker_count: usize,
    /// Player slots.
    /// One slot per seat ([`MAX_PLAYERS`] long).
    pub players: Vec<Option<Player>>,
    /// `player->refire` of the slot currently in `player_think` (slot is `take()`n).
    pub(crate) thinking_refire: i32,
    /// Damage type for the next `damage_mobj` (poison); taken by that call.
    pub(crate) forced_damage_type: Option<String>,
    /// Set while `PowerReflection` returns damage (no reflecting the reflection).
    pub(crate) reflecting: bool,
    /// Powers of the player whose thinker is running (that player is out of
    /// `players` meanwhile): pawn and power states.
    pub(crate) thinking_powers: Option<(ActorId, Vec<crate::powerups::PowerState>)>,
    /// Puff class for the hitscan being fired (`LineAttack(…, puff)`); `None` = `BulletPuff`.
    pub(crate) puff_override: Option<String>,
    /// `up_move` of the slot currently in `player_think` (slot is `take()`n).
    pub(crate) thinking_up_move: i16,
    /// Weapon whose psprite action is running: state jumps resolve its labels.
    pub(crate) psprite_weapon: Option<ClassId>,
    /// Current weapon psprite state while its action runs (flash offsets).
    pub(crate) psprite_state: Option<StateId>,
    /// Flash segment index requested by the fire action (`None` = no flash).
    /// Set inside `A_Fire*` after a successful deplete (docs/30 N4/N5).
    pub(crate) psprite_flash_request: Option<usize>,
    /// `A_Overlay*` calls made from actor context while the player is thinking
    /// (ZScript weapon functions): applied once the psprite action returns.
    pub(crate) psp_action_queue: Vec<(ActorId, String, Vec<String>)>,
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
    /// Active walkable rotating floors.
    pub spinners: Vec<crate::specials::FloorSpinner>,
    /// Active locator-following subway / train cars.
    pub subways: Vec<crate::specials::FloorSubway>,
    /// Active `SE 20` stretch / shrink sectors.
    pub stretches: Vec<crate::specials::FloorStretch>,
    /// Active `SE 22` teeth prongs.
    pub teeth: Vec<crate::specials::FloorTeeth>,
    /// Active `SE 16` reactor skews.
    pub skews: Vec<crate::specials::FloorSkew>,
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
    /// While a `CustomInventory` Pickup state runs, give/take inventory on this actor.
    pub(crate) inventory_beneficiary: Option<ActorId>,
    /// Sound events produced this tic (drained by the game shell).
    pub sound_events: Vec<SimSoundEvent>,
    /// Sound names queued when the last tic started (CON `ifsound` / `ifnosounds`).
    pub sound_heard: Vec<String>,
    /// ACS `SetMusic` / `LocalSetMusic` request (lump or SNDINFO name; drained by shell).
    pub music_request: Option<String>,
    /// ACS `SetMusicVolume` (0..=1).
    pub music_volume: f32,
    /// ACS `FadeTo` / `CancelFade` screen overlay (RGBA 0..=1 + remaining tics).
    pub screen_fade: ScreenFade,
    /// ACS `SetFont` current font name (HUD / print bootstrap).
    pub acs_font: String,
    /// Live ACS `HudMessage`s (GZDoom status-bar message list).
    pub hud_messages: Vec<crate::hud_msg::AcsHudMessage>,
    /// Decal events produced this tic (drained by the renderer / shell).
    pub decal_events: Vec<SimDecalEvent>,
    /// ACS `SpawnParticle` events this tic (drained by the shell / renderer).
    pub particle_events: Vec<SimParticleEvent>,
    /// Render-only FX cues (`docs/29` §8.K). Empty unless [`Self::fx_events_enabled`].
    pub fx_events: Vec<crate::fx_events::FxEvent>,
    /// When false (servers / headless), [`Self::push_fx`] is a no-op.
    pub fx_events_enabled: bool,
    /// ACS `SetHUDClipRect` (zero width disables).
    pub hud_clip: HudClipRect,
    /// ACS `SetHUDWrapWidth` (0 = default).
    pub hud_wrap_width: i32,
    /// ACS `SetCVarString` string store (name → value).
    pub acs_cvar_strings: std::collections::BTreeMap<String, String>,
    /// Mod CVars (CVARINFO defaults, overridden from Settings): lower-case
    /// name → value text (`1` / `0` for bools). ACS `GetCVar` / `SetCVar`
    /// and ZScript `CVar` handles read these (see [`World::set_mod_cvar`]).
    pub mod_cvars: std::collections::BTreeMap<String, String>,
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
    /// Drive `DukeCamera` views while a Duke `.DMO` is playing (`SE 27`).
    pub demo_cameras: bool,
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
    /// `PPShader` calls from ZScript for the render shell (render-only).
    pub post_shader_ops: Vec<crate::zs_native_ops::PostShaderOp>,
    /// MAPINFO `skymist` / `ChangeSkyMist` texture (render-only; not in checksum).
    pub sky_mist: Option<String>,
    /// MAPINFO `skymistyscale` (clamped 0.002–544).
    pub sky_mist_yscale: f64,
    /// MAPINFO `useskymist` / `ChangeSkyMist(usemist)` (`LEVEL3_SKYMIST`).
    pub use_sky_mist: bool,
    /// MAPINFO `thickfogdistance` / `SetThickFog` (≤ 0 = off).
    pub thick_fog_distance: f64,
    /// MAPINFO `thickfogmultiplier` / `SetThickFog`.
    pub thick_fog_multiplier: f64,
    /// HUD messages produced this tic (drained by the shell).
    pub messages: Vec<HudMessage>,
    /// Retained Mod HUD widgets (`docs/27` §9). Not part of [`Self::checksum`].
    pub mod_hud: crate::mod_hud::ModHud,
    /// Battle royale zone rings (`docs/28` §9 BE1). Render-only; not in checksum.
    pub mod_zones: crate::mod_zones::ModZones,
    /// Sectors in the level's main area ([`crate::reach`]), built on first use.
    pub reach_area: std::sync::OnceLock<Vec<bool>>,
    /// Lobby / session display names per seat (`docs/28` §9 BE3).
    pub player_names: crate::player_names::PlayerNames,
    /// Diagnostic lines for the drop-down console (drained by the shell).
    pub debug_log: Vec<String>,
    /// Deaths since the last [`World::drain_kill_events`] (shell-only).
    pub kill_events: Vec<KillEvent>,
    /// Per-seat `(kills, items, secrets)` this level, for the scoreboard
    /// (`player_t.killcount` / `itemcount` / `secretcount`). Not in the
    /// checksum.
    pub player_stats: [[u32; 3]; MAX_PLAYERS],
    /// Per player this level: times hit (damage landed) and damage taken.
    pub player_hits: [[u32; 2]; MAX_PLAYERS],
    /// The seat whose player is out of [`Self::players`] while it thinks or
    /// touches items, with its pawn.
    pub(crate) taken_player: Option<(usize, ActorId)>,
    /// Pause flag (menu / pause key).
    pub paused: bool,
    /// Deathmatch rules: players can hurt each other; the status bar shows
    /// frags instead of `STARMS`.
    pub deathmatch: bool,
    /// Teammates may damage each other (`game_set_rule` TEAM_DAMAGE; BE5).
    pub team_damage: bool,
    /// Self damage allowed (`game_set_rule` SELF_DAMAGE; BE5).
    pub self_damage: bool,
    /// Squad DBNO: PvP lethal damage floors player health at 1 (`docs/28` BR6).
    pub squad_dbno: bool,
    /// Dead players stay down instead of respawning with Use (mod rule).
    pub no_player_respawn: bool,
    /// MAPINFO `NoJump`: jumping is off on this map.
    pub no_jump: bool,
    /// MAPINFO `NoCrouch`: crouching is off on this map.
    pub no_crouch: bool,
    /// Sound zones and their reverb environment IDs (render-side only).
    pub sound_env: crate::sound_env::SoundEnvState,
    /// MAPINFO skill `ArmorFactor` (`SKILLP_ArmorFactor`).
    pub armor_factor: f64,
    /// MAPINFO `damagetype` definitions, by lowercase name.
    pub damage_types: std::collections::BTreeMap<String, DamageTypeInfo>,
    /// Netgame weapon stay (`sv_weaponstay` / docs/30 W17): map weapons are not
    /// removed on pickup so each player can take them once. Dropped weapons
    /// always disappear. Mods toggle via `game_set_rule(WEAPON_STAY)`.
    pub weapon_stay: bool,
    /// Set once the match is decided (mod `game_end_match` or the time limit).
    pub match_over: Option<MatchOver>,
    /// Per-seat stats from the last `game_end_match_ex` call (BE11).
    pub match_standings: Vec<MatchStanding>,
    /// Tics left before the room time limit ends the match (`0` = no limit).
    pub match_tics_left: u32,
    /// Bitmask of seats driven by [`crate::bot::run_bot_brains`] (`docs/28` BE8).
    pub bot_seats: u64,
    /// Bot difficulty for every bot seat (`crate::bot::skill`; `0` = normal).
    pub bot_skill: u8,
    /// Seats whose display name was picked by [`World::set_seat_bot`].
    pub bot_named: u64,
    /// Level exits are ignored (mod rule `NO_EXIT`, e.g. Battle Royale).
    pub no_exit: bool,
    /// Inventories saved by [`World::save_player_inventories`]: seat,
    /// items, ready weapon.
    pub saved_inventories: Vec<(usize, Vec<crate::inventory::InvItem>, Option<ClassId>)>,
    /// Level time of each player's latest death (standings tie-break).
    pub player_died_at: [Option<u32>; MAX_PLAYERS],
    /// Doom skill 1..=5 (filters map things).
    pub skill: u8,
    /// Fast monsters (Nightmare or `-fast`); uses `FastSpeed` when set.
    pub fast_monsters: bool,
    /// `sv_fastweapons`. 0 is normal, 1 halves weapon frames, 2 or more makes them 1 tic.
    pub fast_weapons: i32,
    /// Respawn `COUNTKILL` corpses (`-respawn` / Nightmare skill).
    pub respawn_monsters: bool,
    /// Respawn picked-up items (Duke `ud.respawn_items`).
    pub respawn_items: bool,
    /// Picked-up items waiting to come back (`respawn_items`): due tic, class,
    /// spot, angle.
    pub item_respawns: Vec<(u32, ClassId, DVec3, Angle)>,
    /// Respawn inventory pickups (Duke `ud.respawn_inventory`).
    pub respawn_inventory: bool,
    /// No monsters (Duke `ud.monsters_off`).
    pub monsters_off: bool,
    /// Came from a secret exit (Duke `ud.from_bonus`).
    pub from_bonus: bool,
    /// Pending secret level number (Duke `ud.secretlevel`; `0` = none).
    pub secretlevel: i32,
    /// End-of-game flag (Duke `ud.eog`).
    pub eog: bool,
    /// Duke CON `global_random`.
    pub global_random: i32,
    /// Cooperative play (Duke `ud.coop`).
    pub coop: bool,
    /// Friendly fire (Duke `ud.ffire`).
    pub friendly_fire: bool,
    /// Respawn markers (Duke `ud.marker`).
    pub marker: bool,
    /// Previous level index (Duke `ud.last_level`).
    pub last_level: i32,
    /// Active camera sprite (`-1` = none).
    pub camsprite: i32,
    /// Duke `pskybits`.
    pub pskybits: i32,
    /// Duke `pskyoff[8]`.
    pub pskyoff: [i16; 8],
    /// Duke `parallaxyscale`.
    pub parallaxyscale: i32,
    /// Multiplayer connect-list head (`-1` = none).
    pub connecthead: i32,
    /// Multiplayer connect-list next links.
    pub connectpoint2: [i16; 16],
    /// Duke `numplayersprites`.
    pub numplayersprites: i32,
    /// DukeMatch `frags[16][16]`.
    pub duke_frags: [[i16; 16]; 16],
    /// Map came from the Build/Duke converter (its classes carry
    /// `+ROOMSCALEDCOLLISION`): sectors may overlap, so collision tolerates a
    /// centre sector whose floor is out of reach.
    pub build_overlaps: bool,
    /// Duke `animatecnt`.
    pub animatecnt: i32,
    /// Duke `animategoal` (first `animatecnt` slots).
    pub animategoal: Vec<i32>,
    /// Duke `animatevel`.
    pub animatevel: Vec<i32>,
    /// Duke `animateptr`.
    pub animateptr: Vec<i32>,
    /// Duke `numanimwalls`.
    pub numanimwalls: i32,
    /// Duke `spriteqamount`.
    pub spriteqamount: i32,
    /// Duke `numclouds`.
    pub numclouds: i32,
    /// Duke `numcyclers`.
    pub numcyclers: i32,
    /// Duke `gotpic` tile-seen bits.
    pub gotpic: Vec<u8>,
    /// Duke `playerswhenstarted`.
    pub playerswhenstarted: i32,
    /// Round-robin index into live `BossTarget` actors for `A_BrainSpit`.
    pub boss_target_cursor: usize,
    /// `A_BrainSpit` easy toggle (skills 1–2 fire every other cube).
    pub brain_spit_easy: bool,
    /// Event bits WASM mods subscribed to ([`crate::script_mods::ModRuntime`]).
    pub script_event_mask: u64,
    /// Module index currently serving a game import (Mod HUD ownership).
    pub script_mod_index: u16,
    /// Sim events queued this tic for WASM mod handlers.
    pub script_events: Vec<crate::script_mods::ScriptEvent>,
    /// Monster corpse FIFO (`sv_corpsequeuesize`, default 64). Oldest destroyed when full.
    pub corpse_queue: std::collections::VecDeque<ActorId>,
    /// Max retained monster corpses (`0` disables enqueue).
    pub corpse_queue_size: usize,
    /// Actors whose Spawn state lasts 0 tics. GZDoom leaves that state for its
    /// first `Tick` to walk (`--tics <= 0` → `SetState(next)`), and that walk
    /// uses up the tic: the state it lands on keeps its full duration. Room
    /// walks the chain at spawn instead, so the first state-timer decrement is
    /// skipped for these. Without this, Brutal Doom's 2-tic health-bonus flare
    /// (`LENB A 0 … LENB A 2 bright; stop`, respawned every 2 tics) lived one
    /// tic and blinked on/off instead of glowing steadily.
    pub(crate) zero_tic_spawned: std::collections::HashSet<ActorId>,
    /// Actors moved through a portal this tic, for render interpolation
    /// (cleared at the start of each tic; not part of the checksum).
    pub portal_moves: Vec<crate::portal::PortalMove>,
    /// Actors spawned in a `NoDelay` state whose first tick has not run yet
    /// (GZDoom `MF7_HANDLENODELAY`).
    pub(crate) no_delay_pending: std::collections::HashSet<ActorId>,
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
    /// When false, hitscans and player missiles follow pitch (Duke Auto Aim off).
    pub autoaim: bool,
    /// Classic `P_Random` / `M_Random` (used when [`SimMathMode::VanillaFixed`]).
    pub vanilla_rng: VanillaRng,
    /// ACS VM state (`BEHAVIOR` scripts for this map).
    pub acs: room_acs::AcsState,
    /// WASM play modules (M11 `room-script`).
    pub scripts: crate::script_host::ScriptHost,
    /// ZScript play runtime (`docs/34`).
    pub zscript: crate::zscript::ZscriptRuntime,
    /// Actors currently dispatching `WorldThingDestroyed`; prevents removal
    /// actions invoked by handlers from recursively redispatching the event.
    zscript_destroyed_callbacks: std::collections::BTreeSet<u32>,
    /// Cached DECORATE→ZVM expression programs (ZS5).
    pub deco_expr_cache: crate::deco_zvm::DecoExprCache,
}

/// How a match ended (read by shells for the final screen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchOver {
    /// Winning player slot (`None` = draw / no winner).
    pub winner: Option<u8>,
    /// Ended by the room time limit rather than the rules.
    pub time_up: bool,
    /// Level time (tics) the match ended.
    pub maptime: u32,
}

/// Rich standings row from mod `game_end_match_ex` (BE11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchStanding {
    /// Player slot.
    pub seat: u8,
    /// 1-based place.
    pub place: u8,
    /// Frags / kills.
    pub kills: i32,
    /// Assists.
    pub assists: i32,
    /// Damage dealt.
    pub damage: i32,
    /// Revives.
    pub revives: i32,
    /// Survival time in tics.
    pub survival_tics: i32,
}

/// One row of [`World::standings`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Standing {
    /// Player slot.
    pub slot: u8,
    /// 1-based place.
    pub place: u8,
    /// Frags (kills of other players minus suicides).
    pub frags: i32,
    /// Times this player died.
    pub deaths: i32,
    /// Alive when the match ended.
    pub alive: bool,
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

/// How a queued sound event should be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimSoundAdjust {
    /// Start a sound.
    Start,
    /// Start only if the actor channel is not currently playing `check_name`.
    StartIfNotSame,
    /// Change the volume of a sound already playing on this channel.
    Volume,
    /// Change the pitch of a sound already playing on this channel.
    Pitch,
    /// Stop every playing sound.
    StopAll,
    /// Stop every sound started by one actor.
    StopActor,
    /// Stop one actor's sounds on channels `channel ..= channel_flags & 0xff`
    /// (bit 8 of `channel_flags` = every channel), UZDoom `StopActorSounds`.
    StopChannels,
}

/// Lightweight sim sound request (mapped to `room-audio::SoundEvent` by the shell).
#[derive(Clone, Debug, PartialEq)]
pub struct SimSoundEvent {
    /// Logical sound name (SNDINFO key).
    pub name: String,
    /// Optional sound name used by `A_StartSoundIfNotSame` as the active-voice check.
    pub check_name: Option<String>,
    /// Start position within the sample, in seconds.
    pub start_time: f32,
    /// Sound class used to resolve `*name` player sounds.
    pub sound_class: Option<String>,
    /// Player sound gender (`male`, `female`, or `other`).
    pub sound_gender: Option<String>,
    /// Generational actor handle packed as generation:32 | index:32.
    pub source_id: Option<u64>,
    /// World position.
    pub x: f64,
    /// Y.
    pub y: f64,
    /// Z.
    pub z: f64,
    /// Volume 0..=1.
    pub volume: f32,
    /// Playback rate. 1 is nominal.
    pub pitch: f32,
    /// Channel. `A_PlaySound` uses 0.
    pub channel: i32,
    /// Start a voice, or change one that is already playing.
    pub adjust: SimSoundAdjust,
    /// `A_PlaySound(..., looping = true)`.
    pub looping: bool,
    /// `A_PlaySound`'s loop mode also avoids restarting a busy actor channel.
    pub no_restart: bool,
    /// GZDoom `CHANF_*` flags supplied to `A_StartSound`.
    pub channel_flags: u32,
    /// Attenuation scale (`1.0` = normal, `0.0` = full volume / no falloff).
    pub attenuation: f32,
    /// When `Some(seat)`, only that console seat should hear it (Mod HUD UI cue).
    pub seat: Option<u8>,
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
    /// Optional UZDoom legacy render style (`None` keeps Room's existing default).
    pub render_style: Option<i32>,
    /// Velocity applied by the render-owned particle store.
    pub velocity: room_core::DVec3,
    /// Acceleration applied by the render-owned particle store.
    pub acceleration: room_core::DVec3,
    /// Initial particle opacity.
    pub alpha: f64,
    /// Opacity delta; negative values follow `SPF_NEGATIVE_FADESTEP` handling.
    pub fade_step: f64,
    /// Per-tic size delta.
    pub size_step: f64,
    /// UZDoom particle flags (`SPF_*`).
    pub flags: u32,
    /// Opacity step used after fade-in when a fade mode is active.
    pub fade_out_step: f64,
    /// Particle roll: start angle, velocity, and acceleration in degrees.
    pub roll: [f64; 3],
    /// `A_SpawnParticleEx` texture lump (`None` = untextured dot).
    pub texture: Option<String>,
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

/// One extra weapon layer ready to draw (`A_Overlay` ids other than 1, 2, 1000).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudPspLayer {
    /// Layer id: below the gun when negative, over it when positive.
    pub id: i32,
    /// Patch lump.
    pub patch: String,
    /// Offset from the gun's place (with `PSPF_ADDWEAPON`) or from `WEAPONTOP`.
    pub ofs_x: f32,
    /// See [`Self::ofs_x`].
    pub ofs_y: f32,
    /// Final alpha.
    pub alpha: f32,
    /// Scale.
    pub scale_x: f32,
    /// Scale.
    pub scale_y: f32,
    /// `PSPF_FLIP`.
    pub flip: bool,
    /// `PSPF_MIRROR`: offset and sway mirrored about the screen center.
    pub mirror: bool,
    /// Rotation, degrees counter-clockwise.
    pub angle: f32,
    /// Pivot.
    pub pivot_x: f32,
    /// Pivot.
    pub pivot_y: f32,
    /// `PSPF_PIVOTPERCENT`.
    pub pivot_percent: bool,
    /// `PSPF_ADDBOB`.
    pub add_bob: bool,
    /// `PSPF_ADDWEAPON`.
    pub add_weapon: bool,
    /// `PSPF_INTERPOLATE`.
    pub interpolate: bool,
    /// Spectre fuzz.
    pub fuzz: bool,
    /// Frame is `Bright`.
    pub bright: bool,
    /// Translation tint.
    pub tint: (f32, f32, f32),
    /// `A_OverlayVertexOffset` corners.
    pub vertex: [(f32, f32); 4],
}

/// One line of the scoreboard ([`World::scoreboard`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScoreRow {
    /// Player seat.
    pub seat: usize,
    /// Display name.
    pub name: String,
    /// Frags (kills of other players, minus suicides).
    pub frags: i32,
    /// Times killed, by anyone.
    pub deaths: i32,
    /// Monsters killed this level.
    pub kills: u32,
    /// Items picked up this level.
    pub items: u32,
    /// Secrets found this level.
    pub secrets: u32,
}

/// Inventory view for a ZScript status bar ([`World::status_bar_items`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusBarItems {
    /// `(class, amount, max)` of each carried item; armor as `BasicArmor`.
    pub inventory: Vec<(String, i32, i32)>,
    /// `Inventory.Icon` lump per class.
    pub icons: Vec<(String, String)>,
    /// Ready weapon class.
    pub ready_weapon: Option<String>,
    /// Its `AmmoType1`.
    pub ammo1: Option<String>,
    /// Its `AmmoType2`.
    pub ammo2: Option<String>,
    /// Slots 0–9 holding an owned weapon.
    pub slots: [bool; 10],
    /// Selected inventory-bar item (`InvSel`).
    pub inv_sel: Option<String>,
    /// Inventory-bar items in bar order.
    pub inv_bar: Vec<String>,
    /// Tics the inventory bar stays open (`inventorytics`).
    pub inventory_tics: i32,
    /// Every weapon in the weapon slots, owned or not, as `(class, slot,
    /// AmmoType1, AmmoType2)` in slot order (`WeaponSlots.GetWeapon`).
    pub weapons: Vec<(String, u8, Option<String>, Option<String>)>,
    /// Spawn-state sprite lump of carried keys (the alt HUD draws these).
    pub spawn_sprites: Vec<(String, String)>,
    /// `Inventory.MaxAmount` defaults of slot weapons' ammo types.
    pub max_amounts: Vec<(String, i32)>,
    /// Running powers as `(PowerupGiver class, tics left, full tics)` for
    /// every giver that hands one out (SBARINFO `PowerupTime`).
    pub power_tics: Vec<(String, i32, i32)>,
}

/// Status-bar / HUD numbers for one player (shell-facing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerHud {
    /// Extra weapon layers in id order.
    pub layers: Vec<HudPspLayer>,
    /// `PowerTargeter` reticle pieces (`None` = power not running).
    pub targeter: Option<crate::powerups::TargeterHud>,
    /// Hit points.
    pub health: i32,
    /// Armor points.
    pub armor: i32,
    /// Ready ammo count (`ammo1` / primary `Weapon.AmmoType`).
    pub ammo: i32,
    /// Max for the ready ammo type (backpack-aware).
    pub ammo_max: i32,
    /// Secondary ammo (`ammo2` / `Weapon.AmmoType2`); `-1` when unused.
    pub ammo2: i32,
    /// Max for secondary ammo type.
    pub ammo2_max: i32,
    /// Key bits: blue=1, yellow=2, red=4.
    pub keys: u8,
    /// Mugshot lump name.
    pub face: String,
    /// Ready weapon patch lump (`PISGA0`, …).
    pub weapon_patch: String,
    /// Muzzle flash patch (`PISFA0`, …); empty when inactive.
    pub flash_patch: String,
    /// Layer-2 sprite from `A_Overlay`. Empty draws nothing.
    pub overlay_patch: String,
    /// Layer-2 extra screen X from `A_OverlayOffset`.
    pub overlay_ofs_x: f32,
    /// Layer-2 extra screen Y from `A_OverlayOffset`.
    pub overlay_ofs_y: f32,
    /// Layer-2 opacity. 1 until `A_OverlayAlpha`.
    pub overlay_alpha: f32,
    /// Layer-2 scale. 1 until `A_OverlayScale`.
    pub overlay_scale_x: f32,
    /// Layer-2 scale Y.
    pub overlay_scale_y: f32,
    /// `PSPF_FLIP` on the layer-2 overlay.
    pub overlay_flip: bool,
    /// `PSPF_MIRROR` on the layer-2 overlay.
    pub overlay_mirror: bool,
    /// Layer-2 rotation in degrees. 0 until `A_OverlayRotate`.
    pub overlay_angle: f32,
    /// Layer-2 scale pivot. 0.5 is the center until `A_OverlayPivot`.
    pub overlay_pivot_x: f32,
    /// Layer-2 scale pivot Y.
    pub overlay_pivot_y: f32,
    /// Layer-2 tint red. 1 until `A_OverlayTranslation`.
    pub overlay_tint_r: f32,
    /// Layer-2 tint green.
    pub overlay_tint_g: f32,
    /// Layer-2 tint blue.
    pub overlay_tint_b: f32,
    /// Layer-2 fuzz from `STYLE_Fuzzy` / `STYLE_OptFuzzy`.
    pub overlay_fuzz: bool,
    /// `PSPF_PIVOTPERCENT` on the layer-2 overlay.
    pub overlay_pivot_percent: bool,
    /// `PSPF_ADDBOB` on the layer-2 overlay.
    pub overlay_add_bob: bool,
    /// `PSPF_ADDWEAPON` on the layer-2 overlay.
    pub overlay_add_weapon: bool,
    /// `PSPF_INTERPOLATE` on the layer-2 overlay.
    pub overlay_interpolate: bool,
    /// Sector light (0–1) + extralight for the gun (docs/30 W6).
    pub weapon_light: f32,
    /// Ready/fire frame has `Bright` (fullbright psprite).
    pub weapon_bright: bool,
    /// Flash frame has `Bright`.
    pub flash_bright: bool,
    /// Partial invisibility → fuzz the psprite.
    pub weapon_fuzz: bool,
    /// Weapon-layer opacity 0–1 (`A_OverlayAlpha`). 1 until a script sets it.
    pub weapon_alpha: f32,
    /// Weapon-layer scale (`A_OverlayScale`). 1 until a script sets it.
    pub weapon_scale_x: f32,
    /// Weapon-layer scale Y. A Y of 0 in the action copies X.
    pub weapon_scale_y: f32,
    /// `PSPF_FLIP` on the weapon layer.
    pub weapon_flip: bool,
    /// `PSPF_MIRROR` on the weapon layer.
    pub weapon_mirror: bool,
    /// `PSPF_PIVOTPERCENT`. False means the pivot is in sprite pixels.
    pub weapon_pivot_percent: bool,
    /// `PSPF_INTERPOLATE`. False snaps the gun offset each tic.
    pub weapon_interpolate: bool,
    /// Scale pivot 0–1. 0.5 is the patch center until `A_OverlayPivot`.
    pub weapon_pivot_x: f32,
    /// Scale pivot Y.
    pub weapon_pivot_y: f32,
    /// Weapon-layer rotation in degrees (`A_OverlayRotate`). 0 is upright.
    pub weapon_angle: f32,
    /// Inventory-bar items as `(icon lump, amount)`, in bar order.
    pub inv_bar: Vec<(String, i32)>,
    /// Index of the selected item in [`Self::inv_bar`].
    pub inv_sel: Option<usize>,
    /// The bar is open (`inventorytics` above 0).
    pub inv_bar_open: bool,
    /// Weapon-layer corner offsets (`A_OverlayVertexOffset`; TL, BL, TR, BR).
    pub weapon_vertex: [(f32, f32); 4],
    /// Layer-2 corner offsets.
    pub overlay_vertex: [(f32, f32); 4],
    /// Ready weapon class name, for `MODELDEF ViewModelFOV`.
    pub weapon_class: String,
    /// Pawn class display name (`Player.DisplayName`, else the class name),
    /// for SBARINFO `PlayerClass`.
    pub player_class: String,
    /// Worn armor class name (SBARINFO `ArmorType`); empty when none.
    pub armor_type: String,
    /// `A_SetCrosshair` index. 0 draws no mark.
    pub crosshair: i32,
    /// Weapon tint from `A_OverlayTranslation`. 1 is untinted.
    pub weapon_tint_r: f32,
    /// Weapon tint green.
    pub weapon_tint_g: f32,
    /// Weapon tint blue.
    pub weapon_tint_b: f32,
    /// Weapon offset baked into `bob_x` (`A_WeaponOffset` when `PSPF_ADDWEAPON` is on).
    pub bob_ofs_x: f32,
    /// Weapon offset baked into `bob_y`.
    pub bob_ofs_y: f32,
    /// Weapon bob X offset (screen pixels, Doom 320 space).
    pub bob_x: f32,
    /// Weapon bob Y offset.
    pub bob_y: f32,
    /// God mode active.
    pub god: bool,
    /// Owned weapon slots (bit `n` = slot `n`, 2..=9; `STARMS` shows 2..=7).
    pub arms: u16,
    /// `[count, max]` for Clip / Shell / RocketAmmo / Cell (STBAR ammo table).
    pub ammo_table: [[i32; 2]; 4],
    /// Screen-flash PLAYPAL index ([`crate::palette_index`]).
    pub palette: u8,
    /// Doom `fixedcolormap` ([`Player::fixed_colormap`]).
    pub colormap: u8,
    /// Deathmatch: draw [`Self::frags`] instead of `STARMS`.
    pub deathmatch: bool,
    /// `st_fragscount`: kills of other players minus suicides.
    pub frags: i32,
    /// Netgame face background `STFB<n>` (player slot), `None` in single player.
    pub face_back: Option<u8>,
}

/// Doom `R_DrawPSprite` light: `scalelight[(lightlevel>>4)+extralight][MAXLIGHTSCALE-1]`
/// as a 0..=1 RGB scale (docs/30 N8).
///
/// Mirrors linuxdoom `R_InitLightTables`: `startmap = (15-i)*4`, then
/// `level = clamp(startmap - 23, 0, 31)` for column `MAXLIGHTSCALE-1` (47).
#[must_use]
pub fn psprite_scalelight(sector_light: i32, extralight: i32) -> f32 {
    const LIGHTLEVELS: i32 = 16;
    const NUMCOLORMAPS: i32 = 32;
    let lightnum = ((sector_light >> 4) + extralight).clamp(0, LIGHTLEVELS - 1);
    let startmap = (LIGHTLEVELS - 1 - lightnum) * 2 * NUMCOLORMAPS / LIGHTLEVELS;
    // `j/DISTMAP` with j = MAXLIGHTSCALE-1 = 47, DISTMAP = 2 → 23.
    let level = (startmap - 23).clamp(0, NUMCOLORMAPS - 1);
    1.0 - (level as f32) / (NUMCOLORMAPS as f32)
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

/// Named weapon tints. Anything else, including empty, stays white.
fn weapon_translation_tint(name: &str) -> (f32, f32, f32) {
    match name {
        "Green" | "green" => (0.25, 1.0, 0.25),
        "Red" | "red" => (1.0, 0.25, 0.25),
        "Ice" | "Blue" | "blue" => (0.35, 0.65, 1.0),
        _ => (1.0, 1.0, 1.0),
    }
}

/// Build a Doom sprite lump name from psprite sprite + frame (`PISG`/`B` → `PISGB0`).
/// `None` when the state names no sprite; `Some("")` for the invisible `TNT1`.
fn weapon_patch_from_sprite_frame(sprite: &str, frame: u8) -> Option<String> {
    let s = sprite.trim().to_ascii_uppercase();
    if s.is_empty() {
        return None;
    }
    if s == "TNT1" {
        return Some(String::new());
    }
    let letter = match frame {
        0..=25 => char::from(b'A' + frame),
        26 => '[',
        27 => '\\',
        28 => ']',
        _ => 'A',
    };
    Some(format!("{s}{letter}0"))
}

/// ZDoom `ML_REPEAT_SPECIAL` — keep special after successful activation.
pub const ML_REPEAT_SPECIAL: i32 = 0x0200;
/// MAPINFO `damagetype` behaviour the playsim uses (UZDoom `DamageTypeDefinition`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DamageTypeInfo {
    /// `NoArmor`.
    pub no_armor: bool,
    /// `Factor` (default damage factor for actors without a typed one).
    pub factor: Option<f64>,
    /// `ReplaceFactor`.
    pub replace_factor: bool,
}

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
/// GZDoom `ML_BLOCKHITSCAN` (UDMF `blockhitscan`).
pub const ML_BLOCKHITSCAN: i32 = 0x0800_0000;

/// Map flags of `line` as runtime [`World::line_flags`] bits.
#[must_use]
pub fn base_line_flags(line: &room_map::Line) -> i32 {
    let hitscan = if line.block_hitscan {
        ML_BLOCKHITSCAN
    } else {
        0
    };
    i32::from(line.flags) | hitscan
}

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
    /// Blocks hitscans (`ML_BLOCKHITSCAN`).
    pub const HITSCAN: i32 = 512;
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

    /// Load a compiled ZScript program and merge any `States` bodies into the
    /// world's [`StateTable`] (ZS3).
    pub fn load_zscript_program(&mut self, program: room_zvm::CompiledProgram) {
        self.load_zscript_program_inner(program, false);
    }

    /// Like [`load_zscript_program`](Self::load_zscript_program), and stub natives
    /// the builtin table does not implement. Used for the UZDoom stdlib.
    pub fn load_zscript_program_stubbed(&mut self, program: room_zvm::CompiledProgram) {
        self.load_zscript_program_inner(program, true);
    }

    fn load_zscript_program_inner(
        &mut self,
        program: room_zvm::CompiledProgram,
        stub_missing: bool,
    ) {
        let class_states = program.class_states.clone();
        let class_defaults = program.class_defaults.clone();
        let layouts: Vec<(String, String)> = program
            .classes
            .iter()
            .map(|c| (c.name.clone(), c.parent.clone()))
            .collect();
        if stub_missing {
            self.zscript.load_program_stubbed(program);
        } else {
            self.zscript.load_program(program);
        }
        {
            let classes = std::sync::Arc::make_mut(&mut self.classes);
            classes.register_zscript_classes(&layouts);
        }
        if !class_defaults.is_empty() {
            let classes = std::sync::Arc::make_mut(&mut self.classes);
            classes.merge_zscript_defaults(&class_defaults);
        }
        if !class_states.is_empty() {
            let states = std::sync::Arc::make_mut(&mut self.states);
            states.merge_zscript_states(&self.classes, &class_states);
        }
        self.sync_zscript_class_tables();
    }

    /// Publish class replacements / parents and the weapon slots to ZScript
    /// natives (`GetReplacement`, `GetParentClass`, `SlotSize`, …).
    pub fn sync_zscript_class_tables(&mut self) {
        if !self.zscript.is_active() {
            return;
        }
        let mut replacements = std::collections::BTreeMap::new();
        let mut parents = std::collections::BTreeMap::new();
        for def in self.classes.iter() {
            let name = def.name.to_ascii_lowercase();
            if let Some(pid) = def.parent {
                if let Some(p) = self.classes.get(pid) {
                    parents.insert(name.clone(), p.name.to_ascii_lowercase());
                }
            }
            let rep = self.classes.replacement(def.id);
            if rep != def.id {
                if let Some(r) = self.classes.get(rep) {
                    replacements.insert(name, r.name.clone());
                }
            }
        }
        let slots: Vec<Vec<String>> = (0u8..10)
            .map(|k| self.weapon_slots.slots.get(&k).cloned().unwrap_or_default())
            .collect();
        self.zscript.set_class_tables(replacements, parents, slots);
    }

    /// Fill the ZScript actor census when the program uses `CheckProximity`.
    fn sync_zscript_census(&mut self) {
        if !self.zscript.is_active() || !self.zscript.needs_census() {
            return;
        }
        let census = self
            .thinkers
            .default
            .iter()
            .filter_map(|&id| {
                let a = self.actors.get(id)?;
                if a.destroyed || a.health <= 0 {
                    return None;
                }
                let class = self.classes.get(a.class)?.name.to_ascii_lowercase();
                Some((class, [a.pos.x(), a.pos.y(), a.pos.z()]))
            })
            .collect();
        self.zscript.set_census(census);
    }

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
        // Not in the browser: there is no terminal and no wall clock (std time panics).
        if !cfg!(test) && !cfg!(target_arch = "wasm32") {
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
            let dt = self
                .recent_shots
                .iter()
                .rev()
                .find(|s| s.source == shot.source)
                .map(|s| shot.tic.saturating_sub(s.tic));
            let dt_s = dt.map(|d| format!(" Δ{d}")).unwrap_or_default();
            eprintln!(
                "room: [{ts}] SHOOT @{tic}{dt_s} {kind}{missile} {src_seat} {}#{} ({:.0},{:.0},{:.0}) → {dst} ({:.0},{:.0},{:.0}) dist={dist:.0}{dmg}",
                shot.source_class,
                shot.source.index(),
                shot.from[0],
                shot.from[1],
                shot.from[2],
                shot.to[0],
                shot.to[1],
                shot.to[2],
                ts = shoot_log_timestamp(),
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
            .map(base_line_flags)
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

    /// Like `from_map_with_skill`, with an explicit math mode so vanilla demo
    /// `P_Random` runs during map-thing / light spawn (Doom: things then specials).
    pub fn from_map_with_skill_mode(
        map: MapGeometry,
        classes: Arc<ClassRegistry>,
        states: Arc<StateTable>,
        skill: u8,
        math_mode: SimMathMode,
    ) -> Result<Self, SimError> {
        Self::from_map_with_slots(
            map,
            classes,
            states,
            skill,
            math_mode,
            crate::weapon_slots::WeaponSlots::with_doom_defaults(),
        )
    }

    /// Like [`Self::from_map_with_skill_mode`] with the KEYCONF weapon slots /
    /// player classes in place before the player starts spawn.
    pub fn from_map_with_slots(
        map: MapGeometry,
        classes: Arc<ClassRegistry>,
        states: Arc<StateTable>,
        skill: u8,
        math_mode: SimMathMode,
        weapon_slots: crate::weapon_slots::WeaponSlots,
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
        let build_overlaps = classes
            .iter()
            .any(|c| c.extra_flags.get_name("ROOMSCALEDCOLLISION"));
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
            live_ids: Vec::new(),
            players: vec![None; MAX_PLAYERS],
            thinking_refire: 0,
            forced_damage_type: None,
            reflecting: false,
            thinking_powers: None,
            puff_override: None,
            thinking_up_move: 0,
            psprite_weapon: None,
            psprite_state: None,
            psprite_flash_request: None,
            psp_action_queue: Vec::new(),
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
            spinners: Vec::new(),
            subways: Vec::new(),
            stretches: Vec::new(),
            teeth: Vec::new(),
            skews: Vec::new(),
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
            weapon_slots,
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
            inventory_beneficiary: None,
            sound_events: Vec::new(),
            sound_heard: Vec::new(),
            music_request: None,
            music_volume: 1.0,
            screen_fade: ScreenFade::default(),
            acs_font: String::new(),
            hud_messages: Vec::new(),
            decal_events: Vec::new(),
            particle_events: Vec::new(),
            fx_events: Vec::new(),
            fx_events_enabled: false,
            hud_clip: HudClipRect::default(),
            hud_wrap_width: 0,
            acs_cvar_strings: std::collections::BTreeMap::new(),
            mod_cvars: std::collections::BTreeMap::new(),
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
            demo_cameras: false,
            sky1_override: None,
            sky2_override: None,
            sky_speed1: 0.0,
            sky_speed2: 0.0,
            sky_mist_speed: 0.0,
            post_shader_ops: Vec::new(),
            sky_mist: None,
            sky_mist_yscale: 1.0,
            use_sky_mist: false,
            thick_fog_distance: -1.0,
            thick_fog_multiplier: 30.0,
            messages: Vec::new(),
            mod_hud: crate::mod_hud::ModHud::new(),
            mod_zones: crate::mod_zones::ModZones::new(),
            reach_area: std::sync::OnceLock::new(),
            player_names: crate::player_names::PlayerNames::default(),
            debug_log: Vec::new(),
            kill_events: Vec::new(),
            player_stats: [[0; 3]; MAX_PLAYERS],
            player_hits: [[0; 2]; MAX_PLAYERS],
            taken_player: None,
            paused: false,
            deathmatch: false,
            team_damage: true,
            self_damage: true,
            squad_dbno: false,
            no_player_respawn: false,
            no_jump: false,
            no_crouch: false,
            sound_env: crate::sound_env::SoundEnvState::default(),
            armor_factor: 1.0,
            damage_types: std::collections::BTreeMap::new(),
            weapon_stay: false,
            match_over: None,
            match_standings: Vec::new(),
            match_tics_left: 0,
            bot_seats: 0,
            bot_skill: 0,
            bot_named: 0,
            no_exit: false,
            saved_inventories: Vec::new(),
            player_died_at: [None; MAX_PLAYERS],
            skill: skill.clamp(1, 5),
            fast_monsters: skill >= 5,
            fast_weapons: 0,
            respawn_monsters: skill >= 5,
            respawn_items: false,
            item_respawns: Vec::new(),
            respawn_inventory: false,
            monsters_off: false,
            from_bonus: false,
            secretlevel: 0,
            eog: false,
            global_random: 0,
            coop: false,
            friendly_fire: false,
            marker: false,
            last_level: 0,
            camsprite: -1,
            pskybits: 0,
            pskyoff: [0; 8],
            parallaxyscale: 0,
            connecthead: -1,
            connectpoint2: [-1; 16],
            numplayersprites: 0,
            duke_frags: [[0; 16]; 16],
            build_overlaps,
            animatecnt: 0,
            animategoal: Vec::new(),
            animatevel: Vec::new(),
            animateptr: Vec::new(),
            numanimwalls: 0,
            spriteqamount: 0,
            numclouds: 0,
            numcyclers: 0,
            gotpic: Vec::new(),
            playerswhenstarted: 1,
            boss_target_cursor: 0,
            brain_spit_easy: false,
            script_event_mask: 0,
            script_mod_index: 0,
            script_events: Vec::new(),
            corpse_queue: std::collections::VecDeque::new(),
            corpse_queue_size: DEFAULT_CORPSE_QUEUE_SIZE,
            zero_tic_spawned: std::collections::HashSet::new(),
            portal_moves: Vec::new(),
            no_delay_pending: std::collections::HashSet::new(),
            body_queue: std::collections::VecDeque::new(),
            body_queue_size: DEFAULT_BODY_QUEUE_SIZE,
            compat: CompatFlags::default(),
            boss_actions: Vec::new(),
            last_damage: None,
            recent_shots: std::collections::VecDeque::new(),
            math_mode,
            autoaim: true,
            acs: room_acs::AcsState::new(),
            scripts: crate::script_host::ScriptHost::new(),
            zscript: crate::zscript::ZscriptRuntime::default(),
            zscript_destroyed_callbacks: std::collections::BTreeSet::new(),
            deco_expr_cache: crate::deco_zvm::DecoExprCache::default(),
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
        crate::specials::init_udmf_floor_spinners(&mut world);
        crate::specials::init_udmf_floor_subways(&mut world);
        crate::specials::init_udmf_floor_stretches(&mut world);
        crate::specials::init_udmf_floor_teeth(&mut world);
        crate::specials::init_udmf_floor_skews(&mut world);
        crate::specials::init_transfers(&mut world);
        world.spawn_map_things()?;
        world.init_sound_zones(0);
        world.map_thinker_count = world.thinkers.default.len();
        crate::specials::init_security_camera_textures(&mut world);
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

    /// Run `PlayerEntered` and apply `GiveInventory` grants to that player.
    pub fn fire_player_entered(&mut self, player_number: i32) {
        self.fire_player_entered_with_return(player_number, false);
    }

    fn fire_player_entered_with_return(&mut self, player_number: i32, from_hub: bool) {
        if self.zscript.event_handlers.is_empty() || !self.zscript.is_active() {
            return;
        }
        self.zscript.sync_host(self.skill, player_number, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        let _ = self.zscript.notify_player_entered(player_number, from_hub);
        let grants = self.zscript.take_grants();
        let Some(mo) = self
            .players
            .get(player_number as usize)
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        for (cookie, item, amount) in grants {
            self.apply_zscript_inventory_change(mo, cookie, &item, amount);
        }
    }

    /// Notify ZScript handlers after a player pawn has spawned and apply queued
    /// actor operations and inventory grants to that pawn.
    pub fn fire_player_spawned(&mut self, player_number: i32) {
        let Some(mo) = self
            .players
            .get(player_number as usize)
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        self.fire_player_spawned_at(player_number, mo);
    }

    fn fire_player_spawned_at(&mut self, player_number: i32, mo: ActorId) {
        if self.zscript.event_handlers.is_empty() || !self.zscript.is_active() {
            return;
        }
        self.zscript.sync_host(self.skill, player_number, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        let _ = self.zscript.notify_player_spawned(player_number);
        crate::action::apply_zscript_actor_ops(self, mo);
        for (cookie, item, amount) in self.zscript.take_grants() {
            self.apply_zscript_inventory_change(mo, cookie, &item, amount);
        }
    }

    /// Dispatch post-respawn ZScript callbacks for a pawn temporarily removed
    /// from `players` while its thinker is executing.
    pub(crate) fn fire_player_respawned(&mut self, player_number: i32, mo: ActorId) {
        if self.zscript.event_handlers.is_empty() || !self.zscript.is_active() {
            return;
        }
        self.zscript.sync_host(self.skill, player_number, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        let _ = self.zscript.notify_player_respawned(player_number);
        crate::action::apply_zscript_actor_ops(self, mo);
        for (cookie, item, amount) in self.zscript.take_grants() {
            self.apply_zscript_inventory_change(mo, cookie, &item, amount);
        }
    }

    pub(crate) fn fire_player_respawning(&mut self, player_number: i32, mo: ActorId) -> bool {
        if self.zscript.event_handlers.is_empty() || !self.zscript.is_active() {
            return true;
        }
        self.zscript.sync_host(self.skill, player_number, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        let allowed = self
            .zscript
            .notify_player_respawning(player_number)
            .unwrap_or(true);
        crate::action::apply_zscript_actor_ops(self, mo);
        for (cookie, item, amount) in self.zscript.take_grants() {
            self.apply_zscript_inventory_change(mo, cookie, &item, amount);
        }
        allowed
    }

    pub(crate) fn fire_player_died(&mut self, player_number: i32, mo: ActorId) {
        if self.zscript.event_handlers.is_empty() || !self.zscript.is_active() {
            return;
        }
        self.zscript.sync_host(self.skill, player_number, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        let _ = self.zscript.notify_player_died(player_number);
        crate::action::apply_zscript_actor_ops(self, mo);
        for (cookie, item, amount) in self.zscript.take_grants() {
            self.apply_zscript_inventory_change(mo, cookie, &item, amount);
        }
    }

    /// Run MAPINFO ZScript `WorldTick` handlers and apply their queued actor
    /// inventory changes. Handler activation is lazy for hosts that attach
    /// their ZScript program after constructing the world.
    fn fire_zscript_world_tick(&mut self) {
        if !self.zscript.is_active() || self.zscript.event_handlers.is_empty() {
            return;
        }
        self.zscript.sync_host(self.skill, 0, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        if !self.zscript.map_handlers_active {
            let _ = self.zscript.activate_map_handlers();
        }
        let _ = self.zscript.notify_world_tick();
        self.apply_zscript_level_ops();
        self.apply_zscript_native_ops(None);
        let grants = self.zscript.take_grants();
        if let Some(player) = self.players.first().and_then(Option::as_ref)
            && let Some(mo) = player.mo
        {
            for (cookie, item, amount) in grants {
                self.apply_zscript_inventory_change(mo, cookie, &item, amount);
            }
        }
    }

    /// Deliver map teardown events and apply inventory effects before unload.
    pub fn fire_zscript_world_unloaded(&mut self) {
        if !self.zscript.is_active() || self.zscript.event_handlers.is_empty() {
            return;
        }
        self.zscript.sync_host(self.skill, 0, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        let _ = self.zscript.unload_map_handlers();
        let grants = self.zscript.take_grants();
        if let Some(player) = self.players.first().and_then(Option::as_ref)
            && let Some(mo) = player.mo
        {
            for (cookie, item, amount) in grants {
                self.apply_zscript_inventory_change(mo, cookie, &item, amount);
            }
        }
    }

    /// Dispatch a killed actor to MAPINFO ZScript handlers and apply effects.
    pub(crate) fn zscript_actor_event_info(
        &self,
        id: ActorId,
    ) -> Option<crate::zscript::ZscriptActorEventInfo> {
        let actor = self.actors.get(id)?;
        Some(crate::zscript::ZscriptActorEventInfo {
            class_name: self
                .classes
                .get(actor.class)
                .map(|class| class.name.clone())
                .unwrap_or_default(),
            health: actor.health,
            tid: actor.tid,
            is_monster: actor.flags & crate::class::FLAG_COUNTKILL != 0,
        })
    }

    pub(crate) fn fire_zscript_thing_died(&mut self, target: ActorId, inflictor: Option<ActorId>) {
        if !self.zscript.is_active() || self.zscript.event_handlers.is_empty() {
            return;
        }
        let Some(thing_info) = self.zscript_actor_event_info(target) else {
            return;
        };
        let inflictor_info = inflictor.and_then(|id| self.zscript_actor_event_info(id));
        self.zscript.sync_host(self.skill, 0, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        if !self.zscript.map_handlers_active {
            let _ = self.zscript.activate_map_handlers();
        }
        let _ = self
            .zscript
            .notify_thing_died(&thing_info, inflictor_info.as_ref());
        crate::action::apply_zscript_actor_ops(self, target);
        for (cookie, item, amount) in self.zscript.take_grants() {
            self.apply_zscript_inventory_change(target, cookie, &item, amount);
        }
    }

    pub(crate) fn fire_zscript_thing_destroyed(&mut self, target: ActorId) {
        if !self.zscript.is_active() || self.zscript.event_handlers.is_empty() {
            return;
        }
        if !self.zscript_destroyed_callbacks.insert(target.index()) {
            return;
        }
        let Some(thing_info) = self.zscript_actor_event_info(target) else {
            self.zscript_destroyed_callbacks.remove(&target.index());
            return;
        };
        self.zscript.sync_host(self.skill, 0, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        if !self.zscript.map_handlers_active {
            let _ = self.zscript.activate_map_handlers();
        }
        let _ = self.zscript.notify_thing_destroyed(&thing_info);
        crate::action::apply_zscript_actor_ops(self, target);
        for (cookie, item, amount) in self.zscript.take_grants() {
            self.apply_zscript_inventory_change(target, cookie, &item, amount);
        }
        self.zscript_destroyed_callbacks.remove(&target.index());
    }

    pub(crate) fn fire_zscript_thing_revived(&mut self, target: ActorId) {
        if !self.zscript.is_active() {
            return;
        }
        let Some(thing_info) = self.zscript_actor_event_info(target) else {
            return;
        };
        let class_name = thing_info.class_name.clone();
        self.zscript.sync_host(self.skill, 0, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        if self
            .zscript
            .resolve_method(&class_name, "OnRevive")
            .is_some()
        {
            let candidate = crate::action::zscript_resurrection_candidate(self, target)
                .map(|id| (id.index(), id.generation()));
            self.zscript
                .set_ai_observers(self.zscript_ai_observers(target));
            self.zscript.set_resurrection_candidate(candidate);
            let _ = self
                .zscript
                .call_actor_virtual(target.index(), &class_name, "OnRevive");
            crate::action::apply_zscript_actor_ops(self, target);
        }
        if !self.zscript.event_handlers.is_empty() {
            if !self.zscript.map_handlers_active {
                let _ = self.zscript.activate_map_handlers();
            }
            let _ = self.zscript.notify_thing_revived(&thing_info);
            crate::action::apply_zscript_actor_ops(self, target);
        }
    }

    pub(crate) fn fire_zscript_thing_damaged(
        &mut self,
        target: ActorId,
        inflictor: Option<ActorId>,
        source: Option<ActorId>,
        damage: i32,
        damage_type: &str,
    ) {
        if !self.zscript.is_active() || self.zscript.event_handlers.is_empty() {
            return;
        }
        let Some(thing_info) = self.zscript_actor_event_info(target) else {
            return;
        };
        let inflictor_info = inflictor.and_then(|id| self.zscript_actor_event_info(id));
        let source_info = source.and_then(|id| self.zscript_actor_event_info(id));
        self.zscript.sync_host(self.skill, 0, 0);
        self.zscript.set_level_checksum(self.map.md5);
        self.zscript_publish_level();
        if !self.zscript.map_handlers_active {
            let _ = self.zscript.activate_map_handlers();
        }
        let _ = self.zscript.notify_thing_damaged(
            &thing_info,
            inflictor_info.as_ref(),
            source_info.as_ref(),
            damage,
            damage_type,
        );
        crate::action::apply_zscript_actor_ops(self, target);
        for (cookie, item, amount) in self.zscript.take_grants() {
            self.apply_zscript_inventory_change(target, cookie, &item, amount);
        }
    }

    /// After ZScript handlers attach, grant items the level-start scripts would
    /// have given: the player, and monsters already placed on the map.
    pub fn arm_zscript_level(&mut self) {
        self.arm_zscript_level_with_return(false);
    }

    /// Arm event callbacks with the map's hub-return state.
    pub fn arm_zscript_level_with_return(&mut self, from_hub: bool) {
        if self.zscript.event_handlers.is_empty() || !self.zscript.is_active() {
            return;
        }
        for seat in 0..self.players.len() {
            if self.players[seat].is_some() {
                self.fire_player_entered_with_return(seat as i32, from_hub);
                self.fire_player_spawned(seat as i32);
            }
        }
        let player_mos: Vec<_> = self.players.iter().flatten().filter_map(|p| p.mo).collect();
        let monsters: Vec<_> = self
            .thinkers
            .default
            .iter()
            .copied()
            .filter(|&id| {
                !player_mos.contains(&id)
                    && self
                        .actors
                        .get(id)
                        .is_some_and(|a| (a.flags & crate::class::FLAG_COUNTKILL) != 0)
            })
            .collect();
        for id in monsters {
            self.zscript.sync_host(self.skill, 0, 0);
            self.zscript.set_level_checksum(self.map.md5);
            self.zscript_publish_level();
            if let Some(info) = self.zscript_actor_event_info(id) {
                let _ = self.zscript.notify_thing_spawned(&info);
            }
            crate::action::apply_zscript_actor_ops(self, id);
            let grants = self.zscript.take_grants();
            for (cookie, item, amount) in grants {
                self.apply_zscript_inventory_change(id, cookie, &item, amount);
            }
        }
    }

    /// Run a script closure with the runtime split out of `self`, so the host
    /// can touch world actors while natives run (`docs/34` §14.3.3).
    pub(crate) fn with_zscript<R>(
        &mut self,
        f: impl FnOnce(&mut crate::zscript::ZscriptRuntime, &mut WorldZHost<'_>) -> R,
    ) -> Option<R> {
        let mut rt = std::mem::take(&mut self.zscript);
        if !rt.is_active() {
            self.zscript = rt;
            return None;
        }
        let r = {
            let mut host = WorldZHost { world: self };
            f(&mut rt, &mut host)
        };
        self.zscript = rt;
        Some(r)
    }

    /// Copy script player TIDs onto the possessed actors.
    pub fn apply_script_player_tids(&mut self) {
        let _ = self.with_zscript(|rt, host| {
            let map = rt.player_tids();
            for (pn, tid) in map {
                let Some(mo) = host
                    .world
                    .players
                    .get(pn as usize)
                    .and_then(|p| p.as_ref())
                    .and_then(|p| p.mo)
                else {
                    continue;
                };
                if let Some(actor) = host.world.actors.get_mut(mo) {
                    actor.tid = tid;
                }
            }
        });
    }

    /// Apply `GiveInventory` / `TakeInventory` grants and `SoundClass` to a player.
    pub fn apply_script_player_effects(&mut self, player_number: i32) {
        let grants = self.zscript.take_grants();
        let Some(mo) = self
            .players
            .get(player_number as usize)
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        for (cookie, item, amount) in grants {
            self.apply_zscript_inventory_change(mo, cookie, &item, amount);
        }
        if let Some(class) = self.zscript.take_sound_class(player_number) {
            if let Some(actor) = self.actors.get_mut(mo) {
                actor.sound_class = class;
            }
        }
    }

    fn spawn_map_things(&mut self) -> Result<(), SimError> {
        let things = self.map.things.clone();
        for (thing_index, thing) in things.iter().enumerate() {
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
                let class = self.player_class();
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
                    let _ = self.give_starting_loadout(id);
                }
                continue;
            }
            let Some(class) = self.classes.find_doomednum(i32::from(thing.type_id)) else {
                continue;
            };
            let z = self.map_thing_z(class, thing);
            let id = self.spawn_at(
                class,
                DVec3::new(f64::from(thing.x), f64::from(thing.y), z),
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
            self.apply_thing_ext(id, self.map.udmf_user.thing_ext(thing_index));
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
                // Map-thing scale (UDMF `scalex/scaley`, Build `xrepeat/yrepeat`
                // from the Duke converter) also sizes the collision box: the
                // converted Build maps rely on it to keep sprites clear of walls.
                // Class `Scale` / `A_SetScale` stay visual only (GZDoom).
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
                a.ext.spawn_flags = thing.flags as u16 as u32;
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
            // Build maps can leave a COUNTKILL box in a wall or overlapping
            // sector. Doom `P_CheckPosition` then keeps them stuck. Skip the
            // vanilla-fixed path so demos stay bit-identical.
            if self.math_mode != SimMathMode::VanillaFixed {
                self.unstick_map_thing(id);
            }
        }
        Ok(())
    }

    /// Nudge a `COUNTKILL` that does not fit at its map spot (up to 48 units).
    fn unstick_map_thing(&mut self, id: ActorId) {
        let Some(actor) = self.actors.get(id).cloned() else {
            return;
        };
        if actor.destroyed || (actor.flags & crate::class::FLAG_COUNTKILL) == 0 {
            return;
        }
        let here = actor.pos;
        let fits = crate::physics::check_position(self, &actor, here, Some(id)).ok;
        if let Some(floor) = crate::physics::floor_at_xy(self, here.x(), here.y()) {
            if fits && (floor - here.z()).abs() <= 1.0 {
                return;
            }
            if fits && (floor - here.z()).abs() <= 64.0 {
                let p = DVec3::new(here.x(), here.y(), floor);
                if crate::physics::check_position(self, &actor, p, Some(id)).ok {
                    if let Some(a) = self.actors.get_mut(id) {
                        a.pos = p;
                        a.spawn_point = p;
                    }
                    return;
                }
            }
        } else if fits {
            return;
        }
        const REACH: i32 = 48;
        let mut best: Option<(i32, DVec3)> = None;
        let mut dy = -REACH;
        while dy <= REACH {
            let mut dx = -REACH;
            while dx <= REACH {
                if dx * dx + dy * dy <= REACH * REACH {
                    let p =
                        DVec3::new(here.x() + f64::from(dx), here.y() + f64::from(dy), here.z());
                    let check = crate::physics::check_position(self, &actor, p, Some(id));
                    if check.ok && (check.floor_z - here.z()).abs() <= 24.0 {
                        let d = dx * dx + dy * dy;
                        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                            best = Some((d, DVec3::new(p.x(), p.y(), check.floor_z)));
                        }
                    }
                }
                dx += 4;
            }
            dy += 4;
        }
        if let Some((_, p)) = best {
            if let Some(a) = self.actors.get_mut(id) {
                a.pos = p;
                a.spawn_point = p;
            }
        }
    }

    /// Spawn coop players 2..=N (`total` clamped 1..=[`MAX_PLAYERS`]).
    ///
    /// Player 1 is already created in `Self::spawn_map_things`. Uses Player 2–4
    /// starts, then deathmatch starts (type 11), then Player 1 + offset + telefog
    /// (`docs/28` BE12). Also enables multiplayer-only map things when `total > 1`.
    pub fn spawn_coop_players(&mut self, total: u8) -> Result<u8, SimError> {
        let total = total.clamp(1, MAX_PLAYERS as u8) as usize;
        if total <= 1 {
            return Ok(self.players[0].is_some() as u8);
        }
        // Vanilla coop: weapons stay on the map for each player (docs/30 W17).
        self.weapon_stay = true;
        self.spawn_multiplayer_only_things()?;
        let mut alive = u8::from(self.players[0].is_some());
        let coop_starts: [i16; 3] = [
            THING_PLAYER2_START,
            THING_PLAYER3_START,
            THING_PLAYER4_START,
        ];
        let dm_starts: Vec<_> = self
            .map
            .things
            .iter()
            .filter(|t| t.type_id == THING_DEATHMATCH_START)
            .cloned()
            .collect();
        let p1 = self
            .map
            .things
            .iter()
            .find(|t| t.type_id == THING_PLAYER1_START)
            .cloned();
        let mut dm_idx = 0usize;
        for seat in 1..total {
            if self.players[seat].is_some() {
                alive = alive.saturating_add(1);
                continue;
            }
            let mut spawned = false;
            // Player 2–4 starts, then ZDoom's Player 5–8 starts (4001–4004).
            let own = match seat {
                1..=3 => Some(coop_starts[seat - 1]),
                4..=7 => Some(4001 + (seat as i16 - 4)),
                _ => None,
            };
            if let Some(tid) = own {
                if let Some(thing) = self.map.things.iter().find(|t| t.type_id == tid).cloned() {
                    self.spawn_coop_seat(seat, &thing, false)?;
                    spawned = true;
                }
            }
            if !spawned && dm_idx < dm_starts.len() {
                let thing = &dm_starts[dm_idx];
                dm_idx += 1;
                self.spawn_coop_seat(seat, thing, false)?;
                spawned = true;
            }
            if !spawned {
                if let Some(ref p1) = p1 {
                    let class = self.player_class();
                    let z = self.map_thing_z(class, p1);
                    let angle = doom_thing_angle(p1.angle);
                    let base = DVec3::new(f64::from(p1.x), f64::from(p1.y), z);
                    let pos = if seat < crate::player::LEGACY_SEATS {
                        let (ox, oy) = p1_fallback_offset(seat);
                        DVec3::new(base.x() + ox, base.y() + oy, z)
                    } else {
                        // Many seats share one start: look outward for room.
                        self.free_spot_near(class, base, seat)
                    };
                    self.spawn_coop_seat_at(seat, pos, angle, true)?;
                    spawned = true;
                }
            }
            if spawned {
                alive = alive.saturating_add(1);
            }
        }
        Ok(alive)
    }

    /// A spot for `class` around `base` that blocks on no wall and no other
    /// solid thing, searched on rings 40 units apart (golden-angle steps), for
    /// the `seat`-th fallback spawn. `base` itself when nothing fits.
    fn free_spot_near(&self, class: room_core::ClassId, base: DVec3, seat: usize) -> DVec3 {
        let Some(def) = self.classes.get(class) else {
            return base;
        };
        // Probe with player 1's pawn (same class), so its own spot is solid.
        let Some(mut probe) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
            .and_then(|mo| self.actors.get(mo))
            .cloned()
        else {
            return base;
        };
        let start_sector = crate::physics::sector_at_xy(self, base.x(), base.y());
        for k in 0..256usize {
            let ring = 1.0 + (k / 8) as f64;
            let ang = (k as f64 + seat as f64) * 2.399_963_229_728_653;
            let r = ring * (def.radius * 2.0 + 8.0);
            let x = base.x() + room_core::dmath::cos(ang) * r;
            let y = base.y() + room_core::dmath::sin(ang) * r;
            let Some(floor) = crate::physics::floor_at_xy(self, x, y) else {
                continue;
            };
            let Some(ceil) = crate::physics::ceiling_at_xy(self, x, y) else {
                continue;
            };
            if ceil - floor < def.height || (floor - base.z()).abs() > 24.0 {
                continue;
            }
            // Stay reachable from the start: walk the straight line in 8-unit
            // steps and refuse a step over a ledge or through a gap too low.
            if crate::physics::sector_at_xy(self, x, y) != start_sector
                && !self.walkable_line(base, DVec3::new(x, y, floor), def.height)
            {
                continue;
            }
            let pos = DVec3::new(x, y, floor);
            probe.pos = pos;
            if crate::physics::check_position(self, &probe, pos, None).ok {
                return pos;
            }
        }
        base
    }

    /// The straight path from `a` to `b` has floor everywhere, no step above
    /// 24 units and room for `height` (fallback spawn spots).
    fn walkable_line(&self, a: DVec3, b: DVec3, height: f64) -> bool {
        let dx = b.x() - a.x();
        let dy = b.y() - a.y();
        let steps = ((dx * dx + dy * dy).sqrt() / 8.0).ceil().max(1.0) as usize;
        let mut prev = a.z();
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let (x, y) = (a.x() + dx * t, a.y() + dy * t);
            let (Some(f), Some(c)) = (
                crate::physics::floor_at_xy(self, x, y),
                crate::physics::ceiling_at_xy(self, x, y),
            ) else {
                return false;
            };
            if f - prev > 24.0 || prev - f > 24.0 || c - f < height {
                return false;
            }
            prev = f;
        }
        true
    }

    fn spawn_coop_seat(
        &mut self,
        seat: usize,
        thing: &room_map::RawThing,
        telefog: bool,
    ) -> Result<(), SimError> {
        let class = self.player_class();
        let z = self.map_thing_z(class, thing);
        let pos = DVec3::new(f64::from(thing.x), f64::from(thing.y), z);
        self.spawn_coop_seat_at(seat, pos, doom_thing_angle(thing.angle), telefog)
    }

    fn spawn_coop_seat_at(
        &mut self,
        seat: usize,
        pos: DVec3,
        angle: room_core::Angle,
        telefog: bool,
    ) -> Result<(), SimError> {
        if self.players[seat].is_some() {
            return Ok(());
        }
        let class = self.player_class();
        let id = self.spawn_at(class, pos, angle)?;
        self.randomize_mapthing_tics(id);
        if telefog {
            crate::specials::spawn_teleport_fog(self, pos);
        }
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
        let _ = self.give_starting_loadout(id);
        Ok(())
    }

    /// Mark a seat as sim-driven bot (`docs/28` BE8).
    /// A bot seat with no session name gets a callsign from
    /// `crate::bot::BOT_NAMES`, picked from the match seed so every peer
    /// agrees (names are in the checksum) and each match differs.
    pub fn set_seat_bot(&mut self, seat: u8, bot: bool) {
        let i = usize::from(seat.min(MAX_PLAYERS as u8 - 1));
        if let Some(p) = self.players[i].as_mut() {
            p.is_bot = bot;
        }
        let bit = crate::player::seat_bit(usize::from(seat));
        if bot {
            self.bot_seats |= bit;
            if self.player_names.stored(i).is_empty() {
                let name = crate::bot::pick_bot_name(self.rng.seed(), i, |n| {
                    (0..MAX_PLAYERS).any(|o| {
                        o != i
                            && self
                                .player_names
                                .stored(o)
                                .eq_ignore_ascii_case(n.as_bytes())
                    })
                });
                self.player_names.set(i, name);
                self.bot_named |= bit;
            }
        } else {
            self.bot_seats &= !bit;
            if self.bot_named & bit != 0 {
                self.player_names.set(i, "");
                self.bot_named &= !bit;
            }
        }
    }

    /// Save every live player's inventory and ready weapon (seats already
    /// saved keep their first copy), for [`World::restore_player_inventories`].
    pub fn save_player_inventories(&mut self) {
        for seat in 0..MAX_PLAYERS {
            if self.saved_inventories.iter().any(|(s, _, _)| *s == seat) {
                continue;
            }
            let Some(p) = self.players[seat].as_ref() else {
                continue;
            };
            let Some(mo) = p.mo else {
                continue;
            };
            if p.player_state != crate::player::PlayerState::Live {
                continue;
            }
            let ready = p.ready_weapon;
            let items = self.inventory.snapshot(mo);
            self.saved_inventories.push((seat, items, ready));
        }
    }

    /// Give every saved player back exactly what was saved (ammo spent and
    /// weapons switched during a pre-match countdown), then forget it.
    pub fn restore_player_inventories(&mut self) {
        for (seat, items, ready) in std::mem::take(&mut self.saved_inventories) {
            let Some(p) = self.players[seat].as_mut() else {
                continue;
            };
            let Some(mo) = p.mo else {
                continue;
            };
            self.inventory.restore(mo, items);
            if p.ready_weapon != ready {
                p.ready_weapon = ready;
                p.pending_weapon = None;
                p.pspr_state = None;
                p.pspr_y = crate::player::WEAPON_BOTTOM;
            }
        }
    }

    /// A full copy of this world for rollback (speculative play ahead of the
    /// confirmed lockstep), ZScript VM included, or `None` when part of it
    /// cannot be copied: loaded WASM play modules.
    #[must_use]
    pub fn snapshot(&self) -> Option<World> {
        self.can_snapshot().then(|| self.clone())
    }

    /// [`Self::snapshot`] would succeed.
    #[must_use]
    pub fn can_snapshot(&self) -> bool {
        self.scripts.is_empty()
    }

    /// Live human (non-bot) players with health left.
    #[must_use]
    pub fn humans_alive(&self) -> usize {
        let live = |mo: Option<ActorId>| {
            mo.and_then(|m| self.actors.get(m))
                .is_some_and(|a| a.health > 0 && !a.destroyed)
        };
        (0..MAX_PLAYERS)
            .filter(|&s| match self.players[s].as_ref() {
                Some(p) => {
                    !p.is_bot && p.player_state == crate::player::PlayerState::Live && live(p.mo)
                }
                // The thinking slot is out of `players` during its turn.
                None => self.taken_player.is_some_and(|(seat, mo)| {
                    seat == s && self.bot_seats & crate::player::seat_bit(s) == 0 && live(Some(mo))
                }),
            })
            .count()
    }

    /// Enable bots on contiguous seats `[first..=last]` (inclusive).
    pub fn fill_bot_seats(&mut self, first: u8, last: u8) {
        for seat in first..=last.min(MAX_PLAYERS as u8 - 1) {
            if self.players[usize::from(seat)].is_some() {
                self.set_seat_bot(seat, true);
            }
        }
    }

    /// UDMF Strife thing keys (`standing`, `strifeally`, `invisible`,
    /// `translucent`), as ZDoom's `MTF_STANDSTILL` / `MTF_SHADOW` / `MTF_ALTSHADOW`.
    fn apply_thing_ext(&mut self, id: ActorId, ext: u32) {
        use room_map::udmf::ext::*;
        let Some(a) = self.actors.get_mut(id) else {
            return;
        };
        if ext & THING_STANDING != 0 {
            a.extra_flags.set_name("STANDSTILL", true);
        }
        if ext & THING_STRIFEALLY != 0 {
            a.flags |= crate::class::FLAG_FRIENDLY;
        }
        if ext & THING_TRANSLUCENT != 0 {
            a.flags |= crate::class::FLAG_SHADOW;
            a.render_style = crate::class::RenderStyle::Translucent;
            a.alpha = 0.25;
        } else if ext & THING_INVISIBLE != 0 {
            a.render_style = crate::class::RenderStyle::None;
        }
    }

    /// Spawn map things flagged `MTF_NOTSINGLE` (bit 16) once multiplayer is active.
    fn spawn_multiplayer_only_things(&mut self) -> Result<(), SimError> {
        let things = self.map.things.clone();
        for (thing_index, thing) in things.iter().enumerate() {
            if (thing.flags & 16) == 0 {
                continue;
            }
            // UDMF `coop = false` / `dm = false`.
            let ext = self.map.udmf_user.thing_ext(thing_index);
            let mode_bit = if self.deathmatch {
                room_map::udmf::ext::THING_NOTDM
            } else {
                room_map::udmf::ext::THING_NOTCOOP
            };
            if ext & mode_bit != 0 {
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
            let z = self.map_thing_z(class, thing);
            let id = self.spawn_at(
                class,
                DVec3::new(f64::from(thing.x), f64::from(thing.y), z),
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
            self.apply_thing_ext(id, ext);
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
        self.respawn_player_at_seat(player, 0)
    }

    /// Whether map spot `t` lies in a UDMF `norespawn` sector.
    fn spot_blocks_respawn(&self, t: &room_map::RawThing) -> bool {
        crate::physics::sector_at_xy(self, f64::from(t.x), f64::from(t.y)).is_some_and(|s| {
            self.map.udmf_user.sector_ext(s) & room_map::udmf::ext::SECTOR_NORESPAWN != 0
        })
    }

    /// Respawn spot for `seat`: a deathmatch start in deathmatch, else the
    /// seat's own player start. Spots in UDMF `norespawn` sectors are skipped
    /// (falling back to any other player start), as ZDoom's `SECMF_NORESPAWN`.
    pub(crate) fn respawn_spot(&self, seat: usize) -> Option<room_map::RawThing> {
        let ok = |t: &&room_map::RawThing| !self.spot_blocks_respawn(t);
        let is_player_start = |id: i16| (1..=4).contains(&id) || (4001..=4004).contains(&id);
        if self.deathmatch {
            let dm: Vec<_> = self
                .map
                .things
                .iter()
                .filter(|t| t.type_id == THING_DEATHMATCH_START)
                .filter(ok)
                .collect();
            if !dm.is_empty() {
                // Deterministic (no RNG draw): rotate through the spots.
                let i = (self.level.time as usize).wrapping_add(seat) % dm.len();
                return Some(dm[i].clone());
            }
        }
        let own: i16 = match seat {
            0..=3 => seat as i16 + 1,
            4..=7 => 4001 + (seat as i16 - 4),
            // Seats past 8 have no start of their own: share the free player
            // starts round-robin (seat 9 → the first free one, …) instead of
            // all piling onto Player 1's.
            _ => {
                let free: Vec<_> = self
                    .map
                    .things
                    .iter()
                    .filter(|t| is_player_start(t.type_id))
                    .filter(ok)
                    .collect();
                if !free.is_empty() {
                    return Some(free[(seat - 8) % free.len()].clone());
                }
                i16::MIN
            }
        };
        self.map
            .things
            .iter()
            .filter(|t| t.type_id == own)
            .find(ok)
            .or_else(|| {
                self.map
                    .things
                    .iter()
                    .find(|t| t.type_id == THING_PLAYER1_START && ok(t))
            })
            .or_else(|| {
                self.map
                    .things
                    .iter()
                    .filter(|t| is_player_start(t.type_id))
                    .find(ok)
            })
            .or_else(|| {
                self.map
                    .things
                    .iter()
                    .find(|t| t.type_id == THING_PLAYER1_START)
            })
            .cloned()
    }

    /// [`Self::respawn_player_at`] for player `seat` (picks that seat's spot).
    pub fn respawn_player_at_seat(&mut self, player: &mut Player, seat: usize) -> bool {
        if player.player_state != crate::player::PlayerState::Dead {
            return false;
        }
        let Some(mo) = player.mo else {
            return false;
        };
        let Some(start) = self.respawn_spot(seat) else {
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
        // Re-apply ready weapon after loadout (give_starting_loadout may set players[0]).
        if let Some(weapon) = self.give_starting_loadout(mo) {
            player.ready_weapon = Some(weapon);
        }
        player.pspr_state = None;
        player.pspr_y = crate::player::WEAPON_BOTTOM;
        true
    }

    /// Spawn z for a map thing: Hexen / UDMF `height` is relative to the floor.
    fn map_thing_z(&self, class: ClassId, thing: &room_map::RawThing) -> f64 {
        let ceiling = self
            .classes
            .get(class)
            .is_some_and(|d| d.flags & crate::class::FLAG_SPAWNCEILING != 0);
        if thing.z == 0 || ceiling {
            return f64::from(thing.z);
        }
        crate::physics::floor_at_xy(self, f64::from(thing.x), f64::from(thing.y)).unwrap_or(0.0)
            + f64::from(thing.z)
    }

    /// Player pawn class: the first KEYCONF `addplayerclass`, else `PlayerPawn`.
    fn player_class(&self) -> ClassId {
        self.weapon_slots
            .chosen_player_class
            .as_deref()
            .and_then(|c| self.classes.find_name(c))
            .or_else(|| {
                self.weapon_slots
                    .player_classes
                    .iter()
                    .find_map(|c| self.classes.find_name(&c.class))
            })
            .or_else(|| self.classes.find_name("PlayerPawn"))
            .or_else(|| self.classes.find_name("Actor"))
            .unwrap_or(ClassId::from_raw(0))
    }

    /// Give `Player.StartItem` entries in order; the first weapon becomes ready.
    fn give_start_items(&mut self, mo: ActorId, items: &[(String, i32)]) -> Option<ClassId> {
        let mut ready = None;
        for (name, amount) in items {
            let Some(cid) = self.classes.find_name(name) else {
                continue;
            };
            let _ = crate::inventory::give_inventory(self, mo, cid, *amount);
            if ready.is_none() && self.classes.get(cid).is_some_and(|c| c.is_weapon) {
                ready = Some(cid);
            }
        }
        if let Some(weapon) = ready {
            if let Some(p) = self.players.iter_mut().flatten().find(|p| p.mo == Some(mo)) {
                p.ready_weapon = Some(weapon);
                p.pspr_state = None;
                p.pspr_y = crate::player::WEAPON_BOTTOM;
            }
        }
        ready
    }

    /// Starting kit: the pawn's `Player.StartItem` list, else Doom's Fist +
    /// Pistol + a clip of bullets (when those classes exist). Returns the
    /// ready weapon.
    fn give_starting_loadout(&mut self, mo: ActorId) -> Option<ClassId> {
        let start_items = self
            .actors
            .get(mo)
            .and_then(|a| self.classes.get(a.class))
            .map(|c| c.start_items.clone())
            .unwrap_or_default();
        if !start_items.is_empty() {
            return self.give_start_items(mo, &start_items);
        }
        let fist = self.classes.find_name("Fist");
        let pistol = self.classes.find_name("Pistol");
        if fist.is_none() && pistol.is_none() {
            return None;
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
                p.pspr_y = crate::player::WEAPON_BOTTOM;
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
        pistol
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

    /// `P_DamageMobj(target, inflictor, source, damage)` for tools and
    /// integration tests (the playsim reaches damage through its own paths).
    /// Returns the damage actually dealt.
    pub fn damage_actor(
        &mut self,
        target: ActorId,
        inflictor: Option<ActorId>,
        source: Option<ActorId>,
        damage: i32,
    ) -> i32 {
        crate::damage::damage_mobj(self, target, inflictor, source, damage)
    }

    /// Spawn an actor from class defaults at `pos`.
    pub fn spawn_at(
        &mut self,
        class: ClassId,
        pos: DVec3,
        angle: Angle,
    ) -> Result<ActorId, SimError> {
        let class = self.classes.replacement(class);
        let def = match self.classes.get(class).cloned() {
            Some(def) => def,
            None => {
                let mut d = self
                    .classes
                    .find_name("Actor")
                    .and_then(|id| self.classes.get(id).cloned())
                    .ok_or_else(|| {
                        SimError::Message("Actor class required for spawn fallback".into())
                    })?;
                d.id = class;
                d
            }
        };
        let spawn_state = self.states.find_label(class, "Spawn");
        let (sprite, frame) = if let Some(sid) = spawn_state {
            if let Some(st) = self.states.get(sid) {
                (st.sprite.clone(), st.frame)
            } else {
                (String::new(), 0)
            }
        } else {
            (String::new(), 0)
        };
        let speed = if self.fast_monsters && def.fast_speed > 0.0 {
            def.fast_speed
        } else {
            def.speed
        };
        // GZDoom: `Scale` is visual only; collision uses the declared size.
        // Room's generated Build/Duke actors opt in to scaled collision.
        let (radius, height) = if def.extra_flags.get_name("ROOMSCALEDCOLLISION") {
            (def.radius * def.scale_x, def.height * def.scale_y)
        } else {
            (def.radius, def.height)
        };
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
                vel: DVec3::new(0.0, 0.0, def.ext.vspeed),
                angle,
                pitch: Angle::ZERO,
                view_pitch: None,
                view_angle: None,
                view_roll: None,
                view_pos: None,
                sprite_ofs_x: 0.0,
                sprite_ofs_y: 0.0,
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
                ext: Box::new(crate::actor::ActorExt::from_class(&def.ext)),
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
                tics: 0,
                sprite: sprite.clone(),
                frame,
                flags: def.flags,
                extra_flags: def.extra_flags,
                reaction_time: def.reaction_time,
                threshold: def.threshold,
                pain_threshold: def.pain_threshold,
                target: None,
                last_heard: None,
                master: None,
                tracer: None,
                goal: None,
                chase_goal: false,
                fast_chase_strafe: 0,
                frightened: (def.flags & crate::class::FLAG_FRIGHTENED) != 0
                    && !def.extra_flags.get_name("NOFEAR"),
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
                hit_kind: 0,
                hit_actor: None,
                hit_pos: DVec3::ZERO,
                hit_normal: DVec3::ZERO,
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
        // `WeaveIndexXY` fixes the start phase; otherwise it is random.
        let float_bob_phase = if def.ext.weave_index_xy >= 0 {
            (def.ext.weave_index_xy & 63) as u8
        } else {
            spawn_rnd & 63
        };
        // Doom pickups are `MF_SPECIAL` and not `MF_SOLID` (weapons/ammo/items).
        let mut flags = def.flags;
        if def.is_weapon || def.inventory_max_amount > 1 {
            flags &= !crate::class::FLAG_SOLID;
        }
        let actor = Actor {
            class,
            pos: spawn_pos,
            vel: DVec3::new(0.0, 0.0, def.ext.vspeed),
            angle,
            pitch: Angle::ZERO,
            view_pitch: None,
            view_angle: None,
            view_roll: None,
            view_pos: None,
            sprite_ofs_x: 0.0,
            sprite_ofs_y: 0.0,
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
            ext: Box::new(crate::actor::ActorExt::from_class(&def.ext)),
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
            accuracy: def.ext.accuracy,
            stamina: def.ext.stamina,
            ripper_level: def.ripper_level,
            rip_level_min: def.rip_level_min,
            rip_level_max: def.rip_level_max,
            // Spawn state entered by `enter_spawn_state` below (no action).
            state: None,
            tics: 0,
            sprite,
            frame,
            flags,
            extra_flags: def.extra_flags,
            // Vanilla `P_SpawnMobj`: Nightmare monsters react immediately.
            reaction_time: if self.fast_monsters {
                0
            } else {
                def.reaction_time
            },
            threshold: def.threshold,
            pain_threshold: def.pain_threshold,
            target: None,
            last_heard: None,
            master: None,
            tracer: None,
            goal: None,
            chase_goal: false,
            fast_chase_strafe: 0,
            frightened: (flags & crate::class::FLAG_FRIGHTENED) != 0
                && !def.extra_flags.get_name("NOFEAR"),
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
            hit_kind: 0,
            hit_actor: None,
            hit_pos: DVec3::ZERO,
            hit_normal: DVec3::ZERO,
        };
        let id = self
            .actors
            .insert(actor)
            .map_err(|e| SimError::Message(e.to_string()))?;
        self.thinkers.link_default(id);
        self.live_ids.push(id);
        if let Some(sid) = spawn_state {
            self.enter_spawn_state(id, sid);
        }
        if (def.flags & crate::class::FLAG_COUNTKILL) != 0 {
            self.level.total_kills = self.level.total_kills.saturating_add(1);
        }
        if (def.flags & crate::class::FLAG_COUNTITEM) != 0 {
            self.level.total_items = self.level.total_items.saturating_add(1);
        }
        self.push_script_event(crate::script_mods::ScriptEvent::ThingSpawned { thing: id });
        // ZS3/ZS4: PostBeginPlay + WorldThingSpawned for matching script classes / handlers.
        if self.zscript.is_active() {
            let class_name = self.classes.get(class).map(|c| c.name.clone());
            if let Some(name) = class_name {
                let candidate = crate::action::zscript_resurrection_candidate(self, id)
                    .map(|candidate| (candidate.index(), candidate.generation()));
                self.zscript.set_ai_observers(self.zscript_ai_observers(id));
                self.zscript.set_resurrection_candidate(candidate);
                let synced = self.zscript_sync_in(id);
                self.zscript.notify_spawned(id.index(), &name);
                self.zscript_sync_out(id, synced);
                self.zscript.set_resurrection_candidate(None);
                crate::action::apply_zscript_actor_ops(self, id);
            }
            if !self.zscript.event_handlers.is_empty() {
                // GS_LEVEL is 0 in the host stub. Skill comes from the world.
                self.zscript.sync_host(self.skill, 0, 0);
                self.zscript.set_level_checksum(self.map.md5);
                self.zscript_publish_level();
                if let Some(info) = self.zscript_actor_event_info(id) {
                    let _ = self.zscript.notify_thing_spawned(&info);
                }
                crate::action::apply_zscript_actor_ops(self, id);
                let grants = self.zscript.take_grants();
                for (cookie, item, amount) in grants {
                    self.apply_zscript_inventory_change(id, cookie, &item, amount);
                }
            }
        }
        Ok(id)
    }

    /// Apply a queued ZScript GiveInventory/TakeInventory to its receiver.
    /// The cookie is the generational actor handle encoded by room-zvm; stale
    /// or non-actor receivers fall back to the call site's owner.
    pub(crate) fn apply_zscript_inventory_change(
        &mut self,
        fallback: ActorId,
        cookie: i64,
        item_name: &str,
        amount: i32,
    ) {
        let raw = cookie as u64;
        let requested = ActorId::from_raw(raw as u32, (raw >> 32) as u32);
        let owner = if self.actors.get(requested).is_some() {
            requested
        } else {
            fallback
        };
        let Some(class) = self.classes.find_name(item_name) else {
            return;
        };
        if amount < 0 {
            let _ = crate::inventory::take_inventory(self, owner, class, amount.saturating_abs());
        } else {
            let _ = crate::inventory::give_inventory(self, owner, class, amount);
        }
    }

    /// Morph an actor to another DECORATE class while retaining its actor id.
    /// The original class is kept in a private user variable so ACS
    /// `UnMorphActor` can restore it without replacing player/thinker handles.
    pub(crate) fn morph_actor_class(&mut self, id: ActorId, class: ClassId) -> bool {
        const ORIGINAL_CLASS: &str = "room_internal_morph_class";
        let Some(def) = self.classes.get(class).cloned() else {
            return false;
        };
        let Some(current) = self.actors.get(id) else {
            return false;
        };
        if current.destroyed || current.user_vars.contains_key(ORIGINAL_CLASS) {
            return false;
        }
        let old_class = current.class;
        let _ = crate::action::call_action(self, id, "A_StopSound");
        let morph_weapon = self.classes.find_name(&def.player_morph_weapon);
        let old_morph_weapon_count =
            morph_weapon.map_or(0, |weapon| self.inventory.count(id, weapon));
        let old_ready_weapon = self.players.iter().flatten().find(|p| p.mo == Some(id));
        {
            let Some(actor) = self.actors.get_mut(id) else {
                return false;
            };
            actor
                .user_vars
                .insert(ORIGINAL_CLASS.to_owned(), old_class.raw() as i32);
            actor.user_vars.insert(
                "room_internal_morph_weapon_count".to_owned(),
                old_morph_weapon_count,
            );
            actor.user_vars.insert(
                "room_internal_morph_weapon".to_owned(),
                old_ready_weapon.map_or(-1, |player| {
                    player.ready_weapon.map_or(-1, |weapon| weapon.raw() as i32)
                }),
            );
            actor.user_vars.insert(
                "room_internal_morph_weapon_class".to_owned(),
                morph_weapon.map_or(-1, |weapon| weapon.raw() as i32),
            );
            actor.class = class;
            actor.flags = def.flags;
            actor.extra_flags = def.extra_flags;
            actor.radius = if def.extra_flags.get_name("ROOMSCALEDCOLLISION") {
                def.radius * def.scale_x
            } else {
                def.radius
            };
            actor.height = if def.extra_flags.get_name("ROOMSCALEDCOLLISION") {
                def.height * def.scale_y
            } else {
                def.height
            };
            actor.speed = if self.fast_monsters && def.fast_speed > 0.0 {
                def.fast_speed
            } else {
                def.speed
            };
            actor.gravity = def.gravity;
            actor.mass = def.mass;
            actor.view_height = def.view_height;
            actor.jump_z = def.jump_z;
            actor.friction = def.friction;
            actor.max_step_height = def.max_step_height;
            actor.max_dropoff_height = def.max_dropoff_height;
            actor.scale_x = def.scale_x;
            actor.scale_y = def.scale_y;
            actor.state = None;
            actor.tics = 0;
        }
        if let Some(weapon) = morph_weapon {
            let _ = crate::inventory::give_inventory(self, id, weapon, 1);
        }
        if let Some(player) = self.players.iter_mut().flatten().find(|p| p.mo == Some(id)) {
            player.cls = class;
            player.view_height = if def.view_height > 0.0 {
                def.view_height
            } else {
                41.0
            };
            player.ready_weapon = morph_weapon;
            player.pending_weapon = None;
            player.pspr_state = None;
            player.pspr_tics = 0;
            player.pspr_y = crate::player::WEAPON_BOTTOM;
        }
        let spawn = self.states.find_label(class, "Spawn");
        self.set_state(id, spawn, true);
        if std::env::var_os("ROOM_TRACE_VEHICLES").is_some() {
            let actor = self.actors.get(id);
            eprintln!(
                "room: VEHICLE MORPH tic={} actor=#{} {} -> {} result=true hp={} weapon={}",
                self.level.time,
                id.index(),
                self.classes
                    .get(old_class)
                    .map_or("<missing>", |c| c.name.as_str()),
                def.name,
                actor.map_or(0, |a| a.health),
                self.players
                    .iter()
                    .flatten()
                    .find(|p| p.mo == Some(id))
                    .and_then(|p| p.ready_weapon)
                    .and_then(|weapon| self.classes.get(weapon))
                    .map_or("none", |c| c.name.as_str()),
            );
        }
        true
    }

    /// Restore an actor changed by [`Self::morph_actor_class`].
    pub(crate) fn unmorph_actor_class(&mut self, id: ActorId) -> bool {
        const ORIGINAL_CLASS: &str = "room_internal_morph_class";
        let Some((raw, saved_weapon, old_morph_weapon_count, morph_weapon_raw)) =
            self.actors.get(id).and_then(|a| {
                Some((
                    *a.user_vars.get(ORIGINAL_CLASS)?,
                    *a.user_vars.get("room_internal_morph_weapon")?,
                    a.user_vars
                        .get("room_internal_morph_weapon_count")
                        .copied()
                        .unwrap_or(0),
                    a.user_vars
                        .get("room_internal_morph_weapon_class")
                        .copied()
                        .unwrap_or(-1),
                ))
            })
        else {
            return false;
        };
        let class = ClassId::from_raw(raw as u32);
        let Some(def) = self.classes.get(class).cloned() else {
            return false;
        };
        let _ = crate::action::call_action(self, id, "A_StopSound");
        {
            let Some(actor) = self.actors.get_mut(id) else {
                return false;
            };
            actor.user_vars.remove(ORIGINAL_CLASS);
            actor.user_vars.remove("room_internal_morph_weapon");
            actor.user_vars.remove("room_internal_morph_weapon_count");
            actor.user_vars.remove("room_internal_morph_weapon_class");
            actor.class = class;
            actor.flags = def.flags;
            actor.extra_flags = def.extra_flags;
            actor.radius = if def.extra_flags.get_name("ROOMSCALEDCOLLISION") {
                def.radius * def.scale_x
            } else {
                def.radius
            };
            actor.height = if def.extra_flags.get_name("ROOMSCALEDCOLLISION") {
                def.height * def.scale_y
            } else {
                def.height
            };
            actor.speed = def.speed;
            actor.gravity = def.gravity;
            actor.mass = def.mass;
            actor.view_height = def.view_height;
            actor.jump_z = def.jump_z;
            actor.friction = def.friction;
            actor.max_step_height = def.max_step_height;
            actor.max_dropoff_height = def.max_dropoff_height;
            actor.scale_x = def.scale_x;
            actor.scale_y = def.scale_y;
            actor.state = None;
            actor.tics = 0;
        }
        if morph_weapon_raw >= 0 {
            let morph_weapon = ClassId::from_raw(morph_weapon_raw as u32);
            crate::inventory::set_inventory(self, id, morph_weapon, old_morph_weapon_count);
        }
        if let Some(player) = self.players.iter_mut().flatten().find(|p| p.mo == Some(id)) {
            player.cls = class;
            player.view_height = if def.view_height > 0.0 {
                def.view_height
            } else {
                41.0
            };
            player.ready_weapon =
                (saved_weapon >= 0).then(|| ClassId::from_raw(saved_weapon as u32));
            player.pending_weapon = None;
            player.pspr_state = None;
            player.pspr_tics = 0;
            player.pspr_y = crate::player::WEAPON_BOTTOM;
        }
        let spawn = self.states.find_label(class, "Spawn");
        self.set_state(id, spawn, true);
        if std::env::var_os("ROOM_TRACE_VEHICLES").is_some() {
            let actor = self.actors.get(id);
            eprintln!(
                "room: VEHICLE UNMORPH tic={} actor=#{} class={} hp={} pos=({:.1},{:.1},{:.1})",
                self.level.time,
                id.index(),
                def.name,
                actor.map_or(0, |a| a.health),
                actor.map_or(0.0, |a| a.pos.x()),
                actor.map_or(0.0, |a| a.pos.y()),
                actor.map_or(0.0, |a| a.pos.z()),
            );
        }
        true
    }

    /// Native fields of `id` for its ZScript object (UZDoom `Actor` names).
    pub(crate) fn zscript_actor_snap(&self, id: ActorId) -> Option<crate::zscript::ActorFieldSnap> {
        let a = self.actors.get(id)?;
        let class = self.classes.get(a.class)?.name.clone();
        let floor = crate::physics::floor_at_xy(self, a.pos.x(), a.pos.y()).unwrap_or(a.pos.z());
        let ceiling = crate::physics::ceiling_at_xy(self, a.pos.x(), a.pos.y())
            .unwrap_or(a.pos.z() + a.height);
        let ptr = |p: Option<ActorId>| {
            let p = p?;
            let other = self.actors.get(p)?;
            let class = self.classes.get(other.class)?.name.clone();
            Some((p.index(), p.generation(), class))
        };
        Some(crate::zscript::ActorFieldSnap {
            id: (id.index(), id.generation()),
            class,
            floats: vec![
                ("pos.x", a.pos.x()),
                ("pos.y", a.pos.y()),
                ("pos.z", a.pos.z()),
                ("prev.x", a.pos.x()),
                ("prev.y", a.pos.y()),
                ("prev.z", a.pos.z()),
                ("vel.x", a.vel.x()),
                ("vel.y", a.vel.y()),
                ("vel.z", a.vel.z()),
                ("angle", a.angle.degrees()),
                ("pitch", a.pitch.degrees()),
                ("roll", a.roll.degrees()),
                ("speed", a.speed),
                ("floatspeed", a.float_speed),
                ("fastspeed", a.fast_speed),
                ("alpha", a.alpha),
                ("scale.x", a.scale_x),
                ("scale.y", a.scale_y),
                ("height", a.height),
                ("radius", a.radius),
                ("floorz", floor),
                ("ceilingz", ceiling),
                ("meleerange", a.melee_range),
                ("gravity", a.gravity),
                ("floorclip", a.ext.floorclip),
                ("friction", a.friction),
                ("maxstepheight", a.max_step_height),
                ("spawnpoint.x", a.spawn_point.x()),
                ("spawnpoint.y", a.spawn_point.y()),
                ("spawnpoint.z", a.spawn_point.z()),
            ],
            ints: vec![
                ("health", i64::from(a.health)),
                ("special1", i64::from(a.ext.special1)),
                ("special2", i64::from(a.ext.special2)),
                ("reactiontime", i64::from(a.reaction_time)),
                ("threshold", i64::from(a.threshold)),
                ("mass", i64::from(a.mass)),
                ("tid", i64::from(a.tid)),
                ("accuracy", i64::from(a.accuracy)),
                ("stamina", i64::from(a.stamina)),
                ("special", i64::from(a.special)),
                ("damage", i64::from(a.damage)),
            ],
            ptrs: vec![
                ("target", ptr(a.target)),
                ("tracer", ptr(a.tracer)),
                ("master", ptr(a.master)),
                ("lastenemy", ptr(a.ext.last_enemy)),
                ("lastheard", ptr(a.last_heard)),
                ("goal", ptr(a.goal)),
            ],
        })
    }

    /// Publish the level to the ZScript VM: portals, and the map objects
    /// (`level.Sectors`, `level.Lines`, …) refreshed once per tic.
    pub(crate) fn zscript_publish_level(&mut self) {
        self.zscript.set_portals(&self.map);
        let tic = self.level.maptime;
        if !self.zscript.map_objects_due(self.map.md5, tic) {
            return;
        }
        let n = self.map.sectors.len();
        let plane = |w: &World, s: usize, floor: bool| {
            let z0 = crate::physics::sector_z_at_pub(w, s, 0.0, 0.0, floor);
            room_zvm::ZsPlane {
                z0,
                gx: crate::physics::sector_z_at_pub(w, s, 1.0, 0.0, floor) - z0,
                gy: crate::physics::sector_z_at_pub(w, s, 0.0, 1.0, floor) - z0,
            }
        };
        let planes = (0..n)
            .map(|s| [plane(self, s, true), plane(self, s, false)])
            .collect();
        let light = (0..n)
            .map(|s| self.sectors.get(s).map_or(0, |d| i32::from(d.light)))
            .collect();
        let flats = (0..n)
            .map(|s| {
                let si = s as u32;
                let sec = &self.map.sectors[s];
                [
                    self.sector_floor_pics
                        .get(&si)
                        .cloned()
                        .unwrap_or_else(|| sec.floor_pic.clone()),
                    self.sector_ceiling_pics
                        .get(&si)
                        .cloned()
                        .unwrap_or_else(|| sec.ceiling_pic.clone()),
                ]
            })
            .collect();
        let sync = crate::zscript::MapSync {
            map: &self.map,
            planes,
            light,
            flats,
            tic,
        };
        self.zscript.sync_map_objects(&sync);
    }

    /// Mirror `id`'s native fields, and those of the actors its pointers name
    /// (`target`, `tracer`, `master`, …), into their script objects.
    pub(crate) fn zscript_sync_in(&mut self, id: ActorId) -> ZscriptSynced {
        let mut out = Vec::new();
        let Some(snap) = self.zscript_actor_snap(id) else {
            return out;
        };
        let others: Vec<ActorId> = snap
            .ptrs
            .iter()
            .filter_map(|(_, p)| p.as_ref())
            .map(|(index, generation, _)| ActorId::from_raw(*index, *generation))
            .filter(|&other| other != id)
            .collect();
        let caller = self
            .actors
            .get(id)
            .map(|a| [a.pos.x(), a.pos.y(), a.pos.z()]);
        self.zscript.set_caller_pos(caller);
        self.zscript_publish_level();
        if let Some(h) = self.zscript.write_actor_fields(&snap) {
            let sector = caller.and_then(|p| crate::physics::sector_at_xy(self, p[0], p[1]));
            self.zscript.set_cur_sector(h, sector);
            out.push((id, h, snap));
        }
        for other in others {
            if out.iter().any(|(seen, _, _)| *seen == other) {
                continue;
            }
            let Some(snap) = self.zscript_actor_snap(other) else {
                continue;
            };
            if let Some(h) = self.zscript.write_actor_fields(&snap) {
                out.push((other, h, snap));
            }
        }
        // Answers the typed natives read: sight from the caller, melee range
        // against its target, and every mirrored actor's inventory.
        let target = self.actors.get(id).and_then(|a| a.target);
        let mut sight = Vec::new();
        let mut target_obj = room_zvm::ObjHandle::NULL;
        let mut melee_ok = false;
        let mut inventory = Vec::new();
        for (other, h, _) in &out {
            if *other != id {
                let seen = crate::sight::check_sight(self, id, *other);
                sight.push((*h, seen));
                if Some(*other) == target {
                    target_obj = *h;
                    melee_ok = seen && crate::action::check_melee_range(self, id, *other);
                }
            }
            let items: Vec<(String, i32, i32)> = self
                .inventory
                .iter(*other)
                .filter_map(|item| {
                    let name = self.classes.get(item.class)?.name.to_ascii_lowercase();
                    Some((name, item.amount, item.max_amount))
                })
                .collect();
            inventory.push((*h, items));
        }
        self.zscript
            .set_call_world(sight, melee_ok, target_obj, inventory);
        out
    }

    /// Apply the fields scripts changed during the call.
    pub(crate) fn zscript_sync_out(&mut self, _id: ActorId, synced: ZscriptSynced) {
        for (id, h, before) in synced {
            self.zscript_sync_out_one(id, h, &before);
        }
    }

    fn zscript_sync_out_one(
        &mut self,
        id: ActorId,
        h: room_zvm::ObjHandle,
        before: &crate::zscript::ActorFieldSnap,
    ) {
        let before = before.clone();
        {}
        let after = self.zscript.read_actor_fields(h, &before);
        if after == before {
            return;
        }
        let resolve = |w: &World, p: &Option<(u32, u32, String)>| -> Option<ActorId> {
            let (index, generation, _) = p.as_ref()?;
            let id = ActorId::from_raw(*index, *generation);
            w.actors.get(id).map(|_| id)
        };
        let ptrs: Vec<(&str, Option<ActorId>)> = after
            .ptrs
            .iter()
            .zip(&before.ptrs)
            .filter(|((_, a), (_, b))| {
                a.as_ref().map(|x| (x.0, x.1)) != b.as_ref().map(|x| (x.0, x.1))
            })
            .map(|((name, p), _)| (*name, resolve(self, p)))
            .collect();
        let Some(a) = self.actors.get_mut(id) else {
            return;
        };
        let mut pos = [a.pos.x(), a.pos.y(), a.pos.z()];
        let mut vel = [a.vel.x(), a.vel.y(), a.vel.z()];
        for ((name, v), (_, old)) in after.floats.iter().zip(&before.floats) {
            if v.to_bits() == old.to_bits() {
                continue;
            }
            let v = *v;
            match *name {
                "pos.x" => pos[0] = v,
                "pos.y" => pos[1] = v,
                "pos.z" => pos[2] = v,
                "vel.x" => vel[0] = v,
                "vel.y" => vel[1] = v,
                "vel.z" => vel[2] = v,
                "angle" => a.angle = Angle::from_degrees(v),
                "pitch" => a.pitch = Angle::from_degrees(v),
                "roll" => a.roll = Angle::from_degrees(v),
                "speed" => a.speed = v,
                "floatspeed" => a.float_speed = v,
                "fastspeed" => a.fast_speed = v,
                "alpha" => a.alpha = v,
                "scale.x" => a.scale_x = v,
                "scale.y" => a.scale_y = v,
                "meleerange" => a.melee_range = v,
                "gravity" => a.gravity = v,
                "floorclip" => a.ext.floorclip = v,
                "friction" => a.friction = v,
                "maxstepheight" => a.max_step_height = v,
                _ => {}
            }
        }
        a.pos = DVec3::new(pos[0], pos[1], pos[2]);
        a.vel = DVec3::new(vel[0], vel[1], vel[2]);
        for ((name, v), (_, old)) in after.ints.iter().zip(&before.ints) {
            if v == old {
                continue;
            }
            let v32 = i32::try_from(*v).unwrap_or(if *v < 0 { i32::MIN } else { i32::MAX });
            match *name {
                "health" => a.health = v32,
                "special1" => a.ext.special1 = v32,
                "special2" => a.ext.special2 = v32,
                "reactiontime" => a.reaction_time = v32,
                "threshold" => a.threshold = v32,
                "mass" => a.mass = v32,
                "tid" => a.tid = v32,
                "accuracy" => a.accuracy = v32,
                "stamina" => a.stamina = v32,
                "special" => a.special = u16::try_from(v32.max(0)).unwrap_or(u16::MAX),
                "damage" => a.damage = v32,
                _ => {}
            }
        }
        for (name, p) in ptrs {
            match name {
                "target" => a.target = p,
                "tracer" => a.tracer = p,
                "master" => a.master = p,
                "lastenemy" => a.ext.last_enemy = p,
                "lastheard" => a.last_heard = p,
                "goal" => a.goal = p,
                _ => {}
            }
        }
    }

    /// Run ZScript `Tick` for an actor whose class name matches a script class (ZS3).
    fn tick_zscript_actor(&mut self, id: ActorId) {
        if !self.zscript.is_active() {
            return;
        }
        let Some(class_name) = self
            .actors
            .get(id)
            .and_then(|a| self.classes.get(a.class))
            .map(|c| c.name.clone())
        else {
            return;
        };
        if !self.zscript.has_method(&class_name, "Tick") {
            return;
        }
        let candidate = crate::action::zscript_resurrection_candidate(self, id)
            .map(|candidate| (candidate.index(), candidate.generation()));
        self.zscript.set_ai_observers(self.zscript_ai_observers(id));
        self.zscript.set_resurrection_candidate(candidate);
        let synced = self.zscript_sync_in(id);
        self.zscript.tick_actor(id.index(), &class_name);
        self.zscript_sync_out(id, synced);
        crate::action::apply_zscript_actor_ops(self, id);
    }

    /// Player observer distances and visibility for ZScript's Check* natives.
    pub(crate) fn zscript_ai_observers(&self, target: ActorId) -> Vec<(f64, f64, bool)> {
        let Some(actor) = self.actors.get(target) else {
            return Vec::new();
        };
        self.players
            .iter()
            .filter_map(Option::as_ref)
            .filter_map(|player| {
                let observer = player.mo?;
                let camera = self.actors.get(observer)?;
                let dx = actor.pos.x() - camera.pos.x();
                let dy = actor.pos.y() - camera.pos.y();
                // GZDoom measures from the camera's center. For a possessed
                // pawn, `view_z` includes crouch/view-offset adjustments;
                // non-player observers use their physical center.
                let eye_z = if player.view_z.is_finite() {
                    player.view_z
                } else {
                    camera.pos.z() + camera.height * 0.5
                };
                let dz = if eye_z > actor.pos.z() + actor.height {
                    actor.pos.z() + actor.height - eye_z
                } else if eye_z < actor.pos.z() {
                    actor.pos.z() - eye_z
                } else {
                    0.0
                };
                Some((
                    dx * dx + dy * dy,
                    dx * dx + dy * dy + dz * dz,
                    crate::sight::check_sight(self, observer, target),
                ))
            })
            .collect()
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

    /// Look through `DukeCamera` actors while a Duke `.DMO` plays.
    pub fn set_demo_cameras(&mut self, enabled: bool) {
        self.demo_cameras = enabled;
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
            check_name: None,
            start_time: 0.0,
            sound_class: None,
            sound_gender: None,
            source_id: None,
            x,
            y,
            z,
            volume,
            pitch: 1.0,
            channel: 0,
            adjust: SimSoundAdjust::Start,
            looping: false,
            no_restart: false,
            channel_flags: 0,
            attenuation,
            seat: None,
        });
    }

    /// Non-positional UI sound heard only by `seat` (0-based), or everyone when `None`.
    pub fn play_seat_sound(&mut self, seat: Option<u8>, name: &str, volume: f32) {
        self.sound_events.push(SimSoundEvent {
            name: name.to_owned(),
            check_name: None,
            start_time: 0.0,
            sound_class: None,
            sound_gender: None,
            source_id: None,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            volume,
            pitch: 1.0,
            channel: 0,
            adjust: SimSoundAdjust::Start,
            looping: false,
            no_restart: false,
            channel_flags: 0,
            attenuation: 0.0,
            seat,
        });
    }

    /// Release Mod HUD input captures owned by `module`.
    pub fn clear_module_captures(&mut self, module: u16) {
        for p in self.players.iter_mut().flatten() {
            if p.capture_module == module {
                p.input_capture = 0;
                p.capture_module = u16::MAX;
            }
        }
    }

    /// Release all input captures (map change / unload).
    pub fn clear_all_captures(&mut self) {
        for p in self.players.iter_mut().flatten() {
            p.input_capture = 0;
            p.capture_module = u16::MAX;
        }
    }

    /// Resume a Duke save-game earthquake (`SE 2` / `A_Quake(4, …, 0, 2048)`).
    pub fn start_duke_quake(&mut self, countdown: i32) {
        if countdown <= 0 {
            return;
        }
        let Some(spot) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if self.actors.get(spot).is_none() {
            return;
        }
        self.quakes.push(Quake {
            spot,
            intensity: 4.0,
            countdown,
            damage_radius: 0.0,
            tremor_radius: 2048.0,
            sound: "world/quake".into(),
            thrust_multiplier: 0.5,
        });
    }

    /// Move player 0 to a Duke save-game pose (Room units, yaw/pitch degrees).
    pub fn set_player_pose(&mut self, x: f64, y: f64, z: f64, yaw_deg: f64, pitch_deg: f64) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if let Some(a) = self.actors.get_mut(mo) {
            a.pos = DVec3::new(x, y, z);
            a.vel = DVec3::ZERO;
            a.angle = Angle::from_degrees(yaw_deg);
            a.pitch = Angle::from_degrees(pitch_deg);
        }
    }

    /// Add degrees to player 0 yaw from a Duke save (`weapon_ang`).
    pub fn add_player_yaw(&mut self, deg: f64) {
        if deg == 0.0 {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if let Some(a) = self.actors.get_mut(mo) {
            a.angle = Angle::from_degrees(a.angle.degrees() + deg);
        }
    }

    /// Set player 0 attacker from a Duke save (`frag_ps` slot).
    pub fn set_player_frag_ps(&mut self, slot: u8) {
        let i = usize::from(slot);
        let mo = self
            .players
            .get(i)
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        if let Some(p) = self.players[0].as_mut() {
            p.attacker = mo;
        }
    }

    /// Mark player 0 dead from a Duke save (`dead_flag`).
    pub fn set_player_dead(&mut self, dead: bool) {
        if !dead {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if let Some(a) = self.actors.get_mut(mo) {
            a.health = 0;
        }
        if let Some(p) = self.players[0].as_mut() {
            p.player_state = crate::player::PlayerState::Dead;
            p.pending_weapon = None;
            p.pspr_state = None;
            p.pspr_tics = 0;
            p.pspr_flash_state = None;
            p.pspr_flash_tics = 0;
            p.extralight = 0;
        }
        self.player_died_at[0] = Some(self.level.maptime);
    }

    /// Set player 0 pawn health from a Duke save (`last_extra`).
    pub fn set_player_health(&mut self, health: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if let Some(a) = self.actors.get_mut(mo) {
            a.health = health.max(0);
        }
    }

    /// Set player 0 clip ammo from a Duke save (`ammo_amount[PISTOL_WEAPON]`).
    pub fn set_player_clip_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(clip) = self.classes.find_name("Clip") else {
            return;
        };
        crate::set_inventory(self, mo, clip, amount.max(0));
    }

    /// Set player 0 shell ammo from a Duke save (`ammo_amount[SHOTGUN_WEAPON]`).
    pub fn set_player_shell_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(shell) = self.classes.find_name("Shell") else {
            return;
        };
        crate::set_inventory(self, mo, shell, amount.max(0));
    }

    /// Set player 0 rocket ammo from a Duke save (`ammo_amount[RPG_WEAPON]`).
    pub fn set_player_rocket_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(rocket) = self.classes.find_name("RocketAmmo") else {
            return;
        };
        crate::set_inventory(self, mo, rocket, amount.max(0));
    }

    /// Set player 0 cell ammo from a Duke save (`ammo_amount[SHRINKER_WEAPON]`).
    pub fn set_player_cell_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(cell) = self.classes.find_name("Cell") else {
            return;
        };
        crate::set_inventory(self, mo, cell, amount.max(0));
    }

    /// Set player 0 pipebomb ammo from a Duke save (`ammo_amount[HANDBOMB_WEAPON]`).
    pub fn set_player_pipebomb_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(ammo) = self.classes.find_name("DukePipebombAmmo") else {
            return;
        };
        crate::set_inventory(self, mo, ammo, amount.max(0));
    }

    /// Set player 0 devastator ammo from a Duke save (`ammo_amount[DEVISTATOR_WEAPON]`).
    pub fn set_player_devastator_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(ammo) = self.classes.find_name("DukeDevastatorAmmo") else {
            return;
        };
        crate::set_inventory(self, mo, ammo, amount.max(0));
    }

    /// Set player 0 freeze ammo from a Duke save (`ammo_amount[FREEZE_WEAPON]`).
    pub fn set_player_freeze_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(ammo) = self.classes.find_name("DukeFreezeAmmo") else {
            return;
        };
        crate::set_inventory(self, mo, ammo, amount.max(0));
    }

    /// Set player 0 tripbomb ammo from a Duke save (`ammo_amount[TRIPBOMB_WEAPON]`).
    pub fn set_player_tripbomb_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(ammo) = self.classes.find_name("DukeTripbombAmmo") else {
            return;
        };
        crate::set_inventory(self, mo, ammo, amount.max(0));
    }

    /// Set player 0 expander ammo from a Duke save (`ammo_amount[GROW_WEAPON]`).
    pub fn set_player_grow_ammo(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(ammo) = self.classes.find_name("DukeGrowAmmo") else {
            return;
        };
        crate::set_inventory(self, mo, ammo, amount.max(0));
    }

    fn duke_weapon_class_for_slot(&self, slot: i16) -> Option<room_core::ClassId> {
        let names: &[&str] = match slot {
            0 => &["DukeKick", "Fist"],
            1 => &["DukePistol", "Pistol"],
            2 => &["DukeShotgun", "Shotgun"],
            3 => &["DukeRipper", "Chaingun"],
            4 => &["DukeRPG", "RocketLauncher"],
            5 => &["DukePipebomb"],
            6 => &["DukeShrinker"],
            7 => &["DukeDevastator"],
            8 => &["DukeTripbomb"],
            9 => &["DukeFreezer"],
            11 => &["DukeExpander"],
            _ => return None,
        };
        names.iter().find_map(|n| self.classes.find_name(n))
    }

    /// Give player 0 every Duke `gotweapon` slot that is set.
    pub fn give_player_gotweapons(&mut self, got: &[u8; 12]) {
        for (slot, flag) in got.iter().enumerate() {
            if *flag == 0 {
                continue;
            }
            if let Ok(slot) = i16::try_from(slot) {
                self.give_player_weapon_slot(slot);
            }
        }
    }

    /// Give player 0 a Duke weapon slot without changing the ready gun.
    pub fn give_player_weapon_slot(&mut self, slot: i16) {
        let Some(cid) = self.duke_weapon_class_for_slot(slot) else {
            return;
        };
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let _ = crate::inventory::give_inventory(self, mo, cid, 1);
    }

    /// Resume player 0 pipebomb wind-up from a Duke save (`hbomb_hold_delay`).
    pub fn set_player_hbomb_hold(&mut self, n: i32) {
        if n <= 0 {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if let Some(cid) = self.classes.find_name("DukeThrowPower") {
            crate::set_inventory(self, mo, cid, n.clamp(1, 6));
        }
        if let Some(cid) = self
            .classes
            .find_name("DukePipebombWeapon")
            .or_else(|| self.classes.find_name("DukePipebomb"))
        {
            let _ = crate::inventory::give_inventory(self, mo, cid, 1);
            if let Some(p) = self.players[0].as_mut() {
                p.ready_weapon = Some(cid);
                p.pending_weapon = None;
            }
            if let Some(st) = self
                .states
                .find_label(cid, "Charge")
                .or_else(|| self.states.find_label(cid, "Fire"))
            {
                let tics = self.states.get(st).map(|s| i32::from(s.tics)).unwrap_or(1);
                if let Some(p) = self.players[0].as_mut() {
                    p.pspr_state = Some(st);
                    p.pspr_tics = tics;
                    p.pspr_y = crate::player::WEAPON_TOP;
                }
            }
        }
        if let Some(p) = self.players[0].as_mut() {
            p.attack_down = true;
        }
    }

    /// Mark player 0 pipebomb remote out from a Duke save (`hbomb_on`).
    pub fn set_player_hbomb_on(&mut self, on: bool) {
        if !on {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(cid) = self.classes.find_name("DukeBombsOut") else {
            return;
        };
        let _ = crate::inventory::give_inventory(self, mo, cid, 1);
    }

    /// Set player 0 medkit from a Duke save (`firstaid_amount`).
    pub fn set_player_firstaid(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(aid) = self
            .classes
            .find_name("DukeFirstAid")
            .or_else(|| self.classes.find_name("FirstAid"))
        else {
            return;
        };
        crate::set_inventory(self, mo, aid, amount.max(0));
    }

    /// Set player 0 steroids from a Duke save (`steroids_amount`).
    pub fn set_player_steroids(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(cid) = self
            .classes
            .find_name("DukeSteroids")
            .or_else(|| self.classes.find_name("Steroids"))
        else {
            return;
        };
        crate::set_inventory(self, mo, cid, amount.max(0));
    }

    /// Engage player 0 flight from a Duke save (`jetpack_on`).
    pub fn set_player_jetpack_on(&mut self, on: bool) {
        if !on {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if let Some(p) = self.players[0].as_mut() {
            p.fly_tics = crate::player::FLIGHT_TICS;
        }
        if let Some(a) = self.actors.get_mut(mo) {
            a.flags |= crate::class::FLAG_NOGRAVITY;
        }
    }

    /// Set player 0 jetpack from a Duke save (`jetpack_amount`).
    pub fn set_player_jetpack(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(cid) = self
            .classes
            .find_name("DukeJetpack")
            .or_else(|| self.classes.find_name("Jetpack"))
        else {
            return;
        };
        crate::set_inventory(self, mo, cid, amount.max(0));
    }

    /// Engage player 0 scuba from a Duke save (`scuba_on`).
    pub fn set_player_scuba_on(&mut self, on: bool) {
        if !on {
            return;
        }
        if let Some(p) = self.players[0].as_mut() {
            p.radsuit = true;
            p.radsuit_tics = crate::player::RADSUIT_TICS;
            let t = i32::try_from(self.level.time).unwrap_or(i32::MAX);
            p.air_finished = t.saturating_add(60 * crate::player::TICRATE);
        }
    }

    /// Set player 0 grounded flag from a Duke save (`on_ground`).
    pub fn set_player_on_ground(&mut self, on: bool) {
        if let Some(p) = self.players[0].as_mut() {
            p.on_ground = on;
        }
    }

    /// Vertical jump impulse. Players always may; other actors need `+CANJUMP` (UZ1).
    pub fn try_actor_jump(&mut self, id: ActorId) -> bool {
        let Some(actor) = self.actors.get(id) else {
            return false;
        };
        // The thinking player's slot is taken out during `player_think`.
        let is_player = self.players.iter().flatten().any(|p| p.mo == Some(id))
            || self.taken_player.is_some_and(|(_, mo)| mo == id);
        if !is_player && !actor.extra_flags.get_name("CANJUMP") {
            return false;
        }
        // `PowerHighJump` multiplies `JumpZ` by its `Strength` (2).
        let jump_z = actor.jump_z * crate::powerups::jump_factor(self, id);
        if jump_z <= 0.0 {
            return false;
        }
        if let Some(a) = self.actors.get_mut(id) {
            a.vel = DVec3::new(a.vel.x(), a.vel.y(), jump_z);
        }
        true
    }

    /// Start player 0 mid-jump from a Duke save (`jumping_counter`).
    pub fn set_player_jumping(&mut self, on: bool) {
        if !on {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let _ = self.try_actor_jump(mo);
        if let Some(p) = self.players[0].as_mut() {
            p.on_ground = false;
        }
    }

    /// Mark player 0 jump still held from a Duke save (`jumping_toggle`).
    pub fn set_player_jumping_toggle(&mut self, on: bool) {
        if let Some(p) = self.players[0].as_mut() {
            p.jump_down = on;
        }
    }

    /// Set player 0 post-teleport lock from a Duke save (`transporter_hold`, already 35 Hz).
    pub fn set_player_transporter_hold(&mut self, tics: u32) {
        if let Some(p) = self.players[0].as_mut() {
            p.transporter_hold_tics = tics;
        }
        if tics == 0 {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if let Some(a) = self.actors.get_mut(mo) {
            a.vel = DVec3::ZERO;
        }
    }

    /// Set player 0 inventory-key hold from a Duke save (`toggle_key_flag`).
    pub fn set_player_toggle_key_flag(&mut self, on: bool) {
        if let Some(p) = self.players[0].as_mut() {
            p.toggle_key_flag = on;
            if on {
                p.invuse_down = true;
            }
        }
    }

    /// Set player 0 selected inventory from a Duke save (`inven_icon`).
    pub fn set_player_inven_icon(&mut self, icon: u8) {
        if let Some(p) = self.players[0].as_mut() {
            p.inven_icon = icon;
        }
    }

    /// Set player 0 footprint palette from a Duke save (`footprintpal`).
    pub fn set_player_footprint_pal(&mut self, pal: u8) {
        if let Some(p) = self.players[0].as_mut() {
            p.footprint_pal = Some(pal);
        }
    }

    /// Set player 0 remaining footprints from a Duke save (`footprintcount`).
    pub fn set_player_footprint_count(&mut self, n: u8) {
        if let Some(p) = self.players[0].as_mut() {
            p.footprint_count = Some(n);
        }
    }

    /// Set player 0 footprint shade from a Duke save (`footprintshade`).
    pub fn set_player_footprint_shade(&mut self, shade: i16) {
        if let Some(p) = self.players[0].as_mut() {
            p.footprint_shade = Some(shade);
        }
    }

    /// Set player 0 previous look pitch from a Duke save (`ohoriz` / `ohorizoff`).
    pub fn set_player_prev_pitch(&mut self, pitch: i32) {
        if let Some(p) = self.players[0].as_mut() {
            p.prev_pitch = Some(pitch.clamp(-90, 90));
        }
    }

    /// Set player 0 HUD inventory-name tics from a Duke save (`invdisptime`).
    pub fn set_player_invdisp_tics(&mut self, tics: u32) {
        let (icon, was) = self.players[0]
            .as_ref()
            .map(|p| (p.inven_icon, p.invdisp_tics))
            .unwrap_or((0, 0));
        if let Some(p) = self.players[0].as_mut() {
            p.invdisp_tics = tics;
        }
        if was == 0 && tics > 0 {
            if let Some(name) = duke_inven_name(icon) {
                self.push_message_kind(MessageKind::Pickup, name);
            }
        }
    }

    /// Set player 0 view-bob phase from a Duke save (`pycount`).
    pub fn set_player_pycount(&mut self, n: i16) {
        if let Some(p) = self.players[0].as_mut() {
            p.pycount = Some(n & 2047);
        }
    }

    /// Set player 0 footstep phase from a Duke save (`walking_snd_toggle`).
    pub fn set_player_walking_snd_toggle(&mut self, on: bool) {
        if let Some(p) = self.players[0].as_mut() {
            p.walk_snd_toggle = on;
        }
    }

    /// Set player 0 palette from a Duke save (`palookup`).
    pub fn set_player_palookup(&mut self, pal: u8) {
        let Some(mo) = self.players[0].as_ref().and_then(|p| p.mo) else {
            return;
        };
        if let Some(a) = self.actors.get_mut(mo) {
            a.translation = if pal == 0 {
                String::new()
            } else {
                format!("DukePal{pal}")
            };
        }
    }

    /// Play player 0 knuckle-crack from a Duke save (`knuckle_incs`).
    pub fn set_player_knuckle_incs(&mut self, on: bool) {
        if !on {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let name = self
            .actors
            .get(mo)
            .and_then(|a| self.classes.get(a.class))
            .and_then(|c| c.idle_taunts.first().cloned())
            .unwrap_or_else(|| "duke/crack".into());
        if let Some(a) = self.actors.get(mo) {
            self.play_sound(&name, a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
        if let Some(p) = self.players[0].as_mut() {
            p.idle_tics = 0;
        }
    }

    /// Set player 0 hard-land dip from a Duke save (`hard_landing`).
    pub fn set_player_hard_landing(&mut self, n: u8) {
        if let Some(p) = self.players[0].as_mut() {
            p.hard_landing = n;
        }
    }

    /// Resume player 0 tip pose from a Duke save (`tipincs`, already 35 Hz).
    pub fn set_player_tip_incs(&mut self, tics: u32) {
        let mo = self.players[0].as_ref().and_then(|p| p.mo);
        if let Some(p) = self.players[0].as_mut() {
            p.tip_incs = tics;
        }
        if tics == 0 {
            return;
        }
        if let Some(a) = mo.and_then(|id| self.actors.get(id)) {
            self.play_sound("duke/duke_tip1", a.pos.x(), a.pos.y(), a.pos.z(), 1.0);
        }
    }

    /// Set player 0 look-recenter from a Duke save (`return_to_center`, already 35 Hz).
    pub fn set_player_return_to_center(&mut self, tics: u32) {
        if let Some(p) = self.players[0].as_mut() {
            p.return_to_center = tics;
        }
    }

    /// Set player 0 screen tilt from a Duke save (`rotscrnang`).
    pub fn set_player_rotscrnang(&mut self, n: i16) {
        let mo = self.players[0].as_ref().and_then(|p| p.mo);
        if let Some(p) = self.players[0].as_mut() {
            p.rotscrnang = n;
        }
        if let Some(a) = mo.and_then(|id| self.actors.get_mut(id)) {
            a.roll = Angle::from_degrees(f64::from(-(i32::from(n) * 360) / 2048));
        }
    }

    /// Set player 0 pain-flash cooldown from a Duke save (`hurt_delay`, already 35 Hz).
    pub fn set_player_hurt_delay(&mut self, tics: u32) {
        if let Some(p) = self.players[0].as_mut() {
            p.hurt_delay_tics = tics;
            let flash = i32::try_from(tics.min(100)).unwrap_or(100);
            if p.damage_count < flash {
                p.damage_count = flash;
            }
        }
    }

    /// Set player 0 remaining air from a Duke save (`airleft`, already 35 Hz).
    pub fn set_player_air_tics(&mut self, tics: u32) {
        if let Some(p) = self.players[0].as_mut() {
            let t = i32::try_from(self.level.time).unwrap_or(i32::MAX);
            let remaining = i32::try_from(tics).unwrap_or(i32::MAX);
            p.air_finished = t.saturating_add(remaining);
        }
    }

    /// Set player 0 scuba from a Duke save (`scuba_amount`).
    pub fn set_player_scuba(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(cid) = self
            .classes
            .find_name("DukeScubaGear")
            .or_else(|| self.classes.find_name("ScubaGear"))
        else {
            return;
        };
        crate::set_inventory(self, mo, cid, amount.max(0));
    }

    /// Set player 0 holoduke from a Duke save (`holoduke_amount`).
    pub fn set_player_holoduke(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(cid) = self
            .classes
            .find_name("DukeHoloduke")
            .or_else(|| self.classes.find_name("Holoduke"))
        else {
            return;
        };
        crate::set_inventory(self, mo, cid, amount.max(0));
    }

    /// Set player 0 boots from a Duke save (`boot_amount`).
    pub fn set_player_boots(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(cid) = self
            .classes
            .find_name("DukeBoots")
            .or_else(|| self.classes.find_name("Boots"))
        else {
            return;
        };
        crate::set_inventory(self, mo, cid, amount.max(0));
    }

    /// Spawn a holoduke decoy at player 0 (`holoduke_on >= 0`).
    pub fn spawn_player_holoduke_decoy(&mut self) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(decoy) = self.classes.find_name("DukeHolodukeDecoy") else {
            return;
        };
        let Some(a) = self.actors.get(mo) else {
            return;
        };
        let pos = a.pos;
        let angle = a.angle;
        let _ = self.spawn_at(decoy, pos, angle);
    }

    /// Give player 0 access cards from a Duke save (`got_access` bits).
    pub fn set_player_got_access(&mut self, bits: i16) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if bits & 1 != 0 {
            if let Some(cid) = self.classes.find_name("BlueCard") {
                let _ = crate::inventory::give_inventory(self, mo, cid, 1);
            }
        }
        if bits & 2 != 0 {
            if let Some(cid) = self.classes.find_name("RedCard") {
                let _ = crate::inventory::give_inventory(self, mo, cid, 1);
            }
        }
        if bits & 4 != 0 {
            if let Some(cid) = self.classes.find_name("YellowCard") {
                let _ = crate::inventory::give_inventory(self, mo, cid, 1);
            }
        }
    }

    /// Set player 0 mid-air fall tics from a Duke save (`falling_counter`).
    pub fn set_player_falling_tics(&mut self, tics: u32) {
        if let Some(p) = self.players[0].as_mut() {
            p.falling_tics = tics;
        }
    }

    /// Engage player 0 night vision from a Duke save (`heat_on`).
    pub fn set_player_heat_on(&mut self, on: bool) {
        if !on {
            return;
        }
        if let Some(p) = self.players[0].as_mut() {
            p.infrared = true;
            p.infrared_tics = crate::player::INFRARED_TICS;
        }
    }

    /// Set player 0 night vision from a Duke save (`heat_amount`).
    pub fn set_player_heat(&mut self, amount: i32) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        let Some(cid) = self
            .classes
            .find_name("DukeNightVision")
            .or_else(|| self.classes.find_name("NightVision"))
        else {
            return;
        };
        crate::set_inventory(self, mo, cid, amount.max(0));
    }

    /// Apply dump `sector[]` floor / ceiling Z (Room units) in map order.
    pub fn apply_duke_sector_zs(&mut self, zs: &[(f64, f64)]) {
        for (sec, &(floor_z, ceiling_z)) in self.sectors.iter_mut().zip(zs.iter()) {
            sec.floor_z = floor_z;
            sec.ceiling_z = ceiling_z;
            sec.prev_floor_z = floor_z;
            sec.prev_ceiling_z = ceiling_z;
        }
    }

    /// Apply dump `sector[]` floor / ceiling tiles in map order.
    pub fn apply_duke_sector_pics(&mut self, pics: &[(String, String)]) {
        for (i, (floor, ceil)) in pics.iter().enumerate() {
            let si = i as u32;
            self.sector_floor_pics.insert(si, floor.clone());
            self.sector_ceiling_pics.insert(si, ceil.clone());
        }
    }

    /// Apply dump `sector[]` light in map order.
    pub fn apply_duke_sector_lights(&mut self, lights: &[i16]) {
        for (sec, &light) in self.sectors.iter_mut().zip(lights.iter()) {
            sec.light = light;
        }
    }

    /// Apply dump `sector[]` fog density in map order.
    pub fn apply_duke_sector_fogs(&mut self, fogs: &[u8]) {
        for (sec, &fog) in self.sectors.iter_mut().zip(fogs.iter()) {
            sec.fog_density = fog;
        }
    }

    /// Apply dump `gotpic` tile-seen bits; any set bit reveals the automap.
    pub fn apply_duke_gotpic(&mut self, bits: &[u8]) {
        self.gotpic = bits.to_vec();
        if bits.iter().any(|&b| b != 0) {
            if let Some(p) = self.players[0].as_mut() {
                p.allmap = true;
            }
        }
    }

    /// Move living `COUNTKILL`s onto dump `sprite[]` XY (extra > 0), in think order.
    pub fn apply_duke_sprite_xy(&mut self, hits: &[(f64, f64)]) {
        let player_mo = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let ids: Vec<_> = self
            .thinkers
            .default
            .iter()
            .copied()
            .filter(|&id| {
                if Some(id) == player_mo {
                    return false;
                }
                self.actors.get(id).is_some_and(|a| {
                    if a.destroyed || a.health <= 0 {
                        return false;
                    }
                    let flags = a.flags | self.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
                    (flags & crate::class::FLAG_COUNTKILL) != 0
                })
            })
            .collect();
        for (id, &(x, y)) in ids.iter().zip(hits.iter()) {
            if let Some(a) = self.actors.get_mut(*id) {
                a.pos = DVec3::new(x, y, a.pos.z());
            }
        }
    }

    /// Apply dump `sprite[]` extra as health onto non-player actors at those XY.
    pub fn apply_duke_sprite_health(&mut self, hits: &[(f64, f64, i32)]) {
        let player_mo = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let ids: Vec<_> = self.thinkers.default.to_vec();
        for &(x, y, hp) in hits {
            if hp <= 0 {
                continue;
            }
            let Some(id) = ids.iter().copied().find(|&id| {
                if Some(id) == player_mo {
                    return false;
                }
                self.actors.get(id).is_some_and(|a| {
                    !a.destroyed && (a.pos.x() - x).abs() < 8.0 && (a.pos.y() - y).abs() < 8.0
                })
            }) else {
                continue;
            };
            if let Some(a) = self.actors.get_mut(id) {
                a.health = hp;
            }
        }
    }

    /// Apply dump `sprite[]` `cstat` onto non-player actors at those XY.
    pub fn apply_duke_sprite_cstat(&mut self, hits: &[(f64, f64, i32)]) {
        let player_mo = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let ids: Vec<_> = self.thinkers.default.to_vec();
        for &(x, y, cstat) in hits {
            let Some(id) = ids.iter().copied().find(|&id| {
                if Some(id) == player_mo {
                    return false;
                }
                self.actors.get(id).is_some_and(|a| {
                    !a.destroyed && (a.pos.x() - x).abs() < 8.0 && (a.pos.y() - y).abs() < 8.0
                })
            }) else {
                continue;
            };
            if let Some(a) = self.actors.get_mut(id) {
                if cstat & 1 != 0 {
                    a.flags |= crate::class::FLAG_SOLID;
                } else {
                    a.flags &= !crate::class::FLAG_SOLID;
                }
                a.alpha = if cstat & 2 != 0 { 0.66 } else { 1.0 };
                if cstat & 32_768 != 0 {
                    a.flags |= crate::class::FLAG_DONTDRAW;
                } else {
                    a.flags &= !crate::class::FLAG_DONTDRAW;
                }
            }
        }
    }

    /// Apply dump `sprite[]` `xrepeat` / `yrepeat` / `clipdist` at those XY.
    pub fn apply_duke_sprite_scale(&mut self, hits: &[(f64, f64, u8, u8, u8)]) {
        let player_mo = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let ids: Vec<_> = self.thinkers.default.to_vec();
        for &(x, y, xrepeat, yrepeat, clipdist) in hits {
            let Some(id) = ids.iter().copied().find(|&id| {
                if Some(id) == player_mo {
                    return false;
                }
                self.actors.get(id).is_some_and(|a| {
                    !a.destroyed && (a.pos.x() - x).abs() < 8.0 && (a.pos.y() - y).abs() < 8.0
                })
            }) else {
                continue;
            };
            if let Some(a) = self.actors.get_mut(id) {
                if xrepeat != 0 || yrepeat != 0 {
                    a.scale_x = f64::from(xrepeat) / 64.0;
                    a.scale_y = f64::from(yrepeat) / 64.0;
                }
                if clipdist != 0 {
                    a.radius = (f64::from(clipdist) / 16.0).clamp(1.0, 64.0);
                }
            }
        }
    }

    /// Apply dump `sprite[]` `pal` onto non-player actors at those XY.
    pub fn apply_duke_sprite_pal(&mut self, hits: &[(f64, f64, u8)]) {
        let player_mo = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let ids: Vec<_> = self.thinkers.default.to_vec();
        for &(x, y, pal) in hits {
            let Some(id) = ids.iter().copied().find(|&id| {
                if Some(id) == player_mo {
                    return false;
                }
                self.actors.get(id).is_some_and(|a| {
                    !a.destroyed && (a.pos.x() - x).abs() < 8.0 && (a.pos.y() - y).abs() < 8.0
                })
            }) else {
                continue;
            };
            if let Some(a) = self.actors.get_mut(id) {
                a.translation = if pal == 0 {
                    String::new()
                } else {
                    format!("DukePal{pal}")
                };
            }
        }
    }

    /// Apply dump `sprite[]` `ang` onto non-player actors at those XY.
    pub fn apply_duke_sprite_ang(&mut self, hits: &[(f64, f64, f64)]) {
        let player_mo = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let ids: Vec<_> = self.thinkers.default.to_vec();
        for &(x, y, yaw) in hits {
            let Some(id) = ids.iter().copied().find(|&id| {
                if Some(id) == player_mo {
                    return false;
                }
                self.actors.get(id).is_some_and(|a| {
                    !a.destroyed && (a.pos.x() - x).abs() < 8.0 && (a.pos.y() - y).abs() < 8.0
                })
            }) else {
                continue;
            };
            if let Some(a) = self.actors.get_mut(id) {
                a.angle = Angle::from_degrees(yaw);
            }
        }
    }

    /// Apply dump `sprite[]` Z onto non-player actors at those XY.
    pub fn apply_duke_sprite_z(&mut self, hits: &[(f64, f64, f64)]) {
        let player_mo = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo);
        let ids: Vec<_> = self.thinkers.default.to_vec();
        for &(x, y, z) in hits {
            let Some(id) = ids.iter().copied().find(|&id| {
                if Some(id) == player_mo {
                    return false;
                }
                self.actors.get(id).is_some_and(|a| {
                    !a.destroyed && (a.pos.x() - x).abs() < 8.0 && (a.pos.y() - y).abs() < 8.0
                })
            }) else {
                continue;
            };
            if let Some(a) = self.actors.get_mut(id) {
                a.pos = DVec3::new(a.pos.x(), a.pos.y(), z);
            }
        }
    }

    /// Apply dump `wall[]` extra onto destructible linedefs at those XY.
    pub fn apply_duke_wall_health(&mut self, hits: &[(f64, f64, i32)]) {
        for &(x, y, hp) in hits {
            let Some(i) = self.health_line_at(x, y) else {
                continue;
            };
            self.line_health.insert(i, hp.max(0));
            if hp <= 0 {
                const CLEAR: i32 = ML_BLOCKING
                    | ML_BLOCKMONSTERS
                    | ML_BLOCK_PLAYERS
                    | ML_BLOCKEVERYTHING
                    | ML_BLOCKHITSCAN;
                let map = std::sync::Arc::make_mut(&mut self.map);
                if let Some(l) = map.lines.get_mut(i as usize) {
                    l.block_projectiles = false;
                    l.block_hitscan = false;
                    l.block_sight = false;
                    l.flags &= !(CLEAR as i16);
                }
                self.line_flags
                    .insert(i, self.line_flags.get(&i).copied().unwrap_or(0) & !CLEAR);
            }
        }
    }

    /// Apply dump `wall[]` mid tiles onto linedefs at those XY.
    pub fn apply_duke_wall_pics(&mut self, hits: &[(f64, f64, String)]) {
        for (x, y, name) in hits {
            let Some(i) = self.wall_line_at(*x, *y) else {
                continue;
            };
            let sides: Vec<u32> = self
                .map
                .lines
                .get(i as usize)
                .map(|l| l.sides.iter().flatten().map(|s| s.raw()).collect())
                .unwrap_or_default();
            for idx in sides {
                self.switch_mids.insert(idx, name.clone());
                if let Some(side) = std::sync::Arc::make_mut(&mut self.map)
                    .sides
                    .get_mut(idx as usize)
                {
                    side.middle = name.clone();
                }
            }
        }
    }

    fn wall_line_at(&self, x: f64, y: f64) -> Option<u32> {
        if let Some(i) = self.health_line_at(x, y) {
            return Some(i);
        }
        for (i, l) in self.map.lines.iter().enumerate() {
            for vid in [l.v1, l.v2] {
                let Some(v) = self.map.vertices.get(vid.raw() as usize) else {
                    continue;
                };
                if (v.pos.x() - x).abs() < 8.0 && (v.pos.y() - y).abs() < 8.0 {
                    return Some(i as u32);
                }
            }
        }
        None
    }

    fn health_line_at(&self, x: f64, y: f64) -> Option<u32> {
        for &i in self.line_health.keys() {
            let Some(l) = self.map.lines.get(i as usize) else {
                continue;
            };
            for vid in [l.v1, l.v2] {
                let Some(v) = self.map.vertices.get(vid.raw() as usize) else {
                    continue;
                };
                if (v.pos.x() - x).abs() < 8.0 && (v.pos.y() - y).abs() < 8.0 {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Set player 0 armor from a Duke save (`shield_amount`).
    pub fn set_player_armor(&mut self, armor: i32) {
        if let Some(p) = self.players[0].as_mut() {
            p.armor = armor.clamp(0, 200);
            p.armor_class = if p.armor > 0 { 1 } else { 0 };
        }
    }

    /// Start player 0's Duke `/` quick kick (`CON pkick`).
    pub fn player_quick_kick(&mut self) {
        let Some(kick) = self
            .classes
            .find_name("DukeKick")
            .or_else(|| self.classes.find_name("Fist"))
        else {
            return;
        };
        let Some(mo) = self.players[0].as_ref().and_then(|p| p.mo) else {
            return;
        };
        if self.inventory.count(mo, kick) == 0 {
            return;
        }
        let Some(fire) = self.states.find_label(kick, "Fire") else {
            return;
        };
        let tics = self
            .states
            .get(fire)
            .map(|st| i32::from(st.tics))
            .unwrap_or(1);
        if let Some(p) = self.players[0].as_mut() {
            if p.quick_kick_restore.is_some() {
                return;
            }
            if p.ready_weapon != Some(kick) {
                p.quick_kick_restore = p.ready_weapon;
            }
            p.pending_weapon = None;
            p.ready_weapon = Some(kick);
            p.pspr_y = crate::player::WEAPON_TOP;
            p.pspr_state = Some(fire);
            p.pspr_tics = tics;
        }
    }

    /// Equip and fire player 0's gun from a Duke save (`wantweaponfire`).
    pub fn set_player_wantweapon_fire(&mut self, slot: i16) {
        if slot < 0 {
            return;
        }
        self.set_player_weapon_slot(slot);
        self.set_player_kickback(1);
    }

    /// Set player 0 quote timer from a Duke save (`fta` / `ftq`, already 35 Hz).
    pub fn set_player_fta_quote(&mut self, tics: u32, quote: i16) {
        if let Some(p) = self.players[0].as_mut() {
            p.fta_tics = tics;
            p.ftq = quote;
        }
    }

    /// Look through the actor at dump sprite XY from a Duke save (`newowner`).
    pub fn look_through_duke_sprite_at(&mut self, x: f64, y: f64) {
        let player_mo = self.players[0].as_ref().and_then(|p| p.mo);
        let Some(id) = self.thinkers.default.iter().copied().find(|&id| {
            if Some(id) == player_mo {
                return false;
            }
            self.actors.get(id).is_some_and(|a| {
                !a.destroyed && (a.pos.x() - x).abs() < 8.0 && (a.pos.y() - y).abs() < 8.0
            })
        }) else {
            return;
        };
        if let Some(p) = self.players[0].as_mut() {
            p.camera = Some(id);
            p.camera_revert = true;
        }
    }

    /// Use the `+USESPECIAL` actor at dump sprite XY from a Duke save (`access_spritenum`).
    pub fn use_duke_sprite_at(&mut self, x: f64, y: f64) {
        let player_mo = self.players[0].as_ref().and_then(|p| p.mo);
        let Some(id) = self.thinkers.default.iter().copied().find(|&id| {
            if Some(id) == player_mo {
                return false;
            }
            self.actors.get(id).is_some_and(|a| {
                if a.destroyed || (a.pos.x() - x).abs() >= 8.0 || (a.pos.y() - y).abs() >= 8.0 {
                    return false;
                }
                let flags = a.flags | self.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
                (flags & crate::class::FLAG_USESPECIAL) != 0
            })
        }) else {
            return;
        };
        if let Some(a) = self.actors.get_mut(id) {
            a.target = player_mo;
        }
        let Some(class) = self.actors.get(id).map(|a| a.class) else {
            return;
        };
        let Some(st) = self.states.find_label(class, "Use") else {
            return;
        };
        self.set_state(id, Some(st), true);
    }

    /// Use the linedef at dump wall XY from a Duke save (`access_wallnum`).
    pub fn use_duke_wall_at(&mut self, x: f64, y: f64) {
        let Some(i) = self.wall_line_at(x, y) else {
            return;
        };
        let mo = self.players[0].as_ref().and_then(|p| p.mo);
        let _ = crate::specials::activate_line(
            self,
            room_core::LineId::from_raw(i),
            mo,
            crate::specials::ActivationType::Use,
        );
    }

    /// Finish player 0's access-card use from a Duke save (`access_incs`).
    pub fn set_player_access_incs(&mut self, on: bool) {
        if !on {
            return;
        }
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            return;
        };
        if !crate::use_lines::use_lines(self, mo) {
            let _ = crate::use_lines::use_actors(self, mo);
        }
    }

    /// Equip and fire player 0's kick from a Duke save (`knee_incs`).
    pub fn set_player_knee_incs(&mut self, n: i16) {
        if n <= 0 {
            return;
        }
        self.set_player_weapon_slot(0);
        self.set_player_kickback(n);
    }

    /// Flash player 0 HUD from a Duke save (`pals` / `pals_time`).
    pub fn set_player_pals_flash(&mut self, n: i32) {
        if let Some(p) = self.players[0].as_mut() {
            p.damage_count = n.clamp(0, 100);
        }
    }

    /// Keep player 0 firing from a Duke save (`rapid_fire_hold`).
    pub fn set_player_rapid_fire_hold(&mut self, on: bool) {
        if let Some(p) = self.players[0].as_mut() {
            p.attack_down = on;
            if on {
                p.refire = p.refire.max(1);
            }
        }
    }

    /// Jump player 0's ready gun to `Fire` from a Duke save (`show_empty_weapon`).
    pub fn set_player_show_empty_weapon(&mut self, on: bool) {
        if on {
            self.set_player_kickback(1);
        }
    }

    /// Jump player 0's ready gun to `Fire` from a Duke save (`kickback_pic`).
    pub fn set_player_kickback(&mut self, pic: i16) {
        if pic <= 0 {
            return;
        }
        let Some(wpn) = self.players[0].as_ref().and_then(|p| p.ready_weapon) else {
            return;
        };
        let Some(fire) = self.states.find_label(wpn, "Fire") else {
            return;
        };
        let tics = self
            .states
            .get(fire)
            .map(|st| i32::from(st.tics))
            .unwrap_or(1);
        if let Some(p) = self.players[0].as_mut() {
            p.pspr_state = Some(fire);
            p.pspr_tics = tics;
            p.pspr_y = crate::player::WEAPON_TOP;
            p.attack_down = true;
            p.refire = i32::from(pic);
        }
    }

    /// Holster player 0's ready gun from a Duke save (`holster_weapon`).
    pub fn set_player_holster(&mut self, on: bool) {
        if !on {
            return;
        }
        if let Some(p) = self.players[0].as_mut() {
            p.ready_weapon = None;
            p.pending_weapon = None;
            p.pspr_state = None;
            p.pspr_tics = 0;
            p.pspr_y = crate::player::WEAPON_BOTTOM;
        }
    }

    /// Set player 0 HUD gun Y from a Duke save (`weapon_pos`).
    pub fn set_player_pspr_y(&mut self, y: f64) {
        if let Some(p) = self.players[0].as_mut() {
            p.pspr_y = y.clamp(crate::player::WEAPON_TOP, crate::player::WEAPON_BOTTOM);
        }
    }

    /// Equip player 0 from a Duke save `curr_weapon` slot.
    pub fn set_player_weapon_slot(&mut self, slot: i16) {
        let Some(cid) = self.duke_weapon_class_for_slot(slot) else {
            return;
        };
        self.give_player_weapon_slot(slot);
        if let Some(p) = self.players[0].as_mut() {
            p.ready_weapon = Some(cid);
            p.pending_weapon = None;
            p.pspr_state = None;
            p.pspr_y = crate::player::WEAPON_TOP;
        }
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

    /// Queue a render-only FX event (no-op when [`Self::fx_events_enabled`] is false).
    pub fn push_fx(&mut self, ev: crate::fx_events::FxEvent) {
        if self.fx_events_enabled {
            self.fx_events.push(ev);
        }
    }

    /// Drain render FX events for the shell / renderer.
    pub fn drain_fx_events(&mut self) -> Vec<crate::fx_events::FxEvent> {
        std::mem::take(&mut self.fx_events)
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
            render_style: None,
            velocity: room_core::DVec3::ZERO,
            acceleration: room_core::DVec3::ZERO,
            alpha: 1.0,
            fade_step: -1.0,
            size_step: 0.0,
            roll: [0.0; 3],
            flags: 0,
            fade_out_step: 0.0,
            texture: None,
        });
    }

    /// Queue a DECORATE/ZScript particle with motion and fade parameters.
    pub fn spawn_particle_ex(
        &mut self,
        color: u32,
        fullbright: bool,
        lifetime: i32,
        size: f64,
        position: room_core::DVec3,
        velocity: room_core::DVec3,
        acceleration: room_core::DVec3,
        alpha: f64,
        fade_step: f64,
        size_step: f64,
        render_style: Option<i32>,
        roll: [f64; 3],
        flags: u32,
        fade_out_step: f64,
        texture: Option<String>,
    ) {
        if lifetime <= 0 {
            return;
        }
        self.particle_events.push(SimParticleEvent {
            color,
            lifetime: lifetime.clamp(1, 255),
            size: size.abs().max(0.1),
            x: position.x(),
            y: position.y(),
            z: position.z(),
            fullbright,
            render_style,
            velocity,
            acceleration,
            alpha: alpha.clamp(0.0, 1.0),
            fade_step,
            size_step,
            roll,
            flags,
            fade_out_step,
            texture: texture.filter(|t| !t.is_empty() && t != "-"),
        });
    }

    /// Drain ACS `SpawnParticle` events since the last call.
    pub fn drain_particle_events(&mut self) -> Vec<SimParticleEvent> {
        std::mem::take(&mut self.particle_events)
    }

    /// Queue a console diagnostic line.
    pub fn push_debug(&mut self, text: impl Into<String>) {
        self.debug_log.push(text.into());
    }

    /// Drain console diagnostics since the last call.
    pub fn drain_debug(&mut self) -> Vec<String> {
        std::mem::take(&mut self.debug_log)
    }

    /// Where the player is, what they face, and which sectors or doors moved.
    pub fn push_view_debug(&mut self, why: &str) {
        let Some(mo) = self
            .players
            .first()
            .and_then(|p| p.as_ref())
            .and_then(|p| p.mo)
        else {
            self.push_debug(format!("{why}: no player"));
            return;
        };
        let Some(actor) = self.actors.get(mo) else {
            return;
        };
        let (x, y, z) = (actor.pos.x(), actor.pos.y(), actor.pos.z());
        let yaw = actor.angle.radians();
        let (fx, fy) = (yaw.cos(), yaw.sin());
        self.push_debug(format!(
            "{why}: pos ({x:.0},{y:.0},{z:.0}) ang {:.0}",
            actor.angle.degrees()
        ));
        match crate::physics::sector_at_xy(self, x, y) {
            Some(sid) => {
                let st = &self.map.sectors[sid];
                let floor = self.sectors.get(sid).map_or(st.floor_height, |s| s.floor_z);
                let ceil = self
                    .sectors
                    .get(sid)
                    .map_or(st.ceiling_height, |s| s.ceiling_z);
                self.push_debug(format!(
                    "sector {sid} floor {floor:.0} (map {:.0}) ceil {ceil:.0} (map {:.0}) gap {:.0} special {} floor {} ceil {}",
                    st.floor_height,
                    st.ceiling_height,
                    ceil - floor,
                    st.special,
                    st.floor_pic,
                    st.ceiling_pic
                ));
            }
            None => self.push_debug("sector none — outside the map"),
        }
        let mut best: Option<(f64, usize)> = None;
        for (i, line) in self.map.lines.iter().enumerate() {
            let Some(v1) = self.map.vertices.get(line.v1.raw() as usize) else {
                continue;
            };
            let Some(v2) = self.map.vertices.get(line.v2.raw() as usize) else {
                continue;
            };
            let mx = (v1.pos.x() + v2.pos.x()) * 0.5;
            let my = (v1.pos.y() + v2.pos.y()) * 0.5;
            let dx = mx - x;
            let dy = my - y;
            let dist = dx.hypot(dy);
            if !(1.0..180.0).contains(&dist) {
                continue;
            }
            if dx / dist * fx + dy / dist * fy < 0.65 {
                continue;
            }
            if best.is_none_or(|(d, _)| dist < d) {
                best = Some((dist, i));
            }
        }
        let facing = if let Some((dist, i)) = best {
            let line = &self.map.lines[i];
            let face = |slot: usize| {
                line.sides[slot]
                    .and_then(|sid| self.map.sides.get(sid.raw() as usize))
                    .map(|s| format!("top={} mid={} bot={}", s.upper, s.middle, s.lower))
                    .unwrap_or_else(|| "-".into())
            };
            format!(
                "facing line {i} dist {dist:.0} special {} front [{}] back [{}]",
                line.special,
                face(0),
                face(1)
            )
        } else {
            "facing line none".to_owned()
        };
        self.push_debug(facing);
        let mut moved_lines = Vec::new();
        let mut moved = 0u32;
        for (i, dyn_s) in self.sectors.iter().enumerate() {
            let Some(st) = self.map.sectors.get(i) else {
                continue;
            };
            let df = (dyn_s.floor_z - st.floor_height).abs();
            let dc = (dyn_s.ceiling_z - st.ceiling_height).abs();
            if df > 48.0 || dc > 48.0 {
                moved += 1;
                if moved <= 8 {
                    moved_lines.push(format!(
                        "moved sector {i} floor {:.0}->{:.0} ceil {:.0}->{:.0}",
                        st.floor_height, dyn_s.floor_z, st.ceiling_height, dyn_s.ceiling_z
                    ));
                }
            }
        }
        for line in moved_lines {
            self.push_debug(line);
        }
        if moved > 8 {
            self.push_debug(format!("{moved} sectors have moved"));
        }
        let polys: Vec<String> = self
            .polyobjs
            .iter()
            .filter(|po| {
                (po.center.x() - x).hypot(po.center.y() - y) < 700.0
                    || po.angle.abs() > 0.05
                    || po.motion.is_some()
            })
            .map(|po| {
                format!(
                    "poly {} ang {:.0} deg at ({:.0},{:.0}) moving {}",
                    po.id,
                    po.angle.to_degrees(),
                    po.center.x(),
                    po.center.y(),
                    po.motion.is_some()
                )
            })
            .collect();
        for line in polys {
            self.push_debug(line);
        }
    }

    /// Queue a HUD message for the shell.
    pub fn push_message(&mut self, text: impl Into<String>) {
        self.push_message_to(MessageTo::Unscoped, text);
    }

    /// Queue a HUD message for everyone or one seat.
    pub fn push_message_to(&mut self, to: MessageTo, text: impl Into<String>) {
        self.push_message_to_kind(to, MessageKind::Other, text);
    }

    /// Queue a classified HUD message (unscoped).
    pub fn push_message_kind(&mut self, kind: MessageKind, text: impl Into<String>) {
        self.push_message_to_kind(MessageTo::Unscoped, kind, text);
    }

    /// Queue a classified HUD message for everyone or one seat.
    pub fn push_message_to_kind(
        &mut self,
        to: MessageTo,
        kind: MessageKind,
        text: impl Into<String>,
    ) {
        self.messages.push(HudMessage {
            to,
            text: text.into(),
            kind,
        });
    }

    /// Drain HUD message texts since the last call (every recipient).
    pub fn drain_messages(&mut self) -> Vec<String> {
        self.drain_hud_messages()
            .into_iter()
            .map(|m| m.text)
            .collect()
    }

    /// Drain HUD messages with their recipients since the last call.
    pub fn drain_hud_messages(&mut self) -> Vec<HudMessage> {
        std::mem::take(&mut self.messages)
    }

    /// Drain deaths recorded since the last call.
    pub fn drain_kill_events(&mut self) -> Vec<KillEvent> {
        std::mem::take(&mut self.kill_events)
    }

    /// Count a kill (0), item (1) or secret (2) for the player whose pawn is
    /// `actor`, if any.
    pub(crate) fn credit_player_stat(&mut self, actor: Option<ActorId>, stat: usize) {
        let Some(actor) = actor else {
            return;
        };
        let seat = self.player_seat_of(actor).map(usize::from).or_else(|| {
            self.taken_player
                .filter(|(_, mo)| *mo == actor)
                .map(|(s, _)| s)
        });
        if let Some(cell) = seat
            .and_then(|s| self.player_stats.get_mut(s))
            .and_then(|r| r.get_mut(stat))
        {
            *cell = cell.saturating_add(1);
        }
    }

    /// Count a kill (0), item (1) or secret (2) for player `seat`.
    pub(crate) fn credit_seat_stat(&mut self, seat: u8, stat: usize) {
        if let Some(cell) = self
            .player_stats
            .get_mut(usize::from(seat))
            .and_then(|r| r.get_mut(stat))
        {
            *cell = cell.saturating_add(1);
        }
    }

    /// Scoreboard rows for every player in the game (UZDoom `HU_DrawScores`).
    #[must_use]
    pub fn scoreboard(&self) -> Vec<ScoreRow> {
        let mut rows = Vec::new();
        for (seat, p) in self.players.iter().enumerate() {
            let Some(p) = p else {
                continue;
            };
            // Doom frag count: others killed, minus suicides.
            let frags: i32 = p
                .frags
                .iter()
                .enumerate()
                .map(|(j, &n)| if j == seat { -n } else { n })
                .sum();
            let deaths: i32 = self
                .players
                .iter()
                .flatten()
                .map(|q| q.frags.get(seat).copied().unwrap_or(0))
                .sum();
            let mut buf = [0u8; 64];
            let n = usize::try_from(self.player_names.write_display(seat, &mut buf))
                .unwrap_or(0)
                .min(buf.len());
            let name = String::from_utf8_lossy(&buf[..n]).into_owned();
            let [kills, items, secrets] = self.player_stats[seat];
            rows.push(ScoreRow {
                seat,
                name,
                frags,
                deaths,
                kills,
                items,
                secrets,
            });
        }
        if self.deathmatch {
            rows.sort_by(|a, b| b.frags.cmp(&a.frags).then(a.seat.cmp(&b.seat)));
        }
        rows
    }

    /// Seat whose player body is `actor`.
    #[must_use]
    pub fn player_seat_of(&self, actor: ActorId) -> Option<u8> {
        self.players
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.mo == Some(actor)))
            .and_then(|i| u8::try_from(i).ok())
    }

    /// Take ACS `StartSlideshow` request (shell shows / plays later).
    pub fn take_slideshow_request(&mut self) -> Option<String> {
        self.slideshow_request.take()
    }

    /// Take ACS `SetSubtitleNumber` request `(log, sound)`.
    pub fn take_subtitle_request(&mut self) -> Option<(i32, String)> {
        self.subtitle_request.take()
    }

    /// `Inventory.Icon` of `class`, else its spawn frame (UZDoom
    /// `GetInventoryIcon` fallback).
    #[must_use]
    pub fn inventory_icon_lump(&self, class: ClassId) -> String {
        let Some(c) = self.classes.get(class) else {
            return String::new();
        };
        if !c.inventory_icon.is_empty() {
            return c.inventory_icon.clone();
        }
        self.states
            .find_label(class, "Spawn")
            .and_then(|sid| self.states.get(sid))
            .and_then(|st| weapon_patch_from_sprite_frame(&st.sprite, st.frame))
            .unwrap_or_default()
    }

    /// What a ZScript status bar reads about player `p`: carried items as
    /// `(class, amount, max)` (armor as `BasicArmor`), each item's
    /// `Inventory.Icon`, the ready weapon and its ammo classes.
    #[must_use]
    pub fn status_bar_items(&self, p: usize) -> Option<StatusBarItems> {
        let player = self.players.get(p).and_then(|s| s.as_ref())?;
        let mo = player.mo?;
        let mut out = StatusBarItems::default();
        let icon_of = |out: &mut StatusBarItems, name: &str, icon: &str| {
            if !icon.is_empty() && !out.icons.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
                out.icons.push((name.to_owned(), icon.to_owned()));
            }
        };
        // Oldest first, so the script's `Inv` chain runs in bar order.
        let items: Vec<_> = self.inventory.iter(mo).cloned().collect();
        for item in items.iter().rev() {
            let Some(c) = self.classes.get(item.class) else {
                continue;
            };
            out.inventory
                .push((c.name.clone(), item.amount, item.max_amount));
            icon_of(&mut out, &c.name, &self.inventory_icon_lump(item.class));
        }
        if player.armor > 0 {
            out.inventory
                .push(("BasicArmor".to_owned(), player.armor, 200));
            let icon = player
                .armor_type
                .and_then(|id| self.classes.get(id))
                .map(|c| c.inventory_icon.clone())
                .filter(|i| !i.is_empty())
                .unwrap_or_else(|| {
                    if player.armor_class >= 2 {
                        "ARM2A0"
                    } else {
                        "ARM1A0"
                    }
                    .to_owned()
                });
            icon_of(&mut out, "BasicArmor", &icon);
        }
        if let Some(w) = player.ready_weapon.and_then(|id| self.classes.get(id)) {
            out.ready_weapon = Some(w.name.clone());
            out.ammo1 = (!w.ammo_type.is_empty()).then(|| w.ammo_type.clone());
            out.ammo2 = (!w.ammo_type2.is_empty()).then(|| w.ammo_type2.clone());
        }
        let name = |id: ClassId| self.classes.get(id).map(|c| c.name.clone());
        out.inv_sel = player.inv_sel.and_then(name);
        out.inv_bar = crate::invbar::invbar_items(self, mo)
            .into_iter()
            .filter_map(name)
            .collect();
        out.inventory_tics = player.inventory_tics;
        // Slot weapons with their ammo, icons and defaults (`hud_showammo 2`
        // lists ammo for weapons not yet found).
        for slot in 0u8..=9 {
            for wname in self.weapon_slots.weapons_for(slot) {
                let Some(cid) = self.classes.find_name(&wname) else {
                    continue;
                };
                let Some(w) = self.classes.get(cid) else {
                    continue;
                };
                let a1 = (!w.ammo_type.is_empty()).then(|| w.ammo_type.clone());
                let a2 = (!w.ammo_type2.is_empty()).then(|| w.ammo_type2.clone());
                icon_of(&mut out, &w.name, &self.inventory_icon_lump(cid));
                for ammo in a1.iter().chain(a2.iter()) {
                    if let Some(acid) = self.classes.find_name(ammo) {
                        icon_of(&mut out, ammo, &self.inventory_icon_lump(acid));
                        if !out
                            .max_amounts
                            .iter()
                            .any(|(n, _)| n.eq_ignore_ascii_case(ammo))
                        {
                            let max = self.classes.get(acid).map_or(0, |c| c.inventory_max_amount);
                            out.max_amounts.push((ammo.clone(), max));
                        }
                    }
                }
                out.weapons.push((w.name.clone(), slot, a1, a2));
            }
        }
        for item in &items {
            let Some(c) = self.classes.get(item.class) else {
                continue;
            };
            if self.classes.inherits_from(item.class, "Key") {
                if let Some(lump) = self
                    .states
                    .find_label(item.class, "Spawn")
                    .and_then(|sid| self.states.get(sid))
                    .and_then(|st| weapon_patch_from_sprite_frame(&st.sprite, st.frame))
                {
                    out.spawn_sprites.push((c.name.clone(), lump));
                }
            }
        }
        if !player.powers.is_empty()
            || player.invul_tics > 0
            || player.blur_tics > 0
            || player.radsuit_tics > 0
            || player.infrared_tics > 0
        {
            for c in self.classes.iter() {
                if c.powerup_type.is_empty() {
                    continue;
                }
                if let Some((tics, max)) = crate::powerups::effect_tics_for_item(self, mo, c.id) {
                    out.power_tics.push((c.name.clone(), tics, max));
                }
            }
        }
        for slot in 0u8..=9 {
            out.slots[usize::from(slot)] = self.weapon_slots.weapons_for(slot).iter().any(|name| {
                self.classes
                    .find_name(name)
                    .is_some_and(|cid| self.inventory.count(mo, cid) > 0)
            });
        }
        Some(out)
    }

    /// Snapshot numbers for the Doom status bar / HUD (player slot `p`).
    #[must_use]
    pub fn player_hud(&self, p: usize) -> Option<PlayerHud> {
        let player = self.players.get(p).and_then(|s| s.as_ref())?;
        let mo = player.mo?;
        let actor = self.actors.get(mo)?;
        const AMMO_TYPES: [&str; 4] = ["Clip", "Shell", "RocketAmmo", "Cell"];
        let ammo_of = |cid: ClassId| -> [i32; 2] {
            let max = self
                .inventory
                .find(mo, cid)
                .map(|item| item.max_amount)
                .or_else(|| self.classes.get(cid).map(|c| c.inventory_max_amount))
                .unwrap_or(0);
            [self.inventory.count(mo, cid), max]
        };
        // Vanilla blanks the big ammo number for `am_noammo` weapons (fist, saw).
        let ammo_cid = match player.ready_weapon.and_then(|w| self.classes.get(w)) {
            Some(w) if w.ammo_type.is_empty() => None,
            Some(w) => self.classes.find_name(&w.ammo_type),
            None => AMMO_TYPES.iter().find_map(|name| {
                let cid = self.classes.find_name(name)?;
                (self.inventory.count(mo, cid) > 0).then_some(cid)
            }),
        };
        let [ammo, ammo_max] = ammo_cid.map_or([-1, 0], ammo_of);
        let ammo2_cid = player
            .ready_weapon
            .and_then(|w| self.classes.get(w))
            .filter(|w| !w.ammo_type2.is_empty())
            .and_then(|w| self.classes.find_name(&w.ammo_type2));
        let [ammo2, ammo2_max] = ammo2_cid.map_or([-1, 0], ammo_of);
        let ammo_table =
            AMMO_TYPES.map(|name| self.classes.find_name(name).map_or([0, 0], ammo_of));
        let mut arms = 0u16;
        for slot in 2u8..=9 {
            if self.weapon_slots.weapons_for(slot).iter().any(|name| {
                self.classes
                    .find_name(name)
                    .is_some_and(|cid| self.inventory.count(mo, cid) > 0)
            }) {
                arms |= 1 << slot;
            }
        }
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
        let face = player.mugshot.lump();
        // Keep drawing while the gun lowers on death (docs/30 W13); `pspr_y`
        // already offsets it off-screen at `WEAPONBOTTOM`.
        // `####` stays on the sprite recorded while the zero-tic chain ran.
        let (weapon_tint_r, weapon_tint_g, weapon_tint_b) =
            weapon_translation_tint(&player.weapon_translation);
        let (overlay_tint_r, overlay_tint_g, overlay_tint_b) =
            weapon_translation_tint(&player.overlay2_translation);
        let weapon_class = player
            .ready_weapon
            .and_then(|id| self.classes.get(id))
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let weapon_patch = if player.pspr_state.is_some() && !player.pspr_sprite.is_empty() {
            weapon_patch_from_sprite_frame(&player.pspr_sprite, player.pspr_frame)
                .unwrap_or_default()
        } else {
            player
                .pspr_state
                .and_then(|sid| self.states.get(sid))
                .and_then(|st| weapon_patch_from_sprite_frame(&st.sprite, st.frame))
                .or_else(|| {
                    player
                        .ready_weapon
                        .filter(|_| player.player_state == crate::player::PlayerState::Live)
                        .and_then(|w| self.classes.get(w))
                        .map(|c| weapon_patch_for_class(&c.name).to_string())
                })
                .unwrap_or_default()
        };
        let flash_st = player
            .pspr_flash_state
            .filter(|_| player.player_state == crate::player::PlayerState::Live)
            .and_then(|sid| self.states.get(sid));
        let flash_patch = flash_st
            .and_then(|st| weapon_patch_from_sprite_frame(&st.sprite, st.frame))
            .unwrap_or_default();
        let flash_bright = flash_st.is_some_and(|st| st.bright);
        let weapon_bright = player
            .pspr_state
            .and_then(|sid| self.states.get(sid))
            .is_some_and(|st| st.bright);
        // Vanilla `R_DrawPSprite`: scalelight[(lightlevel>>4)+extralight][MAXLIGHTSCALE-1]
        // (docs/30 N8). Infrared fixedcolormap → fullbright weapon.
        let sector_light = crate::physics::actor_light_level(self, actor).clamp(0, 255);
        let weapon_light = if player.fixed_colormap() == 1 {
            1.0
        } else {
            psprite_scalelight(sector_light, player.extralight)
        };
        let style_fuzz = matches!(
            player.weapon_style,
            Some(crate::class::RenderStyle::Fuzzy | crate::class::RenderStyle::OptFuzzy)
        );
        let layer2_style_fuzz = matches!(
            player.overlay2_style,
            Some(crate::class::RenderStyle::Fuzzy | crate::class::RenderStyle::OptFuzzy)
        );
        let layer2_fuzz = if player.overlay2_force_style {
            layer2_style_fuzz
        } else {
            player.blur || layer2_style_fuzz
        };
        let weapon_fuzz = if player.weapon_force_style {
            style_fuzz
        } else {
            player.blur || style_fuzz
        };
        // Sway frozen outside `A_WeaponReady` (docs/30 W8.3).
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_possible_truncation)]
        let sway_x = if player.weapon_add_bob {
            player.pspr_bob_x
        } else {
            0.0
        };
        let sway_y = if player.weapon_add_bob {
            player.pspr_bob_y
        } else {
            0.0
        };
        let ofs_x = if player.weapon_add_ofs {
            player.weapon_ofs_x
        } else {
            0.0
        };
        let ofs_y = if player.weapon_add_ofs {
            player.weapon_ofs_y
        } else {
            0.0
        };
        let bob_x = (sway_x + ofs_x) as f32;
        #[allow(clippy::cast_possible_truncation)]
        // `Weapon.YAdjust`: moves the gun down (positive) with the status bar.
        let y_adjust = player
            .ready_weapon
            .and_then(|w| self.classes.get(w))
            .map_or(0.0, |c| c.ext.weapon_y_adjust);
        #[allow(clippy::cast_possible_truncation)]
        let bob_y =
            (sway_y + (player.pspr_y - crate::player::WEAPON_TOP) + ofs_y + y_adjust) as f32;
        let class_weapon_scale = player
            .ready_weapon
            .and_then(|w| self.classes.get(w))
            .map_or((1.0, 1.0), |c| {
                (c.ext.weapon_scale.0, c.ext.weapon_scale.1 / 1.2)
            });
        let player_class = self
            .classes
            .get(actor.class)
            .map(|c| {
                if c.ext.display_name.is_empty() {
                    c.name.clone()
                } else {
                    c.ext.display_name.clone()
                }
            })
            .unwrap_or_default();
        let armor_type = player
            .armor_type
            .and_then(|c| self.classes.get(c))
            .map(|c| c.name.clone())
            .or_else(|| match (player.armor > 0, player.armor_class) {
                (true, 1) => Some("GreenArmor".to_owned()),
                (true, 2) => Some("BlueArmor".to_owned()),
                _ => None,
            })
            .unwrap_or_default();
        // Hexen classes show their armor class (`Σ HexenArmor.Slots / 5`).
        let armor = if player.armor == 0
            && let Some(mo) = player.mo
            && crate::pickup::hexen_armor_profile(self, mo).is_some()
        {
            crate::pickup::hexen_armor_class(self, mo)
        } else {
            player.armor
        };
        let body_alpha = if (0.0..=1.0).contains(&actor.alpha) {
            actor.alpha
        } else {
            1.0
        };
        let layers = player
            .psp_layers
            .iter()
            .filter(|_| player.player_state == crate::player::PlayerState::Live)
            .filter_map(|(&id, l)| {
                use crate::player::pspf;
                l.state?;
                let patch = weapon_patch_from_sprite_frame(&l.sprite, l.frame)?;
                let own_fuzz = matches!(
                    l.style,
                    Some(crate::class::RenderStyle::Fuzzy | crate::class::RenderStyle::OptFuzzy)
                );
                // UZDoom `DPSprite::GetRenderStyle`.
                let fuzz = if l.flags & pspf::RENDERSTYLE != 0 {
                    if l.flags & pspf::FORCESTYLE != 0 {
                        own_fuzz
                    } else {
                        player.blur || own_fuzz
                    }
                } else {
                    player.blur
                };
                let alpha = if l.flags & pspf::FORCEALPHA != 0 {
                    l.alpha
                } else if l.flags & pspf::ALPHA != 0 {
                    body_alpha * l.alpha
                } else {
                    body_alpha
                };
                let add_weapon = l.flags & pspf::ADDWEAPON != 0;
                let bright = l
                    .state
                    .and_then(|sid| self.states.get(sid))
                    .is_some_and(|st| st.bright);
                #[allow(clippy::cast_possible_truncation)]
                Some(HudPspLayer {
                    id,
                    patch,
                    ofs_x: l.x as f32,
                    ofs_y: if add_weapon {
                        l.y as f32
                    } else {
                        (l.y - crate::player::WEAPON_TOP) as f32
                    },
                    alpha: alpha as f32,
                    scale_x: l.scale.0 as f32,
                    scale_y: l.scale.1 as f32,
                    flip: l.flags & pspf::FLIP != 0,
                    mirror: l.flags & pspf::MIRROR != 0,
                    angle: l.angle as f32,
                    pivot_x: l.pivot.0 as f32,
                    pivot_y: l.pivot.1 as f32,
                    pivot_percent: l.flags & pspf::PIVOTPERCENT != 0,
                    add_bob: l.flags & pspf::ADDBOB != 0,
                    add_weapon,
                    interpolate: l.flags & pspf::INTERPOLATE != 0,
                    fuzz,
                    bright,
                    tint: if l.flags & pspf::PLAYERTRANSLATED != 0 {
                        (1.0, 1.0, 1.0)
                    } else {
                        weapon_translation_tint(&l.translation)
                    },
                    vertex: l.vertex.map(|(x, y)| (x as f32, y as f32)),
                })
            })
            .collect();
        Some(PlayerHud {
            layers,
            targeter: player
                .mo
                .and_then(|mo| crate::powerups::targeter_hud(self, mo)),
            player_class,
            armor_type,
            health: actor.health,
            armor,
            ammo,
            ammo_max,
            ammo2,
            ammo2_max,
            keys,
            face,
            weapon_patch,
            flash_patch,
            overlay_patch: weapon_patch_from_sprite_frame(
                &player.overlay2_sprite,
                player.overlay2_frame,
            )
            .unwrap_or_default(),
            overlay_ofs_x: player.overlay2_ofs_x as f32,
            overlay_ofs_y: player.overlay2_ofs_y as f32,
            overlay_alpha: {
                let overlay = player.overlay2_alpha.unwrap_or(1.0);
                let body = if (0.0..=1.0).contains(&actor.alpha) {
                    actor.alpha
                } else {
                    1.0
                };
                let alpha = if player.overlay2_force_alpha {
                    overlay
                } else {
                    overlay * body
                };
                alpha as f32
            },
            overlay_scale_x: player.overlay2_scale.map(|(x, _)| x as f32).unwrap_or(1.0),
            overlay_scale_y: player.overlay2_scale.map(|(_, y)| y as f32).unwrap_or(1.0),
            overlay_flip: player.overlay2_flip,
            overlay_mirror: player.overlay2_mirror,
            overlay_angle: player.overlay2_angle as f32,
            overlay_pivot_x: player.overlay2_pivot.map(|(x, _)| x as f32).unwrap_or(0.5),
            overlay_pivot_y: player.overlay2_pivot.map(|(_, y)| y as f32).unwrap_or(0.5),
            overlay_tint_r,
            overlay_tint_g,
            overlay_tint_b,
            overlay_fuzz: layer2_fuzz,
            overlay_pivot_percent: player.overlay2_pivot_percent,
            overlay_add_bob: player.overlay2_add_bob,
            overlay_add_weapon: player.overlay2_add_weapon,
            overlay_interpolate: player.overlay2_interpolate,
            weapon_light,
            weapon_bright,
            flash_bright,
            weapon_fuzz,
            weapon_alpha: {
                let overlay = player.weapon_alpha.unwrap_or(1.0);
                let body = if (0.0..=1.0).contains(&actor.alpha) {
                    actor.alpha
                } else {
                    1.0
                };
                let alpha = if player.weapon_force_alpha {
                    overlay
                } else {
                    overlay * body
                };
                alpha as f32
            },
            // `Weapon.WeaponScaleX/Y` (relative to GZDoom's 1.0 × 1.2 default)
            // times `A_OverlayScale` on the gun layer.
            weapon_scale_x: (player.weapon_scale.map_or(1.0, |(x, _)| x) * class_weapon_scale.0)
                as f32,
            weapon_scale_y: (player.weapon_scale.map_or(1.0, |(_, y)| y) * class_weapon_scale.1)
                as f32,
            weapon_flip: player.weapon_flip,
            weapon_mirror: player.weapon_mirror,
            weapon_pivot_percent: player.weapon_pivot_percent,
            weapon_interpolate: player.weapon_interpolate,
            weapon_pivot_x: player.weapon_pivot.map(|(x, _)| x as f32).unwrap_or(0.5),
            weapon_pivot_y: player.weapon_pivot.map(|(_, y)| y as f32).unwrap_or(0.5),
            weapon_angle: player.weapon_angle as f32,
            weapon_vertex: player.weapon_vertex.map(|(x, y)| (x as f32, y as f32)),
            inv_bar: crate::invbar::invbar_items(self, mo)
                .into_iter()
                .map(|c| (self.inventory_icon_lump(c), self.inventory.count(mo, c)))
                .collect(),
            inv_sel: player.inv_sel.and_then(|sel| {
                crate::invbar::invbar_items(self, mo)
                    .iter()
                    .position(|&c| c == sel)
            }),
            inv_bar_open: player.inventory_tics > 0,
            overlay_vertex: player.overlay2_vertex.map(|(x, y)| (x as f32, y as f32)),
            weapon_class,
            crosshair: player.crosshair,
            weapon_tint_r,
            weapon_tint_g,
            weapon_tint_b,
            bob_ofs_x: ofs_x as f32,
            bob_ofs_y: ofs_y as f32,
            bob_x,
            bob_y,
            god: player.god,
            arms,
            ammo_table,
            palette: crate::mugshot::palette_for(
                player,
                crate::physics::actor_waterlevel(self, actor),
            ),
            colormap: player.fixed_colormap(),
            deathmatch: self.deathmatch,
            frags: player
                .frags
                .iter()
                .enumerate()
                .map(|(i, &n)| if i == p { -n } else { n })
                .sum(),
            face_back: (self.deathmatch || self.players.iter().flatten().count() > 1)
                .then_some((p % 4) as u8),
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
        self.push_message_kind(
            MessageKind::System,
            if on {
                "Degreelessness Mode On"
            } else {
                "Degreelessness Mode Off"
            },
        );
        on
    }

    /// Toggle `noclip` on player slot `p`. Returns new state.
    pub fn toggle_noclip(&mut self, p: usize) -> bool {
        let Some(player) = self.players.get_mut(p).and_then(|s| s.as_mut()) else {
            return false;
        };
        player.noclip = !player.noclip;
        let on = player.noclip;
        self.push_message_kind(
            MessageKind::System,
            if on {
                "No clipping mode ON"
            } else {
                "No clipping mode OFF"
            },
        );
        on
    }

    /// Toggle level `frozen` (console `freeze`). Returns new state.
    pub fn toggle_freeze(&mut self) -> bool {
        self.level.frozen = !self.level.frozen;
        let on = self.level.frozen;
        self.push_message_kind(
            MessageKind::System,
            if on {
                "Freeze mode ON"
            } else {
                "Freeze mode OFF"
            },
        );
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
            .or_else(|| room_nodebuild::point_in_sector_vanilla(&self.map, a.pos.x(), a.pos.y()))
        else {
            return;
        };
        let mut touched = Vec::new();
        for (i, line) in self.map.lines.iter().enumerate() {
            if (line.front_sector == Some(sid) || line.back_sector == Some(sid))
                && (i32::from(line.flags) & ML_MAPPED) == 0
            {
                touched.push(i as u32);
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

    /// Set a mod CVar (CVARINFO name) for ACS and ZScript alike.
    pub fn set_mod_cvar(&mut self, name: &str, value: &str) {
        self.mod_cvars
            .insert(name.to_ascii_lowercase(), value.trim().to_owned());
        self.zscript.set_cvar_text(name, value);
    }

    /// Hand every mod CVar to the ZScript VM (after a program is loaded).
    pub fn sync_mod_cvars_to_zscript(&mut self) {
        let all: Vec<(String, String)> = self
            .mod_cvars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, value) in all {
            self.zscript.set_cvar_text(&name, &value);
        }
    }

    /// Integer value of a mod CVar (`GetCVar` rules: bools 0/1, floats truncated).
    #[must_use]
    pub fn mod_cvar_int(&self, name: &str) -> Option<i32> {
        let v = self.mod_cvars.get(&name.to_ascii_lowercase())?;
        let t = v.trim();
        if let Ok(i) = t.parse::<i32>() {
            return Some(i);
        }
        if let Ok(f) = t.parse::<f64>() {
            #[allow(clippy::cast_possible_truncation)]
            return Some(f as i32);
        }
        Some(i32::from(t.eq_ignore_ascii_case("true")))
    }

    /// Toggle Computer Area Map (`iddt` bootstrap — full automap reveal).
    pub fn toggle_iddt(&mut self, p: usize) -> bool {
        let Some(player) = self.players.get_mut(p).and_then(|s| s.as_mut()) else {
            return false;
        };
        // Doom `cheating = (cheating + 1) % 3`: map, map + things, off.
        let msg = if !player.allmap {
            player.allmap = true;
            "Computer Area Map ON"
        } else if !player.automap_things {
            player.automap_things = true;
            "Automap things ON"
        } else {
            player.allmap = false;
            player.automap_things = false;
            "Computer Area Map OFF"
        };
        let on = player.allmap;
        self.push_message_kind(MessageKind::System, msg);
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
        self.push_message_kind(MessageKind::System, msg);
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

    /// DukeMatch: spawn deathmatch-only things, drop monsters, sit on a DM start.
    pub fn apply_duke_match(&mut self) {
        self.deathmatch = true;
        self.weapon_stay = true;
        let _ = self.spawn_multiplayer_only_things();
        let duke = self.players.iter().flatten().any(|p| {
            self.classes
                .get(p.cls)
                .is_some_and(|c| c.name == "DukePlayer")
        });
        if duke {
            let player_mos: Vec<_> = self.players.iter().flatten().filter_map(|p| p.mo).collect();
            let kills: Vec<_> = self
                .thinkers
                .default
                .iter()
                .copied()
                .filter(|&id| {
                    !player_mos.contains(&id)
                        && self.actors.get(id).is_some_and(|a| {
                            !a.destroyed && (a.flags & crate::class::FLAG_COUNTKILL) != 0
                        })
                })
                .collect();
            for id in kills {
                self.fire_zscript_thing_destroyed(id);
                if let Some(a) = self.actors.get_mut(id) {
                    a.destroyed = true;
                    a.health = 0;
                }
            }
        }
        let starts: Vec<_> = self
            .map
            .things
            .iter()
            .filter(|t| t.type_id == 11)
            .cloned()
            .collect();
        if starts.is_empty() {
            return;
        }
        let seats: Vec<(usize, ActorId)> = self
            .players
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.as_ref().and_then(|p| p.mo).map(|mo| (i, mo)))
            .collect();
        for (i, mo) in seats {
            let t = &starts[i % starts.len()];
            let z = self.map_thing_z(
                self.actors
                    .get(mo)
                    .map(|a| a.class)
                    .unwrap_or_else(|| self.player_class()),
                t,
            );
            if let Some(a) = self.actors.get_mut(mo) {
                a.pos = DVec3::new(f64::from(t.x), f64::from(t.y), z);
                a.angle = doom_thing_angle(t.angle);
            }
        }
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

    /// Decide the match once (later calls are ignored). Shells show the final
    /// screen; the sim keeps running underneath.
    pub fn end_match(&mut self, winner: Option<u8>, time_up: bool) {
        if self.match_over.is_some() {
            return;
        }
        let winner = winner.filter(|&w| {
            self.players
                .get(usize::from(w))
                .is_some_and(Option::is_some)
        });
        self.match_over = Some(MatchOver {
            winner,
            time_up,
            maptime: self.level.maptime,
        });
    }

    /// Frags of player `slot`: kills of other players minus suicides.
    #[must_use]
    pub fn player_frags(&self, slot: usize) -> i32 {
        let Some(p) = self.players.get(slot).and_then(Option::as_ref) else {
            return 0;
        };
        (0..MAX_PLAYERS)
            .map(|v| if v == slot { -p.frags[v] } else { p.frags[v] })
            .sum()
    }

    /// Players ranked for the final screen: the winner, then whoever is still
    /// alive, then the dead by how long they lasted; frags break ties.
    #[must_use]
    pub fn standings(&self) -> Vec<Standing> {
        let winner = self.match_over.and_then(|m| m.winner);
        let mut rows: Vec<(Standing, u32)> = Vec::new();
        for (slot, p) in self.players.iter().enumerate() {
            let Some(p) = p else {
                continue;
            };
            let frags = self.player_frags(slot);
            let deaths = self.players.iter().flatten().map(|o| o.frags[slot]).sum();
            let alive = p.player_state != crate::player::PlayerState::Dead
                && p.mo
                    .and_then(|mo| self.actors.get(mo))
                    .is_some_and(|a| a.health > 0);
            let lasted = if alive {
                u32::MAX
            } else {
                self.player_died_at[slot].unwrap_or(0)
            };
            rows.push((
                Standing {
                    slot: slot as u8,
                    place: 0,
                    frags,
                    deaths,
                    alive,
                },
                lasted,
            ));
        }
        rows.sort_by(|(a, la), (b, lb)| {
            (Some(b.slot) == winner)
                .cmp(&(Some(a.slot) == winner))
                .then(lb.cmp(la))
                .then(b.frags.cmp(&a.frags))
                .then(a.deaths.cmp(&b.deaths))
                .then(a.slot.cmp(&b.slot))
        });
        rows.into_iter()
            .enumerate()
            .map(|(i, (mut s, _))| {
                s.place = i as u8 + 1;
                s
            })
            .collect()
    }

    /// One game tic (`P_Ticker` subset). See `docs/07-playsim.md §2.2`.
    pub fn tick(&mut self) {
        // Console / mod pause: freeze playsim clocks and thinkers, but still
        // feed `raw_cmd` + script `WorldTick` so Mod HUD menus (draft, etc.)
        // can resolve. `maptime` does not advance.
        if self.paused {
            self.fire_zscript_world_tick();
            self.scripts.sync_from_world(self);
            self.sound_heard = self.sound_events.iter().map(|e| e.name.clone()).collect();
            self.sound_events.clear();
            let now = self.level.maptime;
            self.mod_hud.tick_tweens(now);
            for i in 0..MAX_PLAYERS {
                if let Some(p) = self.players[i].as_mut() {
                    p.raw_cmd = p.cmd;
                }
            }
            let _ = self.scripts.dispatch_world_tick();
            for i in 0..MAX_PLAYERS {
                if let Some(p) = self.players[i].as_mut() {
                    p.prev_buttons = p.raw_cmd.buttons.0;
                }
            }
            return;
        }
        if self.match_tics_left > 0 {
            self.match_tics_left -= 1;
            if self.match_tics_left == 0 {
                self.end_match(None, true);
            }
        }
        {
            let actors = &self.actors;
            self.live_ids.clear();
            self.live_ids.extend(
                self.thinkers
                    .default
                    .iter()
                    .copied()
                    .filter(|&i| actors.get(i).is_some_and(|a| !a.destroyed)),
            );
        }
        self.sync_zscript_census();
        self.scripts.sync_from_world(self);
        self.sound_heard = self.sound_events.iter().map(|e| e.name.clone()).collect();
        self.sound_events.clear();
        self.portal_moves.clear();
        // Kept until drained: text pushed between tics (map start, mods) must
        // reach the shell. Bounded for hosts that never drain.
        if self.messages.len() > 64 {
            let excess = self.messages.len() - 32;
            self.messages.drain(..excess);
        }
        self.screen_fade.tick();
        self.tick_hud_messages();
        let now = self.level.maptime;
        self.mod_hud.tick_tweens(now);
        {
            let actors = &self.actors;
            self.mod_hud.sync_marker_actors(|bits| {
                if bits == 0 {
                    return None;
                }
                let id = crate::actor::ActorId::from_raw(bits as u32, (bits >> 32) as u32);
                let a = actors.get(id).filter(|a| !a.destroyed)?;
                Some([a.pos.x() as i32, a.pos.y() as i32, a.pos.z() as i32])
            });
        }
        tick_contact_damage(self);
        tick_item_respawns(self);
        crate::damage::tick_duke_effects(self);
        // Snapshot plane Z for render interpolation (`interpolations.begin_tic`).
        for sec in &mut self.sectors {
            sec.prev_floor_z = sec.floor_z;
            sec.prev_ceiling_z = sec.ceiling_z;
        }
        crate::bot::run_bot_brains(self);
        for i in 0..MAX_PLAYERS {
            if self.players[i].is_some() {
                let mut p = self.players[i].take().unwrap_or_default();
                self.taken_player = p.mo.map(|mo| (i, mo));
                self.thinking_up_move = p.cmd.up_move;
                think_player::player_think_for_seat(self, &mut p, i as i32);
                self.taken_player = None;
                self.thinking_up_move = 0;
                self.thinking_powers = None;
                self.players[i] = Some(p);
            }
        }
        // events.world_tick (WASM EventHandlers) + flush WASM→ACS starts
        self.fire_zscript_world_tick();
        let _ = self.scripts.dispatch_world_tick();
        // A mod may set `rule::PAUSED` during WorldTick (e.g. Survival draft).
        // Skip the rest of this tic so in-flight missiles cannot still kill.
        if self.paused {
            for i in 0..MAX_PLAYERS {
                if let Some(p) = self.players[i].as_mut() {
                    p.prev_buttons = p.raw_cmd.buttons.0;
                }
            }
            return;
        }
        for (script, map, args) in self.scripts.drain_acs_starts() {
            let mut a = [0i32; 5];
            a[0] = script;
            a[1] = map;
            for (i, v) in args.iter().take(3).enumerate() {
                a[2 + i] = *v;
            }
            let _ = crate::acs::acs_execute(self, &a, false, None, None);
        }
        if !self.level.is_frozen() {
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
            // Destroyed actors stay on the list (checksums hash them) but no
            // longer think or move. Running physics on them cloned every
            // corpse-less effect Brutal Doom ever spawned, every tic.
            if self.actors.get(id).is_none_or(|a| a.destroyed) {
                continue;
            }
            if self.actors.get(id).is_some_and(|a| {
                (a.flags & crate::class::FLAG_DORMANT) != 0 || crate::damage::is_frozen(self, id)
            }) || crate::powerups::actor_time_frozen(self, id)
            {
                continue;
            }
            self.check_no_delay(id);
            // Player pawns already applied cmd velocity this tic; still run XY for monsters.
            crate::physics::xy_movement(self, id);
            crate::physics::z_movement(self, id);
            // Doom touches specials inside `P_TryMove`; approximate post-move.
            self.touch_specials_for_pawn(id);
            self.advance_actor_state(id);
            self.tick_zscript_actor(id);
        }
        if !self.level.is_frozen() {
            specials::tick_strobes(self);
            specials::tick_flickers(self);
            specials::tick_glows(self);
        }
        // Missiles already in-flight + newly spawned this tic (list may grow).
        while ti < self.thinkers.default.len() {
            let id = self.thinkers.default[ti];
            ti += 1;
            // Destroyed actors stay on the list (checksums hash them) but no
            // longer think or move. Running physics on them cloned every
            // corpse-less effect Brutal Doom ever spawned, every tic.
            if self.actors.get(id).is_none_or(|a| a.destroyed) {
                continue;
            }
            if self.actors.get(id).is_some_and(|a| {
                (a.flags & crate::class::FLAG_DORMANT) != 0 || crate::damage::is_frozen(self, id)
            }) || crate::powerups::actor_time_frozen(self, id)
            {
                continue;
            }
            self.check_no_delay(id);
            crate::physics::xy_movement(self, id);
            crate::physics::z_movement(self, id);
            // Player pawns spawned after the map's things (bots, late
            // joiners) pick up items like the others.
            self.touch_specials_for_pawn(id);
            self.advance_actor_state(id);
            self.tick_zscript_actor(id);
        }
        if !self.zero_tic_spawned.is_empty() {
            let actors = &self.actors;
            self.zero_tic_spawned
                .retain(|&id| actors.get(id).is_some_and(|a| !a.destroyed));
        }
        tick_spectator_cameras(self);
        tick_stealth(self);
        crate::damage::tick_poison(self);
        if !self.level.is_frozen() {
            specials::tick_doors(self);
            specials::tick_floors(self);
            specials::tick_plats(self);
            specials::tick_ceilings(self);
            specials::tick_waggles(self);
            specials::tick_elevators(self);
            specials::tick_subways(self);
            specials::tick_stretches(self);
            specials::tick_teeth(self);
            specials::tick_skews(self);
            specials::tick_demo_cameras(self);
            crate::polyobj::tick_polyobjs(self);
            crate::acs::tick_acs(self);
            // ACS `GetPlayerInput(-1, INPUT_OLDBUTTONS)` must see the previous
            // tic. `player_think` already copied this tic into `prev_buttons`.
            for p in self.players.iter_mut().flatten() {
                p.acs_prev_buttons = p.cmd.buttons.0;
            }
            crate::sndseq::tick_sequences(self);
            specials::tick_fades(self);
            specials::tick_scrollers(self);
            specials::tick_spinners(self);
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
        crate::mugshot::tick_players(self);
        self.level.tick();
        // gc_destroyed_actors — later
    }

    /// If `id` is a player's pawn, touch the pickups it overlaps.
    fn touch_specials_for_pawn(&mut self, id: ActorId) {
        let Some(pi) = self
            .players
            .iter()
            .enumerate()
            .find_map(|(i, p)| p.as_ref().and_then(|pl| (pl.mo == Some(id)).then_some(i)))
        else {
            return;
        };
        let mut p = self.players[pi].take().unwrap_or_default();
        self.taken_player = Some((pi, id));
        crate::pickup::touch_specials(self, &mut p, id);
        self.taken_player = None;
        self.players[pi] = Some(p);
    }

    /// GZDoom `ConstructActor` / vanilla `P_SpawnMobj`: take the spawn state
    /// without calling its action ("action routines can't be called yet").
    /// A 0-tic spawn state moves on in the actor's first tick, and a
    /// `NoDelay` one has its action called then (`AActor::CheckNoDelay`).
    fn enter_spawn_state(&mut self, id: ActorId, sid: room_core::StateId) {
        let Some(st) = self.states.get(sid) else {
            return;
        };
        let (tics, fast, nodelay) = (i32::from(st.tics), st.fast, st.nodelay);
        let (sprite, frame) = (st.sprite.clone(), st.frame);
        let fast_monsters = self.fast_monsters;
        let Some(a) = self.actors.get_mut(id) else {
            return;
        };
        a.state = Some(sid);
        let always_fast = a.extra_flags.get_name("ALWAYSFAST");
        a.tics = if (fast_monsters || always_fast) && fast && tics > 0 {
            tics - (tics >> 1)
        } else {
            tics
        };
        if !sprite.is_empty() && !crate::state::sprite_unchanged(&sprite) {
            a.sprite = sprite;
        }
        if frame != crate::state::FRAME_NOCHANGE {
            a.frame = frame;
        }
        note_stealth_state(self, id, Some(sid));
        if nodelay {
            self.no_delay_pending.insert(id);
        }
    }

    /// GZDoom `AActor::CheckNoDelay`, at the start of an actor's tick: the
    /// first tick after spawning in a `NoDelay` state runs that state's
    /// action (and any jump it makes).
    fn check_no_delay(&mut self, id: ActorId) {
        if self.no_delay_pending.is_empty() || !self.no_delay_pending.remove(&id) {
            return;
        }
        let Some(sid) = self.actors.get(id).and_then(|a| a.state) else {
            return;
        };
        let tics = self.actors.get(id).map(|a| a.tics);
        self.set_state(id, Some(sid), true);
        // The action ran, but the state's own timer keeps counting from
        // where it was (only a jump starts a new one).
        if let (Some(tics), Some(a)) = (tics, self.actors.get_mut(id))
            && a.state == Some(sid)
        {
            a.tics = tics;
        }
    }

    /// Enter a state, optionally running its action (`AActor::SetState`).
    ///
    /// Zero-tic chains and action jumps are iterative with a GZDoom-style
    /// 1000-state guard (BD3) so accidental loops cannot overflow the stack.
    pub fn set_state(&mut self, id: ActorId, state: Option<room_core::StateId>, call_action: bool) {
        // Actor state actions never run in the weapon (psprite) context. A
        // gun's A_FirePistol spawns a puff, the puff's 0-tic chain spawns a
        // flare, and the flare's `A_Jump(128, 2)` must offset the flare's own
        // state — not the psprite's — or the flare walks the weapon's states
        // and shows up as a ghost gun billboard (BD "phantom gun frames").
        let saved = (self.psprite_weapon.take(), self.psprite_state.take());
        note_stealth_state(self, id, state);
        self.set_state_chain(id, state, call_action);
        if let Some(actor) = self.actors.get_mut(id)
            && actor.state.is_none()
            && actor
                .user_vars
                .contains_key("room_internal_temp_custom_inventory")
        {
            actor.destroyed = true;
            actor
                .user_vars
                .remove("room_internal_temp_custom_inventory");
        }
        self.psprite_weapon = saved.0;
        self.psprite_state = saved.1;
    }

    fn set_state_chain(
        &mut self,
        id: ActorId,
        state: Option<room_core::StateId>,
        call_action: bool,
    ) {
        const MAX_STATE_CYCLE: u32 = 1000;
        let mut next = state;
        let mut call = call_action;
        let mut cycle = 0u32;
        // Last few states of the chain, reported when the cycle limit trips.
        let mut recent: [Option<room_core::StateId>; 8] = [None; 8];
        // Cheap endless-loop probe: a real chain (even a long 0-tic spawn
        // burst) does not keep coming back to one state. Remember the state
        // entered at transition 64; seeing it 16 more times means a cycle, so
        // stop there instead of spending 1000 transitions (and their spawns).
        let mut probe: Option<room_core::StateId> = None;
        let mut probe_hits = 0u32;
        loop {
            if let Some(sid) = next {
                recent[(cycle as usize) % recent.len()] = Some(sid);
                if cycle == 64 {
                    probe = Some(sid);
                } else if probe == Some(sid) {
                    probe_hits += 1;
                    if probe_hits >= 16 {
                        cycle = MAX_STATE_CYCLE - 1;
                    }
                }
            }
            let Some(sid) = next else {
                if self.actors.get(id).is_some_and(|actor| !actor.destroyed) {
                    self.fire_zscript_thing_destroyed(id);
                }
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
            let callback_id = st.callback_id;
            let module_index = st.module_index;
            let vehicle_active = self.actors.get(id).is_some_and(|actor| {
                let mut class = Some(actor.class);
                for _ in 0..64 {
                    let Some(def) = class.and_then(|cid| self.classes.get(cid)) else {
                        break;
                    };
                    if def.name.eq_ignore_ascii_case("Tank") {
                        return self.states.find_label(actor.class, "Active") == Some(sid);
                    }
                    class = def.parent;
                }
                false
            });
            if vehicle_active {
                eprintln!(
                    "room: VEHICLE STATE tic={} actor=#{} state=Active sprite={} tics={} action={} args={:?} callback={} next={:?}",
                    self.level.time,
                    id.index(),
                    st.sprite,
                    st.tics,
                    action.as_deref().unwrap_or("<none>"),
                    action_args,
                    callback_id,
                    st.next,
                );
            }
            let old_sprite = self.actors.get(id).map(|a| a.sprite.clone());
            let countkill = self.actors.get(id).is_some_and(|a| {
                let class_flags = self.classes.get(a.class).map(|c| c.flags).unwrap_or(0);
                (a.flags | class_flags) & crate::class::FLAG_COUNTKILL != 0
            });
            if let Some(a) = self.actors.get_mut(id) {
                a.state = Some(sid);
                let tics = i32::from(st.tics);
                // `AActor::GetTics`: fast states run in `tics - tics/2` under fast monsters.
                let always_fast = a.extra_flags.get_name("ALWAYSFAST");
                a.tics = if (self.fast_monsters || always_fast) && st.fast && tics > 0 {
                    tics - (tics >> 1)
                } else {
                    tics
                };
                // `####` / `#` keep whatever the actor is already showing.
                // A 0-tic `TNT1` is not a displayed frame (Brutal Doom footstep /
                // flare gaps). Leave the previous picture up so a chain that
                // stops on that state does not blank the actor for a tic.
                let blank = st.sprite.is_empty() || st.sprite.eq_ignore_ascii_case("TNT1");
                let hold_picture = blank && a.tics == 0;
                if !hold_picture && !crate::state::sprite_unchanged(&st.sprite) {
                    a.sprite = st.sprite;
                }
                if !hold_picture && st.frame != crate::state::FRAME_NOCHANGE {
                    a.frame = st.frame;
                }
            }
            // A monster that sat on TNT1 and then picks up a real sprite is the
            // "popped into the room" case (replacement spawn, or a delayed frame).
            if countkill && self.level.maptime >= 1 {
                let was_hidden = match old_sprite.as_deref() {
                    None | Some("") => true,
                    Some(s) if s.eq_ignore_ascii_case("TNT1") => true,
                    Some(_) => false,
                };
                let appear = self.actors.get(id).and_then(|a| {
                    let now_hidden = a.sprite.is_empty() || a.sprite.eq_ignore_ascii_case("TNT1");
                    if !was_hidden || now_hidden {
                        return None;
                    }
                    Some((
                        self.classes
                            .get(a.class)
                            .map(|c| c.name.clone())
                            .unwrap_or_else(|| "?".into()),
                        a.sprite.clone(),
                        a.pos.x(),
                        a.pos.y(),
                        self.level.maptime,
                    ))
                });
                if let Some((name, sprite, x, y, tic)) = appear {
                    self.push_debug(format!(
                        "APPEAR {name}#{id:?} sprite={sprite} @({x:.0},{y:.0}) tic={tic}"
                    ));
                }
            }
            if !call {
                return;
            }
            let mut jump: Option<room_core::StateId> = None;
            // WASM state action wins over native when callback_id ≥ 0 (`docs/31` MA1).
            if callback_id >= 0 {
                if let Some(j) = crate::script_mods::dispatch_actor_state_action(
                    self,
                    module_index,
                    callback_id,
                    id,
                    sid,
                ) {
                    jump = j;
                }
            } else if let Some(name) = action.as_deref() {
                let evaluated = crate::deco_eval::eval_action_args(self, id, &action_args);
                if let ActionResult::Jump(j) = action::call_action_args(self, id, name, &evaluated)
                {
                    jump = Some(j);
                }
            }
            if let Some(j) = jump {
                next = Some(j);
                call = true;
            } else if self.actors.get(id).is_some_and(|a| a.tics == 0) {
                next = self
                    .actors
                    .get(id)
                    .and_then(|a| a.state)
                    .and_then(|s| self.states.get(s))
                    .and_then(|s| s.next);
                call = true;
            } else {
                return;
            }
            cycle += 1;
            if cycle >= MAX_STATE_CYCLE {
                let class = self
                    .actors
                    .get(id)
                    .and_then(|a| self.classes.get(a.class))
                    .map_or_else(|| "?".to_owned(), |c| c.name.clone());
                let mut chain = Vec::new();
                for k in 0..recent.len() {
                    let slot = (cycle as usize + k) % recent.len();
                    if let Some(st) = recent[slot].and_then(|s| self.states.get(s)) {
                        chain.push(format!(
                            "{}{}:{}:{}",
                            st.sprite,
                            char::from(b'A'.saturating_add(st.frame.min(25))),
                            st.tics,
                            st.action.as_deref().unwrap_or("-")
                        ));
                    }
                }
                // Console only, like GZDoom's red `Printf`: a HUD line per stuck
                // Brutal Doom effect actor covered the screen.
                self.push_debug(format!(
                    "Warning: endless 0-tic state loop in {class} #{}; destroying it [{}]",
                    id.index(),
                    chain.join(" > ")
                ));
                // GZDoom `AActor::SetState` destroys an actor stuck in an endless
                // 0-tic loop. Leaving it in place re-ran the whole loop (and its
                // spawns) every tic: Brutal Doom's `BurningZombie` Spawn state
                // fires `A_CustomMissile` in a 0-tic `Loop`.
                if self.actors.get(id).is_some_and(|actor| !actor.destroyed) {
                    self.fire_zscript_thing_destroyed(id);
                }
                if let Some(a) = self.actors.get_mut(id) {
                    a.state = None;
                    a.destroyed = true;
                }
                return;
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
        if !self.zero_tic_spawned.is_empty() && self.zero_tic_spawned.remove(&id) {
            // The spawn chain already used this tic (see `zero_tic_spawned`).
            return;
        }
        let Some(actor) = self.actors.get_mut(id) else {
            return;
        };
        // A class with no states at all (bare test maps, script stubs) has
        // nothing to advance to; ending a chain is what destroys an actor.
        if actor.state.is_none() {
            return;
        }
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

    /// Hash of retained Mod HUD widgets (tests only; not in [`Self::checksum`]).
    #[must_use]
    pub fn mod_hud_hash(&self) -> u64 {
        self.mod_hud.hash()
    }

    /// Hash of retained zone rings (tests only; not in [`Self::checksum`]).
    #[must_use]
    pub fn mod_zones_hash(&self) -> u64 {
        self.mod_zones.hash()
    }

    /// Actors that may still block or be hit, in thinker order: every
    /// non-destroyed actor plus, at most, ones destroyed earlier this tic
    /// (callers re-check `destroyed`). Falls back to the full thinker list
    /// before the first tic (e.g. straight after a load).
    #[must_use]
    pub fn collision_candidates(&self) -> &[ActorId] {
        if self.live_ids.is_empty() {
            &self.thinkers.default
        } else {
            &self.live_ids
        }
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
        // Player weapon / powerup state (netplay + demos). Seats past the
        // first 8 only count while in the game (UZ12 hash compat).
        for (seat, slot) in self.players.iter().enumerate() {
            let Some(p) = slot else {
                if seat < crate::player::LEGACY_SEATS {
                    bytes.push(0);
                }
                continue;
            };
            if seat >= crate::player::LEGACY_SEATS {
                bytes.extend_from_slice(&(seat as u32).to_le_bytes());
            }
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
            bytes.extend_from_slice(&p.pspr_bob_x.to_bits().to_le_bytes());
            bytes.extend_from_slice(&p.pspr_bob_y.to_bits().to_le_bytes());
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
            bytes.push(p.inven_icon);
            bytes.push(u8::from(p.toggle_key_flag));
            bytes.extend_from_slice(&p.invdisp_tics.to_le_bytes());
            bytes.extend_from_slice(&p.prev_pitch.unwrap_or(0).to_le_bytes());
            bytes.extend_from_slice(&p.footprint_shade.unwrap_or(0).to_le_bytes());
            bytes.push(p.footprint_count.unwrap_or(0));
            bytes.push(p.footprint_pal.unwrap_or(0));
            bytes.extend_from_slice(&p.transporter_hold_tics.to_le_bytes());
            bytes.extend_from_slice(&p.fta_tics.to_le_bytes());
            bytes.extend_from_slice(&p.ftq.to_le_bytes());
            bytes.push(p.hard_landing);
            bytes.extend_from_slice(&p.pycount.unwrap_or(0).to_le_bytes());
            bytes.extend_from_slice(&p.hurt_delay_tics.to_le_bytes());
            bytes.extend_from_slice(&p.rotscrnang.to_le_bytes());
            bytes.extend_from_slice(&p.return_to_center.to_le_bytes());
            bytes.extend_from_slice(&p.tip_incs.to_le_bytes());
            bytes.push(u8::from(p.walk_snd_toggle));
            bytes.extend_from_slice(&p.input_capture.to_le_bytes());
            bytes.extend_from_slice(&p.capture_module.to_le_bytes());
            bytes.extend_from_slice(&p.capture_started.to_le_bytes());
            bytes.extend_from_slice(&p.prev_buttons.to_le_bytes());
            bytes.extend_from_slice(
                &p.camera
                    .map(|c| u64::from(c.index()))
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            bytes.push(u8::from(p.camera_revert));
        }
        bytes.extend_from_slice(&self.player_names.checksum_bytes());
        // ZScript program + reachable script state (ZS6 / docs/34 §5.6).
        if let Some(h) = self.zscript.checksum_bytes() {
            bytes.extend_from_slice(&h);
        }
        xxh3_64(&bytes)
    }

    /// Set the display name for `seat` (0-based; session setup, BE3).
    pub fn set_player_display_name(&mut self, seat: usize, name: &str) {
        self.player_names.set(seat, name);
    }
}

fn destroy_queued_corpse(world: &mut World, id: ActorId) {
    if world.actors.get(id).is_some_and(|actor| !actor.destroyed) {
        world.fire_zscript_thing_destroyed(id);
    }
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
    // `MinRespawnTics` (default 12 s) and `RespawnDice` (default 4).
    let (min_tics, dice) = world
        .actors
        .get(id)
        .and_then(|a| world.classes.get(a.class))
        .map_or((12 * 35, 4), |c| {
            (
                c.ext.stored_i32("minrespawntics").unwrap_or(12 * 35),
                c.ext.stored_i32("respawndice").unwrap_or(4),
            )
        });
    if move_count < min_tics {
        return;
    }
    if world.level.time & 31 != 0 {
        return;
    }
    let roll = world.p_random("pr_respawn");
    if i32::from(roll) > dice {
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
    world.fire_zscript_thing_destroyed(id);
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

/// `+STEALTH` (`visdir`): attacking and pain fade the monster in, chasing
/// fades it back out.
fn note_stealth_state(world: &mut World, id: ActorId, state: Option<room_core::StateId>) {
    let Some(sid) = state else {
        return;
    };
    let Some(a) = world.actors.get(id) else {
        return;
    };
    if !a.extra_flags.get_name("STEALTH") {
        return;
    }
    let class = a.class;
    let is = |label: &str| world.states.find_label(class, label) == Some(sid);
    let dir = if is("Missile") || is("Melee") || is("Pain") {
        1.0
    } else if is("See") || is("Spawn") {
        -1.0
    } else {
        return;
    };
    if let Some(a) = world.actors.get_mut(id) {
        a.ext.scratch.insert("visdir".into(), dir);
    }
}

/// Per-tic stealth fade (`AActor::Tick`): `+1/32` alpha per tic while
/// `visdir > 0`, `-3/32` toward `StealthAlpha` while `visdir < 0`.
fn tick_stealth(world: &mut World) {
    let ids: Vec<ActorId> = world.thinkers.default.clone();
    for id in ids {
        let Some(a) = world.actors.get(id) else {
            continue;
        };
        if a.destroyed || !a.extra_flags.get_name("STEALTH") {
            continue;
        }
        let Some(dir) = a.ext.scratch.get("visdir").copied().filter(|d| *d != 0.0) else {
            continue;
        };
        let floor = world
            .classes
            .get(a.class)
            .map_or(0.0, |c| c.ext.stealth_alpha);
        let Some(a) = world.actors.get_mut(id) else {
            continue;
        };
        if dir > 0.0 {
            a.alpha += 1.0 / 32.0;
            if a.alpha >= 1.0 {
                a.alpha = 1.0;
                a.ext.scratch.insert("visdir".into(), 0.0);
            }
        } else {
            a.alpha -= 3.0 / 32.0;
            if a.alpha <= floor {
                a.alpha = floor;
                a.ext.scratch.insert("visdir".into(), 0.0);
            }
        }
        if a.render_style == crate::class::RenderStyle::Normal {
            a.render_style = crate::class::RenderStyle::Translucent;
        }
    }
}

/// `SpectatorCamera.Tick`: follow `tracer` (or the camera's player).
///
/// `SpectatorCamera.LagDistance` 0 keeps the tracer centred. Otherwise the
/// camera lags: past `LagDistance` it starts chasing at the tracer's speed,
/// snaps when twice that far, and `ChaseMode` 1 only chases while the tracer
/// moves, 2 stops after one step. `ChasingTracer` is the starting state.
fn tick_spectator_cameras(world: &mut World) {
    let cams: Vec<ActorId> = world
        .thinkers
        .default
        .iter()
        .copied()
        .filter(|&id| {
            world.actors.get(id).is_some_and(|a| {
                !a.destroyed
                    && world
                        .classes
                        .get(a.class)
                        .is_some_and(|c| c.ext.spectator_camera)
            })
        })
        .collect();
    for id in cams {
        let Some(cam) = world.actors.get(id).cloned() else {
            continue;
        };
        let def = world
            .classes
            .get(cam.class)
            .map(|c| c.ext.clone())
            .unwrap_or_default();
        let lag = def.stored_f64("spectatorcamera.lagdistance").unwrap_or(0.0);
        let mode = def.stored_i32("spectatorcamera.chasemode").unwrap_or(0);
        let mut chasing = cam.ext.scratch.get("chasingtracer").map_or_else(
            || def.stored_bool("spectatorcamera.chasingtracer"),
            |v| *v != 0.0,
        );
        let Some(t) = cam
            .tracer
            .and_then(|t| world.actors.get(t))
            .filter(|t| !t.destroyed)
            .cloned()
        else {
            continue;
        };
        let d = DVec3::new(
            t.pos.x() - cam.pos.x(),
            t.pos.y() - cam.pos.y(),
            t.pos.z() - cam.pos.z(),
        );
        let dist = room_core::dmath::sqrt(d.x() * d.x() + d.y() * d.y() + d.z() * d.z());
        let mut dest = cam.pos;
        if (dist <= 4.0 && chasing) || lag <= 0.0 || dist >= 2.0 * lag {
            dest = t.pos;
            chasing = false;
        } else {
            if dist > lag && !chasing {
                chasing = true;
            }
            if chasing && dist > 0.0 {
                let mut speed =
                    room_core::dmath::sqrt(t.vel.x() * t.vel.x() + t.vel.y() * t.vel.y()) / dist;
                if speed == 0.0 && mode == 0 {
                    speed = t.speed / dist;
                }
                dest = DVec3::new(
                    cam.pos.x() + 2.0 * d.x() * speed,
                    cam.pos.y() + 2.0 * d.y() * speed,
                    cam.pos.z() + 2.0 * d.z() * speed,
                );
                if mode > 1 {
                    chasing = false;
                }
            }
        }
        if let Some(a) = world.actors.get_mut(id) {
            a.pos = dest;
            a.ext
                .scratch
                .insert("chasingtracer".into(), f64::from(u8::from(chasing)));
        }
    }
}

/// Bring back items queued by [`queue_item_respawn`] (GZDoom
/// `Inventory.DoRespawn`): the item reappears at its map spot with `ItemFog`.
fn tick_item_respawns(world: &mut World) {
    if world.item_respawns.is_empty() {
        return;
    }
    let now = world.level.time;
    let due: Vec<(u32, ClassId, DVec3, Angle)> = world
        .item_respawns
        .iter()
        .copied()
        .filter(|r| r.0 <= now)
        .collect();
    world.item_respawns.retain(|r| r.0 > now);
    for (_, class, pos, angle) in due {
        let _ = world.spawn_at(class, pos, angle);
        world.play_sound("misc/spawn", pos.x(), pos.y(), pos.z(), 1.0);
        if let Some(fog) = world.classes.find_name("ItemFog") {
            let _ = world.spawn_at(fog, pos, Angle::ZERO);
        }
    }
}

/// Queue a picked-up map item to respawn (`Inventory.ShouldRespawn`).
///
/// Only with `respawn_items` (deathmatch item respawn), never for dropped
/// items or `+INVENTORY.NEVERRESPAWN`; `FakeInventory` needs
/// `FakeInventory.Respawns`. `+INVENTORY.ALWAYSRESPAWN` respawns without the
/// switch. The delay is `Inventory.RespawnTics` (default 30 s).
pub(crate) fn queue_item_respawn(world: &mut World, item: ActorId) {
    let Some(a) = world.actors.get(item) else {
        return;
    };
    if a.flags & crate::class::FLAG_DROPPED != 0 {
        return;
    }
    let (class, pos, angle) = (a.class, a.spawn_point, a.spawn_angle);
    let Some(c) = world.classes.get(class) else {
        return;
    };
    let always = c.ext.has_qualified_flag("INVENTORY.ALWAYSRESPAWN");
    if !(world.respawn_items || always) || c.ext.has_qualified_flag("INVENTORY.NEVERRESPAWN") {
        return;
    }
    if world.classes.inherits_from(class, "FakeInventory")
        && !c.ext.stored_bool("fakeinventory.respawns")
    {
        return;
    }
    let tics = c
        .ext
        .stored_i32("inventory.respawntics")
        .unwrap_or(30 * 35)
        .max(1);
    let due = world.level.time.saturating_add(tics as u32);
    world.item_respawns.push((due, class, pos, angle));
}

/// `ContactDamage`: hurt overlapping shootable actors every 8 tics.
fn tick_contact_damage(world: &mut World) {
    if world.level.time % 8 != 0 {
        return;
    }
    let fields: Vec<(ActorId, i32, f64, f64, f64, f64, f64)> = world
        .thinkers
        .default
        .iter()
        .copied()
        .filter_map(|id| {
            let a = world.actors.get(id)?;
            if a.destroyed {
                return None;
            }
            let dmg = world.classes.get(a.class)?.contact_damage;
            if dmg <= 0 {
                return None;
            }
            Some((id, dmg, a.pos.x(), a.pos.y(), a.pos.z(), a.radius, a.height))
        })
        .collect();
    let mut hits = Vec::new();
    for &oid in &world.thinkers.default {
        let Some(o) = world.actors.get(oid) else {
            continue;
        };
        if o.destroyed || (o.flags & crate::class::FLAG_SHOOTABLE) == 0 {
            continue;
        }
        if world
            .classes
            .get(o.class)
            .is_some_and(|c| c.contact_damage > 0)
        {
            continue;
        }
        for &(fid, dmg, x, y, z, r, h) in &fields {
            if oid == fid {
                continue;
            }
            let dx = o.pos.x() - x;
            let dy = o.pos.y() - y;
            let rad = o.radius + r;
            if dx * dx + dy * dy >= rad * rad {
                continue;
            }
            if o.pos.z() >= z + h || z >= o.pos.z() + o.height {
                continue;
            }
            hits.push((oid, fid, dmg));
        }
    }
    for (target, source, dmg) in hits {
        crate::damage::damage_mobj(world, target, Some(source), Some(source), dmg);
    }
    let mut victims: Vec<ActorId> = world.thinkers.default.to_vec();
    for p in world.players.iter().flatten() {
        if let Some(mo) = p.mo {
            if !victims.contains(&mo) {
                victims.push(mo);
            }
        }
    }
    let mut line_hits = Vec::new();
    for (i, line) in world.map.lines.iter().enumerate() {
        let Some(dmg) = world
            .map
            .udmf_user
            .lines
            .get(i)
            .and_then(|b| b.ints.get("user_contactdamage").copied())
            .filter(|&d| d > 0)
        else {
            continue;
        };
        let Some(v1) = world.map.vertices.get(line.v1.raw() as usize) else {
            continue;
        };
        let Some(v2) = world.map.vertices.get(line.v2.raw() as usize) else {
            continue;
        };
        let (ax, ay, bx, by) = (v1.pos.x(), v1.pos.y(), v2.pos.x(), v2.pos.y());
        for &oid in &victims {
            let Some(o) = world.actors.get(oid) else {
                continue;
            };
            if o.destroyed || (o.flags & crate::class::FLAG_SHOOTABLE) == 0 {
                continue;
            }
            let d = dist_point_seg(o.pos.x(), o.pos.y(), ax, ay, bx, by);
            if d > o.radius {
                continue;
            }
            line_hits.push((oid, dmg));
        }
    }
    for (target, dmg) in line_hits {
        crate::damage::damage_mobj(world, target, None, None, dmg);
    }
}

fn dist_point_seg(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return (px - ax).hypot(py - ay);
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
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

impl World {
    /// Apply the UZDoom 5.0 sky / fog MAPINFO keys (`skyfog`, `skymist`,
    /// `skymistyscale`, `useskymist`, `thickfogdistance`, `thickfogmultiplier`).
    pub fn apply_mapinfo_fog(&mut self, extras: &std::collections::BTreeMap<String, String>) {
        let num = |k: &str| extras.get(k).and_then(|v| v.trim().parse::<f64>().ok());
        if let Some(v) = num("skyfog") {
            self.sky_fog = v.max(0.0) as f32;
        }
        if let Some(v) = extras.get("skymist") {
            let mut it = v.splitn(2, ',');
            let tex = it.next().unwrap_or("").trim();
            if !tex.is_empty() {
                self.sky_mist = Some(tex.to_owned());
            }
            if let Some(sp) = it.next().and_then(|s| s.trim().parse::<f64>().ok()) {
                // UZDoom: speed * TICRATE / 1000.
                self.sky_mist_speed = sp * 35.0 / 1000.0;
            }
        }
        if let Some(v) = num("skymistyscale") {
            self.sky_mist_yscale = v.clamp(0.002, 544.0);
        }
        if let Some(v) = extras.get("useskymist") {
            self.use_sky_mist = !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | "off"
            );
        }
        if let Some(v) = num("thickfogdistance") {
            self.thick_fog_distance = v;
        }
        if let Some(v) = num("thickfogmultiplier") {
            self.thick_fog_multiplier = v;
        }
    }

    /// Apply queued `Level.SetSkyFog` / `SetThickFog` / `ChangeSkyMist` calls.
    pub fn apply_zscript_level_ops(&mut self) {
        for op in self.zscript.take_level_ops() {
            match op {
                room_zvm::ZsLevelOp::SkyFog(v) => self.sky_fog = v.max(0) as f32,
                room_zvm::ZsLevelOp::ThickFog(d, m) => {
                    self.thick_fog_distance = d;
                    self.thick_fog_multiplier = m;
                }
                room_zvm::ZsLevelOp::SkyMist {
                    texture,
                    use_mist,
                    yscale,
                } => {
                    self.sky_mist = texture;
                    self.use_sky_mist = use_mist;
                    self.sky_mist_yscale = yscale.clamp(0.002, 544.0);
                }
            }
        }
    }

    /// Thick fog for the renderer: `(distance, multiplier)` when active.
    pub fn thick_fog(&self) -> Option<(f32, f32)> {
        (self.thick_fog_distance > 0.0 && self.thick_fog_multiplier > 0.0).then_some({
            (
                self.thick_fog_distance as f32,
                self.thick_fog_multiplier as f32,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::StateTable;
    use crate::player::TicCmd;
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};

    #[test]
    fn seats_past_eight_share_the_free_player_starts() {
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 256.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .thing(2, 320.0, 64.0, 0)
            .thing(3, 576.0, 64.0, 0)
            .thing(4, 832.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let w = World::from_map(map).unwrap();
        let xs: Vec<i16> = (8..12)
            .map(|seat| w.respawn_spot(seat).unwrap().x)
            .collect();
        let mut sorted = xs.clone();
        sorted.sort_unstable();
        sorted.dedup();
        // No pawn stands on any start yet, so four seats past 8 get the four
        // starts instead of everyone landing on Player 1's.
        assert_eq!(sorted.len(), 4, "spread over the free starts: {xs:?}");
    }

    #[test]
    fn zero_tic_tnt1_keeps_the_picture_already_showing() {
        let ir = parse_decorate(
            r#"
actor Z : Actor 9101
{
  States
  {
  Spawn:
    POSS A 1
    TNT1 A 0
    POSS B 2
    Stop
  Hide:
    TNT1 A 4
    Stop
  }
}
"#,
        )
        .unwrap();
        let classes = std::sync::Arc::new(crate::ClassRegistry::from_decorate(&ir).unwrap());
        let states = std::sync::Arc::new(StateTable::from_decorate(&ir, &classes));
        let zid = classes.find_name("Z").unwrap();
        let mut w = World::from_map_with_classes(empty_room(), classes, states).unwrap();
        let id = w
            .spawn_at(
                zid,
                room_core::DVec3::new(64.0, 64.0, 0.0),
                room_core::Angle::ZERO,
            )
            .unwrap();
        let gap = w.states.find_label(zid, "Spawn").unwrap();
        let gap = w.states.get(gap).unwrap().next.unwrap();
        assert!(
            w.states
                .get(gap)
                .unwrap()
                .sprite
                .eq_ignore_ascii_case("TNT1")
        );
        assert_eq!(w.states.get(gap).unwrap().tics, 0);
        w.set_state(id, Some(gap), false);
        let a = w.actors.get(id).unwrap();
        assert_eq!(a.sprite, "POSS");
        assert_eq!(a.frame, 0, "0-tic TNT1 must not blank the frame");
        let hide = w.states.find_label(zid, "Hide").unwrap();
        w.set_state(id, Some(hide), false);
        assert_eq!(w.actors.get(id).unwrap().sprite, "TNT1");
    }

    #[test]
    fn tnt1_weapon_frames_have_no_patch() {
        assert_eq!(
            weapon_patch_from_sprite_frame("PISF", 0).as_deref(),
            Some("PISFA0")
        );
        assert_eq!(
            weapon_patch_from_sprite_frame("tnt1", 0).as_deref(),
            Some("")
        );
        assert_eq!(weapon_patch_from_sprite_frame("", 0), None);
    }

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
    fn checksum_ignores_mod_hud_widgets() {
        use crate::mod_hud::HudWidget;
        let mut w = World::from_map(empty_room()).unwrap();
        let c0 = w.checksum();
        let h0 = w.mod_hud_hash();
        assert_eq!(w.mod_hud.set(0, 1, 1, HudWidget::default(), 1), 0);
        assert_eq!(w.checksum(), c0);
        assert_ne!(w.mod_hud_hash(), h0);
    }

    #[test]
    fn checksum_ignores_mod_zones() {
        let mut w = World::from_map(empty_room()).unwrap();
        let c0 = w.checksum();
        let z0 = w.mod_zones_hash();
        assert_eq!(
            w.mod_zones
                .set(0, 0, 100, 200, 5000, 0, 0, 4000, 0xff0000ff, 0, 1),
            0
        );
        assert_eq!(w.checksum(), c0);
        assert_ne!(w.mod_zones_hash(), z0);
    }

    #[test]
    fn checksum_includes_player_camera() {
        let mut w = World::from_map(empty_room()).unwrap();
        w.tick();
        let c0 = w.checksum();
        let mo = w.players[0].as_ref().and_then(|p| p.mo);
        if let Some(p) = w.players[0].as_mut() {
            if let Some(mo) = mo {
                p.camera = Some(mo);
                p.camera_revert = false;
            }
        }
        assert_ne!(w.checksum(), c0);
    }

    #[test]
    fn checksum_includes_player_names() {
        let mut w = World::from_map(empty_room()).unwrap();
        let c0 = w.checksum();
        w.set_player_display_name(0, "Alice");
        assert_ne!(w.checksum(), c0);
        let c1 = w.checksum();
        w.set_player_display_name(1, "Bob");
        assert_ne!(w.checksum(), c1);
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
    fn spawn_class_scale_is_visual_only() {
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
        // GZDoom: `Scale` never changes the collision box.
        let def = w.classes.get(a.class).unwrap();
        assert!((a.radius - def.radius).abs() < 1e-9);
        assert!((a.height - def.height).abs() < 1e-9);
        assert!(a.scale_x > 1.0);
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
    fn contact_damage_hurts_overlapping_player() {
        let ir = parse_decorate(&format!(
            "{}\nactor Zap : Actor 9901\n{{\n  Health 40\n  Radius 16\n  Height 64\n  \
             +SOLID\n  +SHOOTABLE\n  +NOGRAVITY\n  ContactDamage 8\n  \
             States {{ Spawn: TNT1 A -1 Stop }}\n}}\n",
            include_str!("../../../testdata/actors/doom_basic.txt")
        ))
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
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let pos = w.actors.get(mo).unwrap().pos;
        let hp0 = w.actors.get(mo).unwrap().health;
        w.spawn_at(classes.find_name("Zap").unwrap(), pos, Angle::ZERO)
            .unwrap();
        for _ in 0..8 {
            w.tick();
        }
        let hp = w.actors.get(mo).unwrap().health;
        assert!(hp < hp0, "contact {hp0} → {hp}");
    }

    #[test]
    fn contact_damage_on_a_line_hurts_player() {
        let ir = parse_decorate(include_str!("../../../testdata/actors/doom_basic.txt")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        if map.udmf_user.lines.len() < map.lines.len() {
            map.udmf_user
                .lines
                .resize(map.lines.len(), Default::default());
        }
        map.udmf_user.lines[0]
            .ints
            .insert("user_contactdamage".into(), 8);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let line = &w.map.lines[0];
        let a = w.map.vertices[line.v1.raw() as usize].pos;
        let b = w.map.vertices[line.v2.raw() as usize].pos;
        let hp0 = w.actors.get(mo).unwrap().health;
        let flags = w.actors.get(mo).unwrap().flags;
        let radius = w.actors.get(mo).unwrap().radius;
        let mid = DVec3::new((a.x() + b.x()) / 2.0, (a.y() + b.y()) / 2.0, 0.0);
        let dist = dist_point_seg(mid.x(), mid.y(), a.x(), a.y(), b.x(), b.y());
        assert!(
            w.map.udmf_user.lines[0]
                .ints
                .get("user_contactdamage")
                .is_some_and(|&d| d > 0),
            "user field missing"
        );
        assert!(
            (flags & crate::class::FLAG_SHOOTABLE) != 0,
            "player not shootable {flags:#x}"
        );
        assert!(dist <= radius, "mid dist {dist} radius {radius}");
        for _ in 0..8 {
            if let Some(p) = w.actors.get_mut(mo) {
                p.pos = mid;
            }
            w.tick();
        }
        let hp = w.actors.get(mo).unwrap().health;
        assert!(hp < hp0, "line contact {hp0} → {hp} time {}", w.level.time);
    }

    #[test]
    fn countkill_stuck_in_a_wall_is_nudged_free() {
        let ir = parse_decorate(include_str!("../../../testdata/actors/doom_basic.txt")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(128.0, 128.0, 0)
            .thing(3004, 2.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let w = World::from_map_with_classes(map, classes, states).unwrap();
        let zid = w
            .thinkers
            .default
            .iter()
            .copied()
            .find(|&id| {
                w.actors.get(id).is_some_and(|a| {
                    w.classes
                        .get(a.class)
                        .is_some_and(|c| c.name == "ZombieMan")
                })
            })
            .expect("zombie");
        let z = w.actors.get(zid).unwrap();
        assert!(
            crate::physics::check_position(&w, z, z.pos, Some(zid)).ok,
            "nudged out of the west wall at {:?}",
            z.pos
        );
        assert!(z.pos.x() > 16.0, "x {}", z.pos.x());
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
            ext: 0,
            user: Default::default(),
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
        assert!((friend.radius - 40.0).abs() < 1e-9); // default radius 20 × map scale
        assert!((friend.height - 32.0).abs() < 1e-9); // default height 16 × map scale
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
            ext: 0,
            user: Default::default(),
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
            ext: 0,
            user: Default::default(),
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
    fn spawn_eight_players_uses_dm_starts_after_coop() {
        let mut b = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 1024.0, 1024.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .thing(2, 128.0, 64.0, 0);
        for (x, y) in [
            (200.0, 200.0),
            (800.0, 200.0),
            (200.0, 800.0),
            (800.0, 800.0),
        ] {
            b = b.thing(11, x, y, 0);
        }
        let mut map = b.build_udmf("MAP01").unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map(map).unwrap();
        assert_eq!(w.spawn_coop_players(8).unwrap(), 8);
        for seat in 0..8 {
            assert!(w.players[seat].is_some(), "seat {seat} missing");
        }
    }

    #[test]
    fn bot_brains_are_deterministic_across_runs() {
        let map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .thing(2, 128.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        let run = || {
            let mut map = map.clone();
            apply_nodes(&mut map);
            let mut w = World::from_map(map).unwrap();
            w.deathmatch = true;
            let _ = w.spawn_coop_players(2).unwrap();
            w.fill_bot_seats(0, 1);
            for _ in 0..400 {
                w.set_ticcmds(&[TicCmd::default(); 2]);
                w.tick();
            }
            w.checksum()
        };
        assert_eq!(run(), run());
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
                ext: 0,
                user: Default::default(),
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
            ext: 0,
            user: Default::default(),
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
            ext: 0,
            user: Default::default(),
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
        w.tick();
        let hud = w.player_hud(0).unwrap();
        assert_eq!(hud.ammo, before + 40);
        assert!(hud.ammo_max >= hud.ammo, "ammo_max={}", hud.ammo_max);
        // Default pistol has no AmmoType2 → unused secondary.
        assert_eq!(hud.ammo2, -1);
        assert_eq!(hud.ammo2_max, 0);
        assert_eq!(hud.armor, 50);
        assert_eq!(hud.keys & 1, 1);
        assert_eq!(hud.keys & 8, 8, "blue skull bit");
        assert_eq!(hud.face, "STFGOD0");
        if let Some(p) = w.players[0].as_mut() {
            p.god = false;
            p.invulnerable = true;
            p.invul_tics = 100;
        }
        w.tick();
        assert_eq!(w.player_hud(0).unwrap().face, "STFGOD0");
    }

    #[test]
    fn scoreboard_credits_kills_items_and_secrets_to_the_player() {
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
        let zombie = classes.find_name("ZombieMan").unwrap();
        let z = w
            .spawn_at(
                zombie,
                DVec3::new(160.0, 160.0, 0.0),
                Angle::from_degrees(0.0),
            )
            .unwrap();
        let _ = crate::damage::damage_mobj(&mut w, z, Some(mo), Some(mo), 1000);
        let z2 = w
            .spawn_at(
                zombie,
                DVec3::new(200.0, 200.0, 0.0),
                Angle::from_degrees(0.0),
            )
            .unwrap();
        let _ = crate::damage::damage_mobj(&mut w, z2, None, None, 1000);
        // Doom `P_KillMobj`: in a one-player game every monster death counts
        // for that player, even with no killer.
        assert_eq!(w.player_stats[0][0], 2);
        // Also while the player is out of the list (thinking).
        w.taken_player = Some((0, mo));
        let p = w.players[0].take();
        w.credit_player_stat(Some(mo), 2);
        w.players[0] = p;
        w.taken_player = None;
        assert_eq!(w.player_stats[0][2], 1);
        let rows = w.scoreboard();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kills, 2);
        assert_eq!(rows[0].secrets, 1);
        assert_eq!(rows[0].name, "Player 1");
    }

    #[test]
    fn status_bar_items_list_inventory_armor_and_weapon() {
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
        let _ = crate::inventory::give_inventory(&mut w, mo, clip, 10);
        if let Some(p) = w.players[0].as_mut() {
            p.armor = 50;
        }
        w.tick();
        let items = w.status_bar_items(0).unwrap();
        let clip_row = items
            .inventory
            .iter()
            .find(|(c, _, _)| c.eq_ignore_ascii_case("Clip"));
        assert!(
            clip_row.is_some_and(|(_, n, _)| *n >= 10),
            "{:?}",
            items.inventory
        );
        assert!(
            items
                .inventory
                .iter()
                .any(|(c, n, _)| c == "BasicArmor" && *n == 50)
        );
        assert!(items.icons.iter().any(|(c, _)| c == "BasicArmor"));
        if let Some(weapon) = &items.ready_weapon {
            assert!(items.ammo1.is_some(), "{weapon} has an ammo type");
        }
    }

    #[test]
    fn player_hud_ammo2_from_ammotype2() {
        let mut src = include_str!("../../../testdata/actors/doom_basic.txt").to_owned();
        src.push_str(
            r#"
actor DualAmmoGun : Weapon
{
  Weapon.AmmoType "Clip"
  Weapon.AmmoType2 "Shell"
  States { Ready: PISG A 1 Loop }
}
"#,
        );
        let file = parse_decorate(&src).unwrap();
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
        let gun = classes.find_name("DualAmmoGun").unwrap();
        let shell = classes.find_name("Shell").unwrap();
        let _ = crate::inventory::give_inventory(&mut w, mo, shell, 8);
        if let Some(p) = w.players[0].as_mut() {
            p.ready_weapon = Some(gun);
        }
        let hud = w.player_hud(0).unwrap();
        assert_eq!(hud.ammo2, 8);
        assert!(hud.ammo2_max >= 8, "ammo2_max={}", hud.ammo2_max);
    }

    fn face_world() -> (World, ActorId, Arc<ClassRegistry>) {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let file = parse_decorate(src).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&file).unwrap());
        let states = Arc::new(StateTable::from_decorate(&file, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 512.0, 512.0), 0.0, 128.0)
            .player_start(64.0, 64.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let p = w.players[0].as_mut().unwrap();
        p.notarget = true;
        let mo = p.mo.unwrap();
        (w, mo, classes)
    }

    #[test]
    fn mugshot_looks_around_and_tracks_pain_band() {
        let (mut w, mo, _) = face_world();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..35 * 8 {
            w.tick();
            seen.insert(w.player_hud(0).unwrap().face);
        }
        assert!(
            seen.len() >= 2 && seen.iter().all(|f| f.starts_with("STFST0")),
            "idle face glances left/right: {seen:?}"
        );
        w.actors.get_mut(mo).unwrap().health = 50;
        // The idle face re-reads health when its 17-tic glance runs out.
        for _ in 0..18 {
            w.tick();
        }
        assert!(w.player_hud(0).unwrap().face.starts_with("STFST2"));
    }

    #[test]
    fn mugshot_turns_toward_attacker_and_shows_ouch() {
        let (mut w, mo, classes) = face_world();
        let imp = classes.find_name("DoomImp").unwrap();
        // Player faces east (0°); an attacker due north is to the left.
        let north = w
            .spawn_at(imp, DVec3::new(64.0, 400.0, 0.0), Angle::from_degrees(0.0))
            .unwrap();
        w.tick();
        let _ = crate::damage::damage_mobj(&mut w, mo, None, Some(north), 5);
        w.tick();
        assert_eq!(w.player_hud(0).unwrap().face, "STFTL00");
        for _ in 0..40 {
            w.tick();
        }
        let south = w
            .spawn_at(imp, DVec3::new(64.0, -300.0, 0.0), Angle::from_degrees(0.0))
            .unwrap();
        let _ = crate::damage::damage_mobj(&mut w, mo, None, Some(south), 5);
        w.tick();
        assert_eq!(w.player_hud(0).unwrap().face, "STFTR00");
        for _ in 0..40 {
            w.tick();
        }
        // Losing more than 20 health in one tic: ouch face (GZDoom sign).
        let _ = crate::damage::damage_mobj(&mut w, mo, None, Some(north), 30);
        w.tick();
        let face = w.player_hud(0).unwrap().face;
        assert!(face.starts_with("STFOUCH"), "got {face}");
        let hud = w.player_hud(0).unwrap();
        assert!(
            (1..=8).contains(&hud.palette),
            "red flash, got {}",
            hud.palette
        );
    }

    #[test]
    fn mugshot_evil_grin_on_weapon_pickup() {
        let (mut w, mo, classes) = face_world();
        w.tick();
        let shotgun = classes.find_name("Shotgun").unwrap();
        let item = w
            .spawn_at(
                shotgun,
                DVec3::new(64.0, 64.0, 0.0),
                Angle::from_degrees(0.0),
            )
            .unwrap();
        assert!(crate::pickup::try_pickup(&mut w, mo, item));
        w.tick();
        let hud = w.player_hud(0).unwrap();
        assert_eq!(hud.face, "STFEVL0");
        assert!(
            (9..=12).contains(&hud.palette),
            "gold flash, got {}",
            hud.palette
        );
        assert_ne!(hud.arms & (1 << 3), 0, "ARMS slot 3 lit");
    }

    #[test]
    fn fixed_colormap_inverse_while_invulnerable_and_blinks_out() {
        let mut p = Player::default();
        assert_eq!(p.fixed_colormap(), 0);
        p.infrared = true;
        p.infrared_tics = 1000;
        assert_eq!(p.fixed_colormap(), 1);
        p.invulnerable = true;
        p.invul_tics = 1000;
        assert_eq!(p.fixed_colormap(), crate::player::INVERSE_COLORMAP);
        p.invul_tics = 100; // last 4 s, bit 3 clear → off (masks infrared too)
        assert_eq!(p.fixed_colormap(), 0);
        p.invul_tics = 104;
        assert_eq!(p.fixed_colormap(), crate::player::INVERSE_COLORMAP);
    }

    #[test]
    fn invulnerability_shows_god_face() {
        let (mut w, mo, _) = face_world();
        assert_eq!(crate::inventory::cheat_give(&mut w, mo, "idbeholdv"), 1);
        for _ in 0..3 {
            w.tick();
        }
        let hud = w.player_hud(0).unwrap();
        assert_eq!(hud.colormap, crate::player::INVERSE_COLORMAP);
        assert_eq!(hud.face, "STFGOD0");
    }

    #[test]
    fn deathmatch_counts_frags_and_allows_pvp_damage() {
        let (mut w, mo0, _) = face_world();
        let classes = Arc::clone(&w.classes);
        let pawn = classes.find_name("DoomPlayer").unwrap();
        let mo1 = w
            .spawn_at(
                pawn,
                DVec3::new(200.0, 200.0, 0.0),
                Angle::from_degrees(0.0),
            )
            .unwrap();
        let mut second = Player::default();
        second.possess(mo1, pawn);
        w.players[1] = Some(second);
        // Coop: players cannot hurt each other.
        assert_eq!(
            crate::damage::damage_mobj(&mut w, mo1, None, Some(mo0), 10),
            0
        );
        let hud = w.player_hud(0).unwrap();
        assert!(!hud.deathmatch);
        assert_eq!(hud.face_back, Some(0), "netgame face background");
        assert_eq!(w.player_hud(1).unwrap().face_back, Some(1));

        w.deathmatch = true;
        assert!(crate::damage::damage_mobj(&mut w, mo1, None, Some(mo0), 1000) > 0);
        assert_eq!(w.players[0].as_ref().unwrap().frags[1], 1);
        assert_eq!(w.player_hud(0).unwrap().frags, 1);
        // Killed by the world: counts as a suicide (−1).
        let _ = crate::damage::damage_mobj(&mut w, mo0, None, None, 1000);
        let hud = w.player_hud(0).unwrap();
        assert!(hud.deathmatch);
        assert_eq!(hud.frags, 0);
    }

    #[test]
    fn duke_match_clears_monsters_and_uses_a_dm_start() {
        let src = include_str!("../../../testdata/actors/doom_basic.txt");
        let extra = "actor DukePlayer : DoomPlayer { }\n";
        let ir = parse_decorate(&format!("{src}\n{extra}")).unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 256.0, 256.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .thing(11, 200.0, 64.0, 90)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let duke = classes.find_name("DukePlayer").unwrap();
        if let Some(p) = w.players[0].as_mut() {
            p.cls = duke;
        }
        let zom = w
            .spawn_at(
                classes.find_name("ZombieMan").unwrap(),
                DVec3::new(80.0, 80.0, 0.0),
                Angle::ZERO,
            )
            .unwrap();
        w.apply_duke_match();
        assert!(w.deathmatch);
        assert!(
            w.actors
                .get(zom)
                .is_none_or(|a| a.destroyed || a.health <= 0),
            "DukeMatch drops COUNTKILL"
        );
        let mo = w.players[0].as_ref().unwrap().mo.unwrap();
        let p = w.actors.get(mo).unwrap().pos;
        assert!(
            (p.x() - 200.0).abs() < 2.0 && (p.y() - 64.0).abs() < 2.0,
            "DM start {p:?}"
        );
    }

    #[test]
    fn mugshot_rampage_after_two_seconds_of_fire() {
        let (mut w, _, _) = face_world();
        for _ in 0..80 {
            if let Some(p) = w.players[0].as_mut() {
                p.cmd.buttons = crate::player::Buttons(crate::player::Buttons::ATTACK);
            }
            w.tick();
        }
        assert_eq!(w.player_hud(0).unwrap().face, "STFKILL0");
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
            ext: 0,
            user: Default::default(),
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

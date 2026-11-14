//! Player movement — Chocolate `processinput` (movement-only slice for D5).
//!
//! Full jump/crouch/jetpack/weapons land in later D5/D6 work. This module
//! applies `fvel`/`svel`/`avel` with vanilla friction and `clipmove_xy`.

use room_build_engine::geom::{
    SectorWalls, SectorZ, SpriteZClip, WallXY, clipmove_xy_at_z_sprites_with_lists,
    getzrange_with_sprites, getzsofslope, pushmove, updatesector,
};
use room_build_engine::helpers::{getangle, sincos_q14};
use room_build_engine::numeric::{klabs, mulscale};
use room_build_engine::types::{SectorT, SpriteT, WallT};
use room_build_engine::world::SpriteLists;

/// Chocolate `PHEIGHT` (`duke3d.h`) — sprite Z is `posz + PHEIGHT`.
pub const PHEIGHT: i32 = 38 << 8;

/// Chocolate `gc` default from Atomic `gamestartup` / 1.3D CON fallback.
pub const DUKE_GC: i32 = 176;

/// Chocolate `sync[].bits` position for `gamefunc_Open` (`player.c`).
pub const INPUT_OPEN: u32 = 1 << 29;
/// Chocolate `sync.bits` full-autoaim mode bit (bit 6).
pub const INPUT_AUTO_AIM: u32 = 1 << 6;
/// Chocolate inventory controls packed in `sync.bits`.
pub const INPUT_INVENTORY_USE: u32 = 1 << 30;
pub const INPUT_INVENTORY_LEFT: u32 = 1 << 20;
pub const INPUT_INVENTORY_RIGHT: u32 = 1 << 27;

/// Chocolate `CLIPMASK0`.
pub const CLIPMASK0: u32 = (1 << 16) + 1;

/// Per-tic sync input (Chocolate `input` / `sync[]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DukeInput {
    pub fvel: i16,
    pub svel: i16,
    /// Angular velocity (signed); applied as `avel << 1`.
    pub avel: i8,
    /// Horizon delta (`sync.horz` / DMO `horizon`).
    pub horz: i8,
    /// Chocolate `sync[].bits` (jump=1, crouch=2, …).
    pub bits: u32,
}

impl DukeInput {
    /// Chocolate `getinput`: build a sync record from player-relative
    /// movement. `vel` is forward and `svel` is strafe-left, in key units
    /// (`NORMALKEYMOVE` 40, doubled when running). Duke's sync stores
    /// world-space momentum, so both are rotated by the current heading:
    /// `fvel = vel·cos + svel·sin`, `svel = vel·sin − svel·cos` (mulscale9).
    #[must_use]
    pub fn from_local(vel: i32, svel: i32, ang: i16, avel: i8, horz: i8, bits: u32) -> Self {
        let (sin, cos) = room_build_engine::helpers::approx_sincos_q14(i32::from(ang));
        let ms9 = |a: i32, b: i32| ((i64::from(a) * i64::from(b)) >> 9) as i32;
        let momx = ms9(vel, cos) + ms9(svel, sin);
        let momy = ms9(vel, sin) - ms9(svel, cos);
        Self {
            fvel: momx.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            svel: momy.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            avel,
            horz,
            bits,
        }
    }

    /// Map a parsed `.DMO` player record onto Room sync fields.
    #[must_use]
    pub fn from_dmo(d: &room_build::DmoInput) -> Self {
        Self {
            fvel: d.forward,
            svel: d.strafe,
            avel: d.turn,
            horz: d.horizon,
            bits: d.bits,
        }
    }
}

/// Atomic `USER.CON` `RUNNINGSPEED` → Chocolate `dukefriction` (`gamestartup`).
///
/// `global.c` defaults to `0xcc00` before CON load; retail Atomic sets `53200`.
pub const DUKE_FRICTION: i32 = 53200;

/// `TICSPERFRAME` when `TICRATE=120` and 30 Hz (`120/30 = 4`).
pub const TICS_PER_FRAME: i32 = 4;

/// ART size + picanm offsets for [`getzrange`] sprite planes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TileZMeta {
    pub width: i16,
    pub height: i16,
    pub x_offset: i8,
    pub y_offset: i8,
}

/// Build a [`SpriteZClip`] from a live sprite + ART meta table.
#[must_use]
pub fn sprite_z_clip(s: &SpriteT, tile_meta: &[TileZMeta]) -> SpriteZClip {
    let meta = tile_meta
        .get(s.picnum.max(0) as usize)
        .copied()
        .unwrap_or_default();
    SpriteZClip {
        x: s.x,
        y: s.y,
        z: s.z,
        cstat: s.cstat,
        clipdist: s.clipdist,
        xrepeat: s.xrepeat,
        yrepeat: s.yrepeat,
        xoffset: s.xoffset,
        yoffset: s.yoffset,
        ang: s.ang,
        sectnum: s.sectnum,
        tile_width: meta.width,
        tile_height: meta.height,
        picanm_xoff: meta.x_offset,
        picanm_yoff: meta.y_offset,
    }
}

/// Player pose + XY momentum (subset of `player_struct`).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Player {
    pub posx: i32,
    pub posy: i32,
    pub posz: i32,
    /// Previous-tic pose for `findplayer` (Chocolate `opos*`).
    pub oposx: i32,
    pub oposy: i32,
    pub oposz: i32,
    pub ang: i16,
    /// Last sampled `sync[].avel`, used by Chocolate's view weapon sway.
    #[serde(default)]
    pub last_avel: i8,
    /// Chocolate `player_struct::look_ang` view yaw offset.
    #[serde(default)]
    pub look_ang: i16,
    /// Chocolate `player_struct::rotscrnang`; visual screen roll in Build angle units.
    #[serde(default)]
    pub rotscrnang: i16,
    /// Chocolate `player_struct::auto_aim` (2=full, 1=bullet-only).
    #[serde(default = "default_auto_aim")]
    pub auto_aim: i16,
    /// Chocolate `player_struct::aim_mode` (0 = aim to floor plane when grounded).
    #[serde(default)]
    pub aim_mode: u8,
    /// Chocolate `ud.wchoice[seat]` preference order (one-based weapon IDs;
    /// zero selects the remote detonator).
    #[serde(default = "default_weapon_choices")]
    pub weapon_choices: [u8; 10],
    /// Chocolate `over_shoulder_on`; selects the native third-person camera.
    #[serde(default)]
    pub over_shoulder_on: bool,
    /// Q16 camera distance: Chocolate's 120 Hz clock adds 1024 per frame.
    #[serde(default)]
    pub shoulder_camera_distance: i32,
    /// Build camera selected through an Atomic VIEWSCREEN, as
    /// (x, y, z, angle, sector). `None` means the player view is active.
    #[serde(default)]
    pub remote_camera: Option<(i32, i32, i32, i16, i16)>,
    pub horiz: i16,
    /// Chocolate `ohoriz`, previous sampled horizon for player-view smoothing.
    #[serde(default = "default_horiz")]
    pub previous_horiz: i16,
    /// Chocolate `horizoff` (slope look).
    pub horizoff: i16,
    /// Chocolate `ohorizoff`, previous sampled slope-look offset.
    #[serde(default)]
    pub previous_horizoff: i16,
    /// Chocolate `pyoff` (view Z bob).
    pub pyoff: i32,
    /// Chocolate `pycount` (bob phase, Build angle units).
    pub pycount: i16,
    /// Chocolate `bobpos*` — previous-tic pre-clip pose for walk `xvel`.
    pub bobposx: i32,
    /// Chocolate `hittype[p->i].floorz` / `ceilingz` from processinput's getzrange.
    #[serde(default)]
    pub hit_floorz: i32,
    #[serde(default)]
    pub hit_ceilingz: i32,
    /// Chocolate player-body `sprite[p->i].xvel`, set early in `processinput`.
    #[serde(default)]
    pub body_xvel: i16,
    pub bobposy: i32,
    pub cursectnum: i16,
    pub posxv: i32,
    pub posyv: i32,
    /// Delayed world-axis force from moving sector effectors, consumed by
    /// the next local input sample like Chocolate's `fricxv` / `fricyv`.
    #[serde(default)]
    pub effector_force_x: i32,
    #[serde(default)]
    pub effector_force_y: i32,
    /// Vertical velocity (Chocolate `poszv`).
    pub poszv: i32,
    /// Chocolate `player_struct::truefz`, the cached true floor plane.
    #[serde(default)]
    pub true_floor_z: i32,
    /// Chocolate `player_struct::truecz`, the cached true ceiling plane.
    #[serde(default)]
    pub true_ceiling_z: i32,
    /// Chocolate `player_struct::bobcounter`, reset when a warp elevator transfers the player.
    #[serde(default)]
    pub bob_counter: i32,
    /// Chocolate `player_struct::weapon_sway` (view-only weapon bob phase).
    #[serde(default)]
    pub weapon_sway: i32,
    /// Chocolate `return_to_center` — recenters `horiz` toward 100.
    pub return_to_center: i16,
    /// Sprite index for the player body (`ps[].i`), or `-1`.
    pub sprite_i: i16,
    pub extra: i16,
    /// Previous health snapshot used by Chocolate's damage/healing logic.
    pub last_extra: i16,
    /// Chocolate `airleft` underwater oxygen countdown (15 seconds at 26 tics/s).
    #[serde(default = "default_airleft")]
    pub airleft: i16,
    /// Chocolate `extra_extra8`, Q8 fractional environmental damage accumulator.
    #[serde(default)]
    pub extra_extra8: i16,
    /// Chocolate `player_struct::shield_amount` (body armor, 0–100).
    #[serde(default = "default_shield_amount")]
    pub shield_amount: i16,
    /// Chocolate `hurt_delay` cooldown for damage pain cues.
    #[serde(default)]
    pub hurt_delay: i16,
    /// Atomic inventory counters used by CON `ifpinventory` / `addinventory`.
    #[serde(default)]
    pub steroids_amount: i16,
    #[serde(default)]
    pub scuba_amount: i16,
    #[serde(default)]
    pub holoduke_amount: i16,
    #[serde(default)]
    pub jetpack_amount: i16,
    #[serde(default)]
    pub heat_amount: i16,
    #[serde(default)]
    pub firstaid_amount: i16,
    #[serde(default)]
    pub boot_amount: i16,
    /// Remaining Atomic footprint placements from a blood pool or water landing.
    #[serde(default)]
    pub footprint_count: u8,
    /// Palette copied to the next footprint sprite.
    #[serde(default)]
    pub footprint_pal: u8,
    /// Shade copied to the next footprint sprite.
    #[serde(default)]
    pub footprint_shade: i8,
    /// Chocolate fall counter retained across saves and in-air movement.
    #[serde(default)]
    pub falling_counter: u8,
    /// `PLAYERONWATER` helper sprite tracked by Chocolate `dummyplayersprite`.
    #[serde(default = "no_sprite_index")]
    pub dummy_player_sprite: i16,
    /// Chocolate `inven_icon` selection for the inventory HUD.
    #[serde(default)]
    pub inventory_icon: u8,
    /// Chocolate `refresh_inventory` HUD redraw latch.
    #[serde(default)]
    pub refresh_inventory: u8,
    /// Chocolate `holoduke_on` sprite index (`-1` means no hologram).
    #[serde(default = "no_sprite_index")]
    pub holoduke_on: i16,
    /// Chocolate `heat_on` night vision toggle.
    #[serde(default)]
    pub heat_on: bool,
    /// Chocolate `pals[0..3]` palette flash tint (RGB, 0..=64).
    #[serde(default)]
    pub palette_flash: [u8; 3],
    /// Chocolate `pals_time`, remaining tics for the screen tint.
    #[serde(default)]
    pub palette_flash_tics: u8,
    /// Chocolate `numloogs`, number of loogies on the first-person view.
    #[serde(default)]
    pub num_loogies: u8,
    /// Chocolate `loogcnt`, 30 Hz countdown for the loogie overlay.
    #[serde(default)]
    pub loogie_tics: u8,
    /// Normalized 320×200 virtual-screen X positions for Atomic's loogie sprites.
    #[serde(default)]
    pub loogie_x: [i16; 6],
    /// Normalized 320×200 virtual-screen Y positions for Atomic's loogie sprites.
    #[serde(default)]
    pub loogie_y: [i16; 6],
    /// Chocolate `invdisptime`, inventory HUD display countdown.
    #[serde(default)]
    pub inventory_display_timer: i16,
    /// Chocolate `ud.god` health-write suppression.
    pub god_mode: bool,
    /// Chocolate `actors_killed` counter, used by CON `addkills` and intermission.
    pub actors_killed: u8,
    /// Chocolate `max_actors_killed`, counted from eligible map badguys at spawn.
    #[serde(default)]
    pub max_actors_killed: u8,
    /// Chocolate `frag` score accumulated from player deaths.
    #[serde(default)]
    pub frag_count: u16,
    /// Seat credited for this player's most recent death (`frag_ps`).
    #[serde(default)]
    pub frag_ps: i16,
    /// Number of self-frags accumulated (`fraggedself`).
    #[serde(default)]
    pub fragged_self: u16,
    /// Chocolate `got_access`: red=1, blue=2, yellow=4.
    #[serde(default)]
    pub got_access: u8,
    /// Access-card insertion animation (`access_incs`, 0 means idle).
    #[serde(default)]
    pub access_incs: u8,
    /// Pending wall switch for the access-card insertion, `-1` when absent.
    #[serde(default = "no_sprite_index")]
    pub access_wallnum: i16,
    /// Pending sprite switch for the access-card insertion, `-1` when absent.
    #[serde(default = "no_sprite_index")]
    pub access_spritenum: i16,
    /// Chocolate `secret_rooms`, incremented by entering a secret sector.
    pub secret_rooms: i16,
    /// Chocolate `max_secret_rooms`, counted from secret sectors at prelevel.
    #[serde(default)]
    pub max_secret_rooms: u8,
    /// Chocolate `timebeforeexit`; CON `endofgame` counts this down per frame.
    pub time_before_exit: i16,
    /// Chocolate `customexitsound` (`-1` selects the default exit sound).
    pub custom_exit_sound: i16,
    /// Chocolate `ud.eog`, set as soon as CON `endofgame` executes.
    pub end_of_game: bool,
    /// Co-op mode gates Chocolate's weapon reservation handshake.
    pub coop_mode: bool,
    /// Atomic `ud.coop` mode (0 = DukeMatch, 1 = co-op, 2 = no-spawn DukeMatch).
    #[serde(default)]
    pub game_mode: u8,
    /// Chocolate `ud.ffire`; defaults off, so coop players do not damage each other.
    #[serde(default)]
    pub friendly_fire: bool,
    /// Chocolate `weaprecs` / `weapreccnt` (max 16 reserved weapon actors).
    pub weapon_reservations: [i16; 16],
    pub weapon_reservation_count: u8,
    /// Chocolate `weapon_ang` adjustment while aiming the displayed weapon.
    #[serde(default)]
    pub weapon_ang: i16,
    /// Chocolate `ftq` current first-time quote ID; 0 means no quote.
    pub quote_id: i16,
    /// Chocolate `fta` countdown, set to 100 by `FTA`.
    pub quote_timer: i16,
    /// Chocolate `ud.fta_on`; `quote` has mode 0 and obeys this toggle.
    pub fta_enabled: bool,
    /// Chocolate `tipincs`; consumed by the Duke HUD tip animation.
    pub tip_timer: u8,
    pub curr_weapon: i16,
    /// Chocolate `wantweaponfire`, a weapon requested while the current one fires.
    pub want_weapon_fire: i16,
    /// Chocolate `hbomb_hold_delay`, also selects the alternating Devastator barrel.
    pub hbomb_hold_delay: i16,
    /// Chocolate `hbomb_on`: pipebombs owned by this player detonate after remote fire.
    pub hbomb_on: bool,
    /// Chocolate `rapid_fire_hold`, latched when the pipebomb is held through tic 6.
    pub rapid_fire_hold: bool,
    pub ammo: [i16; 12],
    /// Chocolate `gotweapon[MAX_WEAPONS]`.
    pub gotweapon: [bool; 12],
    /// Chocolate `kickback_pic` (weapon anim / fire phase).
    pub kickback_pic: i16,
    /// Chocolate's last usable weapon before an empty-gun switch.
    #[serde(default)]
    pub last_full_weapon: i16,
    /// Chocolate `show_empty_weapon` click latch.
    #[serde(default)]
    pub show_empty_weapon: bool,
    /// Chocolate hard-landing view dip state.
    #[serde(default)]
    pub hard_landing: u8,
    /// Chocolate knuckle-crack animation state.
    #[serde(default)]
    pub knuckle_incs: u8,
    /// Chocolate alternating walking-sound phase.
    #[serde(default)]
    pub walking_snd_toggle: u8,
    /// Chocolate `player_struct::knee_incs` scripted/player knee animation.
    #[serde(default)]
    pub knee_incs: i16,
    /// Chocolate `player_struct::actorsqu` target locked by `pstomp` / knee.
    #[serde(default = "no_sprite_index")]
    pub actorsqu: i16,
    /// Target to resolve after the scripted knee animation completes.
    #[serde(default)]
    pub pending_knee_target: Option<i16>,
    /// Chocolate `holster_weapon`; `pstomp` clears it when the knee finishes.
    #[serde(default)]
    pub holster_weapon: i16,
    /// Chocolate `fist_incs`, used by the Atomic nuke-button exit sequence.
    #[serde(default)]
    pub fist_incs: i16,
    /// Palette captured from the nuke button (`buttonpalette`).
    #[serde(default)]
    pub button_palette: u8,
    /// Secret destination stored in Atomic `ud.secretlevel` by a colored button.
    #[serde(default)]
    pub secret_level: i16,
    /// Chocolate `quick_kick` countdown, triggered by CON `pkick`.
    pub quick_kick: i16,
    /// Chocolate `last_weapon` (−1 = none).
    pub last_weapon: i16,
    /// Chocolate `weapon_pos` (draw raise/lower).
    pub weapon_pos: i16,
    /// Chocolate `on_ground`.
    pub on_ground: bool,
    /// Chocolate `player_struct::dead_flag` death-tilt angle (0 = active).
    #[serde(default)]
    pub dead_flag: u16,
    /// True while a lethal Freezer hit holds APLAYER in Atomic's PFROZEN state.
    #[serde(default)]
    pub freezer_frozen: bool,
    /// Remaining Atomic `THAWTIME` tics, including the action-frame boundary.
    #[serde(default)]
    pub freezer_tics: u16,
    /// Player body palette restored by CON `getlastpal` when the ice thaws.
    #[serde(default)]
    pub freezer_palette: u8,
    /// One-shot latch for death presentation and frag scoring.
    #[serde(default)]
    pub death_processed: bool,
    /// Chocolate `wackedbyactor`, used to turn the dead player's view toward
    /// the actor that delivered the fatal hit.
    #[serde(default = "no_sprite_index")]
    pub wacked_by_actor: i16,
    /// Chocolate `on_warping_sector` (set by `movetransports`).
    pub on_warping_sector: i16,
    /// Chocolate `transporter_hold`.
    pub transporter_hold: i16,
    /// Chocolate `somethingonplayer`: the green slime clinging to the face.
    #[serde(default)]
    pub something_on_player: Option<i16>,
    /// Chocolate `last_pissed_time`: toilet/urinal use cooldown (26*220).
    #[serde(default)]
    pub last_pissed_time: i16,
    /// Chocolate `jumping_counter` (0 = not jumping).
    pub jumping_counter: i16,
    /// Chocolate `jumping_toggle`.
    pub jumping_toggle: i16,
    /// Chocolate `jetpack_on` (0 = off).
    pub jetpack_on: i16,
    /// Chocolate `scuba_on` (0 = off).
    pub scuba_on: i16,
    /// Chocolate player `toggle_key_flag`, used to edge-trigger pool-ball kicks.
    #[serde(default)]
    pub toggle_key_flag: u8,
    /// Previous tic's cooperative Use bit for edge-triggered wall interactions.
    #[serde(default)]
    pub use_was_down: bool,
    /// Previous cooperative inventory action bit states for edge handling.
    #[serde(default)]
    pub inventory_use_was_down: bool,
    /// Multiplayer use-button edge consumed by Chocolate's dead-player respawn menu.
    #[serde(default)]
    pub respawn_use_was_down: bool,
    #[serde(default)]
    pub inventory_left_was_down: bool,
    #[serde(default)]
    pub inventory_right_was_down: bool,
}

fn no_sprite_index() -> i16 {
    -1
}

fn default_auto_aim() -> i16 {
    2
}

fn default_horiz() -> i16 {
    100
}

fn default_airleft() -> i16 {
    15 * 26
}

fn default_weapon_choices() -> [u8; 10] {
    [3, 4, 5, 7, 8, 6, 0, 2, 9, 1]
}

fn default_shield_amount() -> i16 {
    100
}

impl Default for Player {
    fn default() -> Self {
        Self {
            posx: 0,
            posy: 0,
            posz: 0,
            oposx: 0,
            oposy: 0,
            oposz: 0,
            ang: 0,
            last_avel: 0,
            look_ang: 0,
            rotscrnang: 0,
            auto_aim: 2,
            aim_mode: 0,
            weapon_choices: default_weapon_choices(),
            over_shoulder_on: false,
            shoulder_camera_distance: 0,
            remote_camera: None,
            horiz: 100,
            previous_horiz: 100,
            horizoff: 0,
            previous_horizoff: 0,
            pyoff: 0,
            pycount: 0,
            bobposx: 0,
            hit_floorz: 0,
            hit_ceilingz: 0,
            body_xvel: 0,
            bobposy: 0,
            cursectnum: 0,
            posxv: 0,
            posyv: 0,
            effector_force_x: 0,
            effector_force_y: 0,
            poszv: 0,
            true_floor_z: 0,
            true_ceiling_z: 0,
            bob_counter: 0,
            weapon_sway: 0,
            return_to_center: 9,
            sprite_i: -1,
            extra: 100,
            last_extra: 100,
            airleft: default_airleft(),
            extra_extra8: 0,
            shield_amount: default_shield_amount(),
            hurt_delay: 0,
            steroids_amount: 0,
            scuba_amount: 0,
            holoduke_amount: 0,
            jetpack_amount: 0,
            heat_amount: 0,
            firstaid_amount: 0,
            boot_amount: 0,
            footprint_count: 0,
            footprint_pal: 0,
            footprint_shade: 0,
            falling_counter: 0,
            dummy_player_sprite: -1,
            inventory_icon: 0,
            refresh_inventory: 0,
            holoduke_on: -1,
            heat_on: false,
            palette_flash: [0; 3],
            palette_flash_tics: 0,
            num_loogies: 0,
            loogie_tics: 0,
            loogie_x: [0; 6],
            loogie_y: [0; 6],
            inventory_display_timer: 0,
            god_mode: false,
            actors_killed: 0,
            max_actors_killed: 0,
            frag_count: 0,
            frag_ps: -1,
            fragged_self: 0,
            got_access: 0,
            access_incs: 0,
            access_wallnum: -1,
            access_spritenum: -1,
            secret_rooms: 0,
            max_secret_rooms: 0,
            time_before_exit: 0,
            custom_exit_sound: -1,
            end_of_game: false,
            coop_mode: false,
            game_mode: 0,
            friendly_fire: false,
            weapon_reservations: [0; 16],
            weapon_reservation_count: 0,
            weapon_ang: 0,
            quote_id: 0,
            quote_timer: 0,
            fta_enabled: true,
            tip_timer: 0,
            curr_weapon: 1,
            want_weapon_fire: -1,
            hbomb_hold_delay: 0,
            hbomb_on: false,
            rapid_fire_hold: false,
            ammo: [0, 48, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            // Chocolate `premap.c` grants the hand remote from level start,
            // even before the first pipebomb has been picked up.
            gotweapon: [
                true, true, false, false, false, false, false, false, false, false, true, false,
            ],
            kickback_pic: 0,
            last_full_weapon: -1,
            show_empty_weapon: false,
            hard_landing: 0,
            knuckle_incs: 0,
            walking_snd_toggle: 0,
            knee_incs: 0,
            actorsqu: -1,
            pending_knee_target: None,
            holster_weapon: 0,
            last_pissed_time: 0,
            something_on_player: None,
            fist_incs: 0,
            button_palette: 0,
            secret_level: 0,
            quick_kick: 0,
            last_weapon: -1,
            weapon_pos: 0,
            on_ground: true,
            dead_flag: 0,
            freezer_frozen: false,
            freezer_tics: 0,
            freezer_palette: 0,
            death_processed: false,
            wacked_by_actor: -1,
            on_warping_sector: 0,
            transporter_hold: 0,
            jumping_counter: 0,
            jumping_toggle: 0,
            jetpack_on: 0,
            scuba_on: 0,
            toggle_key_flag: 0,
            use_was_down: false,
            inventory_use_was_down: false,
            respawn_use_was_down: false,
            inventory_left_was_down: false,
            inventory_right_was_down: false,
        }
    }
}

impl Player {
    /// Atomic camera yaw (`ang + look_ang`) in Build angle units.
    #[must_use]
    pub fn view_ang(&self) -> i16 {
        self.ang.wrapping_add(self.look_ang)
    }

    /// Atomic camera horizon (`horiz + horizoff`).
    #[must_use]
    pub fn view_horizon(&self) -> i16 {
        self.horiz.wrapping_add(self.horizoff)
    }
}

/// Chocolate `checkavailinven` — choose the first available icon by priority.
pub fn check_available_inventory(player: &mut Player) {
    player.inventory_icon = if player.firstaid_amount > 0 {
        1
    } else if player.steroids_amount > 0 {
        2
    } else if player.holoduke_amount > 0 {
        3
    } else if player.jetpack_amount > 0 {
        4
    } else if player.heat_amount > 0 {
        5
    } else if player.scuba_amount > 0 {
        6
    } else if player.boot_amount > 0 {
        7
    } else {
        0
    };
}

/// Chocolate `checksectors` inventory ring. Returns the selected FTA quote ID.
pub fn cycle_inventory(player: &mut Player, right: bool) -> Option<i16> {
    let mut icon = player.inventory_icon.min(7);
    for _ in 0..8 {
        icon = match (icon, right) {
            (0 | 1, true) => 2,
            (0 | 1, false) => 7,
            (7, true) => 1,
            (_, true) => icon + 1,
            (_, false) => icon - 1,
        };
        let available = match icon {
            1 => player.firstaid_amount > 0,
            2 => player.steroids_amount > 0,
            3 => player.holoduke_amount > 0,
            4 => player.jetpack_amount > 0,
            5 => player.heat_amount > 0,
            6 => player.scuba_amount > 0,
            7 => player.boot_amount > 0,
            _ => false,
        };
        if available {
            player.inventory_icon = icon;
            return Some(match icon {
                1 => 3,
                2 => 90,
                3 => 91,
                4 => 88,
                5 => 101,
                6 => 89,
                7 => 6,
                _ => 0,
            });
        }
    }
    player.inventory_icon = 0;
    None
}

/// Movement-only `processinput`: angle, Z fall (getzrange + clamp), friction, clipmove.
pub fn processinput_move(
    p: &mut Player,
    sync: DukeInput,
    sectors: &[SectorT],
    walls: &[WallT],
    sprites: &[SpriteT],
    tile_meta: &[TileZMeta],
    dukefriction: i32,
) -> i32 {
    processinput_move_with_lists(
        p,
        sync,
        sectors,
        walls,
        sprites,
        tile_meta,
        dukefriction,
        None,
    )
}

/// `processinput_move` with Chocolate's live sector sprite-list ordering.
pub fn processinput_move_with_lists(
    p: &mut Player,
    sync: DukeInput,
    sectors: &[SectorT],
    walls: &[WallT],
    sprites: &[SpriteT],
    tile_meta: &[TileZMeta],
    dukefriction: i32,
    sprite_lists: Option<&SpriteLists>,
) -> i32 {
    processinput_move_with_lists_clipping(
        p,
        sync,
        sectors,
        walls,
        sprites,
        tile_meta,
        dukefriction,
        sprite_lists,
        false,
    )
}

/// Chocolate movement path with `ud.clipping` control; when enabled, XY moves
/// directly and only `updatesector` runs, with no clipmove/pushmove.
pub fn processinput_move_with_lists_clipping(
    p: &mut Player,
    sync: DukeInput,
    sectors: &[SectorT],
    walls: &[WallT],
    sprites: &[SpriteT],
    tile_meta: &[TileZMeta],
    dukefriction: i32,
    sprite_lists: Option<&SpriteLists>,
    noclip: bool,
) -> i32 {
    let mut movement_hit = 0;
    // Chocolate `processinput` runs once per 30 Hz game frame.
    p.last_avel = sync.avel;
    p.auto_aim = if sync.bits & INPUT_AUTO_AIM != 0 {
        2
    } else {
        1
    };
    let player_palette = usize::try_from(p.sprite_i)
        .ok()
        .and_then(|index| sprites.get(index))
        .map_or(0, |sprite| sprite.pal);
    if p.quick_kick > 0 && player_palette != 1 {
        p.quick_kick -= 1;
    }
    if p.time_before_exit > 1 && p.extra > 0 {
        p.time_before_exit -= 1;
    }
    if p.quote_timer > 0 {
        p.quote_timer -= 1;
        if p.quote_timer == 0 {
            p.quote_id = 0;
        }
    }
    p.tip_timer = p.tip_timer.saturating_sub(1);
    p.loogie_tics = p.loogie_tics.saturating_sub(1);
    // Chocolate `processinput`: screen roll damps toward zero, then the
    // temporary look angle eases back to the player's body angle.
    if p.rotscrnang > 0 {
        p.rotscrnang = (i32::from(p.rotscrnang) - (i32::from(p.rotscrnang) >> 1) - 1) as i16;
    } else if p.rotscrnang < 0 {
        p.rotscrnang = (i32::from(p.rotscrnang) + ((-i32::from(p.rotscrnang)) >> 1) + 1) as i16;
    }
    p.look_ang -= p.look_ang >> 2;
    let doubvel = TICS_PER_FRAME
        * if p.jetpack_on == 0 && p.steroids_amount > 0 && p.steroids_amount < 400 {
            2
        } else {
            1
        };

    // Chocolate `processinput` sets the body sprite's `xvel` from the
    // distance since `bobpos` *before* this tic's clipmove — i.e. the
    // previous tic's horizontal displacement — advances `bobcounter` with the
    // previous tic's `on_ground`, then refreshes `bobpos`. CON `ifp pwalking`
    // and the view bob read this value.
    // Chocolate `processinput`: `weapon_sway` eases back to 1024 when the
    // body is slow or airborne, otherwise follows `bobcounter` (view only).
    if p.body_xvel < 32 || !p.on_ground || p.bob_counter == 1024 {
        if (p.weapon_sway & 2047) > 1024 + 96 {
            p.weapon_sway -= 96;
        } else if (p.weapon_sway & 2047) < 1024 - 96 {
            p.weapon_sway += 96;
        } else {
            p.weapon_sway = 1024;
        }
    } else {
        p.weapon_sway = p.bob_counter;
    }
    {
        let dx_bob = p.posx.wrapping_sub(p.bobposx);
        let dy_bob = p.posy.wrapping_sub(p.bobposy);
        let dist2_bob = dx_bob
            .saturating_mul(dx_bob)
            .saturating_add(dy_bob.saturating_mul(dy_bob));
        p.body_xvel = room_build_engine::numeric::ksqrt(dist2_bob) as i16;
        if p.on_ground {
            p.bob_counter = p.bob_counter.wrapping_add(i32::from(p.body_xvel) >> 1);
        }
        p.bobposx = p.posx;
        p.bobposy = p.posy;
    }

    // Chocolate `findplayer` (movefta / standables) reads `opos` after processinput.
    // Atomic snapshots `opos` mid-function *before* the main clipmove / late Z, so
    // movers still see the previous-tic pose. Snapshot at entry to match that.
    p.oposx = p.posx;
    p.oposy = p.posy;
    p.oposz = p.posz;

    if sync.avel != 0 {
        // Chocolate: `tempang = avel<<1`; underwater lotag 2 → `(tempang-(tempang>>3))*ksgn(doubvel)`.
        let tempang = i32::from(sync.avel) << 1;
        let psectlotag = sectors
            .get(p.cursectnum as usize)
            .map(|s| s.lotag)
            .unwrap_or(0);
        let ksgn_dv = if doubvel > 0 {
            1
        } else if doubvel < 0 {
            -1
        } else {
            0
        };
        let angvel = if psectlotag == 2 {
            (tempang - (tempang >> 3)) * ksgn_dv
        } else {
            tempang * ksgn_dv
        };
        p.ang = ((i32::from(p.ang) + angvel) & 2047) as i16;
    }
    // Chocolate `return_to_center`: pull horiz toward ~100 each tic.
    if p.return_to_center > 0 {
        p.return_to_center -= 1;
        p.horiz = (i32::from(p.horiz) + 33 - (i32::from(p.horiz) / 3)).clamp(-99, 299) as i16;
    }
    if p.aim_mode != 0 {
        // Chocolate mouse aiming: `p->horiz += sync.horz >> 1`.
        p.horiz = (i32::from(p.horiz) + (i32::from(sync.horz) >> 1)).clamp(-99, 299) as i16;
    } else {
        if sync.horz != 0 {
            // Room keeps the legacy free-look delta when aim mode is off.
            p.horiz = (i32::from(p.horiz) + i32::from(sync.horz) * 2).clamp(-99, 299) as i16;
        }
        // Chocolate non-`aim_mode`: snap near-center horiz to 100.
        if p.horiz > 95 && p.horiz < 105 {
            p.horiz = 100;
        }
    }

    // Chocolate: getzrange + fall/ceiling clamp happen *before* XY clipmove.
    apply_player_fall_z(
        p,
        sync,
        sectors,
        walls,
        sprites,
        tile_meta,
        sprite_lists.map(|lists| &lists.sprite_stat[..]),
    );

    if p.posxv != 0 || p.posyv != 0 || sync.fvel != 0 || sync.svel != 0 {
        p.posxv += (i32::from(sync.fvel) * doubvel) << 6;
        p.posyv += (i32::from(sync.svel) * doubvel) << 6;

        let psectlotag = sectors
            .get(p.cursectnum as usize)
            .map(|s| s.lotag)
            .unwrap_or(0);
        // Chocolate: crouch bit2 → friction-0x2000; underwater → -0x1400; else plain.
        // (Knee+kickback>10 path omitted until kb is tracked.)
        let fric = if p.on_ground && (sync.bits & 2) != 0 {
            dukefriction - 0x2000
        } else if psectlotag == 2 {
            dukefriction - 0x1400
        } else {
            dukefriction
        };
        p.posxv = mulscale(p.posxv, fric, 16);
        p.posyv = mulscale(p.posyv, fric, 16);

        if p.posxv.abs() < 2048 && p.posyv.abs() < 2048 {
            p.posxv = 0;
            p.posyv = 0;
        }
    }

    let sec: Vec<SectorWalls> = sectors
        .iter()
        .map(|s| SectorWalls {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
        })
        .collect();
    let sec_z: Vec<SectorZ> = sectors
        .iter()
        .map(|s| SectorZ {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
            ceilingz: s.ceilingz,
            floorz: s.floorz,
            ceilingstat: s.ceilingstat,
            floorstat: s.floorstat,
            ceilingheinum: s.ceilingheinum,
            floorheinum: s.floorheinum,
        })
        .collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();

    if p.posxv != 0 || p.posyv != 0 {
        // Chocolate `clipmove` goals are `pos + (posxv>>14)` — Room takes world
        // deltas. `flordist`/`ceildist` match player.c HORIZONLY (lotag 1 → 4<<8).
        let psectlotag = sectors
            .get(p.cursectnum as usize)
            .map(|s| s.lotag)
            .unwrap_or(0);
        let flordist = if psectlotag == 1 { 4 << 8 } else { 20 << 8 };
        let clips: Vec<SpriteZClip> = sprites
            .iter()
            .map(|sprite| sprite_z_clip(sprite, tile_meta))
            .collect();
        if noclip {
            p.posx = p.posx.wrapping_add(p.posxv >> 14);
            p.posy = p.posy.wrapping_add(p.posyv >> 14);
            updatesector(p.posx, p.posy, &mut p.cursectnum, &sec, &wxy);
        } else {
            movement_hit = clipmove_xy_at_z_sprites_with_lists(
                &mut p.posx,
                &mut p.posy,
                p.posz,
                &mut p.cursectnum,
                p.posxv >> 14,
                p.posyv >> 14,
                164,
                4 << 8,
                flordist,
                &sec,
                &wxy,
                Some(&sec_z),
                &clips,
                1,
                sprite_lists,
            );
        }
    }

    // Chocolate: after setsprite, always `pushmove(..., 164, 4<<8, 4<<8, CLIPMASK0)`.
    if !noclip {
        let _ = pushmove(
            &mut p.posx,
            &mut p.posy,
            p.posz,
            &mut p.cursectnum,
            164,
            4 << 8,
            4 << 8,
            CLIPMASK0,
            &sec,
            &sec_z,
            &wxy,
        );
    }

    // `s->xvel` was set near the top of processinput (see `body_xvel`).
    let walk_xvel = i32::from(p.body_xvel);

    // Chocolate walk view-bob (`pyoff`) — feeds shotgun `sz = posz+pyoff+(4<<8)`.
    let psectlotag = sectors
        .get(p.cursectnum as usize)
        .map(|s| s.lotag)
        .unwrap_or(0);
    if p.jetpack_on == 0 {
        if walk_xvel > 16 {
            if psectlotag != 1 && psectlotag != 2 && p.on_ground {
                p.pycount = ((i32::from(p.pycount) + 52) & 2047) as i16;
                let (sin, _) = sincos_q14(i32::from(p.pycount));
                p.pyoff = klabs(walk_xvel.wrapping_mul(sin)) / 1596;
            }
        } else if psectlotag != 2 && psectlotag != 1 {
            p.pyoff = 0;
        }
    }

    // Chocolate player.c knee_incs cadence used by CON `pstomp` and by the
    // native knee weapon. The scripted target is resolved by DukeSession.
    if p.knee_incs > 0 {
        p.knee_incs += 1;
        p.horiz = p.horiz.wrapping_sub(48);
        p.return_to_center = 9;
        if p.knee_incs > 15 {
            p.knee_incs = 0;
            p.holster_weapon = 0;
            if p.weapon_pos < 0 {
                p.weapon_pos = p.weapon_pos.wrapping_neg();
            }
            if let Ok(target) = usize::try_from(p.actorsqu)
                && let Some(sprite) = sprites.get(target)
            {
                let dx = sprite.x.wrapping_sub(p.posx).saturating_abs();
                let dy = sprite.y.wrapping_sub(p.posy).saturating_abs();
                let dz = (sprite.z.wrapping_sub(p.posz + PHEIGHT) >> 4).saturating_abs();
                let mut largest = dx.max(dy);
                let mut middle = dx.min(dy);
                if largest < dz {
                    middle = largest;
                    largest = dz;
                } else if middle < dz {
                    middle = dz;
                }
                let tail = middle.saturating_add(dx.min(dy).min(dz));
                let distance = largest
                    .saturating_sub(largest >> 4)
                    .saturating_add(tail >> 2)
                    .saturating_add(tail >> 3);
                if distance < 1400 {
                    p.pending_knee_target = Some(p.actorsqu);
                }
            }
            p.actorsqu = -1;
        } else if let Ok(target) = usize::try_from(p.actorsqu)
            && let Some(sprite) = sprites.get(target)
        {
            let goal = getangle(sprite.x.wrapping_sub(p.posx), sprite.y.wrapping_sub(p.posy));
            let mut difference = (goal - i32::from(p.ang)) & 2047;
            if difference > 1024 {
                difference -= 2048;
            }
            p.ang = p.ang.wrapping_add((difference >> 2) as i16);
        }
    }
    if p.cursectnum >= 0 && (p.cursectnum as usize) < sec_z.len() {
        // Chocolate `processinput` caches the true sloped planes before
        // `moveeffectors`; SE17 carries these values as it moves the sector.
        let (ceiling_z, floor_z) = getzsofslope(p.cursectnum, p.posx, p.posy, &sec_z, &wxy);
        p.true_floor_z = floor_z;
        p.true_ceiling_z = ceiling_z;
    }
    movement_hit
}

/// Chocolate fall + water/ground smooth + ceiling rest (`player.c` subset).
pub fn apply_player_fall_z(
    p: &mut Player,
    sync: DukeInput,
    sectors: &[SectorT],
    walls: &[WallT],
    sprites: &[SpriteT],
    tile_meta: &[TileZMeta],
    sprite_stat: Option<&[i16]>,
) {
    if p.cursectnum < 0 || p.cursectnum as usize >= sectors.len() {
        return;
    }

    let secs: Vec<SectorZ> = sectors
        .iter()
        .map(|s| SectorZ {
            wallptr: s.wallptr,
            wallnum: s.wallnum,
            ceilingz: s.ceilingz,
            floorz: s.floorz,
            ceilingstat: s.ceilingstat,
            floorstat: s.floorstat,
            ceilingheinum: s.ceilingheinum,
            floorheinum: s.floorheinum,
        })
        .collect();
    let wxy: Vec<WallXY> = walls.iter().map(WallT::xy).collect();
    let zclips: Vec<SpriteZClip> = sprites
        .iter()
        .map(|s| sprite_z_clip(s, tile_meta))
        .collect();

    let zr = getzrange_with_sprites(
        p.posx,
        p.posy,
        p.posz,
        p.cursectnum,
        163,
        CLIPMASK0,
        &secs,
        &wxy,
        &zclips,
    );
    let mut cz = zr.ceilz;
    let fz = zr.florz;
    // Chocolate `hittype[pi].floorz = fz; hittype[pi].ceilingz = cz;`
    p.hit_floorz = fz;
    p.hit_ceilingz = cz;
    let mut psectlotag = sectors[p.cursectnum as usize].lotag;
    let truefz = sectors[p.cursectnum as usize].floorz;
    let truefdist = (p.posz - truefz).abs();
    // Chocolate: deep water floor from getzrange → treat as dry for fall `i`.
    if (zr.florhit & 49152) == 16384 && psectlotag == 1 && truefdist > PHEIGHT + (16 << 8) {
        psectlotag = 0;
    }
    // Sprite ceiling that is a living badguy → use true sector ceiling.
    if zr.ceilhit >= 0 && (zr.ceilhit & 49152) == 49152 {
        let si = (zr.ceilhit & (room_build_engine::world::MAXSPRITES as i32 - 1)) as usize;
        if let Some(s) = sprites.get(si) {
            // `SpriteT::statnum` is the map-file value; the live stat is in
            // the sprite lists when the caller has them.
            let stat = sprite_stat
                .and_then(|stats| stats.get(si).copied())
                .unwrap_or(s.statnum);
            if stat == crate::stat::STAT_ACTOR && s.extra >= 0 {
                cz = sectors[p.cursectnum as usize].ceilingz;
            }
        }
    }

    // Chocolate underwater (sector lotag 2) — swim / damp, no gravity.
    if psectlotag == 2 {
        p.jumping_counter = 0;
        p.pycount = ((i32::from(p.pycount) + 32) & 2047) as i16;
        let (sin, _) = sincos_q14(i32::from(p.pycount));
        p.pyoff = sin >> 7;
        if sync.bits & 1 != 0 {
            if p.poszv > 0 {
                p.poszv = 0;
            }
            p.poszv -= 348;
            if p.poszv < -(256 * 6) {
                p.poszv = -(256 * 6);
            }
        } else if sync.bits & 2 != 0 {
            if p.poszv < 0 {
                p.poszv = 0;
            }
            p.poszv += 348;
            if p.poszv > 256 * 6 {
                p.poszv = 256 * 6;
            }
        } else {
            if p.poszv < 0 {
                p.poszv += 256;
                if p.poszv > 0 {
                    p.poszv = 0;
                }
            }
            if p.poszv > 0 {
                p.poszv -= 256;
                if p.poszv < 0 {
                    p.poszv = 0;
                }
            }
        }
        if p.poszv > 2048 {
            p.poszv >>= 1;
        }
        p.posz += p.poszv;
        if p.posz > fz - (15 << 8) {
            p.posz += ((fz - (15 << 8)) - p.posz) >> 1;
        }
        if p.posz < cz + (4 << 8) {
            p.posz = cz + (4 << 8);
            p.poszv = 0;
        }
        return;
    }

    if p.jetpack_on > 0 {
        p.on_ground = false;
        p.jumping_counter = 0;
        p.pycount = ((i32::from(p.pycount) + 32) & 2047) as i16;
        let (sin, _) = sincos_q14(i32::from(p.pycount));
        p.pyoff = sin >> 7;
        if p.jetpack_on < 11 {
            p.jetpack_on += 1;
            p.posz -= i32::from(p.jetpack_on) << 7;
        }
        let shrunk = usize::try_from(p.sprite_i)
            .ok()
            .and_then(|index| sprites.get(index))
            .is_some_and(|sprite| sprite.yrepeat < 32);
        let vertical_step = if shrunk { 512 } else { 2048 };
        if sync.bits & 1 != 0 {
            p.posz -= vertical_step;
        }
        if sync.bits & 2 != 0 {
            p.posz += vertical_step;
        }
        let floor_margin = if !shrunk && matches!(psectlotag, 0 | 2) {
            32
        } else {
            16
        };
        if psectlotag != 2 && p.scuba_on != 0 {
            p.scuba_on = 0;
        }
        let floor_limit = fz - (floor_margin << 8);
        if p.posz > floor_limit {
            p.posz += (floor_limit - p.posz) >> 1;
        }
        if p.posz < cz + (18 << 8) {
            p.posz = cz + (18 << 8);
        }
        return;
    }

    // Non-jetpack path. Chocolate `processinput`: `i = 40`, and in shallow
    // water (`psectlotag == 1 && spritebridge == 0`) the view bobs —
    // `pycount += 32; pyoff = sintable[pycount]>>6` with `i = 34` — or, when
    // shrunk, sits at `i = 12` without bobbing. Room does not track
    // `spritebridge` yet, so standing on a sprite bridge over water still bobs.
    let shrunk = usize::try_from(p.sprite_i)
        .ok()
        .and_then(|index| sprites.get(index))
        .is_some_and(|sprite| sprite.yrepeat < 32);
    let i = if psectlotag == 1 {
        if shrunk {
            12
        } else {
            p.pycount = ((i32::from(p.pycount) + 32) & 2047) as i16;
            let (sin, _) = sincos_q14(i32::from(p.pycount));
            p.pyoff = sin >> 6;
            34
        }
    } else {
        40
    };
    let contact = fz - (i << 8);

    if p.posz < contact {
        // Falling.
        p.on_ground = false;
        p.poszv += DUKE_GC + 80;
        if p.poszv >= 4096 + 2048 {
            p.poszv = 4096 + 2048;
        }
    } else {
        // On ground / water surface.
        p.on_ground = true;
        if i == 40 {
            let mut k = (contact - p.posz) >> 1;
            if k.abs() < 256 {
                k = 0;
            }
            p.posz += k;
            p.poszv -= 768;
            if p.poszv < 0 {
                p.poszv = 0;
            }
        } else if p.jumping_counter == 0 {
            // Chocolate water smooth — keep poszv so a dive can still fire.
            p.posz += ((fz - (i << 7)) - p.posz) >> 1;
            if p.on_warping_sector == 0 && p.posz > fz - (16 << 8) {
                p.posz = fz - (16 << 8);
                p.poszv >>= 1;
            }
        }
        p.on_warping_sector = 0;
        // Crouch pushes through the water surface (dive trigger).
        if sync.bits & 2 != 0 {
            p.posz += 2048 + 768;
        }
    }

    p.posz += p.poszv;

    if p.posz < cz + (4 << 8) {
        p.poszv = 128;
        p.posz = cz + (4 << 8);
    }
}

/// Chocolate `setsprite(ps.i, posx, posy, posz+PHEIGHT)` + bob `xvel`.
///
/// `xvel` is Chocolate `ksqrt((pos-bobpos)²)` used by CON `ifp pwalking` etc.
/// Room's `opos` is the pre-move pose (same role as `bobpos` at this point).
pub fn sync_player_sprite(p: &Player, sprites: &mut [SpriteT]) {
    if p.sprite_i < 0 {
        return;
    }
    let i = p.sprite_i as usize;
    if i >= sprites.len() {
        return;
    }
    let xvel = p.body_xvel;
    let s = &mut sprites[i];
    // Chocolate `setsprite(pi, pos…)` updates x/y/z/sect only — not `ang`.
    // `moveplayers` assigns `s->ang = p->ang` *after* APLAYER `execute` so
    // mid-tic `spawn(WATERBUBBLE)` copies the previous-tic sprite ang.
    s.x = p.posx;
    s.y = p.posy;
    s.z = p.posz + PHEIGHT;
    s.sectnum = p.cursectnum;
    s.xvel = xvel;
    // Chocolate `ifp palive` reads `sprite[ps[g_p].i].extra`.
    s.extra = p.extra;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_action_uses_chocolate_sync_bit_29() {
        assert_eq!(INPUT_OPEN, 1_u32 << 29);
        assert_eq!(INPUT_OPEN & (1 << 3), 0);
    }
    use room_build_engine::types::{SectorT, WallT};

    fn unit_square() -> (Vec<SectorT>, Vec<WallT>) {
        let walls = vec![
            WallT {
                x: 0,
                y: 0,
                point2: 1,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 1024,
                y: 0,
                point2: 2,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 1024,
                y: 1024,
                point2: 3,
                nextsector: -1,
                ..Default::default()
            },
            WallT {
                x: 0,
                y: 1024,
                point2: 0,
                nextsector: -1,
                ..Default::default()
            },
        ];
        let sectors = vec![SectorT {
            wallptr: 0,
            wallnum: 4,
            floorz: 0,
            ceilingz: -(64 << 8),
            ..Default::default()
        }];
        (sectors, walls)
    }

    #[test]
    fn forward_input_moves_pos() {
        let (sectors, walls) = unit_square();
        let mut p = Player {
            posx: 512,
            posy: 512,
            posz: -(40 << 8),
            cursectnum: 0,
            ..Default::default()
        };
        processinput_move(
            &mut p,
            DukeInput {
                fvel: 100,
                svel: 0,
                avel: 0,
                horz: 0,
                bits: 0,
            },
            &sectors,
            &walls,
            &[],
            &[],
            DUKE_FRICTION,
        );
        assert!(p.posx != 512 || p.posy != 512);
    }

    #[test]
    fn avel_turns_ang() {
        let (sectors, walls) = unit_square();
        let mut p = Player {
            posx: 512,
            posy: 512,
            posz: -(40 << 8),
            ang: 0,
            cursectnum: 0,
            ..Default::default()
        };
        processinput_move(
            &mut p,
            DukeInput {
                fvel: 0,
                svel: 0,
                avel: 10,
                horz: 0,
                bits: 0,
            },
            &sectors,
            &walls,
            &[],
            &[],
            DUKE_FRICTION,
        );
        assert_eq!(p.ang, 20); // avel<<1
    }

    #[test]
    fn ceiling_clamp_sets_poszv_128() {
        // Sector ceiling high; face sprite provides cz = posz - 144.
        let (mut sectors, walls) = unit_square();
        sectors[0].ceilingz = -38912;
        sectors[0].floorz = 49152;
        let mut p = Player {
            posx: 512,
            posy: 512,
            posz: -24896,
            poszv: 0,
            cursectnum: 0,
            ..Default::default()
        };
        let sprites = [SpriteT {
            x: 512,
            y: 512,
            z: -24896,
            cstat: 1,
            picnum: 1405,
            clipdist: 64,
            xrepeat: 42,
            yrepeat: 36,
            sectnum: 0,
            ..Default::default()
        }];
        let mut meta = vec![TileZMeta::default(); 1406];
        meta[1405] = TileZMeta {
            width: 40,
            height: 82,
            x_offset: -1,
            y_offset: 1,
        };
        apply_player_fall_z(
            &mut p,
            DukeInput::default(),
            &sectors,
            &walls,
            &sprites,
            &meta,
            None,
        );
        assert_eq!(p.posz, -24016);
        assert_eq!(p.poszv, 128);
    }
}

//! Players and `TicCmd` (GZDoom `player_t` / `usercmd_t` subset).

use crate::actor::ActorId;
use room_core::{ClassId, StateId};

/// Max simultaneous players (storage; netplay targets fewer).
pub const MAX_PLAYERS: usize = 8;

/// Button bits (`BT_*` / GZDoom `Button` subset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub u32);

impl Buttons {
    /// Attack.
    pub const ATTACK: u32 = 1 << 0;
    /// Use.
    pub const USE: u32 = 1 << 1;
    /// Jump.
    pub const JUMP: u32 = 1 << 2;
    /// Speed / run.
    pub const SPEED: u32 = 1 << 3;
    /// Strafe modifier.
    pub const STRAFE: u32 = 1 << 4;
    /// Next owned weapon.
    pub const WEAPONNEXT: u32 = 1 << 5;
    /// Previous owned weapon.
    pub const WEAPONPREV: u32 = 1 << 6;
    /// Duke quick kick (melee without switching away from the ready gun).
    pub const QUICKKICK: u32 = 1 << 7;
    /// Use the carried inventory item (Duke medkit / steroids).
    pub const INVUSE: u32 = 1 << 8;
    /// Alt / secondary attack (`docs/31` MA4).
    pub const ALTATTACK: u32 = 1 << 9;
    /// Mod bind `+mod1` (ping / primary mod action; `docs/28` BE4).
    pub const MOD1: u32 = 1 << 16;
    /// Mod bind `+mod2` (map / secondary mod action).
    pub const MOD2: u32 = 1 << 17;
    /// Mod bind `+mod3`.
    pub const MOD3: u32 = 1 << 18;
    /// Mod bind `+mod4`.
    pub const MOD4: u32 = 1 << 19;

    /// True if any of `mask` is set.
    #[must_use]
    pub const fn any(self, mask: u32) -> bool {
        self.0 & mask != 0
    }
}

/// Per-tic player input (`usercmd_t`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TicCmd {
    /// Button mask.
    pub buttons: Buttons,
    /// Pitch delta (1/65536 turn units).
    pub pitch: i16,
    /// Yaw delta.
    pub yaw: i16,
    /// Roll delta.
    pub roll: i16,
    /// Forward move (Doom units × 2048 scale later).
    pub forward_move: i16,
    /// Side move.
    pub side_move: i16,
    /// Up move (fly / swim).
    pub up_move: i16,
    /// Weapon slot request this tic (`0` = none, `1..=7` = Doom slots).
    pub weapon_select: u8,
}

/// Max tics a mod may hold input capture before auto-release (`docs/27` §9.1 E9).
pub const MAX_CAPTURE_TICS: i32 = 35 * 60;

/// Zero captured parts of `cmd` in place (`docs/27` §9.3).
pub fn apply_input_capture(cmd: &mut TicCmd, mask: u32) {
    use room_script_abi::capture;
    if mask & capture::WEAPON_SELECT != 0 {
        cmd.weapon_select = 0;
    }
    if mask & capture::WEAPON_CYCLE != 0 {
        cmd.buttons.0 &= !(Buttons::WEAPONNEXT | Buttons::WEAPONPREV);
    }
    if mask & capture::ATTACK != 0 {
        cmd.buttons.0 &= !Buttons::ATTACK;
    }
    if mask & capture::USE != 0 {
        cmd.buttons.0 &= !Buttons::USE;
    }
    if mask & capture::MOVE != 0 {
        cmd.forward_move = 0;
        cmd.side_move = 0;
        cmd.up_move = 0;
    }
    if mask & capture::TURN != 0 {
        cmd.yaw = 0;
        cmd.pitch = 0;
        cmd.roll = 0;
    }
    if mask & capture::JUMP != 0 {
        cmd.buttons.0 &= !Buttons::JUMP;
    }
}

/// High-level player life-cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayerState {
    /// Not in game.
    #[default]
    Gone,
    /// Alive and thinking.
    Live,
    /// Dead (corpse view).
    Dead,
    /// Waiting to respawn.
    Reborn,
}

/// One local / net player slot.
#[derive(Clone, Debug, Default)]
pub struct Player {
    /// Player pawn actor.
    pub mo: Option<ActorId>,
    /// This tic's command (filtered by [`Self::input_capture`] before think).
    pub cmd: TicCmd,
    /// Raw command before capture filtering (mods read via `player_cmd`).
    pub raw_cmd: TicCmd,
    /// Mod input-capture mask (`room_script_abi::capture`).
    pub input_capture: u32,
    /// Module index that owns the capture (`u16::MAX` = none).
    pub capture_module: u16,
    /// `maptime` when capture was set (auto-release after [`MAX_CAPTURE_TICS`]).
    pub capture_started: u32,
    /// Previous tic's raw button bits (for pressed/released edges).
    pub prev_buttons: u32,
    /// Life-cycle.
    pub player_state: PlayerState,
    /// Player class.
    pub cls: ClassId,
    /// Eye height (map units).
    pub view_height: f64,
    /// Current eye Z.
    pub view_z: f64,
    /// On ground this tic.
    pub on_ground: bool,
    /// Duke crouch (`up_move` down while grounded).
    pub crouching: bool,
    /// Consecutive falling tics (`DukePlayer` long-fall damage).
    pub falling_tics: u32,
    /// Ready weapon inventory class (if any).
    pub ready_weapon: Option<ClassId>,
    /// Weapon being switched to (`WP_NOCHANGE` = `None` while idle).
    pub pending_weapon: Option<ClassId>,
    /// Weapon psprite screen Y (`WEAPONTOP`=32 … `WEAPONBOTTOM`=128).
    pub pspr_y: f64,
    /// Weapon psprite state (`psp_weapon` / Ready–Fire chain).
    pub pspr_state: Option<StateId>,
    /// Remaining tics on the current psprite frame (`-1` = hold).
    pub pspr_tics: i32,
    /// Muzzle flash psprite (`psp_flash`).
    pub pspr_flash_state: Option<StateId>,
    /// Remaining tics on the flash frame.
    pub pspr_flash_tics: i32,
    /// Weapon muzzle extralight (`A_Light1`/`A_Light2` → 1/2; `A_Light0` → 0).
    pub extralight: i32,
    /// Weapon bob amplitude (map units; from XY speed).
    pub bob: f64,
    /// Last weapon sway X written by `A_WeaponReady` (docs/30 W8.3).
    pub pspr_bob_x: f64,
    /// Last weapon sway Y written by `A_WeaponReady` (docs/30 W8.3).
    pub pspr_bob_y: f64,
    /// Attack button was held last tic (edge detect).
    pub attack_down: bool,
    /// Alt-attack button was held last tic (`docs/31` MA4).
    pub alt_attack_down: bool,
    /// True while the ready weapon is in an alt-fire / alt-hold sequence.
    pub alt_fire_active: bool,
    /// Doom `player->refire` — consecutive `A_ReFire` shots (`P_GunShot` spread).
    pub refire: i32,
    /// Jump button was held last tic (edge detect).
    pub jump_down: bool,
    /// Use button was held last tic (Doom `player->usedown` edge).
    pub use_down: bool,
    /// Inventory-use button was held last tic.
    pub invuse_down: bool,
    /// Quick-kick button was held last tic.
    pub quick_kick_down: bool,
    /// Sim-driven bot: [`crate::bot::run_bot_brains`] overwrites [`Self::cmd`] each tic.
    pub is_bot: bool,
    /// Weapon to restore after a quick kick (`None` = not kicking).
    pub quick_kick_restore: Option<ClassId>,
    /// Consecutive idle tics (no move / fire / use / turn). Resets on input.
    pub idle_tics: i32,
    /// Duke footstep phase (`walking_snd_toggle`).
    pub walk_snd_toggle: bool,
    /// `god` / `iddqd` — ignore damage.
    pub god: bool,
    /// `noclip` — pass through walls (`try_move` skips blocking).
    pub noclip: bool,
    /// Armor points (0..=200 Doom scale).
    pub armor: i32,
    /// Armor save class: `0` none, `1` green (1/3), `2` blue (1/2).
    pub armor_class: u8,
    /// Berserk pack active (fist damage boost; M5 flag).
    pub berserk: bool,
    /// Invulnerability sphere active.
    pub invulnerable: bool,
    /// Remaining invuln tics (`0` = inactive).
    pub invul_tics: i32,
    /// Partial invisibility (BlurSphere).
    pub blur: bool,
    /// Remaining blur tics.
    pub blur_tics: i32,
    /// Radiation suit — ignore damaging floors.
    pub radsuit: bool,
    /// Remaining radsuit tics.
    pub radsuit_tics: i32,
    /// Computer Area Map (Allmap) — full automap reveal.
    pub allmap: bool,
    /// Light amplification goggles (Infrared).
    pub infrared: bool,
    /// Remaining infrared tics.
    pub infrared_tics: i32,
    /// Remaining `PowerFlight` tics (`ArtiFly`); the pawn has `NOGRAVITY` while > 0.
    pub fly_tics: i32,
    /// Remaining Duke steroid tics (faster movement).
    pub steroid_tics: i32,
    /// Duke selected inventory (`inven_icon`: 0 none, 1 kit … 7 boots).
    pub inven_icon: u8,
    /// Inventory-key still held (`toggle_key_flag`).
    pub toggle_key_flag: bool,
    /// Remaining HUD inventory-name tics (`invdisptime`, 35 Hz).
    pub invdisp_tics: u32,
    /// Previous look pitch from a Duke save (`ohoriz` + `ohorizoff`).
    pub prev_pitch: Option<i32>,
    /// Duke footprint shade (`footprintshade`).
    pub footprint_shade: Option<i16>,
    /// Remaining Duke footprints (`footprintcount`).
    pub footprint_count: Option<u8>,
    /// Duke footprint palette (`footprintpal`).
    pub footprint_pal: Option<u8>,
    /// Remaining post-teleport lock tics (`transporter_hold`, 35 Hz).
    pub transporter_hold_tics: u32,
    /// Remaining Duke quote display tics (`fta`, 35 Hz).
    pub fta_tics: u32,
    /// Duke quote index (`ftq`) while `fta_tics` > 0.
    pub ftq: i16,
    /// Remaining hard-land view dip (`hard_landing`).
    pub hard_landing: u8,
    /// Duke view-bob phase (`pycount`, 0..=2047) when restored from a save.
    pub pycount: Option<i16>,
    /// Remaining Duke pain-flash cooldown (`hurt_delay`, 35 Hz).
    pub hurt_delay_tics: u32,
    /// Duke screen tilt (`rotscrnang`, Build ang, decays toward 0).
    pub rotscrnang: i16,
    /// Remaining recenter tics (`return_to_center`, 35 Hz).
    pub return_to_center: u32,
    /// Remaining tip pose tics (`tipincs`, 35 Hz).
    pub tip_incs: u32,
    /// Backpack collected — ammo maxes are doubled.
    pub backpack: bool,
    /// Absolute level time when air runs out (ACS `Get/SetAirSupply`; 0 = unset).
    pub air_finished: i32,
    /// `SetPlayerProperty` PROP_FROZEN / PROP_TOTALLYFROZEN.
    pub frozen: bool,
    /// `SetPlayerProperty` PROP_NOTARGET — monsters ignore this player.
    pub notarget: bool,
    /// `ChangeCamera` view actor (`None` = use `mo`).
    pub camera: Option<ActorId>,
    /// Revert to `mo` on move (`ChangeCamera` revert flag).
    pub camera_revert: bool,
    /// Doom `player->damagecount` — recent damage (0..=100), red flash + face.
    pub damage_count: i32,
    /// Doom `player->bonuscount` — recent pickup, gold flash + evil grin gate.
    pub bonus_count: i32,
    /// Doom `player->attacker` — last damage source (face turn / death cam).
    pub attacker: Option<ActorId>,
    /// Doom `powers[pw_strength]` — tics since berserk pickup (red fade-out).
    pub berserk_tics: i32,
    /// Status-bar face animator (`ST_updateFaceWidget`).
    pub mugshot: crate::mugshot::MugShot,
    /// Doom `player->frags[victim]` — kills of each player slot (own = suicides).
    pub frags: [i32; MAX_PLAYERS],
    /// Damage dealt to other players this match (BE10).
    pub damage_dealt: i32,
    /// Last weapon class ref that damaged this pawn (BE10).
    pub last_damage_weapon: i32,
    /// Squad / team index (`255` = unteamed; `docs/28` BE5).
    pub team: u8,
}

impl Player {
    /// Doom `player->fixedcolormap`: [`INVERSE_COLORMAP`] while invulnerable,
    /// `1` (light amp) with infrared goggles, else `0`. Both blink during the
    /// last 4 seconds (`> 4*32 || & 8`), and invulnerability masks infrared.
    #[must_use]
    pub fn fixed_colormap(&self) -> u8 {
        let on = |tics: i32| tics <= 0 || tics > 4 * 32 || tics & 8 != 0;
        if self.invulnerable {
            if on(self.invul_tics) {
                INVERSE_COLORMAP
            } else {
                0
            }
        } else if self.infrared && on(self.infrared_tics) {
            1
        } else {
            0
        }
    }

    /// Bind a pawn and mark live.
    pub fn possess(&mut self, mo: ActorId, cls: ClassId) {
        self.mo = Some(mo);
        self.cls = cls;
        self.player_state = PlayerState::Live;
        self.view_height = 41.0;
        self.on_ground = true;
        self.crouching = false;
        self.ready_weapon = None;
        self.pending_weapon = None;
        // Off-screen until `P_BringUpWeapon` / Select raise (docs/30 W13).
        self.pspr_y = WEAPON_BOTTOM;
        self.pspr_state = None;
        self.pspr_tics = 0;
        self.pspr_flash_state = None;
        self.pspr_flash_tics = 0;
        self.extralight = 0;
        self.bob = 0.0;
        self.pspr_bob_x = 0.0;
        self.pspr_bob_y = 0.0;
        self.attack_down = false;
        self.alt_attack_down = false;
        self.alt_fire_active = false;
        self.refire = 0;
        self.jump_down = false;
        self.use_down = false;
        self.invuse_down = false;
        self.quick_kick_down = false;
        self.quick_kick_restore = None;
        self.idle_tics = 0;
        self.walk_snd_toggle = false;
        self.god = false;
        self.noclip = false;
        self.armor = 0;
        self.armor_class = 0;
        self.berserk = false;
        self.invulnerable = false;
        self.invul_tics = 0;
        self.blur = false;
        self.blur_tics = 0;
        self.radsuit = false;
        self.radsuit_tics = 0;
        self.allmap = false;
        self.infrared = false;
        self.infrared_tics = 0;
        self.fly_tics = 0;
        self.steroid_tics = 0;
        self.inven_icon = 0;
        self.toggle_key_flag = false;
        self.invdisp_tics = 0;
        self.prev_pitch = None;
        self.footprint_shade = None;
        self.footprint_count = None;
        self.footprint_pal = None;
        self.transporter_hold_tics = 0;
        self.fta_tics = 0;
        self.ftq = 0;
        self.hard_landing = 0;
        self.pycount = None;
        self.hurt_delay_tics = 0;
        self.rotscrnang = 0;
        self.return_to_center = 0;
        self.tip_incs = 0;
        self.backpack = false;
        self.frozen = false;
        self.notarget = false;
        self.camera = None;
        self.camera_revert = false;
        self.damage_count = 0;
        self.bonus_count = 0;
        self.attacker = None;
        self.berserk_tics = 0;
        self.mugshot = crate::mugshot::MugShot::default();
    }
}

/// Doom `INVERSECOLORMAP` — COLORMAP index of the invulnerability greyscale.
pub const INVERSE_COLORMAP: u8 = 32;

/// Doom `BONUSADD` — `bonuscount` added per pickup.
pub const BONUS_ADD: i32 = 6;

/// Ready weapon screen Y (`WEAPONTOP`).
pub const WEAPON_TOP: f64 = 32.0;
/// Holstered weapon screen Y (`WEAPONBOTTOM`).
pub const WEAPON_BOTTOM: f64 = 128.0;
/// Per-tic raise/lower step (`LOWERSPEED` / `RAISESPEED`).
pub const WEAPON_SWITCH_SPEED: f64 = 6.0;

/// Vanilla `TICRATE` (35).
pub const TICRATE: i32 = 35;
/// Invulnerability duration (30 s).
pub const INVUL_TICS: i32 = 30 * TICRATE;
/// BlurSphere / partial invis duration (60 s).
pub const BLUR_TICS: i32 = 60 * TICRATE;
/// Radiation suit duration (60 s).
pub const RADSUIT_TICS: i32 = 60 * TICRATE;
/// `PowerFlight` duration (60 s).
pub const FLIGHT_TICS: i32 = 60 * TICRATE;
/// GZDoom `FRICTION_FLY` — vertical drag while flying with no thrust.
pub const FRICTION_FLY: f64 = 0.917_968_75;
/// Infrared goggles duration (120 s).
pub const INFRARED_TICS: i32 = 120 * TICRATE;

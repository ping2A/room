//! Players and `TicCmd` (GZDoom `player_t` / `usercmd_t` subset).

use crate::actor::ActorId;
use ferrite_core::{ClassId, StateId};

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
    /// This tic's command.
    pub cmd: TicCmd,
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
    /// Attack button was held last tic (edge detect).
    pub attack_down: bool,
    /// Doom `player->refire` — consecutive `A_ReFire` shots (`P_GunShot` spread).
    pub refire: i32,
    /// Jump button was held last tic (edge detect).
    pub jump_down: bool,
    /// Use button was held last tic (Doom `player->usedown` edge).
    pub use_down: bool,
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
}

impl Player {
    /// Bind a pawn and mark live.
    pub fn possess(&mut self, mo: ActorId, cls: ClassId) {
        self.mo = Some(mo);
        self.cls = cls;
        self.player_state = PlayerState::Live;
        self.view_height = 41.0;
        self.on_ground = true;
        self.ready_weapon = None;
        self.pending_weapon = None;
        self.pspr_y = WEAPON_TOP;
        self.pspr_state = None;
        self.pspr_tics = 0;
        self.pspr_flash_state = None;
        self.pspr_flash_tics = 0;
        self.extralight = 0;
        self.bob = 0.0;
        self.attack_down = false;
        self.refire = 0;
        self.jump_down = false;
        self.use_down = false;
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
        self.backpack = false;
        self.frozen = false;
        self.notarget = false;
        self.camera = None;
        self.camera_revert = false;
    }
}

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
/// Infrared goggles duration (120 s).
pub const INFRARED_TICS: i32 = 120 * TICRATE;

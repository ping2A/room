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
    /// Reload (`+reload` / BD KEYCONF R; `docs/33` BD4).
    pub const RELOAD: u32 = 1 << 10;
    /// Zoom / kick (`+zoom` / BD Q; `docs/33` BD4).
    pub const ZOOM: u32 = 1 << 11;
    /// User1 (`+user1` / BD grenade G).
    pub const USER1: u32 = 1 << 12;
    /// User2 (`+user2` / BD unload).
    pub const USER2: u32 = 1 << 13;
    /// User3 (`+user3` / BD dual-wield X).
    pub const USER3: u32 = 1 << 14;
    /// User4 (`+user4` / BD ammo drop).
    pub const USER4: u32 = 1 << 15;
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
    /// Strife hazard accumulator (UDMF `damagehazard` sectors), in tics.
    pub hazard_count: i32,
    /// Userinfo `gender` for `$playersound` lookup: 0 male, 1 female,
    /// 2 neutral, 3 other (object / cyborg). `None` = the `gender` CVar.
    pub gender: Option<u8>,
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
    /// Button mask ACS saw on the previous tic (`INPUT_OLDBUTTONS`).
    pub acs_prev_buttons: u32,
    /// Life-cycle.
    pub player_state: PlayerState,
    /// Player class.
    pub cls: ClassId,
    /// Eye height (map units).
    pub view_height: f64,
    /// Current eye Z.
    pub view_z: f64,
    /// Extra eye height from `SetViewPos` (map units).
    pub view_ofs_z: f64,
    /// On ground this tic.
    pub on_ground: bool,
    /// Duke crouch (`up_move` down while grounded).
    pub crouching: bool,
    /// Consecutive falling tics (`DukePlayer` long-fall damage).
    pub falling_tics: u32,
    /// Ready weapon inventory class (if any).
    pub ready_weapon: Option<ClassId>,
    /// Per-weapon `ReloadCounter` used by ZScript state providers.
    pub reload_counters: Vec<(ClassId, i32)>,
    /// Weapon being switched to (`WP_NOCHANGE` = `None` while idle).
    pub pending_weapon: Option<ClassId>,
    /// Weapon psprite screen Y (`WEAPONTOP`=32 … `WEAPONBOTTOM`=128).
    pub pspr_y: f64,
    /// Weapon psprite state (`psp_weapon` / Ready–Fire chain).
    pub pspr_state: Option<StateId>,
    /// Remaining tics on the current psprite frame (`-1` = hold).
    pub pspr_tics: i32,
    /// Sprite the weapon overlay is actually showing.
    ///
    /// DECORATE `####` does not replace this. The state itself still says `####`.
    pub pspr_sprite: String,
    /// Frame letter index for [`Self::pspr_sprite`].
    pub pspr_frame: u8,
    /// Muzzle flash psprite (`psp_flash`).
    pub pspr_flash_state: Option<StateId>,
    /// Remaining tics on the flash frame.
    pub pspr_flash_tics: i32,
    /// Layer-2 overlay state from `A_Overlay`.
    pub overlay2_state: Option<StateId>,
    /// Remaining tics on the layer-2 overlay frame.
    pub overlay2_tics: i32,
    /// Sprite the layer-2 overlay is showing.
    pub overlay2_sprite: String,
    /// Frame letter index for [`Self::overlay2_sprite`].
    pub overlay2_frame: u8,
    /// Extra screen X for the layer-2 overlay (`A_OverlayOffset`).
    pub overlay2_ofs_x: f64,
    /// Extra screen Y for the layer-2 overlay, above `WEAPONTOP`.
    pub overlay2_ofs_y: f64,
    /// Layer-2 alpha from `A_OverlayAlpha`. Absent means fully opaque.
    pub overlay2_alpha: Option<f64>,
    /// Layer-2 scale from `A_OverlayScale`. Absent means 1×1.
    pub overlay2_scale: Option<(f64, f64)>,
    /// `PSPF_FLIP` on the layer-2 overlay.
    pub overlay2_flip: bool,
    /// `PSPF_MIRROR` on the layer-2 overlay.
    pub overlay2_mirror: bool,
    /// Layer-2 rotation in degrees from `A_OverlayRotate`. 0 is upright.
    pub overlay2_angle: f64,
    /// Layer-2 scale pivot from `A_OverlayPivot`. Absent means the patch center.
    pub overlay2_pivot: Option<(f64, f64)>,
    /// Layer-2 translation from `A_OverlayTranslation`. Empty is untinted.
    pub overlay2_translation: String,
    /// Layer-2 render style from `A_OverlayRenderStyle`. Absent keeps the sprite solid.
    pub overlay2_style: Option<crate::class::RenderStyle>,
    /// `PSPF_PIVOTPERCENT` on the layer-2 overlay. False means the pivot is in pixels.
    pub overlay2_pivot_percent: bool,
    /// `PSPF_ADDBOB` on the layer-2 overlay. False drops weapon sway from that sprite.
    pub overlay2_add_bob: bool,
    /// `PSPF_ADDWEAPON` on the layer-2 overlay. False drops `A_WeaponOffset` from that sprite.
    pub overlay2_add_weapon: bool,
    /// `PSPF_INTERPOLATE` on the layer-2 overlay. False snaps that sprite between tics.
    pub overlay2_interpolate: bool,
    /// `PSPF_FORCEALPHA` on the layer-2 overlay. True keeps that sprite off the body alpha.
    pub overlay2_force_alpha: bool,
    /// `PSPF_FORCESTYLE` on the layer-2 overlay. True ignores player invisibility fuzz.
    pub overlay2_force_style: bool,
    /// `PSPF_CVARFAST` on the layer-2 overlay. True shortens that layer when fast weapons are on.
    pub overlay2_cvar_fast: bool,
    /// `PSPF_POWDOUBLE` on the layer-2 overlay. True halves that layer while double firing speed is held.
    pub overlay2_pow_double: bool,
    /// Weapon muzzle extralight (`A_Light1`/`A_Light2` → 1/2; `A_Light0` → 0).
    pub extralight: i32,
    /// Weapon bob amplitude (map units; from XY speed).
    pub bob: f64,
    /// Last weapon sway X written by `A_WeaponReady` (docs/30 W8.3).
    pub pspr_bob_x: f64,
    /// Last weapon sway Y written by `A_WeaponReady` (docs/30 W8.3).
    pub pspr_bob_y: f64,
    /// Extra screen X from `A_WeaponOffset` (pixels).
    pub weapon_ofs_x: f64,
    /// Extra screen Y from `A_WeaponOffset` (pixels above `WEAPONTOP`).
    pub weapon_ofs_y: f64,
    /// Weapon-layer alpha from `A_OverlayAlpha`. Absent means fully opaque.
    pub weapon_alpha: Option<f64>,
    /// Weapon-layer scale from `A_OverlayScale`. Absent means 1×1.
    pub weapon_scale: Option<(f64, f64)>,
    /// `PSPF_FLIP` on the weapon layer.
    pub weapon_flip: bool,
    /// `PSPF_ADDBOB` (bit 1). True keeps weapon sway on the gun.
    pub weapon_add_bob: bool,
    /// `PSPF_ADDWEAPON` (bit 0). True keeps `A_WeaponOffset` on the gun.
    pub weapon_add_ofs: bool,
    /// `PSPF_CVARFAST` (bit 3). `sv_fastweapons` shortens weapon frames.
    pub weapon_cvar_fast: bool,
    /// `PSPF_POWDOUBLE` (bit 2). `PowerDoubleFiringSpeed` shortens weapon frames.
    pub weapon_pow_double: bool,
    /// `PSPF_MIRROR` (bit 5). Flips the weapon sprite top to bottom.
    pub weapon_mirror: bool,
    /// `PSPF_PIVOTPERCENT` (bit 7). Pivot values are fractions of the sprite.
    pub weapon_pivot_percent: bool,
    /// `PSPF_FORCEALPHA` (bit 10). Overlay alpha ignores the player's translucency.
    pub weapon_force_alpha: bool,
    /// `PSPF_FORCESTYLE` (bit 9). Overlay style ignores the player's fuzz.
    pub weapon_force_style: bool,
    /// `PSPF_INTERPOLATE` (bit 8). Ease the gun offset between tics.
    pub weapon_interpolate: bool,
    /// Scale pivot from `A_OverlayPivot`. Absent means the patch center.
    pub weapon_pivot: Option<(f64, f64)>,
    /// Weapon-layer rotation in degrees from `A_OverlayRotate`. 0 is upright.
    pub weapon_angle: f64,
    /// Weapon-layer render style from `A_OverlayRenderStyle`. Absent keeps the gun solid.
    pub weapon_style: Option<crate::class::RenderStyle>,
    /// Attack button was held last tic (edge detect).
    pub attack_down: bool,
    /// Alt-attack button was held last tic (`docs/31` MA4).
    pub alt_attack_down: bool,
    /// True while the ready weapon is in an alt-fire / alt-hold sequence.
    pub alt_fire_active: bool,
    /// Doom `player->refire` — consecutive `A_ReFire` shots (`P_GunShot` spread).
    pub refire: i32,
    /// `A_ZoomFactor` scale (1.0 = default FOV; BD4 / UI).
    pub weapon_zoom: f32,
    /// `A_SetCrosshair` index (0 = default).
    pub crosshair: i32,
    /// Weapon-layer translation from `A_OverlayTranslation`. Empty is untinted.
    pub weapon_translation: String,
    /// Jump button was held last tic (edge detect).
    pub jump_down: bool,
    /// Use button was held last tic (Doom `player->usedown` edge).
    pub use_down: bool,
    /// The Use press that mounted a usable actor is hidden from ACS until
    /// released, so vehicle input scripts do not immediately treat it as exit.
    pub acs_use_latched: bool,
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
    /// `BasicArmor.SavePercent` as a fraction. `0` = use `armor_class`
    /// (green 33.335 %, blue 50 %), which is what vanilla armor gives.
    pub armor_save_percent: f64,
    /// `Armor.MaxAbsorb` of the worn armor (`0` = no per-tic limit).
    pub armor_max_absorb: i32,
    /// `Armor.MaxFullAbsorb` of the worn armor.
    pub armor_max_full_absorb: i32,
    /// Damage absorbed this tic (`BasicArmor.AbsorbCount`; cleared every tic).
    pub armor_absorb_count: i32,
    /// Falling-scream already started for this fall (`*falling`).
    pub falling_scream: bool,
    /// `BasicArmor.BonusCount`: extra armor cap from `Armor.MaxBonus` items.
    pub armor_bonus_count: i32,
    /// Second `iddt`: things are shown on the automap.
    pub automap_things: bool,
    /// Hexen `poisoncount` (green screen; 1 damage every 16 tics while > 0).
    pub poison_count: i32,
    /// Who poisoned the player (credited for the poison damage).
    pub poisoner: Option<ActorId>,
    /// Generic powerups from `PowerupGiver`s (`Power*` class name, lowercase,
    /// → state). Room's Doom pickups keep their dedicated fields.
    pub powers: std::collections::BTreeMap<String, crate::powerups::PowerState>,
    /// Class that gave the worn armor (`BasicArmor.ArmorType`).
    pub armor_type: Option<ClassId>,
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
    /// `A_SetMugshotState` request for the status bar face (consumed by the HUD).
    pub mugshot_state: Option<String>,
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
    /// Armor save fraction for the damage formula (`BasicArmor.AbsorbDamage`).
    #[must_use]
    pub fn armor_fraction(&self) -> f64 {
        if self.armor_save_percent > 0.0 {
            return self.armor_save_percent;
        }
        match self.armor_class {
            0 => 0.0,
            1 => 0.333_35,
            _ => 0.5,
        }
    }

    /// Drop the worn armor's type and limits (armor points are untouched).
    pub fn clear_armor_kind(&mut self) {
        self.armor_save_percent = 0.0;
        self.armor_max_absorb = 0;
        self.armor_max_full_absorb = 0;
        self.armor_type = None;
    }

    /// Clear the secondary weapon overlay layer (GZDoom `A_ClearOverlays`).
    pub(crate) fn clear_secondary_overlay(&mut self) {
        self.overlay2_state = None;
        self.overlay2_tics = 0;
        self.overlay2_sprite.clear();
        self.overlay2_frame = 0;
        self.overlay2_ofs_x = 0.0;
        self.overlay2_ofs_y = 0.0;
        self.overlay2_alpha = None;
        self.overlay2_scale = None;
        self.overlay2_flip = false;
        self.overlay2_mirror = false;
        self.overlay2_angle = 0.0;
        self.overlay2_pivot = None;
        self.overlay2_translation.clear();
        self.overlay2_style = None;
    }

    /// View FOV after `A_ZoomFactor`. A factor of 2 halves `base_deg`.
    #[must_use]
    pub fn zoomed_fov(base_deg: f64, zoom: f32) -> f64 {
        let z = f64::from(zoom).clamp(0.1, 10.0);
        (base_deg / z).clamp(1.0, 175.0)
    }

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
        self.pspr_sprite.clear();
        self.pspr_frame = 0;
        self.pspr_flash_state = None;
        self.pspr_flash_tics = 0;
        self.overlay2_state = None;
        self.overlay2_tics = 0;
        self.overlay2_sprite.clear();
        self.overlay2_frame = 0;
        self.overlay2_ofs_x = 0.0;
        self.overlay2_ofs_y = 0.0;
        self.overlay2_alpha = None;
        self.overlay2_scale = None;
        self.overlay2_flip = false;
        self.overlay2_mirror = false;
        self.overlay2_angle = 0.0;
        self.overlay2_pivot = None;
        self.overlay2_translation.clear();
        self.overlay2_style = None;
        self.overlay2_pivot_percent = true;
        self.overlay2_add_bob = true;
        self.overlay2_add_weapon = true;
        self.overlay2_interpolate = true;
        self.overlay2_force_alpha = false;
        self.overlay2_force_style = false;
        self.overlay2_cvar_fast = true;
        self.overlay2_pow_double = true;
        self.extralight = 0;
        self.bob = 0.0;
        self.pspr_bob_x = 0.0;
        self.pspr_bob_y = 0.0;
        self.weapon_ofs_x = 0.0;
        self.weapon_ofs_y = 0.0;
        self.weapon_alpha = None;
        self.weapon_scale = None;
        self.weapon_flip = false;
        self.weapon_add_bob = true;
        self.weapon_add_ofs = true;
        self.weapon_cvar_fast = false;
        self.weapon_pow_double = false;
        self.weapon_mirror = false;
        self.weapon_pivot_percent = true;
        self.weapon_force_alpha = false;
        self.weapon_force_style = false;
        self.weapon_interpolate = true;
        self.weapon_pivot = None;
        self.weapon_angle = 0.0;
        self.weapon_style = None;
        self.attack_down = false;
        self.alt_attack_down = false;
        self.alt_fire_active = false;
        self.refire = 0;
        self.weapon_zoom = 1.0;
        self.crosshair = 0;
        self.weapon_translation.clear();
        self.jump_down = false;
        self.use_down = false;
        self.acs_use_latched = false;
        self.invuse_down = false;
        self.quick_kick_down = false;
        self.quick_kick_restore = None;
        self.idle_tics = 0;
        self.walk_snd_toggle = false;
        self.god = false;
        self.noclip = false;
        self.armor = 0;
        self.armor_class = 0;
        self.clear_armor_kind();
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

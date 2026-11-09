//! HUD / status-bar input snapshot (from SimView).

/// Per-frame HUD numbers the status bar needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudState {
    /// Current health.
    pub health: i32,
    /// Armor points.
    pub armor: i32,
    /// Ready ammo count (`SBARINFO` `ammo1` / primary `Weapon.AmmoType`).
    pub ammo: i32,
    /// Max ammo for ready type.
    pub ammo_max: i32,
    /// Secondary / reserve ammo (`SBARINFO` `ammo2` / `Weapon.AmmoType2`).
    pub ammo2: i32,
    /// Max for secondary ammo type.
    pub ammo2_max: i32,
    /// Frag count (DM).
    pub frags: i32,
    /// Keys: bit0 blue, bit1 yellow, bit2 red.
    pub keys: u8,
    /// Mugshot face lump (`STFST00`, …).
    pub face: String,
    /// Ready weapon patch (`PISGA0`, …); empty = hide overlay.
    pub weapon_patch: String,
    /// Muzzle flash patch (`PISFA0`, …); empty = none.
    pub flash_patch: String,
    /// Layer-2 sprite from `A_Overlay`. Empty draws nothing.
    pub overlay_patch: String,
    /// Layer-2 extra screen X.
    pub overlay_ofs_x: f32,
    /// Layer-2 extra screen Y.
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
    /// Weapon offset baked into the gun bob.
    pub bob_ofs_x: f32,
    /// Weapon offset baked into the gun bob, Y.
    pub bob_ofs_y: f32,
    /// Sector light 0–1 for the gun (docs/30 W6).
    pub weapon_light: f32,
    /// Ready/fire frame is `Bright`.
    pub weapon_bright: bool,
    /// Flash frame is `Bright`.
    pub flash_bright: bool,
    /// Partial invisibility → fuzz the psprite.
    pub weapon_fuzz: bool,
    /// Weapon-layer opacity 0–1. 1 until `A_OverlayAlpha`.
    pub weapon_alpha: f32,
    /// Weapon-layer scale. 1 until `A_OverlayScale`.
    pub weapon_scale_x: f32,
    /// Weapon-layer scale Y.
    pub weapon_scale_y: f32,
    /// `PSPF_FLIP` on the weapon layer.
    pub weapon_flip: bool,
    /// `PSPF_MIRROR` on the weapon layer.
    pub weapon_mirror: bool,
    /// `PSPF_PIVOTPERCENT`. False means the pivot is in sprite pixels.
    pub weapon_pivot_percent: bool,
    /// `PSPF_INTERPOLATE`. False snaps the gun offset each tic.
    pub weapon_interpolate: bool,
    /// Scale pivot. 0.5 is the patch center until `A_OverlayPivot`.
    pub weapon_pivot_x: f32,
    /// Scale pivot Y.
    pub weapon_pivot_y: f32,
    /// Weapon-layer rotation in degrees. 0 until `A_OverlayRotate`.
    pub weapon_angle: f32,
    /// Ready weapon class (`MODELDEF` `ViewModelFOV`).
    pub weapon_class: String,
    /// Pawn display name (SBARINFO `PlayerClass`).
    pub player_class: String,
    /// Worn armor class (SBARINFO `ArmorType`).
    pub armor_type: String,
    /// `A_SetCrosshair` index. 0 draws no mark.
    pub crosshair: i32,
    /// Weapon tint red. 1 until `A_OverlayTranslation`.
    pub weapon_tint_r: f32,
    /// Weapon tint green.
    pub weapon_tint_g: f32,
    /// Weapon tint blue.
    pub weapon_tint_b: f32,
    /// Weapon bob X (Doom 320 screen space).
    pub bob_x: f32,
    /// Weapon bob Y.
    pub bob_y: f32,
    /// Screenblocks-style: true = fullscreen HUD (no STBAR strip).
    pub fullscreen: bool,
    /// Owned weapon slots (bit `n` = slot `n`, 2..=9; `STARMS` shows 2..=7).
    pub arms: u16,
    /// `[count, max]` for Clip / Shell / RocketAmmo / Cell.
    pub ammo_table: [[i32; 2]; 4],
    /// PLAYPAL flash index (0 none, 1..=8 red, 9..=12 gold, 13 green, 14 underwater).
    pub palette: u8,
    /// Doom `fixedcolormap` (32 = invulnerability: invert the weapon too).
    pub colormap: u8,
    /// Deathmatch: frags replace the `STARMS` panel.
    pub deathmatch: bool,
    /// Netgame face background `STFB<n>`; `None` in single player.
    pub face_back: Option<u8>,
}

/// Doom PLAYPAL flash palettes as a full-screen blend: red tints in 1/9
/// steps, pickup gold in 1/8 steps, radiation-suit green at 1/8.
#[must_use]
pub fn screen_blend(palette: u8) -> Option<crate::draw::Rgba> {
    use crate::draw::Rgba;
    match palette {
        // A full PLAYPAL step mixed as straight red at 8/9 hides the level.
        // Keep a readable tint (palette 8 ≈ 0.32).
        1..=8 => Some(Rgba::new(1.0, 0.0, 0.0, f32::from(palette) / 25.0)),
        9..=12 => Some(Rgba::new(
            215.0 / 255.0,
            186.0 / 255.0,
            69.0 / 255.0,
            f32::from(palette - 8) / 8.0,
        )),
        13 => Some(Rgba::new(0.0, 1.0, 0.0, 0.125)),
        14 => Some(Rgba::new(0.0, 0.25, 0.7, 0.28)),
        _ => None,
    }
}

impl HudState {
    /// Doom defaults for a fresh player.
    #[must_use]
    pub fn doom_default() -> Self {
        Self {
            health: 100,
            armor: 0,
            ammo: 50,
            ammo_max: 200,
            ammo2: 0,
            ammo2_max: 0,
            frags: 0,
            keys: 0,
            face: "STFST00".into(),
            weapon_patch: "PISGA0".into(),
            flash_patch: String::new(),
            overlay_patch: String::new(),
            overlay_ofs_x: 0.0,
            overlay_ofs_y: 0.0,
            overlay_alpha: 1.0,
            overlay_scale_x: 1.0,
            overlay_scale_y: 1.0,
            overlay_flip: false,
            overlay_mirror: false,
            overlay_angle: 0.0,
            overlay_pivot_x: 0.5,
            overlay_pivot_y: 0.5,
            overlay_tint_r: 1.0,
            overlay_tint_g: 1.0,
            overlay_tint_b: 1.0,
            overlay_fuzz: false,
            overlay_pivot_percent: true,
            overlay_add_bob: true,
            overlay_add_weapon: true,
            overlay_interpolate: true,
            bob_ofs_x: 0.0,
            bob_ofs_y: 0.0,
            weapon_light: 1.0,
            weapon_bright: false,
            flash_bright: false,
            weapon_fuzz: false,
            weapon_alpha: 1.0,
            weapon_scale_x: 1.0,
            weapon_scale_y: 1.0,
            weapon_flip: false,
            weapon_mirror: false,
            weapon_pivot_percent: true,
            weapon_interpolate: true,
            weapon_pivot_x: 0.5,
            weapon_pivot_y: 0.5,
            weapon_angle: 0.0,
            weapon_class: String::new(),
            player_class: String::new(),
            armor_type: String::new(),
            crosshair: 0,
            weapon_tint_r: 1.0,
            weapon_tint_g: 1.0,
            weapon_tint_b: 1.0,
            bob_x: 0.0,
            bob_y: 0.0,
            // Classic STBAR strip. F5 toggles overlay numbers (no bar).
            fullscreen: false,
            arms: 1 << 2,
            ammo_table: [[50, 200], [0, 50], [0, 50], [0, 300]],
            palette: 0,
            colormap: 0,
            deathmatch: false,
            face_back: None,
        }
    }

    /// Build from shell-facing player numbers (e.g. `World::player_hud`).
    #[must_use]
    pub fn from_player(
        health: i32,
        armor: i32,
        ammo: i32,
        keys: u8,
        face: impl Into<String>,
        weapon_patch: impl Into<String>,
        fullscreen: bool,
    ) -> Self {
        Self {
            health,
            armor,
            ammo,
            ammo_max: 0,
            ammo2: 0,
            ammo2_max: 0,
            frags: 0,
            keys,
            face: face.into(),
            weapon_patch: weapon_patch.into(),
            flash_patch: String::new(),
            overlay_patch: String::new(),
            overlay_ofs_x: 0.0,
            overlay_ofs_y: 0.0,
            overlay_alpha: 1.0,
            overlay_scale_x: 1.0,
            overlay_scale_y: 1.0,
            overlay_flip: false,
            overlay_mirror: false,
            overlay_angle: 0.0,
            overlay_pivot_x: 0.5,
            overlay_pivot_y: 0.5,
            overlay_tint_r: 1.0,
            overlay_tint_g: 1.0,
            overlay_tint_b: 1.0,
            overlay_fuzz: false,
            overlay_pivot_percent: true,
            overlay_add_bob: true,
            overlay_add_weapon: true,
            overlay_interpolate: true,
            bob_ofs_x: 0.0,
            bob_ofs_y: 0.0,
            weapon_light: 1.0,
            weapon_bright: false,
            flash_bright: false,
            weapon_fuzz: false,
            weapon_alpha: 1.0,
            weapon_scale_x: 1.0,
            weapon_scale_y: 1.0,
            weapon_flip: false,
            weapon_mirror: false,
            weapon_pivot_percent: true,
            weapon_interpolate: true,
            weapon_pivot_x: 0.5,
            weapon_pivot_y: 0.5,
            weapon_angle: 0.0,
            weapon_class: String::new(),
            player_class: String::new(),
            armor_type: String::new(),
            crosshair: 0,
            weapon_tint_r: 1.0,
            weapon_tint_g: 1.0,
            weapon_tint_b: 1.0,
            bob_x: 0.0,
            bob_y: 0.0,
            fullscreen,
            arms: 0,
            ammo_table: [[0; 2]; 4],
            palette: 0,
            colormap: 0,
            deathmatch: false,
            face_back: None,
        }
    }

    /// Attach netgame state: deathmatch frags and the `STFB<n>` face background.
    #[must_use]
    pub fn with_netgame(mut self, deathmatch: bool, frags: i32, face_back: Option<u8>) -> Self {
        self.deathmatch = deathmatch;
        self.frags = frags;
        self.face_back = face_back;
        self
    }

    /// Attach Doom's `fixedcolormap` (see [`Self::colormap`]).
    #[must_use]
    pub fn with_colormap(mut self, colormap: u8) -> Self {
        self.colormap = colormap;
        self
    }

    /// True while the invulnerability inverse colormap is active.
    #[must_use]
    pub fn inverse_colormap(&self) -> bool {
        self.colormap == 32
    }

    /// Attach the `STARMS` weapon bits and the four-row ammo table.
    #[must_use]
    pub fn with_inventory(mut self, arms: u16, ammo_table: [[i32; 2]; 4]) -> Self {
        self.arms = arms;
        self.ammo_table = ammo_table;
        self
    }

    /// Attach the PLAYPAL flash index (see [`screen_blend`]).
    #[must_use]
    pub fn with_palette(mut self, palette: u8) -> Self {
        self.palette = palette;
        self
    }

    /// Attach a muzzle-flash patch lump.
    #[must_use]
    pub fn with_flash(mut self, flash_patch: impl Into<String>) -> Self {
        self.flash_patch = flash_patch.into();
        self
    }

    /// Attach weapon bob offsets.
    #[must_use]
    pub fn with_bob(mut self, bob_x: f32, bob_y: f32) -> Self {
        self.bob_x = bob_x;
        self.bob_y = bob_y;
        self
    }

    /// Set ready-ammo capacity for the status bar.
    #[must_use]
    pub fn with_ammo_max(mut self, ammo_max: i32) -> Self {
        self.ammo_max = ammo_max;
        self
    }

    /// Set secondary / reserve ammo (`ammo2` / `ammo2capacity`).
    #[must_use]
    pub fn with_ammo2(mut self, ammo2: i32, ammo2_max: i32) -> Self {
        self.ammo2 = ammo2;
        self.ammo2_max = ammo2_max;
        self
    }
}

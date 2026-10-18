//! HUD / status-bar input snapshot (from SimView).

/// Per-frame HUD numbers the status bar needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudState {
    /// Current health.
    pub health: i32,
    /// Armor points.
    pub armor: i32,
    /// Ready ammo count.
    pub ammo: i32,
    /// Max ammo for ready type.
    pub ammo_max: i32,
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
        1..=8 => Some(Rgba::new(1.0, 0.0, 0.0, f32::from(palette) / 9.0)),
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
            frags: 0,
            keys: 0,
            face: "STFST00".into(),
            weapon_patch: "PISGA0".into(),
            flash_patch: String::new(),
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
            frags: 0,
            keys,
            face: face.into(),
            weapon_patch: weapon_patch.into(),
            flash_patch: String::new(),
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
}

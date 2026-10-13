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
        }
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

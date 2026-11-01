//! Balance + naming constants (`docs/32` §6).

/// Sprite lump names generated in SK0 (order is fingerprint-stable).
pub const SPRITE_LUMPS: &[&str] = &[
    "SHKMA0", "SHKMB0", "SHKMC0", "SHKMD0", // flying spin
    "SHKSA0",                               // stuck
    "SHKPA0",                               // pickup star
    "SHKBA0",                               // ammo box
    "SHKWA0",                               // weapon pickup
    "SHKGA0", "SHKGB0", "SHKGC0", "SHKGD0", "SHKGE0", // psprite
    "SHKIA0",                               // HUD icon
];

/// Spin frame table angles (0°, 22.5°, 45°, 67.5°) in 256-circle units.
pub const SPIN_ANGLES: [u8; 4] = [0, 16, 32, 48];

/// Logical sound → lump (`docs/32` §3.4).
pub const SOUND_ALIASES: &[(&str, &str)] = &[
    ("shuriken/throw", "DSSHKTHR"),
    ("shuriken/clink", "DSSHKCLK"),
    ("shuriken/flesh", "DSSHKFLS"),
    ("shuriken/pickup", "DSSHKPKP"),
    ("shuriken/box", "DSSHKBOX"),
    ("shuriken/up", "DSSHKUP"),
];

/// DMX sample rate.
pub const SOUND_RATE: u16 = 11_025;

/// Missile / weapon balance.
pub const MISSILE_SPEED: i32 = 30;
pub const MISSILE_RADIUS: i32 = 6;
pub const MISSILE_HEIGHT: i32 = 6;
pub const MISSILE_DAMAGE: i32 = 3;
pub const AMMO_MAX: i32 = 30;
pub const AMMO_BACKPACK_MAX: i32 = 60;
pub const AMMO_BACKPACK_GIVE: i32 = 10;
pub const WEAPON_AMMO_GIVE: i32 = 10;
pub const BOX_AMOUNT: i32 = 10;
pub const SELECTION_ORDER: i32 = 1600;
pub const KICKBACK: i32 = 50;
pub const DEFAULT_SLOT: i32 = 8;
pub const STUCK_LIFETIME_TICS: i32 = 35 * 60;
pub const STUCK_MAX: usize = 32;
pub const IMPACT_CALLBACK: i32 = 1;

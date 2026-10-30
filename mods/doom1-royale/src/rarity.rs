//! Rarity tiers for world loot (common / rare / epic / legendary).

use room_script_sdk::ClassRef;

/// Loot rarity (higher = stronger pickup feedback).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Rarity {
    Common = 0,
    Rare = 1,
    Epic = 2,
    Legendary = 3,
}

impl Rarity {
    pub const LABELS: [&'static str; 4] = ["COMMON", "RARE", "EPIC", "LEGENDARY"];

    #[must_use]
    pub fn label(self) -> &'static str {
        Self::LABELS[self as usize]
    }

    /// Deterministic roll from `0..100`.
    #[must_use]
    pub fn roll(roll: i32) -> Self {
        if roll >= 98 {
            Self::Legendary
        } else if roll >= 90 {
            Self::Epic
        } else if roll >= 70 {
            Self::Rare
        } else {
            Self::Common
        }
    }
}

/// Per-base-item rarity class names (index = [`crate::tables::Item`] discriminant).
pub const RARITY_NAMES: [[&str; 4]; 15] = [
    ["Shotgun", "RareShotgun", "EpicShotgun", "LegendaryShotgun"],
    ["Chaingun", "RareChaingun", "EpicChaingun", "LegendaryChaingun"],
    [
        "RocketLauncher",
        "RareRocketLauncher",
        "EpicRocketLauncher",
        "LegendaryRocketLauncher",
    ],
    ["Chainsaw", "Chainsaw", "Chainsaw", "Chainsaw"],
    ["Shell", "Shell", "ShellBox", "ShellBox"],
    ["ShellBox", "ShellBox", "ShellBox", "ShellBox"],
    ["Clip", "Clip", "ClipBox", "ClipBox"],
    ["ClipBox", "ClipBox", "ClipBox", "ClipBox"],
    ["RocketAmmo", "RocketAmmo", "RocketAmmo", "RocketAmmo"],
    ["Stimpack", "Stimpack", "Medikit", "Medikit"],
    ["Medikit", "Medikit", "Medikit", "Medikit"],
    [
        "GreenArmor",
        "RareGreenArmor",
        "EpicGreenArmor",
        "LegendaryGreenArmor",
    ],
    ["ArmorBonus", "ArmorBonus", "GreenArmor", "RareGreenArmor"],
    ["HealthBonus", "HealthBonus", "Stimpack", "Medikit"],
    ["Berserk", "Berserk", "Berserk", "Berserk"],
];

/// Resolved class for an item + rarity (filled at load from [`RARITY_NAMES`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct RarityTable {
    pub classes: [[ClassRef; 4]; 15],
}

impl RarityTable {
    pub fn resolve<H: crate::host::Host>(&mut self, host: &H) {
        for (item_i, names) in RARITY_NAMES.iter().enumerate() {
            for (r_i, name) in names.iter().enumerate() {
                self.classes[item_i][r_i] = host.class_find(name);
            }
        }
    }

    #[must_use]
    pub fn class_for(&self, item: crate::tables::Item, rarity: Rarity) -> ClassRef {
        self.classes[item as usize][rarity as usize]
    }
}

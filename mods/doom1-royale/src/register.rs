//! Runtime class registration for loot 2.0 (rarity + supply crate).

use room_script_sdk::game::class_prop;
use room_script_sdk::{class_find, reg, ClassRef};

fn register_class(name: &str, parent: &str, props: &[(u32, i32)]) {
    let parent = class_find(parent);
    let _ = reg::class_begin(name, parent, ClassRef::NULL, -1, 0);
    for &(p, v) in props {
        reg::class_property_i32(p, v);
    }
    reg::class_end();
}

/// Register supply crate + rarity subclasses (Track B DECORATE optional).
pub fn register_loot_classes() {
    register_class(
        "SupplyCrate",
        "ShortGreenColumn",
        &[
            (class_prop::HEALTH, 10_000),
            (class_prop::SCALE_PERCENT, 140),
        ],
    );
    for (name, parent, scale) in [
        ("RareShotgun", "Shotgun", 110),
        ("EpicShotgun", "Shotgun", 118),
        ("LegendaryShotgun", "Shotgun", 128),
        ("RareChaingun", "Chaingun", 110),
        ("EpicChaingun", "Chaingun", 118),
        ("LegendaryChaingun", "Chaingun", 128),
        ("RareRocketLauncher", "RocketLauncher", 110),
        ("EpicRocketLauncher", "RocketLauncher", 118),
        ("LegendaryRocketLauncher", "RocketLauncher", 128),
        ("RareGreenArmor", "GreenArmor", 115),
        ("EpicGreenArmor", "GreenArmor", 125),
        ("LegendaryGreenArmor", "GreenArmor", 135),
    ] {
        register_class(name, parent, &[(class_prop::SCALE_PERCENT, scale)]);
    }
}

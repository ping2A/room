//! Mod class registration (`PhantomImp`, `Juggernaut`, elite subclasses).

use room_script_sdk::game::class_prop;
use room_script_sdk::{class_find, reg, ClassRef};

/// Register a DECORATE-style class with integer properties.
pub fn register_class(name: &str, parent: &str, props: &[(u32, i32)]) {
    let parent = class_find(parent);
    let _ = reg::class_begin(name, parent, ClassRef::NULL, -1, 0);
    for &(p, v) in props {
        reg::class_property_i32(p, v);
    }
    reg::class_end();
}

/// Register the SV0 stock classes + SV3 elite subclasses.
pub fn register_stock_classes() {
    register_class(
        "PhantomImp",
        "DoomImp",
        &[
            (class_prop::HEALTH, 80),
            (class_prop::SPEED, 12),
            (class_prop::ALPHA_PERCENT, 55),
        ],
    );
    register_class(
        "Juggernaut",
        "Demon",
        &[
            (class_prop::HEALTH, 400),
            (class_prop::SPEED, 12),
            (class_prop::SCALE_PERCENT, 130),
            (class_prop::RADIUS, 36),
            (class_prop::HEIGHT, 72),
            (class_prop::MASS, 1000),
        ],
    );
    // Elite subclasses — Track B can attach GLDEFS / DECORATE by these names.
    register_class(
        "EliteZombie",
        "ZombieMan",
        &[
            (class_prop::HEALTH, 40),
            (class_prop::SCALE_PERCENT, 110),
        ],
    );
    register_class(
        "EliteImp",
        "DoomImp",
        &[
            (class_prop::HEALTH, 100),
            (class_prop::SCALE_PERCENT, 115),
        ],
    );
    register_class(
        "EliteDemon",
        "Demon",
        &[
            (class_prop::HEALTH, 250),
            (class_prop::SCALE_PERCENT, 120),
        ],
    );
    register_class(
        "Warden",
        "BaronOfHell",
        &[
            (class_prop::HEALTH, 1200),
            (class_prop::SCALE_PERCENT, 120),
            (class_prop::SPEED, 10),
        ],
    );
}

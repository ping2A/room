//! Class registration (`docs/32` §3.5) — parents `Clip` / `Pistol`.

use room_script_abi::{class_flag, class_prop, state_flow_kind};
use room_script_sdk::game::class_property_str;
use room_script_sdk::{class_find, reg, ClassRef};

use crate::tables::{
    AMMO_BACKPACK_GIVE, AMMO_BACKPACK_MAX, AMMO_MAX, BOX_AMOUNT, DEFAULT_SLOT, IMPACT_CALLBACK,
    KICKBACK, MISSILE_DAMAGE, MISSILE_HEIGHT, MISSILE_RADIUS, MISSILE_SPEED, SELECTION_ORDER,
    STUCK_LIFETIME_TICS, WEAPON_AMMO_GIVE,
};

fn sprite4(name: &[u8; 4]) -> [u8; 4] {
    *name
}

fn state_stop() {
    let _ = reg::state_flow(state_flow_kind::STOP, "", 0);
}

fn state_loop() {
    let _ = reg::state_flow(state_flow_kind::LOOP, "", 0);
}

fn state_goto(label: &str) {
    let _ = reg::state_flow(state_flow_kind::GOTO, label, 0);
}

fn register_ammo() {
    let parent = class_find("Clip");
    let _ = reg::class_begin("ShurikenAmmo", parent, ClassRef::NULL, -1, 0);
    reg::class_property_i32(class_prop::INVENTORY_AMOUNT, 1);
    reg::class_property_i32(class_prop::INVENTORY_MAX_AMOUNT, AMMO_MAX);
    reg::class_property_i32(class_prop::AMMO_BACKPACK_AMOUNT, AMMO_BACKPACK_GIVE);
    reg::class_property_i32(class_prop::AMMO_BACKPACK_MAX_AMOUNT, AMMO_BACKPACK_MAX);
    class_property_str(class_prop::INVENTORY_ICON, "SHKIA0");
    class_property_str(class_prop::INVENTORY_PICKUP_MESSAGE, "Picked up a shuriken.");
    class_property_str(class_prop::INVENTORY_PICKUP_SOUND, "shuriken/pickup");
    reg::state_label("Spawn");
    let _ = reg::state(&sprite4(b"SHKP"), b'A', -1, 0, -1);
    state_stop();
    reg::class_end();
}

fn register_box() {
    let parent = class_find("ShurikenAmmo");
    let _ = reg::class_begin("ShurikenBox", parent, ClassRef::NULL, -1, 0);
    reg::class_property_i32(class_prop::INVENTORY_AMOUNT, BOX_AMOUNT);
    class_property_str(
        class_prop::INVENTORY_PICKUP_MESSAGE,
        "Picked up a box of shurikens.",
    );
    class_property_str(class_prop::INVENTORY_PICKUP_SOUND, "shuriken/box");
    reg::state_label("Spawn");
    let _ = reg::state(&sprite4(b"SHKB"), b'A', -1, 0, -1);
    state_stop();
    reg::class_end();
}

fn register_stuck() {
    let parent = class_find("ShurikenAmmo");
    let _ = reg::class_begin("ShurikenStuck", parent, ClassRef::NULL, -1, 0);
    reg::class_property_i32(class_prop::INVENTORY_AMOUNT, 1);
    reg::class_flag(class_flag::NOGRAVITY, true);
    reg::state_label("Spawn");
    let _ = reg::state(&sprite4(b"SHKS"), b'A', STUCK_LIFETIME_TICS, 0, -1);
    state_stop();
    reg::class_end();

    let parent = class_find("ShurikenAmmo");
    let _ = reg::class_begin("ShurikenDropped", parent, ClassRef::NULL, -1, 0);
    reg::class_property_i32(class_prop::INVENTORY_AMOUNT, 1);
    reg::state_label("Spawn");
    let _ = reg::state(&sprite4(b"SHKS"), b'A', STUCK_LIFETIME_TICS, 0, -1);
    state_stop();
    reg::class_end();
}

fn register_weapon(slot: i32) {
    let parent = class_find("Pistol");
    let _ = reg::class_begin("Shuriken", parent, ClassRef::NULL, -1, 0);
    class_property_str(class_prop::TAG, "Shuriken");
    class_property_str(
        class_prop::OBITUARY,
        "%o was diced by %k's shuriken.",
    );
    class_property_str(class_prop::WEAPON_AMMO_TYPE, "ShurikenAmmo");
    reg::class_property_i32(class_prop::WEAPON_AMMO_USE, 1);
    reg::class_property_i32(class_prop::WEAPON_AMMO_GIVE, WEAPON_AMMO_GIVE);
    reg::class_property_i32(class_prop::WEAPON_SELECTION_ORDER, SELECTION_ORDER);
    reg::class_property_i32(class_prop::WEAPON_SLOT_NUMBER, slot);
    reg::class_property_i32(class_prop::WEAPON_KICKBACK, KICKBACK);
    class_property_str(class_prop::WEAPON_UP_SOUND, "shuriken/up");
    reg::class_flag(class_flag::WEAPON_NOALERT, true);
    class_property_str(
        class_prop::INVENTORY_PICKUP_MESSAGE,
        "You got the shurikens!",
    );

    reg::state_label("Spawn");
    let _ = reg::state(&sprite4(b"SHKW"), b'A', -1, 0, -1);
    state_stop();

    reg::state_label("Ready");
    let _ = reg::state(&sprite4(b"SHKG"), b'A', 1, 0, -1);
    let _ = reg::state_action("A_WeaponReady", "");
    state_loop();

    reg::state_label("Deselect");
    let _ = reg::state(&sprite4(b"SHKG"), b'A', 1, 0, -1);
    let _ = reg::state_action("A_Lower", "");
    state_loop();

    reg::state_label("Select");
    let _ = reg::state(&sprite4(b"SHKG"), b'A', 1, 0, -1);
    let _ = reg::state_action("A_Raise", "");
    state_loop();

    reg::state_label("Fire");
    let _ = reg::state(&sprite4(b"SHKG"), b'B', 4, 0, -1);
    let _ = reg::state(&sprite4(b"SHKG"), b'C', 2, 0, -1);
    let _ = reg::state_action("A_FireProjectile", "\"ShurikenMissile\", 0, 1");
    let _ = reg::state(&sprite4(b"SHKG"), b'C', 0, 0, -1);
    let _ = reg::state_action("A_StartSound", "\"shuriken/throw\"");
    let _ = reg::state(&sprite4(b"SHKG"), b'D', 5, 0, -1);
    let _ = reg::state(&sprite4(b"SHKG"), b'A', 4, 0, -1);
    let _ = reg::state_action("A_ReFire", "");
    state_goto("Ready");

    reg::state_label("AltFire");
    let _ = reg::state(&sprite4(b"SHKG"), b'E', 6, 0, -1);
    let _ = reg::state(&sprite4(b"SHKG"), b'C', 2, 0, -1);
    let _ = reg::state_action("A_FireProjectile", "\"ShurikenMissile\", -6, 1");
    let _ = reg::state(&sprite4(b"SHKG"), b'C', 0, 0, -1);
    let _ = reg::state_action("A_FireProjectile", "\"ShurikenMissile\", 0, 1");
    let _ = reg::state(&sprite4(b"SHKG"), b'C', 0, 0, -1);
    let _ = reg::state_action("A_FireProjectile", "\"ShurikenMissile\", 6, 1");
    let _ = reg::state(&sprite4(b"SHKG"), b'C', 0, 0, -1);
    let _ = reg::state_action("A_StartSound", "\"shuriken/throw\"");
    let _ = reg::state(&sprite4(b"SHKG"), b'D', 8, 0, -1);
    let _ = reg::state(&sprite4(b"SHKG"), b'A', 8, 0, -1);
    let _ = reg::state(&sprite4(b"SHKG"), b'A', 6, 0, -1);
    let _ = reg::state_action("A_ReFire", "");
    state_goto("Ready");

    reg::class_end();
}

fn register_missile() {
    let parent = class_find("Actor");
    let _ = reg::class_begin("ShurikenMissile", parent, ClassRef::NULL, -1, 0);
    reg::class_property_i32(class_prop::RADIUS, MISSILE_RADIUS);
    reg::class_property_i32(class_prop::HEIGHT, MISSILE_HEIGHT);
    reg::class_property_i32(class_prop::SPEED, MISSILE_SPEED);
    reg::class_property_i32(class_prop::DAMAGE, MISSILE_DAMAGE);
    reg::class_flag(class_flag::MISSILE, true);
    reg::class_flag(class_flag::NOGRAVITY, true);
    reg::class_flag(class_flag::NOBLOCKMAP, true);
    reg::class_flag(class_flag::DROPOFF, true);
    reg::class_flag(class_flag::NOTELEPORT, true);
    reg::class_flag(class_flag::FORCEXYBILLBOARD, true);
    class_property_str(class_prop::SEE_SOUND, "");
    class_property_str(class_prop::DEATH_SOUND, "");

    reg::state_label("Spawn");
    let _ = reg::state(&sprite4(b"SHKM"), b'A', 2, 0, -1);
    let _ = reg::state(&sprite4(b"SHKM"), b'B', 2, 0, -1);
    let _ = reg::state(&sprite4(b"SHKM"), b'C', 2, 0, -1);
    let _ = reg::state(&sprite4(b"SHKM"), b'D', 2, 0, -1);
    state_loop();

    reg::state_label("Death");
    let _ = reg::state(&sprite4(b"TNT1"), b'A', 0, 0, IMPACT_CALLBACK);
    state_stop();

    reg::state_label("XDeath");
    let _ = reg::state(&sprite4(b"TNT1"), b'A', 0, 0, IMPACT_CALLBACK);
    state_stop();

    reg::class_end();
}

/// Register all shuriken classes + weapon slot.
pub fn register() {
    let slot = reg::option_i32("slot", DEFAULT_SLOT);
    register_ammo();
    register_box();
    register_stuck();
    register_weapon(slot);
    register_missile();
    let _ = reg::weapon_slot(slot, "Shuriken", 0);
}

//! Brutal Doom's ZScript, hand-ported to a Room WASM mod.
//!
//! Brutal Doom ships three small ZScript files (`zscript.zc` includes
//! the other two). This crate is the same code in Rust:
//!
//! | ZScript (`zscript/…`)                | Rust                     |
//! |--------------------------------------|--------------------------|
//! | `DamageHandler.zc` `Script_DamageMult` | [`damage::player_entered`], [`damage::thing_spawned`] |
//! | `DamageHandler.zc` `Damage_Control`  | [`damage::modify_damage`] |
//! | `BD_WCS.zc` `ZandroCompat`           | [`zandro`]               |
//! | `BD_WCS.zc` `ZC_TMapHack`, `ZC_IntStrMap` | [`lumps::LumpTable`] |
//!
//! `room.toml` lists those files under `replaces_zscript`, so Room drops
//! them from Brutal Doom when this mod is mounted after it: the ACS
//! `ScriptCall("ZandroCompat", …)` calls, the skill-6 damage token and its
//! `ModifyDamage` then all run here.
//!
//! To change the mod, edit the functions in `damage.rs` / `zandro.rs`, run
//! `cargo test`, then build the module (see `README.md`).

pub mod damage;
pub mod host;
pub mod lumps;
pub mod zandro;

#[cfg(test)]
mod tests;

use host::SdkHost;
use room_script_sdk::game::{
    Event, ModifyDamage, RoomMod, STATIC_CALLBACK_BASE, StaticCall,
    VIRTUAL_CALLBACK_BASE, class_flag, class_prop, event_mask, virtual_slot,
};
use room_script_sdk::{ClassRef, class_find, reg};

/// The whole port: one event handler, the token class, the static functions.
#[derive(Default)]
pub struct BrutalZScript {
    /// `ZC_TMapHack`'s tables.
    lumps: lumps::LumpTable,
}

/// `ModifyDamage` virtual index of `Damage_Control`.
const DAMAGE_CONTROL_VIRTUAL: i32 = 0;

impl RoomMod for BrutalZScript {
    const NAME: &'static str = "Script_DamageMult";
    const EVENTS: u64 = event_mask::PLAYER_ENTERED | event_mask::WORLD_THING_SPAWNED;

    fn register(&mut self) {
        register_damage_control();
        for (i, f) in zandro::Func::ALL.iter().enumerate() {
            reg::static_function(zandro::CLASS, f.name(), STATIC_CALLBACK_BASE + i as i32);
        }
    }

    fn on_event(&mut self, event: Event) {
        let host = &mut SdkHost;
        match event {
            Event::PlayerEntered { player, .. } => damage::player_entered(host, player),
            Event::ThingSpawned { thing } => damage::thing_spawned(host, thing),
            _ => {}
        }
    }

    fn on_static_call(&mut self, index: i32, call: StaticCall) -> i64 {
        let Some(func) = zandro::Func::from_index(index) else {
            return 0;
        };
        match zandro::call(&mut SdkHost, &mut self.lumps, func, &AcsArgs(call)) {
            zandro::Ret::Int(v) => i64::from(v),
            zandro::Ret::Str(text) => {
                call.return_str(&text);
                0
            }
        }
    }

    fn on_modify_damage(&mut self, index: i32, ev: ModifyDamage<'_>) -> i32 {
        if index != DAMAGE_CONTROL_VIRTUAL {
            return ev.damage;
        }
        damage::modify_damage(
            &SdkHost,
            &damage::Hit {
                damage: ev.damage,
                damage_type: ev.damage_type,
                passive: ev.passive,
                inflictor: ev.inflictor,
                source: ev.source,
            },
        )
    }
}

/// `class Damage_Control : Inventory` and its `Default` block.
fn register_damage_control() {
    let inventory = class_find("Inventory");
    let _ = reg::class_begin(damage::DAMAGE_CONTROL, inventory, ClassRef::NULL, -1, 0);
    // INVENTORY.MaxAmount 1;
    reg::class_property_i32(class_prop::INVENTORY_MAX_AMOUNT, 1);
    // +INVENTORY.AUTOACTIVATE. (+IGNORESKILL, +UNDROPPABLE and +UNTOSSABLE
    // change nothing for a token nobody picks up, drops or tosses; the ABI
    // has no flag ids for them.)
    reg::class_flag(class_flag::INVENTORY_AUTOACTIVATE, true);
    reg::class_virtual(
        virtual_slot::MODIFY_DAMAGE,
        VIRTUAL_CALLBACK_BASE + DAMAGE_CONTROL_VIRTUAL,
    );
    reg::class_end();
}

/// [`zandro::Args`] over the running `ScriptCall`.
struct AcsArgs(StaticCall);

impl zandro::Args for AcsArgs {
    fn int(&self, i: usize) -> i32 {
        self.0.int(i)
    }

    fn string(&self, i: usize) -> String {
        let mut buf = vec![0u8; 256];
        self.0.str_into(i, &mut buf).unwrap_or("").to_owned()
    }
}

room_script_sdk::export_mod!(BrutalZScript);

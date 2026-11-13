//! Every engine call the port makes, behind one trait: [`SdkHost`] talks to
//! Room through `room-script-sdk`; the tests use `FakeHost` (in `tests.rs`).
//!
//! Keeping the game logic on `&mut dyn Host` is what lets the ZScript be
//! checked line by line in plain `cargo test`, without a game running.

use room_script_sdk::game::{self, Actor, actor_i32};
use room_script_sdk::{ClassRef, LOG_INFO, class_find, lump_ns, reg};

/// The engine, as seen by the ported ZScript.
pub trait Host {
    /// ZScript `Skill` (Room's ZScript host reports the playsim skill as is).
    fn skill(&self) -> i32;
    /// Class by name (`ClassRef::NULL` when missing).
    fn class_find(&self, name: &str) -> ClassRef;
    /// `players[slot].mo`.
    fn player(&self, slot: i32) -> Option<Actor>;
    /// `bISMONSTER`.
    fn is_monster(&self, a: Actor) -> bool;
    /// `PlayerPawn(a) != null`.
    fn is_player_pawn(&self, a: Actor) -> bool;
    /// `a.GetClassName() == name` (names compare case-insensitively).
    fn class_name_is(&self, a: Actor, name: &str) -> bool;
    /// `a.GiveInventory(item, amount)`.
    fn give(&mut self, a: Actor, item: &str, amount: i32);
    /// `a.TakeInventory(item, amount)`.
    fn take(&mut self, a: Actor, item: &str, amount: i32);
    /// `a.TID`.
    fn tid(&self, a: Actor) -> i32;
    /// `a.ChangeTID(tid)`.
    fn change_tid(&mut self, a: Actor, tid: i32);
    /// `a.SoundClass = name`.
    fn set_sound_class(&mut self, a: Actor, name: &str);
    /// `players[slot].GetGender()`.
    fn player_gender(&self, slot: i32) -> i32;
    /// `CVar.GetCVar(name, players[slot]).GetInt()`.
    fn player_cvar(&self, slot: i32, name: &str) -> i32;
    /// `Wads.CheckNumForName(name, GLOBALNAMESPACE)` (−1 when absent).
    fn lump_find(&self, name: &str) -> i32;
    /// `Wads.ReadLump(lump)` as bytes.
    fn lump_read(&self, lump: i32) -> Vec<u8>;
    /// `Console.Printf`.
    fn print(&mut self, msg: &str);
}

/// [`Host`] over the real engine imports.
#[derive(Clone, Copy, Debug, Default)]
pub struct SdkHost;

impl Host for SdkHost {
    fn skill(&self) -> i32 {
        game::skill()
    }

    fn class_find(&self, name: &str) -> ClassRef {
        class_find(name)
    }

    fn player(&self, slot: i32) -> Option<Actor> {
        game::player(slot)
    }

    fn is_monster(&self, a: Actor) -> bool {
        a.is_monster()
    }

    fn is_player_pawn(&self, a: Actor) -> bool {
        let pawn = class_find("PlayerPawn");
        a.player().is_some() || (!pawn.is_null() && a.is_a(pawn))
    }

    fn class_name_is(&self, a: Actor, name: &str) -> bool {
        let mut buf = [0u8; 64];
        game::class_name(a.class(), &mut buf).eq_ignore_ascii_case(name)
    }

    fn give(&mut self, a: Actor, item: &str, amount: i32) {
        let class = class_find(item);
        if !class.is_null() {
            a.give(class, amount);
        }
    }

    fn take(&mut self, a: Actor, item: &str, amount: i32) {
        let class = class_find(item);
        if !class.is_null() {
            a.take(class, amount);
        }
    }

    fn tid(&self, a: Actor) -> i32 {
        a.get_i32(actor_i32::TID)
    }

    fn change_tid(&mut self, a: Actor, tid: i32) {
        a.set_i32(actor_i32::TID, tid);
    }

    fn set_sound_class(&mut self, a: Actor, name: &str) {
        a.set_sound_class(name);
    }

    fn player_gender(&self, slot: i32) -> i32 {
        game::player_gender(slot)
    }

    fn player_cvar(&self, slot: i32, name: &str) -> i32 {
        game::player_cvar(slot, name)
    }

    fn lump_find(&self, name: &str) -> i32 {
        reg::lump_find(name, lump_ns::GLOBAL)
    }

    fn lump_read(&self, lump: i32) -> Vec<u8> {
        // `lump_read` copies at most the buffer: grow until the lump fits.
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reg::lump_read(lump, &mut buf);
            let Ok(n) = usize::try_from(n) else {
                return Vec::new();
            };
            if n < buf.len() || buf.len() >= 64 << 20 {
                buf.truncate(n);
                return buf;
            }
            buf.resize(buf.len() * 2, 0);
        }
    }

    fn print(&mut self, msg: &str) {
        room_script_sdk::log(LOG_INFO, msg);
    }
}

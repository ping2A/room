//! Guest-side Rust SDK (`#![no_std]`, optional `alloc` feature) for Room WASM modules.
//!
//! Thin wrappers over `"room"` imports (`docs/12-wasm-scripting.md` §5.3).
#![allow(unsafe_code)] // WASM import/export surface (AGENTS.md §3).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

use room_script_abi::ABI_VERSION;

/// Doom patch encoder (`docs/31` MA3; needs `alloc`).
#[cfg(feature = "alloc")]
pub mod art;
mod bump;
/// World / actor / level control and typed events (play scope).
pub mod game;
/// Retained Mod HUD builder (`docs/27` §9).
pub mod hud;
#[macro_use]
#[allow(unused_imports)]
mod macros;
/// Registration-phase wrappers (`reg_*`).
pub mod reg;
/// DMX sound encoder (`docs/31` MA3; needs `alloc`).
#[cfg(feature = "alloc")]
pub mod sound;
mod sys;
mod sys_game;

/// Heap types re-exported for guest modules (`alloc` feature; the guest must
/// provide a `#[global_allocator]` or link `std`).
#[cfg(feature = "alloc")]
pub mod mem {
    pub use alloc::boxed::Box;
    pub use alloc::string::String;
    pub use alloc::vec::Vec;
}

pub use room_script_abi::{
    ABI_VERSION as ABI_MAJOR, ActionPayload, ActorRef, ClassRef, ModifyDamageV1, NameRef,
    SessionKind, lump_ns,
};
pub use room_script_sdk_macros::{actor, register_stub};

/// Log level: error.
pub const LOG_ERROR: i32 = 0;
/// Log level: warning.
pub const LOG_WARN: i32 = 1;
/// Log level: info.
pub const LOG_INFO: i32 = 2;
/// Log level: debug.
pub const LOG_DEBUG: i32 = 3;

/// Host `room::log`.
pub fn log(level: i32, msg: &str) {
    // SAFETY: pointer/len describe the UTF-8 slice for the duration of the call.
    unsafe {
        sys::log(level, msg.as_ptr() as i32, msg.len() as i32);
    }
}

/// Host `room::level_time` (maptime tics).
#[must_use]
pub fn level_time() -> i32 {
    // SAFETY: import has no pointer args.
    unsafe { sys::level_time() }
}

/// Host `room::player_count`.
#[must_use]
pub fn player_count() -> i32 {
    // SAFETY: import has no pointer args.
    unsafe { sys::player_count() }
}

/// Host `room::actor_exists`.
#[must_use]
pub fn actor_exists(actor: ActorRef) -> bool {
    // SAFETY: ActorRef bits are passed by value.
    unsafe { sys::actor_exists(actor.0 as i64) != 0 }
}

/// Host `room::name_intern`.
#[must_use]
pub fn name_intern(name: &str) -> NameRef {
    // SAFETY: pointer/len describe the UTF-8 slice for the duration of the call.
    unsafe { NameRef(sys::name_intern(name.as_ptr() as i32, name.len() as i32) as u32) }
}

/// Host `room::class_find`.
#[must_use]
pub fn class_find(name: &str) -> ClassRef {
    // SAFETY: pointer/len describe the UTF-8 slice for the duration of the call.
    unsafe { ClassRef(sys::class_find(name.as_ptr() as i32, name.len() as i32) as u32) }
}

/// Host `room::acs_execute` — `args` is packed little-endian into a temp… not available
/// without guest memory; pass raw ptr/len from wasm modules. Native stub returns 0.
#[must_use]
pub fn acs_execute_raw(script: i32, map: i32, args_ptr: i32, n: i32) -> i32 {
    // SAFETY: scalar / guest-pointer import.
    unsafe { sys::acs_execute(script, map, args_ptr, n) }
}

/// Required export: ABI major version.
#[unsafe(no_mangle)]
pub extern "C" fn room_abi_version() -> i32 {
    ABI_VERSION
}

/// Required export: host scratch allocation (bump heap).
#[unsafe(no_mangle)]
pub extern "C" fn room_alloc(size: i32, align: i32) -> i32 {
    bump::room_alloc(size, align)
}

/// Required export: bump allocator ignores free.
#[unsafe(no_mangle)]
pub extern "C" fn room_free(_ptr: i32, _size: i32, _align: i32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_version_matches_crate() {
        assert_eq!(room_abi_version(), 1);
        assert_eq!(ABI_MAJOR, 1);
    }

    #[test]
    fn native_stubs_are_callable() {
        assert_eq!(level_time(), 0);
        assert_eq!(player_count(), 0);
        assert!(!actor_exists(ActorRef::NULL));
        log(LOG_INFO, "sdk");
    }

    #[test]
    fn alloc_export_works() {
        let p = room_alloc(16, 8);
        assert!(p > 0);
        assert_eq!(p % 8, 0);
        room_free(p, 16, 8);
    }

    #[register_stub]
    struct DemoActor;

    #[test]
    fn register_stub_ok() {
        assert_eq!(DemoActor::__room_register(), 0);
    }
}

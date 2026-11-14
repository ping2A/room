//! Battle Royale — example Room WASM mod for `doom1.wad` multiplayer rooms
//! (any Doom map works).
//!
//! BR0: modular layout + [`host::Host`] abstraction with behaviour parity to the
//! original single-file mod.
#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

mod combat;
mod death;
mod drop_select;
mod fixed;
mod host;
mod loot;
mod match_flow;
mod announcer;
mod moments;
mod spots;
mod tables;
mod text;
mod ui;
mod zone;

pub use match_flow::Royale;

room_script_sdk::export_mod!(Royale);

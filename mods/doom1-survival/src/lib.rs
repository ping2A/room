//! Survival Director — Room WASM mod for Doom maps (`docs/27`).
//!
//! SV0: modular layout + [`host::Host`] abstraction with behaviour parity to the
//! original single-file wave director. Later milestones add the AI Director,
//! elites, draft UI, and finale.
#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

mod announcer;
mod bosses;
mod director;
mod economy;
mod elites;
mod encounters;
mod fixed;
mod host;
mod objectives;
mod perks;
mod register;
mod spots;
mod tables;
mod team;
mod text;
mod ui;

pub use director::Survival;

room_script_sdk::export_mod!(Survival);

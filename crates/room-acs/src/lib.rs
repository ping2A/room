//! ACS bytecode loader and virtual machine.
//!
//! Spec: `docs/10-acs.md`. Determinism rules in `docs/02-conventions.md` apply.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod error;
mod load;
mod pcd;
mod vm;

pub use error::AcsError;
pub use load::{
    AcsFormat, AcsFunction, AcsMapArray, AcsModule, SCRIPTF_CLIENTSIDE, SCRIPTF_NET, ScriptDef,
    ScriptType, load_behavior,
};
pub use pcd::{acsf, aprop, levelinfo, secpart};
pub use vm::{
    AcsHost, AcsState, HudMessageArgs, ModuleLinks, RunningScript, ScriptRunState,
    start_death_scripts, start_enter_scripts, start_open_scripts, start_open_scripts_from,
    start_respawn_scripts, start_return_scripts, start_script, start_script_with_activator,
    start_script_with_line, start_scripts_of_type, start_unloading_scripts, suspend_script,
    terminate_script, tick_scripts,
};

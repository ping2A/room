//! Console, commands, CVars, and config persistence.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod config;
mod cvar;
mod dispatch;
mod error;

pub use config::{
    IniApplyResult, apply_ini, load_ini_file, save_ini_file, save_ini_file_excluding, write_ini,
    write_ini_excluding,
};
pub use cvar::{CvarFlags, CvarStore, CvarType, CvarValue};
pub use dispatch::{Console, ExecResult, HostCommand};
pub use error::ConsoleError;

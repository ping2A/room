//! Console errors.

use thiserror::Error;

/// Command / CVar failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConsoleError {
    /// Unknown command or CVar.
    #[error("unknown identifier '{0}'")]
    Unknown(String),
    /// Bad arguments.
    #[error("{0}")]
    Message(String),
}

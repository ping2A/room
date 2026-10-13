//! ACS load / runtime errors.

use thiserror::Error;

/// Errors from loading or interpreting ACS modules.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AcsError {
    /// Truncated or malformed lump.
    #[error("ACS: {0}")]
    Message(String),
}

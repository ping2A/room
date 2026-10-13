//! DeHackEd errors.

use thiserror::Error;

/// Patch parse / apply failure.
#[derive(Debug, Error)]
pub enum DehackedError {
    /// Human-readable message.
    #[error("{0}")]
    Message(String),
}

//! DECORATE parse errors.

use room_sc::ScanError;
use thiserror::Error;

/// DECORATE frontend error.
#[derive(Debug, Error)]
pub enum DecorateError {
    /// Scanner failure.
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// Unexpected token / structure.
    #[error("{0}")]
    Message(String),
}

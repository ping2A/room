//! Decode / encode errors.

use thiserror::Error;

/// Protocol parse / validation failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProtoError {
    /// Truncated or structurally invalid bytes.
    #[error("bad packet")]
    BadPacket,
    /// Unknown binary or JSON message type.
    #[error("unknown message type")]
    UnknownType,
    /// Payload exceeds the configured maximum.
    #[error("payload too large")]
    TooLarge,
    /// JSON failed to parse or failed schema checks.
    #[error("bad json: {0}")]
    BadJson(String),
}

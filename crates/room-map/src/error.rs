//! Map load errors.

use thiserror::Error;

/// Failure while loading or validating a map.
#[derive(Debug, Error)]
pub enum MapError {
    /// Truncated or corrupt binary lump.
    #[error("truncated map lump: {0}")]
    Truncated(&'static str),
    /// Invalid value / index.
    #[error("invalid map data: {0}")]
    Invalid(&'static str),
    /// Map marker or required lump missing.
    #[error("map not found: {0}")]
    NotFound(String),
    /// VFS failure.
    #[error(transparent)]
    Vfs(#[from] room_vfs::VfsError),
    /// Format failure.
    #[error(transparent)]
    Format(#[from] room_formats::FormatError),
    /// Free-form message.
    #[error("{0}")]
    Message(String),
}

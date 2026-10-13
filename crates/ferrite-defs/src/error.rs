//! Definition compilation errors.

use thiserror::Error;

/// Error while building definitions.
#[derive(Debug, Error)]
pub enum DefsError {
    /// Scanner failure.
    #[error(transparent)]
    Scan(#[from] ferrite_sc::ScanError),
    /// Format parse failure.
    #[error(transparent)]
    Format(#[from] ferrite_formats::FormatError),
    /// VFS failure.
    #[error(transparent)]
    Vfs(#[from] ferrite_vfs::VfsError),
    /// Semantic / unknown keyword.
    #[error("{0}")]
    Message(String),
}

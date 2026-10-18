//! Build content errors.

use thiserror::Error;

/// Failure reading or converting Build data.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// Truncated or inconsistent structure.
    #[error("truncated or corrupt data: {0}")]
    Truncated(&'static str),
    /// Value outside the accepted range.
    #[error("invalid value: {0}")]
    Invalid(&'static str),
    /// A required file is missing from the archive.
    #[error("missing {0}")]
    Missing(String),
    /// A Build GRP that is not Duke Nukem 3D.
    #[error("unsupported Build game: {0}")]
    Unsupported(&'static str),
    /// Writing the output WAD failed.
    #[error("wad: {0}")]
    Wad(String),
}

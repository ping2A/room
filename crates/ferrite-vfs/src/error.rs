//! VFS errors.

use thiserror::Error;

/// Errors from mounting containers or reading lumps.
#[derive(Debug, Error)]
pub enum VfsError {
    /// I/O failure against a container source.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Unrecognised or corrupt archive.
    #[error("parse error: {0}")]
    Parse(String),
    /// Lump id is not present in this VFS.
    #[error("unknown lump id {0}")]
    UnknownLump(u32),
    /// Container id is not present in this VFS.
    #[error("unknown container id {0}")]
    UnknownContainer(u32),
    /// Unsupported compression method.
    #[error("unsupported compression: {0}")]
    UnsupportedCompression(String),
}

impl VfsError {
    /// Construct a parse error.
    pub fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }
}

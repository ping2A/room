//! Format parse errors.

use thiserror::Error;

/// Binary format parse failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// Truncated or inconsistent structure.
    #[error("truncated or corrupt data: {0}")]
    Truncated(&'static str),
    /// Value outside the accepted range.
    #[error("invalid value: {0}")]
    Invalid(&'static str),
}

//! DECORATE parse errors.

use room_sc::{ScanError, SourceSpan};
use thiserror::Error;

/// DECORATE frontend error.
#[derive(Clone, Debug, Error)]
pub enum DecorateError {
    /// Scanner failure.
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// Unexpected token / structure (no location).
    #[error("{0}")]
    Message(String),
    /// Located diagnostic (`file:line:col: message`).
    #[error("{location}: {message}")]
    Located {
        /// `file:line:col` or `<expanded>:line:col`.
        location: String,
        /// Human-readable message.
        message: String,
    },
}

impl DecorateError {
    /// Build a located error from a scanner span (expanded coordinates).
    #[must_use]
    pub fn at_span(span: SourceSpan, message: impl Into<String>) -> Self {
        Self::Located {
            location: format!("<expanded>:{}:{}", span.line, span.col),
            message: message.into(),
        }
    }

    /// Build a located error with an already-formatted `file:line:col`.
    #[must_use]
    pub fn at_location(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Located {
            location: location.into(),
            message: message.into(),
        }
    }
}

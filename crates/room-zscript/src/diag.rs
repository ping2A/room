//! Compiler diagnostics.

use crate::span::Span;

/// Diagnostic severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Error — aborts loading the mod.
    Error,
    /// Warning.
    Warning,
    /// Note / info.
    Note,
}

/// One diagnostic message.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// Severity.
    pub severity: Severity,
    /// Human message.
    pub message: String,
    /// Primary span.
    pub span: Span,
    /// Optional source path override.
    pub path: String,
}

impl Diagnostic {
    /// Error at span.
    #[must_use]
    pub fn error(path: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            path: path.into(),
        }
    }

    /// Warning at span.
    #[must_use]
    pub fn warn(path: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            span,
            path: path.into(),
        }
    }

    /// GZDoom-style `file:line:col: message` line.
    #[must_use]
    pub fn format_line(&self) -> String {
        format!(
            "{}:{}:{}: {}: {}",
            if self.path.is_empty() {
                "<input>"
            } else {
                &self.path
            },
            self.span.line.max(1),
            self.span.col.max(1),
            match self.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
            },
            self.message
        )
    }
}

//! Expanded-source → original file/line map for DECORATE `#include`s (BD1).

/// One line of expanded DECORATE and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLine {
    /// Include path (`"<root>"` for the top-level lump).
    pub file: String,
    /// 1-based line in `file`.
    pub line: u32,
}

/// Maps 1-based lines of an expanded buffer to original include locations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    /// Index 0 unused; `lines[n]` describes expanded line `n`.
    pub lines: Vec<SourceLine>,
}

impl SourceMap {
    /// Empty map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lines: vec![SourceLine {
                file: "<root>".into(),
                line: 0,
            }],
        }
    }

    /// Look up an expanded 1-based line.
    #[must_use]
    pub fn lookup(&self, expanded_line: u32) -> Option<&SourceLine> {
        self.lines.get(expanded_line as usize)
    }

    /// Format `file:line` for diagnostics (falls back to expanded line).
    #[must_use]
    pub fn format_loc(&self, expanded_line: u32, column: u32) -> String {
        match self.lookup(expanded_line) {
            Some(loc) if loc.line > 0 => format!("{}:{}:{}", loc.file, loc.line, column),
            _ => format!("<expanded>:{expanded_line}:{column}"),
        }
    }
}

/// Result of `#include` expansion with a source map.
#[derive(Clone, Debug)]
pub struct ExpandedSource {
    /// Fully expanded text.
    pub text: String,
    /// Line map for `text`.
    pub map: SourceMap,
}

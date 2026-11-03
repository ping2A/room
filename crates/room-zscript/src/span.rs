//! Source locations.

/// Opaque source file id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SourceId(pub u32);

/// Byte span in a source file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// Source file.
    pub source: SourceId,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset (exclusive).
    pub end: u32,
    /// 1-based start line.
    pub line: u32,
    /// 1-based start column.
    pub col: u32,
}

impl Span {
    /// Dummy span.
    #[must_use]
    pub fn dummy() -> Self {
        Self::default()
    }
}

/// Named source buffer.
#[derive(Clone, Debug)]
pub struct SourceFile {
    /// Path or lump name.
    pub name: String,
    /// UTF-8 contents.
    pub text: String,
    /// Id.
    pub id: SourceId,
}

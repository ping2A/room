//! Raw flat size detection.

/// Recognised flat dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Map byte length to flat dimensions, if it matches a known raw flat size.
#[must_use]
pub fn flat_dimensions(len: usize) -> Option<FlatSize> {
    let (width, height) = match len {
        4096 => (64, 64),
        8192 => (64, 128),
        16_384 => (128, 128),
        65_536 => (256, 256),
        262_144 => (512, 512),
        1_048_576 => (1024, 1024),
        _ => return None,
    };
    Some(FlatSize { width, height })
}

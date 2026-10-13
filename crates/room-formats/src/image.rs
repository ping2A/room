//! Decoded image IR shared by patch/flat/PNG/… loaders.

/// Paletted 8-bit image (indices into PLAYPAL).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalettedImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major palette indices.
    pub indices: Vec<u8>,
    /// Per-pixel opacity (`0` = transparent). Length equals `indices`, or empty
    /// when every non-zero index is opaque (index 0 may still be transparent).
    pub alpha: Vec<u8>,
}

/// True-color RGBA8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major RGBA bytes (`width * height * 4`).
    pub pixels: Vec<u8>,
}

/// Either paletted or RGBA decoded image plus sprite offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// Left (X) offset / hot-spot.
    pub left_offset: i32,
    /// Top (Y) offset / hot-spot.
    pub top_offset: i32,
    /// True when any pixel is partially or fully transparent.
    pub has_translucency: bool,
    /// Pixel storage.
    pub pixels: ImagePixels,
}

/// Pixel payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImagePixels {
    /// 8-bit indexed.
    Paletted(PalettedImage),
    /// 32-bit RGBA.
    Rgba8(RgbaImage),
}

impl Image {
    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        match &self.pixels {
            ImagePixels::Paletted(p) => p.width,
            ImagePixels::Rgba8(p) => p.width,
        }
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        match &self.pixels {
            ImagePixels::Paletted(p) => p.height,
            ImagePixels::Rgba8(p) => p.height,
        }
    }
}

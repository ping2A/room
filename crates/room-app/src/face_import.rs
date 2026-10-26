//! Desktop face import (docs/26 F7): file path → RGBA for `room-faceswap`.
//! Camera deferred per §12; paste can call the same loader from a temp file.

use room_faceswap::RgbaImage;
use std::path::Path;

/// Errors loading a face image on desktop.
#[derive(Debug, thiserror::Error)]
pub enum FaceImportError {
    /// I/O.
    #[error("read face image: {0}")]
    Io(#[from] std::io::Error),
    /// Decode.
    #[error("decode face image: {0}")]
    Decode(String),
}

/// Load a PNG (or other `image`-crate format with png feature) into RGBA.
///
/// # Errors
///
/// Returns I/O or decode failures; never panics on bad bytes.
#[allow(dead_code)] // wired by Faces UI / CLI hosts as F7 lands
pub fn load_face_rgba(path: &Path) -> Result<RgbaImage, FaceImportError> {
    let data = std::fs::read(path)?;
    let img = image::load_from_memory(&data)
        .map_err(|e| FaceImportError::Decode(e.to_string()))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok(RgbaImage {
        width: w,
        height: h,
        pixels: img.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn missing_file_is_err() {
        let err = load_face_rgba(Path::new("/no/such/face.png"));
        assert!(err.is_err());
    }
}

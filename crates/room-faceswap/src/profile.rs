//! Canonical face asset stored after capture / import.

use crate::image::{AlphaMask, RgbaImage};

/// Opaque face identity (random; never derived from pixels).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FaceId(pub u128);

/// How the face entered the gallery (`docs/26` §3.3 / §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Consent {
    /// Live camera capture of the user.
    SelfCamera,
    /// User attested permission for an imported photo.
    Attested,
}

/// Mean RGB colour (0..=255 per channel).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

/// One oriented face view (front / side / back).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceView {
    /// Canonical 128×128 RGBA crop (eyes at (40,52)/(88,52)).
    pub rgba: RgbaImage,
    /// Feathered matte over the crop.
    pub mask: AlphaMask,
}

impl FaceView {
    /// Build a view, enforcing 128×128.
    pub fn new(rgba: RgbaImage, mask: AlphaMask) -> Result<Self, ProfileError> {
        if rgba.width != 128 || rgba.height != 128 {
            return Err(ProfileError::BadSize {
                got: (rgba.width, rgba.height),
            });
        }
        if mask.width != 128 || mask.height != 128 {
            return Err(ProfileError::BadMaskSize {
                got: (mask.width, mask.height),
            });
        }
        Ok(Self { rgba, mask })
    }
}

/// Gallery entry after alignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceProfile {
    /// Random id.
    pub id: FaceId,
    /// User label.
    pub name: String,
    /// Required front capture.
    pub front: FaceView,
    /// Optional three-quarter / profile left.
    pub left: Option<FaceView>,
    /// Optional three-quarter / profile right.
    pub right: Option<FaceView>,
    /// Optional back-of-head.
    pub back: Option<FaceView>,
    /// Mean skin tone (edge blend).
    pub tone: Rgb,
    /// Mean hair colour above the forehead (back fallback).
    pub hair: Rgb,
    /// Consent record.
    pub consent: Consent,
    /// Local creation time (gallery only; not used in bake hashes).
    pub created: u64,
}

/// Profile construction errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    /// RGBA was not 128×128.
    #[error("face view must be 128×128, got {got:?}")]
    BadSize {
        /// Actual dimensions.
        got: (u32, u32),
    },
    /// Mask was not 128×128.
    #[error("face mask must be 128×128, got {got:?}")]
    BadMaskSize {
        /// Actual dimensions.
        got: (u32, u32),
    },
}

/// Canonical eye centres in the 128×128 crop (`docs/26` §3.5).
pub const CANON_LEFT_EYE: (f32, f32) = (40.0, 52.0);
/// Right eye.
pub const CANON_RIGHT_EYE: (f32, f32) = (88.0, 52.0);
/// Approximate chin Y (tune later).
pub const CANON_CHIN_Y: f32 = 118.0;

//! Per-lump head placement (`docs/26` §4). Parser / estimator land in F1.

/// Approximate head facing for view selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HeadYaw {
    /// Rotation 1.
    Front,
    /// Rotations 2 / 8.
    ThreeQuarterLeft,
    /// Rotations 3 / 7.
    ProfileLeft,
    /// Rotations 4 / 5 / 6.
    Back,
    /// Rotations 3 / 7 (mirrored name).
    ProfileRight,
    /// Rotations 2 / 8 (mirrored name).
    ThreeQuarterRight,
}

/// Whether the head is drawable on this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Visibility {
    /// Full ellipse.
    Full,
    /// Partial coverage (reserved; F1+).
    Partial,
    /// Gib / hidden — compositor leaves the lump unchanged.
    Hidden,
}

/// Where the face goes on one sprite lump.
#[derive(Clone, Debug, PartialEq)]
pub struct HeadAnchor {
    /// Ellipse centre in lump pixels.
    pub center: (f32, f32),
    /// Half-axes (x, y).
    pub radius: (f32, f32),
    /// Eye-line roll in degrees.
    pub roll: f32,
    /// Facing.
    pub yaw: HeadYaw,
    /// Drawability.
    pub visible: Visibility,
    /// Fraction of the ellipse top kept from the sprite (helmet / horns / hair).
    pub keep_top: f32,
}

impl HeadAnchor {
    /// Front-view ellipse helper.
    #[must_use]
    pub fn front(center: (f32, f32), radius: (f32, f32), keep_top: f32) -> Self {
        Self {
            center,
            radius,
            roll: 0.0,
            yaw: HeadYaw::Front,
            visible: Visibility::Full,
            keep_top: keep_top.clamp(0.0, 1.0),
        }
    }
}

/// xxHash64 of decoded palette indices + alpha (key half of an anchor record).
#[must_use]
pub fn lump_pixel_hash(indices: &[u8], alpha: &[u8]) -> u64 {
    use xxhash_rust::xxh3::xxh3_64;
    // F0: xxh3_64 over indices||alpha. F1 HEADANCHORS text may switch to xxh64;
    // document the digest in the parser when it lands.
    let mut buf = Vec::with_capacity(indices.len() + alpha.len());
    buf.extend_from_slice(indices);
    buf.extend_from_slice(alpha);
    xxh3_64(&buf)
}

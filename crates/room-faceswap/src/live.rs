//! Live (render-only) face overlay helpers (`docs/26` §7.2).

use crate::bake::{BakeClassResult, bake_class};
use crate::composite::{CompositeOptions, PalettedSprite};
use crate::parse::HeadAnchorsFile;
use crate::profile::{FaceId, FaceProfile};
use std::collections::BTreeMap;

/// Cache key for a composited lump override.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LiveCacheKey {
    /// Source lump name.
    pub lump: String,
    /// Face id.
    pub face: FaceId,
    /// Profile revision (webcam live bumps this).
    pub revision: u32,
}

/// In-memory override table: `(lump, face)` → composited sprite.
#[derive(Clone, Debug, Default)]
pub struct LiveFaceOverrides {
    /// Map.
    pub by_key: BTreeMap<(String, u128), PalettedSprite>,
}

impl LiveFaceOverrides {
    /// Insert baked lumps for one face.
    pub fn absorb_bake(&mut self, face: FaceId, baked: &BakeClassResult) {
        for lump in &baked.lumps {
            self.by_key.insert(
                (lump.name.to_ascii_uppercase(), face.0),
                lump.sprite.clone(),
            );
        }
    }

    /// Lookup.
    #[must_use]
    pub fn get(&self, lump: &str, face: FaceId) -> Option<&PalettedSprite> {
        self.by_key.get(&(lump.to_ascii_uppercase(), face.0))
    }

    /// Drop everything (Forget all / faces off).
    pub fn clear(&mut self) {
        self.by_key.clear();
    }
}

/// Rebuild overrides for a class (static live mode).
pub fn rebuild_class_overrides(
    lumps: &[(String, PalettedSprite, u64)],
    palette: &[u8; 768],
    profile: &FaceProfile,
    anchors: Option<&HeadAnchorsFile>,
    opts: &CompositeOptions,
) -> Result<BakeClassResult, crate::bake::BakeError> {
    bake_class(lumps, palette, profile, anchors, opts, &[])
}

/// Cap how many lumps a live-webcam pass may re-composite per frame.
pub const LIVE_LUMPS_PER_FRAME: usize = 16;

/// Live-webcam revision: re-bake at most [`LIVE_LUMPS_PER_FRAME`] lumps and
/// merge into `overrides`. Host should call ~every 100 ms while webcam live
/// is on (`docs/26` §7.2 / F5).
pub fn revise_live_lumps(
    overrides: &mut LiveFaceOverrides,
    lumps: &[(String, PalettedSprite, u64)],
    palette: &[u8; 768],
    profile: &FaceProfile,
    anchors: Option<&HeadAnchorsFile>,
    opts: &CompositeOptions,
    start_index: usize,
) -> Result<usize, crate::bake::BakeError> {
    if lumps.is_empty() {
        return Ok(0);
    }
    let n = lumps.len().min(LIVE_LUMPS_PER_FRAME);
    let start = start_index % lumps.len();
    let mut slice = Vec::with_capacity(n);
    for i in 0..n {
        slice.push(lumps[(start + i) % lumps.len()].clone());
    }
    let baked = bake_class(&slice, palette, profile, anchors, opts, &[])?;
    overrides.absorb_bake(profile.id, &baked);
    Ok(baked.lumps.len())
}

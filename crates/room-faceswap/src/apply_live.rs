//! Helpers to push baked faces into a render texture table (F4).

use crate::bake::BakeClassResult;
use crate::composite::PalettedSprite;
use crate::live::LiveFaceOverrides;
use crate::profile::FaceId;

/// Expand a paletted sprite to RGBA8 using a 768-byte PLAYPAL page.
#[must_use]
pub fn sprite_to_rgba(sprite: &PalettedSprite, palette: &[u8; 768]) -> Vec<u8> {
    let n = (sprite.width as usize).saturating_mul(sprite.height as usize);
    let mut out = vec![0u8; n.saturating_mul(4)];
    for i in 0..n {
        let a = sprite.alpha.get(i).copied().unwrap_or(255);
        if a == 0 {
            continue;
        }
        let idx = sprite.indices.get(i).copied().unwrap_or(0) as usize * 3;
        let o = i * 4;
        out[o] = palette[idx];
        out[o + 1] = palette[idx + 1];
        out[o + 2] = palette[idx + 2];
        out[o + 3] = a;
    }
    out
}

/// Register a bake into live overrides (render-only; does not touch sim state).
pub fn install_bake(overrides: &mut LiveFaceOverrides, face: FaceId, baked: &BakeClassResult) {
    overrides.absorb_bake(face, baked);
}

/// Lookup RGBA for `(lump, face)` ready for `TextureManager::upsert_rgba`.
#[must_use]
pub fn override_rgba(
    overrides: &LiveFaceOverrides,
    lump: &str,
    face: FaceId,
    palette: &[u8; 768],
) -> Option<(u32, u32, Vec<u8>)> {
    let spr = overrides.get(lump, face)?;
    Some((
        spr.width,
        spr.height,
        sprite_to_rgba(spr, palette),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::composite::PalettedSprite;

    #[test]
    fn transparent_stays_zero_alpha() {
        let spr = PalettedSprite {
            width: 2,
            height: 1,
            left_offset: 0,
            top_offset: 0,
            indices: vec![1, 2],
            alpha: vec![0, 255],
        };
        let mut pal = [0u8; 768];
        pal[3] = 10;
        pal[4] = 20;
        pal[5] = 30;
        pal[6] = 40;
        pal[7] = 50;
        pal[8] = 60;
        let rgba = sprite_to_rgba(&spr, &pal);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[4..8], &[40, 50, 60, 255]);
    }
}

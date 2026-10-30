//! Bake a face onto every lump of a sprite class (`docs/26` §7.1).

use crate::anchor::{HeadAnchor, HeadYaw, Visibility};
use crate::assign::FaceAssignment;
use crate::composite::{CompositeError, CompositeOptions, PalettedSprite, composite_front};
use crate::estimate::{ClassHint, estimate_anchor, mirror_anchor, yaw_from_lump};
use crate::parse::HeadAnchorsFile;
use crate::profile::FaceProfile;
use std::collections::BTreeMap;

/// One baked sprite lump (palette indices ready for PK3 / override).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BakedLump {
    /// Lump name.
    pub name: String,
    /// Composited sprite.
    pub sprite: PalettedSprite,
}

/// Bake result for a class.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BakeClassResult {
    /// Baked lumps in input order.
    pub lumps: Vec<BakedLump>,
}

/// Errors while baking.
#[derive(Debug, thiserror::Error)]
pub enum BakeError {
    /// Composite failure.
    #[error(transparent)]
    Composite(#[from] CompositeError),
}

/// Resolve an anchor for a lump: shipped table (hash match) → estimate.
#[must_use]
pub fn resolve_anchor(
    file: Option<&HeadAnchorsFile>,
    lump: &str,
    sprite: &PalettedSprite,
    pixel_hash: u64,
    hint: ClassHint,
) -> HeadAnchor {
    let key = lump.to_ascii_uppercase();
    if let Some(f) = file {
        if let Some(e) = f.anchors.get(&key) {
            if e.hash == 0 || e.hash == pixel_hash {
                return e.anchor.clone();
            }
        }
        // Mirrored pair: e.g. A2A8 stored under first name — try prefix before second rot.
        if let Some((base, _)) = split_mirror_name(&key) {
            if let Some(e) = f.anchors.get(base) {
                if e.hash == 0 || e.hash == pixel_hash {
                    return mirror_anchor(&e.anchor, sprite.width);
                }
            }
        }
    }
    estimate_anchor(sprite.width, sprite.height, &sprite.alpha, lump, hint)
}

fn split_mirror_name(name: &str) -> Option<(&str, &str)> {
    // TROOA2A8 → ("TROOA2", "A8") style: 4 prefix + frame + rot + frame + rot
    let b = name.as_bytes();
    if b.len() >= 8 {
        Some((
            core::str::from_utf8(&b[..6]).ok()?,
            core::str::from_utf8(&b[6..]).ok()?,
        ))
    } else {
        None
    }
}

/// Bake `profile` onto every sprite in `lumps` (name, sprite, optional hash).
pub fn bake_class(
    lumps: &[(String, PalettedSprite, u64)],
    palette: &[u8; 768],
    profile: &FaceProfile,
    anchors: Option<&HeadAnchorsFile>,
    opts: &CompositeOptions,
    exclude: &[String],
) -> Result<BakeClassResult, BakeError> {
    let mut out = BakeClassResult::default();
    for (name, spr, hash) in lumps {
        if exclude.iter().any(|e| e.eq_ignore_ascii_case(name)) {
            out.lumps.push(BakedLump {
                name: name.clone(),
                sprite: spr.clone(),
            });
            continue;
        }
        let hint = ClassHint {
            sprite_prefix: None,
            death_frame_index: death_index_from_name(name),
        };
        let mut anchor = resolve_anchor(anchors, name, spr, *hash, hint);
        if matches!(anchor.visible, Visibility::Hidden) {
            out.lumps.push(BakedLump {
                name: name.clone(),
                sprite: spr.clone(),
            });
            continue;
        }
        // Ensure yaw matches lump rotation digit.
        if matches!(anchor.yaw, HeadYaw::Front) {
            anchor.yaw = yaw_from_lump(name);
        }
        let baked = composite_with_yaw(spr, palette, &anchor, profile, opts)?;
        out.lumps.push(BakedLump {
            name: name.clone(),
            sprite: baked,
        });
    }
    Ok(out)
}

fn death_index_from_name(name: &str) -> Option<u8> {
    // Crude: frame letter I+ often death in Doom — leave to anchors; None here.
    let b = name.as_bytes();
    if b.len() >= 5 {
        let frame = (b[4] as char).to_ascii_uppercase();
        if frame >= 'I' {
            return Some((frame as u8) - b'A');
        }
    }
    None
}

fn composite_with_yaw(
    sprite: &PalettedSprite,
    palette: &[u8; 768],
    anchor: &HeadAnchor,
    profile: &FaceProfile,
    opts: &CompositeOptions,
) -> Result<PalettedSprite, CompositeError> {
    match anchor.yaw {
        HeadYaw::Front => composite_front(sprite, palette, anchor, profile, opts),
        HeadYaw::Back => {
            // Default: keep original head (docs §12 decision 5).
            Ok(sprite.clone())
        }
        HeadYaw::ThreeQuarterLeft
        | HeadYaw::ThreeQuarterRight
        | HeadYaw::ProfileLeft
        | HeadYaw::ProfileRight => {
            // Fallback: compressed front (F0/F2 path). Dedicated side views in F5.
            let mut a = anchor.clone();
            let compress = match anchor.yaw {
                HeadYaw::ProfileLeft | HeadYaw::ProfileRight => 0.55,
                _ => 0.80,
            };
            a.radius.0 *= compress;
            match anchor.yaw {
                HeadYaw::ThreeQuarterLeft | HeadYaw::ProfileLeft => {
                    a.center.0 -= a.radius.0 * 0.15;
                }
                HeadYaw::ThreeQuarterRight | HeadYaw::ProfileRight => {
                    a.center.0 += a.radius.0 * 0.15;
                }
                _ => {}
            }
            // Prefer captured side view when present (same composite entry for F2).
            if matches!(anchor.yaw, HeadYaw::ThreeQuarterLeft | HeadYaw::ProfileLeft)
                && profile.left.is_some()
            {
                return composite_front(sprite, palette, &a, profile, opts);
            }
            if matches!(
                anchor.yaw,
                HeadYaw::ThreeQuarterRight | HeadYaw::ProfileRight
            ) && profile.right.is_some()
            {
                return composite_front(sprite, palette, &a, profile, opts);
            }
            composite_front(sprite, palette, &a, profile, opts)
        }
    }
}

/// Encode baked lumps as a minimal PK3 (store-only ZIP of raw patches later —
/// here we emit a simple name→paletted map for the host to write).
#[must_use]
pub fn bake_index(result: &BakeClassResult) -> BTreeMap<String, PalettedSprite> {
    result
        .lumps
        .iter()
        .map(|l| (l.name.clone(), l.sprite.clone()))
        .collect()
}

/// Placeholder: assignment is applied by the host when choosing `profile`.
pub fn _assignment_marker(_: &FaceAssignment) {}

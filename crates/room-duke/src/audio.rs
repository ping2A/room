//! Presentation-side audio helpers shared by the desktop and browser shells.

use crate::con::ScriptImage;

/// Convert Atomic `definesound` pitch ranges to a mixer ratio.
///
/// Chocolate samples the random offset with libc `rand()`. Room derives a
/// stable sample from the tic, sound, and source instead, keeping presentation
/// variation out of the deterministic playsim RNG.
#[must_use]
pub fn pitch_scale(
    script: Option<&ScriptImage>,
    sound_id: u16,
    tic: u32,
    source_id: Option<u64>,
    underwater: bool,
    global: bool,
) -> f32 {
    let Some(definition) = script
        .and_then(|image| image.sounds.get(usize::from(sound_id)))
        .and_then(Option::as_ref)
    else {
        return 1.0;
    };
    let start = i64::from(definition.ps);
    let end = i64::from(definition.pe);
    let low = start.min(end);
    let span = start.abs_diff(end);
    let mut offset = if span == 0 {
        start
    } else {
        let mut hash = u64::from(tic)
            ^ source_id.unwrap_or(0).rotate_left(23)
            ^ u64::from(sound_id).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        hash ^= hash >> 30;
        hash = hash.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        hash ^= hash >> 27;
        hash = hash.wrapping_mul(0x94d0_49bb_1331_11eb);
        hash ^= hash >> 31;
        low + i64::try_from(hash % span).unwrap_or(0)
    };
    if underwater && !global {
        match sound_id {
            // Chocolate `xyzsound`'s PIPEBOMB/LASERTRIP/RPG explosion cases.
            9 | 14 | 17 => offset -= 1024,
            _ if definition.m & 4 == 0 => offset = -768,
            _ => {}
        }
    }
    let bounded = i16::try_from(offset.clamp(-12_000, 12_000)).unwrap_or_default();
    (f32::from(bounded) / 1200.0).exp2()
}

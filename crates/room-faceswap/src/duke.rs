//! Duke / Build palette helpers (`docs/26` §5.1 / F7).

use crate::composite::CompositeOptions;

/// Build-palette skip list: avoid LOOKUP.DAT swap ranges commonly used for
/// enemy `pal` (captain red, etc.). Conservative defaults for Atomic.
#[must_use]
pub fn duke_skip_indices() -> Vec<u8> {
    // Keep a hole for transparent-ish index 255 used by some Build tools.
    let mut v = vec![255];
    // Skin-ramp-ish bands that remaps often touch — host may extend per-pal.
    v.extend(168u8..=175);
    v
}

/// Composite options tuned for a Build PLAYPAL page.
#[must_use]
pub fn duke_composite_options() -> CompositeOptions {
    CompositeOptions {
        transfer_strength: 0.7,
        skip_indices: duke_skip_indices(),
        allow_overflow: false,
    }
}

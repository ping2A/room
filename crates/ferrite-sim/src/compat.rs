//! GZDoom `compatflags` / `compatflags2` bitfields (`docs/19-compatibility.md`).
//!
//! Bit positions match GZDoom `doomdef.h` (`COMPATF_*` / `COMPATF2_*`).
//! Tag call sites with `// COMPAT(compat_name)` for auditability.
//! Additional named bits are added as sim sites start checking them.

/// `compatflags` bitfield (GZDoom `COMPATF_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompatFlags {
    /// Primary flags.
    pub flags: u32,
    /// Secondary flags (`compatflags2`).
    pub flags2: u32,
}

impl CompatFlags {
    /// Empty (GZDoom default / ZDoom behaviour).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            flags: 0,
            flags2: 0,
        }
    }

    /// Construct from raw bitfields.
    #[must_use]
    pub const fn from_bits(flags: u32, flags2: u32) -> Self {
        Self { flags, flags2 }
    }

    /// True if a `COMPATF_*` bit is set.
    #[must_use]
    pub const fn has(self, bit: u32) -> bool {
        self.flags & bit != 0
    }

    /// True if a `COMPATF2_*` bit is set.
    #[must_use]
    pub const fn has2(self, bit: u32) -> bool {
        self.flags2 & bit != 0
    }

    /// MBF21 features enabled (inverse of `COMPATF2_NOMBF21`).
    #[must_use]
    pub const fn mbf21_enabled(self) -> bool {
        !self.has2(COMPATF2_NOMBF21)
    }

    /// OR in additional bits (per-map `compatibility.txt` entries).
    pub fn merge(&mut self, flags: u32, flags2: u32) {
        self.flags |= flags;
        self.flags2 |= flags2;
    }
}

// --- compatflags bits used by current presets / call sites ---

/// Use Doom's shortest texture around behaviour.
pub const COMPATF_SHORTTEX: u32 = 1 << 0;
/// Don't fix loop index for stair building.
pub const COMPATF_STAIRINDEX: u32 = 1 << 1;
/// Any special line can block a use.
pub const COMPATF_USEBLOCKING: u32 = 1 << 8;
/// Disable Boom local door light effect.
pub const COMPATF_NODOORLIGHT: u32 = 1 << 9;
/// Sector-based sound target.
pub const COMPATF_SOUNDTARGET: u32 = 1 << 11;
/// Limit DeHackEd MaxHealth like Doom2.exe.
pub const COMPATF_DEHHEALTH: u32 = 1 << 12;
/// Trace ignores same-sector two-sided lines.
pub const COMPATF_TRACE: u32 = 1 << 13;
/// Additive Boom scrolling.
pub const COMPATF_BOOMSCROLL: u32 = 1 << 15;
/// Original Doom projectile clip heights.
pub const COMPATF_MISSILECLIP: u32 = 1 << 19;
/// Monsters can't be pushed over dropoffs.
pub const COMPATF_CROSSDROPOFF: u32 = 1 << 20;
/// Original `A_Mushroom` velocity.
pub const COMPATF_MUSHROOM: u32 = 1 << 23;
/// Monsters affected by friction / pushers.
pub const COMPATF_MBFMONSTERMOVE: u32 = 1 << 24;
/// Friendly monsters ignore monster-block lines.
pub const COMPATF_NOBLOCKFRIENDS: u32 = 1 << 26;
/// Invert equal-distance sprite sort.
pub const COMPATF_SPRITESORT: u32 = 1 << 27;
/// Find neighbouring light like Doom.
pub const COMPATF_LIGHT: u32 = 1 << 29;
/// Ignore compositing on masked midtextures.
pub const COMPATF_MASKEDMIDTEX: u32 = 1 << 31;

// --- compatflags2 ---

/// Doom floor motion.
pub const COMPATF2_FLOORMOVE: u32 = 1 << 1;
/// No vertical explosion thrust.
pub const COMPATF2_EXPLODE1: u32 = 1 << 8;
/// MBF avoid hazards.
pub const COMPATF2_AVOID_HAZARDS: u32 = 1 << 12;
/// MBF stay on lift.
pub const COMPATF2_STAYONLIFT: u32 = 1 << 13;
/// Disable MBF21 features.
pub const COMPATF2_NOMBF21: u32 = 1 << 14;

/// GZDoom `compatmode` presets used by `COMPLVL` (cases 1/3/5/8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatMode {
    /// Engine defaults (no forced bits).
    Default,
    /// Doom (non-strict) — `compatmode` 1.
    Doom,
    /// Boom — `compatmode` 3.
    Boom,
    /// MBF — `compatmode` 5.
    Mbf,
    /// MBF21 — `compatmode` 8.
    Mbf21,
}

impl CompatMode {
    /// Bitfields for this preset (GZDoom `d_main.cpp` `compatmode` CVar).
    #[must_use]
    pub const fn flags(self) -> CompatFlags {
        match self {
            Self::Default => CompatFlags::new(),
            Self::Doom => CompatFlags::from_bits(
                COMPATF_SHORTTEX
                    | COMPATF_STAIRINDEX
                    | COMPATF_USEBLOCKING
                    | COMPATF_NODOORLIGHT
                    | COMPATF_SPRITESORT
                    | COMPATF_TRACE
                    | COMPATF_MISSILECLIP
                    | COMPATF_SOUNDTARGET
                    | COMPATF_DEHHEALTH
                    | COMPATF_CROSSDROPOFF
                    | COMPATF_LIGHT
                    | COMPATF_MASKEDMIDTEX,
                COMPATF2_FLOORMOVE | COMPATF2_EXPLODE1 | COMPATF2_NOMBF21,
            ),
            Self::Boom => CompatFlags::from_bits(
                COMPATF_TRACE
                    | COMPATF_SOUNDTARGET
                    | COMPATF_BOOMSCROLL
                    | COMPATF_MISSILECLIP
                    | COMPATF_MASKEDMIDTEX,
                COMPATF2_EXPLODE1 | COMPATF2_NOMBF21,
            ),
            Self::Mbf => CompatFlags::from_bits(
                COMPATF_TRACE
                    | COMPATF_SOUNDTARGET
                    | COMPATF_BOOMSCROLL
                    | COMPATF_MISSILECLIP
                    | COMPATF_MUSHROOM
                    | COMPATF_MBFMONSTERMOVE
                    | COMPATF_NOBLOCKFRIENDS
                    | COMPATF_MASKEDMIDTEX,
                COMPATF2_EXPLODE1 | COMPATF2_AVOID_HAZARDS | COMPATF2_STAYONLIFT | COMPATF2_NOMBF21,
            ),
            Self::Mbf21 => CompatFlags::from_bits(
                COMPATF_TRACE
                    | COMPATF_SOUNDTARGET
                    | COMPATF_BOOMSCROLL
                    | COMPATF_MISSILECLIP
                    | COMPATF_CROSSDROPOFF
                    | COMPATF_MUSHROOM
                    | COMPATF_MBFMONSTERMOVE
                    | COMPATF_NOBLOCKFRIENDS
                    | COMPATF_MASKEDMIDTEX,
                COMPATF2_EXPLODE1 | COMPATF2_AVOID_HAZARDS | COMPATF2_STAYONLIFT,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mbf21_enables_mbf21_features() {
        let c = CompatMode::Mbf21.flags();
        assert!(c.mbf21_enabled());
        assert!(c.has(COMPATF_MBFMONSTERMOVE));
        assert!(!c.has2(COMPATF2_NOMBF21));
    }

    #[test]
    fn boom_disables_mbf21() {
        let c = CompatMode::Boom.flags();
        assert!(!c.mbf21_enabled());
        assert!(c.has(COMPATF_BOOMSCROLL));
    }

    #[test]
    fn doom_preset_has_shorttex() {
        assert!(CompatMode::Doom.flags().has(COMPATF_SHORTTEX));
    }
}

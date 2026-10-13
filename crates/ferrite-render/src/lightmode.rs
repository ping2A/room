//! Sector lighting modes (GZDoom `lightmode` + Ferrite `ClassicPalette`).

/// How sector light diminishes with distance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum LightMode {
    /// GZDoom classic hardware (0).
    #[default]
    Standard = 0,
    /// Brighter curve (1).
    Bright = 1,
    /// Dark / Doom diminishing (2).
    Doom = 2,
    /// Software-renderer equation (8).
    DoomSoftware = 8,
    /// Build-engine-like (16).
    Build = 16,
    /// `DoomSoftware` + COLORMAP LUT (Ferrite).
    ClassicPalette = 32,
}

impl LightMode {
    /// Parse from MAPINFO / CVar integer.
    #[must_use]
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Bright,
            2 => Self::Doom,
            8 => Self::DoomSoftware,
            16 => Self::Build,
            32 => Self::ClassicPalette,
            _ => Self::Standard,
        }
    }
}

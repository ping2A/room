//! Lump namespaces (GZDoom `ENamespace`).

/// Resource namespace used for short-name lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum Namespace {
    /// Unnamespaced / global lumps.
    #[default]
    Global = 0,
    /// `sprites/` / `S_START`…`S_END`.
    Sprites,
    /// `flats/` / `F_START`…`F_END`.
    Flats,
    /// `colormaps/` / `C_START`…`C_END`.
    Colormaps,
    /// `acs/` / `A_START`…`A_END`.
    Acs,
    /// `textures/` / `TX_START`…`TX_END`.
    NewTextures,
    /// Blood raw (unused in M1).
    BloodRaw,
    /// Blood sfx (unused in M1).
    BloodSfx,
    /// Blood misc (unused in M1).
    BloodMisc,
    /// Strife voices.
    Voices,
    /// `hires/` / `HI_START`…`HI_END`.
    HiRes,
    /// `voxels/` / `V_START`…`V_END`.
    Voxels,
    /// `patches/` / `P_START`…`P_END` (informational).
    Patches,
    /// `graphics/`.
    Graphics,
    /// `sounds/`.
    Sounds,
    /// `music/`.
    Music,
    /// `fonts/`.
    Fonts,
    /// `models/`.
    Models,
    /// `shaders/`.
    Shaders,
    /// `brightmaps/`.
    Brightmaps,
    /// `materials/`.
    Materials,
    /// `filter/` (remapped before mount).
    Filter,
    /// `maps/`.
    Maps,
    /// Skins.
    Skins,
}

impl Namespace {
    /// Map a top-level archive directory name (ASCII, case-insensitive) to a namespace.
    #[must_use]
    pub fn from_dir(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        Some(match lower.as_str() {
            "sprites" => Self::Sprites,
            "flats" => Self::Flats,
            "textures" => Self::NewTextures,
            "patches" => Self::Patches,
            "graphics" => Self::Graphics,
            "colormaps" => Self::Colormaps,
            "fonts" => Self::Fonts,
            "sounds" => Self::Sounds,
            "music" => Self::Music,
            "voices" => Self::Voices,
            "acs" => Self::Acs,
            "voxels" => Self::Voxels,
            "hires" => Self::HiRes,
            "models" => Self::Models,
            "shaders" => Self::Shaders,
            "brightmaps" => Self::Brightmaps,
            "materials" => Self::Materials,
            "filter" => Self::Filter,
            "maps" => Self::Maps,
            "skins" => Self::Skins,
            _ => return None,
        })
    }

    /// Stable display name for CLI/tools.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Sprites => "sprites",
            Self::Flats => "flats",
            Self::Colormaps => "colormaps",
            Self::Acs => "acs",
            Self::NewTextures => "textures",
            Self::BloodRaw => "bloodraw",
            Self::BloodSfx => "bloodsfx",
            Self::BloodMisc => "bloodmisc",
            Self::Voices => "voices",
            Self::HiRes => "hires",
            Self::Voxels => "voxels",
            Self::Patches => "patches",
            Self::Graphics => "graphics",
            Self::Sounds => "sounds",
            Self::Music => "music",
            Self::Fonts => "fonts",
            Self::Models => "models",
            Self::Shaders => "shaders",
            Self::Brightmaps => "brightmaps",
            Self::Materials => "materials",
            Self::Filter => "filter",
            Self::Maps => "maps",
            Self::Skins => "skins",
        }
    }
}

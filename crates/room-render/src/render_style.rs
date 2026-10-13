//! GZDoom `ERenderStyle` → wgpu blend state (M10).

/// GZDoom render style ids (order matches `ERenderStyle` / `STYLE_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RenderStyle {
    /// No draw.
    None = 0,
    /// Opaque / alpha-test.
    #[default]
    Normal = 1,
    /// Spectre fuzz (shader variant; blend ≈ translucent).
    Fuzzy = 2,
    /// Lost-soul translucent red.
    SoulTrans = 3,
    /// OptFuzzy (same blend as Fuzzy).
    OptFuzzy = 4,
    /// Stencil (fillcolour, alpha).
    Stencil = 5,
    /// Alpha blend.
    Translucent = 6,
    /// Additive.
    Add = 7,
    /// Shaded (modulate by fillcolour).
    Shaded = 8,
    /// Translucent stencil.
    TranslucentStencil = 9,
    /// Soft shadow (darken).
    Shadow = 10,
    /// Subtractive.
    Subtract = 11,
    /// Additive stencil.
    AddStencil = 12,
    /// Additive shaded.
    AddShaded = 13,
    /// Colour blend (lerp toward fillcolour).
    ColorBlend = 14,
    /// Colour add.
    ColorAdd = 15,
    /// Multiply.
    Multiply = 16,
}

impl RenderStyle {
    /// Parse a DECORATE / ZScript style name (case-insensitive).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "normal" => Self::Normal,
            "fuzzy" => Self::Fuzzy,
            "soultrans" => Self::SoulTrans,
            "optfuzzy" => Self::OptFuzzy,
            "stencil" => Self::Stencil,
            "translucent" => Self::Translucent,
            "add" | "additive" => Self::Add,
            "shaded" => Self::Shaded,
            "translucentstencil" => Self::TranslucentStencil,
            "shadow" => Self::Shadow,
            "subtract" | "subtractive" => Self::Subtract,
            "addstencil" => Self::AddStencil,
            "addshaded" => Self::AddShaded,
            "colorblend" | "colourblend" => Self::ColorBlend,
            "coloradd" | "colouradd" => Self::ColorAdd,
            "multiply" => Self::Multiply,
            _ => return None,
        })
    }

    /// Whether the style writes no colour (skip draw).
    #[must_use]
    pub const fn is_invisible(self) -> bool {
        matches!(self, Self::None)
    }

    /// Whether fuzz / spectre shader path is needed.
    #[must_use]
    pub const fn needs_fuzz(self) -> bool {
        matches!(self, Self::Fuzzy | Self::OptFuzzy)
    }

    /// Whether the fragment should force stencil/fill colour.
    #[must_use]
    pub const fn uses_stencil_color(self) -> bool {
        matches!(
            self,
            Self::Stencil | Self::TranslucentStencil | Self::AddStencil
        )
    }

    /// Whether shaded / colour-blend styles modulate by fill colour.
    #[must_use]
    pub const fn uses_fill_modulate(self) -> bool {
        matches!(
            self,
            Self::Shaded | Self::AddShaded | Self::ColorBlend | Self::ColorAdd
        )
    }

    /// Apply fill colour for stencil / shaded styles (`rgb` albedo, `fill` 0..=1).
    #[must_use]
    pub fn apply_fill_color(self, rgb: [f32; 3], fill: [f32; 3], alpha: f32) -> [f32; 4] {
        match self {
            Self::Stencil | Self::TranslucentStencil | Self::AddStencil => {
                [fill[0], fill[1], fill[2], alpha]
            }
            Self::Shaded | Self::AddShaded => {
                [rgb[0] * fill[0], rgb[1] * fill[1], rgb[2] * fill[2], alpha]
            }
            Self::ColorBlend => [
                rgb[0] * (1.0 - alpha) + fill[0] * alpha,
                rgb[1] * (1.0 - alpha) + fill[1] * alpha,
                rgb[2] * (1.0 - alpha) + fill[2] * alpha,
                1.0,
            ],
            Self::ColorAdd => [
                (rgb[0] + fill[0] * alpha).min(1.0),
                (rgb[1] + fill[1] * alpha).min(1.0),
                (rgb[2] + fill[2] * alpha).min(1.0),
                alpha,
            ],
            Self::SoulTrans => [rgb[0], rgb[1] * 0.25, rgb[2] * 0.25, alpha * 0.75],
            Self::Shadow => [0.0, 0.0, 0.0, alpha * 0.5],
            _ => [rgb[0], rgb[1], rgb[2], alpha],
        }
    }

    /// wgpu colour blend for this style (RGB; alpha usually follows).
    #[must_use]
    pub fn blend_state(self) -> Option<wgpu::BlendState> {
        use wgpu::{BlendComponent, BlendFactor, BlendOperation, BlendState};
        let alpha = BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        };
        let color = match self {
            Self::None => return None,
            Self::Normal => {
                return Some(BlendState {
                    color: BlendComponent::REPLACE,
                    alpha: BlendComponent::REPLACE,
                });
            }
            Self::Fuzzy
            | Self::OptFuzzy
            | Self::SoulTrans
            | Self::Translucent
            | Self::Stencil
            | Self::TranslucentStencil
            | Self::Shaded
            | Self::ColorBlend
            | Self::Shadow => BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::OneMinusSrcAlpha,
                operation: BlendOperation::Add,
            },
            Self::Add | Self::AddStencil | Self::AddShaded | Self::ColorAdd => BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            Self::Subtract => BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::ReverseSubtract,
            },
            Self::Multiply => BlendComponent {
                src_factor: BlendFactor::Dst,
                dst_factor: BlendFactor::Zero,
                operation: BlendOperation::Add,
            },
        };
        Some(BlendState { color, alpha })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_names() {
        assert_eq!(RenderStyle::from_name("Add"), Some(RenderStyle::Add));
        assert_eq!(
            RenderStyle::from_name("translucent"),
            Some(RenderStyle::Translucent)
        );
        assert_eq!(RenderStyle::from_name("nope"), None);
    }

    #[test]
    fn additive_uses_one_dst() {
        let b = RenderStyle::Add.blend_state().unwrap();
        assert_eq!(b.color.dst_factor, wgpu::BlendFactor::One);
    }

    #[test]
    fn none_skips_blend() {
        assert!(RenderStyle::None.blend_state().is_none());
        assert!(RenderStyle::None.is_invisible());
    }

    #[test]
    fn fuzz_flags() {
        assert!(RenderStyle::Fuzzy.needs_fuzz());
        assert!(!RenderStyle::Normal.needs_fuzz());
        assert!(RenderStyle::Stencil.uses_stencil_color());
    }

    #[test]
    fn subtract_is_reverse() {
        let b = RenderStyle::Subtract.blend_state().unwrap();
        assert_eq!(b.color.operation, wgpu::BlendOperation::ReverseSubtract);
    }

    #[test]
    fn apply_fill_color_stencil_and_shaded() {
        let st = RenderStyle::Stencil.apply_fill_color([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.5);
        assert!((st[1] - 1.0).abs() < 1e-4);
        assert!((st[3] - 0.5).abs() < 1e-4);
        let sh = RenderStyle::Shaded.apply_fill_color([1.0, 1.0, 1.0], [0.5, 0.25, 0.0], 1.0);
        assert!((sh[0] - 0.5).abs() < 1e-4);
        assert!(RenderStyle::Shaded.uses_fill_modulate());
    }
}

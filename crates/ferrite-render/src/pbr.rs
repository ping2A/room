//! Metallic/roughness PBR helpers (M10 bootstrap — CPU + shader contract).

/// glTF-style packed metal-rough sample: `G` = roughness, `B` = metallic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetalRough {
    /// Metallic 0..=1.
    pub metallic: f32,
    /// Roughness 0..=1.
    pub roughness: f32,
}

impl MetalRough {
    /// Decode from an RGBA texel (unused R/A).
    #[must_use]
    pub fn from_rgba(rgba: [f32; 4]) -> Self {
        Self {
            metallic: rgba[2].clamp(0.0, 1.0),
            roughness: rgba[1].clamp(0.0, 1.0),
        }
    }

    /// Dielectric default (rough plastic).
    #[must_use]
    pub fn dielectric(roughness: f32) -> Self {
        Self {
            metallic: 0.0,
            roughness: roughness.clamp(0.04, 1.0),
        }
    }
}

/// GGX / Trowbridge-Reitz normal distribution `D(n,h,α)`.
#[must_use]
pub fn ggx_d(ndoth: f32, roughness: f32) -> f32 {
    let a = roughness.max(0.04).powi(2);
    let a2 = a * a;
    let d = ndoth * ndoth * (a2 - 1.0) + 1.0;
    a2 / (std::f32::consts::PI * d * d).max(1e-6)
}

/// Schlick Fresnel.
#[must_use]
pub fn fresnel_schlick(vdoth: f32, f0: [f32; 3]) -> [f32; 3] {
    let x = (1.0 - vdoth.clamp(0.0, 1.0)).powi(5);
    [
        f0[0] + (1.0 - f0[0]) * x,
        f0[1] + (1.0 - f0[1]) * x,
        f0[2] + (1.0 - f0[2]) * x,
    ]
}

/// Specular F0 from albedo + metallic (dielectric base 0.04).
#[must_use]
pub fn specular_f0(albedo: [f32; 3], metallic: f32) -> [f32; 3] {
    let m = metallic.clamp(0.0, 1.0);
    [
        0.04 * (1.0 - m) + albedo[0] * m,
        0.04 * (1.0 - m) + albedo[1] * m,
        0.04 * (1.0 - m) + albedo[2] * m,
    ]
}

/// Compact Cook-Torrance specular lobe (no geometry term — bootstrap).
#[must_use]
pub fn pbr_specular(ndoth: f32, vdoth: f32, albedo: [f32; 3], mr: MetalRough) -> [f32; 3] {
    let d = ggx_d(ndoth, mr.roughness);
    let f0 = specular_f0(albedo, mr.metallic);
    let f = fresnel_schlick(vdoth, f0);
    [f[0] * d, f[1] * d, f[2] * d]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metal_rough_from_gltf_pack() {
        let mr = MetalRough::from_rgba([0.0, 0.25, 0.8, 1.0]);
        assert!((mr.roughness - 0.25).abs() < 1e-5);
        assert!((mr.metallic - 0.8).abs() < 1e-5);
    }

    #[test]
    fn ggx_peaks_at_mirror() {
        let sharp = ggx_d(1.0, 0.05);
        let dull = ggx_d(1.0, 0.9);
        assert!(sharp > dull * 10.0);
    }

    #[test]
    fn metal_f0_follows_albedo() {
        let f0 = specular_f0([1.0, 0.0, 0.0], 1.0);
        assert!((f0[0] - 1.0).abs() < 1e-5);
        assert!(f0[1].abs() < 1e-5);
    }

    #[test]
    fn dielectric_f0_is_004() {
        let f0 = specular_f0([1.0, 0.0, 0.0], 0.0);
        assert!((f0[0] - 0.04).abs() < 1e-5);
    }
}

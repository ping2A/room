//! GPU vertex layouts (bytemuck-castable).

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3, Vec4};

/// Wall / sprite quad corner (streamed every frame).
///
/// Layout must stay in lockstep with [`FlatVertex`] (same unlit pipeline attrs).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WallVertex {
    /// XYZ map units (Z up).
    pub pos: [f32; 3],
    /// UV.
    pub uv: [f32; 2],
    /// Light level 0–255 as 0–1.
    pub light: f32,
    /// `x` = alpha; `y` = style: `0` solid (holes→black), `0.5` masked discard, `1` fuzz.
    pub _pad: [f32; 2],
    /// Lightmap atlas UV (ZDRay / FLMP surface).
    pub lm_uv: [f32; 2],
    /// World-space geometric normal (docs/29 U1).
    pub normal: [f32; 3],
    /// World-space tangent XYZ + bitangent sign in `w` (docs/29 U2 TBN).
    pub tangent: [f32; 4],
}

impl WallVertex {
    /// Construct (`alpha` = 1, solid / no mask, `lm_uv` = 0, N=+Z, T=+X).
    #[must_use]
    pub fn new(pos: Vec3, uv: Vec2, light: f32) -> Self {
        Self {
            pos: pos.to_array(),
            uv: uv.to_array(),
            light,
            _pad: [1.0, 0.0],
            lm_uv: [0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        }
    }

    /// Set opacity in `_pad[0]` (shader `@location(3).x`). A negative value
    /// flags `RenderStyle Add` (the shader adds the colour instead of
    /// blending), so the sign must survive: clamping to 0 made every additive
    /// sprite (Brutal Doom fireball trails, sparks, flares) invisible.
    #[must_use]
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self._pad[0] = alpha.clamp(-1.0, 1.0);
        self
    }

    /// Discard transparent texels (sprites, two-sided midtextures / grates).
    ///
    /// Solid one-sided / upper / lower walls stay at `0` so TEXTUREX holes are
    /// filled black instead of showing the room behind.
    #[must_use]
    pub fn with_masked(mut self) -> Self {
        if self._pad[1] < 0.45 {
            self._pad[1] = 0.5;
        }
        self
    }

    /// Spectre fuzz (`_pad[1] = 1`); also implies masked holes.
    #[must_use]
    pub fn with_fuzz(mut self, enabled: bool) -> Self {
        if enabled {
            self._pad[1] = 1.0;
        }
        self
    }

    /// Set lightmap atlas UV.
    #[must_use]
    pub fn with_lm_uv(mut self, lm: Vec2) -> Self {
        self.lm_uv = lm.to_array();
        self
    }

    /// World-space normal + tangent frame (`tangent.w` = bitangent sign).
    #[must_use]
    pub fn with_tbn(mut self, normal: Vec3, tangent: Vec4) -> Self {
        self.normal = normal.to_array();
        self.tangent = tangent.to_array();
        self
    }
}

/// Flat triangle vertex (same GPU layout as [`WallVertex`]).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FlatVertex {
    /// XYZ.
    pub pos: [f32; 3],
    /// UV (world XY / 64 for Doom flats).
    pub uv: [f32; 2],
    /// Light 0–1.
    pub light: f32,
    /// `x` = alpha; `y` = same style codes as [`WallVertex`] (`0` = solid fill).
    pub _pad: [f32; 2],
    /// Lightmap atlas UV.
    pub lm_uv: [f32; 2],
    /// World-space geometric normal.
    pub normal: [f32; 3],
    /// World-space tangent XYZ + bitangent sign.
    pub tangent: [f32; 4],
}

impl FlatVertex {
    /// Construct (`alpha` = 1, solid fill, `lm_uv` = 0, N=+Z, T=+X).
    #[must_use]
    pub fn new(pos: Vec3, uv: Vec2, light: f32) -> Self {
        Self {
            pos: pos.to_array(),
            uv: uv.to_array(),
            light,
            _pad: [1.0, 0.0],
            lm_uv: [0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        }
    }

    /// Set opacity in `_pad[0]` (shader `@location(3)`).
    #[must_use]
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self._pad[0] = alpha.clamp(0.0, 1.0);
        self
    }

    /// Set lightmap atlas UV.
    #[must_use]
    pub fn with_lm_uv(mut self, lm: Vec2) -> Self {
        self.lm_uv = lm.to_array();
        self
    }

    /// World-space normal + tangent frame.
    #[must_use]
    pub fn with_tbn(mut self, normal: Vec3, tangent: Vec4) -> Self {
        self.normal = normal.to_array();
        self.tangent = tangent.to_array();
        self
    }
}

/// Shared view uniforms (std140-ish packing for wgpu).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ViewUniforms {
    /// Column-major view-projection.
    pub view_proj: [[f32; 4]; 4],
    /// Eye XYZ + pad.
    pub eye: [f32; 4],
    /// `x` = [`crate::LightMode`] as u8, `y` = globVis, `z` = fog density,
    /// `w` = [`crate::FuzzType`] as u8 (`gl_fuzztype`).
    pub light_params: [f32; 4],
    /// World clip plane `nx,ny,nz,d` (`n·p + d > 0` discarded). Zero xyz = off.
    pub clip_plane: [f32; 4],
    /// `x` = lightmap blend strength (0 = off), `y` = output gain, `z` = sRGB
    /// encode, `w` = inverse colormap (invulnerability).
    pub lightmap_params: [f32; 4],
    /// UZDoom 5.0 thick fog: `x` = `ThickFogDistance` (0 = off), `y` =
    /// `ThickFogMultiplier`; past the distance the fog distance grows by
    /// `multiplier × (d − distance)` (`main.fp`). `zw` unused.
    pub fog_params: [f32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_and_flat_layouts_match() {
        assert_eq!(
            std::mem::size_of::<WallVertex>(),
            std::mem::size_of::<FlatVertex>()
        );
        assert_eq!(
            std::mem::align_of::<WallVertex>(),
            std::mem::align_of::<FlatVertex>()
        );
    }

    #[test]
    fn with_tbn_stores_normal_and_tangent() {
        let v = WallVertex::new(Vec3::ZERO, Vec2::ZERO, 1.0)
            .with_tbn(Vec3::new(0.0, 1.0, 0.0), Vec4::new(1.0, 0.0, 0.0, -1.0));
        assert_eq!(v.normal, [0.0, 1.0, 0.0]);
        assert_eq!(v.tangent, [1.0, 0.0, 0.0, -1.0]);
    }
}

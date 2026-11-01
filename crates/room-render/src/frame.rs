//! Shared frame graph targets + post chain (`docs/29` §3, SH0–SH1).
//!
//! Shells draw the 3D scene (and zones) into [`FrameTargets::scene_color`], then
//! call [`PostChain::apply`] when [`crate::settings::RenderSettings::needs_post`]
//! is true. HUD draws into the returned LDR view (or directly into scene when
//! post is off).

use crate::DEPTH_STENCIL_FORMAT;
use crate::bloom::BloomPass;
use crate::device::{GpuContext, GpuTier};
use crate::post::{PostPass, PostUniforms, TonemapMode};
use crate::settings::{AaMode, RenderSettings};
use crate::ssao::{SsaoPass, depth_only_view};

/// GPU textures for one frame size / scale.
#[allow(missing_docs)]
pub struct FrameTargets {
    /// Scene colour (HDR on Full when half-float RTs work; else LDR linear).
    pub scene_color: wgpu::Texture,
    pub scene_color_view: wgpu::TextureView,
    /// Aux G-buffer-lite MRT (octahedral normal RG + flags B; Metal stencil).
    pub scene_aux: wgpu::Texture,
    pub scene_aux_view: wgpu::TextureView,
    /// Depth + stencil.
    pub depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    /// Ping-pong for SSAO / bloom composition (same format as scene).
    pub post_a: wgpu::Texture,
    pub post_a_view: wgpu::TextureView,
    pub post_b: wgpu::Texture,
    pub post_b_view: wgpu::TextureView,
    /// Final LDR before HUD (swapchain format).
    pub ldr: wgpu::Texture,
    pub ldr_view: wgpu::TextureView,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Scene colour format (HDR or LDR).
    pub scene_format: wgpu::TextureFormat,
    /// Present / HUD format.
    pub ldr_format: wgpu::TextureFormat,
}

impl FrameTargets {
    /// Allocate targets for `width`×`height`.
    ///
    /// `ldr_format` is the swapchain / present format. Scene uses `Rgba16Float`
    /// on Full when supported by the caller; otherwise `ldr_format` (linear path
    /// still works via tonemap).
    #[must_use]
    pub fn new(
        gpu: &GpuContext,
        width: u32,
        height: u32,
        scene_format: wgpu::TextureFormat,
        ldr_format: wgpu::TextureFormat,
    ) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        let extent = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let color_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        let mk_color = |label: &str, format: wgpu::TextureFormat| {
            let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: color_usage,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            (tex, view)
        };
        let (scene_color, scene_color_view) = mk_color("frame-scene", scene_format);
        let (post_a, post_a_view) = mk_color("frame-post-a", scene_format);
        let (post_b, post_b_view) = mk_color("frame-post-b", scene_format);
        let (ldr, ldr_view) = mk_color("frame-ldr", ldr_format);
        let scene_aux = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-aux"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // docs/29 U16: octahedral normal (RG) + flags (B).
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scene_aux_view = scene_aux.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_STENCIL_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            scene_color,
            scene_color_view,
            scene_aux,
            scene_aux_view,
            depth,
            depth_view,
            post_a,
            post_a_view,
            post_b,
            post_b_view,
            ldr,
            ldr_view,
            width: w,
            height: h,
            scene_format,
            ldr_format,
        }
    }

    /// Prefer half-float scene on Full tier; else use `ldr_format`.
    #[must_use]
    pub fn pick_scene_format(
        tier: GpuTier,
        ldr_format: wgpu::TextureFormat,
    ) -> wgpu::TextureFormat {
        match tier {
            GpuTier::Full => wgpu::TextureFormat::Rgba16Float,
            GpuTier::Downlevel => ldr_format,
        }
    }

    /// Recreate if size or formats changed.
    pub fn ensure(
        &mut self,
        gpu: &GpuContext,
        width: u32,
        height: u32,
        scene_format: wgpu::TextureFormat,
        ldr_format: wgpu::TextureFormat,
    ) {
        if self.width == width.max(1)
            && self.height == height.max(1)
            && self.scene_format == scene_format
            && self.ldr_format == ldr_format
        {
            return;
        }
        *self = Self::new(gpu, width, height, scene_format, ldr_format);
    }
}

/// Inputs for one post-chain invocation.
#[derive(Clone, Copy, Debug)]
pub struct PostChainInputs<'a> {
    /// Active toggles.
    pub settings: &'a RenderSettings,
    /// Screen blend RGBA (damage / bonus / suit); alpha = strength.
    pub blend: [f32; 4],
    /// Framebuffer width.
    pub width: u32,
    /// Framebuffer height.
    pub height: u32,
    /// Elapsed seconds (grain / CRT animation).
    pub time: f32,
}

/// Owns bloom / SSAO / tonemap passes and runs them in order.
pub struct PostChain {
    /// Tonemap / FXAA / screen blend / bloom composite.
    pub post: PostPass,
    /// Kawase mip bloom pyramid.
    pub bloom: BloomPass,
    /// Screen-space AO (Full tier).
    pub ssao: SsaoPass,
    /// Scene format the bloom/SSAO pipelines were built for.
    scene_format: wgpu::TextureFormat,
    /// LDR / present format the tonemap pipeline was built for (R10).
    ldr_format: wgpu::TextureFormat,
}

impl PostChain {
    /// Create passes targeting `ldr_format` for the final tonemap; bloom/SSAO
    /// use `scene_format`.
    pub fn new(
        gpu: &GpuContext,
        scene_format: wgpu::TextureFormat,
        ldr_format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            post: PostPass::new(gpu, ldr_format),
            bloom: BloomPass::new(gpu, scene_format),
            ssao: SsaoPass::new(gpu, scene_format),
            scene_format,
            ldr_format,
        }
    }

    /// Rebuild if the scene or LDR colour format changed (HDR ↔ LDR / R10).
    pub fn ensure_formats(
        &mut self,
        gpu: &GpuContext,
        scene_format: wgpu::TextureFormat,
        ldr_format: wgpu::TextureFormat,
    ) {
        if self.scene_format != scene_format {
            self.bloom = BloomPass::new(gpu, scene_format);
            self.ssao = SsaoPass::new(gpu, scene_format);
            self.scene_format = scene_format;
        }
        if self.ldr_format != ldr_format {
            self.post = PostPass::new(gpu, ldr_format);
            self.ldr_format = ldr_format;
        }
    }

    /// Upload PLAYPAL for palette tonemap.
    pub fn set_playpal(&self, gpu: &GpuContext, rgb: &[u8; 768]) {
        self.post.set_playpal(gpu, rgb);
    }

    /// Run SSAO → bloom → tonemap/FXAA/blend into `targets.ldr`.
    pub fn apply(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        targets: &FrameTargets,
        inputs: PostChainInputs<'_>,
    ) {
        self.apply_views(
            gpu,
            encoder,
            &targets.scene_color_view,
            &targets.depth,
            &targets.post_a_view,
            &targets.ldr_view,
            inputs,
        );
    }

    /// Same as [`Self::apply`] with explicit views (shells that own their own
    /// colour / depth / ping / LDR textures).
    pub fn apply_views(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth_tex: &wgpu::Texture,
        ping: &wgpu::TextureView,
        ldr: &wgpu::TextureView,
        inputs: PostChainInputs<'_>,
    ) {
        let s = inputs.settings;
        let w = inputs.width.max(1);
        let h = inputs.height.max(1);

        let mut src_view = scene;

        // SSAO (Full tier only; CVar-gated).
        if s.ssao > 0 && gpu.tier == GpuTier::Full {
            self.ssao.uniforms.configure(true, w, h);
            self.ssao.uniforms.params[3] = s.ssao_strength;
            let depth = depth_only_view(depth_tex);
            self.ssao.apply(gpu, encoder, src_view, &depth, ping);
            src_view = ping;
        }

        let bloom_threshold = if s.bloom {
            (1.2 - s.bloom_amount * 0.5).clamp(0.15, 1.5)
        } else {
            0.0
        };
        let bloom_view = if s.bloom {
            self.bloom
                .apply(gpu, encoder, src_view, w, h, bloom_threshold)
        } else {
            None
        };
        let bloom_intensity = if bloom_view.is_some() {
            (0.35 + s.bloom_amount * 0.65).clamp(0.0, 1.5)
        } else {
            0.0
        };

        // SMAA/TAA fall back to FXAA until dedicated passes land.
        let aa_on = matches!(s.aa, AaMode::Fxaa | AaMode::Smaa | AaMode::Taa);
        let tonemap = if s.tonemap == TonemapMode::None && s.linear_scene() {
            TonemapMode::RoomSoft
        } else {
            s.tonemap
        };
        self.post.uniforms = PostUniforms::new(
            tonemap,
            s.exposure,
            s.gamma,
            bloom_intensity,
            inputs.blend,
        );
        self.post.uniforms.set_fxaa(aa_on, w, h);
        let crt = if s.crt { 0.65 } else { 0.0 };
        let damage_pulse = if s.damage_fx {
            inputs.blend[3].clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.post.uniforms.set_look_fx(
            s.vignette,
            s.grain,
            s.chromatic,
            crt,
            s.dither > 0,
            s.banding,
            s.sharpen,
            damage_pulse,
            inputs.time,
        );
        self.post
            .apply(gpu, encoder, src_view, bloom_view, ldr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_format_full_is_hdr() {
        assert_eq!(
            FrameTargets::pick_scene_format(GpuTier::Full, wgpu::TextureFormat::Bgra8UnormSrgb),
            wgpu::TextureFormat::Rgba16Float
        );
        assert_eq!(
            FrameTargets::pick_scene_format(
                GpuTier::Downlevel,
                wgpu::TextureFormat::Rgba8UnormSrgb
            ),
            wgpu::TextureFormat::Rgba8UnormSrgb
        );
    }
}

//! Offscreen render target for golden images / headless frames.

use crate::camera::Camera;
use crate::device::GpuContext;
use crate::pass::UnlitPass;
use crate::post::PostPass;
use crate::scene::{MaterialRange, SceneLists};
use thiserror::Error;

/// Errors from offscreen capture.
#[derive(Debug, Error)]
pub enum OffscreenError {
    /// GPU init / map failed.
    #[error("{0}")]
    Message(String),
}

/// RGBA8 offscreen surface + depth/stencil (+ optional stencil aux MRT).
pub struct OffscreenTarget {
    /// Color texture.
    pub color: wgpu::Texture,
    /// Color view.
    pub color_view: wgpu::TextureView,
    /// Dummy R8 target so Metal stencil Replace can write without touching RGB.
    pub stencil_aux: wgpu::Texture,
    /// View of [`Self::stencil_aux`].
    pub stencil_aux_view: wgpu::TextureView,
    /// Depth+stencil texture.
    pub depth: wgpu::Texture,
    /// Depth+stencil view.
    pub depth_view: wgpu::TextureView,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Format.
    pub format: wgpu::TextureFormat,
}

impl OffscreenTarget {
    /// Create an RGBA8 + depth24+stencil8 target.
    #[must_use]
    pub fn new(gpu: &GpuContext, width: u32, height: u32) -> Self {
        Self::with_format(gpu, width, height, wgpu::TextureFormat::Rgba8UnormSrgb)
    }

    /// Target in `format`, which must match the pass that draws into it
    /// (a window's [`UnlitPass`] uses the surface format, often BGRA8).
    #[must_use]
    pub fn with_format(
        gpu: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            // TEXTURE_BINDING so [`crate::post::PostPass`] can sample the scene.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let stencil_aux = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-stencil-aux"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Match scene MRT aux (docs/29 U16 G-buffer-lite).
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::DEPTH_STENCIL_FORMAT,
            // TEXTURE_BINDING for [`crate::ssao::SsaoPass`] depth sampling.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let stencil_aux_view = stencil_aux.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            color,
            color_view,
            stencil_aux,
            stencil_aux_view,
            depth,
            depth_view,
            width,
            height,
            format,
        }
    }
}

/// Draw one unlit frame into an offscreen target.
pub fn render_offscreen(
    gpu: &GpuContext,
    pass: &mut UnlitPass,
    target: &OffscreenTarget,
    camera: &Camera,
    scene: &SceneLists,
) {
    pass.prepare(gpu, camera, scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("offscreen-rp"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.stencil_aux_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Discard,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.draw(&mut rp, scene);
    }
    gpu.queue.submit(Some(encoder.finish()));
}

/// Render scene then apply [`PostPass`] into `post_dst` (same size as `target`).
pub fn render_offscreen_with_post(
    gpu: &GpuContext,
    pass: &mut UnlitPass,
    post: &PostPass,
    target: &OffscreenTarget,
    post_dst: &wgpu::TextureView,
    camera: &Camera,
    scene: &SceneLists,
) {
    pass.prepare(gpu, camera, scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen-post"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("offscreen-scene-rp"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.stencil_aux_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Discard,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.draw(&mut rp, scene);
    }
    post.apply(gpu, &mut encoder, &target.color_view, None, post_dst);
    gpu.queue.submit(Some(encoder.finish()));
}

/// Portal/mirror bootstrap: MRT stencil mask then Equal draw.
///
/// Metal requires colour-target-0 writes for stencil Replace; the Equal pass
/// overwrites mask pixels. Attachment 1 is a dummy R8 (pass layout / future use).
pub fn render_offscreen_stenciled(
    gpu: &GpuContext,
    pass: &mut UnlitPass,
    target: &OffscreenTarget,
    camera: &Camera,
    scene: &SceneLists,
    mask: &[MaterialRange],
) {
    pass.prepare(gpu, camera, scene);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen-stencil"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("offscreen-stencil-rp"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &target.stencil_aux_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Discard,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if !mask.is_empty() {
            pass.draw_stencil_mask(&mut rp, scene, mask, 1);
            pass.draw_stencil_equal(&mut rp, scene, 1);
        }
    }
    gpu.queue.submit(Some(encoder.finish()));
}

//! GLDEFS `HardwareShader PostProcess` shaders in the browser (Color World,
//! Red Noir, …): built from the mounted mods, switched and fed by the
//! scripts' `PPShader` calls, and run as on desktop (`room-app`): on the
//! finished picture (after the post chain, so in gamma space, as GZDoom's
//! shaders expect) with the weapon in it and the status bar not.

use room_defs::gldefs::HardwareShaderDef;
use room_render::{GpuContext, HardwarePostRunner, PostTimes};
use room_sim::zs_native_ops::PostShaderOp;

struct Shader {
    name: String,
    enabled: bool,
    runner: HardwarePostRunner,
}

/// Post shaders of one play session.
pub(crate) struct WebPost {
    /// `PlaySession::session_id` they were built for.
    pub session: u64,
    /// Target format they were built for.
    pub format: wgpu::TextureFormat,
    shaders: Vec<Shader>,
    /// Between two shaders (more than one switched on).
    scratch: Option<wgpu::Texture>,
}

impl WebPost {
    /// Compile `sources` (definition, GLSL) for a `format` scene target.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        session: u64,
        sources: Vec<(HardwareShaderDef, String)>,
    ) -> Self {
        let mut shaders = Vec::new();
        for (def, src) in sources {
            match room_render::compile_post_glsl(&def, &src) {
                Ok(compiled) => shaders.push(Shader {
                    name: def.name.clone(),
                    enabled: def.enabled,
                    runner: HardwarePostRunner::new(device, format, &compiled),
                }),
                Err(e) => web_sys::console::warn_1(
                    &format!("room-web: post shader {}: {e}", def.name).into(),
                ),
            }
        }
        if !shaders.is_empty() {
            web_sys::console::log_1(
                &format!("room-web: {} post shader(s) from mods", shaders.len()).into(),
            );
        }
        Self {
            session,
            format,
            shaders,
            scratch: None,
        }
    }

    /// Apply the scripts' `PPShader` calls.
    pub fn apply(&mut self, ops: Vec<PostShaderOp>) {
        for op in ops {
            match op {
                PostShaderOp::Enable { shader, on } => {
                    for s in self
                        .shaders
                        .iter_mut()
                        .filter(|s| s.name.eq_ignore_ascii_case(&shader))
                    {
                        s.enabled = on;
                    }
                }
                PostShaderOp::Uniform {
                    shader,
                    uniform,
                    values,
                } => {
                    for s in self
                        .shaders
                        .iter_mut()
                        .filter(|s| s.name.eq_ignore_ascii_case(&shader))
                    {
                        s.runner.set_uniform(&uniform, &values);
                    }
                }
            }
        }
    }

    /// Any shader switched on.
    pub fn any_enabled(&self) -> bool {
        self.shaders.iter().any(|s| s.enabled)
    }

    /// Run the switched-on shaders on `picture` (a texture of the format
    /// they were built for) and write the result to `out`. With more than one
    /// on, each but the last goes through a scratch texture back into
    /// `picture`.
    pub fn run_to_view(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        picture: &wgpu::Texture,
        out: &wgpu::TextureView,
        times: PostTimes,
    ) {
        let on: Vec<usize> = (0..self.shaders.len())
            .filter(|&i| self.shaders[i].enabled)
            .collect();
        let Some((&last, rest)) = on.split_last() else {
            return;
        };
        let size = wgpu::Extent3d {
            width: picture.width(),
            height: picture.height(),
            depth_or_array_layers: 1,
        };
        if !rest.is_empty()
            && self.scratch.as_ref().is_none_or(|t| {
                t.width() != size.width
                    || t.height() != size.height
                    || t.format() != picture.format()
            })
        {
            self.scratch = Some(gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("web-mod-post-scratch"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: picture.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            }));
        }
        for &i in rest {
            let Some(scratch) = self.scratch.as_ref() else {
                return;
            };
            let view = scratch.create_view(&wgpu::TextureViewDescriptor::default());
            self.shaders[i].runner.run(
                &gpu.device,
                &gpu.queue,
                encoder,
                picture,
                &view,
                times,
                &[],
            );
            encoder.copy_texture_to_texture(scratch.as_image_copy(), picture.as_image_copy(), size);
        }
        self.shaders[last]
            .runner
            .run(&gpu.device, &gpu.queue, encoder, picture, out, times, &[]);
    }
}

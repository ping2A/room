//! GLDEFS `HardwareShader PostProcess` shaders in the browser (Color World,
//! Red Noir, …): built from the mounted mods, switched and fed by the
//! scripts' `PPShader` calls, and run on the scene before the post chain,
//! as on desktop (`room-app`).

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
    shaders: Vec<Shader>,
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
        Self { session, shaders }
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

    /// Run the switched-on shaders on `scene` (through `ping`, copied back).
    pub fn run(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::Texture,
        ping: &wgpu::Texture,
        times: PostTimes,
    ) {
        let size = wgpu::Extent3d {
            width: scene.width().min(ping.width()),
            height: scene.height().min(ping.height()),
            depth_or_array_layers: 1,
        };
        for s in self.shaders.iter_mut().filter(|s| s.enabled) {
            s.runner
                .run_texture(&gpu.device, &gpu.queue, encoder, scene, ping, times, &[]);
            encoder.copy_texture_to_texture(ping.as_image_copy(), scene.as_image_copy(), size);
        }
    }
}

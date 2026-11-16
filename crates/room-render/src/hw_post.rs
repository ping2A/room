//! GZDoom / UZDoom custom post-process shaders (GLDEFS
//! `HardwareShader PostProcess <target> { … }`, docs/36 UZ6).
//!
//! The mod's GLSL `main()` is compiled with UZDoom's prolog: `InputTexture`,
//! the declared `Texture` samplers, `LastInputTexture` (the previous frame's
//! input), `TexCoord` / `FragColor`, the user `Uniform` block and the 5.0
//! automatic uniforms `InputTimeDelta`, `InputTime` and `InputTimeGame`.
//! naga translates and validates it; [`HardwarePostRunner`] draws it.
//!
//! Bind group 0 layout (shared by every compiled post shader):
//!
//! | binding | resource |
//! |---|---|
//! | 0 | `InputTexture` |
//! | 1 | linear clamp sampler (repeat sampler for user textures: 6) |
//! | 2 | automatic uniforms (`InputTimeDelta`, `InputTime`, `InputTimeGame`) |
//! | 3 | `LastInputTexture` |
//! | 4 | user uniform block |
//! | 6 | repeat sampler |
//! | 8 + k | user texture `k` (GLDEFS order) |

use crate::glsl_translate::glsl_to_wgsl;
use crate::hardware_shader::HardwareShaderError;
use room_defs::HardwareShaderDef;

/// First binding used by user `Texture` samplers.
pub const POST_USER_TEXTURE_BINDING: u32 = 8;

/// One field of the user uniform block (std140).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostUniformField {
    /// GLSL name.
    pub name: String,
    /// GLSL type (`float`, `int`, `vec2`, `vec3`, `vec4`).
    pub ty: String,
    /// Byte offset in the block.
    pub offset: u32,
    /// Number of 32-bit components.
    pub components: u32,
}

/// std140 layout of GLDEFS `Uniform <type> <name>` declarations, plus the
/// block size rounded up to 16 bytes (at least 16).
#[must_use]
pub fn post_uniform_layout(uniforms: &[(String, String)]) -> (Vec<PostUniformField>, u32) {
    let mut fields = Vec::new();
    let mut offset = 0u32;
    for (ty, name) in uniforms {
        let (size, align, components) = match ty.as_str() {
            "float" | "int" => (4, 4, 1),
            "vec2" => (8, 8, 2),
            "vec3" => (12, 16, 3),
            "vec4" => (16, 16, 4),
            _ => continue,
        };
        offset = offset.div_ceil(align) * align;
        fields.push(PostUniformField {
            name: name.clone(),
            ty: ty.clone(),
            offset,
            components,
        });
        offset += size;
    }
    let size = (offset.div_ceil(16) * 16).max(16);
    (fields, size)
}

/// A post shader compiled to WGSL with its uniform / texture layout.
#[derive(Clone, Debug)]
pub struct CompiledPostShader {
    /// GLDEFS `Name`.
    pub name: String,
    /// Post target (`scene`, `beforebloom`, `screen`).
    pub target: String,
    /// Fragment WGSL (entry point `main`).
    pub wgsl: String,
    /// User uniform block layout.
    pub uniforms: Vec<PostUniformField>,
    /// User uniform block size in bytes.
    pub uniform_size: u32,
    /// User texture sampler names → texture lump, in binding order.
    pub textures: Vec<(String, String)>,
    /// GLDEFS `Enabled`.
    pub enabled: bool,
}

/// UZDoom's custom post prolog for `def` (GLSL 450, Room's bindings).
#[must_use]
pub fn post_prolog(def: &HardwareShaderDef) -> String {
    let mut p = String::from("#version 450\n");
    for (name, value) in &def.defines {
        if value.is_empty() {
            p.push_str(&format!("#define {name}\n"));
        } else {
            p.push_str(&format!("#define {name} {value}\n"));
        }
    }
    p.push_str(
        "layout(set = 0, binding = 0) uniform texture2D room_InputTexture;\n\
         layout(set = 0, binding = 1) uniform sampler room_post_clamp;\n\
         layout(set = 0, binding = 2) uniform RoomPostAuto {\n\
         \x20   float InputTimeDelta;\n\
         \x20   float InputTime;\n\
         \x20   float InputTimeGame;\n\
         \x20   float room_auto_pad;\n\
         };\n\
         layout(set = 0, binding = 3) uniform texture2D room_LastInputTexture;\n\
         layout(set = 0, binding = 6) uniform sampler room_post_repeat;\n\
         #define InputTexture sampler2D(room_InputTexture, room_post_clamp)\n\
         #define LastInputTexture sampler2D(room_LastInputTexture, room_post_clamp)\n",
    );
    let (fields, _) = post_uniform_layout(&def.uniforms);
    if !fields.is_empty() {
        p.push_str("layout(set = 0, binding = 4) uniform RoomPostUser {\n");
        for f in &fields {
            p.push_str(&format!("    {} {};\n", f.ty, f.name));
        }
        p.push_str("};\n");
    }
    for (k, name) in def.textures.keys().enumerate() {
        let b = POST_USER_TEXTURE_BINDING + k as u32;
        p.push_str(&format!(
            "layout(set = 0, binding = {b}) uniform texture2D room_tex_{name};\n\
             #define {name} sampler2D(room_tex_{name}, room_post_repeat)\n"
        ));
    }
    p.push_str(
        "layout(location = 0) in vec2 TexCoord;\n\
         layout(location = 0) out vec4 FragColor;\n",
    );
    p
}

/// Compile a GZDoom post shader body (`void main() { … }`) against the
/// UZDoom prolog. A leading `#version` line in the lump is dropped.
pub fn compile_post_glsl(
    def: &HardwareShaderDef,
    source: &str,
) -> Result<CompiledPostShader, HardwareShaderError> {
    let body: String = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("#version"))
        .collect::<Vec<_>>()
        .join("\n");
    if body.trim().is_empty() {
        return Err(HardwareShaderError::Message(format!(
            "post shader {}: empty source",
            def.name
        )));
    }
    let glsl = format!("{}\n{body}\n", post_prolog(def));
    let wgsl = glsl_to_wgsl(&glsl, "fragment")?;
    let (uniforms, uniform_size) = post_uniform_layout(&def.uniforms);
    Ok(CompiledPostShader {
        name: def.name.clone(),
        target: def.target.clone(),
        wgsl,
        uniforms,
        uniform_size,
        textures: def
            .textures
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        enabled: def.enabled,
    })
}

/// Fullscreen-triangle vertex stage feeding `TexCoord` (location 0).
pub const POST_VS_WGSL: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(p[idx], 0.0, 1.0);
    // wgpu texture space: TexCoord (0,0) is the top-left texel, so the image
    // is not flipped (GL's bottom-left origin would mirror every sample).
    out.uv = vec2<f32>(p[idx].x * 0.5 + 0.5, 0.5 - p[idx].y * 0.5);
    return out;
}
"#;

/// The 5.0 automatic post uniforms for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PostTimes {
    /// Seconds since the previous frame (`InputTimeDelta`).
    pub delta: f32,
    /// Seconds since start (`InputTime`).
    pub time: f32,
    /// Game time in seconds, paused with the game (`InputTimeGame`).
    pub game: f32,
}

impl PostTimes {
    /// Bytes of the automatic uniform block (delta, time, game, pad).
    #[must_use]
    pub fn to_bytes(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        for (k, v) in [self.delta, self.time, self.game, 0.0].iter().enumerate() {
            out[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }
}

/// GPU runner for one [`CompiledPostShader`]: owns the pipeline, the uniform
/// buffers and the persistent `LastInputTexture`.
pub struct HardwarePostRunner {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    clamp: wgpu::Sampler,
    repeat: wgpu::Sampler,
    auto_buf: wgpu::Buffer,
    user_buf: wgpu::Buffer,
    user_data: Vec<u8>,
    fields: Vec<PostUniformField>,
    texture_count: usize,
    last: Option<(
        wgpu::Texture,
        wgpu::TextureView,
        u32,
        u32,
        wgpu::TextureFormat,
    )>,
}

impl HardwarePostRunner {
    /// Build the pipeline writing `format`.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shader: &CompiledPostShader,
    ) -> Self {
        let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hw-post-vs"),
            source: wgpu::ShaderSource::Wgsl(POST_VS_WGSL.into()),
        });
        let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&shader.name),
            source: wgpu::ShaderSource::Wgsl(shader.wgsl.as_str().into()),
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let samp_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let buf_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let mut entries = vec![
            tex_entry(0),
            samp_entry(1),
            buf_entry(2),
            tex_entry(3),
            buf_entry(4),
            samp_entry(6),
        ];
        for k in 0..shader.textures.len() {
            entries.push(tex_entry(POST_USER_TEXTURE_BINDING + k as u32));
        }
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hw-post-bgl"),
            entries: &entries,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hw-post-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hw-post-pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &vs,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let sampler = |mode, label| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        let buffer = |size: u32, label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: u64::from(size),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            pipeline,
            bgl,
            clamp: sampler(wgpu::AddressMode::ClampToEdge, "hw-post-clamp"),
            repeat: sampler(wgpu::AddressMode::Repeat, "hw-post-repeat"),
            auto_buf: buffer(16, "hw-post-auto"),
            user_buf: buffer(shader.uniform_size, "hw-post-user"),
            user_data: vec![0; shader.uniform_size as usize],
            fields: shader.uniforms.clone(),
            texture_count: shader.textures.len(),
            last: None,
        }
    }

    /// `PPShader.SetUniform1f/2f/3f/4f/1i`: write a user uniform by name
    /// (ints as their value). Returns `false` for an unknown name.
    pub fn set_uniform(&mut self, name: &str, values: &[f32]) -> bool {
        let Some(f) = self
            .fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
        else {
            return false;
        };
        for k in 0..f.components.min(values.len() as u32) {
            let at = (f.offset + k * 4) as usize;
            let bytes = if f.ty == "int" {
                (values[k as usize] as i32).to_le_bytes()
            } else {
                values[k as usize].to_le_bytes()
            };
            self.user_data[at..at + 4].copy_from_slice(&bytes);
        }
        true
    }

    /// Draw the shader from `input` into `output`, then keep `input` as the
    /// next frame's `LastInputTexture` (`input` needs `COPY_SRC`).
    /// `user_textures` follow GLDEFS order; missing ones sample black.
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Texture,
        output: &wgpu::TextureView,
        times: PostTimes,
        user_textures: &[&wgpu::TextureView],
    ) {
        let (w, h, format) = (input.width(), input.height(), input.format());
        let stale = self
            .last
            .as_ref()
            .is_none_or(|l| l.2 != w || l.3 != h || l.4 != format);
        if stale {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hw-post-last"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.last = Some((tex, view, w, h, format));
        }
        queue.write_buffer(&self.auto_buf, 0, &times.to_bytes());
        queue.write_buffer(&self.user_buf, 0, &self.user_data);
        let Some((last_tex, last_view, ..)) = self.last.as_ref() else {
            return;
        };
        let input_view = input.create_view(&wgpu::TextureViewDescriptor::default());
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&input_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&self.clamp),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: self.auto_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(last_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: self.user_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(&self.repeat),
            },
        ];
        for k in 0..self.texture_count {
            let view = user_textures.get(k).copied().unwrap_or(last_view);
            entries.push(wgpu::BindGroupEntry {
                binding: POST_USER_TEXTURE_BINDING + k as u32,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hw-post-bind"),
            layout: &self.bgl,
            entries: &entries,
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hw-post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_texture(
            input.as_image_copy(),
            last_tex.as_image_copy(),
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn def(uniforms: &[(&str, &str)], textures: &[(&str, &str)]) -> HardwareShaderDef {
        HardwareShaderDef {
            name: "Probe".into(),
            kind: room_defs::HardwareShaderKind::PostProcess,
            shader: "probe.fp".into(),
            target: "scene".into(),
            uniforms: uniforms
                .iter()
                .map(|(t, n)| ((*t).to_owned(), (*n).to_owned()))
                .collect(),
            textures: textures
                .iter()
                .map(|(n, p)| ((*n).to_owned(), (*p).to_owned()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn std140_layout_aligns_vec3_and_vec2() {
        let u: Vec<(String, String)> = [("float", "a"), ("vec3", "b"), ("vec2", "c"), ("int", "d")]
            .iter()
            .map(|(t, n)| ((*t).to_owned(), (*n).to_owned()))
            .collect();
        let (f, size) = post_uniform_layout(&u);
        let offs: Vec<u32> = f.iter().map(|f| f.offset).collect();
        assert_eq!(offs, vec![0, 16, 32, 40]);
        assert_eq!(size, 48);
        assert_eq!(post_uniform_layout(&[]).1, 16);
    }

    #[test]
    fn compiles_a_uzdoom_post_shader_using_every_auto_input() {
        let src = r#"#version 330
void main()
{
    vec2 size = vec2(textureSize(InputTexture, 0));
    vec2 wobble = vec2(sin(InputTime + TexCoord.y * 10.0), 0.0) * strength / size;
    vec4 now = texture(InputTexture, TexCoord + wobble);
    vec4 prev = texture(LastInputTexture, TexCoord);
    vec4 n = texture(noise, TexCoord * 4.0 + vec2(InputTimeGame));
    FragColor = mix(now, prev, clamp(InputTimeDelta * 10.0, 0.0, 0.9)) + n * tint;
}
"#;
        let c = compile_post_glsl(
            &def(
                &[("float", "strength"), ("vec4", "tint")],
                &[("noise", "NOISE")],
            ),
            src,
        )
        .unwrap();
        for needle in [
            "@group(0) @binding(0)",
            "@group(0) @binding(2)",
            "@group(0) @binding(3)",
            "@group(0) @binding(4)",
            "@group(0) @binding(8)",
            "InputTimeDelta",
            "InputTimeGame",
            "fn main(",
        ] {
            assert!(c.wgsl.contains(needle), "missing {needle}:\n{}", c.wgsl);
        }
        assert_eq!(c.uniform_size, 32);
        assert_eq!(c.textures, vec![("noise".to_owned(), "NOISE".to_owned())]);
    }

    #[test]
    fn undeclared_uniform_fails_to_compile() {
        let src = "void main() { FragColor = texture(InputTexture, TexCoord) * missing; }";
        assert!(compile_post_glsl(&def(&[], &[]), src).is_err());
    }

    #[test]
    fn auto_uniform_bytes_are_delta_time_game() {
        let b = PostTimes {
            delta: 0.5,
            time: 2.0,
            game: 1.0,
        }
        .to_bytes();
        assert_eq!(&b[0..4], &0.5f32.to_le_bytes());
        assert_eq!(&b[4..8], &2.0f32.to_le_bytes());
        assert_eq!(&b[8..12], &1.0f32.to_le_bytes());
    }
}

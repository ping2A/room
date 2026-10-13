//! GLDEFS `HardwareShader` → WGSL compile / bind catalog (M10 bootstrap).

use crate::glsl_translate::{GlslError, glsl_to_wgsl};
use room_defs::{HardwareShaderDef, HardwareShaderKind, MaterialDef};
use std::collections::BTreeMap;
use thiserror::Error;

/// Hardware-shader catalog errors.
#[derive(Debug, Error)]
pub enum HardwareShaderError {
    /// GLSL→WGSL failed.
    #[error(transparent)]
    Glsl(#[from] GlslError),
    /// Missing source / empty shader path.
    #[error("{0}")]
    Message(String),
}

/// Compiled material / post shader ready for pipeline creation.
#[derive(Clone, Debug)]
pub struct CompiledHardwareShader {
    /// Texture or post-process name (uppercase for materials).
    pub name: String,
    /// Material vs post.
    pub kind: HardwareShaderKind,
    /// Original source path / lump.
    pub source_path: String,
    /// Animation speed.
    pub speed: f32,
    /// Extra texture uniforms.
    pub textures: BTreeMap<String, String>,
    /// Emitted WGSL (naga).
    pub wgsl: String,
}

/// Name → compiled HardwareShader (load-time; draw bind still uses default pass).
#[derive(Clone, Debug, Default)]
pub struct HardwareShaderCatalog {
    /// Material shaders by texture name.
    materials: BTreeMap<String, CompiledHardwareShader>,
    /// Post-process shaders by name.
    post: BTreeMap<String, CompiledHardwareShader>,
}

impl HardwareShaderCatalog {
    /// Empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Material shader for `texture`, if compiled.
    #[must_use]
    pub fn material(&self, name: &str) -> Option<&CompiledHardwareShader> {
        self.materials.get(&normalize(name))
    }

    /// Post-process shader by name.
    #[must_use]
    pub fn post_process(&self, name: &str) -> Option<&CompiledHardwareShader> {
        self.post.get(name)
    }

    /// Number of compiled material shaders.
    #[must_use]
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    /// Compile and insert from a GLDEFS [`HardwareShaderDef`] + GLSL source bytes.
    pub fn bind_def(
        &mut self,
        def: &HardwareShaderDef,
        glsl_source: &str,
    ) -> Result<(), HardwareShaderError> {
        let wgsl = compile_material_glsl(glsl_source)?;
        let compiled = CompiledHardwareShader {
            name: def.name.clone(),
            kind: def.kind,
            source_path: def.shader.clone(),
            speed: def.speed,
            textures: def.textures.clone(),
            wgsl,
        };
        match def.kind {
            HardwareShaderKind::Material => {
                self.materials.insert(normalize(&def.name), compiled);
            }
            HardwareShaderKind::PostProcess => {
                self.post.insert(def.name.clone(), compiled);
            }
        }
        Ok(())
    }

    /// Compile from a [`MaterialDef`] that carries `shader` + GLSL source.
    pub fn bind_material(
        &mut self,
        mat: &MaterialDef,
        glsl_source: &str,
    ) -> Result<(), HardwareShaderError> {
        let path = mat
            .shader
            .as_deref()
            .ok_or_else(|| HardwareShaderError::Message("material has no shader path".into()))?;
        let wgsl = compile_material_glsl(glsl_source)?;
        self.materials.insert(
            normalize(&mat.name),
            CompiledHardwareShader {
                name: mat.name.clone(),
                kind: HardwareShaderKind::Material,
                source_path: path.to_owned(),
                speed: mat.shader_speed,
                textures: mat.shader_textures.clone(),
                wgsl,
            },
        );
        Ok(())
    }

    /// Create a wgpu shader module from a compiled entry (for custom pipelines).
    pub fn create_module(
        device: &wgpu::Device,
        compiled: &CompiledHardwareShader,
    ) -> wgpu::ShaderModule {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&compiled.name),
            source: wgpu::ShaderSource::Wgsl(compiled.wgsl.as_str().into()),
        })
    }
}

/// Fullscreen triangle WGSL host that samples `tex` (for post HardwareShaders).
pub const HARDWARE_POST_HOST_WGSL: &str = r#"
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
    out.uv = p[idx] * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var input_samp: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(input_tex, input_samp, in.uv);
}
"#;

/// Compose host vs + custom WGSL fragment body into a post shader.
///
/// `fs_body` is inserted into `fs_main` and may use `in.uv`, `input_tex`, `input_samp`.
/// Returns full WGSL suitable for [`create_hardware_post_pipeline`].
#[must_use]
pub fn compose_hardware_post_wgsl(fs_body: &str) -> String {
    format!(
        r#"
struct VsOut {{
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {{
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(p[idx], 0.0, 1.0);
    out.uv = p[idx] * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}}

@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var input_samp: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {{
{fs_body}
}}
"#
    )
}

/// Build a fullscreen post pipeline from host WGSL (bootstrap; custom FS swap TBD).
pub fn create_hardware_post_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    wgsl: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("hw-post"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hw-post-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hw-post-pl"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("hw-post-pipe"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
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
    })
}

/// Translate a full GLSL 450 fragment (or wrap a GZDoom `ProcessTexel` snippet).
pub fn compile_material_glsl(source: &str) -> Result<String, HardwareShaderError> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(HardwareShaderError::Message("empty GLSL source".into()));
    }
    // Already a full GLSL shader?
    if trimmed.contains("#version") || trimmed.contains("void main") {
        return Ok(glsl_to_wgsl(trimmed, "fragment")?);
    }
    // GZDoom material snippet → wrap in a minimal fragment template.
    let wrapped = wrap_process_texel(trimmed);
    Ok(glsl_to_wgsl(&wrapped, "fragment")?)
}

fn wrap_process_texel(snippet: &str) -> String {
    format!(
        r#"#version 450
layout(location = 0) in vec2 vTexCoord;
layout(location = 0) out vec4 fragColor;
layout(set = 0, binding = 0) uniform texture2D tex;
layout(set = 0, binding = 1) uniform sampler samp;
vec4 getTexel(vec2 uv) {{
    return texture(sampler2D(tex, samp), uv);
}}
vec4 ProcessTexel() {{
{snippet}
}}
void main() {{
    fragColor = ProcessTexel();
}}
"#
    )
}

fn normalize(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_defs::HardwareShaderDef;

    #[test]
    fn compiles_full_glsl_fragment() {
        let glsl = r#"
#version 450
layout(location = 0) out vec4 fragColor;
void main() {
    fragColor = vec4(0.2, 0.4, 0.8, 1.0);
}
"#;
        let wgsl = compile_material_glsl(glsl).unwrap();
        assert!(wgsl.contains("fn main") || wgsl.contains("vec4"));
    }

    #[test]
    fn wraps_process_texel_snippet() {
        let snippet = "return getTexel(vTexCoord) * vec4(1.0, 0.5, 0.5, 1.0);";
        let wgsl = compile_material_glsl(snippet).unwrap();
        assert!(wgsl.contains("fn main") || wgsl.contains("ProcessTexel") || wgsl.contains("vec4"));
    }

    #[test]
    fn catalog_binds_def() {
        let mut cat = HardwareShaderCatalog::new();
        let def = HardwareShaderDef {
            name: "WARP".into(),
            kind: HardwareShaderKind::Material,
            shader: "warp.fp".into(),
            speed: 1.0,
            ..Default::default()
        };
        let glsl = r#"
#version 450
layout(location = 0) out vec4 fragColor;
void main() { fragColor = vec4(1.0); }
"#;
        cat.bind_def(&def, glsl).unwrap();
        assert!(cat.material("warp").is_some());
        assert_eq!(cat.material_count(), 1);
    }

    #[test]
    fn compose_post_includes_body() {
        let wgsl = compose_hardware_post_wgsl(
            "return textureSample(input_tex, input_samp, in.uv) * vec4(1.0, 0.5, 0.5, 1.0);",
        );
        assert!(wgsl.contains("fn vs_main"));
        assert!(wgsl.contains("0.5, 0.5, 1.0"));
    }
}

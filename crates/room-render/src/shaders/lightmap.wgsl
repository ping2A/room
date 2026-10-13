// Lightmap atlas sample bootstrap (ZDRay / FLMP).

struct LmUniforms {
    /// xy = atlas size, z = surface index (unused in FS), w = strength.
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: LmUniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_samp: sampler;

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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(atlas, atlas_samp, in.uv);
    return vec4<f32>(c.rgb * u.params.w, 1.0);
}

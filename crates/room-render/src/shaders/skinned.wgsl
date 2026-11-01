//! IQM GPU skinning bootstrap (M10): bone palette + 4-weight verts → unlit shade.

struct ViewUniforms {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light_params: vec4<f32>,
    clip_plane: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: ViewUniforms;

struct BoneUniforms {
    // x = bone count (informational)
    count: vec4<u32>,
    mats: array<mat4x4<f32>, 64>,
};

@group(0) @binding(1) var<uniform> bones: BoneUniforms;

@group(1) @binding(0) var tex_albedo: texture_2d<f32>;
@group(1) @binding(1) var samp_albedo: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: f32,
    @location(3) bone_indices: vec4<f32>,
    @location(4) bone_weights: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: f32,
    @location(2) world_pos: vec3<f32>,
};

fn skin_point(p: vec3<f32>, indices: vec4<f32>, weights: vec4<f32>) -> vec3<f32> {
    var wsum = weights.x + weights.y + weights.z + weights.w;
    if (wsum < 1e-6) {
        return p;
    }
    var out = vec3<f32>(0.0);
    let idx = vec4<u32>(
        u32(indices.x),
        u32(indices.y),
        u32(indices.z),
        u32(indices.w),
    );
    let w = weights / wsum;
    for (var i = 0; i < 4; i++) {
        let bi = idx[i];
        if (bi >= 64u || w[i] <= 0.0) {
            continue;
        }
        let hp = bones.mats[bi] * vec4<f32>(p, 1.0);
        out += hp.xyz * w[i];
    }
    return out;
}

@vertex
fn vs_main(vin: VsIn) -> VsOut {
    let skinned = skin_point(vin.pos, vin.bone_indices, vin.bone_weights);
    var o: VsOut;
    o.clip = view.view_proj * vec4<f32>(skinned, 1.0);
    o.uv = vin.uv;
    o.light = vin.light;
    o.world_pos = skinned;
    return o;
}

@fragment
fn fs_main(vin: VsOut) -> @location(0) vec4<f32> {
    // Mirror / portal clip (docs/29 SK1) — same test as unlit.wgsl.
    let cn = view.clip_plane.xyz;
    if (dot(cn, cn) > 1e-12) {
        let s = dot(cn, vin.world_pos) + view.clip_plane.w;
        if (s > 1e-3) {
            discard;
        }
    }
    let albedo = textureSample(tex_albedo, samp_albedo, vin.uv);
    if (albedo.a < 0.1) {
        discard;
    }
    let lit = albedo.rgb * vin.light;
    return vec4<f32>(lit, albedo.a);
}

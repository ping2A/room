// Screen-space AO bootstrap (`gl_ssao`) — depth-edge samples + bilateral blur.

struct SsaoUniforms {
    // x = enabled, y = radius (UV), z = bias, w = intensity
    params: vec4<f32>,
    // x = 1/width, y = 1/height, z = blur depth sigma, w unused
    rcp: vec4<f32>,
};

@group(0) @binding(0) var<uniform> ssao: SsaoUniforms;
@group(0) @binding(1) var color_tex: texture_2d<f32>;
@group(0) @binding(2) var color_samp: sampler;
@group(0) @binding(3) var depth_tex: texture_depth_2d;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var o: VsOut;
    o.clip = vec4<f32>(pos[vi], 0.0, 1.0);
    o.uv = pos[vi] * 0.5 + vec2<f32>(0.5, 0.5);
    o.uv.y = 1.0 - o.uv.y;
    return o;
}

fn sample_depth(uv: vec2<f32>) -> f32 {
    let dims = textureDimensions(depth_tex);
    let px = vec2<i32>(
        i32(clamp(uv.x, 0.0, 1.0) * f32(dims.x - 1u)),
        i32(clamp(uv.y, 0.0, 1.0) * f32(dims.y - 1u)),
    );
    return textureLoad(depth_tex, px, 0);
}

fn raw_occlusion(uv: vec2<f32>, d0: f32) -> f32 {
    let radius = max(ssao.params.y, 0.001);
    let bias = ssao.params.z;
    let intensity = ssao.params.w;
    var occ = 0.0;
    let taps = array<vec2<f32>, 8>(
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.7071, 0.7071),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(-0.7071, 0.7071),
        vec2<f32>(-1.0, 0.0),
        vec2<f32>(-0.7071, -0.7071),
        vec2<f32>(0.0, -1.0),
        vec2<f32>(0.7071, -0.7071),
    );
    for (var i = 0; i < 8; i++) {
        let suv = uv + taps[i] * radius * ssao.rcp.xy * 120.0;
        let d = sample_depth(suv);
        let delta = d0 - d;
        if (delta > bias) {
            occ += smoothstep(bias, bias + 0.05, delta);
        }
    }
    return clamp(occ / 8.0 * intensity, 0.0, 1.0);
}

/// Depth-aware weight for bilateral AO blur.
fn bilateral_w(d0: f32, d1: f32, sigma: f32) -> f32 {
    let dd = abs(d0 - d1);
    let s = max(sigma, 1e-4);
    return exp(-(dd * dd) / (2.0 * s * s));
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    let color = textureSample(color_tex, color_samp, v.uv);
    if (ssao.params.x < 0.5) {
        return color;
    }
    let d0 = sample_depth(v.uv);
    if (d0 > 0.999) {
        return color;
    }
    // Centre occlusion + 4-neighbour bilateral blur (depth-edge preserving).
    let sigma = select(0.02, ssao.rcp.z, ssao.rcp.z > 0.0);
    var occ = raw_occlusion(v.uv, d0);
    var wsum = 1.0;
    let offs = array<vec2<f32>, 4>(
        vec2<f32>(ssao.rcp.x, 0.0),
        vec2<f32>(-ssao.rcp.x, 0.0),
        vec2<f32>(0.0, ssao.rcp.y),
        vec2<f32>(0.0, -ssao.rcp.y),
    );
    for (var i = 0; i < 4; i++) {
        let uv = v.uv + offs[i];
        let d = sample_depth(uv);
        let w = bilateral_w(d0, d, sigma);
        occ += raw_occlusion(uv, d) * w;
        wsum += w;
    }
    occ = clamp(occ / wsum, 0.0, 1.0);
    let shaded = color.rgb * (1.0 - occ * 0.65);
    return vec4<f32>(shaded, color.a);
}

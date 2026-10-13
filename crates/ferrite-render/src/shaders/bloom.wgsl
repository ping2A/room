//! Bloom downsample / upsample (true mip chain bootstrap).

struct BloomUniforms {
    // x = mode (0=extract+down, 1=upsample add), y = threshold, z = 1/src_w, w = 1/src_h
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> bloom: BloomUniforms;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;

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

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    let rcp = bloom.params.zw;
    // 4-tap box (Kawase-ish) for both down and up.
    let o = rcp * 0.5;
    let c0 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(o.x, o.y)).rgb;
    let c1 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(-o.x, o.y)).rgb;
    let c2 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(o.x, -o.y)).rgb;
    let c3 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(-o.x, -o.y)).rgb;
    var avg = (c0 + c1 + c2 + c3) * 0.25;
    if (bloom.params.x < 0.5) {
        // Extract / downsample: soft threshold on first level only (threshold>0).
        let th = bloom.params.y;
        if (th > 0.0) {
            let soft = max(luma(avg) - th, 0.0);
            avg = avg * soft;
        }
    }
    return vec4<f32>(avg, 1.0);
}

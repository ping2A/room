//! Bloom downsample / upsample (docs/29 R3/R4: soft knee + Karis + tent).

struct BloomUniforms {
    // x = mode (0=extract/down, 1=upsample add), y = threshold, z = 1/src_w, w = 1/src_h
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
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn soft_knee_threshold(c: vec3<f32>, th: f32) -> vec3<f32> {
    // R4: soft knee + anti-firefly clamp.
    let clamped = min(c, vec3<f32>(64.0));
    let l = luma(clamped);
    let knee = th * 0.5;
    let soft = l - th + knee;
    let soft2 = max(soft, 0.0);
    let soft_curve = min(soft2 * soft2 / max(2.0 * knee, 1e-4), 2.0 * knee);
    let contribution = max(l - th, soft_curve);
    return clamped * (contribution / max(l, 1e-4));
}

fn karis_w(c: vec3<f32>) -> f32 {
    return 1.0 / (1.0 + luma(c));
}

fn sample_karis(uv: vec2<f32>, rcp: vec2<f32>) -> vec3<f32> {
    // 13-tap Jimenez-style downsample with Karis luminance weights on first level.
    let o = rcp;
    let c00 = textureSample(src_tex, src_samp, uv).rgb;
    let c10 = textureSample(src_tex, src_samp, uv + vec2<f32>(-o.x, -o.y)).rgb;
    let c11 = textureSample(src_tex, src_samp, uv + vec2<f32>(o.x, -o.y)).rgb;
    let c12 = textureSample(src_tex, src_samp, uv + vec2<f32>(-o.x, o.y)).rgb;
    let c13 = textureSample(src_tex, src_samp, uv + vec2<f32>(o.x, o.y)).rgb;
    let c20 = textureSample(src_tex, src_samp, uv + vec2<f32>(-2.0 * o.x, 0.0)).rgb;
    let c21 = textureSample(src_tex, src_samp, uv + vec2<f32>(2.0 * o.x, 0.0)).rgb;
    let c22 = textureSample(src_tex, src_samp, uv + vec2<f32>(0.0, -2.0 * o.y)).rgb;
    let c23 = textureSample(src_tex, src_samp, uv + vec2<f32>(0.0, 2.0 * o.y)).rgb;
    let c30 = textureSample(src_tex, src_samp, uv + vec2<f32>(-2.0 * o.x, -2.0 * o.y)).rgb;
    let c31 = textureSample(src_tex, src_samp, uv + vec2<f32>(2.0 * o.x, -2.0 * o.y)).rgb;
    let c32 = textureSample(src_tex, src_samp, uv + vec2<f32>(-2.0 * o.x, 2.0 * o.y)).rgb;
    let c33 = textureSample(src_tex, src_samp, uv + vec2<f32>(2.0 * o.x, 2.0 * o.y)).rgb;
    let w00 = karis_w(c00);
    let w10 = karis_w(c10);
    let w11 = karis_w(c11);
    let w12 = karis_w(c12);
    let w13 = karis_w(c13);
    let w20 = karis_w(c20);
    let w21 = karis_w(c21);
    let w22 = karis_w(c22);
    let w23 = karis_w(c23);
    let w30 = karis_w(c30);
    let w31 = karis_w(c31);
    let w32 = karis_w(c32);
    let w33 = karis_w(c33);
    let sum = c00 * w00 * 0.125
        + (c10 * w10 + c11 * w11 + c12 * w12 + c13 * w13) * 0.125
        + (c20 * w20 + c21 * w21 + c22 * w22 + c23 * w23) * 0.0625
        + (c30 * w30 + c31 * w31 + c32 * w32 + c33 * w33) * 0.03125;
    let wsum = w00 * 0.125
        + (w10 + w11 + w12 + w13) * 0.125
        + (w20 + w21 + w22 + w23) * 0.0625
        + (w30 + w31 + w32 + w33) * 0.03125;
    return sum / max(wsum, 1e-4);
}

fn sample_box(uv: vec2<f32>, rcp: vec2<f32>) -> vec3<f32> {
    let o = rcp * 0.5;
    let c0 = textureSample(src_tex, src_samp, uv + vec2<f32>(o.x, o.y)).rgb;
    let c1 = textureSample(src_tex, src_samp, uv + vec2<f32>(-o.x, o.y)).rgb;
    let c2 = textureSample(src_tex, src_samp, uv + vec2<f32>(o.x, -o.y)).rgb;
    let c3 = textureSample(src_tex, src_samp, uv + vec2<f32>(-o.x, -o.y)).rgb;
    return (c0 + c1 + c2 + c3) * 0.25;
}

fn sample_tent(uv: vec2<f32>, rcp: vec2<f32>) -> vec3<f32> {
    // 9-tap tent upsample.
    let o = rcp;
    let c = textureSample(src_tex, src_samp, uv).rgb * 4.0
        + textureSample(src_tex, src_samp, uv + vec2<f32>(-o.x, 0.0)).rgb * 2.0
        + textureSample(src_tex, src_samp, uv + vec2<f32>(o.x, 0.0)).rgb * 2.0
        + textureSample(src_tex, src_samp, uv + vec2<f32>(0.0, -o.y)).rgb * 2.0
        + textureSample(src_tex, src_samp, uv + vec2<f32>(0.0, o.y)).rgb * 2.0
        + textureSample(src_tex, src_samp, uv + vec2<f32>(-o.x, -o.y)).rgb
        + textureSample(src_tex, src_samp, uv + vec2<f32>(o.x, -o.y)).rgb
        + textureSample(src_tex, src_samp, uv + vec2<f32>(-o.x, o.y)).rgb
        + textureSample(src_tex, src_samp, uv + vec2<f32>(o.x, o.y)).rgb;
    return c / 16.0;
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    let rcp = bloom.params.zw;
    let th = bloom.params.y;
    if (bloom.params.x < 0.5) {
        // Extract / downsample.
        var avg: vec3<f32>;
        if (th > 0.0) {
            avg = sample_karis(v.uv, rcp);
            avg = soft_knee_threshold(avg, th);
        } else {
            avg = sample_box(v.uv, rcp);
        }
        return vec4<f32>(avg, 1.0);
    }
    // Upsample (additive blend set in the pipeline / caller).
    return vec4<f32>(sample_tent(v.uv, rcp), 1.0);
}

// Fullscreen post: tonemap + gamma + soft bloom threshold + screen blend.

struct PostUniforms {
    // x = tonemap mode, y = exposure, z = gamma, w = bloom threshold (0 = off)
    params: vec4<f32>,
    // RGBA screen blend (damage/bonus/suit flash) — GZDoom V_AddBlend stand-in
    blend: vec4<f32>,
    // x = fxaa on (1), y = 1/width, z = 1/height, w unused
    fxaa: vec4<f32>,
};

@group(0) @binding(0) var<uniform> post: PostUniforms;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;
// PLAYPAL palette 0 as 256×1 RGBA8 (nearest).
@group(0) @binding(3) var pal_tex: texture_2d<f32>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // Fullscreen triangle covering clip space.
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var o: VsOut;
    o.clip = vec4<f32>(pos[vi], 0.0, 1.0);
    o.uv = pos[vi] * 0.5 + vec2<f32>(0.5, 0.5);
    // wgpu / Metal: flip Y for texture sample if needed — NDC y-up, tex y-down.
    o.uv.y = 1.0 - o.uv.y;
    return o;
}

fn pal_rgb(i: i32) -> vec3<f32> {
    return textureLoad(pal_tex, vec2<i32>(clamp(i, 0, 255), 0), 0).rgb;
}

fn quantize_palette(c: vec3<f32>) -> vec3<f32> {
    let t = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0;
    var best = 0;
    var best_d = 1e20;
    for (var i = 0; i < 256; i++) {
        let p = pal_rgb(i) * 255.0;
        let d = dot(p - t, p - t);
        if (d < best_d) {
            best_d = d;
            best = i;
        }
    }
    return pal_rgb(best);
}

fn tonemap(c: vec3<f32>, mode: i32, exposure: f32) -> vec3<f32> {
    let x = c * max(exposure, 0.01);
    switch mode {
    case 1: { // Reinhard
        return x / (x + vec3<f32>(1.0));
    }
    case 2: { // Linear clamp
        return clamp(x, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    case 3: { // Hejl-Dawson-ish filmic (compact)
        let a = max(vec3<f32>(0.0), x - vec3<f32>(0.004));
        return (a * (6.2 * a + 0.5)) / (a * (6.2 * a + 1.7) + 0.06);
    }
    case 4: { // PLAYPAL nearest
        return quantize_palette(x);
    }
    default: {
        return x;
    }
    }
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// Compact FXAA 3-ish (Jimenez) — quality bootstrap, not full preset.
fn apply_fxaa(uv: vec2<f32>) -> vec4<f32> {
    let rcp = post.fxaa.yz;
    let rgbM = textureSample(src_tex, src_samp, uv).rgb;
    let rgbNW = textureSample(src_tex, src_samp, uv + vec2<f32>(-rcp.x, -rcp.y)).rgb;
    let rgbNE = textureSample(src_tex, src_samp, uv + vec2<f32>(rcp.x, -rcp.y)).rgb;
    let rgbSW = textureSample(src_tex, src_samp, uv + vec2<f32>(-rcp.x, rcp.y)).rgb;
    let rgbSE = textureSample(src_tex, src_samp, uv + vec2<f32>(rcp.x, rcp.y)).rgb;
    let lumaM = luma(rgbM);
    let lumaMin = min(lumaM, min(min(luma(rgbNW), luma(rgbNE)), min(luma(rgbSW), luma(rgbSE))));
    let lumaMax = max(lumaM, max(max(luma(rgbNW), luma(rgbNE)), max(luma(rgbSW), luma(rgbSE))));
    let dir = vec2<f32>(
        -((luma(rgbNW) + luma(rgbNE)) - (luma(rgbSW) + luma(rgbSE))),
        ((luma(rgbNW) + luma(rgbSW)) - (luma(rgbNE) + luma(rgbSE))),
    );
    let dirReduce = max((luma(rgbNW) + luma(rgbNE) + luma(rgbSW) + luma(rgbSE)) * 0.03125, 0.0078125);
    let rcpDir = 1.0 / (min(abs(dir.x), abs(dir.y)) + dirReduce);
    var d = clamp(dir * rcpDir, vec2<f32>(-8.0), vec2<f32>(8.0)) * rcp;
    let rgbA = 0.5 * (
        textureSample(src_tex, src_samp, uv + d * (1.0 / 3.0 - 0.5)).rgb +
        textureSample(src_tex, src_samp, uv + d * (2.0 / 3.0 - 0.5)).rgb
    );
    let rgbB = rgbA * 0.5 + 0.25 * (
        textureSample(src_tex, src_samp, uv + d * -0.5).rgb +
        textureSample(src_tex, src_samp, uv + d * 0.5).rgb
    );
    let lumaB = luma(rgbB);
    if (lumaB < lumaMin || lumaB > lumaMax) {
        return vec4<f32>(rgbA, 1.0);
    }
    return vec4<f32>(rgbB, 1.0);
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    var raw: vec4<f32>;
    if (post.fxaa.x > 0.5) {
        raw = apply_fxaa(v.uv);
    } else {
        raw = textureSample(src_tex, src_samp, v.uv);
    }
    var color = raw.rgb;
    let mode = i32(post.params.x + 0.5);
    let exposure = post.params.y;
    let gamma = post.params.z;
    let bloom_th = post.params.w;
    // Soft threshold + multi-scale Kawase-ish bloom (mip stand-in).
    if (bloom_th > 0.0) {
        let rcp = post.fxaa.yz;
        var bloom = vec3<f32>(0.0);
        var wsum = 0.0;
        // Four Kawase "mip" radii (stand-in until true downsample chain).
        for (var level = 0; level < 4; level++) {
            let scale = f32(1 << u32(level)); // 1, 2, 4, 8 texel radii
            let o = rcp * scale * 2.0;
            let c0 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(o.x, o.y)).rgb;
            let c1 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(-o.x, o.y)).rgb;
            let c2 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(o.x, -o.y)).rgb;
            let c3 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(-o.x, -o.y)).rgb;
            // Cross samples for a fuller Kawase tap.
            let c4 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(o.x, 0.0)).rgb;
            let c5 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(-o.x, 0.0)).rgb;
            let c6 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(0.0, o.y)).rgb;
            let c7 = textureSample(src_tex, src_samp, v.uv + vec2<f32>(0.0, -o.y)).rgb;
            let avg = (c0 + c1 + c2 + c3 + c4 + c5 + c6 + c7) * 0.125;
            let soft = max(luma(avg) - bloom_th, 0.0);
            let w = 1.0 / (1.0 + f32(level));
            bloom += avg * soft * w;
            wsum += w;
        }
        color = color + bloom / max(wsum, 1e-3) * 0.85;
    }
    color = tonemap(color, mode, exposure);
    // Hejl / palette already display-referred; skip extra gamma.
    if (mode != 3 && mode != 4) {
        let g = max(gamma, 0.01);
        color = pow(max(color, vec3<f32>(0.0)), vec3<f32>(1.0 / g));
    }
    // Screen blend from playsim (alpha = blend strength).
    let ba = clamp(post.blend.a, 0.0, 1.0);
    color = mix(color, post.blend.rgb, ba);
    return vec4<f32>(color, raw.a);
}

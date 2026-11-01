// Fullscreen post: bloom add + tonemap → LDR, then FXAA/sharpen/screen FX.
// `docs/29` SHR: R2 (real bloom tex), P1 (FXAA on LDR), P2 (sharpen LDR),
// P3 (dither before banding), P5 (RoomSoft linear; encode only if needed).

struct PostUniforms {
    // x = tonemap mode, y = exposure, z = gamma, w = bloom intensity (0 = off)
    params: vec4<f32>,
    // RGBA screen blend (damage/bonus/suit flash) — GZDoom V_AddBlend stand-in
    blend: vec4<f32>,
    // x = fxaa on (1), y = 1/width, z = 1/height, w = time seconds
    fxaa: vec4<f32>,
    // x = vignette, y = grain, z = chromatic, w = crt
    extras: vec4<f32>,
    // x = dither, y = banding, z = sharpen, w = damage vignette pulse
    extras2: vec4<f32>,
    // x = encode_srgb (1 = write sRGB in-shader; 0 when target is *Srgb)
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> post: PostUniforms;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;
// PLAYPAL palette 0 as 256×1 RGBA8 (nearest) — fallback / debug.
@group(0) @binding(3) var pal_tex: texture_2d<f32>;
// 32³ RGB→palette colour LUT (nearest).
@group(0) @binding(4) var pal_lut: texture_3d<f32>;
@group(0) @binding(5) var pal_lut_samp: sampler;
// BloomPass composite (black 1×1 when bloom off) — R2.
@group(0) @binding(6) var bloom_tex: texture_2d<f32>;

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

fn quantize_palette_lut(c: vec3<f32>) -> vec3<f32> {
    let t = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    return textureSampleLevel(pal_lut, pal_lut_samp, t, 0.0).rgb;
}

fn uncharted2_tonemap_partial(x: f32) -> f32 {
    let A = 0.15;
    let B = 0.50;
    let C = 0.10;
    let D = 0.20;
    let E = 0.02;
    let F = 0.30;
    return ((x * (A * x + C * B) + D * E) / (x * (A * x + B) + D * F)) - E / F;
}

fn linear_to_srgb(c: f32) -> f32 {
    let x = max(c, 0.0);
    if (x <= 0.0031308) {
        return x * 12.92;
    }
    return 1.055 * pow(x, 1.0 / 2.4) - 0.055;
}

// Soft toe in linear space (P5: no sRGB encode here).
fn room_soft(c: vec3<f32>) -> vec3<f32> {
    return c * (vec3<f32>(0.65) + vec3<f32>(0.35) / (c + vec3<f32>(0.35)));
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
    case 3: { // Hejl-Dawson-ish filmic (display-referred)
        let a = max(vec3<f32>(0.0), x - vec3<f32>(0.004));
        return (a * (6.2 * a + 0.5)) / (a * (6.2 * a + 1.7) + 0.06);
    }
    case 4: { // PLAYPAL via 3D LUT
        return quantize_palette_lut(x);
    }
    case 5: { // Uncharted 2
        let w = uncharted2_tonemap_partial(2.0);
        return vec3<f32>(
            uncharted2_tonemap_partial(x.x) / w,
            uncharted2_tonemap_partial(x.y) / w,
            uncharted2_tonemap_partial(x.z) / w,
        );
    }
    case 6: { // AgX-ish
        let t = max(x * 0.8, vec3<f32>(0.0));
        let y = t / (t + vec3<f32>(1.0));
        return pow(y, vec3<f32>(1.1));
    }
    case 7: { // Khronos PBR Neutral (scalar)
        let f = 0.04;
        var o = vec3<f32>(0.0);
        for (var i = 0; i < 3; i++) {
            let v = max(x[i], 0.0);
            if (v < f) {
                o[i] = v;
            } else {
                o[i] = (v - f) / (1.0 + (v - f)) + f;
            }
        }
        return o;
    }
    case 8: { // RoomSoft toe (linear out)
        return room_soft(x);
    }
    default: {
        return x;
    }
    }
}

fn luma709(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn hdr_to_ldr(uv: vec2<f32>) -> vec3<f32> {
    var color = textureSample(src_tex, src_samp, uv).rgb;
    let bloom_amt = post.params.w;
    if (bloom_amt > 0.001) {
        color = color + textureSample(bloom_tex, src_samp, uv).rgb * bloom_amt;
    }
    let mode = i32(post.params.x + 0.5);
    let exposure = post.params.y;
    let gamma = post.params.z;
    color = tonemap(color, mode, exposure);
    // Hejl / Palette / RoomSoft handle their own transfer; others apply user gamma.
    if (mode != 3 && mode != 4 && mode != 8) {
        let g = max(gamma, 0.01);
        color = pow(max(color, vec3<f32>(0.0)), vec3<f32>(1.0 / g));
    }
    return color;
}

fn apply_fxaa_ldr(uv: vec2<f32>) -> vec3<f32> {
    let rcp = post.fxaa.yz;
    let rgbM = hdr_to_ldr(uv);
    let rgbNW = hdr_to_ldr(uv + vec2<f32>(-rcp.x, -rcp.y));
    let rgbNE = hdr_to_ldr(uv + vec2<f32>(rcp.x, -rcp.y));
    let rgbSW = hdr_to_ldr(uv + vec2<f32>(-rcp.x, rcp.y));
    let rgbSE = hdr_to_ldr(uv + vec2<f32>(rcp.x, rcp.y));
    let lumaM = luma709(rgbM);
    let lumaMin = min(lumaM, min(min(luma709(rgbNW), luma709(rgbNE)), min(luma709(rgbSW), luma709(rgbSE))));
    let lumaMax = max(lumaM, max(max(luma709(rgbNW), luma709(rgbNE)), max(luma709(rgbSW), luma709(rgbSE))));
    let dir = vec2<f32>(
        -((luma709(rgbNW) + luma709(rgbNE)) - (luma709(rgbSW) + luma709(rgbSE))),
        ((luma709(rgbNW) + luma709(rgbSW)) - (luma709(rgbNE) + luma709(rgbSE))),
    );
    let dirReduce = max((luma709(rgbNW) + luma709(rgbNE) + luma709(rgbSW) + luma709(rgbSE)) * 0.03125, 0.0078125);
    let rcpDir = 1.0 / (min(abs(dir.x), abs(dir.y)) + dirReduce);
    var d = clamp(dir * rcpDir, vec2<f32>(-8.0), vec2<f32>(8.0)) * rcp;
    let rgbA = 0.5 * (
        hdr_to_ldr(uv + d * (1.0 / 3.0 - 0.5)) +
        hdr_to_ldr(uv + d * (2.0 / 3.0 - 0.5))
    );
    let rgbB = rgbA * 0.5 + 0.25 * (
        hdr_to_ldr(uv + d * -0.5) +
        hdr_to_ldr(uv + d * 0.5)
    );
    let lumaB = luma709(rgbB);
    if (lumaB < lumaMin || lumaB > lumaMax) {
        return rgbA;
    }
    return rgbB;
}

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn bayer4(p: vec2<f32>) -> f32 {
    let x = i32(p.x) & 3;
    let y = i32(p.y) & 3;
    let m = array<f32, 16>(
        0.0, 8.0, 2.0, 10.0,
        12.0, 4.0, 14.0, 6.0,
        3.0, 11.0, 1.0, 9.0,
        15.0, 7.0, 13.0, 5.0,
    );
    return m[y * 4 + x] / 16.0;
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    var uv = v.uv;
    let crt_amt = post.extras.w;
    if (crt_amt > 0.001) {
        let c = uv * 2.0 - 1.0;
        let r2 = dot(c, c);
        uv = (c * (1.0 + crt_amt * 0.12 * r2)) * 0.5 + 0.5;
    }

    // Chromatic and FXAA are independent (P1): CA is a lens offset into LDR samples.
    let ca = post.extras.z;
    var color: vec3<f32>;
    if (ca > 0.001) {
        let rcp = post.fxaa.yz;
        let dir = (uv - vec2<f32>(0.5, 0.5)) * ca * 3.0;
        let r = hdr_to_ldr(uv + dir * rcp * 40.0).r;
        let g = hdr_to_ldr(uv).g;
        let b = hdr_to_ldr(uv - dir * rcp * 40.0).b;
        color = vec3<f32>(r, g, b);
    } else if (post.fxaa.x > 0.5) {
        color = apply_fxaa_ldr(uv);
    } else {
        color = hdr_to_ldr(uv);
    }

    // P3: dither before banding (step = 1/31); then quantize.
    let px = uv / max(post.fxaa.yz, vec2<f32>(1e-6));
    if (post.extras2.y > 0.5) {
        if (post.extras2.x > 0.5) {
            let d = (bayer4(px) - 0.5) / 31.0;
            color = clamp(color + vec3<f32>(d), vec3<f32>(0.0), vec3<f32>(1.0));
        }
        color = floor(color * 31.0 + 0.5) / 31.0;
    } else if (post.extras2.x > 0.5) {
        // Final 8-bit dither when not banding.
        let d = (bayer4(px) - 0.5) / 255.0;
        color = clamp(color + vec3<f32>(d), vec3<f32>(0.0), vec3<f32>(1.0));
    }

    let grain = post.extras.y;
    if (grain > 0.001) {
        let n = hash21(uv * vec2<f32>(1920.0, 1080.0) + post.fxaa.w);
        color = clamp(color + (n - 0.5) * grain * 0.35, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let vig = post.extras.x;
    if (vig > 0.001) {
        let d = length(uv - vec2<f32>(0.5, 0.5));
        let f = smoothstep(0.35, 0.95, d);
        color = color * (1.0 - f * vig);
    }
    if (crt_amt > 0.001) {
        let scan = 0.92 + 0.08 * sin(uv.y * 3.14159265 * (1.0 / max(post.fxaa.z, 1e-6)));
        color = color * mix(1.0, scan, crt_amt);
        let mask = 0.85 + 0.15 * sin(uv.x * 3.14159265 * (1.0 / max(post.fxaa.y, 1e-6)) * 0.333);
        color = color * mix(1.0, mask, crt_amt * 0.5);
    }

    // P2: sharpen on LDR neighbourhood (via hdr_to_ldr).
    let sharp = post.extras2.z;
    if (sharp > 0.001) {
        let rcp = post.fxaa.yz;
        let n = hdr_to_ldr(uv + vec2<f32>(0.0, -rcp.y));
        let s = hdr_to_ldr(uv + vec2<f32>(0.0, rcp.y));
        let e = hdr_to_ldr(uv + vec2<f32>(rcp.x, 0.0));
        let w = hdr_to_ldr(uv + vec2<f32>(-rcp.x, 0.0));
        let blur = (n + s + e + w) * 0.25;
        color = clamp(color + (color - blur) * sharp * 1.5, vec3<f32>(0.0), vec3<f32>(1.5));
    }

    let dmg = post.extras2.w;
    if (dmg > 0.001) {
        let d = length(uv - vec2<f32>(0.5, 0.5));
        let edge = smoothstep(0.25, 0.95, d);
        let pulse = 0.65 + 0.35 * sin(post.fxaa.w * 6.5);
        color = mix(color, vec3<f32>(0.85, 0.05, 0.02), edge * dmg * pulse * 0.55);
        let grey = vec3<f32>(luma709(color));
        color = mix(color, grey, dmg * 0.25 * edge);
    }
    let ba = clamp(post.blend.a, 0.0, 1.0);
    color = mix(color, post.blend.rgb, ba);

    // P5: encode only when the colour target is not already *Srgb.
    if (post.flags.x > 0.5) {
        color = vec3<f32>(linear_to_srgb(color.x), linear_to_srgb(color.y), linear_to_srgb(color.z));
    }
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

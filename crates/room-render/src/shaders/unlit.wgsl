struct ViewUniforms {
    view_proj: mat4x4<f32>,
    // xyz = eye, w = time seconds (fuzz animation)
    eye: vec4<f32>,
    // x = lightmode, y = globVis, z = fog density, w = gl_fuzztype
    light_params: vec4<f32>,
    // World clip plane; discard when n·p + d > 0. Zero xyz disables.
    clip_plane: vec4<f32>,
    // x = lightmap strength (0 = off)
    lightmap_params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: ViewUniforms;

struct GpuLight {
    pos_radius: vec4<f32>,
    color_flags: vec4<f32>,
    // xyz = spot dir, w = outer cone cos (≥1 = omni)
    spot_dir_outer: vec4<f32>,
    // x = inner cone cos
    spot_inner: vec4<f32>,
};

struct LightUniforms {
    // x = light count, y = 1 if clustered, z = 1 if 1D shadowmaps
    count: vec4<u32>,
    origin: vec4<f32>,
    cell: vec4<f32>,
    lights: array<GpuLight, 32>,
};

@group(0) @binding(1) var<uniform> dyn_lights: LightUniforms;
// Cluster bitmasks as RGBA8 (little-endian u32), 64×54 texels.
@group(0) @binding(2) var cluster_masks: texture_2d<f32>;
// 1D shadowmaps: R32F, width=512 samples, height=light index.
@group(0) @binding(3) var shadowmaps: texture_2d<f32>;
// ZDRay / FLMP lightmap atlas (1×1 black when unused).
@group(0) @binding(4) var tex_lightmap: texture_2d<f32>;
@group(0) @binding(5) var samp_lightmap: sampler;

const SHADOWMAP_RES: f32 = 512.0;

fn shadow_visibility(light_i: u32, world_pos: vec3<f32>, light_pos: vec3<f32>) -> f32 {
    if (dyn_lights.count.z == 0u) {
        return 1.0;
    }
    let dxy = world_pos.xy - light_pos.xy;
    let dist = length(dxy);
    if (dist < 1e-3) {
        return 1.0;
    }
    let ang = atan2(dxy.y, dxy.x); // −π‥π
    let u = (ang + 3.14159265) / 6.2831853; // 0‥1
    let sx = i32(clamp(floor(u * SHADOWMAP_RES), 0.0, SHADOWMAP_RES - 1.0));
    let occ = textureLoad(shadowmaps, vec2<i32>(sx, i32(light_i)), 0).r;
    // Soft edge: fully lit until just past the occluder distance.
    return select(0.0, 1.0, dist <= occ * 1.02);
}

@group(1) @binding(0) var tex_albedo: texture_2d<f32>;
@group(1) @binding(1) var samp_albedo: sampler;
// GLDEFS brightmap (black = no change, white = fullbright). 1×1 black when unused.
@group(1) @binding(2) var tex_bright: texture_2d<f32>;
// GLDEFS normal map (flat = 0.5,0.5,1). 1×1 flat when unused.
@group(1) @binding(3) var tex_normal: texture_2d<f32>;
// glTF metal-rough: G=roughness, B=metallic. Default (0,0.5,0) = dielectric.
@group(1) @binding(4) var tex_metal_rough: texture_2d<f32>;

fn cluster_mask_at(world_pos: vec3<f32>) -> u32 {
    let origin = dyn_lights.origin.xyz;
    let cell = max(dyn_lights.cell.xyz, vec3<f32>(1.0));
    let local = (world_pos - origin) / cell;
    let ix = i32(clamp(floor(local.x), 0.0, 15.0));
    let iy = i32(clamp(floor(local.y), 0.0, 8.0));
    let iz = i32(clamp(floor(local.z), 0.0, 23.0));
    let idx = ix + iy * 16 + iz * 16 * 9;
    let tx = idx % 64;
    let ty = idx / 64;
    let t = textureLoad(cluster_masks, vec2<i32>(tx, ty), 0);
    // Reconstruct u32 from unorm bytes (approx — uploaded as raw bytes via rgba8).
    let b0 = u32(round(t.r * 255.0));
    let b1 = u32(round(t.g * 255.0));
    let b2 = u32(round(t.b * 255.0));
    let b3 = u32(round(t.a * 255.0));
    return b0 | (b1 << 8u) | (b2 << 16u) | (b3 << 24u);
}

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: f32,
    // x = alpha; y = 0 solid (holes→black), ≥0.45 masked discard, 1 fuzz
    @location(3) style: vec2<f32>,
    @location(4) lm_uv: vec2<f32>,
    // Geometric world normal (docs/29 U1).
    @location(5) normal: vec3<f32>,
    // Tangent XYZ + bitangent sign (docs/29 U2).
    @location(6) tangent: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: f32,
    @location(2) world_pos: vec3<f32>,
    @location(3) alpha: f32,
    @location(4) fuzz: f32,
    @location(5) lm_uv: vec2<f32>,
    @location(6) normal: vec3<f32>,
    @location(7) tangent: vec4<f32>,
};

struct FsOutMrt {
    @location(0) color: vec4<f32>,
    @location(1) aux: vec4<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = view.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.light = v.light;
    o.world_pos = v.pos;
    o.alpha = v.style.x;
    o.fuzz = v.style.y;
    o.lm_uv = v.lm_uv;
    o.normal = v.normal;
    o.tangent = v.tangent;
    return o;
}

/// Build a world-space normal from tangent-space sample + vertex TBN (U1/U2).
fn world_normal_from_map(n_ts: vec3<f32>, geom_n: vec3<f32>, tangent: vec4<f32>) -> vec3<f32> {
    var n = normalize(geom_n);
    if (dot(n, n) < 1e-6) {
        n = vec3<f32>(0.0, 0.0, 1.0);
    }
    var t = tangent.xyz;
    if (dot(t, t) < 1e-6) {
        // Arbitrary tangent perpendicular to N.
        let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(n.z) > 0.9);
        t = normalize(cross(up, n));
    } else {
        t = normalize(t - n * dot(n, t));
    }
    let b = cross(n, t) * sign(tangent.w);
    let ts = normalize(vec3<f32>(n_ts.x, n_ts.y, abs(n_ts.z)));
    return normalize(t * ts.x + b * ts.y + n * ts.z);
}

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Spectre fuzz (`gl_fuzztype`): returns rgb scale + alpha scale.
fn fuzz_modulate(uv: vec2<f32>, kind: i32, time: f32) -> vec2<f32> {
    let n = hash21(uv * 64.0 + vec2<f32>(time * 3.0, time * 1.7));
    switch kind {
    case 1: { // shadow
        return vec2<f32>(0.15 + 0.35 * n, 0.55 + 0.35 * n);
    }
    case 2: { // smooth
        return vec2<f32>(0.4 + 0.4 * n, 0.35 + 0.45 * n);
    }
    case 3: { // swirly
        let s = (sin(uv.x * 6.2831853 + time * 0.07) * 0.5 + 0.5) * n;
        return vec2<f32>(0.25 + 0.5 * s, 0.4 + 0.4 * s);
    }
    case 4: { // translucent noise
        return vec2<f32>(1.0, 0.2 + 0.6 * n);
    }
    case 5: { // software columns
        let col = i32(floor(uv.x * 320.0 + time * 8.0)) % 3;
        let w = select(0.55 + 0.2 * n, 0.2, col == 0);
        return vec2<f32>(w, w);
    }
    case 6: { // jagged
        let band = i32(floor(uv.y * 48.0 + time)) & 1;
        let w = select(0.7, 0.2, band == 0);
        return vec2<f32>(w, 0.45 + 0.3 * n);
    }
    default: {
        return vec2<f32>(1.0, 1.0);
    }
    }
}

// Port of GZDoom R_DoomLightingEquation (software / lightmode 8) — see docs/13 §5.1.
fn doom_software_shade(sector_light: f32, dist: f32, glob_vis: f32) -> f32 {
    // Softened Doom diminishing so mid-range rooms stay readable on HDR displays.
    let L = clamp(sector_light * 255.0, 0.0, 255.0);
    let vis = max(glob_vis, 0.0001);
    // Mild distance falloff; floor keeps far walls from going black.
    let depth_factor = clamp(vis / max(dist, 1.0), 0.55, 1.35);
    return clamp((L / 255.0) * depth_factor, 0.22, 1.0);
}

fn apply_lightmode(sector_light: f32, world_pos: vec3<f32>) -> f32 {
    let mode = i32(view.light_params.x + 0.5);
    let eye = view.eye.xyz;
    let dist = length(world_pos - eye);
    switch mode {
    case 1: {
        return clamp(sector_light * 1.15, 0.0, 1.0);
    }
    case 2: {
        let atten = clamp(1.0 - dist / 2048.0, 0.25, 1.0);
        return sector_light * atten;
    }
    case 8, 32: {
        return doom_software_shade(sector_light, dist, view.light_params.y);
    }
    case 16: {
        let atten = clamp(1.0 - dist / 4096.0, 0.35, 1.0);
        return sector_light * atten;
    }
    default: {
        let atten = clamp(1.0 - dist / 3072.0, 0.4, 1.0);
        return sector_light * atten;
    }
    }
}

fn shade(v: VsOut) -> vec4<f32> {
    // All `textureSample` calls must run in uniform control flow (WebGPU / WGSL).
    // Sample before clip/alpha discard and before the clustered-light loop.
    let texel = textureSample(tex_albedo, samp_albedo, v.uv);
    let bm = textureSample(tex_bright, samp_albedo, v.uv).rgb;
    let n_samp = textureSample(tex_normal, samp_albedo, v.uv).xyz;
    let mr_samp = textureSample(tex_metal_rough, samp_albedo, v.uv);
    let lm_str = view.lightmap_params.x;
    let lm_fallback = (v.lm_uv.x == 0.0 && v.lm_uv.y == 0.0);
    let lm_uv = select(v.lm_uv, fract(v.world_pos.xy / 256.0), lm_fallback);
    // Always sample; strength scales the contribution (avoids sample-after-branch).
    let lm = textureSample(tex_lightmap, samp_lightmap, lm_uv).rgb;

    let cn = view.clip_plane.xyz;
    if (dot(cn, cn) > 1e-12) {
        let s = dot(cn, v.world_pos) + view.clip_plane.w;
        if (s > 1e-3) {
            discard;
        }
    }
    // Masked midtextures / sprites (`style.y` ≥ 0.45): cut out transparent texels.
    // Solid walls/flats (`style.y` ≈ 0): TEXTUREX holes become opaque black so you
    // never see through one-sided / upper / lower faces (Doom1 incomplete composites).
    var albedo = texel.rgb;
    var texel_a = texel.a;
    if (texel_a < 0.08) {
        if (v.fuzz >= 0.45) {
            discard;
        }
        albedo = vec3<f32>(0.0);
        texel_a = 1.0;
    }
    // DECORATE `Bright` / fullbright: vertex light ≥ 0.999 skips Doom diminishing
    // so Missile attack poses and muzzle flashes stay readable in dark sectors.
    let lit = select(apply_lightmode(v.light, v.world_pos), 1.0, v.light >= 0.999);
    let bm_w = clamp(max(bm.r, max(bm.g, bm.b)), 0.0, 1.0);
    // Sector light, optionally replaced by lightmap (docs/29 U3: modulate, not add).
    var sector_term = vec3<f32>(lit);
    if (lm_str > 1e-4 && !lm_fallback) {
        sector_term = mix(sector_term, lm, clamp(lm_str, 0.0, 1.0));
    }
    // Brightmap: mix sector-lit albedo toward fullbright albedo (GZDoom-style).
    var color = mix(albedo * sector_term, albedo, bm_w);
    let n_ts = n_samp * 2.0 - 1.0;
    let metallic = clamp(mr_samp.b, 0.0, 1.0);
    let roughness = clamp(mr_samp.g, 0.04, 1.0);
    // GPU dyn lights — clustered (16×9×24 bitmasks) or full list fallback.
    let n = min(dyn_lights.count.x, 32u);
    var dyn_add = vec3<f32>(0.0);
    let use_clusters = dyn_lights.count.y != 0u;
    var mask = 0xffffffffu;
    if (use_clusters) {
        mask = cluster_mask_at(v.world_pos);
    }
    for (var i = 0u; i < n; i++) {
        if ((mask & (1u << i)) == 0u) {
            continue;
        }
        let L = dyn_lights.lights[i];
        let radius = max(L.pos_radius.w, 1.0);
        let d = distance(v.world_pos, L.pos_radius.xyz);
        if (d < radius) {
            var fall = pow(1.0 - d / radius, 2.0);
            // Spotlight cone: soft falloff between inner and outer cos.
            let outer_cos = L.spot_dir_outer.w;
            if (outer_cos < 0.999) {
                let to_frag = normalize(v.world_pos - L.pos_radius.xyz);
                let cd = dot(normalize(L.spot_dir_outer.xyz), to_frag);
                let inner_cos = max(L.spot_inner.x, outer_cos);
                let spot = clamp((cd - outer_cos) / max(inner_cos - outer_cos, 1e-4), 0.0, 1.0);
                fall = fall * spot * spot;
            }
            // Geometric N·L (U1) + optional tangent-space normal map via TBN (U2).
            let is_flat_nm = length(n_ts - vec3<f32>(0.0, 0.0, 1.0)) < 0.05;
            let n_world = select(
                world_normal_from_map(n_ts, v.normal, v.tangent),
                normalize(select(v.normal, vec3<f32>(0.0, 0.0, 1.0), dot(v.normal, v.normal) < 1e-6)),
                is_flat_nm,
            );
            let ldir = normalize(L.pos_radius.xyz - v.world_pos);
            let ndotl = max(dot(n_world, ldir), 0.0);
            fall = fall * ndotl;
            var spec_rgb = vec3<f32>(0.0);
            if (!is_flat_nm) {
                let view_dir = normalize(view.eye.xyz - v.world_pos);
                let h = normalize(ldir + view_dir);
                let ndoth = max(dot(n_world, h), 0.0);
                let vdoth = max(dot(view_dir, h), 0.0);
                // GGX D
                let a = roughness * roughness;
                let a2 = a * a;
                let dden = ndoth * ndoth * (a2 - 1.0) + 1.0;
                let d = a2 / max(3.14159265 * dden * dden, 1e-6);
                // Schlick F0
                let f0 = mix(vec3<f32>(0.04), albedo, metallic);
                let fres = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - vdoth, 5.0);
                spec_rgb = fres * d * 0.25;
            } else if (metallic > 0.01 || roughness < 0.49) {
                // Metal-rough without normal map: Blinn on geometric normal.
                let shin = mix(8.0, 256.0, 1.0 - roughness);
                let view_dir = normalize(view.eye.xyz - v.world_pos);
                let h = normalize(ldir + view_dir);
                let ndoth = max(dot(n_world, h), 0.0);
                let f0 = mix(vec3<f32>(0.04), albedo, metallic);
                spec_rgb = f0 * pow(ndoth, shin) * 0.35;
            }
            let sh = shadow_visibility(i, v.world_pos, L.pos_radius.xyz);
            let c = L.color_flags.rgb * fall * sh;
            let s = L.color_flags.rgb * spec_rgb * fall * sh;
            if (L.color_flags.w > 0.5) {
                dyn_add -= c;
            } else {
                dyn_add += c + s;
            }
        }
    }
    color = color + albedo * dyn_add;
    // Negative vertex alpha = additive (`RenderStyle Add`).
    let additive = v.alpha < 0.0;
    var out_a = abs(v.alpha) * texel_a;
    if (v.fuzz > 0.5) {
        let kind = i32(view.light_params.w + 0.5);
        if (kind > 0) {
            let fm = fuzz_modulate(v.uv, kind, view.eye.w);
            color = color * fm.x;
            out_a = out_a * fm.y;
        }
    }
    let fog_density = view.light_params.z;
    if (view.lightmap_params.w > 0.5) {
        // Doom `INVERSECOLORMAP` (invulnerability): fixed colormap, so no sector
        // light or fog; inverted luminance taken in display (gamma) space.
        let g = pow(max(albedo, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
        let lum = dot(g, vec3<f32>(0.299, 0.587, 0.114));
        color = vec3<f32>(pow(1.0 - lum, 2.2));
    } else if (fog_density > 0.0) {
        let dist = length(v.world_pos - view.eye.xyz);
        let fog_factor = clamp(1.0 - exp(-fog_density * dist * 0.001), 0.0, 1.0);
        let fog_color = vec3<f32>(0.05, 0.05, 0.08);
        color = mix(color, fog_color, fog_factor);
    }
    // `lightmap_params.y` = output gain (1.0 = identity; web shell may lift).
    color = color * max(view.lightmap_params.y, 0.01);
    // `lightmap_params.z` = 1 → present on a non-sRGB WebGPU canvas.
    // Textures are sampled linear (sRGB uploads) while Doom lighting is a
    // gamma-space multiply — convert lighting result to display-referred bytes
    // and apply a mild toe lift so midtones match a native sRGB window.
    if (view.lightmap_params.z > 0.5) {
        let c = max(color, vec3<f32>(0.0));
        let lo = c * 12.92;
        let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
        color = select(lo, hi, c > vec3<f32>(0.0031308));
        // Soft toe: keep FLOOR4_8 / dark sectors readable on laptop panels.
        color = color * (0.65 + 0.35 / (color + vec3<f32>(0.35)));
    }
    // Straight alpha; additive is flagged by a negative alpha for the
    // premultiplied entry point (`fs_main_mrt_pm`).
    if (additive) {
        return vec4<f32>(color, -out_a);
    }
    return vec4<f32>(color, out_a);
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    let c = shade(v);
    return vec4<f32>(c.rgb, abs(c.a));
}

/// Pack a unit normal into octahedral RG in [0,1] (docs/29 U16 bootstrap).
fn encode_oct_normal(n: vec3<f32>) -> vec2<f32> {
    var v = normalize(n);
    let inv = 1.0 / max(abs(v.x) + abs(v.y) + abs(v.z), 1e-6);
    var o = v.xy * inv;
    if (v.z < 0.0) {
        let sx = select(-1.0, 1.0, o.x >= 0.0);
        let sy = select(-1.0, 1.0, o.y >= 0.0);
        o = (vec2<f32>(1.0) - abs(o.yx)) * vec2<f32>(sx, sy);
    }
    return o * 0.5 + 0.5;
}

/// Stencil Equal draw with aux G-buffer-lite (oct normal + flags).
@fragment
fn fs_main_mrt(v: VsOut) -> FsOutMrt {
    let n = normalize(select(v.normal, vec3<f32>(0.0, 0.0, 1.0), dot(v.normal, v.normal) < 1e-6));
    let oct = encode_oct_normal(n);
    // Flags in B (U16): bit0 emissive/fullbright, bit1 sprite-ish (fuzz≥0.45).
    var flags = 0.0;
    if (v.light >= 0.999) {
        flags = flags + 1.0 / 255.0;
    }
    if (v.fuzz >= 0.45) {
        flags = flags + 2.0 / 255.0;
    }
    let c = shade(v);
    return FsOutMrt(vec4<f32>(c.rgb, abs(c.a)), vec4<f32>(oct.x, oct.y, flags, 1.0));
}

/// Sprite / translucent pass with premultiplied blending (One, OneMinusSrcAlpha).
/// `RenderStyle Add` writes alpha 0: the scene is kept and the colour adds.
@fragment
fn fs_main_mrt_pm(v: VsOut) -> FsOutMrt {
    let c = shade(v);
    let a = abs(c.a);
    let out_a = select(a, 0.0, c.a < 0.0);
    var flags = 0.0;
    if (v.light >= 0.999) {
        flags = flags + 1.0 / 255.0;
    }
    if (v.fuzz >= 0.45) {
        flags = flags + 2.0 / 255.0;
    }
    let n = normalize(select(v.normal, vec3<f32>(0.0, 0.0, 1.0), dot(v.normal, v.normal) < 1e-6));
    let oct = encode_oct_normal(n);
    return FsOutMrt(vec4<f32>(c.rgb * a, out_a), vec4<f32>(oct.x, oct.y, flags, 1.0));
}

/// Stencil mask: no RGB on slot 0; write dummy slot 1 so Metal updates stencil.
@fragment
fn fs_stencil_mask(_v: VsOut) -> FsOutMrt {
    return FsOutMrt(vec4<f32>(0.0), vec4<f32>(1.0));
}

struct ViewUniforms {
    view_proj: mat4x4<f32>,
    // xyz = eye, w = time seconds (scroll / pulse)
    eye: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: ViewUniforms;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = view.view_proj * vec4<f32>(in.pos, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = view.eye.w;
    // Vertical scroll bands + soft pulse (readable BR wall).
    let band = sin(in.uv.y * 40.0 - t * 3.5) * 0.5 + 0.5;
    let pulse = sin(t * 2.2) * 0.08 + 0.92;
    var c = in.color;
    c.a *= (0.72 + 0.28 * band) * pulse;
    return c;
}

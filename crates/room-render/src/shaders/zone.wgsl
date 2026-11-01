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

struct FsOutMrt {
    @location(0) color: vec4<f32>,
    // Dummy aux (R8 stencil slot); matches UnlitPass MRT layout.
    @location(1) aux: vec4<f32>,
};

fn shade(in: VsOut) -> vec4<f32> {
    let t = view.eye.w;
    // Vertical scroll bands + soft pulse (readable BR wall).
    let band = sin(in.uv.y * 40.0 - t * 3.5) * 0.5 + 0.5;
    let pulse = sin(t * 2.2) * 0.08 + 0.92;
    var c = in.color;
    c.a *= (0.72 + 0.28 * band) * pulse;
    return c;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return shade(in);
}

/// World pass is colour + dummy R8; WebGPU requires a frag output per target.
@fragment
fn fs_main_mrt(in: VsOut) -> FsOutMrt {
    return FsOutMrt(shade(in), vec4<f32>(0.0));
}

// Instanced particle billboards (M10).

struct ViewUniforms {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view: ViewUniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) style: f32,
}

@vertex
fn vs_main(
    @location(0) corner: vec2<f32>,
    @location(1) pos_size: vec4<f32>,
    @location(2) color_alpha: vec4<f32>,
    @location(3) style_f: f32,
) -> VsOut {
    let world = pos_size.xyz
        + view.right.xyz * (corner.x * pos_size.w)
        + view.up.xyz * (corner.y * pos_size.w);
    var out: VsOut;
    out.clip = view.view_proj * vec4<f32>(world, 1.0);
    out.uv = corner + vec2<f32>(0.5, 0.5);
    out.color = color_alpha;
    out.style = style_f;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Soft round mask (style 2) or square (0/1).
    let d = length(in.uv - vec2<f32>(0.5, 0.5));
    var a = in.color.a;
    if (in.style > 1.5) {
        a *= clamp(1.0 - d * 2.0, 0.0, 1.0);
    } else if (d > 0.5) {
        discard;
    }
    let rgb = in.color.rgb * a;
    return vec4<f32>(rgb, a);
}

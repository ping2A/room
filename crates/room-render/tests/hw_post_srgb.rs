//! A GZDoom post shader on an sRGB screen works in gamma space, as in
//! GZDoom: `run_texture` hands it the screen's raw bytes. An inverting
//! shader must turn byte 64 into 191 (what the player sees, inverted), not
//! into ~250 (linear light inverted, then encoded). A non-sRGB screen keeps
//! the plain path. Skipped when no wgpu adapter is present.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_defs::{HardwareShaderDef, HardwareShaderKind};
use room_render::{
    GpuContext, HardwarePostRunner, PostTimes, compile_post_glsl, read_texture_rgba,
};

const W: u32 = 4;
const H: u32 = 2;

const INVERT: &str = r#"
void main()
{
    vec4 c = texture(InputTexture, TexCoord);
    FragColor = vec4(vec3(1.0) - c.rgb, 1.0);
}
"#;

fn texture(gpu: &GpuContext, format: wgpu::TextureFormat) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hw-post-srgb"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn invert_once(gpu: &GpuContext, format: wgpu::TextureFormat, byte: u8) -> [u8; 4] {
    let def = HardwareShaderDef {
        name: "Invert".into(),
        kind: HardwareShaderKind::PostProcess,
        shader: "invert.fp".into(),
        target: "scene".into(),
        enabled: true,
        ..Default::default()
    };
    let compiled = compile_post_glsl(&def, INVERT).expect("compile");
    let mut runner = HardwarePostRunner::new(&gpu.device, format, &compiled);
    let input = texture(gpu, format);
    let output = texture(gpu, format);
    let px: Vec<u8> = (0..W * H).flat_map(|_| [byte, byte, byte, 255]).collect();
    gpu.queue.write_texture(
        input.as_image_copy(),
        &px,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(W * 4),
            rows_per_image: Some(H),
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    let mut enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let times = PostTimes {
        delta: 0.0,
        time: 0.0,
        game: 0.0,
    };
    runner.run_texture(
        &gpu.device,
        &gpu.queue,
        &mut enc,
        &input,
        &output,
        times,
        &[],
    );
    gpu.queue.submit([enc.finish()]);
    let rgba = read_texture_rgba(gpu, &output, W, H).expect("readback");
    [rgba[0], rgba[1], rgba[2], rgba[3]]
}

#[test]
fn post_shaders_see_gamma_space_colour_on_srgb_screens() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping hw_post_srgb: no wgpu adapter");
            return;
        }
    };
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        let px = invert_once(&gpu, format, 64);
        for c in &px[..3] {
            assert!(c.abs_diff(191) <= 1, "{format:?}: 64 inverted to {px:?}");
        }
    }
}

//! UZ6 (docs/36): a UZDoom 5.0 custom post shader reading `InputTime*`, a
//! user `Uniform` and `LastInputTexture`, run on the GPU over two frames.
//! Golden: `testdata/goldens/uz6_post_inputs.txt` (pixel bytes per frame).
//! Skipped when no wgpu adapter is present.
//!
//! Regenerate: `ROOM_UPDATE_GOLDEN=1 cargo test -p room-render --test golden_uz6_post`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_defs::{HardwareShaderDef, HardwareShaderKind};
use room_render::{GpuContext, HardwarePostRunner, PostTimes, compile_post_glsl, read_texture_rgba};
use std::fmt::Write as _;

const W: u32 = 4;
const H: u32 = 2;

const SHADER: &str = r#"
void main()
{
    vec4 now = texture(InputTexture, TexCoord);
    vec4 prev = texture(LastInputTexture, TexCoord);
    FragColor = vec4(mix(now.r, prev.r, 0.5) * gain,
                     InputTimeDelta,
                     fract(InputTime),
                     InputTimeGame / 4.0);
}
"#;

fn texture(gpu: &GpuContext, usage: wgpu::TextureUsages) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("uz6-post"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage,
        view_formats: &[],
    })
}

fn upload(gpu: &GpuContext, tex: &wgpu::Texture, red: impl Fn(u32) -> u8) {
    let mut px = Vec::new();
    for y in 0..H {
        for x in 0..W {
            px.extend_from_slice(&[red(y * W + x), 0, 0, 255]);
        }
    }
    gpu.queue.write_texture(
        tex.as_image_copy(),
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
}

#[test]
fn uz6_post_shader_inputs_match_golden() {
    let gpu = match pollster::block_on(GpuContext::headless()) {
        Ok(g) => g,
        Err(_) => {
            eprintln!("skipping uz6 post golden: no wgpu adapter");
            return;
        }
    };
    let def = HardwareShaderDef {
        name: "Probe".into(),
        kind: HardwareShaderKind::PostProcess,
        shader: "probe.fp".into(),
        target: "scene".into(),
        uniforms: vec![("float".into(), "gain".into())],
        enabled: true,
        ..Default::default()
    };
    let compiled = compile_post_glsl(&def, SHADER).expect("compile");
    let mut runner =
        HardwarePostRunner::new(&gpu.device, wgpu::TextureFormat::Rgba8Unorm, &compiled);
    assert!(runner.set_uniform("gain", &[1.0]));
    assert!(!runner.set_uniform("nope", &[1.0]));
    let input = texture(
        &gpu,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
    );
    let output = texture(
        &gpu,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let out_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let mut text = String::new();
    let frames = [
        (
            PostTimes {
                delta: 0.25,
                time: 10.5,
                game: 1.0,
            },
            200u8,
        ),
        (
            PostTimes {
                delta: 0.5,
                time: 11.75,
                game: 2.0,
            },
            0u8,
        ),
    ];
    for (k, (times, base)) in frames.iter().enumerate() {
        upload(&gpu, &input, |i| base.saturating_add((i * 4) as u8));
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        runner.run(&gpu.device, &gpu.queue, &mut enc, &input, &out_view, *times, &[]);
        gpu.queue.submit([enc.finish()]);
        let rgba = read_texture_rgba(&gpu, &output, W, H).expect("readback");
        let _ = writeln!(text, "frame {k}");
        for px in rgba.chunks_exact(4) {
            let _ = writeln!(text, "{} {} {} {}", px[0], px[1], px[2], px[3]);
        }
    }
    // Frame 1 mixes with frame 0's input (LastInputTexture), so red is about
    // half of frame 0's 200+ and frame 1's own 0+.
    assert!(text.contains("frame 1"));
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/goldens/uz6_post_inputs.txt");
    if std::env::var_os("ROOM_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, want, "post output differs from {}", path.display());
}

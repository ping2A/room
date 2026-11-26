//! The weapon drawn on the GPU (`HudSprites`) gives the pixels the CPU
//! canvas gave: same texels (nearest, `dx * src / dest`), same byte
//! arithmetic for light / tint / alpha, same mirroring, on sRGB and plain
//! screens; and a screen tint equals the canvas's full-screen fill.
//! Skipped when no wgpu adapter is present.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_render::{Canvas, CanvasGpu, GpuContext, HudSprites, SpriteQuad, read_texture_rgba};

const W: u32 = 64;
const H: u32 = 48;

fn target(gpu: &GpuContext, format: wgpu::TextureFormat) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("parity"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// A 7×5 picture with holes and partial alpha.
fn picture() -> (u32, u32, Vec<u8>) {
    let (w, h) = (7u32, 5u32);
    let mut px = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let a = if (x + y) % 5 == 0 {
                0
            } else if x == 3 {
                128
            } else {
                255
            };
            px.extend_from_slice(&[(x * 36) as u8, (y * 50) as u8, 200 - (x * 20) as u8, a]);
        }
    }
    (w, h, px)
}

/// What the CPU path did: nearest scale, light, tint, alpha, mirrors.
fn cpu_pixels(q: &SpriteQuad, (sw, sh, src): &(u32, u32, Vec<u8>)) -> Vec<u8> {
    let mut out = vec![0u8; (q.w * q.h * 4) as usize];
    for dy in 0..q.h {
        for dx in 0..q.w {
            let sx = (dx * sw / q.w) as usize;
            let sy = (dy * sh / q.h) as usize;
            let si = (sy * *sw as usize + sx) * 4;
            if src[si + 3] == 0 {
                continue;
            }
            let o = ((dy * q.w + dx) * 4) as usize;
            out[o..o + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    for px in out.chunks_exact_mut(4) {
        for c in 0..3 {
            px[c] = (f32::from(px[c]) * q.light) as u8;
            px[c] = (f32::from(px[c]) * q.tint[c]) as u8;
        }
        px[3] = (f32::from(px[3]) * q.alpha) as u8;
    }
    let w = q.w as usize;
    if q.flip {
        for row in out.chunks_exact_mut(w * 4) {
            for x in 0..w / 2 {
                for c in 0..4 {
                    row.swap(x * 4 + c, (w - 1 - x) * 4 + c);
                }
            }
        }
    }
    if q.mirror {
        let h = q.h as usize;
        for y in 0..h / 2 {
            for i in 0..w * 4 {
                out.swap(y * w * 4 + i, (h - 1 - y) * w * 4 + i);
            }
        }
    }
    out
}

fn render(
    gpu: &GpuContext,
    format: wgpu::TextureFormat,
    canvas: Canvas,
    quads: &[SpriteQuad],
    tint: Option<[f32; 4]>,
) -> Vec<u8> {
    let tex = target(gpu, format);
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let mut overlay = CanvasGpu::new(gpu, format, W, H);
    overlay.rasterize_frame(canvas, None);
    let mut sprites = HudSprites::new(&gpu.device, format);
    let pic = picture();
    sprites.prepare(gpu, (W, H), quads, tint, &mut |_| Some(pic.clone()));
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.2,
                        g: 0.3,
                        b: 0.4,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        sprites.draw(&mut rp);
        overlay.blit(gpu, &mut rp);
    }
    gpu.queue.submit(Some(encoder.finish()));
    read_texture_rgba(gpu, &tex, W, H).expect("readback")
}

fn max_diff(a: &[u8], b: &[u8]) -> u8 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

#[test]
fn gpu_hud_pictures_and_tints_match_the_canvas() {
    let Ok(gpu) = pollster::block_on(GpuContext::headless()) else {
        eprintln!("skipping hud_sprites_parity: no wgpu adapter");
        return;
    };
    let quads = [
        SpriteQuad {
            key: "a".into(),
            x: 3,
            y: 4,
            w: 23,
            h: 17,
            light: 0.8,
            tint: [1.0, 0.6, 1.0],
            alpha: 1.0,
            flip: false,
            mirror: false,
            smooth: false,
        },
        SpriteQuad {
            key: "a".into(),
            x: 30,
            y: 20,
            w: 30,
            h: 25,
            light: 1.0,
            tint: [1.0; 3],
            alpha: 0.7,
            flip: true,
            mirror: true,
            smooth: false,
        },
    ];
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        // CPU: the scaled pixels blitted into the canvas.
        let mut canvas = Canvas::new();
        canvas.clear([0.0; 4]);
        for q in &quads {
            canvas.blit_rgba(q.x as f32, q.y as f32, q.w, q.h, cpu_pixels(q, &picture()));
        }
        let cpu = render(&gpu, format, canvas, &[], None);
        let mut empty = Canvas::new();
        empty.clear([0.0; 4]);
        let gpu_px = render(&gpu, format, empty, &quads, None);
        // Pictures do not overlap: blending order cannot differ.
        assert!(max_diff(&cpu, &gpu_px) <= 1, "{format:?}: pictures differ");

        // A pain flash: canvas fill vs GPU tint.
        let red = [0.9, 0.1, 0.1, 0.35];
        let mut fill = Canvas::new();
        fill.clear([0.0; 4]);
        fill.fill_rect(0.0, 0.0, W as f32, H as f32, red);
        let cpu = render(&gpu, format, fill, &[], None);
        let mut empty = Canvas::new();
        empty.clear([0.0; 4]);
        let gpu_px = render(&gpu, format, empty, &[], Some(red));
        assert!(max_diff(&cpu, &gpu_px) <= 1, "{format:?}: tints differ");
    }
}

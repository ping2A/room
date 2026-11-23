//! GPU upload / blit of a CPU [`Canvas`] RGBA overlay.

use crate::canvas::{Canvas, CanvasCmd};
use crate::device::GpuContext;
use crate::fonts::{Fon1Face, TextDraw};

const OVERLAY_WGSL: &str = r#"
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    // Fullscreen triangle.
    var pos = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2(0.0, 1.0),
        vec2(2.0, 1.0),
        vec2(0.0, -1.0),
    );
    var o: VsOut;
    o.clip = vec4(pos[i], 0.0, 1.0);
    o.uv = uv[i];
    return o;
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, v.uv);
    // Holes stay holes. Partial alpha must blend: the pain flash is a red
    // fill around a=0.1..0.9, and forcing it to 1 painted a solid red sheet.
    if (c.a < 0.02) {
        discard;
    }
    return c;
}
"#;

/// CPU RGBA scratch + GPU texture for 2D overlay.
pub struct CanvasGpu {
    raster: CanvasRaster,
    texture: wgpu::Texture,
    /// Kept alive for `bind`.
    _view: wgpu::TextureView,
    /// Kept alive for `bind`.
    _sampler: wgpu::Sampler,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl CanvasGpu {
    /// Create an overlay matching the swapchain size.
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let device = &gpu.device;
        // Doom PLAYPAL bytes are display-referred. On an sRGB swapchain, store the
        // overlay as sRGB so sample→linear→encode round-trips. On a non-sRGB
        // WebGPU canvas (`Bgra8Unorm`), an sRGB overlay texture would decode to
        // linear and be shown un-encoded (STBAR / weapon look crushed-dark).
        let overlay_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: overlay_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas-overlay"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_WGSL.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas-pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Self {
            raster: CanvasRaster::new(width, height),
            texture,
            _view: view,
            _sampler: sampler,
            bind,
            pipeline,
        }
    }

    /// Rasterize canvas commands (optionally with a FON1 face for text).
    pub fn rasterize(&mut self, canvas: &Canvas, font: Option<&Fon1Face>) {
        self.raster.draw(&canvas.cmds, font);
    }

    /// [`Self::rasterize`] for a per-frame canvas: a frame identical to the
    /// previous one (a HUD at rest) costs one comparison, and only rows that
    /// changed are uploaded by [`Self::blit`].
    pub fn rasterize_frame(&mut self, canvas: Canvas, font: Option<&Fon1Face>) {
        self.raster.draw_frame(canvas, font);
    }

    /// The CPU pixels (`width`×`height` RGBA).
    #[must_use]
    pub fn raster(&self) -> &CanvasRaster {
        &self.raster
    }

    /// Upload CPU pixels and draw the overlay (no depth).
    pub fn blit(&mut self, gpu: &GpuContext, pass: &mut wgpu::RenderPass<'_>) {
        let width = self.raster.width;
        for (top, bottom) in self.raster.take_changed_rows() {
            let row_bytes = (width * 4) as usize;
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: top, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &self.raster.rgba[top as usize * row_bytes..bottom as usize * row_bytes],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(bottom - top),
                },
                wgpu::Extent3d {
                    width,
                    height: bottom - top,
                    depth_or_array_layers: 1,
                },
            );
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// CPU side of the overlay: the RGBA pixels and which rows hold anything.
///
/// Only rows drawn last frame are wiped and only rows that changed are
/// uploaded, so a status bar and weapon cost their own rows, not the screen.
/// The pixels are exactly those of a full clear-and-redraw.
#[derive(Clone, Debug)]
pub struct CanvasRaster {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    /// Rows that may hold a non-zero pixel (every other row is all zero).
    drawn: Vec<bool>,
    /// Rows changed since [`Self::take_changed_rows`].
    changed: Vec<bool>,
    /// Rows written or wiped by the current draw.
    touched: Vec<bool>,
    /// The drawn rows as they were before the current draw.
    before: Vec<u8>,
    /// `drawn` before the current draw.
    was_drawn: Vec<bool>,
    /// Commands last drawn by [`Self::draw_frame`] (no font).
    last: Option<Vec<CanvasCmd>>,
}

impl CanvasRaster {
    /// Transparent `width`×`height` pixels.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let rows = height as usize;
        Self {
            width,
            height,
            rgba: vec![0u8; (width * height * 4) as usize],
            drawn: vec![false; rows],
            // A fresh texture is zeroed already.
            changed: vec![false; rows],
            touched: vec![false; rows],
            before: vec![0u8; (width * height * 4) as usize],
            was_drawn: vec![false; rows],
            last: None,
        }
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// RGBA pixels, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.rgba
    }

    /// Row runs `[top, bottom)` changed since the last call (gaps of a few
    /// rows are bridged to keep uploads few).
    pub fn take_changed_rows(&mut self) -> Vec<(u32, u32)> {
        const BRIDGE: usize = 8;
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for (y, c) in self.changed.iter_mut().enumerate() {
            if !std::mem::take(c) {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            let y = y as u32;
            match runs.last_mut() {
                Some(run) if (y - run.1) as usize <= BRIDGE => run.1 = y + 1,
                _ => runs.push((y, y + 1)),
            }
        }
        runs
    }

    fn mark(&mut self, top: u32, bottom: u32) {
        let (top, bottom) = (
            top.min(self.height) as usize,
            bottom.min(self.height) as usize,
        );
        for y in top..bottom {
            self.drawn[y] = true;
            self.touched[y] = true;
        }
    }

    /// Zero every drawn row.
    fn wipe(&mut self) {
        let row = (self.width * 4) as usize;
        for y in 0..self.height as usize {
            if std::mem::take(&mut self.drawn[y]) {
                self.rgba[y * row..(y + 1) * row].fill(0);
                self.touched[y] = true;
            }
        }
    }

    /// [`Self::draw`], skipped when `canvas` is the same as the last frame.
    pub fn draw_frame(&mut self, canvas: Canvas, font: Option<&Fon1Face>) {
        if font.is_none() && self.last.as_deref() == Some(canvas.cmds.as_slice()) {
            return;
        }
        self.draw(&canvas.cmds, font);
        if font.is_none() {
            self.last = Some(canvas.cmds);
        }
    }

    /// Clear to transparent and draw `cmds`.
    pub fn draw(&mut self, cmds: &[CanvasCmd], font: Option<&Fon1Face>) {
        self.last = None;
        let row = (self.width * 4) as usize;
        self.was_drawn.copy_from_slice(&self.drawn);
        for y in 0..self.height as usize {
            if self.drawn[y] {
                self.before[y * row..(y + 1) * row]
                    .copy_from_slice(&self.rgba[y * row..(y + 1) * row]);
            }
        }
        self.wipe();
        self.paint(cmds, font);
        // A row redrawn to the same bytes (a status bar at rest under a
        // bobbing weapon) needs no upload.
        for y in 0..self.height as usize {
            if !std::mem::take(&mut self.touched[y]) {
                continue;
            }
            let now = &self.rgba[y * row..(y + 1) * row];
            let same = if self.was_drawn[y] {
                now == &self.before[y * row..(y + 1) * row]
            } else {
                now.iter().all(|&b| b == 0)
            };
            if !same {
                self.changed[y] = true;
            }
        }
    }

    fn paint(&mut self, cmds: &[CanvasCmd], font: Option<&Fon1Face>) {
        for cmd in cmds {
            match cmd {
                CanvasCmd::Clear { color } => {
                    let c = rgba8(*color);
                    if c == [0; 4] {
                        self.wipe();
                        continue;
                    }
                    if c.iter().all(|&b| b == c[0]) {
                        self.rgba.fill(c[0]);
                    } else {
                        for px in self.rgba.chunks_exact_mut(4) {
                            px.copy_from_slice(&c);
                        }
                    }
                    self.mark(0, self.height);
                }
                CanvasCmd::FillRect { x, y, w, h, color } => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x0 = (*x as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y0 = (*y as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x1 = (((*x + *w) as i32).max(0) as u32).min(self.width);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y1 = (((*y + *h) as i32).max(0) as u32).min(self.height);
                    let sr = color[0].clamp(0.0, 1.0);
                    let sg = color[1].clamp(0.0, 1.0);
                    let sb = color[2].clamp(0.0, 1.0);
                    let sa = color[3].clamp(0.0, 1.0);
                    if x0 >= x1 || y0 >= y1 || (sa < 0.999 && sa <= 0.001) {
                        continue;
                    }
                    self.mark(y0, y1);
                    let width = self.width;
                    let span = |py: u32| {
                        ((py * width + x0) * 4) as usize..((py * width + x1) * 4) as usize
                    };
                    if sa >= 0.999 {
                        let c = rgba8(*color);
                        for py in y0..y1 {
                            for px in self.rgba[span(py)].chunks_exact_mut(4) {
                                px.copy_from_slice(&c);
                            }
                        }
                        continue;
                    }
                    // Source-over in float, as before; one remembered result
                    // covers runs of identical destination pixels (an empty
                    // area under a screen tint is one computation).
                    let blend = |d: [u8; 4]| {
                        let dr = f32::from(d[0]) / 255.0;
                        let dg = f32::from(d[1]) / 255.0;
                        let db = f32::from(d[2]) / 255.0;
                        let da = f32::from(d[3]) / 255.0;
                        let out_a = sa + da * (1.0 - sa);
                        let out = |s: f32, d: f32| {
                            if out_a > 0.0 {
                                (s * sa + d * da * (1.0 - sa)) / out_a
                            } else {
                                0.0
                            }
                        };
                        rgba8([out(sr, dr), out(sg, dg), out(sb, db), out_a])
                    };
                    let mut memo: Option<([u8; 4], [u8; 4])> = None;
                    for py in y0..y1 {
                        for px in self.rgba[span(py)].chunks_exact_mut(4) {
                            let d = [px[0], px[1], px[2], px[3]];
                            let o = match memo {
                                Some((k, o)) if k == d => o,
                                _ => {
                                    let o = blend(d);
                                    memo = Some((d, o));
                                    o
                                }
                            };
                            px.copy_from_slice(&o);
                        }
                    }
                }
                CanvasCmd::BlitRgba { x, y, w, h, pixels } => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x0 = (*x as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y0 = (*y as i32).max(0) as u32;
                    let src_w = *w;
                    let src_h = *h;
                    let need = (src_w as usize)
                        .saturating_mul(src_h as usize)
                        .saturating_mul(4);
                    if src_w == 0 || src_h == 0 || pixels.len() < need {
                        continue;
                    }
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x_off = (*x as i32).min(0).unsigned_abs();
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y_off = (*y as i32).min(0).unsigned_abs();
                    if y_off >= src_h || x_off >= src_w || x0 >= self.width {
                        continue;
                    }
                    self.mark(y0, y0.saturating_add(src_h - y_off));
                    for row in y_off..src_h {
                        let dy = y0 + (row - y_off);
                        if dy >= self.height {
                            break;
                        }
                        for col in x_off..src_w {
                            let dx = x0 + (col - x_off);
                            if dx >= self.width {
                                break;
                            }
                            let si = ((row * src_w + col) * 4) as usize;
                            let a = pixels[si + 3];
                            if a == 0 {
                                continue;
                            }
                            let o = ((dy * self.width + dx) * 4) as usize;
                            if a == 255 {
                                self.rgba[o..o + 4].copy_from_slice(&pixels[si..si + 4]);
                                continue;
                            }
                            let sa = u32::from(a);
                            let da = u32::from(self.rgba[o + 3]);
                            let out_a = sa * 255 + da * (255 - sa);
                            if out_a == 0 {
                                continue;
                            }
                            for c in 0..3 {
                                let sc = u32::from(pixels[si + c]);
                                let dc = u32::from(self.rgba[o + c]);
                                #[allow(clippy::cast_possible_truncation)]
                                {
                                    self.rgba[o + c] =
                                        ((sc * sa * 255 + dc * da * (255 - sa)) / out_a) as u8;
                                }
                            }
                            #[allow(clippy::cast_possible_truncation)]
                            {
                                self.rgba[o + 3] = (out_a / 255) as u8;
                            }
                        }
                    }
                }
                CanvasCmd::Text { x, y, text, color } => {
                    if let Some(face) = font {
                        // Glyph rows are the face's business: mark them all.
                        self.mark(0, self.height);
                        let c = rgba8(*color);
                        #[allow(clippy::cast_possible_truncation)]
                        let mut dest = TextDraw {
                            rgba: &mut self.rgba,
                            buf_w: self.width,
                            buf_h: self.height,
                            x: *x as i32,
                            y: *y as i32,
                            color: c,
                        };
                        face.draw_text(&mut dest, text);
                    } else {
                        #[allow(clippy::cast_possible_truncation)]
                        let (tx, ty) = (*x as i32, *y as i32);
                        #[allow(clippy::cast_sign_loss)]
                        self.mark(ty.max(0) as u32, (ty + 7).max(0) as u32);
                        paint_fallback_text(
                            &mut self.rgba,
                            self.width,
                            self.height,
                            tx,
                            ty,
                            text,
                            rgba8(*color),
                        );
                    }
                }
            }
        }
    }
}

fn rgba8(c: [f32; 4]) -> [u8; 4] {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

/// Tiny 5×7 uppercase bitmap for `CanvasCmd::Text` when no FON1 face is bound.
fn paint_fallback_text(
    rgba: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    origin_x: i32,
    origin_y: i32,
    text: &str,
    color: [u8; 4],
) {
    // Columns packed in low 5 bits, 7 rows; bit0 = leftmost.
    fn glyph(ch: u8) -> [u8; 7] {
        match ch.to_ascii_uppercase() {
            b'0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
            b'1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
            b'2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
            b'3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
            b'4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
            b'5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
            b'6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
            b'7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
            b'8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
            b'9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
            b'%' => [0x19, 0x1A, 0x04, 0x08, 0x13, 0x03, 0x00],
            b'-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
            b'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
            b'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
            b'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
            b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
            b'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
            b'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
            b'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
            b'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
            b'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
            b'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
            b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
            b'M' => [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11],
            b'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
            b'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
            b'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
            b'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
            b'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
            b'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
            b'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
            b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
            b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
            b'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
            b'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
            b'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
            b'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
            b'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
            b'.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
            b',' => [0x00, 0x00, 0x00, 0x00, 0x0C, 0x04, 0x08],
            b'!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04],
            b'?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
            b':' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
            b'\'' => [0x04, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00],
            b'/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
            b'+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
            b'(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
            b')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
            _ => [0x00; 7],
        }
    }
    let mut cx = origin_x;
    for ch in text.bytes() {
        if ch == b' ' {
            cx += 6;
            continue;
        }
        let rows = glyph(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                let px = cx + col;
                let py = origin_y + row as i32;
                if px < 0 || py < 0 || px >= buf_w as i32 || py >= buf_h as i32 {
                    continue;
                }
                let o = ((py as u32 * buf_w + px as u32) * 4) as usize;
                if o + 4 <= rgba.len() {
                    rgba[o..o + 4].copy_from_slice(&color);
                }
            }
        }
        cx += 6;
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]

    use super::*;

    /// The rasterizer as it was (a full clear and redraw every frame): the
    /// reference the row-tracking one must match byte for byte.
    fn reference(width: u32, height: u32, rgba: &mut [u8], canvas: &Canvas) {
        let font: Option<&Fon1Face> = None;
        rgba.fill(0);
        for cmd in &canvas.cmds {
            match cmd {
                CanvasCmd::Clear { color } => {
                    let c = rgba8(*color);
                    for px in rgba.chunks_exact_mut(4) {
                        px.copy_from_slice(&c);
                    }
                }
                CanvasCmd::FillRect { x, y, w, h, color } => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x0 = (*x as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y0 = (*y as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x1 = ((*x + *w) as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y1 = ((*y + *h) as i32).max(0) as u32;
                    let sr = color[0].clamp(0.0, 1.0);
                    let sg = color[1].clamp(0.0, 1.0);
                    let sb = color[2].clamp(0.0, 1.0);
                    let sa = color[3].clamp(0.0, 1.0);
                    for py in y0..y1.min(height) {
                        for px in x0..x1.min(width) {
                            let o = ((py * width + px) * 4) as usize;
                            if sa >= 0.999 {
                                rgba[o..o + 4].copy_from_slice(&rgba8(*color));
                                continue;
                            }
                            if sa <= 0.001 {
                                continue;
                            }
                            // Straight alpha over (src over dst) in the overlay buffer.
                            let dr = f32::from(rgba[o]) / 255.0;
                            let dg = f32::from(rgba[o + 1]) / 255.0;
                            let db = f32::from(rgba[o + 2]) / 255.0;
                            let da = f32::from(rgba[o + 3]) / 255.0;
                            let out_a = sa + da * (1.0 - sa);
                            let out_r = if out_a > 0.0 {
                                (sr * sa + dr * da * (1.0 - sa)) / out_a
                            } else {
                                0.0
                            };
                            let out_g = if out_a > 0.0 {
                                (sg * sa + dg * da * (1.0 - sa)) / out_a
                            } else {
                                0.0
                            };
                            let out_b = if out_a > 0.0 {
                                (sb * sa + db * da * (1.0 - sa)) / out_a
                            } else {
                                0.0
                            };
                            rgba[o..o + 4].copy_from_slice(&rgba8([out_r, out_g, out_b, out_a]));
                        }
                    }
                }
                CanvasCmd::BlitRgba { x, y, w, h, pixels } => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x0 = (*x as i32).max(0) as u32;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y0 = (*y as i32).max(0) as u32;
                    let src_w = *w;
                    let src_h = *h;
                    let need = (src_w as usize)
                        .saturating_mul(src_h as usize)
                        .saturating_mul(4);
                    if src_w == 0 || src_h == 0 || pixels.len() < need {
                        continue;
                    }
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let x_off = (*x as i32).min(0).unsigned_abs();
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let y_off = (*y as i32).min(0).unsigned_abs();
                    for row in y_off..src_h {
                        let dy = y0 + (row - y_off);
                        if dy >= height {
                            break;
                        }
                        for col in x_off..src_w {
                            let dx = x0 + (col - x_off);
                            if dx >= width {
                                break;
                            }
                            let si = ((row * src_w + col) * 4) as usize;
                            let a = pixels[si + 3];
                            if a == 0 {
                                continue;
                            }
                            let o = ((dy * width + dx) * 4) as usize;
                            if a == 255 {
                                rgba[o..o + 4].copy_from_slice(&pixels[si..si + 4]);
                                continue;
                            }
                            // Straight alpha over (translucent HUD pictures / fades).
                            let sa = u32::from(a);
                            let da = u32::from(rgba[o + 3]);
                            let out_a = sa * 255 + da * (255 - sa);
                            if out_a == 0 {
                                continue;
                            }
                            for c in 0..3 {
                                let sc = u32::from(pixels[si + c]);
                                let dc = u32::from(rgba[o + c]);
                                #[allow(clippy::cast_possible_truncation)]
                                {
                                    rgba[o + c] =
                                        ((sc * sa * 255 + dc * da * (255 - sa)) / out_a) as u8;
                                }
                            }
                            #[allow(clippy::cast_possible_truncation)]
                            {
                                rgba[o + 3] = (out_a / 255) as u8;
                            }
                        }
                    }
                }
                CanvasCmd::Text { x, y, text, color } => {
                    if let Some(face) = font {
                        let c = rgba8(*color);
                        #[allow(clippy::cast_possible_truncation)]
                        let mut dest = TextDraw {
                            rgba: rgba,
                            buf_w: width,
                            buf_h: height,
                            x: *x as i32,
                            y: *y as i32,
                            color: c,
                        };
                        face.draw_text(&mut dest, text);
                    } else {
                        // Last-resort 5×7 bitmap so HUD/menu never go fully blank
                        // when SoftFont failed to resolve (prefer Patch STTNUM/STCFN).
                        #[allow(clippy::cast_possible_truncation)]
                        paint_fallback_text(
                            rgba,
                            width,
                            height,
                            *x as i32,
                            *y as i32,
                            text,
                            rgba8(*color),
                        );
                    }
                }
            }
        }
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn f(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * ((self.next() >> 40) as f32 / (1u64 << 24) as f32)
        }
        fn pick(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn random_color(r: &mut Rng) -> [f32; 4] {
        let alpha = match r.pick(4) {
            0 => 1.0,
            1 => 0.0,
            2 => r.f(-0.2, 1.2),
            _ => r.f(0.05, 0.95),
        };
        [r.f(-0.1, 1.1), r.f(0.0, 1.0), r.f(0.0, 1.0), alpha]
    }

    fn random_canvas(r: &mut Rng, w: u32, h: u32) -> Canvas {
        let mut c = Canvas::new();
        if r.pick(2) == 0 {
            c.clear([0.0; 4]);
        }
        for _ in 0..r.pick(9) {
            let (x, y) = (r.f(-40.0, w as f32 + 10.0), r.f(-40.0, h as f32 + 10.0));
            match r.pick(10) {
                0 => c.clear(random_color(r)),
                1 | 2 | 3 => {
                    let (fw, fh) = (r.f(-5.0, w as f32), r.f(-5.0, h as f32));
                    c.fill_rect(x, y, fw, fh, random_color(r));
                }
                4 => c.fill_rect(0.0, 0.0, w as f32, h as f32, random_color(r)),
                5 | 6 | 7 => {
                    let (bw, bh) = (1 + r.pick(40) as u32, 1 + r.pick(30) as u32);
                    let mut px = vec![0u8; (bw * bh * 4) as usize];
                    for p in px.chunks_exact_mut(4) {
                        p.copy_from_slice(&[
                            r.next() as u8,
                            r.next() as u8,
                            r.next() as u8,
                            match r.pick(3) {
                                0 => 0,
                                1 => 255,
                                _ => r.next() as u8,
                            },
                        ]);
                    }
                    c.blit_rgba(x, y, bw, bh, px);
                }
                _ => c.text(x, y, "HP 100% AMMO", random_color(r)),
            }
        }
        c
    }

    /// Frame after frame, the pixels equal a full redraw, and every row whose
    /// pixels changed is reported for upload: a texture updated with the
    /// reported rows only stays equal to the pixels.
    #[test]
    fn row_tracking_matches_a_full_redraw_every_frame() {
        let mut r = Rng(0x9e37_79b9_7f4a_7c15);
        for (w, h) in [(64u32, 48u32), (37, 61), (1, 1), (128, 16)] {
            let mut raster = CanvasRaster::new(w, h);
            let mut texture = vec![0u8; (w * h * 4) as usize];
            let mut previous: Option<Canvas> = None;
            for frame in 0..400 {
                // Often repeat the last frame (a HUD at rest).
                let canvas = match &previous {
                    Some(p) if r.pick(4) == 0 => p.clone(),
                    _ => random_canvas(&mut r, w, h),
                };
                let mut want = vec![0u8; (w * h * 4) as usize];
                reference(w, h, &mut want, &canvas);
                if r.pick(3) == 0 {
                    raster.draw(&canvas.cmds, None);
                } else {
                    raster.draw_frame(canvas.clone(), None);
                }
                assert!(
                    raster.pixels() == want.as_slice(),
                    "{w}x{h} frame {frame}: pixels differ"
                );
                if r.pick(5) != 0 {
                    let row = (w * 4) as usize;
                    for (top, bottom) in raster.take_changed_rows() {
                        let (a, b) = (top as usize * row, bottom as usize * row);
                        texture[a..b].copy_from_slice(&raster.pixels()[a..b]);
                    }
                    assert!(
                        texture == want,
                        "{w}x{h} frame {frame}: uploaded rows miss a change"
                    );
                }
                previous = Some(canvas);
            }
        }
    }

    #[test]
    fn a_hud_at_rest_uploads_nothing_and_a_bar_uploads_its_rows() {
        let mut raster = CanvasRaster::new(320, 200);
        let bar = |c: &mut Canvas| {
            c.clear([0.0; 4]);
            c.fill_rect(0.0, 168.0, 320.0, 32.0, [0.5, 0.5, 0.5, 1.0]);
        };
        let mut a = Canvas::new();
        bar(&mut a);
        raster.draw_frame(a.clone(), None);
        assert_eq!(raster.take_changed_rows(), vec![(168, 200)]);
        raster.draw_frame(a, None);
        assert!(
            raster.take_changed_rows().is_empty(),
            "same frame: nothing to upload"
        );
        let mut b = Canvas::new();
        bar(&mut b);
        b.fill_rect(0.0, 0.0, 320.0, 8.0, [1.0, 0.0, 0.0, 0.5]);
        raster.draw_frame(b, None);
        assert_eq!(
            raster.take_changed_rows(),
            vec![(0, 8)],
            "a message adds its rows only"
        );
        let mut c = Canvas::new();
        bar(&mut c);
        raster.draw_frame(c, None);
        assert_eq!(
            raster.take_changed_rows(),
            vec![(0, 8)],
            "and wiping it uploads them again"
        );
    }
}

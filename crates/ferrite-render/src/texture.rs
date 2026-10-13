//! CPU-side texture registry + GPU upload helpers.

use ferrite_core::TextureId;
use ferrite_formats::{Image, ImagePixels, Playpal};
use std::collections::BTreeMap;

/// How a texture is stored on the GPU once uploaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormatKind {
    /// Paletted 8-bit indices (`R8Uint`).
    Paletted,
    /// Expanded or true-colour RGBA.
    Rgba,
}

/// Metadata for one managed texture.
#[derive(Clone, Debug)]
pub struct TextureEntry {
    /// Engine texture id.
    pub id: TextureId,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Preferred GPU format.
    pub kind: TextureFormatKind,
    /// Optional GPU texture handle (set after upload).
    pub gpu: Option<wgpu::Texture>,
    /// Optional GPU view.
    pub view: Option<wgpu::TextureView>,
}

/// Texture cache keyed by [`TextureId`].
#[derive(Default)]
pub struct TextureManager {
    entries: BTreeMap<u32, TextureEntry>,
    next_id: u32,
}

impl TextureManager {
    /// Empty manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a fresh [`TextureId`].
    #[must_use]
    pub fn alloc_id(&mut self) -> TextureId {
        let id = TextureId::from_raw(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// Register (or replace) a texture's CPU metadata.
    pub fn register(&mut self, entry: TextureEntry) {
        self.entries.insert(entry.id.raw(), entry);
    }

    /// Lookup by id.
    #[must_use]
    pub fn get(&self, id: TextureId) -> Option<&TextureEntry> {
        self.entries.get(&id.raw())
    }

    /// Number of registered textures.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Expand a paletted/RGBA [`Image`] with `PLAYPAL` palette 0 and upload as `Rgba8UnormSrgb`.
    pub fn upload_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &Image,
        playpal: Option<&Playpal>,
    ) -> TextureId {
        let id = self.alloc_id();
        let (w, h) = (image.width(), image.height());
        let rgba = expand_image(image, playpal);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tex"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.register(TextureEntry {
            id,
            width: w,
            height: h,
            kind: TextureFormatKind::Rgba,
            gpu: Some(texture),
            view: Some(view),
        });
        id
    }

    /// Upload or replace an RGBA8 texture (camera canvases / dynamic skins).
    pub fn upsert_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: Option<TextureId>,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> TextureId {
        let expected = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        let owned;
        let pixels: &[u8] = if rgba.len() >= expected {
            &rgba[..expected]
        } else {
            owned = {
                let mut v = rgba.to_vec();
                v.resize(expected, 0);
                v
            };
            owned.as_slice()
        };
        if let Some(id) = id {
            if let Some(entry) = self.entries.get_mut(&id.raw()) {
                if entry.width == width
                    && entry.height == height
                    && entry.gpu.is_some()
                    && matches!(entry.kind, TextureFormatKind::Rgba)
                {
                    if let Some(tex) = entry.gpu.as_ref() {
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: tex,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            pixels,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(width * 4),
                                rows_per_image: Some(height),
                            },
                            wgpu::Extent3d {
                                width,
                                height,
                                depth_or_array_layers: 1,
                            },
                        );
                        return id;
                    }
                }
            }
        }
        let id = id.unwrap_or_else(|| self.alloc_id());
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tex-rgba"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.register(TextureEntry {
            id,
            width,
            height,
            kind: TextureFormatKind::Rgba,
            gpu: Some(texture),
            view: Some(view),
        });
        id
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use ferrite_formats::{Image, ImagePixels, PalettedImage};

    #[test]
    fn patch_index_zero_is_transparent_when_flagged() {
        let image = Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: true,
            pixels: ImagePixels::Paletted(PalettedImage {
                width: 2,
                height: 1,
                indices: vec![0, 1],
                alpha: Vec::new(),
            }),
        };
        let rgba = expand_image(&image, None);
        assert_eq!(rgba[3], 0, "hole");
        assert_eq!(rgba[7], 255, "opaque index");
    }

    #[test]
    fn flat_index_zero_stays_opaque() {
        let image = Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: false,
            pixels: ImagePixels::Paletted(PalettedImage {
                width: 1,
                height: 1,
                indices: vec![0],
                alpha: Vec::new(),
            }),
        };
        let rgba = expand_image(&image, None);
        assert_eq!(rgba[3], 255);
    }
}

fn expand_image(image: &Image, playpal: Option<&Playpal>) -> Vec<u8> {
    let w = image.width() as usize;
    let h = image.height() as usize;
    let mut out = vec![0u8; w * h * 4];
    match &image.pixels {
        ImagePixels::Rgba8(rgba) => {
            let n = (w * h * 4).min(rgba.pixels.len());
            out[..n].copy_from_slice(&rgba.pixels[..n]);
        }
        ImagePixels::Paletted(pal) => {
            let palette = playpal.and_then(|p| p.palettes.first());
            let use_idx0_hole = image.has_translucency && pal.alpha.is_empty();
            for (i, &idx) in pal.indices.iter().enumerate().take(w * h) {
                // Patches/sprites: empty alpha + `has_translucency` → index 0 is the hole.
                // Flats / opaque sources: index 0 is a real color.
                let alpha = if !pal.alpha.is_empty() {
                    pal.alpha.get(i).copied().unwrap_or(255)
                } else if use_idx0_hole && idx == 0 {
                    0
                } else {
                    255
                };
                let (r, g, b, a) = if let Some(pal0) = palette {
                    let o = usize::from(idx) * 3;
                    if o + 2 < pal0.rgb.len() {
                        (pal0.rgb[o], pal0.rgb[o + 1], pal0.rgb[o + 2], alpha)
                    } else {
                        (idx, idx, idx, alpha)
                    }
                } else {
                    (idx, idx, idx, alpha)
                };
                let base = i * 4;
                out[base] = r;
                out[base + 1] = g;
                out[base + 2] = b;
                out[base + 3] = a;
            }
        }
    }
    out
}

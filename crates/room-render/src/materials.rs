//! Name → GPU texture catalog (walls/flats from VFS + TEXTUREX).

use crate::texture::TextureManager;
use room_core::TextureId;
use room_formats::{
    Image, ImagePixels, PalettedImage, Playpal, Pnames, TexturexDef, decode_image, flat_dimensions,
    parse_playpal, parse_pnames, parse_texturex,
};
use room_vfs::{Namespace, Vfs};
use std::collections::BTreeMap;

/// Resolved materials keyed by Doom texture / flat name (uppercase).
pub struct MaterialCatalog {
    /// Name → texture id.
    by_name: BTreeMap<String, TextureId>,
    /// Shared texture manager.
    pub textures: TextureManager,
    /// PLAYPAL (palette 0 used for expansion).
    playpal: Option<Playpal>,
    /// `PNAMES` table when loaded.
    pnames: Option<Pnames>,
    /// TEXTURE1/2 definitions keyed by uppercase name.
    texturex: BTreeMap<String, TexturexDef>,
    /// `TX_START` textures (uppercase) → pixel size; these replace TEXTUREX
    /// composites of the same name (GZDoom precedence).
    tx_sizes: BTreeMap<String, (u32, u32)>,
    /// Base texture (uppercase) → brightmap lump name (GLDEFS).
    brightmaps: BTreeMap<String, String>,
    /// Base texture → normal / specular / PBR map lump names (GLDEFS `material`).
    pbr_maps: BTreeMap<String, MaterialMaps>,
    /// Nearest sampler shared by all.
    pub sampler: Option<wgpu::Sampler>,
    /// Wanted anisotropic filtering (`gl_texture_filter_anisotropic`; 1 = off).
    pub anisotropy: u16,
    /// Anisotropy the current [`Self::sampler`] was built with.
    sampler_anisotropy: u16,
}

/// Optional GLDEFS material maps (paths / lump names).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaterialMaps {
    /// Normal map.
    pub normal: Option<String>,
    /// Specular map.
    pub specular: Option<String>,
    /// Metallic (PBR).
    pub metallic: Option<String>,
    /// Roughness (PBR).
    pub roughness: Option<String>,
    /// Ambient occlusion.
    pub ao: Option<String>,
    /// Custom `HardwareShader` / material `shader` path.
    pub shader: Option<String>,
}

impl Default for MaterialCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialCatalog {
    /// Empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_name: BTreeMap::new(),
            textures: TextureManager::new(),
            playpal: None,
            pnames: None,
            texturex: BTreeMap::new(),
            tx_sizes: BTreeMap::new(),
            brightmaps: BTreeMap::new(),
            pbr_maps: BTreeMap::new(),
            sampler: None,
            anisotropy: 1,
            sampler_anisotropy: 1,
        }
    }

    /// Install GLDEFS brightmap / material brightmap bindings.
    pub fn set_brightmaps(&mut self, maps: BTreeMap<String, String>) {
        self.brightmaps = maps;
    }

    /// Install GLDEFS `material` normal/specular/PBR map names.
    pub fn set_material_maps(&mut self, maps: BTreeMap<String, MaterialMaps>) {
        self.pbr_maps = maps;
    }

    /// Brightmap lump for `name`, if any.
    #[must_use]
    pub fn brightmap_name(&self, name: &str) -> Option<&str> {
        self.brightmaps.get(&normalize(name)).map(String::as_str)
    }

    /// PBR / normal / specular maps for `name`, if any.
    #[must_use]
    pub fn material_maps(&self, name: &str) -> Option<&MaterialMaps> {
        self.pbr_maps.get(&normalize(name))
    }

    /// `true` if any normal/specular/PBR map is bound for `name`.
    #[must_use]
    pub fn has_pbr(&self, name: &str) -> bool {
        self.material_maps(name).is_some_and(|m| {
            m.normal.is_some()
                || m.specular.is_some()
                || m.metallic.is_some()
                || m.roughness.is_some()
                || m.ao.is_some()
        })
    }

    /// Custom material shader path for `name`, if any.
    #[must_use]
    pub fn shader_path(&self, name: &str) -> Option<&str> {
        self.material_maps(name).and_then(|m| m.shader.as_deref())
    }

    /// Ensure the shared **repeating** sampler exists. Nearest (Doom pixels)
    /// unless [`Self::anisotropy`] > 1: wgpu only allows anisotropy on fully
    /// linear samplers, so that mode trades the crisp look for filtering
    /// (GZDoom's trilinear + anisotropic option).
    pub fn ensure_sampler(&mut self, device: &wgpu::Device) {
        let aniso = sampler_anisotropy(self.anisotropy);
        if self.sampler.is_some() && self.sampler_anisotropy == aniso {
            return;
        }
        let filter = if aniso > 1 {
            wgpu::FilterMode::Linear
        } else {
            wgpu::FilterMode::Nearest
        };
        self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(if aniso > 1 {
                "mat-aniso-repeat"
            } else {
                "mat-nearest-repeat"
            }),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter: filter,
            anisotropy_clamp: aniso,
            ..Default::default()
        }));
        self.sampler_anisotropy = aniso;
    }

    /// Load PLAYPAL from the VFS if present.
    pub fn load_playpal(&mut self, vfs: &Vfs) {
        if let Some(lump) = vfs
            .find("PLAYPAL", Namespace::Global)
            .or_else(|| vfs.find_any_ns("PLAYPAL"))
        {
            if let Ok(data) = vfs.read(lump) {
                if let Ok(pal) = parse_playpal(&data) {
                    self.playpal = Some(pal);
                }
            }
        }
    }

    /// Load `PNAMES` + `TEXTURE1`/`TEXTURE2` for composed wall textures.
    pub fn load_texturex(&mut self, vfs: &Vfs) {
        if let Some(lump) = vfs
            .find("PNAMES", Namespace::Global)
            .or_else(|| vfs.find_any_ns("PNAMES"))
        {
            if let Ok(data) = vfs.read(lump) {
                if let Ok(p) = parse_pnames(&data) {
                    self.pnames = Some(p);
                }
            }
        }
        for name in ["TEXTURE1", "TEXTURE2"] {
            if let Some(lump) = vfs
                .find(name, Namespace::Global)
                .or_else(|| vfs.find_any_ns(name))
            {
                if let Ok(data) = vfs.read(lump) {
                    if let Ok(defs) = parse_texturex(&data) {
                        for d in defs {
                            self.texturex.insert(d.name.clone(), d);
                        }
                    }
                }
            }
        }
        self.tx_sizes.clear();
        for (_, info) in vfs.iter() {
            if info.namespace != Namespace::NewTextures {
                continue;
            }
            let name = normalize(info.short_name.as_str());
            if let Some(img) = load_tx_image(vfs, &name) {
                self.tx_sizes.insert(name, (img.width(), img.height()));
            }
        }
    }

    /// Lookup a previously uploaded texture.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<TextureId> {
        self.by_name.get(&normalize(name)).copied()
    }

    /// TEXTUREX width/height for wall UV (defaults to 128×128 like Doom's common tiles).
    #[must_use]
    pub fn wall_texel_size(&self, name: &str) -> (f32, f32) {
        let key = normalize(name);
        if let Some(&(w, h)) = self.tx_sizes.get(&key) {
            return (w.max(1) as f32, h.max(1) as f32);
        }
        self.texturex
            .get(&key)
            .map(|d| (f32::from(d.width.max(1)), f32::from(d.height.max(1))))
            .unwrap_or((128.0, 128.0))
    }

    /// Snapshot of wall sizes for scene UV (name → (w, h)); `TX_START`
    /// replacements override TEXTUREX.
    #[must_use]
    pub fn wall_texel_sizes(&self) -> BTreeMap<String, (f32, f32)> {
        let mut out: BTreeMap<String, (f32, f32)> = self
            .texturex
            .iter()
            .map(|(k, d)| {
                (
                    k.clone(),
                    (f32::from(d.width.max(1)), f32::from(d.height.max(1))),
                )
            })
            .collect();
        for (k, &(w, h)) in &self.tx_sizes {
            out.insert(k.clone(), (w.max(1) as f32, h.max(1) as f32));
        }
        out
    }

    /// Insert or refresh an RGBA8 texture by Doom name (camera canvases).
    pub fn upsert_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        name: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Option<TextureId> {
        let key = normalize(name);
        if key.is_empty() || key == "-" || width == 0 || height == 0 {
            return None;
        }
        let prev = self.by_name.get(&key).copied();
        let id = self
            .textures
            .upsert_rgba(device, queue, prev, width, height, rgba);
        self.by_name.insert(key, id);
        Some(id)
    }

    /// Copy a GPU texture onto `name` (camera canvases; no CPU readback).
    pub fn upsert_copy(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        name: &str,
        src: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Option<TextureId> {
        let key = normalize(name);
        if key.is_empty() || key == "-" || width == 0 || height == 0 {
            return None;
        }
        let prev = self.by_name.get(&key).copied();
        let id = self
            .textures
            .upsert_copy(device, queue, prev, src, width, height);
        self.by_name.insert(key, id);
        Some(id)
    }

    /// Upload `name` from VFS (textures or flats namespace). Returns existing id if cached.
    pub fn get_or_upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vfs: &Vfs,
        name: &str,
    ) -> Option<TextureId> {
        let key = normalize(name);
        if key.is_empty() || key == "-" {
            return None;
        }
        if let Some(id) = self.by_name.get(&key) {
            return Some(*id);
        }
        let image = self.source_image(vfs, &key)?;
        let id = self
            .textures
            .upload_image(device, queue, &image, self.playpal.as_ref());
        self.by_name.insert(key, id);
        Some(id)
    }

    /// CPU RGBA8 preview of a wall texture, flat, patch or sprite (editor
    /// thumbnails). Returns `(width, height, rgba)`.
    #[must_use]
    pub fn preview_rgba(&self, vfs: &Vfs, name: &str) -> Option<(u32, u32, Vec<u8>)> {
        let image = self.source_image(vfs, name)?;
        Some(image_to_rgba(&image, self.playpal.as_ref()))
    }

    /// Decoded picture a wall / flat name resolves to: a `TX_START` lump,
    /// else the TEXTUREX composite, else a patch / flat / sprite lump.
    #[must_use]
    pub fn source_image(&self, vfs: &Vfs, name: &str) -> Option<Image> {
        let key = normalize(name);
        if key.is_empty() || key == "-" {
            return None;
        }
        if let Some((sky, top)) = crate::sky::parse_sky_cap_name(&key) {
            return self.sky_cap_image(vfs, sky, top);
        }
        load_tx_image(vfs, &key)
            .or_else(|| self.compose_texturex(vfs, &key))
            .or_else(|| load_named_image(vfs, &key))
    }

    /// Sky cap gradient (see [`crate::sky::sky_cap_image`]).
    fn sky_cap_image(&self, vfs: &Vfs, sky: &str, top: bool) -> Option<Image> {
        let (w, h, rgba) = image_to_rgba(&self.source_image(vfs, sky)?, self.playpal.as_ref());
        let pixels = crate::sky::sky_cap_image(w, h, &rgba, top)?;
        Some(Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: false,
            pixels: ImagePixels::Rgba8(room_formats::image::RgbaImage {
                width: w,
                height: crate::sky::CAP_ROWS,
                pixels,
            }),
        })
    }

    /// Wall texture names (TEXTUREX composites + `TX_START` lumps, sorted).
    pub fn texture_names(&self) -> impl Iterator<Item = &str> {
        let mut names: Vec<&str> = self
            .texturex
            .keys()
            .chain(self.tx_sizes.keys())
            .map(String::as_str)
            .collect();
        names.sort_unstable();
        names.dedup();
        names.into_iter()
    }

    /// TEXTUREX composites that draw patch `patch` (uppercase names).
    #[must_use]
    pub fn textures_using_patch(&self, patch: &str) -> Vec<String> {
        let Some(pnames) = self.pnames.as_ref() else {
            return Vec::new();
        };
        let want = normalize(patch);
        self.texturex
            .values()
            .filter(|d| {
                d.patches.iter().any(|p| {
                    pnames
                        .names
                        .get(usize::from(p.pnames_index))
                        .is_some_and(|n| normalize(n) == want)
                })
            })
            .map(|d| d.name.clone())
            .collect()
    }

    /// Loaded `PLAYPAL` (for raw lump previews).
    #[must_use]
    pub fn playpal(&self) -> Option<&Playpal> {
        self.playpal.as_ref()
    }

    fn compose_texturex(&self, vfs: &Vfs, name: &str) -> Option<Image> {
        let def = self.texturex.get(name)?;
        let w = u32::try_from(def.width.max(0)).ok()?.max(1);
        let h = u32::try_from(def.height.max(0)).ok()?.max(1);
        // Transparent clear: only texels covered by an opaque patch become solid.
        // Solid walls/flats fill remaining holes black in the shader; masked mids
        // discard them. Never "punch" holes — that erased earlier patches and
        // turned column gaps into false see-through on solid-looking mids.
        let mut indices = vec![0u8; (w * h) as usize];
        let mut alpha = vec![0u8; (w * h) as usize];
        if !def.patches.is_empty() {
            let pnames = self.pnames.as_ref()?;
            for patch in &def.patches {
                let pname = pnames.names.get(usize::from(patch.pnames_index))?;
                let img = load_named_image(vfs, pname)?;
                blit_paletted(
                    &mut indices,
                    &mut alpha,
                    w,
                    h,
                    &img,
                    patch.origin_x,
                    patch.origin_y,
                );
            }
        }
        Some(Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: alpha.iter().any(|&a| a < 255),
            pixels: ImagePixels::Paletted(PalettedImage {
                width: w,
                height: h,
                indices,
                alpha,
            }),
        })
    }
}

fn blit_paletted(
    dest: &mut [u8],
    alpha: &mut [u8],
    dw: u32,
    dh: u32,
    img: &Image,
    ox: i16,
    oy: i16,
) {
    let ImagePixels::Paletted(src) = &img.pixels else {
        return;
    };
    let explicit_alpha = !src.alpha.is_empty();
    // Legacy patches with empty alpha + `has_translucency` treat index 0 as a hole.
    let idx0_hole = img.has_translucency && !explicit_alpha;
    for sy in 0..src.height {
        for sx in 0..src.width {
            let dx = i32::from(ox) + sx as i32;
            let dy = i32::from(oy) + sy as i32;
            if dx < 0 || dy < 0 {
                continue;
            }
            let dx = dx as u32;
            let dy = dy as u32;
            if dx >= dw || dy >= dh {
                continue;
            }
            let si = (sy * src.width + sx) as usize;
            let idx = src.indices.get(si).copied().unwrap_or(0);
            let di = (dy * dw + dx) as usize;
            let src_a = src.alpha.get(si).copied().unwrap_or(255);
            let hole = if explicit_alpha {
                src_a == 0
            } else {
                idx0_hole && idx == 0
            };
            if hole {
                // Leave destination unchanged (multi-patch underlay stays).
                continue;
            }
            dest[di] = idx;
            alpha[di] = if explicit_alpha { src_a } else { 255 };
        }
    }
}

fn normalize(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::VfsBuilder;

    #[test]
    fn texturex_empty_stays_transparent_clear() {
        let mut cat = MaterialCatalog::new();
        cat.texturex.insert(
            "HOLEY".into(),
            TexturexDef {
                name: "HOLEY".into(),
                width: 8,
                height: 8,
                flags: 0,
                scale_x: 0,
                scale_y: 0,
                patches: vec![],
            },
        );
        let vfs = VfsBuilder::new().build().unwrap();
        // Empty patch list → transparent clear (solid walls fill black in shader).
        let img = cat.compose_texturex(&vfs, "HOLEY").unwrap();
        let ImagePixels::Paletted(pal) = &img.pixels else {
            panic!("expected paletted");
        };
        assert!(pal.alpha.iter().all(|&a| a == 0));
        assert!(img.has_translucency);
    }

    #[test]
    fn tx_start_lump_replaces_texturex_composite() {
        let tx = room_formats::encode_patch(4, 2, 0, 0, &[7; 8], &[]).unwrap();
        let mut w = room_vfs::WadWriter::pwad();
        w.marker("TX_START").add("WALLA", tx).marker("TX_END");
        let vfs = VfsBuilder::new()
            .mount_memory("tx.wad", w.finish().unwrap())
            .unwrap()
            .build()
            .unwrap();
        let mut cat = MaterialCatalog::new();
        cat.texturex.insert(
            "WALLA".into(),
            TexturexDef {
                name: "WALLA".into(),
                width: 64,
                height: 128,
                flags: 0,
                scale_x: 0,
                scale_y: 0,
                patches: vec![],
            },
        );
        cat.load_texturex(&vfs);
        let img = cat.source_image(&vfs, "walla").unwrap();
        assert_eq!((img.width(), img.height()), (4, 2));
        assert_eq!(cat.wall_texel_size("WALLA"), (4.0, 2.0));
        assert_eq!(cat.wall_texel_sizes().get("WALLA"), Some(&(4.0, 2.0)));
        assert_eq!(cat.texture_names().collect::<Vec<_>>(), vec!["WALLA"]);
    }

    #[test]
    fn texturex_patch_holes_leave_clear_transparent() {
        // 2×2 patch: only (0,0) opaque index 1; rest index 0 holes.
        let patch = Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: true,
            pixels: ImagePixels::Paletted(PalettedImage {
                width: 2,
                height: 2,
                indices: vec![1, 0, 0, 0],
                alpha: Vec::new(),
            }),
        };
        let mut indices = vec![0u8; 4];
        let mut alpha = vec![0u8; 4];
        blit_paletted(&mut indices, &mut alpha, 2, 2, &patch, 0, 0);
        assert_eq!(indices[0], 1);
        assert_eq!(alpha[0], 255);
        assert_eq!(alpha[1], 0);
        assert_eq!(alpha[2], 0);
        assert_eq!(alpha[3], 0);
    }

    #[test]
    fn doom1_solid_wall_textures_mostly_opaque() {
        use std::path::PathBuf;
        let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/doom/doom1.wad");
        if !wad.is_file() {
            return;
        }
        let vfs = VfsBuilder::new().mount_path(&wad).unwrap().build().unwrap();
        let mut cat = MaterialCatalog::new();
        cat.load_playpal(&vfs);
        cat.load_texturex(&vfs);
        // Common one-sided / upper / lower faces — should be nearly fully covered.
        for name in ["STARTAN3", "BIGDOOR2", "BROWN1", "COMPTILE", "DOOR3"] {
            let Some(img) = cat.compose_texturex(&vfs, name) else {
                panic!("compose failed for {name}");
            };
            let ImagePixels::Paletted(pal) = &img.pixels else {
                panic!("{name}: expected paletted");
            };
            let holes = pal.alpha.iter().filter(|&&a| a < 255).count();
            let total = pal.alpha.len().max(1);
            let hole_pct = 100.0 * holes as f64 / total as f64;
            assert!(
                hole_pct < 5.0,
                "{name}: {hole_pct:.1}% holes ({holes}/{total}) — false transparency"
            );
        }
    }

    #[test]
    fn texturex_blit_skips_holes_preserves_underlay() {
        let under = Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: false,
            pixels: ImagePixels::Paletted(PalettedImage {
                width: 2,
                height: 2,
                indices: vec![9, 9, 9, 9],
                alpha: vec![255, 255, 255, 255],
            }),
        };
        let holey = Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: true,
            pixels: ImagePixels::Paletted(PalettedImage {
                width: 2,
                height: 2,
                indices: vec![3, 0, 0, 0],
                alpha: Vec::new(),
            }),
        };
        let mut indices = vec![0u8; 4];
        let mut alpha = vec![0u8; 4];
        blit_paletted(&mut indices, &mut alpha, 2, 2, &under, 0, 0);
        blit_paletted(&mut indices, &mut alpha, 2, 2, &holey, 0, 0);
        assert_eq!(indices[0], 3);
        assert_eq!(alpha[0], 255);
        // Hole pixels must not erase the underlay.
        assert_eq!(indices[1], 9);
        assert_eq!(alpha[1], 255);
        assert_eq!(indices[2], 9);
        assert_eq!(alpha[2], 255);
        assert_eq!(indices[3], 9);
        assert_eq!(alpha[3], 255);
    }
}

/// Decode any picture lump (patch / PNG / raw flat) to RGBA8 `(w, h, rgba)`.
#[must_use]
pub fn lump_preview_rgba(data: &[u8], playpal: Option<&Playpal>) -> Option<(u32, u32, Vec<u8>)> {
    let image = decode_image(data).ok().or_else(|| {
        let dims = flat_dimensions(data.len())?;
        Some(Image {
            left_offset: 0,
            top_offset: 0,
            has_translucency: false,
            pixels: room_formats::ImagePixels::Paletted(room_formats::PalettedImage {
                width: dims.width,
                height: dims.height,
                indices: data.to_vec(),
                alpha: Vec::new(),
            }),
        })
    })?;
    Some(image_to_rgba(&image, playpal))
}

fn image_to_rgba(image: &Image, playpal: Option<&Playpal>) -> (u32, u32, Vec<u8>) {
    match &image.pixels {
        ImagePixels::Rgba8(rgba) => (rgba.width, rgba.height, rgba.pixels.clone()),
        ImagePixels::Paletted(p) => {
            let pal = playpal.and_then(|pp| pp.palettes.first());
            let mut out = Vec::with_capacity(p.indices.len() * 4);
            let hole0 = image.has_translucency && p.alpha.is_empty();
            for (i, &idx) in p.indices.iter().enumerate() {
                let (r, g, b) = match pal {
                    Some(pal) => {
                        let o = usize::from(idx) * 3;
                        (pal.rgb[o], pal.rgb[o + 1], pal.rgb[o + 2])
                    }
                    None => (idx, idx, idx),
                };
                let a = match p.alpha.get(i) {
                    Some(&a) => a,
                    None if hole0 && idx == 0 => 0,
                    None => 255,
                };
                out.extend_from_slice(&[r, g, b, a]);
            }
            (p.width, p.height, out)
        }
    }
}

fn load_tx_image(vfs: &Vfs, name: &str) -> Option<Image> {
    let lump = vfs.find(name, Namespace::NewTextures)?;
    decode_image(&vfs.read(lump).ok()?).ok()
}

fn load_named_image(vfs: &Vfs, name: &str) -> Option<Image> {
    // Prefer sprites / TX_ / patches / flats, then global.
    let lump = vfs
        .find(name, Namespace::Sprites)
        .or_else(|| vfs.find(name, Namespace::NewTextures))
        .or_else(|| vfs.find(name, Namespace::Patches))
        .or_else(|| vfs.find(name, Namespace::Flats))
        .or_else(|| vfs.find(name, Namespace::Global))
        .or_else(|| vfs.find_any_ns(name))?;
    let data = vfs.read(lump).ok()?;
    if let Ok(img) = decode_image(&data) {
        return Some(img);
    }
    // Raw flat: 64×64 (or power-of-two square) indices.
    let dims = flat_dimensions(data.len())?;
    Some(Image {
        left_offset: 0,
        top_offset: 0,
        has_translucency: false,
        pixels: room_formats::ImagePixels::Paletted(room_formats::PalettedImage {
            width: dims.width,
            height: dims.height,
            indices: data.to_vec(),
            alpha: Vec::new(),
        }),
    })
}

/// Clamp a requested anisotropy to wgpu's legal values (1, 2, 4, 8, 16).
#[must_use]
pub fn sampler_anisotropy(requested: u16) -> u16 {
    let v = requested.clamp(1, 16);
    1 << (15 - v.leading_zeros())
}

#[cfg(test)]
mod anisotropy_tests {
    use super::sampler_anisotropy;

    #[test]
    fn anisotropy_rounds_down_to_a_power_of_two() {
        assert_eq!(sampler_anisotropy(0), 1);
        assert_eq!(sampler_anisotropy(1), 1);
        assert_eq!(sampler_anisotropy(3), 2);
        assert_eq!(sampler_anisotropy(8), 8);
        assert_eq!(sampler_anisotropy(15), 8);
        assert_eq!(sampler_anisotropy(64), 16);
    }
}

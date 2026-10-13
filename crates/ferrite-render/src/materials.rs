//! Name → GPU texture catalog (walls/flats from VFS + TEXTUREX).

use crate::texture::TextureManager;
use ferrite_core::TextureId;
use ferrite_formats::{
    Image, ImagePixels, PalettedImage, Playpal, Pnames, TexturexDef, decode_image, flat_dimensions,
    parse_playpal, parse_pnames, parse_texturex,
};
use ferrite_vfs::{Namespace, Vfs};
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
    /// Base texture (uppercase) → brightmap lump name (GLDEFS).
    brightmaps: BTreeMap<String, String>,
    /// Base texture → normal / specular / PBR map lump names (GLDEFS `material`).
    pbr_maps: BTreeMap<String, MaterialMaps>,
    /// Nearest sampler shared by all.
    pub sampler: Option<wgpu::Sampler>,
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
            brightmaps: BTreeMap::new(),
            pbr_maps: BTreeMap::new(),
            sampler: None,
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

    /// Ensure a nearest **repeating** sampler exists (Doom walls/flats tile in UV).
    pub fn ensure_sampler(&mut self, device: &wgpu::Device) {
        if self.sampler.is_none() {
            self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("mat-nearest-repeat"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }));
        }
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
    }

    /// Lookup a previously uploaded texture.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<TextureId> {
        self.by_name.get(&normalize(name)).copied()
    }

    /// TEXTUREX width/height for wall UV (defaults to 128×128 like Doom's common tiles).
    #[must_use]
    pub fn wall_texel_size(&self, name: &str) -> (f32, f32) {
        self.texturex
            .get(&normalize(name))
            .map(|d| {
                (
                    f32::from(d.width.max(1)),
                    f32::from(d.height.max(1)),
                )
            })
            .unwrap_or((128.0, 128.0))
    }

    /// Snapshot of TEXTUREX sizes for scene UV (name → (w, h)).
    #[must_use]
    pub fn wall_texel_sizes(&self) -> BTreeMap<String, (f32, f32)> {
        self.texturex
            .iter()
            .map(|(k, d)| {
                (
                    k.clone(),
                    (f32::from(d.width.max(1)), f32::from(d.height.max(1))),
                )
            })
            .collect()
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
        let image = self
            .compose_texturex(vfs, &key)
            .or_else(|| load_named_image(vfs, &key))?;
        let id = self
            .textures
            .upload_image(device, queue, &image, self.playpal.as_ref());
        self.by_name.insert(key, id);
        Some(id)
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
    use ferrite_vfs::VfsBuilder;

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
        let wad = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/doom/doom1.wad");
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
        pixels: ferrite_formats::ImagePixels::Paletted(ferrite_formats::PalettedImage {
            width: dims.width,
            height: dims.height,
            indices: data.to_vec(),
            alpha: Vec::new(),
        }),
    })
}


//! EDuke32 DEF resource declarations.
//!
//! This module deliberately parses declarations into a small syntax tree and
//! leaves archive lookup and renderer policy to the caller. It supports nested
//! definition blocks, quoted paths, line and block comments, and the tile
//! replacement directives used by Duke mods.

use crate::error::BuildError;
use crate::palette::Palette;
use room_formats::image::ImagePixels;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

const MAX_HIGH_RES_TILE_PIXELS: usize = 16 * 1024 * 1024;
const MAX_DEF_SKYBOX_PIXELS: usize = 256 * 1024 * 1024;
const MAX_DUMMY_RANGE_PIXELS: usize = 64 * 1024 * 1024;
const MAX_DEF_MUSIC_TRACKS: usize = 4096;
const MAX_DEF_BASE_PALETTES: usize = 256;
const MAX_DEF_BLEND_TABLES: usize = 256;
const MAX_DEF_SKYBOXES: usize = 4096;
const MAX_DEF_INCLUDES: usize = 4096;
const MAX_DEF_VOXELS: usize = 65_536;
const MAX_DEF_TINTS: usize = 256;
const MAX_CACHED_PALETTE_COLORS: usize = 65_536;

fn nearest_palette_index(palette: &Palette, rgb: [u8; 3], cache: &mut HashMap<[u8; 3], u8>) -> u8 {
    if let Some(&index) = cache.get(&rgb) {
        return index;
    }
    let index = palette.nearest(rgb.map(u32::from));
    if cache.len() < MAX_CACHED_PALETTE_COLORS {
        cache.insert(rgb, index);
    }
    index
}

/// A DEF directive and its nested directives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefDirective {
    /// Directive keyword, normalized to lowercase.
    pub keyword: String,
    /// Positional and inline option tokens.
    pub args: Vec<String>,
    /// Nested directives between this directive's braces.
    pub children: Vec<Self>,
    /// One-based source line.
    pub line: usize,
}

/// Parsed DEF file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DefDocument {
    /// Top-level declarations in source order.
    pub directives: Vec<DefDirective>,
}

/// A map-specific music resource declared with EDuke32's `Music` DEF block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefMusicTrack {
    /// Map or music identifier from the `ID` child.
    pub id: String,
    /// Music resource path from the `file` child.
    pub file: String,
    /// One-based source line of the `Music` block.
    pub line: usize,
}

/// A DEF `include` directive. The caller resolves the path against its mounted
/// resource stack so filesystem and archive policy stay outside the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefInclude {
    /// Included DEF path as written, with its original separators preserved.
    pub file: String,
    /// One-based source line.
    pub line: usize,
}

/// A voxel model assignment from a DEF `voxel` block.
#[derive(Debug, Clone, PartialEq)]
pub struct DefVoxel {
    /// KVX resource path as written.
    pub file: String,
    /// Zero-based voxel-model index in this DEF document's source order.
    /// Multiple tile mappings from one `voxel` block share the same index.
    pub model_index: usize,
    /// First ART tile replaced by this voxel.
    pub tile_start: u16,
    /// Last ART tile replaced by this voxel (inclusive).
    pub tile_end: u16,
    /// Model scale, defaulting to 1.
    pub scale: f32,
    /// BuildGDX-compatible rotating voxel extension flag.
    pub rotate: bool,
    /// One-based source line of the voxel declaration.
    pub line: usize,
}

/// A palette tint declared by DEF `tint { ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefTint {
    /// Palette number selected by the tint.
    pub palette: u8,
    /// Multiply/colorize RGB tint channels.
    pub rgb: [u8; 3],
    /// Optional RGB shade channels.
    pub shade_rgb: Option<[u8; 3]>,
    /// EDuke32 `HICTINT_*` flags.
    pub flags: u16,
    /// One-based source line.
    pub line: usize,
}

/// A high-resolution tile file referenced by `tilefromtexture` or by a
/// palette branch of a `texture` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighResTileSpec {
    /// Build tile number replaced by this image.
    pub tile: u16,
    /// Palette-specific override from a DEF `texture` `pal` block.
    /// `None` is the default tile image; `Some(0)` is normalized to `None`.
    pub palette: Option<u8>,
    /// Image path relative to the DEF source file.
    pub file: String,
    /// Horizontal tile offset (`xoff`), defaulting to zero.
    pub x_offset: i32,
    /// Vertical tile offset (`yoff`), defaulting to zero.
    pub y_offset: i32,
    /// Minimum image alpha converted to an opaque palette pixel.
    pub opaque_alpha_threshold: u8,
    /// Source line of the declaration.
    pub line: usize,
}

/// One palette-specific image branch in a DEF `texture` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefTextureVariant {
    /// Build tile number replaced by this image.
    pub tile: u16,
    /// Palette bank selected by the `pal` block.
    pub palette: u8,
    /// Image path relative to the DEF source file.
    pub file: String,
    /// Minimum image alpha converted to an opaque palette pixel.
    pub opaque_alpha_threshold: u8,
    /// Source line of the containing texture declaration.
    pub line: usize,
}

/// Six image resources for one tile/palette DEF skybox override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefSkybox {
    /// Parallax tile overridden by this skybox.
    pub tile: u16,
    /// Palette bank selected by the declaration.
    pub palette: u8,
    /// Face paths in EDuke32 order: front, right, back, left, top, down.
    pub faces: [String; 6],
    /// One-based source line of the skybox block.
    pub line: usize,
}

/// Palette-mapped image face imported from a DEF skybox resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSkyboxFace {
    /// Native image width.
    pub width: u16,
    /// Native image height.
    pub height: u16,
    /// Column-major Build palette indices (`255` is transparent).
    pub pixels: Arc<[u8]>,
    /// Column-major source colors as `0xFFRRGGBB` (`0` where transparent),
    /// kept for true-colour rendering of RGB images; `None` for paletted ones.
    pub true_color: Option<Arc<[u32]>>,
}

/// Pack an opaque source color for true-colour rendering (`0` stays "none").
#[must_use]
pub fn pack_true_color([r, g, b]: [u8; 3]) -> u32 {
    0xFF00_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}

/// Six decoded faces ready for a renderer's skybox cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSkybox {
    /// Parallax tile overridden by this skybox.
    pub tile: i16,
    /// Palette bank selected by the declaration.
    pub palette: u8,
    /// Faces in EDuke32 order: front, right, back, left, top, down.
    pub faces: [ImportedSkyboxFace; 6],
}

/// Ordered full-screen base-palette operation declared by DEF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasePaletteOperation {
    /// Load 256 RGB colors from a raw palette resource.
    Load {
        /// Base palette identifier.
        palette: u8,
        /// Resource path relative to the DEF source file.
        file: String,
        /// Byte offset of the RGB data in the resource.
        offset: usize,
        /// Left shift applied to each source color channel.
        shift_left: u8,
        /// One-based source line.
        line: usize,
    },
    /// Copy an already loaded base palette.
    Copy {
        /// Destination base palette identifier.
        palette: u8,
        /// Source base palette identifier.
        source: u8,
        /// One-based source line.
        line: usize,
    },
    /// Remove a previously loaded base palette override.
    Undef {
        /// Base palette identifier.
        palette: u8,
        /// One-based source line.
        line: usize,
    },
}

/// Ordered classic blend-table operation declared by DEF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlendTableOperation {
    /// Load one 256×256 indexed table from a raw resource.
    Load {
        /// Destination blend-table identifier (0 is reserved by the game).
        table: u8,
        /// Resource path relative to the DEF source file.
        file: String,
        /// Byte offset of the 65,536-byte table.
        offset: usize,
        /// One-based source line.
        line: usize,
    },
    /// Copy an already loaded blend table.
    Copy {
        /// Destination blend-table identifier.
        table: u8,
        /// Source blend-table identifier.
        source: u8,
        /// One-based source line.
        line: usize,
    },
    /// Remove a previously loaded blend table override.
    Undef {
        /// Blend-table identifier.
        table: u8,
        /// One-based source line.
        line: usize,
    },
}

/// A decoded high-resolution replacement ready to insert into the indexed
/// Build tile set. Pixels use Build's column-major layout; palette entries
/// are selected from the supplied GRP palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedTile {
    /// Build tile number being replaced.
    pub tile: i16,
    /// Palette bank for a DEF `texture` variant; `None` replaces the base tile.
    pub palette: Option<u8>,
    /// Native image width.
    pub width: u16,
    /// Native image height.
    pub height: u16,
    /// Column-major palette indices (`255` is transparent).
    pub pixels: Vec<u8>,
    /// Column-major source colors as `0xFFRRGGBB` (`0` where transparent),
    /// kept for true-colour rendering of RGB images; `None` for paletted ones.
    pub true_color: Option<Vec<u32>>,
    /// Horizontal offset from the DEF declaration.
    pub x_offset: i8,
    /// Vertical offset from the DEF declaration.
    pub y_offset: i8,
}

/// Ordered tile-set action declared in a DEF file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileOperation {
    /// Copy an existing tile into a new tile number (`copytile <dest> { tile <src> }`).
    Copy {
        /// Destination tile number.
        destination: i16,
        /// Existing tile whose data is copied.
        source: i16,
    },
    /// Reserve a transparent tile with explicit dimensions.
    Dummy {
        /// Tile number to reserve.
        tile: i16,
        /// Reserved tile width in pixels.
        width: u16,
        /// Reserved tile height in pixels.
        height: u16,
    },
    /// Reserve an inclusive range of transparent tiles with one size.
    DummyRange {
        /// First tile in the reserved range.
        start: i16,
        /// Last tile in the reserved range.
        end: i16,
        /// Reserved tile width in pixels.
        width: u16,
        /// Reserved tile height in pixels.
        height: u16,
    },
    /// Apply ART-style animation metadata to a tile range.
    AnimationRange {
        /// First tile in the range (the animation base tile).
        start: i16,
        /// Final tile in the range.
        end: i16,
        /// Frame speed from 0 (fastest) through 15.
        speed: u8,
        /// Animation mode: 0 none, 1 oscillate, 2 forward, or 3 backward.
        animation: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Word(String),
    Open,
    Close,
    Newline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    line: usize,
}

impl DefDocument {
    /// Parse a DEF source file.
    ///
    /// # Errors
    ///
    /// Returns an error for unterminated strings/comments, unmatched braces,
    /// or a directive without a keyword.
    pub fn parse(source: &str) -> Result<Self, BuildError> {
        let tokens = tokenize(source)?;
        let mut cursor = 0;
        let directives = parse_block(&tokens, &mut cursor, false)?;
        if cursor != tokens.len() {
            return Err(BuildError::Invalid("DEF trailing tokens"));
        }
        Ok(Self { directives })
    }

    /// Extract image replacements from `tilefromtexture` and every `texture`
    /// palette branch. A palette-zero texture replaces the default tile;
    /// nonzero palettes remain per-palette overrides.
    ///
    /// The parser preserves image dimensions and pixels by leaving decoding to
    /// `room-formats`; the returned offsets are applied by the renderer.
    ///
    /// # Errors
    ///
    /// Returns an error when a declaration has an invalid tile number or a
    /// missing image path / malformed offset.
    pub fn high_res_tiles(&self) -> Result<Vec<HighResTileSpec>, BuildError> {
        let mut out = Vec::new();
        collect_tile_specs(&self.directives, &mut out)?;
        Ok(out)
    }

    /// Extract palette-specific file branches from DEF `texture` blocks.
    ///
    /// This describes resources without imposing a renderer's palette or
    /// true-colour policy.
    pub fn texture_variants(&self) -> Result<Vec<DefTextureVariant>, BuildError> {
        let mut variants = Vec::new();
        collect_texture_variants(&self.directives, &mut variants)?;
        Ok(variants)
    }

    /// Extract `basepalette` raw loads, copies, and undefinitions in source order.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid palette identifiers, malformed resource
    /// fields, or a palette declaration without exactly one operation.
    pub fn base_palette_operations(&self) -> Result<Vec<BasePaletteOperation>, BuildError> {
        let mut operations = Vec::new();
        collect_base_palette_operations(&self.directives, &mut operations)?;
        Ok(operations)
    }

    /// Extract classic `blendtable` raw loads, copies and undefinitions in
    /// source order. OpenGL `glblend` parameters are retained by the syntax
    /// tree but are not part of the indexed software renderer.
    pub fn blend_table_operations(&self) -> Result<Vec<BlendTableOperation>, BuildError> {
        let mut operations = Vec::new();
        collect_blend_table_operations(&self.directives, &mut operations)?;
        Ok(operations)
    }

    /// Decode and palette-map all `tilefromtexture` and `texture` assets in
    /// this document. Later declarations for one tile/palette pair replace
    /// earlier ones.
    ///
    /// # Errors
    ///
    /// Returns an error if a referenced asset is absent, malformed, too large
    /// for Build tile dimensions, or has an offset outside the Build byte range.
    pub fn import_high_res_tiles(
        &self,
        assets: &[(&str, &[u8])],
        palette: &Palette,
    ) -> Result<Vec<ImportedTile>, BuildError> {
        let assets: BTreeMap<String, &[u8]> = assets
            .iter()
            .map(|(name, bytes)| (normalize_asset_name(name), *bytes))
            .collect();
        let mut imported = BTreeMap::new();
        let mut palette_cache = HashMap::new();
        for spec in self.high_res_tiles()? {
            let key = normalize_asset_name(&spec.file);
            let bytes = assets
                .get(&key)
                .ok_or_else(|| BuildError::Missing(spec.file.clone()))?;
            let image = decode_tile_image(bytes)
                .map_err(|error| BuildError::Resource(format!("{}: {error}", spec.file)))?;
            let width = u16::try_from(image.width())
                .map_err(|_| BuildError::Invalid("DEF tile image width"))?;
            let height = u16::try_from(image.height())
                .map_err(|_| BuildError::Invalid("DEF tile image height"))?;
            let x_offset = i8::try_from(spec.x_offset)
                .map_err(|_| BuildError::Invalid("DEF tile xoff range"))?;
            let y_offset = i8::try_from(spec.y_offset)
                .map_err(|_| BuildError::Invalid("DEF tile yoff range"))?;
            let count = usize::from(width)
                .checked_mul(usize::from(height))
                .ok_or(BuildError::Invalid("DEF tile image size"))?;
            if count > MAX_HIGH_RES_TILE_PIXELS {
                return Err(BuildError::Invalid("DEF tile image size"));
            }
            let mut column_major = vec![255; count];
            let mut true_color = None;
            match image.pixels {
                ImagePixels::Paletted(pixels) => {
                    if pixels.indices.len() != count
                        || (!pixels.alpha.is_empty() && pixels.alpha.len() != count)
                    {
                        return Err(BuildError::Invalid("DEF tile image pixel count"));
                    }
                    for y in 0..usize::from(height) {
                        for x in 0..usize::from(width) {
                            let source = y * usize::from(width) + x;
                            let destination = x * usize::from(height) + y;
                            column_major[destination] = if !pixels.alpha.is_empty()
                                && pixels.alpha[source] < spec.opaque_alpha_threshold
                            {
                                255
                            } else {
                                pixels.indices[source]
                            };
                        }
                    }
                }
                ImagePixels::Rgba8(pixels) => {
                    let rgba_len = count
                        .checked_mul(4)
                        .ok_or(BuildError::Invalid("DEF tile image size"))?;
                    if pixels.pixels.len() != rgba_len {
                        return Err(BuildError::Invalid("DEF tile image pixel count"));
                    }
                    let mut colors = vec![0_u32; count];
                    for y in 0..usize::from(height) {
                        for x in 0..usize::from(width) {
                            let source = (y * usize::from(width) + x) * 4;
                            let destination = x * usize::from(height) + y;
                            let rgb = [
                                pixels.pixels[source],
                                pixels.pixels[source + 1],
                                pixels.pixels[source + 2],
                            ];
                            column_major[destination] =
                                if pixels.pixels[source + 3] < spec.opaque_alpha_threshold {
                                    255
                                } else {
                                    colors[destination] = pack_true_color(rgb);
                                    nearest_palette_index(palette, rgb, &mut palette_cache)
                                };
                        }
                    }
                    true_color = Some(colors);
                }
            }
            let tile_id =
                i16::try_from(spec.tile).map_err(|_| BuildError::Invalid("DEF tile number"))?;
            imported.insert(
                (tile_id, spec.palette),
                ImportedTile {
                    tile: tile_id,
                    palette: spec.palette,
                    width,
                    height,
                    pixels: column_major,
                    true_color,
                    x_offset,
                    y_offset,
                },
            );
        }
        Ok(imported.into_values().collect())
    }

    /// Decode every DEF skybox face and map it through the supplied Build
    /// palette. Later declarations for a tile/palette pair replace earlier
    /// ones.
    ///
    /// # Errors
    ///
    /// Returns an error if a face is missing, malformed, too large, or the
    /// document exceeds the bounded aggregate face-pixel budget.
    pub fn import_skyboxes(
        &self,
        assets: &[(&str, &[u8])],
        palette: &Palette,
    ) -> Result<Vec<ImportedSkybox>, BuildError> {
        let assets: BTreeMap<String, &[u8]> = assets
            .iter()
            .map(|(name, bytes)| (normalize_asset_name(name), *bytes))
            .collect();
        let mut imported = BTreeMap::new();
        let mut decoded_faces = BTreeMap::<String, ImportedSkyboxFace>::new();
        let mut palette_cache = HashMap::new();
        let mut total_pixels = 0usize;
        for skybox in self.skyboxes()? {
            let mut faces = Vec::with_capacity(skybox.faces.len());
            for file in &skybox.faces {
                let key = normalize_asset_name(file);
                if let Some(face) = decoded_faces.get(&key) {
                    faces.push(face.clone());
                    continue;
                }
                let bytes = assets
                    .get(&key)
                    .ok_or_else(|| BuildError::Missing(file.clone()))?;
                let image = decode_tile_image(bytes)
                    .map_err(|error| BuildError::Resource(format!("{file}: {error}")))?;
                let width = u16::try_from(image.width())
                    .map_err(|_| BuildError::Invalid("DEF skybox image width"))?;
                let height = u16::try_from(image.height())
                    .map_err(|_| BuildError::Invalid("DEF skybox image height"))?;
                let count = usize::from(width)
                    .checked_mul(usize::from(height))
                    .ok_or(BuildError::Invalid("DEF skybox image size"))?;
                if width == 0 || height == 0 || count > MAX_HIGH_RES_TILE_PIXELS {
                    return Err(BuildError::Invalid("DEF skybox image size"));
                }
                total_pixels = total_pixels
                    .checked_add(count)
                    .filter(|total| *total <= MAX_DEF_SKYBOX_PIXELS)
                    .ok_or(BuildError::Invalid("DEF skybox total image size"))?;
                let mut column_major = vec![255; count];
                let mut true_color = None;
                match image.pixels {
                    ImagePixels::Paletted(pixels) => {
                        if pixels.indices.len() != count
                            || (!pixels.alpha.is_empty() && pixels.alpha.len() != count)
                        {
                            return Err(BuildError::Invalid("DEF skybox image pixel count"));
                        }
                        for y in 0..usize::from(height) {
                            for x in 0..usize::from(width) {
                                let source = y * usize::from(width) + x;
                                let destination = x * usize::from(height) + y;
                                column_major[destination] =
                                    if !pixels.alpha.is_empty() && pixels.alpha[source] < 174 {
                                        255
                                    } else {
                                        pixels.indices[source]
                                    };
                            }
                        }
                    }
                    ImagePixels::Rgba8(pixels) => {
                        let rgba_len = count
                            .checked_mul(4)
                            .ok_or(BuildError::Invalid("DEF skybox image size"))?;
                        if pixels.pixels.len() != rgba_len {
                            return Err(BuildError::Invalid("DEF skybox image pixel count"));
                        }
                        let mut colors = vec![0_u32; count];
                        for y in 0..usize::from(height) {
                            for x in 0..usize::from(width) {
                                let source = (y * usize::from(width) + x) * 4;
                                let destination = x * usize::from(height) + y;
                                let rgb = [
                                    pixels.pixels[source],
                                    pixels.pixels[source + 1],
                                    pixels.pixels[source + 2],
                                ];
                                column_major[destination] = if pixels.pixels[source + 3] < 174 {
                                    255
                                } else {
                                    colors[destination] = pack_true_color(rgb);
                                    nearest_palette_index(palette, rgb, &mut palette_cache)
                                };
                            }
                        }
                        true_color = Some(Arc::from(colors));
                    }
                }
                let face = ImportedSkyboxFace {
                    width,
                    height,
                    pixels: Arc::from(column_major),
                    true_color,
                };
                decoded_faces.insert(key, face.clone());
                faces.push(face);
            }
            let faces: [ImportedSkyboxFace; 6] = faces
                .try_into()
                .map_err(|_| BuildError::Invalid("DEF skybox face count"))?;
            let tile =
                i16::try_from(skybox.tile).map_err(|_| BuildError::Invalid("DEF skybox tile"))?;
            imported.insert(
                (tile, skybox.palette),
                ImportedSkybox {
                    tile,
                    palette: skybox.palette,
                    faces,
                },
            );
        }
        Ok(imported.into_values().collect())
    }

    /// Extract `copytile`, `dummytile` and `dummytilerange` actions in source
    /// order so a tile-set owner can apply them against its current ART data.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid tile numbers or dimensions.
    pub fn tile_operations(&self) -> Result<Vec<TileOperation>, BuildError> {
        let mut operations = Vec::new();
        collect_tile_operations(&self.directives, &mut operations)?;
        Ok(operations)
    }

    /// Extract map-specific `Music { ID ... file ... }` blocks.
    ///
    /// Identifiers are preserved; callers should compare them without regard
    /// to ASCII case. Filesystem or archive lookup remains the caller's job.
    ///
    /// # Errors
    ///
    /// Returns an error if a music block has duplicate/malformed `ID` or
    /// `file` children, or if the document declares too many music blocks.
    pub fn music_tracks(&self) -> Result<Vec<DefMusicTrack>, BuildError> {
        let mut out = Vec::new();
        collect_music_tracks(&self.directives, &mut out)?;
        Ok(out)
    }

    /// Extract complete six-face `skybox` resources in source order.
    ///
    /// The returned file paths are relative to the DEF source; loading and
    /// renderer selection remain the caller's responsibility.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid tile/palette values, repeated fields,
    /// missing face files, or too many skybox blocks.
    pub fn skyboxes(&self) -> Result<Vec<DefSkybox>, BuildError> {
        let mut out = Vec::new();
        collect_skyboxes(&self.directives, &mut out)?;
        Ok(out)
    }

    /// Extract `include` and `#include` paths in source order.
    ///
    /// Includes are returned without resolving them. This keeps archive
    /// overlays, case folding and path lookup in the caller.
    pub fn includes(&self) -> Result<Vec<DefInclude>, BuildError> {
        let mut out = Vec::new();
        collect_includes(&self.directives, &mut out)?;
        Ok(out)
    }

    /// Extract voxel model assignments in source order.
    ///
    /// A `tile` child maps one tile. A `tile0` / `tile1` pair maps an
    /// inclusive range. The parser validates all declared tile numbers and
    /// retains the renderer-specific `rotate` extension when present.
    pub fn voxels(&self) -> Result<Vec<DefVoxel>, BuildError> {
        let mut out = Vec::new();
        let mut next_model_index = 0;
        collect_voxels(&self.directives, &mut out, &mut next_model_index)?;
        Ok(out)
    }

    /// Extract palette tints declared by `tint { ... }` blocks.
    pub fn tints(&self) -> Result<Vec<DefTint>, BuildError> {
        let mut out = Vec::new();
        collect_tints(&self.directives, &mut out)?;
        Ok(out)
    }
}

fn collect_includes(
    directives: &[DefDirective],
    out: &mut Vec<DefInclude>,
) -> Result<(), BuildError> {
    for directive in directives {
        if matches!(directive.keyword.as_str(), "include" | "#include") {
            let file = directive
                .args
                .first()
                .filter(|path| !path.is_empty())
                .cloned()
                .ok_or(BuildError::Invalid("DEF include path"))?;
            if directive.args.len() != 1 || out.len() >= MAX_DEF_INCLUDES {
                return Err(BuildError::Invalid("DEF include count or arguments"));
            }
            out.push(DefInclude {
                file,
                line: directive.line,
            });
        }
        collect_includes(&directive.children, out)?;
    }
    Ok(())
}

fn parse_def_tile(value: Option<&String>, error: &'static str) -> Result<u16, BuildError> {
    value
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|tile| u32::from(*tile) < crate::art::MAX_TILES)
        .ok_or(BuildError::Invalid(error))
}

fn collect_voxels(
    directives: &[DefDirective],
    out: &mut Vec<DefVoxel>,
    next_model_index: &mut usize,
) -> Result<(), BuildError> {
    for directive in directives {
        if directive.keyword == "voxel" {
            if out.len() >= MAX_DEF_VOXELS {
                return Err(BuildError::Invalid("DEF voxel count"));
            }
            let file = directive
                .args
                .first()
                .filter(|file| !file.is_empty())
                .cloned()
                .ok_or(BuildError::Invalid("DEF voxel file"))?;
            let model_index = *next_model_index;
            *next_model_index = next_model_index
                .checked_add(1)
                .ok_or(BuildError::Invalid("DEF voxel count"))?;
            let mut tiles = Vec::new();
            let mut tile0 = None;
            let mut tile1 = None;
            let mut scale = 1.0_f32;
            let mut scale_seen = false;
            let mut rotate = false;

            let mut parse_options = |keyword: &str, args: &[String]| -> Result<(), BuildError> {
                match keyword.to_ascii_lowercase().as_str() {
                    "tile" => {
                        let value = parse_def_tile(args.first(), "DEF voxel tile")?;
                        tiles.push((value, value));
                    }
                    "tile0" => {
                        let value = parse_def_tile(args.first(), "DEF voxel tile0")?;
                        if tile0.replace(value).is_some() || tile1.is_some() {
                            return Err(BuildError::Invalid("DEF voxel tile range"));
                        }
                    }
                    "tile1" => {
                        let value = parse_def_tile(args.first(), "DEF voxel tile1")?;
                        if tile1.replace(value).is_some() || tile0.is_none() {
                            return Err(BuildError::Invalid("DEF voxel tile range"));
                        }
                    }
                    "scale" => {
                        let value = args
                            .first()
                            .and_then(|value| value.parse::<f32>().ok())
                            .filter(|value| value.is_finite() && *value > 0.0 && *value <= 64.0)
                            .ok_or(BuildError::Invalid("DEF voxel scale"))?;
                        if scale_seen {
                            return Err(BuildError::Invalid("DEF duplicate voxel scale"));
                        }
                        scale = value;
                        scale_seen = true;
                    }
                    "rotate" if args.is_empty() => rotate = true,
                    _ => {}
                }
                Ok(())
            };

            // Some DEF authors put several options on one line. The generic
            // syntax tree stores that whole line on its first directive, so
            // flatten each block line back into keyword/value tokens here.
            let mut option_tokens = directive.args.iter().skip(1).cloned().collect::<Vec<_>>();
            for child in &directive.children {
                option_tokens.push(child.keyword.clone());
                option_tokens.extend(child.args.iter().cloned());
            }
            let mut index = 0;
            while index < option_tokens.len() {
                let keyword = &option_tokens[index];
                if keyword.eq_ignore_ascii_case("rotate") {
                    parse_options(keyword, &[])?;
                    index += 1;
                } else if matches!(
                    keyword.to_ascii_lowercase().as_str(),
                    "tile" | "tile0" | "tile1" | "scale"
                ) {
                    let value = option_tokens
                        .get(index + 1)
                        .cloned()
                        .ok_or(BuildError::Invalid("DEF voxel option value"))?;
                    parse_options(keyword, std::slice::from_ref(&value))?;
                    index += 2;
                } else {
                    index += 1;
                }
            }

            match (tile0, tile1) {
                (Some(start), Some(end)) if start <= end => tiles.push((start, end)),
                (Some(_), Some(_)) => {
                    return Err(BuildError::Invalid("DEF voxel tile range order"));
                }
                (None, None) => {}
                _ => return Err(BuildError::Invalid("DEF voxel tile range")),
            }
            if tiles.is_empty() {
                return Err(BuildError::Invalid("DEF voxel tile mapping"));
            }
            let new_count = out
                .len()
                .checked_add(tiles.len())
                .filter(|count| *count <= MAX_DEF_VOXELS)
                .ok_or(BuildError::Invalid("DEF voxel count"))?;
            out.reserve(new_count - out.len());
            for (tile_start, tile_end) in tiles {
                out.push(DefVoxel {
                    file: file.clone(),
                    model_index,
                    tile_start,
                    tile_end,
                    scale,
                    rotate,
                    line: directive.line,
                });
            }
        }
        collect_voxels(&directive.children, out, next_model_index)?;
    }
    Ok(())
}

fn collect_tints(directives: &[DefDirective], out: &mut Vec<DefTint>) -> Result<(), BuildError> {
    for directive in directives {
        if directive.keyword == "tint" {
            if out.len() >= MAX_DEF_TINTS {
                return Err(BuildError::Invalid("DEF tint count"));
            }
            let mut palette = None;
            let mut rgb = [None; 3];
            let mut shade_rgb = [None; 3];
            let mut flags = None;
            let mut tint_tokens = Vec::new();
            for child in &directive.children {
                tint_tokens.push(child.keyword.clone());
                tint_tokens.extend(child.args.iter().cloned());
            }
            let mut index = 0;
            while index + 1 < tint_tokens.len() {
                let keyword = tint_tokens[index].to_ascii_lowercase();
                let value = &tint_tokens[index + 1];
                match keyword.as_str() {
                    "pal" => set_unique(&mut palette, value.parse::<u8>().ok(), "DEF tint pal")?,
                    "red" | "r" => {
                        set_unique(&mut rgb[0], value.parse::<u8>().ok(), "DEF tint red")?
                    }
                    "green" | "g" => {
                        set_unique(&mut rgb[1], value.parse::<u8>().ok(), "DEF tint green")?
                    }
                    "blue" | "b" => {
                        set_unique(&mut rgb[2], value.parse::<u8>().ok(), "DEF tint blue")?
                    }
                    "shadered" | "sr" => set_unique(
                        &mut shade_rgb[0],
                        value.parse::<u8>().ok(),
                        "DEF tint shadered",
                    )?,
                    "shadegreen" | "sg" => set_unique(
                        &mut shade_rgb[1],
                        value.parse::<u8>().ok(),
                        "DEF tint shadegreen",
                    )?,
                    "shadeblue" | "sb" => set_unique(
                        &mut shade_rgb[2],
                        value.parse::<u8>().ok(),
                        "DEF tint shadeblue",
                    )?,
                    "flags" => set_unique(&mut flags, value.parse::<u16>().ok(), "DEF tint flags")?,
                    _ => {}
                }
                index += 2;
            }
            if index < tint_tokens.len() {
                return Err(BuildError::Invalid("DEF tint value"));
            }
            let palette = palette.ok_or(BuildError::Invalid("DEF tint pal"))?;
            let rgb = [
                rgb[0].ok_or(BuildError::Invalid("DEF tint red"))?,
                rgb[1].ok_or(BuildError::Invalid("DEF tint green"))?,
                rgb[2].ok_or(BuildError::Invalid("DEF tint blue"))?,
            ];
            let shade_rgb = match (shade_rgb[0], shade_rgb[1], shade_rgb[2]) {
                (None, None, None) => None,
                (Some(red), Some(green), Some(blue)) => Some([red, green, blue]),
                _ => return Err(BuildError::Invalid("DEF tint shade RGB")),
            };
            out.push(DefTint {
                palette,
                rgb,
                shade_rgb,
                flags: flags.unwrap_or(0),
                line: directive.line,
            });
        }
        collect_tints(&directive.children, out)?;
    }
    Ok(())
}

fn set_unique<T>(
    slot: &mut Option<T>,
    value: Option<T>,
    error: &'static str,
) -> Result<(), BuildError> {
    let value = value.ok_or(BuildError::Invalid(error))?;
    if slot.replace(value).is_some() {
        return Err(BuildError::Invalid(error));
    }
    Ok(())
}

fn collect_music_tracks(
    directives: &[DefDirective],
    out: &mut Vec<DefMusicTrack>,
) -> Result<(), BuildError> {
    for directive in directives {
        if directive.keyword == "music" {
            let mut id = None;
            let mut file = None;
            for child in &directive.children {
                let mut pending = match child.keyword.as_str() {
                    "id" => Some("id"),
                    "file" => Some("file"),
                    _ => None,
                };
                for token in &child.args {
                    if let Some(key) = pending.take() {
                        let slot = if key == "id" { &mut id } else { &mut file };
                        if token.is_empty() || slot.replace(token.clone()).is_some() {
                            return Err(BuildError::Invalid("DEF music ID/file child"));
                        }
                        continue;
                    }
                    let key = match token.to_ascii_lowercase().as_str() {
                        "id" => "id",
                        "file" => "file",
                        _ => continue,
                    };
                    pending = Some(key);
                }
                if pending.is_some() {
                    return Err(BuildError::Invalid("DEF music ID/file child"));
                }
            }
            if let (Some(id), Some(file)) = (id, file) {
                if id.is_empty() || file.is_empty() || out.len() >= MAX_DEF_MUSIC_TRACKS {
                    return Err(BuildError::Invalid("DEF music block"));
                }
                out.push(DefMusicTrack {
                    id,
                    file,
                    line: directive.line,
                });
            }
        }
        collect_music_tracks(&directive.children, out)?;
    }
    Ok(())
}

fn collect_skyboxes(
    directives: &[DefDirective],
    out: &mut Vec<DefSkybox>,
) -> Result<(), BuildError> {
    for directive in directives {
        if directive.keyword == "skybox" {
            if out.len() >= MAX_DEF_SKYBOXES {
                return Err(BuildError::Invalid("DEF skybox count"));
            }
            let mut tile = None;
            let mut palette = None;
            let mut faces: [Option<String>; 6] = std::array::from_fn(|_| None);
            for child in &directive.children {
                if child.keyword == "tile" {
                    if tile.is_some() {
                        return Err(BuildError::Invalid("DEF skybox tile"));
                    }
                    tile = child
                        .args
                        .first()
                        .and_then(|value| value.parse::<u16>().ok())
                        .filter(|value| u32::from(*value) < crate::art::MAX_TILES);
                    if tile.is_none() {
                        return Err(BuildError::Invalid("DEF skybox tile"));
                    }
                    let mut index = 1;
                    while index < child.args.len() {
                        if child.args[index].eq_ignore_ascii_case("pal") {
                            if palette.is_some() {
                                return Err(BuildError::Invalid("DEF skybox palette"));
                            }
                            palette = child
                                .args
                                .get(index + 1)
                                .and_then(|value| value.parse::<u8>().ok());
                            if palette.is_none() {
                                return Err(BuildError::Invalid("DEF skybox palette"));
                            }
                            index += 2;
                        } else {
                            return Err(BuildError::Invalid("DEF skybox tile option"));
                        }
                    }
                } else if child.keyword == "pal" {
                    if palette.is_some() {
                        return Err(BuildError::Invalid("DEF skybox palette"));
                    }
                    palette = child
                        .args
                        .first()
                        .and_then(|value| value.parse::<u8>().ok());
                    if palette.is_none() {
                        return Err(BuildError::Invalid("DEF skybox palette"));
                    }
                } else if let Some(face_index) = skybox_face_index(&child.keyword) {
                    let file = child
                        .args
                        .first()
                        .filter(|value| !value.is_empty())
                        .cloned()
                        .ok_or(BuildError::Invalid("DEF skybox face file"))?;
                    if faces[face_index].replace(file).is_some() {
                        return Err(BuildError::Invalid("DEF skybox duplicate face"));
                    }
                }
            }

            let [front, right, back, left, top, down] = faces;
            let faces = [
                front.ok_or(BuildError::Invalid("DEF skybox missing face"))?,
                right.ok_or(BuildError::Invalid("DEF skybox missing face"))?,
                back.ok_or(BuildError::Invalid("DEF skybox missing face"))?,
                left.ok_or(BuildError::Invalid("DEF skybox missing face"))?,
                top.ok_or(BuildError::Invalid("DEF skybox missing face"))?,
                down.ok_or(BuildError::Invalid("DEF skybox missing face"))?,
            ];
            out.push(DefSkybox {
                tile: tile.ok_or(BuildError::Invalid("DEF skybox tile"))?,
                palette: palette.ok_or(BuildError::Invalid("DEF skybox palette"))?,
                faces,
                line: directive.line,
            });
        }
        collect_skyboxes(&directive.children, out)?;
    }
    Ok(())
}

fn skybox_face_index(keyword: &str) -> Option<usize> {
    match keyword {
        "front" | "ft" | "forward" => Some(0),
        "right" | "rt" => Some(1),
        "back" | "bk" => Some(2),
        "left" | "lf" | "lt" => Some(3),
        "top" | "up" | "ceiling" | "ceil" => Some(4),
        "down" | "dn" | "floor" | "bottom" => Some(5),
        _ => None,
    }
}

/// Decode a high-resolution tile image (PNG, BMP and other supported formats)
/// without resizing it.
pub fn decode_tile_image(data: &[u8]) -> Result<room_formats::Image, room_formats::FormatError> {
    room_formats::decode_image(data)
}

fn normalize_asset_name(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

fn collect_tile_operations(
    directives: &[DefDirective],
    operations: &mut Vec<TileOperation>,
) -> Result<(), BuildError> {
    for directive in directives {
        match directive.keyword.as_str() {
            "copytile" => {
                let destination = parse_tile_id(directive.args.first())?;
                let source = directive
                    .children
                    .iter()
                    .find(|child| child.keyword == "tile")
                    .and_then(|child| child.args.first())
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|tile| u32::from(*tile) < crate::art::MAX_TILES)
                    .and_then(|tile| i16::try_from(tile).ok())
                    .ok_or(BuildError::Invalid("DEF copytile source"))?;
                operations.push(TileOperation::Copy {
                    destination,
                    source,
                });
            }
            "dummytile" if directive.args.len() >= 3 => {
                let width = parse_tile_dimension(directive.args.get(1))?;
                let height = parse_tile_dimension(directive.args.get(2))?;
                validate_dummy_size(width, height)?;
                operations.push(TileOperation::Dummy {
                    tile: parse_tile_id(directive.args.first())?,
                    width,
                    height,
                });
            }
            "dummytilerange" if directive.args.len() >= 4 => {
                let start = parse_tile_id(directive.args.first())?;
                let end = parse_tile_id(directive.args.get(1))?;
                if end < start {
                    return Err(BuildError::Invalid("DEF dummytilerange order"));
                }
                let width = parse_tile_dimension(directive.args.get(2))?;
                let height = parse_tile_dimension(directive.args.get(3))?;
                validate_dummy_size(width, height)?;
                let range_len = usize::try_from(i32::from(end) - i32::from(start) + 1)
                    .map_err(|_| BuildError::Invalid("DEF dummytilerange size"))?;
                let range_pixels = range_len
                    .checked_mul(usize::from(width) * usize::from(height))
                    .ok_or(BuildError::Invalid("DEF dummytilerange size"))?;
                if range_pixels > MAX_DUMMY_RANGE_PIXELS {
                    return Err(BuildError::Invalid("DEF dummytilerange size"));
                }
                operations.push(TileOperation::DummyRange {
                    start,
                    end,
                    width,
                    height,
                });
            }
            "animtilerange" if directive.args.len() >= 4 => {
                let start = parse_tile_id(directive.args.first())?;
                let end = parse_tile_id(directive.args.get(1))?;
                let speed = directive
                    .args
                    .get(2)
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|speed| *speed <= 15)
                    .ok_or(BuildError::Invalid("DEF animtilerange speed"))?;
                let animation = directive
                    .args
                    .get(3)
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|animation| *animation <= 3)
                    .ok_or(BuildError::Invalid("DEF animtilerange type"))?;
                if (animation == 3 && start < end) || (animation != 3 && end < start) {
                    return Err(BuildError::Invalid("DEF animtilerange order"));
                }
                operations.push(TileOperation::AnimationRange {
                    start,
                    end,
                    speed,
                    animation,
                });
            }
            _ => {}
        }
        collect_tile_operations(&directive.children, operations)?;
    }
    Ok(())
}

fn parse_tile_id(value: Option<&String>) -> Result<i16, BuildError> {
    value
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|tile| u32::from(*tile) < crate::art::MAX_TILES)
        .and_then(|tile| i16::try_from(tile).ok())
        .ok_or(BuildError::Invalid("DEF tile number"))
}

fn parse_tile_dimension(value: Option<&String>) -> Result<u16, BuildError> {
    value
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|dimension| *dimension > 0)
        .ok_or(BuildError::Invalid("DEF tile dimension"))
}

fn validate_dummy_size(width: u16, height: u16) -> Result<(), BuildError> {
    let pixels = usize::from(width)
        .checked_mul(usize::from(height))
        .ok_or(BuildError::Invalid("DEF tile dimension"))?;
    if pixels > MAX_HIGH_RES_TILE_PIXELS {
        return Err(BuildError::Invalid("DEF tile dimension"));
    }
    Ok(())
}

fn collect_tile_specs(
    directives: &[DefDirective],
    out: &mut Vec<HighResTileSpec>,
) -> Result<(), BuildError> {
    for directive in directives {
        if directive.keyword == "tilefromtexture" {
            let tile = directive
                .args
                .first()
                .and_then(|value| value.parse::<u16>().ok())
                .filter(|tile| u32::from(*tile) < crate::art::MAX_TILES)
                .ok_or(BuildError::Invalid("DEF tilefromtexture tile"))?;
            let mut file = None;
            let mut x_offset = 0;
            let mut y_offset = 0;
            let mut opaque_alpha_threshold = 174;
            parse_offset_options(&directive.args, &mut x_offset, &mut y_offset)?;
            parse_alpha_options(&directive.args, &mut opaque_alpha_threshold)?;
            for child in &directive.children {
                if child.keyword == "file" {
                    file = child.args.first().cloned();
                }
                parse_offset_options(&child.args, &mut x_offset, &mut y_offset)?;
                parse_alpha_options(&child.args, &mut opaque_alpha_threshold)?;
                match child.keyword.as_str() {
                    "xoff" | "xoffset" => {
                        x_offset = child
                            .args
                            .first()
                            .and_then(|value| value.parse().ok())
                            .ok_or(BuildError::Invalid("DEF tilefromtexture xoff"))?;
                    }
                    "yoff" | "yoffset" => {
                        y_offset = child
                            .args
                            .first()
                            .and_then(|value| value.parse().ok())
                            .ok_or(BuildError::Invalid("DEF tilefromtexture yoff"))?;
                    }
                    _ => {}
                }
            }
            let file = file.ok_or(BuildError::Invalid("DEF tilefromtexture file"))?;
            out.push(HighResTileSpec {
                tile,
                file,
                x_offset,
                y_offset,
                opaque_alpha_threshold,
                palette: None,
                line: directive.line,
            });
        } else if directive.keyword == "texture" {
            // EDuke32 falls back to palette 0 for palette banks without an
            // explicit branch. Keep each declared branch so renderers can
            // select the matching image and fall back to the base tile.
            for variant in parse_texture_variants(directive)? {
                out.push(HighResTileSpec {
                    tile: variant.tile,
                    palette: (variant.palette != 0).then_some(variant.palette),
                    file: variant.file,
                    x_offset: 0,
                    y_offset: 0,
                    opaque_alpha_threshold: variant.opaque_alpha_threshold,
                    line: variant.line,
                });
            }
        }
        collect_tile_specs(&directive.children, out)?;
    }
    Ok(())
}

fn collect_texture_variants(
    directives: &[DefDirective],
    out: &mut Vec<DefTextureVariant>,
) -> Result<(), BuildError> {
    for directive in directives {
        if directive.keyword == "texture" {
            out.extend(parse_texture_variants(directive)?);
        }
        collect_texture_variants(&directive.children, out)?;
    }
    Ok(())
}

fn collect_base_palette_operations(
    directives: &[DefDirective],
    out: &mut Vec<BasePaletteOperation>,
) -> Result<(), BuildError> {
    for directive in directives {
        if directive.keyword == "basepalette" {
            if out.len() >= MAX_DEF_BASE_PALETTES {
                return Err(BuildError::Invalid("DEF basepalette count"));
            }
            let palette = directive
                .args
                .first()
                .and_then(|value| value.parse::<u8>().ok())
                .ok_or(BuildError::Invalid("DEF basepalette id"))?;
            let mut operation = None;
            for child in &directive.children {
                let candidate = match child.keyword.as_str() {
                    "raw" => {
                        let mut file = None;
                        let mut offset = 0usize;
                        let mut shift_left = 0u8;
                        let mut offset_seen = false;
                        let mut shift_left_seen = false;
                        for field in &child.children {
                            if field.keyword == "file" {
                                let filename = field
                                    .args
                                    .first()
                                    .filter(|filename| !filename.is_empty())
                                    .ok_or(BuildError::Invalid("DEF basepalette raw file"))?;
                                if file.replace(filename.clone()).is_some() {
                                    return Err(BuildError::Invalid("DEF basepalette raw file"));
                                }
                                let mut index = 1;
                                while index < field.args.len() {
                                    let key = field
                                        .args
                                        .get(index)
                                        .ok_or(BuildError::Invalid("DEF basepalette raw option"))?;
                                    let value = field
                                        .args
                                        .get(index + 1)
                                        .ok_or(BuildError::Invalid("DEF basepalette raw option"))?;
                                    match key.to_ascii_lowercase().as_str() {
                                        "offset" if !offset_seen => {
                                            offset = value.parse::<usize>().map_err(|_| {
                                                BuildError::Invalid("DEF basepalette raw offset")
                                            })?;
                                            offset_seen = true;
                                        }
                                        "shiftleft" if !shift_left_seen => {
                                            shift_left = value
                                                .parse::<u8>()
                                                .ok()
                                                .filter(|value| *value <= 7)
                                                .ok_or(BuildError::Invalid(
                                                    "DEF basepalette raw shiftleft",
                                                ))?;
                                            shift_left_seen = true;
                                        }
                                        _ => {
                                            return Err(BuildError::Invalid(
                                                "DEF basepalette raw option",
                                            ));
                                        }
                                    }
                                    index += 2;
                                }
                            } else if field.keyword == "offset" && !offset_seen {
                                offset = field
                                    .args
                                    .first()
                                    .and_then(|value| value.parse::<usize>().ok())
                                    .ok_or(BuildError::Invalid("DEF basepalette raw offset"))?;
                                offset_seen = true;
                            } else if field.keyword == "shiftleft" && !shift_left_seen {
                                shift_left = field
                                    .args
                                    .first()
                                    .and_then(|value| value.parse::<u8>().ok())
                                    .filter(|value| *value <= 7)
                                    .ok_or(BuildError::Invalid("DEF basepalette raw shiftleft"))?;
                                shift_left_seen = true;
                            } else if matches!(field.keyword.as_str(), "offset" | "shiftleft") {
                                return Err(BuildError::Invalid("DEF basepalette raw option"));
                            }
                        }
                        let file = file.ok_or(BuildError::Invalid("DEF basepalette raw file"))?;
                        BasePaletteOperation::Load {
                            palette,
                            file,
                            offset,
                            shift_left,
                            line: directive.line,
                        }
                    }
                    "copy" => {
                        let source = child
                            .args
                            .first()
                            .and_then(|value| value.parse::<u8>().ok())
                            .ok_or(BuildError::Invalid("DEF basepalette copy"))?;
                        BasePaletteOperation::Copy {
                            palette,
                            source,
                            line: directive.line,
                        }
                    }
                    "undef" => BasePaletteOperation::Undef {
                        palette,
                        line: directive.line,
                    },
                    _ => continue,
                };
                if operation.replace(candidate).is_some() {
                    return Err(BuildError::Invalid("DEF basepalette operation"));
                }
            }
            out.push(operation.ok_or(BuildError::Invalid("DEF basepalette operation"))?);
        }
        collect_base_palette_operations(&directive.children, out)?;
    }
    Ok(())
}

fn collect_blend_table_operations(
    directives: &[DefDirective],
    out: &mut Vec<BlendTableOperation>,
) -> Result<(), BuildError> {
    for directive in directives {
        if directive.keyword == "blendtable" {
            if out.len() >= MAX_DEF_BLEND_TABLES {
                return Err(BuildError::Invalid("DEF blendtable count"));
            }
            let table = directive
                .args
                .first()
                .and_then(|value| value.parse::<u8>().ok())
                .filter(|table| *table != 0)
                .ok_or(BuildError::Invalid("DEF blendtable id"))?;
            let mut operation = None;
            for child in &directive.children {
                let candidate = match child.keyword.as_str() {
                    "raw" => {
                        let mut file = None;
                        let mut offset = 0usize;
                        let mut offset_seen = false;
                        for field in &child.children {
                            match field.keyword.as_str() {
                                "file" | "name" => {
                                    let filename = field
                                        .args
                                        .first()
                                        .filter(|filename| !filename.is_empty())
                                        .ok_or(BuildError::Invalid("DEF blendtable raw file"))?;
                                    if file.replace(filename.clone()).is_some() {
                                        return Err(BuildError::Invalid("DEF blendtable raw file"));
                                    }
                                    let mut index = 1;
                                    while index < field.args.len() {
                                        let key = field.args.get(index).ok_or(
                                            BuildError::Invalid("DEF blendtable raw option"),
                                        )?;
                                        let value = field.args.get(index + 1).ok_or(
                                            BuildError::Invalid("DEF blendtable raw option"),
                                        )?;
                                        if key.eq_ignore_ascii_case("offset") && !offset_seen {
                                            offset = value.parse::<usize>().map_err(|_| {
                                                BuildError::Invalid("DEF blendtable raw offset")
                                            })?;
                                            offset_seen = true;
                                        } else {
                                            return Err(BuildError::Invalid(
                                                "DEF blendtable raw option",
                                            ));
                                        }
                                        index += 2;
                                    }
                                }
                                "offset" if !offset_seen => {
                                    offset = field
                                        .args
                                        .first()
                                        .and_then(|value| value.parse::<usize>().ok())
                                        .ok_or(BuildError::Invalid("DEF blendtable raw offset"))?;
                                    offset_seen = true;
                                }
                                "offset" => {
                                    return Err(BuildError::Invalid("DEF blendtable raw offset"));
                                }
                                _ => return Err(BuildError::Invalid("DEF blendtable raw option")),
                            }
                        }
                        BlendTableOperation::Load {
                            table,
                            file: file.ok_or(BuildError::Invalid("DEF blendtable raw file"))?,
                            offset,
                            line: directive.line,
                        }
                    }
                    "copy" => BlendTableOperation::Copy {
                        table,
                        source: child
                            .args
                            .first()
                            .and_then(|value| value.parse::<u8>().ok())
                            .ok_or(BuildError::Invalid("DEF blendtable copy"))?,
                        line: directive.line,
                    },
                    "undef" => BlendTableOperation::Undef {
                        table,
                        line: directive.line,
                    },
                    // OpenGL blend factors do not replace a classic indexed
                    // table, but are accepted as a separate renderer feature.
                    "glblend" => continue,
                    _ => continue,
                };
                if operation.replace(candidate).is_some() {
                    return Err(BuildError::Invalid("DEF blendtable operation"));
                }
            }
            if let Some(operation) = operation {
                out.push(operation);
            }
        }
        collect_blend_table_operations(&directive.children, out)?;
    }
    Ok(())
}

fn parse_texture_variants(directive: &DefDirective) -> Result<Vec<DefTextureVariant>, BuildError> {
    let tile = directive
        .args
        .first()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|tile| u32::from(*tile) < crate::art::MAX_TILES)
        .ok_or(BuildError::Invalid("DEF texture tile"))?;
    let mut variants = Vec::new();
    for palette_block in directive
        .children
        .iter()
        .filter(|child| child.keyword == "pal")
    {
        let palette = palette_block
            .args
            .first()
            .and_then(|value| value.parse::<u8>().ok())
            .ok_or(BuildError::Invalid("DEF texture palette"))?;
        let mut file = None;
        let mut opaque_alpha_threshold = 174;
        for child in &palette_block.children {
            if matches!(child.keyword.as_str(), "file" | "name") {
                file = child.args.first().cloned();
            }
            parse_alpha_options(&child.args, &mut opaque_alpha_threshold)?;
        }
        let file = file.ok_or(BuildError::Invalid("DEF texture palette file"))?;
        variants.push(DefTextureVariant {
            tile,
            palette,
            file,
            opaque_alpha_threshold,
            line: directive.line,
        });
    }
    Ok(variants)
}

fn parse_offset_options(
    args: &[String],
    x_offset: &mut i32,
    y_offset: &mut i32,
) -> Result<(), BuildError> {
    for (index, option) in args.iter().enumerate() {
        match option.to_ascii_lowercase().as_str() {
            "xoff" | "xoffset" => {
                *x_offset = args
                    .get(index + 1)
                    .and_then(|value| value.parse().ok())
                    .ok_or(BuildError::Invalid("DEF tilefromtexture xoff"))?;
            }
            "yoff" | "yoffset" => {
                *y_offset = args
                    .get(index + 1)
                    .and_then(|value| value.parse().ok())
                    .ok_or(BuildError::Invalid("DEF tilefromtexture yoff"))?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_alpha_options(args: &[String], threshold: &mut u8) -> Result<(), BuildError> {
    for (index, option) in args.iter().enumerate() {
        if option.eq_ignore_ascii_case("alphacut") {
            let value = args
                .get(index + 1)
                .ok_or(BuildError::Invalid("DEF tilefromtexture alphacut"))?;
            *threshold = alpha_threshold(value)?;
        }
    }
    Ok(())
}

fn alpha_threshold(value: &str) -> Result<u8, BuildError> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let whole = if whole.is_empty() {
        0
    } else {
        whole
            .parse::<u32>()
            .map_err(|_| BuildError::Invalid("DEF tilefromtexture alphacut"))?
    };
    if whole > 1 || fraction.len() > 6 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(BuildError::Invalid("DEF tilefromtexture alphacut"));
    }
    let denominator = 10u32
        .checked_pow(fraction.len() as u32)
        .ok_or(BuildError::Invalid("DEF tilefromtexture alphacut"))?;
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u32>()
            .map_err(|_| BuildError::Invalid("DEF tilefromtexture alphacut"))?
    };
    let numerator = whole
        .checked_mul(denominator)
        .and_then(|integer| integer.checked_add(fractional))
        .filter(|number| *number <= denominator)
        .ok_or(BuildError::Invalid("DEF tilefromtexture alphacut"))?;
    let opaque_numerator = denominator - numerator;
    let threshold = (opaque_numerator * 255).div_ceil(denominator);
    u8::try_from(threshold).map_err(|_| BuildError::Invalid("DEF tilefromtexture alphacut"))
}

fn tokenize(source: &str) -> Result<Vec<Token>, BuildError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    let mut line = 1;
    while cursor < chars.len() {
        let current = chars[cursor];
        match current {
            '\n' => {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    line,
                });
                line += 1;
                cursor += 1;
            }
            '\r' | ' ' | '\t' | '\u{000c}' => cursor += 1,
            '/' if chars.get(cursor + 1) == Some(&'/') => {
                cursor += 2;
                while cursor < chars.len() && chars[cursor] != '\n' {
                    cursor += 1;
                }
            }
            '/' if chars.get(cursor + 1) == Some(&'*') => {
                cursor += 2;
                let mut closed = false;
                while cursor + 1 < chars.len() {
                    if chars[cursor] == '\n' {
                        line += 1;
                    }
                    if chars[cursor] == '*' && chars[cursor + 1] == '/' {
                        cursor += 2;
                        closed = true;
                        break;
                    }
                    cursor += 1;
                }
                if !closed {
                    return Err(BuildError::Invalid("DEF unterminated block comment"));
                }
            }
            '{' => {
                tokens.push(Token {
                    kind: TokenKind::Open,
                    line,
                });
                cursor += 1;
            }
            '}' => {
                tokens.push(Token {
                    kind: TokenKind::Close,
                    line,
                });
                cursor += 1;
            }
            '"' => {
                cursor += 1;
                let mut value = String::new();
                let mut closed = false;
                while cursor < chars.len() {
                    match chars[cursor] {
                        '"' => {
                            cursor += 1;
                            closed = true;
                            break;
                        }
                        '\\' if chars
                            .get(cursor + 1)
                            .is_some_and(|next| *next == '"' || *next == '\\') =>
                        {
                            cursor += 1;
                            value.push(chars[cursor]);
                            cursor += 1;
                        }
                        '\n' => {
                            line += 1;
                            value.push('\n');
                            cursor += 1;
                        }
                        value_char => {
                            value.push(value_char);
                            cursor += 1;
                        }
                    }
                }
                if !closed {
                    return Err(BuildError::Invalid("DEF unterminated string"));
                }
                tokens.push(Token {
                    kind: TokenKind::Word(value),
                    line,
                });
            }
            _ => {
                let start = cursor;
                while cursor < chars.len()
                    && !chars[cursor].is_whitespace()
                    && chars[cursor] != '{'
                    && chars[cursor] != '}'
                    && !(chars[cursor] == '/' && matches!(chars.get(cursor + 1), Some('/' | '*')))
                {
                    cursor += 1;
                }
                if start == cursor {
                    return Err(BuildError::Invalid("DEF token"));
                }
                tokens.push(Token {
                    kind: TokenKind::Word(chars[start..cursor].iter().collect()),
                    line,
                });
            }
        }
    }
    Ok(tokens)
}

fn parse_block(
    tokens: &[Token],
    cursor: &mut usize,
    nested: bool,
) -> Result<Vec<DefDirective>, BuildError> {
    let mut directives = Vec::new();
    while *cursor < tokens.len() {
        match &tokens[*cursor].kind {
            TokenKind::Newline => *cursor += 1,
            TokenKind::Close if nested => {
                *cursor += 1;
                return Ok(directives);
            }
            TokenKind::Close => return Err(BuildError::Invalid("DEF unmatched closing brace")),
            TokenKind::Open => return Err(BuildError::Invalid("DEF block without directive")),
            TokenKind::Word(_) => {
                let line = tokens[*cursor].line;
                let TokenKind::Word(keyword) = &tokens[*cursor].kind else {
                    return Err(BuildError::Invalid("DEF directive keyword"));
                };
                let keyword = keyword.to_ascii_lowercase();
                *cursor += 1;
                let mut args = Vec::new();
                let mut children = Vec::new();
                let mut has_block = false;
                while *cursor < tokens.len() {
                    match &tokens[*cursor].kind {
                        TokenKind::Word(word) => {
                            args.push(word.clone());
                            *cursor += 1;
                        }
                        TokenKind::Newline => {
                            *cursor += 1;
                            while matches!(
                                tokens.get(*cursor).map(|token| &token.kind),
                                Some(TokenKind::Newline)
                            ) {
                                *cursor += 1;
                            }
                            if matches!(
                                tokens.get(*cursor).map(|token| &token.kind),
                                Some(TokenKind::Open)
                            ) {
                                *cursor += 1;
                                children = parse_block(tokens, cursor, true)?;
                                has_block = true;
                            }
                            break;
                        }
                        TokenKind::Open => {
                            *cursor += 1;
                            children = parse_block(tokens, cursor, true)?;
                            has_block = true;
                            break;
                        }
                        TokenKind::Close => break,
                    }
                }
                directives.push(DefDirective {
                    keyword,
                    args,
                    children,
                    line,
                });
                if has_block {
                    continue;
                }
            }
        }
    }
    if nested {
        Err(BuildError::Invalid("DEF unclosed block"))
    } else {
        Ok(directives)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn cached_palette_mapping_matches_exact_palette_search() {
        let palette = Palette::parse(&crate::palette::build_palette()).unwrap();
        let rgb = [93, 147, 211];
        let expected = palette.nearest(rgb.map(u32::from));
        let mut cache = HashMap::new();

        assert_eq!(nearest_palette_index(&palette, rgb, &mut cache), expected);
        assert_eq!(nearest_palette_index(&palette, rgb, &mut cache), expected);
        assert_eq!(cache.get(&rgb), Some(&expected));
    }

    #[test]
    fn parses_nested_tile_replacements_and_offsets() {
        let source = r#"
            // replacement with an escaped path separator
            tilefromtexture 9020 { file "sprites\\9020.png" nofullbright xoff 8 yoff -3 }
            texture 100 { pal 0 { file "base.png" } }
        "#;
        let doc = DefDocument::parse(source).unwrap();
        assert_eq!(doc.directives.len(), 2);
        assert_eq!(doc.directives[1].children[0].children[0].args, ["base.png"]);
        assert_eq!(
            doc.high_res_tiles().unwrap(),
            [
                HighResTileSpec {
                    tile: 9020,
                    palette: None,
                    file: "sprites\\9020.png".into(),
                    x_offset: 8,
                    y_offset: -3,
                    opaque_alpha_threshold: 174,
                    line: 3,
                },
                HighResTileSpec {
                    tile: 100,
                    palette: None,
                    file: "base.png".into(),
                    x_offset: 0,
                    y_offset: 0,
                    opaque_alpha_threshold: 174,
                    line: 4,
                }
            ]
        );
    }

    #[test]
    fn extracts_palette_zero_texture_and_alpha_cut() {
        let document = DefDocument::parse(
            r#"texture 3586 { pal 21 { file "red.png" } pal 0 { file "base.png" alphacut 0.32 } }"#,
        )
        .unwrap();
        assert_eq!(
            document.high_res_tiles().unwrap(),
            [
                HighResTileSpec {
                    tile: 3586,
                    palette: Some(21),
                    file: "red.png".into(),
                    x_offset: 0,
                    y_offset: 0,
                    opaque_alpha_threshold: 174,
                    line: 1,
                },
                HighResTileSpec {
                    tile: 3586,
                    palette: None,
                    file: "base.png".into(),
                    x_offset: 0,
                    y_offset: 0,
                    opaque_alpha_threshold: 174,
                    line: 1,
                }
            ]
        );
        assert_eq!(
            document.texture_variants().unwrap(),
            [
                DefTextureVariant {
                    tile: 3586,
                    palette: 21,
                    file: "red.png".into(),
                    opaque_alpha_threshold: 174,
                    line: 1,
                },
                DefTextureVariant {
                    tile: 3586,
                    palette: 0,
                    file: "base.png".into(),
                    opaque_alpha_threshold: 174,
                    line: 1,
                },
            ]
        );
    }

    #[test]
    fn texture_without_palette_zero_is_a_palette_specific_replacement() {
        let document = DefDocument::parse(r#"texture 3586 { pal 21 { file "red.png" } }"#).unwrap();
        assert_eq!(
            document.high_res_tiles().unwrap(),
            [HighResTileSpec {
                tile: 3586,
                palette: Some(21),
                file: "red.png".into(),
                x_offset: 0,
                y_offset: 0,
                opaque_alpha_threshold: 174,
                line: 1,
            }]
        );
        assert_eq!(document.texture_variants().unwrap()[0].palette, 21,);
    }

    #[test]
    fn rejects_texture_palette_without_file_or_invalid_bank() {
        let no_file = DefDocument::parse("texture 3586 { pal 0 { xscale 2.0 } }").unwrap();
        assert_eq!(
            no_file.texture_variants(),
            Err(BuildError::Invalid("DEF texture palette file"))
        );
        let invalid_bank = DefDocument::parse("texture 3586 { pal 256 { file x.png } }").unwrap();
        assert_eq!(
            invalid_bank.texture_variants(),
            Err(BuildError::Invalid("DEF texture palette"))
        );
    }

    #[test]
    fn extracts_map_music_ids_and_paths() {
        let source = r#"
            Music { ID "E1L1" file "resources/music/stalker.ogg" }
            music {
                id "e1l2"
                file "alternate.mid"
            }
        "#;
        let doc = DefDocument::parse(source).unwrap();
        assert_eq!(
            doc.music_tracks().unwrap(),
            [
                DefMusicTrack {
                    id: "E1L1".into(),
                    file: "resources/music/stalker.ogg".into(),
                    line: 2,
                },
                DefMusicTrack {
                    id: "e1l2".into(),
                    file: "alternate.mid".into(),
                    line: 3,
                },
            ]
        );
    }

    #[test]
    fn extracts_def_includes_in_source_order() {
        let document =
            DefDocument::parse("include resources/voxels.def\n#INCLUDE \"extra\\\\tiles.def\"\n")
                .unwrap();
        assert_eq!(
            document.includes().unwrap(),
            [
                DefInclude {
                    file: "resources/voxels.def".into(),
                    line: 1,
                },
                DefInclude {
                    file: "extra\\tiles.def".into(),
                    line: 2,
                },
            ]
        );
        assert!(DefDocument::parse("include\n").unwrap().includes().is_err());
    }

    #[test]
    fn extracts_voxel_tiles_ranges_scale_and_rotation() {
        let document = DefDocument::parse(
            r#"
                voxel "voxels/one.kvx" { tile 130 }
                voxel "voxels/range.kvx" { tile0 200 tile1 205 scale 0.75 rotate }
                voxel "voxels/multi.kvx" { tile 300 tile 991 }
            "#,
        )
        .unwrap();
        assert_eq!(
            document.voxels().unwrap(),
            [
                DefVoxel {
                    file: "voxels/one.kvx".into(),
                    model_index: 0,
                    tile_start: 130,
                    tile_end: 130,
                    scale: 1.0,
                    rotate: false,
                    line: 2,
                },
                DefVoxel {
                    file: "voxels/range.kvx".into(),
                    model_index: 1,
                    tile_start: 200,
                    tile_end: 205,
                    scale: 0.75,
                    rotate: true,
                    line: 3,
                },
                DefVoxel {
                    file: "voxels/multi.kvx".into(),
                    model_index: 2,
                    tile_start: 300,
                    tile_end: 300,
                    scale: 1.0,
                    rotate: false,
                    line: 4,
                },
                DefVoxel {
                    file: "voxels/multi.kvx".into(),
                    model_index: 2,
                    tile_start: 991,
                    tile_end: 991,
                    scale: 1.0,
                    rotate: false,
                    line: 4,
                },
            ]
        );
    }

    #[test]
    fn rejects_incomplete_voxel_ranges_and_invalid_scales() {
        for source in [
            "voxel model.kvx { tile0 3 }",
            "voxel model.kvx { tile1 3 tile0 2 }",
            "voxel model.kvx { tile 2 scale 0 }",
            "voxel model.kvx { tile 2 scale 1 scale 2 }",
        ] {
            assert!(
                DefDocument::parse(source).unwrap().voxels().is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn extracts_palette_tints_and_optional_shade_rgb() {
        let document = DefDocument::parse(
            "tint { pal 3 red 127 green 128 blue 255 flags 3 }\n\
             tint { pal 7 r 1 g 2 b 3 sr 4 sg 5 sb 6 }\n",
        )
        .unwrap();
        assert_eq!(
            document.tints().unwrap(),
            [
                DefTint {
                    palette: 3,
                    rgb: [127, 128, 255],
                    shade_rgb: None,
                    flags: 3,
                    line: 1,
                },
                DefTint {
                    palette: 7,
                    rgb: [1, 2, 3],
                    shade_rgb: Some([4, 5, 6]),
                    flags: 0,
                    line: 2,
                },
            ]
        );
        assert!(
            DefDocument::parse("tint { pal 1 red 0 blue 0 }\n")
                .unwrap()
                .tints()
                .is_err()
        );
    }

    #[test]
    fn rejects_duplicate_music_file_children() {
        let doc = DefDocument::parse("Music { ID E1L1 file first.ogg file second.ogg }").unwrap();
        assert_eq!(
            doc.music_tracks(),
            Err(BuildError::Invalid("DEF music ID/file child"))
        );
    }

    #[test]
    fn extracts_skybox_faces_and_face_aliases() {
        let document = DefDocument::parse(
            r#"
                skybox {
                    tile 95 pal 7
                    ft "front.png" nocompress
                    rt "right.png"
                    bk "back.png"
                    lt "left.png"
                    up "top.png"
                    dn "down.png"
                }
            "#,
        )
        .unwrap();
        assert_eq!(
            document.skyboxes().unwrap(),
            [DefSkybox {
                tile: 95,
                palette: 7,
                faces: [
                    "front.png".into(),
                    "right.png".into(),
                    "back.png".into(),
                    "left.png".into(),
                    "top.png".into(),
                    "down.png".into(),
                ],
                line: 2,
            }]
        );
    }

    #[test]
    fn parses_the_bundled_brutal_active_skybox_resources() {
        // Brutal Duke is a local-only third-party fixture (not in git); skip when absent.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/brutal-duke-v0.2.0/brutal/resources/skyboxes.def"
        );
        let Ok(source) = std::fs::read_to_string(path) else {
            eprintln!("skip: {path} not present");
            return;
        };
        let document = DefDocument::parse(&source).unwrap();
        let skyboxes = document.skyboxes().unwrap();
        // The file contains 22 textual declarations; four legacy add-on
        // sections are disabled inside block comments.
        assert_eq!(skyboxes.len(), 18);
        let palette_variant = skyboxes
            .iter()
            .find(|skybox| skybox.tile == 94 && skybox.palette == 13)
            .unwrap();
        assert_eq!(
            palette_variant.faces,
            [
                "resources\\skyboxes\\vaca\\0094_13_north.png",
                "resources\\skyboxes\\vaca\\0094_13_north.png",
                "resources\\skyboxes\\vaca\\0094_13_north.png",
                "resources\\skyboxes\\vaca\\0094_13_north.png",
                "resources\\skyboxes\\vaca\\0094_13_top.png",
                "resources\\skyboxes\\vaca\\0094_13_bottom.png",
            ]
        );
    }

    #[test]
    fn imports_and_shares_skybox_faces_by_resource_path() {
        let document = DefDocument::parse(
            r#"
                skybox {
                    tile 95 pal 0
                    front "face.bmp"
                    right "face.bmp"
                    back "face.bmp"
                    left "face.bmp"
                    top "face.bmp"
                    down "face.bmp"
                }
            "#,
        )
        .unwrap();
        let bmp = bmp_2x1_red_green();
        let palette = Palette {
            rgb: [0; 768],
            shades: Vec::new(),
            numshades: 1,
            translucency: None,
        };

        let imported = document
            .import_skyboxes(&[("face.bmp", bmp.as_slice())], &palette)
            .unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(
            (imported[0].faces[0].width, imported[0].faces[0].height),
            (2, 1)
        );
        assert!(std::sync::Arc::ptr_eq(
            &imported[0].faces[0].pixels,
            &imported[0].faces[5].pixels,
        ));
    }

    #[test]
    fn rejects_incomplete_or_duplicated_skybox_resources() {
        let missing = DefDocument::parse(
            r#"
                skybox {
                    tile 95 pal 0
                    front f.png
                    right r.png
                    back b.png
                    left l.png
                    top t.png
                }
            "#,
        )
        .unwrap();
        assert_eq!(
            missing.skyboxes(),
            Err(BuildError::Invalid("DEF skybox missing face"))
        );

        let duplicate = DefDocument::parse(
            r#"
                skybox {
                    tile 95 pal 0
                    front f.png
                    ft other.png
                    right r.png
                    back b.png
                    left l.png
                    top t.png
                    down d.png
                }
            "#,
        )
        .unwrap();
        assert_eq!(
            duplicate.skyboxes(),
            Err(BuildError::Invalid("DEF skybox duplicate face"))
        );
    }

    #[test]
    fn extracts_base_palette_load_copy_and_undef_in_source_order() {
        let doc = DefDocument::parse(
            r#"
                basepalette 240 { raw { file "resources\\palettes\\cinema.pal" offset 768 shiftleft 2 } }
                basepalette 241 { copy 240 }
                basepalette 241 { undef }
            "#,
        )
        .unwrap();
        assert_eq!(
            doc.base_palette_operations().unwrap(),
            [
                BasePaletteOperation::Load {
                    palette: 240,
                    file: "resources\\palettes\\cinema.pal".into(),
                    offset: 768,
                    shift_left: 2,
                    line: 2,
                },
                BasePaletteOperation::Copy {
                    palette: 241,
                    source: 240,
                    line: 3,
                },
                BasePaletteOperation::Undef {
                    palette: 241,
                    line: 4,
                },
            ]
        );
    }

    #[test]
    fn rejects_invalid_base_palette_resource_fields() {
        let missing_file = DefDocument::parse("basepalette 240 { raw { offset 768 } }").unwrap();
        assert_eq!(
            missing_file.base_palette_operations(),
            Err(BuildError::Invalid("DEF basepalette raw file"))
        );
        let invalid_shift =
            DefDocument::parse("basepalette 240 { raw { file palette.pal shiftleft 8 } }").unwrap();
        assert_eq!(
            invalid_shift.base_palette_operations(),
            Err(BuildError::Invalid("DEF basepalette raw shiftleft"))
        );
        let conflicting =
            DefDocument::parse("basepalette 240 { raw { file palette.pal } copy 1 }").unwrap();
        assert_eq!(
            conflicting.base_palette_operations(),
            Err(BuildError::Invalid("DEF basepalette operation"))
        );
    }

    #[test]
    fn extracts_blend_table_load_copy_and_undef_in_source_order() {
        let document = DefDocument::parse(
            r#"
                blendtable 200 { copy 0 glblend { both { src DST_COLOR dst SRC_COLOR } } }
                blendtable 201 { raw { file "multiply.raw" offset 65536 } }
                blendtable 201 { undef }
            "#,
        )
        .unwrap();

        assert_eq!(
            document.blend_table_operations().unwrap(),
            [
                BlendTableOperation::Copy {
                    table: 200,
                    source: 0,
                    line: 2,
                },
                BlendTableOperation::Load {
                    table: 201,
                    file: "multiply.raw".into(),
                    offset: 65536,
                    line: 3,
                },
                BlendTableOperation::Undef {
                    table: 201,
                    line: 4,
                },
            ]
        );
    }

    #[test]
    fn rejects_malformed_blend_table_resources_and_ids() {
        let missing_file = DefDocument::parse("blendtable 201 { raw { offset 10 } }").unwrap();
        assert_eq!(
            missing_file.blend_table_operations(),
            Err(BuildError::Invalid("DEF blendtable raw file"))
        );
        let invalid_offset =
            DefDocument::parse("blendtable 201 { raw { file table.raw offset -1 } }").unwrap();
        assert_eq!(
            invalid_offset.blend_table_operations(),
            Err(BuildError::Invalid("DEF blendtable raw offset"))
        );
        let reserved_id = DefDocument::parse("blendtable 0 { copy 0 }").unwrap();
        assert_eq!(
            reserved_id.blend_table_operations(),
            Err(BuildError::Invalid("DEF blendtable id"))
        );
        let conflicting =
            DefDocument::parse("blendtable 201 { raw { file one.raw } copy 0 }").unwrap();
        assert_eq!(
            conflicting.blend_table_operations(),
            Err(BuildError::Invalid("DEF blendtable operation"))
        );
    }

    #[test]
    fn rejects_unbalanced_or_malformed_def() {
        assert!(DefDocument::parse("tilefromtexture 1 { file x\n").is_err());
        assert!(DefDocument::parse("}\n").is_err());
        assert!(
            DefDocument::parse("tilefromtexture 1 { file x xoff }\n")
                .unwrap()
                .high_res_tiles()
                .is_err()
        );
    }

    #[test]
    fn accepts_brace_on_the_following_line() {
        let doc = DefDocument::parse("blendtable 200\n{\ncopy 0\n}\n").unwrap();
        assert_eq!(doc.directives[0].keyword, "blendtable");
        assert_eq!(doc.directives[0].args, ["200"]);
        assert_eq!(doc.directives[0].children[0].keyword, "copy");
    }

    #[test]
    fn imports_native_size_bmp_with_palette_and_offsets() {
        let document = DefDocument::parse(
            "tilefromtexture 9000 { file \"resources\\\\tile.bmp\" xoff 3 yoff -2 }\n",
        )
        .unwrap();
        let bmp = bmp_2x1_red_green();
        let assets = [("resources/tile.bmp", bmp.as_slice())];
        let mut palette = Palette {
            rgb: [0; 768],
            shades: Vec::new(),
            numshades: 1,
            translucency: None,
        };
        palette.rgb[3..6].copy_from_slice(&[255, 0, 0]);
        palette.rgb[6..9].copy_from_slice(&[0, 255, 0]);

        let imported = document.import_high_res_tiles(&assets, &palette).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].tile, 9000);
        assert_eq!((imported[0].width, imported[0].height), (2, 1));
        assert_eq!(imported[0].pixels, [1, 2]);
        assert_eq!((imported[0].x_offset, imported[0].y_offset), (3, -2));
    }

    #[test]
    fn parses_copy_and_dummy_tile_operations() {
        let document = DefDocument::parse(
            "copytile 8531 { tile 1031 }\ndummytile 9803 1 1\ndummytilerange 9693 9695 2 3\nanimtilerange 10 12 3 2\n",
        )
        .unwrap();
        assert_eq!(
            document.tile_operations().unwrap(),
            [
                TileOperation::Copy {
                    destination: 8531,
                    source: 1031,
                },
                TileOperation::Dummy {
                    tile: 9803,
                    width: 1,
                    height: 1,
                },
                TileOperation::DummyRange {
                    start: 9693,
                    end: 9695,
                    width: 2,
                    height: 3,
                },
                TileOperation::AnimationRange {
                    start: 10,
                    end: 12,
                    speed: 3,
                    animation: 2,
                },
            ]
        );
    }

    #[test]
    fn reads_alphacut_as_transparency_threshold() {
        let document =
            DefDocument::parse("tilefromtexture 9000 { file \"tile.png\" alphacut 0.5 }\n")
                .unwrap();
        assert_eq!(
            document.high_res_tiles().unwrap()[0].opaque_alpha_threshold,
            128
        );
    }

    fn bmp_2x1_red_green() -> Vec<u8> {
        let mut bmp = Vec::new();
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&62u32.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes());
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&2i32.to_le_bytes());
        bmp.extend_from_slice(&(-1i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes());
        bmp.extend_from_slice(&8u32.to_le_bytes());
        bmp.extend_from_slice(&[0; 16]);
        bmp.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
        bmp
    }
}

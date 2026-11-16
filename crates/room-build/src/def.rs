//! EDuke32 DEF resource declarations.
//!
//! This module deliberately parses declarations into a small syntax tree and
//! leaves archive lookup and renderer policy to the caller. It supports nested
//! definition blocks, quoted paths, line and block comments, and the tile
//! replacement directives used by Duke mods.

use crate::error::BuildError;
use crate::palette::Palette;
use room_formats::image::ImagePixels;
use std::collections::BTreeMap;

const MAX_HIGH_RES_TILE_PIXELS: usize = 16 * 1024 * 1024;
const MAX_DUMMY_RANGE_PIXELS: usize = 64 * 1024 * 1024;
const MAX_DEF_MUSIC_TRACKS: usize = 4096;

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

/// A high-resolution tile file referenced by `tilefromtexture` or by the
/// palette-zero branch of a `texture` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighResTileSpec {
    /// Build tile number replaced by this image.
    pub tile: u16,
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

/// A decoded high-resolution replacement ready to insert into the indexed
/// Build tile set. Pixels use Build's column-major layout; palette entries
/// are selected from the supplied GRP palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedTile {
    /// Build tile number being replaced.
    pub tile: i16,
    /// Native image width.
    pub width: u16,
    /// Native image height.
    pub height: u16,
    /// Column-major palette indices (`255` is transparent).
    pub pixels: Vec<u8>,
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

    /// Extract image replacements from `tilefromtexture` and base-palette
    /// (`texture ... pal 0`) declarations.
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
    /// true-colour policy. The indexed desktop path currently imports only
    /// palette 0 through [`Self::high_res_tiles`].
    pub fn texture_variants(&self) -> Result<Vec<DefTextureVariant>, BuildError> {
        let mut variants = Vec::new();
        collect_texture_variants(&self.directives, &mut variants)?;
        Ok(variants)
    }

    /// Decode and palette-map all `tilefromtexture` assets in this document.
    /// Later declarations for one tile replace earlier ones.
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
                    for y in 0..usize::from(height) {
                        for x in 0..usize::from(width) {
                            let source = (y * usize::from(width) + x) * 4;
                            let destination = x * usize::from(height) + y;
                            column_major[destination] =
                                if pixels.pixels[source + 3] < spec.opaque_alpha_threshold {
                                    255
                                } else {
                                    palette.nearest([
                                        u32::from(pixels.pixels[source]),
                                        u32::from(pixels.pixels[source + 1]),
                                        u32::from(pixels.pixels[source + 2]),
                                    ])
                                };
                        }
                    }
                }
            }
            imported.insert(
                i16::try_from(spec.tile).map_err(|_| BuildError::Invalid("DEF tile number"))?,
                ImportedTile {
                    tile: i16::try_from(spec.tile)
                        .map_err(|_| BuildError::Invalid("DEF tile number"))?,
                    width,
                    height,
                    pixels: column_major,
                    x_offset,
                    y_offset,
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
                line: directive.line,
            });
        } else if directive.keyword == "texture" {
            // EDuke32 falls back to palette 0 for palette banks without an
            // explicit branch. The classic indexed renderer stores one tile
            // image, so import that base branch and leave other pal variants
            // to renderers with per-palette texture storage.
            for variant in parse_texture_variants(directive)? {
                if variant.palette == 0 {
                    out.push(HighResTileSpec {
                        tile: variant.tile,
                        file: variant.file,
                        x_offset: 0,
                        y_offset: 0,
                        opaque_alpha_threshold: variant.opaque_alpha_threshold,
                        line: variant.line,
                    });
                }
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
                    file: "sprites\\9020.png".into(),
                    x_offset: 8,
                    y_offset: -3,
                    opaque_alpha_threshold: 174,
                    line: 3,
                },
                HighResTileSpec {
                    tile: 100,
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
            [HighResTileSpec {
                tile: 3586,
                file: "base.png".into(),
                x_offset: 0,
                y_offset: 0,
                opaque_alpha_threshold: 174,
                line: 1,
            }]
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
    fn texture_without_palette_zero_is_not_an_indexed_replacement() {
        let document = DefDocument::parse(r#"texture 3586 { pal 21 { file "red.png" } }"#).unwrap();
        assert!(document.high_res_tiles().unwrap().is_empty());
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
    fn rejects_duplicate_music_file_children() {
        let doc = DefDocument::parse("Music { ID E1L1 file first.ogg file second.ogg }").unwrap();
        assert_eq!(
            doc.music_tracks(),
            Err(BuildError::Invalid("DEF music ID/file child"))
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

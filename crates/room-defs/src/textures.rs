//! TEXTURES text lump + binary PNAMES / TEXTURE1 helpers.

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// How a TEXTURES definition is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureUse {
    /// Generic / wall texture.
    Texture,
    /// Sprite.
    Sprite,
    /// Graphic.
    Graphic,
    /// Wall texture.
    WallTexture,
    /// Flat.
    Flat,
}

/// One patch placement inside a composed texture.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchRef {
    /// Patch / graphic lump name.
    pub name: String,
    /// X offset.
    pub x: i32,
    /// Y offset.
    pub y: i32,
}

/// A TEXTURES definition.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureDef {
    /// Use type.
    pub use_type: TextureUse,
    /// Name.
    pub name: String,
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
    /// Optional flag.
    pub optional: bool,
    /// Patches.
    pub patches: Vec<PatchRef>,
    /// X scale (default 1.0).
    pub x_scale: f64,
    /// Y scale.
    pub y_scale: f64,
}

/// Texture registry from TEXTURES (+ later TEXTUREx).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureRegistry {
    /// `(usetype, uppercase name) → def` (last wins).
    pub by_name: BTreeMap<(TextureUse, String), TextureDef>,
}

/// Parse a TEXTURES lump (subset).
pub fn parse_textures(text: &str) -> Result<TextureRegistry, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            ..Default::default()
        },
    );
    let mut reg = TextureRegistry::default();

    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(kw) => {
                let use_type = match kw.to_ascii_lowercase().as_str() {
                    "texture" => TextureUse::Texture,
                    "sprite" => TextureUse::Sprite,
                    "graphic" => TextureUse::Graphic,
                    "walltexture" => TextureUse::WallTexture,
                    "flat" => TextureUse::Flat,
                    "define" => {
                        // Define "Name" width height
                        let name = read_name(&mut sc)?;
                        let w = sc.must_get_number()? as i32;
                        let h = sc.must_get_number()? as i32;
                        let def = TextureDef {
                            use_type: TextureUse::Texture,
                            name: name.to_ascii_uppercase(),
                            width: w,
                            height: h,
                            optional: false,
                            patches: Vec::new(),
                            x_scale: 1.0,
                            y_scale: 1.0,
                        };
                        reg.by_name
                            .insert((TextureUse::Texture, def.name.clone()), def);
                        continue;
                    }
                    _ => continue,
                };
                let mut optional = false;
                let mut name_tok = sc.next_token()?;
                if let Tok::Ident(s) = &name_tok {
                    if Scanner::compare(s, "optional") {
                        optional = true;
                        name_tok = sc.next_token()?;
                    }
                }
                let name = match name_tok {
                    Tok::Ident(s) => s.to_owned(),
                    Tok::String(s) => s.into_owned(),
                    other => {
                        return Err(DefsError::Message(format!(
                            "TEXTURES: expected name, got {other:?}"
                        )));
                    }
                };
                let _ = sc.check_punct(Punct::Comma)?;
                let width = sc.must_get_number()? as i32;
                let _ = sc.check_punct(Punct::Comma)?;
                let height = sc.must_get_number()? as i32;
                let mut def = TextureDef {
                    use_type,
                    name: name.to_ascii_uppercase(),
                    width,
                    height,
                    optional,
                    patches: Vec::new(),
                    x_scale: 1.0,
                    y_scale: 1.0,
                };
                if sc.check_punct(Punct::LBrace)? {
                    parse_texture_body(&mut sc, &mut def)?;
                }
                reg.by_name.insert((use_type, def.name.clone()), def);
            }
            _ => {}
        }
    }
    Ok(reg)
}

fn parse_texture_body(sc: &mut Scanner<'_>, def: &mut TextureDef) -> Result<(), DefsError> {
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => return Ok(()),
            Tok::Ident(kw) => {
                let k = kw.to_ascii_lowercase();
                match k.as_str() {
                    "patch" | "graphic" | "sprite" => {
                        let name = read_name(sc)?;
                        let _ = sc.check_punct(Punct::Comma)?;
                        let x = sc.must_get_number()? as i32;
                        let _ = sc.check_punct(Punct::Comma)?;
                        let y = sc.must_get_number()? as i32;
                        if sc.check_punct(Punct::LBrace)? {
                            // skip patch options block
                            let mut depth = 1;
                            while depth > 0 {
                                match sc.next_token()? {
                                    Tok::Punct(Punct::LBrace) => depth += 1,
                                    Tok::Punct(Punct::RBrace) => depth -= 1,
                                    Tok::Eof => break,
                                    _ => {}
                                }
                            }
                        }
                        def.patches.push(PatchRef {
                            name: name.to_ascii_uppercase(),
                            x,
                            y,
                        });
                    }
                    "xscale" => {
                        let _ = sc.check_punct(Punct::Eq)?;
                        def.x_scale = match sc.next_token()? {
                            Tok::Float(f) => f,
                            Tok::Int(n) => n as f64,
                            other => {
                                return Err(DefsError::Message(format!(
                                    "TEXTURES: bad XScale {other:?}"
                                )));
                            }
                        };
                        let _ = sc.check_punct(Punct::Semi)?;
                    }
                    "yscale" => {
                        let _ = sc.check_punct(Punct::Eq)?;
                        def.y_scale = match sc.next_token()? {
                            Tok::Float(f) => f,
                            Tok::Int(n) => n as f64,
                            other => {
                                return Err(DefsError::Message(format!(
                                    "TEXTURES: bad YScale {other:?}"
                                )));
                            }
                        };
                        let _ = sc.check_punct(Punct::Semi)?;
                    }
                    "offset" | "worldpanning" | "nodecals" | "nulltexture" | "notrim" => {
                        // skip until `;` or swallow bare keyword
                        loop {
                            match sc.next_token()? {
                                Tok::Punct(Punct::Semi) | Tok::Eof => break,
                                Tok::Punct(Punct::LBrace) => {
                                    let mut d = 1;
                                    while d > 0 {
                                        match sc.next_token()? {
                                            Tok::Punct(Punct::LBrace) => d += 1,
                                            Tok::Punct(Punct::RBrace) => d -= 1,
                                            Tok::Eof => break,
                                            _ => {}
                                        }
                                    }
                                    break;
                                }
                                Tok::Ident(_) | Tok::Punct(Punct::RBrace) => {
                                    // put back rbrace
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {
                        let _ = sc.check_punct(Punct::Eq)?;
                        let _ = sc.next_token()?;
                        let _ = sc.check_punct(Punct::Semi)?;
                    }
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "TEXTURES body: unexpected {other:?}"
                )));
            }
        }
    }
}

fn read_name(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        other => Err(DefsError::Message(format!(
            "TEXTURES: expected name, got {other:?}"
        ))),
    }
}

/// Merge Doom/Strife `TEXTURE1`/`TEXTURE2` records into a registry.
///
/// Patch indices are resolved through `pnames`. Missing indices get a
/// synthetic `PNAMEnnnn` name and are still recorded.
pub fn merge_texturex(
    reg: &mut TextureRegistry,
    defs: &[room_formats::TexturexDef],
    pnames: &[String],
) {
    for d in defs {
        let patches = d
            .patches
            .iter()
            .map(|p| {
                let name = pnames
                    .get(p.pnames_index as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("PNAME{:04}", p.pnames_index));
                PatchRef {
                    name,
                    x: i32::from(p.origin_x),
                    y: i32::from(p.origin_y),
                }
            })
            .collect();
        let x_scale = if d.scale_x == 0 {
            1.0
        } else {
            f64::from(d.scale_x) / 8.0
        };
        let y_scale = if d.scale_y == 0 {
            1.0
        } else {
            f64::from(d.scale_y) / 8.0
        };
        let key = (TextureUse::WallTexture, d.name.clone());
        reg.by_name.insert(
            key,
            TextureDef {
                use_type: TextureUse::WallTexture,
                name: d.name.clone(),
                width: i32::from(d.width),
                height: i32::from(d.height),
                optional: false,
                patches,
                x_scale,
                y_scale,
            },
        );
    }
}

/// Parse binary PNAMES lump → list of patch names.
pub fn parse_pnames(data: &[u8]) -> Result<Vec<String>, DefsError> {
    if data.len() < 4 {
        return Err(DefsError::Message("PNAMES too short".into()));
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let need = 4 + count * 8;
    if data.len() < need {
        return Err(DefsError::Message("PNAMES truncated".into()));
    }
    let mut names = Vec::with_capacity(count);
    for i in 0..count {
        let start = 4 + i * 8;
        let raw = &data[start..start + 8];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(8);
        let s = std::str::from_utf8(&raw[..end])
            .map_err(|_| DefsError::Message("PNAMES non-utf8".into()))?
            .to_ascii_uppercase();
        names.push(s);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn textures_and_pnames() {
        let text = r#"
Texture "BRICK1", 128, 128
{
  Patch "BRICKA", 0, 0
  XScale 1.0
}
"#;
        let reg = parse_textures(text).unwrap();
        let def = reg
            .by_name
            .get(&(TextureUse::Texture, "BRICK1".into()))
            .unwrap();
        assert_eq!(def.width, 128);
        assert_eq!(def.patches[0].name, "BRICKA");

        let mut pnames = Vec::new();
        pnames.extend_from_slice(&1u32.to_le_bytes());
        pnames.extend_from_slice(b"BRICKA\0\0");
        let names = parse_pnames(&pnames).unwrap();
        assert_eq!(names, vec!["BRICKA"]);
    }
}

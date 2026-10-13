//! MODELDEF parser (M10 bootstrap — defs only; mesh load later).

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One sprite-frame → model-frame binding (`FrameIndex` / `Frame`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelFrameBind {
    /// Sprite name (4 chars, uppercase).
    pub sprite: String,
    /// Frame letter (`A`..=`Z`).
    pub frame: char,
    /// Model slot index (`Model N …`).
    pub model_index: u32,
    /// Numeric keyframe (`FrameIndex`) when set.
    pub keyframe: Option<u32>,
    /// Named MD2 frame (`Frame … "name"`) when set.
    pub frame_name: Option<String>,
}

/// One MODELDEF `Model ActorName { … }` block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelDef {
    /// Actor class name.
    pub actor: String,
    /// Base path for model/skin files.
    pub path: String,
    /// Model files by index.
    pub models: BTreeMap<u32, String>,
    /// Skins by model index.
    pub skins: BTreeMap<u32, String>,
    /// Surface skins `(model_index, surface_index)` → path.
    pub surface_skins: BTreeMap<(u32, u32), String>,
    /// XYZ scale (default 1,1,1).
    pub scale: [f32; 3],
    /// XYZ offset.
    pub offset: [f32; 3],
    /// Z-only offset (`ZOffset`).
    pub z_offset: f32,
    /// Angle offset (degrees).
    pub angle_offset: f32,
    /// Pitch offset (degrees).
    pub pitch_offset: f32,
    /// Roll offset (degrees).
    pub roll_offset: f32,
    /// Flag names (uppercase) collected from the block.
    pub flags: Vec<String>,
    /// Sprite frame bindings.
    pub frames: Vec<ModelFrameBind>,
}

/// Compiled MODELDEF registry (later model for same actor wins).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelDefs {
    /// Actor class (case-insensitive key) → definition.
    pub by_actor: BTreeMap<String, ModelDef>,
}

/// Parse a MODELDEF lump.
pub fn parse_modeldef(text: &str) -> Result<ModelDefs, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
        },
    );
    let mut out = ModelDefs::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) if kw.eq_ignore_ascii_case("model") => {
                let def = parse_model_block(&mut sc)?;
                out.by_actor.insert(def.actor.to_ascii_lowercase(), def);
            }
            Tok::Ident(_) => {
                // Skip unknown top-level (e.g. future keywords).
                if sc.check_punct(Punct::LBrace)? {
                    skip_block(&mut sc)?;
                } else {
                    loop {
                        match sc.next_token()? {
                            Tok::Punct(Punct::Semi) | Tok::Eof => break,
                            Tok::Punct(Punct::LBrace) => {
                                skip_block(&mut sc)?;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            other => {
                return Err(DefsError::Message(format!(
                    "MODELDEF: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(out)
}

fn parse_model_block(sc: &mut Scanner<'_>) -> Result<ModelDef, DefsError> {
    let actor = read_name(sc)?;
    expect_lbrace(sc)?;
    let mut def = ModelDef {
        actor,
        scale: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(key) => {
                let k = key.to_ascii_lowercase();
                match k.as_str() {
                    "path" => def.path = read_name(sc)?,
                    "model" => {
                        let idx = read_u32(sc)?;
                        let file = read_name(sc)?;
                        def.models.insert(idx, file);
                    }
                    "skin" => {
                        let idx = read_u32(sc)?;
                        let file = read_name(sc)?;
                        def.skins.insert(idx, file);
                    }
                    "surfaceskin" => {
                        let mi = read_u32(sc)?;
                        let si = read_u32(sc)?;
                        let file = read_name(sc)?;
                        def.surface_skins.insert((mi, si), file);
                    }
                    "scale" => {
                        def.scale = [read_f32(sc)?, read_f32(sc)?, read_f32(sc)?];
                    }
                    "offset" => {
                        def.offset = [read_f32(sc)?, read_f32(sc)?, read_f32(sc)?];
                    }
                    "zoffset" => def.z_offset = read_f32(sc)?,
                    "angleoffset" => def.angle_offset = read_f32(sc)?,
                    "pitchoffset" => def.pitch_offset = read_f32(sc)?,
                    "rolloffset" => def.roll_offset = read_f32(sc)?,
                    "frameindex" => {
                        let sprite = read_name(sc)?.to_ascii_uppercase();
                        let frame = read_frame_letter(sc)?;
                        let model_index = read_u32(sc)?;
                        let keyframe = read_u32(sc)?;
                        def.frames.push(ModelFrameBind {
                            sprite,
                            frame,
                            model_index,
                            keyframe: Some(keyframe),
                            frame_name: None,
                        });
                    }
                    "frame" => {
                        let sprite = read_name(sc)?.to_ascii_uppercase();
                        let frame = read_frame_letter(sc)?;
                        let model_index = read_u32(sc)?;
                        let name = read_name(sc)?;
                        def.frames.push(ModelFrameBind {
                            sprite,
                            frame,
                            model_index,
                            keyframe: None,
                            frame_name: Some(name),
                        });
                    }
                    // Common flags — store name, no value.
                    "pitchfrommomentum"
                    | "ignoretranslation"
                    | "interpolatedoubledframes"
                    | "nointerpolation"
                    | "useactorpitch"
                    | "useactorroll"
                    | "inheritactorpitch"
                    | "inheritactorroll"
                    | "rotating"
                    | "dontcullbackfaces"
                    | "userotationcenter"
                    | "correctpixelstretch"
                    | "scaleweaponfov"
                    | "baseframe" => {
                        def.flags.push(k.to_ascii_uppercase());
                    }
                    _ => {
                        // Skip unknown property values until `;` / `}`.
                        loop {
                            match sc.next_token()? {
                                Tok::Punct(Punct::Semi) | Tok::Eof => break,
                                Tok::Punct(Punct::RBrace) => return Ok(def),
                                _ => {}
                            }
                        }
                        continue;
                    }
                }
                let _ = sc.check_punct(Punct::Semi)?;
            }
            other => {
                return Err(DefsError::Message(format!(
                    "MODELDEF body: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(def)
}

fn expect_lbrace(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    match sc.next_token()? {
        Tok::Punct(Punct::LBrace) => Ok(()),
        other => Err(DefsError::Message(format!(
            "MODELDEF: expected `{{`, got {other:?}"
        ))),
    }
}

fn read_name(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        other => Err(DefsError::Message(format!(
            "MODELDEF: expected name, got {other:?}"
        ))),
    }
}

fn read_frame_letter(sc: &mut Scanner<'_>) -> Result<char, DefsError> {
    let s = read_name(sc)?;
    let ch = s.chars().next().unwrap_or('A');
    Ok(ch.to_ascii_uppercase())
}

fn read_u32(sc: &mut Scanner<'_>) -> Result<u32, DefsError> {
    match sc.next_token()? {
        Tok::Int(n) if n >= 0 => Ok(n as u32),
        Tok::Float(f) if f >= 0.0 => Ok(f as u32),
        other => Err(DefsError::Message(format!(
            "MODELDEF: expected u32, got {other:?}"
        ))),
    }
}

fn read_f32(sc: &mut Scanner<'_>) -> Result<f32, DefsError> {
    match sc.next_token()? {
        Tok::Float(f) => Ok(f as f32),
        Tok::Int(n) => Ok(n as f32),
        Tok::Ident(s) => s
            .parse()
            .map_err(|_| DefsError::Message(format!("MODELDEF: bad number {s}"))),
        other => Err(DefsError::Message(format!(
            "MODELDEF: expected number, got {other:?}"
        ))),
    }
}

fn skip_block(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    let mut depth = 1i32;
    while depth > 0 {
        match sc.next_token()? {
            Tok::Punct(Punct::LBrace) => depth += 1,
            Tok::Punct(Punct::RBrace) => depth -= 1,
            Tok::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_frameindex_model() {
        let text = r#"
Model ImpModel
{
	Path "models/imp"
	Model 0 "imp.md3"
	Skin 0 "imp.png"
	Scale 1.0 1.0 1.0
	FrameIndex TROO A 0 0
	FrameIndex TROO B 0 1
	PITCHFROMMOMENTUM
}
"#;
        let defs = parse_modeldef(text).unwrap();
        let m = defs.by_actor.get("impmodel").unwrap();
        assert_eq!(m.path, "models/imp");
        assert_eq!(m.models.get(&0).map(String::as_str), Some("imp.md3"));
        assert_eq!(m.skins.get(&0).map(String::as_str), Some("imp.png"));
        assert_eq!(m.frames.len(), 2);
        assert_eq!(m.frames[0].sprite, "TROO");
        assert_eq!(m.frames[0].frame, 'A');
        assert_eq!(m.frames[0].keyframe, Some(0));
        assert!(m.flags.iter().any(|f| f == "PITCHFROMMOMENTUM"));
    }

    #[test]
    fn parses_named_frame() {
        let text = r#"
Model Rocket
{
	Model 0 "rocket.md2"
	Frame MISL A 0 "spawn"
}
"#;
        let defs = parse_modeldef(text).unwrap();
        let m = defs.by_actor.get("rocket").unwrap();
        assert_eq!(m.frames[0].frame_name.as_deref(), Some("spawn"));
        assert!(m.frames[0].keyframe.is_none());
    }
}

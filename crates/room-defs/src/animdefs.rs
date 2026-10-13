//! ANIMDEFS parser (non-C mode).

use crate::error::DefsError;
use room_sc::{ScanOptions, Scanner, Tok};

/// One animation frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimPic {
    /// Pic name or number as string.
    pub name: String,
    /// Fixed tics, if not random.
    pub tics: Option<i64>,
    /// Random tics range.
    pub rand: Option<(i64, i64)>,
}

/// Flat/texture animation definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimDef {
    /// `flat` or `texture`.
    pub kind: String,
    /// Base name.
    pub name: String,
    /// Optional flag.
    pub optional: bool,
    /// Oscillate flag.
    pub oscillate: bool,
    /// Allow decals.
    pub allow_decals: bool,
    /// Pics in order.
    pub pics: Vec<AnimPic>,
}

/// Switch animation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchDef {
    /// Game filter (`doom`, …) if any.
    pub game: Option<String>,
    /// Switch texture name.
    pub name: String,
    /// On sequence pic names.
    pub on_pics: Vec<String>,
    /// Off sequence pic names.
    pub off_pics: Vec<String>,
}

/// Warp effect.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpDef {
    /// `warp` or `warp2`.
    pub kind: String,
    /// Texture name.
    pub name: String,
    /// Speed.
    pub speed: Option<f64>,
}

/// `cameratexture` canvas (bound later via ACS `SetCameraToTexture`).
#[derive(Clone, Debug, PartialEq)]
pub struct CameraTextureDef {
    /// Texture name.
    pub name: String,
    /// Internal width.
    pub width: i32,
    /// Internal height.
    pub height: i32,
    /// Display fit width (defaults to `width`).
    pub fit_width: f64,
    /// Display fit height (defaults to `height`).
    pub fit_height: f64,
    /// HDR canvas.
    pub hdr: bool,
    /// World-unit panning.
    pub world_panning: bool,
}

/// Parsed ANIMDEFS.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimDefs {
    /// Flat/texture anims.
    pub anims: Vec<AnimDef>,
    /// Switches.
    pub switches: Vec<SwitchDef>,
    /// Warps.
    pub warps: Vec<WarpDef>,
    /// Camera textures.
    pub camera_textures: Vec<CameraTextureDef>,
    /// Warnings.
    pub warnings: Vec<String>,
}

/// Parse ANIMDEFS text.
pub fn parse_animdefs(text: &str) -> Result<AnimDefs, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: false,
            semicolon_comments: true,
        },
    );
    let mut out = AnimDefs::default();

    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(kw) => {
                let k = kw.to_ascii_lowercase();
                match k.as_str() {
                    "flat" | "texture" => {
                        let mut def = AnimDef {
                            kind: k,
                            name: String::new(),
                            optional: false,
                            oscillate: false,
                            allow_decals: false,
                            pics: Vec::new(),
                        };
                        loop {
                            match sc.next_token()? {
                                Tok::Ident(s) if Scanner::compare(s, "optional") => {
                                    def.optional = true;
                                }
                                Tok::Ident(s) if Scanner::compare(s, "allowdecals") => {
                                    def.allow_decals = true;
                                }
                                Tok::Ident(s) if Scanner::compare(s, "oscillate") => {
                                    def.oscillate = true;
                                }
                                Tok::Ident(s) if Scanner::compare(s, "pic") => {
                                    sc.unget(Tok::Ident(s));
                                    break;
                                }
                                Tok::Ident(s) | Tok::String(std::borrow::Cow::Borrowed(s)) => {
                                    if def.name.is_empty() {
                                        def.name = s.to_ascii_uppercase();
                                    } else {
                                        sc.unget(Tok::Ident(s));
                                        break;
                                    }
                                }
                                Tok::String(std::borrow::Cow::Owned(s)) => {
                                    if def.name.is_empty() {
                                        def.name = s.to_ascii_uppercase();
                                    } else {
                                        sc.unget(Tok::String(std::borrow::Cow::Owned(s)));
                                        break;
                                    }
                                }
                                other => {
                                    sc.unget(other);
                                    break;
                                }
                            }
                        }
                        // If we broke on `pic`, re-read pics
                        parse_anim_pics(&mut sc, &mut def)?;
                        out.anims.push(def);
                    }
                    "warp" | "warp2" => {
                        let name = read_word(&mut sc)?;
                        let mut speed = None;
                        let peek = sc.next_token()?;
                        match peek {
                            Tok::Float(f) => speed = Some(f),
                            Tok::Int(n) => speed = Some(n as f64),
                            Tok::Ident(s) if Scanner::compare(s, "speed") => {
                                speed = match sc.next_token()? {
                                    Tok::Float(f) => Some(f),
                                    Tok::Int(n) => Some(n as f64),
                                    _ => None,
                                };
                            }
                            Tok::Ident(s) if Scanner::compare(s, "allowdecals") => {}
                            other => sc.unget(other),
                        }
                        out.warps.push(WarpDef {
                            kind: k,
                            name: name.to_ascii_uppercase(),
                            speed,
                        });
                    }
                    "cameratexture" => {
                        let name = read_word(&mut sc)?.to_ascii_uppercase();
                        let width = sc.must_get_number()? as i32;
                        let height = sc.must_get_number()? as i32;
                        let mut def = CameraTextureDef {
                            name,
                            width,
                            height,
                            fit_width: f64::from(width),
                            fit_height: f64::from(height),
                            hdr: false,
                            world_panning: false,
                        };
                        loop {
                            match sc.next_token()? {
                                Tok::Ident(s) if Scanner::compare(s, "hdr") => def.hdr = true,
                                Tok::Ident(s) if Scanner::compare(s, "fit") => {
                                    def.fit_width = sc.must_get_number()? as f64;
                                    def.fit_height = sc.must_get_number()? as f64;
                                }
                                Tok::Ident(s) if Scanner::compare(s, "worldpanning") => {
                                    def.world_panning = true;
                                }
                                Tok::Ident(s)
                                    if matches!(
                                        s.to_ascii_lowercase().as_str(),
                                        "flat"
                                            | "texture"
                                            | "warp"
                                            | "warp2"
                                            | "switch"
                                            | "cameratexture"
                                            | "animateddoor"
                                    ) =>
                                {
                                    sc.unget(Tok::Ident(s));
                                    break;
                                }
                                Tok::Eof => break,
                                other => {
                                    sc.unget(other);
                                    break;
                                }
                            }
                        }
                        out.camera_textures.push(def);
                    }
                    "switch" => {
                        let mut game = None;
                        let mut name_tok = sc.next_token()?;
                        if let Tok::Ident(s) = &name_tok {
                            let low = s.to_ascii_lowercase();
                            if matches!(
                                low.as_str(),
                                "doom" | "heretic" | "hexen" | "strife" | "chex"
                            ) {
                                game = Some(low);
                                name_tok = sc.next_token()?;
                            }
                        }
                        let name = match name_tok {
                            Tok::Ident(s) => s.to_ascii_uppercase(),
                            Tok::String(s) => s.into_owned().to_ascii_uppercase(),
                            other => {
                                return Err(DefsError::Message(format!(
                                    "ANIMDEFS switch: bad name {other:?}"
                                )));
                            }
                        };
                        let mut on_pics = Vec::new();
                        let mut off_pics = Vec::new();
                        let mut on_phase = true;
                        loop {
                            match sc.next_token()? {
                                Tok::Eof => break,
                                Tok::Ident(s) if Scanner::compare(s, "on") => on_phase = true,
                                Tok::Ident(s) if Scanner::compare(s, "off") => on_phase = false,
                                Tok::Ident(s) if Scanner::compare(s, "pic") => {
                                    let pic = read_word(&mut sc)?;
                                    match sc.next_token()? {
                                        Tok::Ident(x) if Scanner::compare(x, "tics") => {
                                            let _ = sc.must_get_number()?;
                                        }
                                        other => sc.unget(other),
                                    }
                                    if on_phase {
                                        on_pics.push(pic.to_ascii_uppercase());
                                    } else {
                                        off_pics.push(pic.to_ascii_uppercase());
                                    }
                                }
                                Tok::Ident(s)
                                    if matches!(
                                        s.to_ascii_lowercase().as_str(),
                                        "flat"
                                            | "texture"
                                            | "warp"
                                            | "warp2"
                                            | "switch"
                                            | "cameratexture"
                                            | "animateddoor"
                                    ) =>
                                {
                                    sc.unget(Tok::Ident(s));
                                    break;
                                }
                                Tok::Ident(s) if Scanner::compare(s, "sound") => {
                                    let _ = read_word(&mut sc)?;
                                }
                                _ => {}
                            }
                        }
                        out.switches.push(SwitchDef {
                            game,
                            name,
                            on_pics,
                            off_pics,
                        });
                    }
                    other => {
                        out.warnings.push(format!("ANIMDEFS: unsupported {other}"));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn parse_anim_pics(sc: &mut Scanner<'_>, def: &mut AnimDef) -> Result<(), DefsError> {
    // May already be positioned before first pic or need to read pic keywords.
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Ident(s) if Scanner::compare(s, "pic") => {
                let name = read_word(sc)?;
                let mut tics = None;
                let mut rand = None;
                match sc.next_token()? {
                    Tok::Ident(t) if Scanner::compare(t, "tics") => {
                        tics = Some(sc.must_get_number()?);
                    }
                    Tok::Ident(t) if Scanner::compare(t, "rand") => {
                        let a = sc.must_get_number()?;
                        let b = sc.must_get_number()?;
                        rand = Some((a, b));
                    }
                    Tok::Int(n) => tics = Some(n),
                    other => sc.unget(other),
                }
                def.pics.push(AnimPic {
                    name: name.to_ascii_uppercase(),
                    tics,
                    rand,
                });
            }
            Tok::Ident(s)
                if matches!(
                    s.to_ascii_lowercase().as_str(),
                    "flat"
                        | "texture"
                        | "warp"
                        | "warp2"
                        | "switch"
                        | "cameratexture"
                        | "range"
                        | "animateddoor"
                ) =>
            {
                sc.unget(Tok::Ident(s));
                break;
            }
            Tok::Ident(s) if Scanner::compare(s, "range") => {
                let last = read_word(sc)?;
                def.pics.push(AnimPic {
                    name: format!("RANGE:{last}"),
                    tics: None,
                    rand: None,
                });
                let _ = sc.next_token(); // tics/rand…
            }
            other => {
                sc.unget(other);
                break;
            }
        }
    }
    Ok(())
}

fn read_word(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        other => Err(DefsError::Message(format!(
            "ANIMDEFS: expected word, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn flat_animation() {
        let text = r#"
flat NUKAGE1
    pic NUKAGE1 tics 8
    pic NUKAGE2 tics 8
    pic NUKAGE3 tics 8
texture BLOOD1 allowdecals
    pic BLOOD1 tics 8
    pic BLOOD2 tics 8
"#;
        let a = parse_animdefs(text).unwrap();
        assert_eq!(a.anims.len(), 2);
        assert_eq!(a.anims[0].name, "NUKAGE1");
        assert_eq!(a.anims[0].pics.len(), 3);
        assert!(a.anims[1].allow_decals);
    }

    #[test]
    fn camera_texture() {
        let text = r#"
cameratexture TCAMTEX1 128 64 fit 80 56
cameratexture HDRCAM 64 64 hdr fit 32 32 worldpanning
"#;
        let a = parse_animdefs(text).unwrap();
        assert_eq!(a.camera_textures.len(), 2);
        assert_eq!(a.camera_textures[0].name, "TCAMTEX1");
        assert_eq!(a.camera_textures[0].width, 128);
        assert_eq!(a.camera_textures[0].fit_width, 80.0);
        assert!(a.camera_textures[1].hdr);
        assert!(a.camera_textures[1].world_panning);
    }
}

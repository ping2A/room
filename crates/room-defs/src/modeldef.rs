//! MODELDEF parser (M10 bootstrap — defs only; mesh load later).

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
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
    /// First-person weapon FOV (`ViewModelFOV`). 0 means use the player FOV.
    pub view_model_fov: f32,
    /// Flag names (uppercase) collected from the block.
    pub flags: Vec<String>,
    /// Sprite frame bindings.
    pub frames: Vec<ModelFrameBind>,
    /// `Rotation-Speed` for `ROTATING` (turns per 200 tics; default 1).
    pub rotation_speed: f32,
    /// `Rotation-Vector`, in UZDoom's Y-up render axes (default `0 1 0`, up).
    pub rotation_vector: [f32; 3],
    /// `Rotation-Center` (model units).
    pub rotation_center: [f32; 3],
}

/// UZDoom `MDL_*` bits (`A_SetModelFlag` / MODELDEF keywords).
pub mod mdl {
    /// `IGNORETRANSLATION`.
    pub const IGNORETRANSLATION: u32 = 1;
    /// `PITCHFROMMOMENTUM`.
    pub const PITCHFROMMOMENTUM: u32 = 2;
    /// `ROTATING`.
    pub const ROTATING: u32 = 4;
    /// `INTERPOLATEDOUBLEDFRAMES`.
    pub const INTERPOLATEDOUBLEDFRAMES: u32 = 8;
    /// `NOINTERPOLATION`.
    pub const NOINTERPOLATION: u32 = 16;
    /// `USEACTORPITCH` (and `INHERITACTORPITCH`).
    pub const USEACTORPITCH: u32 = 32;
    /// `USEACTORROLL` (and `INHERITACTORROLL`).
    pub const USEACTORROLL: u32 = 64;
    /// `BADROTATION` (old `INHERITACTOR*` sign).
    pub const BADROTATION: u32 = 128;
    /// `DONTCULLBACKFACES`.
    pub const DONTCULLBACKFACES: u32 = 256;
    /// `USEROTATIONCENTER`.
    pub const USEROTATIONCENTER: u32 = 512;
    /// `NOPERPIXELLIGHTING`.
    pub const NOPERPIXELLIGHTING: u32 = 1024;
    /// `SCALEWEAPONFOV`.
    pub const SCALEWEAPONFOV: u32 = 2048;
    /// `CORRECTPIXELSTRETCH`.
    pub const CORRECTPIXELSTRETCH: u32 = 1 << 13;
    /// `FORCECULLBACKFACES`.
    pub const FORCECULLBACKFACES: u32 = 1 << 14;
}

impl ModelDef {
    /// The block's flags as `MDL_*` bits.
    #[must_use]
    pub fn mdl_flags(&self) -> u32 {
        self.flags
            .iter()
            .map(|f| match f.as_str() {
                "IGNORETRANSLATION" => mdl::IGNORETRANSLATION,
                "PITCHFROMMOMENTUM" => mdl::PITCHFROMMOMENTUM,
                "ROTATING" => mdl::ROTATING,
                "INTERPOLATEDOUBLEDFRAMES" => mdl::INTERPOLATEDOUBLEDFRAMES,
                "NOINTERPOLATION" => mdl::NOINTERPOLATION,
                "USEACTORPITCH" => mdl::USEACTORPITCH,
                "USEACTORROLL" => mdl::USEACTORROLL,
                // UZDoom: the INHERIT spellings keep the old sign.
                "INHERITACTORPITCH" => mdl::USEACTORPITCH | mdl::BADROTATION,
                "INHERITACTORROLL" => mdl::USEACTORROLL | mdl::BADROTATION,
                "DONTCULLBACKFACES" => mdl::DONTCULLBACKFACES,
                "USEROTATIONCENTER" => mdl::USEROTATIONCENTER,
                "NOPERPIXELLIGHTING" => mdl::NOPERPIXELLIGHTING,
                "SCALEWEAPONFOV" => mdl::SCALEWEAPONFOV,
                "CORRECTPIXELSTRETCH" => mdl::CORRECTPIXELSTRETCH,
                "FORCECULLBACKFACES" => mdl::FORCECULLBACKFACES,
                _ => 0,
            })
            .fold(0, |a, b| a | b)
    }
}

/// Compiled MODELDEF registry (later model for same actor wins).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelDefs {
    /// Actor class (case-insensitive key) → definition (the last block).
    pub by_actor: BTreeMap<String, ModelDef>,
    /// Actor class → every `Model` block for it, in load order. Mods often
    /// split one actor's frames over several blocks, each with its own
    /// models, skins and offsets.
    pub blocks: BTreeMap<String, Vec<ModelDef>>,
}

impl ModelDefs {
    /// Add a parsed block.
    pub fn insert(&mut self, def: ModelDef) {
        let key = def.actor.to_ascii_lowercase();
        self.blocks
            .entry(key.clone())
            .or_default()
            .push(def.clone());
        self.by_actor.insert(key, def);
    }

    /// Merge another lump's definitions (later blocks win frame lookups).
    pub fn extend(&mut self, other: ModelDefs) {
        for (k, v) in other.blocks {
            self.blocks.entry(k.clone()).or_default().extend(v);
        }
        for (k, v) in other.by_actor {
            self.by_actor.insert(k, v);
        }
    }

    /// UZDoom `FindModelFrame`: the block that binds `sprite` / `frame` for
    /// `class` (the last one when several do) and its bindings, one per model
    /// slot. `None` means the actor is drawn as a sprite in that frame.
    #[must_use]
    pub fn frame(
        &self,
        class: &str,
        sprite: &str,
        frame: char,
    ) -> Option<(&ModelDef, Vec<&ModelFrameBind>)> {
        let blocks = self.blocks.get(&class.to_ascii_lowercase())?;
        let frame = frame.to_ascii_uppercase();
        blocks.iter().rev().find_map(|def| {
            let binds: Vec<&ModelFrameBind> = def
                .frames
                .iter()
                .filter(|b| b.frame == frame && b.sprite.eq_ignore_ascii_case(sprite))
                .collect();
            (!binds.is_empty()).then_some((def, binds))
        })
    }
}

/// How much to enlarge a first-person weapon when `ViewModelFOV` is narrower
/// than the player FOV. 0 keeps the gun at 1×.
#[must_use]
pub fn view_model_weapon_scale(player_fov: f32, view_model_fov: f32) -> f32 {
    if view_model_fov <= 0.1 || player_fov <= 0.1 {
        1.0
    } else {
        (player_fov / view_model_fov).clamp(0.25, 4.0)
    }
}

/// Parse a MODELDEF lump.
pub fn parse_modeldef(text: &str) -> Result<ModelDefs, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
            ..Default::default()
        },
    );
    let mut out = ModelDefs::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) if kw.eq_ignore_ascii_case("model") => {
                let def = parse_model_block(&mut sc)?;
                out.insert(def);
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
        rotation_speed: 1.0,
        rotation_vector: [0.0, 1.0, 0.0],
        ..Default::default()
    };
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(key) => {
                let mut k = key.to_ascii_lowercase();
                // `Rotation-Speed` and friends may scan as `Rotation - Speed`.
                if k == "rotation" && sc.check_punct(Punct::Minus)? {
                    k = format!("rotation-{}", read_name(sc)?.to_ascii_lowercase());
                }
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
                    "viewmodelfov" => def.view_model_fov = read_f32(sc)?,
                    "rotation-speed" => def.rotation_speed = read_f32(sc)?,
                    "rotation-vector" => {
                        def.rotation_vector = [read_f32(sc)?, read_f32(sc)?, read_f32(sc)?];
                    }
                    "rotation-center" => {
                        def.rotation_center = [read_f32(sc)?, read_f32(sc)?, read_f32(sc)?];
                    }
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
                    | "noperpixellighting"
                    | "forcecullbackfaces"
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
    let neg = sc.check_punct(Punct::Minus)?;
    let v = match sc.next_token()? {
        Tok::Float(f) => f as f32,
        Tok::Int(n) => n as f32,
        Tok::Ident(s) => s
            .parse()
            .map_err(|_| DefsError::Message(format!("MODELDEF: bad number {s}")))?,
        other => {
            return Err(DefsError::Message(format!(
                "MODELDEF: expected number, got {other:?}"
            )));
        }
    };
    Ok(if neg { -v } else { v })
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
    fn blocks_merge_and_frames_resolve() {
        let text = r#"
Model Imp { Path "models/imp" Model 0 "walk.md3" Skin 0 "imp.png" FrameIndex TROO A 0 0 FrameIndex TROO B 0 1 }
Model Imp { Path "models/imp" Model 0 "attack.md3" Model 1 "fireball.md3" Rotating Rotation-Speed 2 FrameIndex TROO E 0 3 FrameIndex TROO E 1 0 }
"#;
        let defs = parse_modeldef(text).unwrap();
        assert_eq!(defs.blocks["imp"].len(), 2);
        let (def, binds) = defs.frame("IMP", "troo", 'b').unwrap();
        assert_eq!(def.models[&0], "walk.md3");
        assert_eq!(binds[0].keyframe, Some(1));
        let (def, binds) = defs.frame("imp", "TROO", 'E').unwrap();
        assert_eq!(binds.len(), 2);
        assert_eq!(def.models[&1], "fireball.md3");
        assert!((def.rotation_speed - 2.0).abs() < 1e-6);
        assert_ne!(def.mdl_flags() & mdl::ROTATING, 0);
        assert!(
            defs.frame("imp", "TROO", 'C').is_none(),
            "unbound frame draws the sprite"
        );
        let mut all = ModelDefs::default();
        all.extend(defs.clone());
        all.extend(defs);
        assert_eq!(all.blocks["imp"].len(), 4);
    }

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

    #[test]
    fn view_model_fov_half_the_player_fov_doubles_the_gun() {
        let text = r#"
Model Pistol
{
	ViewModelFOV 45
}
"#;
        let defs = parse_modeldef(text).unwrap();
        let m = defs.by_actor.get("pistol").unwrap();
        assert!((m.view_model_fov - 45.0).abs() < 1e-4);
        let scale = view_model_weapon_scale(90.0, m.view_model_fov);
        assert!((scale - 2.0).abs() < 1e-4);
        assert!((view_model_weapon_scale(90.0, 0.0) - 1.0).abs() < 1e-4);
    }
}

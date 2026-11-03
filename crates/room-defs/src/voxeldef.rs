//! VOXELDEF parser (GZDoom `voxeldef.cpp`).
//!
//! A block is one or more sprite names, `=`, a voxel file, then an optional
//! `{ … }` of display options. A 4-character name applies to every frame; a
//! 5-character name is sprite + frame (`POSSA`).

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One VOXELDEF entry (sprite name → voxel file and display options).
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelDef {
    /// Sprite key (uppercase). Four characters = every frame; five = one frame.
    pub name: String,
    /// Voxel file as written (`csawa`, `voxels/pinv_shell.kvx`).
    pub file: String,
    /// Uniform scale (default 1).
    pub scale: f32,
    /// Spin when placed and dropped (`Spin=` sets both). Degrees follow
    /// GZDoom's model turn rate (`spin / 55.55` turns per 200 tics).
    pub placed_spin: i32,
    /// Spin when the actor has `DROPPED`.
    pub dropped_spin: i32,
    /// Draw yaw in degrees. GZDoom's default is 90 (Build faces a different
    /// north); a lump `AngleOffset` is stored as `value + 90`.
    pub angle_offset: f32,
    /// Model offset in voxel units (`XOffset` / `YOffset` / `ZOffset`).
    pub offset: [f32; 3],
    /// `OverridePalette` — use the game `PLAYPAL` instead of the KVX palette.
    pub override_palette: bool,
    /// `PitchFromMomentum`.
    pub pitch_from_momentum: bool,
    /// `UseActorPitch`.
    pub use_actor_pitch: bool,
    /// `UseActorRoll`.
    pub use_actor_roll: bool,
}

impl Default for VoxelDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            file: String::new(),
            scale: 1.0,
            placed_spin: 0,
            dropped_spin: 0,
            angle_offset: 90.0,
            offset: [0.0; 3],
            override_palette: false,
            pitch_from_momentum: false,
            use_actor_pitch: false,
            use_actor_roll: false,
        }
    }
}

/// Compiled VOXELDEF registry (later entry for the same name wins).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelDefs {
    /// Name (uppercase) → options.
    pub by_name: BTreeMap<String, VoxelDef>,
}

impl VoxelDefs {
    /// Voxel for `sprite` (4 chars) and `frame` (0 = A). A frame-specific
    /// entry wins over the all-frames entry.
    #[must_use]
    pub fn for_sprite(&self, sprite: &str, frame: u8) -> Option<&VoxelDef> {
        let sprite = sprite.trim().to_ascii_uppercase();
        if sprite.is_empty() {
            return None;
        }
        let letter = char::from(b'A' + frame.min(25));
        let specific = format!("{sprite}{letter}");
        self.by_name
            .get(&specific)
            .or_else(|| self.by_name.get(&sprite))
    }
}

/// Parse a VOXELDEF lump.
pub fn parse_voxeldef(text: &str) -> Result<VoxelDefs, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
            ..Default::default()
        },
    );
    let mut out = VoxelDefs::default();
    loop {
        let mut names: Vec<String> = Vec::new();
        let old_form = loop {
            match sc.next_token()? {
                Tok::Eof => {
                    if names.is_empty() {
                        return Ok(out);
                    }
                    return Err(DefsError::Message(
                        "VOXELDEF: unexpected end of file".into(),
                    ));
                }
                Tok::Punct(Punct::Semi) => {}
                Tok::Ident(name) => names.push(name.to_string()),
                Tok::String(name) => names.push(name.into_owned()),
                Tok::Punct(Punct::Eq) => break false,
                Tok::Punct(Punct::LBrace) if names.len() == 1 => break true,
                other => {
                    return Err(DefsError::Message(format!(
                        "VOXELDEF: unexpected {other:?}"
                    )));
                }
            }
        };
        if names.is_empty() {
            return Err(DefsError::Message("VOXELDEF: no sprites specified".into()));
        }
        let file = if old_form {
            names[0].clone()
        } else {
            match sc.next_token()? {
                Tok::String(s) => s.into_owned(),
                Tok::Ident(s) => s.to_string(),
                other => {
                    return Err(DefsError::Message(format!(
                        "VOXELDEF: expected voxel file, got {other:?}"
                    )));
                }
            }
        };
        let mut def = VoxelDef {
            file,
            ..VoxelDef::default()
        };
        if old_form || sc.check_punct(Punct::LBrace)? {
            read_options(&mut sc, &mut def)?;
        }
        for name in names {
            let key = name.to_ascii_uppercase();
            let mut one = def.clone();
            one.name = key.clone();
            out.by_name.insert(key, one);
        }
    }
}

fn read_options(sc: &mut Scanner<'_>, def: &mut VoxelDef) -> Result<(), DefsError> {
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => return Ok(()),
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(key) => {
                let k = key.to_ascii_lowercase();
                match k.as_str() {
                    "scale" => {
                        expect_eq(sc)?;
                        def.scale = read_f32(sc)?;
                    }
                    "spin" => {
                        expect_eq(sc)?;
                        let v = read_i32(sc)?;
                        def.placed_spin = v;
                        def.dropped_spin = v;
                    }
                    "placedspin" => {
                        expect_eq(sc)?;
                        def.placed_spin = read_i32(sc)?;
                    }
                    "droppedspin" => {
                        expect_eq(sc)?;
                        def.dropped_spin = read_i32(sc)?;
                    }
                    "angleoffset" => {
                        expect_eq(sc)?;
                        def.angle_offset = read_f32(sc)? + 90.0;
                    }
                    "xoffset" => {
                        expect_eq(sc)?;
                        def.offset[0] = read_f32(sc)?;
                    }
                    "yoffset" => {
                        expect_eq(sc)?;
                        def.offset[1] = read_f32(sc)?;
                    }
                    "zoffset" => {
                        expect_eq(sc)?;
                        def.offset[2] = read_f32(sc)?;
                    }
                    "overridepalette" => def.override_palette = true,
                    "pitchfrommomentum" => def.pitch_from_momentum = true,
                    "useactorpitch" => def.use_actor_pitch = true,
                    "useactorroll" => def.use_actor_roll = true,
                    _ => {
                        if sc.check_punct(Punct::Eq)? {
                            let _ = sc.next_token()?;
                        }
                    }
                }
                let _ = sc.check_punct(Punct::Semi)?;
            }
            other => {
                return Err(DefsError::Message(format!(
                    "VOXELDEF body: unexpected {other:?}"
                )));
            }
        }
    }
}

fn expect_eq(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    if sc.check_punct(Punct::Eq)? {
        Ok(())
    } else {
        Err(DefsError::Message("VOXELDEF: expected `=`".into()))
    }
}

fn read_f32(sc: &mut Scanner<'_>) -> Result<f32, DefsError> {
    let neg = sc.check_punct(Punct::Minus)?;
    let v = match sc.next_token()? {
        Tok::Float(f) => f as f32,
        Tok::Int(n) => n as f32,
        Tok::Ident(s) => s
            .parse()
            .map_err(|_| DefsError::Message(format!("VOXELDEF: bad number {s}")))?,
        other => {
            return Err(DefsError::Message(format!(
                "VOXELDEF: expected number, got {other:?}"
            )));
        }
    };
    Ok(if neg { -v } else { v })
}

fn read_i32(sc: &mut Scanner<'_>) -> Result<i32, DefsError> {
    Ok(read_f32(sc)? as i32)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_scale_spin_angle() {
        let text = r#"
crate
{
	Scale = 0.5
	Spin = 10
	AngleOffset = 45
	OverridePalette
}
"#;
        let defs = parse_voxeldef(text).unwrap();
        let v = defs.by_name.get("CRATE").unwrap();
        assert!((v.scale - 0.5).abs() < 1e-3);
        assert_eq!(v.placed_spin, 10);
        assert_eq!(v.dropped_spin, 10);
        // Lump value plus GZDoom's Build +90°.
        assert!((v.angle_offset - 135.0).abs() < 1e-3);
        assert!(v.override_palette);
        assert_eq!(v.file, "crate");
    }

    #[test]
    fn placed_and_dropped_spin_differ() {
        let text = r#"
barrel
{
	PlacedSpin = 5
	DroppedSpin = -3
}
"#;
        let defs = parse_voxeldef(text).unwrap();
        let v = defs.by_name.get("BARREL").unwrap();
        assert_eq!(v.placed_spin, 5);
        assert_eq!(v.dropped_spin, -3);
        assert!((v.angle_offset - 90.0).abs() < 1e-3);
    }

    #[test]
    fn gzdoom_assignment_form() {
        let text = r#"
// Pistol
pista = "pista" { PlacedSpin = 150 DroppedSpin = 150 }
possa = "possa" {}
pinse = "voxels/pins_shell.kvx" { PlacedSpin = 15 }
misla = "misla" { /*UseActorPitch*/ }
"#;
        let defs = parse_voxeldef(text).unwrap();
        let p = defs.by_name.get("PISTA").unwrap();
        assert_eq!(p.file, "pista");
        assert_eq!(p.placed_spin, 150);
        assert_eq!(p.dropped_spin, 150);
        assert!((p.angle_offset - 90.0).abs() < 1e-3);
        let shell = defs.for_sprite("PINS", 4).unwrap();
        assert_eq!(shell.file, "voxels/pins_shell.kvx");
        assert_eq!(shell.placed_spin, 15);
        assert!(defs.for_sprite("POSS", 0).unwrap().file == "possa");
        assert!(!defs.by_name.get("MISLA").unwrap().use_actor_pitch);
    }
}

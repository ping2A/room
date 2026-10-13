//! VOXELDEF parser (M10 bootstrap — defs only; KVX mesh later).

use crate::error::DefsError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One VOXELDEF entry (voxel lump / sprite name → display options).
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelDef {
    /// Voxel or sprite name (uppercase key).
    pub name: String,
    /// Uniform scale (default 1).
    pub scale: f32,
    /// Spin when placed and dropped (`Spin=` sets both).
    pub placed_spin: i32,
    /// Spin when dropped (may differ from placed).
    pub dropped_spin: i32,
    /// Angle offset in degrees (GZDoom adds 90° for Build north; we store the
    /// lump value and apply +90 at draw time).
    pub angle_offset: f32,
    /// `OverridePalette` — use voxel palette instead of game PLAYPAL.
    pub override_palette: bool,
}

impl Default for VoxelDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            scale: 1.0,
            placed_spin: 0,
            dropped_spin: 0,
            angle_offset: 0.0,
            override_palette: false,
        }
    }
}

/// Compiled VOXELDEF registry (later entry for the same name wins).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelDefs {
    /// Name (uppercase) → options.
    pub by_name: BTreeMap<String, VoxelDef>,
}

/// Parse a VOXELDEF lump.
pub fn parse_voxeldef(text: &str) -> Result<VoxelDefs, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
        },
    );
    let mut out = VoxelDefs::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(name) => {
                let def = parse_voxel_block(&mut sc, name)?;
                out.by_name.insert(def.name.clone(), def);
            }
            Tok::String(name) => {
                let def = parse_voxel_block(&mut sc, &name)?;
                out.by_name.insert(def.name.clone(), def);
            }
            other => {
                return Err(DefsError::Message(format!(
                    "VOXELDEF: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(out)
}

fn parse_voxel_block(sc: &mut Scanner<'_>, name: &str) -> Result<VoxelDef, DefsError> {
    expect_lbrace(sc)?;
    let mut def = VoxelDef {
        name: name.to_ascii_uppercase(),
        ..Default::default()
    };
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
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
                        def.angle_offset = read_f32(sc)?;
                    }
                    "overridepalette" => {
                        def.override_palette = true;
                    }
                    _ => {
                        // Skip unknown `key [= value…]` until `;` / `}`.
                        let _ = sc.check_punct(Punct::Eq)?;
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
                    "VOXELDEF body: unexpected {other:?}"
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
            "VOXELDEF: expected `{{`, got {other:?}"
        ))),
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
        assert!((v.angle_offset - 45.0).abs() < 1e-3);
        assert!(v.override_palette);
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
    }
}

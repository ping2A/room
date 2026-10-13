//! DECALDEF parser (decals, groups, animators, generators).

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One decal definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecalDef {
    /// Name.
    pub name: String,
    /// Optional numeric id.
    pub id: Option<i32>,
    /// Picture lump.
    pub pic: Option<String>,
    /// Additive blend.
    pub add: bool,
    /// Shade colour string (`"00 00 00"` / `"BloodDefault"`).
    pub shade: Option<String>,
    /// Alpha translucency 0..=1.
    pub translucent: Option<f64>,
    /// Solid (opaque).
    pub solid: bool,
    /// X scale.
    pub x_scale: Option<f64>,
    /// Y scale.
    pub y_scale: Option<f64>,
    /// Random flip X.
    pub random_flip_x: bool,
    /// Random flip Y.
    pub random_flip_y: bool,
    /// Fullbright.
    pub fullbright: bool,
    /// Fuzzy.
    pub fuzzy: bool,
    /// Lower decal name.
    pub lower_decal: Option<String>,
    /// Animator name.
    pub animator: Option<String>,
    /// Opaque blood.
    pub opaque_blood: bool,
    /// Extra properties.
    pub extras: BTreeMap<String, String>,
}

/// Weighted group of decals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DecalGroup {
    /// Group name.
    pub name: String,
    /// `(decal, weight)` entries.
    pub entries: Vec<(String, i32)>,
}

/// Animator / fader / stretcher / slider / colorchanger / combiner.
#[derive(Clone, Debug, PartialEq)]
pub enum DecalAnimator {
    /// `fader`.
    Fader {
        /// Name.
        name: String,
        /// Properties.
        props: BTreeMap<String, String>,
    },
    /// `stretcher`.
    Stretcher {
        /// Name.
        name: String,
        /// Properties.
        props: BTreeMap<String, String>,
    },
    /// `slider`.
    Slider {
        /// Name.
        name: String,
        /// Properties.
        props: BTreeMap<String, String>,
    },
    /// `colorchanger`.
    ColorChanger {
        /// Name.
        name: String,
        /// Properties.
        props: BTreeMap<String, String>,
    },
    /// `combiner` — list of animator names.
    Combiner {
        /// Name.
        name: String,
        /// Child animator names.
        children: Vec<String>,
    },
}

impl DecalAnimator {
    /// Animator name.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            DecalAnimator::Fader { name, .. }
            | DecalAnimator::Stretcher { name, .. }
            | DecalAnimator::Slider { name, .. }
            | DecalAnimator::ColorChanger { name, .. }
            | DecalAnimator::Combiner { name, .. } => name,
        }
    }
}

/// Compiled DECALDEF registry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecalInfo {
    /// Decals by name.
    pub decals: BTreeMap<String, DecalDef>,
    /// Groups by name.
    pub groups: BTreeMap<String, DecalGroup>,
    /// Animators by name.
    pub animators: BTreeMap<String, DecalAnimator>,
    /// Generator: actor class → decal/group name.
    pub generators: BTreeMap<String, String>,
}

/// Resolved decal animator params (fader + stretcher + slider + colorchanger; combiner merges).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecalAnimParams {
    /// Fade `(start_tics, time_tics)`.
    pub fade: Option<(u32, u32)>,
    /// Stretch: start, time, optional GoalX/GoalY scale factors.
    pub stretch: Option<DecalStretchParams>,
    /// Slide: start, time, DistY map units.
    pub slide: Option<DecalSlideParams>,
    /// Colorchanger: start, time, goal RGB 0..=1.
    pub color: Option<DecalColorParams>,
}

/// Stretcher animator (seconds → tics at 35 Hz).
#[derive(Clone, Debug, PartialEq)]
pub struct DecalStretchParams {
    /// Tics before stretch begins.
    pub start_tics: u32,
    /// Stretch duration in tics.
    pub time_tics: u32,
    /// Final X scale factor (`None` = leave width).
    pub goal_x: Option<f64>,
    /// Final Y scale factor (`None` = leave height).
    pub goal_y: Option<f64>,
}

/// Slider animator (seconds → tics at 35 Hz; DistY only — DistX unsupported in GZDoom).
#[derive(Clone, Debug, PartialEq)]
pub struct DecalSlideParams {
    /// Tics before slide begins.
    pub start_tics: u32,
    /// Slide duration in tics.
    pub time_tics: u32,
    /// Vertical distance in map units (positive up).
    pub dist_y: f64,
}

/// Colorchanger animator (seconds → tics at 35 Hz).
#[derive(Clone, Debug, PartialEq)]
pub struct DecalColorParams {
    /// Tics before colour change begins.
    pub start_tics: u32,
    /// Duration in tics.
    pub time_tics: u32,
    /// Goal RGB 0..=1 (stock `ToBlack` → black).
    pub goal_rgb: [f64; 3],
}

impl DecalInfo {
    /// Resolve a placed name (decal or first group entry) → optional fader `(start_tics, time_tics)`.
    #[must_use]
    pub fn fader_tics(&self, placed_name: &str) -> Option<(u32, u32)> {
        self.anim_params(placed_name).fade
    }

    /// Resolve fader / stretcher / slider (including `combiner` children) for a placed name.
    #[must_use]
    pub fn anim_params(&self, placed_name: &str) -> DecalAnimParams {
        let mut out = DecalAnimParams::default();
        let Some(def_name) = self.resolve_decal_name(placed_name) else {
            return out;
        };
        let Some(def) = self.decals.get(def_name) else {
            return out;
        };
        let Some(anim) = def.animator.as_deref() else {
            return out;
        };
        self.collect_anim(anim, &mut out, 0);
        out
    }

    fn resolve_decal_name<'a>(&'a self, placed_name: &'a str) -> Option<&'a str> {
        self.pick_decal_name(placed_name, 0)
    }

    /// Pick a concrete decal def name from a decal or weighted group.
    ///
    /// `salt` selects among group entries (stable for a given impact position).
    #[must_use]
    pub fn pick_decal_name<'a>(&'a self, placed_name: &'a str, salt: u32) -> Option<&'a str> {
        if self.decals.contains_key(placed_name) {
            return Some(placed_name);
        }
        let g = self.groups.get(placed_name)?;
        if g.entries.is_empty() {
            return None;
        }
        let total: i32 = g.entries.iter().map(|(_, w)| (*w).max(0)).sum();
        if total <= 0 {
            return g.entries.first().map(|(n, _)| n.as_str());
        }
        #[allow(clippy::cast_possible_wrap)]
        let mut tick = (salt as i32).rem_euclid(total);
        for (name, weight) in &g.entries {
            tick -= (*weight).max(0);
            if tick < 0 {
                return Some(name.as_str());
            }
        }
        g.entries.first().map(|(n, _)| n.as_str())
    }

    /// Texture lump to bind for a placed decal/group (`pic`, else the def name).
    #[must_use]
    pub fn place_texture(&self, placed_name: &str, salt: u32) -> Option<String> {
        let def_name = self.pick_decal_name(placed_name, salt)?;
        let def = self.decals.get(def_name)?;
        Some(
            def.pic
                .clone()
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| def.name.clone()),
        )
    }

    fn collect_anim(&self, anim: &str, out: &mut DecalAnimParams, depth: u8) {
        if depth > 8 {
            return;
        }
        let Some(a) = self.animators.get(anim) else {
            return;
        };
        match a {
            DecalAnimator::Fader { props, .. } => {
                if out.fade.is_none() {
                    out.fade = secs_pair(props, "decaystart", "decaytime");
                }
            }
            DecalAnimator::Stretcher { props, .. } => {
                if out.stretch.is_none() {
                    let goal_x = props.get("goalx").and_then(|s| s.parse::<f64>().ok());
                    let goal_y = props.get("goaly").and_then(|s| s.parse::<f64>().ok());
                    if goal_x.is_none() && goal_y.is_none() {
                        return;
                    }
                    let (start_tics, time_tics) =
                        secs_pair(props, "stretchstart", "stretchtime").unwrap_or((0, 1));
                    out.stretch = Some(DecalStretchParams {
                        start_tics,
                        time_tics,
                        goal_x,
                        goal_y,
                    });
                }
            }
            DecalAnimator::Slider { props, .. } => {
                if out.slide.is_none() {
                    let dist_y = props
                        .get("disty")
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    if dist_y == 0.0 {
                        return;
                    }
                    let (start_tics, time_tics) =
                        secs_pair(props, "slidestart", "slidetime").unwrap_or((0, 1));
                    out.slide = Some(DecalSlideParams {
                        start_tics,
                        time_tics,
                        dist_y,
                    });
                }
            }
            DecalAnimator::Combiner { children, .. } => {
                for c in children {
                    self.collect_anim(c, out, depth + 1);
                }
            }
            DecalAnimator::ColorChanger { props, .. } => {
                if out.color.is_none() {
                    let (start_tics, time_tics) =
                        secs_pair(props, "fadestart", "fadetime").unwrap_or((0, 1));
                    let goal_rgb = props
                        .get("color")
                        .and_then(|s| parse_rgb_triple(s))
                        .unwrap_or([0.0, 0.0, 0.0]);
                    out.color = Some(DecalColorParams {
                        start_tics,
                        time_tics,
                        goal_rgb,
                    });
                }
            }
        }
    }
}

fn parse_rgb_triple(s: &str) -> Option<[f64; 3]> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let mut out = [0.0; 3];
    for (i, p) in parts.iter().enumerate() {
        let v = u8::from_str_radix(p, 16)
            .ok()
            .or_else(|| p.parse::<u8>().ok())?;
        out[i] = f64::from(v) / 255.0;
    }
    Some(out)
}

fn secs_pair(
    props: &BTreeMap<String, String>,
    start_key: &str,
    time_key: &str,
) -> Option<(u32, u32)> {
    let start_s = props
        .get(start_key)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let time_s = props
        .get(time_key)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    if time_s <= 0.0 {
        return None;
    }
    let start = (start_s * 35.0).round().clamp(0.0, f64::from(u32::MAX)) as u32;
    let time = (time_s * 35.0).round().clamp(1.0, f64::from(u32::MAX)) as u32;
    Some((start, time))
}

/// Parse a DECALDEF lump (additive merge is the caller's job).
pub fn parse_decaldef(text: &str) -> Result<DecalInfo, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
        },
    );
    let mut info = DecalInfo::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) => {
                let key = kw.to_ascii_lowercase();
                match key.as_str() {
                    "decal" => {
                        let def = parse_decal(&mut sc)?;
                        info.decals.insert(def.name.clone(), def);
                    }
                    "decalgroup" => {
                        let g = parse_decalgroup(&mut sc)?;
                        info.groups.insert(g.name.clone(), g);
                    }
                    "fader" => {
                        let (name, props) = parse_named_props(&mut sc)?;
                        info.animators
                            .insert(name.clone(), DecalAnimator::Fader { name, props });
                    }
                    "stretcher" => {
                        let (name, props) = parse_named_props(&mut sc)?;
                        info.animators
                            .insert(name.clone(), DecalAnimator::Stretcher { name, props });
                    }
                    "slider" => {
                        let (name, props) = parse_named_props(&mut sc)?;
                        info.animators
                            .insert(name.clone(), DecalAnimator::Slider { name, props });
                    }
                    "colorchanger" => {
                        let (name, props) = parse_named_props(&mut sc)?;
                        info.animators
                            .insert(name.clone(), DecalAnimator::ColorChanger { name, props });
                    }
                    "combiner" => {
                        let name = read_name(&mut sc)?;
                        expect_lbrace(&mut sc)?;
                        let mut children = Vec::new();
                        loop {
                            match sc.next_token()? {
                                Tok::Punct(Punct::RBrace) | Tok::Eof => break,
                                Tok::Ident(c) => children.push(c.to_owned()),
                                Tok::String(c) => children.push(c.into_owned()),
                                Tok::Punct(Punct::Semi) => {}
                                other => {
                                    return Err(DefsError::Message(format!(
                                        "DECALDEF combiner: unexpected {other:?}"
                                    )));
                                }
                            }
                        }
                        info.animators
                            .insert(name.clone(), DecalAnimator::Combiner { name, children });
                    }
                    "generator" => {
                        let class = read_name(&mut sc)?;
                        let decal = read_name(&mut sc)?;
                        info.generators.insert(class, decal);
                    }
                    "include" => {
                        let _ = read_name(&mut sc)?;
                    }
                    _ => {
                        if sc.check_punct(Punct::LBrace)? {
                            skip_block(&mut sc)?;
                        }
                    }
                }
            }
            other => {
                return Err(DefsError::Message(format!(
                    "DECALDEF: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(info)
}

fn parse_decal(sc: &mut Scanner<'_>) -> Result<DecalDef, DefsError> {
    let name = read_name(sc)?;
    let mut def = DecalDef {
        name,
        ..Default::default()
    };
    // Optional numeric id before `{`.
    match sc.next_token()? {
        Tok::Int(n) => {
            def.id = Some(n as i32);
            expect_lbrace(sc)?;
        }
        Tok::Punct(Punct::LBrace) => {}
        other => {
            return Err(DefsError::Message(format!(
                "DECALDEF decal: expected `{{` or id, got {other:?}"
            )));
        }
    }
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Ident(k) => {
                let key = read_hyphen_key(sc, k)?;
                match key.as_str() {
                    "pic" => def.pic = Some(read_name(sc)?),
                    "add" => {
                        def.add = true;
                        // Optional alpha: `add` or `add 1.0`.
                        match sc.next_token()? {
                            Tok::Float(f) => {
                                def.translucent = Some(f);
                            }
                            Tok::Int(n) => {
                                def.translucent = Some(n as f64);
                            }
                            other => sc.unget(other),
                        }
                    }
                    "shade" => def.shade = Some(read_name(sc)?),
                    "translucent" => def.translucent = Some(read_f64(sc)?),
                    "solid" => def.solid = true,
                    "x-scale" => def.x_scale = Some(read_f64(sc)?),
                    "y-scale" => def.y_scale = Some(read_f64(sc)?),
                    "randomflipx" => def.random_flip_x = true,
                    "randomflipy" => def.random_flip_y = true,
                    "fullbright" => def.fullbright = true,
                    "fuzzy" => def.fuzzy = true,
                    "lowerdecal" => def.lower_decal = Some(read_name(sc)?),
                    "animator" => def.animator = Some(read_name(sc)?),
                    "opaqueblood" => def.opaque_blood = true,
                    _ => {
                        // Flag-only unknown vs valued.
                        match sc.next_token()? {
                            Tok::Punct(Punct::RBrace) => {
                                sc.unget(Tok::Punct(Punct::RBrace));
                                def.extras.insert(key, "true".into());
                            }
                            Tok::Ident(n) if is_decal_prop_start(n) => {
                                sc.unget(Tok::Ident(n));
                                def.extras.insert(key, "true".into());
                            }
                            Tok::Ident(v) => {
                                def.extras.insert(key, v.to_owned());
                            }
                            Tok::String(v) => {
                                def.extras.insert(key, v.into_owned());
                            }
                            Tok::Int(n) => {
                                def.extras.insert(key, n.to_string());
                            }
                            Tok::Float(f) => {
                                def.extras.insert(key, f.to_string());
                            }
                            other => {
                                sc.unget(other);
                                def.extras.insert(key, "true".into());
                            }
                        }
                    }
                }
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "DECALDEF decal body: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(def)
}

fn is_decal_prop_start(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "pic"
            | "add"
            | "shade"
            | "translucent"
            | "solid"
            | "x"
            | "y"
            | "randomflipx"
            | "randomflipy"
            | "fullbright"
            | "fuzzy"
            | "lowerdecal"
            | "animator"
            | "opaqueblood"
    )
}

fn parse_decalgroup(sc: &mut Scanner<'_>) -> Result<DecalGroup, DefsError> {
    let name = read_name(sc)?;
    expect_lbrace(sc)?;
    let mut entries = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Ident(d) => {
                let decal = d.to_owned();
                let weight = match sc.next_token()? {
                    Tok::Int(n) => n as i32,
                    other => {
                        sc.unget(other);
                        1
                    }
                };
                entries.push((decal, weight));
            }
            Tok::String(d) => {
                let decal = d.into_owned();
                let weight = match sc.next_token()? {
                    Tok::Int(n) => n as i32,
                    other => {
                        sc.unget(other);
                        1
                    }
                };
                entries.push((decal, weight));
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "DECALDEF decalgroup: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(DecalGroup { name, entries })
}

fn parse_named_props(
    sc: &mut Scanner<'_>,
) -> Result<(String, BTreeMap<String, String>), DefsError> {
    let name = read_name(sc)?;
    expect_lbrace(sc)?;
    let mut props = BTreeMap::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Ident(k) => {
                let key = read_hyphen_key(sc, k)?;
                let val = read_value_token(sc).unwrap_or_else(|_| "true".into());
                props.insert(key, val);
            }
            Tok::Punct(Punct::Semi) => {}
            other => {
                return Err(DefsError::Message(format!(
                    "DECALDEF animator: unexpected {other:?}"
                )));
            }
        }
    }
    Ok((name, props))
}

fn read_hyphen_key(sc: &mut Scanner<'_>, first: &str) -> Result<String, DefsError> {
    let mut key = first.to_ascii_lowercase();
    // Only `x-scale` / `y-scale` (avoid treating `DistY -5` as a hyphenated key;
    // the scanner only supports one unget token).
    if matches!(key.as_str(), "x" | "y") && sc.check_punct(Punct::Minus)? {
        let rest = sc.must_get_ident()?.to_ascii_lowercase();
        key.push('-');
        key.push_str(&rest);
    }
    Ok(key)
}

fn expect_lbrace(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    match sc.next_token()? {
        Tok::Punct(Punct::LBrace) => Ok(()),
        other => Err(DefsError::Message(format!(
            "DECALDEF: expected `{{`, got {other:?}"
        ))),
    }
}

fn read_name(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        other => Err(DefsError::Message(format!(
            "DECALDEF: expected name, got {other:?}"
        ))),
    }
}

fn read_value_token(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        Tok::Int(n) => Ok(n.to_string()),
        Tok::Float(f) => Ok(f.to_string()),
        Tok::Punct(Punct::Minus) => match sc.next_token()? {
            Tok::Int(n) => Ok((-(n as i32)).to_string()),
            Tok::Float(f) => Ok((-f).to_string()),
            other => {
                sc.unget(other);
                sc.unget(Tok::Punct(Punct::Minus));
                Err(DefsError::Message(
                    "DECALDEF: expected number after '-'".into(),
                ))
            }
        },
        other => {
            sc.unget(other);
            Err(DefsError::Message("DECALDEF: expected value token".into()))
        }
    }
}

fn read_f64(sc: &mut Scanner<'_>) -> Result<f64, DefsError> {
    match sc.next_token()? {
        Tok::Float(f) => Ok(f),
        Tok::Int(n) => Ok(n as f64),
        Tok::Punct(Punct::Minus) => match sc.next_token()? {
            Tok::Float(f) => Ok(-f),
            Tok::Int(n) => Ok(-(n as f64)),
            other => Err(DefsError::Message(format!(
                "DECALDEF: expected float, got {other:?}"
            ))),
        },
        other => Err(DefsError::Message(format!(
            "DECALDEF: expected float, got {other:?}"
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
    fn parses_decal_group_generator() {
        let text = r#"
fader GoAway
{
	DecayStart	0.5
	DecayTime	1.0
}

decal BulletChip1
{
	pic CHIP1
	translucent 0.85
	shade "00 00 00"
	x-scale 0.5
	y-scale 0.5
	randomflipx
	randomflipy
}

decal BulletChip2
{
	pic CHIP2
}

decalgroup BulletChip
{
	BulletChip1	1
	BulletChip2	1
}

generator Pistol BulletChip
"#;
        let info = parse_decaldef(text).unwrap();
        assert!(info.animators.contains_key("GoAway"));
        let d = info.decals.get("BulletChip1").unwrap();
        assert_eq!(d.pic.as_deref(), Some("CHIP1"));
        assert!((d.translucent.unwrap() - 0.85).abs() < 1e-9);
        assert_eq!(d.shade.as_deref(), Some("00 00 00"));
        assert!((d.x_scale.unwrap() - 0.5).abs() < 1e-9);
        assert!(d.random_flip_x && d.random_flip_y);
        assert_eq!(info.groups.get("BulletChip").unwrap().entries.len(), 2);
        assert_eq!(
            info.generators.get("Pistol").map(String::as_str),
            Some("BulletChip")
        );
        assert_eq!(
            info.place_texture("BulletChip", 0).as_deref(),
            Some("CHIP1")
        );
        assert_eq!(
            info.place_texture("BulletChip", 1).as_deref(),
            Some("CHIP2")
        );
    }

    #[test]
    fn fader_tics_from_animator() {
        let text = r#"
fader GoAway { DecayStart 0.5 DecayTime 1.0 }
decal Chip { pic CHIP1 animator GoAway }
"#;
        let info = parse_decaldef(text).unwrap();
        assert_eq!(info.fader_tics("Chip"), Some((18, 35)));
        assert_eq!(info.fader_tics("Missing"), None);
    }

    #[test]
    fn stretcher_slider_combiner_params() {
        let text = r#"
stretcher BloodStretcher { StretchTime 1.0 GoalY 2.0 }
slider BloodSlider { SlideTime 1.0 DistY -5 }
combiner BloodSmearer { BloodStretcher BloodSlider }
decal Smear { pic BLOOD1 animator BloodSmearer }
"#;
        let info = parse_decaldef(text).unwrap();
        let p = info.anim_params("Smear");
        let stretch = p.stretch.as_ref().unwrap();
        assert_eq!(stretch.time_tics, 35);
        assert_eq!(stretch.goal_y, Some(2.0));
        assert!(stretch.goal_x.is_none());
        let slide = p.slide.as_ref().unwrap();
        assert_eq!(slide.time_tics, 35);
        assert!((slide.dist_y - (-5.0)).abs() < 1e-9);
    }

    #[test]
    fn colorchanger_params() {
        let text = r#"
colorchanger ToBlack { FadeStart 0.5 FadeTime 1.0 }
decal Chip { pic CHIP1 animator ToBlack }
"#;
        let info = parse_decaldef(text).unwrap();
        let c = info.anim_params("Chip").color.unwrap();
        assert_eq!(c.start_tics, 18);
        assert_eq!(c.time_tics, 35);
        assert_eq!(c.goal_rgb, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn parses_gzdoom_decaldef_fixture() {
        let text = include_str!("../../../testdata/decaldef/gzdoom_decaldef.txt");
        let info = parse_decaldef(text).unwrap();
        assert!(info.decals.contains_key("BulletChip1"));
        assert!(info.groups.contains_key("BulletChip"));
        assert!(info.animators.contains_key("BloodSmearer"));
        assert!(info.generators.contains_key("Pistol"));
        assert!(info.generators.len() > 10);
    }
}

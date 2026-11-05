//! ID24 JSON lumps: `SKYDEFS`, `DEMOLOOP`, `GAMECONF` (typed) and any other
//! JSON-lump envelope (`TRAKINFO`, `SBARDEF`, `INTERLEVEL`, `FINALE`, …) kept
//! as raw JSON (`docs/36` UZ14).
//!
//! Every ID24 lump follows the "JSON lump 1.0.0" root: an object with a
//! `type` string, a `version` string (`1.x.y` accepted) and a `data` object.
//! Field names, types and resolution rules follow the ID24 formal
//! specifications (CC0). Typed parsers reject structural errors with a
//! message naming the lump and the offending field.

use crate::error::DefsError;
use serde_json::Value;

fn err(lump: &str, msg: impl core::fmt::Display) -> DefsError {
    DefsError::Message(format!("{lump}: {msg}"))
}

/// A JSON-lump root with its `data` still untyped.
#[derive(Clone, Debug, PartialEq)]
pub struct RawJsonLump {
    /// `type` string (`skydefs`, `demoloop`, `gameconf`, …).
    pub kind: String,
    /// `version` string.
    pub version: String,
    /// The `data` value (any JSON).
    pub data: Value,
}

/// Parse and validate a JSON-lump root. `expected` (when given) must equal the
/// `type` field case-insensitively; the version major must be `1`.
pub fn parse_json_lump(text: &str, expected: Option<&str>) -> Result<RawJsonLump, DefsError> {
    let label = expected.map_or_else(|| "JSON lump".to_owned(), str::to_ascii_uppercase);
    let label = label.as_str();
    let text = text.trim_start_matches('\u{feff}');
    let root: Value = serde_json::from_str(text).map_err(|e| err(label, e))?;
    let obj = root
        .as_object()
        .ok_or_else(|| err(label, "root must be an object"))?;
    let kind = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| err(label, "missing string field `type`"))?
        .to_owned();
    if let Some(want) = expected {
        if !kind.eq_ignore_ascii_case(want) {
            return Err(err(label, format!("type is {kind:?}, expected {want:?}")));
        }
    }
    let version = obj
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| err(label, "missing string field `version`"))?
        .to_owned();
    let major = version.split('.').next().unwrap_or("");
    if major != "1" {
        return Err(err(
            label,
            format!("unsupported version {version:?} (need 1.x.y)"),
        ));
    }
    let data = obj
        .get("data")
        .cloned()
        .ok_or_else(|| err(label, "missing field `data`"))?;
    Ok(RawJsonLump {
        kind,
        version,
        data,
    })
}

// ── field helpers ───────────────────────────────────────────────────────

fn field<'a>(o: &'a serde_json::Map<String, Value>, k: &str) -> Option<&'a Value> {
    o.get(k).filter(|v| !v.is_null())
}

fn f_str(
    lump: &str,
    o: &serde_json::Map<String, Value>,
    k: &str,
) -> Result<Option<String>, DefsError> {
    match field(o, k) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(err(lump, format!("`{k}` must be a string"))),
    }
}

fn f_num(
    lump: &str,
    o: &serde_json::Map<String, Value>,
    k: &str,
) -> Result<Option<f64>, DefsError> {
    match field(o, k) {
        None => Ok(None),
        Some(v) => v
            .as_f64()
            .map(Some)
            .ok_or_else(|| err(lump, format!("`{k}` must be a number"))),
    }
}

fn f_int(
    lump: &str,
    o: &serde_json::Map<String, Value>,
    k: &str,
) -> Result<Option<i64>, DefsError> {
    match field(o, k) {
        None => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| err(lump, format!("`{k}` must be an integer"))),
    }
}

fn f_obj<'a>(
    lump: &str,
    o: &'a serde_json::Map<String, Value>,
    k: &str,
) -> Result<Option<&'a serde_json::Map<String, Value>>, DefsError> {
    match field(o, k) {
        None => Ok(None),
        Some(Value::Object(m)) => Ok(Some(m)),
        Some(_) => Err(err(lump, format!("`{k}` must be an object"))),
    }
}

fn f_arr<'a>(
    lump: &str,
    o: &'a serde_json::Map<String, Value>,
    k: &str,
) -> Result<Option<&'a Vec<Value>>, DefsError> {
    match field(o, k) {
        None => Ok(None),
        Some(Value::Array(a)) => Ok(Some(a)),
        Some(_) => Err(err(lump, format!("`{k}` must be an array"))),
    }
}

fn as_obj<'a>(
    lump: &str,
    v: &'a Value,
    what: &str,
) -> Result<&'a serde_json::Map<String, Value>, DefsError> {
    v.as_object()
        .ok_or_else(|| err(lump, format!("{what} must be an object")))
}

// ── SKYDEFS ─────────────────────────────────────────────────────────────

/// Sky type. The spec names Normal, Fire and With Foreground in this order;
/// numeric codes `0/1/2` follow that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkyKind {
    /// Plain texture.
    Normal,
    /// Doom-64 style fire sky rendered into a TEXTURE1/2 target.
    Fire,
    /// Hexen style: foreground drawn over the background, palette index 0 transparent.
    WithForeground,
}

/// Fire sky parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct FireSky {
    /// Palette indices from the decay colour to the starting colour (≥ 1 entry).
    pub palette: Vec<u8>,
    /// Seconds between simulation steps (> 0).
    pub update_time: f64,
}

/// Foreground texture parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct ForeTex {
    /// TEXTURE1/2 name.
    pub name: String,
    /// Y texel row at the vertical centre of the screen.
    pub mid: f64,
    /// Texels per second, horizontal.
    pub scroll_x: f64,
    /// Texels per second, vertical.
    pub scroll_y: f64,
    /// Width of one X texel.
    pub scale_x: f64,
    /// Height of one Y texel.
    pub scale_y: f64,
}

/// One sky definition.
#[derive(Clone, Debug, PartialEq)]
pub struct SkyDef {
    /// Sky type.
    pub kind: SkyKind,
    /// TEXTURE1/2 name (also the lookup key).
    pub name: String,
    /// Y texel row at the vertical centre of the screen.
    pub mid: f64,
    /// Texels per second, horizontal.
    pub scroll_x: f64,
    /// Texels per second, vertical.
    pub scroll_y: f64,
    /// Width of one X texel.
    pub scale_x: f64,
    /// Height of one Y texel.
    pub scale_y: f64,
    /// Fire parameters (`Some` iff `kind == Fire`).
    pub fire: Option<FireSky>,
    /// Foreground (`Some` iff `kind == WithForeground`).
    pub foreground: Option<ForeTex>,
}

/// Flat → sky mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatMap {
    /// Flat name (any case; compare case-insensitively).
    pub flat: String,
    /// Sky texture name.
    pub sky: String,
}

/// Parsed `SKYDEFS`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkyDefs {
    /// Sky definitions in file order.
    pub skies: Vec<SkyDef>,
    /// Flat mappings in file order.
    pub flatmapping: Vec<FlatMap>,
}

impl SkyDefs {
    /// Sky definition by texture name (case-insensitive).
    #[must_use]
    pub fn sky(&self, name: &str) -> Option<&SkyDef> {
        self.skies
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Sky texture a flat is mapped to (case-insensitive; last mapping wins).
    #[must_use]
    pub fn sky_for_flat(&self, flat: &str) -> Option<&str> {
        self.flatmapping
            .iter()
            .rev()
            .find(|m| m.flat.eq_ignore_ascii_case(flat))
            .map(|m| m.sky.as_str())
    }
}

fn parse_fore(lump: &str, o: &serde_json::Map<String, Value>) -> Result<ForeTex, DefsError> {
    Ok(ForeTex {
        name: f_str(lump, o, "name")?.ok_or_else(|| err(lump, "foregroundtex.name missing"))?,
        mid: f_num(lump, o, "mid")?.unwrap_or(0.0),
        scroll_x: f_num(lump, o, "scrollx")?.unwrap_or(0.0),
        scroll_y: f_num(lump, o, "scrolly")?.unwrap_or(0.0),
        scale_x: f_num(lump, o, "scalex")?.unwrap_or(1.0),
        scale_y: f_num(lump, o, "scaley")?.unwrap_or(1.0),
    })
}

/// Parse a `SKYDEFS` lump.
pub fn parse_skydefs(text: &str) -> Result<SkyDefs, DefsError> {
    const L: &str = "SKYDEFS";
    let raw = parse_json_lump(text, Some("skydefs"))?;
    let data = as_obj(L, &raw.data, "data")?;
    let mut out = SkyDefs::default();
    if let Some(skies) = f_arr(L, data, "skies")? {
        if skies.is_empty() {
            return Err(err(L, "`skies` must have at least one entry or be null"));
        }
        for (i, sv) in skies.iter().enumerate() {
            let s = as_obj(L, sv, &format!("skies[{i}]"))?;
            let kind = match f_int(L, s, "type")? {
                Some(0) => SkyKind::Normal,
                Some(1) => SkyKind::Fire,
                Some(2) => SkyKind::WithForeground,
                Some(n) => return Err(err(L, format!("skies[{i}].type {n} is invalid"))),
                None => return Err(err(L, format!("skies[{i}].type missing"))),
            };
            let name =
                f_str(L, s, "name")?.ok_or_else(|| err(L, format!("skies[{i}].name missing")))?;
            let fire = match f_obj(L, s, "fire")? {
                None => None,
                Some(f) => {
                    if kind != SkyKind::Fire {
                        return Err(err(
                            L,
                            format!("skies[{i}].fire only allowed for Fire skies"),
                        ));
                    }
                    let pal = f_arr(L, f, "palette")?
                        .ok_or_else(|| err(L, format!("skies[{i}].fire.palette missing")))?;
                    if pal.is_empty() {
                        return Err(err(L, format!("skies[{i}].fire.palette is empty")));
                    }
                    let mut palette = Vec::with_capacity(pal.len());
                    for v in pal {
                        let n = v
                            .as_i64()
                            .filter(|n| (0..=255).contains(n))
                            .ok_or_else(|| {
                                err(
                                    L,
                                    format!("skies[{i}].fire.palette entries must be 0..=255"),
                                )
                            })?;
                        palette.push(n as u8);
                    }
                    let update_time = f_num(L, f, "updatetime")?
                        .ok_or_else(|| err(L, format!("skies[{i}].fire.updatetime missing")))?;
                    if update_time <= 0.0 {
                        return Err(err(L, format!("skies[{i}].fire.updatetime must be > 0")));
                    }
                    Some(FireSky {
                        palette,
                        update_time,
                    })
                }
            };
            if kind == SkyKind::Fire && fire.is_none() {
                return Err(err(L, format!("skies[{i}]: Fire sky needs `fire`")));
            }
            let foreground = match f_obj(L, s, "foregroundtex")? {
                None => None,
                Some(f) => {
                    if kind != SkyKind::WithForeground {
                        return Err(err(
                            L,
                            format!(
                                "skies[{i}].foregroundtex only allowed for With Foreground skies"
                            ),
                        ));
                    }
                    Some(parse_fore(L, f)?)
                }
            };
            if kind == SkyKind::WithForeground && foreground.is_none() {
                return Err(err(
                    L,
                    format!("skies[{i}]: With Foreground sky needs `foregroundtex`"),
                ));
            }
            out.skies.push(SkyDef {
                kind,
                name,
                mid: f_num(L, s, "mid")?.unwrap_or(0.0),
                scroll_x: f_num(L, s, "scrollx")?.unwrap_or(0.0),
                scroll_y: f_num(L, s, "scrolly")?.unwrap_or(0.0),
                scale_x: f_num(L, s, "scalex")?.unwrap_or(1.0),
                scale_y: f_num(L, s, "scaley")?.unwrap_or(1.0),
                fire,
                foreground,
            });
        }
    }
    if let Some(maps) = f_arr(L, data, "flatmapping")? {
        if maps.is_empty() {
            return Err(err(
                L,
                "`flatmapping` must have at least one entry or be null",
            ));
        }
        for (i, mv) in maps.iter().enumerate() {
            let m = as_obj(L, mv, &format!("flatmapping[{i}]"))?;
            out.flatmapping.push(FlatMap {
                flat: f_str(L, m, "flat")?
                    .ok_or_else(|| err(L, format!("flatmapping[{i}].flat missing")))?,
                sky: f_str(L, m, "sky")?
                    .ok_or_else(|| err(L, format!("flatmapping[{i}].sky missing")))?,
            });
        }
    }
    Ok(out)
}

// ── DEMOLOOP ────────────────────────────────────────────────────────────

/// Demo-loop entry type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoKind {
    /// Full-screen art for `duration` seconds.
    Art,
    /// Demo lump playback.
    Demo,
}

/// Transition out of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutroWipe {
    /// Cut.
    Immediate,
    /// Vanilla screen melt.
    Melt,
}

/// One demo-loop entry.
#[derive(Clone, Debug, PartialEq)]
pub struct DemoLoopEntry {
    /// Lump to show / play.
    pub primary: String,
    /// Music lump to switch to (art screens only).
    pub secondary: Option<String>,
    /// Seconds to hold an art screen (`None` for demos: they end by themselves).
    pub duration: Option<f64>,
    /// Entry type.
    pub kind: DemoKind,
    /// Wipe to the next entry.
    pub wipe: OutroWipe,
}

/// Parsed `DEMOLOOP`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DemoLoop {
    /// Entries in loop order (≥ 1).
    pub entries: Vec<DemoLoopEntry>,
}

/// Parse a `DEMOLOOP` lump.
pub fn parse_demoloop(text: &str) -> Result<DemoLoop, DefsError> {
    const L: &str = "DEMOLOOP";
    let raw = parse_json_lump(text, Some("demoloop"))?;
    let data = as_obj(L, &raw.data, "data")?;
    let arr = f_arr(L, data, "entries")?
        .filter(|a| !a.is_empty())
        .ok_or_else(|| err(L, "`entries` must have at least one entry"))?;
    let mut out = DemoLoop::default();
    for (i, ev) in arr.iter().enumerate() {
        let e = as_obj(L, ev, &format!("entries[{i}]"))?;
        let kind = match f_int(L, e, "type")? {
            Some(0) => DemoKind::Art,
            Some(1) => DemoKind::Demo,
            Some(n) => return Err(err(L, format!("entries[{i}].type {n} is invalid"))),
            None => return Err(err(L, format!("entries[{i}].type missing"))),
        };
        let wipe = match f_int(L, e, "outrowipe")? {
            None | Some(0) => OutroWipe::Immediate,
            Some(1) => OutroWipe::Melt,
            Some(n) => return Err(err(L, format!("entries[{i}].outrowipe {n} is invalid"))),
        };
        let primary = f_str(L, e, "primarylump")?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| err(L, format!("entries[{i}].primarylump missing")))?;
        let duration = f_num(L, e, "duration")?;
        if kind == DemoKind::Art && !duration.is_some_and(|d| d > 0.0) {
            return Err(err(
                L,
                format!("entries[{i}]: art screen needs duration > 0"),
            ));
        }
        out.entries.push(DemoLoopEntry {
            primary,
            secondary: f_str(L, e, "secondarylump")?.filter(|s| !s.is_empty()),
            duration,
            kind,
            wipe,
        });
    }
    Ok(out)
}

// ── GAMECONF ────────────────────────────────────────────────────────────

/// Engine feature set a mod requires (ordered least → most features).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Executable {
    /// `doom1.9`
    Doom19,
    /// `limitremoving`
    LimitRemoving,
    /// `bugfixed`
    BugFixed,
    /// `boom2.02`
    Boom202,
    /// `complevel9`
    Complevel9,
    /// `mbf`
    Mbf,
    /// `mbfextra` (MBF + DEHEXTRA)
    MbfExtra,
    /// `mbf21`
    Mbf21,
    /// `mbf21ex` (deprecated; MBF21 + DSDHacked)
    Mbf21Ex,
    /// `id24`
    Id24,
}

impl Executable {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "doom1.9" => Self::Doom19,
            "limitremoving" => Self::LimitRemoving,
            "bugfixed" => Self::BugFixed,
            "boom2.02" => Self::Boom202,
            "complevel9" => Self::Complevel9,
            "mbf" => Self::Mbf,
            "mbfextra" => Self::MbfExtra,
            "mbf21" => Self::Mbf21,
            "mbf21ex" => Self::Mbf21Ex,
            "id24" => Self::Id24,
            _ => return None,
        })
    }
}

/// Game mode a mod runs in (ordered lowest → highest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GameMode {
    /// Doom with 3 episodes.
    Registered,
    /// Doom with 4 episodes.
    Retail,
    /// Doom II.
    Commercial,
}

impl GameMode {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "registered" => Self::Registered,
            "retail" => Self::Retail,
            "commercial" => Self::Commercial,
            _ => return None,
        })
    }
}

/// Merged `GAMECONF` state across every lump in load order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameConf {
    /// Mod title.
    pub title: Option<String>,
    /// Author(s).
    pub author: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Release version string.
    pub version: Option<String>,
    /// IWAD file name (no path); later lumps overwrite.
    pub iwad: Option<String>,
    /// PWAD file names; additive.
    pub pwads: Vec<String>,
    /// Player translation names (≥ 4); later lumps overwrite entirely.
    pub player_translations: Option<Vec<String>>,
    /// Feature set; resolved with `max`.
    pub executable: Option<Executable>,
    /// Game mode; resolved with `max`.
    pub mode: Option<GameMode>,
    /// Options text (`\n` separated), accumulated in encounter order.
    pub options: String,
}

fn file_name(lump: &str, what: &str, s: &str) -> Result<String, DefsError> {
    if s.is_empty() || s.contains('/') || s.contains('\\') {
        return Err(err(
            lump,
            format!("{what} {s:?} must be a file name without a path"),
        ));
    }
    Ok(s.to_owned())
}

impl GameConf {
    /// Parse one `GAMECONF` lump (a single-lump state; merge with [`Self::merge`]).
    pub fn parse(text: &str) -> Result<Self, DefsError> {
        const L: &str = "GAMECONF";
        let raw = parse_json_lump(text, Some("gameconf"))?;
        let data = as_obj(L, &raw.data, "data")?;
        let mut c = GameConf {
            title: f_str(L, data, "title")?,
            author: f_str(L, data, "author")?,
            description: f_str(L, data, "description")?,
            version: f_str(L, data, "version")?,
            ..Default::default()
        };
        if let Some(s) = f_str(L, data, "iwad")? {
            c.iwad = Some(file_name(L, "iwad", &s)?);
        }
        if let Some(a) = f_arr(L, data, "pwads")? {
            for v in a {
                let s = v
                    .as_str()
                    .ok_or_else(|| err(L, "`pwads` entries must be strings"))?;
                c.pwads.push(file_name(L, "pwad", s)?);
            }
        }
        if let Some(a) = f_arr(L, data, "playertranslations")? {
            if a.len() < 4 {
                return Err(err(L, "`playertranslations` needs at least 4 entries"));
            }
            let mut v = Vec::with_capacity(a.len());
            for x in a {
                v.push(
                    x.as_str()
                        .ok_or_else(|| err(L, "`playertranslations` entries must be strings"))?
                        .to_owned(),
                );
            }
            c.player_translations = Some(v);
        }
        if let Some(s) = f_str(L, data, "executable")? {
            c.executable = Some(
                Executable::parse(&s).ok_or_else(|| err(L, format!("unknown executable {s:?}")))?,
            );
        }
        if let Some(s) = f_str(L, data, "mode")? {
            c.mode =
                Some(GameMode::parse(&s).ok_or_else(|| err(L, format!("unknown mode {s:?}")))?);
        }
        if let Some(s) = f_str(L, data, "options")? {
            c.options = s;
        }
        Ok(c)
    }

    /// Fold a later lump into `self` using the spec's resolution rules:
    /// non-null scalars overwrite, `pwads` and `options` accumulate,
    /// `playertranslations` overwrites whole, `executable`/`mode` take the max.
    pub fn merge(&mut self, later: GameConf) {
        macro_rules! over {
            ($f:ident) => {
                if later.$f.is_some() {
                    self.$f = later.$f;
                }
            };
        }
        over!(title);
        over!(author);
        over!(description);
        over!(version);
        over!(iwad);
        over!(player_translations);
        self.pwads.extend(later.pwads);
        self.executable = self.executable.max(later.executable);
        self.mode = self.mode.max(later.mode);
        if !later.options.is_empty() {
            if !self.options.is_empty() && !self.options.ends_with('\n') {
                self.options.push('\n');
            }
            self.options.push_str(&later.options);
        }
    }

    /// Non-empty trimmed option lines.
    pub fn option_lines(&self) -> impl Iterator<Item = &str> {
        self.options
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
    }
}

/// ID24 data collected by the definitions builder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Id24Defs {
    /// `SKYDEFS` (last lump wins).
    pub skydefs: Option<SkyDefs>,
    /// `DEMOLOOP` (last lump wins).
    pub demoloop: Option<DemoLoop>,
    /// `GAMECONF` merged across lumps in load order.
    pub gameconf: GameConf,
    /// Other fixed-name JSON lumps (`TRAKINFO`, `SBARDEF`), keyed by upper-case lump name.
    pub raw: std::collections::BTreeMap<String, RawJsonLump>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn lump(kind: &str, data: &str) -> String {
        format!(r#"{{"type":"{kind}","version":"1.0.0","data":{data}}}"#)
    }

    #[test]
    fn envelope_checks_type_and_version() {
        assert!(parse_json_lump(&lump("skydefs", "{}"), Some("skydefs")).is_ok());
        assert!(parse_json_lump(&lump("skydefs", "{}"), Some("demoloop")).is_err());
        let v2 = r#"{"type":"skydefs","version":"2.0.0","data":{}}"#;
        assert!(parse_json_lump(v2, Some("skydefs")).is_err());
        assert!(parse_json_lump("[]", None).is_err());
        assert!(parse_json_lump(r#"{"type":"x","version":"1.0.0"}"#, None).is_err());
        let bom = format!("\u{feff}{}", lump("skydefs", "{}"));
        assert!(parse_json_lump(&bom, Some("SKYDEFS")).is_ok());
    }

    #[test]
    fn skydefs_all_three_kinds_and_flatmapping() {
        let d = lump(
            "skydefs",
            r#"{"skies":[
              {"type":0,"name":"SKY1","mid":100,"scrollx":2.5,"scalex":2},
              {"type":1,"name":"SKYFIRE","fire":{"palette":[0,1,2,255],"updatetime":0.03}},
              {"type":2,"name":"SKYHEX","foregroundtex":{"name":"CLOUDS","scrolly":4}}
            ],"flatmapping":[{"flat":"F_SKY1","sky":"SKY1"},{"flat":"F_SKYD","sky":"SKYFIRE"}]}"#,
        );
        let s = parse_skydefs(&d).unwrap();
        assert_eq!(s.skies.len(), 3);
        let a = s.sky("sky1").unwrap();
        assert_eq!(a.kind, SkyKind::Normal);
        assert_eq!(
            (a.mid, a.scroll_x, a.scale_x, a.scale_y),
            (100.0, 2.5, 2.0, 1.0)
        );
        let f = s.sky("SKYFIRE").unwrap();
        assert_eq!(f.fire.as_ref().unwrap().palette, vec![0, 1, 2, 255]);
        assert!((f.fire.as_ref().unwrap().update_time - 0.03).abs() < 1e-12);
        let h = s.sky("SKYHEX").unwrap();
        assert_eq!(h.foreground.as_ref().unwrap().name, "CLOUDS");
        assert_eq!(h.foreground.as_ref().unwrap().scroll_y, 4.0);
        assert_eq!(s.sky_for_flat("f_skyd"), Some("SKYFIRE"));
        assert_eq!(s.sky_for_flat("F_NONE"), None);
    }

    #[test]
    fn skydefs_rejects_bad_input() {
        let bad = |data: &str| parse_skydefs(&lump("skydefs", data)).is_err();
        assert!(bad(r#"{"skies":[]}"#));
        assert!(bad(r#"{"flatmapping":[]}"#));
        assert!(bad(r#"{"skies":[{"type":9,"name":"A"}]}"#));
        assert!(bad(r#"{"skies":[{"type":1,"name":"A"}]}"#)); // fire missing
        assert!(bad(
            r#"{"skies":[{"type":1,"name":"A","fire":{"palette":[],"updatetime":1}}]}"#
        ));
        assert!(bad(
            r#"{"skies":[{"type":1,"name":"A","fire":{"palette":[300],"updatetime":1}}]}"#
        ));
        assert!(bad(
            r#"{"skies":[{"type":1,"name":"A","fire":{"palette":[1],"updatetime":0}}]}"#
        ));
        assert!(bad(
            r#"{"skies":[{"type":0,"name":"A","fire":{"palette":[1],"updatetime":1}}]}"#
        ));
        assert!(bad(r#"{"skies":[{"type":2,"name":"A"}]}"#)); // foreground missing
        assert!(bad(
            r#"{"skies":[{"type":0,"name":"A","foregroundtex":{"name":"B"}}]}"#
        ));
        assert!(bad(r#"{"skies":[{"type":0}]}"#));
        // null arrays are allowed
        assert!(parse_skydefs(&lump("skydefs", r#"{"skies":null,"flatmapping":null}"#)).is_ok());
    }

    #[test]
    fn demoloop_entries() {
        let d = lump(
            "demoloop",
            r#"{"entries":[
              {"primarylump":"TITLEPIC","secondarylump":"D_DM2TTL","duration":11,"type":0,"outrowipe":1},
              {"primarylump":"DEMO1","type":1}
            ]}"#,
        );
        let l = parse_demoloop(&d).unwrap();
        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.entries[0].kind, DemoKind::Art);
        assert_eq!(l.entries[0].secondary.as_deref(), Some("D_DM2TTL"));
        assert_eq!(l.entries[0].duration, Some(11.0));
        assert_eq!(l.entries[0].wipe, OutroWipe::Melt);
        assert_eq!(l.entries[1].kind, DemoKind::Demo);
        assert_eq!(l.entries[1].wipe, OutroWipe::Immediate);
        let bad = |data: &str| parse_demoloop(&lump("demoloop", data)).is_err();
        assert!(bad(r#"{"entries":[]}"#));
        assert!(bad(r#"{"entries":null}"#));
        assert!(bad(r#"{"entries":[{"primarylump":"A","type":0}]}"#)); // art needs duration
        assert!(bad(
            r#"{"entries":[{"primarylump":"A","type":0,"duration":0}]}"#
        ));
        assert!(bad(
            r#"{"entries":[{"primarylump":"A","type":2,"duration":1}]}"#
        ));
        assert!(bad(r#"{"entries":[{"type":1}]}"#));
        assert!(bad(
            r#"{"entries":[{"primarylump":"A","type":1,"outrowipe":5}]}"#
        ));
    }

    #[test]
    fn gameconf_parse_and_validation() {
        let d = lump(
            "gameconf",
            r#"{"title":"T","iwad":"doom2.wad","pwads":["a.wad","b.pk3"],
                "playertranslations":["g","i","b","r"],"executable":"MBF21","mode":"commercial",
                "options":"comp_soul 1\ncomp_moveblock 0"}"#,
        );
        let c = GameConf::parse(&d).unwrap();
        assert_eq!(c.title.as_deref(), Some("T"));
        assert_eq!(c.iwad.as_deref(), Some("doom2.wad"));
        assert_eq!(c.pwads, vec!["a.wad", "b.pk3"]);
        assert_eq!(c.executable, Some(Executable::Mbf21));
        assert_eq!(c.mode, Some(GameMode::Commercial));
        assert_eq!(
            c.option_lines().collect::<Vec<_>>(),
            ["comp_soul 1", "comp_moveblock 0"]
        );
        let bad = |data: &str| GameConf::parse(&lump("gameconf", data)).is_err();
        assert!(bad(r#"{"iwad":"dir/doom2.wad"}"#));
        assert!(bad(r#"{"pwads":["..\\x.wad"]}"#));
        assert!(bad(r#"{"playertranslations":["a","b","c"]}"#));
        assert!(bad(r#"{"executable":"zdoom"}"#));
        assert!(bad(r#"{"mode":"shareware"}"#));
        assert!(GameConf::parse(&lump("gameconf", r#"{"title":null,"pwads":null}"#)).is_ok());
    }

    #[test]
    fn gameconf_merge_rules() {
        let a = GameConf::parse(&lump(
            "gameconf",
            r#"{"title":"Base","iwad":"doom.wad","pwads":["x.wad"],"executable":"boom2.02",
                "mode":"retail","playertranslations":["1","2","3","4"],"options":"a 1"}"#,
        ))
        .unwrap();
        let b = GameConf::parse(&lump(
            "gameconf",
            r#"{"title":null,"iwad":"doom2.wad","pwads":["y.wad"],"executable":"doom1.9",
                "mode":"commercial","playertranslations":["5","6","7","8","9"],"options":"b 2"}"#,
        ))
        .unwrap();
        let mut m = a;
        m.merge(b);
        assert_eq!(m.title.as_deref(), Some("Base")); // null keeps previous
        assert_eq!(m.iwad.as_deref(), Some("doom2.wad")); // overwrite
        assert_eq!(m.pwads, vec!["x.wad", "y.wad"]); // additive
        assert_eq!(m.executable, Some(Executable::Boom202)); // max
        assert_eq!(m.mode, Some(GameMode::Commercial)); // max
        assert_eq!(m.player_translations.as_ref().unwrap().len(), 5); // overwrite whole
        assert_eq!(m.option_lines().collect::<Vec<_>>(), ["a 1", "b 2"]); // accumulate
    }

    #[test]
    fn executable_ordering_matches_spec_list() {
        use Executable::*;
        let order = [
            Doom19,
            LimitRemoving,
            BugFixed,
            Boom202,
            Complevel9,
            Mbf,
            MbfExtra,
            Mbf21,
            Mbf21Ex,
            Id24,
        ];
        for w in order.windows(2) {
            assert!(w[0] < w[1]);
        }
    }
}

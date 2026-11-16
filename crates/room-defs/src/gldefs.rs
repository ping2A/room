//! GLDEFS parser (skyboxes + dynamic light defs — M10 bootstrap).

use crate::error::DefsError;
use room_sc::{Punct, ScanOptions, Scanner, Tok};
use std::collections::BTreeMap;

/// One GLDEFS skybox (cubemap faces).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkyboxDef {
    /// Skybox name (MAPINFO `skybox` / `Skybox`).
    pub name: String,
    /// Flip top face UV.
    pub flip_top: bool,
    /// Face textures: `up`, `down`, `north`, `east`, `south`, `west`
    /// (3-face form may only set `north`/`east`/`south`).
    pub faces: BTreeMap<String, String>,
}

/// Dynamic light kind (GZDoom GLDEFS subset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynLightKind {
    /// Constant point light.
    Point,
    /// Pulsing radius.
    Pulse,
    /// Flickering radius.
    Flicker,
    /// Random flicker (`flickerlight2`).
    FlickerRandom,
    /// Spotlight cone (`spotlight` — direction/angle still bootstrap).
    Spot,
    /// Sector-attached light (`sectorlight` — treated as point for now).
    Sector,
}

/// One named dynamic light definition (`pointlight` / `pulselight` / …).
#[derive(Clone, Debug, PartialEq)]
pub struct DynLightDef {
    /// Definition name (object frames reference this).
    pub name: String,
    /// Light behaviour.
    pub kind: DynLightKind,
    /// RGB 0..=1.
    pub color: [f32; 3],
    /// Size / radius (map units; GZDoom `size`).
    pub size: f32,
    /// Secondary size for pulse/flicker (`secondarySize`).
    pub secondary_size: f32,
    /// Interval in seconds (`interval`); 0 = unused.
    pub interval: f32,
    /// Offset XYZ from actor origin.
    pub offset: [f32; 3],
    /// Subtractive light.
    pub subtractive: bool,
    /// Attenuated (inverse-square-ish) vs additive Doom light.
    pub attenuate: bool,
    /// Do not light the owning actor.
    pub dont_light_self: bool,
    /// Spotlight outer cone half-angle in degrees (`angle`; 0 = point/omni).
    pub spot_angle: f32,
    /// Spotlight inner cone half-angle in degrees (`pitch` reused as inner, or `0` = ½ outer).
    pub spot_inner: f32,
}

impl Default for DynLightDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: DynLightKind::Point,
            color: [1.0, 1.0, 1.0],
            size: 64.0,
            secondary_size: 0.0,
            interval: 0.0,
            offset: [0.0; 3],
            subtractive: false,
            attenuate: false,
            dont_light_self: false,
            spot_angle: 0.0,
            spot_inner: 0.0,
        }
    }
}

/// Frame → light association (`object Class { frame A light Name }`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectLightAssoc {
    /// Actor class name.
    pub class: String,
    /// Frame letter (`A`..=`Z`) or empty = all frames.
    pub frame: String,
    /// Light definition name.
    pub light: String,
}

impl ObjectLightAssoc {
    /// Whether this `frame` key applies to an actor showing `sprite` frame
    /// `frame` (0 = `A`). GZDoom keys are a sprite name (`BON1`: every frame)
    /// or a sprite plus frame letters (`TROOE`); a bare letter (`F`) or an
    /// empty key is Room's shorthand for "this frame of any sprite" / "all".
    #[must_use]
    pub fn matches(&self, sprite: &str, frame: u8) -> bool {
        let key = self.frame.trim();
        let letter = char::from(b'A' + frame.min(25));
        match key.len() {
            0 => true,
            1 => key.eq_ignore_ascii_case(&letter.to_string()),
            n if n < 4 || !key.is_char_boundary(4) => false,
            _ => {
                let (spr, letters) = key.split_at(4);
                spr.eq_ignore_ascii_case(sprite)
                    && (letters.is_empty()
                        || letters.chars().any(|c| c.eq_ignore_ascii_case(&letter)))
            }
        }
    }
}

/// Target kind for `brightmap` / `material` (flat / sprite / texture / any).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlTexKind {
    /// Unspecified / any.
    #[default]
    Any,
    /// Floor/ceiling flat.
    Flat,
    /// Wall texture.
    Texture,
    /// Sprite lump.
    Sprite,
}

/// `brightmap [flat|sprite|texture] NAME { map BM … }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrightmapDef {
    /// Base graphic name (uppercase).
    pub name: String,
    /// Brightmap graphic name.
    pub map: String,
    /// Kind hint.
    pub kind: GlTexKind,
    /// `disablefullbright` — Bright frames ignore brightmap regions.
    pub disable_fullbright: bool,
}

/// One `Glow { Texture "NAME", color [, height] [, fullbright] }` entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlowTextureDef {
    /// Flat / texture name (uppercase).
    pub name: String,
    /// RGB 0..=1 (from hex triplet or ignored if only listed under Flats/Walls).
    pub color: [f32; 3],
    /// Glow height in map units (`0` = default).
    pub height: f32,
    /// Surface itself draws fullbright.
    pub fullbright: bool,
}

/// `material [flat|sprite|texture] NAME { brightmap … normal … }`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialDef {
    /// Base graphic name (uppercase).
    pub name: String,
    /// Kind hint.
    pub kind: GlTexKind,
    /// Optional brightmap.
    pub brightmap: Option<String>,
    /// Optional normal map.
    pub normal: Option<String>,
    /// Optional specular map.
    pub specular: Option<String>,
    /// Optional metallic map (PBR).
    pub metallic: Option<String>,
    /// Optional roughness map (PBR).
    pub roughness: Option<String>,
    /// Optional ambient occlusion.
    pub ao: Option<String>,
    /// Custom fragment shader path (`shader "file"` / HardwareShader).
    pub shader: Option<String>,
    /// Shader animation speed (`speed`).
    pub shader_speed: f32,
    /// Extra named texture uniforms (`texture name "file"`).
    pub shader_textures: BTreeMap<String, String>,
    /// `define NAME [= value]` preprocessor stubs (value may be empty).
    pub shader_defines: BTreeMap<String, String>,
    /// `disablefullbright`.
    pub disable_fullbright: bool,
}

/// Top-level `HardwareShader` / `hardwareshader` target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HardwareShaderKind {
    /// Bound to a texture / material (`HardwareShader Texture NAME`).
    #[default]
    Material,
    /// Post-process pass (`HardwareShader PostProcess NAME`).
    PostProcess,
}

/// `HardwareShader [Texture|PostProcess] NAME { shader "…" … }`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HardwareShaderDef {
    /// Texture or post-process name (uppercase for materials).
    pub name: String,
    /// Material vs post.
    pub kind: HardwareShaderKind,
    /// Shader source path / lump.
    pub shader: String,
    /// Animation speed.
    pub speed: f32,
    /// Named texture uniforms.
    pub textures: BTreeMap<String, String>,
    /// Define macros.
    pub defines: BTreeMap<String, String>,
    /// Post-process target (`HardwareShader PostProcess scene` → `scene`).
    pub target: String,
    /// `Uniform <type> <name>` declarations (post shaders), in order.
    pub uniforms: Vec<(String, String)>,
    /// `Enabled` — post shader active from the start.
    pub enabled: bool,
}

/// Compiled GLDEFS registry (subset).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlDefs {
    /// Skyboxes by lowercase name.
    pub skyboxes: BTreeMap<String, SkyboxDef>,
    /// Dynamic lights by lowercase name.
    pub lights: BTreeMap<String, DynLightDef>,
    /// Actor frame → light bindings.
    pub object_lights: Vec<ObjectLightAssoc>,
    /// Brightmaps keyed by base texture uppercase name.
    pub brightmaps: BTreeMap<String, BrightmapDef>,
    /// Glow texture entries (explicit `Texture` lines).
    pub glow_textures: Vec<GlowTextureDef>,
    /// Flat names listed under `Glow { Flats { … } }` (uppercase).
    pub glow_flats: Vec<String>,
    /// Wall texture names under `Glow { Walls { … } }` (uppercase).
    pub glow_walls: Vec<String>,
    /// Materials keyed by base texture uppercase name.
    pub materials: BTreeMap<String, MaterialDef>,
    /// Top-level `HardwareShader` entries keyed by uppercase name.
    pub hardware_shaders: BTreeMap<String, HardwareShaderDef>,
}

impl GlDefs {
    /// Built-in hitscan / imp attack lights (M10 bootstrap when IWAD has no GLDEFS).
    #[must_use]
    pub fn doom_attack_defaults() -> Self {
        let mut lights = BTreeMap::new();
        let mk = |name: &str, size: f32, color: [f32; 3]| {
            (
                name.to_ascii_lowercase(),
                DynLightDef {
                    name: name.to_owned(),
                    kind: DynLightKind::Point,
                    color,
                    size,
                    offset: [0.0, 0.0, 40.0],
                    ..DynLightDef::default()
                },
            )
        };
        for (k, v) in [
            mk("ZombieShot", 56.0, [1.0, 0.85, 0.4]),
            mk("ShotgunShot", 72.0, [1.0, 0.8, 0.35]),
            mk("ChaingunShot", 48.0, [1.0, 0.9, 0.45]),
            mk("ImpFire", 64.0, [1.0, 0.45, 0.15]),
        ] {
            lights.insert(k, v);
        }
        let object_lights = vec![
            ObjectLightAssoc {
                class: "ZombieMan".into(),
                frame: "F".into(),
                light: "ZombieShot".into(),
            },
            ObjectLightAssoc {
                class: "ShotgunGuy".into(),
                frame: "F".into(),
                light: "ShotgunShot".into(),
            },
            ObjectLightAssoc {
                class: "ChaingunGuy".into(),
                frame: "F".into(),
                light: "ChaingunShot".into(),
            },
            ObjectLightAssoc {
                class: "ChaingunGuy".into(),
                frame: "E".into(),
                light: "ChaingunShot".into(),
            },
            ObjectLightAssoc {
                class: "DoomImp".into(),
                frame: "G".into(),
                light: "ImpFire".into(),
            },
        ];
        Self {
            skyboxes: BTreeMap::new(),
            lights,
            object_lights,
            ..Default::default()
        }
    }

    /// Merge `other` on top (other wins on name collisions).
    pub fn merge_from(&mut self, other: GlDefs) {
        self.skyboxes.extend(other.skyboxes);
        self.lights.extend(other.lights);
        self.object_lights.extend(other.object_lights);
        self.brightmaps.extend(other.brightmaps);
        self.glow_textures.extend(other.glow_textures);
        self.glow_flats.extend(other.glow_flats);
        self.glow_walls.extend(other.glow_walls);
        self.materials.extend(other.materials);
        self.hardware_shaders.extend(other.hardware_shaders);
    }

    /// Brightmap lump for a texture / flat / sprite name, if defined.
    #[must_use]
    pub fn brightmap_for(&self, name: &str) -> Option<&str> {
        let key = name.trim().to_ascii_uppercase();
        if let Some(m) = self.materials.get(&key) {
            if let Some(bm) = m.brightmap.as_ref() {
                return Some(bm.as_str());
            }
        }
        self.brightmaps.get(&key).map(|b| b.map.as_str())
    }

    /// `true` when a GLDEFS brightmap disables an actor sprite's `Bright` state.
    /// UZDoom applies this override to the selected sprite texture; actor
    /// definitions commonly refer to the four-character sprite family instead
    /// of the resolved frame lump, so accept either spelling.
    #[must_use]
    pub fn disables_sprite_fullbright(&self, sprite: &str) -> bool {
        let key = sprite.trim().to_ascii_uppercase();
        self.brightmaps.iter().any(|(name, def)| {
            def.kind == GlTexKind::Sprite
                && def.disable_fullbright
                && (name == &key || name.starts_with(&key) || key.starts_with(name))
        })
    }

    /// `true` when this flat/texture should draw fullbright from Glow defs.
    #[must_use]
    pub fn glow_fullbright(&self, name: &str) -> bool {
        let key = name.trim().to_ascii_uppercase();
        self.glow_textures
            .iter()
            .any(|g| g.name == key && g.fullbright)
            || self.glow_flats.iter().any(|n| n == &key)
            || self.glow_walls.iter().any(|n| n == &key)
    }
}

/// Parse a GLDEFS lump (skyboxes + dynamic lights; other blocks skipped).
pub fn parse_gldefs(text: &str) -> Result<GlDefs, DefsError> {
    let mut sc = Scanner::from_str(
        text,
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
            ..Default::default()
        },
    );
    let mut info = GlDefs::default();
    loop {
        match sc.next_token()? {
            Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) => {
                let key = kw.to_ascii_lowercase();
                match key.as_str() {
                    "skybox" => {
                        let def = parse_skybox(&mut sc)?;
                        info.skyboxes.insert(def.name.to_ascii_lowercase(), def);
                    }
                    "pointlight" => {
                        let def = parse_light(&mut sc, DynLightKind::Point)?;
                        info.lights.insert(def.name.to_ascii_lowercase(), def);
                    }
                    "pulselight" => {
                        let def = parse_light(&mut sc, DynLightKind::Pulse)?;
                        info.lights.insert(def.name.to_ascii_lowercase(), def);
                    }
                    "flickerlight" => {
                        let def = parse_light(&mut sc, DynLightKind::Flicker)?;
                        info.lights.insert(def.name.to_ascii_lowercase(), def);
                    }
                    "flickerlight2" => {
                        let def = parse_light(&mut sc, DynLightKind::FlickerRandom)?;
                        info.lights.insert(def.name.to_ascii_lowercase(), def);
                    }
                    "spotlight" => {
                        let def = parse_light(&mut sc, DynLightKind::Spot)?;
                        info.lights.insert(def.name.to_ascii_lowercase(), def);
                    }
                    "sectorlight" => {
                        let def = parse_light(&mut sc, DynLightKind::Sector)?;
                        info.lights.insert(def.name.to_ascii_lowercase(), def);
                    }
                    "object" => {
                        let assoc = parse_object_lights(&mut sc)?;
                        info.object_lights.extend(assoc);
                    }
                    "brightmap" => {
                        let def = parse_brightmap(&mut sc)?;
                        info.brightmaps.insert(def.name.to_ascii_uppercase(), def);
                    }
                    "material" => {
                        let def = parse_material(&mut sc)?;
                        info.materials.insert(def.name.to_ascii_uppercase(), def);
                    }
                    "hardwareshader" => {
                        let def = parse_hardware_shader(&mut sc)?;
                        let key = def.name.to_ascii_uppercase();
                        // Also mirror material shaders into materials.shader.
                        if def.kind == HardwareShaderKind::Material && !def.shader.is_empty() {
                            let entry =
                                info.materials
                                    .entry(key.clone())
                                    .or_insert_with(|| MaterialDef {
                                        name: key.clone(),
                                        ..Default::default()
                                    });
                            entry.shader = Some(def.shader.clone());
                            entry.shader_speed = def.speed;
                            entry.shader_textures.extend(def.textures.clone());
                            entry.shader_defines.extend(def.defines.clone());
                        }
                        info.hardware_shaders.insert(key, def);
                    }
                    "glow" => {
                        parse_glow_block(&mut sc, &mut info)?;
                    }
                    _ => {
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
                }
            }
            other => {
                return Err(DefsError::Message(format!("GLDEFS: unexpected {other:?}")));
            }
        }
    }
    Ok(info)
}

fn parse_skybox(sc: &mut Scanner<'_>) -> Result<SkyboxDef, DefsError> {
    let name = match sc.next_token()? {
        Tok::Ident(s) => s.to_owned(),
        Tok::String(s) => s.into_owned(),
        other => {
            return Err(DefsError::Message(format!(
                "GLDEFS skybox: expected name, got {other:?}"
            )));
        }
    };
    let mut def = SkyboxDef {
        name,
        ..Default::default()
    };
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::LBrace) => break,
            Tok::Ident(f) if f.eq_ignore_ascii_case("fliptop") => def.flip_top = true,
            Tok::String(f) if f.eq_ignore_ascii_case("fliptop") => def.flip_top = true,
            other => {
                return Err(DefsError::Message(format!(
                    "GLDEFS skybox: expected `{{` or fliptop, got {other:?}"
                )));
            }
        }
    }
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(face) => {
                let face_key = face.to_ascii_lowercase();
                let tex = read_name(sc)?;
                let _ = sc.check_punct(Punct::Semi)?;
                def.faces.insert(face_key, tex);
            }
            other => {
                return Err(DefsError::Message(format!(
                    "GLDEFS skybox body: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(def)
}

fn parse_light(sc: &mut Scanner<'_>, kind: DynLightKind) -> Result<DynLightDef, DefsError> {
    let name = read_name(sc)?;
    let mut def = DynLightDef {
        name,
        kind,
        ..DynLightDef::default()
    };
    expect_lbrace(sc)?;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(key) => {
                let k = key.to_ascii_lowercase();
                match k.as_str() {
                    "color" => {
                        def.color = [read_f32(sc)?, read_f32(sc)?, read_f32(sc)?];
                    }
                    "size" => def.size = read_f32(sc)?,
                    "secondarysize" => def.secondary_size = read_f32(sc)?,
                    "interval" => def.interval = read_f32(sc)?,
                    "chance" => {
                        // Flicker probability — stored in interval for bootstrap.
                        def.interval = read_f32(sc)?;
                    }
                    "offset" => {
                        def.offset = [read_f32(sc)?, read_f32(sc)?, read_f32(sc)?];
                    }
                    "angle" => def.spot_angle = read_f32(sc)?,
                    "pitch" | "innerangle" | "spotinner" => def.spot_inner = read_f32(sc)?,
                    "subtractive" | "attenuate" | "dontlightself" => {
                        if matches!(k.as_str(), "subtractive") {
                            def.subtractive = true;
                        } else if matches!(k.as_str(), "attenuate") {
                            def.attenuate = true;
                        } else {
                            def.dont_light_self = true;
                        }
                        // GZDoom allows optional `1`/`0` after flag keywords.
                        match sc.next_token()? {
                            Tok::Int(_) | Tok::Float(_) => {}
                            Tok::Ident(v)
                                if Scanner::compare(v, "true") || Scanner::compare(v, "false") => {}
                            other => sc.unget(other),
                        }
                    }
                    _ => {
                        // Skip one value token for unknown keys.
                        let _ = sc.next_token()?;
                    }
                }
                let _ = sc.check_punct(Punct::Semi)?;
            }
            other => {
                return Err(DefsError::Message(format!(
                    "GLDEFS light: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(def)
}

fn parse_object_lights(sc: &mut Scanner<'_>) -> Result<Vec<ObjectLightAssoc>, DefsError> {
    let class = read_name(sc)?;
    expect_lbrace(sc)?;
    let mut out = Vec::new();
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) if kw.eq_ignore_ascii_case("frame") => {
                let frame = read_name(sc)?;
                // GZDoom: `frame NAME light L` or `frame NAME { light L ; … }`
                let braced = sc.check_punct(Punct::LBrace)?;
                loop {
                    match sc.next_token()? {
                        Tok::Ident(k) if k.eq_ignore_ascii_case("light") => {
                            let light = read_name(sc)?;
                            out.push(ObjectLightAssoc {
                                class: class.clone(),
                                frame: frame.clone(),
                                light,
                            });
                        }
                        Tok::Punct(Punct::Semi) => {
                            if !braced {
                                break;
                            }
                        }
                        Tok::Punct(Punct::RBrace) => {
                            if braced {
                                // End of `frame { … }` — stay in object.
                                break;
                            }
                            // Bare `frame … light L` hit object `}` — done.
                            return Ok(out);
                        }
                        Tok::Eof => break,
                        other if !braced => {
                            // Next object key (e.g. another `frame`) — put back.
                            sc.unget(other);
                            break;
                        }
                        _ => break,
                    }
                }
            }
            Tok::Ident(_) => loop {
                match sc.next_token()? {
                    Tok::Punct(Punct::Semi) | Tok::Eof => break,
                    Tok::Punct(Punct::RBrace) => return Ok(out),
                    _ => {}
                }
            },
            other => {
                return Err(DefsError::Message(format!(
                    "GLDEFS object: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(out)
}

fn parse_tex_kind(sc: &mut Scanner<'_>) -> Result<(GlTexKind, String), DefsError> {
    let first = read_name(sc)?;
    let lower = first.to_ascii_lowercase();
    match lower.as_str() {
        "flat" => Ok((GlTexKind::Flat, read_name(sc)?)),
        "sprite" => Ok((GlTexKind::Sprite, read_name(sc)?)),
        "texture" => Ok((GlTexKind::Texture, read_name(sc)?)),
        _ => Ok((GlTexKind::Any, first)),
    }
}

fn parse_brightmap(sc: &mut Scanner<'_>) -> Result<BrightmapDef, DefsError> {
    let (kind, name) = parse_tex_kind(sc)?;
    let mut def = BrightmapDef {
        name: name.to_ascii_uppercase(),
        kind,
        ..Default::default()
    };
    expect_lbrace(sc)?;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(key) => {
                let k = key.to_ascii_lowercase();
                match k.as_str() {
                    "map" => def.map = read_name_or_path(sc)?,
                    "disablefullbright" => def.disable_fullbright = true,
                    "iwad" | "thiswad" => {}
                    _ => {
                        let _ = sc.next_token()?;
                    }
                }
                let _ = sc.check_punct(Punct::Semi)?;
            }
            other => {
                return Err(DefsError::Message(format!(
                    "GLDEFS brightmap: unexpected {other:?}"
                )));
            }
        }
    }
    if def.map.is_empty() {
        return Err(DefsError::Message("GLDEFS brightmap: missing `map`".into()));
    }
    Ok(def)
}

fn parse_material(sc: &mut Scanner<'_>) -> Result<MaterialDef, DefsError> {
    let (kind, name) = parse_tex_kind(sc)?;
    let mut def = MaterialDef {
        name: name.to_ascii_uppercase(),
        kind,
        ..Default::default()
    };
    expect_lbrace(sc)?;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(key) => {
                let k = key.to_ascii_lowercase();
                match k.as_str() {
                    "brightmap" => def.brightmap = Some(read_name_or_path(sc)?),
                    "normal" => def.normal = Some(read_name_or_path(sc)?),
                    "specular" => def.specular = Some(read_name_or_path(sc)?),
                    "metallic" => def.metallic = Some(read_name_or_path(sc)?),
                    "roughness" => def.roughness = Some(read_name_or_path(sc)?),
                    "ao" => def.ao = Some(read_name_or_path(sc)?),
                    "disablefullbright" => def.disable_fullbright = true,
                    "iwad" | "thiswad" | "nomipmap" => {}
                    "glossiness" | "specularlevel" => {
                        let _ = read_f32(sc)?;
                    }
                    "shader" => {
                        def.shader = Some(read_name_or_path(sc)?);
                    }
                    "speed" => {
                        def.shader_speed = read_f32(sc)?;
                    }
                    "define" => {
                        let (n, v) = read_define(sc)?;
                        def.shader_defines.insert(n, v);
                    }
                    "texture" => {
                        let (n, path) = read_shader_texture(sc)?;
                        def.shader_textures.insert(n, path);
                    }
                    _ => {
                        let _ = sc.next_token()?;
                    }
                }
                let _ = sc.check_punct(Punct::Semi)?;
            }
            other => {
                return Err(DefsError::Message(format!(
                    "GLDEFS material: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(def)
}

fn parse_hardware_shader(sc: &mut Scanner<'_>) -> Result<HardwareShaderDef, DefsError> {
    let mut kind = HardwareShaderKind::Material;
    // Optional `Texture` / `PostProcess` then NAME.
    let name = match sc.next_token()? {
        Tok::Ident(kw) => {
            let k = kw.to_ascii_lowercase();
            match k.as_str() {
                "texture" | "sprite" | "flat" => {
                    kind = HardwareShaderKind::Material;
                    read_name_or_path(sc)?
                }
                "postprocess" | "post" => {
                    kind = HardwareShaderKind::PostProcess;
                    // GZDoom: `PostProcess <target> { Name "…" … }`.
                    read_name_or_path(sc)?
                }
                _ => kw.to_owned(),
            }
        }
        Tok::String(s) => s.into_owned(),
        other => {
            return Err(DefsError::Message(format!(
                "GLDEFS HardwareShader: unexpected {other:?}"
            )));
        }
    };
    let mut def = HardwareShaderDef {
        name: if kind == HardwareShaderKind::PostProcess {
            name
        } else {
            name.to_ascii_uppercase()
        },
        kind,
        ..Default::default()
    };
    if kind == HardwareShaderKind::PostProcess {
        def.target = def.name.to_ascii_lowercase();
    }
    expect_lbrace(sc)?;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(key) => {
                let k = key.to_ascii_lowercase();
                match k.as_str() {
                    "shader" | "wgsl" => {
                        def.shader = read_name_or_path(sc)?;
                        // GZDoom post: `Shader "file.fp" 330` (GLSL version).
                        match sc.next_token()? {
                            Tok::Int(_) => {}
                            other => sc.unget(other),
                        }
                    }
                    "name" if def.kind == HardwareShaderKind::PostProcess => {
                        def.name = read_name_or_path(sc)?;
                    }
                    "uniform" => {
                        let ty = read_name(sc)?.to_ascii_lowercase();
                        let name = read_name(sc)?;
                        def.uniforms.push((ty, name));
                    }
                    "enabled" => def.enabled = true,
                    "speed" => def.speed = read_f32(sc)?,
                    "define" => {
                        let (n, v) = read_define(sc)?;
                        def.defines.insert(n, v);
                    }
                    "texture" => {
                        let (n, path) = read_shader_texture(sc)?;
                        def.textures.insert(n, path);
                    }
                    _ => {
                        let _ = sc.next_token()?;
                    }
                }
                let _ = sc.check_punct(Punct::Semi)?;
            }
            other => {
                return Err(DefsError::Message(format!(
                    "GLDEFS HardwareShader: unexpected {other:?}"
                )));
            }
        }
    }
    if def.shader.is_empty() {
        return Err(DefsError::Message(
            "GLDEFS HardwareShader: missing `shader`".into(),
        ));
    }
    Ok(def)
}

fn read_define(sc: &mut Scanner<'_>) -> Result<(String, String), DefsError> {
    let name = read_name(sc)?;
    let value = if sc.check_punct(Punct::Eq)? {
        match sc.next_token()? {
            Tok::Ident(v) => v.to_owned(),
            Tok::String(v) => v.into_owned(),
            Tok::Int(i) => i.to_string(),
            Tok::Float(f) => f.to_string(),
            _ => String::new(),
        }
    } else {
        String::new()
    };
    Ok((name, value))
}

fn read_shader_texture(sc: &mut Scanner<'_>) -> Result<(String, String), DefsError> {
    let name = read_name(sc)?;
    let path = read_name_or_path(sc)?;
    Ok((name, path))
}

fn parse_glow_block(sc: &mut Scanner<'_>, info: &mut GlDefs) -> Result<(), DefsError> {
    expect_lbrace(sc)?;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::RBrace) | Tok::Eof => break,
            Tok::Punct(Punct::Semi) => {}
            Tok::Ident(kw) => {
                let k = kw.to_ascii_lowercase();
                match k.as_str() {
                    "flats" => {
                        expect_lbrace(sc)?;
                        loop {
                            match sc.next_token()? {
                                Tok::Punct(Punct::RBrace) | Tok::Eof => break,
                                Tok::Punct(Punct::Semi) => {}
                                Tok::Ident(n) => {
                                    info.glow_flats.push(n.to_ascii_uppercase());
                                }
                                Tok::String(n) => {
                                    info.glow_flats.push(n.to_ascii_uppercase());
                                }
                                _ => {}
                            }
                        }
                    }
                    "walls" => {
                        expect_lbrace(sc)?;
                        loop {
                            match sc.next_token()? {
                                Tok::Punct(Punct::RBrace) | Tok::Eof => break,
                                Tok::Punct(Punct::Semi) => {}
                                Tok::Ident(n) => {
                                    info.glow_walls.push(n.to_ascii_uppercase());
                                }
                                Tok::String(n) => {
                                    info.glow_walls.push(n.to_ascii_uppercase());
                                }
                                _ => {}
                            }
                        }
                    }
                    "texture" => {
                        let name = read_name(sc)?;
                        let _ = sc.check_punct(Punct::Comma)?;
                        let color = read_glow_color(sc)?;
                        let mut height = 0.0f32;
                        let mut fullbright = false;
                        loop {
                            match sc.next_token()? {
                                Tok::Punct(Punct::Semi) | Tok::Eof => break,
                                Tok::Punct(Punct::Comma) => {}
                                Tok::Punct(Punct::RBrace) => {
                                    info.glow_textures.push(GlowTextureDef {
                                        name: name.to_ascii_uppercase(),
                                        color,
                                        height,
                                        fullbright,
                                    });
                                    return Ok(());
                                }
                                Tok::Ident(w) if w.eq_ignore_ascii_case("fullbright") => {
                                    fullbright = true;
                                }
                                Tok::Float(f) => height = f as f32,
                                Tok::Int(n) => height = n as f32,
                                _ => {}
                            }
                        }
                        info.glow_textures.push(GlowTextureDef {
                            name: name.to_ascii_uppercase(),
                            color,
                            height,
                            fullbright,
                        });
                    }
                    _ => {
                        if sc.check_punct(Punct::LBrace)? {
                            skip_block(sc)?;
                        }
                    }
                }
            }
            other => {
                return Err(DefsError::Message(format!(
                    "GLDEFS glow: unexpected {other:?}"
                )));
            }
        }
    }
    Ok(())
}

fn read_glow_color(sc: &mut Scanner<'_>) -> Result<[f32; 3], DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => parse_hex_rgb(s),
        Tok::String(s) => parse_hex_rgb(&s),
        Tok::Float(r) => {
            let g = read_f32(sc)?;
            let b = read_f32(sc)?;
            Ok([r as f32, g, b])
        }
        Tok::Int(r) => {
            // Hex without prefix sometimes arrives as int; treat 0xRRGGBB.
            if (0x000000..=0xFFFFFF).contains(&r) && r > 255 {
                let u = r as u32;
                Ok([
                    ((u >> 16) & 0xFF) as f32 / 255.0,
                    ((u >> 8) & 0xFF) as f32 / 255.0,
                    (u & 0xFF) as f32 / 255.0,
                ])
            } else {
                let g = read_f32(sc)?;
                let b = read_f32(sc)?;
                Ok([r as f32, g, b])
            }
        }
        other => Err(DefsError::Message(format!(
            "GLDEFS glow: bad color {other:?}"
        ))),
    }
}

fn parse_hex_rgb(s: &str) -> Result<[f32; 3], DefsError> {
    let t = s.trim().trim_start_matches('#');
    if t.len() == 6 {
        if let Ok(u) = u32::from_str_radix(t, 16) {
            return Ok([
                ((u >> 16) & 0xFF) as f32 / 255.0,
                ((u >> 8) & 0xFF) as f32 / 255.0,
                (u & 0xFF) as f32 / 255.0,
            ]);
        }
    }
    // Unrecognized X11 name — neutral warm glow.
    Ok([1.0, 0.85, 0.6])
}

fn expect_lbrace(sc: &mut Scanner<'_>) -> Result<(), DefsError> {
    match sc.next_token()? {
        Tok::Punct(Punct::LBrace) => Ok(()),
        other => Err(DefsError::Message(format!(
            "GLDEFS: expected `{{`, got {other:?}"
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
            .map_err(|_| DefsError::Message(format!("GLDEFS: bad number {s}")))?,
        other => {
            return Err(DefsError::Message(format!(
                "GLDEFS: expected number, got {other:?}"
            )));
        }
    };
    Ok(if neg { -v } else { v })
}

fn read_name(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    match sc.next_token()? {
        Tok::Ident(s) => Ok(s.to_owned()),
        Tok::String(s) => Ok(s.into_owned()),
        // `pointlight 2SHADOW` — leading digits lex as Int then Ident.
        Tok::Int(n) => {
            let mut s = n.to_string();
            match sc.next_token()? {
                Tok::Ident(id) => {
                    s.push_str(id);
                    Ok(s)
                }
                other => {
                    sc.unget(other);
                    Ok(s)
                }
            }
        }
        other => Err(DefsError::Message(format!(
            "GLDEFS: expected name, got {other:?}"
        ))),
    }
}

/// Name or PK3 path (`brightmaps/foo.png` — slashes join idents).
fn read_name_or_path(sc: &mut Scanner<'_>) -> Result<String, DefsError> {
    let mut out = read_name(sc)?;
    while sc.check_punct(Punct::Slash)? {
        out.push('/');
        out.push_str(&read_name(sc)?);
    }
    Ok(out)
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
    fn parses_six_face_skybox() {
        let text = r#"
skybox MyBox fliptop
{
	up SKYUP
	down SKYDN
	north SKYN
	east SKYE
	south SKYS
	west SKYW
}
"#;
        let info = parse_gldefs(text).unwrap();
        let box_ = info.skyboxes.get("mybox").unwrap();
        assert!(box_.flip_top);
        assert_eq!(box_.faces.get("up").map(String::as_str), Some("SKYUP"));
        assert_eq!(box_.faces.len(), 6);
    }

    #[test]
    fn parses_pointlight_and_object_frame() {
        let text = r#"
pointlight ZombieShot
{
	color 1.0 0.8 0.4
	size 48
	offset 0 0 40
}
object ZombieMan
{
	frame F light ZombieShot
}
object DoomImp
{
	frame TROOE { light ZombieShot }
}
"#;
        let info = parse_gldefs(text).unwrap();
        let lit = info.lights.get("zombieshot").unwrap();
        assert_eq!(lit.kind, DynLightKind::Point);
        assert!((lit.size - 48.0).abs() < 1e-3);
        assert!((lit.color[0] - 1.0).abs() < 1e-3);
        assert_eq!(info.object_lights.len(), 2);
        assert_eq!(info.object_lights[0].class, "ZombieMan");
        assert_eq!(info.object_lights[0].frame, "F");
        assert_eq!(info.object_lights[0].light, "ZombieShot");
        assert_eq!(info.object_lights[1].frame, "TROOE");
    }

    #[test]
    fn object_frame_keys_match_like_gzdoom() {
        let a = |frame: &str| ObjectLightAssoc {
            class: "HealthPlus".into(),
            frame: frame.into(),
            light: "BLUEITEM".into(),
        };
        assert!(a("BON1").matches("BON1", 0));
        assert!(a("BON1").matches("bon1", 3));
        assert!(!a("BON1").matches("BON2", 0));
        assert!(a("TROOE").matches("TROO", 4));
        assert!(!a("TROOE").matches("TROO", 3));
        assert!(a("F").matches("POSS", 5));
        assert!(a("").matches("POSS", 1));
    }

    #[test]
    fn parses_brightmap_material_and_glow() {
        let text = r#"
brightmap sprite POSSF1
{
	map brightmaps/POSSF1
}
material texture COMPTALL
{
	brightmap COMPTALL_BM
	normal COMPTALL_N
}
Glow
{
	Flats
	{
		NUKAGE1
	}
	Texture "CEIL1_2", FF8020, 64, fullbright
}
"#;
        let info = parse_gldefs(text).unwrap();
        let bm = info.brightmaps.get("POSSF1").unwrap();
        assert_eq!(bm.map, "brightmaps/POSSF1");
        assert_eq!(bm.kind, GlTexKind::Sprite);
        let mat = info.materials.get("COMPTALL").unwrap();
        assert_eq!(mat.brightmap.as_deref(), Some("COMPTALL_BM"));
        assert_eq!(mat.normal.as_deref(), Some("COMPTALL_N"));
        assert_eq!(info.glow_flats, vec!["NUKAGE1".to_owned()]);
        assert_eq!(info.glow_textures.len(), 1);
        assert!(info.glow_textures[0].fullbright);
        assert!(info.glow_fullbright("CEIL1_2"));
        assert_eq!(info.brightmap_for("comptall"), Some("COMPTALL_BM"));
    }

    #[test]
    fn parses_spotlight_and_sectorlight() {
        let text = r#"
spotlight Beam
{
	color 1.0 1.0 0.5
	size 128
	offset 0 0 40
	angle 45
	pitch 20
}
sectorlight RoomGlow
{
	color 0.2 0.2 0.4
	size 256
}
"#;
        let info = parse_gldefs(text).unwrap();
        let beam = info.lights.get("beam").unwrap();
        assert_eq!(beam.kind, DynLightKind::Spot);
        assert!((beam.spot_angle - 45.0).abs() < 1e-3);
        assert!((beam.spot_inner - 20.0).abs() < 1e-3);
        assert_eq!(
            info.lights.get("roomglow").unwrap().kind,
            DynLightKind::Sector
        );
    }

    #[test]
    fn parses_hardware_shader_and_material_shader() {
        let text = r#"
HardwareShader Texture WARPTEX
{
	Shader "shaders/warp.fp"
	Speed 0.5
	Texture noise "shaders/noise.png"
	Define STRENGTH = 2
}
material texture SHINY
{
	shader "shaders/shiny.fp"
	speed 1.25
}
"#;
        let info = parse_gldefs(text).unwrap();
        let hs = info.hardware_shaders.get("WARPTEX").unwrap();
        assert_eq!(hs.kind, HardwareShaderKind::Material);
        assert_eq!(hs.shader, "shaders/warp.fp");
        assert!((hs.speed - 0.5).abs() < 1e-4);
        assert_eq!(
            hs.textures.get("noise").map(String::as_str),
            Some("shaders/noise.png")
        );
        assert_eq!(hs.defines.get("STRENGTH").map(String::as_str), Some("2"));
        let mat = info.materials.get("WARPTEX").unwrap();
        assert_eq!(mat.shader.as_deref(), Some("shaders/warp.fp"));
        let shiny = info.materials.get("SHINY").unwrap();
        assert_eq!(shiny.shader.as_deref(), Some("shaders/shiny.fp"));
        assert!((shiny.shader_speed - 1.25).abs() < 1e-4);
    }

    #[test]
    fn parses_gzdoom_post_process_shader() {
        let text = r#"
HardwareShader PostProcess scene
{
	Name "WaterWarp"
	Shader "shaders/waterwarp.fp" 330
	Uniform float strength
	Uniform vec2 offset
	Texture noise "textures/noise.png"
	Enabled
}
"#;
        let info = parse_gldefs(text).unwrap();
        let hs = info.hardware_shaders.get("WATERWARP").unwrap();
        assert_eq!(hs.kind, HardwareShaderKind::PostProcess);
        assert_eq!(hs.name, "WaterWarp");
        assert_eq!(hs.target, "scene");
        assert_eq!(hs.shader, "shaders/waterwarp.fp");
        assert_eq!(
            hs.uniforms,
            vec![
                ("float".to_owned(), "strength".to_owned()),
                ("vec2".to_owned(), "offset".to_owned())
            ]
        );
        assert!(hs.enabled);
        assert!(hs.textures.contains_key("noise"));
    }
}

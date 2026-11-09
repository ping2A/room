//! CVar store (M5 subset).

use crate::error::ConsoleError;
use std::collections::BTreeMap;

/// CVar value kinds.
#[derive(Clone, Debug, PartialEq)]
pub enum CvarValue {
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i32),
    /// Float.
    Float(f64),
    /// String.
    String(String),
}

impl CvarValue {
    /// Parse from a console argument token.
    pub fn parse_as(ty: CvarType, s: &str) -> Result<Self, ConsoleError> {
        match ty {
            CvarType::Bool => match s.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Ok(Self::Bool(true)),
                "0" | "false" | "no" | "off" => Ok(Self::Bool(false)),
                _ => Err(ConsoleError::Message(format!("bad bool '{s}'"))),
            },
            CvarType::Int => s
                .parse()
                .map(Self::Int)
                .map_err(|_| ConsoleError::Message(format!("bad int '{s}'"))),
            CvarType::Float => s
                .parse()
                .map(Self::Float)
                .map_err(|_| ConsoleError::Message(format!("bad float '{s}'"))),
            CvarType::String => Ok(Self::String(s.to_owned())),
        }
    }

    /// Display for `get` / echo.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => format!("{f}"),
            Self::String(s) => s.clone(),
        }
    }
}

/// Declared type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CvarType {
    /// Bool.
    Bool,
    /// Int.
    Int,
    /// Float.
    Float,
    /// String.
    String,
}

/// CVar flags (subset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CvarFlags(pub u32);

impl CvarFlags {
    /// Persist to config.
    pub const ARCHIVE: Self = Self(1 << 0);
    /// Server-info (synced).
    pub const SERVERINFO: Self = Self(1 << 1);
    /// Cheat-gated.
    pub const CHEAT: Self = Self(1 << 2);
    /// Cannot be set by user.
    pub const NOSET: Self = Self(1 << 3);

    /// True if all bits in `other` are set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Bitwise or.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// One registered CVar.
#[derive(Clone, Debug)]
struct CvarEntry {
    ty: CvarType,
    value: CvarValue,
    default: CvarValue,
    flags: CvarFlags,
}

/// Named CVar table (BTreeMap for deterministic iteration).
#[derive(Clone, Debug, Default)]
pub struct CvarStore {
    entries: BTreeMap<String, CvarEntry>,
}

impl CvarStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a CVar. Overwrites if present.
    pub fn register(&mut self, name: &str, ty: CvarType, default: CvarValue, flags: CvarFlags) {
        let key = name.to_ascii_lowercase();
        self.entries.insert(
            key,
            CvarEntry {
                ty,
                value: default.clone(),
                default,
                flags,
            },
        );
    }

    /// Register common engine defaults used in M5.
    pub fn register_engine_defaults(&mut self) {
        self.register(
            "sv_cheats",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::SERVERINFO.union(CvarFlags::ARCHIVE),
        );
        self.register(
            "gravity",
            CvarType::Float,
            CvarValue::Float(1.0),
            CvarFlags::SERVERINFO,
        );
        self.register(
            "cl_run",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "cl_crosshair",
            CvarType::Bool,
            CvarValue::Bool(true),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "fullscreen",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "vid_fps",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "invertmouse",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "m_sensitivity",
            CvarType::Float,
            CvarValue::Float(1.0),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "mouse_look",
            CvarType::Bool,
            CvarValue::Bool(true),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "autoaim",
            CvarType::Bool,
            CvarValue::Bool(true),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_autoaim",
            CvarType::Int,
            CvarValue::Int(2),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_sound",
            CvarType::Bool,
            CvarValue::Bool(true),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_lockout",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_music",
            CvarType::Bool,
            CvarValue::Bool(true),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_talk",
            CvarType::Bool,
            CvarValue::Bool(true),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_ambience",
            CvarType::Bool,
            CvarValue::Bool(true),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_reverse_stereo",
            CvarType::Bool,
            CvarValue::Bool(false),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_sfx_volume",
            CvarType::Int,
            CvarValue::Int(255),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "duke_music_volume",
            CvarType::Int,
            CvarValue::Int(255),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "snd_menuvolume",
            CvarType::Float,
            CvarValue::Float(1.0),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "language",
            CvarType::String,
            CvarValue::String("auto".into()),
            CvarFlags::ARCHIVE,
        );
        // Userinfo `gender` (GZDoom default `neutral`): `$playersound` lookup.
        self.register(
            "gender",
            CvarType::String,
            CvarValue::String("neutral".into()),
            CvarFlags::ARCHIVE,
        );
        self.register(
            "name",
            CvarType::String,
            CvarValue::String(String::new()),
            CvarFlags::ARCHIVE,
        );
        self.register_shader_defaults();
    }

    /// Renderer / post / FX toggles (`docs/29-shaders.md`). All ARCHIVE so they
    /// persist; defaults keep the classic look (modern effects off).
    pub fn register_shader_defaults(&mut self) {
        let a = CvarFlags::ARCHIVE;
        let reg_i = |s: &mut Self, name: &str, v: i32| {
            s.register(name, CvarType::Int, CvarValue::Int(v), a);
        };
        let reg_f = |s: &mut Self, name: &str, v: f64| {
            s.register(name, CvarType::Float, CvarValue::Float(v), a);
        };
        let reg_b = |s: &mut Self, name: &str, v: bool| {
            s.register(name, CvarType::Bool, CvarValue::Bool(v), a);
        };

        reg_i(self, "r_quality", 0);
        reg_i(self, "gl_tonemap", 0);
        reg_f(self, "r_exposure", 1.0);
        reg_f(self, "r_gamma", 1.0);
        reg_i(self, "gl_fxaa", 0);
        reg_i(self, "r_aa", 0);
        reg_f(self, "r_sharpen", 0.0);
        reg_f(self, "vid_scalefactor", 1.0);
        reg_i(self, "vid_scalemode", 0);
        reg_b(self, "gl_bloom", false);
        reg_f(self, "gl_bloom_amount", 0.8);
        reg_i(self, "gl_ssao", 0);
        reg_f(self, "gl_ssao_strength", 1.0);
        reg_i(self, "r_postchain", 0); // 0 auto, 1 force on, 2 force off
        reg_i(self, "r_dither", 0);
        reg_b(self, "r_banding", false);
        reg_b(self, "r_crt", false);
        reg_i(self, "r_crt_preset", 0);
        reg_b(self, "r_softparticles", false);
        reg_b(self, "r_outlines", false);
        reg_b(self, "r_hitflash", false);
        reg_b(self, "r_colorgrade", false);
        reg_b(self, "r_damagefx", false);
        reg_b(self, "r_menublur", false);
        reg_b(self, "r_hudfx", false);
        reg_i(self, "gl_shadowmap_filter", 0);
        reg_b(self, "gl_light_sprites", false);
        reg_b(self, "r_autonormals", false);
        reg_b(self, "r_lightshafts", false);
        reg_b(self, "r_volfog", false);
        reg_b(self, "r_ambientbounce", false);
        reg_b(self, "gl_lightmap", false);
        reg_i(self, "r_liquids", 0);
        reg_b(self, "r_heathaze", false);
        reg_b(self, "r_parallax", false);
        reg_b(self, "r_ssr", false);
        reg_b(self, "r_detail", false);
        reg_b(self, "r_skyfx", false);
        reg_b(self, "r_decalfx", false);
        reg_b(self, "r_particlefx", false);
        reg_b(self, "r_dissolve", false);
        reg_b(self, "r_actorspriteshadow", false);
        reg_b(self, "r_autoexposure", false);
        reg_b(self, "r_powerupfx", false);
        reg_b(self, "r_underwaterfx", false);
        reg_f(self, "r_vignette", 0.0);
        reg_f(self, "r_grain", 0.0);
        reg_f(self, "r_chromatic", 0.0);
        reg_b(self, "r_dof", false);
        reg_b(self, "r_motionblur", false);
        reg_b(self, "r_muzzlefx", false);
        reg_b(self, "r_tracers", false);
        reg_b(self, "r_impactfx", false);
        reg_b(self, "r_viewkick", false);
        reg_f(self, "r_viewkick_scale", 1.0);
        reg_f(self, "r_quakeintensity", 1.0);
        reg_b(self, "r_weaponsmoke", false);
        reg_b(self, "r_casings", false);
        reg_b(self, "r_explosionfx", false);
        reg_b(self, "r_trails", false);
        reg_b(self, "r_hitconfirm", false);
        reg_b(self, "r_bloodfx", false);
        reg_b(self, "r_blooddrips", false);
        reg_b(self, "r_bloodwet", false);
        reg_b(self, "r_corpsepools", false);
        reg_b(self, "r_gibs", false);
        reg_b(self, "r_screenblood", false);
        reg_b(self, "r_weaponblood", false);
        reg_b(self, "r_wounds", false);
        reg_i(self, "r_gore", 1);
        reg_b(self, "r_reduce_flashing", false);
        reg_i(self, "r_extralight", 0);
        reg_b(self, "showendoom", false);
        reg_b(self, "use_joystick", true);
        reg_f(self, "joy_deadzone", 0.2);
        reg_f(self, "joy_curve", 1.6);
        reg_f(self, "joy_sensitivity", 1.0);
        reg_f(self, "joy_speedmultiplier", 1.0);
        reg_b(self, "joy_invertpitch", false);
        reg_i(self, "m_doubleclicktime", 250);
        reg_b(self, "snd_reverb", true);
        reg_b(self, "snd_waterreverb", true);
        reg_i(self, "gl_texture_filter_anisotropic", 1);
        reg_b(self, "r_gpuprofile", false);
    }

    /// Expand `r_quality` into individual shader CVars (classic → ultra).
    pub fn apply_quality_preset(&mut self, level: i32) {
        let level = level.clamp(0, 3);
        let _ = self.set("r_quality", &level.to_string());
        // Always reset the modern stack, then layer the preset.
        let writes: &[(&str, &str)] = match level {
            0 => &[
                ("gl_tonemap", "0"),
                ("r_exposure", "1"),
                ("gl_fxaa", "0"),
                ("r_aa", "0"),
                ("r_sharpen", "0"),
                ("gl_bloom", "0"),
                ("gl_bloom_amount", "0.8"),
                ("gl_ssao", "0"),
                ("gl_ssao_strength", "1"),
                ("r_softparticles", "0"),
                ("r_outlines", "0"),
                ("r_hitflash", "0"),
                ("r_colorgrade", "0"),
                ("r_damagefx", "0"),
                ("r_liquids", "0"),
                ("r_heathaze", "0"),
                ("gl_shadowmap_filter", "0"),
                ("r_lightshafts", "0"),
                ("r_autoexposure", "0"),
                ("r_particlefx", "0"),
                ("r_weaponsmoke", "0"),
                ("r_casings", "0"),
                ("r_muzzlefx", "0"),
                ("r_tracers", "0"),
                ("r_impactfx", "0"),
                ("r_viewkick", "0"),
                ("r_explosionfx", "0"),
                ("r_trails", "0"),
                ("r_hitconfirm", "0"),
                ("r_bloodfx", "0"),
                ("r_corpsepools", "0"),
                ("r_gore", "1"),
            ],
            1 => &[
                ("gl_tonemap", "8"), // RoomSoft
                ("gl_fxaa", "1"),
                ("r_aa", "1"),
                ("gl_bloom", "1"),
                ("gl_bloom_amount", "0.6"),
                ("gl_ssao", "0"),
                ("r_softparticles", "1"),
                ("r_hitflash", "1"),
                ("r_outlines", "0"),
                ("r_colorgrade", "0"),
                ("r_damagefx", "0"),
                ("r_liquids", "0"),
                ("r_heathaze", "0"),
                ("gl_shadowmap_filter", "0"),
                ("r_lightshafts", "0"),
                ("r_autoexposure", "0"),
                ("r_sharpen", "0"),
                ("r_muzzlefx", "1"),
                ("r_tracers", "1"),
                ("r_impactfx", "1"),
                ("r_viewkick", "1"),
                ("r_explosionfx", "1"),
            ],
            2 => &[
                ("gl_tonemap", "6"), // AgX
                ("gl_fxaa", "2"),
                ("r_aa", "2"),
                ("gl_bloom", "1"),
                ("gl_bloom_amount", "0.75"),
                ("gl_ssao", "1"),
                ("gl_ssao_strength", "1"),
                ("r_softparticles", "1"),
                ("r_outlines", "1"),
                ("r_hitflash", "1"),
                ("r_colorgrade", "1"),
                ("r_damagefx", "1"),
                ("r_liquids", "1"),
                ("gl_shadowmap_filter", "1"),
                ("r_sharpen", "0.2"),
                ("r_heathaze", "0"),
                ("r_lightshafts", "0"),
                ("r_autoexposure", "0"),
            ],
            _ => &[
                ("gl_tonemap", "7"), // Khronos Neutral
                ("gl_fxaa", "3"),
                ("r_aa", "3"),
                ("gl_bloom", "1"),
                ("gl_bloom_amount", "0.85"),
                ("gl_ssao", "3"),
                ("gl_ssao_strength", "1.2"),
                ("r_softparticles", "1"),
                ("r_outlines", "1"),
                ("r_hitflash", "1"),
                ("r_colorgrade", "1"),
                ("r_damagefx", "1"),
                ("r_liquids", "2"),
                ("r_heathaze", "1"),
                ("gl_shadowmap_filter", "2"),
                ("r_lightshafts", "1"),
                ("r_autoexposure", "1"),
                ("r_sharpen", "0.35"),
                ("r_particlefx", "1"),
                ("r_weaponsmoke", "1"),
                ("r_casings", "1"),
                ("r_gore", "2"),
                ("r_bloodfx", "1"),
                ("r_corpsepools", "1"),
            ],
        };
        for (name, val) in writes {
            let _ = self.set(name, val);
        }
    }

    /// Names with [`CvarFlags::ARCHIVE`] (for config write).
    #[must_use]
    pub fn archive_names(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.flags.contains(CvarFlags::ARCHIVE))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Get value by name.
    pub fn get(&self, name: &str) -> Option<&CvarValue> {
        self.entries
            .get(&name.to_ascii_lowercase())
            .map(|e| &e.value)
    }

    /// Set value; respects `NOSET`.
    pub fn set(&mut self, name: &str, raw: &str) -> Result<(), ConsoleError> {
        let key = name.to_ascii_lowercase();
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| ConsoleError::Unknown(name.to_owned()))?;
        if entry.flags.contains(CvarFlags::NOSET) {
            return Err(ConsoleError::Message(format!("{name} is read-only")));
        }
        entry.value = CvarValue::parse_as(entry.ty, raw)?;
        Ok(())
    }

    /// Toggle a bool CVar.
    pub fn toggle(&mut self, name: &str) -> Result<bool, ConsoleError> {
        let key = name.to_ascii_lowercase();
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| ConsoleError::Unknown(name.to_owned()))?;
        match &mut entry.value {
            CvarValue::Bool(b) => {
                *b = !*b;
                Ok(*b)
            }
            _ => Err(ConsoleError::Message(format!("{name} is not bool"))),
        }
    }

    /// Reset to default.
    pub fn reset(&mut self, name: &str) -> Result<(), ConsoleError> {
        let key = name.to_ascii_lowercase();
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| ConsoleError::Unknown(name.to_owned()))?;
        entry.value = entry.default.clone();
        Ok(())
    }

    /// Sorted names.
    pub fn list(&self) -> impl Iterator<Item = (&str, &CvarValue)> {
        self.entries.iter().map(|(k, e)| (k.as_str(), &e.value))
    }
}

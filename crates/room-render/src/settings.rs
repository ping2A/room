//! Render quality / post CVars (`docs/29-shaders.md`).
//!
//! Every catalogue effect has an enable path here. Unimplemented effects are
//! stored and reported so shells/console can toggle them; [`Self::needs_post`]
//! only considers effects the frame graph actually runs today.

use crate::post::TonemapMode;

/// Anti-aliasing mode (`gl_fxaa` / `r_aa`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum AaMode {
    /// Off.
    #[default]
    Off = 0,
    /// FXAA (existing post path).
    Fxaa = 1,
    /// SMAA 1x (SH1+; falls back to FXAA until implemented).
    Smaa = 2,
    /// Temporal AA (Full tier; off until SH7).
    Taa = 3,
}

/// CRT preset (`r_crt_preset`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum CrtPreset {
    /// Off / unused.
    #[default]
    Off = 0,
    /// Consumer TV.
    ConsumerTv = 1,
    /// PC monitor ~1994.
    PcMonitor1994 = 2,
}

/// Gore level (`r_gore`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum GoreLevel {
    /// No blood (puffs only).
    Off = 0,
    /// Vanilla sim blood sprites only.
    #[default]
    Vanilla = 1,
    /// Enhanced (spray / pools / drips; no lens blood).
    Enhanced = 2,
    /// Full catalogue.
    Full = 3,
}

/// Snapshot of all renderer toggles. Defaults keep the classic look (modern
/// effects off). Shells sync from the console each frame / on CVar change.
///
/// Field names match the CVars in [`SHADER_CVAR_NAMES`] / `docs/29-shaders.md`.
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct RenderSettings {
    /// `r_quality` 0..=3 (low / medium / high / ultra). Applied via
    /// [`Self::apply_quality`]; individual CVars still override afterward.
    pub quality: u8,

    // --- tonemap / exposure / AA (SH0–SH1) ---
    /// `gl_tonemap` → [`TonemapMode`].
    pub tonemap: TonemapMode,
    /// `r_exposure`.
    pub exposure: f32,
    /// Display gamma (post); 1.0 = identity after filmic/Hejl.
    pub gamma: f32,
    /// `gl_fxaa` / `r_aa`.
    pub aa: AaMode,
    /// `r_sharpen` (0 = off).
    pub sharpen: f32,
    /// `vid_scalefactor` (1.0 = native).
    pub scale_factor: f32,
    /// `vid_scalemode` (0 = linear upsample, 1 = nearest, …).
    pub scale_mode: i32,

    // --- wired post (SH1) ---
    /// `gl_bloom` (0 off, >0 on).
    pub bloom: bool,
    /// `gl_bloom_amount` / bloom threshold soft knee.
    pub bloom_amount: f32,
    /// `gl_ssao` (0 off, 1 bootstrap, 3 GTAO when available).
    pub ssao: i32,
    /// `gl_ssao_strength`.
    pub ssao_strength: f32,
    /// Force the post chain even when all effects are off (`r_postchain` 1).
    pub force_post: bool,
    /// Disable the post chain entirely (`r_postchain` 2) — scene draws LDR direct.
    pub disable_post: bool,

    // --- faithful / retro (SH5) ---
    pub dither: i32,
    pub banding: bool,
    pub crt: bool,
    pub crt_preset: CrtPreset,
    pub soft_particles: bool,
    pub outlines: bool,
    pub hit_flash: bool,
    pub color_grade: bool,
    pub damage_fx: bool,
    pub menu_blur: bool,
    pub hud_fx: bool,

    // --- lighting extras ---
    pub shadowmap_filter: i32,
    pub light_sprites: bool,
    pub autonormals: bool,
    pub lightshafts: bool,
    pub volfog: bool,
    pub ambient_bounce: bool,
    pub lightmap: bool,

    // --- surfaces ---
    pub liquids: i32,
    pub heat_haze: bool,
    pub parallax: bool,
    pub ssr: bool,
    pub detail: bool,
    pub sky_fx: bool,
    pub decal_fx: bool,

    // --- actors / particles ---
    pub particle_fx: bool,
    pub dissolve: bool,
    /// `r_actorspriteshadow`: 0 off, 1 `+CASTSPRITESHADOW` actors (UZDoom's
    /// default), 2 also monsters and players without `+NOSPRITESHADOW`.
    pub actor_sprite_shadow: u8,
    /// `r_actorspriteshadowalpha` (0.5).
    pub actor_sprite_shadow_alpha: f32,
    /// `r_actorspriteshadowfadeheight` (0 = no fade).
    pub actor_sprite_shadow_fade_height: f32,

    // --- more post ---
    pub auto_exposure: bool,
    pub powerup_fx: bool,
    pub underwater_fx: bool,
    pub vignette: f32,
    pub grain: f32,
    pub chromatic: f32,
    pub dof: bool,
    pub motion_blur: bool,

    // --- hardcore shooting / blood (SH10) ---
    pub muzzle_fx: bool,
    pub tracers: bool,
    pub impact_fx: bool,
    pub view_kick: bool,
    pub view_kick_scale: f32,
    pub weapon_smoke: bool,
    pub casings: bool,
    pub explosion_fx: bool,
    pub trails: bool,
    pub hit_confirm: bool,
    pub blood_fx: bool,
    pub blood_drips: bool,
    pub blood_wet: bool,
    pub corpse_pools: bool,
    pub gibs: bool,
    pub screen_blood: bool,
    pub weapon_blood: bool,
    pub wounds: bool,
    pub gore: GoreLevel,
    pub reduce_flashing: bool,
    /// Accessibility extra light, 0–4 (8 light levels per step).
    pub extra_light: i32,
    /// Anisotropic filtering samples for world textures (1 = off, up to 16).
    pub anisotropy: u16,

    /// GPU pass profiler overlay (`r_gpuprofile`).
    pub gpu_profile: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            quality: 0,
            tonemap: TonemapMode::None,
            exposure: 1.0,
            gamma: 1.0,
            aa: AaMode::Off,
            sharpen: 0.0,
            scale_factor: 1.0,
            scale_mode: 0,
            bloom: false,
            bloom_amount: 0.8,
            ssao: 0,
            ssao_strength: 1.0,
            force_post: false,
            disable_post: false,
            dither: 0,
            banding: false,
            crt: false,
            crt_preset: CrtPreset::Off,
            soft_particles: false,
            outlines: false,
            hit_flash: false,
            color_grade: false,
            damage_fx: false,
            menu_blur: false,
            hud_fx: false,
            shadowmap_filter: 0,
            light_sprites: true,
            autonormals: false,
            lightshafts: false,
            volfog: false,
            ambient_bounce: false,
            lightmap: true,
            liquids: 0,
            heat_haze: false,
            parallax: false,
            ssr: false,
            detail: false,
            sky_fx: false,
            decal_fx: false,
            particle_fx: false,
            dissolve: false,
            actor_sprite_shadow: 1,
            actor_sprite_shadow_alpha: 0.5,
            actor_sprite_shadow_fade_height: 0.0,
            auto_exposure: false,
            powerup_fx: false,
            underwater_fx: false,
            vignette: 0.0,
            grain: 0.0,
            chromatic: 0.0,
            dof: false,
            motion_blur: false,
            muzzle_fx: false,
            tracers: false,
            impact_fx: false,
            view_kick: false,
            view_kick_scale: 1.0,
            weapon_smoke: false,
            casings: false,
            explosion_fx: false,
            trails: false,
            hit_confirm: false,
            blood_fx: false,
            blood_drips: false,
            blood_wet: false,
            corpse_pools: false,
            gibs: false,
            screen_blood: false,
            weapon_blood: false,
            wounds: false,
            gore: GoreLevel::Vanilla,
            reduce_flashing: false,
            extra_light: 0,
            anisotropy: 1,
            gpu_profile: false,
        }
    }
}

impl RenderSettings {
    /// True when the frame graph should run the post chain (HDR → LDR).
    #[must_use]
    pub fn needs_post(&self) -> bool {
        if self.disable_post {
            return false;
        }
        if self.force_post {
            return true;
        }
        // R7: only CVars that actually change the post pass. Stand-ins that
        // merely dim the HUD (menu_blur) or are not wired yet must not flip
        // the whole frame into RoomSoft tonemap.
        self.bloom
            || self.ssao > 0
            || self.aa != AaMode::Off
            || self.tonemap != TonemapMode::None
            || self.sharpen > 0.0
            || self.crt
            || self.dither > 0
            || self.banding
            || self.damage_fx
            || self.vignette > 0.0
            || self.grain > 0.0
            || self.chromatic > 0.0
    }

    /// Scene colour should stay linear (no in-shader sRGB / toe).
    #[must_use]
    pub fn linear_scene(&self) -> bool {
        self.needs_post()
    }

    /// CVar name/value pairs for a quality preset (for console `set r_quality`).
    #[must_use]
    pub fn quality_cvar_writes(level: u8) -> Vec<(&'static str, String)> {
        let mut s = Self::default();
        s.apply_quality(level);
        vec![
            ("r_quality", s.quality.to_string()),
            ("gl_tonemap", (s.tonemap as u32).to_string()),
            ("r_exposure", format!("{}", s.exposure)),
            ("gl_fxaa", (s.aa as u8).to_string()),
            ("r_aa", (s.aa as u8).to_string()),
            ("r_sharpen", format!("{}", s.sharpen)),
            ("gl_bloom", if s.bloom { "1" } else { "0" }.into()),
            ("gl_bloom_amount", format!("{}", s.bloom_amount)),
            ("gl_ssao", s.ssao.to_string()),
            ("gl_ssao_strength", format!("{}", s.ssao_strength)),
            (
                "r_softparticles",
                if s.soft_particles { "1" } else { "0" }.into(),
            ),
            ("r_outlines", if s.outlines { "1" } else { "0" }.into()),
            ("r_hitflash", if s.hit_flash { "1" } else { "0" }.into()),
            ("r_colorgrade", if s.color_grade { "1" } else { "0" }.into()),
            ("r_damagefx", if s.damage_fx { "1" } else { "0" }.into()),
            ("r_liquids", s.liquids.to_string()),
            ("r_heathaze", if s.heat_haze { "1" } else { "0" }.into()),
            ("gl_shadowmap_filter", s.shadowmap_filter.to_string()),
            (
                "r_lightshafts",
                if s.lightshafts { "1" } else { "0" }.into(),
            ),
            (
                "r_autoexposure",
                if s.auto_exposure { "1" } else { "0" }.into(),
            ),
            ("r_particlefx", if s.particle_fx { "1" } else { "0" }.into()),
            (
                "r_weaponsmoke",
                if s.weapon_smoke { "1" } else { "0" }.into(),
            ),
            ("r_casings", if s.casings { "1" } else { "0" }.into()),
            ("r_gore", (s.gore as u8).to_string()),
            ("r_bloodfx", if s.blood_fx { "1" } else { "0" }.into()),
            (
                "r_corpsepools",
                if s.corpse_pools { "1" } else { "0" }.into(),
            ),
        ]
    }

    /// Apply a quality preset (overwrites related toggles).
    pub fn apply_quality(&mut self, level: u8) {
        self.quality = level.min(3);
        // Reset modern post to faithful, then layer the preset.
        self.bloom = false;
        self.ssao = 0;
        self.aa = AaMode::Off;
        self.sharpen = 0.0;
        self.tonemap = TonemapMode::None;
        self.exposure = 1.0;
        self.soft_particles = false;
        self.outlines = false;
        self.color_grade = false;
        self.damage_fx = false;
        self.crt = false;
        self.liquids = 0;
        self.heat_haze = false;
        self.shadowmap_filter = 0;
        self.lightshafts = false;
        self.auto_exposure = false;
        self.muzzle_fx = false;
        self.tracers = false;
        self.impact_fx = false;
        self.explosion_fx = false;
        self.view_kick = false;
        self.blood_fx = false;
        match self.quality {
            0 => {
                // Faithful / low — classic look.
            }
            1 => {
                self.tonemap = TonemapMode::RoomSoft;
                self.aa = AaMode::Fxaa;
                self.bloom = true;
                self.bloom_amount = 0.6;
                self.soft_particles = true;
                self.hit_flash = true;
                self.muzzle_fx = true;
                self.tracers = true;
                self.impact_fx = true;
                self.view_kick = true;
            }
            2 => {
                self.tonemap = TonemapMode::Agx;
                self.aa = AaMode::Smaa;
                self.bloom = true;
                self.bloom_amount = 0.75;
                self.ssao = 1;
                self.ssao_strength = 1.0;
                self.soft_particles = true;
                self.outlines = true;
                self.hit_flash = true;
                self.color_grade = true;
                self.damage_fx = true;
                self.liquids = 1;
                self.shadowmap_filter = 1;
                self.sharpen = 0.2;
            }
            _ => {
                self.tonemap = TonemapMode::KhronosNeutral;
                self.aa = AaMode::Taa;
                self.bloom = true;
                self.bloom_amount = 0.85;
                self.ssao = 3;
                self.ssao_strength = 1.2;
                self.soft_particles = true;
                self.outlines = true;
                self.hit_flash = true;
                self.color_grade = true;
                self.damage_fx = true;
                self.liquids = 2;
                self.heat_haze = true;
                self.shadowmap_filter = 2;
                self.lightshafts = true;
                self.auto_exposure = true;
                self.sharpen = 0.35;
                self.particle_fx = true;
                self.weapon_smoke = true;
                self.casings = true;
                self.gore = GoreLevel::Enhanced;
                self.blood_fx = true;
                self.corpse_pools = true;
            }
        }
    }

    /// Sync from a console-like getter. Recognized names return `true`.
    ///
    /// `get` returns `(bool?, int?, float?)` for the CVar; unused slots are
    /// `None`. String CVars are not required for the shader set today.
    pub fn sync_cvar(
        &mut self,
        name: &str,
        b: Option<bool>,
        i: Option<i32>,
        f: Option<f64>,
    ) -> bool {
        let n = name.to_ascii_lowercase();
        match n.as_str() {
            "r_quality" => {
                // Preset application writes individual CVars (see console `set`);
                // sync only records the level so archived individuals stay authoritative.
                if let Some(v) = i {
                    self.quality = v.clamp(0, 3) as u8;
                }
                true
            }
            "gl_tonemap" => {
                if let Some(v) = i {
                    self.tonemap = TonemapMode::from_u32(v.max(0) as u32);
                }
                true
            }
            "r_exposure" => {
                if let Some(v) = f {
                    self.exposure = v.clamp(0.01, 16.0) as f32;
                }
                true
            }
            "r_gamma" | "vid_gamma" => {
                if let Some(v) = f {
                    self.gamma = v.clamp(0.1, 4.0) as f32;
                }
                true
            }
            "gl_fxaa" => {
                if let Some(v) = i {
                    self.aa = match v {
                        0 => AaMode::Off,
                        2 => AaMode::Smaa,
                        3 => AaMode::Taa,
                        _ => AaMode::Fxaa,
                    };
                } else if let Some(v) = b {
                    self.aa = if v { AaMode::Fxaa } else { AaMode::Off };
                }
                true
            }
            "r_aa" => {
                if let Some(v) = i {
                    self.aa = match v {
                        0 => AaMode::Off,
                        2 => AaMode::Smaa,
                        3 => AaMode::Taa,
                        _ => AaMode::Fxaa,
                    };
                }
                true
            }
            "r_sharpen" => {
                if let Some(v) = f {
                    self.sharpen = v.max(0.0) as f32;
                }
                true
            }
            "vid_scalefactor" => {
                if let Some(v) = f {
                    self.scale_factor = v.clamp(0.1, 1.0) as f32;
                }
                true
            }
            "vid_scalemode" => {
                if let Some(v) = i {
                    self.scale_mode = v;
                }
                true
            }
            "gl_bloom" => {
                self.bloom = i.map(|v| v != 0).or(b).unwrap_or(self.bloom);
                true
            }
            "gl_bloom_amount" => {
                if let Some(v) = f {
                    self.bloom_amount = v.clamp(0.0, 4.0) as f32;
                }
                true
            }
            "gl_ssao" => {
                if let Some(v) = i {
                    self.ssao = v.max(0);
                } else if let Some(v) = b {
                    self.ssao = i32::from(v);
                }
                true
            }
            "gl_ssao_strength" => {
                if let Some(v) = f {
                    self.ssao_strength = v.clamp(0.0, 4.0) as f32;
                }
                true
            }
            "r_postchain" => {
                match i.unwrap_or(0) {
                    1 => {
                        self.force_post = true;
                        self.disable_post = false;
                    }
                    2 => {
                        self.force_post = false;
                        self.disable_post = true;
                    }
                    _ => {
                        self.force_post = false;
                        self.disable_post = false;
                    }
                }
                true
            }
            "r_dither" => {
                if let Some(v) = i {
                    self.dither = v.max(0);
                }
                true
            }
            "r_banding" => {
                self.banding = b.or(i.map(|v| v != 0)).unwrap_or(self.banding);
                true
            }
            "r_crt" => {
                self.crt = b.or(i.map(|v| v != 0)).unwrap_or(self.crt);
                true
            }
            "r_crt_preset" => {
                if let Some(v) = i {
                    self.crt_preset = match v {
                        1 => CrtPreset::ConsumerTv,
                        2 => CrtPreset::PcMonitor1994,
                        _ => CrtPreset::Off,
                    };
                }
                true
            }
            "r_softparticles" => {
                self.soft_particles = b.or(i.map(|v| v != 0)).unwrap_or(self.soft_particles);
                true
            }
            "r_outlines" => {
                self.outlines = b.or(i.map(|v| v != 0)).unwrap_or(self.outlines);
                true
            }
            "r_hitflash" => {
                self.hit_flash = b.or(i.map(|v| v != 0)).unwrap_or(self.hit_flash);
                true
            }
            "r_colorgrade" => {
                self.color_grade = b.or(i.map(|v| v != 0)).unwrap_or(self.color_grade);
                true
            }
            "r_damagefx" => {
                self.damage_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.damage_fx);
                true
            }
            "r_menublur" => {
                self.menu_blur = b.or(i.map(|v| v != 0)).unwrap_or(self.menu_blur);
                true
            }
            "r_hudfx" => {
                self.hud_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.hud_fx);
                true
            }
            "gl_shadowmap_filter" => {
                if let Some(v) = i {
                    self.shadowmap_filter = v.max(0);
                }
                true
            }
            "gl_light_sprites" => {
                self.light_sprites = b.or(i.map(|v| v != 0)).unwrap_or(self.light_sprites);
                true
            }
            "r_autonormals" => {
                self.autonormals = b.or(i.map(|v| v != 0)).unwrap_or(self.autonormals);
                true
            }
            "r_lightshafts" => {
                self.lightshafts = b.or(i.map(|v| v != 0)).unwrap_or(self.lightshafts);
                true
            }
            "r_volfog" => {
                self.volfog = b.or(i.map(|v| v != 0)).unwrap_or(self.volfog);
                true
            }
            "r_ambientbounce" => {
                self.ambient_bounce = b.or(i.map(|v| v != 0)).unwrap_or(self.ambient_bounce);
                true
            }
            "gl_lightmap" => {
                self.lightmap = b.or(i.map(|v| v != 0)).unwrap_or(self.lightmap);
                true
            }
            "r_liquids" => {
                if let Some(v) = i {
                    self.liquids = v.max(0);
                } else if let Some(v) = b {
                    self.liquids = i32::from(v);
                }
                true
            }
            "r_heathaze" => {
                self.heat_haze = b.or(i.map(|v| v != 0)).unwrap_or(self.heat_haze);
                true
            }
            "r_parallax" => {
                self.parallax = b.or(i.map(|v| v != 0)).unwrap_or(self.parallax);
                true
            }
            "r_ssr" => {
                self.ssr = b.or(i.map(|v| v != 0)).unwrap_or(self.ssr);
                true
            }
            "r_detail" => {
                self.detail = b.or(i.map(|v| v != 0)).unwrap_or(self.detail);
                true
            }
            "r_skyfx" => {
                self.sky_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.sky_fx);
                true
            }
            "r_decalfx" => {
                self.decal_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.decal_fx);
                true
            }
            "r_particlefx" => {
                self.particle_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.particle_fx);
                true
            }
            "r_dissolve" => {
                self.dissolve = b.or(i.map(|v| v != 0)).unwrap_or(self.dissolve);
                true
            }
            "r_actorspriteshadow" => {
                self.actor_sprite_shadow = i
                    .map(|v| v.clamp(0, 2) as u8)
                    .or(b.map(|on| if on { 2 } else { 0 }))
                    .unwrap_or(self.actor_sprite_shadow);
                true
            }
            "r_actorspriteshadowalpha" => {
                #[allow(clippy::cast_possible_truncation)]
                if let Some(v) = f.or(i.map(f64::from)) {
                    self.actor_sprite_shadow_alpha = v.clamp(0.0, 1.0) as f32;
                }
                true
            }
            "r_actorspriteshadowfadeheight" => {
                #[allow(clippy::cast_possible_truncation)]
                if let Some(v) = f.or(i.map(f64::from)) {
                    self.actor_sprite_shadow_fade_height = v.max(0.0) as f32;
                }
                true
            }
            "r_autoexposure" => {
                self.auto_exposure = b.or(i.map(|v| v != 0)).unwrap_or(self.auto_exposure);
                true
            }
            "r_powerupfx" => {
                self.powerup_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.powerup_fx);
                true
            }
            "r_underwaterfx" => {
                self.underwater_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.underwater_fx);
                true
            }
            "r_vignette" => {
                if let Some(v) = f {
                    self.vignette = v.clamp(0.0, 1.0) as f32;
                } else if let Some(v) = b {
                    self.vignette = if v { 0.35 } else { 0.0 };
                }
                true
            }
            "r_grain" => {
                if let Some(v) = f {
                    self.grain = v.clamp(0.0, 1.0) as f32;
                } else if let Some(v) = b {
                    self.grain = if v { 0.15 } else { 0.0 };
                }
                true
            }
            "r_chromatic" => {
                if let Some(v) = f {
                    self.chromatic = v.clamp(0.0, 1.0) as f32;
                } else if let Some(v) = b {
                    self.chromatic = if v { 0.2 } else { 0.0 };
                }
                true
            }
            "r_dof" => {
                self.dof = b.or(i.map(|v| v != 0)).unwrap_or(self.dof);
                true
            }
            "r_motionblur" => {
                self.motion_blur = b.or(i.map(|v| v != 0)).unwrap_or(self.motion_blur);
                true
            }
            "r_muzzlefx" => {
                self.muzzle_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.muzzle_fx);
                true
            }
            "r_tracers" => {
                self.tracers = b.or(i.map(|v| v != 0)).unwrap_or(self.tracers);
                true
            }
            "r_impactfx" => {
                self.impact_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.impact_fx);
                true
            }
            "r_viewkick" => {
                self.view_kick = b.or(i.map(|v| v != 0)).unwrap_or(self.view_kick);
                true
            }
            "r_viewkick_scale" => {
                if let Some(v) = f {
                    self.view_kick_scale = v.clamp(0.0, 4.0) as f32;
                }
                true
            }
            "r_weaponsmoke" => {
                self.weapon_smoke = b.or(i.map(|v| v != 0)).unwrap_or(self.weapon_smoke);
                true
            }
            "r_casings" => {
                self.casings = b.or(i.map(|v| v != 0)).unwrap_or(self.casings);
                true
            }
            "r_explosionfx" => {
                self.explosion_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.explosion_fx);
                true
            }
            "r_trails" => {
                self.trails = b.or(i.map(|v| v != 0)).unwrap_or(self.trails);
                true
            }
            "r_hitconfirm" => {
                self.hit_confirm = b.or(i.map(|v| v != 0)).unwrap_or(self.hit_confirm);
                true
            }
            "r_bloodfx" => {
                self.blood_fx = b.or(i.map(|v| v != 0)).unwrap_or(self.blood_fx);
                true
            }
            "r_blooddrips" => {
                self.blood_drips = b.or(i.map(|v| v != 0)).unwrap_or(self.blood_drips);
                true
            }
            "r_bloodwet" => {
                self.blood_wet = b.or(i.map(|v| v != 0)).unwrap_or(self.blood_wet);
                true
            }
            "r_corpsepools" => {
                self.corpse_pools = b.or(i.map(|v| v != 0)).unwrap_or(self.corpse_pools);
                true
            }
            "r_gibs" => {
                self.gibs = b.or(i.map(|v| v != 0)).unwrap_or(self.gibs);
                true
            }
            "r_screenblood" => {
                self.screen_blood = b.or(i.map(|v| v != 0)).unwrap_or(self.screen_blood);
                true
            }
            "r_weaponblood" => {
                self.weapon_blood = b.or(i.map(|v| v != 0)).unwrap_or(self.weapon_blood);
                true
            }
            "r_wounds" => {
                self.wounds = b.or(i.map(|v| v != 0)).unwrap_or(self.wounds);
                true
            }
            "r_gore" => {
                if let Some(v) = i {
                    self.gore = match v.clamp(0, 3) {
                        0 => GoreLevel::Off,
                        1 => GoreLevel::Vanilla,
                        2 => GoreLevel::Enhanced,
                        _ => GoreLevel::Full,
                    };
                }
                true
            }
            "r_extralight" => {
                if let Some(v) = i {
                    self.extra_light = v.clamp(0, 4);
                } else if let Some(v) = b {
                    self.extra_light = i32::from(v);
                }
                true
            }
            "gl_texture_filter_anisotropic" => {
                let v = i
                    .or(f.map(|f| f as i32))
                    .unwrap_or(if b == Some(true) { 8 } else { 1 });
                self.anisotropy = crate::materials::sampler_anisotropy(v.clamp(0, 16) as u16);
                true
            }
            "r_reduce_flashing" => {
                self.reduce_flashing = b.or(i.map(|v| v != 0)).unwrap_or(self.reduce_flashing);
                true
            }
            "r_gpuprofile" => {
                self.gpu_profile = b.or(i.map(|v| v != 0)).unwrap_or(self.gpu_profile);
                true
            }
            _ => false,
        }
    }
}

/// Names registered as shader / post CVars (for console + docs).
pub const SHADER_CVAR_NAMES: &[&str] = &[
    "r_quality",
    "gl_tonemap",
    "r_exposure",
    "r_gamma",
    "gl_fxaa",
    "r_aa",
    "r_sharpen",
    "vid_scalefactor",
    "vid_scalemode",
    "gl_bloom",
    "gl_bloom_amount",
    "gl_ssao",
    "gl_ssao_strength",
    "r_postchain",
    "r_dither",
    "r_banding",
    "r_crt",
    "r_crt_preset",
    "r_softparticles",
    "r_outlines",
    "r_hitflash",
    "r_colorgrade",
    "r_damagefx",
    "r_menublur",
    "r_hudfx",
    "gl_shadowmap_filter",
    "gl_light_sprites",
    "r_autonormals",
    "r_lightshafts",
    "r_volfog",
    "r_ambientbounce",
    "gl_lightmap",
    "r_liquids",
    "r_heathaze",
    "r_parallax",
    "r_ssr",
    "r_detail",
    "r_skyfx",
    "r_decalfx",
    "r_particlefx",
    "r_dissolve",
    "r_actorspriteshadow",
    "r_actorspriteshadowalpha",
    "r_actorspriteshadowfadeheight",
    "r_autoexposure",
    "r_powerupfx",
    "r_underwaterfx",
    "r_vignette",
    "r_grain",
    "r_chromatic",
    "r_dof",
    "r_motionblur",
    "r_muzzlefx",
    "r_tracers",
    "r_impactfx",
    "r_viewkick",
    "r_viewkick_scale",
    "r_weaponsmoke",
    "r_casings",
    "r_explosionfx",
    "r_trails",
    "r_hitconfirm",
    "r_bloodfx",
    "r_blooddrips",
    "r_bloodwet",
    "r_corpsepools",
    "r_gibs",
    "r_screenblood",
    "r_weaponblood",
    "r_wounds",
    "r_gore",
    "r_reduce_flashing",
    "r_extralight",
    "gl_texture_filter_anisotropic",
    "r_gpuprofile",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_skips_post() {
        let s = RenderSettings::default();
        assert!(!s.needs_post());
        assert!(!s.linear_scene());
    }

    #[test]
    fn bloom_enables_post() {
        let mut s = RenderSettings::default();
        assert!(s.sync_cvar("gl_bloom", Some(true), None, None));
        assert!(s.needs_post());
        assert!(s.linear_scene());
    }

    #[test]
    fn quality_medium_enables_stack() {
        let mut s = RenderSettings::default();
        s.apply_quality(2);
        assert!(s.bloom);
        assert!(s.ssao > 0);
        assert!(s.needs_post());
    }

    #[test]
    fn disable_post_wins() {
        let mut s = RenderSettings::default();
        s.bloom = true;
        assert!(s.sync_cvar("r_postchain", None, Some(2), None));
        assert!(!s.needs_post());
    }

    #[test]
    fn unimplemented_cvars_do_not_enable_post() {
        let mut s = RenderSettings::default();
        for name in [
            "r_colorgrade",
            "r_autoexposure",
            "r_powerupfx",
            "r_underwaterfx",
            "r_dof",
            "r_motionblur",
            "r_menublur",
            "vid_scalefactor",
        ] {
            assert!(s.sync_cvar(name, Some(true), Some(1), Some(0.5)), "{name}");
            assert!(
                !s.needs_post(),
                "{name} must not flip needs_post (docs/29 R7)"
            );
        }
    }

    #[test]
    fn every_shader_cvar_name_is_recognized() {
        let mut s = RenderSettings::default();
        for name in SHADER_CVAR_NAMES {
            assert!(
                s.sync_cvar(name, Some(false), Some(0), Some(0.0)),
                "unrecognized {name}"
            );
        }
    }
}

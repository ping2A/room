//! Sync [`room_render::RenderSettings`] from the console CVar store + graphics menu.

use room_console::{CvarStore, CvarValue};
use room_render::{RenderSettings, SHADER_CVAR_NAMES, TonemapMode};
use room_ui::{ListMenu, MenuItem, Vec2};

/// Build settings from the live CVar table (individuals are authoritative).
#[must_use]
pub fn render_settings_from_cvars(cvars: &CvarStore) -> RenderSettings {
    let mut s = RenderSettings::default();
    for name in SHADER_CVAR_NAMES {
        match cvars.get(name) {
            Some(CvarValue::Bool(b)) => {
                let _ = s.sync_cvar(name, Some(*b), None, None);
            }
            Some(CvarValue::Int(i)) => {
                let _ = s.sync_cvar(name, None, Some(*i), None);
            }
            Some(CvarValue::Float(f)) => {
                let _ = s.sync_cvar(name, None, None, Some(*f));
            }
            _ => {}
        }
    }
    s
}

/// Reset every shader / FX CVar to faithful defaults (nothing modern enabled).
///
/// Used by `--combat-demo` so a previous `room.ini` cannot leak bloom/CRT/etc.
pub fn reset_shaders_to_defaults(cvars: &mut CvarStore) {
    cvars.apply_quality_preset(0);
    // Quality preset leaves some shooting/blood flags on; force them off too.
    let off_bool = [
        "r_muzzlefx",
        "r_tracers",
        "r_impactfx",
        "r_viewkick",
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
        "gl_light_sprites",
        "gl_lightmap",
        "r_gpuprofile",
        "r_reduce_flashing",
    ];
    for name in off_bool {
        let _ = cvars.set(name, "0");
    }
    let _ = cvars.set("r_gore", "1"); // vanilla sprites only
    let _ = cvars.set("r_viewkick_scale", "1");
    let _ = cvars.set("r_exposure", "1");
    let _ = cvars.set("r_gamma", "1");
    let _ = cvars.set("gl_bloom_amount", "0.8");
    let _ = cvars.set("gl_ssao_strength", "1");
    let _ = cvars.set("vid_scalefactor", "1");
    let _ = cvars.set("r_postchain", "0");
}

fn bool_on(cvars: &CvarStore, name: &str) -> bool {
    match cvars.get(name) {
        Some(CvarValue::Bool(b)) => *b,
        Some(CvarValue::Int(i)) => *i != 0,
        Some(CvarValue::Float(f)) => *f != 0.0,
        _ => false,
    }
}

fn int_val(cvars: &CvarStore, name: &str) -> i32 {
    match cvars.get(name) {
        Some(CvarValue::Int(i)) => *i,
        Some(CvarValue::Bool(b)) => i32::from(*b),
        Some(CvarValue::Float(f)) => *f as i32,
        _ => 0,
    }
}

fn float_val(cvars: &CvarStore, name: &str) -> f64 {
    match cvars.get(name) {
        Some(CvarValue::Float(f)) => *f,
        Some(CvarValue::Int(i)) => f64::from(*i),
        Some(CvarValue::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

fn on_off(cvars: &CvarStore, name: &str) -> &'static str {
    if bool_on(cvars, name) { "ON" } else { "OFF" }
}

fn quality_name(v: i32) -> &'static str {
    match v {
        0 => "FAITHFUL",
        1 => "LOW",
        2 => "MEDIUM",
        _ => "ULTRA",
    }
}

fn tonemap_name(v: i32) -> &'static str {
    match TonemapMode::from_u32(v.max(0) as u32) {
        TonemapMode::None => "NONE",
        TonemapMode::Reinhard => "REINHARD",
        TonemapMode::Linear => "LINEAR",
        TonemapMode::Hejl => "HEJL",
        TonemapMode::Palette => "PALETTE",
        TonemapMode::Uncharted2 => "UNCHARTED2",
        TonemapMode::Agx => "AGX",
        TonemapMode::KhronosNeutral => "KHRONOS",
        TonemapMode::RoomSoft => "ROOMSOFT",
    }
}

fn gore_name(v: i32) -> &'static str {
    match v.clamp(0, 3) {
        0 => "OFF",
        1 => "VANILLA",
        2 => "ENHANCED",
        _ => "FULL",
    }
}

fn aa_name(v: i32) -> &'static str {
    match v {
        0 => "OFF",
        2 => "SMAA",
        3 => "TAA",
        _ => "FXAA",
    }
}

fn header(label: &str) -> MenuItem {
    MenuItem::text(format!("-- {label} --"), "noop")
}

fn tog(cvars: &CvarStore, label: &str, cvar: &str) -> MenuItem {
    MenuItem::text(
        format!("{label}: {}", on_off(cvars, cvar)),
        format!("option {cvar}"),
    )
}

fn tog_f(cvars: &CvarStore, label: &str, cvar: &str, on: f64) -> MenuItem {
    let on_now = float_val(cvars, cvar) > 0.01;
    MenuItem::text(
        format!("{label}: {}", if on_now { "ON" } else { "OFF" }),
        format!("optiontogglefloat {cvar} {on}"),
    )
}

fn cycle(cvars: &CvarStore, label: &str, cvar: &str, lo: i32, hi: i32, shown: String) -> MenuItem {
    let _ = (cvars, lo, hi);
    MenuItem::text(
        format!("{label}: {shown}"),
        format!("optioncycle {cvar} {lo} {hi}"),
    )
}

/// Live Graphics menu reflecting current CVars (scrollable; every catalogue toggle).
#[must_use]
pub fn graphics_menu(cvars: &CvarStore) -> ListMenu {
    let q = int_val(cvars, "r_quality");
    let tm = int_val(cvars, "gl_tonemap");
    let aa = int_val(cvars, "gl_fxaa").max(int_val(cvars, "r_aa"));
    let ssao = int_val(cvars, "gl_ssao");
    let gore = int_val(cvars, "r_gore");
    let mut items = vec![
        header("PRESET"),
        cycle(cvars, "QUALITY", "r_quality", 0, 3, quality_name(q).into()),
        cycle(
            cvars,
            "TONEMAP",
            "gl_tonemap",
            0,
            8,
            tonemap_name(tm).into(),
        ),
        cycle(cvars, "ANTI-ALIAS", "gl_fxaa", 0, 3, aa_name(aa).into()),
        header("POST"),
        tog(cvars, "BLOOM", "gl_bloom"),
        cycle(cvars, "SSAO", "gl_ssao", 0, 3, ssao.to_string()),
        tog(cvars, "CRT", "r_crt"),
        cycle(
            cvars,
            "CRT PRESET",
            "r_crt_preset",
            0,
            2,
            int_val(cvars, "r_crt_preset").to_string(),
        ),
        tog(cvars, "DITHER", "r_dither"),
        tog(cvars, "BANDING", "r_banding"),
        tog_f(cvars, "VIGNETTE", "r_vignette", 0.35),
        tog_f(cvars, "FILM GRAIN", "r_grain", 0.15),
        tog_f(cvars, "CHROMATIC", "r_chromatic", 0.2),
        tog(cvars, "COLOR GRADE", "r_colorgrade"),
        tog(cvars, "AUTO EXPOSURE", "r_autoexposure"),
        tog(cvars, "DOF", "r_dof"),
        tog(cvars, "MOTION BLUR", "r_motionblur"),
        tog(cvars, "DAMAGE FX", "r_damagefx"),
        tog(cvars, "POWERUP FX", "r_powerupfx"),
        tog(cvars, "UNDERWATER FX", "r_underwaterfx"),
        tog(cvars, "MENU BLUR", "r_menublur"),
        tog(cvars, "HUD FX", "r_hudfx"),
        header("LIGHTING"),
        tog(cvars, "LIT SPRITES", "gl_light_sprites"),
        tog(cvars, "LIGHTMAP", "gl_lightmap"),
        cycle(
            cvars,
            "SHADOW FILTER",
            "gl_shadowmap_filter",
            0,
            2,
            int_val(cvars, "gl_shadowmap_filter").to_string(),
        ),
        tog(cvars, "AUTO NORMALS", "r_autonormals"),
        tog(cvars, "LIGHT SHAFTS", "r_lightshafts"),
        tog(cvars, "VOL FOG", "r_volfog"),
        tog(cvars, "AMBIENT BOUNCE", "r_ambientbounce"),
        header("SURFACES"),
        cycle(
            cvars,
            "LIQUIDS",
            "r_liquids",
            0,
            2,
            int_val(cvars, "r_liquids").to_string(),
        ),
        tog(cvars, "HEAT HAZE", "r_heathaze"),
        tog(cvars, "PARALLAX", "r_parallax"),
        tog(cvars, "SSR", "r_ssr"),
        tog(cvars, "DETAIL TEX", "r_detail"),
        tog(cvars, "SKY FX", "r_skyfx"),
        tog(cvars, "DECAL FX", "r_decalfx"),
        header("ACTORS"),
        tog(cvars, "SOFT PARTICLES", "r_softparticles"),
        tog(cvars, "PARTICLE FX", "r_particlefx"),
        tog(cvars, "OUTLINES", "r_outlines"),
        tog(cvars, "HIT FLASH", "r_hitflash"),
        tog(cvars, "SPRITE SHADOW", "r_actorspriteshadow"),
        tog(cvars, "DISSOLVE", "r_dissolve"),
        header("SHOOTING"),
        tog(cvars, "MUZZLE FX", "r_muzzlefx"),
        tog(cvars, "TRACERS", "r_tracers"),
        tog(cvars, "IMPACT FX", "r_impactfx"),
        tog(cvars, "EXPLOSION FX", "r_explosionfx"),
        tog(cvars, "TRAILS", "r_trails"),
        tog(cvars, "VIEW KICK", "r_viewkick"),
        tog(cvars, "WEAPON SMOKE", "r_weaponsmoke"),
        tog(cvars, "CASINGS", "r_casings"),
        tog(cvars, "HIT CONFIRM", "r_hitconfirm"),
        header("BLOOD / GORE"),
        cycle(cvars, "GORE", "r_gore", 0, 3, gore_name(gore).into()),
        tog(cvars, "BLOOD FX", "r_bloodfx"),
        tog(cvars, "BLOOD DRIPS", "r_blooddrips"),
        tog(cvars, "WET BLOOD", "r_bloodwet"),
        tog(cvars, "CORPSE POOLS", "r_corpsepools"),
        tog(cvars, "GIBS", "r_gibs"),
        tog(cvars, "SCREEN BLOOD", "r_screenblood"),
        tog(cvars, "WEAPON BLOOD", "r_weaponblood"),
        tog(cvars, "WOUNDS", "r_wounds"),
        tog(cvars, "REDUCE FLASHING", "r_reduce_flashing"),
        header("DEBUG"),
        tog(cvars, "GPU PROFILE", "r_gpuprofile"),
        MenuItem::text("BACK", "menu_back"),
    ];
    // Preserve selected when refreshing labels.
    let _ = &mut items;
    ListMenu {
        title: "Graphics".into(),
        items,
        selected: 0,
        scroll: 0,
        // Fit above classic STBAR (y≈168): title ~y8, rows end ≤150.
        visible_rows: 11,
        origin: Vec2::new(16.0, 22.0),
        dim_background: true,
        // STCFN ~8px tall; leave a clear gap so rows don't fuse vertically.
        line_spacing: 12.0,
        show_title: true,
    }
}

/// Cycle an int CVar in `[lo, hi]` inclusive.
pub fn cycle_cvar(cvars: &mut CvarStore, name: &str, lo: i32, hi: i32) -> i32 {
    let cur = int_val(cvars, name);
    let next = if cur >= hi { lo } else { cur + 1 };
    let _ = cvars.set(name, &next.to_string());
    if name.eq_ignore_ascii_case("r_quality") {
        cvars.apply_quality_preset(next);
    }
    next
}

/// Toggle a float CVar between `0` and `on_value`.
pub fn toggle_float_cvar(cvars: &mut CvarStore, name: &str, on_value: f64) -> f64 {
    let cur = float_val(cvars, name);
    let next = if cur > 0.01 { 0.0 } else { on_value };
    let _ = cvars.set(name, &format!("{next}"));
    next
}

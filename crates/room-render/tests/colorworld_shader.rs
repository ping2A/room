//! `mods/colorworld`: its GLDEFS post shader parses and compiles through
//! Room's GLSL → WGSL path (naga validates the result), so a shader edit
//! that Room cannot run fails here instead of silently in the game.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

fn mod_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods/colorworld/pk3")
}

#[test]
fn the_colorworld_post_shader_compiles() {
    let gldefs = std::fs::read_to_string(mod_dir().join("GLDEFS")).unwrap();
    let defs = room_defs::gldefs::parse_gldefs(&gldefs).unwrap();
    let def = defs
        .hardware_shaders
        .values()
        .find(|d| d.name == "ColorWorld")
        .expect("ColorWorld in GLDEFS");
    assert_eq!(def.kind, room_defs::HardwareShaderKind::PostProcess);
    assert!(!def.enabled, "off until the handler turns it on");
    let names: Vec<&str> = def.uniforms.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, ["mode", "tics", "speed", "strength"]);
    let src = std::fs::read_to_string(mod_dir().join(&def.shader)).unwrap();
    let compiled = room_render::compile_post_glsl(def, &src).expect("compiles to WGSL");
    let fields: Vec<(&str, &str, u32)> = compiled
        .uniforms
        .iter()
        .map(|f| (f.name.as_str(), f.ty.as_str(), f.offset))
        .collect();
    assert_eq!(
        fields,
        [
            ("mode", "int", 0),
            ("tics", "int", 4),
            ("speed", "int", 8),
            ("strength", "int", 12)
        ]
    );
}

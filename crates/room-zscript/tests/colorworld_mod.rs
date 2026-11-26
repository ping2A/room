//! The Color World mod's ZScript (`mods/colorworld/pk3/ZSCRIPT`) type-checks
//! against Room's stdlib: `CVar.FindCVar`, `PPShader.*`, `EventHandler.WorldTick`.

// Test helpers: a failed setup step should panic the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
fn colorworld_zscript_compiles() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../mods/colorworld/pk3/ZSCRIPT"
    );
    let text = std::fs::read_to_string(path).expect("read colorworld ZSCRIPT");
    let result = room_zscript::check(&[("ZSCRIPT".to_owned(), text)]);
    assert!(result.ok(), "{:#?}", result.diagnostics);
    let program = result.program.expect("program");
    assert!(
        program
            .classes
            .iter()
            .any(|c| c.name == "ColorWorldHandler"),
        "handler class compiled"
    );
}

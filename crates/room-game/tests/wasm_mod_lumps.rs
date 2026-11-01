//! MA3: generated lumps → in-memory PWAD (`docs/31`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_formats::patch::parse_patch;
use room_game::{GameOptions, Headless, Skill, load_embedded_engine_data};
use room_script::ModuleScope;
use room_sim::script_mods::ModSource;
use room_vfs::{Namespace, VfsBuilder};

fn lumps_mod() -> ModSource {
    let wasm = wat::parse_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../room-script/testdata/mod_lumps.wat"
    ))
    .expect("mod_lumps.wat");
    ModSource {
        mod_id: "test.mod_lumps".into(),
        name: "mod_lumps".into(),
        scope: ModuleScope::Play,
        bytes: wasm,
        options: Default::default(),
    }
}

#[test]
fn wasm_mod_lumps_mounts_sprite_and_sndinfo() {
    let data = load_embedded_engine_data().unwrap();
    let mut game = Headless::new(GameOptions {
        skill: Skill::new(3),
        ..Default::default()
    });
    game.set_defs(data.classes, data.states);

    let mut vfs = VfsBuilder::new().build().unwrap();
    game.set_script_mods_with_vfs(vec![lumps_mod()], Some(&mut vfs))
        .unwrap();

    let wad = game.generated_mod_wad();
    assert!(!wad.is_empty(), "generated PWAD");
    assert!(wad.starts_with(b"PWAD"));

    let id = vfs
        .find("TSTLUMP0", Namespace::Sprites)
        .expect("sprite lump in mounted layer");
    let bytes = vfs.read(id).unwrap();
    let patch = parse_patch(&bytes).expect("valid Doom patch");
    assert_eq!(patch.width, 1);
    assert_eq!(patch.height, 1);

    let snd = vfs.find("SNDINFO", Namespace::Global).expect("SNDINFO");
    let text = String::from_utf8(vfs.read(snd).unwrap().to_vec()).unwrap();
    assert!(text.contains("test/beep"), "{text}");
    assert!(text.contains("DSTSTBEP"), "{text}");
}

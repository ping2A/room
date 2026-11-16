//! `room mod convert` (`docs/37` ZW9): convert a small ZScript mod, verify it
//! tic by tic against the original on Freedoom E1M1, then load the converted
//! package the way the game does. Brutal Doom variant gated on
//! `ROOM_BRUTALDOOM_PK3`. Run with `--features zvm-aot` so the converted side
//! executes its WebAssembly.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_game::mod_convert::{ConvertOptions, convert};
use std::path::PathBuf;

fn iwad() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/freedoom/freedoom1.wad");
    p.is_file().then_some(p)
}

const MOD_ZSCRIPT: &str = r#"
version "4.0"
class ConvProbe : EventHandler
{
    int spawned;
    int sum;
    override void WorldThingSpawned(WorldEvent e)
    {
        spawned++;
        for (int i = 0; i < 40; i++)
        {
            sum += (i * spawned) % 7;
        }
        if (e.Thing && e.Thing.bISMONSTER && (spawned % 3) == 0)
        {
            Console.Printf("probe %d %d", spawned, sum);
        }
    }
}
"#;

fn write_mod(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("zscript.zs"), MOD_ZSCRIPT).unwrap();
    std::fs::write(
        dir.join("MAPINFO.txt"),
        "GameInfo\n{\n  AddEventHandlers = \"ConvProbe\"\n}\n",
    )
    .unwrap();
}

#[test]
fn convert_verify_and_load_small_mod() {
    let Some(iwad) = iwad() else {
        eprintln!("skip: no freedoom1.wad");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("room-convert-{}", std::process::id()));
    let input = tmp.join("probe");
    write_mod(&input);
    let output = tmp.join("probe-converted.pk3");
    let report = convert(&ConvertOptions {
        input: input.clone(),
        output: output.clone(),
        iwad: Some(iwad.clone()),
        map: Some("E1M1".into()),
        verify_tics: 300,
        skill: 3,
        keep_zscript: true,
        allow_missing_natives: false,
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let v = report.verify.clone().expect("verified");
    assert!(v.passed(), "{report:?}");
    assert_eq!(report.sources, vec!["zscript.zs".to_owned()]);
    #[cfg(feature = "zvm-aot")]
    assert!(v.aot_calls > 0, "converted session ran no wasm: {v:?}");
    eprintln!(
        "verified {} tics, {} wasm calls ({})",
        v.tics, v.aot_calls, v.engine
    );

    // The package loads without ZScript sources and says it was verified.
    let vfs = room_game::mount_embedded_engine(room_vfs::VfsBuilder::new())
        .unwrap()
        .mount_path(&iwad)
        .unwrap()
        .mount_path(&output)
        .unwrap()
        .build()
        .unwrap();
    let zs = room_game::load_zscript_from_vfs(&vfs);
    assert!(zs.ok, "{:?}", zs.notes);
    assert!(zs.notes[0].starts_with("converted mod"), "{:?}", zs.notes);
    assert!(
        !zs.notes.iter().any(|n| n.contains("warning")),
        "{:?}",
        zs.notes
    );
    let program = zs.program.unwrap();
    assert!(
        program
            .func_by_name
            .contains_key("ConvProbe.WorldThingSpawned")
    );

    // With another ZScript mod on top, the module cannot be used.
    let other = tmp.join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("zscript.zs"), "class OtherThing : Actor {}\n").unwrap();
    let vfs = room_game::mount_embedded_engine(room_vfs::VfsBuilder::new())
        .unwrap()
        .mount_path(&iwad)
        .unwrap()
        .mount_path(&output)
        .unwrap()
        .mount_path(&other)
        .unwrap()
        .build()
        .unwrap();
    let zs = room_game::load_zscript_from_vfs(&vfs);
    assert!(
        !zs.ok && zs.notes[0].contains("convert the mod"),
        "{:?}",
        zs.notes
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn convert_brutal_doom_gated() {
    let (Ok(pk3), Some(iwad)) = (std::env::var("ROOM_BRUTALDOOM_PK3"), iwad()) else {
        eprintln!("skip: set ROOM_BRUTALDOOM_PK3");
        return;
    };
    let tics = std::env::var("ROOM_BD_AOT_TICS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(700);
    let output = std::env::temp_dir().join(format!("bd-converted-{}.pk3", std::process::id()));
    let report = convert(&ConvertOptions {
        input: PathBuf::from(pk3),
        output: output.clone(),
        iwad: Some(iwad),
        map: Some("E1M1".into()),
        verify_tics: tics,
        skill: 6,
        keep_zscript: false,
        allow_missing_natives: false,
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let v = report.verify.clone().expect("verified");
    eprintln!(
        "BD converted: {}/{} functions, {} wasm bytes; {} {} tics identical: {}; {} wasm calls ({})",
        report.compiled,
        report.functions,
        report.wasm_bytes,
        v.map,
        v.tics,
        v.passed(),
        v.aot_calls,
        v.engine
    );
    assert!(v.passed(), "{report:?}");
    assert!(report.missing_natives.is_empty());
    let _ = std::fs::remove_file(&output);
}

#[test]
fn converted_status_bar_draws_like_the_source() {
    let zs = "version \"4.10\"\nclass MyBar : DoomStatusBar\n{\n    override void Draw(int state, double ticfrac)\n    {\n        Super.Draw(state, ticfrac);\n        for (int i = 0; i < 3; i++)\n            DrawString(mHUDFont, FormatNumber(CPlayer.health + i, 3), (160, 100 + i * 10));\n    }\n}\n";
    let tmp = std::env::temp_dir().join(format!("room-convert-ui-{}", std::process::id()));
    let input = tmp.join("bar");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::write(input.join("zscript.zs"), zs).unwrap();
    std::fs::write(
        input.join("MAPINFO.txt"),
        "GameInfo\n{\n  StatusBarClass = \"MyBar\"\n}\n",
    )
    .unwrap();
    let output = tmp.join("bar-converted.pk3");
    let report = convert(&ConvertOptions {
        input: input.clone(),
        output: output.clone(),
        iwad: None,
        map: None,
        verify_tics: 0,
        skill: 3,
        keep_zscript: false,
        allow_missing_natives: true,
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.status_bar_class.as_deref(), Some("MyBar"));
    assert!(report.ui_wasm_bytes > 0, "{:?}", report.warnings);
    let draw = |pkg: &std::path::Path| {
        let vfs = room_game::mount_embedded_engine(room_vfs::VfsBuilder::new())
            .unwrap()
            .mount_path(pkg)
            .unwrap()
            .build()
            .unwrap();
        let prog = room_game::load_status_bar_program(&vfs).unwrap();
        let mut bar = room_ui::ScriptStatusBar::new(prog, "MyBar").unwrap();
        let snap = room_ui::SbarSnapshot {
            health: 57,
            face: "STFST01".into(),
            ..Default::default()
        };
        let dims = |_: &str| {
            Some(room_ui::PatchDims {
                w: 14.0,
                h: 16.0,
                left: 0.0,
                top: 0.0,
            })
        };
        let mut list = room_ui::UiDrawList::default();
        bar.draw(&snap, false, 1.0, &dims, &mut list);
        assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
        format!("{:?}", list.cmds)
    };
    assert_eq!(
        draw(&input),
        draw(&output),
        "converted status bar draws differently"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

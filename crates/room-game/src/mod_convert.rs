//! `room mod convert` (`docs/37` ZW9): replace a mod's ZScript sources by a
//! WebAssembly module and prove the result plays like the original.
//!
//! The output package holds every input entry except the ZScript sources,
//! plus [`CONVERTED_MODULE`] (a `room-zvm-wasm` RoomMod module: compiled code,
//! program image and flags) and [`CONVERTED_RECORD`] (what was converted and
//! whether it verified). [`crate::load_zscript_from_vfs`] loads such a package
//! without compiling ZScript.
//!
//! Verification runs the original package (ZScript on the interpreter) and the
//! converted one (shipped wasm) headless on the same map with the same
//! scripted input, saves and reloads both halfway, and compares
//! `World::checksum` and the ZScript checksum every tic.

use crate::zscript_load::{CONVERTED_MODULE, CONVERTED_RECORD, CONVERTED_UI_MODULE};
use crate::{GameOptions, Headless, Skill};
use room_sim::{Buttons, TicCmd};
use room_vfs::{Vfs, VfsBuilder};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What to convert and how to check it.
#[derive(Clone, Debug)]
pub struct ConvertOptions {
    /// Mod package: `.pk3`/`.zip` or a directory.
    pub input: PathBuf,
    /// Output `.pk3`.
    pub output: PathBuf,
    /// IWAD for verification (none → verification skipped, record says so).
    pub iwad: Option<PathBuf>,
    /// Map for verification (default: `MAP01` when present, else `E1M1`).
    pub map: Option<String>,
    /// Tics to verify (0 = skip).
    pub verify_tics: u32,
    /// Skill for verification (Brutal Doom's ZScript acts at 6).
    pub skill: u8,
    /// Also store the sources under `room/zscript-src/` (renamed `.src`, so
    /// they are not compiled).
    pub keep_zscript: bool,
    /// Convert even when the program calls natives this build lacks.
    pub allow_missing_natives: bool,
}

/// Outcome of a verification run.
#[derive(Clone, Debug, Default)]
pub struct VerifyResult {
    /// Map played.
    pub map: String,
    /// Tics compared.
    pub tics: u32,
    /// Tic of the save + reload (both sessions).
    pub save_load_tic: u32,
    /// First tic where the checksums differ.
    pub first_divergence: Option<u32>,
    /// ZScript top-level calls the converted session ran as wasm.
    pub aot_calls: u64,
    /// Wasm engine of the converted session.
    pub engine: String,
}

impl VerifyResult {
    /// True when every tic matched and the wasm code actually ran.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.first_divergence.is_none() && self.tics > 0
    }
}

/// What [`convert`] did.
#[derive(Clone, Debug, Default)]
pub struct ConvertReport {
    /// ZScript source paths replaced by the module.
    pub sources: Vec<String>,
    /// Functions in the program / compiled to wasm.
    pub functions: usize,
    /// Functions compiled to wasm.
    pub compiled: usize,
    /// Module size.
    pub wasm_bytes: usize,
    /// Natives the program calls.
    pub natives: Vec<String>,
    /// Natives this build does not implement (the VM would trap on them).
    pub missing_natives: Vec<String>,
    /// Entries copied from the input.
    pub entries_copied: usize,
    /// Verification, when an IWAD was given.
    pub verify: Option<VerifyResult>,
    /// Notes for the user.
    pub warnings: Vec<String>,
    /// `GameInfo.StatusBarClass`, when the mod sets one (a UI module is written).
    pub status_bar_class: Option<String>,
    /// Size of the UI module (library + mod), when written.
    pub ui_wasm_bytes: usize,
}

fn is_zip(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && &magic == b"PK\x03\x04"
}

fn sha256_of(path: &Path) -> Result<String, String> {
    let mut h = Sha256::new();
    if path.is_dir() {
        let mut files = Vec::new();
        walk(path, path, &mut files)?;
        files.sort();
        for rel in files {
            h.update(rel.as_bytes());
            h.update(std::fs::read(path.join(&rel)).map_err(|e| e.to_string())?);
        }
    } else {
        h.update(std::fs::read(path).map_err(|e| e.to_string())?);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out)?;
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn mount(paths: &[&Path]) -> Result<Vfs, String> {
    let mut b = crate::mount_embedded_engine(VfsBuilder::new()).map_err(|e| e.to_string())?;
    for p in paths {
        b = b
            .mount_path(p)
            .map_err(|e| format!("{}: {e}", p.display()))?;
    }
    b.build().map_err(|e| e.to_string())
}

/// Convert `opts.input` into `opts.output`.
pub fn convert(opts: &ConvertOptions) -> Result<ConvertReport, String> {
    let mut report = ConvertReport::default();
    // The mod's own sources …
    let mod_vfs = VfsBuilder::new()
        .mount_path(&opts.input)
        .map_err(|e| format!("{}: {e}", opts.input.display()))?
        .build()
        .map_err(|e| e.to_string())?;
    let (mod_sources, skipped) = crate::zscript_sources_from_vfs(&mod_vfs);
    if mod_sources.is_empty() {
        return Err("the input has no ZScript to convert".into());
    }
    report.warnings.extend(skipped);
    // … compiled the way the game compiles them: with the engine's ZScript.
    let vfs = mount(&[opts.input.as_path()])?;
    let (sources, _) = crate::zscript_sources_from_vfs(&vfs);
    let mod_paths: BTreeSet<String> = mod_sources
        .iter()
        .map(|(p, _)| p.to_ascii_lowercase())
        .collect();
    let external: Vec<(String, String)> = sources
        .iter()
        .filter(|(p, _)| !mod_paths.contains(&p.to_ascii_lowercase()))
        .cloned()
        .collect();
    let status_bar_class = room_defs::DefinitionsBuilder::new(&mod_vfs)
        .build()
        .ok()
        .and_then(|d| d.mapinfo.statusbar_class);
    let result = room_zscript::check(&sources);
    // The play program, as `load_zscript_from_vfs` compiles it. A mod whose
    // ZScript is only a status bar does not compile without the UZDoom
    // library; the game then runs no play program, and neither do we.
    let mut play_art = None;
    if result.ok() {
        let program = result.program.ok_or("compiler produced no program")?;
        // Natives the program needs, against this build's table (after the
        // parent-class fallback the VM applies).
        let linked = room_zvm::Zvm::new(program.clone());
        for name in &linked.program.native_imports {
            report.natives.push(name.clone());
            if linked.natives.id_ci(name).is_none() {
                report.missing_natives.push(name.clone());
            }
        }
        if !report.missing_natives.is_empty() && !opts.allow_missing_natives {
            return Err(format!(
                "natives this Room build does not implement (the VM would trap on them): {}",
                report.missing_natives.join(", ")
            ));
        }
        let art = room_zvm_wasm::roommod::compile_room_mod_with(&program, &external)
            .map_err(|e| e.to_string())?;
        report.functions = art.compiled.len();
        report.compiled = art.stats.compiled;
        report.wasm_bytes = art.bytes.len();
        for (idx, why) in &art.unsupported {
            report.warnings.push(format!(
                "{} stays interpreted: {why}",
                program.functions[*idx as usize].name
            ));
        }
        play_art = Some(art);
    } else if status_bar_class.is_some() {
        report.warnings.push(
            "the play-scope program does not compile without the UZDoom library (the game \
             runs none either); only the status-bar module is written"
                .into(),
        );
    } else {
        let errs: Vec<String> = result
            .diagnostics
            .iter()
            .take(20)
            .map(|d| d.format_line())
            .collect();
        return Err(format!("ZScript does not compile:\n{}", errs.join("\n")));
    }
    report.sources = mod_sources.iter().map(|(p, _)| p.clone()).collect();
    let skip: BTreeSet<String> = report
        .sources
        .iter()
        .map(|p| p.to_ascii_lowercase())
        .collect();

    // Package: input entries minus ZScript sources, plus the module.
    let out = std::fs::File::create(&opts.output)
        .map_err(|e| format!("{}: {e}", opts.output.display()))?;
    let mut zw = zip::ZipWriter::new(out);
    let deflate = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    if is_zip(&opts.input) {
        let f = std::fs::File::open(&opts.input).map_err(|e| e.to_string())?;
        let mut za = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
        for i in 0..za.len() {
            let entry = za.by_index_raw(i).map_err(|e| e.to_string())?;
            let name = entry.name().replace('\\', "/");
            if skip.contains(&name.to_ascii_lowercase()) || name.starts_with("room/zscript") {
                continue;
            }
            zw.raw_copy_file(entry).map_err(|e| e.to_string())?;
            report.entries_copied += 1;
        }
    } else if opts.input.is_dir() {
        let mut files = Vec::new();
        walk(&opts.input, &opts.input, &mut files)?;
        files.sort();
        for rel in files {
            if skip.contains(&rel.to_ascii_lowercase()) || rel.starts_with("room/zscript") {
                continue;
            }
            let bytes = std::fs::read(opts.input.join(&rel)).map_err(|e| e.to_string())?;
            zw.start_file(rel.as_str(), deflate)
                .map_err(|e| e.to_string())?;
            zw.write_all(&bytes).map_err(|e| e.to_string())?;
            report.entries_copied += 1;
        }
    } else {
        return Err("only .pk3/.zip packages and directories can be converted (not WADs)".into());
    }
    if let Some(art) = &play_art {
        zw.start_file(CONVERTED_MODULE, stored)
            .map_err(|e| e.to_string())?;
        zw.write_all(&art.bytes).map_err(|e| e.to_string())?;
    }
    // UI scope (`docs/37` §6.6.3): a ZScript status bar is compiled with the
    // UZDoom library, as `load_status_bar_program` does at run time.
    if let Some(class) = status_bar_class {
        let mut ui_sources = crate::uzdoom_stdlib_sources();
        if ui_sources.is_empty() {
            report.warnings.push(format!(
                "StatusBarClass {class}: wadsrc/uzdoom not found, no UI module"
            ));
        } else {
            ui_sources.extend(crate::status_bar_mod_sources(&vfs));
            let ui = room_zscript::check(&ui_sources);
            match ui.program {
                Some(ui_program) if ui.ok() => {
                    let ui_external: Vec<(String, String)> = ui_sources
                        .iter()
                        .filter(|(p, _)| !mod_paths.contains(&p.to_ascii_lowercase()))
                        .cloned()
                        .collect();
                    let ui_art =
                        room_zvm_wasm::roommod::compile_room_mod_with(&ui_program, &ui_external)
                            .map_err(|e| e.to_string())?;
                    zw.start_file(CONVERTED_UI_MODULE, stored)
                        .map_err(|e| e.to_string())?;
                    zw.write_all(&ui_art.bytes).map_err(|e| e.to_string())?;
                    report.ui_wasm_bytes = ui_art.bytes.len();
                }
                _ => report.warnings.push(format!(
                    "StatusBarClass {class}: library + mod do not compile, no UI module: {}",
                    ui.diagnostics
                        .iter()
                        .filter(|d| format!("{:?}", d.severity) == "Error")
                        .take(3)
                        .map(|d| d.format_line())
                        .collect::<Vec<_>>()
                        .join("; ")
                )),
            }
        }
        report.status_bar_class = Some(class);
    }
    if opts.keep_zscript {
        for (path, text) in &mod_sources {
            zw.start_file(format!("room/zscript-src/{path}.src"), deflate)
                .map_err(|e| e.to_string())?;
            zw.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        }
    }
    zw.finish().map_err(|e| e.to_string())?;

    // Verify, then append the record.
    match (&opts.iwad, opts.verify_tics) {
        (Some(iwad), n) if n > 0 => {
            let v = verify_converted(
                iwad,
                &opts.input,
                &opts.output,
                opts.map.as_deref(),
                n,
                opts.skill,
            )?;
            if !v.passed() {
                report.warnings.push(format!(
                    "verification failed: first divergence at tic {:?}",
                    v.first_divergence
                ));
            }
            report.verify = Some(v);
        }
        _ => report
            .warnings
            .push("not verified (needs --iwad and --verify N > 0)".into()),
    }
    let record = record_json(opts, &report)?;
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&opts.output)
        .map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new_append(f).map_err(|e| e.to_string())?;
    zw.start_file(CONVERTED_RECORD, deflate)
        .map_err(|e| e.to_string())?;
    zw.write_all(record.as_bytes()).map_err(|e| e.to_string())?;
    zw.finish().map_err(|e| e.to_string())?;
    Ok(report)
}

fn record_json(opts: &ConvertOptions, r: &ConvertReport) -> Result<String, String> {
    let verify = r.verify.as_ref().map(|v| {
        serde_json::json!({
            "map": v.map,
            "tics": v.tics,
            "save_load_tic": v.save_load_tic,
            "first_divergence": v.first_divergence,
            "aot_calls": v.aot_calls,
            "engine": v.engine,
            "skill": opts.skill,
        })
    });
    let value = serde_json::json!({
        "tool": "room mod convert",
        "room_version": env!("CARGO_PKG_VERSION"),
        "input": opts.input.file_name().map(|n| n.to_string_lossy().into_owned()),
        "input_sha256": sha256_of(&opts.input)?,
        "abi": room_zvm_wasm::roommod::abi_fingerprint(),
        "sources": r.sources,
        "functions": r.functions,
        "compiled": r.compiled,
        "natives": r.natives,
        "missing_natives": r.missing_natives,
        "status_bar_class": r.status_bar_class,
        "ui_module_bytes": r.ui_wasm_bytes,
        "warnings": r.warnings,
        "verified": r.verify.as_ref().is_some_and(VerifyResult::passed),
        "verify": verify,
    });
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

/// Scripted input for verification: walk, strafe, turn, fire, use.
#[must_use]
pub fn scripted_cmd(tic: u32) -> TicCmd {
    let mut cmd = TicCmd::default();
    let phase = tic % 140;
    cmd.forward_move = if phase < 70 { 25 } else { -10 };
    cmd.side_move = if (tic / 35) % 2 == 0 { 8 } else { -8 };
    cmd.yaw = if phase % 35 < 10 { 900 } else { 0 };
    let mut b = 0;
    if tic % 20 < 6 {
        b |= Buttons::ATTACK;
    }
    if tic % 50 == 0 {
        b |= Buttons::USE;
    }
    cmd.buttons = Buttons(b);
    cmd
}

/// Per-tic `(World::checksum, ZScript checksum)` of a headless session, the
/// AOT call count and engine. Saves and reloads at `save_at`.
pub fn session_trace(
    vfs: &Vfs,
    map: &str,
    tics: u32,
    skill: u8,
    save_at: u32,
) -> Result<(Vec<(u64, [u8; 32])>, u64, String), String> {
    let mut game = Headless::new(GameOptions {
        skill: Skill(skill),
        ..GameOptions::default()
    });
    let defs = room_defs::DefinitionsBuilder::new(vfs)
        .build()
        .map_err(|e| e.to_string())?;
    let handlers = defs.zscript_event_handlers();
    game.set_complevel(defs.complevel);
    game.set_terrain(&defs.terrain);
    game.set_keyconf(&defs.keyconf);
    game.set_acs_library_pool(crate::acs_library_pool(vfs), defs.loadacs.libraries.clone());
    game.set_mapinfo(Arc::new(defs.mapinfo));
    let zs = crate::load_zscript_from_vfs(vfs);
    if !zs.ok {
        return Err(format!("ZScript: {}", zs.notes.join("; ")));
    }
    game.set_zscript_lumps(crate::zscript_wad_lumps(vfs, zs.program.as_ref()));
    game.set_zscript(zs.program, handlers);
    let data = crate::load_embedded_engine_data_with(vfs).map_err(|e| e.to_string())?;
    game.set_defs(data.classes, data.states);
    let mut level = room_map::load_map(vfs, map).map_err(|e| format!("{map}: {e}"))?;
    room_nodebuild::ensure_nodes(&mut level);
    game.start_custom_map(level, Skill(skill))
        .map_err(|e| e.to_string())?;
    // Verification enters every compiled function through wasm, not only the
    // `auto` policy's hot ones, so all of the shipped code is exercised.
    if let Some(w) = game.world_mut() {
        w.zscript.set_aot_all();
    }
    let mut trace = Vec::with_capacity(tics as usize);
    for t in 0..tics {
        if t == save_at && t > 0 {
            let bytes = game
                .save_game()
                .map_err(|e| format!("save at tic {t}: {e}"))?;
            game.load_game(&bytes)
                .map_err(|e| format!("load at tic {t}: {e}"))?;
        }
        game.tick(&[scripted_cmd(t)]);
        let w = game.world().ok_or("no world")?;
        trace.push((w.checksum(), w.zscript.checksum_bytes().unwrap_or([0; 32])));
    }
    let w = game.world().ok_or("no world")?;
    Ok((
        trace,
        w.zscript.aot_calls(),
        w.zscript.aot_engine().unwrap_or("interpreter").to_owned(),
    ))
}

/// Play `original` and `converted` side by side for `tics` tics.
pub fn verify_converted(
    iwad: &Path,
    original: &Path,
    converted: &Path,
    map: Option<&str>,
    tics: u32,
    skill: u8,
) -> Result<VerifyResult, String> {
    // The original must run on the interpreter: forget converted modules.
    room_zvm_wasm::roommod::clear_shipped();
    let vfs_a = mount(&[iwad, original])?;
    let map = match map {
        Some(m) => m.to_owned(),
        None if vfs_a.find("MAP01", room_vfs::Namespace::Global).is_some() => "MAP01".into(),
        None => "E1M1".into(),
    };
    let save_at = tics / 2;
    let (a, _, _) = session_trace(&vfs_a, &map, tics, skill, save_at)?;
    drop(vfs_a);
    let vfs_b = mount(&[iwad, converted])?;
    let (b, aot_calls, engine) = session_trace(&vfs_b, &map, tics, skill, save_at)?;
    room_zvm_wasm::roommod::clear_shipped();
    let first_divergence = a
        .iter()
        .zip(&b)
        .position(|(x, y)| x != y)
        .map(|i| i as u32)
        .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len()) as u32));
    Ok(VerifyResult {
        map,
        tics,
        save_load_tic: save_at,
        first_divergence,
        aot_calls,
        engine,
    })
}

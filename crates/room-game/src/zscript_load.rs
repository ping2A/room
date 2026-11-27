//! Collect / compile ZScript from a mounted VFS for play sessions (`docs/34`).

use room_vfs::Vfs;
use room_zvm::CompiledProgram;
use std::collections::BTreeMap;

/// Result of scanning + compiling ZScript sources from the VFS.
#[derive(Debug)]
pub struct ZscriptLoadReport {
    /// Source paths that were compiled (entry first).
    pub files: Vec<String>,
    /// Compiled program when check succeeded.
    pub program: Option<CompiledProgram>,
    /// True when there were no errors (or no sources).
    pub ok: bool,
    /// Human-readable diagnostics (errors + first warnings).
    pub notes: Vec<String>,
    /// Sources that could not be read, or were decoded lossy.
    pub skipped: Vec<String>,
    /// The mod did not compile against Room's small `base.zs` and was compiled
    /// with the vendored UZDoom library instead: load it with
    /// `World::load_zscript_program_stubbed`.
    pub uzdoom_library: bool,
}

/// Collect `.zs` / `.zc` / `.zsc` / `ZSCRIPT` lumps from `vfs`, compile them, and
/// return a report. Empty VFS → `ok` with no program.
///
/// A package made by `room mod convert` (`docs/37` ZW9) has no ZScript
/// sources but a [`CONVERTED_MODULE`]: its program is read from the module and
/// the module is registered as the program's compiled code, so the VM runs it
/// (AOT tier) without the ZScript compiler.
#[must_use]
pub fn load_zscript_from_vfs(vfs: &Vfs) -> ZscriptLoadReport {
    let (mut sources, skipped) = zscript_sources_from_vfs(vfs);
    if let Some(id) = vfs.find_full(CONVERTED_MODULE) {
        return converted_report(vfs, id, sources, skipped);
    }
    if !sources.is_empty() {
        sources.extend(cvar_globals_source(vfs));
    }
    compile_report(sources, skipped)
}

/// Path of the generated source declaring the mod's CVARINFO names.
pub const CVAR_GLOBALS_SOURCE: &str = "room/cvarinfo_globals.zs";

/// GZDoom scripts read CVARINFO variables by name (`if (SpawnBalance == 2)`).
/// Declare them for the compiler as fields of `class _CVars`; reads go to the
/// host's CVar store. `None` when the mod has no CVARINFO.
#[must_use]
pub fn cvar_globals_source(vfs: &Vfs) -> Option<(String, String)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut body = String::new();
    for id in vfs.find_all("CVARINFO", room_vfs::Namespace::Global) {
        let Ok(bytes) = vfs.read(id) else {
            continue;
        };
        let Ok(decls) = room_defs::parse_cvarinfo(&String::from_utf8_lossy(&bytes)) else {
            continue;
        };
        for d in decls {
            let ok = d
                .name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && d.name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !ok || !seen.insert(d.name.to_ascii_lowercase()) {
                continue;
            }
            let ty = match d.ty {
                room_defs::CvarType::Float => "double",
                room_defs::CvarType::String => "String",
                room_defs::CvarType::Bool => "bool",
                room_defs::CvarType::Int | room_defs::CvarType::Color => "int",
            };
            body.push_str(&format!("\tnative readonly {ty} {};\n", d.name));
        }
    }
    (!body.is_empty()).then(|| {
        (
            CVAR_GLOBALS_SOURCE.to_owned(),
            format!("class _CVars native\n{{\n{body}}}\n"),
        )
    })
}

/// Path of a converted mod's module inside its package (`room mod convert`).
pub const CONVERTED_MODULE: &str = "room/zscript.wasm";
/// A converted mod's status-bar program (UZDoom library + mod), when the mod
/// sets `GameInfo.StatusBarClass`.
pub const CONVERTED_UI_MODULE: &str = "room/zscript_ui.wasm";
/// Conversion record next to [`CONVERTED_MODULE`].
pub const CONVERTED_RECORD: &str = "room/convert.json";

fn converted_report(
    vfs: &Vfs,
    id: room_vfs::LumpId,
    sources: Vec<(String, String)>,
    skipped: Vec<String>,
) -> ZscriptLoadReport {
    let files = vec![CONVERTED_MODULE.to_owned()];
    let loaded = vfs
        .read(id)
        .map_err(|e| e.to_string())
        .and_then(|b| room_zvm_wasm::read_room_mod(&b).map_err(|e| e.to_string()));
    let m = match loaded {
        Ok(m) => m,
        Err(e) => {
            return ZscriptLoadReport {
                uzdoom_library: false,
                files,
                program: None,
                ok: false,
                notes: vec![format!("{CONVERTED_MODULE}: {e}")],
                skipped,
            };
        }
    };
    // The module's program already contains the ZScript of the other archives
    // it was converted against (the engine's own). Any other set of sources
    // means it cannot be used: GZDoom compiles all ZScript as one program.
    if !m.matches_external(&sources) {
        let mut report = compile_report(sources, skipped);
        report.ok = false;
        report.program = None;
        report.notes.insert(
            0,
            format!(
                "{CONVERTED_MODULE}: compiled against other ZScript sources than the ones \
                 loaded now (another ZScript mod, or a different Room build); convert the mod \
                 again"
            ),
        );
        return report;
    }
    let verified = vfs
        .find_full(CONVERTED_RECORD)
        .and_then(|r| vfs.read(r).ok())
        .map(|b| String::from_utf8_lossy(&b).replace([' ', '\n', '\t'], ""))
        .is_some_and(|t| t.contains("\"verified\":true"));
    let mut notes = vec![format!(
        "converted mod: {} function(s) as WebAssembly, {} class(es), no ZScript compile",
        m.program.functions.len(),
        m.program.classes.len()
    )];
    if !verified {
        notes.push(format!(
            "warning: {CONVERTED_RECORD} does not say the conversion was verified; behaviour may \
             differ from the original mod"
        ));
    }
    let program = m.program.clone();
    room_zvm_wasm::register_shipped(m);
    ZscriptLoadReport {
        uzdoom_library: false,
        files,
        program: Some(program),
        ok: true,
        notes,
        skipped,
    }
}

/// The vendored UZDoom ZScript library (`wadsrc/uzdoom`) as `(path, text)`,
/// entry file first. Empty when the folder is not found.
#[must_use]
pub fn uzdoom_stdlib_sources() -> Vec<(String, String)> {
    let candidates = [
        std::path::PathBuf::from("wadsrc/uzdoom"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom"),
    ];
    let Some(root) = candidates
        .into_iter()
        .find(|p| p.join("zscript.txt").is_file())
    else {
        return Vec::new();
    };
    let Ok(entry) = std::fs::read_to_string(root.join("zscript.txt")) else {
        return Vec::new();
    };
    // Paths under `uzdoom/` so a mod's own `zscript.txt` / `zscript/…` files
    // stay separate; the entry's `#include`s are rewritten to match.
    let mut text = String::new();
    let mut files = Vec::new();
    for line in entry.lines() {
        if let Some(rest) = line.trim().strip_prefix("#include") {
            let path = rest.trim().trim_matches('"').replace('\\', "/");
            text.push_str(&format!("#include \"uzdoom/{path}\"\n"));
            if let Ok(body) = std::fs::read_to_string(root.join(&path)) {
                files.push((format!("uzdoom/{path}"), body));
            }
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }
    let mut sources = vec![("uzdoom/zscript.txt".to_owned(), text)];
    sources.extend(files);
    sources
}

/// Program for a ZScript status bar: the UZDoom library plus the mod's own
/// ZScript from `vfs` (GZDoom `GameInfo.StatusBarClass`). `Err` carries the
/// first diagnostics.
pub fn load_status_bar_program(vfs: &Vfs) -> Result<CompiledProgram, String> {
    let mut sources = uzdoom_stdlib_sources();
    if sources.is_empty() {
        return Err("wadsrc/uzdoom not found".into());
    }
    sources.extend(status_bar_mod_sources(vfs));
    sources.extend(cvar_globals_source(vfs));
    // A converted mod ships its UI program (library + mod) precompiled.
    if let Some(id) = vfs.find_full(CONVERTED_UI_MODULE) {
        let m = vfs
            .read(id)
            .map_err(|e| e.to_string())
            .and_then(|b| room_zvm_wasm::read_room_mod(&b).map_err(|e| e.to_string()))?;
        if !m.matches_external(&sources) {
            return Err(format!(
                "{CONVERTED_UI_MODULE} was compiled against other ZScript sources; convert the \
                 mod again"
            ));
        }
        let program = m.program.clone();
        room_zvm_wasm::register_shipped(m);
        return Ok(program);
    }
    let result = room_zscript::check_lenient(&sources);
    if !result.ok() {
        return Err(result
            .diagnostics
            .iter()
            .filter(|d| format!("{:?}", d.severity) == "Error")
            .take(4)
            .map(|d| format!("{} ({})", d.message, d.path))
            .collect::<Vec<_>>()
            .join("; "));
    }
    result.program.ok_or_else(|| {
        result
            .diagnostics
            .iter()
            .take(4)
            .map(|d| format!("{} ({})", d.message, d.path))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

/// The mounted ZScript a status-bar program adds to the UZDoom library: every
/// source except the engine's own `room.pk3` stubs, which the library
/// supersedes (compiling both fails on duplicate definitions).
#[must_use]
pub fn status_bar_mod_sources(vfs: &Vfs) -> Vec<(String, String)> {
    let mut engine = std::collections::BTreeSet::new();
    for (_, info) in vfs.iter() {
        if vfs
            .container(info.container)
            .is_some_and(|c| c.name == "room.pk3")
        {
            engine.insert(info.full_path.to_ascii_lowercase());
        }
    }
    let (mods, _) = zscript_sources_from_vfs(vfs);
    mods.into_iter()
        .filter(|(p, _)| !engine.contains(&p.to_ascii_lowercase()))
        .collect()
}

/// ZScript sources of `vfs` as `(path, text)`, entry file (`zscript` /
/// `zscript.zc`) first, plus the paths that were skipped.
///
/// A file whose path an archive loaded later also has is replaced by the
/// later one (GZDoom full-path lookup). WAD lumps (path = short name, e.g.
/// two PWADs each with `ZSCRIPT`) are all kept, except that a container named
/// `editor-overrides.*` (the web editor's edits) replaces the latest earlier
/// source with the same path.
#[must_use]
pub fn zscript_sources_from_vfs(vfs: &Vfs) -> (Vec<(String, String)>, Vec<String>) {
    let (mut sources, mut skipped) = collect_zscript_sources(vfs);
    let replaced = crate::mods::replaced_zscript(vfs);
    if !replaced.is_empty() {
        drop_replaced_sources(&mut sources, &replaced, &mut skipped);
    }
    sort_entry_first(&mut sources);
    (sources, skipped)
}

/// Remove the files a WASM port replaces (`[mod] replaces_zscript`) and
/// comment out the `#include`s that name them, so the rest still compiles.
fn drop_replaced_sources(
    sources: &mut Vec<(String, String)>,
    replaced: &[String],
    notes: &mut Vec<String>,
) {
    let norm = |p: &str| {
        p.replace('\\', "/")
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_ascii_lowercase()
    };
    sources.retain(|(path, _)| {
        let gone = replaced.contains(&norm(path));
        if gone {
            notes.push(format!("{path}: replaced by a WASM mod (replaces_zscript)"));
        }
        !gone
    });
    for (_, text) in sources.iter_mut() {
        if !text.contains("#include") {
            continue;
        }
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            let target = include_paths(line);
            if target.first().is_some_and(|t| replaced.contains(&norm(t))) {
                out.push_str("// replaced by a WASM mod: ");
            }
            out.push_str(line);
        }
        *text = out;
    }
}

fn sort_entry_first(sources: &mut [(String, String)]) {
    sources.sort_by(|a, b| {
        let score = |n: &str| if is_zscript_root(n) { 0 } else { 1 };
        score(&a.0).cmp(&score(&b.0)).then_with(|| a.0.cmp(&b.0))
    });
}

fn compile_report(sources: Vec<(String, String)>, skipped: Vec<String>) -> ZscriptLoadReport {
    let strict = compile_report_strict(sources.clone(), skipped.clone());
    if strict.ok || sources.is_empty() {
        return strict;
    }
    // Mods written against the whole GZDoom library (Project Brutality) do not
    // compile against Room's `base.zs`: try the vendored UZDoom library, with
    // any function Room cannot lower yet running as an empty body.
    let library = uzdoom_stdlib_sources();
    if library.is_empty() {
        return strict;
    }
    // Room's own stub library (mounted from room.pk3) would redefine `Actor`.
    let mut all: Vec<(String, String)> = sources
        .iter()
        .filter(|(_, text)| !room_zscript::is_room_stdlib_source(text))
        .cloned()
        .collect();
    all.extend(library);
    let result = room_zscript::check_lenient(&all);
    let ok = result_ok(&result.diagnostics);
    let Some(prog) = result.program.filter(|_| ok) else {
        let mut strict = strict;
        let errors: Vec<String> = result
            .diagnostics
            .iter()
            .filter(|d| d.severity == room_zscript::Severity::Error)
            .take(8)
            .map(room_zscript::Diagnostic::format_line)
            .collect();
        strict.notes.insert(
            1,
            format!(
                "with the UZDoom library it fails too: {}",
                errors.join(" | ")
            ),
        );
        return strict;
    };
    let stubbed: Vec<&room_zscript::Diagnostic> = result
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("runs as an empty function"))
        .collect();
    let mut notes = vec![format!(
        "compiled ok with the UZDoom library: {} file(s), {} class(es), {} function(s); \
         {} function(s) run as empty bodies",
        sources.len(),
        prog.classes.len(),
        prog.functions.len(),
        stubbed.len()
    )];
    for d in stubbed.iter().take(24) {
        notes.push(format!(
            "Warning: {} ({}:{})",
            d.message, d.path, d.span.line
        ));
    }
    if stubbed.len() > 24 {
        notes.push(format!("… {} more empty function(s)", stubbed.len() - 24));
    }
    // Stores to engine fields Room does not model (`psp.sprite = …`).
    let ignored: Vec<&room_zscript::Diagnostic> = result
        .diagnostics
        .iter()
        .filter(|d| {
            d.message
                .contains("store to an unsupported field is ignored")
        })
        .collect();
    for d in ignored.iter().take(8) {
        notes.push(format!(
            "Warning: {} ({}:{})",
            d.message, d.path, d.span.line
        ));
    }
    if ignored.len() > 8 {
        notes.push(format!(
            "… {} more ignored field store(s)",
            ignored.len() - 8
        ));
    }
    ZscriptLoadReport {
        uzdoom_library: true,
        files: sources.iter().map(|(p, _)| p.clone()).collect(),
        program: Some(prog),
        ok: true,
        notes,
        skipped,
    }
}

fn result_ok(diags: &[room_zscript::Diagnostic]) -> bool {
    !diags
        .iter()
        .any(|d| d.severity == room_zscript::Severity::Error)
}

fn compile_report_strict(
    sources: Vec<(String, String)>,
    skipped: Vec<String>,
) -> ZscriptLoadReport {
    let files: Vec<String> = sources.iter().map(|(p, _)| p.clone()).collect();
    if sources.is_empty() {
        return ZscriptLoadReport {
            uzdoom_library: false,
            files,
            program: None,
            ok: true,
            notes: vec!["no ZScript sources in VFS".into()],
            skipped,
        };
    }
    let result = room_zscript::check(&sources);
    let mut notes = Vec::new();
    for d in result.diagnostics.iter().take(24) {
        notes.push(format!("{:?}: {} ({})", d.severity, d.message, d.path));
    }
    let extra = result.diagnostics.len().saturating_sub(24);
    if extra > 0 {
        notes.push(format!("… {extra} more diagnostic(s)"));
    }
    if result.ok() {
        let prog = result.program;
        let n_fn = prog.as_ref().map(|p| p.functions.len()).unwrap_or(0);
        let n_cls = prog.as_ref().map(|p| p.classes.len()).unwrap_or(0);
        notes.insert(
            0,
            format!(
                "compiled ok: {} file(s), {} class(es), {} function(s)",
                files.len(),
                n_cls,
                n_fn
            ),
        );
        ZscriptLoadReport {
            uzdoom_library: false,
            files,
            program: prog,
            ok: true,
            notes,
            skipped,
        }
    } else {
        notes.insert(0, format!("compile failed ({} file(s))", files.len()));
        ZscriptLoadReport {
            uzdoom_library: false,
            files,
            program: None,
            ok: false,
            notes,
            skipped,
        }
    }
}

/// A `ZSCRIPT` root: the `ZSCRIPT` lump of a WAD, or `zscript.<any ext>` at
/// the top of an archive (`zscript.txt`, `zscript.zc`, `zscript.zs`), as
/// GZDoom loads them.
fn is_zscript_root(path: &str) -> bool {
    let p = path.replace('\\', "/");
    if p.contains('/') {
        return false;
    }
    let stem = p.split('.').next().unwrap_or(&p);
    stem.eq_ignore_ascii_case("zscript")
}

/// `#include "path"` targets of a ZScript source, normalised for VFS lookup.
fn include_paths(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let rest = l.trim_start().strip_prefix("#include")?.trim();
            let rest = rest.strip_prefix('"')?;
            let end = rest.find('"')?;
            let p = rest[..end].replace('\\', "/");
            let p = p.trim_start_matches("./").trim_start_matches('/');
            (!p.is_empty()).then(|| p.to_owned())
        })
        .collect()
}

fn read_text(
    vfs: &Vfs,
    id: room_vfs::LumpId,
    path: &str,
    skipped: &mut Vec<String>,
) -> Option<String> {
    let bytes = match vfs.read(id) {
        Ok(b) => b,
        Err(e) => {
            skipped.push(format!("{path}: read failed: {e}"));
            return None;
        }
    };
    let (text, lossy) = match String::from_utf8(bytes.to_vec()) {
        Ok(t) => (t, false),
        Err(e) => {
            let raw = e.into_bytes();
            (String::from_utf8_lossy(&raw).into_owned(), true)
        }
    };
    if lossy {
        skipped.push(format!(
            "{path}: invalid utf-8 (loaded with replacement characters)"
        ));
    }
    if text.trim().is_empty() {
        skipped.push(format!("{path}: empty"));
        return None;
    }
    Some(text)
}

fn collect_zscript_sources(vfs: &Vfs) -> (Vec<(String, String)>, Vec<String>) {
    let mut sources: Vec<(String, String)> = Vec::new();
    // Container of each source (`None`: WAD lump / loose file — always kept).
    let mut origin: Vec<Option<u32>> = Vec::new();
    let mut skipped = Vec::new();
    // Lowercase path → index in `sources` (archive files and editor overrides).
    let mut by_path: std::collections::BTreeMap<String, usize> = Default::default();
    for (id, info) in vfs.iter() {
        let path = if !info.full_path.is_empty() {
            info.full_path.clone()
        } else {
            info.short_name.as_str().to_owned()
        };
        let lower = path.to_ascii_lowercase();
        let is_zs = lower.ends_with(".zs")
            || lower.ends_with(".zc")
            || lower.ends_with(".zsc")
            || lower == "zscript"
            || lower.ends_with("/zscript")
            || is_zscript_root(&lower);
        if !is_zs {
            continue;
        }
        let Some(text) = read_text(vfs, id, &path, &mut skipped) else {
            continue;
        };
        let container = vfs.container(info.container);
        let archive = container.is_some_and(|c| {
            !matches!(
                c.kind,
                room_vfs::ContainerKind::Wad | room_vfs::ContainerKind::LooseFile
            )
        });
        let editor = container.is_some_and(|c| c.name.starts_with("editor-overrides."));
        let key = path.to_ascii_lowercase();
        if (archive || editor)
            && let Some(&i) = by_path.get(&key)
            && let Some(slot) = sources.get_mut(i)
        {
            *slot = (path, text);
            if let Some(o) = origin.get_mut(i) {
                *o = archive.then_some(info.container.0);
            }
            continue;
        }
        by_path.insert(key, sources.len());
        sources.push((path, text));
        origin.push(archive.then_some(info.container.0));
    }
    // GZDoom includes any file by path, whatever its extension: mods keep
    // ZScript in `actors/foo.txt` or `src/x.txt` and pull it in from the root.
    let mut next = 0;
    while next < sources.len() {
        for inc in room_zscript::include_targets(&sources[next].0, &sources[next].1) {
            let key = inc.to_ascii_lowercase();
            if by_path.contains_key(&key) {
                continue;
            }
            let Some(id) = vfs.find_full(&key) else {
                continue;
            };
            by_path.insert(key, sources.len());
            if let Some(text) = read_text(vfs, id, &inc, &mut skipped) {
                sources.push((inc, text));
                origin.push(None);
            }
        }
        next += 1;
    }
    prune_unreachable(&mut sources, &origin, &mut skipped);
    (sources, skipped)
}

/// GZDoom compiles an archive's `zscript` root and what it includes, nothing
/// else: drop the other `.zs`/`.zc` files of an archive that has a root
/// (examples, `.bak` copies, disabled code). Archives without a root keep all.
fn prune_unreachable(
    sources: &mut Vec<(String, String)>,
    origin: &[Option<u32>],
    skipped: &mut Vec<String>,
) {
    let rooted: std::collections::BTreeSet<u32> = sources
        .iter()
        .zip(origin)
        .filter(|((p, _), _)| is_zscript_root(p))
        .filter_map(|(_, o)| *o)
        .collect();
    if rooted.is_empty() {
        return;
    }
    let index: std::collections::BTreeMap<String, usize> = sources
        .iter()
        .enumerate()
        .map(|(i, (p, _))| (p.to_ascii_lowercase(), i))
        .collect();
    let mut keep = vec![false; sources.len()];
    let mut work: Vec<usize> = Vec::new();
    for (i, ((p, _), o)) in sources.iter().zip(origin).enumerate() {
        if is_zscript_root(p) || !o.is_some_and(|c| rooted.contains(&c)) {
            keep[i] = true;
            work.push(i);
        }
    }
    while let Some(i) = work.pop() {
        for inc in room_zscript::include_targets(&sources[i].0, &sources[i].1) {
            if let Some(&j) = index.get(&inc.to_ascii_lowercase()) {
                if !keep[j] {
                    keep[j] = true;
                    work.push(j);
                }
            }
        }
    }
    let mut k = 0;
    sources.retain(|(p, _)| {
        let kept = keep[k];
        k += 1;
        if !kept {
            skipped.push(format!("{p}: not included from the archive's ZSCRIPT root"));
        }
        kept
    });
}

/// Log a load report to stderr (`room: ZScript: …`).
///
/// Set `ROOM_ZSCRIPT_DEBUG=1` for per-file paths and diagnostic lines.
pub fn log_zscript_report(report: &ZscriptLoadReport, handlers: &[String]) {
    let debug = std::env::var_os("ROOM_ZSCRIPT_DEBUG").is_some_and(|v| v != "0");
    if report.files.is_empty() {
        eprintln!("room: ZScript: none found in VFS");
        for s in &report.skipped {
            eprintln!("room: ZScript skip: {s}");
        }
        return;
    }
    let status = if report.ok { "ok" } else { "FAILED" };
    eprintln!(
        "room: ZScript: {status} — {} file(s), handlers=[{}]",
        report.files.len(),
        handlers.join(", ")
    );
    if let Some(prog) = &report.program {
        let names: Vec<&str> = prog.classes.iter().map(|c| c.name.as_str()).collect();
        eprintln!(
            "room: ZScript classes ({}): {}",
            names.len(),
            names.join(", ")
        );
        for h in handlers {
            let found = prog.class_by_name.keys().any(|n| n.eq_ignore_ascii_case(h));
            if !found {
                eprintln!("room: ZScript handler '{h}': CLASS MISSING from program");
            } else if debug {
                eprintln!("room: ZScript handler '{h}': class present");
            }
        }
    }
    for s in &report.skipped {
        eprintln!("room: ZScript skip: {s}");
    }
    if debug {
        for f in &report.files {
            eprintln!("room: ZScript source: {f}");
        }
        for n in &report.notes {
            eprintln!("room: ZScript note: {n}");
        }
    } else if let Some(first) = report.notes.first() {
        eprintln!("room: ZScript: {first}");
        if !report.ok {
            for n in report.notes.iter().skip(1).take(8) {
                eprintln!("room: ZScript: {n}");
            }
            eprintln!("room: ZScript: set ROOM_ZSCRIPT_DEBUG=1 for full diagnostics");
        }
    }
}

/// Text lumps a program can read through `Wads.CheckNumForFullName` /
/// `CheckNumForName` + `ReadLump`: every string constant that names a file
/// in `vfs` (`"pbdata/matsys/materials.json"`) or a global lump, keyed by
/// that string. Binary lumps and lumps over 1 MiB are left out.
#[must_use]
pub fn zscript_wad_lumps(vfs: &Vfs, program: Option<&CompiledProgram>) -> BTreeMap<String, String> {
    const MAX: usize = 1 << 20;
    let mut out = BTreeMap::new();
    let Some(program) = program else {
        return out;
    };
    let mut seen = std::collections::BTreeSet::new();
    for name in program.functions.iter().flat_map(|f| f.const_s.iter()) {
        if name.is_empty() || name.len() > 255 || !seen.insert(name.as_str()) {
            continue;
        }
        let id = if name.contains('/') {
            vfs.find_full(name)
        } else if name.len() <= 8 && !name.contains(' ') {
            vfs.find(name, room_vfs::Namespace::Global)
        } else {
            None
        };
        let Some(bytes) = id.and_then(|id| vfs.read(id).ok()) else {
            continue;
        };
        if bytes.len() > MAX {
            continue;
        }
        if let Ok(text) = std::str::from_utf8(&bytes) {
            out.insert(name.clone(), text.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod source_tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::{VfsBuilder, WadWriter, zip_from_files};

    /// String constants that name files or lumps become the `Wads.*` view:
    /// PB reads `pbdata/matsys/materials.json` this way.
    #[test]
    fn wad_lumps_follow_the_programs_string_constants() {
        let pk3 = zip_from_files(&[
            ("pbdata/matsys/materials.json", b"{\"water\":{}}".as_slice()),
            ("pbdata/other.json", b"{}".as_slice()),
            ("sprites/play.bin", [0xffu8, 0xfe, 0x00].as_slice()),
        ])
        .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("mod.pk3", pk3)
            .unwrap()
            .build()
            .unwrap();
        let src = r#"
class ReadsJson
{
    static int Go()
    {
        int lump = Wads.CheckNumForFullName("pbdata/matsys/materials.json");
        string text = Wads.ReadLump(lump);
        string missing = "pbdata/none.json";
        string binary = "sprites/play.bin";
        return text.Length();
    }
}
"#;
        let result = room_zscript::check(&[("reads.zs".into(), src.into())]);
        let prog = result.program.expect("program");
        let lumps = zscript_wad_lumps(&vfs, Some(&prog));
        assert_eq!(
            lumps.keys().collect::<Vec<_>>(),
            ["pbdata/matsys/materials.json"],
            "only named text files"
        );
        let mut rt = room_sim::ZscriptRuntime::default();
        rt.load_program(prog);
        rt.set_wad_lumps(lumps);
        assert_eq!(rt.script_call("ReadsJson.Go").unwrap(), 12);
    }

    #[test]
    fn later_archive_replaces_same_path() {
        let base = zip_from_files(&[
            ("zscript.zs", b"#include \"zscript/a.zs\"".as_slice()),
            ("zscript/a.zs", b"class A {}".as_slice()),
        ])
        .unwrap();
        let edits = zip_from_files(&[
            ("zscript/a.zs", b"class A2 {}".as_slice()),
            ("room-editor.txt", b"".as_slice()),
        ])
        .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("mod.pk3", base)
            .unwrap()
            .mount_memory("editor-overrides.pk3", edits)
            .unwrap()
            .build()
            .unwrap();
        let (sources, _) = zscript_sources_from_vfs(&vfs);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].0, "zscript.zs");
        let a: Vec<_> = sources
            .iter()
            .filter(|(p, _)| p == "zscript/a.zs")
            .collect();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].1, "class A2 {}");
    }

    #[test]
    fn mod_manifest_replaces_zscript_files() {
        let base = zip_from_files(&[
            (
                "zscript.zc",
                b"version \"3.2.3\"\n#include \"zscript/BD_WCS.zc\"\n#include \"zscript/Keep.zc\"\n"
                    .as_slice(),
            ),
            ("zscript/BD_WCS.zc", b"class ZandroCompat {}".as_slice()),
            ("zscript/Keep.zc", b"class Keep {}".as_slice()),
        ])
        .unwrap();
        let port = WadWriter::pwad()
            .add(
                "ROOMMOD",
                b"[mod]\nid = \"p\"\nreplaces_zscript = [\"zscript/bd_wcs.zc\"]\n".to_vec(),
            )
            .finish()
            .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("bd.pk3", base)
            .unwrap()
            .mount_memory("port.wad", port)
            .unwrap()
            .build()
            .unwrap();
        let (sources, notes) = zscript_sources_from_vfs(&vfs);
        let paths: Vec<_> = sources.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["zscript.zc", "zscript/keep.zc"]); // zip paths are lower-cased
        assert!(
            sources[0]
                .1
                .contains("// replaced by a WASM mod: #include \"zscript/BD_WCS.zc\"")
        );
        assert!(sources[0].1.contains("\n#include \"zscript/Keep.zc\""));
        assert!(notes.iter().any(|n| n.contains("replaces_zscript")));
        assert!(load_zscript_from_vfs(&vfs).ok);
    }

    #[test]
    fn wad_zscript_lumps_are_kept_unless_editor_override() {
        let wad = |t: &str| {
            WadWriter::pwad()
                .add("ZSCRIPT", t.as_bytes().to_vec())
                .finish()
                .unwrap()
        };
        let vfs = VfsBuilder::new()
            .mount_memory("a.wad", wad("class A {}"))
            .unwrap()
            .mount_memory("b.wad", wad("class B {}"))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(zscript_sources_from_vfs(&vfs).0.len(), 2);
        let vfs = VfsBuilder::new()
            .mount_memory("a.wad", wad("class A {}"))
            .unwrap()
            .mount_memory("editor-overrides.wad", wad("class A2 {}"))
            .unwrap()
            .build()
            .unwrap();
        let (sources, _) = zscript_sources_from_vfs(&vfs);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].1, "class A2 {}");
    }

    #[test]
    fn cvarinfo_names_are_script_globals() {
        let pk3 = zip_from_files(&[
            (
                "zscript.zs",
                b"version \"4.10\"\nclass Spawner : Actor\n{\n    int Pick() { return SpawnBalance == 2 ? levelsToPlay : 0; }\n}\n"
                    .as_slice(),
            ),
            (
                "cvarinfo.txt",
                b"server int SpawnBalance = 2;\nserver int levelsToPlay = 32;\nuser float pb_fov = 1.5;\n"
                    .as_slice(),
            ),
        ])
        .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("mod.pk3", pk3)
            .unwrap()
            .build()
            .unwrap();
        let (_, text) = cvar_globals_source(&vfs).unwrap();
        assert!(text.contains("native readonly int SpawnBalance;"), "{text}");
        assert!(text.contains("native readonly double pb_fov;"), "{text}");
        let report = load_zscript_from_vfs(&vfs);
        assert!(report.ok, "{:?}", report.notes);
    }

    #[test]
    fn includes_with_any_extension_and_txt_roots_are_loaded() {
        let pk3 = zip_from_files(&[
            (
                "zscript.txt",
                b"version \"4.10\"\n#include \"src/handlers.txt\"\n".as_slice(),
            ),
            (
                "src/handlers.txt",
                b"#include \"actors/imp.txt\"\n#include \"src/rel.zs\"\n/*\n#include \"src/old.zc\"\n*/\nclass H : EventHandler {}\n".as_slice(),
            ),
            ("actors/imp.txt", b"class FastImp : Actor {}\n".as_slice()),
            ("src/unused.txt", b"not zscript".as_slice()),
            ("src/rel.zs", b"#include \"./deep/inner.zs\"\nclass Rel {}\n".as_slice()),
            ("src/deep/inner.zs", b"class Inner {}\n".as_slice()),
            ("src/example.zs", b"this is not even zscript\n".as_slice()),
            ("src/old.zc", b"class Old {}\n".as_slice()),
        ])
        .unwrap();
        let vfs = VfsBuilder::new()
            .mount_memory("mod.pk3", pk3)
            .unwrap()
            .build()
            .unwrap();
        let (sources, _) = zscript_sources_from_vfs(&vfs);
        let names: Vec<&str> = sources.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(names[0], "zscript.txt");
        assert!(names.contains(&"src/handlers.txt"), "{names:?}");
        assert!(names.contains(&"actors/imp.txt"), "{names:?}");
        assert!(!names.contains(&"src/unused.txt"));
        assert!(names.contains(&"src/deep/inner.zs"), "{names:?}");
        // Not included (or only in a comment): GZDoom never compiles them.
        assert!(!names.contains(&"src/example.zs"), "{names:?}");
        assert!(!names.contains(&"src/old.zc"), "{names:?}");
        let res = room_zscript::check(&sources);
        assert!(
            res.ok(),
            "{:?}",
            res.diagnostics
                .iter()
                .map(|d| d.format_line())
                .collect::<Vec<_>>()
        );
    }
}

/// Every ACS library lump (`acs/` namespace) by short name, last mount winning,
/// for [`crate::Headless::set_acs_library_pool`] (GZDoom `ns_acslibrary`).
#[must_use]
pub fn acs_library_pool(vfs: &Vfs) -> Vec<(String, Vec<u8>)> {
    let mut pool: Vec<(String, Vec<u8>)> = Vec::new();
    for (id, info) in vfs.iter() {
        if info.namespace != room_vfs::Namespace::Acs || info.short_name.is_empty() {
            continue;
        }
        let name = info.short_name.as_str().to_ascii_uppercase();
        let Ok(bytes) = vfs.read(id) else {
            continue;
        };
        pool.retain(|(n, _)| *n != name);
        pool.push((name, bytes.to_vec()));
    }
    pool
}

#[cfg(test)]
mod status_bar_tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn status_bar_int_array_field_push() {
        let zs = b"version \"4.10\"\nclass MyBar2 : BaseStatusBar\n{\n    Array<int> cols;\n    int n;\n    override void Init()\n    {\n        Super.Init();\n        cols.Push(Font.FindFontColor(\"Red\"));\n        n = cols.Size();\n        names.Push(\"x\");\n    }\n}\nextend class MyBar2\n{\n    Array<String> names;\n    String title;\n    Array<int> more;\n}\n";
        let pk3 = room_vfs::zip_from_files(&[("zscript.zs", zs.as_slice())]).unwrap();
        let vfs = room_vfs::VfsBuilder::new()
            .mount_memory("bar.pk3", pk3)
            .unwrap()
            .build()
            .unwrap();
        let prog = load_status_bar_program(&vfs).unwrap();
        let mut bar = room_ui::ScriptStatusBar::new(prog, "MyBar2").unwrap();
        let snap = room_ui::SbarSnapshot::default();
        let dims = |_: &str| None;
        let mut list = room_ui::UiDrawList::default();
        bar.draw(&snap, false, 1.0, &dims, &mut list);
        assert!(bar.last_error.is_none(), "{:?}", bar.last_error);
    }

    #[test]
    fn mod_status_bar_compiles_over_the_uzdoom_library() {
        let zs = b"version \"4.10\"\nclass MyBar : DoomStatusBar\n{\n    override void Draw(int state, double ticfrac)\n    {\n        Super.Draw(state, ticfrac);\n        DrawString(mHUDFont, FormatNumber(CPlayer.health, 3), (160, 100));\n    }\n}\n";
        let pk3 = room_vfs::zip_from_files(&[("zscript.zs", zs)]).unwrap();
        let vfs = room_vfs::VfsBuilder::new()
            .mount_memory("bar.pk3", pk3)
            .unwrap()
            .build()
            .unwrap();
        let prog = load_status_bar_program(&vfs).unwrap();
        let mut bar = room_ui::ScriptStatusBar::new(prog, "MyBar").unwrap();
        assert_eq!(bar.class().to_ascii_lowercase(), "mybar");
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
        let at_100 = list.cmds.iter().any(|c| {
            matches!(c, room_ui::DrawCmd::Patch { name, pos, .. }
                if name.starts_with("STTNUM") && (pos.y - 100.0).abs() < 20.0)
        });
        assert!(at_100, "mod's own DrawString ran: {:?}", list.cmds.len());
    }
}

//! Collect / compile ZScript from a mounted VFS for play sessions (`docs/34`).

use room_vfs::Vfs;
use room_zvm::CompiledProgram;

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
}

/// Collect `.zs` / `.zc` / `.zsc` / `ZSCRIPT` lumps from `vfs`, compile them, and
/// return a report. Empty VFS → `ok` with no program.
#[must_use]
pub fn load_zscript_from_vfs(vfs: &Vfs) -> ZscriptLoadReport {
    let (sources, skipped) = zscript_sources_from_vfs(vfs);
    compile_report(sources, skipped)
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
    let (mut sources, skipped) = collect_zscript_sources(vfs);
    sort_entry_first(&mut sources);
    (sources, skipped)
}

fn sort_entry_first(sources: &mut [(String, String)]) {
    sources.sort_by(|a, b| {
        let score = |n: &str| if is_zscript_root(n) { 0 } else { 1 };
        score(&a.0).cmp(&score(&b.0)).then_with(|| a.0.cmp(&b.0))
    });
}

fn compile_report(sources: Vec<(String, String)>, skipped: Vec<String>) -> ZscriptLoadReport {
    let files: Vec<String> = sources.iter().map(|(p, _)| p.clone()).collect();
    if sources.is_empty() {
        return ZscriptLoadReport {
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
            files,
            program: prog,
            ok: true,
            notes,
            skipped,
        }
    } else {
        notes.insert(0, format!("compile failed ({} file(s))", files.len()));
        ZscriptLoadReport {
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

fn read_text(vfs: &Vfs, id: room_vfs::LumpId, path: &str, skipped: &mut Vec<String>) -> Option<String> {
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
            continue;
        }
        by_path.insert(key, sources.len());
        sources.push((path, text));
    }
    // GZDoom includes any file by path, whatever its extension: mods keep
    // ZScript in `actors/foo.txt` or `src/x.txt` and pull it in from the root.
    let mut next = 0;
    while next < sources.len() {
        for inc in include_paths(&sources[next].1) {
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
            }
        }
        next += 1;
    }
    (sources, skipped)
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

#[cfg(test)]
mod source_tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::{VfsBuilder, WadWriter, zip_from_files};

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
    fn includes_with_any_extension_and_txt_roots_are_loaded() {
        let pk3 = zip_from_files(&[
            ("zscript.txt", b"version \"4.10\"\n#include \"src/handlers.txt\"\n".as_slice()),
            ("src/handlers.txt", b"#include \"actors/imp.txt\"\nclass H : EventHandler {}\n".as_slice()),
            ("actors/imp.txt", b"class FastImp : Actor {}\n".as_slice()),
            ("src/unused.txt", b"not zscript".as_slice()),
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
        let res = room_zscript::check(&sources);
        assert!(res.ok(), "{:?}", res.diagnostics.iter().map(|d| d.format_line()).collect::<Vec<_>>());
    }
}

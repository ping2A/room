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
    let (mut sources, skipped) = collect_zscript_sources(vfs);
    sources.sort_by(|a, b| {
        let score = |n: &str| {
            let base = n.rsplit(['/', '\\']).next().unwrap_or(n);
            if base.eq_ignore_ascii_case("zscript.zc") || base.eq_ignore_ascii_case("zscript") {
                0
            } else {
                1
            }
        };
        score(&a.0).cmp(&score(&b.0)).then_with(|| a.0.cmp(&b.0))
    });
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

fn collect_zscript_sources(vfs: &Vfs) -> (Vec<(String, String)>, Vec<String>) {
    let mut sources = Vec::new();
    let mut skipped = Vec::new();
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
            || lower.ends_with("/zscript");
        if !is_zs {
            continue;
        }
        let bytes = match vfs.read(id) {
            Ok(b) => b,
            Err(e) => {
                skipped.push(format!("{path}: read failed: {e}"));
                continue;
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
            continue;
        }
        sources.push((path, text));
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

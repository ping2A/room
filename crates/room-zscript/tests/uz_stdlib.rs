//! UZ3: vendored UZDoom `zscript.txt` loads as one translation unit.

use std::fs;
use std::path::PathBuf;

fn uzdoom_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom")
}

fn load_tree() -> Vec<(String, String)> {
    let root = uzdoom_root();
    let entry = fs::read_to_string(root.join("zscript.txt")).expect("zscript.txt");
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("#include") else {
            continue;
        };
        let path = rest.trim().trim_matches('"');
        let text = fs::read_to_string(root.join(path))
            .unwrap_or_else(|err| panic!("missing include {path}: {err}"));
        sources.push((path.replace('\\', "/"), text));
    }
    sources
}

#[test]
fn uzdoom_zscript_txt_resolves_every_include() {
    let sources = load_tree();
    // zscript.txt plus every #include.
    assert!(sources.len() > 200, "includes: {}", sources.len());
    let result = room_zscript::check(&sources);
    let errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == room_zscript::Severity::Error)
        .collect();
    let mut buckets: Vec<(String, usize)> = Vec::new();
    for diag in &errors {
        let key = diag
            .message
            .split_once('\'')
            .map(|(head, _)| head.trim().to_owned())
            .unwrap_or_else(|| diag.message.clone());
        if let Some((_, n)) = buckets.iter_mut().find(|(k, _)| k == &key) {
            *n += 1;
        } else {
            buckets.push((key, 1));
        }
    }
    buckets.sort_by(|a, b| b.1.cmp(&a.1));
    for (key, n) in buckets.iter().take(6) {
        eprintln!("uz3 {n:5} {key}");
    }
    let sema: Vec<_> = errors.iter().filter(|d| d.path != "lower.zs").collect();
    eprintln!(
        "uz3 sema {} lower {}",
        sema.len(),
        errors.len() - sema.len()
    );
    assert!(
        sema.is_empty(),
        "type checker produced {} errors, first: {}",
        sema.len(),
        sema.first().map(|d| d.message.as_str()).unwrap_or("")
    );
    // The type checker is clean, so `check` reaches the lowerer.
    // Measured 2026-11-03: the vendored stdlib lowers with no errors.
    let lower = errors.len() - sema.len();
    assert!(lower <= 0, "lowerer produced {lower} errors (ceiling 0)");
    let prog = result.program.expect("lowered program");
    let mut table = room_zvm::NativeTable::with_builtins();
    let imports = prog.native_imports.len();
    let stubs = table.bind_imports(&prog.native_imports);
    let bound = imports - stubs;
    eprintln!("uz3 natives {imports} implemented {bound} stubs {stubs}");
    // UZ3 acceptance (docs/36): every import binds to a native — computed,
    // queued for the playsim, engine-managed, or a host native that traps
    // with the host's name — none is a silent return-zero stub.
    assert_eq!(stubs, 0, "return-zero stubs left");
    assert!(
        prog.native_imports
            .iter()
            .all(|name| table.id_ci(name).is_some()),
        "stdlib native imports must bind"
    );
}

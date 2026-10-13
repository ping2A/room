//! Validate WGSL shaders with naga at build time (docs/13 §shaders).

use std::path::PathBuf;

fn main() {
    let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let manifest = PathBuf::from(dir);
    let shader_dir = manifest.join("src/shaders");
    println!("cargo:rerun-if-changed=src/shaders");
    let Ok(rd) = std::fs::read_dir(&shader_dir) else {
        return;
    };
    let mut errors = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("wgsl") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("{}: read: {e}", path.display()));
                continue;
            }
        };
        match naga::front::wgsl::parse_str(&src) {
            Ok(module) => {
                let info = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::all(),
                )
                .validate(&module);
                if let Err(e) = info {
                    errors.push(format!("{}: validate: {e:#?}", path.display()));
                }
            }
            Err(e) => errors.push(format!("{}: parse: {e}", path.display())),
        }
    }
    if !errors.is_empty() {
        panic!("WGSL shader validation failed:\n{}", errors.join("\n"));
    }
}

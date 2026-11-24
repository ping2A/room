//! Command-line ACS compiler: `acsc SCRIPTS.acs [-I dir]... [-o BEHAVIOR.o]`.
//!
//! Compiles with Room's built-in compiler and checks the result loads.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = None;
    let mut output = None;
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" => output = args.next().map(PathBuf::from),
            "-I" => dirs.extend(args.next().map(PathBuf::from)),
            "-h" | "--help" => {
                println!("usage: acsc SCRIPTS.acs [-I include-dir]... [-o BEHAVIOR.o]");
                return ExitCode::SUCCESS;
            }
            _ => input = Some(PathBuf::from(a)),
        }
    }
    let Some(input) = input else {
        eprintln!("usage: acsc SCRIPTS.acs [-I include-dir]... [-o BEHAVIOR.o]");
        return ExitCode::FAILURE;
    };
    let source = match std::fs::read(&input) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => {
            eprintln!("{}: {e}", input.display());
            return ExitCode::FAILURE;
        }
    };
    if let Some(parent) = input.parent() {
        dirs.insert(0, parent.to_path_buf());
    }
    let mut include = |name: &str| -> Option<String> {
        dirs.iter()
            .find_map(|d| find_file(d, name))
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    let name = input.file_name().map_or_else(
        || "SCRIPTS".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let bytes = match room_acs::compile_acs(&source, &name, &mut include) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let module = match room_acs::load_behavior(&bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: compiled module doesn't load: {e}", input.display());
            return ExitCode::FAILURE;
        }
    };
    let output = output.unwrap_or_else(|| input.with_extension("o"));
    if let Err(e) = std::fs::write(&output, &bytes) {
        eprintln!("{}: {e}", output.display());
        return ExitCode::FAILURE;
    }
    println!(
        "{}: {} bytes, {} scripts, {} functions, {} strings",
        output.display(),
        bytes.len(),
        module.scripts.len(),
        module.functions.len(),
        module.string_offsets.len()
    );
    ExitCode::SUCCESS
}

/// `name` inside `dir`, matching letter case loosely (lumps are case-insensitive).
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let want = name.replace('\\', "/").to_ascii_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|f| f.to_string_lossy().to_ascii_lowercase() == want)
        })
}

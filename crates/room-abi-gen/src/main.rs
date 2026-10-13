//! Generates host glue and guest bindings from the scripting IDL.
//!
//! M11 bootstrap: parse `api/*.ron` and emit a Rust host-import catalogue.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct ApiFn {
    name: String,
    scope: String,
    params: Vec<(String, String)>,
    results: Vec<String>,
    doc: String,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("room-abi-gen: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        bail!("usage: room-abi-gen list <api.ron>…");
    };
    match cmd.as_str() {
        "list" => {
            let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
            if paths.is_empty() {
                bail!("usage: room-abi-gen list <api.ron>…");
            }
            for path in paths {
                list_ron(&path)?;
            }
            Ok(())
        }
        _ => bail!("unknown command {cmd:?} (supported: list)"),
    }
}

fn list_ron(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let fns: Vec<ApiFn> =
        ron::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    println!("// generated from {}", path.display());
    println!("pub const HOST_IMPORTS: &[(&str, &str)] = &[");
    for f in &fns {
        let sig = format_sig(f);
        println!(
            "    (\"{}\", \"{}\"), // {} — {}",
            f.name, f.scope, sig, f.doc
        );
    }
    println!("];");
    Ok(())
}

fn format_sig(f: &ApiFn) -> String {
    let params: Vec<String> = f.params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
    let rets = if f.results.is_empty() {
        "()".into()
    } else {
        f.results.join(", ")
    };
    format!("({}) -> {}", params.join(", "), rets)
}

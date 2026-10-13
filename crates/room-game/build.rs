//! Pack `wadsrc/room` into `OUT_DIR/room.pk3` for `include_bytes!`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::env;
use std::fs::{self, File};
use std::io::copy;
use std::path::{Path, PathBuf};
use std::process;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn main() {
    if let Err(e) = run() {
        eprintln!("room-game build.rs: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let src = manifest_dir.join("../../wadsrc/room");
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let out_pk3 = out_dir.join("room.pk3");

    println!("cargo:rerun-if-changed={}", src.display());
    if src.is_dir() {
        walk_rerun(&src);
    }

    if !src.is_dir() {
        fs::write(&out_pk3, [])?;
        return Ok(());
    }

    let file = File::create(&out_pk3)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    pack_dir(&mut zip, &src, Path::new(""), opts)?;
    zip.finish()?;
    Ok(())
}

fn walk_rerun(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            walk_rerun(&path);
        }
    }
}

fn pack_dir(
    zip: &mut ZipWriter<File>,
    root: &Path,
    rel: &Path,
    opts: SimpleFileOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = root.join(rel);
    let mut entries: Vec<_> = fs::read_dir(&dir)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "README.md" || name_str == ".DS_Store" {
            continue;
        }
        let child_rel = rel.join(&name);
        if path.is_dir() {
            pack_dir(zip, root, &child_rel, opts)?;
            continue;
        }
        let archive_name = child_rel.to_string_lossy().replace('\\', "/");
        zip.start_file(archive_name, opts)?;
        let mut f = File::open(&path)?;
        copy(&mut f, zip)?;
    }
    Ok(())
}

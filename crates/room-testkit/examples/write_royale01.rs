//! Write `mods/doom1-royale/maps/ROYALE01.wad` (run from repo root).
use room_testkit::royale01;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_dir = root.join("mods/doom1-royale/maps");
    std::fs::create_dir_all(&out_dir).expect("create maps dir");
    let path = out_dir.join("ROYALE01.wad");
    let wad = royale01()
        .build_udmf_wad("ROYALE01")
        .expect("build ROYALE01 wad");
    std::fs::write(&path, &wad).expect("write wad");
    eprintln!("wrote {} ({} bytes)", path.display(), wad.len());
}

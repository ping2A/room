//! PK3 helpers for the web editor (`editor.html`): map WADs stored under
//! `maps/` inside an archive, archive path overrides, and writing the working
//! archive back out as a PK3.
//!
//! Host-testable (no `web-sys`); the wasm bridge lives in `browser/editor.rs`.

use bytes::Bytes;
use room_vfs::{ContainerKind, Namespace, Vfs, VfsBuilder, zip_from_files};
use room_zscript::{Diagnostic, Severity};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Container name of the editor's archive-path overrides (`zscript/foo.zs` …).
pub(crate) const PATH_OVERRIDES: &str = "editor-overrides.pk3";

/// A `maps/NAME.wad` entry of a mounted archive, mounted as its own WAD right
/// after the archive so its map can be opened, edited and played.
#[derive(Clone, Debug)]
pub(crate) struct NestedWad {
    /// Container that holds the entry.
    pub owner: String,
    /// Mount name (`owner:maps/name.wad`).
    pub name: String,
    pub data: Bytes,
}

/// Every `maps/*.wad` entry of an archive container (PK3 / PK7 / directory).
pub(crate) fn map_wads(vfs: &Vfs) -> Vec<NestedWad> {
    let mut out = Vec::new();
    for (id, info) in vfs.iter() {
        if info.namespace != Namespace::Maps
            || !info.full_path.to_ascii_lowercase().ends_with(".wad")
        {
            continue;
        }
        let Some(c) = vfs.container(info.container) else {
            continue;
        };
        if matches!(c.kind, ContainerKind::Wad | ContainerKind::LooseFile) {
            continue;
        }
        let Ok(data) = vfs.read(id) else {
            continue;
        };
        if data.len() < 12 || !(data.starts_with(b"PWAD") || data.starts_with(b"IWAD")) {
            continue;
        }
        out.push(NestedWad {
            owner: c.name.clone(),
            name: format!("{}:{}", c.name, info.full_path),
            data,
        });
    }
    out
}

/// Mount the archive `name`, then each `maps/*.wad` of it (from a first
/// [`map_wads`] pass) right after it, so later archives still override them.
pub(crate) fn mount_with_nested(
    b: VfsBuilder,
    name: &str,
    data: Bytes,
    nested: &[NestedWad],
) -> Result<VfsBuilder, String> {
    let mut b = b
        .mount_memory(name.to_owned(), data)
        .map_err(|e| e.to_string())?;
    for n in nested.iter().filter(|n| n.owner == name) {
        b = b
            .mount_memory(n.name.clone(), n.data.clone())
            .map_err(|e| e.to_string())?;
    }
    Ok(b)
}

/// Normalised archive path: `/` separators, no leading `/`, lowercase.
/// Rejects empty paths, `..` segments and paths over 255 bytes.
pub(crate) fn normalize_path(path: &str) -> Result<String, String> {
    let p = path.trim().replace('\\', "/");
    let p = p.trim_start_matches('/').to_ascii_lowercase();
    if p.is_empty() || p.ends_with('/') {
        return Err(format!("not a file path: {path:?}"));
    }
    if p.len() > 255 {
        return Err("path is longer than 255 bytes".into());
    }
    if p.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return Err(format!("invalid path: {path:?}"));
    }
    Ok(p)
}

/// Root file added when every entry sits in one top-level folder: GZDoom (and
/// Room) strip such a lone root folder, which would move `zscript/foo.zs` to
/// `foo.zs`.
const ROOT_NOTE: (&str, &[u8]) = ("room-editor.txt", b"Written by the Room editor.\n");

/// Zip `files`, keeping paths as given (see [`ROOT_NOTE`]).
fn zip_keep_paths(mut files: Vec<(&str, &[u8])>) -> Result<Vec<u8>, String> {
    let top = |p: &str| p.split_once('/').map(|(t, _)| t.to_ascii_lowercase());
    let first = files.first().and_then(|(p, _)| top(p));
    if first.is_some() && files.iter().all(|(p, _)| top(p) == first) {
        files.push(ROOT_NOTE);
    }
    zip_from_files(&files).map_err(|e| e.to_string())
}

/// The archive overrides as a PK3 (stored order = path order).
pub(crate) fn overrides_pk3(paths: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    zip_keep_paths(
        paths
            .iter()
            .map(|(p, d)| (p.as_str(), d.as_slice()))
            .collect(),
    )
}

/// Entries of the archive container `name` (in archive order), by full path.
pub(crate) fn archive_entries(vfs: &Vfs, name: &str) -> Vec<(String, Bytes)> {
    let mut out = Vec::new();
    for (id, info) in vfs.iter() {
        let Some(c) = vfs.container(info.container) else {
            continue;
        };
        if c.name != name || matches!(c.kind, ContainerKind::Wad | ContainerKind::LooseFile) {
            continue;
        }
        if info.full_path.is_empty() || info.full_path.ends_with('/') {
            continue;
        }
        if let Ok(data) = vfs.read(id) {
            out.push((info.full_path.clone(), data));
        }
    }
    out
}

/// Folder that holds lumps of an editor override namespace inside a PK3.
fn ns_folder(ns: &str) -> &'static str {
    match ns {
        "flats" => "flats/",
        "sprites" => "sprites/",
        "patches" => "patches/",
        "textures" => "textures/",
        "acs" => "acs/",
        _ => "",
    }
}

/// Ordered file list with case-insensitive path replacement.
#[derive(Default)]
struct Files {
    list: Vec<(String, Vec<u8>)>,
    index: BTreeMap<String, usize>,
}

impl Files {
    fn put(&mut self, path: &str, data: Vec<u8>) {
        let key = path.to_ascii_lowercase();
        if let Some(&i) = self.index.get(&key) {
            if let Some(slot) = self.list.get_mut(i) {
                slot.1 = data;
            }
            return;
        }
        self.index.insert(key, self.list.len());
        self.list.push((path.to_owned(), data));
    }

    /// Replace the file in `folder` (no deeper) whose stem is `name` (8-char
    /// short-name match), else add `folder + name`.
    fn put_lump(&mut self, folder: &str, name: &str, data: Vec<u8>) {
        let hit = self.list.iter().position(|(p, _)| {
            let lower = p.to_ascii_lowercase();
            let Some(rest) = lower.strip_prefix(folder) else {
                return false;
            };
            !rest.contains('/')
                && rest
                    .split('.')
                    .next()
                    .is_some_and(|stem| stem.eq_ignore_ascii_case(name))
        });
        match hit {
            Some(i) => {
                if let Some(slot) = self.list.get_mut(i) {
                    slot.1 = data;
                }
            }
            None => {
                let ext = if data.starts_with(b"\x89PNG") {
                    ".png"
                } else {
                    ""
                };
                let path = format!("{folder}{}{ext}", name.to_ascii_lowercase());
                self.put(&path, data);
            }
        }
    }
}

/// What goes into an exported PK3.
#[derive(Default)]
pub(crate) struct Pk3Export {
    /// Working archive entries (empty = a brand-new PK3).
    pub base: Vec<(String, Bytes)>,
    /// Archive path overrides (`zscript/foo.zs` → bytes).
    pub paths: BTreeMap<String, Vec<u8>>,
    /// Short-name lump overrides: namespace → name → bytes.
    pub lumps: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    /// Edited maps: name → map PWAD bytes, written as `maps/<name>.wad`.
    pub maps: Vec<(String, Vec<u8>)>,
    /// DECORATE generated from actor value edits (`decorate.room-edits`).
    pub actor_decorate: Option<String>,
}

/// Build the PK3: the working archive with every override applied.
pub(crate) fn build_pk3(ex: &Pk3Export) -> Result<Vec<u8>, String> {
    let mut files = Files::default();
    for (path, data) in &ex.base {
        files.put(path, data.to_vec());
    }
    for (ns, lumps) in &ex.lumps {
        for (name, data) in lumps {
            files.put_lump(ns_folder(ns), name, data.clone());
        }
    }
    for (path, data) in &ex.paths {
        files.put(path, data.clone());
    }
    for (name, wad) in &ex.maps {
        files.put(
            &format!("maps/{}.wad", name.to_ascii_lowercase()),
            wad.clone(),
        );
    }
    if let Some(text) = &ex.actor_decorate {
        // Short name DECORATE: loaded after the archive's own DECORATE.
        files.put("decorate.room-edits", text.clone().into_bytes());
    }
    zip_keep_paths(
        files
            .list
            .iter()
            .map(|(p, d)| (p.as_str(), d.as_slice()))
            .collect(),
    )
}

fn diag_json(d: &Diagnostic, exact: bool) -> Value {
    json!({
        "severity": match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        },
        "message": d.message,
        "path": d.path,
        "line": d.span.line.max(1),
        "col": d.span.col.max(1),
        "exact": exact,
        "text": d.format_line(),
    })
}

/// Check ZScript `text` (the file at `path`) for the editor.
///
/// 1. The file alone (its `#include` lines blanked so line numbers match):
///    syntax errors with exact lines (`scope: "file"`).
/// 2. If that parses, the whole program in `vfs` with `text` in place of the
///    file (`scope: "program"`); lines there may be relative to the
///    `#include`-expanded unit, so they are reported as text only.
pub(crate) fn check_zscript(vfs: &Vfs, path: &str, text: &str) -> Value {
    let name = if path.is_empty() { "zscript" } else { path };
    let local: String = text
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("#include") {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let own = room_zscript::parse_file(name, &local);
    let errors: Vec<Value> = own
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| diag_json(d, true))
        .collect();
    if !errors.is_empty() {
        return json!({ "scope": "file", "ok": false, "files": 1, "diagnostics": errors });
    }
    let (mut sources, _) = room_game::zscript_sources_from_vfs(vfs);
    let key = name.to_ascii_lowercase();
    match sources
        .iter_mut()
        .find(|(p, _)| p.to_ascii_lowercase() == key)
    {
        Some(slot) => slot.1 = text.to_owned(),
        None => sources.push((name.to_owned(), text.to_owned())),
    }
    let res = room_zscript::check(&sources);
    let diagnostics: Vec<Value> = res
        .diagnostics
        .iter()
        .take(200)
        .map(|d| diag_json(d, false))
        .collect();
    json!({
        "scope": "program",
        "ok": res.ok(),
        "files": sources.len(),
        "diagnostics": diagnostics,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use room_vfs::WadWriter;

    fn map_wad(name: &str) -> Vec<u8> {
        WadWriter::pwad()
            .marker(name)
            .add("TEXTMAP", b"namespace = \"zdoom\";".to_vec())
            .marker("ENDMAP")
            .finish()
            .unwrap()
    }

    fn mount(name: &str, data: Vec<u8>) -> Vfs {
        VfsBuilder::new()
            .mount_memory(name, data)
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn finds_map_wads_in_pk3() {
        let wad = map_wad("MYMAP");
        let pk3 = zip_from_files(&[
            ("zscript.zs", b"version \"4.10\"".as_slice()),
            ("maps/mymap.wad", wad.as_slice()),
            ("maps/readme.txt", b"hi".as_slice()),
        ])
        .unwrap();
        let vfs = mount("mod.pk3", pk3);
        let nested = map_wads(&vfs);
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].owner, "mod.pk3");
        assert_eq!(nested[0].name, "mod.pk3:maps/mymap.wad");
        assert_eq!(nested[0].data.as_ref(), wad.as_slice());
        // Mounted after its archive, the map marker is found by name.
        let vfs = VfsBuilder::new()
            .mount_memory(
                "mod.pk3",
                zip_from_files(&[("maps/mymap.wad", wad.as_slice())]).unwrap(),
            )
            .unwrap()
            .mount_memory(nested[0].name.clone(), nested[0].data.clone())
            .unwrap()
            .build()
            .unwrap();
        assert!(vfs.find("MYMAP", Namespace::Global).is_some());
    }

    #[test]
    fn plain_wad_has_no_map_wads() {
        let vfs = mount("map.wad", map_wad("MAP01"));
        assert!(map_wads(&vfs).is_empty());
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(
            normalize_path("/ZScript\\Foo.zs").unwrap(),
            "zscript/foo.zs"
        );
        assert!(normalize_path("").is_err());
        assert!(normalize_path("a/../b").is_err());
        assert!(normalize_path("dir/").is_err());
        assert!(normalize_path("a//b").is_err());
    }

    #[test]
    fn path_overrides_shadow_archive_files() {
        let base = zip_from_files(&[("zscript/foo.zs", b"class A {}".as_slice())]).unwrap();
        let mut paths = BTreeMap::new();
        paths.insert("zscript/foo.zs".to_owned(), b"class B {}".to_vec());
        let vfs = VfsBuilder::new()
            .mount_memory("mod.pk3", base)
            .unwrap()
            .mount_memory(PATH_OVERRIDES, overrides_pk3(&paths).unwrap())
            .unwrap()
            .build()
            .unwrap();
        let id = vfs.find_full("zscript/foo.zs").unwrap();
        assert_eq!(vfs.read(id).unwrap().as_ref(), b"class B {}");
        assert_eq!(
            vfs.container(vfs.lump(id).unwrap().container).unwrap().name,
            PATH_OVERRIDES
        );
    }

    #[test]
    fn single_folder_export_keeps_its_folder() {
        let ex = Pk3Export {
            paths: [("zscript/a.zs".to_owned(), b"class A {}".to_vec())].into(),
            ..Pk3Export::default()
        };
        let out = mount("out.pk3", build_pk3(&ex).unwrap());
        assert!(out.find_full("zscript/a.zs").is_some());
    }

    #[test]
    fn exports_working_archive_with_overrides() {
        let base = zip_from_files(&[
            ("zscript.zs", b"#include \"zscript/foo.zs\"".as_slice()),
            ("zscript/foo.zs", b"class A {}".as_slice()),
            ("decorate.txt", b"actor X {}".as_slice()),
            ("sprites/possa1.png", b"\x89PNGold".as_slice()),
            ("maps/map01.wad", map_wad("MAP01").as_slice()),
        ])
        .unwrap();
        let vfs = mount("mod.pk3", base);
        let mut ex = Pk3Export {
            base: archive_entries(&vfs, "mod.pk3"),
            ..Pk3Export::default()
        };
        assert_eq!(ex.base.len(), 5);
        ex.paths
            .insert("zscript/foo.zs".into(), b"class B {}".to_vec());
        ex.paths
            .insert("zscript/new.zs".into(), b"class C {}".to_vec());
        ex.lumps
            .entry("sprites".into())
            .or_default()
            .insert("POSSA1".into(), b"\x89PNGnew".to_vec());
        ex.lumps
            .entry("global".into())
            .or_default()
            .insert("MAPINFO".into(), b"map MAP01 \"x\" {}".to_vec());
        ex.maps.push(("MAP01".into(), map_wad("MAP01")));
        ex.maps.push(("MAP02".into(), map_wad("MAP02")));
        ex.actor_decorate = Some("actor Y {}".into());
        let out = mount("out.pk3", build_pk3(&ex).unwrap());
        let read = |p: &str| out.read(out.find_full(p).unwrap()).unwrap().to_vec();
        assert_eq!(read("zscript/foo.zs"), b"class B {}");
        assert_eq!(read("zscript/new.zs"), b"class C {}");
        assert_eq!(read("zscript.zs"), b"#include \"zscript/foo.zs\"");
        assert_eq!(read("decorate.txt"), b"actor X {}");
        assert_eq!(read("sprites/possa1.png"), b"\x89PNGnew");
        assert_eq!(read("mapinfo"), b"map MAP01 \"x\" {}");
        assert_eq!(read("decorate.room-edits"), b"actor Y {}");
        assert!(out.find_full("maps/map02.wad").is_some());
        // Replaced in place, not duplicated.
        let n = archive_entries(&out, "out.pk3").len();
        assert_eq!(n, 9);
        assert_eq!(map_wads(&out).len(), 2);
    }

    #[test]
    fn zscript_syntax_error_has_exact_line() {
        let vfs = mount(
            "mod.pk3",
            zip_from_files(&[("readme.txt", b"x".as_slice())]).unwrap(),
        );
        let src = "version \"4.10\"\n#include \"zscript/other.zs\"\nclass Foo : Actor\n{\n  int x = ;\n}\n";
        let v = check_zscript(&vfs, "zscript/foo.zs", src);
        assert_eq!(v["scope"], "file");
        assert_eq!(v["ok"], false);
        let d = &v["diagnostics"][0];
        assert_eq!(d["exact"], true);
        assert_eq!(d["line"], 5, "{v}");
    }

    #[test]
    fn zscript_valid_file_checks_program() {
        let vfs = mount(
            "mod.pk3",
            zip_from_files(&[("readme.txt", b"x".as_slice())]).unwrap(),
        );
        let src = "class Foo : Actor\n{\n  Default\n  {\n    Health 50;\n    +SOLID\n  }\n  States\n  {\n  Spawn:\n    POSS A 10;\n    loop;\n  }\n}\n";
        let v = check_zscript(&vfs, "zscript/foo.zs", src);
        assert_eq!(v["scope"], "program", "{v}");
        assert!(v["files"].as_u64().unwrap() >= 1);
    }

    /// Mount like the editor does: archive, then its map WADs (two passes).
    fn mount_editor(name: &str, data: Vec<u8>, paths: &BTreeMap<String, Vec<u8>>) -> Vfs {
        let data = Bytes::from(data);
        let build = |nested: &[NestedWad]| {
            let mut b = mount_with_nested(
                VfsBuilder::new().nested_map_wads(false),
                name,
                data.clone(),
                nested,
            )
            .unwrap();
            if !paths.is_empty() {
                b = mount_with_nested(
                    b,
                    PATH_OVERRIDES,
                    overrides_pk3(paths).unwrap().into(),
                    nested,
                )
                .unwrap();
            }
            b.build().unwrap()
        };
        let first = build(&[]);
        let nested = map_wads(&first);
        if nested.is_empty() {
            first
        } else {
            build(&nested)
        }
    }

    /// Deterministic, poorly compressible bytes.
    fn noise(n: usize) -> Vec<u8> {
        let mut x: u32 = 0x1234_5678;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })
            .collect()
    }

    /// Imported PK3 wrapped in one top-level folder (as zipped by many
    /// tools), with a root zscript.txt that #includes, a map in
    /// maps/*.wad and a large file: open → edit → Export PK3 → re-open.
    #[test]
    fn import_wrapped_pk3_round_trips() {
        let big = noise(12 << 20);
        let root = "version \"4.10\"\n#include \"zscript/monsters.zs\"\n";
        let monsters = "class FastImp : Actor\n{\n  Default\n  {\n    Health 90;\n    +SOLID\n  }\n  States\n  {\n  Spawn:\n    TROO A 10;\n    Loop;\n  }\n}\n";
        let pk3 = zip_from_files(&[
            ("MyMod/zscript.txt", root.as_bytes()),
            ("MyMod/zscript/monsters.zs", monsters.as_bytes()),
            ("MyMod/maps/e1m1.wad", map_wad("E1M1").as_slice()),
            ("MyMod/music/big.ogg", big.as_slice()),
        ])
        .unwrap();
        let mut paths = BTreeMap::new();
        let vfs = mount_editor("mymod.pk3", pk3.clone(), &paths);
        // Wrapper folder stripped; the map inside maps/ is mounted and found.
        assert!(vfs.find_full("zscript/monsters.zs").is_some());
        assert!(vfs.find_full("MyMod/zscript.txt").is_none());
        let nested = map_wads(&vfs);
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].name, "mymod.pk3:maps/e1m1.wad");
        assert!(vfs.find("E1M1", Namespace::Global).is_some());
        // The root ZSCRIPT pulls the included file into the program.
        let (sources, _) = room_game::zscript_sources_from_vfs(&vfs);
        assert!(
            sources
                .iter()
                .any(|(p, _)| p.eq_ignore_ascii_case("zscript/monsters.zs")),
            "{sources:?}"
        );
        let v = check_zscript(&vfs, "zscript/monsters.zs", monsters);
        assert_eq!(v["scope"], "program", "{v}");
        // Edit the included file by path, as roomEditorSetPath does.
        let edited = monsters.replace("Health 90", "Health 45");
        paths.insert(
            normalize_path("ZScript/Monsters.zs").unwrap(),
            edited.clone().into_bytes(),
        );
        let vfs = mount_editor("mymod.pk3", pk3, &paths);
        let id = vfs.find_full("zscript/monsters.zs").unwrap();
        assert_eq!(vfs.read(id).unwrap().as_ref(), edited.as_bytes());
        assert!(vfs.find("E1M1", Namespace::Global).is_some());
        // Export PK3 of the working archive, then import that.
        let ex = Pk3Export {
            base: archive_entries(&vfs, "mymod.pk3"),
            paths: paths.clone(),
            maps: vec![("E1M1".into(), map_wad("E1M1"))],
            ..Pk3Export::default()
        };
        assert_eq!(ex.base.len(), 4);
        let out = build_pk3(&ex).unwrap();
        let back = mount_editor("mymod-room-edit.pk3", out, &BTreeMap::new());
        let read = |p: &str| back.read(back.find_full(p).unwrap()).unwrap().to_vec();
        assert_eq!(read("zscript/monsters.zs"), edited.as_bytes());
        assert_eq!(read("zscript.txt"), root.as_bytes());
        assert_eq!(read("music/big.ogg"), big);
        assert_eq!(archive_entries(&back, "mymod-room-edit.pk3").len(), 4);
        assert!(back.find("E1M1", Namespace::Global).is_some());
        let (sources, _) = room_game::zscript_sources_from_vfs(&back);
        assert!(sources.iter().any(|(_, t)| t.contains("Health 45")));
    }

    /// A PK3 with only a root zscript.zs and nothing else round-trips with a
    /// brand-new file added in a folder.
    #[test]
    fn root_zscript_zs_and_new_file() {
        let pk3 = zip_from_files(&[(
            "zscript.zs",
            b"version \"4.10\"\n#include \"zscript/new.zs\"\n".as_slice(),
        )])
        .unwrap();
        let mut paths = BTreeMap::new();
        paths.insert(
            "zscript/new.zs".to_owned(),
            b"class NewThing : Actor {}\n".to_vec(),
        );
        let vfs = mount_editor("solo.pk3", pk3, &paths);
        // A lone root file is not a wrapper folder: it stays mounted.
        assert!(vfs.find_full("zscript.zs").is_some());
        let (sources, _) = room_game::zscript_sources_from_vfs(&vfs);
        assert!(
            sources
                .iter()
                .any(|(p, _)| p.eq_ignore_ascii_case("zscript.zs")),
            "{sources:?}"
        );
        assert!(
            sources
                .iter()
                .any(|(p, _)| p.eq_ignore_ascii_case("zscript/new.zs")),
            "{sources:?}"
        );
        let ex = Pk3Export {
            base: archive_entries(&vfs, "solo.pk3"),
            paths,
            ..Pk3Export::default()
        };
        let back = mount_editor(
            "solo-room-edit.pk3",
            build_pk3(&ex).unwrap(),
            &BTreeMap::new(),
        );
        assert!(back.find_full("zscript.zs").is_some());
        assert!(back.find_full("zscript/new.zs").is_some());
    }
}

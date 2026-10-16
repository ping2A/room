//! WASM mod discovery: `room.toml` (PK3 / directory root) or `ROOMMOD` (WAD lump).

use room_script::manifest::Manifest;
use room_sim::script_mods::ModSource;
use room_vfs::{LumpId, Vfs};

/// WAD lump holding a mod manifest (same syntax as `room.toml`).
pub const MANIFEST_LUMP: &str = "ROOMMOD";

fn is_manifest(vfs: &Vfs, id: LumpId) -> bool {
    vfs.lump(id).is_some_and(|l| {
        l.full_path.eq_ignore_ascii_case("room.toml")
            || l.short_name.as_str().eq_ignore_ascii_case(MANIFEST_LUMP)
    })
}

/// Module file in the manifest's own container (path, then lump name), then anywhere.
fn resolve_module(vfs: &Vfs, manifest: LumpId, file: &str) -> Option<LumpId> {
    let container = vfs.lump(manifest)?.container;
    let path = file.replace('\\', "/");
    vfs.iter()
        .filter(|(_, l)| l.container == container)
        .filter(|(_, l)| {
            l.full_path.eq_ignore_ascii_case(&path)
                || l.short_name.as_str().eq_ignore_ascii_case(&path)
        })
        .map(|(id, _)| id)
        .last()
        .or_else(|| vfs.find_full(&path))
        .or_else(|| (path.len() <= 8 && !path.contains('/')).then(|| vfs.find_any_ns(&path))?)
}

/// Every play/ui module declared by mounted manifests, in mount order. A later
/// manifest with the same mod `id` replaces the earlier one (PWAD override).
/// Problems are reported as messages; a broken mod never stops the others.
#[must_use]
pub fn discover_mods(vfs: &Vfs) -> (Vec<ModSource>, Vec<String>) {
    let mut mods: Vec<(String, Vec<ModSource>)> = Vec::new();
    let mut errors = Vec::new();
    for id in vfs.iter_lumps().filter(|&id| is_manifest(vfs, id)) {
        let Ok(bytes) = vfs.read(id) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let manifest = match Manifest::parse(&text) {
            Ok(m) => m,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        let mod_id = if manifest.id.is_empty() {
            format!("lump{}", id.0)
        } else {
            manifest.id.clone()
        };
        let mut out = Vec::new();
        for m in &manifest.modules {
            let Some(lump) = resolve_module(vfs, id, &m.file) else {
                errors.push(format!("{mod_id}: module file {} not found", m.file));
                continue;
            };
            match vfs.read(lump) {
                Ok(bytes) => out.push(ModSource {
                    mod_id: mod_id.clone(),
                    name: m.name.clone(),
                    scope: m.scope,
                    bytes: bytes.to_vec(),
                }),
                Err(e) => errors.push(format!("{mod_id}: {}: {e}", m.file)),
            }
        }
        match mods.iter_mut().find(|(k, _)| *k == mod_id) {
            Some(slot) => slot.1 = out,
            None => mods.push((mod_id, out)),
        }
    }
    (mods.into_iter().flat_map(|(_, v)| v).collect(), errors)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use bytes::Bytes;
    use room_vfs::{VfsBuilder, WadWriter};

    fn wad(lumps: &[(&str, &[u8])]) -> Bytes {
        let mut w = WadWriter::pwad();
        for (n, d) in lumps {
            w.add(n, *d);
        }
        Bytes::from(w.finish().unwrap())
    }

    #[test]
    fn wad_manifest_resolves_lumps_and_later_wad_overrides() {
        let a = wad(&[
            (
                "ROOMMOD",
                b"[mod]\nid = \"m\"\n[[module]]\nfile = \"DIRECT\"\nname = \"old\"\n",
            ),
            ("DIRECT", b"aaa"),
        ]);
        let b = wad(&[
            (
                "ROOMMOD",
                b"[mod]\nid = \"m\"\n[[module]]\nfile = \"DIRECT\"\nname = \"new\"\n",
            ),
            ("DIRECT", b"bbb"),
        ]);
        let c = wad(&[("ROOMMOD", b"[[module]]\nfile = \"NOPE\"\n")]);
        let vfs = VfsBuilder::new()
            .mount_memory("a.wad", a)
            .unwrap()
            .mount_memory("b.wad", b)
            .unwrap()
            .mount_memory("c.wad", c)
            .unwrap()
            .build()
            .unwrap();
        let (mods, errors) = discover_mods(&vfs);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "new");
        assert_eq!(mods[0].bytes, b"bbb");
        assert_eq!(errors.len(), 1, "{errors:?}");
    }
}

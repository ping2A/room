//! Editor ACS: compile `SCRIPTS` source with `room-acs`'s compiler, and give
//! the current map the result as its `BEHAVIOR` (play-testing restarts the
//! level so the new scripts run).

use super::{start_session, with_editor, with_play};
use room_vfs::Vfs;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeSet;
use wasm_bindgen::prelude::*;

thread_local! {
    /// Maps whose `BEHAVIOR` / `SCRIPTS` came from the editor's compiler; reloading
    /// such a map keeps them instead of re-reading the game's copy.
    static EDITED: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

/// Whether `map` has scripts compiled in the editor.
pub(super) fn edited(map: &str) -> bool {
    EDITED.with(|e| e.borrow().contains(&map.to_ascii_uppercase()))
}

/// Forget compiled scripts (a new game was opened).
pub(super) fn clear() {
    EDITED.with(|e| e.borrow_mut().clear());
}

/// Starter source for a map that has no scripts yet.
const STARTER: &str =
    "#include \"zcommon.acs\"\n\nscript 1 OPEN\n{\n    Print(s:\"Hello from ACS\");\n}\n";

/// Text of an `#include`d file from the loaded game: full path, then under
/// `acs/` or `acs_source/`, then the 8-character lump name.
fn include_text(vfs: &Vfs, name: &str) -> Option<String> {
    let path = name.replace('\\', "/");
    let id = vfs
        .find_full(&path)
        .or_else(|| vfs.find_full(&format!("acs/{path}")))
        .or_else(|| vfs.find_full(&format!("acs_source/{path}")))
        .or_else(|| {
            let stem = path.rsplit('/').next()?.split('.').next()?;
            vfs.find_any_ns(&stem[..stem.len().min(8)])
        })?;
    let bytes = vfs.read(id).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Compile with includes from `vfs`: `(bytes, summary JSON)`.
fn compile(vfs: &Vfs, name: &str, source: &str) -> (Option<Vec<u8>>, Value) {
    let mut include = |inc: &str| include_text(vfs, inc);
    match room_acs::compile_acs_module(source, name, &mut include) {
        Ok(compiled) => match room_acs::load_behavior(&compiled.bytes) {
            Ok(m) => {
                let summary = json!({
                    "ok": true,
                    "bytes": compiled.bytes.len(),
                    "scripts": m.scripts.len(),
                    "functions": m.functions.iter().filter(|f| f.address != 0).count(),
                    "strings": m.string_offsets.len(),
                    "library": compiled.library,
                    "imports": compiled.imports,
                });
                (Some(compiled.bytes), summary)
            }
            Err(e) => (
                None,
                json!({ "ok": false, "error": { "file": name, "line": 0, "message": format!("compiled module doesn't load: {e}") } }),
            ),
        },
        Err(e) => (
            None,
            json!({ "ok": false, "error": { "file": e.file, "line": e.line, "message": e.message } }),
        ),
    }
}

/// Compile ACS without applying it: `{ok, bytes, scripts, functions, strings}`
/// or `{ok: false, error: {file, line, message}}`.
#[wasm_bindgen(js_name = roomEditorCompileAcs)]
pub fn room_editor_compile_acs(name: &str, source: &str) -> Result<String, JsValue> {
    with_editor(|ed| Ok(compile(&ed.vfs, name, source).1.to_string()))
}

/// The current map's ACS: `{map, format, source, starter, compiled}`. `source`
/// is the map's `SCRIPTS` lump (empty when it has none; `starter` then holds a
/// template), `compiled` whether it has a `BEHAVIOR` with scripts.
#[wasm_bindgen(js_name = roomEditorMapAcs)]
pub fn room_editor_map_acs() -> Result<String, JsValue> {
    with_editor(|ed| {
        let Some(doc) = ed.doc.as_ref() else {
            return Err("open a map first".into());
        };
        let (behavior, extra) = ed.blobs.get(&doc.name).cloned().unwrap_or_default();
        let source = extra
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("SCRIPTS"))
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default();
        let scripts = behavior
            .as_deref()
            .and_then(|b| room_acs::load_behavior(b).ok())
            .map_or(0, |m| m.scripts.len());
        Ok(json!({
            "map": doc.name,
            "format": format!("{:?}", doc.format).to_ascii_lowercase(),
            "source": source,
            "starter": STARTER,
            "scripts": scripts,
        })
        .to_string())
    })
}

/// Compile `source` as `map`'s scripts. On success the map gets the result as
/// `BEHAVIOR` and keeps the source as its `SCRIPTS` lump (both are exported
/// with the map); when `map` is the open map the level restarts. On an error
/// nothing changes. Returns the same JSON as [`room_editor_compile_acs`].
#[wasm_bindgen(js_name = roomEditorSetMapAcs)]
pub fn room_editor_set_map_acs(map: &str, source: &str) -> Result<String, JsValue> {
    with_editor(|ed| {
        let name = map.to_ascii_uppercase();
        let (bytes, summary) = compile(&ed.vfs, "SCRIPTS", source);
        let Some(bytes) = bytes else {
            return Ok(summary.to_string());
        };
        let entry = ed.blobs.entry(name.clone()).or_default();
        entry.0 = Some(bytes.clone());
        entry.1.retain(|(n, _)| !n.eq_ignore_ascii_case("SCRIPTS"));
        entry.1.push(("SCRIPTS".into(), source.as_bytes().to_vec()));
        let extra = entry.1.clone();
        EDITED.with(|e| e.borrow_mut().insert(name.clone()));
        let current = ed
            .doc
            .as_ref()
            .is_some_and(|d| d.name.eq_ignore_ascii_case(&name));
        if !current {
            return Ok(summary.to_string());
        }
        if let Some(doc) = ed.doc.as_mut() {
            doc.behavior = Some(bytes.clone());
            doc.extra_lumps = extra;
        }
        if let Some(geom) = ed.geom.as_mut() {
            geom.behavior = Some(bytes);
        }
        if let Some(geom) = ed.geom.clone() {
            let cam = with_play(|p| p.current_eye()).flatten();
            start_session(ed, geom, cam)?;
        }
        Ok(summary.to_string())
    })
}

/// Names the compiler knows, for completion: `[[name, kind, min, max, number], …]`
/// (`min`/`max` are `-1` when any argument count is accepted; `number` is the
/// special number, CallFunc index or constant value).
#[wasm_bindgen(js_name = roomEditorAcsNames)]
#[must_use]
pub fn room_editor_acs_names() -> String {
    let list: Vec<Value> = room_acs::acs_known_names()
        .into_iter()
        .map(|k| {
            let (lo, hi) = k
                .args
                .map_or((-1, -1), |(a, b)| (i32::from(a), i32::from(b)));
            json!([k.name, k.kind, lo, hi, k.number])
        })
        .collect();
    Value::Array(list).to_string()
}

/// Compile a `#library` source and install it as the `acs` lump of its name
/// (`acs/NAME` in a PK3, between `A_START`/`A_END` in a WAD); with `autoload`
/// the name is added to the editor's `LOADACS`, so every map loads it. Call
/// `roomEditorApplyLumps` after. Returns the compile JSON (with `library`).
#[wasm_bindgen(js_name = roomEditorInstallAcsLibrary)]
pub fn room_editor_install_acs_library(
    name: &str,
    source: &str,
    autoload: bool,
) -> Result<String, JsValue> {
    with_editor(|ed| {
        let (bytes, summary) = compile(&ed.vfs, name, source);
        let Some(bytes) = bytes else {
            return Ok(summary.to_string());
        };
        let Some(lib) = summary["library"].as_str().map(str::to_ascii_uppercase) else {
            return Ok(json!({ "ok": false, "error": { "file": name, "line": 1,
                "message": "not a library: add #library \"NAME\" at the top" } })
            .to_string());
        };
        if lib.is_empty() || lib.len() > 8 {
            return Ok(json!({ "ok": false, "error": { "file": name, "line": 1,
                "message": format!("library name \"{lib}\" must be 1 to 8 characters (it is the lump name)") } })
            .to_string());
        }
        ed.overrides
            .entry("acs".into())
            .or_default()
            .insert(lib.clone(), bytes);
        if autoload {
            let global = ed.overrides.entry("global".into()).or_default();
            let mut text = global
                .get("LOADACS")
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            if !text
                .split_whitespace()
                .any(|w| w.eq_ignore_ascii_case(&lib))
            {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&lib);
                text.push('\n');
                global.insert("LOADACS".into(), text.into_bytes());
            }
        }
        Ok(summary.to_string())
    })
}

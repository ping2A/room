//! Editor "Scripts" panel: WASM mod modules (imported `.wasm` or WAT compiled
//! in the browser) kept as `ROOMMOD` + module lumps in the override PWAD, so
//! play tests and the exported PWAD load them like any other mod.

use super::{Editor, err, with_editor, with_play};
use room_script::ModuleScope;
use room_script::manifest::{Manifest, ManifestModule};
use room_sim::script_mods::{ModRuntime, ModSource};
use room_vfs::Vfs;
use serde_json::{Value, json};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

const OVERRIDES_CONTAINER: &str = "editor-overrides.wad";

/// One module of the editor's own mod.
#[derive(Clone)]
pub(super) struct ScriptModule {
    lump: String,
    name: String,
    wasm: Vec<u8>,
    source: Option<String>,
}

/// The mod the editor writes (`ROOMMOD` in the override PWAD).
pub(super) struct EditorMod {
    id: String,
    name: String,
    version: String,
    modules: Vec<ScriptModule>,
    /// Lumps currently written into `overrides["global"]`.
    written: Vec<String>,
}

impl Default for EditorMod {
    fn default() -> Self {
        Self {
            id: "editor.mod".into(),
            name: "Editor mod".into(),
            version: "0.1.0".into(),
            modules: Vec::new(),
            written: Vec::new(),
        }
    }
}

fn source_lump(lump: &str) -> String {
    format!("S_{}", &lump[..lump.len().min(6)])
}

/// `"wat"` for WebAssembly text (`(module` after `;;` comments), else `"as"`
/// (AssemblyScript).
fn source_lang(src: &str) -> &'static str {
    let code = src
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty() && !l.starts_with(";;"));
    if code.is_some_and(|l| l.starts_with("(module")) {
        "wat"
    } else {
        "as"
    }
}

fn lump_name(name: &str) -> Result<String, String> {
    let n = name.trim().to_ascii_uppercase();
    if n.is_empty() || n.len() > 8 || !n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(format!(
            "module lump must be 1–8 letters, digits or _: {name:?}"
        ));
    }
    if matches!(n.as_str(), "ROOMMOD" | "DECORATE" | "MAPINFO" | "SNDINFO") || n.starts_with("S_") {
        return Err(format!("{n} is reserved"));
    }
    Ok(n)
}

impl Editor {
    /// Rewrite the `ROOMMOD` / module / source override lumps.
    fn sync_script_lumps(&mut self) {
        let global = self.overrides.entry("global".into()).or_default();
        for l in self.script_mod.written.drain(..) {
            global.remove(&l);
        }
        if self.script_mod.modules.is_empty() {
            return;
        }
        let m = &self.script_mod;
        let manifest = Manifest {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            abi: room_script::ABI_VERSION,
            max_players: None,
            modules: m
                .modules
                .iter()
                .map(|s| ManifestModule {
                    file: s.lump.clone(),
                    scope: ModuleScope::Play,
                    name: s.name.clone(),
                    source: s.source.as_ref().map(|_| source_lump(&s.lump)),
                })
                .collect(),
        };
        let mut written = vec!["ROOMMOD".to_owned()];
        global.insert("ROOMMOD".into(), manifest.to_toml().into_bytes());
        for s in &m.modules {
            global.insert(s.lump.clone(), s.wasm.clone());
            written.push(s.lump.clone());
            if let Some(src) = &s.source {
                let l = source_lump(&s.lump);
                global.insert(l.clone(), src.clone().into_bytes());
                written.push(l);
            }
        }
        self.script_mod.written = written;
    }

    fn scripts_json(&self) -> Value {
        let m = &self.script_mod;
        let (mounted, errors) = room_game::discover_mods(&self.vfs);
        let mounted: Vec<Value> = mounted
            .iter()
            .filter(|s| s.mod_id != m.id)
            .map(|s| json!({"mod": s.mod_id, "name": s.name, "size": s.bytes.len()}))
            .collect();
        let loaded = with_play(|p| p.script_mod_names()).unwrap_or_default();
        json!({
            "id": m.id, "name": m.name, "version": m.version,
            "modules": m.modules.iter().map(|s| json!({
                "lump": s.lump, "name": s.name, "size": s.wasm.len(),
                "hasSource": s.source.is_some(),
                "lang": s.source.as_deref().map(source_lang),
            })).collect::<Vec<_>>(),
            "mounted": mounted,
            "errors": errors,
            "loaded": loaded,
        })
    }

    /// Compile + register + init the module against the current classes.
    fn validate_module(&self, name: &str, wasm: &[u8]) -> Result<Vec<String>, String> {
        let src = ModSource {
            mod_id: self.script_mod.id.clone(),
            name: name.to_owned(),
            scope: ModuleScope::Play,
            bytes: wasm.to_vec(),
        };
        let rt =
            ModRuntime::new(&[src], Arc::clone(&self.data.classes)).map_err(|e| e.to_string())?;
        rt.merge_classes(&self.data.classes, &self.data.states)
            .map_err(|e| e.to_string())?;
        Ok(rt.drain_log())
    }
}

/// Last manifest mounted from a PWAD (not the editor's own overrides).
fn pwad_manifest(vfs: &Vfs) -> Option<(Manifest, room_vfs::ContainerId)> {
    vfs.iter()
        .filter(|(_, l)| {
            l.short_name.as_str().eq_ignore_ascii_case("ROOMMOD")
                || l.full_path.eq_ignore_ascii_case("room.toml")
        })
        .filter(|(_, l)| {
            vfs.container(l.container)
                .is_some_and(|c| c.name != OVERRIDES_CONTAINER)
        })
        .filter_map(|(id, l)| {
            let text = vfs.read(id).ok()?;
            Some((
                Manifest::parse(&String::from_utf8_lossy(&text)).ok()?,
                l.container,
            ))
        })
        .last()
}

fn container_lump(vfs: &Vfs, c: room_vfs::ContainerId, file: &str) -> Option<Vec<u8>> {
    let id = vfs
        .iter()
        .filter(|(_, l)| l.container == c)
        .filter(|(_, l)| {
            l.full_path.eq_ignore_ascii_case(file)
                || l.short_name.as_str().eq_ignore_ascii_case(file)
        })
        .map(|(id, _)| id)
        .last()?;
    vfs.read(id).ok().map(|b| b.to_vec())
}

/// Scripts panel state: `{id, name, version, modules: [{lump, name, size,
/// hasSource}], mounted: [{mod, name, size}], errors, loaded}`.
#[wasm_bindgen(js_name = roomEditorScripts)]
pub fn room_editor_scripts() -> Result<String, JsValue> {
    with_editor(|ed| Ok(ed.scripts_json().to_string()))
}

/// Compile WebAssembly text to a module (errors carry line:column).
#[wasm_bindgen(js_name = roomEditorCompileWat)]
pub fn room_editor_compile_wat(text: &str) -> Result<Vec<u8>, JsValue> {
    wat::parse_str(text).map_err(err)
}

/// Add or replace module `lump` (validated: compiles, links, registers).
/// `source` keeps the WAT text for later editing. Returns the module's
/// registration log; call `roomEditorApplyLumps` to restart with it.
#[wasm_bindgen(js_name = roomEditorSetScript)]
pub fn room_editor_set_script(
    lump: &str,
    name: &str,
    module: Vec<u8>,
    source: Option<String>,
) -> Result<String, JsValue> {
    with_editor(|ed| {
        let lump = lump_name(lump)?;
        let name = if name.trim().is_empty() {
            lump.to_ascii_lowercase()
        } else {
            name.trim().to_owned()
        };
        let clash = ed
            .script_mod
            .modules
            .iter()
            .any(|m| m.lump != lump && source_lump(&m.lump) == source_lump(&lump));
        if clash && source.is_some() {
            return Err(format!(
                "another module's source lump is already {}",
                source_lump(&lump)
            ));
        }
        let log = ed.validate_module(&name, &module)?;
        let module = ScriptModule {
            lump: lump.clone(),
            name,
            wasm: module,
            source,
        };
        match ed.script_mod.modules.iter_mut().find(|m| m.lump == lump) {
            Some(m) => *m = module,
            None => ed.script_mod.modules.push(module),
        }
        ed.sync_script_lumps();
        Ok(json!({"log": log}).to_string())
    })
}

/// Swap the running play test's mods for the current set (PWAD mods + the
/// editor's scripts) without restarting the level. `{live: false}` when not
/// playing; otherwise `{live: true, loaded: [names]}`.
#[wasm_bindgen(js_name = roomEditorLiveReloadScripts)]
pub fn room_editor_live_reload_scripts() -> Result<String, JsValue> {
    with_editor(|ed| {
        if with_play(|_| ()).is_none() {
            return Ok(json!({"live": false}).to_string());
        }
        ed.rebuild()?;
        let (sources, errors) = room_game::discover_mods(&ed.vfs);
        if let Some(e) = errors.first() {
            return Err(e.clone());
        }
        let names = with_play(|p| p.game.hot_swap_script_mods(sources))
            .ok_or_else(|| "play test ended".to_owned())?
            .map_err(|e| e.to_string())?;
        Ok(json!({"live": true, "loaded": names}).to_string())
    })
}

/// Remove module `lump`.
#[wasm_bindgen(js_name = roomEditorRemoveScript)]
pub fn room_editor_remove_script(lump: &str) -> Result<(), JsValue> {
    with_editor(|ed| {
        ed.script_mod
            .modules
            .retain(|m| !m.lump.eq_ignore_ascii_case(lump));
        ed.sync_script_lumps();
        Ok(())
    })
}

/// Mod identity written to `ROOMMOD`.
#[wasm_bindgen(js_name = roomEditorSetModInfo)]
pub fn room_editor_set_mod_info(id: &str, name: &str, version: &str) -> Result<(), JsValue> {
    with_editor(|ed| {
        let id = id.trim();
        if id.is_empty() {
            return Err("mod id must not be empty".into());
        }
        ed.script_mod.id = id.to_owned();
        ed.script_mod.name = name.trim().to_owned();
        ed.script_mod.version = version.trim().to_owned();
        ed.sync_script_lumps();
        Ok(())
    })
}

/// Module bytes (download) and WAT source (`null` when imported as `.wasm`).
#[wasm_bindgen(js_name = roomEditorScriptGet)]
pub fn room_editor_script_get(lump: &str) -> Result<JsValue, JsValue> {
    with_editor(|ed| {
        let m = ed
            .script_mod
            .modules
            .iter()
            .find(|m| m.lump.eq_ignore_ascii_case(lump))
            .ok_or_else(|| format!("no module {lump}"))?;
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &o,
            &"wasm".into(),
            &js_sys::Uint8Array::from(&m.wasm[..]).into(),
        );
        let src = m.source.as_deref().map_or(JsValue::NULL, JsValue::from_str);
        let _ = js_sys::Reflect::set(&o, &"source".into(), &src);
        Ok(o.into())
    })
}

/// Copy the last PWAD mod (manifest, modules, WAT sources) into the editor's
/// mod so it can be edited and re-exported; it then overrides the PWAD copy.
#[wasm_bindgen(js_name = roomEditorImportScripts)]
pub fn room_editor_import_scripts() -> Result<String, JsValue> {
    with_editor(|ed| {
        let (manifest, c) =
            pwad_manifest(&ed.vfs).ok_or("no ROOMMOD / room.toml in the loaded PWADs")?;
        let mut modules = Vec::new();
        for m in &manifest.modules {
            let wasm = container_lump(&ed.vfs, c, &m.file)
                .ok_or_else(|| format!("module file {} not found", m.file))?;
            let source = m
                .source
                .as_deref()
                .and_then(|s| container_lump(&ed.vfs, c, s))
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            let stem = m.file.rsplit('/').next().unwrap_or(&m.file);
            let stem = stem.split('.').next().unwrap_or(stem);
            modules.push(ScriptModule {
                lump: lump_name(stem)?,
                name: m.name.clone(),
                wasm,
                source,
            });
        }
        ed.script_mod = EditorMod {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            modules,
            written: std::mem::take(&mut ed.script_mod.written),
        };
        ed.sync_script_lumps();
        Ok(ed.scripts_json().to_string())
    })
}

/// Mod log lines from the running play test since the last call.
#[wasm_bindgen(js_name = roomEditorScriptLog)]
pub fn room_editor_script_log() -> Vec<JsValue> {
    with_play(|p| p.drain_script_log())
        .unwrap_or_default()
        .into_iter()
        .map(JsValue::from)
        .collect()
}

/// Host imports for the reference panel: `[{name, params, result, doc}]`.
#[wasm_bindgen(js_name = roomEditorScriptApi)]
pub fn room_editor_script_api() -> String {
    use room_script::{GameFn, ValKind};
    let kind = |k: ValKind| match k {
        ValKind::I32 => "i32",
        ValKind::I64 => "i64",
        ValKind::F64 => "f64",
    };
    let core: &[(&str, &str, &str)] = &[
        ("log", "i32 i32 i32", ""),
        ("level_time", "", "i32"),
        ("player_count", "", "i32"),
        ("actor_exists", "i64", "i32"),
        ("name_intern", "i32 i32", "i32"),
        ("class_find", "i32 i32", "i32"),
        ("acs_execute", "i32 i32 i32 i32", "i32"),
        ("reg_event_handler", "i32 i32 i32 i32 i32 i32 i32", "i32"),
        ("reg_class_begin", "i32 i32 i32 i32 i32 i32", "i32"),
        ("reg_class_property_i32", "i32 i32", ""),
        ("reg_class_property_str", "i32 i32 i32", ""),
        ("reg_class_flag", "i32 i32", ""),
        ("reg_class_end", "", ""),
    ];
    let mut out: Vec<Value> = core
        .iter()
        .map(|(n, p, r)| json!({"name": n, "params": p, "result": r}))
        .collect();
    for f in GameFn::ALL {
        out.push(json!({
            "name": f.name(),
            "params": f.params().iter().map(|k| kind(*k)).collect::<Vec<_>>().join(" "),
            "result": f.result().map_or("", kind),
        }));
    }
    Value::Array(out).to_string()
}

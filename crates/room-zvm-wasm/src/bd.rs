//! Brutal Doom example (`docs/37` §8.5, ZW3): compile Brutal Doom's own
//! ZScript, run it on the interpreter and on the AOT tier with the same inputs,
//! and require identical results, traps, fuel, heap and play-host state.
//!
//! Workloads:
//! 1. `ZC_IntStrMap` (BD's dictionary for GZDoom < 4.3.1): scripted insert /
//!    get / remove sequences, checked against a Rust `BTreeMap` too.
//! 2. `ZandroCompat` lump API (what BD's ACS calls through `ScriptCall`):
//!    open real lumps from the pk3, read them byte by byte, and check the bytes
//!    against the archive.
//! 3. Every function in the program with seeded random arguments, to compare
//!    the trap paths too.

use crate::check::Tiers;
use room_zvm::{CompiledProgram, ObjHandle};
use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

/// BD's ZScript lumps, in the order GZDoom compiles them.
pub const BD_ZSCRIPT: [&str; 3] = [
    "zscript.zc",
    "zscript/BD_WCS.zc",
    "zscript/DamageHandler.zc",
];

/// Lumps the lump-API workload reads back through `ZandroCompat`.
pub const BD_LUMPS: [&str; 3] = ["SNDINFO", "KEYCONF", "CVARINFO"];

/// Result of [`run`].
#[derive(Debug, Default)]
pub struct BdReport {
    /// Functions in the program.
    pub functions: usize,
    /// Functions compiled to wasm.
    pub compiled: usize,
    /// Functions left on the interpreter, with the reason.
    pub interpreted: Vec<String>,
    /// Module size.
    pub wasm_bytes: usize,
    /// Calls compared between tiers.
    pub calls: usize,
    /// Calls the AOT tier served (the rest went to the interpreter).
    pub aot_calls: u64,
    /// Calls that trapped (identically on both tiers).
    pub traps: usize,
    /// Lump bytes read back through BD's `LumpRead`.
    pub lump_bytes: usize,
    /// Interpreter / AOT wall time of the comparison runs (ms).
    pub interp_ms: f64,
    /// AOT wall time (ms).
    pub aot_ms: f64,
    /// Wasm engine (`wasmtime` / `wasmi`).
    pub engine: &'static str,
}

impl std::fmt::Display for BdReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Brutal Doom ZScript → wasm: {}/{} functions compiled, {} bytes",
            self.compiled, self.functions, self.wasm_bytes
        )?;
        for i in &self.interpreted {
            writeln!(f, "  interpreted: {i}")?;
        }
        writeln!(
            f,
            "differential: {} calls identical on both tiers ({} served by wasm, {} traps), {} lump bytes read back",
            self.calls, self.aot_calls, self.traps, self.lump_bytes
        )?;
        write!(
            f,
            "time: interpreter {:.2} ms, aot ({}) {:.2} ms",
            self.interp_ms, self.engine, self.aot_ms
        )
    }
}

/// Raw lumps by name, as read from the pk3.
pub type Lumps = BTreeMap<String, Vec<u8>>;

/// Read BD's ZScript sources and the [`BD_LUMPS`] from `brutal22test6.pk3`.
pub fn read_pk3(path: &Path) -> Result<(Vec<(String, String)>, Lumps), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut sources = Vec::new();
    for name in BD_ZSCRIPT {
        let mut entry = zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
        sources.push((name.to_owned(), text));
    }
    let mut lumps = BTreeMap::new();
    let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    for want in BD_LUMPS {
        let found = names.iter().find(|n| {
            let base = n.rsplit('/').next().unwrap_or(n);
            let stem = base.split('.').next().unwrap_or(base);
            stem.eq_ignore_ascii_case(want) && !n.ends_with('/')
        });
        if let Some(n) = found {
            let mut entry = zip.by_name(n).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            lumps.insert(want.to_owned(), bytes);
        }
    }
    Ok((sources, lumps))
}

/// Rust model of BD's `ZC_IntStrMap`, bugs included: `insert` stops scanning
/// at the first larger key and otherwise inserts at index 0, so the arrays are
/// not sorted and a key can be stored twice; `remove` deletes the first match.
#[derive(Default)]
struct IntStrMapModel {
    keys: Vec<i64>,
    values: Vec<String>,
}

impl IntStrMapModel {
    fn insert(&mut self, key: i64, val: &str) {
        let mut idx = 0;
        let mut found = false;
        while idx < self.keys.len() {
            if self.keys[idx] == key {
                self.values[idx] = val.to_owned();
                return;
            }
            if self.keys[idx] > key {
                found = true;
                break;
            }
            idx += 1;
        }
        if !found {
            idx = 0;
        }
        self.keys.insert(idx, key);
        self.values.insert(idx, val.to_owned());
    }

    fn find(&self, key: i64) -> Option<usize> {
        self.keys.iter().position(|k| *k == key)
    }

    fn remove(&mut self, key: i64) {
        if let Some(i) = self.find(key) {
            self.keys.remove(i);
            self.values.remove(i);
        }
    }

    fn get(&self, key: i64) -> String {
        self.find(key)
            .map(|i| self.values[i].clone())
            .unwrap_or_default()
    }
}

/// Lowercase latin-1 view of lump bytes, as `Wads.ReadLump` stores them.
fn lump_text(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Compile BD's ZScript and run the three workloads on both tiers.
pub fn run(
    sources: &[(String, String)],
    lumps: &BTreeMap<String, Vec<u8>>,
) -> Result<BdReport, String> {
    run_as(sources, lumps, false)
}

/// [`run`], with the AOT side either the in-process AOT module (`false`) or a
/// converted-mod module read back from its bytes (`true`, `docs/37` ZW6).
pub fn run_as(
    sources: &[(String, String)],
    lumps: &BTreeMap<String, Vec<u8>>,
    room_mod: bool,
) -> Result<BdReport, String> {
    let result = room_zscript::check(sources);
    if !result.ok() {
        let msgs: Vec<String> = result.diagnostics.iter().map(|d| d.format_line()).collect();
        return Err(format!("BD ZScript does not compile:\n{}", msgs.join("\n")));
    }
    let program: CompiledProgram = result.program.ok_or("no program")?;
    let mut t = if room_mod {
        Tiers::new_room_mod(program)?
    } else {
        Tiers::new(program)?
    };
    let mut report = BdReport {
        functions: t.artifact.compiled.len(),
        compiled: t.artifact.stats.compiled,
        wasm_bytes: t.artifact.bytes.len(),
        ..BdReport::default()
    };
    for (idx, why) in &t.artifact.unsupported {
        let name = &t.aot.program.functions[*idx as usize].name;
        report.interpreted.push(format!("{name}: {why}"));
    }
    for vm in [&mut t.interp, &mut t.aot] {
        vm.play.skill = 6;
        vm.play.gamestate = 0;
        for (name, bytes) in lumps {
            vm.play.wads.insert(name.clone(), lump_text(bytes));
        }
        let hack = vm.new_object("ZC_TMapHack").ok_or("ZC_TMapHack missing")?;
        vm.play.handlers.insert("ZC_TMapHack".into(), hack);
    }

    // 1. ZC_IntStrMap against a reference map.
    let map_i = t
        .interp
        .new_object("ZC_IntStrMap")
        .ok_or("ZC_IntStrMap missing")?;
    let map_a = t
        .aot
        .new_object("ZC_IntStrMap")
        .ok_or("ZC_IntStrMap missing")?;
    if map_i != map_a {
        return Err("heaps diverged before the first call".into());
    }
    let map = map_i;
    let mut reference = IntStrMapModel::default();
    let mut rng = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    for step in 0..400 {
        let key = (next() % 64) as i64 - 8;
        match next() % 4 {
            0 | 1 => {
                let val = format!("v{step}_{key}");
                t.call("ZC_IntStrMap.insert", |vm| {
                    vm.call_method_banks("ZC_IntStrMap.insert", map, &[key], &[], &[&val], &[])
                })?;
                reference.insert(key, &val);
            }
            2 => {
                t.call("ZC_IntStrMap.remove", |vm| {
                    vm.call_method_banks("ZC_IntStrMap.remove", map, &[key], &[], &[], &[])
                })?;
                reference.remove(key);
            }
            _ => {
                let got = t.call("ZC_IntStrMap.get", |vm| {
                    vm.call_method_banks("ZC_IntStrMap.get", map, &[key], &[], &[], &[])
                })?;
                let want = reference.get(key);
                if got.last_s != want {
                    return Err(format!(
                        "ZC_IntStrMap.get({key}) = {:?}, reference {want:?}",
                        got.last_s
                    ));
                }
                let has = t.call("ZC_IntStrMap.checkKey", |vm| {
                    vm.call_method_banks("ZC_IntStrMap.checkKey", map, &[key], &[], &[], &[])
                })?;
                if (has.result == Ok(1)) != reference.find(key).is_some() {
                    return Err(format!(
                        "ZC_IntStrMap.checkKey({key}) disagrees with reference"
                    ));
                }
            }
        }
    }

    // 2. ZandroCompat lump API over real BD lumps.
    for (name, bytes) in lumps {
        let open = t.call("ZandroCompat.LumpOpen", |vm| {
            vm.call_method_banks(
                "ZandroCompat.LumpOpen",
                ObjHandle::NULL,
                &[-2, 0],
                &[],
                &[name],
                &[],
            )
        })?;
        let Ok(lump) = open.result else {
            return Err(format!("LumpOpen({name}) trapped: {:?}", open.result));
        };
        if lump < 0 {
            return Err(format!("LumpOpen({name}) = {lump}"));
        }
        let size = t.call("ZandroCompat.LumpGetInfoInt", |vm| {
            vm.call_method_banks(
                "ZandroCompat.LumpGetInfoInt",
                ObjHandle::NULL,
                &[lump, 0],
                &[],
                &[],
                &[],
            )
        })?;
        if size.result != Ok(bytes.len() as i64) {
            return Err(format!(
                "LumpGetInfoInt({name}) = {:?}, want {}",
                size.result,
                bytes.len()
            ));
        }
        let n = bytes.len().min(512);
        for (pos, want) in bytes.iter().take(n).enumerate() {
            let got = t.call("ZandroCompat.LumpRead", |vm| {
                vm.call_method_banks(
                    "ZandroCompat.LumpRead",
                    ObjHandle::NULL,
                    &[lump, pos as i64, 1],
                    &[],
                    &[],
                    &[],
                )
            })?;
            if got.result != Ok(i64::from(*want)) {
                return Err(format!(
                    "LumpRead({name}, {pos}) = {:?}, want {want}",
                    got.result
                ));
            }
            report.lump_bytes += 1;
        }
        for kind in [0, 2, 3, 4, 5, 9] {
            t.call("ZandroCompat.LumpRead kinds", |vm| {
                vm.call_method_banks(
                    "ZandroCompat.LumpRead",
                    ObjHandle::NULL,
                    &[lump, 1, kind],
                    &[],
                    &[],
                    &[],
                )
            })?;
        }
        t.call("ZandroCompat.LumpReadString", |vm| {
            vm.call_method_banks(
                "ZandroCompat.LumpReadString",
                ObjHandle::NULL,
                &[lump, 4, 40],
                &[],
                &[],
                &[],
            )
        })?;
        t.call("ZandroCompat.LumpGetInfoStr", |vm| {
            vm.call_method_banks(
                "ZandroCompat.LumpGetInfoStr",
                ObjHandle::NULL,
                &[lump, 1],
                &[],
                &[],
                &[],
            )
        })?;
        t.call("ZandroCompat.LumpClose", |vm| {
            vm.call_method_banks(
                "ZandroCompat.LumpClose",
                ObjHandle::NULL,
                &[lump],
                &[],
                &[],
                &[],
            )
        })?;
    }

    // 3. Every function, seeded arguments (including the trap paths).
    t.call_everything(8, 0x9e37_79b9_7f4a_7c15)?;
    report.calls = t.calls;
    report.traps = t.traps;
    report.aot_calls = t.aot.aot_calls();
    report.interp_ms = t.interp_ms;
    report.aot_ms = t.aot_ms;
    report.engine = t.aot.aot_engine().unwrap_or("?");
    Ok(report)
}

//! Brutal Doom / mod compatibility harness (`docs/33` BD0).

use anyhow::{Context, Result, bail};
use room_decorate::{
    ActorDef, ClassDefIr, FlagOp, StateItem, expand_includes_mapped, parse_decorate, split_actors,
};
use room_vfs::{Namespace, Vfs, VfsBuilder};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Aggregated compatibility report.
#[derive(Clone, Debug, Default)]
pub struct ModCheckReport {
    /// Archives inspected.
    pub archives: Vec<String>,
    /// DECORATE actors attempted (lenient per-actor).
    pub actors_total: u32,
    /// Actors that parsed.
    pub actors_ok: u32,
    /// Actors that failed to parse.
    pub actors_fail: u32,
    /// First-error buckets (message prefix → count).
    pub parse_error_buckets: BTreeMap<String, u32>,
    /// Sample parse failures.
    pub parse_fail_samples: Vec<String>,
    /// Flag name (upper) → count of unknown flag ops.
    pub unknown_flags: BTreeMap<String, u32>,
    /// Property name (lower) → count of unknown properties (BD2).
    pub unknown_properties: BTreeMap<String, u32>,
    /// Parent class names missing from the built-in + parsed set.
    pub missing_parents: BTreeMap<String, u32>,
    /// Action names unknown even with case-fold.
    pub unknown_actions: BTreeMap<String, u32>,
    /// Actions known only after case-fold (dispatch is case-sensitive).
    pub case_mismatch_actions: BTreeMap<String, u32>,
    /// ACS objects loaded.
    pub acs_ok: u32,
    /// ACS load failures.
    pub acs_fail: u32,
    /// ACS chunk fourccs of interest (FUNC/ARAY/…).
    pub acs_chunks_seen: BTreeMap<String, u32>,
    /// ZScript sources found.
    pub zscript_files: u32,
    /// ZScript check ok.
    pub zscript_ok: bool,
    /// ZScript notes.
    pub zscript_notes: Vec<String>,
    /// BD7 text lumps present (name → true if parsed without builder warning).
    pub lumps_seen: BTreeMap<String, bool>,
    /// BD7 / defs builder warnings (parse failures).
    pub lump_errors: Vec<String>,
    /// Player classes registered via KEYCONF `addplayerclass`.
    pub player_classes: Vec<String>,
    /// MAPINFO skill count after clearskills merge.
    pub mapinfo_skills: u32,
}

impl ModCheckReport {
    /// Markdown summary.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        s.push_str("# Mod compatibility report\n\n");
        s.push_str(&format!("Archives: {}\n\n", self.archives.join(", ")));
        s.push_str("## DECORATE\n\n");
        s.push_str(&format!(
            "- actors: {} ok / {} fail / {} total\n",
            self.actors_ok, self.actors_fail, self.actors_total
        ));
        if !self.parse_error_buckets.is_empty() {
            s.push_str("- parse error buckets:\n");
            for (k, n) in self.parse_error_buckets.iter().take(20) {
                s.push_str(&format!("  - {k}: {n}\n"));
            }
        }
        if !self.parse_fail_samples.is_empty() {
            s.push_str("- parse fail samples:\n");
            for sample in self.parse_fail_samples.iter().take(8) {
                s.push_str(&format!("  - {sample}\n"));
            }
        }
        s.push_str(&format!(
            "- unknown flags: {} distinct\n",
            self.unknown_flags.len()
        ));
        for (k, n) in self.unknown_flags.iter().take(15) {
            s.push_str(&format!("  - {k}: {n}\n"));
        }
        s.push_str(&format!(
            "- unknown properties: {} distinct\n",
            self.unknown_properties.len()
        ));
        for (k, n) in self.unknown_properties.iter().take(15) {
            s.push_str(&format!("  - {k}: {n}\n"));
        }
        s.push_str(&format!(
            "- missing parents: {} distinct\n",
            self.missing_parents.len()
        ));
        for (k, n) in self.missing_parents.iter().take(15) {
            s.push_str(&format!("  - {k}: {n}\n"));
        }
        s.push_str(&format!(
            "- unknown actions: {}\n",
            self.unknown_actions.len()
        ));
        for (k, n) in self.unknown_actions.iter().take(15) {
            s.push_str(&format!("  - {k}: {n}\n"));
        }
        s.push_str(&format!(
            "- case-mismatch actions: {}\n",
            self.case_mismatch_actions.len()
        ));
        for (k, n) in self.case_mismatch_actions.iter().take(10) {
            s.push_str(&format!("  - {k}: {n}\n"));
        }
        s.push_str("\n## ACS\n\n");
        s.push_str(&format!(
            "- objects: {} ok / {} fail\n",
            self.acs_ok, self.acs_fail
        ));
        if !self.acs_chunks_seen.is_empty() {
            s.push_str("- chunks seen:\n");
            for (k, n) in &self.acs_chunks_seen {
                s.push_str(&format!("  - {k}: {n}\n"));
            }
        }
        s.push_str("\n## ZScript\n\n");
        s.push_str(&format!(
            "- files: {}; check_ok={}\n",
            self.zscript_files, self.zscript_ok
        ));
        for n in self.zscript_notes.iter().take(10) {
            s.push_str(&format!("- {n}\n"));
        }
        s.push_str("\n## Lumps (BD7)\n\n");
        s.push_str(&format!(
            "- seen: {}; errors: {}; skills: {}; player_classes: {}\n",
            self.lumps_seen.len(),
            self.lump_errors.len(),
            self.mapinfo_skills,
            self.player_classes.len()
        ));
        for (k, ok) in &self.lumps_seen {
            s.push_str(&format!("  - {k}: {}\n", if *ok { "ok" } else { "fail" }));
        }
        for e in self.lump_errors.iter().take(12) {
            s.push_str(&format!("  - error: {e}\n"));
        }
        for c in self.player_classes.iter().take(8) {
            s.push_str(&format!("  - class: {c}\n"));
        }
        s
    }

    /// Compact JSON summary.
    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            "{{\"actors_total\":{},\"actors_ok\":{},\"actors_fail\":{},\"unknown_flags\":{},\"unknown_properties\":{},\"missing_parents\":{},\"unknown_actions\":{},\"case_mismatch_actions\":{},\"acs_ok\":{},\"acs_fail\":{},\"zscript_files\":{},\"zscript_ok\":{},\"lump_errors\":{},\"lumps_seen\":{},\"mapinfo_skills\":{},\"player_classes\":{}}}",
            self.actors_total,
            self.actors_ok,
            self.actors_fail,
            self.unknown_flags.len(),
            self.unknown_properties.len(),
            self.missing_parents.len(),
            self.unknown_actions.len(),
            self.case_mismatch_actions.len(),
            self.acs_ok,
            self.acs_fail,
            self.zscript_files,
            self.zscript_ok,
            self.lump_errors.len(),
            self.lumps_seen.len(),
            self.mapinfo_skills,
            self.player_classes.len(),
        )
    }
}

/// Run the harness on `archives` (WAD/PK3/dir), optionally with an IWAD first.
pub fn run_mod_check(archives: &[PathBuf], iwad: Option<&Path>) -> Result<ModCheckReport> {
    if archives.is_empty() {
        bail!("mod check requires at least one archive path");
    }
    let mut builder = VfsBuilder::new();
    if let Some(iwad) = iwad {
        builder = builder
            .mount_path(iwad)
            .with_context(|| format!("mount iwad {}", iwad.display()))?;
    }
    for a in archives {
        builder = builder
            .mount_path(a)
            .with_context(|| format!("mount {}", a.display()))?;
    }
    let vfs = builder.build().context("build vfs")?;
    let mut report = ModCheckReport {
        archives: archives.iter().map(|p| p.display().to_string()).collect(),
        ..Default::default()
    };

    check_decorate(&vfs, &mut report)?;
    check_acs(&vfs, &mut report);
    check_zscript(&vfs, archives, &mut report);
    check_lumps(&vfs, &mut report);
    Ok(report)
}

/// BD7: parse KEYCONF/CVARINFO/MENUDEF/SBARINFO/MAPINFO/GLDEFS/MODELDEF/
/// DECALDEF/TERRAIN/SNDINFO/LANGUAGE via `DefinitionsBuilder`.
fn check_lumps(vfs: &Vfs, report: &mut ModCheckReport) {
    const BD7: &[&str] = &[
        "KEYCONF", "CVARINFO", "MENUDEF", "SBARINFO", "MAPINFO", "GLDEFS", "MODELDEF", "DECALDEF",
        "TERRAIN", "SNDINFO", "LANGUAGE",
    ];
    let defs = match room_defs::DefinitionsBuilder::new(vfs).build() {
        Ok(d) => d,
        Err(e) => {
            report.lump_errors.push(format!("defs build: {e}"));
            return;
        }
    };
    for name in BD7 {
        let present = match *name {
            "KEYCONF" => !defs.keyconf.commands.is_empty(),
            "CVARINFO" => !defs.cvars.is_empty(),
            "MENUDEF" => !defs.menudef.blocks.is_empty(),
            "SBARINFO" => defs.sbarinfo.is_some(),
            "MAPINFO" => {
                !defs.mapinfo.maps.is_empty()
                    || !defs.mapinfo.skills.is_empty()
                    || !defs.mapinfo.event_handlers.is_empty()
            }
            "GLDEFS" => !defs.gldefs.lights.is_empty() || !defs.gldefs.object_lights.is_empty(),
            "MODELDEF" => !defs.modeldefs.by_actor.is_empty(),
            "DECALDEF" => !defs.decaldef.decals.is_empty(),
            "TERRAIN" => !defs.terrain.terrains.is_empty() || !defs.terrain.floors.is_empty(),
            "SNDINFO" => !defs.sounds.sounds.is_empty(),
            "LANGUAGE" => !defs.language.tables.is_empty(),
            _ => false,
        };
        if present
            || vfs.find(name, Namespace::Global).is_some()
            || matches!(*name, "MAPINFO")
                && (vfs.find("MAPINFO", Namespace::Global).is_some()
                    || vfs.find("mapinfo", Namespace::Global).is_some())
        {
            let warn = defs
                .warnings
                .iter()
                .any(|w| w.to_ascii_uppercase().starts_with(&format!("{name}:")));
            report.lumps_seen.insert((*name).to_owned(), !warn);
        }
    }
    for w in &defs.warnings {
        let upper = w.to_ascii_uppercase();
        if BD7.iter().any(|n| upper.starts_with(&format!("{n}:"))) {
            report.lump_errors.push(w.clone());
        }
    }
    report.mapinfo_skills = defs.mapinfo.skills.len() as u32;
    for cmd in &defs.keyconf.commands {
        if let room_defs::KeyConfCmd::AddPlayerClass { class, .. } = cmd {
            report.player_classes.push(class.clone());
        }
    }
}

fn check_decorate(vfs: &Vfs, report: &mut ModCheckReport) -> Result<()> {
    let Some(root) = read_lump_text(vfs, "DECORATE") else {
        report.zscript_notes.push("no DECORATE lump found".into());
        return Ok(());
    };
    let base_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/room/actors/doom.txt");
    let base = std::fs::read_to_string(&base_path).unwrap_or_default();
    let (expanded, source_map) =
        match expand_includes_mapped(&root, |path| resolve_include(vfs, path)) {
            Ok(exp) => (exp.text, Some(exp.map)),
            Err(e) => {
                report
                    .parse_fail_samples
                    .push(format!("include expand: {e}"));
                (root, None)
            }
        };
    let combined = if base.is_empty() {
        expanded
    } else {
        format!("{base}\n{expanded}")
    };

    let chunks = split_actors(&combined);
    report.actors_total = chunks.len() as u32;
    let mut ok_ir = ClassDefIr::default();
    for chunk in &chunks {
        match parse_decorate(chunk) {
            Ok(ir) => {
                report.actors_ok += 1;
                for a in ir.actors {
                    tally_actor(&a, report);
                    ok_ir.actors.push(a);
                }
            }
            Err(e) => {
                report.actors_fail += 1;
                let msg = format!("{e}");
                let bucket = bucketize_parse_error(&msg);
                *report.parse_error_buckets.entry(bucket).or_insert(0) += 1;
                if report.parse_fail_samples.len() < 8 {
                    let head: String = chunk.chars().take(72).collect();
                    let loc = source_map
                        .as_ref()
                        .and_then(|m| m.lookup(1))
                        .map(|l| format!("{}:{}: ", l.file, l.line))
                        .unwrap_or_default();
                    report
                        .parse_fail_samples
                        .push(format!("{loc}{head}… → {msg}"));
                }
            }
        }
    }

    let mut have: BTreeMap<String, ()> = BTreeMap::new();
    if let Ok(base_ir) = parse_decorate(&base) {
        for a in &base_ir.actors {
            have.insert(a.name.to_ascii_lowercase(), ());
        }
    }
    for name in ["Actor", "Thinker", "Object"] {
        have.insert(name.to_ascii_lowercase(), ());
    }
    let mut missing = BTreeMap::new();
    for a in &ok_ir.actors {
        if let Some(p) = &a.parent {
            if !have.contains_key(&p.to_ascii_lowercase()) {
                *missing.entry(p.clone()).or_insert(0) += 1;
            }
        }
        have.insert(a.name.to_ascii_lowercase(), ());
    }
    report.missing_parents = missing;
    Ok(())
}

fn tally_actor(a: &ActorDef, report: &mut ModCheckReport) {
    for f in &a.flags {
        tally_flag(f, report);
    }
    for p in &a.properties {
        if room_sim::is_known_property(&p.name) {
            continue;
        }
        let key = p.name.to_ascii_lowercase();
        *report.unknown_properties.entry(key).or_insert(0) += 1;
    }
    let Some(states) = &a.states else {
        return;
    };
    for item in &states.items {
        let StateItem::Line(line) = item else {
            continue;
        };
        let Some(name) = &line.action else {
            continue;
        };
        if room_sim::is_known_action(name) || room_sim::is_special_action(name) {
            continue;
        }
        if room_sim::is_known_action_ci(name) {
            *report
                .case_mismatch_actions
                .entry(name.clone())
                .or_insert(0) += 1;
        } else {
            *report.unknown_actions.entry(name.clone()).or_insert(0) += 1;
        }
    }
}

fn tally_flag(op: &FlagOp, report: &mut ModCheckReport) {
    // Qualified flags: check last segment and full upper name.
    if room_sim::is_known_flag(&op.name) {
        return;
    }
    if let Some((_, leaf)) = op.name.rsplit_once('.') {
        if room_sim::is_known_flag(leaf) {
            return;
        }
    }
    let key = op.name.to_ascii_uppercase();
    *report.unknown_flags.entry(key).or_insert(0) += 1;
}

fn bucketize_parse_error(msg: &str) -> String {
    let m = msg.to_ascii_lowercase();
    if m.contains("light(")
        || m.contains("offset(")
        || m.contains("unexpected") && m.contains("light")
    {
        "D1 state Light/Offset".into()
    } else if m.contains("dropitem") {
        "D6 DropItem".into()
    } else {
        msg.chars().take(64).collect()
    }
}

fn read_lump_text(vfs: &Vfs, name: &str) -> Option<String> {
    let id = vfs
        .find(name, Namespace::Global)
        .or_else(|| vfs.find_any_ns(name))
        .or_else(|| vfs.find_full(&format!("{}.txt", name.to_ascii_lowercase())))
        .or_else(|| vfs.find_full(&format!("{name}.txt")))?;
    let bytes = vfs.read(id).ok()?;
    String::from_utf8(bytes.to_vec()).ok()
}

fn resolve_include(vfs: &Vfs, path: &str) -> Result<String, room_decorate::DecorateError> {
    let norm = path.replace('\\', "/");
    let candidates = [
        norm.clone(),
        norm.to_ascii_lowercase(),
        norm.trim_start_matches('/').to_owned(),
    ];
    for c in &candidates {
        if let Some(id) = vfs.find_full(c) {
            let bytes = vfs
                .read(id)
                .map_err(|e| room_decorate::DecorateError::Message(format!("read {c}: {e}")))?;
            return String::from_utf8(bytes.to_vec())
                .map_err(|e| room_decorate::DecorateError::Message(format!("utf8 {c}: {e}")));
        }
        let stem = Path::new(c)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(c);
        if let Some(id) = vfs.find(stem, Namespace::Global) {
            let bytes = vfs
                .read(id)
                .map_err(|e| room_decorate::DecorateError::Message(format!("read {stem}: {e}")))?;
            return String::from_utf8(bytes.to_vec())
                .map_err(|e| room_decorate::DecorateError::Message(format!("utf8 {stem}: {e}")));
        }
    }
    Err(room_decorate::DecorateError::Message(format!(
        "include not found: {path}"
    )))
}

fn check_acs(vfs: &Vfs, report: &mut ModCheckReport) {
    for name in ["BD_Main", "BD_Hash", "zancmpat", "BEHAVIOR"] {
        let Some(id) = vfs
            .find(name, Namespace::Global)
            .or_else(|| vfs.find(name, Namespace::Acs))
            .or_else(|| vfs.find_any_ns(name))
        else {
            continue;
        };
        let Ok(bytes) = vfs.read(id) else {
            report.acs_fail += 1;
            continue;
        };
        match room_acs::load_behavior(&bytes) {
            Ok(_) => {
                report.acs_ok += 1;
                scan_acs_chunks(&bytes, report);
            }
            Err(_) => report.acs_fail += 1,
        }
    }
}

fn scan_acs_chunks(bytes: &[u8], report: &mut ModCheckReport) {
    const TRACK: &[&[u8]] = &[
        b"FUNC", b"FNAM", b"ARAY", b"AINI", b"SARY", b"FARY", b"MINI", b"MEXP", b"ALIB", b"ASTR",
        b"ATAG", b"SFLG", b"SVCT", b"SPTR", b"STRL", b"SNAM", b"LOAD",
    ];
    let mut i = 0;
    while i + 8 < bytes.len() {
        let id = &bytes[i..i + 4];
        if id.iter().all(|b| b.is_ascii_uppercase()) {
            let size = u32::from_le_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]])
                as usize;
            if size > 0 && size < 16_000_000 && i + 8 + size <= bytes.len() {
                if TRACK.iter().any(|k| *k == id) {
                    let key = String::from_utf8_lossy(id).into_owned();
                    *report.acs_chunks_seen.entry(key).or_insert(0) += 1;
                }
                i += 8 + size;
                continue;
            }
        }
        i += 1;
    }
}

fn check_zscript(vfs: &Vfs, archives: &[PathBuf], report: &mut ModCheckReport) {
    let mut sources: Vec<(String, String)> = Vec::new();
    // Prefer VFS (covers PK3/WAD mounts); fall back to directory walk.
    collect_zscript_from_vfs(vfs, &mut sources);
    if sources.is_empty() {
        for arch in archives {
            collect_zscript(arch, &mut sources);
        }
    }
    // `#include` resolution expects the entry `zscript.zc` first.
    sources.sort_by(|a, b| {
        let score = |n: &str| {
            let base = n.rsplit(['/', '\\']).next().unwrap_or(n);
            if base.eq_ignore_ascii_case("zscript.zc") || base.eq_ignore_ascii_case("zscript") {
                0
            } else {
                1
            }
        };
        score(&a.0).cmp(&score(&b.0)).then_with(|| a.0.cmp(&b.0))
    });
    report.zscript_files = sources.len() as u32;
    if sources.is_empty() {
        report.zscript_ok = true;
        return;
    }
    let result = room_zscript::check(&sources);
    report.zscript_ok = result.ok();
    for d in result.diagnostics.iter().take(12) {
        report
            .zscript_notes
            .push(format!("{:?}: {} ({})", d.severity, d.message, d.path));
    }
}

fn collect_zscript_from_vfs(vfs: &Vfs, out: &mut Vec<(String, String)>) {
    for (id, info) in vfs.iter() {
        let path = if !info.full_path.is_empty() {
            info.full_path.clone()
        } else {
            info.short_name.as_str().to_owned()
        };
        let lower = path.to_ascii_lowercase();
        let is_zs = lower.ends_with(".zs")
            || lower.ends_with(".zc")
            || lower.ends_with(".zsc")
            || lower == "zscript"
            || lower.ends_with("/zscript");
        if !is_zs {
            continue;
        }
        let Ok(bytes) = vfs.read(id) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes.to_vec()) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        out.push((path, text));
    }
}

fn collect_zscript(path: &Path, out: &mut Vec<(String, String)>) {
    if path.is_file() {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        // Loose .zs/.zc/.zsc — or a PK3/ZIP/WAD (handled via VFS elsewhere).
        if matches!(ext.as_str(), "zs" | "zc" | "zsc") {
            if let Ok(s) = std::fs::read_to_string(path) {
                out.push((path.display().to_string(), s));
            }
        }
        return;
    }
    if path.is_dir() {
        for f in walkdir_shallow(path, 8) {
            collect_zscript(&f, out);
        }
    }
}

fn walkdir_shallow(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn rec(dir: &Path, depth: usize, max: usize, out: &mut Vec<PathBuf>) {
        if depth > max {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                rec(&p, depth + 1, max, out);
            } else {
                out.push(p);
            }
        }
    }
    rec(root, 0, max_depth, &mut out);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn split_actors_finds_two() {
        let src = r#"
actor Foo : Actor { Health 1 States { Spawn: POSS A -1 Stop } }
actor Bar : Actor { Health 2 States { Spawn: POSS A -1 Stop } }
"#;
        assert_eq!(room_decorate::split_actors(src).len(), 2);
    }

    #[test]
    fn fixture_mod_check_runs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/mods/bd_check");
        assert!(root.is_dir(), "missing fixture {}", root.display());
        let report = run_mod_check(&[root], None).unwrap();
        assert!(
            report.actors_total >= 2,
            "actors_total={}",
            report.actors_total
        );
        // BD1 landed Light()/Fail: labels — fixture may have 0 parse fails now.
        assert!(
            report.unknown_flags.contains_key("BDCHECK_NOT_A_REAL_FLAG"),
            "expected synthetic unknown flag; flags={:?}",
            report.unknown_flags.keys().collect::<Vec<_>>()
        );
        assert!(
            report.missing_parents.contains_key("BdCheckMissingBase"),
            "expected synthetic missing parent; parents={:?}",
            report.missing_parents.keys().collect::<Vec<_>>()
        );
        assert!(
            report
                .unknown_properties
                .keys()
                .any(|k| k.eq_ignore_ascii_case("BdCheck.NotARealProp")),
            "expected synthetic unknown property; props={:?}",
            report.unknown_properties.keys().collect::<Vec<_>>()
        );
        // Case-fold: A_WEAPONREADY must resolve (BD3), not sit in unknown_actions.
        assert!(
            !report
                .unknown_actions
                .keys()
                .any(|k| k.eq_ignore_ascii_case("A_WEAPONREADY")),
            "A_WEAPONREADY should be known via case-fold; unknowns={:?}",
            report.unknown_actions.keys().collect::<Vec<_>>()
        );
        // BD7: fixture text lumps parse with 0 errors.
        assert!(
            report.lump_errors.is_empty(),
            "BD7 lump errors: {:?}",
            report.lump_errors
        );
        for name in [
            "KEYCONF", "CVARINFO", "MENUDEF", "SBARINFO", "MAPINFO", "GLDEFS", "MODELDEF",
            "DECALDEF", "TERRAIN", "SNDINFO", "LANGUAGE",
        ] {
            assert!(
                report.lumps_seen.get(name) == Some(&true),
                "BD7 expected {name} ok; seen={:?}",
                report.lumps_seen
            );
        }
        assert!(
            report.mapinfo_skills >= 2,
            "expected clearskills + skills; got {}",
            report.mapinfo_skills
        );
        assert!(
            report
                .player_classes
                .iter()
                .any(|c| c.eq_ignore_ascii_case("DoomPlayer")),
            "expected KEYCONF player class; got {:?}",
            report.player_classes
        );
    }

    #[test]
    fn brutal_doom_pk3_gated() {
        let Ok(pk3) = std::env::var("ROOM_BRUTALDOOM_PK3") else {
            return;
        };
        let path = PathBuf::from(pk3);
        if !path.is_file() {
            return;
        }
        let report = run_mod_check(&[path.clone()], None).expect("BD mod check");
        // BD0/BD1/BD2 commercial bar (docs/33): full parse, no unknown tables.
        assert!(
            report.actors_total > 4000,
            "expected thousands of BD actors, got {}",
            report.actors_total
        );
        assert_eq!(
            report.actors_fail, 0,
            "DECORATE parse fails: {:?}",
            report.parse_fail_samples
        );
        assert_eq!(report.actors_ok, report.actors_total);
        assert!(
            report.unknown_flags.is_empty(),
            "unknown flags: {:?}",
            report.unknown_flags.keys().collect::<Vec<_>>()
        );
        assert!(
            report.unknown_properties.is_empty(),
            "unknown props: {:?}",
            report.unknown_properties.keys().collect::<Vec<_>>()
        );
        assert!(
            report.missing_parents.is_empty(),
            "missing parents: {:?}",
            report.missing_parents.keys().collect::<Vec<_>>()
        );
        assert!(
            report.unknown_actions.is_empty(),
            "unknown actions: {:?}",
            report.unknown_actions.keys().collect::<Vec<_>>()
        );
        assert!(report.zscript_files > 0, "BD should ship ZScript files");
        assert!(
            report.zscript_ok,
            "ZScript notes: {:?}",
            report.zscript_notes
        );
        assert!(
            report.lump_errors.is_empty(),
            "BD7 lump errors: {:?}",
            report.lump_errors
        );
        eprintln!(
            "BD gate: actors={}/{} zscript_files={} lumps={} skills={} classes={:?}",
            report.actors_ok,
            report.actors_total,
            report.zscript_files,
            report.lumps_seen.len(),
            report.mapinfo_skills,
            report.player_classes
        );
    }
}

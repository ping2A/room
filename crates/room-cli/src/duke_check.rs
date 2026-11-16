//! EDuke32 / Brutal Duke compatibility inventory (`docs/25` ED0).

use anyhow::{Context, Result, bail};
use room_build::def::DefDocument;
use room_build::map_hack::MapHackDocument;
use room_duke::con::{
    ConCompileOptions, ConDialect, EdInstruction, EdukeVmState, IncludeResolver, KEYWORDS,
    NullIncludes, compile_with_options,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
struct Hit {
    occurrences: usize,
    locations: Vec<String>,
}

impl Hit {
    fn record(&mut self, location: String) {
        self.occurrences += 1;
        if self.locations.len() < 3 && !self.locations.contains(&location) {
            self.locations.push(location);
        }
    }
}

#[derive(Debug)]
struct SourceTree {
    root: PathBuf,
    con_files: Vec<PathBuf>,
    def_files: Vec<PathBuf>,
    mhk_files: Vec<PathBuf>,
    con_by_path: BTreeMap<String, Vec<u8>>,
    con_by_basename: BTreeMap<String, String>,
    resolved_includes: BTreeSet<String>,
    missing_includes: BTreeSet<String>,
}

impl SourceTree {
    fn load(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            bail!("{} is not a directory", root.display());
        }
        let root = root
            .canonicalize()
            .with_context(|| format!("resolve {}", root.display()))?;
        let mut paths = Vec::new();
        collect_files(&root, 0, &mut paths)?;
        paths.sort();

        let con_files = paths
            .iter()
            .filter(|path| extension_is(path, "con"))
            .cloned()
            .collect::<Vec<_>>();
        let def_files = paths
            .iter()
            .filter(|path| extension_is(path, "def"))
            .cloned()
            .collect::<Vec<_>>();
        let mhk_files = paths
            .iter()
            .filter(|path| extension_is(path, "mhk"))
            .cloned()
            .collect::<Vec<_>>();
        let mut con_by_path = BTreeMap::new();
        let mut con_by_basename = BTreeMap::new();
        for path in &con_files {
            let rel = path.strip_prefix(&root).unwrap_or(path);
            let key = normalize_include(&rel.to_string_lossy());
            let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
            con_by_basename
                .entry(file_name_key(path))
                .or_insert_with(|| key.clone());
            con_by_path.insert(key, bytes);
        }

        Ok(Self {
            root,
            con_files,
            def_files,
            mhk_files,
            con_by_path,
            con_by_basename,
            resolved_includes: BTreeSet::new(),
            missing_includes: BTreeSet::new(),
        })
    }

    fn location(&self, path: &Path, line: usize) -> String {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        format!("{}:{line}", relative.display())
    }

    fn root_con(&self) -> Result<&Path> {
        self.con_files
            .iter()
            .find(|path| file_name_key(path) == "eduke.con")
            .or_else(|| {
                self.con_files
                    .iter()
                    .find(|path| file_name_key(path) == "game.con")
            })
            .map(PathBuf::as_path)
            .context("no eduke.con or GAME.CON found in the mod directory")
    }
}

impl IncludeResolver for SourceTree {
    fn read_include(&mut self, name: &str) -> Option<Vec<u8>> {
        let normalized = normalize_include(name);
        let resolved = self.con_by_path.get(&normalized).or_else(|| {
            let basename = normalized.rsplit('/').next().unwrap_or(&normalized);
            self.con_by_basename
                .get(basename)
                .and_then(|path| self.con_by_path.get(path))
        });
        if let Some(bytes) = resolved {
            self.resolved_includes.insert(normalized);
            Some(bytes.clone())
        } else {
            self.missing_includes.insert(normalized);
            None
        }
    }
}

#[derive(Debug, Default)]
struct Report {
    con_files: usize,
    con_lines: usize,
    mod_con_files: usize,
    mod_con_lines: usize,
    con_compile_errors: u32,
    con_error_heads: BTreeMap<String, u32>,
    con_error_samples: BTreeMap<String, Vec<String>>,
    con_script_words: usize,
    con_script_sha256: String,
    includes_resolved: usize,
    includes_missing: usize,
    eduke_variables: usize,
    eduke_arrays: usize,
    eduke_states: usize,
    eduke_events: usize,
    eduke_projectiles: usize,
    eduke_damage_tiles: usize,
    eduke_vm_instructions: usize,
    eduke_vm_noops: usize,
    eduke_vm_noop_heads: BTreeMap<String, u32>,
    eduke_storage_status: String,
    distinct_non_atomic_identifiers: usize,
    unknown_command_heads: BTreeMap<String, Hit>,
    referenced_members: BTreeMap<String, Hit>,
    unknown_events: BTreeMap<String, Hit>,
    event_occurrences: usize,
    def_files: usize,
    def_lines: usize,
    def_high_res_tiles: usize,
    def_tile_images_decoded: usize,
    def_tile_image_errors: Vec<String>,
    def_parse_errors: Vec<String>,
    def_brace_errors: Vec<String>,
    map_hack_entries: usize,
    mhk_files: usize,
    mhk_commands: usize,
    mhk_parse_errors: Vec<String>,
    missing_include_names: Vec<String>,
    def_statement_counts: BTreeMap<String, usize>,
    unknown_def_tokens: BTreeMap<String, Hit>,
}

impl Report {
    fn markdown(&self, dialect: ConDialect) -> String {
        let mut out = String::new();
        out.push_str("# Duke mod compatibility report\n\n");
        out.push_str(&format!(
            "Dialect: {dialect:?} (partial compiler and VM; selected event and actor call sites are dispatched by DukeSession)\n\n"
        ));
        out.push_str("## CON\n\n");
        out.push_str(&format!(
            "- files: {}; lines: {}; Brutal Duke CON files/lines: {}/{}; include names resolved/unresolved: {}/{}; remaining compile errors: {}; emitted words: {}; script SHA-256: `{}`\n",
            self.con_files,
            self.con_lines,
            self.mod_con_files,
            self.mod_con_lines,
            self.includes_resolved,
            self.includes_missing,
            self.con_compile_errors,
            self.con_script_words,
            self.con_script_sha256,
        ));
        out.push_str("- EDuke32 diagnostics count unsupported statements once per source line; the EDuke32 instruction stream is a partial semantic subset, while the parallel Atomic wordcode remains for Atomic compatibility\n");
        if !self.con_error_heads.is_empty() {
            let mut heads = self.con_error_heads.iter().collect::<Vec<_>>();
            heads.sort_by(|(left_name, left_count), (right_name, right_count)| {
                right_count
                    .cmp(left_count)
                    .then_with(|| left_name.cmp(right_name))
            });
            out.push_str("- compile errors by command head (top 30):\n");
            for (head, count) in heads.into_iter().take(30) {
                let samples = self
                    .con_error_samples
                    .get(head)
                    .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join(", "))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  - `{head}`: {count} (sample lines: {samples})\n"
                ));
            }
        }
        for name in &self.missing_include_names {
            out.push_str(&format!("  - missing include: `{name}`\n"));
        }
        out.push_str(&format!(
            "- distinct CON identifiers outside the 108 Atomic commands: {}\n",
            self.distinct_non_atomic_identifiers,
        ));
        out.push_str(&format!(
            "- EDuke32 metadata parsed: {} vars / {} arrays / {} states / {} event hooks / {} projectile properties / {} damage-event tiles\n",
            self.eduke_variables,
            self.eduke_arrays,
            self.eduke_states,
            self.eduke_events,
            self.eduke_projectiles,
            self.eduke_damage_tiles,
        ));
        out.push_str(&format!(
            "- EDuke32 declaration storage: {}\n",
            self.eduke_storage_status
        ));
        out.push_str(&format!(
            "- EDuke32 executable-core instructions: {} ({} explicit no-op instructions; partial source subset; selected event and actor call sites are dispatched by DukeSession)\n",
            self.eduke_vm_instructions,
            self.eduke_vm_noops,
        ));
        if !self.eduke_vm_noop_heads.is_empty() {
            let mut heads = self.eduke_vm_noop_heads.iter().collect::<Vec<_>>();
            heads.sort_by(|(left_name, left_count), (right_name, right_count)| {
                right_count
                    .cmp(left_count)
                    .then_with(|| left_name.cmp(right_name))
            });
            out.push_str("- EDuke32 explicit no-op statements by command head (top 40):\n");
            for (head, count) in heads.into_iter().take(40) {
                out.push_str(&format!("  - `{head}`: {count}\n"));
            }
        }
        append_hits(
            &mut out,
            "Non-Atomic command heads",
            &self.unknown_command_heads,
        );
        out.push_str(&format!("- event blocks: {}\n", self.event_occurrences));
        append_hits(&mut out, "Referenced event names", &self.unknown_events);
        append_hits(
            &mut out,
            "Referenced struct members",
            &self.referenced_members,
        );

        out.push_str("\n## DEF\n\n");
        out.push_str(&format!(
            "- files: {}; lines: {}\n",
            self.def_files, self.def_lines
        ));
        out.push_str(&format!(
            "- parsed high-resolution tile replacements: {}\n",
            self.def_high_res_tiles
        ));
        out.push_str(&format!(
            "- high-resolution tile images decoded: {}; asset errors: {}\n",
            self.def_tile_images_decoded,
            self.def_tile_image_errors.len()
        ));
        for issue in self.def_tile_image_errors.iter().take(20) {
            out.push_str(&format!("  - {issue}\n"));
        }
        out.push_str(&format!(
            "- DEF parser errors: {}\n",
            self.def_parse_errors.len()
        ));
        for issue in self.def_parse_errors.iter().take(20) {
            out.push_str(&format!("  - {issue}\n"));
        }
        if !self.def_statement_counts.is_empty() {
            out.push_str("- key declaration counts:\n");
            for (word, count) in &self.def_statement_counts {
                out.push_str(&format!("  - `{word}`: {count}\n"));
            }
        }
        append_hits(&mut out, "DEF statement heads", &self.unknown_def_tokens);
        out.push_str(&format!(
            "- structural brace issues: {}\n",
            self.def_brace_errors.len()
        ));
        for issue in self.def_brace_errors.iter().take(20) {
            out.push_str(&format!("  - {issue}\n"));
        }
        out.push_str("\n## Map hacks\n\n");
        out.push_str(&format!(
            "- DEF map entries: {}; MHK files: {}; parsed sprite/light commands: {}; parse errors: {}\n",
            self.map_hack_entries,
            self.mhk_files,
            self.mhk_commands,
            self.mhk_parse_errors.len()
        ));
        for issue in self.mhk_parse_errors.iter().take(20) {
            out.push_str(&format!("  - {issue}\n"));
        }
        out
    }
}

fn append_hits(out: &mut String, title: &str, hits: &BTreeMap<String, Hit>) {
    let occurrences = hits.values().map(|hit| hit.occurrences).sum::<usize>();
    out.push_str(&format!(
        "- {title}: {} distinct / {occurrences} occurrences\n",
        hits.len()
    ));
    for (name, hit) in hits {
        out.push_str(&format!(
            "  - `{name}` ({}): {}\n",
            hit.occurrences,
            hit.locations.join(", ")
        ));
    }
}

/// Inventory and compile a Duke CON/DEF source tree.
pub fn run_mod_check(root: &Path, dialect: ConDialect) -> Result<String> {
    let mut tree = SourceTree::load(root)?;
    let root_con = tree.root_con()?.to_path_buf();
    let root_bytes = fs::read(&root_con).with_context(|| format!("read {}", root_con.display()))?;
    let image = compile_with_options(&root_bytes, &mut tree, ConCompileOptions { dialect });
    let eduke_storage_status = if dialect == ConDialect::EDuke32 {
        match EdukeVmState::from_image(&image) {
            Ok(state) => format!(
                "initialized ({} variables, {} arrays)",
                state.variable_count(),
                state.array_count()
            ),
            Err(error) => format!("initialization failed: {error}"),
        }
    } else {
        "not requested for Atomic dialect".to_owned()
    };
    let mut report = Report {
        con_files: tree.con_files.len(),
        con_compile_errors: image.errors,
        con_error_heads: image.eduke_error_heads.clone(),
        con_error_samples: image.eduke_error_samples.clone(),
        con_script_words: image.con_size(),
        con_script_sha256: image.script_sha256_hex(),
        includes_resolved: 0,
        includes_missing: 0,
        eduke_variables: image.eduke_variables.len(),
        eduke_arrays: image.eduke_arrays.len(),
        eduke_states: image.eduke_states.len(),
        eduke_events: image.eduke_events.len(),
        eduke_projectiles: image.eduke_projectiles.len(),
        eduke_damage_tiles: image.eduke_damage_event_tiles.len(),
        eduke_vm_instructions: image.eduke_program.instructions.len(),
        eduke_vm_noops: image
            .eduke_program
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction, EdInstruction::NoOp))
            .count(),
        eduke_vm_noop_heads: image.eduke_noop_heads.clone(),
        eduke_storage_status,
        def_files: tree.def_files.len(),
        ..Default::default()
    };

    let mut identifiers = BTreeSet::new();
    for path in &tree.con_files {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let source = String::from_utf8_lossy(&bytes);
        let lines = clean_lines(&source);
        report.con_lines += lines.len();
        let relative = path.strip_prefix(&tree.root).unwrap_or(path);
        let relative = normalize_include(&relative.to_string_lossy());
        if relative == "eduke.con"
            || relative == "scripts/brutal.con"
            || relative.starts_with("scripts/brutal/")
        {
            report.mod_con_files += 1;
            report.mod_con_lines += lines.len();
        }
        for (line_index, line) in lines.iter().enumerate() {
            let line_no = line_index + 1;
            let location = tree.location(path, line_no);
            let words = identifiers_in(line);
            for word in &words {
                let lower = word.to_ascii_lowercase();
                if !KEYWORDS.contains(&lower.as_str()) && !word.bytes().all(|b| b.is_ascii_digit())
                {
                    identifiers.insert(lower);
                }
            }

            let heads = statement_heads(line);
            for word in &heads {
                let lower = word.to_ascii_lowercase();
                if !KEYWORDS.contains(&lower.as_str()) {
                    record(&mut report.unknown_command_heads, &lower, &location);
                }
            }

            if let Some(head) = heads.first().map(|word| word.to_ascii_lowercase())
                && matches!(head.as_str(), "appendevent" | "onevent")
            {
                if let Some(event) = words.get(1) {
                    let event = event.to_ascii_uppercase();
                    report.event_occurrences += 1;
                    let family = match event.as_str() {
                        "EVENT_WEAPKEY2" | "EVENT_WEAPKEY4" => "EVENT_WEAPKEY",
                        _ => event.as_str(),
                    };
                    record(&mut report.unknown_events, family, &location);
                }
            }

            if heads.iter().any(|word| {
                matches!(
                    word.to_ascii_lowercase().as_str(),
                    "geta"
                        | "seta"
                        | "getactor"
                        | "setactor"
                        | "gets"
                        | "sets"
                        | "getsector"
                        | "setsector"
                        | "getwall"
                        | "setwall"
                        | "getp"
                        | "setp"
                        | "getplayer"
                        | "setplayer"
                        | "getinput"
                        | "setinput"
                        | "gettspr"
                        | "settspr"
                        | "getprojectile"
                        | "setprojectile"
                        | "getthisprojectile"
                        | "setthisprojectile"
                        | "getuserdef"
                        | "setuserdef"
                        | "gettiledata"
                        | "settiledata"
                )
            }) {
                for member in dotted_members(line) {
                    record(
                        &mut report.referenced_members,
                        &member.to_ascii_lowercase(),
                        &location,
                    );
                }
            }
        }
    }
    report.distinct_non_atomic_identifiers = identifiers.len();
    report.includes_resolved = tree.resolved_includes.len();
    report.includes_missing = tree.missing_includes.len();
    report.missing_include_names = tree.missing_includes.iter().cloned().collect();

    for path in &tree.def_files {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let source = String::from_utf8_lossy(&bytes);
        match DefDocument::parse(&source) {
            Ok(document) => match document.high_res_tiles() {
                Ok(tiles) => {
                    report.def_high_res_tiles += tiles.len();
                    for tile in tiles {
                        let asset_name = tile.file.replace('\\', "/");
                        let asset_path = tree.root.join(&asset_name);
                        match fs::read(&asset_path) {
                            Ok(data) => match room_build::def::decode_tile_image(&data) {
                                Ok(_) => report.def_tile_images_decoded += 1,
                                Err(error) => report.def_tile_image_errors.push(format!(
                                    "{}:{} (`{}`): {error}",
                                    path.display(),
                                    tile.line,
                                    asset_name
                                )),
                            },
                            Err(error) => report.def_tile_image_errors.push(format!(
                                "{}:{} (`{}`): {error}",
                                path.display(),
                                tile.line,
                                asset_name
                            )),
                        }
                    }
                }
                Err(error) => report
                    .def_parse_errors
                    .push(format!("{}: {error}", path.display())),
            },
            Err(error) => report
                .def_parse_errors
                .push(format!("{}: {error}", path.display())),
        }
        if let Ok(document) = DefDocument::parse(&source)
            && let Err(error) = document.tile_operations()
        {
            report
                .def_parse_errors
                .push(format!("{}: {error}", path.display()));
        }
        let lines = clean_lines(&source);
        let mut brace_depth = 0i32;
        for (line_index, line) in lines.iter().enumerate() {
            let line_no = line_index + 1;
            let location = tree.location(path, line_no);
            let heads = statement_heads(line);
            for word in &heads {
                let lower = word.to_ascii_lowercase();
                *report
                    .def_statement_counts
                    .entry(lower.clone())
                    .or_default() += 1;
                if lower != "include" {
                    record(&mut report.unknown_def_tokens, &lower, &location);
                }
            }
            for byte in line.bytes() {
                match byte {
                    b'{' => brace_depth += 1,
                    b'}' => {
                        brace_depth -= 1;
                        if brace_depth < 0 {
                            report
                                .def_brace_errors
                                .push(format!("{location}: unmatched `}}`"));
                            brace_depth = 0;
                        }
                    }
                    _ => {}
                }
            }
        }
        report.def_lines += lines.len();
        if brace_depth > 0 {
            report.def_brace_errors.push(format!(
                "{}: {brace_depth} unclosed `{{` block(s)",
                tree.location(path, lines.len().max(1))
            ));
        }
    }

    for path in &tree.def_files {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let source = String::from_utf8_lossy(&bytes);
        match DefDocument::parse(&source)
            .and_then(|document| room_build::map_hack::map_hack_infos(&document.directives))
        {
            Ok(infos) => report.map_hack_entries += infos.len(),
            Err(error) => report
                .def_parse_errors
                .push(format!("{}: {error}", path.display())),
        }
    }
    report.mhk_files = tree.mhk_files.len();
    for path in &tree.mhk_files {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let source = String::from_utf8_lossy(&bytes);
        match MapHackDocument::parse(&source) {
            Ok(document) => report.mhk_commands += document.commands.len(),
            Err(error) => report
                .mhk_parse_errors
                .push(format!("{}: {error}", path.display())),
        }
    }

    Ok(report.markdown(dialect))
}

/// Compile a self-contained EDuke32 CON fixture and collect its `echo` output.
pub fn run_con_reference(path: &Path, event: &str) -> Result<Vec<String>> {
    let source = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let image = compile_with_options(
        &source,
        &mut NullIncludes,
        ConCompileOptions {
            dialect: ConDialect::EDuke32,
        },
    );
    if image.errors != 0 {
        bail!(
            "{} has {} EDuke32 CON compile errors: {:?}",
            path.display(),
            image.errors,
            image.eduke_error_heads
        );
    }
    if !image
        .eduke_event_entries
        .iter()
        .any(|(name, _, _)| name.eq_ignore_ascii_case(event))
    {
        bail!("{} does not register event `{event}`", path.display());
    }
    let mut state = EdukeVmState::from_image(&image)?;
    state.execute_registered_event_hooks(&image, event, 0, 0, 1_000_000, 64)?;
    Ok(state.take_console_output())
}

/// Compare captured EDuke32 console/log text against Room's ordered echo lines.
pub fn verify_con_reference(actual: &[String], reference: &str) -> Result<()> {
    let expected = reference
        .lines()
        .filter_map(|line| {
            line.find("ED2REF|")
                .map(|index| line[index..].trim_end().to_owned())
        })
        .collect::<Vec<_>>();
    if expected.is_empty() {
        bail!("reference dump contains no `ED2REF|` lines");
    }
    if actual != expected {
        let mismatch = actual
            .iter()
            .zip(&expected)
            .position(|(actual, expected)| actual != expected)
            .unwrap_or(actual.len().min(expected.len()));
        bail!(
            "EDuke32 reference mismatch at line {}: Room {:?}, reference {:?} ({} Room lines, {} reference lines)",
            mismatch + 1,
            actual.get(mismatch),
            expected.get(mismatch),
            actual.len(),
            expected.len()
        );
    }
    Ok(())
}

fn collect_files(dir: &Path, depth: usize, output: &mut Vec<PathBuf>) -> Result<()> {
    if depth > 64 {
        bail!("source tree is nested too deeply at {}", dir.display());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("read directory {}", dir.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", dir.display()))?;
        let kind = entry
            .file_type()
            .with_context(|| format!("stat {}", entry.path().display()))?;
        if kind.is_dir() {
            collect_files(&entry.path(), depth + 1, output)?;
        } else if kind.is_file() {
            output.push(entry.path());
        }
    }
    Ok(())
}

fn extension_is(path: &Path, extension: &str) -> bool {
    path.extension()
        .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case(extension))
}

fn file_name_key(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn normalize_include(name: &str) -> String {
    name.trim_matches(['"', '\''])
        .replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
        .to_ascii_lowercase()
}

fn clean_lines(source: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_block = false;
    for raw in source.lines() {
        let chars = raw.chars().collect::<Vec<_>>();
        let mut line = String::with_capacity(raw.len());
        let mut at = 0;
        let mut in_string = false;
        let mut escaped = false;
        while at < chars.len() {
            let c = chars[at];
            let next = chars.get(at + 1).copied();
            if in_block {
                if c == '*' && next == Some('/') {
                    in_block = false;
                    at += 2;
                } else {
                    at += 1;
                }
                continue;
            }
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
                line.push(' ');
                at += 1;
                continue;
            }
            if c == '"' {
                in_string = true;
                line.push(' ');
                at += 1;
                continue;
            }
            if c == '/' && next == Some('/') {
                break;
            }
            if c == '/' && next == Some('*') {
                in_block = true;
                at += 2;
                continue;
            }
            line.push(c);
            at += 1;
        }
        lines.push(line);
    }
    lines
}

fn identifiers_in(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut words = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let start = at;
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            words.push(String::from_utf8_lossy(&bytes[start..at]).into_owned());
        } else {
            at += 1;
        }
    }
    words
}

fn statement_heads(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut heads = Vec::new();
    let mut at = 0;
    let mut statement_start = true;
    while at < bytes.len() {
        match bytes[at] {
            b'{' | b';' => {
                statement_start = true;
                at += 1;
            }
            b'}' => {
                statement_start = true;
                at += 1;
            }
            b if b.is_ascii_whitespace() => at += 1,
            b if b.is_ascii_alphabetic() || b == b'_' => {
                let start = at;
                at += 1;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                if statement_start {
                    let is_label = bytes.get(at) == Some(&b':');
                    if !is_label {
                        heads.push(String::from_utf8_lossy(&bytes[start..at]).into_owned());
                    }
                    statement_start = false;
                }
            }
            _ => at += 1,
        }
    }
    heads
}

fn dotted_members(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut members = Vec::new();
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at] == b'.' && (bytes[at + 1].is_ascii_alphabetic() || bytes[at + 1] == b'_') {
            let start = at + 1;
            at = start + 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            members.push(String::from_utf8_lossy(&bytes[start..at]).into_owned());
        } else {
            at += 1;
        }
    }
    members
}

fn record(hits: &mut BTreeMap<String, Hit>, token: &str, location: &str) {
    hits.entry(token.to_owned())
        .or_default()
        .record(location.to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_keeps_statement_heads_and_locations_after_comments() {
        let lines = clean_lines(
            "// set ignored 1\n/* comment */ appendevent EVENT_GAME\nifvarn FOO 1 { setvar FOO 2 }\n",
        );
        assert!(statement_heads(&lines[0]).is_empty());
        assert_eq!(statement_heads(&lines[1]), ["appendevent"]);
        assert_eq!(statement_heads(&lines[2]), ["ifvarn", "setvar"]);
        assert_eq!(dotted_members("geta[THISACTOR].htflags value"), ["htflags"]);
    }

    #[test]
    fn ed2_reference_fixture_echoes_repeatable_vm_values() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("testdata/ed2-vm-reference.con");
        let output = run_con_reference(&fixture, "EVENT_INIT").expect("reference fixture runs");
        assert_eq!(
            output,
            [
                "ED2REF|wrap|-2147483648",
                "ED2REF|division|-2",
                "ED2REF|comparison|9",
                "ED2REF|loop|3",
                "ED2REF|switch|20",
                "ED2REF|state-order|111",
                "ED2REF|sqrt|4",
                "ED2REF|hypotenuse|5",
                "ED2REF|random|196",
                "ED2REF|randomseed|-775671331",
            ]
        );
    }

    #[test]
    fn ed2_reference_comparison_filters_engine_log_prefixes_and_reports_mismatches() {
        let actual = vec!["ED2REF|wrap|-2147483648".to_owned()];
        assert!(
            verify_con_reference(
                &actual,
                "startup noise\n[EDuke32] ED2REF|wrap|-2147483648\n"
            )
            .is_ok()
        );
        assert!(verify_con_reference(&actual, "ED2REF|wrap|2147483647\n").is_err());
        assert!(verify_con_reference(&actual, "no labeled output\n").is_err());
    }

    #[test]
    fn brutal_duke_gated() {
        let Ok(root) = std::env::var("ROOM_BRUTALDUKE_DIR") else {
            return;
        };
        let report = run_mod_check(Path::new(&root), ConDialect::EDuke32).expect("mod report");
        assert!(report.contains("Dialect: EDuke32"));
        assert!(report.contains("Non-Atomic command heads"));
        assert!(report.contains("Referenced struct members"));
        assert!(report.contains("Referenced event names"));
        assert!(report.contains("DEF statement heads"));
        assert!(report.contains("Brutal Duke CON files/lines: 48/34100"));
        assert!(report.contains("include names resolved/unresolved: 49/0"));
        assert!(report.contains("remaining compile errors: 0"));
        assert!(report.contains(
            "EDuke32 metadata parsed: 1696 vars / 22 arrays / 703 states / 125 event hooks / 273 projectile properties / 68 damage-event tiles"
        ));
        assert!(report.contains(
            "EDuke32 executable-core instructions: 25648 (176 explicit no-op instructions"
        ));
        assert!(report.contains("- `resetplayer`: 4"));
        assert!(
            report.contains("EDuke32 declaration storage: initialized (1697 variables, 22 arrays)")
        );
        assert!(report.contains("event blocks: 126"));
        assert!(report.contains("Referenced event names: 36 distinct / 126 occurrences"));
        assert!(report.contains("Non-Atomic command heads: 132 distinct / 17840 occurrences"));
        assert!(report.contains("`defineprojectile` (273)"));
        assert!(report.contains("`damageeventtile` (68)"));
        assert!(report.contains("`EVENT_GAME`"));
        assert!(report.contains("Referenced struct members: 132 distinct / 1527 occurrences"));
        assert!(report.contains("files: 28; lines: 3737"));
        assert!(report.contains("DEF statement heads: 39 distinct / 3844 occurrences"));
        assert!(report.contains("`tilefromtexture`: 393"));
        assert!(report.contains("structural brace issues: 0"));
    }

    #[test]
    fn brutal_duke_box_map_game_and_spawn_hooks_do_not_fault() {
        let Ok(root) = std::env::var("ROOM_BRUTALDUKE_DIR") else {
            return;
        };
        let root = Path::new(&root);
        let mut tree = SourceTree::load(root).expect("load Brutal Duke source tree");
        let root_con = tree.root_con().expect("find the root CON").to_path_buf();
        let source = fs::read(&root_con).expect("read the root CON");
        let image = compile_with_options(
            &source,
            &mut tree,
            ConCompileOptions {
                dialect: ConDialect::EDuke32,
            },
        );
        assert_eq!(image.errors, 0, "errors={:?}", image.eduke_error_heads);
        let expected_hook_count = |event: &str| {
            image
                .eduke_event_entries
                .iter()
                .filter(|(name, _, _)| name.eq_ignore_ascii_case(event))
                .count()
        };

        let map_bytes = fs::read(root.join("maps/box.map")).expect("read box.map");
        let world =
            room_build_engine::BuildWorld::load_map_bytes(&map_bytes).expect("load box.map");
        let mut session = room_duke::DukeSession::new();
        session
            .try_load_script(image.clone())
            .expect("initialize EDuke32 runtime state");
        session.set_world_with_skill(world, 2);
        let map_hook_errors = session.take_eduke_runtime_errors();
        assert!(
            map_hook_errors.is_empty(),
            "map-load EDuke32 hooks faulted: {map_hook_errors:?}"
        );
        let actor = session
            .lists
            .sprite_stat
            .iter()
            .position(|status| *status != room_build_engine::MAXSTATUS as i16)
            .expect("box.map has a live sprite");
        let game_executions = session
            .execute_eduke_event("EVENT_GAME", 0, actor, 2_000_000, 64)
            .unwrap_or_else(|error| panic!("EVENT_GAME hook fault: {error}"));
        assert!(
            !game_executions.is_empty(),
            "EVENT_GAME hooks are registered"
        );
        let expected_game_hooks = expected_hook_count("EVENT_GAME");
        assert!(expected_game_hooks > 0, "EVENT_GAME hooks are registered");
        assert!(
            game_executions.len() == expected_game_hooks
                || game_executions
                    .last()
                    .is_some_and(|execution| execution.returned),
            "all hooks run unless one explicitly returns: {game_executions:?}"
        );

        let expected_spawn_hooks = expected_hook_count("EVENT_SPAWN");
        assert!(expected_spawn_hooks > 0, "EVENT_SPAWN hooks are registered");
    }
}

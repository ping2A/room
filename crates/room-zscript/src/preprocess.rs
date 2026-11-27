//! `#include` and `version` header handling.

use crate::diag::{Diagnostic, Severity};
use crate::span::{SourceFile, SourceId, Span};
use std::collections::BTreeMap;

/// Result of preprocessing a translation unit.
#[derive(Debug, Default)]
pub struct PreprocessResult {
    /// Flattened source text.
    pub text: String,
    /// File map for spans (logical — simple single-file for now with includes inlined).
    pub files: Vec<SourceFile>,
    /// Declared version from the first `version "x.y"` if any.
    pub version: Option<String>,
    /// Diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// For each line of `text`: index into `files` and 1-based line there.
    pub line_map: Vec<(u32, u32)>,
}

impl PreprocessResult {
    /// File and 1-based line of flattened line `line` (1-based).
    #[must_use]
    pub fn origin(&self, line: u32) -> Option<(&str, u32)> {
        let (file, l) = *self.line_map.get(line.checked_sub(1)? as usize)?;
        Some((self.files.get(file as usize)?.name.as_str(), l))
    }
}

/// Resolve `#include "path"` via a lookup closure (VFS / embedded).
pub fn preprocess(
    entry_name: &str,
    entry_text: &str,
    mut resolve: impl FnMut(&str) -> Option<String>,
) -> PreprocessResult {
    let mut out = PreprocessResult::default();
    // `(name, text, remainder, first line)`. A remainder is the tail of a file
    // after an include, so it is not recorded as a new source.
    let mut stack = vec![(entry_name.to_owned(), entry_text.to_owned(), false, 1u32)];
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    let mut file_id = 0u32;

    while let Some((name, text, remainder, first_line)) = stack.pop() {
        let id = if remainder {
            // The includer's own id (not the file it just included).
            out.files
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(&name))
                .map_or(SourceId(0), |f| f.id)
        } else {
            let key = name.to_ascii_lowercase();
            if seen.contains_key(&key) {
                continue;
            }
            seen.insert(key, ());
            let id = SourceId(file_id);
            file_id += 1;
            out.files.push(SourceFile {
                name: name.clone(),
                text: text.clone(),
                id,
            });
            id
        };

        let lines: Vec<&str> = text.lines().collect();
        let file_ix = out
            .files
            .iter()
            .position(|f| f.id == id)
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or(0);
        // `/* … #include "x" … */`: commented-out includes stay text.
        let mut in_block = false;
        for (line_no, line) in lines.iter().copied().enumerate() {
            let trimmed = line.trim();
            let commented = in_block;
            in_block = block_comment_after(line, in_block);
            let here = first_line + line_no as u32;
            if commented {
                out.text.push_str(line);
                out.text.push('\n');
                out.line_map.push((file_ix, here));
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim();
                if let Some(v) = split_string_lit(rest) {
                    if out.version.is_none() {
                        out.version = Some(v);
                    }
                    // Keep the line so the parser can see it too.
                    out.text.push_str(line);
                    out.text.push('\n');
                    out.line_map.push((file_ix, here));
                    continue;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("#include") {
                let rest = rest.trim();
                if let Some(raw) = split_string_lit(rest) {
                    let path = resolve_include(&name, &raw);
                    match resolve(&path) {
                        Some(included) => {
                            let rest_text = lines[line_no + 1..].join("\n");
                            if !rest_text.is_empty() {
                                stack.push((name.clone(), rest_text, true, here + 1));
                            }
                            // Include body is expanded here, then the parent tail.
                            stack.push((path, included, false, 1));
                            break;
                        }
                        None => out.diagnostics.push(Diagnostic {
                            severity: Severity::Error,
                            message: format!("cannot #include \"{path}\""),
                            span: Span {
                                source: id,
                                start: 0,
                                end: 0,
                                line: here,
                                col: 1,
                            },
                            path: name.clone(),
                        }),
                    }
                    continue;
                }
            }
            out.text.push_str(line);
            out.text.push('\n');
            out.line_map.push((file_ix, here));
        }
    }
    out
}

/// Whether a `/* */` comment is still open after `line`, given whether one was
/// open before it. Strings and `//` comments are skipped.
fn block_comment_after(line: &str, mut open: bool) -> bool {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if open {
            if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                open = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => return false,
            b'/' if b.get(i + 1) == Some(&b'*') => {
                open = true;
                i += 2;
            }
            _ => i += 1,
        }
    }
    open
}

/// `#include` target as an archive path. `./x` and `../x` are relative to the
/// including file (GZDoom 4.x); anything else is relative to the archive root.
#[must_use]
pub fn resolve_include(includer: &str, target: &str) -> String {
    let target = target.replace('\\', "/");
    if !(target.starts_with("./") || target.starts_with("../")) {
        return target.trim_start_matches('/').to_owned();
    }
    let includer = includer.replace('\\', "/");
    let mut parts: Vec<&str> = includer.split('/').collect();
    parts.pop(); // file name
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.retain(|p| !p.is_empty());
    parts.join("/")
}

/// `#include` targets of `text` (a file at `path`), resolved like
/// [`preprocess`] does and skipping commented-out lines.
#[must_use]
pub fn include_targets(path: &str, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        let commented = in_block;
        in_block = block_comment_after(line, in_block);
        if commented {
            continue;
        }
        let Some(rest) = line.trim().strip_prefix("#include") else {
            continue;
        };
        if let Some(raw) = split_string_lit(rest.trim()) {
            let p = resolve_include(path, &raw);
            if !p.is_empty() {
                out.push(p);
            }
        }
    }
    out
}

fn split_string_lit(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches(';').trim();
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn version_and_include() {
        let r = preprocess(
            "a.zs",
            "version \"3.2.3\"\n#include \"b.zs\"\nclass A {}\n",
            |p| {
                if p == "b.zs" {
                    Some("class B {}\n".into())
                } else {
                    None
                }
            },
        );
        assert_eq!(r.version.as_deref(), Some("3.2.3"));
        assert!(r.text.contains("class A"));
        assert!(r.text.contains("class B"));
        let b = r.text.find("class B").unwrap();
        let a = r.text.find("class A").unwrap();
        assert!(b < a, "include is expanded at the #include site");
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn relative_and_commented_includes() {
        assert_eq!(
            resolve_include("zscript/ZJSON/Include.zs", "./JsonBase.zs"),
            "zscript/ZJSON/JsonBase.zs"
        );
        assert_eq!(
            resolve_include("zscript/ui/pbtk/pbtk.zs", "../x.zs"),
            "zscript/ui/x.zs"
        );
        assert_eq!(
            resolve_include("zscript/a.zs", "zscript/b.zs"),
            "zscript/b.zs"
        );
        let root = "#include \"zscript/ZJSON/Include.zs\"\n/*#include \"gone/a.zs\"\n#include \"gone/b.zs\"*/\nclass R {}\n";
        assert_eq!(
            include_targets("ZSCRIPT.zc", root),
            ["zscript/ZJSON/Include.zs"]
        );
        let mut asked = Vec::new();
        let out = preprocess("ZSCRIPT.zc", root, |p| {
            asked.push(p.to_owned());
            (p == "zscript/ZJSON/Include.zs")
                .then(|| "#include \"./JsonBase.zs\"\n".to_owned())
                .or_else(|| (p == "zscript/ZJSON/JsonBase.zs").then(|| "class J {}\n".to_owned()))
        });
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(
            asked,
            ["zscript/ZJSON/Include.zs", "zscript/ZJSON/JsonBase.zs"]
        );
        assert!(out.text.contains("class J") && out.text.contains("class R"));
    }

    #[test]
    fn line_map_points_into_included_files() {
        let root = "version \"4.0\"\n#include \"b.zs\"\nclass A {}\n";
        let out = preprocess("zscript.zs", root, |p| {
            (p == "b.zs").then(|| "// b\nclass B {}\n".to_owned())
        });
        let lines: Vec<&str> = out.text.lines().collect();
        let b = lines.iter().position(|l| l.contains("class B")).unwrap() as u32 + 1;
        let a = lines.iter().position(|l| l.contains("class A")).unwrap() as u32 + 1;
        assert_eq!(out.origin(b), Some(("b.zs", 2)));
        assert_eq!(out.origin(a), Some(("zscript.zs", 3)));
        assert_eq!(out.origin(1), Some(("zscript.zs", 1)));
    }
}

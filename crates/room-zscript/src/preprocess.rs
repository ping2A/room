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
}

/// Resolve `#include "path"` via a lookup closure (VFS / embedded).
pub fn preprocess(
    entry_name: &str,
    entry_text: &str,
    mut resolve: impl FnMut(&str) -> Option<String>,
) -> PreprocessResult {
    let mut out = PreprocessResult::default();
    // `(name, text, remainder)`. A remainder is the tail of a file after an
    // include, so it is not recorded as a new source.
    let mut stack = vec![(entry_name.to_owned(), entry_text.to_owned(), false)];
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    let mut file_id = 0u32;

    while let Some((name, text, remainder)) = stack.pop() {
        let id = if remainder {
            out.files.last().map(|f| f.id).unwrap_or(SourceId(0))
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
        for (line_no, line) in lines.iter().copied().enumerate() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim();
                if let Some(v) = split_string_lit(rest) {
                    if out.version.is_none() {
                        out.version = Some(v);
                    }
                    // Keep the line so the parser can see it too.
                    out.text.push_str(line);
                    out.text.push('\n');
                    continue;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("#include") {
                let rest = rest.trim();
                if let Some(path) = split_string_lit(rest) {
                    match resolve(&path) {
                        Some(included) => {
                            let rest_text = lines[line_no + 1..].join("\n");
                            if !rest_text.is_empty() {
                                stack.push((name.clone(), rest_text, true));
                            }
                            // Include body is expanded here, then the parent tail.
                            stack.push((path, included, false));
                            break;
                        }
                        None => out.diagnostics.push(Diagnostic {
                            severity: Severity::Error,
                            message: format!("cannot #include \"{path}\""),
                            span: Span {
                                source: id,
                                start: 0,
                                end: 0,
                                line: (line_no as u32) + 1,
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
}

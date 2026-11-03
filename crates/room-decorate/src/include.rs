//! `#include` preprocessing for DECORATE (GZDoom-compatible subset).

use crate::error::DecorateError;
use crate::sourcemap::{ExpandedSource, SourceLine, SourceMap};
use std::collections::BTreeSet;

/// Expand `#include "path"` / `#include <path>` lines by calling `resolve`.
///
/// Nested includes are supported; cycles error. Lines that are only whitespace
/// or `//` comments before `#include` are still treated as include directives
/// when the first non-space token is `#include`.
pub fn expand_includes(
    src: &str,
    resolve: impl FnMut(&str) -> Result<String, DecorateError>,
) -> Result<String, DecorateError> {
    Ok(expand_includes_mapped(src, resolve)?.text)
}

/// Expand includes and retain a per-line source map (BD1).
pub fn expand_includes_mapped(
    src: &str,
    mut resolve: impl FnMut(&str) -> Result<String, DecorateError>,
) -> Result<ExpandedSource, DecorateError> {
    let mut map = SourceMap::new();
    let text = expand_inner(src, &mut resolve, &mut BTreeSet::new(), "<root>", &mut map)?;
    Ok(ExpandedSource { text, map })
}

fn expand_inner(
    src: &str,
    resolve: &mut dyn FnMut(&str) -> Result<String, DecorateError>,
    stack: &mut BTreeSet<String>,
    from: &str,
    map: &mut SourceMap,
) -> Result<String, DecorateError> {
    let mut out = String::with_capacity(src.len());
    for (lineno0, line) in src.lines().enumerate() {
        let orig_line = (lineno0 + 1) as u32;
        let trimmed = line.trim_start();
        if let Some(path) = parse_include_line(trimmed) {
            if !stack.insert(path.to_owned()) {
                return Err(DecorateError::Message(format!(
                    "include cycle involving {path:?} (from {from}:{orig_line})"
                )));
            }
            let nested = resolve(path)?;
            let expanded = expand_inner(&nested, resolve, stack, path, map)?;
            stack.remove(path);
            out.push_str(&expanded);
            if !expanded.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
            out.push('\n');
            map.lines.push(SourceLine {
                file: from.to_owned(),
                line: orig_line,
            });
        }
    }
    Ok(out)
}

fn parse_include_line(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("#include")?.trim_start();
    if let Some(inner) = rest.strip_prefix('"') {
        let end = inner.find('"')?;
        return Some(&inner[..end]);
    }
    if let Some(inner) = rest.strip_prefix('<') {
        let end = inner.find('>')?;
        return Some(&inner[..end]);
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn expands_nested_includes() {
        let mut files = BTreeMap::new();
        files.insert(
            "root.txt",
            "#include \"a.txt\"\nactor Root : Actor {}\n".to_owned(),
        );
        files.insert("a.txt", "#include \"b.txt\"\n// mid\n".to_owned());
        files.insert("b.txt", "actor Leaf : Actor {}\n".to_owned());
        let expanded = expand_includes(&files["root.txt"], |p| {
            files
                .get(p)
                .cloned()
                .ok_or_else(|| DecorateError::Message(format!("missing {p}")))
        })
        .unwrap();
        assert!(expanded.contains("actor Leaf"));
        assert!(expanded.contains("actor Root"));
        assert!(!expanded.contains("#include"));
    }

    #[test]
    fn mapped_expand_preserves_origins() {
        let mut files = BTreeMap::new();
        files.insert(
            "root.txt",
            "#include \"child.txt\"\nactor Root {}\n".to_owned(),
        );
        files.insert("child.txt", "actor Child {}\n".to_owned());
        let exp = expand_includes_mapped(&files["root.txt"], |p| {
            files
                .get(p)
                .cloned()
                .ok_or_else(|| DecorateError::Message(format!("missing {p}")))
        })
        .unwrap();
        // First content line is from child.txt:1
        let loc = exp.map.lookup(1).unwrap();
        assert_eq!(loc.file, "child.txt");
        assert_eq!(loc.line, 1);
        // Root actor line follows
        let mut found_root = false;
        for (i, line) in exp.text.lines().enumerate() {
            if line.contains("actor Root") {
                let loc = exp.map.lookup((i + 1) as u32).unwrap();
                assert_eq!(loc.file, "<root>");
                found_root = true;
            }
        }
        assert!(found_root);
    }

    #[test]
    fn detects_cycles() {
        let err = expand_includes("#include \"a.txt\"\n", |p| {
            assert_eq!(p, "a.txt");
            Ok("#include \"a.txt\"\n".into())
        })
        .unwrap_err();
        assert!(format!("{err}").contains("cycle"));
    }
}

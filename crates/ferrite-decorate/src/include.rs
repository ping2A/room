//! `#include` preprocessing for DECORATE (GZDoom-compatible subset).

use crate::error::DecorateError;
use std::collections::BTreeSet;

/// Expand `#include "path"` / `#include <path>` lines by calling `resolve`.
///
/// Nested includes are supported; cycles error. Lines that are only whitespace
/// or `//` comments before `#include` are still treated as include directives
/// when the first non-space token is `#include`.
pub fn expand_includes(
    src: &str,
    mut resolve: impl FnMut(&str) -> Result<String, DecorateError>,
) -> Result<String, DecorateError> {
    expand_inner(src, &mut resolve, &mut BTreeSet::new(), "<root>")
}

fn expand_inner(
    src: &str,
    resolve: &mut dyn FnMut(&str) -> Result<String, DecorateError>,
    stack: &mut BTreeSet<String>,
    from: &str,
) -> Result<String, DecorateError> {
    let mut out = String::with_capacity(src.len());
    for (lineno, line) in src.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(path) = parse_include_line(trimmed) {
            if !stack.insert(path.to_owned()) {
                return Err(DecorateError::Message(format!(
                    "include cycle involving {path:?} (from {from}:{lineno})"
                )));
            }
            let nested = resolve(path)?;
            let expanded = expand_inner(&nested, resolve, stack, path)?;
            stack.remove(path);
            out.push_str(&expanded);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
            out.push('\n');
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
    fn detects_cycles() {
        let err = expand_includes("#include \"a.txt\"\n", |p| {
            if p == "a.txt" {
                Ok("#include \"a.txt\"\n".into())
            } else {
                Err(DecorateError::Message("no".into()))
            }
        })
        .unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }
}

//! ZScript front end (`docs/34-zscript-vm.md`).

#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![allow(clippy::module_name_repetitions)]

mod ast;
mod diag;
mod lex;
mod lower;
mod parse;
mod preprocess;
mod sema;
mod span;
mod specials;
mod static_arrays;
mod types;

pub use ast::*;
pub use diag::{Diagnostic, Severity};
pub use lex::{Token, TokenKind, lex};
pub use lower::{lower, lower_lenient};
pub use parse::{ParseResult, RootOrigins, parse_file, parse_sources};
pub use preprocess::{PreprocessResult, include_targets, preprocess, resolve_include};
pub use sema::{Analysis, analyze};
pub use span::{SourceFile, SourceId, Span};
pub use specials::special_number;
pub use types::{ClassTable, Conv, ZType, coerce};

/// Room stdlib stubs (`wadsrc/room/zscript/base.zs`) — BD / EventHandler virtuals.
const ROOM_STDLIB: &str = include_str!("../../../wadsrc/room/zscript/base.zs");
/// Play-scope names (`docs/34` §14.3.2).
const ROOM_GLOBALS: &str = include_str!("../../../wadsrc/room/zscript/globals.zs");
/// `String` methods used by Brutal Doom and the conformance probe.
const ROOM_BUILTINS: &str = include_str!("../../../wadsrc/room/zscript/builtins.zs");

/// True when `text` is one of Room's own stub library files (`wadsrc/room/zscript`),
/// e.g. mounted from `room.pk3`. They clash with the UZDoom library.
#[must_use]
pub fn is_room_stdlib_source(text: &str) -> bool {
    [ROOM_STDLIB, ROOM_GLOBALS, ROOM_BUILTINS]
        .iter()
        .any(|lib| lib.trim() == text.trim())
}

/// Append Room stdlib unless the unit already provides `base.zs` / `EventHandler`.
///
/// Kept **after** the caller's sources so `sources[0]` remains the `#include`
/// entry for [`parse_sources`] (GZDoom translation-unit style).
#[must_use]
pub fn with_room_stdlib(sources: &[(String, String)]) -> Vec<(String, String)> {
    let already = sources.iter().any(|(path, text)| {
        let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
        base.eq_ignore_ascii_case("base.zs") || text.contains("class EventHandler")
    });
    if already {
        return sources.to_vec();
    }
    if sources.is_empty() {
        return vec![
            (
                "wadsrc/room/zscript/globals.zs".into(),
                ROOM_GLOBALS.to_owned(),
            ),
            (
                "wadsrc/room/zscript/builtins.zs".into(),
                ROOM_BUILTINS.to_owned(),
            ),
            ("wadsrc/room/zscript/base.zs".into(), ROOM_STDLIB.to_owned()),
        ];
    }
    let mut out = sources.to_vec();
    out.push((
        "wadsrc/room/zscript/globals.zs".into(),
        ROOM_GLOBALS.to_owned(),
    ));
    out.push((
        "wadsrc/room/zscript/builtins.zs".into(),
        ROOM_BUILTINS.to_owned(),
    ));
    out.push(("wadsrc/room/zscript/base.zs".into(), ROOM_STDLIB.to_owned()));
    out
}

/// High-level check used by `room-cli zscript check`.
///
/// Always type-checks against Room's BD-subset stdlib stubs (ZS1) unless the
/// caller already included an equivalent `base.zs`.
pub fn check(sources: &[(String, String)]) -> CheckResult {
    check_with(sources, false)
}

/// [`check`], but functions that do not lower run as empty bodies, each
/// reported as a warning ([`lower_lenient`]). Parse and type errors still fail.
pub fn check_lenient(sources: &[(String, String)]) -> CheckResult {
    check_with(sources, true)
}

fn check_with(sources: &[(String, String)], lenient: bool) -> CheckResult {
    let sources = with_room_stdlib(sources);
    let mut parsed = parse_sources(&sources);
    let mut diags = std::mem::take(&mut parsed.diagnostics);
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return CheckResult {
            diagnostics: diags,
            analysis: None,
            program: None,
        };
    }
    let located = |mut v: Vec<Diagnostic>| {
        for d in &mut v {
            parsed.locate(d);
        }
        v
    };
    let analysis = match analyze(&parsed.unit) {
        Ok(a) => {
            diags.extend(located(a.warnings.clone()));
            a
        }
        Err(more) => {
            diags.extend(located(more));
            return CheckResult {
                diagnostics: diags,
                analysis: None,
                program: None,
            };
        }
    };
    let lowered = if lenient {
        lower_lenient(&parsed.unit, &analysis)
    } else {
        lower(&parsed.unit, &analysis).map(|p| (p, Vec::new()))
    };
    let program = match lowered {
        Ok((p, stubbed)) => {
            diags.extend(located(stubbed));
            Some(p)
        }
        Err(more) => {
            diags.extend(located(more));
            None
        }
    };
    CheckResult {
        diagnostics: diags,
        analysis: Some(analysis),
        program,
    }
}

/// Result of [`check`].
#[derive(Debug, Default)]
pub struct CheckResult {
    /// Diagnostics gathered.
    pub diagnostics: Vec<Diagnostic>,
    /// Successful analysis (if any).
    pub analysis: Option<Analysis>,
    /// Compiled program (filled by later pipeline stages).
    pub program: Option<room_zvm::CompiledProgram>,
}

impl CheckResult {
    /// True when no error-severity diagnostics.
    #[must_use]
    pub fn ok(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_injects_stdlib_so_eventhandler_overrides_resolve() {
        let src = r#"
class BdCheckHandler : EventHandler
{
    override void WorldLoaded(WorldEvent e)
    {
    }
}
"#;
        let result = check(&[("handler.zs".into(), src.into())]);
        assert!(result.ok(), "{:?}", result.diagnostics);
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("WorldLoaded")),
            "WorldLoaded override should resolve via base.zs; diags={:?}",
            result.diagnostics
        );
        let a = result.analysis.as_ref().unwrap();
        assert!(
            a.classes.contains_key("EventHandler")
                || a.classes
                    .keys()
                    .any(|k| k.eq_ignore_ascii_case("EventHandler"))
        );
    }
}

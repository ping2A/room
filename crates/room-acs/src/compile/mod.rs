//! A small ACS compiler: source text → `ACSE` `BEHAVIOR` bytes that
//! [`crate::load_behavior`] and the VM run.
//!
//! It follows ACC: scripts (numbered or named, with types and flags),
//! functions, map / world / global variables and arrays (multi-dimensional
//! map arrays, local arrays), `#include` / `#define`, every Hexen / ZDoom
//! statement form, print items (`s:`, `d:`, `i:`, `c:`, `f:`, `n:`, `l:`,
//! `k:`, `b:`, `x:`), `HudMessage`, `StrParam`, `Log`, the internal
//! p-code functions, the line specials and the `CallFunc` functions.
//! The standard headers (`zcommon.acs`, `zdefs.acs`, `zspecial.acs`,
//! `zwvars.acs`) are built in. Libraries (`#import`) are not supported.

mod builtins;
mod codegen;
mod defs;
mod lex;
mod parse;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::fmt;

use lex::{Tok, Token};

/// Error position: index into the compiler's file list, and a line.
#[derive(Clone, Debug)]
pub(crate) struct Diag {
    pub file: u16,
    pub line: u32,
    pub message: String,
}

/// A compile error with its source position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    /// File the error is in (the main file's name, or an `#include` name).
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}

impl std::error::Error for CompileError {}

/// Headers whose contents the compiler already knows.
const BUILTIN_HEADERS: &[&str] = &["zcommon.acs", "zdefs.acs", "zspecial.acs", "zwvars.acs"];

/// Compile ACS `source` (named `file_name` in errors). `include` returns the
/// text of an `#include`d file, or `None` when it does not exist.
pub fn compile_acs(
    source: &str,
    file_name: &str,
    include: &mut dyn FnMut(&str) -> Option<String>,
) -> Result<Vec<u8>, CompileError> {
    compile_acs_module(source, file_name, include).map(|m| m.bytes)
}

/// A compiled module.
#[derive(Clone, Debug)]
pub struct CompiledAcs {
    /// `BEHAVIOR` bytes.
    pub bytes: Vec<u8>,
    /// `#library "NAME"`: the module is a library, loaded by name (`LOADACS`,
    /// `#import`).
    pub library: Option<String>,
    /// Libraries this module `#import`s.
    pub imports: Vec<String>,
}

/// Like [`compile_acs`], also telling whether the source is a `#library` and
/// which libraries it imports.
pub fn compile_acs_module(
    source: &str,
    file_name: &str,
    include: &mut dyn FnMut(&str) -> Option<String>,
) -> Result<CompiledAcs, CompileError> {
    let mut pre = Pre {
        files: vec![file_name.to_owned()],
        seen: HashSet::new(),
        defines: HashMap::new(),
        include,
    };
    let result = (|| -> Result<CompiledAcs, Diag> {
        let tokens = pre.file(source, 0, 0)?;
        let program = parse::parse(tokens)?;
        let bytes = codegen::generate(&program)?;
        Ok(CompiledAcs {
            bytes,
            library: program.library.clone(),
            imports: program.imports.clone(),
        })
    })();
    result.map_err(|d| CompileError {
        file: pre
            .files
            .get(usize::from(d.file))
            .cloned()
            .unwrap_or_default(),
        line: d.line,
        message: d.message,
    })
}

/// Compile ACS that includes nothing but the standard headers.
pub fn compile_acs_standalone(source: &str) -> Result<Vec<u8>, CompileError> {
    compile_acs(source, "SCRIPTS", &mut |_| None)
}

/// The preprocessor: splices `#include`s and expands `#define`s.
struct Pre<'a> {
    files: Vec<String>,
    seen: HashSet<String>,
    defines: HashMap<String, Vec<Token>>,
    include: &'a mut dyn FnMut(&str) -> Option<String>,
}

impl Pre<'_> {
    fn file(&mut self, src: &str, file: u16, depth: u32) -> Result<Vec<Token>, Diag> {
        let toks = lex::lex(src, file)?;
        let mut out = Vec::with_capacity(toks.len());
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            let line = t.line;
            let err = |message: String| Diag {
                file,
                line,
                message,
            };
            match &t.tok {
                Tok::Eof => break,
                Tok::Directive(d) => {
                    // The rest of the directive is the tokens on its line.
                    let mut j = i + 1;
                    while j < toks.len() && toks[j].line == line && toks[j].tok != Tok::Eof {
                        j += 1;
                    }
                    let args = &toks[i + 1..j];
                    i = j;
                    match d.as_str() {
                        "include" => {
                            let Some(Token {
                                tok: Tok::Str(name),
                                ..
                            }) = args.first()
                            else {
                                return Err(err("#include needs a file name in quotes".into()));
                            };
                            let base = name
                                .rsplit(['/', '\\'])
                                .next()
                                .unwrap_or(name)
                                .to_ascii_lowercase();
                            if BUILTIN_HEADERS.contains(&base.as_str()) {
                                continue;
                            }
                            let key = name.to_ascii_lowercase();
                            if !self.seen.insert(key) {
                                continue;
                            }
                            if depth > 16 {
                                return Err(err("#include nested too deeply".into()));
                            }
                            let Some(text) = (self.include)(name) else {
                                return Err(err(format!("can't find #include \"{name}\"")));
                            };
                            let id = u16::try_from(self.files.len())
                                .map_err(|_| err("too many files".into()))?;
                            self.files.push(name.clone());
                            out.extend(self.file(&text, id, depth + 1)?);
                        }
                        "define" | "libdefine" => {
                            let Some(Token {
                                tok: Tok::Ident(name),
                                ..
                            }) = args.first()
                            else {
                                return Err(err(format!("#{d} needs a name")));
                            };
                            if args.len() < 2 {
                                return Err(err(format!("#{d} {name} needs a value")));
                            }
                            self.defines
                                .insert(name.to_ascii_lowercase(), args[1..].to_vec());
                        }
                        "import" => {
                            let Some(Token {
                                tok: Tok::Str(name),
                                ..
                            }) = args.first()
                            else {
                                return Err(err("#import needs a file name in quotes".into()));
                            };
                            let base = name
                                .rsplit(['/', '\\'])
                                .next()
                                .unwrap_or(name)
                                .to_ascii_lowercase();
                            if BUILTIN_HEADERS.contains(&base.as_str())
                                || !self.seen.insert(name.to_ascii_lowercase())
                            {
                                continue;
                            }
                            if depth > 16 {
                                return Err(err("#import nested too deeply".into()));
                            }
                            let Some(text) = (self.include)(name) else {
                                return Err(err(format!("can't find #import \"{name}\"")));
                            };
                            let id = u16::try_from(self.files.len())
                                .map_err(|_| err("too many files".into()))?;
                            self.files.push(name.clone());
                            // The parser declares what is between these markers
                            // as imports from that library.
                            let mark = |d: &str| Token {
                                tok: Tok::Directive(d.into()),
                                file: id,
                                line: 1,
                            };
                            out.push(mark("importbegin"));
                            out.extend(self.file(&text, id, depth + 1)?);
                            out.push(mark("importend"));
                        }
                        "library" => {
                            let Some(
                                t @ Token {
                                    tok: Tok::Str(_), ..
                                },
                            ) = args.first()
                            else {
                                return Err(err("#library needs a name in quotes".into()));
                            };
                            out.push(Token {
                                tok: Tok::Directive("library".into()),
                                file,
                                line,
                            });
                            out.push(t.clone());
                        }
                        "pragma" | "nocompact" | "wadauthor" | "nowadauthor" | "encryptstrings" => {
                        }
                        other => return Err(err(format!("unknown directive #{other}"))),
                    }
                }
                Tok::Ident(name) if self.defines.contains_key(&name.to_ascii_lowercase()) => {
                    let mut stack = Vec::new();
                    self.expand(name, line, file, &mut out, &mut stack)?;
                    i += 1;
                }
                _ => {
                    out.push(t.clone());
                    i += 1;
                }
            }
        }
        if depth == 0 {
            let line = out.last().map_or(1, |t| t.line);
            out.push(Token {
                tok: Tok::Eof,
                file,
                line,
            });
        }
        Ok(out)
    }

    /// Expand a `#define` at the use site (`line`), recursively.
    fn expand(
        &self,
        name: &str,
        line: u32,
        file: u16,
        out: &mut Vec<Token>,
        stack: &mut Vec<String>,
    ) -> Result<(), Diag> {
        let key = name.to_ascii_lowercase();
        if stack.contains(&key) || stack.len() > 32 {
            return Err(Diag {
                file,
                line,
                message: format!("#define {name} refers to itself"),
            });
        }
        let Some(body) = self.defines.get(&key) else {
            out.push(Token {
                tok: Tok::Ident(name.to_owned()),
                file,
                line,
            });
            return Ok(());
        };
        stack.push(key);
        for t in body {
            match &t.tok {
                Tok::Ident(n) if self.defines.contains_key(&n.to_ascii_lowercase()) => {
                    self.expand(n, line, file, out, stack)?
                }
                tok => out.push(Token {
                    tok: tok.clone(),
                    file,
                    line,
                }),
            }
        }
        stack.pop();
        Ok(())
    }
}

/// A name the compiler knows, for editor completion and argument hints.
#[derive(Clone, Debug)]
pub struct AcsKnownName {
    /// Name as written in ACS (any letter case compiles).
    pub name: &'static str,
    /// `special`, `function` (internal), `callfunc`, `print` or `constant`.
    pub kind: &'static str,
    /// Line special number, `CallFunc` index or constant value.
    pub number: i32,
    /// Argument range; `None` when the compiler accepts any count.
    pub args: Option<(u8, u8)>,
}

/// Every built-in name: specials, internal functions, `CallFunc` functions,
/// print functions and constants.
#[must_use]
pub fn acs_known_names() -> Vec<AcsKnownName> {
    let mut out = Vec::new();
    for &(name, number, min, max) in defs::SPECIALS {
        let min = min.max(0) as u8;
        let max = if max < 0 { 5 } else { (max as u8).max(min) };
        out.push(AcsKnownName {
            name,
            kind: "special",
            number,
            args: Some((min, max)),
        });
    }
    for b in builtins::BUILTINS {
        out.push(AcsKnownName {
            name: b.name,
            kind: "function",
            number: b.pcode as i32,
            args: Some((b.min, b.argc)),
        });
    }
    for &(name, number) in defs::CALLFUNCS {
        out.push(AcsKnownName {
            name,
            kind: "callfunc",
            number,
            args: None,
        });
    }
    for name in [
        "Print",
        "PrintBold",
        "Log",
        "HudMessage",
        "HudMessageBold",
        "StrParam",
        "CreateTranslation",
        "ACS_ExecuteWait",
        "ACS_NamedExecuteWait",
    ] {
        out.push(AcsKnownName {
            name,
            kind: "print",
            number: 0,
            args: None,
        });
    }
    for &(name, number) in defs::CONSTANTS {
        out.push(AcsKnownName {
            name,
            kind: "constant",
            number,
            args: None,
        });
    }
    out
}

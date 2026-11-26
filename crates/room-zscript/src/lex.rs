//! ZScript lexer (GZDoom token set, case-insensitive keywords).

use crate::diag::{Diagnostic, Severity};
use crate::span::{SourceId, Span};

/// Token kind.
#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    /// End of file.
    Eof,
    /// Identifier (original spelling preserved; compare case-insensitively).
    Ident(String),
    /// Integer literal.
    Int(i64),
    /// Float literal.
    Float(f64),
    /// String literal (concat of adjacent already done by lexer consumer).
    String(String),
    /// Name literal `'Foo'`.
    Name(String),
    /// Keyword (lowercased).
    Kw(String),
    /// Punctuation / operator as string (e.g. `==`, `+=`, `...`).
    Punct(String),
}

/// Spanned token.
#[derive(Clone, Debug)]
pub struct Token {
    /// Kind.
    pub kind: TokenKind,
    /// Span.
    pub span: Span,
}

const KEYWORDS: &[&str] = &[
    "class",
    "struct",
    "enum",
    "const",
    "static",
    "native",
    "private",
    "protected",
    "internal",
    "readonly",
    "transient",
    "meta",
    "abstract",
    "extend",
    "mixin",
    "version",
    "deprecated",
    "ui",
    "play",
    "clearscope",
    "virtualscope",
    "data",
    "virtual",
    "override",
    "final",
    "action",
    "default",
    "states",
    "property",
    "flagdef",
    "replaces",
    "true",
    "false",
    "null",
    "self",
    "invoker",
    "super",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "until",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "foreach",
    "let",
    "is",
    "dot",
    "cross",
    "out",
    "ref",
    "in",
    "vararg",
    "sizeof",
    "alignof",
    "include",
    "bool",
    "int",
    "uint",
    "float",
    "double",
    "string",
    "name",
    "sound",
    "color",
    "void",
    "array",
    "map",
    "vector2",
    "vector3",
    "vector4",
    "quat",
    "state",
    "statelabel",
    "spriteid",
    "textureid",
    "int8",
    "int16",
    "uint8",
    "uint16",
];

fn is_keyword(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    KEYWORDS.contains(&lower.as_str())
}

/// Lex a source buffer.
pub fn lex(source: SourceId, path: &str, text: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diags = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut col = 1u32;

    let bump = |i: &mut usize, line: &mut u32, col: &mut u32, n: usize, bytes: &[u8]| {
        for _ in 0..n {
            if *i >= bytes.len() {
                break;
            }
            if bytes[*i] == b'\n' {
                *line += 1;
                *col = 1;
            } else {
                *col += 1;
            }
            *i += 1;
        }
    };

    while i < bytes.len() {
        let start = i;
        let start_line = line;
        let start_col = col;
        let c = bytes[i];

        // Whitespace
        if c.is_ascii_whitespace() {
            bump(&mut i, &mut line, &mut col, 1, bytes);
            continue;
        }

        // Comments
        if c == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                bump(&mut i, &mut line, &mut col, 2, bytes);
                while i < bytes.len() && bytes[i] != b'\n' {
                    bump(&mut i, &mut line, &mut col, 1, bytes);
                }
                continue;
            }
            if bytes[i + 1] == b'*' {
                bump(&mut i, &mut line, &mut col, 2, bytes);
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    bump(&mut i, &mut line, &mut col, 1, bytes);
                }
                if i + 1 < bytes.len() {
                    bump(&mut i, &mut line, &mut col, 2, bytes);
                }
                continue;
            }
        }

        // Preprocessor line kept as punct '#' + ident for preprocess pass —
        // actually preprocess runs on raw text. Lexer sees post-preprocess.
        // String
        if c == b'"' {
            bump(&mut i, &mut line, &mut col, 1, bytes);
            let mut out = String::new();
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    let e = bytes[i + 1];
                    let ch = match e {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'\\' => '\\',
                        b'"' => '"',
                        b'\'' => '\'',
                        other => other as char,
                    };
                    out.push(ch);
                    bump(&mut i, &mut line, &mut col, 2, bytes);
                } else {
                    out.push(bytes[i] as char);
                    bump(&mut i, &mut line, &mut col, 1, bytes);
                }
            }
            if i < bytes.len() {
                bump(&mut i, &mut line, &mut col, 1, bytes);
            } else {
                diags.push(Diagnostic {
                    severity: Severity::Error,
                    message: "unterminated string".into(),
                    span: Span {
                        source,
                        start: start as u32,
                        end: i as u32,
                        line: start_line,
                        col: start_col,
                    },
                    path: path.into(),
                });
            }
            // Adjacent string concat
            tokens.push(Token {
                kind: TokenKind::String(out),
                span: Span {
                    source,
                    start: start as u32,
                    end: i as u32,
                    line: start_line,
                    col: start_col,
                },
            });
            continue;
        }

        // Name 'Foo'
        if c == b'\'' {
            bump(&mut i, &mut line, &mut col, 1, bytes);
            let mut out = String::new();
            while i < bytes.len() && bytes[i] != b'\'' {
                out.push(bytes[i] as char);
                bump(&mut i, &mut line, &mut col, 1, bytes);
            }
            if i < bytes.len() {
                bump(&mut i, &mut line, &mut col, 1, bytes);
            }
            tokens.push(Token {
                kind: TokenKind::Name(out),
                span: Span {
                    source,
                    start: start as u32,
                    end: i as u32,
                    line: start_line,
                    col: start_col,
                },
            });
            continue;
        }

        // Number (decimal, `0x` hex, floats)
        if c.is_ascii_digit() || (c == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit())
        {
            let mut j = i;
            let mut is_float = false;
            // Hex: 0xNNN
            if c == b'0' && j + 1 < bytes.len() && (bytes[j + 1] == b'x' || bytes[j + 1] == b'X') {
                j += 2;
                while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
                    j += 1;
                }
                let lit = &text[i..j];
                let n = j - i;
                bump(&mut i, &mut line, &mut col, n, bytes);
                let val =
                    i64::from_str_radix(lit.trim_start_matches(['0', 'x', 'X']), 16).unwrap_or(0);
                tokens.push(Token {
                    kind: TokenKind::Int(val),
                    span: Span {
                        source,
                        start: start as u32,
                        end: i as u32,
                        line: start_line,
                        col: start_col,
                    },
                });
                continue;
            }
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'.' {
                // `65536.` (GZDoom) or `1.5`
                is_float = true;
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
            }
            if j < bytes.len() && (bytes[j] == b'e' || bytes[j] == b'E') {
                is_float = true;
                j += 1;
                if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                    j += 1;
                }
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
            }
            // suffixes u/f/d ignored
            while j < bytes.len()
                && matches!(
                    bytes[j],
                    b'u' | b'U' | b'f' | b'F' | b'd' | b'D' | b'l' | b'L'
                )
            {
                j += 1;
            }
            let lit = &text[i..j];
            let kind = if is_float {
                TokenKind::Float(
                    lit.trim_end_matches(|c: char| {
                        matches!(c, 'u' | 'U' | 'f' | 'F' | 'd' | 'D' | 'l' | 'L')
                    })
                    .parse()
                    .unwrap_or(0.0),
                )
            } else {
                let clean = lit.trim_end_matches(|c: char| {
                    matches!(c, 'u' | 'U' | 'f' | 'F' | 'd' | 'D' | 'l' | 'L')
                });
                TokenKind::Int(clean.parse().unwrap_or(0))
            };
            let n = j - i;
            bump(&mut i, &mut line, &mut col, n, bytes);
            tokens.push(Token {
                kind,
                span: Span {
                    source,
                    start: start as u32,
                    end: i as u32,
                    line: start_line,
                    col: start_col,
                },
            });
            continue;
        }

        // Identifier / keyword
        if c.is_ascii_alphabetic() || c == b'_' {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let ident = text[i..j].to_owned();
            let n = j - i;
            bump(&mut i, &mut line, &mut col, n, bytes);
            let kind = if is_keyword(&ident) {
                TokenKind::Kw(ident.to_ascii_lowercase())
            } else {
                TokenKind::Ident(ident)
            };
            tokens.push(Token {
                kind,
                span: Span {
                    source,
                    start: start as u32,
                    end: i as u32,
                    line: start_line,
                    col: start_col,
                },
            });
            continue;
        }

        // Multi-char punct
        let rest = &text[i..];
        let puncts = [
            "...", "<<=", ">>=", ">>>", "<=>", "~==", "<>=", "**=", "+=", "-=", "*=", "/=", "%=",
            "&=", "|=", "^=", "==", "!=", "<=", ">=", "<<", ">>", "&&", "||", "++", "--", "->",
            "::", "..", "**",
        ];
        let mut matched = None;
        for p in puncts {
            if rest.starts_with(p) {
                matched = Some(p);
                break;
            }
        }
        let p = matched.unwrap_or_else(|| {
            // single char
            &text[i..i + 1]
        });
        let n = p.len();
        bump(&mut i, &mut line, &mut col, n, bytes);
        tokens.push(Token {
            kind: TokenKind::Punct(p.to_owned()),
            span: Span {
                source,
                start: start as u32,
                end: i as u32,
                line: start_line,
                col: start_col,
            },
        });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span {
            source,
            start: i as u32,
            end: i as u32,
            line,
            col,
        },
    });

    // Merge adjacent string literals.
    let mut merged = Vec::with_capacity(tokens.len());
    let mut idx = 0;
    while idx < tokens.len() {
        if let TokenKind::String(mut s) = tokens[idx].kind.clone() {
            let span_start = tokens[idx].span;
            let mut end = tokens[idx].span.end;
            idx += 1;
            while idx < tokens.len() {
                if let TokenKind::String(more) = &tokens[idx].kind {
                    s.push_str(more);
                    end = tokens[idx].span.end;
                    idx += 1;
                } else {
                    break;
                }
            }
            merged.push(Token {
                kind: TokenKind::String(s),
                span: Span { end, ..span_start },
            });
        } else {
            merged.push(tokens[idx].clone());
            idx += 1;
        }
    }

    (merged, diags)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn lex_keywords_case_insensitive() {
        let (toks, d) = lex(SourceId(0), "t", "Class Foo { }");
        assert!(d.is_empty());
        assert!(matches!(&toks[0].kind, TokenKind::Kw(k) if k == "class"));
        assert!(matches!(&toks[1].kind, TokenKind::Ident(i) if i == "Foo"));
    }

    #[test]
    fn lex_string_concat() {
        let (toks, _) = lex(SourceId(0), "t", r#""a" "b" "c""#);
        assert!(matches!(&toks[0].kind, TokenKind::String(s) if s == "abc"));
    }

    #[test]
    fn lex_name_literal() {
        let (toks, _) = lex(SourceId(0), "t", "'Damage_Control'");
        assert!(matches!(&toks[0].kind, TokenKind::Name(n) if n == "Damage_Control"));
    }

    #[test]
    fn lex_hex_and_trailing_dot_float() {
        let (toks, d) = lex(SourceId(0), "t", "0x6800 65536.");
        assert!(d.is_empty(), "{d:?}");
        assert!(matches!(&toks[0].kind, TokenKind::Int(v) if *v == 0x6800));
        assert!(matches!(&toks[1].kind, TokenKind::Float(v) if (*v - 65536.0).abs() < 1e-9));
    }
}

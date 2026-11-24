//! ACS lexer: identifiers (case-insensitive keywords), numbers (decimal,
//! hex, fixed point `1.5`, characters `'a'`), strings (kept raw: escapes are
//! resolved when the module is loaded, as with ACC), punctuation, and `#`
//! directives. Every token remembers its file and line.

use super::Diag;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Tok {
    Ident(String),
    Int(i32),
    Str(String),
    Punct(&'static str),
    /// `#name` at the start of a directive.
    Directive(String),
    Eof,
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub tok: Tok,
    pub file: u16,
    pub line: u32,
}

const PUNCTS: &[&str] = &[
    "<<=", ">>=", "++", "--", "&&", "||", "==", "!=", "<=", ">=", "<<", ">>", "+=", "-=", "*=",
    "/=", "%=", "&=", "|=", "^=", "{", "}", "(", ")", "[", "]", ";", ",", ":", "?", "=", "+", "-",
    "*", "/", "%", "<", ">", "!", "~", "&", "|", "^", "#", "@",
];

/// Tokens of one source file.
pub(crate) fn lex(src: &str, file: u16) -> Result<Vec<Token>, Diag> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut line = 1u32;
    let mut out = Vec::new();
    let err = |line: u32, msg: String| Diag {
        file,
        line,
        message: msg,
    };
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            loop {
                if i + 1 >= b.len() {
                    return Err(err(line, "unterminated /* comment".into()));
                }
                if b[i] == b'\n' {
                    line += 1;
                }
                if b[i] == b'*' && b[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        let at = line;
        if c == b'#' && b.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic()) {
            let start = i + 1;
            i = start;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Directive(src[start..i].to_ascii_lowercase()),
                file,
                line: at,
            });
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(src[start..i].to_owned()),
                file,
                line: at,
            });
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            let value = if c == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X')) {
                i += 2;
                let h = i;
                while i < b.len() && b[i].is_ascii_hexdigit() {
                    i += 1;
                }
                u32::from_str_radix(&src[h..i], 16)
                    .map_err(|_| err(at, format!("bad hex number {}", &src[start..i])))?
                    as i32
            } else {
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                let text = &src[start..i];
                if text.contains('.') {
                    let v: f64 = text
                        .parse()
                        .map_err(|_| err(at, format!("bad number {text}")))?;
                    (v * 65536.0).round() as i64 as i32
                } else {
                    let v: i64 = text
                        .parse()
                        .map_err(|_| err(at, format!("bad number {text}")))?;
                    if v > i64::from(u32::MAX) {
                        return Err(err(at, format!("number {text} is too large")));
                    }
                    v as u32 as i32
                }
            };
            if i < b.len() && (b[i].is_ascii_alphabetic() || b[i] == b'_') {
                return Err(err(at, format!("bad number {}…", &src[start..=i])));
            }
            out.push(Token {
                tok: Tok::Int(value),
                file,
                line: at,
            });
            continue;
        }
        if c == b'"' {
            i += 1;
            let start = i;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                if i < b.len() && b[i] == b'\n' {
                    return Err(err(at, "string runs past the end of the line".into()));
                }
                i += 1;
            }
            if i >= b.len() {
                return Err(err(at, "unterminated string".into()));
            }
            let text = src[start..i].to_owned();
            i += 1;
            // "a" "b" is one string.
            if let Some(Token {
                tok: Tok::Str(prev),
                ..
            }) = out.last_mut()
            {
                prev.push_str(&text);
            } else {
                out.push(Token {
                    tok: Tok::Str(text),
                    file,
                    line: at,
                });
            }
            continue;
        }
        if c == b'\'' {
            i += 1;
            let ch = match b.get(i) {
                Some(b'\\') => {
                    i += 1;
                    match b.get(i) {
                        Some(b'n') => i32::from(b'\n'),
                        Some(b't') => i32::from(b'\t'),
                        Some(b'r') => i32::from(b'\r'),
                        Some(b'0') => 0,
                        Some(&x) => i32::from(x),
                        None => return Err(err(at, "unterminated character".into())),
                    }
                }
                Some(&x) => i32::from(x),
                None => return Err(err(at, "unterminated character".into())),
            };
            i += 1;
            if b.get(i) != Some(&b'\'') {
                return Err(err(at, "a character literal holds one character".into()));
            }
            i += 1;
            out.push(Token {
                tok: Tok::Int(ch),
                file,
                line: at,
            });
            continue;
        }
        let rest = &src[i..];
        let Some(p) = PUNCTS.iter().find(|p| rest.starts_with(**p)) else {
            return Err(err(
                at,
                format!(
                    "unexpected character {:?}",
                    rest.chars().next().unwrap_or('?')
                ),
            ));
        };
        i += p.len();
        out.push(Token {
            tok: Tok::Punct(p),
            file,
            line: at,
        });
    }
    out.push(Token {
        tok: Tok::Eof,
        file,
        line,
    });
    Ok(out)
}

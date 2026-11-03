//! ZDoom-style text scanner shared by all text lump parsers.
//!
//! Equivalent of GZDoom `FScanner` (`sc_man.cpp`). See `docs/05-text-lumps.md`.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use std::borrow::Cow;
use thiserror::Error;

/// Byte offset / line / column in a lump.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSpan {
    /// Byte offset of the token start.
    pub offset: u32,
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub col: u32,
}

/// Scanner configuration.
#[derive(Clone, Debug)]
pub struct ScanOptions {
    /// C-mode punctuation (DECORATE, new MAPINFO, …).
    pub c_mode: bool,
    /// Treat `;` as a line comment (SNDINFO, old MAPINFO, SNDSEQ).
    pub semicolon_comments: bool,
    /// Process `\\` / `\"` / `\\n` escapes inside `"…"` strings.
    /// DECORATE sets this `false` so `"\\"` frame tokens close correctly.
    pub string_escapes: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            c_mode: false,
            semicolon_comments: false,
            string_escapes: true,
        }
    }
}

/// Multi-character and single punctuation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Punct {
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `;`
    Semi,
    /// `,`
    Comma,
    /// `=`
    Eq,
    /// `:`
    Colon,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `!`
    Bang,
    /// `~`
    Tilde,
    /// `&`
    Amp,
    /// `|`
    Pipe,
    /// `^`
    Caret,
    /// `?`
    Question,
    /// `.`
    Dot,
    /// `==`
    EqEq,
    /// `!=`
    NotEq,
    /// `<=`
    LtEq,
    /// `>=`
    GtEq,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `<<`
    Shl,
    /// `>>`
    Shr,
    /// `>>>`
    UShr,
    /// `#` (DECORATE keep-frame letter)
    Hash,
    /// `\` (DECORATE sprite frame letter)
    Backslash,
    /// `++`
    PlusPlus,
    /// `--`
    MinusMinus,
    /// `+=`
    PlusEq,
    /// `-=`
    MinusEq,
    /// `*=`
    StarEq,
    /// `/=`
    SlashEq,
    /// `::`
    ColonColon,
    /// `..`
    DotDot,
}

/// One token borrowed from the scanned source when possible.
#[derive(Clone, Debug, PartialEq)]
pub enum Tok<'a> {
    /// Identifier / keyword.
    Ident(&'a str),
    /// Quoted string (escapes expanded).
    String(Cow<'a, str>),
    /// Integer (`0x`, octal, decimal) as `i64`.
    Int(i64),
    /// Floating-point number.
    Float(f64),
    /// Punctuation.
    Punct(Punct),
    /// End of input.
    Eof,
}

/// Scanner error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{kind} at line {span:?}")]
pub struct ScanError {
    /// Location.
    pub span: SourceSpan,
    /// Kind.
    pub kind: ScanErrorKind,
}

/// Error classification.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ScanErrorKind {
    /// Unexpected character.
    #[error("unexpected character {0:?}")]
    UnexpectedChar(char),
    /// Unterminated string.
    #[error("unterminated string")]
    UnterminatedString,
    /// Unterminated block comment.
    #[error("unterminated block comment")]
    UnterminatedComment,
    /// Expected a different token.
    #[error("expected {0}")]
    Expected(&'static str),
    /// Invalid number.
    #[error("invalid number")]
    InvalidNumber,
}

/// Text lump scanner.
pub struct Scanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
    opts: ScanOptions,
    /// Pushback stack (front = next to read). Small — DECORATE may unget 2 tokens.
    pending: Vec<(Tok<'a>, SourceSpan)>,
    last_span: SourceSpan,
}

impl<'a> Scanner<'a> {
    /// Scan an in-memory string (tests / included text).
    #[must_use]
    pub fn from_str(src: &'a str, opts: ScanOptions) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            opts,
            pending: Vec::new(),
            last_span: SourceSpan {
                offset: 0,
                line: 1,
                col: 1,
            },
        }
    }

    /// Span of the most recently returned token.
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        self.last_span
    }

    /// Unscanned remainder of the source (after `pos`; ignores `unget` buffer).
    #[must_use]
    pub fn remaining(&self) -> &'a str {
        self.src.get(self.pos..).unwrap_or("")
    }

    /// Advance the scanner past `nbytes` of the current `remaining()` prefix.
    pub fn bump_bytes(&mut self, nbytes: usize) {
        let end = (self.pos + nbytes).min(self.bytes.len());
        while self.pos < end {
            self.bump();
        }
    }

    /// Push a token back (LIFO — last unget is read first).
    pub fn unget(&mut self, tok: Tok<'a>) {
        self.pending.push((tok, self.last_span));
    }

    /// Next token.
    pub fn next_token(&mut self) -> Result<Tok<'a>, ScanError> {
        if let Some((tok, span)) = self.pending.pop() {
            self.last_span = span;
            return Ok(tok);
        }
        self.skip_trivia()?;
        let start = self.mark();
        if self.pos >= self.bytes.len() {
            self.last_span = start;
            return Ok(Tok::Eof);
        }
        let tok = if self.opts.c_mode {
            self.lex_c()?
        } else {
            self.lex_non_c()?
        };
        self.last_span = start;
        Ok(tok)
    }

    /// Require an identifier (case preserved).
    pub fn must_get_ident(&mut self) -> Result<&'a str, ScanError> {
        match self.next_token()? {
            Tok::Ident(s) => Ok(s),
            _ => Err(self.err(ScanErrorKind::Expected("identifier"))),
        }
    }

    /// Require a number (int or float truncated toward zero for ints via separate APIs).
    pub fn must_get_number(&mut self) -> Result<i64, ScanError> {
        match self.next_token()? {
            Tok::Int(n) => Ok(n),
            Tok::Float(f) => Ok(f as i64),
            _ => Err(self.err(ScanErrorKind::Expected("number"))),
        }
    }

    /// Require a string.
    pub fn must_get_string(&mut self) -> Result<Cow<'a, str>, ScanError> {
        match self.next_token()? {
            Tok::String(s) => Ok(s),
            _ => Err(self.err(ScanErrorKind::Expected("string"))),
        }
    }

    /// True if the next token is this punctuation (consumed).
    pub fn check_punct(&mut self, p: Punct) -> Result<bool, ScanError> {
        let tok = self.next_token()?;
        match tok {
            Tok::Punct(q) if q == p => Ok(true),
            other => {
                self.unget(other);
                Ok(false)
            }
        }
    }

    /// Case-insensitive compare of an identifier token string.
    #[must_use]
    pub fn compare(a: &str, b: &str) -> bool {
        a.eq_ignore_ascii_case(b)
    }

    fn mark(&self) -> SourceSpan {
        SourceSpan {
            offset: self.pos as u32,
            line: self.line,
            col: self.col,
        }
    }

    fn err(&self, kind: ScanErrorKind) -> ScanError {
        ScanError {
            span: self.last_span,
            kind,
        }
    }

    fn err_at(&self, span: SourceSpan, kind: ScanErrorKind) -> ScanError {
        ScanError { span, kind }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    fn skip_trivia(&mut self) -> Result<(), ScanError> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => {
                    self.bump();
                }
                Some(b'/') if self.bytes.get(self.pos + 1) == Some(&b'/') => {
                    self.bump();
                    self.bump();
                    while let Some(b) = self.peek() {
                        if b == b'\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some(b'/') if self.bytes.get(self.pos + 1) == Some(&b'*') => {
                    let span = self.mark();
                    self.bump();
                    self.bump();
                    let mut closed = false;
                    while let Some(b) = self.bump() {
                        if b == b'*' && self.peek() == Some(b'/') {
                            self.bump();
                            closed = true;
                            break;
                        }
                    }
                    if !closed {
                        return Err(self.err_at(span, ScanErrorKind::UnterminatedComment));
                    }
                }
                Some(b';') if self.opts.semicolon_comments => {
                    self.bump();
                    while let Some(b) = self.peek() {
                        if b == b'\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn lex_c(&mut self) -> Result<Tok<'a>, ScanError> {
        let b = self.peek().unwrap_or(0);
        if b == b'"' {
            return self.lex_string();
        }
        if b.is_ascii_digit()
            || (b == b'.' && self.bytes.get(self.pos + 1).is_some_and(u8::is_ascii_digit))
        {
            return self.lex_number();
        }
        if b == b'_' || b.is_ascii_alphabetic() || b >= 0x80 {
            // Non-ASCII: BD/GZDoom labels may contain UTF-8 letters (`Death.Сut`).
            return self.lex_ident_c();
        }
        self.lex_punct()
    }

    fn lex_non_c(&mut self) -> Result<Tok<'a>, ScanError> {
        let b = self.peek().unwrap_or(0);
        if b == b'"' {
            return self.lex_string();
        }
        // Splitters in non-C mode.
        if matches!(b, b'{' | b'}' | b'(' | b')' | b'=' | b',' | b';') {
            return self.lex_punct();
        }
        if b.is_ascii_digit()
            || (b == b'-' && self.bytes.get(self.pos + 1).is_some_and(u8::is_ascii_digit))
        {
            // Prefer number when the whole run is numeric-looking; else string-token.
            let start = self.pos;
            let mut i = start;
            if self.bytes[i] == b'-' {
                i += 1;
            }
            let mut saw_digit = false;
            let mut saw_dot = false;
            while i < self.bytes.len() {
                let c = self.bytes[i];
                if c.is_ascii_digit() {
                    saw_digit = true;
                    i += 1;
                } else if c == b'.' && !saw_dot {
                    saw_dot = true;
                    i += 1;
                } else {
                    break;
                }
            }
            let next = self.bytes.get(i).copied();
            let ends = next.is_none_or(|c| {
                c.is_ascii_whitespace()
                    || matches!(c, b'{' | b'}' | b'(' | b')' | b'=' | b',' | b';')
            });
            if saw_digit && ends {
                return self.lex_number();
            }
        }
        // Run of non-whitespace / non-splitters.
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace()
                || matches!(c, b'{' | b'}' | b'(' | b')' | b'=' | b',' | b';')
            {
                break;
            }
            if c == b'"' {
                break;
            }
            self.bump();
        }
        let s = &self.src[start..self.pos];
        Ok(Tok::Ident(s))
    }

    fn lex_ident_c(&mut self) -> Result<Tok<'a>, ScanError> {
        let start = self.pos;
        match self.peek() {
            Some(c) if c >= 0x80 => {
                let n = utf8_char_len(c);
                for _ in 0..n {
                    if self.bump().is_none() {
                        break;
                    }
                }
            }
            _ => {
                self.bump();
            }
        }
        while let Some(c) = self.peek() {
            if c == b'_' || c.is_ascii_alphanumeric() {
                self.bump();
            } else if c >= 0x80 {
                let n = utf8_char_len(c);
                for _ in 0..n {
                    if self.bump().is_none() {
                        break;
                    }
                }
            } else {
                break;
            }
        }
        Ok(Tok::Ident(&self.src[start..self.pos]))
    }

    fn lex_string(&mut self) -> Result<Tok<'a>, ScanError> {
        let span = self.mark();
        self.bump(); // "
        let mut out = String::new();
        let mut simple = true;
        loop {
            match self.bump() {
                None => return Err(self.err_at(span, ScanErrorKind::UnterminatedString)),
                Some(b'"') => break,
                Some(b'\\') if self.opts.string_escapes => {
                    simple = false;
                    match self.bump() {
                        Some(b'n') => out.push('\n'),
                        Some(b't') => out.push('\t'),
                        Some(b'r') => out.push('\r'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'"') => out.push('"'),
                        Some(b'x') => {
                            let h1 = self.bump();
                            let h2 = self.bump();
                            let v = hex_byte(h1, h2).ok_or_else(|| {
                                self.err_at(span, ScanErrorKind::UnexpectedChar('x'))
                            })?;
                            out.push(char::from(v));
                        }
                        Some(b'\n') => {} // line continuation
                        Some(c) => out.push(char::from(c)),
                        None => return Err(self.err_at(span, ScanErrorKind::UnterminatedString)),
                    }
                }
                Some(b'\\') => {
                    // Literal backslash (DECORATE frame `"\\"`).
                    if !simple {
                        out.push('\\');
                    }
                }
                Some(c) => {
                    if !simple {
                        out.push(char::from(c));
                    }
                }
            }
            if simple {
                // still scanning; if we break on close we'll slice
            }
        }
        if simple {
            // content is between quotes without escapes
            // find start after opening quote
            let open = span.offset as usize + 1;
            let close = self.pos - 1;
            Ok(Tok::String(Cow::Borrowed(&self.src[open..close])))
        } else {
            // rebuild with escapes — re-scan would be cleaner; we built `out` only on escapes.
            // Fix: always build owned when we saw escape; for the path above when simple stayed
            // true we borrow. When simple became false we need full decode from scratch.
            let open = span.offset as usize + 1;
            let close = self.pos - 1;
            Ok(Tok::String(Cow::Owned(unescape(&self.src[open..close]))))
        }
    }

    fn lex_number(&mut self) -> Result<Tok<'a>, ScanError> {
        let start = self.pos;
        let span = self.mark();
        if self.peek() == Some(b'-') {
            self.bump();
        }
        let mut is_float = false;
        if self.peek() == Some(b'0')
            && matches!(
                self.bytes.get(self.pos + 1).map(|c| c.to_ascii_lowercase()),
                Some(b'x')
            )
        {
            self.bump();
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.bump();
            }
            let s = &self.src[start..self.pos];
            let n = i64::from_str_radix(
                s.trim_start_matches('-')
                    .trim_start_matches("0x")
                    .trim_start_matches("0X"),
                16,
            )
            .map_err(|_| self.err_at(span, ScanErrorKind::InvalidNumber))?;
            let n = if s.starts_with('-') { -n } else { n };
            return Ok(Tok::Int(n));
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some(b'.') && self.bytes.get(self.pos + 1).is_some_and(u8::is_ascii_digit)
        {
            is_float = true;
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        // Exponent only when clearly numeric (`1e3`, `1e-2`) — not sprite tails
        // like DECORATE `13EI` (docs/33 BD digit-leading sprites).
        if matches!(self.peek().map(|c| c.to_ascii_lowercase()), Some(b'e')) {
            let after = self.bytes.get(self.pos + 1).copied();
            let after2 = self.bytes.get(self.pos + 2).copied();
            let exp_ok = match after {
                Some(b'+' | b'-') => after2.is_some_and(|c| c.is_ascii_digit()),
                Some(c) if c.is_ascii_digit() => true,
                _ => false,
            };
            if exp_ok {
                is_float = true;
                self.bump();
                if matches!(self.peek(), Some(b'+' | b'-')) {
                    self.bump();
                }
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                }
            }
        }
        // `f` suffix only at end of a numeric token (not `13FI` sprite).
        if matches!(self.peek().map(|c| c.to_ascii_lowercase()), Some(b'f')) {
            let after = self.bytes.get(self.pos + 1).copied();
            let f_ok = after.is_none_or(|c| {
                c.is_ascii_whitespace()
                    || matches!(
                        c,
                        b'{' | b'}' | b'(' | b')' | b',' | b';' | b'"' | b'+' | b'-' | b'*' | b'/'
                            | b'=' | b'<' | b'>' | b'!' | b'|' | b'&'
                    )
            });
            if f_ok {
                is_float = true;
                self.bump();
            }
        }
        let s = &self.src[start..self.pos];
        if is_float {
            let cleaned = s.trim_end_matches(['f', 'F']);
            let f: f64 = cleaned
                .parse()
                .map_err(|_| self.err_at(span, ScanErrorKind::InvalidNumber))?;
            Ok(Tok::Float(f))
        } else if s.starts_with('0') && s.len() > 1 && s.bytes().all(|c| c.is_ascii_digit()) {
            // octal, C strtol base 0
            let n = i64::from_str_radix(s.trim_start_matches('-'), 8)
                .map_err(|_| self.err_at(span, ScanErrorKind::InvalidNumber))?;
            let n = if s.starts_with('-') { -n } else { n };
            Ok(Tok::Int(n))
        } else {
            let n: i64 = s
                .parse()
                .map_err(|_| self.err_at(span, ScanErrorKind::InvalidNumber))?;
            Ok(Tok::Int(n))
        }
    }

    fn lex_punct(&mut self) -> Result<Tok<'a>, ScanError> {
        let span = self.mark();
        let a = self
            .bump()
            .ok_or_else(|| self.err_at(span, ScanErrorKind::UnexpectedChar('\0')))?;
        let b = self.peek();
        let c = self.bytes.get(self.pos + 1).copied();
        let punct = match (a, b, c) {
            (b'>', Some(b'>'), Some(b'>')) => {
                self.bump();
                self.bump();
                Punct::UShr
            }
            (b'<', Some(b'<'), _) => {
                self.bump();
                Punct::Shl
            }
            (b'>', Some(b'>'), _) => {
                self.bump();
                Punct::Shr
            }
            (b'&', Some(b'&'), _) => {
                self.bump();
                Punct::AndAnd
            }
            (b'|', Some(b'|'), _) => {
                self.bump();
                Punct::OrOr
            }
            (b'=', Some(b'='), _) => {
                self.bump();
                Punct::EqEq
            }
            (b'!', Some(b'='), _) => {
                self.bump();
                Punct::NotEq
            }
            (b'<', Some(b'='), _) => {
                self.bump();
                Punct::LtEq
            }
            (b'>', Some(b'='), _) => {
                self.bump();
                Punct::GtEq
            }
            (b'+', Some(b'+'), _) => {
                self.bump();
                Punct::PlusPlus
            }
            (b'-', Some(b'-'), _) => {
                self.bump();
                Punct::MinusMinus
            }
            (b'+', Some(b'='), _) => {
                self.bump();
                Punct::PlusEq
            }
            (b'-', Some(b'='), _) => {
                self.bump();
                Punct::MinusEq
            }
            (b'*', Some(b'='), _) => {
                self.bump();
                Punct::StarEq
            }
            (b'/', Some(b'='), _) => {
                self.bump();
                Punct::SlashEq
            }
            (b':', Some(b':'), _) => {
                self.bump();
                Punct::ColonColon
            }
            (b'.', Some(b'.'), _) => {
                self.bump();
                Punct::DotDot
            }
            (b'{', _, _) => Punct::LBrace,
            (b'}', _, _) => Punct::RBrace,
            (b'(', _, _) => Punct::LParen,
            (b')', _, _) => Punct::RParen,
            (b'[', _, _) => Punct::LBracket,
            (b']', _, _) => Punct::RBracket,
            (b';', _, _) => Punct::Semi,
            (b',', _, _) => Punct::Comma,
            (b'=', _, _) => Punct::Eq,
            (b':', _, _) => Punct::Colon,
            (b'+', _, _) => Punct::Plus,
            (b'-', _, _) => Punct::Minus,
            (b'*', _, _) => Punct::Star,
            (b'/', _, _) => Punct::Slash,
            (b'%', _, _) => Punct::Percent,
            (b'<', _, _) => Punct::Lt,
            (b'>', _, _) => Punct::Gt,
            (b'!', _, _) => Punct::Bang,
            (b'~', _, _) => Punct::Tilde,
            (b'&', _, _) => Punct::Amp,
            (b'|', _, _) => Punct::Pipe,
            (b'^', _, _) => Punct::Caret,
            (b'?', _, _) => Punct::Question,
            (b'.', _, _) => Punct::Dot,
            (b'#', _, _) => Punct::Hash,
            (b'\\', _, _) => Punct::Backslash,
            _ => {
                return Err(self.err_at(span, ScanErrorKind::UnexpectedChar(char::from(a))));
            }
        };
        Ok(Tok::Punct(punct))
    }
}

fn utf8_char_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1, // continuation / invalid: consume one to make progress
    }
}

fn hex_byte(h1: Option<u8>, h2: Option<u8>) -> Option<u8> {
    let a = (h1?).to_ascii_lowercase();
    let b = (h2?).to_ascii_lowercase();
    let n1 = match a {
        b'0'..=b'9' => a - b'0',
        b'a'..=b'f' => a - b'a' + 10,
        _ => return None,
    };
    let n2 = match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => return None,
    };
    Some((n1 << 4) | n2)
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('x') => {
                let h1 = chars.next().unwrap_or('0');
                let h2 = chars.next().unwrap_or('0');
                if let Some(v) = hex_byte(Some(h1 as u8), Some(h2 as u8)) {
                    out.push(char::from(v));
                }
            }
            Some('\n') => {}
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn c_mode_basic() {
        let mut sc = Scanner::from_str(
            r#"Actor Imp { Health = 60; }"#,
            ScanOptions {
                c_mode: true,
                ..Default::default()
            },
        );
        assert!(matches!(sc.next_token().unwrap(), Tok::Ident("Actor")));
        assert!(matches!(sc.next_token().unwrap(), Tok::Ident("Imp")));
        assert!(sc.check_punct(Punct::LBrace).unwrap());
        assert!(matches!(sc.next_token().unwrap(), Tok::Ident("Health")));
        assert!(sc.check_punct(Punct::Eq).unwrap());
        assert_eq!(sc.must_get_number().unwrap(), 60);
        assert!(sc.check_punct(Punct::Semi).unwrap());
        assert!(sc.check_punct(Punct::RBrace).unwrap());
    }

    #[test]
    fn numbers_hex_octal_float() {
        let mut sc = Scanner::from_str(
            "0xFF 010 1.5 1e2",
            ScanOptions {
                c_mode: true,
                ..Default::default()
            },
        );
        assert_eq!(sc.next_token().unwrap(), Tok::Int(255));
        assert_eq!(sc.next_token().unwrap(), Tok::Int(8));
        assert_eq!(sc.next_token().unwrap(), Tok::Float(1.5));
        assert_eq!(sc.next_token().unwrap(), Tok::Float(100.0));
    }

    #[test]
    fn non_c_sndinfo_tokens() {
        let mut sc = Scanner::from_str(
            "weapons/pistol DSPISTOL\n$random foo { a b }",
            ScanOptions {
                c_mode: false,
                semicolon_comments: true,
             ..Default::default() },
        );
        assert_eq!(sc.next_token().unwrap(), Tok::Ident("weapons/pistol"));
        assert_eq!(sc.next_token().unwrap(), Tok::Ident("DSPISTOL"));
        assert_eq!(sc.next_token().unwrap(), Tok::Ident("$random"));
        assert_eq!(sc.next_token().unwrap(), Tok::Ident("foo"));
        assert!(sc.check_punct(Punct::LBrace).unwrap());
    }

    #[test]
    fn string_escapes() {
        let mut sc = Scanner::from_str(
            r#""hi\nthere""#,
            ScanOptions {
                c_mode: true,
                ..Default::default()
            },
        );
        match sc.next_token().unwrap() {
            Tok::String(s) => assert_eq!(s, "hi\nthere"),
            other => panic!("{other:?}"),
        }
    }
}

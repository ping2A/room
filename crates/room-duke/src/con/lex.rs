//! CON tokenizer matching Chocolate `ispecial` / `isaltok` / `getlabel`.

/// True for characters that terminate tokens (space, CR, LF).
#[must_use]
pub fn is_special(c: u8) -> bool {
    matches!(c, b' ' | b'\r' | b'\n')
}

/// Characters allowed inside a CON token (`isaltok`).
#[must_use]
pub fn is_altok(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'{' | b'}' | b'/' | b'*' | b'-' | b'_' | b'.')
}

/// Cursor over CON source bytes.
#[derive(Debug, Clone)]
pub struct Lexer<'a> {
    /// Remaining input (full buffer; `pos` indexes into it).
    pub src: &'a [u8],
    /// Byte offset.
    pub pos: usize,
    /// 1-based line (increments on `\n`).
    pub line: u32,
}

impl<'a> Lexer<'a> {
    /// Start at the beginning of `src`.
    #[must_use]
    pub fn new(src: &'a [u8]) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
        }
    }

    /// True at end of buffer or on a NUL (Chocolate text terminator).
    #[must_use]
    pub fn eof(&self) -> bool {
        self.pos >= self.src.len() || self.src[self.pos] == 0
    }

    pub(crate) fn bump(&mut self) -> Option<u8> {
        if self.pos >= self.src.len() {
            return None;
        }
        let c = self.src[self.pos];
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
        }
        Some(c)
    }

    #[must_use]
    pub(crate) fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    /// Skip whitespace (and bare CR/LF). Does not skip `//` / `/*` (those are keywords).
    pub fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if is_special(c) {
                self.bump();
            } else {
                break;
            }
        }
    }

    /// Next token preserving case, or `None` at EOF.
    ///
    /// Matches Chocolate `transword` / `keyword` token gathering (`isaltok` runs).
    /// Keywords are matched case-sensitively against the lowercase `keyw[]` table;
    /// label names keep file case (Atomic uses `define pjumping` vs `action PJUMPING`).
    pub fn next_token(&mut self) -> Option<String> {
        self.skip_ws();
        if self.eof() {
            return None;
        }
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_altok(c) {
                self.bump();
            } else {
                break;
            }
        }
        if self.pos == start {
            // Non-altok junk: consume one byte so we do not spin.
            self.bump();
            return Some(String::new());
        }
        let raw = &self.src[start..self.pos];
        Some(String::from_utf8_lossy(raw).into_owned())
    }

    /// Read a label name after a keyword (`getlabel`): skip to alnum, then
    /// take until `ispecial`. Preserves case like Chocolate.
    pub fn get_label(&mut self) -> String {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() {
                break;
            }
            self.bump();
            if self.eof() {
                return String::new();
            }
        }
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_special(c) {
                break;
            }
            self.bump();
        }
        String::from_utf8_lossy(&self.src[start..self.pos]).into_owned()
    }

    /// Skip spaces only (not CR/LF) — used before quote / sound filenames.
    pub fn skip_spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.bump();
        }
    }

    /// Read until end of line (exclusive of `\n`); advances past `\n` if present.
    pub fn rest_of_line(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'\n' || c == b'\r' {
                break;
            }
            self.bump();
        }
        let s = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
        if self.peek() == Some(b'\r') {
            self.bump();
        }
        if self.peek() == Some(b'\n') {
            self.bump();
        }
        s
    }

    /// Space-delimited token preserving case (sound / map / music names).
    /// Stops at newline; callers that allow wrapped names should [`skip_ws`] first.
    pub fn raw_token(&mut self) -> String {
        self.skip_spaces();
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\n' || c == b'\r' {
                break;
            }
            self.bump();
        }
        String::from_utf8_lossy(&self.src[start..self.pos]).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_define() {
        let mut lx = Lexer::new(b"define FOO 42\n");
        assert_eq!(lx.next_token().as_deref(), Some("define"));
        // get_label used for the symbol; next_token would also work for FOO
        assert_eq!(lx.get_label(), "FOO");
        assert_eq!(lx.next_token().as_deref(), Some("42"));
    }

    #[test]
    fn label_case_preserved() {
        let mut lx = Lexer::new(b"action PJUMPING\n");
        assert_eq!(lx.next_token().as_deref(), Some("action"));
        assert_eq!(lx.get_label(), "PJUMPING");
    }
}

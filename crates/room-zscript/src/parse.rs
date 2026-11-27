//! Recursive-descent ZScript parser.

use crate::ast::*;
use crate::diag::{Diagnostic, Severity};
use crate::lex::{Token, TokenKind, lex};
use crate::preprocess::preprocess;
use crate::span::{SourceId, Span};
use std::collections::BTreeMap;

/// Fallback for `peek` when the token stream is empty (`lex` always appends `Eof`).
static EOF_TOKEN: Token = Token {
    kind: TokenKind::Eof,
    span: Span {
        source: SourceId(0),
        start: 0,
        end: 0,
        line: 0,
        col: 0,
    },
};

/// Parser output.
#[derive(Debug, Default)]
pub struct ParseResult {
    /// Unit.
    pub unit: TranslationUnit,
    /// Diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Where each parsed root's lines came from, indexed by `Span::source`.
    pub origins: Vec<RootOrigins>,
}

/// Line origins of one preprocessed root (its `#include`s inlined).
#[derive(Debug, Default, Clone)]
pub struct RootOrigins {
    /// Root file name.
    pub root: String,
    /// Files the root pulled in (`line_map` indexes these).
    pub files: Vec<String>,
    /// Per flattened line: (index into `files`, 1-based line). Empty = the
    /// root's own lines, unchanged.
    pub line_map: Vec<(u32, u32)>,
}

impl ParseResult {
    /// Point `d` at the file and line it came from (`#include`d files are
    /// flattened before parsing, so raw spans count lines of the whole root).
    pub fn locate(&self, d: &mut Diagnostic) {
        let Some(o) = self.origins.get(d.span.source.0 as usize) else {
            return;
        };
        if !(d.path.is_empty() || d.path == "<input>" || d.path == o.root) {
            return;
        }
        if o.line_map.is_empty() {
            if d.path.is_empty() {
                d.path = o.root.clone();
            }
            return;
        }
        let Some(&(f, line)) = d
            .span
            .line
            .checked_sub(1)
            .and_then(|i| o.line_map.get(i as usize))
        else {
            return;
        };
        if let Some(name) = o.files.get(f as usize) {
            d.path = name.clone();
            d.span.line = line;
        }
    }
}

fn origins_of(root: &str, pp: &crate::preprocess::PreprocessResult) -> RootOrigins {
    RootOrigins {
        root: root.to_owned(),
        files: pp.files.iter().map(|f| f.name.clone()).collect(),
        line_map: pp.line_map.clone(),
    }
}

/// Parse multiple named sources (concatenated after preprocess of each).
pub fn parse_sources(sources: &[(String, String)]) -> ParseResult {
    if sources.is_empty() {
        return ParseResult::default();
    }
    let mut lookup: std::collections::BTreeMap<String, String> = BTreeMap::new();
    for (n, t) in sources {
        lookup.insert(n.to_ascii_lowercase(), t.clone());
        let base = n
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(n)
            .to_ascii_lowercase();
        lookup.entry(base).or_insert_with(|| t.clone());
    }
    let (entry_name, entry_text) = &sources[0];
    let pp = preprocess(entry_name, entry_text, |path| {
        lookup.get(&path.to_ascii_lowercase()).cloned()
    });
    let mut result = parse_file_with_version(SourceId(0), entry_name, &pp.text, pp.version.clone());
    result.origins = vec![origins_of(entry_name, &pp)];
    result.diagnostics.extend(pp.diagnostics);
    if result.unit.version.is_none() {
        result.unit.version = pp.version.clone();
    }
    let mut root_versions = vec![pp.version.clone()];
    // Parse additional sources that weren't pulled in via #include. Each one
    // is its own root (GZDoom: every archive's `zscript` lump), so its own
    // `#include`s are expanded too — e.g. Brutal Doom's `zscript.zc` loaded
    // together with another mod's `ZSCRIPT`.
    let mut included: BTreeMap<String, ()> = BTreeMap::new();
    let mark = |files: &[crate::span::SourceFile], included: &mut BTreeMap<String, ()>| {
        for f in files {
            let full = f.name.to_ascii_lowercase();
            let base = f
                .name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&f.name)
                .to_ascii_lowercase();
            included.insert(full, ());
            included.insert(base, ());
        }
    };
    mark(&pp.files, &mut included);
    for (name, text) in sources.iter().skip(1) {
        let key = name.to_ascii_lowercase();
        let base = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(name)
            .to_ascii_lowercase();
        if included.contains_key(&key) || included.contains_key(&base) {
            continue;
        }
        let id = SourceId(u32::try_from(result.origins.len()).unwrap_or(u32::MAX));
        let extra = if text.contains("#include") {
            let pp = preprocess(name, text, |path| {
                lookup.get(&path.to_ascii_lowercase()).cloned()
            });
            mark(&pp.files, &mut included);
            let mut r = parse_file_with_version(id, name, &pp.text, pp.version.clone());
            r.origins = vec![origins_of(name, &pp)];
            root_versions.push(pp.version.clone());
            r.diagnostics.extend(pp.diagnostics);
            r
        } else {
            let mut r = parse_file_with_version(id, name, text, None);
            root_versions.push(r.unit.version.clone());
            r.origins = vec![RootOrigins {
                root: name.clone(),
                ..RootOrigins::default()
            }];
            r
        };
        result.origins.extend(extra.origins);
        result.unit.decls.extend(extra.unit.decls);
        result.unit.static_arrays.extend(extra.unit.static_arrays);
        result.diagnostics.extend(extra.diagnostics);
    }
    crate::static_arrays::post_parse(&mut result.unit);
    result.unit.root_versions = std::mem::take(&mut root_versions);
    let mut diags = std::mem::take(&mut result.diagnostics);
    for d in &mut diags {
        result.locate(d);
    }
    result.diagnostics = diags;
    result
}

/// `foreach (name : iter)` binds `name` to `iter[0]` so the type checker sees it.
fn foreach_while(names: Vec<String>, iter: Expr, body: Block, span: Span) -> Stmt {
    let mut stmts = Vec::new();
    let last = names.len().saturating_sub(1);
    for (i, name) in names.into_iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        let init = if i == last {
            Some(Expr::Index {
                arr: Box::new(iter.clone()),
                index: Box::new(Expr::Int(0, span)),
                span,
            })
        } else {
            None
        };
        stmts.push(Stmt::Local {
            ty: None,
            names: vec![name],
            init,
            extra_inits: Vec::new(),
            span,
        });
    }
    stmts.extend(body.stmts);
    Stmt::While {
        cond: iter,
        body: Block {
            stmts,
            span: body.span,
        },
        span,
    }
}

/// Parse a single file's text.
pub fn parse_file(path: &str, text: &str) -> ParseResult {
    let mut r = parse_file_with_version(SourceId(0), path, text, None);
    crate::static_arrays::post_parse(&mut r.unit);
    r
}

fn parse_file_with_version(
    source: SourceId,
    path: &str,
    text: &str,
    version: Option<String>,
) -> ParseResult {
    let (tokens, mut diags) = lex(source, path, text);
    let mut p = Parser {
        tokens,
        pos: 0,
        path: path.to_owned(),
        version,
        diags: Vec::new(),
        extra_rets: Vec::new(),
        pending: Vec::new(),
        temps: 0,
        owner: String::new(),
        static_arrays: Vec::new(),
    };
    let mut unit = p.parse_unit();
    unit.static_arrays = std::mem::take(&mut p.static_arrays);
    diags.append(&mut p.diags);
    ParseResult {
        unit,
        diagnostics: diags,
        origins: Vec::new(),
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    path: String,
    version: Option<String>,
    diags: Vec<Diagnostic>,
    /// Return types after the first (`int, int GetAmount(…)`) for the next method.
    extra_rets: Vec<TypeRef>,
    /// Statements to splice into the enclosing block right after the one just
    /// parsed (array initializer pushes, multi-assign stores).
    pending: Vec<Stmt>,
    /// Counter for parser-made temporaries.
    temps: u32,
    /// Class / struct whose body is being parsed.
    owner: String,
    /// Class-level `static const T name[] = { … };` tables seen so far.
    static_arrays: Vec<crate::ast::StaticArray>,
}

impl Parser {
    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .unwrap_or(&EOF_TOKEN)
    }

    fn bump(&mut self) -> Token {
        let t = self.peek().clone();
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

    fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic {
            severity: Severity::Error,
            message: msg.into(),
            span,
            path: self.path.clone(),
        });
    }

    fn warn(&mut self, span: Span, msg: impl Into<String>) {
        self.diags
            .push(Diagnostic::warn(self.path.clone(), span, msg));
    }

    fn expect_punct(&mut self, p: &str) -> bool {
        if matches!(&self.peek().kind, TokenKind::Punct(x) if x == p) {
            self.bump();
            true
        } else {
            let span = self.peek().span;
            self.error(span, format!("expected '{p}'"));
            false
        }
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if matches!(&self.peek().kind, TokenKind::Punct(x) if x == p) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn peek_n(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .or_else(|| self.tokens.last())
            .unwrap_or(&EOF_TOKEN)
    }

    fn eat_kw(&mut self, k: &str) -> bool {
        if matches!(&self.peek().kind, TokenKind::Kw(x) if x == k) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn check_kw(&self, k: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Kw(x) if x == k)
    }

    fn parse_unit(&mut self) -> TranslationUnit {
        let mut unit = TranslationUnit::default();
        while !matches!(self.peek().kind, TokenKind::Eof) {
            // version "x.y";
            if self.eat_kw("version") {
                if let TokenKind::String(v) = self.bump().kind {
                    self.version = Some(v.clone());
                    unit.version = Some(v);
                }
                self.eat_punct(";");
                continue;
            }
            if let Some(d) = self.parse_decl() {
                unit.decls.push(d);
            } else {
                // recover: skip a token
                let span = self.peek().span;
                self.error(span, format!("unexpected token {:?}", self.peek().kind));
                self.bump();
            }
        }
        unit
    }

    fn parse_decl(&mut self) -> Option<Decl> {
        if self.eat_kw("extend") {
            if self.eat_kw("class") {
                return Some(Decl::ExtendClass(
                    self.parse_class_body(ClassFlags::default())?,
                ));
            }
            // GZDoom `extend struct _ { … }` (global engine struct).
            if self.eat_kw("struct") {
                return Some(Decl::Struct(self.parse_struct()?));
            }
            return None;
        }
        if self.eat_kw("mixin") {
            if self.eat_kw("class") {
                return Some(Decl::Mixin(self.parse_class_body(ClassFlags::default())?));
            }
            return None;
        }
        if self.eat_kw("class") {
            return Some(Decl::Class(self.parse_class_body(ClassFlags::default())?));
        }
        // leading scope/native/abstract before class
        let mut flags = ClassFlags::default();
        loop {
            if self.eat_kw("abstract") {
                flags.abstract_ = true;
                continue;
            }
            if self.eat_kw("native") {
                flags.native = true;
                continue;
            }
            if self.check_kw("ui") || self.check_kw("play") || self.check_kw("clearscope") {
                if let TokenKind::Kw(s) = self.bump().kind {
                    flags.scope = Some(s);
                }
                continue;
            }
            break;
        }
        if self.eat_kw("class") {
            return Some(Decl::Class(self.parse_class_body(flags)?));
        }
        if self.eat_kw("struct") {
            return Some(Decl::Struct(self.parse_struct()?));
        }
        if self.eat_kw("enum") {
            return Some(Decl::Enum(self.parse_enum()?));
        }
        if self.eat_kw("const") {
            return Some(Decl::Const(self.parse_const()?));
        }
        None
    }

    fn parse_ident_name(&mut self) -> Option<String> {
        match self.bump().kind {
            TokenKind::Ident(s) | TokenKind::Kw(s) => Some(s),
            _ => None,
        }
    }

    fn parse_class_body(&mut self, mut flags: ClassFlags) -> Option<ClassDecl> {
        let span = self.peek().span;
        let name = self.parse_ident_name()?;
        let mut parent = None;
        let mut replaces = None;
        if self.eat_punct(":") {
            parent = self.parse_ident_name();
        }
        // GZDoom trailing quals: `class Foo : Object native play version("2.4")`
        // and optional `replaces Bar`.
        loop {
            if self.eat_kw("native") {
                flags.native = true;
                continue;
            }
            if self.eat_kw("abstract") {
                flags.abstract_ = true;
                continue;
            }
            if self.eat_kw("final") {
                continue;
            }
            // GZDoom `sealed(SubA, SubB)` — closed hierarchy.
            if matches!(&self.peek().kind, TokenKind::Ident(s) | TokenKind::Kw(s) if s == "sealed")
            {
                self.bump();
                if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "(") {
                    self.skip_balanced('(', ')');
                }
                continue;
            }
            if self.eat_kw("replaces") {
                replaces = self.parse_ident_name();
                continue;
            }
            if self.check_kw("ui")
                || self.check_kw("play")
                || self.check_kw("clearscope")
                || self.check_kw("virtualscope")
            {
                if let TokenKind::Kw(s) = self.bump().kind {
                    flags.scope = Some(s);
                }
                continue;
            }
            if self.check_kw("deprecated") {
                self.bump();
                if self.eat_punct("(") {
                    let mut text = Vec::new();
                    while !matches!(&self.peek().kind, TokenKind::Eof)
                        && !matches!(&self.peek().kind, TokenKind::Punct(p) if p == ")")
                    {
                        if let TokenKind::String(v) = self.bump().kind {
                            text.push(v);
                        }
                    }
                    self.expect_punct(")");
                    let mut text = text.into_iter();
                    flags.deprecated = Some((
                        text.next().unwrap_or_default(),
                        text.next().unwrap_or_default(),
                    ));
                }
                continue;
            }
            if self.check_kw("version") {
                self.bump();
                self.skip_balanced('(', ')');
                continue;
            }
            if matches!(&self.peek().kind, TokenKind::Ident(s) if s.eq_ignore_ascii_case("unsafe"))
            {
                self.bump();
                if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "(") {
                    self.skip_balanced('(', ')');
                }
                continue;
            }
            break;
        }
        if !self.expect_punct("{") {
            return None;
        }
        let mut members = Vec::new();
        let mut mixins = Vec::new();
        let outer_owner = std::mem::replace(&mut self.owner, name.clone());
        while !matches!(&self.peek().kind, TokenKind::Punct(p) if p == "}")
            && !matches!(self.peek().kind, TokenKind::Eof)
        {
            if self.eat_kw("mixin") {
                if let Some(n) = self.parse_ident_name() {
                    mixins.push(n.clone());
                    members.push(Member::MixinUse {
                        name: n,
                        span: self.peek().span,
                    });
                }
                self.eat_punct(";");
                continue;
            }
            let before = self.pos;
            if let Some(m) = self.parse_member() {
                members.push(m);
            } else if self.pos == before {
                self.bump();
            }
        }
        self.expect_punct("}");
        self.owner = outer_owner;
        // GZDoom often writes `class Foo { … };` / `struct Bar { … };`
        self.eat_punct(";");
        Some(ClassDecl {
            name,
            parent,
            replaces,
            flags,
            mixins,
            members,
            span,
        })
    }

    fn parse_struct(&mut self) -> Option<StructDecl> {
        let span = self.peek().span;
        let name = self.parse_ident_name()?;
        // GZDoom: `struct Foo native play version("4.12") { … }`
        // also `struct _ native unsafe(internal) { … }`
        let mut native = false;
        loop {
            if self.eat_kw("native") {
                native = true;
                continue;
            }
            if self.eat_kw("ui")
                || self.eat_kw("play")
                || self.eat_kw("clearscope")
                || self.eat_kw("virtualscope")
            {
                continue;
            }
            if matches!(&self.peek().kind, TokenKind::Ident(s) if s.eq_ignore_ascii_case("unsafe"))
            {
                self.bump();
                if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "(") {
                    self.skip_balanced('(', ')');
                }
                continue;
            }
            if self.check_kw("version") || self.check_kw("deprecated") {
                self.bump();
                self.skip_balanced('(', ')');
                continue;
            }
            break;
        }
        if !self.expect_punct("{") {
            return None;
        }
        let mut members = Vec::new();
        let outer_owner = std::mem::replace(&mut self.owner, name.clone());
        while !matches!(&self.peek().kind, TokenKind::Punct(p) if p == "}")
            && !matches!(self.peek().kind, TokenKind::Eof)
        {
            let before = self.pos;
            if let Some(m) = self.parse_member() {
                members.push(m);
            } else if self.pos == before {
                self.bump();
            }
        }
        self.expect_punct("}");
        self.owner = outer_owner;
        self.eat_punct(";");
        Some(StructDecl {
            name,
            members,
            native,
            span,
        })
    }

    fn parse_enum(&mut self) -> Option<EnumDecl> {
        let span = self.peek().span;
        let name = self.parse_ident_name().unwrap_or_default();
        // optional `: int`
        if self.eat_punct(":") {
            let _ = self.parse_type();
        }
        if !self.expect_punct("{") {
            return None;
        }
        let mut variants = Vec::new();
        while !matches!(&self.peek().kind, TokenKind::Punct(p) if p == "}")
            && !matches!(self.peek().kind, TokenKind::Eof)
        {
            let vname = self.parse_ident_name()?;
            let mut val = None;
            if self.eat_punct("=") {
                val = Some(self.parse_expr(0));
            }
            variants.push((vname, val));
            self.eat_punct(",");
        }
        self.expect_punct("}");
        self.eat_punct(";");
        Some(EnumDecl {
            name,
            variants,
            span,
        })
    }

    fn parse_const(&mut self) -> Option<ConstDecl> {
        let span = self.peek().span;
        // GZDoom: `const NAME = expr;` (no type) or `const type NAME = expr;`
        let typed = !matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == "=");
        if typed {
            let _ty = self.parse_type();
        }
        let name = self.parse_ident_name()?;
        self.expect_punct("=");
        let value = self.parse_expr(0);
        self.expect_punct(";");
        Some(ConstDecl { name, value, span })
    }

    fn parse_member(&mut self) -> Option<Member> {
        if self.eat_kw("enum") {
            return Some(Member::NestedEnum(self.parse_enum()?));
        }
        if self.eat_kw("struct") {
            return Some(Member::NestedStruct(self.parse_struct()?));
        }
        // Nested `const NAME = expr;` / `const type NAME = expr;` (vs method `const`).
        if self.check_kw("const") {
            let mut i = 1usize;
            let mut is_const_decl = false;
            while i < 24 {
                match &self.peek_n(i).kind {
                    TokenKind::Punct(p) if p == "=" => {
                        is_const_decl = true;
                        break;
                    }
                    TokenKind::Punct(p) if p == "(" || p == "{" || p == ";" => break,
                    TokenKind::Eof => break,
                    _ => i += 1,
                }
            }
            if is_const_decl {
                self.bump();
                return Some(Member::NestedConst(self.parse_const()?));
            }
        }
        if self.eat_kw("default") {
            let span = self.peek().span;
            let body = self.capture_brace_block();
            return Some(Member::Default(DefaultBlock {
                lines: body
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect(),
                span,
            }));
        }
        if self.eat_kw("states") {
            let span = self.peek().span;
            // GZDoom `States(Actor)` / `States(Overlay)` optional filter.
            if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "(") {
                self.skip_balanced('(', ')');
            }
            let body = self.capture_brace_block();
            return Some(Member::States(StatesBlock { body, span }));
        }
        if self.eat_kw("property") {
            let span = self.peek().span;
            let name = self.parse_ident_name()?;
            self.expect_punct(":");
            let mut fields = Vec::new();
            while !self.eat_punct(";") && !matches!(self.peek().kind, TokenKind::Eof) {
                if let Some(f) = self.parse_ident_name() {
                    fields.push(f);
                } else {
                    break;
                }
                self.eat_punct(",");
            }
            return Some(Member::Property { name, fields, span });
        }
        if self.eat_kw("flagdef") {
            let span = self.peek().span;
            let name = self.parse_ident_name()?;
            self.expect_punct(":");
            let field = self.parse_ident_name()?;
            self.expect_punct(",");
            let bit = self.parse_expr(0);
            self.expect_punct(";");
            return Some(Member::FlagDef {
                name,
                field,
                bit,
                span,
            });
        }

        // Method / field: gather quals then type then name, look for '(' → method
        let mut mq = MethodQuals::default();
        let mut fq = FieldQuals::default();
        loop {
            // `version("4.12") virtual void Foo(...)` / `deprecated(...) …`
            if self.check_kw("version") || self.check_kw("deprecated") {
                self.bump();
                self.skip_balanced('(', ')');
                continue;
            }
            if self.eat_kw("static") {
                mq.static_ = true;
                continue;
            }
            if self.eat_kw("virtual") {
                mq.virtual_ = true;
                continue;
            }
            if self.eat_kw("override") {
                mq.override_ = true;
                continue;
            }
            if self.eat_kw("final") {
                mq.final_ = true;
                continue;
            }
            if self.eat_kw("abstract") {
                // GZDoom abstract methods (`abstract int ModifyValue(...);`).
                mq.virtual_ = true;
                continue;
            }
            if self.eat_kw("action") {
                mq.action = true;
                continue;
            }
            if self.eat_kw("native") {
                mq.native = true;
                fq.native = true;
                continue;
            }
            if self.eat_kw("vararg") {
                // GZDoom `vararg` natives (`Screen.DrawText`, `SendNetworkCommand`).
                mq.vararg = true;
                continue;
            }
            // `readonly<Actor>` is a type. `readonly int` is a qualifier.
            if self.check_kw("readonly")
                && matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == "<")
            {
                break;
            }
            if self.eat_kw("readonly") {
                fq.readonly = true;
                continue;
            }
            if self.eat_kw("internal") {
                fq.internal = true;
                continue;
            }
            if self.eat_kw("transient") {
                fq.transient = true;
                continue;
            }
            if self.eat_kw("meta") {
                fq.meta = true;
                continue;
            }
            if self.eat_kw("private") {
                fq.access = Some("private".into());
                continue;
            }
            // UZDoom 4.15.1: field prediction qualifier.
            if matches!(&self.peek().kind, TokenKind::Ident(s) if s.eq_ignore_ascii_case("norollback"))
            {
                let span = self.bump().span;
                fq.norollback = true;
                if !version_at_least(self.version.as_deref().unwrap_or("2.3"), 4, 15, 1) {
                    self.error(span, "norollback requires ZScript version 4.15.1 or above");
                }
                continue;
            }
            if self.eat_kw("protected") {
                fq.access = Some("protected".into());
                continue;
            }
            if self.check_kw("ui")
                || self.check_kw("play")
                || self.check_kw("clearscope")
                || self.check_kw("virtualscope")
            {
                if let TokenKind::Kw(s) = self.bump().kind {
                    fq.scope = Some(s.clone());
                    mq.scope = Some(s);
                }
                continue;
            }
            if matches!(&self.peek().kind, TokenKind::Ident(s) if s.eq_ignore_ascii_case("unsafe"))
            {
                let unsafe_span = self.bump().span;
                if !version_at_least(self.version.as_deref().unwrap_or("2.3"), 4, 15, 1) {
                    self.error(
                        unsafe_span,
                        "unsafe effects require ZScript version 4.15.1 or above",
                    );
                }
                if self.eat_punct("(") {
                    let effect = self.parse_ident_name().unwrap_or_default();
                    self.expect_punct(")");
                    if effect.eq_ignore_ascii_case("const") {
                        mq.const_ = true;
                        mq.unsafe_const = true;
                    } else if effect.eq_ignore_ascii_case("clearscope") {
                        fq.scope = Some("unsafe(clearscope)".into());
                    } else {
                        self.error(unsafe_span, format!("unknown unsafe effect '{effect}'"));
                    }
                } else {
                    self.error(unsafe_span, "expected '(' after unsafe");
                }
                continue;
            }
            if self.eat_kw("const") {
                mq.const_ = true;
                continue;
            }
            break;
        }

        let ty = self.parse_type();
        // GZDoom multi-return: `vector3, int Name(...)` — commas right after the
        // first type (distinct from `int a, b;` field lists where `,` follows a name).
        if matches!(&self.peek().kind, TokenKind::Punct(p) if p == ",") {
            let save = self.pos;
            let mut extras = 0i32;
            let mut extra_types = Vec::new();
            while self.eat_punct(",") {
                let before = self.pos;
                let extra = self.parse_type();
                if self.pos == before {
                    extras = -1;
                    break;
                }
                extra_types.push(extra);
                extras += 1;
            }
            let multi_method = extras > 0
                && matches!(&self.peek().kind, TokenKind::Ident(_) | TokenKind::Kw(_))
                && matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == "(");
            if multi_method {
                self.extra_rets = extra_types;
            } else {
                self.pos = save;
            }
        }
        // `const` after type for methods
        if self.eat_kw("const") {
            mq.const_ = true;
        }
        let name = self.parse_ident_name()?;
        if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "(") {
            return Some(Member::Method(self.parse_method(name, ty, mq)?));
        }
        // field list — optional C-style array size `Args[3]`
        let mut names = vec![name];
        let mut sized = false;
        let mut ty = ty;
        if self.eat_punct("[") {
            sized = true;
            if !self.eat_punct("]") {
                let _ = self.parse_expr(0);
                self.expect_punct("]");
            }
            // `static const String Names[] = { "a", "b" };`: a class-wide table.
            if self.eat_punct("=") {
                let span = self.peek().span;
                let values = if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
                    self.parse_brace_init()
                } else {
                    vec![self.parse_expr(0)]
                };
                self.eat_punct(";");
                self.static_arrays.push(crate::ast::StaticArray {
                    owner: self.owner.clone(),
                    name: names[0].clone(),
                    elem: ty,
                    values,
                    span,
                });
                return None;
            }
            ty = TypeRef::Array(Box::new(ty));
        }
        while !sized && self.eat_punct(",") {
            if let Some(n) = self.parse_ident_name() {
                names.push(n);
            }
        }
        if fq.norollback && fq.scope.as_deref().is_some_and(|scope| scope == "play") {
            self.error(
                self.peek().span,
                "norollback cannot be used with play-scoped fields",
            );
        }
        self.expect_punct(";");
        Some(Member::Field(FieldDecl {
            ty,
            names,
            quals: fq,
            span: self.peek().span,
        }))
    }

    fn parse_method(
        &mut self,
        name: String,
        ret: TypeRef,
        quals: MethodQuals,
    ) -> Option<MethodDecl> {
        let span = self.peek().span;
        self.expect_punct("(");
        let mut params = Vec::new();
        // C-style empty: `(void)`
        if self.eat_kw("void") {
            self.expect_punct(")");
            // fall through to body / `;`
            let mut quals = quals;
            self.parse_func_const(&mut quals);
            while self.check_kw("version") || self.check_kw("deprecated") {
                self.bump();
                self.skip_balanced('(', ')');
            }
            let body = if self.eat_punct(";") {
                None
            } else if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
                Some(self.parse_block())
            } else {
                self.expect_punct(";");
                None
            };
            return Some(MethodDecl {
                extra_rets: std::mem::take(&mut self.extra_rets),
                name,
                ret,
                params,
                body,
                quals,
                span,
            });
        }
        while !self.eat_punct(")") && !matches!(self.peek().kind, TokenKind::Eof) {
            // C-style `...` varargs (GZDoom `vararg` natives).
            if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "...") {
                self.bump();
                self.eat_punct(",");
                continue;
            }
            let mut mode = None;
            // GZDoom `in out` / `out` / `ref` / `in`.
            loop {
                if self.eat_kw("out") {
                    mode = Some(match mode.as_deref() {
                        Some("in") => "in out".into(),
                        _ => "out".into(),
                    });
                    continue;
                }
                if self.eat_kw("in") {
                    mode = Some(match mode.as_deref() {
                        Some("out") => "in out".into(),
                        _ => "in".into(),
                    });
                    continue;
                }
                if self.eat_kw("ref") {
                    mode = Some("ref".into());
                    continue;
                }
                break;
            }
            let ty = self.parse_type();
            // GZDoom/C++-style `int& name` / `int &name` reference params.
            if self.eat_punct("&") {
                mode = Some(match mode.as_deref() {
                    Some(m) => format!("{m} ref"),
                    None => "ref".into(),
                });
            }
            let pname = self.parse_ident_name().unwrap_or_default();
            let mut default = None;
            if self.eat_punct("=") {
                default = Some(self.parse_expr(0));
            }
            params.push(Param {
                ty,
                name: pname,
                mode,
                default,
            });
            self.eat_punct(",");
        }
        // optional const after params
        let mut quals = quals;
        self.parse_func_const(&mut quals);
        while self.check_kw("version") || self.check_kw("deprecated") {
            self.bump();
            self.skip_balanced('(', ')');
        }
        let body = if self.eat_punct(";") {
            None
        } else if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
            Some(self.parse_block())
        } else {
            self.expect_punct(";");
            None
        };
        Some(MethodDecl {
            extra_rets: std::mem::take(&mut self.extra_rets),
            name,
            ret,
            params,
            body,
            quals,
            span,
        })
    }

    fn parse_type(&mut self) -> TypeRef {
        // GZDoom `@Type` — native/engine pointer (treated as the named type).
        let _at = self.eat_punct("@");
        if self.eat_kw("void") {
            return TypeRef::Void;
        }
        // `Function<clearscope int(int, int)>` / similar generics with nested `()`.
        if matches!(
            &self.peek().kind,
            TokenKind::Ident(s) | TokenKind::Kw(s) if s.eq_ignore_ascii_case("function")
        ) {
            self.bump();
            if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "<") {
                self.skip_angle_group();
            }
            return TypeRef::Named("Function".into());
        }
        if self.eat_kw("array") {
            self.expect_punct("<");
            let inner = self.parse_type();
            self.expect_punct(">");
            return TypeRef::Array(Box::new(inner));
        }
        if self.eat_kw("map") {
            self.expect_punct("<");
            let k = self.parse_type();
            self.expect_punct(",");
            let v = self.parse_type();
            self.expect_punct(">");
            return TypeRef::Map(Box::new(k), Box::new(v));
        }
        if self.eat_kw("readonly") {
            self.expect_punct("<");
            let inner = self.parse_type();
            self.expect_punct(">");
            return TypeRef::Readonly(Box::new(inner));
        }
        if self.eat_kw("class") {
            if self.eat_punct("<") {
                let n = self.parse_ident_name().unwrap_or_default();
                self.expect_punct(">");
                return TypeRef::ClassOf(n);
            }
            return TypeRef::Named("class".into());
        }
        // keyword types or ident
        match &self.peek().kind {
            TokenKind::Kw(k) | TokenKind::Ident(k) => {
                let name = k.clone();
                self.bump();
                if name.eq_ignore_ascii_case("mapiterator")
                    && matches!(&self.peek().kind, TokenKind::Punct(p) if p == "<")
                {
                    self.expect_punct("<");
                    let k = self.parse_type();
                    self.expect_punct(",");
                    let v = self.parse_type();
                    self.expect_punct(">");
                    return TypeRef::MapIterator(Box::new(k), Box::new(v));
                }
                // `MapIterator<Name, Service>` and other generic classes.
                if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "<") {
                    self.skip_angle_group();
                }
                // arrays: Type[]
                if self.eat_punct("[") {
                    self.eat_punct("]");
                    return TypeRef::Array(Box::new(TypeRef::Named(name)));
                }
                TypeRef::Named(name)
            }
            _ => TypeRef::Void,
        }
    }

    fn parse_block(&mut self) -> Block {
        let span = self.peek().span;
        self.expect_punct("{");
        let mut stmts = Vec::new();
        while !matches!(&self.peek().kind, TokenKind::Punct(p) if p == "}")
            && !matches!(self.peek().kind, TokenKind::Eof)
        {
            self.parse_stmt_into(&mut stmts);
        }
        self.expect_punct("}");
        Block { stmts, span }
    }

    /// One statement plus anything it queued in `pending`.
    fn parse_stmt_into(&mut self, out: &mut Vec<Stmt>) {
        let stmt = self.parse_stmt();
        out.push(stmt);
        out.append(&mut self.pending);
    }

    /// A statement used where only one is allowed (`if (c) stmt`): wrapped in
    /// a block when it queued follow-up statements.
    fn parse_single_stmt(&mut self) -> Vec<Stmt> {
        let mut out = Vec::new();
        self.parse_stmt_into(&mut out);
        out
    }

    /// `{ a, b, c }` array initializer (trailing comma allowed); nested braces
    /// are flattened.
    fn parse_brace_init(&mut self) -> Vec<Expr> {
        let mut elems = Vec::new();
        self.expect_punct("{");
        while !self.eat_punct("}") && !matches!(self.peek().kind, TokenKind::Eof) {
            if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
                elems.extend(self.parse_brace_init());
            } else {
                let before = self.pos;
                elems.push(self.parse_expr(0));
                if self.pos == before {
                    self.bump();
                }
            }
            self.eat_punct(",");
        }
        elems
    }

    /// Queue `name.Push(elem)` for each initializer element.
    fn queue_array_pushes(&mut self, name: &str, elems: Vec<Expr>, span: Span) {
        for e in elems {
            let espan = e.span();
            self.pending.push(Stmt::Expr {
                expr: Expr::Call {
                    callee: Box::new(Expr::Member {
                        obj: Box::new(Expr::Ident(name.to_owned(), span)),
                        field: "Push".into(),
                        span: espan,
                    }),
                    args: vec![e],
                    span: espan,
                },
                span: espan,
            });
        }
    }

    fn parse_stmt(&mut self) -> Stmt {
        let span = self.peek().span;
        // Empty statement (GZDoom tolerates stray `;`).
        if self.eat_punct(";") {
            return Stmt::Expr {
                expr: Expr::Int(0, span),
                span,
            };
        }
        // Multi-assign / destructure: `[a, b] = expr;`
        if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "[") {
            let save = self.pos;
            self.bump();
            let mut names = Vec::new();
            let mut stores = Vec::new();
            let mut ok = true;
            while !self.eat_punct("]") && !matches!(self.peek().kind, TokenKind::Eof) {
                // A plain name (a local, a field such as `RealFloor`, `_`) is
                // assigned in place; `v.x` lands in a temporary, then is stored.
                let simple = matches!(&self.peek().kind, TokenKind::Ident(_) | TokenKind::Kw(_))
                    && matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == "," || p == "]");
                if simple {
                    names.push(self.parse_ident_name().unwrap_or_default());
                } else {
                    // `[v.x, v.y] = f();`: land in a temporary, then store it.
                    let before = self.pos;
                    let target = self.parse_expr(0);
                    if self.pos == before {
                        ok = false;
                        break;
                    }
                    let tmp = format!("__room_mt{}", self.temps);
                    self.temps += 1;
                    let tspan = target.span();
                    stores.push(Stmt::Expr {
                        expr: Expr::Binary {
                            op: "=".into(),
                            lhs: Box::new(target),
                            rhs: Box::new(Expr::Ident(tmp.clone(), tspan)),
                            span: tspan,
                        },
                        span: tspan,
                    });
                    names.push(tmp);
                }
                self.eat_punct(",");
            }
            if ok && self.eat_punct("=") {
                let init = self.parse_expr(0);
                self.eat_punct(";");
                self.pending.extend(stores);
                return Stmt::Local {
                    ty: None,
                    names,
                    init: Some(init),
                    extra_inits: Vec::new(),
                    span,
                };
            }
            self.pos = save;
        }
        if self.eat_kw("return") {
            let mut values = Vec::new();
            if !self.eat_punct(";") {
                values.push(self.parse_expr(0));
                while self.eat_punct(",") {
                    values.push(self.parse_expr(0));
                }
                self.expect_punct(";");
            }
            return Stmt::Return { values, span };
        }
        if self.eat_kw("break") {
            self.eat_punct(";");
            return Stmt::Break(span);
        }
        if self.eat_kw("continue") {
            self.eat_punct(";");
            return Stmt::Continue(span);
        }
        if self.eat_kw("if") {
            self.expect_punct("(");
            let cond = self.parse_expr(0);
            self.expect_punct(")");
            let then_b = if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
                self.parse_block()
            } else {
                Block {
                    stmts: self.parse_single_stmt(),
                    span,
                }
            };
            let else_b = if self.eat_kw("else") {
                Some(
                    if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
                        self.parse_block()
                    } else {
                        Block {
                            stmts: self.parse_single_stmt(),
                            span,
                        }
                    },
                )
            } else {
                None
            };
            return Stmt::If {
                cond,
                then_b,
                else_b,
                span,
            };
        }
        if self.eat_kw("while") {
            self.expect_punct("(");
            let cond = self.parse_expr(0);
            self.expect_punct(")");
            let body = self.parse_block_or_stmt();
            return Stmt::While { cond, body, span };
        }
        if self.eat_kw("do") {
            let body = self.parse_block_or_stmt();
            let until = self.eat_kw("until");
            if !until {
                self.expect_kw_or_ident("while");
            }
            self.expect_punct("(");
            let cond = self.parse_expr(0);
            self.expect_punct(")");
            self.eat_punct(";");
            return Stmt::DoWhile {
                body,
                cond,
                until,
                span,
            };
        }
        if self.eat_kw("switch") {
            self.expect_punct("(");
            let cond = self.parse_expr(0);
            self.expect_punct(")");
            let body = self.parse_block();
            return Stmt::Switch { cond, body, span };
        }
        if self.eat_kw("case") {
            let value = self.parse_expr(0);
            self.expect_punct(":");
            return Stmt::Case {
                value: Some(value),
                span,
            };
        }
        // `default:` label (not class Default { }).
        if self.check_kw("default")
            && matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == ":")
        {
            self.bump();
            self.bump();
            return Stmt::Case { value: None, span };
        }
        if self.eat_kw("foreach") {
            // `foreach (Type name : expr)` / `foreach (name : expr)` /
            // `foreach (key, value : map)`.
            self.expect_punct("(");
            if matches!(&self.peek().kind, TokenKind::Ident(_) | TokenKind::Kw(_))
                && matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == ",")
            {
                let key = self.parse_ident_name();
                self.eat_punct(",");
                let value = self.parse_ident_name();
                self.expect_punct(":");
                let iter = self.parse_expr(0);
                self.expect_punct(")");
                let body = self.parse_block_or_stmt();
                return foreach_while(
                    vec![key.unwrap_or_default(), value.unwrap_or_default()],
                    iter,
                    body,
                    span,
                );
            }
            let save = self.pos;
            let _ = self.parse_type();
            // `foreach (name : iter)` makes parse_type consume the loop variable.
            let loop_name = if matches!(&self.peek().kind, TokenKind::Ident(_) | TokenKind::Kw(_))
                && matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == ":")
            {
                self.parse_ident_name()
            } else {
                self.pos = save;
                self.parse_ident_name()
            };
            self.expect_punct(":");
            let iter = self.parse_expr(0);
            self.expect_punct(")");
            let body = self.parse_block_or_stmt();
            // Desugar to a while. The loop variable is a local so typeck can see it.
            return foreach_while(vec![loop_name.unwrap_or_default()], iter, body, span);
        }
        if self.eat_kw("for") {
            self.expect_punct("(");
            let init = if self.eat_punct(";") {
                None
            } else {
                // Local decl or expression statement (without requiring trailing `;` eat twice).
                let s = self.parse_for_init();
                Some(Box::new(s))
            };
            let cond = if self.eat_punct(";") {
                None
            } else {
                let e = Some(self.parse_expr(0));
                self.eat_punct(";");
                e
            };
            let step = if self.eat_punct(")") {
                None
            } else {
                let e = Some(self.parse_comma_expr());
                self.expect_punct(")");
                e
            };
            let body = self.parse_block_or_stmt();
            return Stmt::For {
                init,
                cond,
                step,
                body,
                span,
            };
        }
        if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
            return Stmt::Block(self.parse_block());
        }
        // `static const Type name[] = { … };` inside methods (GZDoom).
        {
            let save = self.pos;
            let mut quals = false;
            while self.eat_kw("static")
                || self.eat_kw("const")
                || self.eat_kw("private")
                || self.eat_kw("protected")
                || (self.check_kw("readonly")
                    && !matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == "<")
                    && self.eat_kw("readonly"))
            {
                quals = true;
            }
            if quals {
                let mut ty = self.parse_type();
                let name = self.parse_ident_name().unwrap_or_default();
                if self.eat_punct("[") {
                    if !self.eat_punct("]") {
                        let _ = self.parse_expr(0);
                        self.expect_punct("]");
                    }
                    ty = TypeRef::Array(Box::new(ty));
                }
                let mut init = None;
                if self.eat_punct("=") {
                    if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
                        let elems = self.parse_brace_init();
                        self.queue_array_pushes(&name, elems, span);
                    } else {
                        init = Some(self.parse_expr(0));
                    }
                }
                self.eat_punct(";");
                return Stmt::Local {
                    ty: Some(ty),
                    names: vec![name],
                    init,
                    extra_inits: Vec::new(),
                    span,
                };
            }
            self.pos = save;
        }
        if self.eat_kw("let") {
            // `let name = …;` or destructuring `let [a, b] = …;`
            let names = if self.eat_punct("[") {
                let mut names = Vec::new();
                while !self.eat_punct("]") && !matches!(self.peek().kind, TokenKind::Eof) {
                    if let Some(n) = self.parse_ident_name() {
                        names.push(n);
                    }
                    self.eat_punct(",");
                }
                names
            } else {
                vec![self.parse_ident_name().unwrap_or_default()]
            };
            let init = if self.eat_punct("=") {
                Some(self.parse_expr(0))
            } else {
                None
            };
            self.expect_punct(";");
            return Stmt::Local {
                ty: None,
                names,
                init,
                extra_inits: Vec::new(),
                span,
            };
        }
        if let Some(local) = self.try_parse_local(span, true) {
            return local;
        }
        let expr = self.parse_expr(0);
        self.eat_punct(";");
        Stmt::Expr { expr, span }
    }

    fn expect_kw_or_ident(&mut self, k: &str) {
        if self.eat_kw(k) {
            return;
        }
        if matches!(&self.peek().kind, TokenKind::Ident(s) if s.eq_ignore_ascii_case(k)) {
            self.bump();
            return;
        }
        self.error(self.peek().span, format!("expected `{k}`"));
    }

    /// `for (` init — local or expr, consuming the trailing `;`.
    fn parse_for_init(&mut self) -> Stmt {
        let span = self.peek().span;
        if self.eat_kw("let") {
            let name = self.parse_ident_name().unwrap_or_default();
            let init = if self.eat_punct("=") {
                Some(self.parse_expr(0))
            } else {
                None
            };
            self.eat_punct(";");
            return Stmt::Local {
                ty: None,
                names: vec![name],
                init,
                extra_inits: Vec::new(),
                span,
            };
        }
        if let Some(local) = self.try_parse_local(span, true) {
            return local;
        }
        let expr = self.parse_comma_expr();
        self.eat_punct(";");
        Stmt::Expr { expr, span }
    }

    /// Expression that allows C comma operator (`a = 0, b = 1`).
    fn parse_comma_expr(&mut self) -> Expr {
        let mut e = self.parse_expr(0);
        while self.eat_punct(",") {
            let rhs = self.parse_expr(0);
            let span = e.span();
            e = Expr::Binary {
                op: ",".into(),
                lhs: Box::new(e),
                rhs: Box::new(rhs),
                span,
            };
        }
        e
    }

    /// Try `Type name [= expr] [, name…];` — returns `None` and rewinds on failure.
    fn try_parse_local(&mut self, span: Span, require_semi: bool) -> Option<Stmt> {
        let save = self.pos;
        let mut maybe_ty = self.parse_type();
        if !matches!(&self.peek().kind, TokenKind::Ident(_) | TokenKind::Kw(_)) {
            self.pos = save;
            return None;
        }
        // Method call / cast `Type(` — not a local.
        if matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == "(") {
            // Still could be `Type name(` — no: name then `(`. If peek is name and peek_n(1) is `(`,
            // it's a call starting with type-looking ident — not a local.
            // Locals: name then `=`, `;`, or `,`.
        }
        let name = match &self.peek().kind {
            TokenKind::Ident(s) | TokenKind::Kw(s) => s.clone(),
            _ => {
                self.pos = save;
                return None;
            }
        };
        // After type+name, local if `=`, `;`, `,`, or a C array `name[n]`.
        match &self.peek_n(1).kind {
            TokenKind::Punct(p) if p == "=" || p == ";" || p == "," || p == "[" => {}
            _ => {
                self.pos = save;
                return None;
            }
        }
        self.bump(); // name
        let mut fixed_size = None;
        if self.eat_punct("[") {
            if !self.eat_punct("]") {
                fixed_size = Some(self.parse_expr(0));
                self.expect_punct("]");
            }
            maybe_ty = TypeRef::Array(Box::new(maybe_ty));
        }
        let mut names = vec![name];
        let mut init = None;
        let mut brace = None;
        if self.eat_punct("=") {
            if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
                // `int a[4] = { 1, 2, 3, 4 };`: start empty, push each value.
                brace = Some(self.parse_brace_init());
            } else {
                init = Some(self.parse_expr(0));
            }
        } else if fixed_size.is_some() {
            // `bool locks[6];`: a fixed array keeps its size as the initializer.
            init = fixed_size;
        }
        let mut extra_inits = Vec::new();
        while self.eat_punct(",") {
            if let Some(n) = self.parse_ident_name() {
                names.push(n);
            } else {
                break;
            }
            if self.eat_punct("=") {
                extra_inits.push(Some(self.parse_expr(0)));
            } else {
                extra_inits.push(None);
            }
        }
        if require_semi && !self.eat_punct(";") {
            self.pos = save;
            return None;
        }
        if let Some(elems) = brace {
            let first = names[0].clone();
            self.queue_array_pushes(&first, elems, span);
        }
        Some(Stmt::Local {
            ty: Some(maybe_ty),
            names,
            init,
            extra_inits,
            span,
        })
    }

    fn parse_block_or_stmt(&mut self) -> Block {
        if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
            self.parse_block()
        } else {
            let span = self.peek().span;
            Block {
                stmts: self.parse_single_stmt(),
                span,
            }
        }
    }

    fn binding_power(op: &str) -> Option<(u8, u8)> {
        Some(match op {
            "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" => (2, 1),
            "?" => (4, 3), // ternary handled specially
            "||" => (5, 6),
            "&&" => (7, 8),
            "|" => (9, 10),
            "^" => (11, 12),
            "&" => (13, 14),
            "==" | "!=" | "~==" => (15, 16),
            "<" | "<=" | ">" | ">=" | "<=>" | "<>=" | "is" => (17, 18),
            "<<" | ">>" | ">>>" => (19, 20),
            ".." => (21, 22),
            "+" | "-" => (23, 24),
            "*" | "/" | "%" | "cross" | "dot" => (25, 26),
            "**" => (27, 28),
            _ => return None,
        })
    }

    fn parse_expr(&mut self, min_bp: u8) -> Expr {
        let mut lhs = self.parse_prefix();
        loop {
            let op = match &self.peek().kind {
                TokenKind::Punct(p) => p.clone(),
                TokenKind::Kw(k) if k == "is" || k == "dot" || k == "cross" => k.clone(),
                _ => break,
            };
            // postfix already handled in parse_prefix trail
            let Some((l_bp, r_bp)) = Self::binding_power(&op) else {
                break;
            };
            if l_bp < min_bp {
                break;
            }
            let op_span = self.peek().span;
            self.bump();
            let file_version = self.version.as_deref().unwrap_or("2.3");
            if op == "<=>" && !version_at_least(file_version, 4, 15, 1) {
                self.error(op_span, "<=> requires ZScript version 4.15.1 or above");
            } else if op == "<>=" && version_at_least(file_version, 4, 15, 1) {
                self.warn(op_span, "<>= is deprecated in favor of <=>");
            }
            if op == "?" {
                let then_e = self.parse_expr(0);
                self.expect_punct(":");
                let else_e = self.parse_expr(r_bp);
                let span = lhs.span();
                lhs = Expr::Ternary {
                    cond: Box::new(lhs),
                    then_e: Box::new(then_e),
                    else_e: Box::new(else_e),
                    span,
                };
                continue;
            }
            let rhs = self.parse_expr(r_bp);
            let span = lhs.span();
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        lhs
    }

    fn parse_prefix(&mut self) -> Expr {
        let span = self.peek().span;
        if self.eat_punct("(") {
            // Prefer `(Type)expr` cast before grouping so `class<T>` is not
            // misread as a comparison.
            let save = self.pos;
            let ty = self.parse_type();
            if self.eat_punct(")") && self.looks_like_expr_start() {
                let e = self.parse_prefix();
                let ty_name = type_ref_name(&ty);
                return self.parse_postfix(Expr::Cast {
                    ty: ty_name,
                    expr: Box::new(e),
                    span,
                });
            }
            self.pos = save;
            // Vector/tuple `(x, y)` or grouped `(expr)`.
            let first = self.parse_expr(0);
            if self.eat_punct(",") {
                let mut elems = vec![first];
                loop {
                    elems.push(self.parse_expr(0));
                    if !self.eat_punct(",") {
                        break;
                    }
                }
                self.expect_punct(")");
                return self.parse_postfix(Expr::Tuple { elems, span });
            }
            self.expect_punct(")");
            return self.parse_postfix(first);
        }
        // unary (incl. unary `+`)
        if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "-" || p == "+" || p == "!" || p == "~" || p == "++" || p == "--")
        {
            let op = if let TokenKind::Punct(p) = self.bump().kind {
                p
            } else {
                String::new()
            };
            let expr = self.parse_prefix();
            return Expr::Unary {
                op,
                expr: Box::new(expr),
                span,
            };
        }
        if self.eat_kw("new") {
            if self.eat_punct("(") {
                let class = match self.bump().kind {
                    TokenKind::String(s) | TokenKind::Ident(s) | TokenKind::Name(s) => s,
                    _ => String::new(),
                };
                self.expect_punct(")");
                return self.parse_postfix(Expr::New { class, span });
            }
            let class = self.parse_ident_name().unwrap_or_default();
            return self.parse_postfix(Expr::New { class, span });
        }
        let primary = match self.bump().kind {
            TokenKind::Int(v) => Expr::Int(v, span),
            TokenKind::Float(v) => Expr::Float(v, span),
            TokenKind::String(s) => Expr::String(s, span),
            TokenKind::Name(s) => Expr::Name(s, span),
            TokenKind::Ident(s) | TokenKind::Kw(s) => Expr::Ident(s, span),
            other => {
                self.error(span, format!("expected expression, got {other:?}"));
                Expr::Int(0, span)
            }
        };
        self.parse_postfix(primary)
    }

    fn parse_postfix(&mut self, mut lhs: Expr) -> Expr {
        loop {
            if self.eat_punct(".") {
                let field = self.parse_ident_name().unwrap_or_default();
                let span = lhs.span();
                lhs = Expr::Member {
                    obj: Box::new(lhs),
                    field,
                    span,
                };
                continue;
            }
            if self.eat_punct("[") {
                let index = self.parse_expr(0);
                self.expect_punct("]");
                let span = lhs.span();
                lhs = Expr::Index {
                    arr: Box::new(lhs),
                    index: Box::new(index),
                    span,
                };
                continue;
            }
            if self.eat_punct("(") {
                let mut args = Vec::new();
                while !self.eat_punct(")") && !matches!(self.peek().kind, TokenKind::Eof) {
                    // Named args: `name: expr` (GZDoom), kept as a `named:` pair so
                    // the lowerer can place it at the parameter of that name.
                    if matches!(&self.peek().kind, TokenKind::Ident(_) | TokenKind::Kw(_))
                        && matches!(&self.peek_n(1).kind, TokenKind::Punct(p) if p == ":")
                    {
                        let name_tok = self.bump();
                        self.bump();
                        let name = match name_tok.kind {
                            TokenKind::Ident(n) | TokenKind::Kw(n) => n,
                            _ => String::new(),
                        };
                        let value = self.parse_expr(0);
                        let span = value.span();
                        args.push(Expr::Binary {
                            op: NAMED_ARG.into(),
                            lhs: Box::new(Expr::Ident(name, name_tok.span)),
                            rhs: Box::new(value),
                            span,
                        });
                        self.eat_punct(",");
                        continue;
                    }
                    args.push(self.parse_expr(0));
                    self.eat_punct(",");
                }
                let span = lhs.span();
                lhs = Expr::Call {
                    callee: Box::new(lhs),
                    args,
                    span,
                };
                continue;
            }
            // Postfix ++ / --
            if matches!(&self.peek().kind, TokenKind::Punct(p) if p == "++" || p == "--") {
                let op = if let TokenKind::Punct(p) = self.bump().kind {
                    p
                } else {
                    String::new()
                };
                let span = lhs.span();
                lhs = Expr::Unary {
                    op: format!("post{op}"),
                    expr: Box::new(lhs),
                    span,
                };
                continue;
            }
            break;
        }
        lhs
    }

    fn looks_like_expr_start(&self) -> bool {
        match &self.peek().kind {
            TokenKind::Ident(_)
            | TokenKind::Kw(_)
            | TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::String(_)
            | TokenKind::Name(_) => true,
            TokenKind::Punct(p) => {
                matches!(p.as_str(), "(" | "-" | "!" | "~" | "++" | "--" | "[")
            }
            _ => false,
        }
    }

    fn capture_brace_block(&mut self) -> String {
        if !matches!(&self.peek().kind, TokenKind::Punct(p) if p == "{") {
            return String::new();
        }
        // reconstruct from token spans is hard; walk tokens and rebuild text from kinds
        self.bump(); // {
        let mut depth = 1i32;
        let mut out = String::from("{\n");
        while depth > 0 && !matches!(self.peek().kind, TokenKind::Eof) {
            let t = self.bump();
            match &t.kind {
                TokenKind::Punct(p) if p == "{" => {
                    depth += 1;
                    out.push('{');
                }
                TokenKind::Punct(p) if p == "}" => {
                    depth -= 1;
                    if depth > 0 {
                        out.push('}');
                    }
                }
                TokenKind::Punct(p) => out.push_str(p),
                TokenKind::Ident(s) | TokenKind::Kw(s) => {
                    out.push(' ');
                    out.push_str(s);
                    out.push(' ');
                }
                TokenKind::Int(v) => out.push_str(&v.to_string()),
                TokenKind::Float(v) => out.push_str(&v.to_string()),
                TokenKind::String(s) => {
                    out.push('"');
                    out.push_str(s);
                    out.push('"');
                }
                TokenKind::Name(s) => {
                    out.push('\'');
                    out.push_str(s);
                    out.push('\'');
                }
                TokenKind::Eof => break,
            }
            out.push(' ');
        }
        out.push('}');
        out
    }

    fn skip_balanced(&mut self, open: char, close: char) {
        let o = open.to_string();
        let c = close.to_string();
        if !self.eat_punct(&o) {
            return;
        }
        let mut depth = 1i32;
        while depth > 0 && !matches!(self.peek().kind, TokenKind::Eof) {
            if matches!(&self.peek().kind, TokenKind::Punct(p) if p == &o) {
                depth += 1;
            } else if matches!(&self.peek().kind, TokenKind::Punct(p) if p == &c) {
                depth -= 1;
            }
            self.bump();
        }
    }

    /// Skip `< … >` tracking nested `()` / `<>` (e.g. `Function<clearscope int(int,int)>`).
    fn skip_angle_group(&mut self) {
        if !self.eat_punct("<") {
            return;
        }
        let mut angle = 1i32;
        let mut paren = 0i32;
        while angle > 0 && !matches!(self.peek().kind, TokenKind::Eof) {
            match &self.peek().kind {
                TokenKind::Punct(p) if p == "<" && paren == 0 => angle += 1,
                TokenKind::Punct(p) if p == ">" && paren == 0 => angle -= 1,
                TokenKind::Punct(p) if p == "(" => paren += 1,
                TokenKind::Punct(p) if p == ")" => paren -= 1,
                _ => {}
            }
            self.bump();
        }
    }
}

fn type_ref_name(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Named(n) | TypeRef::ClassOf(n) => n.clone(),
        TypeRef::Array(inner) => format!("Array<{}>", type_ref_name(inner)),
        TypeRef::Map(k, v) => format!("Map<{},{}>", type_ref_name(k), type_ref_name(v)),
        TypeRef::MapIterator(k, v) => {
            format!("MapIterator<{},{}>", type_ref_name(k), type_ref_name(v))
        }
        TypeRef::Readonly(inner) => format!("readonly<{}>", type_ref_name(inner)),
        TypeRef::Void => "void".into(),
    }
}

impl Parser {
    /// Trailing `const` / UZDoom 4.15.1 `unsafe(const)` after a parameter list.
    fn parse_func_const(&mut self, quals: &mut MethodQuals) {
        if self.eat_kw("const") {
            quals.const_ = true;
            return;
        }
        let is_unsafe =
            matches!(&self.peek().kind, TokenKind::Ident(s) if s.eq_ignore_ascii_case("unsafe"));
        if !is_unsafe {
            return;
        }
        let span = self.bump().span;
        if !version_at_least(self.version.as_deref().unwrap_or("2.3"), 4, 15, 1) {
            self.error(
                span,
                "unsafe effects require ZScript version 4.15.1 or above",
            );
        }
        if self.eat_punct("(") {
            let ok = self.eat_kw("const")
                || self
                    .parse_ident_name()
                    .is_some_and(|e| e.eq_ignore_ascii_case("const"));
            self.expect_punct(")");
            if ok {
                quals.const_ = true;
                quals.unsafe_const = true;
            } else {
                self.error(span, "expected unsafe(const) after a parameter list");
            }
        } else {
            self.error(span, "expected '(' after unsafe");
        }
    }
}

pub(crate) fn version_at_least(version: &str, major: u32, minor: u32, patch: u32) -> bool {
    let mut parts = version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u32>().unwrap_or(0));
    [
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    ] >= [major, minor, patch]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn parse_trailing_native_class() {
        let r = parse_file(
            "base.zs",
            r#"
version "4.12.0"
class Object native
{
    native static Object New(class<Object> type);
    native void Destroy();
}
class Thinker : Object native {}
class Actor : Thinker native
{
    native int health;
}
"#,
        );
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(r.unit.decls.len(), 3);
        match &r.unit.decls[0] {
            Decl::Class(c) => assert!(c.flags.native, "Object should be native"),
            _ => panic!("expected class"),
        }
    }

    #[test]
    fn parse_struct_native_play_version_and_versioned_method() {
        let r = parse_file(
            "events.zs",
            r#"
struct NetworkCommand native play version("4.12")
{
    native readonly int Player;
    native int ReadInt();
}
class StaticEventHandler
{
    version("4.12") virtual void NetworkCommandProcess(NetworkCommand cmd) {}
    version("4.12") clearscope static native vararg bool SendNetworkCommand(Name cmd, ...);
}
"#,
        );
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(r.unit.decls.len(), 2);
    }

    #[test]
    fn parse_gzdoom_events_excerpt_fixture() {
        // Local-only fixture (not in git); skip when absent, e.g. on CI.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/zscript/gzdoom_events_excerpt.zs"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("skip: {path} not present");
            return;
        };
        let r = parse_file("gzdoom_events_excerpt.zs", &text);
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn parse_untyped_const_and_extend_struct() {
        let r = parse_file(
            "t.zs",
            r#"
const FLAG_NO_CHANGE = -1;
const MAXPLAYERS = 64;
const HR_SHADOW = (0x6800 / 65536.);
extend struct _
{
    native int gametic;
}
"#,
        );
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn parse_multi_return_and_let_destructure() {
        let r = parse_file(
            "t.zs",
            r#"
class Level
{
    deprecated("3.8", "Use Level.PickDeathMatchStart() instead") static vector3, int G_PickDeathmatchStart()
    {
        let [a, b] = level.PickDeathmatchStart();
        return a, b;
    }
    void Draw()
    {
        statusBar.DrawTexture(tex, (x, y), 1., (32, 32));
    }
    enum EStatnums { STAT_INFO, STAT_DEFAULT = 100 }
}
"#,
        );
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn parse_minimal_class() {
        let r = parse_file(
            "t.zs",
            r#"
version "3.2.3"
class Foo : Actor
{
    int cool;
    override void Tick() { cool = cool + 1; }
}
"#,
        );
        assert!(
            r.diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count()
                == 0,
            "{:?}",
            r.diagnostics
        );
        assert_eq!(r.unit.decls.len(), 1);
    }

    #[test]
    fn parse_bd_like_handler() {
        let src = r#"
version "3.2.3"
class Script_DamageMult : EventHandler
{
    override void PlayerEntered(PlayerEvent e)
    {
        if (skill >= 6)
        {
            players[e.PlayerNumber].mo.GiveInventory("Damage_Control", 1);
        }
    }
}
"#;
        let r = parse_file("DamageHandler.zc", src);
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.format_line())
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn parse_map_foreach_and_norollback_field() {
        let r = parse_file(
            "t.zs",
            r#"version "4.15.1"
class Scoreboard
{
    void Draw(Map<int, int> teamScores)
    {
        foreach (t, score : teamScores) { }
    }
}
struct PlayerInfo
{
    norollback clearscope Inventory InvSel;
}
"#,
        );
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.format_line())
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
    }
}

#[cfg(test)]
mod second_root_tests {
    use super::*;

    /// Two archives each with a root `zscript` lump: the second root's
    /// `#include`s must be expanded too (Brutal Doom + another ZScript mod).
    #[test]
    fn a_second_root_expands_its_includes() {
        let sources = vec![
            (
                "ZSCRIPT".to_owned(),
                "version \"4.0\"\nclass First {}\n".to_owned(),
            ),
            (
                "zscript.zc".to_owned(),
                "version \"4.0\"\n#include \"zscript/b.zc\"\n".to_owned(),
            ),
            ("zscript/b.zc".to_owned(), "class Second {}\n".to_owned()),
        ];
        let r = parse_sources(&sources);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        let names: Vec<String> = r
            .unit
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Class(c) => Some(c.name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            ["First", "Second"],
            "Second is parsed once, via the include"
        );
    }
}

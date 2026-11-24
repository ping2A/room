//! ACS parser: tokens → a small syntax tree. Names are lower-cased (ACS is
//! case-insensitive).

use super::Diag;
use super::lex::{Tok, Token};

#[derive(Clone, Debug)]
pub(crate) struct Expr {
    pub kind: ExprKind,
    pub file: u16,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub(crate) enum ExprKind {
    Num(i32),
    Str(String),
    Name(String),
    Index(String, Vec<Expr>),
    Call(String, Vec<Expr>),
    /// `Print(s:"a", d:x)`; `extra` holds the arguments after `;` (HudMessage).
    Print {
        name: String,
        items: Vec<(char, Expr)>,
        extra: Vec<Expr>,
    },
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    IncDec {
        inc: bool,
        post: bool,
        target: Box<Expr>,
    },
    /// `CreateTranslation(n, ranges…)`: (TRANSLATIONRANGEn p-code, values).
    Translation(Box<Expr>, Vec<(u8, Vec<Expr>)>),
}

#[derive(Clone, Debug)]
pub(crate) enum Init {
    Expr(Expr),
    List(Vec<Init>, u32),
}

#[derive(Clone, Debug)]
pub(crate) struct VarDecl {
    pub name: String,
    /// Array dimensions; `None` = size taken from the initializer.
    pub dims: Vec<Option<Expr>>,
    pub init: Option<Init>,
    /// Declared `str` (kept for diagnostics; values are string indices either way).
    #[allow(dead_code)]
    pub is_str: bool,
    pub file: u16,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub(crate) enum Stmt {
    Block(Vec<Stmt>),
    Local(Vec<VarDecl>),
    Expr(Expr),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While {
        cond: Expr,
        body: Box<Stmt>,
        until: bool,
    },
    Do {
        body: Box<Stmt>,
        cond: Expr,
        until: bool,
    },
    For {
        init: Vec<Stmt>,
        cond: Option<Expr>,
        step: Vec<Stmt>,
        body: Box<Stmt>,
    },
    Switch {
        value: Expr,
        body: Vec<SwitchItem>,
    },
    Break(u16, u32),
    Continue(u16, u32),
    Return(Option<Expr>, u16, u32),
    Restart,
    Terminate,
    Suspend,
    Empty,
}

#[derive(Clone, Debug)]
pub(crate) enum SwitchItem {
    Case(Expr),
    Default(u16, u32),
    Stmt(Stmt),
}

#[derive(Clone, Debug)]
pub(crate) enum ScriptId {
    Num(Expr),
    Name(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    World,
    Global,
}

#[derive(Clone, Debug)]
pub(crate) enum Top {
    MapVar(VarDecl),
    ScopeVar {
        scope: Scope,
        index: Expr,
        name: String,
        array: bool,
        file: u16,
        line: u32,
    },
    Script {
        id: ScriptId,
        params: Vec<String>,
        kind: u16,
        flags: u16,
        body: Stmt,
        file: u16,
        line: u32,
    },
    Function {
        name: String,
        params: Vec<String>,
        returns: bool,
        body: Stmt,
        file: u16,
        line: u32,
    },
}

pub(crate) struct Program {
    pub tops: Vec<Top>,
    /// Per top: `Some(n)` when it comes from the `n`-th `#import` (1-based,
    /// index into `imports` + 1) — declared, not compiled here.
    pub import_of: Vec<Option<u8>>,
    /// `#library "NAME"` of this module.
    pub library: Option<String>,
    /// Library names of the `#import`ed files, in order (the `LOAD` chunk).
    pub imports: Vec<String>,
    /// `special` declarations: name, number (negative = CallFunc), min, max args.
    pub specials: Vec<(String, i32, u8, u8)>,
}

const TYPES: &[&str] = &["int", "str", "bool", "fixed"];
const PRINTS: &[&str] = &[
    "print",
    "printbold",
    "log",
    "hudmessage",
    "hudmessagebold",
    "strparam",
];
const SCRIPT_TYPES: &[(&str, u16)] = &[
    ("open", 1),
    ("respawn", 2),
    ("death", 3),
    ("enter", 4),
    ("pickup", 5),
    ("bluereturn", 6),
    ("redreturn", 7),
    ("whitereturn", 8),
    ("lightning", 12),
    ("unloading", 13),
    ("disconnect", 14),
    ("return", 15),
    ("event", 16),
    ("kill", 17),
    ("reopen", 18),
];

pub(crate) fn parse(toks: Vec<Token>) -> Result<Program, Diag> {
    let mut p = Parser {
        toks,
        pos: 0,
        specials: Vec::new(),
    };
    let mut tops = Vec::new();
    let mut import_of = Vec::new();
    let mut library = None;
    let mut imports: Vec<String> = Vec::new();
    // Open `#import`s: library index once its `#library` line is seen.
    let mut open: Vec<(Option<u8>, u16)> = Vec::new();
    while !p.at_eof() {
        if let Tok::Directive(d) = p.peek().clone() {
            let (file, line) = p.here();
            p.bump();
            match d.as_str() {
                "importbegin" => open.push((None, file)),
                "importend" => {
                    if let Some((None, f)) = open.pop() {
                        return Err(Diag {
                            file: f,
                            line: 1,
                            message: "an #import'ed file needs a #library \"NAME\" line".into(),
                        });
                    }
                }
                "library" => {
                    let Tok::Str(name) = p.bump() else {
                        return Err(Diag {
                            file,
                            line,
                            message: "#library needs a name in quotes".into(),
                        });
                    };
                    match open.last_mut() {
                        Some(top) => {
                            let i = match imports.iter().position(|n| n.eq_ignore_ascii_case(&name))
                            {
                                Some(i) => i,
                                None => {
                                    imports.push(name);
                                    imports.len() - 1
                                }
                            };
                            top.0 = Some(u8::try_from(i + 1).unwrap_or(u8::MAX));
                        }
                        None => library = Some(name),
                    }
                }
                _ => {
                    return Err(Diag {
                        file,
                        line,
                        message: format!("#{d} must be on its own at the top level"),
                    });
                }
            }
            continue;
        }
        let before = tops.len();
        p.top(&mut tops)?;
        let from = open.last().map(|o| o.0.unwrap_or(u8::MAX));
        import_of.resize(tops.len().max(before), from);
    }
    if let Some((_, f)) = open.pop() {
        return Err(Diag {
            file: f,
            line: 1,
            message: "unterminated #import".into(),
        });
    }
    Ok(Program {
        tops,
        import_of,
        library,
        imports,
        specials: p.specials,
    })
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    specials: Vec<(String, i32, u8, u8)>,
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("'{s}'"),
        Tok::Int(v) => format!("number {v}"),
        Tok::Str(_) => "a string".into(),
        Tok::Punct(p) => format!("'{p}'"),
        Tok::Directive(d) => format!("#{d}"),
        Tok::Eof => "the end of the file".into(),
    }
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)].tok
    }
    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].tok
    }
    fn here(&self) -> (u16, u32) {
        let t = &self.toks[self.pos.min(self.toks.len() - 1)];
        (t.file, t.line)
    }
    fn at_eof(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }
    fn bump(&mut self) -> Tok {
        let t = self.peek().clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn err<T>(&self, message: impl Into<String>) -> Result<T, Diag> {
        let (file, line) = self.here();
        Err(Diag {
            file,
            line,
            message: message.into(),
        })
    }
    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, p: &str) -> Result<(), Diag> {
        if self.eat(p) {
            Ok(())
        } else {
            let found = describe(self.peek());
            self.err(format!("expected '{p}' but found {found}"))
        }
    }
    /// The current identifier, lower-cased.
    fn word(&self) -> Option<String> {
        match self.peek() {
            Tok::Ident(s) => Some(s.to_ascii_lowercase()),
            _ => None,
        }
    }
    fn is_word(&self, w: &str) -> bool {
        self.word().as_deref() == Some(w)
    }
    fn ident(&mut self, what: &str) -> Result<String, Diag> {
        match self.peek() {
            Tok::Ident(s) => {
                let s = s.to_ascii_lowercase();
                self.bump();
                Ok(s)
            }
            t => {
                let found = describe(t);
                self.err(format!("expected {what} but found {found}"))
            }
        }
    }

    fn top(&mut self, out: &mut Vec<Top>) -> Result<(), Diag> {
        let (file, line) = self.here();
        if self.eat(";") {
            return Ok(());
        }
        let Some(w) = self.word() else {
            let found = describe(self.peek());
            return self.err(format!(
                "expected a script, function or variable but found {found}"
            ));
        };
        match w.as_str() {
            "script" => {
                self.bump();
                out.push(self.script(file, line)?);
            }
            "function" => {
                self.bump();
                let ret = self.ident("a return type")?;
                if !TYPES.contains(&ret.as_str()) && ret != "void" {
                    return self.err(format!("unknown return type '{ret}'"));
                }
                let name = self.ident("a function name")?;
                let params = self.params()?;
                let body = self.block()?;
                out.push(Top::Function {
                    name,
                    params,
                    returns: ret != "void",
                    body,
                    file,
                    line,
                });
            }
            "world" | "global" => {
                self.bump();
                let scope = if w == "world" {
                    Scope::World
                } else {
                    Scope::Global
                };
                let ty = self.ident("a type")?;
                if !TYPES.contains(&ty.as_str()) {
                    return self.err(format!("unknown type '{ty}'"));
                }
                loop {
                    let (file, line) = self.here();
                    let index = self.expr_no_assign()?;
                    self.expect(":")?;
                    let name = self.ident("a variable name")?;
                    let array = if self.eat("[") {
                        self.expect("]")?;
                        true
                    } else {
                        false
                    };
                    out.push(Top::ScopeVar {
                        scope,
                        index,
                        name,
                        array,
                        file,
                        line,
                    });
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect(";")?;
            }
            "special" => {
                self.bump();
                self.special_decl()?;
            }
            "static" => {
                self.bump();
            }
            t if TYPES.contains(&t) => {
                for d in self.decls()? {
                    out.push(Top::MapVar(d));
                }
            }
            _ => {
                return self.err(format!(
                    "expected a script, function or variable but found '{w}'"
                ));
            }
        }
        Ok(())
    }

    /// `special 80:ACS_Execute(1,5), -39:ACS_NamedExecute(1,5);`
    fn special_decl(&mut self) -> Result<(), Diag> {
        loop {
            let neg = self.eat("-");
            let Tok::Int(n) = self.bump() else {
                return self.err("expected a special number");
            };
            self.expect(":")?;
            let name = self.ident("a special name")?;
            self.expect("(")?;
            let Tok::Int(min) = self.bump() else {
                return self.err("expected the argument count");
            };
            let max = if self.eat(",") {
                let Tok::Int(m) = self.bump() else {
                    return self.err("expected the maximum argument count");
                };
                m
            } else {
                min
            };
            self.expect(")")?;
            let n = if neg { -n } else { n };
            self.specials
                .push((name, n, min.clamp(0, 255) as u8, max.clamp(0, 255) as u8));
            if !self.eat(",") {
                break;
            }
        }
        self.expect(";")
    }

    fn script(&mut self, file: u16, line: u32) -> Result<Top, Diag> {
        let id = match self.peek().clone() {
            Tok::Str(s) => {
                self.bump();
                ScriptId::Name(s)
            }
            _ => ScriptId::Num(self.expr_no_assign_unary()?),
        };
        let mut params = Vec::new();
        let mut kind = 0u16;
        let mut flags = 0u16;
        loop {
            if self.is_punct("(") {
                params = self.params()?;
                continue;
            }
            let Some(w) = self.word() else { break };
            if let Some(&(_, k)) = SCRIPT_TYPES.iter().find(|(n, _)| *n == w) {
                kind = k;
            } else if w == "net" {
                flags |= 1;
            } else if w == "clientside" {
                flags |= 2;
            } else {
                return self.err(format!("unknown script type or flag '{w}'"));
            }
            self.bump();
        }
        let body = self.block()?;
        Ok(Top::Script {
            id,
            params,
            kind,
            flags,
            body,
            file,
            line,
        })
    }

    /// `(void)` or `(int a, str b)`.
    fn params(&mut self) -> Result<Vec<String>, Diag> {
        self.expect("(")?;
        let mut out = Vec::new();
        if self.is_word("void") {
            self.bump();
            self.expect(")")?;
            return Ok(out);
        }
        if self.eat(")") {
            return Ok(out);
        }
        loop {
            let ty = self.ident("a parameter type")?;
            if !TYPES.contains(&ty.as_str()) {
                return self.err(format!("unknown parameter type '{ty}'"));
            }
            out.push(self.ident("a parameter name")?);
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(out)
    }

    /// `int a = 1, b[3] = {1, 2, 3};`
    fn decls(&mut self) -> Result<Vec<VarDecl>, Diag> {
        let ty = self.ident("a type")?;
        let is_str = ty == "str";
        let mut out = Vec::new();
        loop {
            let (file, line) = self.here();
            let name = self.ident("a variable name")?;
            let mut dims = Vec::new();
            while self.eat("[") {
                if self.eat("]") {
                    dims.push(None);
                } else {
                    dims.push(Some(self.expr()?));
                    self.expect("]")?;
                }
            }
            let init = if self.eat("=") {
                Some(self.init()?)
            } else {
                None
            };
            out.push(VarDecl {
                name,
                dims,
                init,
                is_str,
                file,
                line,
            });
            if !self.eat(",") {
                break;
            }
        }
        self.expect(";")?;
        Ok(out)
    }

    fn init(&mut self) -> Result<Init, Diag> {
        let (_, line) = self.here();
        if self.eat("{") {
            let mut items = Vec::new();
            while !self.is_punct("}") {
                items.push(self.init()?);
                if !self.eat(",") {
                    break;
                }
            }
            self.expect("}")?;
            Ok(Init::List(items, line))
        } else {
            Ok(Init::Expr(self.expr()?))
        }
    }

    fn block(&mut self) -> Result<Stmt, Diag> {
        self.expect("{")?;
        let mut body = Vec::new();
        while !self.eat("}") {
            if self.at_eof() {
                return self.err("missing '}' before the end of the file");
            }
            body.push(self.stmt()?);
        }
        Ok(Stmt::Block(body))
    }

    fn paren(&mut self) -> Result<Expr, Diag> {
        self.expect("(")?;
        let e = self.expr()?;
        self.expect(")")?;
        Ok(e)
    }

    fn stmt(&mut self) -> Result<Stmt, Diag> {
        let (file, line) = self.here();
        if self.is_punct("{") {
            return self.block();
        }
        if self.eat(";") {
            return Ok(Stmt::Empty);
        }
        let w = self.word().unwrap_or_default();
        match w.as_str() {
            t if TYPES.contains(&t) => Ok(Stmt::Local(self.decls()?)),
            "static" => {
                self.bump();
                Ok(Stmt::Local(self.decls()?))
            }
            "if" => {
                self.bump();
                let cond = self.paren()?;
                let then = Box::new(self.stmt()?);
                let other = if self.is_word("else") {
                    self.bump();
                    Some(Box::new(self.stmt()?))
                } else {
                    None
                };
                Ok(Stmt::If(cond, then, other))
            }
            "while" | "until" => {
                self.bump();
                let cond = self.paren()?;
                let body = Box::new(self.stmt()?);
                Ok(Stmt::While {
                    cond,
                    body,
                    until: w == "until",
                })
            }
            "do" => {
                self.bump();
                let body = Box::new(self.stmt()?);
                let until = match self.word().as_deref() {
                    Some("while") => false,
                    Some("until") => true,
                    _ => return self.err("expected 'while' or 'until' after the do body"),
                };
                self.bump();
                let cond = self.paren()?;
                self.expect(";")?;
                Ok(Stmt::Do { body, cond, until })
            }
            "for" => {
                self.bump();
                self.expect("(")?;
                let mut init = Vec::new();
                if self.word().is_some_and(|w| TYPES.contains(&w.as_str())) {
                    init.push(Stmt::Local(self.decls()?));
                } else {
                    if !self.is_punct(";") {
                        loop {
                            init.push(Stmt::Expr(self.expr()?));
                            if !self.eat(",") {
                                break;
                            }
                        }
                    }
                    self.expect(";")?;
                }
                let cond = if self.is_punct(";") {
                    None
                } else {
                    Some(self.expr()?)
                };
                self.expect(";")?;
                let mut step = Vec::new();
                if !self.is_punct(")") {
                    loop {
                        step.push(Stmt::Expr(self.expr()?));
                        if !self.eat(",") {
                            break;
                        }
                    }
                }
                self.expect(")")?;
                let body = Box::new(self.stmt()?);
                Ok(Stmt::For {
                    init,
                    cond,
                    step,
                    body,
                })
            }
            "switch" => {
                self.bump();
                let value = self.paren()?;
                self.expect("{")?;
                let mut body = Vec::new();
                while !self.eat("}") {
                    if self.at_eof() {
                        return self.err("missing '}' at the end of the switch");
                    }
                    if self.is_word("case") {
                        self.bump();
                        body.push(SwitchItem::Case(self.expr_no_assign()?));
                        self.expect(":")?;
                    } else if self.is_word("default") {
                        let (f, l) = self.here();
                        self.bump();
                        self.expect(":")?;
                        body.push(SwitchItem::Default(f, l));
                    } else {
                        body.push(SwitchItem::Stmt(self.stmt()?));
                    }
                }
                Ok(Stmt::Switch { value, body })
            }
            "case" | "default" => self.err(format!("'{w}' outside a switch")),
            "break" => {
                self.bump();
                self.expect(";")?;
                Ok(Stmt::Break(file, line))
            }
            "continue" => {
                self.bump();
                self.expect(";")?;
                Ok(Stmt::Continue(file, line))
            }
            "return" => {
                self.bump();
                let value = if self.is_punct(";") {
                    None
                } else {
                    Some(self.expr()?)
                };
                self.expect(";")?;
                Ok(Stmt::Return(value, file, line))
            }
            "restart" | "terminate" | "suspend" => {
                self.bump();
                self.expect(";")?;
                Ok(match w.as_str() {
                    "restart" => Stmt::Restart,
                    "terminate" => Stmt::Terminate,
                    _ => Stmt::Suspend,
                })
            }
            _ => {
                let e = self.expr()?;
                self.expect(";")?;
                Ok(Stmt::Expr(e))
            }
        }
    }

    fn mk(&self, kind: ExprKind, file: u16, line: u32) -> Expr {
        Expr { kind, file, line }
    }

    pub(crate) fn expr(&mut self) -> Result<Expr, Diag> {
        let (file, line) = self.here();
        let lhs = self.ternary()?;
        const ASSIGN: &[&str] = &[
            "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
        ];
        if let Tok::Punct(p) = self.peek().clone() {
            if ASSIGN.contains(&p) {
                if !matches!(lhs.kind, ExprKind::Name(_) | ExprKind::Index(..)) {
                    return self.err(format!("the left side of '{p}' isn't a variable"));
                }
                self.bump();
                let rhs = self.expr()?;
                return Ok(self.mk(
                    ExprKind::Assign(p, Box::new(lhs), Box::new(rhs)),
                    file,
                    line,
                ));
            }
        }
        Ok(lhs)
    }

    /// An expression without assignment (case labels, world variable indices).
    fn expr_no_assign(&mut self) -> Result<Expr, Diag> {
        self.ternary()
    }

    /// A script number: operators but no ternary, so `script 1 (void)` keeps
    /// its parameter list.
    fn expr_no_assign_unary(&mut self) -> Result<Expr, Diag> {
        self.binary(0)
    }

    fn ternary(&mut self) -> Result<Expr, Diag> {
        let (file, line) = self.here();
        let cond = self.binary(0)?;
        if self.eat("?") {
            let a = self.expr()?;
            self.expect(":")?;
            let b = self.ternary()?;
            return Ok(self.mk(
                ExprKind::Ternary(Box::new(cond), Box::new(a), Box::new(b)),
                file,
                line,
            ));
        }
        Ok(cond)
    }

    fn binary(&mut self, level: usize) -> Result<Expr, Diag> {
        const LEVELS: &[&[&str]] = &[
            &["||"],
            &["&&"],
            &["|"],
            &["^"],
            &["&"],
            &["==", "!="],
            &["<", ">", "<=", ">="],
            &["<<", ">>"],
            &["+", "-"],
            &["*", "/", "%"],
        ];
        if level >= LEVELS.len() {
            return self.unary();
        }
        let mut lhs = self.binary(level + 1)?;
        loop {
            let (file, line) = self.here();
            let Tok::Punct(p) = self.peek().clone() else {
                break;
            };
            if !LEVELS[level].contains(&p) {
                break;
            }
            self.bump();
            let rhs = self.binary(level + 1)?;
            lhs = self.mk(
                ExprKind::Binary(p, Box::new(lhs), Box::new(rhs)),
                file,
                line,
            );
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, Diag> {
        let (file, line) = self.here();
        if let Tok::Punct(p) = self.peek().clone() {
            match p {
                "-" | "!" | "~" | "+" => {
                    self.bump();
                    let e = self.unary()?;
                    if p == "+" {
                        return Ok(e);
                    }
                    return Ok(self.mk(ExprKind::Unary(p, Box::new(e)), file, line));
                }
                "++" | "--" => {
                    self.bump();
                    let target = self.unary()?;
                    if !matches!(target.kind, ExprKind::Name(_) | ExprKind::Index(..)) {
                        return self.err(format!("'{p}' needs a variable"));
                    }
                    return Ok(self.mk(
                        ExprKind::IncDec {
                            inc: p == "++",
                            post: false,
                            target: Box::new(target),
                        },
                        file,
                        line,
                    ));
                }
                _ => {}
            }
        }
        let mut e = self.primary()?;
        loop {
            let (file, line) = self.here();
            if self.is_punct("++") || self.is_punct("--") {
                let inc = self.is_punct("++");
                if !matches!(e.kind, ExprKind::Name(_) | ExprKind::Index(..)) {
                    return self.err("'++' / '--' needs a variable");
                }
                self.bump();
                e = self.mk(
                    ExprKind::IncDec {
                        inc,
                        post: true,
                        target: Box::new(e),
                    },
                    file,
                    line,
                );
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, Diag> {
        let (file, line) = self.here();
        match self.bump() {
            Tok::Int(v) => Ok(self.mk(ExprKind::Num(v), file, line)),
            Tok::Str(s) => Ok(self.mk(ExprKind::Str(s), file, line)),
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Ident(name) => {
                let name = name.to_ascii_lowercase();
                if self.is_punct("(") {
                    self.bump();
                    // `x:` items: a print function (or a misspelt one, reported by name later).
                    let items = matches!((self.peek(), self.peek_at(1)), (Tok::Ident(k), Tok::Punct(":")) if k.len() == 1);
                    if PRINTS.contains(&name.as_str()) || items {
                        return self.print_call(name, file, line);
                    }
                    if name == "createtranslation" {
                        return self.translation(file, line);
                    }
                    let mut args = Vec::new();
                    if !self.is_punct(")") {
                        if self.is_word("void") && matches!(self.peek_at(1), Tok::Punct(")")) {
                            self.bump();
                        } else {
                            loop {
                                // `const:` asks ACC for the direct p-code; the value is the same.
                                if self.is_word("const")
                                    && matches!(self.peek_at(1), Tok::Punct(":"))
                                {
                                    self.bump();
                                    self.bump();
                                }
                                args.push(self.expr()?);
                                if !self.eat(",") {
                                    break;
                                }
                            }
                        }
                    }
                    self.expect(")")?;
                    return Ok(self.mk(ExprKind::Call(name, args), file, line));
                }
                if self.is_punct("[") {
                    let mut idx = Vec::new();
                    while self.eat("[") {
                        idx.push(self.expr()?);
                        self.expect("]")?;
                    }
                    return Ok(self.mk(ExprKind::Index(name, idx), file, line));
                }
                Ok(self.mk(ExprKind::Name(name), file, line))
            }
            t => Err(Diag {
                file,
                line,
                message: format!("expected a value but found {}", describe(&t)),
            }),
        }
    }

    /// `CreateTranslation(n, 0:1=2:3, 4:5=[r,g,b]:[r,g,b], 6:7=%[..]:[..],
    /// 8:9=#[r,g,b], 10:11=@amount[r,g,b])`.
    fn translation(&mut self, file: u16, line: u32) -> Result<Expr, Diag> {
        let num = self.ternary()?;
        let mut ranges = Vec::new();
        while self.eat(",") {
            let mut v = vec![self.ternary()?];
            self.expect(":")?;
            v.push(self.ternary()?);
            self.expect("=")?;
            let kind = if self.is_punct("[") || self.is_punct("%") {
                let kind = if self.eat("%") { 3 } else { 2 };
                self.rgb(&mut v)?;
                self.expect(":")?;
                self.rgb(&mut v)?;
                kind
            } else if self.eat("#") {
                self.rgb(&mut v)?;
                4
            } else if self.eat("@") {
                v.push(self.unary()?);
                self.rgb(&mut v)?;
                5
            } else {
                v.push(self.ternary()?);
                self.expect(":")?;
                v.push(self.ternary()?);
                1
            };
            ranges.push((kind, v));
        }
        self.expect(")")?;
        Ok(self.mk(ExprKind::Translation(Box::new(num), ranges), file, line))
    }

    /// `[r, g, b]`.
    fn rgb(&mut self, out: &mut Vec<Expr>) -> Result<(), Diag> {
        self.expect("[")?;
        for i in 0..3 {
            if i > 0 {
                self.expect(",")?;
            }
            out.push(self.ternary()?);
        }
        self.expect("]")
    }

    /// `print(s:"x", d:n)` and `hudmessage(s:"x"; type, id, color, x, y, hold, ...)`.
    fn print_call(&mut self, name: String, file: u16, line: u32) -> Result<Expr, Diag> {
        let mut items = Vec::new();
        let mut extra = Vec::new();
        if !self.is_punct(")") && !self.is_punct(";") {
            loop {
                let kind = match (self.peek().clone(), self.peek_at(1).clone()) {
                    (Tok::Ident(k), Tok::Punct(":")) => k.to_ascii_lowercase(),
                    _ => return self.err(format!("{name} items look like s:\"text\" or d:number")),
                };
                let c = match kind.as_str() {
                    "s" | "d" | "i" | "c" | "f" | "n" | "l" | "k" | "b" | "x" => {
                        kind.as_bytes()[0] as char
                    }
                    "a" => return self.err("a: (character arrays) isn't supported"),
                    _ => return self.err(format!("unknown print item '{kind}:'")),
                };
                self.bump();
                self.bump();
                items.push((c, self.expr()?));
                if !self.eat(",") {
                    break;
                }
            }
        }
        if self.eat(";") {
            loop {
                extra.push(self.expr()?);
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect(")")?;
        Ok(self.mk(ExprKind::Print { name, items, extra }, file, line))
    }
}

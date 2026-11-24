//! Code generation: syntax tree → `ACSE` bytes (4-byte p-codes and chunks).

use std::collections::HashMap;

use super::Diag;
use super::builtins::{BUILTINS, Builtin, default_arg};
use super::defs::{CALLFUNCS, CONSTANTS, SPECIALS};
use super::parse::{
    Expr, ExprKind, Init, Program, Scope, ScriptId, Stmt, SwitchItem, Top, VarDecl,
};
use crate::pcd::*;

const MAX_MAP_VARS: u32 = 128;
const MAX_WORLD_VARS: i32 = 256;
const MAX_GLOBAL_VARS: i32 = 64;
/// `ACS_Execute`, used by `ACS_ExecuteWait`.
const ACS_EXECUTE: i32 = 80;

/// Where a variable lives: script / function locals, map, world, global.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Space {
    Local,
    Map,
    World,
    Global,
}

impl Space {
    fn col(self) -> usize {
        match self {
            Space::Local => 0,
            Space::Map => 1,
            Space::World => 2,
            Space::Global => 3,
        }
    }
}

/// Variable p-codes by operator, in `Space` order.
const VAR_OPS: &[(&str, [u32; 4])] = &[
    (
        "=",
        [
            ASSIGNSCRIPTVAR,
            ASSIGNMAPVAR,
            ASSIGNWORLDVAR,
            ASSIGNGLOBALVAR,
        ],
    ),
    ("+=", [ADDSCRIPTVAR, ADDMAPVAR, ADDWORLDVAR, ADDGLOBALVAR]),
    ("-=", [SUBSCRIPTVAR, SUBMAPVAR, SUBWORLDVAR, SUBGLOBALVAR]),
    ("*=", [MULSCRIPTVAR, MULMAPVAR, MULWORLDVAR, MULGLOBALVAR]),
    ("/=", [DIVSCRIPTVAR, DIVMAPVAR, DIVWORLDVAR, DIVGLOBALVAR]),
    ("%=", [MODSCRIPTVAR, MODMAPVAR, MODWORLDVAR, MODGLOBALVAR]),
    ("&=", [ANDSCRIPTVAR, ANDMAPVAR, ANDWORLDVAR, ANDGLOBALVAR]),
    ("|=", [ORSCRIPTVAR, ORMAPVAR, ORWORLDVAR, ORGLOBALVAR]),
    ("^=", [EORSCRIPTVAR, EORMAPVAR, EORWORLDVAR, EORGLOBALVAR]),
    ("<<=", [LSSCRIPTVAR, LSMAPVAR, LSWORLDVAR, LSGLOBALVAR]),
    (">>=", [RSSCRIPTVAR, RSMAPVAR, RSWORLDVAR, RSGLOBALVAR]),
    (
        "push",
        [PUSHSCRIPTVAR, PUSHMAPVAR, PUSHWORLDVAR, PUSHGLOBALVAR],
    ),
    ("++", [INCSCRIPTVAR, INCMAPVAR, INCWORLDVAR, INCGLOBALVAR]),
    ("--", [DECSCRIPTVAR, DECMAPVAR, DECWORLDVAR, DECGLOBALVAR]),
];

/// Array p-codes by operator, in `Space` order.
const ARRAY_OPS: &[(&str, [u32; 4])] = &[
    (
        "=",
        [
            ASSIGNSCRIPTARRAY,
            ASSIGNMAPARRAY,
            ASSIGNWORLDARRAY,
            ASSIGNGLOBALARRAY,
        ],
    ),
    (
        "+=",
        [ADDSCRIPTARRAY, ADDMAPARRAY, ADDWORLDARRAY, ADDGLOBALARRAY],
    ),
    (
        "-=",
        [SUBSCRIPTARRAY, SUBMAPARRAY, SUBWORLDARRAY, SUBGLOBALARRAY],
    ),
    (
        "*=",
        [MULSCRIPTARRAY, MULMAPARRAY, MULWORLDARRAY, MULGLOBALARRAY],
    ),
    (
        "/=",
        [DIVSCRIPTARRAY, DIVMAPARRAY, DIVWORLDARRAY, DIVGLOBALARRAY],
    ),
    (
        "%=",
        [MODSCRIPTARRAY, MODMAPARRAY, MODWORLDARRAY, MODGLOBALARRAY],
    ),
    (
        "&=",
        [ANDSCRIPTARRAY, ANDMAPARRAY, ANDWORLDARRAY, ANDGLOBALARRAY],
    ),
    (
        "|=",
        [ORSCRIPTARRAY, ORMAPARRAY, ORWORLDARRAY, ORGLOBALARRAY],
    ),
    (
        "^=",
        [EORSCRIPTARRAY, EORMAPARRAY, EORWORLDARRAY, EORGLOBALARRAY],
    ),
    (
        "<<=",
        [LSSCRIPTARRAY, LSMAPARRAY, LSWORLDARRAY, LSGLOBALARRAY],
    ),
    (
        ">>=",
        [RSSCRIPTARRAY, RSMAPARRAY, RSWORLDARRAY, RSGLOBALARRAY],
    ),
    (
        "push",
        [
            PUSHSCRIPTARRAY,
            PUSHMAPARRAY,
            PUSHWORLDARRAY,
            PUSHGLOBALARRAY,
        ],
    ),
    (
        "++",
        [INCSCRIPTARRAY, INCMAPARRAY, INCWORLDARRAY, INCGLOBALARRAY],
    ),
    (
        "--",
        [DECSCRIPTARRAY, DECMAPARRAY, DECWORLDARRAY, DECGLOBALARRAY],
    ),
];

const BINARY_OPS: &[(&str, u32)] = &[
    ("+", ADD),
    ("-", SUBTRACT),
    ("*", MULTIPLY),
    ("/", DIVIDE),
    ("%", MODULUS),
    ("==", EQ),
    ("!=", NE),
    ("<", LT),
    (">", GT),
    ("<=", LE),
    (">=", GE),
    ("&&", ANDLOGICAL),
    ("||", ORLOGICAL),
    ("&", ANDBITWISE),
    ("|", ORBITWISE),
    ("^", EORBITWISE),
    ("<<", LSHIFT),
    (">>", RSHIFT),
];

fn op_for(table: &[(&str, [u32; 4])], op: &str, space: Space) -> u32 {
    table
        .iter()
        .find(|(o, _)| *o == op)
        .map_or(NOP, |(_, c)| c[space.col()])
}

/// A variable or array a name refers to.
#[derive(Clone, Debug)]
enum Var {
    Scalar(Space, u32),
    /// Array: space, number (map variable / array index), dimensions
    /// (empty for world / global arrays, which have no declared size).
    Array(Space, u32, Vec<u32>),
}

struct Func {
    index: u32,
    argc: u8,
    returns: bool,
}

struct ScriptOut {
    number: i32,
    kind: u16,
    flags: u16,
    address: u32,
    argc: u32,
    locals: u32,
    arrays: Vec<u32>,
}

struct FuncOut {
    /// `#import`ed: no code here; the VM links it by name (address 0).
    imported: bool,
    argc: u8,
    locals: u8,
    returns: bool,
    address: u32,
    arrays: Vec<u32>,
}

#[derive(Clone, Copy)]
struct Label(usize);

struct Loop {
    brk: Label,
    cont: Option<Label>,
}

struct Gen {
    code: Vec<u8>,
    strings: Vec<String>,
    string_ix: HashMap<String, u32>,
    globals: HashMap<String, Var>,
    map_count: u32,
    mini: Vec<i32>,
    arrays: Vec<(u32, u32, Vec<i32>)>,
    /// Map variable names by slot (`MEXP`; empty for imported slots).
    map_names: Vec<String>,
    /// `#import`ed map variables (`MIMP`) and arrays (`AIMP`).
    mimp: Vec<(u32, String)>,
    aimp: Vec<(u32, u32, String)>,
    /// String-valued map variables (`MSTR`) and array cells (`ATAG`), for libraries.
    mstr: Vec<u32>,
    atags: Vec<(u32, Vec<u8>)>,
    /// `#library` module: export names and string tags.
    library: bool,
    /// Imported library names (`LOAD`).
    loads: Vec<String>,
    funcs: HashMap<String, Func>,
    func_out: Vec<FuncOut>,
    func_names: Vec<String>,
    scripts: Vec<ScriptOut>,
    script_names: Vec<String>,
    specials: HashMap<String, (i32, u8, u8)>,
    callfuncs: HashMap<String, i32>,
    constants: HashMap<String, i32>,
    builtins: HashMap<String, &'static Builtin>,
    // Per body.
    scopes: Vec<HashMap<String, Var>>,
    local_count: u32,
    local_arrays: Vec<u32>,
    /// `Some(returns)` inside a function.
    function: Option<bool>,
    loops: Vec<Loop>,
    labels: Vec<Option<u32>>,
    fixups: Vec<(usize, Label)>,
}

/// A string literal initializer.
fn is_str_init(init: &Init) -> bool {
    matches!(
        init,
        Init::Expr(Expr {
            kind: ExprKind::Str(_),
            ..
        })
    )
}

fn diag(e: &Expr, message: impl Into<String>) -> Diag {
    Diag {
        file: e.file,
        line: e.line,
        message: message.into(),
    }
}

pub(crate) fn generate(p: &Program) -> Result<Vec<u8>, Diag> {
    let mut g = Gen {
        code: b"ACSE\0\0\0\0".to_vec(),
        strings: Vec::new(),
        string_ix: HashMap::new(),
        globals: HashMap::new(),
        map_count: 0,
        mini: vec![0; MAX_MAP_VARS as usize],
        arrays: Vec::new(),
        map_names: Vec::new(),
        mimp: Vec::new(),
        aimp: Vec::new(),
        mstr: Vec::new(),
        atags: Vec::new(),
        library: p.library.is_some(),
        loads: p.imports.clone(),
        funcs: HashMap::new(),
        func_out: Vec::new(),
        func_names: Vec::new(),
        scripts: Vec::new(),
        script_names: Vec::new(),
        specials: SPECIALS
            .iter()
            .map(|&(n, num, min, max)| {
                let min = min.max(0) as u8;
                let max = if max < 0 { 5 } else { max as u8 };
                (n.to_ascii_lowercase(), (num, min, max.max(min)))
            })
            .collect(),
        callfuncs: CALLFUNCS
            .iter()
            .map(|&(n, i)| (n.to_ascii_lowercase(), i))
            .collect(),
        constants: CONSTANTS
            .iter()
            .map(|&(n, v)| (n.to_ascii_lowercase(), v))
            .collect(),
        builtins: BUILTINS
            .iter()
            .map(|b| (b.name.to_ascii_lowercase(), b))
            .collect(),
        scopes: Vec::new(),
        local_count: 0,
        local_arrays: Vec::new(),
        function: None,
        loops: Vec::new(),
        labels: Vec::new(),
        fixups: Vec::new(),
    };
    for (name, num, min, max) in &p.specials {
        if *num < 0 {
            g.callfuncs.insert(name.clone(), -num);
        } else {
            let min = (*min).min(5);
            g.specials
                .insert(name.clone(), (*num, min, (*max).clamp(min, 5)));
        }
    }
    // Declarations first, so scripts can call functions defined below them.
    for (k, top) in p.tops.iter().enumerate() {
        let import = p.import_of.get(k).copied().flatten();
        match top {
            Top::MapVar(d) => g.map_var(d, import.is_some())?,
            Top::ScopeVar {
                scope,
                index,
                name,
                array,
                file,
                line,
            } => {
                let at = Diag {
                    file: *file,
                    line: *line,
                    message: String::new(),
                };
                let n = g.const_eval(index)?;
                let (space, max) = match scope {
                    Scope::World => (Space::World, MAX_WORLD_VARS),
                    Scope::Global => (Space::Global, MAX_GLOBAL_VARS),
                };
                if !(0..max).contains(&n) {
                    return Err(Diag {
                        message: format!(
                            "{} variable numbers go from 0 to {}",
                            if *scope == Scope::World {
                                "world"
                            } else {
                                "global"
                            },
                            max - 1
                        ),
                        ..at
                    });
                }
                let v = if *array {
                    Var::Array(space, n as u32, Vec::new())
                } else {
                    Var::Scalar(space, n as u32)
                };
                g.declare_global(name, v, &at)?;
            }
            Top::Function {
                name,
                params,
                returns,
                file,
                line,
                ..
            } => {
                if g.funcs.contains_key(name) {
                    return Err(Diag {
                        file: *file,
                        line: *line,
                        message: format!("function {name} is defined twice"),
                    });
                }
                let index = g.func_out.len() as u32;
                g.funcs.insert(
                    name.clone(),
                    Func {
                        index,
                        argc: params.len() as u8,
                        returns: *returns,
                    },
                );
                g.func_out.push(FuncOut {
                    imported: import.is_some(),
                    argc: params.len() as u8,
                    locals: 0,
                    returns: *returns,
                    address: 0,
                    arrays: Vec::new(),
                });
                g.func_names.push(name.clone());
            }
            Top::Script { .. } => {}
        }
    }
    for (k, top) in p.tops.iter().enumerate() {
        if p.import_of.get(k).copied().flatten().is_some() {
            // Imported scripts and functions run from their library.
            continue;
        }
        match top {
            Top::Script {
                id,
                params,
                kind,
                flags,
                body,
                file,
                line,
            } => {
                let at = Diag {
                    file: *file,
                    line: *line,
                    message: String::new(),
                };
                let number = match id {
                    ScriptId::Num(e) => {
                        let n = g.const_eval(e)?;
                        if !(0..=32767).contains(&n) {
                            return Err(diag(e, "script numbers go from 0 to 32767"));
                        }
                        n
                    }
                    ScriptId::Name(s) => {
                        if g.script_names.iter().any(|n| n.eq_ignore_ascii_case(s)) {
                            return Err(Diag {
                                message: format!("script \"{s}\" is defined twice"),
                                ..at
                            });
                        }
                        g.script_names.push(s.clone());
                        -(g.script_names.len() as i32)
                    }
                };
                if g.scripts.iter().any(|s| s.number == number) {
                    return Err(Diag {
                        message: format!("script {number} is defined twice"),
                        ..at
                    });
                }
                if params.len() > 4 {
                    return Err(Diag {
                        message: "scripts take at most 4 arguments".into(),
                        ..at
                    });
                }
                // Closed, DISCONNECT (player number) and EVENT scripts take arguments.
                if ![0, 14, 16].contains(kind) && !params.is_empty() {
                    return Err(Diag {
                        message: "only closed, disconnect and event scripts take arguments".into(),
                        ..at
                    });
                }
                let address = g.pos();
                g.begin_body(params, None);
                g.stmt(body)?;
                g.op(TERMINATE);
                let (locals, arrays) = g.end_body()?;
                g.scripts.push(ScriptOut {
                    number,
                    kind: *kind,
                    flags: *flags,
                    address,
                    argc: params.len() as u32,
                    locals,
                    arrays,
                });
            }
            Top::Function {
                name,
                params,
                returns,
                body,
                file,
                line,
            } => {
                let index = g.funcs[name].index as usize;
                let address = g.pos();
                g.begin_body(params, Some(*returns));
                g.stmt(body)?;
                if *returns {
                    g.push_num(0);
                    g.op(RETURNVAL);
                } else {
                    g.op(RETURNVOID);
                }
                let (locals, arrays) = g.end_body()?;
                let extra = locals - params.len() as u32;
                if extra > 255 {
                    return Err(Diag {
                        file: *file,
                        line: *line,
                        message: "too many local variables in this function".into(),
                    });
                }
                let f = &mut g.func_out[index];
                f.address = address;
                f.locals = extra as u8;
                f.arrays = arrays;
            }
            _ => {}
        }
    }
    Ok(g.finish())
}

impl Gen {
    fn pos(&self) -> u32 {
        self.code.len() as u32
    }
    fn word(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    fn op(&mut self, op: u32) {
        self.word(op);
    }
    fn op1(&mut self, op: u32, a: i32) {
        self.word(op);
        self.word(a as u32);
    }
    fn push_num(&mut self, v: i32) {
        self.op1(PUSHNUMBER, v);
    }
    fn label(&mut self) -> Label {
        self.labels.push(None);
        Label(self.labels.len() - 1)
    }
    fn place(&mut self, l: Label) {
        self.labels[l.0] = Some(self.pos());
    }
    fn jump(&mut self, op: u32, l: Label) {
        self.word(op);
        self.fixups.push((self.code.len(), l));
        self.word(0);
    }
    fn string(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.string_ix.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.to_owned());
        self.string_ix.insert(s.to_owned(), i);
        i
    }

    fn declare_global(&mut self, name: &str, v: Var, at: &Diag) -> Result<(), Diag> {
        if self.globals.contains_key(name) {
            return Err(Diag {
                message: format!("{name} is declared twice"),
                ..at.clone()
            });
        }
        self.globals.insert(name.to_owned(), v);
        Ok(())
    }

    fn map_var(&mut self, d: &VarDecl, import: bool) -> Result<(), Diag> {
        let at = Diag {
            file: d.file,
            line: d.line,
            message: String::new(),
        };
        if self.map_count >= MAX_MAP_VARS {
            return Err(Diag {
                message: format!("too many map variables (at most {MAX_MAP_VARS})"),
                ..at
            });
        }
        let index = self.map_count;
        self.map_count += 1;
        self.map_names.push(if import {
            String::new()
        } else {
            d.name.clone()
        });
        if d.dims.is_empty() {
            if import {
                self.mimp.push((index, d.name.clone()));
            } else if let Some(init) = &d.init {
                self.mini[index as usize] = self.init_value(init, &at)?;
                if is_str_init(init) {
                    self.mstr.push(index);
                }
            }
            return self.declare_global(&d.name, Var::Scalar(Space::Map, index), &at);
        }
        let dims = self.dims(d)?;
        let size: u32 = dims.iter().product();
        if import {
            self.aimp.push((index, size, d.name.clone()));
            return self.declare_global(&d.name, Var::Array(Space::Map, index, dims), &at);
        }
        let mut values = Vec::new();
        if let Some(init) = &d.init {
            values = vec![0; size as usize];
            let mut strs = vec![false; size as usize];
            self.flatten(init, &dims, 0, &mut values, &mut strs, &at)?;
            if let Some(last) = strs.iter().rposition(|&s| s) {
                self.atags
                    .push((index, strs[..=last].iter().map(|&s| u8::from(s)).collect()));
            }
            while values.last() == Some(&0) {
                values.pop();
            }
        }
        self.arrays.push((index, size, values));
        self.declare_global(&d.name, Var::Array(Space::Map, index, dims), &at)
    }

    /// Array dimensions; an empty first dimension takes the initializer's length.
    fn dims(&mut self, d: &VarDecl) -> Result<Vec<u32>, Diag> {
        let at = Diag {
            file: d.file,
            line: d.line,
            message: String::new(),
        };
        let mut dims = Vec::new();
        for (i, dim) in d.dims.iter().enumerate() {
            let n = match dim {
                Some(e) => self.const_eval(e)?,
                None if i == 0 => match &d.init {
                    Some(Init::List(items, _)) => items.len() as i32,
                    _ => {
                        return Err(Diag {
                            message: format!("array {} needs a size or an initializer", d.name),
                            ..at
                        });
                    }
                },
                None => {
                    return Err(Diag {
                        message: "only the first array dimension can be left empty".into(),
                        ..at
                    });
                }
            };
            if !(1..=65536).contains(&n) {
                return Err(Diag {
                    message: format!("array size {n} is out of range"),
                    ..at
                });
            }
            dims.push(n as u32);
        }
        if dims.iter().map(|&d| u64::from(d)).product::<u64>() > 1_048_576 {
            return Err(Diag {
                message: format!("array {} is too large", d.name),
                ..at
            });
        }
        Ok(dims)
    }

    fn init_value(&mut self, init: &Init, at: &Diag) -> Result<i32, Diag> {
        match init {
            Init::Expr(Expr {
                kind: ExprKind::Str(s),
                ..
            }) => Ok(self.string(s) as i32),
            Init::Expr(e) => self.const_eval(e),
            Init::List(..) => Err(Diag {
                message: "a list initializes an array, not a variable".into(),
                ..at.clone()
            }),
        }
    }

    fn flatten(
        &mut self,
        init: &Init,
        dims: &[u32],
        base: usize,
        out: &mut [i32],
        strs: &mut [bool],
        at: &Diag,
    ) -> Result<(), Diag> {
        let Init::List(items, line) = init else {
            if dims.is_empty() {
                out[base] = self.init_value(init, at)?;
                strs[base] = is_str_init(init);
                return Ok(());
            }
            return Err(Diag {
                message: "an array needs a { } initializer".into(),
                ..at.clone()
            });
        };
        let Some((&n, rest)) = dims.split_first() else {
            return Err(Diag {
                line: *line,
                message: "too many braces in the initializer".into(),
                ..at.clone()
            });
        };
        if items.len() > n as usize {
            return Err(Diag {
                line: *line,
                message: format!("{} values for an array of {n}", items.len()),
                ..at.clone()
            });
        }
        let stride: usize = rest.iter().map(|&d| d as usize).product();
        for (i, item) in items.iter().enumerate() {
            self.flatten(item, rest, base + i * stride, out, strs, at)?;
        }
        Ok(())
    }

    fn const_eval(&self, e: &Expr) -> Result<i32, Diag> {
        self.try_const(e)
            .ok_or_else(|| diag(e, "expected a constant value"))
    }

    /// Fold a constant expression (numbers, constants, operators).
    fn try_const(&self, e: &Expr) -> Option<i32> {
        Some(match &e.kind {
            ExprKind::Num(v) => *v,
            ExprKind::Name(n) => {
                if self.lookup(n).is_some() {
                    return None;
                }
                *self.constants.get(n)?
            }
            ExprKind::Unary(op, a) => {
                let a = self.try_const(a)?;
                match *op {
                    "-" => a.wrapping_neg(),
                    "!" => i32::from(a == 0),
                    _ => !a,
                }
            }
            ExprKind::Binary(op, a, b) => {
                let (a, b) = (self.try_const(a)?, self.try_const(b)?);
                match *op {
                    "+" => a.wrapping_add(b),
                    "-" => a.wrapping_sub(b),
                    "*" => a.wrapping_mul(b),
                    "/" if b != 0 => a.wrapping_div(b),
                    "%" if b != 0 => a.wrapping_rem(b),
                    "==" => i32::from(a == b),
                    "!=" => i32::from(a != b),
                    "<" => i32::from(a < b),
                    ">" => i32::from(a > b),
                    "<=" => i32::from(a <= b),
                    ">=" => i32::from(a >= b),
                    "&&" => i32::from(a != 0 && b != 0),
                    "||" => i32::from(a != 0 || b != 0),
                    "&" => a & b,
                    "|" => a | b,
                    "^" => a ^ b,
                    "<<" => a.wrapping_shl(b as u32),
                    ">>" => a.wrapping_shr(b as u32),
                    _ => return None,
                }
            }
            ExprKind::Ternary(c, a, b) => {
                if self.try_const(c)? != 0 {
                    self.try_const(a)?
                } else {
                    self.try_const(b)?
                }
            }
            _ => return None,
        })
    }

    fn lookup(&self, name: &str) -> Option<&Var> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(name))
            .or_else(|| self.globals.get(name))
    }

    // ---- Bodies -------------------------------------------------------------

    fn begin_body(&mut self, params: &[String], function: Option<bool>) {
        self.scopes = vec![HashMap::new()];
        self.local_count = 0;
        self.local_arrays.clear();
        self.function = function;
        self.loops.clear();
        self.labels.clear();
        self.fixups.clear();
        for p in params {
            let i = self.local_count;
            self.local_count += 1;
            self.scopes[0].insert(p.clone(), Var::Scalar(Space::Local, i));
        }
    }

    fn end_body(&mut self) -> Result<(u32, Vec<u32>), Diag> {
        for &(at, l) in &self.fixups {
            let Some(target) = self.labels[l.0] else {
                return Err(Diag {
                    file: 0,
                    line: 0,
                    message: "internal error: unplaced label".into(),
                });
            };
            self.code[at..at + 4].copy_from_slice(&target.to_le_bytes());
        }
        Ok((self.local_count, std::mem::take(&mut self.local_arrays)))
    }

    fn local(&mut self, d: &VarDecl) -> Result<(), Diag> {
        let at = Diag {
            file: d.file,
            line: d.line,
            message: String::new(),
        };
        if self.scopes.last().is_some_and(|s| s.contains_key(&d.name)) {
            return Err(Diag {
                message: format!("{} is declared twice", d.name),
                ..at
            });
        }
        if d.dims.is_empty() {
            let i = self.local_count;
            self.local_count += 1;
            if let Some(init) = &d.init {
                let Init::Expr(e) = init else {
                    return Err(Diag {
                        message: "a list initializes an array, not a variable".into(),
                        ..at
                    });
                };
                self.expr(e)?;
                self.op1(ASSIGNSCRIPTVAR, i as i32);
            }
            if let Some(s) = self.scopes.last_mut() {
                s.insert(d.name.clone(), Var::Scalar(Space::Local, i));
            }
            return Ok(());
        }
        let dims = self.dims(d)?;
        let num = self.local_arrays.len() as u32;
        self.local_arrays.push(dims.iter().product());
        if let Some(init) = &d.init {
            // Local arrays start zeroed; set the listed values with code.
            let mut cells: Vec<(usize, &Expr)> = Vec::new();
            fn walk<'a>(
                init: &'a Init,
                dims: &[u32],
                base: usize,
                out: &mut Vec<(usize, &'a Expr)>,
                at: &Diag,
            ) -> Result<(), Diag> {
                match init {
                    Init::Expr(e) if dims.is_empty() => {
                        out.push((base, e));
                        Ok(())
                    }
                    Init::List(items, line) if !dims.is_empty() => {
                        if items.len() > dims[0] as usize {
                            return Err(Diag {
                                line: *line,
                                message: format!(
                                    "{} values for an array of {}",
                                    items.len(),
                                    dims[0]
                                ),
                                ..at.clone()
                            });
                        }
                        let stride: usize = dims[1..].iter().map(|&d| d as usize).product();
                        for (i, it) in items.iter().enumerate() {
                            walk(it, &dims[1..], base + i * stride, out, at)?;
                        }
                        Ok(())
                    }
                    _ => Err(Diag {
                        message: "the initializer doesn't match the array's shape".into(),
                        ..at.clone()
                    }),
                }
            }
            walk(init, &dims, 0, &mut cells, &at)?;
            for (cell, e) in cells {
                self.push_num(cell as i32);
                self.expr(e)?;
                self.op1(ASSIGNSCRIPTARRAY, num as i32);
            }
        }
        if let Some(s) = self.scopes.last_mut() {
            s.insert(d.name.clone(), Var::Array(Space::Local, num, dims));
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), Diag> {
        match s {
            Stmt::Block(body) => {
                self.scopes.push(HashMap::new());
                for s in body {
                    self.stmt(s)?;
                }
                self.scopes.pop();
            }
            Stmt::Local(decls) => {
                for d in decls {
                    self.local(d)?;
                }
            }
            Stmt::Expr(e) => self.discard(e)?,
            Stmt::If(cond, then, other) => {
                let else_l = self.label();
                self.cond_jump(cond, false, else_l)?;
                self.stmt(then)?;
                if let Some(other) = other {
                    let end = self.label();
                    self.jump(GOTO, end);
                    self.place(else_l);
                    self.stmt(other)?;
                    self.place(end);
                } else {
                    self.place(else_l);
                }
            }
            Stmt::While { cond, body, until } => {
                let top = self.label();
                let end = self.label();
                self.place(top);
                self.cond_jump(cond, *until, end)?;
                self.loops.push(Loop {
                    brk: end,
                    cont: Some(top),
                });
                self.stmt(body)?;
                self.loops.pop();
                self.jump(GOTO, top);
                self.place(end);
            }
            Stmt::Do { body, cond, until } => {
                let top = self.label();
                let cont = self.label();
                let end = self.label();
                self.place(top);
                self.loops.push(Loop {
                    brk: end,
                    cont: Some(cont),
                });
                self.stmt(body)?;
                self.loops.pop();
                self.place(cont);
                self.expr(cond)?;
                self.jump(if *until { IFNOTGOTO } else { IFGOTO }, top);
                self.place(end);
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                self.scopes.push(HashMap::new());
                for s in init {
                    self.stmt(s)?;
                }
                let top = self.label();
                let cont = self.label();
                let end = self.label();
                self.place(top);
                if let Some(c) = cond {
                    self.cond_jump(c, false, end)?;
                }
                self.loops.push(Loop {
                    brk: end,
                    cont: Some(cont),
                });
                self.stmt(body)?;
                self.loops.pop();
                self.place(cont);
                for s in step {
                    self.stmt(s)?;
                }
                self.jump(GOTO, top);
                self.place(end);
                self.scopes.pop();
            }
            Stmt::Switch { value, body } => self.switch(value, body)?,
            Stmt::Break(file, line) => {
                let Some(l) = self.loops.last().map(|l| l.brk) else {
                    return Err(Diag {
                        file: *file,
                        line: *line,
                        message: "break outside a loop or switch".into(),
                    });
                };
                self.jump(GOTO, l);
            }
            Stmt::Continue(file, line) => {
                let Some(l) = self.loops.iter().rev().find_map(|l| l.cont) else {
                    return Err(Diag {
                        file: *file,
                        line: *line,
                        message: "continue outside a loop".into(),
                    });
                };
                self.jump(GOTO, l);
            }
            Stmt::Return(value, file, line) => {
                let at = Diag {
                    file: *file,
                    line: *line,
                    message: String::new(),
                };
                match (self.function, value) {
                    (None, None) => self.op(TERMINATE),
                    (None, Some(_)) => {
                        return Err(Diag {
                            message: "scripts don't return values (use SetResultValue)".into(),
                            ..at
                        });
                    }
                    (Some(false), None) => self.op(RETURNVOID),
                    (Some(false), Some(_)) => {
                        return Err(Diag {
                            message: "this function returns void".into(),
                            ..at
                        });
                    }
                    (Some(true), None) => {
                        return Err(Diag {
                            message: "this function must return a value".into(),
                            ..at
                        });
                    }
                    (Some(true), Some(v)) => {
                        self.expr(v)?;
                        self.op(RETURNVAL);
                    }
                }
            }
            Stmt::Restart => self.op(RESTART),
            Stmt::Terminate => self.op(TERMINATE),
            Stmt::Suspend => self.op(SUSPEND),
            Stmt::Empty => {}
        }
        Ok(())
    }

    /// Jump to `target` when `cond` is false (or true when `when_true`).
    fn cond_jump(&mut self, cond: &Expr, when_true: bool, target: Label) -> Result<(), Diag> {
        self.expr(cond)?;
        self.jump(if when_true { IFGOTO } else { IFNOTGOTO }, target);
        Ok(())
    }

    fn switch(&mut self, value: &Expr, body: &[SwitchItem]) -> Result<(), Diag> {
        let table = self.label();
        let end = self.label();
        self.expr(value)?;
        self.jump(GOTO, table);
        let mut cases: Vec<(i32, Label)> = Vec::new();
        let mut default = None;
        self.loops.push(Loop {
            brk: end,
            cont: None,
        });
        self.scopes.push(HashMap::new());
        for item in body {
            match item {
                SwitchItem::Case(e) => {
                    let v = self.const_eval(e)?;
                    if cases.iter().any(|(c, _)| *c == v) {
                        return Err(diag(e, format!("case {v} appears twice")));
                    }
                    let l = self.label();
                    self.place(l);
                    cases.push((v, l));
                }
                SwitchItem::Default(file, line) => {
                    if default.is_some() {
                        return Err(Diag {
                            file: *file,
                            line: *line,
                            message: "two default labels".into(),
                        });
                    }
                    let l = self.label();
                    self.place(l);
                    default = Some(l);
                }
                SwitchItem::Stmt(s) => self.stmt(s)?,
            }
        }
        self.scopes.pop();
        self.loops.pop();
        self.jump(GOTO, end);
        self.place(table);
        for (v, l) in cases {
            self.op1(CASEGOTO, v);
            self.fixups.push((self.code.len(), l));
            self.word(0);
        }
        self.op(DROP);
        self.jump(GOTO, default.unwrap_or(end));
        self.place(end);
        Ok(())
    }

    // ---- Expressions --------------------------------------------------------

    /// Evaluate `e` for its effect only.
    fn discard(&mut self, e: &Expr) -> Result<(), Diag> {
        match &e.kind {
            ExprKind::Assign(op, target, value) => self.assign(op, target, value, false),
            ExprKind::IncDec { inc, target, .. } => self.incdec(*inc, false, target, false),
            ExprKind::Call(name, args) => self.call(e, name, args, false),
            ExprKind::Print { .. } => self.print(e, false),
            ExprKind::Translation(num, ranges) => {
                self.expr(num)?;
                self.op(STARTTRANSLATION);
                for (kind, values) in ranges {
                    self.args(values)?;
                    self.op(match kind {
                        1 => TRANSLATIONRANGE1,
                        2 => TRANSLATIONRANGE2,
                        3 => TRANSLATIONRANGE3,
                        4 => TRANSLATIONRANGE4,
                        _ => TRANSLATIONRANGE5,
                    });
                }
                self.op(ENDTRANSLATION);
                Ok(())
            }
            _ => {
                self.expr(e)?;
                self.op(DROP);
                Ok(())
            }
        }
    }

    /// Push the value of `e`.
    fn expr(&mut self, e: &Expr) -> Result<(), Diag> {
        if let Some(v) = self.try_const(e) {
            self.push_num(v);
            return Ok(());
        }
        match &e.kind {
            ExprKind::Num(v) => self.push_num(*v),
            ExprKind::Str(s) => {
                let i = self.string(s);
                self.push_num(i as i32);
                self.op(TAGSTRING);
            }
            ExprKind::Name(n) => match self.lookup(n).cloned() {
                Some(Var::Scalar(space, i)) => self.op1(op_for(VAR_OPS, "push", space), i as i32),
                Some(Var::Array(..)) => {
                    return Err(diag(e, format!("{n} is an array; index it with []")));
                }
                None => {
                    if let Some(&(num, _, _)) = self.specials.get(n) {
                        self.push_num(num);
                    } else if self.funcs.contains_key(n) || self.builtins.contains_key(n.as_str()) {
                        return Err(diag(e, format!("{n} is a function; call it with ()")));
                    } else {
                        return Err(diag(e, format!("{n} isn't declared")));
                    }
                }
            },
            ExprKind::Index(..) => {
                let (space, num) = self.element(e)?;
                self.op1(op_for(ARRAY_OPS, "push", space), num as i32);
            }
            ExprKind::Call(name, args) => self.call(e, name, args, true)?,
            ExprKind::Print { .. } => self.print(e, true)?,
            ExprKind::Unary(op, a) => {
                self.expr(a)?;
                self.op(match *op {
                    "-" => UNARYMINUS,
                    "!" => NEGATELOGICAL,
                    _ => NEGATEBINARY,
                });
            }
            ExprKind::Binary(op, a, b) => {
                self.expr(a)?;
                self.expr(b)?;
                let code = BINARY_OPS
                    .iter()
                    .find(|(o, _)| o == op)
                    .map_or(NOP, |(_, c)| *c);
                self.op(code);
            }
            ExprKind::Ternary(c, a, b) => {
                let other = self.label();
                let end = self.label();
                self.cond_jump(c, false, other)?;
                self.expr(a)?;
                self.jump(GOTO, end);
                self.place(other);
                self.expr(b)?;
                self.place(end);
            }
            ExprKind::Assign(op, target, value) => self.assign(op, target, value, true)?,
            ExprKind::IncDec { inc, post, target } => self.incdec(*inc, *post, target, true)?,
            ExprKind::Translation(..) => return Err(diag(e, "CreateTranslation returns nothing")),
        }
        Ok(())
    }

    /// Push the flat index of an array element; returns the array's space and number.
    fn element(&mut self, e: &Expr) -> Result<(Space, u32), Diag> {
        let ExprKind::Index(name, idx) = &e.kind else {
            return Err(diag(e, "expected an array element"));
        };
        let Some(var) = self.lookup(name).cloned() else {
            return Err(diag(e, format!("{name} isn't declared")));
        };
        let Var::Array(space, num, dims) = var else {
            return Err(diag(e, format!("{name} isn't an array")));
        };
        let want = dims.len().max(1);
        if idx.len() != want {
            return Err(diag(
                e,
                format!(
                    "{name} takes {want} index{}",
                    if want == 1 { "" } else { "es" }
                ),
            ));
        }
        // Row-major: a[i][j][k] = i*(d1*d2) + j*d2 + k.
        for (i, ix) in idx.iter().enumerate() {
            self.expr(ix)?;
            let stride: u32 = dims.get(i + 1..).map_or(1, |d| d.iter().product());
            if stride != 1 {
                self.push_num(stride as i32);
                self.op(MULTIPLY);
            }
            if i > 0 {
                self.op(ADD);
            }
        }
        Ok((space, num))
    }

    fn target(&mut self, t: &Expr) -> Result<Var, Diag> {
        match &t.kind {
            ExprKind::Name(n) => match self.lookup(n).cloned() {
                Some(v @ Var::Scalar(..)) => Ok(v),
                Some(Var::Array(..)) => Err(diag(t, format!("{n} is an array; index it with []"))),
                None => Err(diag(t, format!("{n} isn't declared"))),
            },
            _ => Err(diag(t, "expected a variable")),
        }
    }

    fn assign(&mut self, op: &str, target: &Expr, value: &Expr, want: bool) -> Result<(), Diag> {
        if matches!(target.kind, ExprKind::Index(..)) {
            let (space, num) = self.element(target)?;
            self.expr(value)?;
            self.op1(op_for(ARRAY_OPS, op, space), num as i32);
            if want {
                self.element(target)?;
                self.op1(op_for(ARRAY_OPS, "push", space), num as i32);
            }
            return Ok(());
        }
        let Var::Scalar(space, i) = self.target(target)? else {
            unreachable!()
        };
        self.expr(value)?;
        self.op1(op_for(VAR_OPS, op, space), i as i32);
        if want {
            self.op1(op_for(VAR_OPS, "push", space), i as i32);
        }
        Ok(())
    }

    fn incdec(&mut self, inc: bool, post: bool, target: &Expr, want: bool) -> Result<(), Diag> {
        let op = if inc { "++" } else { "--" };
        if matches!(target.kind, ExprKind::Index(..)) {
            if want && post {
                let (space, num) = self.element(target)?;
                self.op1(op_for(ARRAY_OPS, "push", space), num as i32);
            }
            let (space, num) = self.element(target)?;
            self.op1(op_for(ARRAY_OPS, op, space), num as i32);
            if want && !post {
                self.element(target)?;
                self.op1(op_for(ARRAY_OPS, "push", space), num as i32);
            }
            return Ok(());
        }
        let Var::Scalar(space, i) = self.target(target)? else {
            unreachable!()
        };
        if want && post {
            self.op1(op_for(VAR_OPS, "push", space), i as i32);
        }
        self.op1(op_for(VAR_OPS, op, space), i as i32);
        if want && !post {
            self.op1(op_for(VAR_OPS, "push", space), i as i32);
        }
        Ok(())
    }

    fn args(&mut self, args: &[Expr]) -> Result<(), Diag> {
        for a in args {
            self.expr(a)?;
        }
        Ok(())
    }

    fn call(&mut self, e: &Expr, name: &str, args: &[Expr], want: bool) -> Result<(), Diag> {
        let count = |lo: usize, hi: usize| -> Result<(), Diag> {
            if args.len() < lo || args.len() > hi {
                let range = if lo == hi {
                    format!("{lo}")
                } else {
                    format!("{lo} to {hi}")
                };
                return Err(diag(
                    e,
                    format!(
                        "{name} takes {range} argument{}, not {}",
                        if hi == 1 { "" } else { "s" },
                        args.len()
                    ),
                ));
            }
            Ok(())
        };
        if let Some(f) = self.funcs.get(name) {
            let (index, argc, returns) = (f.index, usize::from(f.argc), f.returns);
            count(argc, argc)?;
            if want && !returns {
                return Err(diag(e, format!("{name} returns nothing")));
            }
            self.args(args)?;
            self.op1(
                if !want && returns { CALLDISCARD } else { CALL },
                index as i32,
            );
            return Ok(());
        }
        // Waits that ACC builds from two p-codes.
        if name == "acs_executewait" || name == "acs_namedexecutewait" {
            if want {
                return Err(diag(e, format!("{name} returns nothing")));
            }
            count(1, 5)?;
            self.args(args)?;
            if name == "acs_executewait" {
                self.op1(LSPEC1 + args.len() as u32 - 1, ACS_EXECUTE);
                self.expr(&args[0])?;
                self.op(SCRIPTWAIT);
            } else {
                let idx = self
                    .callfuncs
                    .get("acs_namedexecute")
                    .copied()
                    .unwrap_or(39);
                self.word(CALLFUNC);
                self.word(args.len() as u32);
                self.word(idx as u32);
                self.op(DROP);
                self.expr(&args[0])?;
                self.op(SCRIPTWAITNAMED);
            }
            return Ok(());
        }
        if let Some(&b) = self.builtins.get(name) {
            count(usize::from(b.min), usize::from(b.argc))?;
            if want && !b.returns {
                return Err(diag(e, format!("{name} returns nothing")));
            }
            // ScriptWait("name") waits for a named script.
            let pcode = if b.pcode == SCRIPTWAIT && matches!(args[0].kind, ExprKind::Str(_)) {
                SCRIPTWAITNAMED
            } else {
                b.pcode
            };
            self.args(args)?;
            for i in args.len()..usize::from(b.argc) {
                self.push_num(default_arg(name, i));
            }
            self.op(pcode);
            if b.returns && !want {
                self.op(DROP);
            }
            return Ok(());
        }
        if let Some(&(num, min, max)) = self.specials.get(name) {
            count(usize::from(min), usize::from(max))?;
            self.args(args)?;
            if want {
                for _ in args.len()..5 {
                    self.push_num(0);
                }
                self.op1(LSPEC5RESULT, num);
            } else if args.is_empty() {
                self.push_num(0);
                self.op1(LSPEC1, num);
            } else {
                self.op1(LSPEC1 + args.len() as u32 - 1, num);
            }
            return Ok(());
        }
        if let Some(&idx) = self.callfuncs.get(name) {
            self.args(args)?;
            self.word(CALLFUNC);
            self.word(args.len() as u32);
            self.word(idx as u32);
            if !want {
                self.op(DROP);
            }
            return Ok(());
        }
        Err(diag(e, format!("unknown function {name}")))
    }

    fn print(&mut self, e: &Expr, want: bool) -> Result<(), Diag> {
        let ExprKind::Print { name, items, extra } = &e.kind else {
            return Ok(());
        };
        if ![
            "print",
            "printbold",
            "log",
            "hudmessage",
            "hudmessagebold",
            "strparam",
        ]
        .contains(&name.as_str())
        {
            return Err(diag(
                e,
                format!(
                    "unknown function {name} (s:, d: … items go in Print, PrintBold, Log, HudMessage or StrParam)"
                ),
            ));
        }
        let returns = name == "strparam";
        if want && !returns {
            return Err(diag(e, format!("{name} returns nothing")));
        }
        let hud = name.starts_with("hudmessage");
        if hud && extra.len() < 6 {
            return Err(diag(
                e,
                format!("{name} needs type, id, color, x, y and hold after the ';'"),
            ));
        }
        if !hud && !extra.is_empty() {
            return Err(diag(e, format!("{name} has no arguments after ';'")));
        }
        self.op(BEGINPRINT);
        for (kind, v) in items {
            self.expr(v)?;
            self.op(match kind {
                's' => PRINTSTRING,
                'c' => PRINTCHARACTER,
                'f' => PRINTFIXED,
                'n' => PRINTNAME,
                'l' => PRINTLOCALIZED,
                'k' => PRINTBIND,
                'b' => PRINTBINARY,
                'x' => PRINTHEX,
                _ => PRINTNUMBER,
            });
        }
        match name.as_str() {
            "print" => self.op(ENDPRINT),
            "printbold" => self.op(ENDPRINTBOLD),
            "log" => self.op(ENDLOG),
            "strparam" => {
                self.op(SAVESTRING);
                if !want {
                    self.op(DROP);
                }
            }
            _ => {
                self.op(MOREHUDMESSAGE);
                self.args(&extra[..6])?;
                self.op(OPTHUDMESSAGE);
                self.args(&extra[6..])?;
                self.op(if name == "hudmessagebold" {
                    ENDHUDMESSAGEBOLD
                } else {
                    ENDHUDMESSAGE
                });
            }
        }
        Ok(())
    }

    // ---- Output -------------------------------------------------------------

    fn chunk(&mut self, id: &[u8; 4], payload: &[u8]) {
        self.code.extend_from_slice(id);
        self.word(payload.len() as u32);
        self.code.extend_from_slice(payload);
    }

    /// `count, offsets…, NUL-terminated names` (SNAM / FNAM).
    fn name_table(names: &[String]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&(names.len() as u32).to_le_bytes());
        let mut off = 4 + 4 * names.len();
        for n in names {
            p.extend_from_slice(&(off as u32).to_le_bytes());
            off += n.len() + 1;
        }
        for n in names {
            p.extend_from_slice(n.as_bytes());
            p.push(0);
        }
        p
    }

    fn finish(mut self) -> Vec<u8> {
        while self.code.len() % 4 != 0 {
            self.code.push(0);
        }
        let chunks = self.pos();
        self.code[4..8].copy_from_slice(&chunks.to_le_bytes());

        let mut sptr = Vec::new();
        let mut sflg = Vec::new();
        let mut svct = Vec::new();
        for s in &self.scripts {
            sptr.extend_from_slice(&(s.number as i16).to_le_bytes());
            sptr.extend_from_slice(&s.kind.to_le_bytes());
            sptr.extend_from_slice(&s.address.to_le_bytes());
            sptr.extend_from_slice(&s.argc.to_le_bytes());
            if s.flags != 0 {
                sflg.extend_from_slice(&(s.number as i16).to_le_bytes());
                sflg.extend_from_slice(&s.flags.to_le_bytes());
            }
            svct.extend_from_slice(&(s.number as i16).to_le_bytes());
            svct.extend_from_slice(&(s.locals.clamp(1, 65535) as u16).to_le_bytes());
        }
        self.chunk(b"SPTR", &sptr);
        if !sflg.is_empty() {
            self.chunk(b"SFLG", &sflg);
        }
        self.chunk(b"SVCT", &svct);
        if !self.script_names.is_empty() {
            let p = Self::name_table(&self.script_names);
            self.chunk(b"SNAM", &p);
        }
        if !self.func_out.is_empty() {
            let mut p = Vec::new();
            for f in &self.func_out {
                // Imports: address 0 and import number 0; the VM links them by name.
                p.extend_from_slice(&[f.argc, f.locals, u8::from(f.returns), 0]);
                let address = if f.imported { 0 } else { f.address };
                p.extend_from_slice(&address.to_le_bytes());
            }
            self.chunk(b"FUNC", &p);
            let p = Self::name_table(&self.func_names);
            self.chunk(b"FNAM", &p);
        }
        if !self.strings.is_empty() {
            let mut p = Vec::new();
            p.extend_from_slice(&0u32.to_le_bytes());
            p.extend_from_slice(&(self.strings.len() as u32).to_le_bytes());
            p.extend_from_slice(&0u32.to_le_bytes());
            let mut off = 12 + 4 * self.strings.len();
            for s in &self.strings {
                p.extend_from_slice(&(off as u32).to_le_bytes());
                off += s.len() + 1;
            }
            for s in &self.strings {
                p.extend_from_slice(s.as_bytes());
                p.push(0);
            }
            self.chunk(b"STRL", &p);
        }
        if let Some(last) = self.mini.iter().rposition(|&v| v != 0) {
            let mut p = 0u32.to_le_bytes().to_vec();
            for v in &self.mini[..=last] {
                p.extend_from_slice(&v.to_le_bytes());
            }
            self.chunk(b"MINI", &p);
        }
        if !self.arrays.is_empty() {
            let mut p = Vec::new();
            for (var, size, _) in &self.arrays {
                p.extend_from_slice(&var.to_le_bytes());
                p.extend_from_slice(&size.to_le_bytes());
            }
            self.chunk(b"ARAY", &p);
            let inits: Vec<(u32, Vec<i32>)> = self
                .arrays
                .iter()
                .filter(|a| !a.2.is_empty())
                .map(|a| (a.0, a.2.clone()))
                .collect();
            for (var, values) in inits {
                let mut p = var.to_le_bytes().to_vec();
                for v in values {
                    p.extend_from_slice(&v.to_le_bytes());
                }
                self.chunk(b"AINI", &p);
            }
        }
        let sary: Vec<(i32, Vec<u32>)> = self
            .scripts
            .iter()
            .filter(|s| !s.arrays.is_empty())
            .map(|s| (s.number, s.arrays.clone()))
            .collect();
        for (num, sizes) in sary {
            let mut p = (num as i16).to_le_bytes().to_vec();
            for s in sizes {
                p.extend_from_slice(&s.to_le_bytes());
            }
            self.chunk(b"SARY", &p);
        }
        let fary: Vec<(usize, Vec<u32>)> = self
            .func_out
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.arrays.is_empty())
            .map(|(i, f)| (i, f.arrays.clone()))
            .collect();
        for (i, sizes) in fary {
            let mut p = (i as u16).to_le_bytes().to_vec();
            for s in sizes {
                p.extend_from_slice(&s.to_le_bytes());
            }
            self.chunk(b"FARY", &p);
        }
        if !self.loads.is_empty() {
            let mut p = Vec::new();
            for n in &self.loads {
                p.extend_from_slice(n.as_bytes());
                p.push(0);
            }
            self.chunk(b"LOAD", &p);
        }
        if !self.mimp.is_empty() {
            let mut p = Vec::new();
            for (var, name) in &self.mimp {
                p.extend_from_slice(&var.to_le_bytes());
                p.extend_from_slice(name.as_bytes());
                p.push(0);
            }
            self.chunk(b"MIMP", &p);
        }
        if !self.aimp.is_empty() {
            let mut p = (self.aimp.len() as u32).to_le_bytes().to_vec();
            for (var, size, name) in &self.aimp {
                p.extend_from_slice(&var.to_le_bytes());
                p.extend_from_slice(&size.to_le_bytes());
                p.extend_from_slice(name.as_bytes());
                p.push(0);
            }
            self.chunk(b"AIMP", &p);
        }
        if self.library {
            // Exported names by slot, so importers link to them.
            let p = Self::name_table(&self.map_names);
            self.chunk(b"MEXP", &p);
            if !self.mstr.is_empty() {
                let p: Vec<u8> = self.mstr.iter().flat_map(|v| v.to_le_bytes()).collect();
                self.chunk(b"MSTR", &p);
            }
            for (var, tags) in std::mem::take(&mut self.atags) {
                let mut p = vec![0u8];
                p.extend_from_slice(&var.to_le_bytes());
                p.extend_from_slice(&tags);
                self.chunk(b"ATAG", &p);
            }
        }
        // The loader wants at least a 32-byte lump.
        while self.code.len() < 32 {
            self.code.push(0);
        }
        self.code
    }
}

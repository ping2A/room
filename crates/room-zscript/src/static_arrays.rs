//! Class-level constant tables: `static const String Names[] = { "a", "b" };`.
//!
//! GZDoom keeps these as one read-only array per class. Room has no
//! class-static storage, so after parsing every method that reads a table gets
//! a local copy built at entry (`Array<T> t; t.Push(…)…`) and its reads —
//! `Names[i]`, `Names.Size()`, `Owner.Names[i]` — are pointed at that local.
//! The tables are small (name lists, flag tables), so the copy is cheap.

use crate::ast::{Block, ClassDecl, Decl, Expr, Member, Stmt, TranslationUnit, TypeRef};
use std::collections::{BTreeMap, BTreeSet};

/// Local name standing in for `owner.name` inside a method.
fn local_name(owner: &str, name: &str) -> String {
    format!(
        "__room_sa_{}_{}",
        owner.to_ascii_lowercase(),
        name.to_ascii_lowercase()
    )
}

struct Tables<'a> {
    /// owner (lower) → name (lower) → table index.
    by_owner: BTreeMap<String, BTreeMap<String, usize>>,
    /// class (lower) → parent (lower).
    parents: BTreeMap<String, String>,
    /// class (lower) → mixins (lower).
    mixins: BTreeMap<String, Vec<String>>,
    unit: &'a [crate::ast::StaticArray],
}

impl Tables<'_> {
    /// Table `name` visible unqualified from class `class`.
    fn visible(&self, class: &str, name: &str) -> Option<usize> {
        let name = name.to_ascii_lowercase();
        let mut cur = Some(class.to_ascii_lowercase());
        for _ in 0..64 {
            let c = cur?;
            if let Some(i) = self.by_owner.get(&c).and_then(|m| m.get(&name)) {
                return Some(*i);
            }
            for mixin in self.mixins.get(&c).into_iter().flatten() {
                if let Some(i) = self.by_owner.get(mixin).and_then(|m| m.get(&name)) {
                    return Some(*i);
                }
            }
            cur = self.parents.get(&c).cloned();
        }
        None
    }

    /// `Owner.name` written out.
    fn qualified(&self, owner: &str, name: &str) -> Option<usize> {
        let key = owner.to_ascii_lowercase();
        if self.by_owner.contains_key(&key) || self.mixins.contains_key(&key) {
            self.visible(owner, name)
        } else {
            None
        }
    }
}

/// Post-parse passes: mixin expansion, then the class tables.
pub(crate) fn post_parse(unit: &mut TranslationUnit) {
    expand_mixins(unit);
    apply(unit);
    // A mixin is only text to paste: GZDoom never compiles it on its own
    // (its methods call `Super.Tick()`, read `Tracer`, … of the user class).
    unit.decls.retain(|d| !matches!(d, Decl::Mixin(_)));
}

/// `mixin Foo;` pastes the members of `mixin class Foo` into the class
/// (GZDoom: textual inclusion), so its methods, fields and tables belong to
/// every class that uses it.
fn expand_mixins(unit: &mut TranslationUnit) {
    let mut mixins: BTreeMap<String, Vec<Member>> = BTreeMap::new();
    for d in &unit.decls {
        if let Decl::Mixin(m) = d {
            let members = m
                .members
                .iter()
                .filter(|x| !matches!(x, Member::MixinUse { .. }))
                .cloned()
                .collect();
            mixins.insert(m.name.to_ascii_lowercase(), members);
        }
    }
    if mixins.is_empty() {
        return;
    }
    for d in &mut unit.decls {
        if let Decl::Class(c) | Decl::ExtendClass(c) = d {
            if c.mixins.is_empty() || c.members.iter().any(is_expanded_marker) {
                continue;
            }
            let mut out = Vec::with_capacity(c.members.len());
            for m in std::mem::take(&mut c.members) {
                if let Member::MixinUse { name, span } = &m {
                    if let Some(body) = mixins.get(&name.to_ascii_lowercase()) {
                        out.push(Member::MixinUse {
                            name: format!("{EXPANDED}{name}"),
                            span: *span,
                        });
                        out.extend(body.iter().cloned());
                        continue;
                    }
                }
                out.push(m);
            }
            c.members = out;
        }
    }
}

/// Prefix marking a `mixin` line already pasted (keeps the pass idempotent).
const EXPANDED: &str = "__room_expanded:";

fn is_expanded_marker(m: &Member) -> bool {
    matches!(m, Member::MixinUse { name, .. } if name.starts_with(EXPANDED))
}

/// Rewrite table reads and add the per-method local copies.
fn apply(unit: &mut TranslationUnit) {
    if unit.static_arrays.is_empty() {
        return;
    }
    let tables_src = std::mem::take(&mut unit.static_arrays);
    let mut tables = Tables {
        by_owner: BTreeMap::new(),
        parents: BTreeMap::new(),
        mixins: BTreeMap::new(),
        unit: &tables_src,
    };
    for (i, t) in tables_src.iter().enumerate() {
        tables
            .by_owner
            .entry(t.owner.to_ascii_lowercase())
            .or_default()
            .entry(t.name.to_ascii_lowercase())
            .or_insert(i);
    }
    for d in &unit.decls {
        if let Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) = d {
            let key = c.name.to_ascii_lowercase();
            if let Some(p) = &c.parent {
                tables.parents.insert(key.clone(), p.to_ascii_lowercase());
            }
            let list = tables.mixins.entry(key).or_default();
            list.extend(c.mixins.iter().map(|m| m.to_ascii_lowercase()));
        }
    }
    for d in &mut unit.decls {
        match d {
            Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => class(&tables, c),
            Decl::Struct(s) => {
                let name = s.name.clone();
                members(&tables, &name, &mut s.members);
            }
            _ => {}
        }
    }
    unit.static_arrays = tables_src;
}

fn class(tables: &Tables<'_>, c: &mut ClassDecl) {
    let name = c.name.clone();
    members(tables, &name, &mut c.members);
}

fn members(tables: &Tables<'_>, owner: &str, list: &mut [Member]) {
    for m in list {
        match m {
            Member::Method(md) => {
                let Some(body) = md.body.as_mut() else {
                    continue;
                };
                let mut shadow: BTreeSet<String> = md
                    .params
                    .iter()
                    .map(|p| p.name.to_ascii_lowercase())
                    .collect();
                declared_locals(body, &mut shadow);
                let mut used = BTreeSet::new();
                let mut rw = Rewriter {
                    tables,
                    owner,
                    shadow: &shadow,
                    used: &mut used,
                };
                rw.block(body);
                if used.is_empty() {
                    continue;
                }
                let span = body.span;
                let mut prologue = Vec::new();
                for i in used {
                    let t = &tables.unit[i];
                    let local = local_name(&t.owner, &t.name);
                    prologue.push(Stmt::Local {
                        ty: Some(TypeRef::Array(Box::new(t.elem.clone()))),
                        names: vec![local.clone()],
                        init: None,
                        extra_inits: Vec::new(),
                        span,
                    });
                    for v in &t.values {
                        let vs = v.span();
                        prologue.push(Stmt::Expr {
                            expr: Expr::Call {
                                callee: Box::new(Expr::Member {
                                    obj: Box::new(Expr::Ident(local.clone(), vs)),
                                    field: "Push".into(),
                                    span: vs,
                                }),
                                args: vec![v.clone()],
                                span: vs,
                            },
                            span: vs,
                        });
                    }
                }
                prologue.append(&mut body.stmts);
                body.stmts = prologue;
            }
            Member::NestedStruct(s) => {
                let name = s.name.clone();
                members(tables, &name, &mut s.members);
            }
            _ => {}
        }
    }
}

fn declared_locals(block: &Block, out: &mut BTreeSet<String>) {
    for s in &block.stmts {
        declared_in_stmt(s, out);
    }
}

fn declared_in_stmt(s: &Stmt, out: &mut BTreeSet<String>) {
    match s {
        Stmt::Local { names, .. } => {
            out.extend(names.iter().map(|n| n.to_ascii_lowercase()));
        }
        Stmt::If { then_b, else_b, .. } => {
            declared_locals(then_b, out);
            if let Some(e) = else_b {
                declared_locals(e, out);
            }
        }
        Stmt::While { body, .. }
        | Stmt::DoWhile { body, .. }
        | Stmt::Switch { body, .. }
        | Stmt::Block(body) => declared_locals(body, out),
        Stmt::For { init, body, .. } => {
            if let Some(i) = init {
                declared_in_stmt(i, out);
            }
            declared_locals(body, out);
        }
        _ => {}
    }
}

struct Rewriter<'a, 'b> {
    tables: &'a Tables<'b>,
    owner: &'a str,
    shadow: &'a BTreeSet<String>,
    used: &'a mut BTreeSet<usize>,
}

impl Rewriter<'_, '_> {
    fn block(&mut self, b: &mut Block) {
        for s in &mut b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Local {
                init, extra_inits, ..
            } => {
                if let Some(e) = init {
                    self.expr(e);
                }
                for e in extra_inits.iter_mut().flatten() {
                    self.expr(e);
                }
            }
            Stmt::Expr { expr, .. } => self.expr(expr),
            Stmt::Return { values, .. } => {
                for v in values {
                    self.expr(v);
                }
            }
            Stmt::If {
                cond,
                then_b,
                else_b,
                ..
            } => {
                self.expr(cond);
                self.block(then_b);
                if let Some(e) = else_b {
                    self.block(e);
                }
            }
            Stmt::While { cond, body, .. }
            | Stmt::DoWhile { cond, body, .. }
            | Stmt::Switch { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                if let Some(i) = init {
                    self.stmt(i);
                }
                if let Some(c) = cond {
                    self.expr(c);
                }
                if let Some(st) = step {
                    self.expr(st);
                }
                self.block(body);
            }
            Stmt::Case { value, .. } => {
                if let Some(v) = value {
                    self.expr(v);
                }
            }
            Stmt::Block(b) => self.block(b),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }

    fn hit(&mut self, i: usize, span: crate::span::Span) -> Expr {
        self.used.insert(i);
        let t = &self.tables.unit[i];
        Expr::Ident(local_name(&t.owner, &t.name), span)
    }

    fn expr(&mut self, e: &mut Expr) {
        match e {
            Expr::Ident(name, span) => {
                if self.shadow.contains(&name.to_ascii_lowercase()) {
                    return;
                }
                if let Some(i) = self.tables.visible(self.owner, name) {
                    *e = self.hit(i, *span);
                }
            }
            Expr::Member { obj, field, span } => {
                if let Expr::Ident(owner, _) = obj.as_ref() {
                    let lower = owner.to_ascii_lowercase();
                    // `self.PB_ActUpgrades`: the class's own table.
                    if (lower == "self" || lower == "invoker") && !self.shadow.contains(&lower) {
                        if let Some(i) = self.tables.visible(self.owner, field) {
                            *e = self.hit(i, *span);
                            return;
                        }
                    }
                    if !self.shadow.contains(&lower) {
                        if let Some(i) = self.tables.qualified(owner, field) {
                            *e = self.hit(i, *span);
                            return;
                        }
                    }
                }
                self.expr(obj);
            }
            Expr::Binary { op, rhs, .. } if op == crate::ast::NAMED_ARG => self.expr(rhs),
            Expr::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => self.expr(expr),
            Expr::Call { callee, args, .. } => {
                // A call's own name (`Foo(…)`) is never a table.
                if !matches!(callee.as_ref(), Expr::Ident(..)) {
                    self.expr(callee);
                }
                for a in args {
                    self.expr(a);
                }
            }
            Expr::Index { arr, index, .. } => {
                self.expr(arr);
                self.expr(index);
            }
            Expr::Ternary {
                cond,
                then_e,
                else_e,
                ..
            } => {
                self.expr(cond);
                self.expr(then_e);
                self.expr(else_e);
            }
            Expr::Tuple { elems, .. } => {
                for x in elems {
                    self.expr(x);
                }
            }
            Expr::Int(..)
            | Expr::Float(..)
            | Expr::String(..)
            | Expr::Name(..)
            | Expr::New { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use crate::ast::{Decl, Member, Stmt};

    fn method_body<'a>(unit: &'a crate::ast::TranslationUnit, class: &str, m: &str) -> &'a [Stmt] {
        for d in &unit.decls {
            if let Decl::Class(c) | Decl::Mixin(c) = d {
                if c.name.eq_ignore_ascii_case(class) {
                    for mem in &c.members {
                        if let Member::Method(md) = mem {
                            if md.name.eq_ignore_ascii_case(m) {
                                return &md.body.as_ref().unwrap().stmts;
                            }
                        }
                    }
                }
            }
        }
        panic!("no {class}.{m}");
    }

    #[test]
    fn class_table_becomes_method_local() {
        let src = r#"
class A {
    static const String Names[] = { "John", "Jeff", "Chris", };
    String Pick(int i) { return Names[i]; }
    int NoUse() { return 1; }
}
class B : A {
    int Count() { return Names.Size(); }
    String Own() { return self.Names[0]; }
}
class C {
    String Other() { return A.Names[0]; }
    String Shadow(String Names) { return Names; }
}
"#;
        let r = crate::parse::parse_file("t.zs", src);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        let pick = method_body(&r.unit, "A", "Pick");
        // Local + 3 pushes + return.
        assert_eq!(pick.len(), 5, "{pick:?}");
        assert!(matches!(&pick[0], Stmt::Local { names, .. } if names[0] == "__room_sa_a_names"));
        assert_eq!(method_body(&r.unit, "A", "NoUse").len(), 1);
        assert_eq!(method_body(&r.unit, "B", "Count").len(), 5);
        assert_eq!(method_body(&r.unit, "B", "Own").len(), 5);
        assert_eq!(method_body(&r.unit, "C", "Other").len(), 5);
        assert_eq!(method_body(&r.unit, "C", "Shadow").len(), 1);
    }

    #[test]
    fn mixin_table_is_visible_in_mixin_methods() {
        let src = r#"
mixin class M {
    static const String First[] =
    {
        "a",
        "b"
    };
    String Get() { return First[random(0, First.Size() - 1)]; }
}
class Z { mixin M; }
"#;
        let r = crate::parse::parse_file("t.zs", src);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(method_body(&r.unit, "Z", "Get").len(), 4);
    }
}

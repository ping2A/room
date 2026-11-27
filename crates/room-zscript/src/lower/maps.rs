//! `Map<K, V>` / `MapIterator<K, V>` as UZDoom's native structs.
//!
//! UZDoom instantiates each generic map as one of the native structs in
//! `engine/maps.zs` (`Map_Str_Obj`, `MapIterator_I32_F64`, …). When those
//! are in the unit, the types are rewritten to them before lowering, so the
//! methods resolve to their `native` declarations and run as
//! `map_<k>_<v>.<method>` natives. The structs are values: a local or field
//! of one is allocated like a script struct.

use crate::ast::{Block, Decl, Member, Stmt, TranslationUnit, TypeRef};
use std::collections::BTreeSet;

type Names = BTreeSet<String>;

/// Whether `name` is one of the native map or iterator structs.
pub(super) fn is_map_struct(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.starts_with("map_") || l.starts_with("mapiterator_")
}

/// The unit has the UZDoom map structs and uses a generic map somewhere.
pub(super) fn wanted(unit: &TranslationUnit) -> bool {
    let declared = unit
        .decls
        .iter()
        .any(|d| matches!(d, Decl::Struct(s) if s.name.eq_ignore_ascii_case("Map_Str_Obj")));
    declared && unit.decls.iter().any(decl_uses_maps)
}

/// A copy of `unit` with every generic map type replaced.
pub(super) fn rewritten(unit: &TranslationUnit) -> TranslationUnit {
    let mut unit = unit.clone();
    let mut names = Names::new();
    for d in &unit.decls {
        match d {
            Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => {
                names.insert(c.name.to_ascii_lowercase());
                struct_names(&c.members, &mut names);
            }
            Decl::Struct(s) => {
                names.insert(s.name.to_ascii_lowercase());
                struct_names(&s.members, &mut names);
            }
            _ => {}
        }
    }
    for d in &mut unit.decls {
        match d {
            Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => {
                members(&mut c.members, &names)
            }
            Decl::Struct(s) => members(&mut s.members, &names),
            _ => {}
        }
    }
    unit
}

fn struct_names(ms: &[Member], out: &mut Names) {
    for m in ms {
        if let Member::NestedStruct(s) = m {
            out.insert(s.name.to_ascii_lowercase());
            struct_names(&s.members, out);
        }
    }
}

fn decl_uses_maps(d: &Decl) -> bool {
    let ms = match d {
        Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => &c.members,
        Decl::Struct(s) => &s.members,
        _ => return false,
    };
    members_use_maps(ms)
}

fn members_use_maps(ms: &[Member]) -> bool {
    ms.iter().any(|m| match m {
        Member::Field(f) => is_generic(&f.ty),
        Member::Method(meth) => {
            is_generic(&meth.ret)
                || meth.params.iter().any(|p| is_generic(&p.ty))
                || meth.body.as_ref().is_some_and(block_uses_maps)
        }
        Member::NestedStruct(s) => members_use_maps(&s.members),
        _ => false,
    })
}

fn block_uses_maps(b: &Block) -> bool {
    b.stmts.iter().any(stmt_uses_maps)
}

fn stmt_uses_maps(s: &Stmt) -> bool {
    match s {
        Stmt::Local { ty, .. } => ty.as_ref().is_some_and(is_generic),
        Stmt::If { then_b, else_b, .. } => {
            block_uses_maps(then_b) || else_b.as_ref().is_some_and(block_uses_maps)
        }
        Stmt::While { body, .. }
        | Stmt::DoWhile { body, .. }
        | Stmt::Switch { body, .. }
        | Stmt::Block(body) => block_uses_maps(body),
        Stmt::For { init, body, .. } => {
            init.as_deref().is_some_and(stmt_uses_maps) || block_uses_maps(body)
        }
        _ => false,
    }
}

fn is_generic(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Map(..) | TypeRef::MapIterator(..) => true,
        TypeRef::Array(inner) | TypeRef::Readonly(inner) => is_generic(inner),
        _ => false,
    }
}

fn members(ms: &mut [Member], n: &Names) {
    for m in ms {
        match m {
            Member::Field(f) => ty(&mut f.ty, n),
            Member::Method(meth) => {
                ty(&mut meth.ret, n);
                for t in &mut meth.extra_rets {
                    ty(t, n);
                }
                for p in &mut meth.params {
                    ty(&mut p.ty, n);
                }
                if let Some(b) = &mut meth.body {
                    block(b, n);
                }
            }
            Member::NestedStruct(s) => members(&mut s.members, n),
            _ => {}
        }
    }
}

fn block(b: &mut Block, n: &Names) {
    for s in &mut b.stmts {
        stmt(s, n);
    }
}

fn stmt(s: &mut Stmt, n: &Names) {
    match s {
        Stmt::Local { ty: Some(t), .. } => ty(t, n),
        Stmt::If { then_b, else_b, .. } => {
            block(then_b, n);
            if let Some(e) = else_b {
                block(e, n);
            }
        }
        Stmt::While { body, .. }
        | Stmt::DoWhile { body, .. }
        | Stmt::Switch { body, .. }
        | Stmt::Block(body) => block(body, n),
        Stmt::For { init, body, .. } => {
            if let Some(i) = init {
                stmt(i, n);
            }
            block(body, n);
        }
        _ => {}
    }
}

fn ty(t: &mut TypeRef, n: &Names) {
    match t {
        TypeRef::Map(k, v) => *t = TypeRef::Named(format!("Map_{}_{}", key(k), value(v, n))),
        TypeRef::MapIterator(k, v) => {
            *t = TypeRef::Named(format!("MapIterator_{}_{}", key(k), value(v, n)));
        }
        TypeRef::Array(inner) | TypeRef::Readonly(inner) => ty(inner, n),
        _ => {}
    }
}

fn base(t: &TypeRef) -> &TypeRef {
    match t {
        TypeRef::Readonly(inner) => base(inner),
        other => other,
    }
}

/// Names are strings in Room, so `Map<Name, …>` keys by string.
fn key(t: &TypeRef) -> &'static str {
    match base(t) {
        TypeRef::Named(n) if n.eq_ignore_ascii_case("string") || n.eq_ignore_ascii_case("name") => {
            "Str"
        }
        _ => "I32",
    }
}

/// Classes and structs are objects; ints, enums and the value types
/// Room keeps as ints are `I32`.
fn value(t: &TypeRef, names: &Names) -> &'static str {
    match base(t) {
        TypeRef::ClassOf(_) => "Str",
        TypeRef::Named(n) => match n.to_ascii_lowercase().as_str() {
            "string" | "name" | "class" => "Str",
            "double" => "F64",
            "float" => "F32",
            l if names.contains(l) => "Obj",
            _ => "I32",
        },
        _ => "Obj",
    }
}

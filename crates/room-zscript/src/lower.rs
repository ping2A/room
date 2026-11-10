//! Statement lowering onto the [`room_zvm::Op::Arg`] / `Call` / `Ret` ABI.
//!
//! This pass covers static integer methods: locals, arithmetic, bitwise ops,
//! `if` / `while` / `do` / `for`, `switch`, ternary, `break` / `continue`, and
//! static calls. The older `compile.rs` pass still lowers the full Brutal Doom subset.

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::sema::Analysis;
use room_zvm::{ClassLayout, CompiledFunction, CompiledProgram, FuncParam, Op};
use std::collections::BTreeMap;

mod fvec;
use fvec::*;

#[derive(Clone)]
struct FnSig {
    idx: u32,
    instance: bool,
    /// Param banks, with `self` (bank 3) first on instance methods.
    params: Vec<u8>,
    ret: u8,
    /// User-argument indexes that are `out` or `ref`.
    outs: Vec<usize>,
    /// Declared `native` with no body. Calls lower to `CallNat`.
    native: bool,
    /// Vector size of each user parameter (0 = not a vector).
    vec_params: Vec<u8>,
    /// User parameters typed `class<T>`; natives take these as names.
    class_params: Vec<bool>,
    /// Lowercased class of an object result (`Actor GetPointer(…)`).
    ret_class: String,
    /// Banks of every result (`int, int GetAmount(…)` → `[0, 0]`).
    ret_banks: Vec<u8>,
    /// Default value of each user parameter, filled in at the call when the
    /// argument is left out.
    defaults: Vec<Option<Expr>>,
    /// `vararg` native (`Screen.DrawText(…, ...)`): arguments past the
    /// declared ones are passed too, followed by a layout, see
    /// [`prepare_sig_args`].
    varargs: bool,
}

struct Jump {
    breaks: Vec<usize>,
    continues: Vec<usize>,
    is_loop: bool,
}

struct Lx {
    locals: BTreeMap<String, u8>,
    locals_s: BTreeMap<String, u8>,
    /// Array local → (object register, string elements).
    locals_a: BTreeMap<String, (u8, bool)>,
    /// Object local → (register, lowercased class).
    locals_o: BTreeMap<String, (u8, String)>,
    locals_f: BTreeMap<String, u8>,
    /// Vector local → (first float register, components).
    locals_v: BTreeMap<String, (u8, u8)>,
    /// Int locals of a value struct type (`TextureID`) → lowercased type.
    local_types: BTreeMap<String, String>,
    /// Fields of a value struct type (`TextureID Icon`) by lowercased name →
    /// lowercased type, for untyped `let icon = inv.Icon`.
    value_fields: BTreeMap<String, String>,
    next: u16,
    next_s: u16,
    next_a: u16,
    next_f: u16,
    /// Most registers each bank has used (`next` drops back after a
    /// statement's temporaries).
    peak: [u16; 4],
    /// Lowercased `Class.method`.
    funcs: BTreeMap<String, FnSig>,
    /// Lowercased class → id (`NewObj` ext).
    class_ids: BTreeMap<String, u32>,
    /// Lowercased class → array fields `(name, slot, string elements)`.
    array_fields: BTreeMap<String, Vec<(String, u16, bool)>>,
    /// Lowercased class → int/bool field name → slot.
    int_fields: BTreeMap<String, BTreeMap<String, u16>>,
    /// Lowercased class → string field name → slot.
    string_fields: BTreeMap<String, BTreeMap<String, u16>>,
    /// Lowercased class → float/vector field → (first float slot, components).
    float_fields: BTreeMap<String, BTreeMap<String, (u16, u8)>>,
    /// Lowercased class → object field → (slot, field class).
    object_fields: BTreeMap<String, BTreeMap<String, (u16, String)>>,
    /// Lowercased class → lowercased parent.
    parents: BTreeMap<String, String>,
    consts: BTreeMap<String, i64>,
    /// Lowercased float constant → value. Compiler-side folding only.
    float_consts: BTreeMap<String, f64>,
    /// Lowercased string constant → value.
    string_consts: BTreeMap<String, String>,
    /// Lowercased `double` / `float` fields of struct `_`.
    float_globals: std::collections::BTreeSet<String>,
    self_reg: Option<u8>,
    instance: bool,
    ret_bank: u8,
    /// Banks of the current function's extra results.
    extra_ret_banks: Vec<u8>,
    out_reg: Option<u8>,
    /// Native import name → compile-time id (`CallNat` ext, remapped at link).
    natives: BTreeMap<String, u32>,
    native_list: Vec<String>,
    class_name: String,
    jumps: Vec<Jump>,
    diags: Vec<Diagnostic>,
}

impl Lx {
    fn alloc(&mut self) -> u8 {
        let r = self.next;
        self.next = self.next.saturating_add(1);
        self.peak[0] = self.peak[0].max(self.next);
        r as u8
    }

    fn alloc_a(&mut self) -> u8 {
        let r = self.next_a;
        self.next_a = self.next_a.saturating_add(1);
        self.peak[3] = self.peak[3].max(self.next_a);
        r as u8
    }

    fn alloc_f(&mut self) -> u8 {
        let r = self.next_f;
        self.next_f = self.next_f.saturating_add(1);
        self.peak[1] = self.peak[1].max(self.next_f);
        r as u8
    }

    fn alloc_s(&mut self) -> u8 {
        let r = self.next_s;
        self.next_s = self.next_s.saturating_add(1);
        self.peak[2] = self.peak[2].max(self.next_s);
        r as u8
    }

    fn native_id(&mut self, name: &str) -> u32 {
        if let Some((_, &id)) = self
            .natives
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            return id;
        }
        let id = self.native_list.len() as u32;
        self.native_list.push(name.to_owned());
        self.natives.insert(name.to_owned(), id);
        id
    }

    fn err(&mut self, span: crate::span::Span, message: impl Into<String>) {
        self.diags
            .push(Diagnostic::error("lower.zs", span, message));
    }
}

/// Fold a constant expression that is only integers, names, and bitwise ops.
fn const_int(
    expr: &Expr,
    consts: &BTreeMap<String, i64>,
    parents: &BTreeMap<String, String>,
) -> Option<i64> {
    match expr {
        Expr::Int(v, _) => Some(*v),
        Expr::Ident(name, _) => consts.get(&name.to_ascii_lowercase()).copied(),
        Expr::Member { obj, field, .. } => {
            let Expr::Ident(cls, _) = obj.as_ref() else {
                return None;
            };
            lookup_int_const(consts, parents, cls, field)
        }
        Expr::Unary { op, expr, .. } if op == "-" => {
            const_int(expr, consts, parents).map(i64::wrapping_neg)
        }
        Expr::Unary { op, expr, .. } if op == "~" => const_int(expr, consts, parents).map(|v| !v),
        Expr::Binary { op, lhs, rhs, .. } => {
            let a = const_int(lhs, consts, parents)?;
            let b = const_int(rhs, consts, parents)?;
            Some(match op.as_str() {
                "+" => a.wrapping_add(b),
                "-" => a.wrapping_sub(b),
                "*" => a.wrapping_mul(b),
                "/" if b != 0 => a / b,
                "|" => a | b,
                "&" => a & b,
                "^" => a ^ b,
                "<<" => a.wrapping_shl(b as u32),
                ">>" => a.wrapping_shr(b as u32),
                _ => return None,
            })
        }
        _ => None,
    }
}

fn lookup_int_const(
    consts: &BTreeMap<String, i64>,
    parents: &BTreeMap<String, String>,
    cls: &str,
    field: &str,
) -> Option<i64> {
    let field = field.to_ascii_lowercase();
    let mut cur = Some(cls.to_ascii_lowercase());
    for _ in 0..64 {
        let name = cur?;
        if let Some(&v) = consts.get(&format!("{name}.{field}")) {
            return Some(v);
        }
        cur = parents.get(&name).filter(|p| !p.is_empty()).cloned();
    }
    None
}

fn lookup_float_const(
    consts: &BTreeMap<String, f64>,
    parents: &BTreeMap<String, String>,
    cls: &str,
    field: &str,
) -> Option<f64> {
    let field = field.to_ascii_lowercase();
    let mut cur = Some(cls.to_ascii_lowercase());
    for _ in 0..64 {
        let name = cur?;
        if let Some(&v) = consts.get(&format!("{name}.{field}")) {
            return Some(v);
        }
        cur = parents.get(&name).filter(|p| !p.is_empty()).cloned();
    }
    None
}

fn const_float(
    expr: &Expr,
    ints: &BTreeMap<String, i64>,
    floats: &BTreeMap<String, f64>,
    parents: &BTreeMap<String, String>,
) -> Option<f64> {
    match expr {
        Expr::Float(v, _) => Some(*v),
        Expr::Int(v, _) => Some(*v as f64),
        Expr::Ident(name, _) => {
            let key = name.to_ascii_lowercase();
            floats
                .get(&key)
                .copied()
                .or_else(|| ints.get(&key).map(|v| *v as f64))
        }
        Expr::Member { obj, field, .. } => {
            let Expr::Ident(cls, _) = obj.as_ref() else {
                return None;
            };
            let field_l = field.to_ascii_lowercase();
            let mut cur = Some(cls.to_ascii_lowercase());
            for _ in 0..64 {
                let name = cur?;
                let key = format!("{name}.{field_l}");
                if let Some(&v) = floats.get(&key) {
                    return Some(v);
                }
                if let Some(&v) = ints.get(&key) {
                    return Some(v as f64);
                }
                cur = parents.get(&name).filter(|p| !p.is_empty()).cloned();
            }
            None
        }
        Expr::Unary { op, expr, .. } if op == "+" => const_float(expr, ints, floats, parents),
        Expr::Unary { op, expr, .. } if op == "-" => {
            const_float(expr, ints, floats, parents).map(|v| -v)
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let a = const_float(lhs, ints, floats, parents)?;
            let b = const_float(rhs, ints, floats, parents)?;
            Some(match op.as_str() {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" if b != 0.0 => a / b,
                _ => return None,
            })
        }
        _ => None,
    }
}

fn record_const(
    name: &str,
    value: &Expr,
    consts: &mut BTreeMap<String, i64>,
    floats: &mut BTreeMap<String, f64>,
    strings: &mut BTreeMap<String, String>,
    parents: &BTreeMap<String, String>,
) {
    let key = name.to_ascii_lowercase();
    if let Some(v) = const_int(value, consts, parents) {
        consts.entry(key).or_insert(v);
        return;
    }
    if let Some(v) = const_float(value, consts, floats, parents) {
        floats.entry(key).or_insert(v);
        return;
    }
    if let Some(v) = const_string(value, strings) {
        strings.entry(key).or_insert(v);
    }
}

fn const_string(expr: &Expr, strings: &BTreeMap<String, String>) -> Option<String> {
    match expr {
        Expr::String(s, _) | Expr::Name(s, _) => Some(s.clone()),
        Expr::Ident(name, _) => strings.get(&name.to_ascii_lowercase()).cloned(),
        Expr::Binary { op, lhs, rhs, .. } if op == ".." || op == "+" => Some(format!(
            "{}{}",
            const_string(lhs, strings)?,
            const_string(rhs, strings)?
        )),
        _ => None,
    }
}

/// Record `Owner.variant` and `Enum.variant`. Bare names are recorded too;
/// `or_insert` keeps the first variant when two enums share a name.
fn note_enum(
    consts: &mut BTreeMap<String, i64>,
    parents: &BTreeMap<String, String>,
    owner: &str,
    en: &EnumDecl,
) {
    let mut next = 0i64;
    for (name, val) in &en.variants {
        let v = val
            .as_ref()
            .and_then(|e| const_int(e, consts, parents))
            .unwrap_or(next);
        next = v.wrapping_add(1);
        consts.entry(name.to_ascii_lowercase()).or_insert(v);
        consts
            .entry(format!("{}.{}", owner, name).to_ascii_lowercase())
            .or_insert(v);
        consts
            .entry(format!("{}.{}", en.name, name).to_ascii_lowercase())
            .or_insert(v);
    }
}

fn nested_structs<'a>(members: &'a [Member], out: &mut Vec<&'a StructDecl>) {
    for member in members {
        if let Member::NestedStruct(s) = member {
            out.push(s);
            nested_structs(&s.members, out);
        }
    }
}

/// Lower methods onto the call ABI in `docs/34` §14.3.3.
///
/// Covers static integer methods plus instance methods whose fields are
/// arrays (the probe's `ZC_IntStrMap`). Methods this pass does not understand
/// produce an error diagnostic.
pub fn lower(
    unit: &TranslationUnit,
    _analysis: &Analysis,
) -> Result<CompiledProgram, Vec<Diagnostic>> {
    let mut methods: Vec<(String, MethodDecl)> = Vec::new();
    let mut natives: Vec<(String, MethodDecl)> = Vec::new();
    let mut prog = CompiledProgram::default();
    let mut class_ids = BTreeMap::new();
    let mut array_fields: BTreeMap<String, Vec<(String, u16, bool)>> = BTreeMap::new();
    let mut int_fields: BTreeMap<String, BTreeMap<String, u16>> = BTreeMap::new();
    let mut string_fields: BTreeMap<String, BTreeMap<String, u16>> = BTreeMap::new();
    let mut float_fields: BTreeMap<String, BTreeMap<String, (u16, u8)>> = BTreeMap::new();
    let mut object_fields: BTreeMap<String, BTreeMap<String, (u16, String)>> = BTreeMap::new();
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    let mut type_names = std::collections::BTreeSet::new();
    let mut consts = BTreeMap::new();
    let mut float_consts: BTreeMap<String, f64> = BTreeMap::new();
    let mut string_consts: BTreeMap<String, String> = BTreeMap::new();
    let mut float_globals = std::collections::BTreeSet::new();
    consts.insert("gs_level".to_owned(), 0);
    consts.insert("int.min".to_owned(), i32::MIN as i64);
    consts.insert("int.max".to_owned(), i32::MAX as i64);
    consts.insert("uint.min".to_owned(), 0);
    consts.insert("uint.max".to_owned(), u32::MAX as i64);
    // GZDoom's fixed-point unit, used as `double.equal_epsilon`.
    let equal_epsilon = 1.0 / 65536.0;
    float_consts.insert("double.equal_epsilon".to_owned(), equal_epsilon);
    float_consts.insert("float.equal_epsilon".to_owned(), equal_epsilon);
    for decl in &unit.decls {
        match decl {
            Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => {
                type_names.insert(c.name.to_ascii_lowercase());
            }
            Decl::Struct(s) => {
                type_names.insert(s.name.to_ascii_lowercase());
            }
            Decl::Const(c) => record_const(
                &c.name,
                &c.value,
                &mut consts,
                &mut float_consts,
                &mut string_consts,
                &parents,
            ),
            Decl::Enum(en) => note_enum(&mut consts, &parents, &en.name, en),
        }
    }
    let mut nested = Vec::new();
    for decl in &unit.decls {
        match decl {
            Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => {
                nested_structs(&c.members, &mut nested);
            }
            Decl::Struct(s) => nested_structs(&s.members, &mut nested),
            _ => {}
        }
    }
    for s in &nested {
        type_names.insert(s.name.to_ascii_lowercase());
    }
    let mut layouts: Vec<(String, String, &[Member], bool)> = Vec::new();
    for decl in &unit.decls {
        match decl {
            Decl::Class(c) | Decl::Mixin(c) => layouts.push((
                c.name.clone(),
                c.parent.clone().unwrap_or_default(),
                &c.members,
                false,
            )),
            Decl::ExtendClass(c) => layouts.push((
                c.name.clone(),
                c.parent.clone().unwrap_or_default(),
                &c.members,
                true,
            )),
            Decl::Struct(s) => layouts.push((s.name.clone(), String::new(), &s.members, false)),
            _ => {}
        }
    }
    for s in &nested {
        layouts.push((s.name.clone(), String::new(), &s.members, false));
    }
    // A class's fields follow its parent's, so a subclass never reuses an
    // inherited slot. `extend class` fields are appended to the class.
    let mut merged_fields: BTreeMap<String, Vec<Member>> = BTreeMap::new();
    let mut layout_parent: BTreeMap<String, String> = BTreeMap::new();
    for (name, parent, members, is_extend) in &layouts {
        let key = name.to_ascii_lowercase();
        if !is_extend || !parent.is_empty() {
            layout_parent
                .entry(key.clone())
                .or_insert_with(|| parent.to_ascii_lowercase());
        }
        merged_fields.entry(key).or_default().extend(
            members
                .iter()
                .filter(|m| matches!(m, Member::Field(_) | Member::FlagDef { .. }))
                .cloned(),
        );
    }
    let mut field_base: BTreeMap<String, [u16; 4]> = BTreeMap::new();
    for key in merged_fields.keys() {
        layout_base(key, &merged_fields, &layout_parent, &type_names, &mut field_base, 0);
    }
    let mut laid_out: BTreeMap<String, u32> = BTreeMap::new();
    for (name, parent, members, is_extend) in layouts {
        let id = prog.classes.len() as u32;
        let name_l = name.to_ascii_lowercase();
        class_ids.entry(name_l.clone()).or_insert(id);
        parents
            .entry(name_l.clone())
            .or_insert_with(|| parent.to_ascii_lowercase());
        let (layout, arrays, objects, floats) = if let Some(&first) = laid_out.get(&name_l) {
            // Already laid out with every `extend class` field: reuse it.
            let _ = is_extend;
            let layout = prog.classes[first as usize].clone();
            (layout, Vec::new(), Vec::new(), Vec::new())
        } else {
            laid_out.insert(name_l.clone(), id);
            let start = layout_parent
                .get(&name_l)
                .and_then(|p| field_base.get(p).copied().map(|b| (p, b)))
                .map(|(p, base)| {
                    let own = own_field_counts(p, &merged_fields, &type_names);
                    [base[0] + own[0], base[1] + own[1], base[2] + own[2], base[3] + own[3]]
                })
                .unwrap_or([0; 4]);
            let fields = merged_fields.get(&name_l).cloned().unwrap_or_default();
            let parent_name = layout_parent.get(&name_l).cloned().unwrap_or(parent.clone());
            let mut out = layout_members(&name, &parent_name, &fields, &type_names, start);
            if out.0.parent.is_empty() {
                out.0.parent = parent.clone();
            }
            out
        };
        let floats_slot = float_fields.entry(name_l.clone()).or_default();
        for (fname, slot, dim) in floats {
            floats_slot.entry(fname.to_ascii_lowercase()).or_insert((slot, dim));
        }
        // `extend class` adds members. Keep fields already declared on the class.
        let arrays_slot = array_fields.entry(name_l.clone()).or_default();
        for item in arrays {
            if !arrays_slot
                .iter()
                .any(|(n, _, _)| n.eq_ignore_ascii_case(&item.0))
            {
                arrays_slot.push(item);
            }
        }
        let mut ints = BTreeMap::new();
        for (fname, (bank, slot)) in &layout.fields {
            if *bank == 0 {
                ints.insert(fname.clone(), *slot);
            }
        }
        let ints_slot = int_fields.entry(name_l.clone()).or_default();
        for (fname, slot) in ints {
            ints_slot.entry(fname).or_insert(slot);
        }
        let mut strs = BTreeMap::new();
        for (fname, (bank, slot)) in &layout.fields {
            if *bank == 2 {
                strs.insert(fname.clone(), *slot);
            }
        }
        let strs_slot = string_fields.entry(name_l.clone()).or_default();
        for (fname, slot) in strs {
            strs_slot.entry(fname).or_insert(slot);
        }
        for member in members {
            match member {
                Member::NestedConst(c) => {
                    record_const(
                        &c.name,
                        &c.value,
                        &mut consts,
                        &mut float_consts,
                        &mut string_consts,
                        &parents,
                    );
                    record_const(
                        &format!("{}.{}", name, c.name),
                        &c.value,
                        &mut consts,
                        &mut float_consts,
                        &mut string_consts,
                        &parents,
                    );
                }
                Member::NestedEnum(en) => note_enum(&mut consts, &parents, &name, en),
                Member::Field(field) if name_l == "_" => {
                    let TypeRef::Named(t) = &field.ty else {
                        continue;
                    };
                    if t.eq_ignore_ascii_case("double") || t.eq_ignore_ascii_case("float") {
                        for fname in &field.names {
                            float_globals.insert(fname.to_ascii_lowercase());
                        }
                    }
                }
                _ => {}
            }
        }
        let mut objs = BTreeMap::new();
        for (fname, slot, ty) in objects {
            objs.insert(fname.to_ascii_lowercase(), (slot, ty));
        }
        let objs_slot = object_fields.entry(name_l.clone()).or_default();
        for (fname, slot) in objs {
            objs_slot.entry(fname).or_insert(slot);
        }
        prog.classes.push(layout);
        for member in members {
            match member {
                Member::Method(meth) if meth.body.is_some() => {
                    methods.push((name.clone(), meth.clone()));
                }
                Member::Method(meth) => {
                    natives.push((name.clone(), meth.clone()));
                }
                Member::States(st) if !st.body.trim().is_empty() => {
                    prog.class_states.insert(name.clone(), st.body.clone());
                }
                Member::Default(d) if !d.lines.is_empty() => {
                    prog.class_defaults.insert(name.clone(), d.lines.join("\n"));
                }
                _ => {}
            }
        }
    }
    // A second pass folds constants that name a parent class's constant,
    // such as `Thinker.TICRATE` after `Object.TICRATE` is recorded.
    for decl in &unit.decls {
        let (name, members) = match decl {
            Decl::Class(c) | Decl::ExtendClass(c) | Decl::Mixin(c) => {
                (c.name.as_str(), c.members.as_slice())
            }
            Decl::Struct(s) => (s.name.as_str(), s.members.as_slice()),
            _ => continue,
        };
        for member in members {
            let Member::NestedConst(c) = member else {
                continue;
            };
            record_const(
                &c.name,
                &c.value,
                &mut consts,
                &mut float_consts,
                &mut string_consts,
                &parents,
            );
            record_const(
                &format!("{}.{}", name, c.name),
                &c.value,
                &mut consts,
                &mut float_consts,
                &mut string_consts,
                &parents,
            );
        }
    }
    // Pointers the engine provides on Actor without a ZScript field declaration.
    {
        let ptrs = [
            ("target", "actor"),
            ("tracer", "actor"),
            ("master", "actor"),
            ("lastenemy", "actor"),
            ("lastheard", "actor"),
            ("player", "playerinfo"),
        ];
        let objs = object_fields.entry("actor".to_owned()).or_default();
        for (i, (field, ty)) in ptrs.into_iter().enumerate() {
            objs.entry(field.to_owned())
                .or_insert((400 + i as u16, ty.to_owned()));
        }
    }
    // Untyped object locals still see Actor fields (`mo.health`, `mo.target`).
    parents
        .entry("object".to_owned())
        .or_insert_with(|| "actor".to_owned());
    let mut lx = Lx {
        locals: BTreeMap::new(),
        locals_s: BTreeMap::new(),
        locals_a: BTreeMap::new(),
        locals_o: BTreeMap::new(),
        locals_f: BTreeMap::new(),
        locals_v: BTreeMap::new(),
        local_types: BTreeMap::new(),
        value_fields: value_field_types(&merged_fields),
        next: 0,
        next_s: 0,
        next_a: 0,
        next_f: 0,
        peak: [0; 4],
        funcs: BTreeMap::new(),
        class_ids,
        array_fields,
        int_fields,
        string_fields,
        float_fields,
        object_fields,
        parents,
        consts,
        float_consts,
        string_consts,
        float_globals,
        self_reg: None,
        instance: false,
        ret_bank: 0,
        extra_ret_banks: Vec::new(),
        out_reg: None,
        natives: BTreeMap::new(),
        native_list: Vec::new(),
        class_name: String::new(),
        jumps: Vec::new(),
        diags: Vec::new(),
    };
    for (i, (class, meth)) in methods.iter().enumerate() {
        let instance = !meth.quals.static_;
        let mut params = Vec::new();
        if instance {
            params.push(3);
        }
        let mut outs = Vec::new();
        let vec_params: Vec<u8> = meth
            .params
            .iter()
            .map(|p| fvec::type_vector_dim(&p.ty).unwrap_or(0))
            .collect();
        let class_params: Vec<bool> = meth
            .params
            .iter()
            .map(|p| matches!(p.ty, TypeRef::ClassOf(_)))
            .collect();
        for (pi, p) in meth.params.iter().enumerate() {
            params.push(value_bank(&p.ty, &lx.class_ids));
            if p.mode
                .as_deref()
                .is_some_and(|m| m.to_ascii_lowercase().contains("out") || m.eq_ignore_ascii_case("ref"))
            {
                outs.push(pi);
            }
        }
        lx.funcs.insert(
            format!("{class}.{}", meth.name).to_ascii_lowercase(),
            FnSig {
                idx: i as u32,
                instance,
                params,
                ret: value_bank(&meth.ret, &lx.class_ids),
                outs,
                native: false,
                vec_params,
                class_params,
                defaults: meth.params.iter().map(|p| p.default.clone()).collect(),
                varargs: false,
                ret_class: p_type_name(&meth.ret).to_ascii_lowercase(),
                ret_banks: std::iter::once(&meth.ret)
                    .chain(meth.extra_rets.iter())
                    .map(|t| value_bank(t, &lx.class_ids))
                    .collect(),
            },
        );
    }
    for (class, meth) in &natives {
        let instance = !meth.quals.static_;
        let mut params = Vec::new();
        if instance {
            params.push(3);
        }
        let mut outs = Vec::new();
        let vec_params: Vec<u8> = meth
            .params
            .iter()
            .map(|p| fvec::type_vector_dim(&p.ty).unwrap_or(0))
            .collect();
        let class_params: Vec<bool> = meth
            .params
            .iter()
            .map(|p| matches!(p.ty, TypeRef::ClassOf(_)))
            .collect();
        for (pi, p) in meth.params.iter().enumerate() {
            params.push(value_bank(&p.ty, &lx.class_ids));
            if p.mode
                .as_deref()
                .is_some_and(|m| m.to_ascii_lowercase().contains("out") || m.eq_ignore_ascii_case("ref"))
            {
                outs.push(pi);
            }
        }
        lx.funcs
            .entry(format!("{class}.{}", meth.name).to_ascii_lowercase())
            .or_insert(FnSig {
                idx: 0,
                instance,
                params,
                ret: value_bank(&meth.ret, &lx.class_ids),
                outs,
                native: true,
                vec_params,
                class_params,
                defaults: meth.params.iter().map(|p| p.default.clone()).collect(),
                varargs: meth.quals.vararg,
                ret_class: p_type_name(&meth.ret).to_ascii_lowercase(),
                ret_banks: std::iter::once(&meth.ret)
                    .chain(meth.extra_rets.iter())
                    .map(|t| value_bank(t, &lx.class_ids))
                    .collect(),
            });
    }
    for (class, meth) in &methods {
        lx.locals.clear();
        lx.locals_s.clear();
        lx.locals_a.clear();
        lx.locals_o.clear();
        lx.locals_f.clear();
        lx.locals_v.clear();
        lx.local_types.clear();
        lx.next = 0;
        lx.next_s = 0;
        lx.next_a = 0;
        lx.next_f = 0;
        lx.peak = [0; 4];
        lx.jumps.clear();
        lx.self_reg = None;
        lx.class_name = class.clone();
        lx.instance = !meth.quals.static_;
        lx.ret_bank = value_bank(&meth.ret, &lx.class_ids);
        lx.extra_ret_banks = meth
            .extra_rets
            .iter()
            .map(|t| value_bank(t, &lx.class_ids))
            .collect();
        lx.out_reg = None;
        let mut f = CompiledFunction {
            name: format!("{class}.{}", meth.name),
            ..Default::default()
        };
        if lx.instance {
            let r = lx.alloc_a();
            lx.self_reg = Some(r);
            let class_l = class.to_ascii_lowercase();
            lx.locals_o.insert("self".to_owned(), (r, class_l.clone()));
            lx.locals_o.insert("invoker".to_owned(), (r, class_l));
            f.params.push(FuncParam {
                bank: 3,
                reg: u16::from(r),
            });
            let mut cur = Some(class.to_ascii_lowercase());
            for _ in 0..64 {
                let Some(name) = cur else { break };
                if let Some(fields) = lx.array_fields.get(&name).cloned() {
                    for (fname, slot, string_elem) in fields {
                        if lx.locals_a.keys().any(|k| k.eq_ignore_ascii_case(&fname)) {
                            continue;
                        }
                        let ar = lx.alloc_a();
                        f.emit_ext(Op::LoadFieldA, ar, r, 0, u32::from(slot));
                        lx.locals_a.insert(fname, (ar, string_elem));
                    }
                }
                cur = parent_of(&lx, &name);
            }
        }
        // Global objects live on struct `_` (`level`, `deh`, …).
        if let Some(fields) = lx.object_fields.get("_").cloned() {
            for (fname, (_slot, ty)) in fields {
                if lx.locals_o.keys().any(|k| k.eq_ignore_ascii_case(&fname)) {
                    continue;
                }
                let reg = lx.alloc_a();
                let id = lx.native_id(&format!("_.{fname}"));
                f.emit_ext(Op::CallNat, 0, 3, 0, id);
                f.emit(Op::GetRet, 3, reg, 0);
                lx.locals_o.insert(fname, (reg, ty));
            }
        }
        let globals = lx.float_globals.clone();
        for fname in globals {
            if lx.locals_f.keys().any(|k| k.eq_ignore_ascii_case(&fname)) {
                continue;
            }
            let reg = lx.alloc_f();
            let id = lx.native_id(&format!("_.{fname}"));
            f.emit_ext(Op::CallNat, 0, 1, 0, id);
            f.emit(Op::GetRet, 1, reg, 0);
            lx.locals_f.insert(fname, reg);
        }
        if let Some(fields) = lx.int_fields.get("_").cloned() {
            for fname in fields.into_keys() {
                if lx.locals.keys().any(|k| k.eq_ignore_ascii_case(&fname)) {
                    continue;
                }
                let reg = lx.alloc();
                let id = lx.native_id(&format!("_.{fname}"));
                f.emit_ext(Op::CallNat, 0, 0, 0, id);
                f.emit(Op::GetRet, 0, reg, 0);
                lx.locals.insert(fname, reg);
            }
        }
        // Action functions receive an implicit `stateinfo` of type `FStateParamInfo`.
        if !lx
            .locals_o
            .keys()
            .any(|k| k.eq_ignore_ascii_case("stateinfo"))
        {
            let reg = lx.alloc_a();
            f.emit(Op::LoadNull, reg, 0, 0);
            lx.locals_o
                .insert("stateinfo".to_owned(), (reg, "fstateparaminfo".to_owned()));
        }
        // Global arrays live on struct `_` (`playeringame`, `players`, …).
        if let Some(fields) = lx.array_fields.get("_").cloned() {
            for (fname, _slot, string_elem) in fields {
                if lx.locals_a.keys().any(|k| k.eq_ignore_ascii_case(&fname)) {
                    continue;
                }
                let ar = lx.alloc_a();
                let id = lx.native_id(&format!("_.{fname}"));
                f.emit_ext(Op::CallNat, 0, 3, 0, id);
                f.emit(Op::GetRet, 3, ar, 0);
                lx.locals_a.insert(fname, (ar, string_elem));
            }
        }
        for p in &meth.params {
            if let Some(string_elem) = array_elem_kind(&p.ty) {
                let r = lx.alloc_a();
                lx.locals_a.insert(p.name.clone(), (r, string_elem));
                f.params.push(FuncParam {
                    bank: 3,
                    reg: u16::from(r),
                });
                continue;
            }
            if let Some(dim) = fvec::type_vector_dim(&p.ty) {
                // A vector parameter arrives as its components, one float each.
                let base = alloc_fv(&mut lx, dim);
                lx.locals_v.insert(p.name.clone(), (base, dim));
                for c in 0..dim {
                    f.params.push(FuncParam {
                        bank: 1,
                        reg: u16::from(base + c),
                    });
                }
                continue;
            }
            match value_bank(&p.ty, &lx.class_ids) {
                2 => {
                    let r = lx.alloc_s();
                    lx.locals_s.insert(p.name.clone(), r);
                    f.params.push(FuncParam {
                        bank: 2,
                        reg: u16::from(r),
                    });
                }
                1 => {
                    let r = lx.alloc_f();
                    lx.locals_f.insert(p.name.clone(), r);
                    f.params.push(FuncParam {
                        bank: 1,
                        reg: u16::from(r),
                    });
                }
                3 => {
                    let r = lx.alloc_a();
                    lx.locals_o
                        .insert(p.name.clone(), (r, p_type_name(&p.ty).to_ascii_lowercase()));
                    f.params.push(FuncParam {
                        bank: 3,
                        reg: u16::from(r),
                    });
                }
                _ => {
                    let r = lx.alloc();
                    lx.locals.insert(p.name.clone(), r);
                    // `TextureID tex` parameters call `textureid.*` natives.
                    if let Some(t) = named_type(&p.ty)
                        .map(str::to_ascii_lowercase)
                        .filter(|t| VALUE_TYPES.contains(&t.as_str()))
                    {
                        lx.local_types.insert(p.name.to_ascii_lowercase(), t);
                    }
                    f.params.push(FuncParam {
                        bank: 0,
                        reg: u16::from(r),
                    });
                }
            }
        }
        lx.out_reg = meth.params.iter().find_map(|p| {
            let out = p
                .mode
                .as_deref()
                .is_some_and(|m| m.to_ascii_lowercase().contains("out") || m.eq_ignore_ascii_case("ref"));
            if out && value_bank(&p.ty, &lx.class_ids) == 0 {
                lx.locals.get(&p.name).copied()
            } else {
                None
            }
        });
        if let Some(body) = &meth.body {
            emit_block(&mut f, body, &mut lx);
        }
        let out_reg = lx.out_reg;
        if let Some(reg) = out_reg {
            f.emit(Op::Ret, 0, reg, 0);
        } else {
            f.emit(Op::Ret, 0, 0, 0);
        }
        let most = lx.peak.iter().copied().max().unwrap_or(0);
        if most > 255 {
            let span = meth.span;
            lx.err(
                span,
                format!("{}: needs {most} registers in one bank (limit 255)", f.name),
            );
        }
        f.n_i = lx.peak[0].clamp(1, 255) as u8;
        f.n_f = lx.peak[1].clamp(1, 255) as u8;
        f.n_s = lx.peak[2].clamp(1, 255) as u8;
        f.n_a = lx.peak[3].clamp(1, 255) as u8;
        prog.functions.push(f);
    }
    for name in &lx.native_list {
        prog.intern_native(name);
    }
    if lx.diags.is_empty() {
        prog.finalize();
        Ok(prog)
    } else {
        Err(lx.diags)
    }
}

fn call_returns_string(callee: &Expr, lx: &Lx) -> bool {
    if let Some((_, _, ret)) = string_native(callee, lx) {
        return ret == 2;
    }
    if let Some(target) = resolve_call(callee, lx) {
        if lx.funcs.get(&target.key).is_some_and(|sig| sig.ret == 2) {
            return true;
        }
    }
    // `CPlayer.weapons.GetWeapon(…)`: a method on a typed field chain.
    if let Expr::Member { obj, field, .. } = callee {
        if let Some(class) = fvec::static_class(obj, lx) {
            if let Some(t) = lookup_method(lx, &class, field, None) {
                return lx.funcs.get(&t.key).is_some_and(|sig| sig.ret == 2);
            }
        }
    }
    false
}

/// Untyped `let s = ChooseSpawn()` / `let text = String.Format(...)`.
fn expr_yields_string(expr: &Expr, lx: &Lx) -> bool {
    match expr {
        Expr::String(_, _) | Expr::Name(_, _) => true,
        Expr::Ident(name, _) => {
            lx.string_consts.contains_key(&name.to_ascii_lowercase())
                || lx.locals_s.keys().any(|k| k.eq_ignore_ascii_case(name))
        }
        Expr::Binary { op, lhs, rhs, .. } if op == ".." || op == "+" => {
            expr_yields_string(lhs, lx) || expr_yields_string(rhs, lx)
        }
        Expr::Call { callee, .. } => {
            call_returns_string(callee, lx)
                || matches!(callee.as_ref(),
                    Expr::Member { field, .. } | Expr::Ident(field, _)
                        if field.eq_ignore_ascii_case("GetClass"))
        }
        // `(class<Ammo>)(type)`: the parser keeps only `Ammo`; a class value
        // stays a name.
        Expr::Cast { expr, .. } => expr_yields_string(expr, lx),
        Expr::Ternary { then_e, else_e, .. } => {
            expr_yields_string(then_e, lx) || expr_yields_string(else_e, lx)
        }
        // `orderedammos[i]` of a string / name / class array.
        Expr::Index { arr, .. } => match arr.as_ref() {
            Expr::Ident(name, _) => array_local(lx, name).is_some_and(|(_, s)| s),
            _ => false,
        },
        // `weapdef.AmmoType1`: a string / name / class field of a typed object.
        Expr::Member { obj, field, .. } => fvec::static_class(obj, lx)
            .is_some_and(|class| lookup_string_field(lx, &class, field).is_some()),
        _ => false,
    }
}

/// Value struct types `let` can infer.
const VALUE_TYPES: [&str; 5] = ["textureid", "spriteid", "translationid", "sound", "color"];

/// Field name → value struct type, for names every declaration agrees on.
fn value_field_types(merged: &BTreeMap<String, Vec<Member>>) -> BTreeMap<String, String> {
    let mut seen: BTreeMap<String, Option<String>> = BTreeMap::new();
    for members in merged.values() {
        for m in members {
            let Member::Field(field) = m else {
                continue;
            };
            let ty = named_type(&field.ty)
                .map(str::to_ascii_lowercase)
                .filter(|t| VALUE_TYPES.contains(&t.as_str()));
            for name in &field.names {
                let key = name.to_ascii_lowercase();
                match seen.get(&key) {
                    None => {
                        seen.insert(key, ty.clone());
                    }
                    Some(prev) if *prev != ty => {
                        seen.insert(key, None);
                    }
                    _ => {}
                }
            }
        }
    }
    seen.into_iter().filter_map(|(k, v)| v.map(|t| (k, t))).collect()
}

/// The value struct type of `expr` (`inv.Icon`, a `TextureID` local, a
/// ternary of those), if known.
fn value_type_of(expr: &Expr, lx: &Lx) -> Option<String> {
    match expr {
        Expr::Ident(name, _) => lx.local_types.get(&name.to_ascii_lowercase()).cloned(),
        Expr::Member { field, .. } => lx.value_fields.get(&field.to_ascii_lowercase()).cloned(),
        Expr::Ternary { then_e, else_e, .. } => {
            value_type_of(then_e, lx).or_else(|| value_type_of(else_e, lx))
        }
        _ => None,
    }
}

fn emit_block(f: &mut CompiledFunction, block: &Block, lx: &mut Lx) {
    for stmt in &block.stmts {
        emit_stmt_scoped(f, stmt, lx);
    }
}

/// [`emit_stmt`], then free the statement's temporaries: when it declared no
/// local (here or in a nested block), its registers are free again.
fn emit_stmt_scoped(f: &mut CompiledFunction, stmt: &Stmt, lx: &mut Lx) {
    let mark = (lx.next, lx.next_f, lx.next_s, lx.next_a);
    let declared = local_count(lx);
    emit_stmt(f, stmt, lx);
    if !matches!(stmt, Stmt::Local { .. }) && local_count(lx) == declared {
        (lx.next, lx.next_f, lx.next_s, lx.next_a) = mark;
    }
}

fn local_count(lx: &Lx) -> usize {
    lx.locals.len()
        + lx.locals_s.len()
        + lx.locals_a.len()
        + lx.locals_o.len()
        + lx.locals_f.len()
        + lx.locals_v.len()
}

fn emit_stmt(f: &mut CompiledFunction, stmt: &Stmt, lx: &mut Lx) {
    match stmt {
        Stmt::Local {
            ty: None,
            names,
            init: Some(init @ Expr::Call { callee, .. }),
            extra_inits,
            ..
        } if names.len() > 1 && extra_inits.is_empty() => {
            emit_destructure(f, lx, names, init, callee);
        }
        Stmt::Local {
            ty,
            names,
            init,
            extra_inits,
            span: _,
        } => {
            let type_name = ty.as_ref().and_then(named_type);
            let string_local = ty.as_ref().is_some_and(is_class_type)
                || type_name.is_some_and(|t| {
                    t.eq_ignore_ascii_case("string") || t.eq_ignore_ascii_case("name")
                })
                || (ty.is_none()
                && init.as_ref().is_some_and(|e| expr_yields_string(e, lx)));
            let float_local = type_name.is_some_and(is_float_type_name)
                || (ty.is_none() && init.as_ref().is_some_and(|e| expr_is_float(e, lx)));
            let class_local = if let Some(t) = type_name {
                if lx.class_ids.contains_key(&t.to_ascii_lowercase()) {
                    Some(t.to_ascii_lowercase())
                } else {
                    None
                }
            } else if ty.is_none() {
                init.as_ref().and_then(|e| init_object_class(e, lx))
            } else {
                None
            };
            let array_elems = ty.as_ref().and_then(array_elem_kind);
            let vec_dim = match ty {
                Some(t) => fvec::type_vector_dim(t),
                None => init.as_ref().and_then(|e| expr_vector_dim(e, lx)),
            };
            for (k, name) in names.iter().enumerate() {
                let name_init = if k == 0 {
                    init.as_ref()
                } else {
                    extra_inits.get(k - 1).and_then(|e| e.as_ref())
                };
                if let Some(dim) = vec_dim {
                    let base = alloc_fv(lx, dim);
                    match name_init {
                        Some(init) => {
                            let v = emit_expr_v(f, lx, init, dim);
                            for c in 0..dim {
                                f.emit(Op::MoveF, base + c, v + c, 0);
                            }
                        }
                        None => {
                            for c in 0..dim {
                                let z = load_f(f, lx, 0.0);
                                f.emit(Op::MoveF, base + c, z, 0);
                            }
                        }
                    }
                    lx.locals_v.insert(name.clone(), (base, dim));
                } else if let Some(string_elem) = array_elems {
                    let r = lx.alloc_a();
                    lx.locals_a.insert(name.clone(), (r, string_elem));
                    let fixed = name_init.and_then(|e| const_int(e, &lx.consts, &lx.parents));
                    if let Some(n) = fixed {
                        // Fixed-size array: `n` zeroed elements.
                        emit_new_array(f, lx, string_elem, r);
                        let zero = if string_elem {
                            load_s(f, lx, "")
                        } else {
                            load_i(f, lx, 0)
                        };
                        let op = if string_elem { Op::ArrayPushS } else { Op::ArrayPushI };
                        let n = n.clamp(0, 1 << 16);
                        if n <= 16 {
                            for _ in 0..n {
                                f.emit(op, r, zero, 0);
                            }
                        } else {
                            // counter = n; while counter { push; counter-- }
                            let counter = load_i(f, lx, n);
                            let one = load_i(f, lx, 1);
                            let start = f.code.len();
                            let jend = emit_jcc(f, Op::JmpIfNot, counter);
                            f.emit(op, r, zero, 0);
                            f.emit(Op::SubI, counter, counter, one);
                            let back = emit_jcc(f, Op::Jmp, 0);
                            patch_to(f, back, start);
                            patch(f, jend);
                        }
                    } else if k == 0 && init.is_none() {
                        emit_new_array(f, lx, string_elem, r);
                    }
                } else if float_local {
                    let r = lx.alloc_f();
                    lx.locals_f.insert(name.clone(), r);
                    if let Some(init) = name_init {
                        let v = emit_expr_f(f, init, lx);
                        if v != r {
                            f.emit(Op::MoveF, r, v, 0);
                        }
                    } else {
                        // Registers are reused between statements: start at 0.
                        let z = load_f(f, lx, 0.0);
                        f.emit(Op::MoveF, r, z, 0);
                    }
                } else if string_local {
                    let r = lx.alloc_s();
                    lx.locals_s.insert(name.clone(), r);
                    if let Some(init) = name_init {
                        let v = emit_expr_s(f, init, lx);
                        if v != r {
                            f.emit(Op::MoveS, r, v, 0);
                        }
                    } else {
                        let z = load_s(f, lx, "");
                        f.emit(Op::MoveS, r, z, 0);
                    }
                } else if let Some(class) = class_local.clone() {
                    let r = lx.alloc_a();
                    lx.locals_o.insert(name.clone(), (r, class));
                    if let Some(init) = name_init {
                        let v = emit_expr_a(f, init, lx);
                        if v != r {
                            f.emit(Op::MoveA, r, v, 0);
                        }
                    } else {
                        f.emit(Op::LoadNull, r, 0, 0);
                    }
                } else {
                    let r = lx.alloc();
                    lx.locals.insert(name.clone(), r);
                    // `let AltIcon = inv.AltHUDIcon`: a value struct by inference.
                    let inferred = if ty.is_none() {
                        name_init.and_then(|e| value_type_of(e, lx))
                    } else {
                        None
                    };
                    if let Some(t) = inferred {
                        lx.local_types.insert(name.to_ascii_lowercase(), t);
                    }
                    if let Some(t) = type_name {
                        if matches!(
                            t.to_ascii_lowercase().as_str(),
                            "textureid" | "spriteid" | "translationid" | "sound" | "color"
                        ) {
                            lx.local_types
                                .insert(name.to_ascii_lowercase(), t.to_ascii_lowercase());
                        }
                    }
                    if let Some(init) = name_init {
                        let v = emit_expr(f, init, lx);
                        if v != r {
                            f.emit(Op::MoveI, r, v, 0);
                        }
                    } else {
                        let z = load_i(f, lx, 0);
                        f.emit(Op::MoveI, r, z, 0);
                    }
                }
            }
        }
        Stmt::Expr { expr, .. } => {
            let _ = emit_expr(f, expr, lx);
        }
        Stmt::Return { values, .. } => {
            let (bank, r) = if lx.ret_bank == 3 {
                (3, values.first().map(|e| emit_expr_a(f, e, lx)).unwrap_or(0))
            } else if lx.ret_bank == 1 {
                (1, values.first().map(|e| emit_expr_f(f, e, lx)).unwrap_or(0))
            } else if lx.ret_bank == 2 {
                (2, values.first().map(|e| emit_expr_s(f, e, lx)).unwrap_or(0))
            } else {
                let r = values
                    .first()
                    .map(|e| emit_expr(f, e, lx))
                    .unwrap_or(lx.out_reg.unwrap_or(0));
                (0, r)
            };
            // `return a, b;`: extra results go to `rets[1..]` first.
            let extras: Vec<(u8, u8)> = values
                .iter()
                .skip(1)
                .enumerate()
                .map(|(k, e)| {
                    let bank = lx.extra_ret_banks.get(k).copied().unwrap_or(0);
                    let reg = match bank {
                        1 => emit_expr_f(f, e, lx),
                        2 => emit_expr_s(f, e, lx),
                        3 => emit_expr_a(f, e, lx),
                        _ => emit_expr(f, e, lx),
                    };
                    (bank, reg)
                })
                .collect();
            for (k, (b, reg)) in extras.into_iter().enumerate() {
                f.emit(Op::SetRet, b, reg, u8::try_from(k + 1).unwrap_or(7));
            }
            f.emit(Op::Ret, bank, r, 0);
        }
        Stmt::If {
            cond,
            then_b,
            else_b,
            ..
        } => {
            let c = emit_expr(f, cond, lx);
            let jelse = emit_jcc(f, Op::JmpIfNot, c);
            emit_block(f, then_b, lx);
            if let Some(else_b) = else_b {
                let jend = emit_jcc(f, Op::Jmp, 0);
                patch(f, jelse);
                emit_block(f, else_b, lx);
                patch(f, jend);
            } else {
                patch(f, jelse);
            }
        }
        Stmt::While { cond, body, .. } => {
            let start = f.code.len();
            lx.jumps.push(Jump {
                breaks: Vec::new(),
                continues: Vec::new(),
                is_loop: true,
            });
            let c = emit_expr(f, cond, lx);
            let jend = emit_jcc(f, Op::JmpIfNot, c);
            emit_block(f, body, lx);
            let back = emit_jcc(f, Op::Jmp, 0);
            patch_to(f, back, start);
            let end = f.code.len();
            patch(f, jend);
            end_loop(f, lx, end, start);
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
            ..
        } => {
            if let Some(init) = init {
                emit_stmt(f, init, lx);
            }
            let start = f.code.len();
            lx.jumps.push(Jump {
                breaks: Vec::new(),
                continues: Vec::new(),
                is_loop: true,
            });
            let jend = if let Some(cond) = cond {
                let c = emit_expr(f, cond, lx);
                Some(emit_jcc(f, Op::JmpIfNot, c))
            } else {
                None
            };
            emit_block(f, body, lx);
            let step_pc = f.code.len();
            if let Some(step) = step {
                let _ = emit_expr(f, step, lx);
            }
            let back = emit_jcc(f, Op::Jmp, 0);
            patch_to(f, back, start);
            let end = f.code.len();
            if let Some(jend) = jend {
                patch(f, jend);
            }
            end_loop(f, lx, end, step_pc);
        }
        Stmt::DoWhile {
            body, cond, until, ..
        } => {
            lx.jumps.push(Jump {
                breaks: Vec::new(),
                continues: Vec::new(),
                is_loop: true,
            });
            let start = f.code.len();
            emit_block(f, body, lx);
            let cond_pc = f.code.len();
            let c = emit_expr(f, cond, lx);
            if *until {
                let jend = emit_jcc(f, Op::JmpIf, c);
                let back = emit_jcc(f, Op::Jmp, 0);
                patch_to(f, back, start);
                patch(f, jend);
            } else {
                let back = emit_jcc(f, Op::JmpIf, c);
                patch_to(f, back, start);
            }
            let end = f.code.len();
            end_loop(f, lx, end, cond_pc);
        }
        Stmt::Switch { cond, body, .. } => emit_switch(f, cond, body, lx),
        Stmt::Case { .. } => {}
        Stmt::Block(block) => emit_block(f, block, lx),
        Stmt::Break(span) => {
            if let Some(frame) = lx.jumps.last_mut() {
                frame.breaks.push(emit_jcc(f, Op::Jmp, 0));
            } else {
                lx.err(*span, "break outside loop");
            }
        }
        Stmt::Continue(span) => {
            if let Some(frame) = lx.jumps.iter_mut().rev().find(|j| j.is_loop) {
                frame.continues.push(emit_jcc(f, Op::Jmp, 0));
            } else {
                lx.err(*span, "continue outside loop");
            }
        }
    }
}

fn emit_expr(f: &mut CompiledFunction, expr: &Expr, lx: &mut Lx) -> u8 {
    if matches!(expr, Expr::Ident(..) | Expr::Member { .. } | Expr::Call { .. }) {
        if let Some(r) = try_emit_float_read(f, lx, expr) {
            return f2i(f, lx, r);
        }
        if matches!(expr, Expr::Ident(..) | Expr::Member { .. }) {
            if let Some(dim) = expr_vector_dim(expr, lx) {
                // A vector in an int context: its first component.
                let v = emit_expr_v(f, lx, expr, dim);
                return f2i(f, lx, v);
            }
        }
    }
    match expr {
        Expr::Int(v, _) => load_i(f, lx, *v),
        Expr::Ident(name, span) => {
            if name.eq_ignore_ascii_case("true") {
                return load_i(f, lx, 1);
            }
            if name.eq_ignore_ascii_case("false") || name.eq_ignore_ascii_case("null") {
                return load_i(f, lx, 0);
            }
            // Locals and parameters shadow constants and class names (`int state`).
            if let Some(r) = int_local(lx, name) {
                return r;
            }
            if let Some((reg, _)) = object_local(lx, name) {
                return truthy(f, lx, reg);
            }
            if let Some(native) = host_int(name) {
                return emit_host_int(f, lx, native);
            }
            if let Some(v) = scoped_const(lx, name) {
                return load_i(f, lx, v);
            }
            if let Some(&v) = lx.consts.get(&name.to_ascii_lowercase()) {
                return load_i(f, lx, v);
            }
            if let Some(&class_id) = lx.class_ids.get(&name.to_ascii_lowercase()) {
                return load_i(f, lx, i64::from(class_id));
            }
            if let Some(s) = lx.string_consts.get(&name.to_ascii_lowercase()).cloned() {
                let reg = load_s(f, lx, &s);
                return string_truthy(f, lx, reg);
            }
            if let Some(&v) = lx.float_consts.get(&name.to_ascii_lowercase()) {
                let loaded = load_f(f, lx, v);
                return f2i(f, lx, loaded);
            }
            if let Some(r) = int_local(lx, name) {
                return r;
            }
            if let Some(&r) = lx
                .locals_f
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, r)| r)
            {
                return f2i(f, lx, r);
            }
            if let Some((reg, _)) = object_local(lx, name) {
                return truthy(f, lx, reg);
            }
            if let Some(&r) = lx
                .locals_s
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, r)| r)
            {
                return string_truthy(f, lx, r);
            }
            if let (Some(slot), Some(self_reg)) = (self_int_slot(lx, name), lx.self_reg) {
                let dest = lx.alloc();
                f.emit_ext(Op::LoadFieldI, dest, self_reg, 0, u32::from(slot));
                return dest;
            }
            if let (Some(slot), Some(self_reg)) = (self_object_slot(lx, name), lx.self_reg) {
                let dest = lx.alloc_a();
                f.emit_ext(Op::LoadFieldA, dest, self_reg, 0, u32::from(slot));
                return truthy(f, lx, dest);
            }
            if let (Some(slot), Some(self_reg)) = (
                lookup_string_field(lx, &lx.class_name, name)
                    .or_else(|| declaring_string_slot(lx, name)),
                lx.self_reg,
            ) {
                let s = lx.alloc_s();
                f.emit_ext(Op::LoadFieldS, s, self_reg, 0, u32::from(slot));
                return string_truthy(f, lx, s);
            }
            if let Some(dest) = emit_bflag(f, lx, name, None) {
                return dest;
            }
            if is_cvar_name(name) {
                return emit_host_int(f, lx, &name.to_ascii_lowercase());
            }
            if let Some(n) = crate::special_number(name) {
                return load_i(f, lx, i64::from(n));
            }
            if let Some((reg, _)) = array_local(lx, name) {
                return truthy(f, lx, reg);
            }
            if lookup_method(lx, &lx.class_name, name, lx.self_reg).is_some() {
                return load_i(f, lx, 0);
            }
            lx.err(*span, format!("unknown name {name}"));
            load_i(f, lx, 0)
        }
        Expr::Unary { op, expr, span } => match op.as_str() {
            "-" => {
                let r = emit_expr(f, expr, lx);
                let z = load_i(f, lx, 0);
                let d = lx.alloc();
                f.emit(Op::SubI, d, z, r);
                d
            }
            "~" => {
                let r = emit_expr(f, expr, lx);
                let mask = load_i(f, lx, -1);
                let d = lx.alloc();
                f.emit(Op::BitXor, d, r, mask);
                d
            }
            "!" => {
                if let Expr::Ident(name, _) = expr.as_ref() {
                    if let Some((reg, _)) = object_local(lx, name) {
                        let d = lx.alloc();
                        f.emit(Op::IsNull, d, reg, 0);
                        return d;
                    }
                }
                let r = emit_expr(f, expr, lx);
                let z = load_i(f, lx, 0);
                let d = lx.alloc();
                f.emit(Op::EqI, d, r, z);
                d
            }
            "++" | "--" | "post++" | "post--" => {
                emit_inc(f, expr, lx, op.ends_with("--"), op.starts_with("post"))
            }
            "+" => emit_expr(f, expr, lx),
            _ => {
                lx.err(*span, format!("unary {op}"));
                0
            }
        },
        Expr::Binary { op, lhs, rhs, span } => emit_binary(f, lx, op, lhs, rhs, *span),
        Expr::Ternary {
            cond,
            then_e,
            else_e,
            ..
        } => {
            let dest = lx.alloc();
            let c = emit_expr(f, cond, lx);
            let jelse = emit_jcc(f, Op::JmpIfNot, c);
            let t = emit_expr(f, then_e, lx);
            f.emit(Op::MoveI, dest, t, 0);
            let jend = emit_jcc(f, Op::Jmp, 0);
            patch(f, jelse);
            let e = emit_expr(f, else_e, lx);
            f.emit(Op::MoveI, dest, e, 0);
            patch(f, jend);
            dest
        }
        Expr::Member { obj, field, span } => emit_member_i(f, obj, field, lx, *span),
        Expr::Index { arr, index, span } => emit_index_i(f, arr, index, lx, *span),
        Expr::Call {
            callee,
            args,
            span: _,
        } => {
            if let Some(native) = array_method(callee, args, lx) {
                return emit_array_call(f, lx, callee, args, native);
            }
            if let Some((native, instance, ret_bank)) = string_native(callee, lx) {
                let recv = instance.then(|| match callee.as_ref() {
                    Expr::Member { obj, .. } => obj.as_ref(),
                    other => other,
                });
                return emit_native(f, lx, &native, recv, args, ret_bank, None);
            }
            if let Some(dest) = emit_new_expr(f, lx, callee, args) {
                return dest;
            }
            if let Some(class) = cast_target(callee, lx) {
                let obj = emit_cast(f, lx, &class, args.first());
                return truthy(f, lx, obj);
            }
            if let Some(dest) = emit_script_call(f, lx, callee, args, 0) {
                return dest;
            }
            if let Some(dest) = emit_actor_native(f, lx, callee, args, 0) {
                return dest;
            }
            if let Some(dest) = emit_builtin(f, lx, callee, args, 0) {
                return dest;
            }
            if let Some(dest) = emit_field_method(f, lx, callee, args, 0) {
                return dest;
            }
            if let Some(dest) = emit_type_call(f, lx, callee, args, 0) {
                return dest;
            }
            if let Some(name) = rng_call_name(callee) {
                let value = emit_math_call(f, lx, &name, args);
                return f2i(f, lx, value);
            }
            return emit_unresolved_call(f, lx, callee, args, 0);
        }
        Expr::Float(v, _) => load_i(f, lx, *v as i64),
        Expr::String(s, _) | Expr::Name(s, _) => load_i(f, lx, i64::from(!s.is_empty())),
        Expr::Cast { expr, .. } => emit_expr(f, expr, lx),
        Expr::New { class, .. } => {
            let obj = emit_new(f, lx, class);
            truthy(f, lx, obj)
        }
        Expr::Tuple { elems, .. } => elems.first().map(|e| emit_expr(f, e, lx)).unwrap_or(0),
    }
}

/// `String.Format` vararg layout for one argument in `bank` (see
/// `prepare_sig_args`).
fn emit_format_layout(f: &mut CompiledFunction, lx: &mut Lx, bank: u8) {
    let code = load_i(f, lx, i64::from(bank));
    let n = load_i(f, lx, 1);
    f.emit(Op::Arg, 0, code, 0);
    f.emit(Op::Arg, 0, n, 0);
}

fn format_int(f: &mut CompiledFunction, lx: &mut Lx, reg: u8) -> u8 {
    let fmt = load_s(f, lx, "%i");
    let id = lx.native_id("String.Format");
    f.emit(Op::Arg, 2, fmt, 0);
    f.emit(Op::Arg, 0, reg, 0);
    emit_format_layout(f, lx, 0);
    f.emit_ext(Op::CallNat, 1, 2, 0, id);
    let dest = lx.alloc_s();
    f.emit(Op::GetRet, 2, dest, 0);
    dest
}

fn format_float(f: &mut CompiledFunction, lx: &mut Lx, reg: u8) -> u8 {
    let fmt = load_s(f, lx, "%f");
    let id = lx.native_id("String.Format");
    f.emit(Op::Arg, 2, fmt, 0);
    f.emit(Op::Arg, 1, reg, 0);
    emit_format_layout(f, lx, 1);
    f.emit_ext(Op::CallNat, 1, 2, 0, id);
    let dest = lx.alloc_s();
    f.emit(Op::GetRet, 2, dest, 0);
    dest
}

fn emit_expr_s(f: &mut CompiledFunction, expr: &Expr, lx: &mut Lx) -> u8 {
    match expr {
        Expr::String(s, _) | Expr::Name(s, _) => load_s(f, lx, s),
        Expr::Binary { op, lhs, rhs, .. } if op == ".." || op == "+" => {
            let l = emit_expr_s(f, lhs, lx);
            let r = emit_expr_s(f, rhs, lx);
            let dest = lx.alloc_s();
            f.emit(Op::ConcatS, dest, l, r);
            dest
        }
        Expr::Member { obj, field, span } => {
            if let Expr::Ident(cls, _) = obj.as_ref() {
                let qualified = format!("{cls}.{field}").to_ascii_lowercase();
                if let Some(s) = lx
                    .string_consts
                    .get(&qualified)
                    .cloned()
                    .or_else(|| lx.string_consts.get(&field.to_ascii_lowercase()).cloned())
                {
                    return load_s(f, lx, &s);
                }
                if let Some(n) = lookup_int_const(&lx.consts, &lx.parents, cls, field) {
                    let v = load_i(f, lx, n);
                    return format_int(f, lx, v);
                }
            }
            if let Some((reg, class)) = object_base(f, obj, lx) {
                if let Some(slot) = lookup_string_field(lx, &class, field)
                    .or_else(|| declaring_string_slot(lx, field))
                {
                    let dest = lx.alloc_s();
                    f.emit_ext(Op::LoadFieldS, dest, reg, 0, u32::from(slot));
                    return dest;
                }
                if let Some(slot) =
                    lookup_int_field(lx, &class, field).or_else(|| declaring_int_slot(lx, field))
                {
                    let cur = lx.alloc();
                    f.emit_ext(Op::LoadFieldI, cur, reg, 0, u32::from(slot));
                    let fmt = load_s(f, lx, "%i");
                    let id = lx.native_id("String.Format");
                    f.emit(Op::Arg, 2, fmt, 0);
                    f.emit(Op::Arg, 0, cur, 0);
                    emit_format_layout(f, lx, 0);
                    f.emit_ext(Op::CallNat, 1, 2, 0, id);
                    let dest = lx.alloc_s();
                    f.emit(Op::GetRet, 2, dest, 0);
                    return dest;
                }
            }
            lx.err(*span, "string expression");
            load_s(f, lx, "")
        }
        Expr::Ident(name, span) => {
            if name.eq_ignore_ascii_case("null") || name.eq_ignore_ascii_case("nullptr") {
                return load_s(f, lx, "");
            }
            if let Some(s) = lx.string_consts.get(&name.to_ascii_lowercase()).cloned() {
                return load_s(f, lx, &s);
            }
            if lx.class_ids.contains_key(&name.to_ascii_lowercase())
                && int_local(lx, name).is_none()
                && lx.locals_s.keys().all(|k| !k.eq_ignore_ascii_case(name))
            {
                return load_s(f, lx, name);
            }
            if let Some(&r) = lx
                .locals_s
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, r)| r)
            {
                r
            } else if let (Some(slot), Some(self_reg)) = (
                lookup_string_field(lx, &lx.class_name, name)
                    .or_else(|| declaring_string_slot(lx, name)),
                lx.self_reg,
            ) {
                let dest = lx.alloc_s();
                f.emit_ext(Op::LoadFieldS, dest, self_reg, 0, u32::from(slot));
                dest
            } else if let Some(r) = int_local(lx, name) {
                format_int(f, lx, r)
            } else if let (Some(slot), Some(self_reg)) = (self_int_slot(lx, name), lx.self_reg) {
                let cur = lx.alloc();
                f.emit_ext(Op::LoadFieldI, cur, self_reg, 0, u32::from(slot));
                format_int(f, lx, cur)
            } else if let Some(r) = lx
                .locals_f
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, r)| *r)
            {
                format_float(f, lx, r)
            } else if object_local(lx, name).is_some() {
                // An untyped `let` guessed as an object (a call result whose
                // type the lowerer does not know): no string value.
                load_s(f, lx, "")
            } else {
                lx.err(*span, format!("unknown string {name}"));
                load_s(f, lx, "")
            }
        }
        Expr::Index { arr, index, span } => emit_index_s(f, arr, index, lx, *span),
        Expr::Ternary {
            cond,
            then_e,
            else_e,
            ..
        } => {
            let dest = lx.alloc_s();
            let c = emit_expr(f, cond, lx);
            let jelse = emit_jcc(f, Op::JmpIfNot, c);
            let t = emit_expr_s(f, then_e, lx);
            f.emit(Op::MoveS, dest, t, 0);
            let jend = emit_jcc(f, Op::Jmp, 0);
            patch(f, jelse);
            let e = emit_expr_s(f, else_e, lx);
            f.emit(Op::MoveS, dest, e, 0);
            patch(f, jend);
            dest
        }
        Expr::Call { callee, args, span } => {
            if let Some((native, instance, ret_bank)) = string_native(callee, lx) {
                if ret_bank != 2 {
                    lx.err(*span, format!("{native} does not return a string"));
                    return load_s(f, lx, "");
                }
                let recv = instance.then(|| match callee.as_ref() {
                    Expr::Member { obj, .. } => obj.as_ref(),
                    other => other,
                });
                return emit_native(f, lx, &native, recv, args, ret_bank, None);
            }
            if let Some(dest) = emit_script_call(f, lx, callee, args, 2) {
                return dest;
            }
            if let Some(dest) = emit_actor_native(f, lx, callee, args, 2) {
                return dest;
            }
            if let Some(dest) = emit_builtin(f, lx, callee, args, 2) {
                return dest;
            }
            if let Some(dest) = emit_field_method(f, lx, callee, args, 2) {
                return dest;
            }
            if let Some(dest) = emit_type_call(f, lx, callee, args, 2) {
                return dest;
            }
            emit_unresolved_call(f, lx, callee, args, 2)
        }
        other => {
            let v = emit_expr(f, other, lx);
            format_int(f, lx, v)
        }
    }
}

fn definitely_not_string(expr: &Expr, lx: &Lx) -> bool {
    match expr {
        Expr::Member { field, .. } if is_vector_axis(field) => true,
        Expr::Ident(name, _) => {
            int_local(lx, name).is_some()
                || lx.locals_f.keys().any(|k| k.eq_ignore_ascii_case(name))
                || (object_local(lx, name).is_some()
                    && lx.locals_s.keys().all(|k| !k.eq_ignore_ascii_case(name)))
        }
        _ => false,
    }
}

/// `String.*` method. `(name, instance, return bank)` — bank 2 is a string.
fn string_native(callee: &Expr, lx: &Lx) -> Option<(String, bool, u8)> {
    let Expr::Member { obj, field, .. } = callee else {
        return None;
    };
    let field_l = field.to_ascii_lowercase();
    let (ret_bank, instance_only) = match field_l.as_str() {
        "format" | "mid" | "left" | "right" | "filter" | "replace" | "tolower" | "toupper"
        | "makelower" | "makeupper" => (2u8, field_l != "format"),
        "charcodeat" | "length" | "toint" | "indexof" | "split" => (0, true),
        _ => return None,
    };
    let static_class =
        matches!(obj.as_ref(), Expr::Ident(name, _) if name.eq_ignore_ascii_case("string"));
    let instance = instance_only || !static_class;
    if field_l == "format" && !static_class {
        return None;
    }
    // `Vel.XY.Length()` is a vector method, not `String.Length`.
    if instance && definitely_not_string(obj, lx) {
        return None;
    }
    Some((format!("String.{field}"), instance, ret_bank))
}

fn emit_native(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    name: &str,
    recv: Option<&Expr>,
    args: &[Expr],
    ret_bank: u8,
    obj: Option<u8>,
) -> u8 {
    let mut prepared = Vec::new();
    if let Some(obj) = obj {
        prepared.push((3u8, obj));
    }
    if let Some(recv) = recv {
        prepared.push((2u8, emit_expr_s(f, recv, lx)));
    }
    if name.eq_ignore_ascii_case("String.Format") {
        // `vararg`: the format string, then each argument in its own bank
        // and the layout (see `prepare_sig_args`).
        let mut layout = Vec::new();
        for (k, arg) in args.iter().enumerate() {
            if k == 0 || expr_is_string(arg, lx) {
                prepared.push((2, emit_expr_s(f, arg, lx)));
                if k > 0 {
                    layout.push(2);
                }
            } else if fvec::expr_is_float(arg, lx) {
                prepared.push((1, emit_expr_f(f, arg, lx)));
                layout.push(1);
            } else {
                prepared.push((0, emit_expr(f, arg, lx)));
                layout.push(0);
            }
        }
        for bank in &layout {
            prepared.push((0, load_i(f, lx, i64::from(*bank))));
        }
        prepared.push((0, load_i(f, lx, layout.len() as i64)));
    } else if name.eq_ignore_ascii_case("String.Split") {
        // Out array, delimiter, keep-empty. The omitted flag defaults to keep.
        if let Some(arr) = args.first() {
            if let Some((reg, _)) = array_expr(f, lx, arr) {
                prepared.push((3, reg));
            }
        }
        if let Some(delim) = args.get(1) {
            prepared.push((2, emit_expr_s(f, delim, lx)));
        }
        let keep = if let Some(flag) = args.get(2) {
            emit_expr(f, flag, lx)
        } else {
            load_i(f, lx, 1)
        };
        prepared.push((0, keep));
    } else if name.eq_ignore_ascii_case("Actor.A_Burst")
        || name.eq_ignore_ascii_case("global.A_Burst")
    {
        // Class values are represented by their stable program-local class
        // id. String/name class expressions are also accepted by UZDoom.
        let class_reg = match args.first() {
            None => (0, load_i(f, lx, 0)),
            Some(arg) => match arg {
                Expr::Ident(name, _) if lx.class_ids.contains_key(&name.to_ascii_lowercase()) => {
                    (2, load_s(f, lx, name))
                }
                Expr::Name(name, _) | Expr::String(name, _) => (2, load_s(f, lx, name)),
                expr if expr_yields_string(expr, lx) => (2, emit_expr_s(f, expr, lx)),
                _ => (0, emit_expr(f, arg, lx)),
            },
        };
        prepared.push(class_reg);
    } else if name.eq_ignore_ascii_case("Actor.A_RadiusDamageSelf")
        || name.eq_ignore_ascii_case("global.A_RadiusDamageSelf")
    {
        // Preserve ZScript native defaults because CallNat transports only
        // explicit arguments, unlike UZDoom's generated VM thunk.
        let damage = args
            .first()
            .map(|arg| emit_expr(f, arg, lx))
            .unwrap_or_else(|| load_i(f, lx, 128));
        let distance = args
            .get(1)
            .map(|arg| emit_expr_f(f, arg, lx))
            .unwrap_or_else(|| load_f(f, lx, 128.0));
        let flags = args
            .get(2)
            .map(|arg| emit_expr(f, arg, lx))
            .unwrap_or_else(|| load_i(f, lx, 0));
        prepared.extend([(0, damage), (1, distance), (0, flags)]);
        if let Some(flash_type) = args.get(3) {
            let class_name = match flash_type {
                Expr::Ident(name, _) if name.eq_ignore_ascii_case("null") => String::new(),
                Expr::Ident(name, _) | Expr::Name(name, _) | Expr::String(name, _) => name.clone(),
                _ => String::new(),
            };
            // This VM has no first-class Class handle yet; carry direct class
            // literals as names so the playsim can spawn the requested flash.
            prepared.push((2, load_s(f, lx, &class_name)));
        }
    } else if name.eq_ignore_ascii_case("Actor.A_Chase")
        || name.eq_ignore_ascii_case("global.A_Chase")
    {
        // `statelabel` parameters are carried in the string bank by this VM.
        // Preserve the null/explicit-state distinction so the native can tell
        // an omitted default label from an explicitly disabled attack state.
        for (index, arg) in args.iter().enumerate() {
            if index < 2 {
                let state = match arg {
                    Expr::Ident(ident, _) if ident.eq_ignore_ascii_case("null") => {
                        "__room_null_state".to_owned()
                    }
                    Expr::String(value, _) | Expr::Name(value, _) => value.clone(),
                    _ => String::new(),
                };
                prepared.push((2, load_s(f, lx, &state)));
            } else {
                prepared.push((0, emit_expr(f, arg, lx)));
            }
        }
    } else {
        for arg in args {
            if let Expr::String(s, _) = arg {
                prepared.push((2, load_s(f, lx, s)));
            } else if let Expr::Ident(name, _) = arg {
                if let Some(r) = lx
                    .locals_s
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, r)| *r)
                {
                    prepared.push((2, r));
                    continue;
                }
                if let Some(s) = lx.string_consts.get(&name.to_ascii_lowercase()).cloned() {
                    prepared.push((2, load_s(f, lx, &s)));
                    continue;
                }
                prepared.push((0, emit_expr(f, arg, lx)));
            } else {
                prepared.push((0, emit_expr(f, arg, lx)));
            }
        }
    }
    for (bank, reg) in prepared {
        f.emit(Op::Arg, bank, reg, 0);
    }
    let id = lx.native_id(name);
    f.emit_ext(Op::CallNat, 1, ret_bank, 0, id);
    match ret_bank {
        1 => {
            let dest = lx.alloc_f();
            f.emit(Op::GetRet, 1, dest, 0);
            dest
        }
        2 => {
            let dest = lx.alloc_s();
            f.emit(Op::GetRet, 2, dest, 0);
            dest
        }
        3 => {
            let dest = lx.alloc_a();
            f.emit(Op::GetRet, 3, dest, 0);
            dest
        }
        _ => {
            let dest = lx.alloc();
            f.emit(Op::GetRet, 0, dest, 0);
            dest
        }
    }
}

/// Static `Type.Method(...)` on a known class or struct. The native traps until it is bound.
fn emit_type_call(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
    ret_bank: u8,
) -> Option<u8> {
    let Expr::Member { obj, field, .. } = callee else {
        return None;
    };
    if let Expr::Ident(name, _) = obj.as_ref() {
        if lx.class_ids.contains_key(&name.to_ascii_lowercase()) {
            // A declared method: typed arguments (floats, objects, vectors).
            if let Some(t) = lookup_method(lx, name, field, None) {
                if let Some(sig) = lx.funcs.get(&t.key).cloned() {
                    if !sig.instance {
                        return Some(emit_sig_call(f, lx, &t.key, &sig, None, args, ret_bank));
                    }
                }
            }
            return Some(emit_native(
                f,
                lx,
                &format!("{name}.{field}"),
                None,
                args,
                ret_bank,
                None,
            ));
        }
    }
    let (reg, class) = object_base(f, obj, lx)?;
    let class = if class.is_empty() {
        "object".to_owned()
    } else {
        class
    };
    // A declared method on the receiver's class chain: typed arguments.
    if let Some(t) = lookup_method(lx, &class, field, None) {
        if let Some(sig) = lx.funcs.get(&t.key).cloned() {
            if sig.instance {
                return Some(emit_sig_call(f, lx, &t.key, &sig, Some(reg), args, ret_bank));
            }
        }
    }
    Some(emit_native(
        f,
        lx,
        &format!("{class}.{field}"),
        None,
        args,
        ret_bank,
        Some(reg),
    ))
}

/// Class of the object `callee` returns, when it is a declared method.
fn call_result_class(f_callee: &Expr, lx: &Lx) -> Option<String> {
    let Expr::Member { obj, field, .. } = f_callee else {
        return resolve_call(f_callee, lx)
            .and_then(|t| lx.funcs.get(&t.key))
            .map(|sig| sig.ret_class.clone())
            .filter(|c| lx.class_ids.contains_key(c));
    };
    let class = match obj.as_ref() {
        Expr::Ident(n, _) if lx.class_ids.contains_key(&n.to_ascii_lowercase()) => {
            n.to_ascii_lowercase()
        }
        _ => fvec::static_class(obj, lx)?,
    };
    let t = lookup_method(lx, &class, field, None)?;
    lx.funcs
        .get(&t.key)
        .map(|sig| sig.ret_class.clone())
        .filter(|c| lx.class_ids.contains_key(c))
}

/// Call `sig` (script function or native) with typed arguments; `recv` is
/// the receiver of an instance method. Returns the result in `want`'s bank.
fn emit_sig_call(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    key: &str,
    sig: &FnSig,
    recv: Option<u8>,
    args: &[Expr],
    want: u8,
) -> u8 {
    let user: Vec<u8> = if sig.instance {
        sig.params.get(1..).unwrap_or_default().to_vec()
    } else {
        sig.params.clone()
    };
    let mut prepared = Vec::new();
    if sig.instance {
        if let Some(r) = recv.or(lx.self_reg) {
            prepared.push((3u8, r));
        }
    }
    prepared.extend(prepare_sig_args(f, lx, sig, &user, args));
    for (bank, reg) in prepared {
        f.emit(Op::Arg, bank, reg, 0);
    }
    if sig.native {
        let id = lx.native_id(key);
        f.emit_ext(Op::CallNat, 1, want, 0, id);
    } else {
        f.emit_ext(Op::Call, 1, 0, 0, sig.idx);
    }
    match want {
        1 => {
            let dest = lx.alloc_f();
            f.emit(Op::GetRet, 1, dest, 0);
            dest
        }
        2 => {
            let dest = lx.alloc_s();
            f.emit(Op::GetRet, 2, dest, 0);
            dest
        }
        3 => {
            let dest = lx.alloc_a();
            f.emit(Op::GetRet, 3, dest, 0);
            dest
        }
        _ => {
            let dest = lx.alloc();
            f.emit(Op::GetRet, 0, dest, 0);
            dest
        }
    }
}

fn is_bflag(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2 && bytes[0].eq_ignore_ascii_case(&b'b') && bytes[1].is_ascii_uppercase()
}

/// Read `bNoGravity`, or write it when `value` is set.
fn emit_bflag(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    name: &str,
    value: Option<&Expr>,
) -> Option<u8> {
    if !is_bflag(name) {
        return None;
    }
    let self_reg = lx.self_reg?;
    let flag = load_s(f, lx, &name[1..]);
    let id = if value.is_some() {
        lx.native_id("Actor.ChangeFlag")
    } else {
        lx.native_id("Actor.CheckFlag")
    };
    f.emit(Op::Arg, 3, self_reg, 0);
    f.emit(Op::Arg, 2, flag, 0);
    if let Some(value) = value {
        let on = emit_expr(f, value, lx);
        f.emit(Op::Arg, 0, on, 0);
        f.emit_ext(Op::CallNat, 1, 0, 0, id);
        return Some(0);
    }
    f.emit_ext(Op::CallNat, 1, 0, 0, id);
    let dest = lx.alloc();
    f.emit(Op::GetRet, 0, dest, 0);
    Some(dest)
}

fn load_s(f: &mut CompiledFunction, lx: &mut Lx, text: &str) -> u8 {
    let r = lx.alloc_s();
    let idx = f.intern_s(text);
    f.emit_ext(Op::LoadConstS, r, 0, 0, idx);
    r
}

fn emit_new_array(f: &mut CompiledFunction, lx: &mut Lx, string_elem: bool, dest: u8) {
    let name = if string_elem {
        "Array.NewS"
    } else {
        "Array.NewI"
    };
    let id = lx.native_id(name);
    f.emit_ext(Op::CallNat, 1, 3, 0, id);
    f.emit(Op::GetRet, 3, dest, 0);
}

fn array_method(callee: &Expr, args: &[Expr], lx: &Lx) -> Option<&'static str> {
    let Expr::Member { obj, field, .. } = callee else {
        return None;
    };
    let Expr::Ident(name, _) = obj.as_ref() else {
        return None;
    };
    let (_, string_elem) = array_local(lx, name)?;
    let field_l = field.to_ascii_lowercase();
    let string_value = args.iter().any(|a| {
        matches!(a, Expr::String(_, _))
            || matches!(a, Expr::Ident(n, _) if lx.locals_s.contains_key(n))
    });
    Some(match field_l.as_str() {
        "push" if string_elem || string_value => "Array.PushS",
        "push" => "Array.Push",
        "insert" if string_elem || string_value => "Array.InsertS",
        "insert" => "Array.Insert",
        "delete" => "Array.Delete",
        "find" if string_elem => "Array.FindS",
        "find" => "Array.Find",
        "size" => "Array.Size",
        "clear" => "Array.Clear",
        _ => return None,
    })
}

fn emit_array_call(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
    native: &str,
) -> u8 {
    let Expr::Member { obj, .. } = callee else {
        return 0;
    };
    let Expr::Ident(name, span) = obj.as_ref() else {
        lx.err(obj.span(), "array receiver");
        return 0;
    };
    let Some((reg, _)) = array_local(lx, name) else {
        lx.err(*span, format!("unknown array {name}"));
        return 0;
    };
    f.emit(Op::Arg, 3, reg, 0);
    match native {
        "Array.PushS" => {
            if let Some(arg) = args.first() {
                let r = emit_expr_s(f, arg, lx);
                f.emit(Op::Arg, 2, r, 0);
            }
        }
        "Array.InsertS" => {
            if let Some(idx) = args.first() {
                let r = emit_expr(f, idx, lx);
                f.emit(Op::Arg, 0, r, 0);
            }
            if let Some(val) = args.get(1) {
                let r = emit_expr_s(f, val, lx);
                f.emit(Op::Arg, 2, r, 0);
            }
        }
        "Array.Size" | "Array.Clear" => {}
        "Array.FindS" => {
            if let Some(arg) = args.first() {
                let r = emit_expr_s(f, arg, lx);
                f.emit(Op::Arg, 2, r, 0);
            }
        }
        "Array.Push" => {
            let zero = load_i(f, lx, 0);
            f.emit(Op::Arg, 0, zero, 0);
            if let Some(arg) = args.first() {
                let r = emit_expr(f, arg, lx);
                f.emit(Op::Arg, 0, r, 0);
            }
        }
        _ => {
            for arg in args {
                let r = emit_expr(f, arg, lx);
                f.emit(Op::Arg, 0, r, 0);
            }
        }
    }
    let id = lx.native_id(native);
    let ret_bank = if native == "Array.Size" { 0 } else { 0 };
    f.emit_ext(Op::CallNat, 1, ret_bank, 0, id);
    if matches!(native, "Array.Size" | "Array.Find" | "Array.FindS") {
        let dest = lx.alloc();
        f.emit(Op::GetRet, 0, dest, 0);
        dest
    } else {
        0
    }
}

fn init_object_class(expr: &Expr, lx: &Lx) -> Option<String> {
    match expr {
        Expr::New { class, .. } => Some(class.to_ascii_lowercase()),
        Expr::Cast { ty, .. } => Some(ty.to_ascii_lowercase()),
        Expr::Call { callee, .. } => {
            if let Some(target) = resolve_call(callee, lx) {
                if let Some(sig) = lx.funcs.get(&target.key) {
                    if sig.ret == 3 {
                        let class = if lx.class_ids.contains_key(&sig.ret_class) {
                            sig.ret_class.clone()
                        } else {
                            "object".to_owned()
                        };
                        return Some(class);
                    }
                    return None;
                }
            }
            if let Some(class) = cast_target(callee, lx) {
                return Some(class.to_ascii_lowercase());
            }
            if let Expr::Ident(n, _) = callee.as_ref() {
                if n.eq_ignore_ascii_case("new") || n.eq_ignore_ascii_case("find") {
                    return Some("object".to_owned());
                }
                // `let item = FindInventory(...)` / `Spawn(...)`.
                if n.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    return Some("object".to_owned());
                }
            }
            if let Expr::Member { obj, field, .. } = callee.as_ref() {
                if let Expr::Ident(cls, _) = obj.as_ref() {
                    let key = format!("{cls}.{field}").to_ascii_lowercase();
                    if matches!(
                        key.as_str(),
                        "eventhandler.find" | "object.new" | "cvar.getcvar"
                    ) {
                        return Some("object".to_owned());
                    }
                }
                // `let grenade = SpawnMissile(...)`.
                if field.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    return Some("object".to_owned());
                }
            }
            None
        }
        Expr::Ident(name, _) => {
            if let Some((_, class)) = object_local(lx, name) {
                return Some(class);
            }
            lookup_object_field(lx, &lx.class_name, name).map(|(_, ty)| ty)
        }
        Expr::Member { obj, field, .. } => {
            let class = init_object_class(obj, lx)?;
            lookup_object_field(lx, &class, field)
                .or_else(|| declaring_object_slot(lx, field))
                .map(|(_, ty)| ty)
        }
        Expr::Index { arr, .. } => object_elem_array(arr, lx).then(|| "object".to_owned()),
        Expr::Ternary { then_e, else_e, .. } => {
            init_object_class(then_e, lx).or_else(|| init_object_class(else_e, lx))
        }
        _ => None,
    }
}

fn object_elem_array(expr: &Expr, lx: &Lx) -> bool {
    match expr {
        Expr::Ident(name, _) => array_local(lx, name).is_some_and(|(_, string_elem)| !string_elem),
        Expr::Member { obj, field, .. } => {
            let class = init_object_class(obj, lx).unwrap_or_else(|| lx.class_name.clone());
            lookup_array_field(lx, &class, field)
                .or_else(|| declaring_array_field(lx, field))
                .is_some_and(|(_, string_elem)| !string_elem)
        }
        Expr::Index { arr, .. } => object_elem_array(arr, lx),
        _ => false,
    }
}

fn int_local(lx: &Lx, name: &str) -> Option<u8> {
    lx.locals
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, r)| *r)
}

fn object_local(lx: &Lx, name: &str) -> Option<(u8, String)> {
    lx.locals_o
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, (reg, class))| (*reg, class.clone()))
}

fn array_local(lx: &Lx, name: &str) -> Option<(u8, bool)> {
    lx.locals_a
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
}

fn lookup_array_field(lx: &Lx, class: &str, field: &str) -> Option<(u16, bool)> {
    let mut cur = Some(class.to_ascii_lowercase());
    for _ in 0..64 {
        let name = cur?;
        if let Some(fields) = lx.array_fields.get(&name) {
            if let Some((_, slot, string_elem)) = fields
                .iter()
                .find(|(n, _, _)| n.eq_ignore_ascii_case(field))
            {
                return Some((*slot, *string_elem));
            }
        }
        cur = parent_of(lx, &name);
    }
    None
}

fn declaring_array_field(lx: &Lx, field: &str) -> Option<(u16, bool)> {
    let mut found: Option<(usize, (u16, bool))> = None;
    for (class, fields) in &lx.array_fields {
        let Some((_, slot, string_elem)) = fields
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(field))
        else {
            continue;
        };
        let inherited = parent_of(lx, class).is_some_and(|parent| {
            lx.array_fields
                .get(&parent)
                .is_some_and(|list| list.iter().any(|(n, _, _)| n.eq_ignore_ascii_case(field)))
        });
        if inherited {
            continue;
        }
        let weight = parent_weight(lx, class);
        if found.as_ref().is_some_and(|(best, _)| weight <= *best) {
            continue;
        }
        found = Some((weight, (*slot, *string_elem)));
    }
    found.map(|(_, slot)| slot)
}

/// A local array, or an array field loaded from an object (`level.lines`).
fn array_expr(f: &mut CompiledFunction, lx: &mut Lx, arr: &Expr) -> Option<(u8, bool)> {
    match arr {
        Expr::Ident(name, _) => array_local(lx, name),
        Expr::Member { obj, field, .. } => {
            let (reg, class) = object_base(f, obj, lx)?;
            let (slot, string_elem) = lookup_array_field(lx, &class, field)
                .or_else(|| declaring_array_field(lx, field))?;
            let dest = lx.alloc_a();
            f.emit_ext(Op::LoadFieldA, dest, reg, 0, u32::from(slot));
            Some((dest, string_elem))
        }
        // `collectedItems[0][i]` — the inner index yields another array.
        Expr::Index { arr, index, .. } => {
            let (reg, string_elem) = array_expr(f, lx, arr)?;
            if string_elem {
                return None;
            }
            let idx = emit_expr(f, index, lx);
            let dest = lx.alloc_a();
            f.emit(Op::ArrayGetI, dest, reg, idx);
            Some((dest, false))
        }
        _ => None,
    }
}

fn parent_of(lx: &Lx, class: &str) -> Option<String> {
    lx.parents
        .get(&class.to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .cloned()
}

fn lookup_int_field(lx: &Lx, class: &str, field: &str) -> Option<u16> {
    let field = field.to_ascii_lowercase();
    let mut cur = Some(class.to_ascii_lowercase());
    for _ in 0..64 {
        let name = cur?;
        if let Some(slot) = lx.int_fields.get(&name).and_then(|m| m.get(&field)) {
            return Some(*slot);
        }
        cur = parent_of(lx, &name);
    }
    None
}

fn lookup_string_field(lx: &Lx, class: &str, field: &str) -> Option<u16> {
    let field = field.to_ascii_lowercase();
    let mut cur = Some(class.to_ascii_lowercase());
    for _ in 0..64 {
        let name = cur?;
        if let Some(slot) = lx.string_fields.get(&name).and_then(|m| m.get(&field)) {
            return Some(*slot);
        }
        cur = parent_of(lx, &name);
    }
    None
}

fn lookup_object_field(lx: &Lx, class: &str, field: &str) -> Option<(u16, String)> {
    let field = field.to_ascii_lowercase();
    let mut cur = Some(class.to_ascii_lowercase());
    for _ in 0..64 {
        let name = cur?;
        if let Some(slot) = lx.object_fields.get(&name).and_then(|m| m.get(&field)) {
            return Some(slot.clone());
        }
        cur = parent_of(lx, &name);
    }
    None
}

fn lookup_method(lx: &Lx, class: &str, field: &str, self_reg: Option<u8>) -> Option<CallTarget> {
    let field = field.to_ascii_lowercase();
    let mut cur = Some(class.to_ascii_lowercase());
    for _ in 0..64 {
        let name = cur?;
        let key = format!("{name}.{field}");
        if let Some(sig) = lx.funcs.get(&key) {
            return Some(CallTarget {
                key,
                self_reg: if sig.instance { self_reg } else { None },
            });
        }
        cur = parent_of(lx, &name);
    }
    None
}

fn self_object_slot(lx: &Lx, name: &str) -> Option<u16> {
    lookup_object_field(lx, &lx.class_name, name).map(|(slot, _)| slot)
}

fn self_int_slot(lx: &Lx, name: &str) -> Option<u16> {
    lookup_int_field(lx, &lx.class_name, name)
}

fn host_int(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "skill" => Some("skill"),
        "gamestate" => Some("gamestate"),
        "consoleplayer" => Some("consoleplayer"),
        "gametic" => Some("gametic"),
        "maxplayers" => Some("maxplayers"),
        "ticrate" => Some("ticrate"),
        "deathmatch" => Some("deathmatch"),
        "teamplay" => Some("teamplay"),
        "timelimit" => Some("timelimit"),
        "dmflags" => Some("dmflags"),
        "dmflags2" => Some("dmflags2"),
        "compatflags" => Some("compatflags"),
        "alwaysapplydmflags" => Some("alwaysapplydmflags"),
        "multiplayer" => Some("multiplayer"),
        "cleanxfac" => Some("cleanxfac"),
        "cleanyfac" => Some("cleanyfac"),
        "cleanxfac_1" => Some("cleanxfac_1"),
        "cleanyfac_1" => Some("cleanyfac_1"),
        "cleanwidth" => Some("cleanwidth"),
        "cleanheight" => Some("cleanheight"),
        "cleanwidth_1" => Some("cleanwidth_1"),
        "cleanheight_1" => Some("cleanheight_1"),
        "menuactive" => Some("menuactive"),
        "paletteflash" => Some("paletteflash"),
        "netgame" => Some("netgame"),
        "demoplayback" => Some("demoplayback"),
        "gameaction" => Some("gameaction"),
        "gameticrate" => Some("gameticrate"),
        _ => None,
    }
}

fn emit_host_int(f: &mut CompiledFunction, lx: &mut Lx, native: &str) -> u8 {
    let id = lx.native_id(native);
    f.emit_ext(Op::CallNat, 1, 0, 0, id);
    let dest = lx.alloc();
    f.emit(Op::GetRet, 0, dest, 0);
    dest
}

fn truthy(f: &mut CompiledFunction, lx: &mut Lx, reg: u8) -> u8 {
    let is_null = lx.alloc();
    f.emit(Op::IsNull, is_null, reg, 0);
    let zero = load_i(f, lx, 0);
    let dest = lx.alloc();
    f.emit(Op::EqI, dest, is_null, zero);
    dest
}

fn emit_default(f: &mut CompiledFunction, lx: &mut Lx, reg: u8, class: String) -> (u8, String) {
    let dest = lx.alloc_a();
    let id = lx.native_id("Actor.GetDefault");
    f.emit(Op::Arg, 3, reg, 0);
    f.emit_ext(Op::CallNat, 1, 3, 0, id);
    f.emit(Op::GetRet, 3, dest, 0);
    (dest, class)
}

fn object_base(f: &mut CompiledFunction, expr: &Expr, lx: &mut Lx) -> Option<(u8, String)> {
    match expr {
        Expr::Ident(name, _) => {
            if name.eq_ignore_ascii_case("default") {
                let self_reg = lx.self_reg?;
                return Some(emit_default(
                    f,
                    lx,
                    self_reg,
                    lx.class_name.to_ascii_lowercase(),
                ));
            }
            if let Some((reg, class)) = object_local(lx, name) {
                return Some((reg, class));
            }
            if let Some((slot, ty)) = lookup_object_field(lx, &lx.class_name, name) {
                let self_reg = lx.self_reg?;
                let dest = lx.alloc_a();
                f.emit_ext(Op::LoadFieldA, dest, self_reg, 0, u32::from(slot));
                return Some((dest, ty));
            }
            if let Some((reg, _)) = array_local(lx, name) {
                return Some((reg, "array".to_owned()));
            }
            None
        }
        Expr::Member { obj, field, .. } => {
            if field.eq_ignore_ascii_case("default") {
                let (reg, class) = object_base(f, obj, lx)?;
                return Some(emit_default(f, lx, reg, class));
            }
            let (reg, class) = object_base(f, obj, lx)?;
            let (slot, ty) = lookup_object_field(lx, &class, field)
                .or_else(|| declaring_object_slot(lx, field))?;
            let dest = lx.alloc_a();
            f.emit_ext(Op::LoadFieldA, dest, reg, 0, u32::from(slot));
            Some((dest, ty))
        }
        Expr::Cast { ty, expr, .. } => {
            let dest = emit_cast(f, lx, ty, Some(expr));
            Some((dest, ty.to_ascii_lowercase()))
        }
        Expr::Call { callee, args, .. } => {
            if let Expr::Ident(name, _) = callee.as_ref() {
                if name.eq_ignore_ascii_case("new") {
                    return None;
                }
            }
            if let Some(class) = cast_target(callee, lx) {
                let dest = emit_cast(f, lx, &class, args.first());
                return Some((dest, class.to_ascii_lowercase()));
            }
            let result_class =
                call_result_class(callee, lx).unwrap_or_else(|| "object".to_owned());
            if let Some(dest) = emit_type_call(f, lx, callee, args, 3) {
                return Some((dest, result_class));
            }
            if let Some(dest) = emit_script_call(f, lx, callee, args, 3) {
                return Some((dest, result_class));
            }
            // `GetDefaultByType(cls).MaxAmount` — the function is an engine
            // builtin with no ZScript declaration. A known method that does
            // not return an object stays unresolved.
            if resolve_call(callee, lx).is_none() {
                let dest = emit_unresolved_call(f, lx, callee, args, 3);
                return Some((dest, "object".to_owned()));
            }
            None
        }
        Expr::Index { arr, index, .. } => {
            if let Expr::Ident(name, _) = arr.as_ref() {
                if name.eq_ignore_ascii_case("players") {
                    let idx = emit_expr(f, index, lx);
                    let id = lx.native_id("players.at");
                    f.emit(Op::Arg, 0, idx, 0);
                    f.emit_ext(Op::CallNat, 1, 3, 0, id);
                    let dest = lx.alloc_a();
                    f.emit(Op::GetRet, 3, dest, 0);
                    return Some((dest, "playerinfo".to_owned()));
                }
            }
            let (reg, string_elem) = array_expr(f, lx, arr)?;
            if string_elem {
                return None;
            }
            let idx = emit_expr(f, index, lx);
            let dest = lx.alloc_a();
            f.emit(Op::ArrayGetI, dest, reg, idx);
            Some((dest, "object".to_owned()))
        }
        _ => None,
    }
}

fn emit_member_i(
    f: &mut CompiledFunction,
    obj: &Expr,
    field: &str,
    lx: &mut Lx,
    span: crate::span::Span,
) -> u8 {
    if let Expr::Ident(cls, _) = obj {
        if let Some(v) = lookup_int_const(&lx.consts, &lx.parents, cls, field) {
            return load_i(f, lx, v);
        }
        if let Some(v) = lookup_float_const(&lx.float_consts, &lx.parents, cls, field) {
            let reg = load_f(f, lx, v);
            return f2i(f, lx, reg);
        }
    }
    if field.eq_ignore_ascii_case("default") {
        if let Some((reg, class)) = object_base(f, obj, lx) {
            let (dest, _) = emit_default(f, lx, reg, class);
            return truthy(f, lx, dest);
        }
    }
    let Some((reg, class)) = object_base(f, obj, lx) else {
        if let Some(dest) = load_vector_component(f, lx, obj, field) {
            return dest;
        }
        if let Expr::Ident(cls, _) = obj {
            let key = format!("{cls}.{field}").to_ascii_lowercase();
            if lx.funcs.contains_key(&key) {
                return load_i(f, lx, 0);
            }
        }
        lx.err(span, format!("unknown field {field}"));
        return 0;
    };
    if let Some(v) = lookup_int_const(&lx.consts, &lx.parents, &class, field) {
        return load_i(f, lx, v);
    }
    if let Some(v) = lookup_float_const(&lx.float_consts, &lx.parents, &class, field) {
        let reg = load_f(f, lx, v);
        return f2i(f, lx, reg);
    }
    if let Some(slot) =
        lookup_int_field(lx, &class, field).or_else(|| declaring_int_slot(lx, field))
    {
        let dest = lx.alloc();
        f.emit_ext(Op::LoadFieldI, dest, reg, 0, u32::from(slot));
        return dest;
    }
    if let Some((slot, _)) =
        lookup_object_field(lx, &class, field).or_else(|| declaring_object_slot(lx, field))
    {
        let dest = lx.alloc_a();
        f.emit_ext(Op::LoadFieldA, dest, reg, 0, u32::from(slot));
        return truthy(f, lx, dest);
    }
    if let Some(dest) = load_vector_component(f, lx, obj, field) {
        return dest;
    }
    if is_bflag(field) {
        return check_flag(f, lx, reg, &field[1..]);
    }
    if let Some(slot) =
        lookup_string_field(lx, &class, field).or_else(|| declaring_string_slot(lx, field))
    {
        let s = lx.alloc_s();
        f.emit_ext(Op::LoadFieldS, s, reg, 0, u32::from(slot));
        return string_truthy(f, lx, s);
    }
    if let Some((slot, _)) =
        lookup_array_field(lx, &class, field).or_else(|| declaring_array_field(lx, field))
    {
        let dest = lx.alloc_a();
        f.emit_ext(Op::LoadFieldA, dest, reg, 0, u32::from(slot));
        return truthy(f, lx, dest);
    }
    lx.err(span, format!("unknown field {field}"));
    0
}

/// The class that declares `field`.
///
/// Same tie-break as [`declaring_int_slot`]: the declarer with the most
/// subclasses wins.
fn declaring_object_slot(lx: &Lx, field: &str) -> Option<(u16, String)> {
    let field = field.to_ascii_lowercase();
    let mut found: Option<(usize, (u16, String))> = None;
    for (class, map) in &lx.object_fields {
        let Some(slot) = map.get(&field) else {
            continue;
        };
        let inherited = parent_of(lx, class).is_some_and(|parent| {
            lx.object_fields
                .get(&parent)
                .is_some_and(|m| m.contains_key(&field))
        });
        if inherited {
            continue;
        }
        let weight = parent_weight(lx, class);
        if found.as_ref().is_some_and(|(best, _)| weight <= *best) {
            continue;
        }
        found = Some((weight, slot.clone()));
    }
    found.map(|(_, slot)| slot)
}

fn declaring_string_slot(lx: &Lx, field: &str) -> Option<u16> {
    let field = field.to_ascii_lowercase();
    let mut found: Option<(usize, u16)> = None;
    for (class, map) in &lx.string_fields {
        let Some(&slot) = map.get(&field) else {
            continue;
        };
        let inherited = parent_of(lx, class).is_some_and(|parent| {
            lx.string_fields
                .get(&parent)
                .is_some_and(|m| m.contains_key(&field))
        });
        if inherited {
            continue;
        }
        let weight = parent_weight(lx, class);
        if found.as_ref().is_some_and(|(best, _)| weight <= *best) {
            continue;
        }
        found = Some((weight, slot));
    }
    found.map(|(_, slot)| slot)
}

fn parent_weight(lx: &Lx, class: &str) -> usize {
    lx.parents
        .values()
        .filter(|parent| parent.eq_ignore_ascii_case(class))
        .count()
}

/// The class that declares `field`.
///
/// When several classes declare the same name, the one with the most
/// subclasses wins. `Amount` lives on both `Inventory` and `DropItem`;
/// inventory is the one callers mean.
fn declaring_int_slot(lx: &Lx, field: &str) -> Option<u16> {
    let field = field.to_ascii_lowercase();
    let mut found: Option<(usize, u16)> = None;
    for (class, map) in &lx.int_fields {
        let Some(&slot) = map.get(&field) else {
            continue;
        };
        let inherited = parent_of(lx, class).is_some_and(|parent| {
            lx.int_fields
                .get(&parent)
                .is_some_and(|m| m.contains_key(&field))
        });
        if inherited {
            continue;
        }
        let weight = parent_weight(lx, class);
        if found.as_ref().is_some_and(|(best, _)| weight <= *best) {
            continue;
        }
        found = Some((weight, slot));
    }
    found.map(|(_, slot)| slot)
}

fn check_flag(f: &mut CompiledFunction, lx: &mut Lx, obj: u8, flag: &str) -> u8 {
    let name = load_s(f, lx, flag);
    let id = lx.native_id("Actor.CheckFlag");
    f.emit(Op::Arg, 3, obj, 0);
    f.emit(Op::Arg, 2, name, 0);
    f.emit_ext(Op::CallNat, 1, 0, 0, id);
    let dest = lx.alloc();
    f.emit(Op::GetRet, 0, dest, 0);
    dest
}

fn is_cvar_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    // User CVars are lowercase (`idmypos`, `paletteflash`, `sv_infiniteammo`).
    first.is_ascii_lowercase()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_scalar_vector(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "vector2" | "vector3" | "vector4" | "fvector2" | "fvector3" | "fvector4" | "quat" | "fquat"
    )
}

/// `random[rng]()` / `random2[rng]()` / `frandom[rng]()`.
fn emit_unresolved_call(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
    ret_bank: u8,
) -> u8 {
    // `texture.IsValid()` on a value-typed local (`TextureID texture`): the
    // native is named for the type and takes the value as its first argument.
    if let Expr::Member { obj, field, .. } = callee {
        if let Expr::Ident(name, _) = obj.as_ref() {
            if let Some(ty) = lx.local_types.get(&name.to_ascii_lowercase()).cloned() {
                let mut with_self = vec![obj.as_ref().clone()];
                with_self.extend(args.iter().cloned());
                return emit_native(f, lx, &format!("{ty}.{field}"), None, &with_self, ret_bank, None);
            }
        }
    }
    // `type.GetClassName()` on a `class<T>` / name local: the native is
    // named for the value (`Class.GetClassName`) and takes the string first.
    if let Expr::Member { obj, field, .. } = callee {
        if let Expr::Ident(name, _) = obj.as_ref() {
            if lx.locals_s.keys().any(|k| k.eq_ignore_ascii_case(name))
                && object_local(lx, name).is_none()
            {
                let recv = obj.as_ref().clone();
                let owner = if matches!(
                    field.to_ascii_lowercase().as_str(),
                    "getclassname" | "isabstract" | "getparentclass"
                ) {
                    "Class"
                } else {
                    "String"
                };
                return emit_native(f, lx, &format!("{owner}.{field}"), Some(&recv), args, ret_bank, None);
            }
        }
    }
    let native = unresolved_call_name(callee);
    let obj = match callee {
        Expr::Member { obj, .. } => object_base(f, obj, lx).map(|(reg, _)| reg),
        _ => None,
    };
    emit_native(f, lx, &native, None, args, ret_bank, obj)
}

fn unresolved_call_name(callee: &Expr) -> String {
    match callee {
        Expr::Member { obj, field, .. } => {
            if let Expr::Ident(name, _) = obj.as_ref() {
                format!("{name}.{field}")
            } else {
                format!("object.{field}")
            }
        }
        Expr::Ident(name, _) => format!("global.{name}"),
        _ => "global.call".to_owned(),
    }
}

fn rng_call_name(callee: &Expr) -> Option<String> {
    let Expr::Index { arr, .. } = callee else {
        return None;
    };
    let Expr::Ident(name, _) = arr.as_ref() else {
        return None;
    };
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "random" | "random2" | "frandom" | "randompick"
    )
    .then_some(name)
}

fn string_truthy(f: &mut CompiledFunction, lx: &mut Lx, reg: u8) -> u8 {
    let empty = load_s(f, lx, "");
    let id = lx.native_id("String.Eq");
    f.emit(Op::Arg, 2, reg, 0);
    f.emit(Op::Arg, 2, empty, 0);
    f.emit_ext(Op::CallNat, 1, 0, 0, id);
    let eq = lx.alloc();
    f.emit(Op::GetRet, 0, eq, 0);
    let zero = load_i(f, lx, 0);
    let dest = lx.alloc();
    f.emit(Op::EqI, dest, eq, zero);
    dest
}

fn is_vector_axis(field: &str) -> bool {
    matches!(
        field.to_ascii_lowercase().as_str(),
        "x" | "y" | "z" | "w" | "xy" | "r" | "g" | "b" | "a"
    )
}

/// `vel.Z` / `self.pos.X` when the vector is stored as one scalar slot.
fn load_vector_component(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    obj: &Expr,
    field: &str,
) -> Option<u8> {
    if !is_vector_axis(field) {
        return None;
    }
    match obj {
        Expr::Ident(name, _) => {
            if let Some(reg) = int_local(lx, name) {
                return Some(reg);
            }
            let slot = self_int_slot(lx, name)?;
            let self_reg = lx.self_reg?;
            let dest = lx.alloc();
            f.emit_ext(Op::LoadFieldI, dest, self_reg, 0, u32::from(slot));
            Some(dest)
        }
        Expr::Member {
            obj: inner,
            field: vec_field,
            ..
        } => {
            let (reg, class) = object_base(f, inner, lx)?;
            let slot = lookup_int_field(lx, &class, vec_field)
                .or_else(|| declaring_int_slot(lx, vec_field))?;
            let dest = lx.alloc();
            f.emit_ext(Op::LoadFieldI, dest, reg, 0, u32::from(slot));
            Some(dest)
        }
        _ => None,
    }
}

fn store_vector_component(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    obj: &Expr,
    field: &str,
    rhs: &Expr,
) -> bool {
    if !is_vector_axis(field) {
        return false;
    }
    let v = emit_expr(f, rhs, lx);
    match obj {
        Expr::Ident(name, _) => {
            if let Some(dest) = int_local(lx, name) {
                f.emit(Op::MoveI, dest, v, 0);
                return true;
            }
            if let (Some(slot), Some(self_reg)) = (self_int_slot(lx, name), lx.self_reg) {
                f.emit_ext(Op::StoreFieldI, self_reg, v, 0, u32::from(slot));
                return true;
            }
            false
        }
        Expr::Member {
            obj: inner,
            field: vec_field,
            ..
        } => {
            let Some((reg, class)) = object_base(f, inner, lx) else {
                return false;
            };
            let Some(slot) = lookup_int_field(lx, &class, vec_field)
                .or_else(|| declaring_int_slot(lx, vec_field))
            else {
                return false;
            };
            f.emit_ext(Op::StoreFieldI, reg, v, 0, u32::from(slot));
            true
        }
        _ => false,
    }
}

/// `Vel.Z += n` when the vector is one scalar slot.
fn update_vector_component(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    obj: &Expr,
    field: &str,
    op: &str,
    rhs: &Expr,
) -> bool {
    if !is_vector_axis(field) {
        return false;
    }
    let Some(code) = int_assign_op(op) else {
        return false;
    };
    let apply = |f: &mut CompiledFunction, lx: &mut Lx, cur: u8| -> u8 {
        let rv = emit_expr(f, rhs, lx);
        let dest = lx.alloc();
        f.emit(code, dest, cur, rv);
        dest
    };
    match obj {
        Expr::Ident(name, _) => {
            if let Some(dest) = int_local(lx, name) {
                let out = apply(f, lx, dest);
                f.emit(Op::MoveI, dest, out, 0);
                return true;
            }
            if let (Some(slot), Some(self_reg)) = (self_int_slot(lx, name), lx.self_reg) {
                let cur = lx.alloc();
                f.emit_ext(Op::LoadFieldI, cur, self_reg, 0, u32::from(slot));
                let out = apply(f, lx, cur);
                f.emit_ext(Op::StoreFieldI, self_reg, out, 0, u32::from(slot));
                return true;
            }
            false
        }
        Expr::Member {
            obj: inner,
            field: vec_field,
            ..
        } => {
            let Some((reg, class)) = object_base(f, inner, lx) else {
                return false;
            };
            let Some(slot) = lookup_int_field(lx, &class, vec_field)
                .or_else(|| declaring_int_slot(lx, vec_field))
            else {
                return false;
            };
            let cur = lx.alloc();
            f.emit_ext(Op::LoadFieldI, cur, reg, 0, u32::from(slot));
            let out = apply(f, lx, cur);
            f.emit_ext(Op::StoreFieldI, reg, out, 0, u32::from(slot));
            true
        }
        _ => false,
    }
}

fn emit_actor_native(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
    want: u8,
) -> Option<u8> {
    let Expr::Member { obj, field, .. } = callee else {
        return None;
    };
    let recv = if let Some((reg, _)) = object_base(f, obj, lx) {
        reg
    } else if matches!(obj.as_ref(), Expr::Call { .. }) {
        emit_expr_a(f, obj, lx)
    } else {
        return None;
    };
    match field.to_ascii_lowercase().as_str() {
        "giveinventory" if want == 0 => {
            let item = args.first().map(|a| emit_expr_s(f, a, lx)).unwrap_or(0);
            let amt = args
                .get(1)
                .map(|a| emit_expr(f, a, lx))
                .unwrap_or_else(|| load_i(f, lx, 1));
            let id = lx.native_id("Actor.GiveInventory");
            f.emit(Op::Arg, 3, recv, 0);
            f.emit(Op::Arg, 2, item, 0);
            f.emit(Op::Arg, 0, amt, 0);
            f.emit_ext(Op::CallNat, 1, 0, 0, id);
            Some(0)
        }
        "takeinventory" if want == 0 => {
            let item = args.first().map(|a| emit_expr_s(f, a, lx)).unwrap_or(0);
            let amt = args
                .get(1)
                .map(|a| emit_expr(f, a, lx))
                .unwrap_or_else(|| load_i(f, lx, 1));
            let neg = lx.alloc();
            let zero = load_i(f, lx, 0);
            f.emit(Op::SubI, neg, zero, amt);
            let id = lx.native_id("Actor.GiveInventory");
            f.emit(Op::Arg, 3, recv, 0);
            f.emit(Op::Arg, 2, item, 0);
            f.emit(Op::Arg, 0, neg, 0);
            f.emit_ext(Op::CallNat, 1, 0, 0, id);
            Some(0)
        }
        "getgender" | "getint" if want == 0 => {
            let name = if field.eq_ignore_ascii_case("getgender") {
                "PlayerInfo.GetGender"
            } else {
                "CVar.GetInt"
            };
            let id = lx.native_id(name);
            f.emit(Op::Arg, 3, recv, 0);
            f.emit_ext(Op::CallNat, 1, 0, 0, id);
            let dest = lx.alloc();
            f.emit(Op::GetRet, 0, dest, 0);
            Some(dest)
        }
        "changetid" if want == 0 => {
            let tid = args.first().map(|a| emit_expr(f, a, lx)).unwrap_or(0);
            let id = lx.native_id("Actor.ChangeTID");
            f.emit(Op::Arg, 3, recv, 0);
            f.emit(Op::Arg, 0, tid, 0);
            f.emit_ext(Op::CallNat, 1, 0, 0, id);
            Some(0)
        }
        "getclassname" if want == 2 => {
            let id = lx.native_id("Actor.GetClassName");
            f.emit(Op::Arg, 3, recv, 0);
            f.emit_ext(Op::CallNat, 1, 2, 0, id);
            let dest = lx.alloc_s();
            f.emit(Op::GetRet, 2, dest, 0);
            Some(dest)
        }
        _ => None,
    }
}

fn emit_field_update(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    obj: &Expr,
    field: &str,
    op: &str,
    rhs: &Expr,
) -> bool {
    if op == "=" {
        if emit_field_store(f, lx, obj, field, rhs) {
            return true;
        }
        return store_vector_component(f, lx, obj, field, rhs);
    }
    let Some((reg, class)) = object_base(f, obj, lx) else {
        return update_vector_component(f, lx, obj, field, op, rhs);
    };
    let Some(slot) = lookup_int_field(lx, &class, field).or_else(|| declaring_int_slot(lx, field))
    else {
        if is_bflag(field) {
            let cur = check_flag(f, lx, reg, &field[1..]);
            let rv = emit_expr(f, rhs, lx);
            let dest = lx.alloc();
            let Some(code) = int_assign_op(op) else {
                return false;
            };
            f.emit(code, dest, cur, rv);
            let flag = load_s(f, lx, &field[1..]);
            let id = lx.native_id("Actor.ChangeFlag");
            f.emit(Op::Arg, 3, reg, 0);
            f.emit(Op::Arg, 2, flag, 0);
            f.emit(Op::Arg, 0, dest, 0);
            f.emit_ext(Op::CallNat, 1, 0, 0, id);
            return true;
        }
        return update_vector_component(f, lx, obj, field, op, rhs);
    };
    let cur = lx.alloc();
    f.emit_ext(Op::LoadFieldI, cur, reg, 0, u32::from(slot));
    let rv = emit_expr(f, rhs, lx);
    let dest = lx.alloc();
    let Some(code) = int_assign_op(op) else {
        return false;
    };
    f.emit(code, dest, cur, rv);
    f.emit_ext(Op::StoreFieldI, reg, dest, 0, u32::from(slot));
    true
}

fn emit_field_store(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    obj: &Expr,
    field: &str,
    rhs: &Expr,
) -> bool {
    let Some((reg, class)) = object_base(f, obj, lx) else {
        return false;
    };
    let field_l = field.to_ascii_lowercase();
    if let Some(slot) = lookup_int_field(lx, &class, field) {
        let v = emit_expr(f, rhs, lx);
        f.emit_ext(Op::StoreFieldI, reg, v, 0, u32::from(slot));
        return true;
    }
    if let Some((slot, _)) =
        lookup_object_field(lx, &class, field).or_else(|| declaring_object_slot(lx, field))
    {
        let v = emit_expr_a(f, rhs, lx);
        f.emit_ext(Op::StoreFieldA, reg, v, 0, u32::from(slot));
        return true;
    }
    if field_l == "soundclass" {
        let v = emit_expr_s(f, rhs, lx);
        let id = lx.native_id("Actor.SetSoundClass");
        f.emit(Op::Arg, 3, reg, 0);
        f.emit(Op::Arg, 2, v, 0);
        f.emit_ext(Op::CallNat, 1, 0, 0, id);
        return true;
    }
    if let Some(slot) =
        lookup_string_field(lx, &class, field).or_else(|| declaring_string_slot(lx, field))
    {
        let v = emit_expr_s(f, rhs, lx);
        f.emit_ext(Op::StoreFieldS, reg, v, 0, u32::from(slot));
        return true;
    }
    if let Some(slot) = declaring_int_slot(lx, field) {
        let v = emit_expr(f, rhs, lx);
        f.emit_ext(Op::StoreFieldI, reg, v, 0, u32::from(slot));
        return true;
    }
    if is_bflag(field) {
        let flag = load_s(f, lx, &field[1..]);
        let on = emit_expr(f, rhs, lx);
        let id = lx.native_id("Actor.ChangeFlag");
        f.emit(Op::Arg, 3, reg, 0);
        f.emit(Op::Arg, 2, flag, 0);
        f.emit(Op::Arg, 0, on, 0);
        f.emit_ext(Op::CallNat, 1, 0, 0, id);
        return true;
    }
    false
}

fn string_cmp(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    op: &str,
    lhs: &Expr,
    rhs: &Expr,
) -> Option<u8> {
    if op != "==" && op != "!=" {
        return None;
    }
    if !expr_is_string(lhs, lx) && !expr_is_string(rhs, lx) {
        return None;
    }
    let l = emit_expr_s(f, lhs, lx);
    let r = emit_expr_s(f, rhs, lx);
    let id = lx.native_id("String.Eq");
    f.emit(Op::Arg, 2, l, 0);
    f.emit(Op::Arg, 2, r, 0);
    f.emit_ext(Op::CallNat, 1, 0, 0, id);
    let dest = lx.alloc();
    f.emit(Op::GetRet, 0, dest, 0);
    if op == "!=" {
        let zero = load_i(f, lx, 0);
        let inv = lx.alloc();
        f.emit(Op::EqI, inv, dest, zero);
        return Some(inv);
    }
    Some(dest)
}

fn expr_is_string(expr: &Expr, lx: &Lx) -> bool {
    match expr {
        Expr::String(_, _) | Expr::Name(_, _) => true,
        Expr::Ident(name, _) => lx.locals_s.contains_key(name),
        _ => false,
    }
}

fn emit_logic(f: &mut CompiledFunction, lx: &mut Lx, op: &str, lhs: &Expr, rhs: &Expr) -> u8 {
    let dest = lx.alloc();
    let l = emit_expr(f, lhs, lx);
    f.emit(Op::MoveI, dest, l, 0);
    let jump = if op == "||" {
        emit_jcc(f, Op::JmpIf, dest)
    } else {
        emit_jcc(f, Op::JmpIfNot, dest)
    };
    let r = emit_expr(f, rhs, lx);
    f.emit(Op::MoveI, dest, r, 0);
    patch(f, jump);
    dest
}

fn string_index_reg(f: &mut CompiledFunction, lx: &mut Lx, arr: &Expr) -> Option<u8> {
    match arr {
        Expr::String(s, _) | Expr::Name(s, _) => Some(load_s(f, lx, s)),
        Expr::Ident(name, _) => lx
            .locals_s
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, r)| *r),
        Expr::Member { obj, field, .. } => {
            let (reg, class) = object_base(f, obj, lx)?;
            let slot = lookup_string_field(lx, &class, field)
                .or_else(|| declaring_string_slot(lx, field))?;
            let dest = lx.alloc_s();
            f.emit_ext(Op::LoadFieldS, dest, reg, 0, u32::from(slot));
            Some(dest)
        }
        _ => None,
    }
}

fn emit_index_i(
    f: &mut CompiledFunction,
    arr: &Expr,
    index: &Expr,
    lx: &mut Lx,
    span: crate::span::Span,
) -> u8 {
    if let Some((reg, string_elem)) = array_expr(f, lx, arr) {
        if string_elem {
            let idx = emit_expr(f, index, lx);
            let dest = lx.alloc_s();
            f.emit(Op::ArrayGetS, dest, reg, idx);
            return string_truthy(f, lx, dest);
        }
        let idx = emit_expr(f, index, lx);
        let dest = lx.alloc();
        f.emit(Op::ArrayGetI, dest, reg, idx);
        return dest;
    }
    if let Some(sreg) = string_index_reg(f, lx, arr) {
        let idx = emit_expr(f, index, lx);
        let id = lx.native_id("String.CharCodeAt");
        f.emit(Op::Arg, 2, sreg, 0);
        f.emit(Op::Arg, 0, idx, 0);
        f.emit_ext(Op::CallNat, 1, 0, 0, id);
        let dest = lx.alloc();
        f.emit(Op::GetRet, 0, dest, 0);
        return dest;
    }
    match arr {
        Expr::Ident(name, _) => lx.err(span, format!("unknown array {name}")),
        _ => lx.err(span, "array index"),
    }
    0
}

fn emit_index_s(
    f: &mut CompiledFunction,
    arr: &Expr,
    index: &Expr,
    lx: &mut Lx,
    span: crate::span::Span,
) -> u8 {
    let Some((reg, _)) = array_expr(f, lx, arr) else {
        match arr {
            Expr::Ident(name, _) => lx.err(span, format!("unknown array {name}")),
            _ => lx.err(span, "array index"),
        }
        return load_s(f, lx, "");
    };
    let idx = emit_expr(f, index, lx);
    let dest = lx.alloc_s();
    f.emit(Op::ArrayGetS, dest, reg, idx);
    dest
}

fn named_type(ty: &TypeRef) -> Option<&str> {
    match ty {
        TypeRef::Readonly(inner) => named_type(inner),
        TypeRef::Named(t) => Some(t.as_str()),
        _ => None,
    }
}

fn is_class_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::ClassOf(_) => true,
        TypeRef::Readonly(inner) => is_class_type(inner),
        TypeRef::Named(t) => t.eq_ignore_ascii_case("class"),
        _ => false,
    }
}

fn p_type_name(ty: &TypeRef) -> &str {
    named_type(ty).unwrap_or("")
}

fn array_elem_kind(ty: &TypeRef) -> Option<bool> {
    match ty {
        // Strings, names and class values (`Array< Class<Ammo> >`) are
        // carried by name, so they live in string arrays.
        TypeRef::Array(inner) => Some(matches!(
            inner.as_ref(),
            TypeRef::Named(t) if t.eq_ignore_ascii_case("string") || t.eq_ignore_ascii_case("name")
        ) || matches!(inner.as_ref(), TypeRef::ClassOf(_))),
        // `Array<T>` is a named type once the generic arguments are skipped.
        TypeRef::Named(t) if t.eq_ignore_ascii_case("array") => Some(false),
        TypeRef::Map(_, _) => Some(false),
        _ => None,
    }
}

fn value_bank(ty: &TypeRef, class_ids: &BTreeMap<String, u32>) -> u8 {
    if array_elem_kind(ty).is_some() {
        return 3;
    }
    let bank = param_bank(ty);
    if bank != 0 {
        return bank;
    }
    if class_ids.contains_key(&p_type_name(ty).to_ascii_lowercase()) {
        3
    } else {
        0
    }
}

fn param_bank(ty: &TypeRef) -> u8 {
    // Class values are carried by name: DECORATE classes have no script id.
    if is_class_type(ty) {
        return 2;
    }
    match named_type(ty).unwrap_or("") {
        t if t.eq_ignore_ascii_case("string") || t.eq_ignore_ascii_case("name") => 2,
        t if t.eq_ignore_ascii_case("double") || t.eq_ignore_ascii_case("float") => 1,
        _ => 0,
    }
}

/// Field counts `[i, a, s, f]` declared by `class` itself (with `extend class`).
fn own_field_counts(
    class: &str,
    merged: &BTreeMap<String, Vec<Member>>,
    type_names: &std::collections::BTreeSet<String>,
) -> [u16; 4] {
    let Some(members) = merged.get(class) else {
        return [0; 4];
    };
    let (layout, ..) = layout_members(class, "", members, type_names, [0; 4]);
    [layout.n_i, layout.n_a, layout.n_s, layout.n_f]
}

/// First slot `[i, a, s, f]` of `class`'s own fields: its ancestors' totals.
fn layout_base(
    class: &str,
    merged: &BTreeMap<String, Vec<Member>>,
    parents: &BTreeMap<String, String>,
    type_names: &std::collections::BTreeSet<String>,
    memo: &mut BTreeMap<String, [u16; 4]>,
    depth: u32,
) -> [u16; 4] {
    if let Some(base) = memo.get(class) {
        return *base;
    }
    let parent = parents.get(class).filter(|p| !p.is_empty() && p.as_str() != class);
    let base = match parent {
        Some(p) if depth < 64 => {
            let pb = layout_base(p, merged, parents, type_names, memo, depth + 1);
            let own = own_field_counts(p, merged, type_names);
            [pb[0] + own[0], pb[1] + own[1], pb[2] + own[2], pb[3] + own[3]]
        }
        _ => [0; 4],
    };
    memo.insert(class.to_owned(), base);
    base
}

fn layout_members(
    name: &str,
    parent: &str,
    members: &[Member],
    type_names: &std::collections::BTreeSet<String>,
    start: [u16; 4],
) -> (
    ClassLayout,
    Vec<(String, u16, bool)>,
    Vec<(String, u16, String)>,
    Vec<(String, u16, u8)>,
) {
    let mut layout = ClassLayout {
        name: name.to_owned(),
        parent: parent.to_owned(),
        ..ClassLayout::default()
    };
    let mut arrays = Vec::new();
    let mut objects = Vec::new();
    let mut floats = Vec::new();
    let [mut n_i, mut n_a, mut n_s, mut n_f] = start;
    for member in members {
        if let Member::FlagDef { name, .. } = member {
            // `flagdef NoGravity` is the bool field `bNoGravity`.
            let fname = format!("b{name}").to_ascii_lowercase();
            if !layout.fields.contains_key(&fname) {
                layout.fields.insert(fname, (0, n_i));
                n_i += 1;
            }
            continue;
        }
        let Member::Field(field) = member else {
            continue;
        };
        for fname in &field.names {
            match &field.ty {
                TypeRef::Array(inner) => {
                    // Names and class values are carried as strings.
                    let string_elem = matches!(
                        inner.as_ref(),
                        TypeRef::Named(t)
                            if t.eq_ignore_ascii_case("string") || t.eq_ignore_ascii_case("name")
                    ) || matches!(inner.as_ref(), TypeRef::ClassOf(_));
                    if string_elem {
                        layout.array_s.push(n_a);
                    } else {
                        layout.array_i.push(n_a);
                    }
                    layout.fields.insert(fname.to_ascii_lowercase(), (1, n_a));
                    arrays.push((fname.clone(), n_a, string_elem));
                    n_a += 1;
                }
                ty if is_class_type(ty) => {
                    layout.fields.insert(fname.to_ascii_lowercase(), (2, n_s));
                    n_s += 1;
                }
                TypeRef::Named(t)
                    if t.eq_ignore_ascii_case("string") || t.eq_ignore_ascii_case("name") =>
                {
                    layout.fields.insert(fname.to_ascii_lowercase(), (2, n_s));
                    n_s += 1;
                }
                TypeRef::Named(t) if is_float_type_name(t) || vector_dim(t).is_some() => {
                    let dim = vector_dim(t).unwrap_or(1);
                    layout.fields.insert(fname.to_ascii_lowercase(), (3, n_f));
                    floats.push((fname.clone(), n_f, dim));
                    n_f += u16::from(dim);
                }
                TypeRef::Named(t)
                    if !is_scalar_vector(t) && type_names.contains(&t.to_ascii_lowercase()) =>
                {
                    layout.fields.insert(fname.to_ascii_lowercase(), (1, n_a));
                    objects.push((fname.clone(), n_a, t.to_ascii_lowercase()));
                    n_a += 1;
                }
                _ => {
                    layout.fields.insert(fname.to_ascii_lowercase(), (0, n_i));
                    n_i += 1;
                }
            }
        }
    }
    layout.n_i = n_i;
    layout.n_a = n_a;
    layout.n_s = n_s;
    layout.n_f = n_f;
    (layout, arrays, objects, floats)
}

struct CallTarget {
    key: String,
    self_reg: Option<u8>,
}

fn resolve_call(callee: &Expr, lx: &Lx) -> Option<CallTarget> {
    match callee {
        Expr::Member { obj, field, .. } => {
            if let Expr::Ident(name, _) = obj.as_ref() {
                // `Super.X()`: the parent's implementation, statically (no
                // virtual dispatch back into the override that is calling it).
                if name.eq_ignore_ascii_case("super") && object_local(lx, name).is_none() {
                    let parent = parent_of(lx, &lx.class_name)?;
                    let self_reg = if lx.instance { lx.self_reg } else { None };
                    return lookup_method(lx, &parent, field, self_reg);
                }
                if let Some((reg, class)) = object_local(lx, name) {
                    if let Some(target) = lookup_method(lx, &class, field, Some(reg)) {
                        return Some(target);
                    }
                }
                if lx.class_ids.contains_key(&name.to_ascii_lowercase()) {
                    return lookup_method(lx, name, field, None);
                }
                // `Level.X()` — the `level` global is a `LevelLocals`.
                if name.eq_ignore_ascii_case("level") {
                    if let Some(target) = lookup_method(lx, "LevelLocals", field, None) {
                        return Some(target);
                    }
                }
            }
            qualify(callee, &lx.class_name).map(|name| CallTarget {
                key: name.to_ascii_lowercase(),
                self_reg: None,
            })
        }
        Expr::Ident(name, _) => {
            let self_reg = if lx.instance { lx.self_reg } else { None };
            lookup_method(lx, &lx.class_name, name, self_reg).or_else(|| {
                let key = format!("{}.{}", lx.class_name, name).to_ascii_lowercase();
                let sig = lx.funcs.get(&key)?;
                let self_reg = if sig.instance { lx.self_reg } else { None };
                Some(CallTarget { key, self_reg })
            })
        }
        _ => None,
    }
}

fn emit_script_call(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
    want: u8,
) -> Option<u8> {
    let target = resolve_call(callee, lx)?;
    let sig = lx.funcs.get(&target.key)?.clone();
    if sig.ret != want {
        return None;
    }
    let user = if sig.instance {
        &sig.params[1..]
    } else {
        &sig.params[..]
    };
    // Nested calls take the arg stack, so evaluate every argument first.
    let mut prepared = Vec::new();
    if let Some(self_r) = target.self_reg {
        prepared.push((3u8, self_r));
    }
    let user = user.to_vec();
    prepared.extend(prepare_sig_args(f, lx, &sig, &user, args));
    for (bank, reg) in prepared {
        f.emit(Op::Arg, bank, reg, 0);
    }
    if sig.native {
        let id = lx.native_id(&target.key);
        f.emit_ext(Op::CallNat, 1, want, 0, id);
    } else {
        f.emit_ext(Op::Call, 1, 0, 0, sig.idx);
    }
    if want == 1 {
        let dest = lx.alloc_f();
        f.emit(Op::GetRet, 1, dest, 0);
        Some(dest)
    } else if want == 2 {
        let dest = lx.alloc_s();
        f.emit(Op::GetRet, 2, dest, 0);
        Some(dest)
    } else if want == 3 {
        let dest = lx.alloc_a();
        f.emit(Op::GetRet, 3, dest, 0);
        Some(dest)
    } else {
        let dest = lx.alloc();
        f.emit(Op::GetRet, 0, dest, 0);
        for &idx in &sig.outs {
            if let Some(Expr::Ident(name, _)) = args.get(idx) {
                if let Some(local) = int_local(lx, name) {
                    f.emit(Op::MoveI, local, dest, 0);
                }
            }
        }
        Some(dest)
    }
}

/// Arguments for a call to `sig`, by declared parameter: vectors as their
/// float components, `class<T>` of natives as names, others by bank.
///
/// A `vararg` native also gets the arguments past its declared ones, each
/// in the bank of its own type (string, float, else int), then, at the end
/// of the int bank, the bank of each extra in call order and their count:
/// `…extras…, bank₀, …, bankₙ₋₁, n`. The native rebuilds the ordered list
/// from that (`DTA_Alpha, 0.5, DTA_VirtualWidth, w`).
fn prepare_sig_args(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    sig: &FnSig,
    user: &[u8],
    args: &[Expr],
) -> Vec<(u8, u8)> {
    let mut prepared = Vec::new();
    for (k, (arg, bank)) in args.iter().zip(user.iter().copied()).enumerate() {
        let dim = sig.vec_params.get(k).copied().unwrap_or(0);
        if dim > 0 {
            let v = emit_expr_v(f, lx, arg, dim);
            for j in 0..dim {
                prepared.push((1, v + j));
            }
            continue;
        }
        if sig.native && sig.class_params.get(k).copied().unwrap_or(false) {
            prepared.push(class_name_arg(f, lx, arg));
            continue;
        }
        match bank {
            1 => prepared.push((1, emit_expr_f(f, arg, lx))),
            2 => prepared.push((2, emit_expr_s(f, arg, lx))),
            3 => prepared.push((3, emit_expr_a(f, arg, lx))),
            _ => prepared.push((0, emit_expr(f, arg, lx))),
        }
    }
    if sig.native && sig.varargs {
        let declared = sig.vec_params.len();
        let mut layout = Vec::new();
        for arg in args.iter().skip(declared) {
            let bank: u8 = if expr_is_string(arg, lx) {
                prepared.push((2, emit_expr_s(f, arg, lx)));
                2
            } else if fvec::expr_is_float(arg, lx) {
                prepared.push((1, emit_expr_f(f, arg, lx)));
                1
            } else {
                prepared.push((0, emit_expr(f, arg, lx)));
                0
            };
            layout.push(bank);
        }
        for bank in &layout {
            prepared.push((0, load_i(f, lx, i64::from(*bank))));
        }
        prepared.push((0, load_i(f, lx, layout.len() as i64)));
        return prepared;
    }
    // Left-out arguments take their declared defaults (`alpha = 1.`,
    // `scale = (1, 1)`), else the callee would see zeros. A default that does
    // not lower in the caller's scope is dropped (and stays zero).
    // Natives read their own defaults for missing arguments (they are shared
    // with DECORATE calls), so only script functions get them filled here.
    for k in args.len()..user.len() {
        if sig.native {
            break;
        }
        let Some(Some(default)) = sig.defaults.get(k) else {
            break;
        };
        let bank = user[k];
        let code_len = f.code.len();
        let diag_len = lx.diags.len();
        let mut part = Vec::new();
        let dim = sig.vec_params.get(k).copied().unwrap_or(0);
        if dim > 0 {
            let v = emit_expr_v(f, lx, default, dim);
            for j in 0..dim {
                part.push((1, v + j));
            }
        } else {
            part.push(match bank {
                1 => (1, emit_expr_f(f, default, lx)),
                2 => (2, emit_expr_s(f, default, lx)),
                3 => (3, emit_expr_a(f, default, lx)),
                _ => (0, emit_expr(f, default, lx)),
            });
        }
        if lx.diags.len() != diag_len {
            lx.diags.truncate(diag_len);
            f.code.truncate(code_len);
            break;
        }
        prepared.extend(part);
    }
    prepared
}

fn emit_field_method(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
    want: u8,
) -> Option<u8> {
    let Expr::Member { obj, field, .. } = callee else {
        return None;
    };
    let (recv, class) = object_base(f, obj, lx)?;
    let key = lookup_method(lx, &class, field, None)
        .map(|t| t.key)
        .unwrap_or_else(|| format!("{class}.{field}").to_ascii_lowercase());
    let sig = lx.funcs.get(&key)?.clone();
    if sig.ret != want {
        return None;
    }
    let user = if sig.instance {
        &sig.params[1..]
    } else {
        &sig.params[..]
    };
    let mut prepared = vec![(3u8, recv)];
    let user = user.to_vec();
    prepared.extend(prepare_sig_args(f, lx, &sig, &user, args));
    for (bank, reg) in prepared {
        f.emit(Op::Arg, bank, reg, 0);
    }
    if sig.native {
        let id = lx.native_id(&key);
        f.emit_ext(Op::CallNat, 1, want, 0, id);
    } else {
        f.emit_ext(Op::Call, 1, 0, 0, sig.idx);
    }
    match want {
        1 => {
            let dest = lx.alloc_f();
            f.emit(Op::GetRet, 1, dest, 0);
            Some(dest)
        }
        2 => {
            let dest = lx.alloc_s();
            f.emit(Op::GetRet, 2, dest, 0);
            Some(dest)
        }
        3 => {
            let dest = lx.alloc_a();
            f.emit(Op::GetRet, 3, dest, 0);
            Some(dest)
        }
        _ => {
            let dest = lx.alloc();
            f.emit(Op::GetRet, 0, dest, 0);
            Some(dest)
        }
    }
}

fn emit_builtin(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
    want: u8,
) -> Option<u8> {
    let (name, banks, ret): (&str, &[u8], u8) = match callee {
        Expr::Ident(n, _) if n.eq_ignore_ascii_case("find") => ("EventHandler.Find", &[2], 3),
        Expr::Member { obj, field, .. } => {
            let Expr::Ident(cls, _) = obj.as_ref() else {
                return None;
            };
            let key = format!("{cls}.{field}").to_ascii_lowercase();
            match key.as_str() {
                "wads.checknumforfullname" => ("Wads.CheckNumForFullName", &[2], 0),
                "wads.checknumforname" => ("Wads.CheckNumForName", &[2, 0], 0),
                "wads.findlump" => ("Wads.FindLump", &[2, 0], 0),
                "wads.readlump" => ("Wads.ReadLump", &[0], 2),
                "cvar.getcvar" => ("CVar.GetCVar", &[2, 3], 3),
                "eventhandler.find" => ("EventHandler.Find", &[2], 3),
                "object.new" => ("Object.New", &[2], 3),
                "console.printf" => ("Console.Printf", &[2], 0),
                _ => return None,
            }
        }
        _ => return None,
    };
    if ret != want {
        return None;
    }
    let mut prepared = Vec::new();
    for (arg, bank) in args.iter().zip(banks.iter().copied()) {
        let reg = match bank {
            2 => emit_expr_s(f, arg, lx),
            3 => emit_expr_a(f, arg, lx),
            _ => emit_expr(f, arg, lx),
        };
        prepared.push((bank, reg));
    }
    let id = lx.native_id(name);
    for (bank, reg) in prepared {
        f.emit(Op::Arg, bank, reg, 0);
    }
    f.emit_ext(Op::CallNat, 1, ret, 0, id);
    match ret {
        2 => {
            let dest = lx.alloc_s();
            f.emit(Op::GetRet, 2, dest, 0);
            Some(dest)
        }
        3 => {
            let dest = lx.alloc_a();
            f.emit(Op::GetRet, 3, dest, 0);
            Some(dest)
        }
        _ => {
            let dest = lx.alloc();
            f.emit(Op::GetRet, 0, dest, 0);
            Some(dest)
        }
    }
}

fn emit_new_expr(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
) -> Option<u8> {
    let Expr::Ident(name, _) = callee else {
        return None;
    };
    if !name.eq_ignore_ascii_case("new") {
        return None;
    }
    let class = match args.first() {
        Some(Expr::String(s, _)) | Some(Expr::Name(s, _)) => s.clone(),
        Some(Expr::Ident(name, _)) if lx.class_ids.contains_key(&name.to_ascii_lowercase()) => {
            name.clone()
        }
        Some(expr) => {
            let cls = emit_expr(f, expr, lx);
            let id = lx.native_id("object.new");
            f.emit(Op::Arg, 0, cls, 0);
            f.emit_ext(Op::CallNat, 1, 3, 0, id);
            let dest = lx.alloc_a();
            f.emit(Op::GetRet, 3, dest, 0);
            return Some(dest);
        }
        None => String::new(),
    };
    Some(emit_new(f, lx, &class))
}

fn emit_new(f: &mut CompiledFunction, lx: &mut Lx, class: &str) -> u8 {
    let dest = lx.alloc_a();
    match lx.class_ids.get(&class.to_ascii_lowercase()).copied() {
        Some(id) => f.emit_ext(Op::NewObj, dest, 0, 0, id),
        None => lx.err(
            crate::span::Span::default(),
            format!("unknown class {class}"),
        ),
    }
    dest
}

fn emit_expr_a(f: &mut CompiledFunction, expr: &Expr, lx: &mut Lx) -> u8 {
    if let Some((reg, _)) = object_base(f, expr, lx) {
        return reg;
    }
    match expr {
        Expr::Call { callee, args, .. } => {
            if let Some(class) = cast_target(callee, lx) {
                return emit_cast(f, lx, &class, args.first());
            }
            if let Some(dest) = emit_new_expr(f, lx, callee, args) {
                return dest;
            }
            if let Some(dest) = emit_script_call(f, lx, callee, args, 3) {
                return dest;
            }
            if let Some(dest) = emit_builtin(f, lx, callee, args, 3) {
                return dest;
            }
            if let Some(dest) = emit_field_method(f, lx, callee, args, 3) {
                return dest;
            }
            if let Some(dest) = emit_type_call(f, lx, callee, args, 3) {
                return dest;
            }
            0
        }
        Expr::Cast { ty, expr, .. } => emit_cast(f, lx, ty, Some(expr)),
        Expr::New { class, .. } => emit_new(f, lx, class),
        // `Font fnt = "BIGFONT";`: a name where an object is expected is a font.
        Expr::String(..) | Expr::Name(..) => {
            let name = emit_expr_s(f, expr, lx);
            f.emit(Op::Arg, 2, name, 0);
            let id = lx.native_id("Font.GetFont");
            f.emit_ext(Op::CallNat, 1, 3, 0, id);
            let dest = lx.alloc_a();
            f.emit(Op::GetRet, 3, dest, 0);
            dest
        }
        Expr::Ternary {
            cond,
            then_e,
            else_e,
            ..
        } => {
            let dest = lx.alloc_a();
            let c = emit_expr(f, cond, lx);
            let jelse = emit_jcc(f, Op::JmpIfNot, c);
            let t = emit_expr_a(f, then_e, lx);
            f.emit(Op::MoveA, dest, t, 0);
            let jend = emit_jcc(f, Op::Jmp, 0);
            patch(f, jelse);
            let e = emit_expr_a(f, else_e, lx);
            f.emit(Op::MoveA, dest, e, 0);
            patch(f, jend);
            dest
        }
        Expr::Binary { op, lhs, rhs, span } if op == "=" => {
            // `item = item.Inv` used as an object.
            emit_binary(f, lx, op, lhs, rhs, *span)
        }
        Expr::Ident(name, _span) => {
            if name.eq_ignore_ascii_case("null") || name.eq_ignore_ascii_case("nullptr") {
                let dest = lx.alloc_a();
                f.emit(Op::LoadNull, dest, 0, 0);
                return dest;
            }
            if let Some((r, _)) = array_local(lx, name) {
                return r;
            }
            if let Some((r, _)) = object_local(lx, name) {
                r
            } else {
                let dest = lx.alloc_a();
                f.emit(Op::LoadNull, dest, 0, 0);
                dest
            }
        }
        _ => {
            let dest = lx.alloc_a();
            f.emit(Op::LoadNull, dest, 0, 0);
            dest
        }
    }
}

fn cast_target(callee: &Expr, lx: &Lx) -> Option<String> {
    let Expr::Ident(name, _) = callee else {
        return None;
    };
    if lx.class_ids.contains_key(&name.to_ascii_lowercase()) {
        Some(name.clone())
    } else {
        None
    }
}

fn emit_cast(f: &mut CompiledFunction, lx: &mut Lx, class: &str, arg: Option<&Expr>) -> u8 {
    let dest = lx.alloc_a();
    let Some(arg) = arg else {
        f.emit(Op::LoadNull, dest, 0, 0);
        return dest;
    };
    let src = emit_expr_a(f, arg, lx);
    let Some(cid) = lx.class_ids.get(&class.to_ascii_lowercase()).copied() else {
        f.emit(Op::LoadNull, dest, 0, 0);
        return dest;
    };
    let flag = lx.alloc();
    f.emit_ext(Op::IsClass, flag, src, 0, cid);
    let jbad = emit_jcc(f, Op::JmpIfNot, flag);
    f.emit(Op::MoveA, dest, src, 0);
    let jend = emit_jcc(f, Op::Jmp, 0);
    patch(f, jbad);
    f.emit(Op::LoadNull, dest, 0, 0);
    patch(f, jend);
    dest
}

fn emit_is(f: &mut CompiledFunction, lx: &mut Lx, lhs: &Expr, rhs: &Expr) -> u8 {
    let class = match rhs {
        Expr::String(s, _) | Expr::Name(s, _) | Expr::Ident(s, _) => Some(s.clone()),
        _ => None,
    };
    if let Some(class) = class {
        let obj = emit_expr_a(f, lhs, lx);
        let Some(&cid) = lx.class_ids.get(&class.to_ascii_lowercase()) else {
            return load_i(f, lx, 0);
        };
        let flag = lx.alloc();
        f.emit_ext(Op::IsClass, flag, obj, 0, cid);
        return flag;
    }
    // `weapon is SisterWeapon.GetClass()` and `toucher is RestrictedToPlayerClass[i]`.
    let obj = emit_expr_a(f, lhs, lx);
    let cls = emit_expr(f, rhs, lx);
    let id = lx.native_id("object.isclass");
    f.emit(Op::Arg, 3, obj, 0);
    f.emit(Op::Arg, 0, cls, 0);
    f.emit_ext(Op::CallNat, 1, 0, 0, id);
    let dest = lx.alloc();
    f.emit(Op::GetRet, 0, dest, 0);
    dest
}

fn null_cmp(f: &mut CompiledFunction, lx: &mut Lx, op: &str, lhs: &Expr, rhs: &Expr) -> Option<u8> {
    if op != "==" && op != "!=" {
        return None;
    }
    let (obj, want_null) = match (lhs, rhs) {
        (e, Expr::Ident(n, _)) if n.eq_ignore_ascii_case("null") => (e, op == "=="),
        (Expr::Ident(n, _), e) if n.eq_ignore_ascii_case("null") => (e, op == "=="),
        _ => return None,
    };
    let Expr::Ident(name, _) = obj else {
        return None;
    };
    let reg = object_local(lx, name).map(|(r, _)| r)?;
    let is_null = lx.alloc();
    f.emit(Op::IsNull, is_null, reg, 0);
    if want_null {
        return Some(is_null);
    }
    let zero = load_i(f, lx, 0);
    let dest = lx.alloc();
    f.emit(Op::EqI, dest, is_null, zero);
    Some(dest)
}

fn emit_expr_f(f: &mut CompiledFunction, expr: &Expr, lx: &mut Lx) -> u8 {
    match expr {
        Expr::Float(v, _) => load_f(f, lx, *v),
        Expr::Int(v, _) => load_f(f, lx, *v as f64),
        Expr::Ident(name, span) => {
            if let Some(native) = host_int(name) {
                let v = emit_host_int(f, lx, native);
                return i2f(f, lx, v);
            }
            if let Some(&r) = lx
                .locals_f
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, r)| r)
            {
                r
            } else if let Some(r) = int_local(lx, name) {
                i2f(f, lx, r)
            } else if let Some(r) = try_emit_float_read(f, lx, expr) {
                r
            } else if let Some(dim) = expr_vector_dim(expr, lx) {
                emit_expr_v(f, lx, expr, dim)
            } else if let (Some(slot), Some(self_reg)) = (
                self_int_slot(lx, name).or_else(|| declaring_int_slot(lx, name)),
                lx.self_reg,
            ) {
                let cur = lx.alloc();
                f.emit_ext(Op::LoadFieldI, cur, self_reg, 0, u32::from(slot));
                i2f(f, lx, cur)
            } else if let Some(&v) = lx.consts.get(&name.to_ascii_lowercase()) {
                load_f(f, lx, v as f64)
            } else if let Some(&v) = lx.float_consts.get(&name.to_ascii_lowercase()) {
                load_f(f, lx, v)
            } else if let Some((reg, _)) = object_local(lx, name) {
                let bit = truthy(f, lx, reg);
                i2f(f, lx, bit)
            } else if is_cvar_name(name) {
                let v = emit_host_int(f, lx, &name.to_ascii_lowercase());
                i2f(f, lx, v)
            } else {
                lx.err(*span, format!("unknown float {name}"));
                load_f(f, lx, 0.0)
            }
        }
        Expr::Member { obj, field, .. } => {
            if let Expr::Ident(cls, _) = obj.as_ref() {
                let field_l = field.to_ascii_lowercase();
                let mut cur = Some(cls.to_ascii_lowercase());
                for _ in 0..64 {
                    let Some(name) = cur else { break };
                    let key = format!("{name}.{field_l}");
                    if let Some(&v) = lx.float_consts.get(&key) {
                        return load_f(f, lx, v);
                    }
                    if let Some(&v) = lx.consts.get(&key) {
                        return load_f(f, lx, v as f64);
                    }
                    cur = lx.parents.get(&name).filter(|p| !p.is_empty()).cloned();
                }
            }
            if let Some(r) = try_emit_float_read(f, lx, expr) {
                return r;
            }
            if let Some(dim) = expr_vector_dim(expr, lx) {
                return emit_expr_v(f, lx, expr, dim);
            }
            if let Expr::Ident(name, _) = obj.as_ref() {
                if is_vector_axis(field) {
                    if let Some(r) = lx
                        .locals_f
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(name))
                        .map(|(_, r)| *r)
                    {
                        return r;
                    }
                }
            }
            if let Some(reg) = load_vector_component(f, lx, obj, field) {
                return i2f(f, lx, reg);
            }
            if let Some((reg, class)) = object_base(f, obj, lx) {
                if let Some(slot) =
                    lookup_int_field(lx, &class, field).or_else(|| declaring_int_slot(lx, field))
                {
                    let cur = lx.alloc();
                    f.emit_ext(Op::LoadFieldI, cur, reg, 0, u32::from(slot));
                    return i2f(f, lx, cur);
                }
            }
            lx.err(expr.span(), "float expression");
            load_f(f, lx, 0.0)
        }
        Expr::Cast { expr, .. } => emit_expr_f(f, expr, lx),
        Expr::Unary { op, expr, .. } if op == "+" => emit_expr_f(f, expr, lx),
        Expr::Unary { op, expr, .. } if op == "-" => {
            let r = emit_expr_f(f, expr, lx);
            let z = load_f(f, lx, 0.0);
            let dest = lx.alloc_f();
            f.emit(Op::SubF, dest, z, r);
            dest
        }
        Expr::Call { callee, args, .. } if try_float_call(callee, args, lx) => {
            try_emit_float_read(f, lx, expr).unwrap_or(0)
        }
        Expr::Call { callee, args, .. } => {
            if let Some(dest) = emit_script_call(f, lx, callee, args, 1) {
                return dest;
            }
            if let Some(dest) = emit_script_call(f, lx, callee, args, 0) {
                return i2f(f, lx, dest);
            }
            if let Some(dest) = emit_field_method(f, lx, callee, args, 0) {
                return i2f(f, lx, dest);
            }
            if let Some(dest) = emit_type_call(f, lx, callee, args, 0) {
                return i2f(f, lx, dest);
            }
            if let Some(dest) = emit_type_call(f, lx, callee, args, 1) {
                return dest;
            }
            if let Expr::Ident(name, _) = callee.as_ref() {
                return emit_math_call(f, lx, name, args);
            }
            if let Some(name) = rng_call_name(callee) {
                return emit_math_call(f, lx, &name, args);
            }
            emit_unresolved_call(f, lx, callee, args, 1)
        }
        Expr::Binary { op, lhs, rhs, .. }
            if (op == "*" || op.eq_ignore_ascii_case("dot"))
                && expr_vector_dim(lhs, lx).is_some()
                && expr_vector_dim(rhs, lx).is_some() =>
        {
            emit_dot(f, lx, lhs, rhs)
        }
        Expr::Binary { op, lhs, rhs, span } if op == "=" => {
            emit_assign_value_f(f, lx, lhs, rhs, *span)
        }
        Expr::Binary { op, lhs, rhs, span } => {
            let code = match op.as_str() {
                "+" => Op::AddF,
                "-" => Op::SubF,
                "*" => Op::MulF,
                "/" => Op::DivF,
                "+=" | "-=" | "*=" | "/=" => {
                    let _ = emit_binary(f, lx, op, lhs, rhs, *span);
                    if let Expr::Ident(name, _) = lhs.as_ref() {
                        if let Some(r) = lx
                            .locals_f
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(name))
                            .map(|(_, r)| *r)
                        {
                            return r;
                        }
                    }
                    return load_f(f, lx, 0.0);
                }
                "**" => {
                    let l = emit_expr_f(f, lhs, lx);
                    let r = emit_expr_f(f, rhs, lx);
                    let id = lx.native_id("math.pow");
                    f.emit(Op::Arg, 1, l, 0);
                    f.emit(Op::Arg, 1, r, 0);
                    f.emit_ext(Op::CallNat, 1, 1, 0, id);
                    let dest = lx.alloc_f();
                    f.emit(Op::GetRet, 1, dest, 0);
                    return dest;
                }
                "<<" | ">>" | "&" | "|" | "^" => {
                    let v = emit_binary(f, lx, op, lhs, rhs, *span);
                    return i2f(f, lx, v);
                }
                "%" => {
                    if !expr_is_float(lhs, lx) && !expr_is_float(rhs, lx) {
                        let v = emit_binary(f, lx, op, lhs, rhs, *span);
                        return i2f(f, lx, v);
                    }
                    return emit_fmod(f, lx, lhs, rhs);
                }
                "<" | "<=" | ">" | ">=" | "==" | "!=" | "&&" | "||" => {
                    let v = emit_binary(f, lx, op, lhs, rhs, *span);
                    return i2f(f, lx, v);
                }
                _ => {
                    lx.err(*span, format!("float op {op}"));
                    return load_f(f, lx, 0.0);
                }
            };
            let l = emit_expr_f(f, lhs, lx);
            let r = emit_expr_f(f, rhs, lx);
            let dest = lx.alloc_f();
            f.emit(code, dest, l, r);
            dest
        }
        Expr::Ternary {
            cond,
            then_e,
            else_e,
            ..
        } => {
            let dest = lx.alloc_f();
            let c = emit_expr(f, cond, lx);
            let jelse = emit_jcc(f, Op::JmpIfNot, c);
            let t = emit_expr_f(f, then_e, lx);
            f.emit(Op::MoveF, dest, t, 0);
            let jend = emit_jcc(f, Op::Jmp, 0);
            patch(f, jelse);
            let e = emit_expr_f(f, else_e, lx);
            f.emit(Op::MoveF, dest, e, 0);
            patch(f, jend);
            dest
        }
        Expr::Index { arr, index, span } => {
            let value = emit_index_i(f, arr, index, lx, *span);
            i2f(f, lx, value)
        }
        Expr::Tuple { elems, .. } => {
            let mut out = load_f(f, lx, 0.0);
            for (i, elem) in elems.iter().enumerate() {
                let v = emit_expr_f(f, elem, lx);
                if i == 0 {
                    out = v;
                }
            }
            out
        }
        other => {
            lx.err(other.span(), "float expression");
            load_f(f, lx, 0.0)
        }
    }
}

/// A `class<T>` argument to a native: a literal or class name goes in the
/// string bank; anything else passes its runtime class id.
fn class_name_arg(f: &mut CompiledFunction, lx: &mut Lx, arg: &Expr) -> (u8, u8) {
    match arg {
        Expr::String(name, _) | Expr::Name(name, _) => (2, load_s(f, lx, name)),
        Expr::Ident(name, _) if lx.class_ids.contains_key(&name.to_ascii_lowercase()) => {
            (2, load_s(f, lx, name))
        }
        expr if expr_yields_string(expr, lx) => (2, emit_expr_s(f, expr, lx)),
        _ => (0, emit_expr(f, arg, lx)),
    }
}

/// A bare constant as the current class (or an ancestor) declares it, before
/// any same-named constant of another class.
fn scoped_const(lx: &Lx, name: &str) -> Option<i64> {
    let mut cur = Some(lx.class_name.to_ascii_lowercase());
    for _ in 0..64 {
        let class = cur?;
        if let Some(&v) = lx.consts.get(&format!("{class}.{name}").to_ascii_lowercase()) {
            return Some(v);
        }
        cur = parent_of(lx, &class);
    }
    None
}

/// Result banks of the function or native `callee` names.
fn call_ret_banks(callee: &Expr, lx: &Lx) -> Option<Vec<u8>> {
    if let Some(t) = resolve_call(callee, lx) {
        if let Some(sig) = lx.funcs.get(&t.key) {
            return Some(sig.ret_banks.clone());
        }
    }
    let Expr::Member { obj, field, .. } = callee else {
        return None;
    };
    let class = match obj.as_ref() {
        Expr::Ident(n, _) => object_local(lx, n)
            .map(|(_, c)| c)
            .or_else(|| lookup_object_field(lx, &lx.class_name, n).map(|(_, c)| c)),
        _ => None,
    }?;
    let t = lookup_method(lx, &class, field, None)?;
    lx.funcs.get(&t.key).map(|sig| sig.ret_banks.clone())
}

/// `[a, b] = F();`: each name takes one result. Existing locals are assigned;
/// new names become locals of the result's bank.
fn emit_destructure(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    names: &[String],
    init: &Expr,
    callee: &Expr,
) {
    let banks = call_ret_banks(callee, lx).unwrap_or_default();
    let bank_of = |k: usize| banks.get(k).copied().unwrap_or(0);
    // The first result comes back through the call itself.
    let first = match bank_of(0) {
        1 => emit_expr_f(f, init, lx),
        2 => emit_expr_s(f, init, lx),
        3 => emit_expr_a(f, init, lx),
        _ => emit_expr(f, init, lx),
    };
    // Read the rest before anything else can call.
    let mut values = vec![(bank_of(0), first)];
    for k in 1..names.len() {
        let bank = bank_of(k);
        let reg = match bank {
            1 => lx.alloc_f(),
            2 => lx.alloc_s(),
            3 => lx.alloc_a(),
            _ => lx.alloc(),
        };
        f.emit(Op::GetRet, bank, reg, u8::try_from(k).unwrap_or(7));
        values.push((bank, reg));
    }
    for (name, (bank, reg)) in names.iter().zip(values) {
        if name == "_" || name.is_empty() || fvec::vector_local(lx, name).is_some() {
            // Vector results are not carried through multi-returns yet.
            continue;
        }
        if let Some(r) = int_local(lx, name) {
            let v = if bank == 1 { f2i(f, lx, reg) } else { reg };
            f.emit(Op::MoveI, r, v, 0);
        } else if let Some(r) = fvec::float_local(lx, name) {
            let v = if bank == 1 { reg } else { i2f(f, lx, reg) };
            f.emit(Op::MoveF, r, v, 0);
        } else if let Some(&r) = lx.locals_s.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, r)| r) {
            f.emit(Op::MoveS, r, reg, 0);
        } else if let Some((r, _)) = object_local(lx, name) {
            f.emit(Op::MoveA, r, reg, 0);
        } else {
            match bank {
                1 => {
                    lx.locals_f.insert(name.clone(), reg);
                }
                2 => {
                    lx.locals_s.insert(name.clone(), reg);
                }
                3 => {
                    lx.locals_o.insert(name.clone(), (reg, "object".to_owned()));
                }
                _ => {
                    lx.locals.insert(name.clone(), reg);
                }
            }
        }
    }
}

/// `v.Length()` and friends handled by [`fvec`].
fn try_float_call(callee: &Expr, args: &[Expr], lx: &Lx) -> bool {
    let Expr::Member { obj, field, .. } = callee else {
        return false;
    };
    args.is_empty() && field.eq_ignore_ascii_case("length") && expr_vector_dim(obj, lx).is_some()
}

fn emit_math_call(f: &mut CompiledFunction, lx: &mut Lx, name: &str, args: &[Expr]) -> u8 {
    let mut prepared = Vec::new();
    for arg in args {
        prepared.push(emit_expr_f(f, arg, lx));
    }
    let id = lx.native_id(&format!("math.{}", name.to_ascii_lowercase()));
    for reg in prepared {
        f.emit(Op::Arg, 1, reg, 0);
    }
    f.emit_ext(Op::CallNat, 1, 1, 0, id);
    let dest = lx.alloc_f();
    f.emit(Op::GetRet, 1, dest, 0);
    dest
}

fn f2i(f: &mut CompiledFunction, lx: &mut Lx, src: u8) -> u8 {
    let dest = lx.alloc();
    f.emit(Op::F2I, dest, src, 0);
    dest
}

fn i2f(f: &mut CompiledFunction, lx: &mut Lx, src: u8) -> u8 {
    let dest = lx.alloc_f();
    f.emit(Op::I2F, dest, src, 0);
    dest
}

fn load_f(f: &mut CompiledFunction, lx: &mut Lx, v: f64) -> u8 {
    let r = lx.alloc_f();
    let idx = f.intern_f(v);
    f.emit_ext(Op::LoadConstF, r, 0, 0, idx);
    r
}

fn emit_array_store(f: &mut CompiledFunction, lx: &mut Lx, arr: &Expr, index: &Expr, rhs: &Expr) {
    let Some((reg, string_elem)) = array_expr(f, lx, arr) else {
        match arr {
            Expr::Ident(name, _) => lx.err(arr.span(), format!("unknown array {name}")),
            _ => lx.err(arr.span(), "array store"),
        }
        return;
    };
    let idx = emit_expr(f, index, lx);
    if string_elem {
        let v = emit_expr_s(f, rhs, lx);
        f.emit(Op::ArraySetS, reg, idx, v);
    } else {
        let v = emit_expr(f, rhs, lx);
        f.emit(Op::ArraySetI, reg, idx, v);
    }
}

fn emit_inc(f: &mut CompiledFunction, expr: &Expr, lx: &mut Lx, dec: bool, post: bool) -> u8 {
    if let Some(r) = try_emit_float_inc(f, lx, expr, dec, post) {
        return r;
    }
    // Local register, or an int field `(object, slot)` to load and store.
    let target: Result<u8, (u8, u16)> = match expr {
        Expr::Ident(name, span) => {
            if let Some(r) = int_local(lx, name) {
                Ok(r)
            } else if let Some(r) = lx
                .locals_f
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, r)| *r)
            {
                let old = if post {
                    let old = lx.alloc_f();
                    f.emit(Op::MoveF, old, r, 0);
                    Some(old)
                } else {
                    None
                };
                let one = load_f(f, lx, 1.0);
                let dest = lx.alloc_f();
                f.emit(if dec { Op::SubF } else { Op::AddF }, dest, r, one);
                f.emit(Op::MoveF, r, dest, 0);
                return old.unwrap_or(r);
            } else if let (Some(slot), Some(self_reg)) = (self_int_slot(lx, name), lx.self_reg) {
                Err((self_reg, slot))
            } else {
                lx.err(*span, format!("unknown name {name}"));
                return 0;
            }
        }
        Expr::Index { arr, index, .. } => {
            let Some((reg, string_elem)) = array_expr(f, lx, arr) else {
                lx.err(expr.span(), "++ target");
                return 0;
            };
            if string_elem {
                lx.err(expr.span(), "++ target");
                return 0;
            }
            let idx = emit_expr(f, index, lx);
            let cur = lx.alloc();
            f.emit(Op::ArrayGetI, cur, reg, idx);
            let old = if post {
                let old = lx.alloc();
                f.emit(Op::MoveI, old, cur, 0);
                Some(old)
            } else {
                None
            };
            let one = load_i(f, lx, 1);
            let dest = lx.alloc();
            f.emit(if dec { Op::SubI } else { Op::AddI }, dest, cur, one);
            f.emit(Op::ArraySetI, reg, idx, dest);
            return old.unwrap_or(dest);
        }
        Expr::Member { obj, field, .. } => {
            let Some((reg, class)) = object_base(f, obj, lx) else {
                lx.err(expr.span(), "++ target");
                return 0;
            };
            let Some(slot) =
                lookup_int_field(lx, &class, field).or_else(|| declaring_int_slot(lx, field))
            else {
                lx.err(expr.span(), "++ target");
                return 0;
            };
            Err((reg, slot))
        }
        _ => {
            lx.err(expr.span(), "++ target");
            return 0;
        }
    };
    let (cur, store) = match target {
        Ok(r) => (r, None),
        Err((reg, slot)) => {
            let cur = lx.alloc();
            f.emit_ext(Op::LoadFieldI, cur, reg, 0, u32::from(slot));
            (cur, Some((reg, slot)))
        }
    };
    let old = if post {
        let old = lx.alloc();
        f.emit(Op::MoveI, old, cur, 0);
        Some(old)
    } else {
        None
    };
    let one = load_i(f, lx, 1);
    let dest = if store.is_some() { lx.alloc() } else { cur };
    if dec {
        f.emit(Op::SubI, dest, cur, one);
    } else {
        f.emit(Op::AddI, dest, cur, one);
    }
    if let Some((reg, slot)) = store {
        f.emit_ext(Op::StoreFieldI, reg, dest, 0, u32::from(slot));
    }
    old.unwrap_or(dest)
}

fn int_assign_op(op: &str) -> Option<Op> {
    Some(match op {
        "+=" => Op::AddI,
        "-=" => Op::SubI,
        "*=" => Op::MulI,
        "/=" => Op::DivI,
        "%=" => Op::ModI,
        "|=" => Op::BitOr,
        "&=" => Op::BitAnd,
        "^=" => Op::BitXor,
        "<<=" => Op::Shl,
        ">>=" => Op::Shr,
        _ => return None,
    })
}

fn emit_binary(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    op: &str,
    lhs: &Expr,
    rhs: &Expr,
    span: crate::span::Span,
) -> u8 {
    if op == "," {
        let _ = emit_expr(f, lhs, lx);
        return emit_expr(f, rhs, lx);
    }
    if let Some(r) = null_cmp(f, lx, op, lhs, rhs) {
        return r;
    }
    if let Some(r) = string_cmp(f, lx, op, lhs, rhs) {
        return r;
    }
    if op == "||" || op == "&&" {
        return emit_logic(f, lx, op, lhs, rhs);
    }
    if op == "is" {
        return emit_is(f, lx, lhs, rhs);
    }
    if op == "<=>" || op == "<>=" {
        let left = emit_expr_f(f, lhs, lx);
        let right = emit_expr_f(f, rhs, lx);
        let result = lx.alloc();
        f.emit(Op::CmpF, result, left, right);
        return result;
    }
    if (op == "=" || int_assign_op(op).is_some()) && try_emit_float_assign(f, lx, op, lhs, rhs) {
        return 0;
    }
    if let Some(r) = try_emit_float_compare(f, lx, op, lhs, rhs) {
        return r;
    }
    if matches!(op, "+" | "-" | "*" | "/")
        && (expr_is_float(lhs, lx)
            || expr_is_float(rhs, lx)
            || (op == "*" && expr_vector_dim(lhs, lx).is_some() && expr_vector_dim(rhs, lx).is_some()))
    {
        let whole = Expr::Binary {
            op: op.to_owned(),
            lhs: Box::new(lhs.clone()),
            rhs: Box::new(rhs.clone()),
            span,
        };
        let r = emit_expr_f(f, &whole, lx);
        return f2i(f, lx, r);
    }
    if op == "=" || int_assign_op(op).is_some() {
        if let Expr::Index { arr, index, .. } = lhs {
            if op == "=" {
                emit_array_store(f, lx, arr, index, rhs);
                return 0;
            }
            if let Some((reg, string_elem)) = array_expr(f, lx, arr) {
                if !string_elem {
                    if let Some(code) = int_assign_op(op) {
                        let idx = emit_expr(f, index, lx);
                        let cur = lx.alloc();
                        f.emit(Op::ArrayGetI, cur, reg, idx);
                        let rv = emit_expr(f, rhs, lx);
                        let dest = lx.alloc();
                        f.emit(code, dest, cur, rv);
                        f.emit(Op::ArraySetI, reg, idx, dest);
                        return 0;
                    }
                }
            }
            lx.err(lhs.span(), "assignment target");
            return 0;
        }
        if op == "=" {
            if let Expr::Ident(name, _) = lhs {
                // `if ((inv = Key(…)))`: the assignment's value as a truth
                // value (the result register is an int).
                if let Some((dest, _)) = object_local(lx, name) {
                    let v = emit_expr_a(f, rhs, lx);
                    if v != dest {
                        f.emit(Op::MoveA, dest, v, 0);
                    }
                    return truthy(f, lx, dest);
                }
                if let Some(dest) = lx
                    .locals_s
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, r)| *r)
                {
                    let v = emit_expr_s(f, rhs, lx);
                    if v != dest {
                        f.emit(Op::MoveS, dest, v, 0);
                    }
                    return string_truthy(f, lx, dest);
                }
            }
        }
        if let Expr::Member { obj, field, .. } = lhs {
            if emit_field_update(f, lx, obj, field, op, rhs) {
                return 0;
            }
        }
        let Expr::Ident(name, span) = lhs else {
            lx.err(lhs.span(), "assignment target");
            return 0;
        };
        if let Some(dest) = lx
            .locals_f
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, r)| *r)
        {
            let rv = emit_expr_f(f, rhs, lx);
            if op == "=" {
                if rv != dest {
                    f.emit(Op::MoveF, dest, rv, 0);
                }
            } else {
                let code = match op {
                    "+=" => Op::AddF,
                    "-=" => Op::SubF,
                    "*=" => Op::MulF,
                    "/=" => Op::DivF,
                    _ => {
                        lx.err(*span, format!("float assign {op}"));
                        return dest;
                    }
                };
                f.emit(code, dest, dest, rv);
            }
            return dest;
        }
        let Some(dest) = int_local(lx, name) else {
            if op == "=" {
                if let (Some(slot), Some(self_reg)) = (self_object_slot(lx, name), lx.self_reg) {
                    let v = emit_expr_a(f, rhs, lx);
                    f.emit_ext(Op::StoreFieldA, self_reg, v, 0, u32::from(slot));
                    return 0;
                }
            }
            if let (Some(slot), Some(self_reg)) = (self_int_slot(lx, name), lx.self_reg) {
                let rv = emit_expr(f, rhs, lx);
                if op == "=" {
                    f.emit_ext(Op::StoreFieldI, self_reg, rv, 0, u32::from(slot));
                    return rv;
                }
                let cur = lx.alloc();
                f.emit_ext(Op::LoadFieldI, cur, self_reg, 0, u32::from(slot));
                let dest = lx.alloc();
                if let Some(code) = int_assign_op(op) {
                    f.emit(code, dest, cur, rv);
                }
                f.emit_ext(Op::StoreFieldI, self_reg, dest, 0, u32::from(slot));
                return dest;
            }
            if op == "=" {
                if emit_bflag(f, lx, name, Some(rhs)).is_some() {
                    return 0;
                }
                if let (Some(slot), Some(self_reg)) = (
                    lookup_string_field(lx, &lx.class_name, name)
                        .or_else(|| declaring_string_slot(lx, name)),
                    lx.self_reg,
                ) {
                    let v = emit_expr_s(f, rhs, lx);
                    f.emit_ext(Op::StoreFieldS, self_reg, v, 0, u32::from(slot));
                    return 0;
                }
            }
            if is_bflag(name) {
                if let Some(cur) = emit_bflag(f, lx, name, None) {
                    let rv = emit_expr(f, rhs, lx);
                    let dest = lx.alloc();
                    if let Some(code) = int_assign_op(op) {
                        f.emit(code, dest, cur, rv);
                    } else {
                        f.emit(Op::MoveI, dest, rv, 0);
                    }
                    let self_reg = lx.self_reg.unwrap_or(0);
                    let flag = load_s(f, lx, &name[1..]);
                    let id = lx.native_id("Actor.ChangeFlag");
                    f.emit(Op::Arg, 3, self_reg, 0);
                    f.emit(Op::Arg, 2, flag, 0);
                    f.emit(Op::Arg, 0, dest, 0);
                    f.emit_ext(Op::CallNat, 1, 0, 0, id);
                    return dest;
                }
            }
            lx.err(*span, format!("unknown name {name}"));
            return 0;
        };
        let rv = emit_expr(f, rhs, lx);
        if op == "=" {
            f.emit(Op::MoveI, dest, rv, 0);
        } else if let Some(code) = int_assign_op(op) {
            f.emit(code, dest, dest, rv);
        }
        return dest;
    }
    if op == ".." {
        let l = emit_expr_s(f, lhs, lx);
        let r = emit_expr_s(f, rhs, lx);
        let dest = lx.alloc_s();
        f.emit(Op::ConcatS, dest, l, r);
        return string_truthy(f, lx, dest);
    }
    if op == "**" {
        let l = emit_expr_f(f, lhs, lx);
        let r = emit_expr_f(f, rhs, lx);
        let id = lx.native_id("math.pow");
        f.emit(Op::Arg, 1, l, 0);
        f.emit(Op::Arg, 1, r, 0);
        f.emit_ext(Op::CallNat, 1, 1, 0, id);
        let dest = lx.alloc_f();
        f.emit(Op::GetRet, 1, dest, 0);
        return f2i(f, lx, dest);
    }
    let l = emit_expr(f, lhs, lx);
    let r = emit_expr(f, rhs, lx);
    let d = lx.alloc();
    let op_code = match op {
        "+" => Op::AddI,
        "-" => Op::SubI,
        "*" => Op::MulI,
        "/" => Op::DivI,
        "%" => Op::ModI,
        "<" => Op::LtI,
        "<=" => Op::LeI,
        "==" => Op::EqI,
        "!=" => Op::NeI,
        ">" => Op::GtI,
        ">=" => Op::GeI,
        "&" => Op::BitAnd,
        "|" => Op::BitOr,
        "^" => Op::BitXor,
        "<<" => Op::Shl,
        ">>" => Op::Shr,
        "~==" => Op::EqI,
        _ => {
            lx.err(span, format!("operator {op}"));
            return d;
        }
    };
    f.emit(op_code, d, l, r);
    d
}

fn emit_switch(f: &mut CompiledFunction, cond: &Expr, body: &Block, lx: &mut Lx) {
    let disc = emit_expr(f, cond, lx);
    let saved = lx.alloc();
    f.emit(Op::MoveI, saved, disc, 0);
    lx.jumps.push(Jump {
        breaks: Vec::new(),
        continues: Vec::new(),
        is_loop: false,
    });
    let stmts = &body.stmts;
    let mut cases: Vec<(usize, usize, usize)> = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    for (i, s) in stmts.iter().enumerate() {
        if let Stmt::Case { .. } = s {
            if let Some((case_i, start)) = open.take() {
                cases.push((case_i, start, i));
            }
            open = Some((i, i + 1));
        }
    }
    if let Some((case_i, start)) = open {
        cases.push((case_i, start, stmts.len()));
    }
    let mut jumps: Vec<(usize, usize)> = Vec::new();
    let mut default_i: Option<usize> = None;
    for (ci, (case_i, _, _)) in cases.iter().copied().enumerate() {
        match &stmts[case_i] {
            Stmt::Case { value: Some(v), .. } => {
                let mark = lx.next;
                let cv = emit_expr(f, v, lx);
                let eq = lx.alloc();
                f.emit(Op::EqI, eq, saved, cv);
                jumps.push((emit_jcc(f, Op::JmpIf, eq), ci));
                lx.next = mark;
            }
            Stmt::Case { value: None, .. } => default_i = Some(ci),
            _ => {}
        }
    }
    let miss = emit_jcc(f, Op::Jmp, 0);
    let mut starts = vec![0usize; cases.len()];
    for (ci, (_, from, to)) in cases.iter().copied().enumerate() {
        starts[ci] = f.code.len();
        for s in &stmts[from..to] {
            emit_stmt_scoped(f, s, lx);
        }
    }
    let end = f.code.len();
    for (at, ci) in jumps {
        patch_to(f, at, starts[ci]);
    }
    patch_to(f, miss, default_i.map(|i| starts[i]).unwrap_or(end));
    end_loop(f, lx, end, end);
}

fn qualify(callee: &Expr, class_name: &str) -> Option<String> {
    match callee {
        Expr::Member { obj, field, .. } => {
            let Expr::Ident(class, _) = obj.as_ref() else {
                return None;
            };
            Some(format!("{class}.{field}"))
        }
        Expr::Ident(name, _) => Some(format!("{class_name}.{name}")),
        _ => None,
    }
}

fn load_i(f: &mut CompiledFunction, lx: &mut Lx, v: i64) -> u8 {
    let r = lx.alloc();
    let idx = f.intern_i(v);
    f.emit_ext(Op::LoadConstI, r, 0, 0, idx);
    r
}

fn emit_jcc(f: &mut CompiledFunction, op: Op, reg: u8) -> usize {
    let at = f.code.len();
    f.emit_ext(op, reg, 0, 0, 0);
    at
}

fn patch(f: &mut CompiledFunction, at: usize) {
    patch_to(f, at, f.code.len());
}

fn patch_to(f: &mut CompiledFunction, at: usize, target: usize) {
    let after = at + 2;
    f.code[at + 1] = (target as isize - after as isize) as u32;
}

/// Patch and pop the current jump frame.
fn end_loop(f: &mut CompiledFunction, lx: &mut Lx, break_pc: usize, continue_pc: usize) {
    if let Some(frame) = lx.jumps.pop() {
        for at in frame.breaks {
            patch_to(f, at, break_pc);
        }
        for at in frame.continues {
            patch_to(f, at, continue_pc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analyze, parse_sources};
    use room_zvm::Zvm;

    fn run(src: &str, name: &str, args: &[i64]) -> i64 {
        let parsed = parse_sources(&[("p.zs".into(), src.into())]);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let analysis = analyze(&parsed.unit).expect("sema");
        let prog = lower(&parsed.unit, &analysis).expect("lower");
        let mut vm = Zvm::new(prog);
        vm.call_with(name, args, &[])
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    #[test]
    fn vector_params_and_left_out_defaults_reach_the_callee() {
        let src = r#"
class V
{
    static int Pack(Vector2 p, int k = 3, double a = 1.5, Vector2 s = (7, 8))
    {
        double v = p.x * 100000 + p.y * 10000 + k * 1000 + a * 100 + s.x * 10 + s.y;
        return int(v);
    }
    static int AllDefaults() { return Pack((4, 2)); }
    static int SomeGiven() { return Pack((1, 0), 9, 0.5); }
    static int Temps()
    {
        int t = 0;
        for (int i = 0; i < 3; i++) t += Pack((0, 0), i);
        int untouched;
        return t + untouched;
    }
}
"#;
        assert_eq!(run(src, "V.AllDefaults", &[]), 400000 + 20000 + 3000 + 150 + 78);
        assert_eq!(run(src, "V.SomeGiven", &[]), 100000 + 9000 + 50 + 78);
        // Three calls, k = 0, 1, 2; a fresh local starts at 0.
        assert_eq!(run(src, "V.Temps", &[]), 3 * 228 + 3000);
    }

    #[test]
    fn multi_return_fills_every_name() {
        let src = r#"
class M
{
    static int, int Pair(int a) { return a + 1, a * 10; }
    static double, int Mixed() { return 0.5, 7; }
    static int Go()
    {
        int x, y;
        [x, y] = Pair(4);
        double h;
        int n;
        [h, n] = Mixed();
        if (h != 0.5) return -1;
        return x * 1000 + y + n * 100000;
    }
    static int Fresh()
    {
        [a, b] = Pair(1);
        return a * 10 + b;
    }
}
"#;
        assert_eq!(run(src, "M.Go", &[]), 5000 + 40 + 700000);
        assert_eq!(run(src, "M.Fresh", &[]), 2 * 10 + 10);
    }

    #[test]
    fn class_values_travel_as_names() {
        let src = r#"
class K
{
    static int F(class<Object> t) { if (t == NULL) return 1; if (t == "Clip") return 3; return 2; }
    static int G() { return F("Clip"); }
    static int H() { return F(null); }
}
"#;
        assert_eq!(run(src, "K.G", &[]), 3);
        assert_eq!(run(src, "K.H", &[]), 1);
    }

    #[test]
    fn subclass_fields_do_not_overlap_parent_fields() {
        let src = r#"
class A { int a; double fa; }
class B : A
{
    int b;
    double fb;
    static int Go()
    {
        B o = new("B");
        o.a = 1; o.b = 2; o.fa = 0.5; o.fb = 0.25;
        return o.a * 1000 + o.b * 100 + int(o.fa * 10) * 10 + int(o.fb * 4);
    }
}
"#;
        assert_eq!(run(src, "B.Go", &[]), 1251);
    }

    #[test]
    fn float_and_vector_fields_keep_fractions_and_components() {
        let src = r#"
class Box
{
    double speed;
    vector3 v;
    vector2 p;
    int n;

    void Push(double by) { speed += by; v.z = speed * 2; p = (v.x, 0.25); }

    static int Comp()
    {
        Box b = new("Box");
        b.v = (1.5, -2.25, 3.0);
        b.speed = 0.75;
        b.Push(0.5);
        // speed 1.25, v.z 2.5, p (1.5, 0.25)
        return int(b.speed * 100) + int(b.v.z * 1000) + int(b.p.y * 100000);
    }

    static int Vec()
    {
        Box b = new("Box");
        b.v = (3, 4, 0);
        vector3 d = b.v * 2 - (1, 1, 1);
        d.x += 0.5;
        b.v += d;
        // d (5.5, 7, -1), b.v (8.5, 11, -1)
        return int(b.v.x * 10) * 10000 + int(b.v.y) * 100 + int(-b.v.z);
    }

    static double Len()
    {
        vector2 a = (3, 4);
        vector3 c = (1, 2, 2);
        return a.Length() + c.Length() + (a * (1, 1));
    }

    static int Cmp()
    {
        Box b = new("Box");
        b.speed = 0.5;
        int r = 0;
        if (b.speed > 0.25) r += 1;
        if (b.speed < 0.75) r += 10;
        if (b.speed == 0.5) r += 100;
        b.speed++;
        if (b.speed > 1.25) r += 1000;
        return r;
    }
}
"#;
        assert_eq!(run(src, "Box.Comp", &[]), 125 + 2500 + 25000);
        assert_eq!(run(src, "Box.Vec", &[]), 85 * 10000 + 11 * 100 + 1);
        assert_eq!(run(src, "Box.Cmp", &[]), 1111);
        let parsed = parse_sources(&[("p.zs".into(), src.into())]);
        let analysis = analyze(&parsed.unit).expect("sema");
        let prog = lower(&parsed.unit, &analysis).expect("lower");
        let mut vm = Zvm::new(prog);
        let len = vm.call_f64("Box.Len").expect("len");
        assert!((len - (5.0 + 3.0 + 7.0)).abs() < 1e-9, "{len}");
    }

    #[test]
    fn call_abi_runs_probe_integer_shapes() {
        let src = r#"
class String {
    native static String Format(String fmt, int arg);
    native int ToInt(int base);
    native int CharCodeAt(int i);
    native String Mid(int a, int b);
    native int Length();
}
class P
{
    static int Ret() { return 5; }
    static int Add() { int t = 2; t += 3; return t; }
    static int Call() { return P.Ret() + 1; }
    static int Wh() { int i = 0; while (i < 7) i++; return i; }
    static int If1(int x) { if (x > 3) return 1; return -1; }
    static int Loop() { int t = 0; for (int i = 0; i < 10; i++) t += i; return t; }
    static int Brk() { int i; for (i = 0; i < 100; i++) { if (i == 4) break; } return i; }
    static int DoW() { int i = 0; do { i += 2; } while (i < 7); return i; }
    static int Cont() { int t = 0; for (int i = 0; i < 5; i++) { if (i == 2) continue; t += i; } return t; }
    static int Sw(int x) { switch (x) { case 1: return 10; case 2: return 20; default: return 99; } }
    static int SwFall(int x) { int r = 0; switch (x) { case 1: r += 1; case 2: r += 2; break; case 3: r = 30; } return r; }
    static int Tern(int x) { return (x > 3) ? 1 : -1; }
    static int Neg() { int temp = 200; return (temp > 127) ? (-temp) : temp; }
    static int Bits() { int v = 0x1234; return ((v >> 4) & 0xff) | (1 << 12); }
    static int Fmt() { String s = String.Format("%i", 42); return s.ToInt(10); }
    static int Chr() { String b = "AB"; return b.CharCodeAt(1); }
    static int Mid() { String b = "hello"; return b.Mid(1, 3).Length(); }
    static int Arr() { Array<int> a; a.Push(3); a.Push(4); a.Insert(0, 9); return a.Size() * 100 + a[0]; }
    static int ArrDel() { Array<int> a; a.Push(3); a.Push(4); a.Delete(0); return a[0]; }
    static int ArrS() { Array<string> a; a.Push("x"); a.Insert(0, "yz"); return a[0].Length() + a.Size() * 10; }
    static int MapLen() { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insert(5, "abc"); return m.get(5).Length(); }
    static int MapInt() { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insertInt(7, 9); m.insertInt(3, 1); return m.getInt(7) * 10 + m.getInt(3); }
    static int MapDel() { ZC_IntStrMap m = new("ZC_IntStrMap"); m.insert(1, "a"); m.remove(1); return m.checkKey(1); }
    static bool NotNot() { Actor a = null; return !!a; }
    static int Cast() { Object o = new("ZC_IntStrMap"); ZC_IntStrMap m = ZC_IntStrMap(o); return m != null; }
    static int CastBad() { Object o = new("ZC_IntStrMap"); EventHandler e = EventHandler(o); return e == null; }
    static double Fl() { double d = 1.5; return d * 2; }
    static int Out() { int r; P.SetOut(r); return r; }
    static void SetOut(out int r) { r = 77; }
    static int Scale(int damage, int burn, int passive) {
        Actor source = new("Actor");
        source.bISMONSTER = 1;
        Actor inf = new("Actor");
        int outv;
        string ty = "Hit";
        if (burn) ty = "Burn";
        Damage_Control d = new("Damage_Control");
        d.ModifyDamage(damage, ty, outv, passive, inf, source);
        return outv;
    }
}
class Actor { bool bISMONSTER; }
class Damage_Control {
    void ModifyDamage(int damage, string damageType, out int newDamage, bool passive, Actor inflictor, Actor source) {
        if (!passive || !source || !inflictor) return;
        if (source.bISMONSTER) {
            if (damageType == "Burn") newDamage = damage * 3;
            else newDamage = damage * 7;
        }
    }
}
class Object {}
class EventHandler {}
class ZC_IntStrMap
{
    Array<int> keys;
    Array<string> values;
    void insert(int key, string val)
    {
        int idx;
        int keysSize = keys.size();
        bool foundIdx = false;
        for (idx = 0; idx < keysSize; idx++)
        {
            if (keys[idx] == key)
            {
                values[idx] = val;
                return;
            }
            if (keys[idx] > key)
            {
                foundIdx = true;
                break;
            }
        }
        if (!foundIdx)
        {
            idx = keysSize;
        }
        keys.insert(idx, key);
        values.insert(idx, val);
    }
    void insertInt(int key, int val)
    {
        insert(key, String.Format("%i", val));
    }
    int binSearch(int key)
    {
        int idx;
        int keysSize = keys.size();
        bool foundIdx = false;
        for (idx = 0; idx < keysSize; idx++)
        {
            if (keys[idx] == key)
            {
                foundIdx = true;
                break;
            }
        }
        return (foundIdx) ? idx : -1;
    }
    void remove(int key)
    {
        int idx = binSearch(key);
        if (idx > -1)
        {
            keys.delete(idx);
            values.delete(idx);
        }
    }
    string get(int key)
    {
        int idx = binSearch(key);
        if (idx > -1)
            return values[idx];
        return "";
    }
    int getInt(int key)
    {
        return get(key).toInt(10);
    }
    int checkKey(int key)
    {
        return binSearch(key) > -1;
    }
}
"#;
        assert_eq!(run(src, "P.Ret", &[]), 5);
        assert_eq!(run(src, "P.Add", &[]), 5);
        assert_eq!(run(src, "P.Call", &[]), 6);
        assert_eq!(run(src, "P.Wh", &[]), 7);
        assert_eq!(run(src, "P.If1", &[1]), -1);
        assert_eq!(run(src, "P.If1", &[4]), 1);
        assert_eq!(run(src, "P.Loop", &[]), 45);
        assert_eq!(run(src, "P.Brk", &[]), 4);
        assert_eq!(run(src, "P.DoW", &[]), 8);
        assert_eq!(run(src, "P.Cont", &[]), 8);
        assert_eq!(run(src, "P.Sw", &[2]), 20);
        assert_eq!(run(src, "P.Sw", &[9]), 99);
        assert_eq!(run(src, "P.SwFall", &[1]), 3);
        assert_eq!(run(src, "P.SwFall", &[3]), 30);
        assert_eq!(run(src, "P.Tern", &[4]), 1);
        assert_eq!(run(src, "P.Tern", &[3]), -1);
        assert_eq!(run(src, "P.Neg", &[]), -200);
        assert_eq!(run(src, "P.Bits", &[]), 0x1023);
        assert_eq!(run(src, "P.Fmt", &[]), 42);
        assert_eq!(run(src, "P.Chr", &[]), 66);
        assert_eq!(run(src, "P.Mid", &[]), 3);
        assert_eq!(run(src, "P.Arr", &[]), 309);
        assert_eq!(run(src, "P.ArrDel", &[]), 4);
        assert_eq!(run(src, "P.ArrS", &[]), 22);
        assert_eq!(run(src, "P.MapLen", &[]), 3);
        assert_eq!(run(src, "P.MapInt", &[]), 91);
        assert_eq!(run(src, "P.MapDel", &[]), 0);
        assert_eq!(run(src, "P.NotNot", &[]), 0);
        assert_eq!(run(src, "P.Cast", &[]), 1);
        assert_eq!(run(src, "P.CastBad", &[]), 1);
        assert_eq!(run(src, "P.Out", &[]), 77);
        assert_eq!(run(src, "P.Scale", &[10, 0, 1]), 70);
        assert_eq!(run(src, "P.Scale", &[10, 1, 1]), 30);
        assert_eq!(run(src, "P.Scale", &[10, 0, 0]), 0);
        let parsed = parse_sources(&[("p.zs".into(), src.into())]);
        let analysis = analyze(&parsed.unit).expect("sema");
        let prog = lower(&parsed.unit, &analysis).expect("lower");
        let mut vm = Zvm::new(prog);
        let fl = vm.call_f64("P.Fl").expect("fl");
        assert_eq!(fl, 3.0);
    }

    #[test]
    fn lower_real_damage_handler() {
        let text = std::fs::read_to_string(
            "/Users/toto/Code/games/room/testdata/zscript/bd_real/DamageHandler.zc",
        )
        .expect("damage");
        let sources = crate::with_room_stdlib(&[("zscript/DamageHandler.zc".into(), text)]);
        let parsed = parse_sources(&sources);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let analysis = analyze(&parsed.unit).expect("sema");
        lower(&parsed.unit, &analysis).expect("lower");
    }

    #[test]
    fn lower_real_bd_sources() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ws = root.join("../..");
        let read = |p: &str| std::fs::read_to_string(ws.join(p)).expect(p);
        let sources = crate::with_room_stdlib(&[
            (
                "zscript.zc".into(),
                read("testdata/zscript/bd_real/zscript.zc"),
            ),
            (
                "zscript/BD_WCS.zc".into(),
                read("testdata/zscript/bd_real/BD_WCS.zc"),
            ),
            (
                "zscript/DamageHandler.zc".into(),
                read("testdata/zscript/bd_real/DamageHandler.zc"),
            ),
        ]);
        let parsed = parse_sources(&sources);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let analysis = analyze(&parsed.unit).expect("sema");
        match lower(&parsed.unit, &analysis) {
            Ok(_) => {}
            Err(err) => {
                let mut msgs: Vec<_> = err
                    .into_iter()
                    .map(|d| format!("{}:{} {}", d.span.line, d.span.col, d.message))
                    .collect();
                msgs.sort();
                msgs.dedup();
                panic!("{}", msgs.join("\n"));
            }
        }
    }

    #[test]
    fn probe_file_executes_with_real_bd_map() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ws = root.join("../..");
        let read = |p: &str| std::fs::read_to_string(ws.join(p)).expect(p);
        let sources = vec![
            (
                "zscript.zc".into(),
                read("testdata/zscript/bd_real/zscript.zc"),
            ),
            (
                "zscript/BD_WCS.zc".into(),
                read("testdata/zscript/bd_real/BD_WCS.zc"),
            ),
            (
                "zscript/DamageHandler.zc".into(),
                read("testdata/zscript/bd_real/DamageHandler.zc"),
            ),
            (
                "probe.zs".into(),
                read("testdata/zscript/conformance/probe.zs"),
            ),
        ];
        let result = crate::check(&sources);
        assert!(result.ok(), "{:?}", result.diagnostics);
        let mut vm = Zvm::new(result.program.expect("program"));
        let call = |vm: &mut Zvm, name: &str, args: &[i64]| {
            vm.call_with(&format!("Probe.{name}"), args, &[])
                .unwrap_or_else(|e| panic!("{name}: {e}"))
        };
        assert_eq!(call(&mut vm, "Ret", &[]), 5);
        assert_eq!(call(&mut vm, "Add", &[]), 5);
        assert_eq!(call(&mut vm, "Loop", &[]), 45);
        assert_eq!(call(&mut vm, "Wh", &[]), 7);
        assert_eq!(call(&mut vm, "DoW", &[]), 8);
        assert_eq!(call(&mut vm, "Sw", &[2]), 20);
        assert_eq!(call(&mut vm, "Sw", &[9]), 99);
        assert_eq!(call(&mut vm, "SwFall", &[1]), 3);
        assert_eq!(call(&mut vm, "Tern", &[4]), 1);
        assert_eq!(call(&mut vm, "Tern", &[3]), -1);
        assert_eq!(call(&mut vm, "If1", &[1]), -1);
        assert_eq!(call(&mut vm, "Call", &[]), 6);
        assert_eq!(call(&mut vm, "Brk", &[]), 4);
        assert_eq!(call(&mut vm, "Cont", &[]), 8);
        assert_eq!(call(&mut vm, "Fmt", &[]), 42);
        assert_eq!(call(&mut vm, "Chr", &[]), 66);
        assert_eq!(call(&mut vm, "Mid", &[]), 3);
        assert_eq!(call(&mut vm, "Arr", &[]), 309);
        assert_eq!(call(&mut vm, "ArrDel", &[]), 4);
        assert_eq!(call(&mut vm, "ArrS", &[]), 22);
        assert_eq!(call(&mut vm, "MapLen", &[]), 3);
        assert_eq!(call(&mut vm, "MapInt", &[]), 91);
        assert_eq!(call(&mut vm, "MapDel", &[]), 0);
        assert_eq!(call(&mut vm, "Bits", &[]), 0x1023);
        assert_eq!(call(&mut vm, "Neg", &[]), -200);
        assert_eq!(call(&mut vm, "NotNot", &[]), 0);
        assert_eq!(call(&mut vm, "Cast", &[]), 1);
        assert_eq!(call(&mut vm, "CastBad", &[]), 1);
        assert_eq!(call(&mut vm, "Out", &[]), 77);
        let fl = vm
            .call_f64("Probe.Fl")
            .unwrap_or_else(|e| panic!("Fl: {e}"));
        assert!((fl - 3.0).abs() < 1e-9, "{fl}");
    }
}

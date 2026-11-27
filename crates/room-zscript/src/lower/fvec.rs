//! Float and vector fields, vector locals and vector expressions.
//!
//! Object fields declared `double` / `float` live in the object's float bank
//! (`fields_f`, `LoadFieldF` / `StoreFieldF`). `vector2` / `vector3` /
//! `vector4` / `quat` fields take 2–4 consecutive float slots, x first.
//! Vector locals take as many consecutive float registers.

use super::*;

/// `double`, `float` and their sized spellings.
pub(super) fn is_float_type_name(t: &str) -> bool {
    matches!(
        t.to_ascii_lowercase().as_str(),
        "double" | "float" | "float64" | "float32"
    )
}

/// Component count of a vector type name.
pub(super) fn vector_dim(t: &str) -> Option<u8> {
    match t.to_ascii_lowercase().as_str() {
        "vector2" | "fvector2" => Some(2),
        "vector3" | "fvector3" => Some(3),
        "vector4" | "fvector4" | "quat" | "fquat" => Some(4),
        _ => None,
    }
}

/// Vector dimension of a declared type.
pub(super) fn type_vector_dim(ty: &TypeRef) -> Option<u8> {
    match ty {
        TypeRef::Named(t) => vector_dim(t),
        TypeRef::Readonly(inner) => type_vector_dim(inner),
        _ => None,
    }
}

/// `x` → 0 … `w` → 3 (also `r g b a`).
pub(super) fn axis_index(field: &str) -> Option<u8> {
    match field.to_ascii_lowercase().as_str() {
        "x" | "r" => Some(0),
        "y" | "g" => Some(1),
        "z" | "b" => Some(2),
        "w" | "a" => Some(3),
        _ => None,
    }
}

/// `xy` / `xyz` swizzles: leading components.
pub(super) fn swizzle_len(field: &str) -> Option<u8> {
    match field.to_ascii_lowercase().as_str() {
        "xy" => Some(2),
        "xyz" => Some(3),
        _ => None,
    }
}

/// Float field `(slot, dim)` on `class` or an ancestor. `dim` is 1 for scalars.
pub(super) fn lookup_float_field(lx: &Lx, class: &str, field: &str) -> Option<(u16, u8)> {
    let field = field.to_ascii_lowercase();
    let mut cur = Some(class.to_ascii_lowercase());
    for _ in 0..64 {
        let name = cur?;
        if let Some(slot) = lx.float_fields.get(&name).and_then(|m| m.get(&field)) {
            return Some(*slot);
        }
        cur = parent_of(lx, &name);
    }
    None
}

/// The class that declares float `field` (untyped receivers).
pub(super) fn declaring_float_field(lx: &Lx, field: &str) -> Option<(u16, u8)> {
    let field = field.to_ascii_lowercase();
    let mut found: Option<(usize, (u16, u8))> = None;
    for (class, map) in &lx.float_fields {
        let Some(&slot) = map.get(&field) else {
            continue;
        };
        let inherited = parent_of(lx, class).is_some_and(|parent| {
            lx.float_fields
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

pub(super) fn self_float_field(lx: &Lx, name: &str) -> Option<(u16, u8)> {
    lx.self_reg?;
    if int_local(lx, name).is_some()
        || object_local(lx, name).is_some()
        || float_local(lx, name).is_some()
        || vector_local(lx, name).is_some()
    {
        return None;
    }
    lookup_float_field(lx, &lx.class_name, name)
}

pub(super) fn float_local(lx: &Lx, name: &str) -> Option<u8> {
    lx.locals_f
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, r)| *r)
}

pub(super) fn vector_local(lx: &Lx, name: &str) -> Option<(u8, u8)> {
    lx.locals_v
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, r)| *r)
}

/// `dim` consecutive float registers.
pub(super) fn alloc_fv(lx: &mut Lx, dim: u8) -> u8 {
    let base = lx.alloc_f();
    for _ in 1..dim {
        lx.alloc_f();
    }
    base
}

/// Class of an object expression, without emitting code.
pub(super) fn static_class(expr: &Expr, lx: &Lx) -> Option<String> {
    match expr {
        Expr::Ident(name, _) => {
            if let Some((_, class)) = object_local(lx, name) {
                return Some(class);
            }
            if vector_local(lx, name).is_some() || float_local(lx, name).is_some() {
                return None;
            }
            lookup_object_field(lx, &lx.class_name, name).map(|(_, ty)| ty)
        }
        Expr::Member { obj, field, .. } => {
            let class = static_class(obj, lx)?;
            lookup_object_field(lx, &class, field)
                .or_else(|| declaring_object_slot(lx, field))
                .map(|(_, ty)| ty)
        }
        Expr::Cast { ty, .. } => Some(ty.to_ascii_lowercase()),
        _ => None,
    }
}

/// Where a vector value lives.
#[derive(Clone, Copy)]
pub(super) enum VecPlace {
    /// Consecutive float registers.
    Regs { base: u8, dim: u8 },
    /// Consecutive float fields of an object register.
    Field { obj: u8, slot: u16, dim: u8 },
}

impl VecPlace {
    pub(super) fn dim(self) -> u8 {
        match self {
            Self::Regs { dim, .. } | Self::Field { dim, .. } => dim,
        }
    }

    fn component(self, k: u8) -> FloatPlace {
        match self {
            Self::Regs { base, .. } => FloatPlace::Reg(base + k),
            Self::Field { obj, slot, .. } => FloatPlace::Field {
                obj,
                slot: slot + u16::from(k),
            },
        }
    }
}

/// Where a scalar float lives.
#[derive(Clone, Copy)]
pub(super) enum FloatPlace {
    Reg(u8),
    Field { obj: u8, slot: u16 },
}

impl FloatPlace {
    fn load(self, f: &mut CompiledFunction, lx: &mut Lx) -> u8 {
        match self {
            Self::Reg(r) => r,
            Self::Field { obj, slot } => {
                let dest = lx.alloc_f();
                f.emit_ext(Op::LoadFieldF, dest, obj, 0, u32::from(slot));
                dest
            }
        }
    }

    fn store(self, f: &mut CompiledFunction, value: u8) {
        match self {
            Self::Reg(r) => {
                if r != value {
                    f.emit(Op::MoveF, r, value, 0);
                }
            }
            Self::Field { obj, slot } => {
                f.emit_ext(Op::StoreFieldF, obj, value, 0, u32::from(slot));
            }
        }
    }
}

/// Float field named by `obj.field`, resolved without emitting code. An
/// object whose class is not known statically (a call result, an array
/// element) falls back to the class that declares the field.
fn member_float_field_static(obj: &Expr, field: &str, lx: &Lx) -> Option<(u16, u8)> {
    if expr_vector_dim(obj, lx).is_some() {
        return None;
    }
    if let Expr::Ident(n, _) = obj {
        if (n.eq_ignore_ascii_case("self") || n.eq_ignore_ascii_case("invoker")) && lx.instance {
            return lookup_float_field(lx, &lx.class_name, field);
        }
        if int_local(lx, n).is_some() || float_local(lx, n).is_some() {
            return None;
        }
    }
    match static_class(obj, lx) {
        Some(class) => {
            lookup_float_field(lx, &class, field).or_else(|| declaring_float_field(lx, field))
        }
        None => declaring_float_field(lx, field),
    }
}

/// Static dimension of a vector place expression (no code emitted).
fn place_dim_static(expr: &Expr, lx: &Lx) -> Option<u8> {
    match expr {
        Expr::Ident(name, _) => {
            if let Some((_, dim)) = vector_local(lx, name) {
                return Some(dim);
            }
            self_float_field(lx, name)
                .map(|(_, dim)| dim)
                .filter(|&d| d > 1)
        }
        Expr::Member { obj, field, .. } => member_float_field_static(obj, field, lx)
            .map(|(_, dim)| dim)
            .filter(|&d| d > 1),
        _ => None,
    }
}

/// Resolve a vector place, emitting the object load when needed.
pub(super) fn vector_place(f: &mut CompiledFunction, lx: &mut Lx, expr: &Expr) -> Option<VecPlace> {
    place_dim_static(expr, lx)?;
    match expr {
        Expr::Ident(name, _) => {
            if let Some((base, dim)) = vector_local(lx, name) {
                return Some(VecPlace::Regs { base, dim });
            }
            let (slot, dim) = self_float_field(lx, name)?;
            Some(VecPlace::Field {
                obj: lx.self_reg?,
                slot,
                dim,
            })
        }
        Expr::Member { obj, field, .. } => {
            let (reg, class) = object_base(f, obj, lx)?;
            let (slot, dim) = lookup_float_field(lx, &class, field)
                .or_else(|| declaring_float_field(lx, field))?;
            (dim > 1).then_some(VecPlace::Field {
                obj: reg,
                slot,
                dim,
            })
        }
        _ => None,
    }
}

/// True when `expr` names a scalar float storage location.
fn float_place_static(expr: &Expr, lx: &Lx) -> bool {
    match expr {
        Expr::Ident(name, _) => {
            // A local of another type shadows a float field (`string speed;`
            // inside an actor, whose `speed` is a double).
            let shadowed = int_local(lx, name).is_some()
                || object_local(lx, name).is_some()
                || lx.locals_s.keys().any(|k| k.eq_ignore_ascii_case(name))
                || lx.locals_a.keys().any(|k| k.eq_ignore_ascii_case(name))
                || vector_local(lx, name).is_some();
            float_local(lx, name).is_some()
                || (!shadowed && self_float_field(lx, name).is_some_and(|(_, dim)| dim == 1))
        }
        Expr::Member { obj, field, .. } => {
            if axis_index(field).is_some() && place_dim_static(obj, lx).is_some() {
                return true;
            }
            member_float_field_static(obj, field, lx).is_some_and(|(_, dim)| dim == 1)
        }
        _ => false,
    }
}

/// Resolve a scalar float place: a float local, a scalar float field or
/// one component of a vector place.
pub(super) fn float_place(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    expr: &Expr,
) -> Option<FloatPlace> {
    if !float_place_static(expr, lx) {
        return None;
    }
    match expr {
        Expr::Ident(name, _) => {
            if let Some(r) = float_local(lx, name) {
                return Some(FloatPlace::Reg(r));
            }
            let (slot, _) = self_float_field(lx, name)?;
            Some(FloatPlace::Field {
                obj: lx.self_reg?,
                slot,
            })
        }
        Expr::Member { obj, field, .. } => {
            if let Some(k) = axis_index(field) {
                if let Some(place) = vector_place(f, lx, obj) {
                    if k < place.dim() {
                        return Some(place.component(k));
                    }
                    return None;
                }
            }
            let (reg, class) = object_base(f, obj, lx)?;
            let (slot, dim) = lookup_float_field(lx, &class, field)
                .or_else(|| declaring_float_field(lx, field))?;
            (dim == 1).then_some(FloatPlace::Field { obj: reg, slot })
        }
        _ => None,
    }
}

/// A call to a native that returns a vector (`Vec3Offset`, `Vec2To`, …).
fn native_vector_call(callee: &Expr, lx: &Lx) -> Option<u8> {
    let key = vector_native_key(callee, lx)?;
    room_zvm::typed_native_vector_result(&key)
}

/// `class.method` of a native named by `callee`, receiver class included.
fn vector_native_key(callee: &Expr, lx: &Lx) -> Option<String> {
    if let Some(target) = resolve_call(callee, lx) {
        if lx.funcs.get(&target.key).is_some_and(|sig| sig.native) {
            return Some(target.key);
        }
    }
    let Expr::Member { obj, field, .. } = callee else {
        return None;
    };
    let mut class = static_class(obj, lx)?;
    for _ in 0..64 {
        let key = format!("{class}.{field}").to_ascii_lowercase();
        if lx.funcs.get(&key).is_some_and(|sig| sig.native) {
            return Some(key);
        }
        class = parent_of(lx, &class)?;
    }
    None
}

/// Builtins that return vectors, and their sizes.
fn builtin_vector_call(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "angletovector" | "rotatevector" => Some(2),
        // Constructor calls: `Quat(x, y, z, w)`, `Vector3(x, y, z)`, …
        other => vector_dim(other),
    }
}

/// Static vector size of an expression, `None` for scalars.
pub(super) fn expr_vector_dim(expr: &Expr, lx: &Lx) -> Option<u8> {
    match expr {
        Expr::Ident(..) => place_dim_static(expr, lx),
        Expr::Member { obj, field, .. } => {
            if let Some(n) = swizzle_len(field) {
                if expr_vector_dim(obj, lx).is_some() {
                    return Some(n);
                }
            }
            if field.eq_ignore_ascii_case("unit") && expr_vector_dim(obj, lx).is_some() {
                return expr_vector_dim(obj, lx);
            }
            place_dim_static(expr, lx)
        }
        Expr::Tuple { elems, .. } if (2..=4).contains(&elems.len()) => {
            let mut n = 0u8;
            for e in elems {
                n += expr_vector_dim(e, lx).unwrap_or(1);
            }
            (2..=4).contains(&n).then_some(n)
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let (l, r) = (expr_vector_dim(lhs, lx), expr_vector_dim(rhs, lx));
            match op.as_str() {
                "+" | "-" => l.or(r),
                "*" => match (l, r) {
                    (Some(_), Some(_)) => None,
                    (a, b) => a.or(b),
                },
                "/" => l,
                _ if op.eq_ignore_ascii_case("cross") => Some(3),
                _ => None,
            }
        }
        Expr::Unary { op, expr, .. } if op == "-" || op == "+" => expr_vector_dim(expr, lx),
        Expr::Ternary { then_e, else_e, .. } => {
            expr_vector_dim(then_e, lx).or_else(|| expr_vector_dim(else_e, lx))
        }
        Expr::Call { callee, .. } => {
            if let Some(dim) = native_vector_call(callee, lx) {
                return Some(dim);
            }
            if let Some(dim) = script_vector_ret(callee, lx) {
                return Some(dim);
            }
            match callee.as_ref() {
                Expr::Ident(name, _) => builtin_vector_call(name),
                Expr::Member { obj, field, .. } => {
                    if field.eq_ignore_ascii_case("unit") {
                        return expr_vector_dim(obj, lx);
                    }
                    None
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// True when `expr` is statically a floating-point scalar.
pub(super) fn expr_is_float(expr: &Expr, lx: &Lx) -> bool {
    match expr {
        Expr::Float(..) => true,
        Expr::Ident(name, _) => {
            float_place_static(expr, lx) || lx.float_consts.contains_key(&name.to_ascii_lowercase())
        }
        Expr::Member { obj, field, .. } => {
            if float_place_static(expr, lx) {
                return true;
            }
            if field.eq_ignore_ascii_case("length") && expr_vector_dim(obj, lx).is_some() {
                return true;
            }
            if let Expr::Ident(cls, _) = obj.as_ref() {
                return lookup_float_const(&lx.float_consts, &lx.parents, cls, field).is_some();
            }
            false
        }
        Expr::Unary { op, expr, .. } if op == "-" || op == "+" => expr_is_float(expr, lx),
        Expr::Binary { op, lhs, rhs, .. } => match op.as_str() {
            "+" | "-" | "/" | "**" => expr_is_float(lhs, lx) || expr_is_float(rhs, lx),
            "*" => {
                (expr_vector_dim(lhs, lx).is_some() && expr_vector_dim(rhs, lx).is_some())
                    || ((expr_is_float(lhs, lx) || expr_is_float(rhs, lx))
                        && expr_vector_dim(lhs, lx).is_none()
                        && expr_vector_dim(rhs, lx).is_none())
            }
            "dot" | "DOT" | "Dot" => true,
            _ => false,
        },
        Expr::Ternary { then_e, else_e, .. } => {
            expr_is_float(then_e, lx) || expr_is_float(else_e, lx)
        }
        Expr::Cast { ty, .. } => is_float_type_name(ty),
        Expr::Call { callee, .. } => match callee.as_ref() {
            Expr::Ident(name, _) => {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "frandom"
                        | "frandom2"
                        | "fcrandom"
                        | "sin"
                        | "cos"
                        | "tan"
                        | "asin"
                        | "acos"
                        | "atan"
                        | "atan2"
                        | "vectorangle"
                        | "sqrt"
                        | "double"
                        | "float"
                        | "fmin"
                        | "fmax"
                        | "deltaangle"
                        | "absangle"
                        | "frandompick"
                ) || resolve_call(callee, lx)
                    .and_then(|t| lx.funcs.get(&t.key))
                    .is_some_and(|sig| sig.ret == 1)
            }
            Expr::Member { obj, field, .. } => {
                (field.eq_ignore_ascii_case("length") && expr_vector_dim(obj, lx).is_some())
                    || resolve_call(callee, lx)
                        .and_then(|t| lx.funcs.get(&t.key))
                        .is_some_and(|sig| sig.ret == 1)
            }
            _ => false,
        },
        _ => false,
    }
}

/// Copy `dim` components from `place` into fresh registers.
fn load_place(f: &mut CompiledFunction, lx: &mut Lx, place: VecPlace, dim: u8) -> u8 {
    let base = alloc_fv(lx, dim);
    for k in 0..dim {
        let v = if k < place.dim() {
            place.component(k).load(f, lx)
        } else {
            load_f(f, lx, 0.0)
        };
        f.emit(Op::MoveF, base + k, v, 0);
    }
    base
}

/// Evaluate a vector expression into `dim` consecutive float registers.
pub(super) fn emit_expr_v(f: &mut CompiledFunction, lx: &mut Lx, expr: &Expr, dim: u8) -> u8 {
    if let Some(call) = named_call(expr, lx) {
        return emit_expr_v(f, lx, &call, dim);
    }
    if let Expr::Binary { op, rhs, .. } = expr {
        if op == NAMED_ARG {
            return emit_expr_v(f, lx, rhs, dim);
        }
    }
    match expr {
        Expr::Ident(..) => {
            if let Some(place) = vector_place(f, lx, expr) {
                return load_place(f, lx, place, dim);
            }
        }
        Expr::Member { obj, field, .. } => {
            if let Some(n) = swizzle_len(field) {
                if let Some(src) = expr_vector_dim(obj, lx) {
                    let v = emit_expr_v(f, lx, obj, src);
                    let base = alloc_fv(lx, dim);
                    for k in 0..dim {
                        if k < n && k < src {
                            f.emit(Op::MoveF, base + k, v + k, 0);
                        } else {
                            let z = load_f(f, lx, 0.0);
                            f.emit(Op::MoveF, base + k, z, 0);
                        }
                    }
                    return base;
                }
            }
            if field.eq_ignore_ascii_case("unit") && expr_vector_dim(obj, lx).is_some() {
                return emit_unit(f, lx, obj, dim);
            }
            if let Some(place) = vector_place(f, lx, expr) {
                return load_place(f, lx, place, dim);
            }
        }
        Expr::Tuple { elems, .. } => {
            let base = alloc_fv(lx, dim);
            let mut k = 0u8;
            for e in elems {
                if let Some(n) = expr_vector_dim(e, lx) {
                    let v = emit_expr_v(f, lx, e, n);
                    for j in 0..n {
                        if k < dim {
                            f.emit(Op::MoveF, base + k, v + j, 0);
                        }
                        k += 1;
                    }
                } else {
                    let v = emit_expr_f(f, e, lx);
                    if k < dim {
                        f.emit(Op::MoveF, base + k, v, 0);
                    }
                    k += 1;
                }
            }
            while k < dim {
                let z = load_f(f, lx, 0.0);
                f.emit(Op::MoveF, base + k, z, 0);
                k += 1;
            }
            return base;
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let (ld, rd) = (expr_vector_dim(lhs, lx), expr_vector_dim(rhs, lx));
            if op.eq_ignore_ascii_case("cross") {
                // a × b = (ay bz − az by, az bx − ax bz, ax by − ay bx)
                let a = emit_expr_v(f, lx, lhs, 3);
                let b = emit_expr_v(f, lx, rhs, 3);
                let base = alloc_fv(lx, dim.max(3));
                for (k, (i, j)) in [(1u8, 2u8), (2, 0), (0, 1)].into_iter().enumerate() {
                    let p = lx.alloc_f();
                    let q = lx.alloc_f();
                    f.emit(Op::MulF, p, a + i, b + j);
                    f.emit(Op::MulF, q, a + j, b + i);
                    f.emit(Op::SubF, base + k as u8, p, q);
                }
                for k in 3..dim {
                    let z = load_f(f, lx, 0.0);
                    f.emit(Op::MoveF, base + k, z, 0);
                }
                return base;
            }
            match (op.as_str(), ld, rd) {
                ("+" | "-", _, _) => {
                    let l = emit_expr_v(f, lx, lhs, dim);
                    let r = emit_expr_v(f, lx, rhs, dim);
                    let base = alloc_fv(lx, dim);
                    let code = if op == "+" { Op::AddF } else { Op::SubF };
                    for k in 0..dim {
                        f.emit(code, base + k, l + k, r + k);
                    }
                    return base;
                }
                ("*", Some(_), None) | ("/", Some(_), _) => {
                    let v = emit_expr_v(f, lx, lhs, dim);
                    let s = emit_expr_f(f, rhs, lx);
                    let base = alloc_fv(lx, dim);
                    let code = if op == "*" { Op::MulF } else { Op::DivF };
                    for k in 0..dim {
                        f.emit(code, base + k, v + k, s);
                    }
                    return base;
                }
                ("*", None, Some(_)) => {
                    let s = emit_expr_f(f, lhs, lx);
                    let v = emit_expr_v(f, lx, rhs, dim);
                    let base = alloc_fv(lx, dim);
                    for k in 0..dim {
                        f.emit(Op::MulF, base + k, v + k, s);
                    }
                    return base;
                }
                _ => {}
            }
        }
        Expr::Unary {
            op, expr: inner, ..
        } if op == "-" || op == "+" => {
            let v = emit_expr_v(f, lx, inner, dim);
            if op == "+" {
                return v;
            }
            let z = load_f(f, lx, 0.0);
            let base = alloc_fv(lx, dim);
            for k in 0..dim {
                f.emit(Op::SubF, base + k, z, v + k);
            }
            return base;
        }
        Expr::Ternary {
            cond,
            then_e,
            else_e,
            ..
        } => {
            let base = alloc_fv(lx, dim);
            let c = emit_expr(f, cond, lx);
            let jelse = emit_jcc(f, Op::JmpIfNot, c);
            let t = emit_expr_v(f, lx, then_e, dim);
            for k in 0..dim {
                f.emit(Op::MoveF, base + k, t + k, 0);
            }
            let jend = emit_jcc(f, Op::Jmp, 0);
            patch(f, jelse);
            let e = emit_expr_v(f, lx, else_e, dim);
            for k in 0..dim {
                f.emit(Op::MoveF, base + k, e + k, 0);
            }
            patch(f, jend);
            return base;
        }
        Expr::Call { callee, args, .. } => {
            if let Expr::Ident(name, _) = callee.as_ref() {
                if vector_dim(name).is_some() {
                    // `Quat(0, 0, 0, 1)` / `Vector3(x, y, z)`: a tuple literal.
                    let tuple = Expr::Tuple {
                        elems: args.clone(),
                        span: crate::span::Span::default(),
                    };
                    return emit_expr_v(f, lx, &tuple, dim);
                }
                if name.eq_ignore_ascii_case("AngleToVector") {
                    // (cos(a) * len, sin(a) * len), len defaults to 1.
                    let a = match args.first() {
                        Some(e) => emit_expr_f(f, e, lx),
                        None => load_f(f, lx, 0.0),
                    };
                    let len = match args.get(1) {
                        Some(e) => emit_expr_f(f, e, lx),
                        None => load_f(f, lx, 1.0),
                    };
                    let c = math1(f, lx, "cos", a);
                    let s = math1(f, lx, "sin", a);
                    let base = alloc_fv(lx, dim);
                    f.emit(Op::MulF, base, c, len);
                    if dim > 1 {
                        f.emit(Op::MulF, base + 1, s, len);
                    }
                    for k in 2..dim {
                        let z = load_f(f, lx, 0.0);
                        f.emit(Op::MoveF, base + k, z, 0);
                    }
                    return base;
                }
            }
            if let Expr::Member { obj, field, .. } = callee.as_ref() {
                if field.eq_ignore_ascii_case("unit") && expr_vector_dim(obj, lx).is_some() {
                    return emit_unit(f, lx, obj, dim);
                }
            }
            if let Some(n) = native_vector_call(callee, lx) {
                if let Some(base) = emit_native_vector_call(f, lx, callee, args, n, dim) {
                    return base;
                }
            }
            if let Some((v, n)) = emit_vector_script_call(f, lx, callee, args) {
                if n == dim {
                    return v;
                }
                let base = alloc_fv(lx, dim);
                for k in 0..dim {
                    if k < n {
                        f.emit(Op::MoveF, base + k, v + k, 0);
                    } else {
                        let z = load_f(f, lx, 0.0);
                        f.emit(Op::MoveF, base + k, z, 0);
                    }
                }
                return base;
            }
        }
        _ => {}
    }
    // Anything else (unknown calls): evaluate for side effects, yield zeros.
    let _ = emit_expr(f, expr, lx);
    let base = alloc_fv(lx, dim);
    for k in 0..dim {
        let z = load_f(f, lx, 0.0);
        f.emit(Op::MoveF, base + k, z, 0);
    }
    base
}

/// Call a vector-returning native; the result lands in `dim` registers.
fn emit_native_vector_call(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    callee: &Expr,
    args: &[Expr],
    n: u8,
    dim: u8,
) -> Option<u8> {
    let key = vector_native_key(callee, lx)?;
    let sig = lx.funcs.get(&key)?.clone();
    let user = if sig.instance {
        &sig.params[1..]
    } else {
        &sig.params[..]
    };
    let mut prepared = Vec::new();
    if sig.instance {
        let recv = match callee {
            Expr::Member { obj, .. } => object_base(f, obj, lx).map(|(r, _)| r),
            _ => lx.self_reg,
        };
        prepared.push((3u8, recv?));
    }
    let user = user.to_vec();
    prepared.extend(prepare_sig_args(f, lx, &sig, &user, args));
    for (bank, reg) in prepared {
        f.emit(Op::Arg, bank, reg, 0);
    }
    let id = lx.native_id(&key);
    f.emit_ext(Op::CallNat, 1, 4, 0, id);
    let base = alloc_fv(lx, dim);
    for k in 0..dim {
        if k < n {
            f.emit(Op::GetRet, 1, base + k, k);
        } else {
            let z = load_f(f, lx, 0.0);
            f.emit(Op::MoveF, base + k, z, 0);
        }
    }
    Some(base)
}

fn math1(f: &mut CompiledFunction, lx: &mut Lx, name: &str, arg: u8) -> u8 {
    let id = lx.native_id(&format!("math.{name}"));
    f.emit(Op::Arg, 1, arg, 0);
    f.emit_ext(Op::CallNat, 1, 1, 0, id);
    let dest = lx.alloc_f();
    f.emit(Op::GetRet, 1, dest, 0);
    dest
}

/// Squared length of `dim` registers.
fn length_sq(f: &mut CompiledFunction, lx: &mut Lx, v: u8, dim: u8) -> u8 {
    let acc = load_f(f, lx, 0.0);
    let sum = lx.alloc_f();
    f.emit(Op::MoveF, sum, acc, 0);
    for k in 0..dim {
        let sq = lx.alloc_f();
        f.emit(Op::MulF, sq, v + k, v + k);
        f.emit(Op::AddF, sum, sum, sq);
    }
    sum
}

/// `v.Length()`.
pub(super) fn emit_length(f: &mut CompiledFunction, lx: &mut Lx, obj: &Expr) -> Option<u8> {
    let dim = expr_vector_dim(obj, lx)?;
    let v = emit_expr_v(f, lx, obj, dim);
    let sq = length_sq(f, lx, v, dim);
    Some(math1(f, lx, "sqrt", sq))
}

/// `v.Unit()`: zero stays zero.
fn emit_unit(f: &mut CompiledFunction, lx: &mut Lx, obj: &Expr, dim: u8) -> u8 {
    let src = expr_vector_dim(obj, lx).unwrap_or(dim);
    let v = emit_expr_v(f, lx, obj, src);
    let sq = length_sq(f, lx, v, src);
    let len = math1(f, lx, "sqrt", sq);
    let base = alloc_fv(lx, dim);
    let zero = load_f(f, lx, 0.0);
    let cmp = lx.alloc();
    f.emit(Op::CmpF, cmp, len, zero);
    for k in 0..dim {
        f.emit(Op::MoveF, base + k, zero, 0);
    }
    let jzero = emit_jcc(f, Op::JmpIfNot, cmp);
    for k in 0..dim.min(src) {
        f.emit(Op::DivF, base + k, v + k, len);
    }
    patch(f, jzero);
    base
}

/// Dot product of two vector expressions.
pub(super) fn emit_dot(f: &mut CompiledFunction, lx: &mut Lx, lhs: &Expr, rhs: &Expr) -> u8 {
    let dim = expr_vector_dim(lhs, lx)
        .unwrap_or(3)
        .min(expr_vector_dim(rhs, lx).unwrap_or(3));
    let l = emit_expr_v(f, lx, lhs, dim);
    let r = emit_expr_v(f, lx, rhs, dim);
    let sum = load_f(f, lx, 0.0);
    let acc = lx.alloc_f();
    f.emit(Op::MoveF, acc, sum, 0);
    for k in 0..dim {
        let p = lx.alloc_f();
        f.emit(Op::MulF, p, l + k, r + k);
        f.emit(Op::AddF, acc, acc, p);
    }
    acc
}

/// Scalar float read: float place, `.Length()`, `a * b` dot product.
pub(super) fn try_emit_float_read(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    expr: &Expr,
) -> Option<u8> {
    match expr {
        Expr::Ident(..) | Expr::Member { .. } => {
            if let Expr::Member { obj, field, .. } = expr {
                if field.eq_ignore_ascii_case("length") && expr_vector_dim(obj, lx).is_some() {
                    return emit_length(f, lx, obj);
                }
            }
            let place = float_place(f, lx, expr)?;
            Some(place.load(f, lx))
        }
        Expr::Call { callee, args, .. } if args.is_empty() => {
            let Expr::Member { obj, field, .. } = callee.as_ref() else {
                return None;
            };
            if field.eq_ignore_ascii_case("length") && expr_vector_dim(obj, lx).is_some() {
                return emit_length(f, lx, obj);
            }
            None
        }
        Expr::Binary { op, lhs, rhs, .. }
            if (op == "*" || op.eq_ignore_ascii_case("dot"))
                && expr_vector_dim(lhs, lx).is_some()
                && expr_vector_dim(rhs, lx).is_some() =>
        {
            Some(emit_dot(f, lx, lhs, rhs))
        }
        _ => None,
    }
}

/// Assignment to a vector or float place. Returns `true` when handled.
pub(super) fn try_emit_float_assign(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    op: &str,
    lhs: &Expr,
    rhs: &Expr,
) -> bool {
    let swizzle = match lhs {
        Expr::Member { obj, field, .. } => swizzle_len(field)
            .filter(|_| place_dim_static(obj, lx).is_some())
            .map(|n| (obj.as_ref(), n)),
        _ => None,
    };
    let target_dim = match swizzle {
        Some((obj, n)) => place_dim_static(obj, lx).map(|d| d.min(n)),
        None => place_dim_static(lhs, lx),
    };
    if let Some(dim) = target_dim {
        let place = match swizzle {
            Some((obj, _)) => vector_place(f, lx, obj).map(|p| match p {
                VecPlace::Regs { base, .. } => VecPlace::Regs { base, dim },
                VecPlace::Field { obj, slot, .. } => VecPlace::Field { obj, slot, dim },
            }),
            None => vector_place(f, lx, lhs),
        };
        let Some(place) = place else {
            return false;
        };
        match op {
            "=" | "+=" | "-=" => {
                let v = emit_expr_v(f, lx, rhs, dim);
                for k in 0..dim {
                    let comp = place.component(k);
                    if op == "=" {
                        comp.store(f, v + k);
                    } else {
                        let cur = comp.load(f, lx);
                        let out = lx.alloc_f();
                        let code = if op == "+=" { Op::AddF } else { Op::SubF };
                        f.emit(code, out, cur, v + k);
                        comp.store(f, out);
                    }
                }
            }
            "*=" | "/=" => {
                let s = emit_expr_f(f, rhs, lx);
                for k in 0..dim {
                    let comp = place.component(k);
                    let cur = comp.load(f, lx);
                    let out = lx.alloc_f();
                    let code = if op == "*=" { Op::MulF } else { Op::DivF };
                    f.emit(code, out, cur, s);
                    comp.store(f, out);
                }
            }
            _ => {
                lx.err(lhs.span(), format!("vector assign {op}"));
            }
        }
        return true;
    }
    if !float_place_static(lhs, lx) {
        return false;
    }
    let Some(place) = float_place(f, lx, lhs) else {
        return false;
    };
    let rv = emit_expr_f(f, rhs, lx);
    if op == "=" {
        place.store(f, rv);
        return true;
    }
    let code = match op {
        "+=" => Op::AddF,
        "-=" => Op::SubF,
        "*=" => Op::MulF,
        "/=" => Op::DivF,
        "%=" => {
            let v = emit_fmod(f, lx, lhs, rhs);
            place.store(f, v);
            return true;
        }
        _ => {
            lx.err(lhs.span(), format!("float assign {op}"));
            return true;
        }
    };
    let cur = place.load(f, lx);
    let out = lx.alloc_f();
    f.emit(code, out, cur, rv);
    place.store(f, out);
    true
}

/// `a = b` as a float-valued expression (`Vel.X = Vel.Y = 0`).
pub(super) fn emit_assign_value_f(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    lhs: &Expr,
    rhs: &Expr,
    span: crate::span::Span,
) -> u8 {
    if expr_vector_dim(lhs, lx).is_none() {
        if let Some(place) = float_place(f, lx, lhs) {
            let v = emit_expr_f(f, rhs, lx);
            place.store(f, v);
            return v;
        }
    }
    let r = emit_binary(f, lx, "=", lhs, rhs, span);
    i2f(f, lx, r)
}

/// `++` / `--` on a float place. Returns the int-converted result.
pub(super) fn try_emit_float_inc(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    expr: &Expr,
    dec: bool,
    post: bool,
) -> Option<u8> {
    if !float_place_static(expr, lx)
        || matches!(expr, Expr::Ident(n, _) if float_local(lx, n).is_some())
    {
        return None;
    }
    let place = float_place(f, lx, expr)?;
    let cur = place.load(f, lx);
    let old = lx.alloc_f();
    f.emit(Op::MoveF, old, cur, 0);
    let one = load_f(f, lx, 1.0);
    let out = lx.alloc_f();
    f.emit(if dec { Op::SubF } else { Op::AddF }, out, cur, one);
    place.store(f, out);
    Some(f2i(f, lx, if post { old } else { out }))
}

/// Float comparison `lhs op rhs` → int 0/1, when either side is a float.
pub(super) fn try_emit_float_compare(
    f: &mut CompiledFunction,
    lx: &mut Lx,
    op: &str,
    lhs: &Expr,
    rhs: &Expr,
) -> Option<u8> {
    let cmp_op = match op {
        "<" => Op::LtI,
        "<=" => Op::LeI,
        ">" => Op::GtI,
        ">=" => Op::GeI,
        "==" => Op::EqI,
        "!=" => Op::NeI,
        _ => return None,
    };
    if !(expr_is_float(lhs, lx) || expr_is_float(rhs, lx)) {
        return None;
    }
    let l = emit_expr_f(f, lhs, lx);
    let r = emit_expr_f(f, rhs, lx);
    let c = lx.alloc();
    f.emit(Op::CmpF, c, l, r);
    let zero = load_i(f, lx, 0);
    let out = lx.alloc();
    f.emit(cmp_op, out, c, zero);
    Some(out)
}

/// C `fmod`: `a - b * trunc(a / b)`.
pub(super) fn emit_fmod(f: &mut CompiledFunction, lx: &mut Lx, lhs: &Expr, rhs: &Expr) -> u8 {
    let a = emit_expr_f(f, lhs, lx);
    let b = emit_expr_f(f, rhs, lx);
    let q = lx.alloc_f();
    f.emit(Op::DivF, q, a, b);
    let t = math1(f, lx, "int", q);
    let m = lx.alloc_f();
    f.emit(Op::MulF, m, t, b);
    let out = lx.alloc_f();
    f.emit(Op::SubF, out, a, m);
    out
}

//! ZScript `Map<K, V>` and `MapIterator<K, V>`.
//!
//! The compiler lowers `Map<String, Actor>` to UZDoom's native struct
//! `Map_Str_Obj` (and `MapIterator<…>` to `MapIterator_Str_Obj`), so every
//! method arrives here as `map_<k>_<v>.<method>`. The key and value kinds
//! come from that name.
//!
//! A map value starts as an empty struct object and becomes, on first use,
//! an `Array<Object>` of two arrays: the keys (`Array<int>` / `Array<string>`)
//! and the values (`Array<int>`, floats as bits, `Array<string>` or
//! `Array<Object>`). An iterator becomes `[map, position]`, the position an
//! `Array<int>` of one element. Lookup is linear; script maps are small.

use crate::heap::{ObjHandle, ObjKind, StrHandle};
use crate::interp::Value;
use crate::natives::{NativeRegs, NativeTable, ZvmCtx};
use crate::trap::Trap;

const KEYS: [&str; 2] = ["i32", "str"];
const VALUES: [&str; 8] = ["i8", "i16", "i32", "f32", "f64", "obj", "ptr", "str"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Int,
    Float,
    Str,
    Obj,
}

fn value_kind(v: &str) -> Kind {
    match v {
        "f32" | "f64" => Kind::Float,
        "str" => Kind::Str,
        "obj" | "ptr" => Kind::Obj,
        _ => Kind::Int,
    }
}

/// `(key kind, value kind)` from `map_str_obj.insert`.
fn kinds(ctx: &ZvmCtx<'_>) -> (Kind, Kind) {
    let name = ctx.play.native_name.to_ascii_lowercase();
    let ty = name.split('.').next().unwrap_or("");
    let mut parts = ty.rsplit('_');
    let v = parts.next().unwrap_or("i32");
    let k = parts.next().unwrap_or("i32");
    let key = if k == "str" { Kind::Str } else { Kind::Int };
    (key, value_kind(v))
}

#[derive(Clone, Copy)]
enum Key {
    I(i64),
    S(StrHandle),
}

/// The key argument: `i[0]` or `s[0]`.
fn key_arg(regs: &NativeRegs<'_>, k: Kind) -> Key {
    if k == Kind::Str {
        Key::S(regs.s.first().copied().unwrap_or_default())
    } else {
        Key::I(regs.i.first().copied().unwrap_or(0))
    }
}

/// The value argument after `skip` key-or-receiver slots of its bank.
fn value_arg(regs: &NativeRegs<'_>, k: Kind, v: Kind, after_key: bool) -> Value {
    let same = usize::from(after_key && k == v);
    match v {
        Kind::Int => Value::I(regs.i.get(same).copied().unwrap_or(0)),
        Kind::Float => Value::F(regs.f.first().copied().unwrap_or(0.0)),
        Kind::Str => Value::S(regs.s.get(same).copied().unwrap_or_default()),
        // `a[0]` is the receiver.
        Kind::Obj => Value::A(regs.a.get(1).copied().unwrap_or_default()),
    }
}

fn ret_value(regs: &mut NativeRegs<'_>, v: Value) {
    match v {
        Value::I(x) => {
            if let Some(s) = regs.i.first_mut() {
                *s = x;
            }
        }
        Value::F(x) => {
            if let Some(s) = regs.f.first_mut() {
                *s = x;
            }
        }
        // String results come back in `s[1]` (see `native_call`).
        Value::S(h) => {
            if let Some(s) = regs.s.get_mut(1) {
                *s = h;
            }
        }
        Value::A(h) => {
            if let Some(s) = regs.a.first_mut() {
                *s = h;
            }
        }
    }
}

fn ret_int(regs: &mut NativeRegs<'_>, x: i64) {
    ret_value(regs, Value::I(x));
}

fn default_value(v: Kind) -> Value {
    match v {
        Kind::Int => Value::I(0),
        Kind::Float => Value::F(0.0),
        Kind::Str => Value::S(StrHandle::default()),
        Kind::Obj => Value::A(ObjHandle::NULL),
    }
}

/// The map's `(keys, values)` arrays, set up on first use.
fn storage(
    ctx: &mut ZvmCtx<'_>,
    map: ObjHandle,
    k: Kind,
    v: Kind,
) -> Option<(ObjHandle, ObjHandle)> {
    if map.is_null() {
        return None;
    }
    if let Some(ObjKind::ArrayA(parts)) = ctx.heap.get(map) {
        if parts.len() == 2 {
            return Some((parts[0], parts[1]));
        }
    }
    ctx.heap.get(map)?;
    let keys = if k == Kind::Str {
        ctx.heap.new_array_s()
    } else {
        ctx.heap.new_array_i()
    };
    let vals = match v {
        Kind::Str => ctx.heap.new_array_s(),
        Kind::Obj => ctx.heap.new_array_a(),
        Kind::Int | Kind::Float => ctx.heap.new_array_i(),
    };
    if let Some(slot) = ctx.heap.get_mut(map) {
        *slot = ObjKind::ArrayA(vec![keys, vals]);
    }
    Some((keys, vals))
}

fn len_of(ctx: &ZvmCtx<'_>, keys: ObjHandle) -> usize {
    match ctx.heap.get(keys) {
        Some(ObjKind::ArrayI(v)) => v.len(),
        Some(ObjKind::ArrayS(v)) => v.len(),
        _ => 0,
    }
}

fn find(ctx: &ZvmCtx<'_>, keys: ObjHandle, key: Key) -> Option<usize> {
    match (ctx.heap.get(keys)?, key) {
        (ObjKind::ArrayI(v), Key::I(x)) => v.iter().position(|k| *k == x),
        (ObjKind::ArrayS(v), Key::S(h)) => {
            let want = ctx.heap.str(h);
            v.iter().position(|k| ctx.heap.str(*k) == want)
        }
        _ => None,
    }
}

fn key_at(ctx: &ZvmCtx<'_>, keys: ObjHandle, at: usize) -> Option<Key> {
    match ctx.heap.get(keys)? {
        ObjKind::ArrayI(v) => v.get(at).map(|x| Key::I(*x)),
        ObjKind::ArrayS(v) => v.get(at).map(|h| Key::S(*h)),
        _ => None,
    }
}

fn value_at(ctx: &ZvmCtx<'_>, vals: ObjHandle, at: usize, v: Kind) -> Value {
    match (ctx.heap.get(vals), v) {
        (Some(ObjKind::ArrayI(a)), Kind::Float) => {
            Value::F(f64::from_bits(a.get(at).copied().unwrap_or(0) as u64))
        }
        (Some(ObjKind::ArrayI(a)), _) => Value::I(a.get(at).copied().unwrap_or(0)),
        (Some(ObjKind::ArrayS(a)), _) => Value::S(a.get(at).copied().unwrap_or_default()),
        (Some(ObjKind::ArrayA(a)), _) => Value::A(a.get(at).copied().unwrap_or_default()),
        _ => default_value(v),
    }
}

fn set_value(ctx: &mut ZvmCtx<'_>, vals: ObjHandle, at: Option<usize>, value: Value) {
    match (ctx.heap.get_mut(vals), value) {
        (Some(ObjKind::ArrayI(a)), Value::I(x)) => put(a, at, x),
        (Some(ObjKind::ArrayI(a)), Value::F(x)) => put(a, at, x.to_bits() as i64),
        (Some(ObjKind::ArrayS(a)), Value::S(h)) => put(a, at, h),
        (Some(ObjKind::ArrayA(a)), Value::A(h)) => put(a, at, h),
        _ => {}
    }
}

fn put<T>(a: &mut Vec<T>, at: Option<usize>, x: T) {
    match at.and_then(|i| a.get_mut(i)) {
        Some(slot) => *slot = x,
        None => a.push(x),
    }
}

fn push_key(ctx: &mut ZvmCtx<'_>, keys: ObjHandle, key: Key) {
    match (ctx.heap.get_mut(keys), key) {
        (Some(ObjKind::ArrayI(a)), Key::I(x)) => a.push(x),
        (Some(ObjKind::ArrayS(a)), Key::S(h)) => a.push(h),
        _ => {}
    }
}

fn remove_at(ctx: &mut ZvmCtx<'_>, h: ObjHandle, at: usize) {
    match ctx.heap.get_mut(h) {
        Some(ObjKind::ArrayI(a)) if at < a.len() => {
            a.remove(at);
        }
        Some(ObjKind::ArrayS(a)) if at < a.len() => {
            a.remove(at);
        }
        Some(ObjKind::ArrayA(a)) if at < a.len() => {
            a.remove(at);
        }
        _ => {}
    }
}

fn clear(ctx: &mut ZvmCtx<'_>, h: ObjHandle) {
    match ctx.heap.get_mut(h) {
        Some(ObjKind::ArrayI(a)) => a.clear(),
        Some(ObjKind::ArrayS(a)) => a.clear(),
        Some(ObjKind::ArrayA(a)) => a.clear(),
        _ => {}
    }
}

fn this(regs: &NativeRegs<'_>) -> ObjHandle {
    regs.a.first().copied().unwrap_or_default()
}

/// Insert or replace `key`'s value.
fn insert(ctx: &mut ZvmCtx<'_>, map: ObjHandle, k: Kind, v: Kind, key: Key, value: Value) {
    let Some((keys, vals)) = storage(ctx, map, k, v) else {
        return;
    };
    match find(ctx, keys, key) {
        Some(at) => set_value(ctx, vals, Some(at), value),
        None => {
            push_key(ctx, keys, key);
            set_value(ctx, vals, None, value);
        }
    }
}

fn map_insert(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let key = key_arg(regs, k);
    let value = value_arg(regs, k, v, true);
    insert(ctx, this(regs), k, v, key, value);
    Ok(())
}

fn map_insert_new(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let key = key_arg(regs, k);
    insert(ctx, this(regs), k, v, key, default_value(v));
    Ok(())
}

/// `Get`: the value, adding the key with a default value when it is missing.
fn map_get(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let key = key_arg(regs, k);
    let map = this(regs);
    let Some((keys, vals)) = storage(ctx, map, k, v) else {
        ret_value(regs, default_value(v));
        return Ok(());
    };
    let value = match find(ctx, keys, key) {
        Some(at) => value_at(ctx, vals, at, v),
        None => {
            insert(ctx, map, k, v, key, default_value(v));
            default_value(v)
        }
    };
    ret_value(regs, value);
    Ok(())
}

fn map_get_if_exists(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let key = key_arg(regs, k);
    let value = storage(ctx, this(regs), k, v)
        .and_then(|(keys, vals)| find(ctx, keys, key).map(|at| value_at(ctx, vals, at, v)))
        .unwrap_or(default_value(v));
    ret_value(regs, value);
    Ok(())
}

/// `CheckValue`: `value, exists`.
fn map_check_value(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let key = key_arg(regs, k);
    let found = storage(ctx, this(regs), k, v)
        .and_then(|(keys, vals)| find(ctx, keys, key).map(|at| value_at(ctx, vals, at, v)));
    let value = found.unwrap_or(default_value(v));
    ret_value(regs, value);
    ctx.play.multi_ret = vec![value, Value::I(i64::from(found.is_some()))];
    Ok(())
}

fn map_check_key(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let key = key_arg(regs, k);
    let has =
        storage(ctx, this(regs), k, v).is_some_and(|(keys, _)| find(ctx, keys, key).is_some());
    ret_int(regs, i64::from(has));
    Ok(())
}

fn map_remove(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let key = key_arg(regs, k);
    if let Some((keys, vals)) = storage(ctx, this(regs), k, v) {
        if let Some(at) = find(ctx, keys, key) {
            remove_at(ctx, keys, at);
            remove_at(ctx, vals, at);
        }
    }
    Ok(())
}

fn map_count(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let n = storage(ctx, this(regs), k, v).map_or(0, |(keys, _)| len_of(ctx, keys));
    ret_int(regs, n as i64);
    Ok(())
}

fn map_clear(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    if let Some((keys, vals)) = storage(ctx, this(regs), k, v) {
        clear(ctx, keys);
        clear(ctx, vals);
    }
    Ok(())
}

fn entries(ctx: &mut ZvmCtx<'_>, map: ObjHandle, k: Kind, v: Kind) -> Vec<(Key, Value)> {
    let Some((keys, vals)) = storage(ctx, map, k, v) else {
        return Vec::new();
    };
    (0..len_of(ctx, keys))
        .filter_map(|at| Some((key_at(ctx, keys, at)?, value_at(ctx, vals, at, v))))
        .collect()
}

/// `Copy(other)`: this map becomes a copy of `other`.
fn map_copy(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let me = this(regs);
    let other = regs.a.get(1).copied().unwrap_or_default();
    let items = entries(ctx, other, k, v);
    if let Some((keys, vals)) = storage(ctx, me, k, v) {
        clear(ctx, keys);
        clear(ctx, vals);
    }
    for (key, value) in items {
        insert(ctx, me, k, v, key, value);
    }
    Ok(())
}

/// `Move(other)`: copy, then empty `other`.
fn map_move(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    map_copy(ctx, regs)?;
    let (k, v) = kinds(ctx);
    let other = regs.a.get(1).copied().unwrap_or_default();
    if let Some((keys, vals)) = storage(ctx, other, k, v) {
        clear(ctx, keys);
        clear(ctx, vals);
    }
    Ok(())
}

fn map_swap(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let me = this(regs);
    let other = regs.a.get(1).copied().unwrap_or_default();
    let (Some(a), Some(b)) = (storage(ctx, me, k, v), storage(ctx, other, k, v)) else {
        return Ok(());
    };
    if let Some(slot) = ctx.heap.get_mut(me) {
        *slot = ObjKind::ArrayA(vec![b.0, b.1]);
    }
    if let Some(slot) = ctx.heap.get_mut(other) {
        *slot = ObjKind::ArrayA(vec![a.0, a.1]);
    }
    Ok(())
}

/// The iterator's `(map, position cell)`, set up on first use.
fn iter_parts(ctx: &mut ZvmCtx<'_>, it: ObjHandle) -> Option<(ObjHandle, ObjHandle)> {
    if it.is_null() {
        return None;
    }
    if let Some(ObjKind::ArrayA(parts)) = ctx.heap.get(it) {
        if parts.len() == 2 {
            return Some((parts[0], parts[1]));
        }
    }
    ctx.heap.get(it)?;
    let pos = ctx.heap.new_array_i();
    if let Some(ObjKind::ArrayI(a)) = ctx.heap.get_mut(pos) {
        a.push(-1);
    }
    if let Some(slot) = ctx.heap.get_mut(it) {
        *slot = ObjKind::ArrayA(vec![ObjHandle::NULL, pos]);
    }
    Some((ObjHandle::NULL, pos))
}

fn pos_of(ctx: &ZvmCtx<'_>, cell: ObjHandle) -> i64 {
    match ctx.heap.get(cell) {
        Some(ObjKind::ArrayI(a)) => a.first().copied().unwrap_or(-1),
        _ => -1,
    }
}

fn set_pos(ctx: &mut ZvmCtx<'_>, cell: ObjHandle, p: i64) {
    if let Some(ObjKind::ArrayI(a)) = ctx.heap.get_mut(cell) {
        if let Some(x) = a.first_mut() {
            *x = p;
        }
    }
}

/// The current entry: `(map, index)` when the iterator stands on one.
fn current(ctx: &mut ZvmCtx<'_>, it: ObjHandle, k: Kind, v: Kind) -> Option<(ObjHandle, usize)> {
    let (map, cell) = iter_parts(ctx, it)?;
    let p = usize::try_from(pos_of(ctx, cell)).ok()?;
    let (keys, _) = storage(ctx, map, k, v)?;
    (p < len_of(ctx, keys)).then_some((map, p))
}

fn iter_init(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let it = this(regs);
    let map = regs.a.get(1).copied().unwrap_or_default();
    let Some((_, cell)) = iter_parts(ctx, it) else {
        ret_int(regs, 0);
        return Ok(());
    };
    if let Some(ObjKind::ArrayA(parts)) = ctx.heap.get_mut(it) {
        parts[0] = map;
    }
    set_pos(ctx, cell, -1);
    let n = storage(ctx, map, k, v).map_or(0, |(keys, _)| len_of(ctx, keys));
    ret_int(regs, i64::from(n > 0));
    Ok(())
}

fn iter_reinit(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let it = this(regs);
    let Some((map, cell)) = iter_parts(ctx, it) else {
        ret_int(regs, 0);
        return Ok(());
    };
    set_pos(ctx, cell, -1);
    let n = storage(ctx, map, k, v).map_or(0, |(keys, _)| len_of(ctx, keys));
    ret_int(regs, i64::from(n > 0));
    Ok(())
}

fn iter_next(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let it = this(regs);
    let Some((map, cell)) = iter_parts(ctx, it) else {
        ret_int(regs, 0);
        return Ok(());
    };
    let n = storage(ctx, map, k, v).map_or(0, |(keys, _)| len_of(ctx, keys)) as i64;
    let p = (pos_of(ctx, cell) + 1).min(n);
    set_pos(ctx, cell, p);
    ret_int(regs, i64::from(p < n));
    Ok(())
}

fn iter_valid(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let ok = current(ctx, this(regs), k, v).is_some();
    ret_int(regs, i64::from(ok));
    Ok(())
}

fn iter_get_key(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let key = current(ctx, this(regs), k, v).and_then(|(map, at)| {
        let (keys, _) = storage(ctx, map, k, v)?;
        key_at(ctx, keys, at)
    });
    match key {
        Some(Key::I(x)) => ret_value(regs, Value::I(x)),
        Some(Key::S(h)) => ret_value(regs, Value::S(h)),
        None if k == Kind::Str => ret_value(regs, Value::S(StrHandle::default())),
        None => ret_value(regs, Value::I(0)),
    }
    Ok(())
}

fn iter_get_value(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let value = current(ctx, this(regs), k, v)
        .and_then(|(map, at)| {
            let (_, vals) = storage(ctx, map, k, v)?;
            Some(value_at(ctx, vals, at, v))
        })
        .unwrap_or(default_value(v));
    ret_value(regs, value);
    Ok(())
}

fn iter_set_value(ctx: &mut ZvmCtx<'_>, regs: &mut NativeRegs<'_>) -> Result<(), Trap> {
    let (k, v) = kinds(ctx);
    let value = value_arg(regs, k, v, false);
    if let Some((map, at)) = current(ctx, this(regs), k, v) {
        if let Some((_, vals)) = storage(ctx, map, k, v) {
            set_value(ctx, vals, Some(at), value);
        }
    }
    Ok(())
}

pub(crate) fn register(t: &mut NativeTable) {
    for k in KEYS {
        for v in VALUES {
            let map = format!("map_{k}_{v}");
            t.register(format!("{map}.insert"), map_insert);
            t.register(format!("{map}.insertnew"), map_insert_new);
            t.register(format!("{map}.get"), map_get);
            t.register(format!("{map}.getifexists"), map_get_if_exists);
            t.register(format!("{map}.checkvalue"), map_check_value);
            t.register(format!("{map}.checkkey"), map_check_key);
            t.register(format!("{map}.remove"), map_remove);
            t.register(format!("{map}.countused"), map_count);
            t.register(format!("{map}.clear"), map_clear);
            t.register(format!("{map}.copy"), map_copy);
            t.register(format!("{map}.move"), map_move);
            t.register(format!("{map}.swap"), map_swap);
            let it = format!("mapiterator_{k}_{v}");
            t.register(format!("{it}.init"), iter_init);
            t.register(format!("{it}.reinit"), iter_reinit);
            t.register(format!("{it}.next"), iter_next);
            t.register(format!("{it}.valid"), iter_valid);
            t.register(format!("{it}.getkey"), iter_get_key);
            t.register(format!("{it}.getvalue"), iter_get_value);
            t.register(format!("{it}.setvalue"), iter_set_value);
        }
    }
}

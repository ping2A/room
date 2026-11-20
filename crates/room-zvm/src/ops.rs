//! Heap-touching opcode semantics shared by the interpreter and the AOT
//! host imports (`docs/37` §5.1), so both tiers run the same code.

use crate::heap::{ObjHandle, ObjKind, StrHandle, ZvmHeap};
use crate::program::CompiledProgram;

/// Why a heap op failed; the caller turns it into a trap with func/line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeapErr {
    /// Stale or null handle.
    Null,
    /// Array index out of range.
    Oob,
}

pub(crate) type HeapResult<T> = Result<T, HeapErr>;

/// `NewObj class_id`: object with the class layout, arrays pre-allocated.
pub(crate) fn new_obj(heap: &mut ZvmHeap, program: &CompiledProgram, class: u32) -> ObjHandle {
    let layout = program
        .classes
        .get(class as usize)
        .cloned()
        .unwrap_or_default();
    let h = heap.new_object_f(class, layout.n_i, layout.n_a, layout.n_s, layout.n_f);
    let made_i: Vec<_> = layout
        .array_i
        .iter()
        .map(|&idx| (idx, heap.new_array_i()))
        .collect();
    let made_s: Vec<_> = layout
        .array_s
        .iter()
        .map(|&idx| (idx, heap.new_array_s()))
        .collect();
    let made_a: Vec<_> = layout
        .array_a
        .iter()
        .map(|&idx| (idx, heap.new_array_a()))
        .collect();
    if let Some(ObjKind::Object { fields_a, .. }) = heap.get_mut(h) {
        for (idx, arr) in made_i.into_iter().chain(made_s).chain(made_a) {
            if let Some(slot) = fields_a.get_mut(idx as usize) {
                *slot = arr;
            }
        }
    }
    h
}

/// `IsNull`.
pub(crate) fn is_null(heap: &ZvmHeap, h: ObjHandle) -> bool {
    h.is_null() || heap.get(h).is_none()
}

/// `IsClass`.
pub(crate) fn is_class(heap: &ZvmHeap, program: &CompiledProgram, h: ObjHandle, want: u32) -> bool {
    match heap.get(h) {
        Some(ObjKind::Object { class_id, .. }) => crate::interp::class_is(program, *class_id, want),
        _ => false,
    }
}

pub(crate) fn ld_i(heap: &ZvmHeap, h: ObjHandle, ext: usize) -> HeapResult<i64> {
    match heap.get(h) {
        Some(ObjKind::Object { fields_i, .. }) => Ok(fields_i.get(ext).copied().unwrap_or(0)),
        None => Err(HeapErr::Null),
        _ => Ok(0),
    }
}

pub(crate) fn ld_a(heap: &ZvmHeap, h: ObjHandle, ext: usize) -> HeapResult<ObjHandle> {
    match heap.get(h) {
        Some(ObjKind::Object { fields_a, .. }) => {
            Ok(fields_a.get(ext).copied().unwrap_or_default())
        }
        None => Err(HeapErr::Null),
        _ => Ok(ObjHandle::NULL),
    }
}

pub(crate) fn ld_s(heap: &ZvmHeap, h: ObjHandle, ext: usize) -> HeapResult<StrHandle> {
    match heap.get(h) {
        Some(ObjKind::Object { fields_s, .. }) => {
            Ok(fields_s.get(ext).copied().unwrap_or_default())
        }
        None => Err(HeapErr::Null),
        _ => Ok(StrHandle::EMPTY),
    }
}

pub(crate) fn ld_f(heap: &ZvmHeap, h: ObjHandle, ext: usize) -> HeapResult<f64> {
    match heap.get(h) {
        Some(ObjKind::Object { fields_f, .. }) => Ok(fields_f.get(ext).copied().unwrap_or(0.0)),
        None => Err(HeapErr::Null),
        _ => Ok(0.0),
    }
}

pub(crate) fn st_i(heap: &mut ZvmHeap, h: ObjHandle, ext: usize, v: i64) -> HeapResult<()> {
    match heap.get_mut(h) {
        Some(ObjKind::Object { fields_i, .. }) => {
            if let Some(slot) = fields_i.get_mut(ext) {
                *slot = v;
            }
            Ok(())
        }
        None => Err(HeapErr::Null),
        _ => Ok(()),
    }
}

pub(crate) fn st_a(heap: &mut ZvmHeap, h: ObjHandle, ext: usize, v: ObjHandle) -> HeapResult<()> {
    match heap.get_mut(h) {
        Some(ObjKind::Object { fields_a, .. }) => {
            if let Some(slot) = fields_a.get_mut(ext) {
                *slot = v;
            }
            Ok(())
        }
        None => Err(HeapErr::Null),
        _ => Ok(()),
    }
}

pub(crate) fn st_s(heap: &mut ZvmHeap, h: ObjHandle, ext: usize, v: StrHandle) -> HeapResult<()> {
    match heap.get_mut(h) {
        Some(ObjKind::Object { fields_s, .. }) => {
            if let Some(slot) = fields_s.get_mut(ext) {
                *slot = v;
            }
            Ok(())
        }
        None => Err(HeapErr::Null),
        _ => Ok(()),
    }
}

pub(crate) fn st_f(heap: &mut ZvmHeap, h: ObjHandle, ext: usize, v: f64) -> HeapResult<()> {
    match heap.get_mut(h) {
        Some(ObjKind::Object { fields_f, .. }) => {
            // Objects made before a layout grew (event proxies) widen here.
            if fields_f.len() <= ext {
                fields_f.resize(ext + 1, 0.0);
            }
            fields_f[ext] = v;
            Ok(())
        }
        None => Err(HeapErr::Null),
        _ => Ok(()),
    }
}

pub(crate) fn arr_len(heap: &ZvmHeap, h: ObjHandle) -> HeapResult<i64> {
    match heap.get(h) {
        Some(ObjKind::ArrayI(v)) => Ok(v.len() as i64),
        Some(ObjKind::ArrayS(v)) => Ok(v.len() as i64),
        Some(ObjKind::ArrayA(v)) => Ok(v.len() as i64),
        None => Err(HeapErr::Null),
        _ => Ok(0),
    }
}

pub(crate) fn arr_get_i(heap: &ZvmHeap, h: ObjHandle, idx: i64) -> HeapResult<i64> {
    match heap.get(h) {
        Some(ObjKind::ArrayI(items)) => {
            if idx < 0 || idx as usize >= items.len() {
                return Err(HeapErr::Oob);
            }
            Ok(items[idx as usize])
        }
        None => Err(HeapErr::Null),
        _ => Ok(0),
    }
}

pub(crate) fn arr_set_i(heap: &mut ZvmHeap, h: ObjHandle, idx: i64, v: i64) -> HeapResult<()> {
    match heap.get_mut(h) {
        Some(ObjKind::ArrayI(items)) => {
            if idx < 0 || idx as usize >= items.len() {
                return Err(HeapErr::Oob);
            }
            items[idx as usize] = v;
            Ok(())
        }
        None => Err(HeapErr::Null),
        _ => Ok(()),
    }
}

pub(crate) fn arr_push_i(heap: &mut ZvmHeap, h: ObjHandle, v: i64) -> HeapResult<()> {
    match heap.get_mut(h) {
        Some(ObjKind::ArrayI(items)) => {
            items.push(v);
            Ok(())
        }
        None => Err(HeapErr::Null),
        _ => Ok(()),
    }
}

pub(crate) fn arr_get_s(heap: &ZvmHeap, h: ObjHandle, idx: i64) -> HeapResult<StrHandle> {
    match heap.get(h) {
        Some(ObjKind::ArrayS(items)) => {
            if idx < 0 || idx as usize >= items.len() {
                return Err(HeapErr::Oob);
            }
            Ok(items[idx as usize])
        }
        None => Err(HeapErr::Null),
        _ => Ok(StrHandle::EMPTY),
    }
}

pub(crate) fn arr_set_s(
    heap: &mut ZvmHeap,
    h: ObjHandle,
    idx: i64,
    v: StrHandle,
) -> HeapResult<()> {
    match heap.get_mut(h) {
        Some(ObjKind::ArrayS(items)) => {
            if idx < 0 || idx as usize >= items.len() {
                return Err(HeapErr::Oob);
            }
            items[idx as usize] = v;
            Ok(())
        }
        None => Err(HeapErr::Null),
        _ => Ok(()),
    }
}

pub(crate) fn arr_push_s(heap: &mut ZvmHeap, h: ObjHandle, v: StrHandle) -> HeapResult<()> {
    match heap.get_mut(h) {
        Some(ObjKind::ArrayS(items)) => {
            items.push(v);
            Ok(())
        }
        None => Err(HeapErr::Null),
        _ => Ok(()),
    }
}

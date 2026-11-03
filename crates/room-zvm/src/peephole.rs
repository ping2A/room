//! Peephole / superinstruction pass (ZS10).

use crate::opcode::{Op, pack, unpack};
use crate::program::{CompiledFunction, CompiledProgram};

/// Optimise all functions in place (deterministic).
pub fn optimize_program(prog: &mut CompiledProgram) {
    for f in &mut prog.functions {
        optimize_function(f);
    }
}

/// Fuse `LoadConstI a,k` + `AddI dest,a,b` when possible into fewer ops.
///
/// Current pass: remove `MoveI a,a` no-ops and fold `LoadConstI` + `MoveI dest,src`
/// when src was just loaded.
pub fn optimize_function(f: &mut CompiledFunction) {
    let mut out = Vec::with_capacity(f.code.len());
    let mut i = 0;
    while i < f.code.len() {
        let (op, a, b, c) = unpack(f.code[i]);
        let Some(opk) = Op::from_u8(op) else {
            out.push(f.code[i]);
            i += 1;
            continue;
        };
        // Drop MoveI a, a
        if opk == Op::MoveI && a == b {
            i += 1;
            continue;
        }
        // LoadConstI a, k ; MoveI d, a  → LoadConstI d, k
        if opk == Op::LoadConstI && i + 2 < f.code.len() {
            let ext = f.code[i + 1];
            let (op2, a2, b2, _) = unpack(f.code[i + 2]);
            if Op::from_u8(op2) == Some(Op::MoveI) && b2 == a {
                out.push(pack(Op::LoadConstI, a2, 0, 0));
                out.push(ext);
                i += 3;
                continue;
            }
        }
        out.push(f.code[i]);
        if has_ext(opk) {
            if i + 1 < f.code.len() {
                out.push(f.code[i + 1]);
            }
            i += 2;
        } else {
            i += 1;
        }
        let _ = (b, c);
    }
    f.code = out;
}

fn has_ext(op: Op) -> bool {
    matches!(
        op,
        Op::LoadConstI
            | Op::LoadConstF
            | Op::LoadConstS
            | Op::Jmp
            | Op::JmpIf
            | Op::JmpIfNot
            | Op::CallStatic
            | Op::CallNative
            | Op::NewObj
            | Op::LoadFieldI
            | Op::StoreFieldI
            | Op::LoadFieldA
            | Op::StoreFieldA
            | Op::LoadFieldS
            | Op::StoreFieldS
            | Op::Trap
            | Op::IsClass
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::interp::Zvm;

    #[test]
    fn peephole_preserves_add() {
        let mut f = CompiledFunction {
            name: "add".into(),
            n_i: 4,
            ..Default::default()
        };
        let c2 = f.intern_i(2);
        let c3 = f.intern_i(3);
        f.emit_ext(Op::LoadConstI, 0, 0, 0, c2);
        f.emit(Op::MoveI, 2, 0, 0); // redundant move
        f.emit_ext(Op::LoadConstI, 1, 0, 0, c3);
        f.emit(Op::AddI, 0, 2, 1);
        f.emit(Op::Return, 0, 0, 0);
        let before = f.code.len();
        optimize_function(&mut f);
        assert!(f.code.len() <= before);
        let mut p = CompiledProgram {
            functions: vec![f],
            ..Default::default()
        };
        p.finalize();
        assert_eq!(Zvm::new(p).call("add").unwrap(), 5);
    }
}

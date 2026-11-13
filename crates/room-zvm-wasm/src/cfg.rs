//! Decoding, basic blocks, CFG, dominators (`docs/37` §4.1).

use crate::Unsupported;
use room_zvm::{CompiledFunction, Op, block_costs, has_ext, jump_target, unpack};

/// One decoded instruction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Insn {
    /// Word index of the opcode.
    pub pc: u32,
    pub op: Op,
    pub a: u8,
    pub b: u8,
    pub c: u8,
    /// Extension word, when the op has one.
    pub ext: u32,
}

/// How control leaves a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Term {
    /// Unconditional edge (`Jmp`, or falling into the next block / exit).
    Goto(usize),
    /// `i[reg] != 0` → `if_true`, else `if_false`.
    Cond {
        reg: u8,
        if_true: usize,
        if_false: usize,
    },
    /// `Ret`, `Return` or `Trap`: emitted by the body.
    Leave,
}

/// A basic block. Block `blocks.len() - 1` is the synthetic implicit-return exit.
#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub insns: Vec<Insn>,
    /// Fuel charged on entry (number of instructions; 0 for the exit).
    pub cost: u32,
    pub term: Term,
}

/// Function CFG with dominance information.
#[derive(Debug)]
pub(crate) struct Cfg {
    pub blocks: Vec<Block>,
    pub exit: usize,
    /// Reverse postorder index; `usize::MAX` when unreachable.
    pub rpo: Vec<usize>,
    pub forward_in: Vec<u32>,
    pub loop_header: Vec<bool>,
    pub dom_children: Vec<Vec<usize>>,
}

pub(crate) fn decode(f: &CompiledFunction) -> Result<Vec<Insn>, Unsupported> {
    let code = &f.code;
    let mut out = Vec::new();
    let mut pc = 0usize;
    while pc < code.len() {
        let (op_b, a, b, c) = unpack(code[pc]);
        let op = Op::from_u8(op_b).ok_or(Unsupported::BadOpcode(op_b))?;
        let ext = if has_ext(op) {
            *code.get(pc + 1).ok_or(Unsupported::TruncatedExt)?
        } else {
            0
        };
        if matches!(op, Op::CallStatic | Op::CallNative) {
            return Err(Unsupported::LegacyCall);
        }
        out.push(Insn {
            pc: pc as u32,
            op,
            a,
            b,
            c,
            ext,
        });
        pc += 1 + usize::from(has_ext(op));
    }
    Ok(out)
}

pub(crate) fn build(f: &CompiledFunction) -> Result<Cfg, Unsupported> {
    let insns = decode(f)?;
    let n = f.code.len();
    let costs = block_costs(&f.code);
    let mut start_of = vec![usize::MAX; n + 1];
    // Map word index → block index for leaders.
    let mut blocks: Vec<Block> = Vec::new();
    let mut is_start = vec![false; n];
    for i in &insns {
        is_start[i.pc as usize] = true;
    }
    for (k, i) in insns.iter().enumerate() {
        let pc = i.pc as usize;
        if costs[pc] > 0 || blocks.is_empty() {
            start_of[pc] = blocks.len();
            blocks.push(Block {
                insns: Vec::new(),
                cost: costs[pc],
                term: Term::Leave,
            });
        }
        let _ = k;
        if let Some(b) = blocks.last_mut() {
            b.insns.push(*i);
        }
    }
    let exit = blocks.len();
    blocks.push(Block {
        insns: Vec::new(),
        cost: 0,
        term: Term::Leave,
    });
    let target_block = |t: usize| -> Result<usize, Unsupported> {
        if t >= n {
            return Ok(exit);
        }
        if !is_start[t] || start_of[t] == usize::MAX {
            return Err(Unsupported::BadJumpTarget);
        }
        Ok(start_of[t])
    };
    for bi in 0..exit {
        let last = *blocks[bi].insns.last().ok_or(Unsupported::BadJumpTarget)?;
        let next_pc = last.pc as usize + 1 + usize::from(has_ext(last.op));
        let fall = if bi + 1 < exit { bi + 1 } else { exit };
        let term = match last.op {
            Op::Jmp => Term::Goto(target_block(jump_target(next_pc, last.ext))?),
            Op::JmpIf | Op::JmpIfNot => {
                let taken = target_block(jump_target(next_pc, last.ext))?;
                let (if_true, if_false) = if last.op == Op::JmpIf {
                    (taken, fall)
                } else {
                    (fall, taken)
                };
                Term::Cond {
                    reg: last.a,
                    if_true,
                    if_false,
                }
            }
            Op::Ret | Op::Return | Op::Trap => Term::Leave,
            _ => Term::Goto(fall),
        };
        blocks[bi].term = term;
    }
    let nb = blocks.len();
    let mut succ = vec![Vec::new(); nb];
    for (bi, b) in blocks.iter().enumerate() {
        match b.term {
            Term::Goto(t) => succ[bi].push(t),
            Term::Cond {
                if_true, if_false, ..
            } => {
                succ[bi].push(if_true);
                succ[bi].push(if_false);
            }
            Term::Leave => {}
        }
    }
    // Reverse postorder (iterative DFS from block 0).
    let mut post = Vec::with_capacity(nb);
    let mut seen = vec![false; nb];
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
    seen[0] = true;
    while let Some(&mut (node, ref mut k)) = stack.last_mut() {
        if *k < succ[node].len() {
            let s = succ[node][*k];
            *k += 1;
            if !seen[s] {
                seen[s] = true;
                stack.push((s, 0));
            }
        } else {
            post.push(node);
            stack.pop();
        }
    }
    let order: Vec<usize> = post.into_iter().rev().collect();
    let mut rpo = vec![usize::MAX; nb];
    for (i, &b) in order.iter().enumerate() {
        rpo[b] = i;
    }
    let mut preds = vec![Vec::new(); nb];
    for (bi, ss) in succ.iter().enumerate() {
        if rpo[bi] == usize::MAX {
            continue;
        }
        for &s in ss {
            preds[s].push(bi);
        }
    }
    // Cooper–Harvey–Kennedy.
    let mut idom = vec![usize::MAX; nb];
    idom[0] = 0;
    let mut changed = true;
    while changed {
        changed = false;
        for &b in order.iter().skip(1) {
            let mut new = usize::MAX;
            for &p in &preds[b] {
                if idom[p] == usize::MAX {
                    continue;
                }
                new = if new == usize::MAX {
                    p
                } else {
                    intersect(&idom, &rpo, p, new)
                };
            }
            if new != usize::MAX && idom[b] != new {
                idom[b] = new;
                changed = true;
            }
        }
    }
    let dominates = |a: usize, mut b: usize| -> bool {
        loop {
            if a == b {
                return true;
            }
            if b == 0 || idom[b] == usize::MAX {
                return false;
            }
            b = idom[b];
        }
    };
    let mut forward_in = vec![0u32; nb];
    let mut loop_header = vec![false; nb];
    for (bi, ss) in succ.iter().enumerate() {
        if rpo[bi] == usize::MAX {
            continue;
        }
        for &s in ss {
            if rpo[s] <= rpo[bi] {
                if !dominates(s, bi) {
                    return Err(Unsupported::Irreducible);
                }
                loop_header[s] = true;
            } else {
                forward_in[s] += 1;
            }
        }
    }
    let mut dom_children = vec![Vec::new(); nb];
    for &b in order.iter().skip(1) {
        dom_children[idom[b]].push(b);
    }
    Ok(Cfg {
        blocks,
        exit,
        rpo,
        forward_in,
        loop_header,
        dom_children,
    })
}

fn intersect(idom: &[usize], rpo: &[usize], mut a: usize, mut b: usize) -> usize {
    while a != b {
        while rpo[a] > rpo[b] {
            a = idom[a];
        }
        while rpo[b] > rpo[a] {
            b = idom[b];
        }
    }
    a
}

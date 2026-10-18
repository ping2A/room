//! A tiny ACS0 assembler for the scripts converted maps need: special
//! calls, texture changes, actor state jumps, delays, run-once blocks and the
//! floor-door toggle.

use std::collections::BTreeMap;

/// One script step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Op {
    /// `special(args…)`.
    Special(i32, [i32; 5]),
    /// `SetLineTexture(line, side, position, texture)`.
    LineTexture {
        line: i32,
        side: i32,
        position: i32,
        texture: String,
    },
    /// `ChangeFloor(tag, flat)` / `ChangeCeiling(tag, flat)`.
    Flat {
        tag: i32,
        texture: String,
        ceiling: bool,
    },
    /// `SetActorState(tid, state)`.
    ActorState { tid: i32, state: String },
    /// `Delay(tics)`.
    Delay(i32),
    /// `ops` run only the first time: map variable `var` remembers it.
    Once { var: i32, ops: Vec<Op> },
    /// Floor door: when the floor of `tag` is shut against its ceiling,
    /// `open`; otherwise `close`.
    IfShut {
        tag: i32,
        open: Vec<Op>,
        close: Vec<Op>,
    },
}

/// An ACS script: number and steps.
pub(crate) type AcsScript = (i32, Vec<Op>);

const TERMINATE: i32 = 1;
const PUSHNUMBER: i32 = 3;
const LSPEC5DIRECT: i32 = 13;
const SUBTRACT: i32 = 15;
const GT: i32 = 22;
const ASSIGNMAPVAR: i32 = 26;
const PUSHMAPVAR: i32 = 29;
const GOTO: i32 = 52;
const IFGOTO: i32 = 53;
const DROP: i32 = 54;
const DELAYDIRECT: i32 = 56;
const CHANGEFLOORDIRECT: i32 = 66;
const CHANGECEILINGDIRECT: i32 = 68;
const SETLINETEXTURE: i32 = 97;
const GETSECTORFLOORZ: i32 = 261;
const GETSECTORCEILINGZ: i32 = 262;
const SETACTORSTATE: i32 = 334;
/// One map unit in ACS fixed point.
const FRACUNIT: i32 = 65536;

struct Asm {
    code: Vec<i32>,
    strings: Vec<String>,
    string_ids: BTreeMap<String, i32>,
}

impl Asm {
    /// File offset of the next code word.
    fn here(&self) -> i32 {
        i32::try_from(8 + 4 * self.code.len()).unwrap_or(i32::MAX)
    }

    fn string(&mut self, s: &str) -> i32 {
        if let Some(&i) = self.string_ids.get(s) {
            return i;
        }
        let i = i32::try_from(self.strings.len()).unwrap_or(0);
        self.strings.push(s.to_owned());
        self.string_ids.insert(s.to_owned(), i);
        i
    }

    fn push(&mut self, v: i32) {
        self.code.extend_from_slice(&[PUSHNUMBER, v]);
    }

    /// Emit a jump whose target is patched later; returns the operand slot.
    fn jump(&mut self, op: i32) -> usize {
        self.code.extend_from_slice(&[op, 0]);
        self.code.len() - 1
    }

    fn land(&mut self, slot: usize) {
        self.code[slot] = self.here();
    }

    fn ops(&mut self, ops: &[Op]) {
        for op in ops {
            self.op(op);
        }
    }

    fn op(&mut self, op: &Op) {
        match op {
            Op::Special(special, args) => {
                self.code.extend_from_slice(&[LSPEC5DIRECT, *special]);
                self.code.extend_from_slice(args);
            }
            Op::LineTexture {
                line,
                side,
                position,
                texture,
            } => {
                let t = self.string(texture);
                for v in [*line, *side, *position, t] {
                    self.push(v);
                }
                self.code.push(SETLINETEXTURE);
            }
            Op::Flat {
                tag,
                texture,
                ceiling,
            } => {
                let t = self.string(texture);
                let pcd = if *ceiling {
                    CHANGECEILINGDIRECT
                } else {
                    CHANGEFLOORDIRECT
                };
                self.code.extend_from_slice(&[pcd, *tag, t]);
            }
            Op::ActorState { tid, state } => {
                let s = self.string(state);
                for v in [*tid, s, 0] {
                    self.push(v);
                }
                self.code.extend_from_slice(&[SETACTORSTATE, DROP]);
            }
            Op::Delay(tics) => self.code.extend_from_slice(&[DELAYDIRECT, (*tics).max(1)]),
            Op::Once { var, ops } => {
                self.code.extend_from_slice(&[PUSHMAPVAR, *var]);
                let skip = self.jump(IFGOTO);
                self.push(1);
                self.code.extend_from_slice(&[ASSIGNMAPVAR, *var]);
                self.ops(ops);
                self.land(skip);
            }
            Op::IfShut { tag, open, close } => {
                for pcd in [GETSECTORCEILINGZ, GETSECTORFLOORZ] {
                    for v in [*tag, 0, 0] {
                        self.push(v);
                    }
                    self.code.push(pcd);
                }
                self.code.push(SUBTRACT);
                self.push(FRACUNIT);
                self.code.push(GT);
                let to_close = self.jump(IFGOTO);
                self.ops(open);
                let done = self.jump(GOTO);
                self.land(to_close);
                self.ops(close);
                self.land(done);
            }
        }
    }
}

/// ACS0 `BEHAVIOR` lump holding `scripts` (numbers ≥ 1000 are `OPEN`).
pub(crate) fn assemble(scripts: &[AcsScript]) -> Vec<u8> {
    let mut asm = Asm {
        code: Vec::new(),
        strings: Vec::new(),
        string_ids: BTreeMap::new(),
    };
    let mut dir = Vec::new();
    for (number, ops) in scripts {
        dir.push((*number, asm.here()));
        asm.ops(ops);
        asm.code.push(TERMINATE);
    }
    let mut out = b"ACS\0".to_vec();
    out.extend_from_slice(&[0; 4]);
    for c in &asm.code {
        out.extend_from_slice(&c.to_le_bytes());
    }
    let mut string_offsets = Vec::new();
    for s in &asm.strings {
        string_offsets.push(u32::try_from(out.len()).unwrap_or(0));
        out.extend_from_slice(s.as_bytes());
        out.push(0);
    }
    while out.len() % 4 != 0 {
        out.push(0);
    }
    let dir_off = u32::try_from(out.len()).unwrap_or(0);
    out[4..8].copy_from_slice(&dir_off.to_le_bytes());
    out.extend_from_slice(&u32::try_from(dir.len()).unwrap_or(0).to_le_bytes());
    for (number, offset) in dir {
        out.extend_from_slice(&number.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out.extend_from_slice(
        &u32::try_from(string_offsets.len())
            .unwrap_or(0)
            .to_le_bytes(),
    );
    for o in string_offsets {
        out.extend_from_slice(&o.to_le_bytes());
    }
    while out.len() < 32 {
        out.push(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_follow_the_code_and_jumps_land_after_blocks() {
        let b = assemble(&[(
            3,
            vec![Op::Once {
                var: 2,
                ops: vec![Op::Flat {
                    tag: 7,
                    texture: "BT0416".into(),
                    ceiling: false,
                }],
            }],
        )]);
        let word = |o: usize| i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        // PUSHMAPVAR 2, IFGOTO end, PUSH 1, ASSIGNMAPVAR 2, CHANGEFLOORDIRECT 7 0, TERMINATE.
        assert_eq!([word(8), word(12), word(16)], [PUSHMAPVAR, 2, IFGOTO]);
        assert_eq!(word(20), 8 + 4 * 11, "jump past the block");
        assert_eq!(word(8 + 4 * 11), TERMINATE);
        let dir = word(4) as usize;
        let string_count = dir + 4 + 12;
        assert_eq!(word(string_count), 1);
        let s = word(string_count + 4) as usize;
        assert_eq!(&b[s..s + 7], b"BT0416\0");
    }
}

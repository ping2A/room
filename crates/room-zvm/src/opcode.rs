//! Bytecode opcodes (`docs/34` §4.1).

/// 8-bit opcode packed in the high byte of a u32 instruction.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// No-op.
    Nop = 0,
    /// `i[a] = i[b]`
    MoveI = 1,
    /// `f[a] = f[b]`
    MoveF = 2,
    /// `s[a] = s[b]`
    MoveS = 3,
    /// `a[a] = a[b]`
    MoveA = 4,
    /// `i[a] = const_i[ext]`
    LoadConstI = 5,
    /// `f[a] = const_f[ext]`
    LoadConstF = 6,
    /// `s[a] = const_s[ext]`
    LoadConstS = 7,
    /// `a[a] = 0` (null)
    LoadNull = 8,
    /// `i[a] = i[b] + i[c]`
    AddI = 10,
    /// `i[a] = i[b] - i[c]`
    SubI = 11,
    /// `i[a] = i[b] * i[c]`
    MulI = 12,
    /// `i[a] = i[b] / i[c]` (trap on 0)
    DivI = 13,
    /// `i[a] = i[b] % i[c]`
    ModI = 14,
    /// `i[a] = i[b] < i[c]`
    LtI = 15,
    /// `i[a] = i[b] <= i[c]`
    LeI = 16,
    /// `i[a] = i[b] == i[c]`
    EqI = 17,
    /// `i[a] = i[b] != i[c]`
    NeI = 18,
    /// `i[a] = i[b] > i[c]`
    GtI = 19,
    /// `i[a] = i[b] >= i[c]`
    GeI = 20,
    /// `f[a] = i[b] as f64`
    I2F = 21,
    /// `i[a] = f[b] as i64` (trunc toward zero)
    F2I = 22,
    /// `f[a] = f[b] + f[c]`
    AddF = 23,
    /// `f[a] = f[b] - f[c]`
    SubF = 24,
    /// `f[a] = f[b] * f[c]`
    MulF = 25,
    /// `f[a] = f[b] / f[c]` (trap on 0)
    DivF = 26,
    /// `pc = ext` (signed relative in word units from next insn)
    Jmp = 30,
    /// if `i[a] != 0` then `pc += ext`
    JmpIf = 31,
    /// if `i[a] == 0` then `pc += ext`
    JmpIfNot = 32,
    /// call static function `ext`, args from regs starting at `a`, count `b`, ret bank `c`
    CallStatic = 40,
    /// call native `ext`
    CallNative = 41,
    /// return; optional value in bank `a` reg `b`
    Return = 42,
    /// `a[a] = new object class_id=ext`
    NewObj = 50,
    /// `i[a] = field_i(a[b], ext)`
    LoadFieldI = 51,
    /// `field_i(a[a], ext) = i[b]`
    StoreFieldI = 52,
    /// `a[a] = field_a(a[b], ext)`
    LoadFieldA = 53,
    /// `field_a(a[a], ext) = a[b]`
    StoreFieldA = 54,
    /// `s[a] = field_s(a[b], ext)`
    LoadFieldS = 55,
    /// `field_s(a[a], ext) = s[b]`
    StoreFieldS = 56,
    /// `f[a] = obj(a-bank b).fields_f[ext]`
    LoadFieldF = 57,
    /// `obj(a-bank a).fields_f[ext] = f[b]`
    StoreFieldF = 58,
    /// `rets[c] = bank a reg b` (extra results of a multi-return).
    SetRet = 59,
    /// `i[a] = array_len(a[b])`
    ArrayLen = 60,
    /// `i[a] = array_get_i(a[b], i[c])`
    ArrayGetI = 61,
    /// `array_set_i(a[a], i[b], i[c])`
    ArraySetI = 62,
    /// `array_push_i(a[a], i[b])`
    ArrayPushI = 63,
    /// `s[a] = array_get_s(a[b], i[c])`
    ArrayGetS = 64,
    /// `array_push_s(a[a], s[b])`
    ArrayPushS = 65,
    /// `array_set_s(a[a], i[b], s[c])`
    ArraySetS = 66,
    /// `s[a] = s[b] .. s[c]`
    ConcatS = 70,
    /// `i[a] = (a[b] == null)`
    IsNull = 71,
    /// explicit trap with const string ext
    Trap = 80,
    /// `i[a] = i[b] & i[c]`
    BitAnd = 90,
    /// `i[a] = i[b] | i[c]`
    BitOr = 91,
    /// `i[a] = i[b] ^ i[c]`
    BitXor = 92,
    /// `i[a] = i[b] << (i[c] & 63)`
    Shl = 93,
    /// `i[a] = i[b] >> (i[c] & 63)` (arithmetic)
    Shr = 94,
    /// `i[a] = 1` when `a[b]` is an instance of class `ext` (walks parents).
    IsClass = 95,
    /// Push `bank a`, register `b` onto the argument stack. Banks: 0=i 1=f 2=s 3=a.
    Arg = 96,
    /// Call function `ext`. Args were pushed with [`Self::Arg`]. `a` is the return count.
    Call = 97,
    /// Virtual call. `ext` is the vtable slot. The first object arg is the receiver.
    CallVirt = 98,
    /// Native call. `ext` is the runtime native id. Args were pushed with [`Self::Arg`].
    CallNat = 99,
    /// Set return slot `c` from `bank a`, register `b`, and return from the function.
    Ret = 100,
    /// Copy return slot `c` into `bank a`, register `b`.
    GetRet = 101,
    /// `i[a] = -1 / 0 / 1` from comparing floating-point values `f[b]`, `f[c]`.
    CmpF = 102,
}

impl Op {
    /// Decode from the high byte of an instruction word.
    #[must_use]
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Nop,
            1 => Self::MoveI,
            2 => Self::MoveF,
            3 => Self::MoveS,
            4 => Self::MoveA,
            5 => Self::LoadConstI,
            6 => Self::LoadConstF,
            7 => Self::LoadConstS,
            8 => Self::LoadNull,
            10 => Self::AddI,
            11 => Self::SubI,
            12 => Self::MulI,
            13 => Self::DivI,
            14 => Self::ModI,
            15 => Self::LtI,
            16 => Self::LeI,
            17 => Self::EqI,
            18 => Self::NeI,
            19 => Self::GtI,
            20 => Self::GeI,
            21 => Self::I2F,
            22 => Self::F2I,
            23 => Self::AddF,
            24 => Self::SubF,
            25 => Self::MulF,
            26 => Self::DivF,
            30 => Self::Jmp,
            31 => Self::JmpIf,
            32 => Self::JmpIfNot,
            40 => Self::CallStatic,
            41 => Self::CallNative,
            42 => Self::Return,
            50 => Self::NewObj,
            51 => Self::LoadFieldI,
            52 => Self::StoreFieldI,
            53 => Self::LoadFieldA,
            54 => Self::StoreFieldA,
            55 => Self::LoadFieldS,
            57 => Self::LoadFieldF,
            58 => Self::StoreFieldF,
            59 => Self::SetRet,
            56 => Self::StoreFieldS,
            60 => Self::ArrayLen,
            61 => Self::ArrayGetI,
            62 => Self::ArraySetI,
            63 => Self::ArrayPushI,
            64 => Self::ArrayGetS,
            65 => Self::ArrayPushS,
            66 => Self::ArraySetS,
            70 => Self::ConcatS,
            71 => Self::IsNull,
            80 => Self::Trap,
            90 => Self::BitAnd,
            91 => Self::BitOr,
            92 => Self::BitXor,
            93 => Self::Shl,
            94 => Self::Shr,
            95 => Self::IsClass,
            96 => Self::Arg,
            97 => Self::Call,
            98 => Self::CallVirt,
            99 => Self::CallNat,
            100 => Self::Ret,
            101 => Self::GetRet,
            102 => Self::CmpF,
            _ => return None,
        })
    }

    /// True when the instruction is followed by an extension word.
    #[must_use]
    pub fn needs_ext(self) -> bool {
        matches!(
            self,
            Self::LoadConstI
                | Self::LoadConstF
                | Self::LoadConstS
                | Self::Jmp
                | Self::JmpIf
                | Self::JmpIfNot
                | Self::CallStatic
                | Self::CallNative
                | Self::NewObj
                | Self::LoadFieldI
                | Self::StoreFieldI
                | Self::LoadFieldA
                | Self::StoreFieldA
                | Self::LoadFieldS
                | Self::StoreFieldS
                | Self::LoadFieldF
                | Self::StoreFieldF
                | Self::Trap
                | Self::IsClass
                | Self::Call
                | Self::CallVirt
                | Self::CallNat
        )
    }
}

/// True when `op` is followed by an extension word.
///
/// Shared by the interpreter, the peephole pass and the WASM code generator
/// (`docs/37` §3) so they cannot disagree on instruction boundaries.
#[must_use]
pub fn has_ext(op: Op) -> bool {
    op.needs_ext()
}

/// Pack `op | a | b | c` into one word.
#[must_use]
pub fn pack(op: Op, a: u8, b: u8, c: u8) -> u32 {
    (u32::from(op as u8) << 24) | (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c)
}

/// Unpack one word.
#[must_use]
pub fn unpack(word: u32) -> (u8, u8, u8, u8) {
    (
        (word >> 24) as u8,
        ((word >> 16) & 0xff) as u8,
        ((word >> 8) & 0xff) as u8,
        (word & 0xff) as u8,
    )
}

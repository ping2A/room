//! Binary form of a [`CompiledProgram`] (`docs/37` ZW6): what a converted mod
//! ships next to its wasm code, so the game can run it without the ZScript
//! compiler. Little-endian, length-prefixed, versioned.

use crate::program::{ClassLayout, CompiledFunction, CompiledProgram, FuncParam};

const MAGIC: &[u8; 4] = b"ZVMP";
/// 2: classes carry `array_a` (object-array fields). 1 is still read.
const VERSION: u32 = 2;

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn len(&mut self, n: usize) {
        self.u32(n as u32);
    }
    fn str(&mut self, s: &str) {
        self.len(s.len());
        self.0.extend_from_slice(s.as_bytes());
    }
}

struct R<'a> {
    b: &'a [u8],
    at: usize,
}

type Res<T> = Result<T, String>;

impl R<'_> {
    fn take(&mut self, n: usize) -> Res<&[u8]> {
        let end = self.at.checked_add(n).ok_or("program image: overflow")?;
        let s = self
            .b
            .get(self.at..end)
            .ok_or_else(|| format!("program image truncated at byte {}", self.at))?;
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Res<u32> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Res<u64> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(u64::from_le_bytes(a))
    }
    fn len(&mut self) -> Res<usize> {
        let n = self.u32()? as usize;
        // Every element takes at least one byte: reject absurd lengths early.
        if n > self.b.len() {
            return Err(format!("program image: bad length {n}"));
        }
        Ok(n)
    }
    fn str(&mut self) -> Res<String> {
        let n = self.len()?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|e| e.to_string())
    }
}

impl CompiledProgram {
    /// Serialize everything the VM needs (functions, classes, vtables, native
    /// import names, `States`/`Default` bodies). Derived maps are rebuilt on
    /// load by [`CompiledProgram::finalize`].
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = W(Vec::new());
        w.0.extend_from_slice(MAGIC);
        w.u32(VERSION);
        w.len(self.functions.len());
        for f in &self.functions {
            w.str(&f.name);
            w.str(&f.file);
            w.u8(f.n_i);
            w.u8(f.n_f);
            w.u8(f.n_s);
            w.u8(f.n_a);
            w.len(f.code.len());
            for x in &f.code {
                w.u32(*x);
            }
            w.len(f.const_i.len());
            for x in &f.const_i {
                w.u64(*x as u64);
            }
            w.len(f.const_f.len());
            for x in &f.const_f {
                w.u64(x.to_bits());
            }
            w.len(f.const_s.len());
            for x in &f.const_s {
                w.str(x);
            }
            w.len(f.lines.len());
            for x in &f.lines {
                w.u32(*x);
            }
            w.len(f.params.len());
            for p in &f.params {
                w.u8(p.bank);
                w.u16(p.reg);
            }
        }
        w.len(self.classes.len());
        for c in &self.classes {
            w.str(&c.name);
            w.str(&c.parent);
            w.u16(c.n_i);
            w.u16(c.n_a);
            w.u16(c.n_s);
            w.u16(c.n_f);
            w.len(c.array_i.len());
            for x in &c.array_i {
                w.u16(*x);
            }
            w.len(c.array_s.len());
            for x in &c.array_s {
                w.u16(*x);
            }
            w.len(c.array_a.len());
            for x in &c.array_a {
                w.u16(*x);
            }
            w.len(c.fields.len());
            for (k, (bank, idx)) in &c.fields {
                w.str(k);
                w.u8(*bank);
                w.u16(*idx);
            }
        }
        w.len(self.native_imports.len());
        for n in &self.native_imports {
            w.str(n);
        }
        for map in [&self.class_states, &self.class_defaults] {
            w.len(map.len());
            for (k, v) in map {
                w.str(k);
                w.str(v);
            }
        }
        w.len(self.vtables.len());
        for (class, slots) in &self.vtables {
            w.u32(*class);
            w.len(slots.len());
            for s in slots {
                w.u32(*s);
            }
        }
        w.0
    }

    /// Inverse of [`CompiledProgram::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut r = R { b: bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return Err("not a ZVM program image".into());
        }
        let version = r.u32()?;
        if version != VERSION && version != 1 {
            return Err(format!(
                "ZVM program image version {version}, expected {VERSION}"
            ));
        }
        let mut p = CompiledProgram::default();
        for _ in 0..r.len()? {
            let mut f = CompiledFunction {
                name: r.str()?,
                file: r.str()?,
                n_i: r.u8()?,
                n_f: r.u8()?,
                n_s: r.u8()?,
                n_a: r.u8()?,
                ..Default::default()
            };
            for _ in 0..r.len()? {
                f.code.push(r.u32()?);
            }
            for _ in 0..r.len()? {
                f.const_i.push(r.u64()? as i64);
            }
            for _ in 0..r.len()? {
                f.const_f.push(f64::from_bits(r.u64()?));
            }
            for _ in 0..r.len()? {
                f.const_s.push(r.str()?);
            }
            for _ in 0..r.len()? {
                f.lines.push(r.u32()?);
            }
            for _ in 0..r.len()? {
                f.params.push(FuncParam {
                    bank: r.u8()?,
                    reg: r.u16()?,
                });
            }
            p.functions.push(f);
        }
        for _ in 0..r.len()? {
            let mut c = ClassLayout {
                name: r.str()?,
                parent: r.str()?,
                n_i: r.u16()?,
                n_a: r.u16()?,
                n_s: r.u16()?,
                n_f: r.u16()?,
                ..Default::default()
            };
            for _ in 0..r.len()? {
                c.array_i.push(r.u16()?);
            }
            for _ in 0..r.len()? {
                c.array_s.push(r.u16()?);
            }
            if version >= 2 {
                for _ in 0..r.len()? {
                    c.array_a.push(r.u16()?);
                }
            }
            for _ in 0..r.len()? {
                let k = r.str()?;
                let bank = r.u8()?;
                let idx = r.u16()?;
                c.fields.insert(k, (bank, idx));
            }
            p.classes.push(c);
        }
        for _ in 0..r.len()? {
            p.native_imports.push(r.str()?);
        }
        for _ in 0..r.len()? {
            let k = r.str()?;
            let v = r.str()?;
            p.class_states.insert(k, v);
        }
        for _ in 0..r.len()? {
            let k = r.str()?;
            let v = r.str()?;
            p.class_defaults.insert(k, v);
        }
        for _ in 0..r.len()? {
            let class = r.u32()?;
            let mut slots = Vec::new();
            for _ in 0..r.len()? {
                slots.push(r.u32()?);
            }
            p.vtables.insert(class, slots);
        }
        if r.at != bytes.len() {
            return Err("program image has trailing bytes".into());
        }
        p.finalize();
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::opcode::Op;

    #[test]
    fn round_trip_keeps_everything() {
        let mut f = CompiledFunction {
            name: "C.m".into(),
            file: "c.zs".into(),
            n_i: 3,
            n_f: 1,
            n_s: 2,
            n_a: 1,
            params: vec![FuncParam { bank: 3, reg: 0 }],
            lines: vec![7, 7, 8],
            ..Default::default()
        };
        let k = f.intern_i(-5);
        f.intern_f(f64::NAN);
        f.intern_s("héllo");
        f.emit_ext(Op::LoadConstI, 0, 0, 0, k);
        f.emit(Op::Return, 0, 0, 0);
        let mut c = ClassLayout {
            name: "C".into(),
            parent: "Object".into(),
            n_i: 2,
            array_i: vec![0],
            ..Default::default()
        };
        c.fields.insert("x".into(), (0, 1));
        let mut p = CompiledProgram {
            functions: vec![f],
            classes: vec![c],
            native_imports: vec!["Console.Printf".into()],
            ..Default::default()
        };
        p.class_states.insert("C".into(), "Spawn: TNT1 A 1".into());
        p.class_defaults.insert("C".into(), "+SOLID".into());
        p.vtables.insert(0, vec![0, 0]);
        p.finalize();
        let q = CompiledProgram::from_bytes(&p.to_bytes()).unwrap();
        assert_eq!(q.to_bytes(), p.to_bytes());
        assert_eq!(q.program_hash, p.program_hash);
        assert_eq!(q.func_by_name, p.func_by_name);
        assert!(q.functions[0].const_f[0].is_nan());
        assert!(CompiledProgram::from_bytes(&p.to_bytes()[..20]).is_err());
    }
}

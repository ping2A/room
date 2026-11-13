//! Differential conformance: interpreter vs AOT (`docs/37` §8.2).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use room_zvm::{CompiledFunction, FuncParam, ObjHandle, Op, Trap, Zvm};

/// Everything a call can observably change.
#[derive(Debug, PartialEq)]
pub(crate) struct Outcome {
    result: Result<i64, Trap>,
    last_i: [i64; 4],
    last_f: u64,
    last_s: String,
    fuel_left: u64,
    heap: u64,
    play: String,
}

pub(crate) fn outcome(vm: &Zvm, result: Result<i64, Trap>) -> Outcome {
    Outcome {
        result,
        last_i: vm.last_i,
        last_f: vm.last_f.to_bits(),
        last_s: vm.last_s.clone(),
        fuel_left: vm.fuel_left(),
        heap: vm.heap.stable_digest(),
        play: format!("{:?}", vm.play),
    }
}

/// Interpreter and AOT VMs over the same linked program.
pub(crate) struct Pair {
    pub interp: Zvm,
    pub aot: Zvm,
    pub art: Artifact,
}

impl Pair {
    pub(crate) fn new(program: CompiledProgram) -> Self {
        let interp = Zvm::new(program.clone());
        let mut aot = Zvm::new(program);
        let art = compile(&aot.program, &Options::default()).expect("codegen");
        aot.attach_aot(&art.bytes, art.compiled.clone())
            .expect("attach");
        aot.set_aot_enabled(true);
        Self { interp, aot, art }
    }

    /// Run `f` on both VMs and require identical outcomes.
    pub(crate) fn check(
        &mut self,
        what: &str,
        f: impl Fn(&mut Zvm) -> Result<i64, Trap>,
    ) -> Outcome {
        let before = self.aot.aot_calls();
        let ra = f(&mut self.interp);
        let rb = f(&mut self.aot);
        let a = outcome(&self.interp, ra);
        let b = outcome(&self.aot, rb);
        assert_eq!(a, b, "{what}: interpreter vs AOT");
        let _ = before;
        a
    }

    pub(crate) fn compiled(&self, name: &str) -> bool {
        let idx = self.aot.program.func_by_name[name] as usize;
        self.art.compiled[idx]
    }
}

fn prog(funcs: Vec<CompiledFunction>) -> CompiledProgram {
    let mut p = CompiledProgram {
        functions: funcs,
        ..Default::default()
    };
    p.finalize();
    p
}

fn func(name: &str, n_i: u8, n_f: u8, n_s: u8, n_a: u8) -> CompiledFunction {
    CompiledFunction {
        name: name.into(),
        n_i,
        n_f,
        n_s,
        n_a,
        ..Default::default()
    }
}

#[test]
fn module_validates_and_lists_functions() {
    let mut f = func("add", 3, 0, 0, 0);
    let c = f.intern_i(40);
    f.emit_ext(Op::LoadConstI, 1, 0, 0, c);
    f.emit(Op::AddI, 0, 0, 1);
    f.emit(Op::Return, 0, 0, 0);
    let art = compile(&prog(vec![f]), &Options::default()).unwrap();
    assert_eq!(art.compiled, vec![true]);
    let wat = to_wat(&art.bytes).unwrap();
    assert!(wat.contains("(export \"f0\""), "{wat}");
}

#[test]
fn arithmetic_loop_and_traps_match() {
    // sum = 0; i = 0; while (i < n) { sum += i * k / d; i++ } return sum
    let mut f = func("loop", 8, 0, 0, 0);
    let c0 = f.intern_i(0);
    let c1 = f.intern_i(1);
    f.params = vec![
        FuncParam { bank: 0, reg: 0 },
        FuncParam { bank: 0, reg: 1 },
        FuncParam { bank: 0, reg: 2 },
    ];
    f.emit_ext(Op::LoadConstI, 3, 0, 0, c0); // sum
    f.emit_ext(Op::LoadConstI, 4, 0, 0, c0); // i
    f.emit_ext(Op::LoadConstI, 7, 0, 0, c1);
    let top = f.code.len();
    f.emit(Op::LtI, 5, 4, 0);
    let jexit = f.code.len();
    f.emit_ext(Op::JmpIfNot, 5, 0, 0, 0);
    f.emit(Op::MulI, 6, 4, 1);
    f.emit(Op::DivI, 6, 6, 2);
    f.emit(Op::AddI, 3, 3, 6);
    f.emit(Op::AddI, 4, 4, 7);
    let back = f.code.len();
    f.emit_ext(Op::Jmp, 0, 0, 0, 0);
    let end = f.code.len();
    f.code[back + 1] = (top as i32 - (back as i32 + 2)) as u32;
    f.code[jexit + 1] = (end as i32 - (jexit as i32 + 2)) as u32;
    f.emit(Op::Return, 0, 3, 0);
    let mut pair = Pair::new(prog(vec![f]));
    assert!(pair.compiled("loop"));
    let ok = pair.check("loop 100", |vm| vm.call_with("loop", &[100, 7, 3], &[]));
    assert_eq!(ok.result, Ok((0..100).map(|i| i * 7 / 3).sum()));
    pair.check("div by zero", |vm| vm.call_with("loop", &[5, 7, 0], &[]));
    pair.check("MIN / -1", |vm| {
        vm.call_with("loop", &[2, i64::MIN, -1], &[])
    });
    for fuel in [0, 1, 3, 4, 5, 9, 10, 11, 57] {
        pair.interp.config.fuel = fuel;
        pair.aot.config.fuel = fuel;
        let o = pair.check("fuel", |vm| vm.call_with("loop", &[1000, 1, 1], &[]));
        assert!(matches!(o.result, Err(Trap::FuelExhausted { .. })), "{o:?}");
    }
}

#[test]
fn floats_conversions_and_compare_match() {
    let mut f = func("fl", 4, 6, 0, 0);
    let vals = [
        0.0,
        -0.0,
        1.5,
        -2.75,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e300,
        -9.3e18,
    ];
    let mut ks = Vec::new();
    for v in vals {
        ks.push(f.intern_f(v));
    }
    let _ = ks;
    f.params = vec![FuncParam { bank: 0, reg: 0 }, FuncParam { bank: 0, reg: 1 }];
    // f0 = const_f[i0]; f1 = const_f[i1] via a jump table is overkill: load both by index trick.
    // Instead compute from integer inputs: f0 = i0 as f64 / 4; f1 = i1 as f64.
    let c4 = f.intern_f(4.0);
    f.emit(Op::I2F, 0, 0, 0);
    f.emit_ext(Op::LoadConstF, 2, 0, 0, c4);
    f.emit(Op::DivF, 0, 0, 2);
    f.emit(Op::I2F, 1, 1, 0);
    f.emit(Op::MulF, 3, 0, 1);
    f.emit(Op::SubF, 3, 3, 1);
    f.emit(Op::AddF, 3, 3, 0);
    f.emit(Op::CmpF, 2, 0, 1);
    f.emit(Op::F2I, 3, 3, 0);
    f.emit(Op::AddI, 0, 2, 3);
    f.emit(Op::DivF, 4, 0, 1);
    f.emit(Op::F2I, 1, 4, 0);
    f.emit(Op::Return, 0, 0, 0);
    let mut pair = Pair::new(prog(vec![f]));
    for (a, b) in [
        (3, 2),
        (-7, 5),
        (1, 0),
        (0, 0),
        (i64::MAX, 3),
        (i64::MIN, -1),
        (9, 9),
    ] {
        pair.check("floats", |vm| vm.call_with("fl", &[a, b], &[]));
    }
    // NaN / infinity / saturation through F2I and CmpF.
    let mut g = func("nan", 2, 3, 0, 0);
    let k_nan = g.intern_f(f64::NAN);
    let k_inf = g.intern_f(f64::INFINITY);
    let k_big = g.intern_f(-9.3e18 * 4.0);
    g.emit_ext(Op::LoadConstF, 0, 0, 0, k_nan);
    g.emit_ext(Op::LoadConstF, 1, 0, 0, k_inf);
    g.emit_ext(Op::LoadConstF, 2, 0, 0, k_big);
    g.emit(Op::CmpF, 0, 0, 1);
    g.emit(Op::F2I, 1, 1, 0);
    g.emit(Op::F2I, 1, 2, 0);
    g.emit(Op::F2I, 0, 0, 0);
    g.emit(Op::Ret, 1, 0, 0);
    let mut pair = Pair::new(prog(vec![g]));
    let o = pair.check("nan", |vm| vm.call("nan"));
    assert_eq!(o.last_f, f64::NAN.to_bits());
}

#[test]
fn calls_return_quirk_rets_and_recursion_match() {
    // callee(i0, f0) { i1 = i0 * 2; SetRet 1 <- i1; Ret f-bank f0 into slot 0 ... }
    let mut callee = func("callee", 3, 1, 0, 0);
    callee.params = vec![FuncParam { bank: 0, reg: 0 }, FuncParam { bank: 1, reg: 0 }];
    callee.emit(Op::AddI, 1, 0, 0);
    callee.emit(Op::SetRet, 0, 1, 1);
    callee.emit(Op::Ret, 1, 0, 0);
    // quirk(i0) { i2 = i0 + i0; Return b=2 }  → caller's i[0] gets i2
    let mut quirk = func("quirk", 3, 0, 0, 0);
    quirk.params = vec![FuncParam { bank: 0, reg: 0 }];
    quirk.emit(Op::AddI, 2, 0, 0);
    quirk.emit(Op::Return, 0, 2, 0);
    // fall(i0): falls off the end (implicit return, caller untouched)
    let mut fall = func("fall", 1, 0, 0, 0);
    fall.emit(Op::Nop, 0, 0, 0);
    // rec(i0): if i0 == 0 return 0 else return rec(i0 - 1) + 1
    let mut rec = func("rec", 4, 0, 0, 0);
    rec.params = vec![FuncParam { bank: 0, reg: 0 }];
    let one = rec.intern_i(1);
    rec.emit_ext(Op::LoadConstI, 1, 0, 0, one);
    rec.emit_ext(Op::JmpIf, 0, 0, 0, 1);
    rec.emit(Op::Return, 0, 0, 0);
    rec.emit(Op::SubI, 2, 0, 1);
    rec.emit(Op::Arg, 0, 2, 0);
    rec.emit_ext(Op::Call, 1, 0, 0, 3); // rec
    rec.emit(Op::GetRet, 0, 3, 0);
    rec.emit(Op::AddI, 3, 3, 1);
    rec.emit(Op::AddI, 3, 0, 1); // i0 was overwritten by the callee's Return quirk
    rec.emit(Op::Ret, 0, 3, 0);
    // main(i0): exercises everything
    let mut main = func("main", 6, 2, 0, 0);
    main.params = vec![FuncParam { bank: 0, reg: 0 }];
    let k = main.intern_f(0.25);
    main.emit_ext(Op::LoadConstF, 0, 0, 0, k);
    main.emit(Op::Arg, 0, 0, 0);
    main.emit(Op::Arg, 1, 0, 0);
    main.emit_ext(Op::Call, 1, 0, 0, 0); // callee
    main.emit(Op::GetRet, 1, 1, 0); // f1 = rets[0] (F)
    main.emit(Op::GetRet, 0, 2, 1); // i2 = rets[1] (I)
    main.emit(Op::GetRet, 3, 0, 0); // bank A from an F slot: no write
    main.emit(Op::GetRet, 2, 3, 5); // empty slot → i[3] = 0
    main.emit(Op::Arg, 0, 2, 0);
    main.emit_ext(Op::Call, 0, 0, 0, 1); // quirk: i0 = i2 * 2
    main.emit_ext(Op::Call, 0, 0, 0, 2); // fall
    main.emit(Op::GetRet, 0, 4, 0); // rets cleared → 0
    main.emit(Op::Arg, 0, 0, 0);
    main.emit_ext(Op::Call, 0, 0, 0, 3); // rec(i0)
    main.emit(Op::GetRet, 0, 5, 0);
    main.emit(Op::AddI, 0, 0, 5);
    main.emit(Op::AddI, 0, 0, 2);
    main.emit(Op::Return, 0, 0, 0);
    let mut pair = Pair::new(prog(vec![callee, quirk, fall, rec, main]));
    for n in [0, 1, 5, 20] {
        pair.check("main", |vm| vm.call_with("main", &[n], &[]));
    }
    // Deep recursion: stack overflow on both tiers at the same frame.
    for limit in [3, 10, 64, 256] {
        pair.interp.config.stack_limit = limit;
        pair.aot.config.stack_limit = limit;
        pair.check("rec deep", |vm| vm.call_with("rec", &[300], &[]));
    }
}

#[test]
fn objects_arrays_strings_and_virtual_calls_match() {
    use room_zvm::ClassLayout;
    let mut p = CompiledProgram::default();
    let mut layout = ClassLayout {
        name: "Box".into(),
        n_i: 2,
        n_a: 2,
        n_s: 1,
        n_f: 1,
        array_i: vec![0],
        array_s: vec![1],
        ..Default::default()
    };
    layout.fields.insert("n".into(), (0, 0));
    p.classes.push(layout);
    p.classes.push(ClassLayout {
        name: "Sub".into(),
        parent: "Box".into(),
        n_i: 2,
        n_a: 2,
        n_s: 1,
        n_f: 1,
        array_i: vec![0],
        array_s: vec![1],
        ..Default::default()
    });
    // Box.get(self) → 10 ; Sub.get(self) → 20 (vtable slot 0)
    let mut bget = func("Box.get", 1, 0, 0, 1);
    bget.params = vec![FuncParam { bank: 3, reg: 0 }];
    let k10 = bget.intern_i(10);
    bget.emit_ext(Op::LoadConstI, 0, 0, 0, k10);
    bget.emit(Op::Ret, 0, 0, 0);
    let mut sget = func("Sub.get", 1, 0, 0, 1);
    sget.params = vec![FuncParam { bank: 3, reg: 0 }];
    let k20 = sget.intern_i(20);
    sget.emit_ext(Op::LoadConstI, 0, 0, 0, k20);
    sget.emit(Op::Ret, 0, 0, 0);
    let mut m = func("run", 6, 1, 4, 4);
    m.params = vec![FuncParam { bank: 0, reg: 0 }];
    let hello = m.intern_s("hello");
    let world = m.intern_s(" world");
    let kf = m.intern_f(2.5);
    m.emit_ext(Op::NewObj, 0, 0, 0, 1); // a0 = new Sub
    m.emit_ext(Op::NewObj, 3, 0, 0, 0); // a3 = new Box
    m.emit_ext(Op::StoreFieldI, 0, 0, 0, 1); // a0.i[1] = i0
    m.emit_ext(Op::LoadFieldI, 1, 0, 0, 1); // i1 = a0.i[1]
    m.emit_ext(Op::LoadFieldA, 1, 0, 0, 0); // a1 = Array<int>
    m.emit(Op::ArrayPushI, 1, 1, 0);
    m.emit(Op::ArrayPushI, 1, 0, 0);
    m.emit(Op::ArrayLen, 2, 1, 0);
    m.emit(Op::ArrayGetI, 3, 1, 0); // index i0: OOB unless i0 is 0 or 1
    m.emit(Op::ArraySetI, 1, 2, 2); // index 2 = len → OOB … only when reached
    m.emit_ext(Op::LoadConstS, 0, 0, 0, hello);
    m.emit_ext(Op::LoadConstS, 1, 0, 0, world);
    m.emit(Op::ConcatS, 2, 0, 1);
    m.emit_ext(Op::StoreFieldS, 0, 2, 0, 0);
    m.emit_ext(Op::LoadFieldS, 3, 0, 0, 0);
    m.emit_ext(Op::LoadFieldA, 2, 0, 0, 1); // Array<string>
    m.emit(Op::ArrayPushS, 2, 3, 0);
    m.emit(Op::ArrayGetS, 0, 2, 4); // i4 = 0
    m.emit_ext(Op::LoadConstF, 0, 0, 0, kf);
    m.emit_ext(Op::StoreFieldF, 0, 0, 0, 3); // widens fields_f
    m.emit_ext(Op::LoadFieldF, 0, 0, 0, 3);
    m.emit_ext(Op::IsClass, 4, 0, 0, 0); // Sub is Box
    m.emit_ext(Op::IsClass, 5, 3, 0, 1); // Box is not Sub
    m.emit(Op::Arg, 3, 0, 0);
    m.emit_ext(Op::CallVirt, 1, 0, 0, 0);
    m.emit(Op::GetRet, 0, 1, 0);
    m.emit(Op::Arg, 3, 3, 0);
    m.emit_ext(Op::CallVirt, 1, 0, 0, 0);
    m.emit(Op::GetRet, 0, 2, 0);
    m.emit(Op::AddI, 1, 1, 2);
    m.emit(Op::AddI, 1, 1, 4);
    m.emit(Op::AddI, 1, 1, 5);
    m.emit(Op::IsNull, 2, 3, 0);
    m.emit(Op::LoadNull, 3, 0, 0);
    m.emit(Op::IsNull, 3, 3, 0);
    m.emit(Op::AddI, 1, 1, 3);
    m.emit_ext(Op::LoadFieldI, 2, 3, 0, 0); // null deref
    m.emit(Op::Ret, 2, 2, 0);
    p.functions = vec![bget, sget, m];
    p.vtables.insert(0, vec![0]);
    p.vtables.insert(1, vec![1]);
    p.finalize();
    let mut pair = Pair::new(p);
    assert!(pair.compiled("run"));
    for n in [0, 1, 2, -1] {
        pair.check("objects", |vm| vm.call_with("run", &[n], &[]));
    }
}

#[test]
fn explicit_trap_and_legacy_fallback() {
    let mut t = func("boom", 1, 0, 0, 0);
    let msg = t.intern_s("assertion failed");
    t.emit_ext(Op::Trap, 0, 0, 0, msg);
    t.lines = vec![42, 42];
    let mut legacy = func("legacy", 2, 0, 0, 0);
    legacy.emit_ext(Op::CallStatic, 0, 0, 0, 0);
    legacy.emit(Op::Return, 0, 0, 0);
    let mut caller = func("caller", 2, 0, 0, 0);
    caller.emit_ext(Op::Call, 0, 0, 0, 1); // legacy → stays interpreted (bridge)
    caller.emit(Op::Return, 0, 0, 0);
    let mut pair = Pair::new(prog(vec![t, legacy, caller]));
    assert!(!pair.compiled("legacy"));
    assert!(pair.compiled("caller"));
    let o = pair.check("trap", |vm| vm.call("boom"));
    assert!(
        matches!(o.result, Err(Trap::Explicit { line: 42, .. })),
        "{o:?}"
    );
    pair.check("bridge into legacy", |vm| vm.call("caller"));
}

/// ZScript source → both tiers.
pub(crate) fn zscript_pair(src: &str) -> Pair {
    let result = room_zscript::check(&[("t.zs".to_owned(), src.to_owned())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    Pair::new(result.program.expect("program"))
}

#[test]
fn zscript_control_flow_matches() {
    let src = r#"
class Probe
{
    static int Collatz(int n)
    {
        int steps = 0;
        while (n != 1 && steps < 1000)
        {
            if (n % 2 == 0) n = n / 2; else n = 3 * n + 1;
            steps++;
        }
        return steps;
    }
    static int Sw(int k)
    {
        int r = 0;
        for (int i = 0; i < k; i++)
        {
            switch (i % 4)
            {
                case 0: r += 1; break;
                case 1: r += 10; continue;
                case 2: r -= 3;
                default: r *= 2; break;
            }
            if (r > 100000) break;
        }
        return r;
    }
    static double Mix(int a, double b)
    {
        double x = a * b;
        if (x > 3.5 || a == 7) x = x - 1.25;
        return x / (b != 0 ? b : 1.0);
    }
    static int Calls(int n)
    {
        return Collatz(n) + Sw(n % 17);
    }
}
"#;
    let mut pair = zscript_pair(src);
    for name in ["Probe.Collatz", "Probe.Sw", "Probe.Calls"] {
        assert!(
            pair.compiled(name),
            "{name} should compile: {:?}",
            pair.art.unsupported
        );
    }
    for n in [1, 2, 7, 27, 97, 871] {
        pair.check("Collatz", |vm| vm.call_with("Probe.Collatz", &[n], &[]));
        pair.check("Sw", |vm| vm.call_with("Probe.Sw", &[n], &[]));
        pair.check("Calls", |vm| vm.call_with("Probe.Calls", &[n], &[]));
    }
    let _ = ObjHandle::NULL;
}

#[test]
fn brutal_doom_zscript_gated() {
    let Ok(pk3) = std::env::var("ROOM_BRUTALDOOM_PK3") else {
        eprintln!("skip: set ROOM_BRUTALDOOM_PK3");
        return;
    };
    let (sources, lumps) = crate::bd::read_pk3(std::path::Path::new(&pk3)).expect("pk3");
    let report = crate::bd::run(&sources, &lumps).unwrap_or_else(|e| panic!("{e}"));
    eprintln!("{report}");
    assert!(report.compiled > 0);
    let shipped = crate::bd::run_as(&sources, &lumps, true).unwrap_or_else(|e| panic!("{e}"));
    eprintln!("as a converted mod: {shipped}");
}

#[test]
fn uzdoom_stdlib_compiles() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../wadsrc/uzdoom");
    let Ok(entry) = std::fs::read_to_string(root.join("zscript.txt")) else {
        return;
    };
    let mut sources = vec![("zscript.txt".to_owned(), entry.clone())];
    for line in entry.lines() {
        let Some(rest) = line.trim().strip_prefix("#include") else {
            continue;
        };
        let path = rest.trim().trim_matches('"');
        if let Ok(t) = std::fs::read_to_string(root.join(path)) {
            sources.push((path.replace('\\', "/"), t));
        }
    }
    let program = room_zscript::check(&sources)
        .program
        .expect("stdlib program");
    let vm = Zvm::new_with_stubs(program);
    let t0 = std::time::Instant::now();
    let art = compile(&vm.program, &Options::default()).expect("codegen");
    let t1 = std::time::Instant::now();
    let mut vm2 = Zvm::new_with_stubs(vm.program.clone());
    vm2.attach_aot(&art.bytes, art.compiled.clone())
        .expect("attach");
    let t2 = std::time::Instant::now();
    let mut why = std::collections::BTreeMap::<String, usize>::new();
    for (_, u) in &art.unsupported {
        *why.entry(format!("{u:?}")).or_default() += 1;
    }
    eprintln!(
        "stdlib: {}/{} compiled, {} insns {} blocks, {} bytes, codegen {:?}, instantiate {:?}, unsupported {why:?}",
        art.stats.compiled,
        art.stats.functions,
        art.stats.insns,
        art.stats.blocks,
        art.bytes.len(),
        t1 - t0,
        t2 - t1
    );
}

#[test]
fn room_mod_module_round_trips_and_matches_interpreter() {
    let src = r#"
class Probe
{
    static int Fact(int n) { int r = 1; for (int i = 2; i <= n; i++) r *= i; return r; }
    static int Say(int n) { Console.Printf("n=%d", n); return n * 2; }
    static string Cat(string a, int n) { string s = a; for (int i = 0; i < n; i++) s = s .. "!"; return s; }
}
"#;
    let result = room_zscript::check(&[("t.zs".to_owned(), src.to_owned())]);
    assert!(result.ok(), "{:?}", result.diagnostics);
    let program = result.program.unwrap();
    let art = crate::roommod::compile_room_mod(&program).unwrap();
    let m = crate::read_room_mod(&art.bytes).unwrap();
    assert_eq!(m.program.to_bytes(), program.to_bytes());
    // Corrupt the ABI section → rejected.
    let mut bad = art.bytes.clone();
    let pos = bad.windows(4).rposition(|w| w == b"trap").unwrap();
    bad[pos] = b'X';
    assert!(crate::read_room_mod(&bad).is_err());
    let mut t = crate::check::Tiers::new_room_mod(program).unwrap();
    for n in [0, 1, 5, 12] {
        t.call("Fact", |vm| vm.call_with("Probe.Fact", &[n], &[])).unwrap();
        t.call("Say", |vm| vm.call_with("Probe.Say", &[n], &[])).unwrap();
        t.call("Cat", |vm| vm.call_with("Probe.Cat", &[n], &["hey"])).unwrap();
    }
    t.call_everything(2, 7).unwrap();
}

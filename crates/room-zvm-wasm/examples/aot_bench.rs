//! Interpreter vs AOT timing (`docs/37` §8.6, informational).
//!
//! `cargo run --release -p room-zvm-wasm --features wasmtime --example aot_bench [bd.pk3]`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use room_zvm::Zvm;
use std::time::Instant;

const SRC: &str = r#"
class Bench
{
    static int Collatz(int n)
    {
        int steps = 0;
        while (n != 1) { if (n % 2 == 0) n = n / 2; else n = 3 * n + 1; steps++; }
        return steps;
    }
    static int Sweep(int limit)
    {
        int total = 0;
        for (int i = 1; i < limit; i++) total += Collatz(i);
        return total;
    }
    static double Poly(int n)
    {
        double acc = 0;
        for (int i = 0; i < n; i++) { double x = i * 0.001; acc += x * x * 3.0 - x * 2.0 + 1.0; }
        return acc;
    }
    static int Tick(int t) { return t + 1; }
}
"#;

fn pair() -> (Zvm, Zvm) {
    let r = room_zscript::check(&[("bench.zs".to_owned(), SRC.to_owned())]);
    assert!(r.ok(), "{:?}", r.diagnostics);
    let p = r.program.unwrap();
    let mut interp = Zvm::new(p.clone());
    let mut aot = Zvm::new(p);
    interp.config.fuel = u64::MAX / 4;
    aot.config.fuel = u64::MAX / 4;
    room_zvm_wasm::enable_aot(&mut aot).unwrap(); // `auto` policy
    (interp, aot)
}

fn time(label: &str, interp: &mut Zvm, aot: &mut Zvm, f: impl Fn(&mut Zvm) -> i64) {
    let t0 = Instant::now();
    let a = f(interp);
    let t1 = Instant::now();
    let b = f(aot);
    let t2 = Instant::now();
    assert_eq!(a, b, "{label}: results differ");
    let (ti, ta) = ((t1 - t0).as_secs_f64() * 1e3, (t2 - t1).as_secs_f64() * 1e3);
    println!(
        "{label:<28} interp {ti:9.2} ms   aot {ta:9.2} ms   x{:.1}",
        ti / ta.max(1e-9)
    );
}

fn main() {
    let (mut interp, mut aot) = pair();
    println!(
        "engine: {} (Zvm {} bytes)",
        aot.aot_engine().unwrap_or("none"),
        std::mem::size_of::<Zvm>()
    );
    // Warm up (JIT compile happened at attach; first call touches the code).
    let _ = aot.call_with("Bench.Tick", &[1], &[]);
    time("Collatz sweep 1..30000", &mut interp, &mut aot, |vm| {
        vm.call_with("Bench.Sweep", &[30_000], &[]).unwrap()
    });
    time("float poly 2e6", &mut interp, &mut aot, |vm| {
        vm.call_with("Bench.Poly", &[2_000_000], &[]).unwrap();
        vm.last_f.to_bits() as i64
    });
    time("10k trivial calls", &mut interp, &mut aot, |vm| {
        let mut x = 0;
        for _ in 0..10_000 {
            x = vm.call_with("Bench.Tick", &[x], &[]).unwrap();
        }
        x
    });
    println!("aot top-level calls so far: {}", aot.aot_calls());
    if let Some(pk3) = std::env::args().nth(1) {
        let (sources, lumps) = room_zvm_wasm::bd::read_pk3(std::path::Path::new(&pk3)).unwrap();
        let report = room_zvm_wasm::bd::run(&sources, &lumps).unwrap();
        println!("{report}");
    }
}

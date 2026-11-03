//! Criterion benches for ZS10 §9 targets (desktop interpreter).
//!
//! Run: `cargo bench -p room-zvm --bench tick_override`

use criterion::{Criterion, criterion_group, criterion_main};
use room_zvm::{CompiledFunction, CompiledProgram, Op, Zvm, optimize_program};

fn trivial_tick_program(n_actors_sim: usize) -> CompiledProgram {
    let mut f = CompiledFunction {
        name: "Actor.Tick".into(),
        n_i: 4,
        ..Default::default()
    };
    // i0 += 1; return
    let one = f.intern_i(1);
    f.emit_ext(Op::LoadConstI, 1, 0, 0, one);
    f.emit(Op::AddI, 0, 0, 1);
    f.emit(Op::Return, 0, 0, 0);
    let mut p = CompiledProgram {
        functions: vec![f],
        ..Default::default()
    };
    p.finalize();
    let _ = n_actors_sim;
    p
}

fn bench_10k_ticks(c: &mut Criterion) {
    let mut prog = trivial_tick_program(10_000);
    optimize_program(&mut prog);
    c.bench_function("zvm_10k_trivial_tick", |b| {
        b.iter(|| {
            let mut vm = Zvm::new(prog.clone());
            for _ in 0..10_000 {
                let _ = vm.call("Actor.Tick");
            }
        });
    });
}

criterion_group!(benches, bench_10k_ticks);
criterion_main!(benches);

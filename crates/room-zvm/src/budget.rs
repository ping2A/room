//! Soft performance / fuel budgets (ZS10).

use crate::interp::{DEFAULT_FUEL, Zvm};
use crate::opcode::Op;
use crate::peephole::optimize_program;
use crate::program::{CompiledFunction, CompiledProgram};

/// Desktop interpreter budget for 10k trivial `Tick` calls (ms, soft).
pub const DESKTOP_10K_TICK_MS: u128 = 4;
/// Web interpreter budget (ms, soft).
pub const WEB_10K_TICK_MS: u128 = 12;

/// Build a trivial Tick program.
#[must_use]
pub fn trivial_tick_program() -> CompiledProgram {
    let mut f = CompiledFunction {
        name: "Actor.Tick".into(),
        n_i: 2,
        ..Default::default()
    };
    let one = f.intern_i(1);
    f.emit_ext(Op::LoadConstI, 1, 0, 0, one);
    f.emit(Op::AddI, 0, 0, 1);
    f.emit(Op::Return, 0, 0, 0);
    let mut p = CompiledProgram {
        functions: vec![f],
        ..Default::default()
    };
    p.finalize();
    optimize_program(&mut p);
    p
}

/// Run `n` trivial ticks; returns elapsed milliseconds (coarse).
#[must_use]
pub fn measure_ticks_ms(n: usize) -> u128 {
    let prog = trivial_tick_program();
    let mut vm = Zvm::new(prog);
    vm.config.fuel = DEFAULT_FUEL.saturating_mul(4);
    let start = std::time::Instant::now();
    for _ in 0..n {
        let _ = vm.call("Actor.Tick");
    }
    start.elapsed().as_millis()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::tier1::conformance_call;

    #[test]
    fn trivial_tick_conformance() {
        let p = trivial_tick_program();
        let _ = conformance_call(&p, "Actor.Tick").unwrap();
    }

    #[test]
    fn ten_k_ticks_under_loose_budget() {
        // CI machines vary; use a loose 500ms floor so this is a smoke check,
        // not the §9 desktop 4ms gate (that lives in criterion benches).
        let ms = measure_ticks_ms(10_000);
        assert!(
            ms < 500,
            "10k ticks took {ms}ms (smoke budget 500ms; see criterion for §9)"
        );
    }
}

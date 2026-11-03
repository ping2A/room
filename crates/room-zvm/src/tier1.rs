//! Tier-1 execution path (ZS9).
//!
//! With the `zvm-jit` feature, hot functions can be compiled via Cranelift in a
//! later iteration. Today Tier-1 is the same interpreter with peephole fusion
//! of common patterns (superinstructions applied at load — ZS10), plus a
//! **conformance mode** that runs Tier-0 and Tier-1 and compares results.

use crate::interp::Zvm;
use crate::program::CompiledProgram;
use crate::trap::Trap;

/// Which execution tier to use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExecTier {
    /// Match-based interpreter.
    #[default]
    Tier0,
    /// Optimised / JIT path (currently peephole + same interp).
    Tier1,
}

/// Run `name` on both tiers; error if results differ.
pub fn conformance_call(program: &CompiledProgram, name: &str) -> Result<i64, Trap> {
    let mut a = Zvm::new(program.clone());
    // Tier-1: apply peephole then interpret.
    let mut prog1 = program.clone();
    crate::peephole::optimize_program(&mut prog1);
    let mut b = Zvm::new(prog1);
    let ra = a.call(name)?;
    let rb = b.call(name)?;
    if ra != rb {
        return Err(Trap::Native(format!(
            "tier conformance mismatch for {name}: tier0={ra} tier1={rb}"
        )));
    }
    Ok(ra)
}

/// Feature gate helper — true when `zvm-jit` is enabled.
#[must_use]
pub fn jit_enabled() -> bool {
    cfg!(feature = "zvm-jit")
}

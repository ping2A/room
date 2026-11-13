//! Coverage report for compiled programs (`docs/37` ZW7): which natives a
//! program calls, whether this build implements them, how many call sites
//! each has, and which functions stay on the interpreter.
//!
//! With the engine-side heap design (`docs/37` §0.2) every native the VM
//! implements is available to compiled code and to converted mods; there is
//! no separate ABI mapping table. "Missing" natives trap on the interpreter
//! and in wasm alike (or return 0 when a host links with stubs).

use crate::Artifact;
use room_zvm::{CompiledProgram, Op, Zvm, has_ext, unpack};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One native referenced by the program.
#[derive(Clone, Debug)]
pub struct NativeRow {
    /// Name as resolved by the VM (parent-class fallback applied).
    pub name: String,
    /// Implemented by this build's native table.
    pub implemented: bool,
    /// `CallNat` sites in the program.
    pub call_sites: usize,
}

/// Program-level coverage.
#[derive(Clone, Debug, Default)]
pub struct CoverageReport {
    /// Label for the report (mod name).
    pub label: String,
    /// Functions in the program.
    pub functions: usize,
    /// Functions compiled to wasm.
    pub compiled: usize,
    /// Functions entered through wasm under the `auto` policy.
    pub hot: usize,
    /// Module size.
    pub wasm_bytes: usize,
    /// Functions left on the interpreter, with the reason.
    pub interpreted: Vec<(String, String)>,
    /// Natives by name.
    pub natives: Vec<NativeRow>,
}

impl CoverageReport {
    /// Natives this build does not implement.
    #[must_use]
    pub fn missing(&self) -> Vec<&str> {
        self.natives
            .iter()
            .filter(|n| !n.implemented)
            .map(|n| n.name.as_str())
            .collect()
    }

    /// Markdown section.
    #[must_use]
    pub fn markdown(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "## {}\n", self.label);
        let _ = writeln!(
            s,
            "- functions: {} compiled to wasm / {} total ({} entered through wasm under the `auto` policy)",
            self.compiled, self.functions, self.hot
        );
        let _ = writeln!(s, "- module: {} bytes", self.wasm_bytes);
        let missing = self.missing();
        let _ = writeln!(
            s,
            "- natives: {} referenced, {} implemented, {} missing",
            self.natives.len(),
            self.natives.len() - missing.len(),
            missing.len()
        );
        if !self.interpreted.is_empty() {
            let _ = writeln!(s, "\nFunctions left on the interpreter:\n");
            for (f, why) in &self.interpreted {
                let _ = writeln!(s, "- `{f}`: {why}");
            }
        }
        if !missing.is_empty() {
            let _ = writeln!(s, "\nMissing natives (trap on both tiers):\n");
            for m in &missing {
                let _ = writeln!(s, "- `{m}`");
            }
        }
        let _ = writeln!(s, "\n| Native | Status | Call sites |\n|---|---|---|");
        for n in &self.natives {
            let _ = writeln!(
                s,
                "| `{}` | {} | {} |",
                n.name,
                if n.implemented { "implemented" } else { "**missing**" },
                n.call_sites
            );
        }
        s
    }
}

/// Coverage of `program` (unlinked, as the compiler produced it) compiled into `art`.
#[must_use]
pub fn coverage(label: &str, program: &CompiledProgram, art: &Artifact) -> CoverageReport {
    let mut sites: BTreeMap<usize, usize> = BTreeMap::new();
    for f in &program.functions {
        let mut pc = 0;
        while pc < f.code.len() {
            let (op_b, _, _, _) = unpack(f.code[pc]);
            let op = Op::from_u8(op_b);
            if matches!(op, Some(Op::CallNat | Op::CallNative)) {
                if let Some(ext) = f.code.get(pc + 1) {
                    *sites.entry(*ext as usize).or_default() += 1;
                }
            }
            pc += 1 + usize::from(op.is_some_and(has_ext));
        }
    }
    let linked = Zvm::new(program.clone());
    let natives = linked
        .program
        .native_imports
        .iter()
        .enumerate()
        .map(|(i, name)| NativeRow {
            name: name.clone(),
            implemented: linked.natives.id_ci(name).is_some(),
            call_sites: sites.get(&i).copied().unwrap_or(0),
        })
        .collect();
    CoverageReport {
        label: label.to_owned(),
        functions: art.compiled.len(),
        compiled: art.stats.compiled,
        hot: art.hot.iter().filter(|h| **h).count(),
        wasm_bytes: art.bytes.len(),
        interpreted: art
            .unsupported
            .iter()
            .map(|(i, why)| (program.functions[*i as usize].name.clone(), why.to_string()))
            .collect(),
        natives,
    }
}

//! Generic differential run (`room zscript wasm --check`, `docs/37` §8.2):
//! call every function of a program on the interpreter and on the AOT tier
//! with the same seeded arguments and compare everything a call can change.

use crate::{Artifact, Options, compile};
use room_zvm::{CompiledProgram, ObjHandle, Trap, Zvm};

/// What a call changed, compared between tiers.
#[derive(Debug, PartialEq)]
pub struct Observed {
    /// Result or trap (kind, function and line).
    pub result: Result<i64, Trap>,
    /// `last_i`.
    pub last_i: [i64; 4],
    /// `last_f` bits.
    pub last_f: u64,
    /// `last_s`.
    pub last_s: String,
    /// Fuel left.
    pub fuel: u64,
    /// Heap digest.
    pub heap: u64,
    /// Play-host state (printf, grants, queued actor ops, …).
    pub play: String,
}

/// Snapshot `vm` after a call that returned `result`.
#[must_use]
pub fn observe(vm: &Zvm, result: Result<i64, Trap>) -> Observed {
    Observed {
        result,
        last_i: vm.last_i,
        last_f: vm.last_f.to_bits(),
        last_s: vm.last_s.clone(),
        fuel: vm.fuel_left(),
        heap: vm.heap.stable_digest(),
        play: format!("{:?}", vm.play),
    }
}

/// Interpreter and AOT VMs over one program.
pub struct Tiers {
    /// Reference tier.
    pub interp: Zvm,
    /// AOT tier.
    pub aot: Zvm,
    /// Generated module.
    pub artifact: Artifact,
    /// Calls compared.
    pub calls: usize,
    /// Calls that trapped (identically).
    pub traps: usize,
    /// Interpreter time (ms).
    pub interp_ms: f64,
    /// AOT time (ms).
    pub aot_ms: f64,
}

impl Tiers {
    /// Link `program` twice and attach the AOT module to the second VM.
    pub fn new(program: CompiledProgram) -> Result<Self, String> {
        let interp = Zvm::new(program.clone());
        let mut aot = Zvm::new(program);
        let artifact = compile(&aot.program, &Options::default()).map_err(|e| e.to_string())?;
        aot.attach_aot(&artifact.bytes, artifact.compiled.clone())?;
        aot.set_aot_enabled(true);
        Ok(Self {
            interp,
            aot,
            artifact,
            calls: 0,
            traps: 0,
            interp_ms: 0.0,
            aot_ms: 0.0,
        })
    }

    /// Like [`Tiers::new`], but the AOT side is a converted-mod module
    /// (`Target::RoomMod`): compiled from the unlinked program, serialized,
    /// read back with [`crate::read_room_mod`] and attached as shipped code.
    pub fn new_room_mod(program: CompiledProgram) -> Result<Self, String> {
        let interp = Zvm::new(program.clone());
        let artifact = crate::roommod::compile_room_mod(&program).map_err(|e| e.to_string())?;
        let m = crate::read_room_mod(&artifact.bytes).map_err(|e| e.to_string())?;
        let mut aot = Zvm::new(m.program.clone());
        aot.attach_shipped(&m.bytes, m.compiled.clone(), m.compiled.clone())?;
        aot.set_aot_enabled(true);
        Ok(Self {
            interp,
            aot,
            artifact,
            calls: 0,
            traps: 0,
            interp_ms: 0.0,
            aot_ms: 0.0,
        })
    }

    /// Run `f` on both tiers; error when anything observable differs.
    pub fn call(
        &mut self,
        what: &str,
        f: impl Fn(&mut Zvm) -> Result<i64, Trap>,
    ) -> Result<Observed, String> {
        let t0 = std::time::Instant::now();
        let ra = f(&mut self.interp);
        let t1 = std::time::Instant::now();
        let rb = f(&mut self.aot);
        let t2 = std::time::Instant::now();
        self.interp_ms += (t1 - t0).as_secs_f64() * 1e3;
        self.aot_ms += (t2 - t1).as_secs_f64() * 1e3;
        let a = observe(&self.interp, ra);
        let b = observe(&self.aot, rb);
        self.calls += 1;
        if a.result.is_err() {
            self.traps += 1;
        }
        if a != b {
            return Err(format!(
                "{what}: tiers differ\ninterp: {a:?}\naot:    {b:?}"
            ));
        }
        Ok(a)
    }

    /// Apply `setup` to both VMs (play-host state, handler objects, …).
    pub fn setup(&mut self, setup: impl Fn(&mut Zvm)) {
        setup(&mut self.interp);
        setup(&mut self.aot);
    }

    /// Call every function `rounds` times with seeded arguments. `self` is a
    /// fresh instance of the function's class when the class exists.
    pub fn call_everything(&mut self, rounds: u64, seed: u64) -> Result<(), String> {
        let mut rng = seed | 1;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let names: Vec<(String, String)> = self
            .aot
            .program
            .functions
            .iter()
            .map(|f| {
                let class = f
                    .name
                    .rsplit_once('.')
                    .map(|(c, _)| c.to_owned())
                    .unwrap_or_default();
                (f.name.clone(), class)
            })
            .collect();
        for round in 0..rounds {
            for (name, class) in &names {
                let self_i = self.interp.new_object(class).unwrap_or(ObjHandle::NULL);
                let self_a = self.aot.new_object(class).unwrap_or(ObjHandle::NULL);
                if self_i != self_a {
                    return Err(format!("{name}: heaps diverged"));
                }
                let ints = [
                    (next() % 8) as i64 - 2,
                    (next() % 300) as i64,
                    (next() % 6) as i64,
                    round as i64,
                ];
                let floats = [f64::from((next() % 64) as u32) / 8.0 - 2.0, -1.0];
                let s = format!("Str{}", next() % 5);
                self.call(name, |vm| {
                    vm.call_method_banks(name, self_i, &ints, &floats, &[&s, "Burn"], &[self_i])
                })?;
            }
        }
        Ok(())
    }
}

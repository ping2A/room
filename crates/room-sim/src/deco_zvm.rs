//! ZS5: compile DECORATE expressions to `room-zvm` bytecode.
//!
//! Integer exprs use the int ALU; float / `frandom` / `sin`/`cos`/`abs`/`sqrt`
//! use float registers. String labels stay on the tree-walker in `deco_eval`.

use crate::actor::ActorId;
use crate::world::World;
use room_decorate::{BinOp, DecoExpr};
use room_zvm::{CompiledFunction, CompiledProgram, Op, Zvm};
use std::collections::BTreeMap;

/// Cache of compiled DECORATE argument expressions.
///
/// Each expression keeps one linked VM. Building a [`Zvm`] registers every
/// builtin native (about a thousand map inserts), and Brutal Doom evaluates
/// `random(...)` arguments for every gib and particle it spawns: a single
/// gory death used to spend seconds rebuilding native tables.
#[derive(Default)]
pub struct DecoExprCache {
    programs: BTreeMap<String, Zvm>,
    float_programs: BTreeMap<String, Zvm>,
}

/// A copy starts with an empty cache: the programs are recompiled on first
/// use, with the same results (world snapshots for rollback).
impl Clone for DecoExprCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl DecoExprCache {
    /// Compile (or fetch) and evaluate an integer-suitable expression.
    pub fn eval_i64(
        &mut self,
        world: &mut World,
        self_id: ActorId,
        raw: &str,
        expr: &DecoExpr,
    ) -> Option<i64> {
        if !zvm_suitable(expr) {
            return None;
        }
        let vm = self.programs.entry(raw.to_owned()).or_insert_with(|| {
            let mut f = compile_expr(expr);
            f.name = "__deco".into();
            let mut p = CompiledProgram {
                functions: vec![f],
                ..Default::default()
            };
            p.finalize();
            link_i(p)
        });
        run_i(world, self_id, vm)
    }

    /// Compile (or fetch) and evaluate a float-suitable expression.
    pub fn eval_f64(
        &mut self,
        world: &mut World,
        self_id: ActorId,
        raw: &str,
        expr: &DecoExpr,
    ) -> Option<f64> {
        if !zvm_suitable_f(expr) {
            return None;
        }
        let vm = self
            .float_programs
            .entry(raw.to_owned())
            .or_insert_with(|| {
                let mut f = compile_expr_f(expr);
                f.name = "__deco_f".into();
                let mut p = CompiledProgram {
                    functions: vec![f],
                    ..Default::default()
                };
                p.finalize();
                link_f(p)
            });
        run_f(world, self_id, vm)
    }
}

/// True when the expression is integer-only and ZVM-backed.
#[must_use]
pub fn zvm_suitable(expr: &DecoExpr) -> bool {
    match expr {
        DecoExpr::Int(_) => true,
        DecoExpr::Float(_) | DecoExpr::String(_) => false,
        DecoExpr::Ident(n) => int_ident(n),
        DecoExpr::Neg(i) | DecoExpr::Not(i) => zvm_suitable(i),
        DecoExpr::Bin { op, lhs, rhs } => {
            !matches!(op, BinOp::And | BinOp::Or | BinOp::BitOr | BinOp::BitAnd)
                && zvm_suitable(lhs)
                && zvm_suitable(rhs)
        }
        DecoExpr::Call { name, args } => {
            name.eq_ignore_ascii_case("random") && args.len() == 2 && args.iter().all(zvm_suitable)
        }
        DecoExpr::Index { base, index } => match base.as_ref() {
            DecoExpr::Ident(n)
                if n.eq_ignore_ascii_case("args")
                    || n.to_ascii_lowercase().starts_with("user_") =>
            {
                zvm_suitable(index)
            }
            _ => false,
        },
    }
}

/// True when the expression can be evaluated on the float ZVM path.
#[must_use]
pub fn zvm_suitable_f(expr: &DecoExpr) -> bool {
    match expr {
        DecoExpr::Int(_) | DecoExpr::Float(_) => true,
        DecoExpr::String(_) => false,
        DecoExpr::Ident(n) => int_ident(n) || float_ident(n),
        DecoExpr::Neg(i) => zvm_suitable_f(i),
        DecoExpr::Not(_) => false,
        DecoExpr::Bin { op, lhs, rhs } => {
            matches!(
                op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
            ) && zvm_suitable_f(lhs)
                && zvm_suitable_f(rhs)
        }
        DecoExpr::Call { name, args } => {
            let n = name.to_ascii_lowercase();
            match n.as_str() {
                "frandom" | "random" => args.len() == 2 && args.iter().all(zvm_suitable_f),
                "sin" | "cos" | "abs" | "sqrt" => {
                    args.len() == 1 && args.iter().all(zvm_suitable_f)
                }
                _ => false,
            }
        }
        DecoExpr::Index { base, index } => match base.as_ref() {
            DecoExpr::Ident(n)
                if n.eq_ignore_ascii_case("args")
                    || n.to_ascii_lowercase().starts_with("user_") =>
            {
                zvm_suitable_f(index)
            }
            _ => false,
        },
    }
}

fn int_ident(n: &str) -> bool {
    let l = n.to_ascii_lowercase();
    matches!(
        l.as_str(),
        "health" | "args" | "playercrouch" | "crouch" | "tid" | "mass" | "waterlevel"
    ) || l.starts_with("user_")
}

fn float_ident(n: &str) -> bool {
    let l = n.to_ascii_lowercase();
    matches!(
        l.as_str(),
        "speed"
            | "radius"
            | "height"
            | "angle"
            | "pitch"
            | "velx"
            | "momx"
            | "vely"
            | "momy"
            | "velz"
            | "momz"
            | "alpha"
            | "x"
            | "y"
            | "z"
            | "floorz"
            | "scalex"
            | "scaley"
    )
}

fn alloc(next: &mut u8) -> u8 {
    let r = *next;
    *next = next.saturating_add(1);
    r
}

fn compile_expr(expr: &DecoExpr) -> CompiledFunction {
    let mut f = CompiledFunction {
        name: "__deco".into(),
        n_i: 64,
        n_s: 2,
        ..Default::default()
    };
    let mut next = 4u8;
    let dest = emit(&mut f, expr, &mut next);
    f.emit(Op::MoveI, 0, dest, 0);
    f.emit(Op::Return, 0, 0, 0);
    f.n_i = next.max(4);
    f
}

fn compile_expr_f(expr: &DecoExpr) -> CompiledFunction {
    let mut f = CompiledFunction {
        name: "__deco_f".into(),
        n_i: 8,
        n_f: 64,
        n_s: 2,
        ..Default::default()
    };
    let mut next = 4u8;
    let dest = emit_f(&mut f, expr, &mut next);
    f.emit(Op::MoveF, 0, dest, 0);
    f.emit_ext(Op::CallNative, 0, 0, 0, N_FRET);
    f.emit(Op::Return, 0, 0, 0);
    f.n_f = next.max(4);
    f
}

fn emit(f: &mut CompiledFunction, expr: &DecoExpr, next: &mut u8) -> u8 {
    match expr {
        DecoExpr::Int(n) => {
            let dest = alloc(next);
            let idx = f.intern_i(i64::from(*n));
            f.emit_ext(Op::LoadConstI, dest, 0, 0, idx);
            dest
        }
        DecoExpr::Ident(name) => {
            let dest = alloc(next);
            let idx = f.intern_s(name);
            f.emit_ext(Op::LoadConstS, 0, 0, 0, idx);
            f.emit_ext(Op::CallNative, 0, 0, 0, N_IDENT);
            f.emit(Op::MoveI, dest, 0, 0);
            dest
        }
        DecoExpr::Neg(inner) => {
            let a = emit(f, inner, next);
            let zreg = alloc(next);
            let dest = alloc(next);
            let z = f.intern_i(0);
            f.emit_ext(Op::LoadConstI, zreg, 0, 0, z);
            f.emit(Op::SubI, dest, zreg, a);
            dest
        }
        DecoExpr::Not(inner) => {
            let a = emit(f, inner, next);
            let zreg = alloc(next);
            let dest = alloc(next);
            let z = f.intern_i(0);
            f.emit_ext(Op::LoadConstI, zreg, 0, 0, z);
            f.emit(Op::EqI, dest, a, zreg);
            dest
        }
        DecoExpr::Bin { op, lhs, rhs } => {
            let a = emit(f, lhs, next);
            let b = emit(f, rhs, next);
            let dest = alloc(next);
            match op {
                BinOp::Add => f.emit(Op::AddI, dest, a, b),
                BinOp::Sub => f.emit(Op::SubI, dest, a, b),
                BinOp::Mul => f.emit(Op::MulI, dest, a, b),
                BinOp::Div => f.emit(Op::DivI, dest, a, b),
                BinOp::Mod => f.emit(Op::ModI, dest, a, b),
                BinOp::Eq => f.emit(Op::EqI, dest, a, b),
                BinOp::Ne => f.emit(Op::NeI, dest, a, b),
                BinOp::Lt => f.emit(Op::LtI, dest, a, b),
                BinOp::Le => f.emit(Op::LeI, dest, a, b),
                BinOp::Gt => f.emit(Op::GtI, dest, a, b),
                BinOp::Ge => f.emit(Op::GeI, dest, a, b),
                BinOp::And | BinOp::Or | BinOp::BitOr | BinOp::BitAnd => {
                    let z = f.intern_i(0);
                    f.emit_ext(Op::LoadConstI, dest, 0, 0, z);
                }
            }
            dest
        }
        DecoExpr::Call { name, args } => {
            let a0 = emit(f, &args[0], next);
            let a1 = emit(f, &args[1], next);
            let dest = alloc(next);
            let idx = f.intern_s(name);
            f.emit_ext(Op::LoadConstS, 0, 0, 0, idx);
            f.emit(Op::MoveI, 1, a0, 0);
            f.emit(Op::MoveI, 2, a1, 0);
            f.emit_ext(Op::CallNative, 0, 0, 0, N_CALL);
            f.emit(Op::MoveI, dest, 0, 0);
            dest
        }
        DecoExpr::Index { base, index } => {
            let dest = alloc(next);
            let DecoExpr::Ident(n) = base.as_ref() else {
                let z = f.intern_i(0);
                f.emit_ext(Op::LoadConstI, dest, 0, 0, z);
                return dest;
            };
            let ir = emit(f, index, next);
            let ni = f.intern_s(n);
            f.emit_ext(Op::LoadConstS, 0, 0, 0, ni);
            f.emit(Op::MoveI, 1, ir, 0);
            f.emit_ext(Op::CallNative, 0, 0, 0, N_INDEX);
            f.emit(Op::MoveI, dest, 0, 0);
            dest
        }
        _ => {
            let dest = alloc(next);
            let z = f.intern_i(0);
            f.emit_ext(Op::LoadConstI, dest, 0, 0, z);
            dest
        }
    }
}

fn emit_f(f: &mut CompiledFunction, expr: &DecoExpr, next: &mut u8) -> u8 {
    match expr {
        DecoExpr::Int(n) => {
            let dest = alloc(next);
            let idx = f.intern_i(i64::from(*n));
            f.emit_ext(Op::LoadConstI, 0, 0, 0, idx);
            f.emit(Op::I2F, dest, 0, 0);
            dest
        }
        DecoExpr::Float(v) => {
            let dest = alloc(next);
            let idx = f.intern_f(*v);
            f.emit_ext(Op::LoadConstF, dest, 0, 0, idx);
            dest
        }
        DecoExpr::Ident(name) => {
            let dest = alloc(next);
            let idx = f.intern_s(name);
            f.emit_ext(Op::LoadConstS, 0, 0, 0, idx);
            if float_ident(name) {
                f.emit_ext(Op::CallNative, 0, 0, 0, N_IDENT_F);
                f.emit(Op::MoveF, dest, 0, 0);
            } else {
                f.emit_ext(Op::CallNative, 0, 0, 0, N_IDENT);
                f.emit(Op::I2F, dest, 0, 0);
            }
            dest
        }
        DecoExpr::Neg(inner) => {
            let a = emit_f(f, inner, next);
            let zreg = alloc(next);
            let dest = alloc(next);
            let z = f.intern_f(0.0);
            f.emit_ext(Op::LoadConstF, zreg, 0, 0, z);
            f.emit(Op::SubF, dest, zreg, a);
            dest
        }
        DecoExpr::Bin { op, lhs, rhs } => {
            let a = emit_f(f, lhs, next);
            let b = emit_f(f, rhs, next);
            let dest = alloc(next);
            match op {
                BinOp::Add => f.emit(Op::AddF, dest, a, b),
                BinOp::Sub => f.emit(Op::SubF, dest, a, b),
                BinOp::Mul => f.emit(Op::MulF, dest, a, b),
                BinOp::Div => f.emit(Op::DivF, dest, a, b),
                BinOp::Mod => {
                    f.emit(Op::MoveF, 0, a, 0);
                    f.emit(Op::MoveF, 1, b, 0);
                    f.emit_ext(Op::CallNative, 0, 0, 0, N_FMOD);
                    f.emit(Op::MoveF, dest, 0, 0);
                }
                _ => {
                    let z = f.intern_f(0.0);
                    f.emit_ext(Op::LoadConstF, dest, 0, 0, z);
                }
            }
            dest
        }
        DecoExpr::Call { name, args } => {
            let a0 = emit_f(f, &args[0], next);
            let a1 = if args.len() > 1 {
                Some(emit_f(f, &args[1], next))
            } else {
                None
            };
            let dest = alloc(next);
            let idx = f.intern_s(name);
            f.emit_ext(Op::LoadConstS, 0, 0, 0, idx);
            f.emit(Op::MoveF, 0, a0, 0);
            if let Some(a1) = a1 {
                f.emit(Op::MoveF, 1, a1, 0);
            }
            f.emit_ext(Op::CallNative, 0, 0, 0, N_FCALL);
            f.emit(Op::MoveF, dest, 0, 0);
            dest
        }
        DecoExpr::Index { base, index } => {
            let dest = alloc(next);
            let DecoExpr::Ident(n) = base.as_ref() else {
                let z = f.intern_f(0.0);
                f.emit_ext(Op::LoadConstF, dest, 0, 0, z);
                return dest;
            };
            let ir = emit_f(f, index, next);
            let ni = f.intern_s(n);
            f.emit_ext(Op::LoadConstS, 0, 0, 0, ni);
            f.emit(Op::F2I, 1, ir, 0);
            f.emit_ext(Op::CallNative, 0, 0, 0, N_INDEX);
            f.emit(Op::I2F, dest, 0, 0);
            dest
        }
        _ => {
            let dest = alloc(next);
            let z = f.intern_f(0.0);
            f.emit_ext(Op::LoadConstF, dest, 0, 0, z);
            dest
        }
    }
}

const N_IDENT: u32 = 90_001;
const N_CALL: u32 = 90_002;
const N_INDEX: u32 = 90_003;
const N_IDENT_F: u32 = 90_004;
const N_FCALL: u32 = 90_005;
const N_FRET: u32 = 90_006;
const N_FMOD: u32 = 90_007;

thread_local! {
    static HOST: std::cell::RefCell<Host> = std::cell::RefCell::new(Host::default());
}

#[derive(Default)]
struct Host {
    health: i32,
    mass: i32,
    tid: i32,
    args: [i32; 5],
    user: BTreeMap<String, i32>,
    user_arrays: BTreeMap<String, BTreeMap<i32, i32>>,
    crouch: i32,
    waterlevel: i32,
    pitch: f64,
    speed: f64,
    radius: f64,
    height: f64,
    angle: f64,
    velx: f64,
    vely: f64,
    velz: f64,
    alpha: f64,
    x: f64,
    y: f64,
    z: f64,
    floorz: f64,
    scalex: f64,
    scaley: f64,
    rng: Vec<u8>,
    rng_i: usize,
    last_f: f64,
}

fn fill_host(world: &mut World, self_id: ActorId) -> Host {
    let mut host = Host::default();
    if let Some(a) = world.actors.get(self_id) {
        host.health = a.health;
        host.mass = a.mass;
        host.tid = a.tid;
        host.args = a.args;
        host.user = a.user_vars.clone();
        host.user_arrays = a
            .user_arrays
            .iter()
            .map(|(k, m)| (k.clone(), m.clone()))
            .collect();
        host.pitch = -a.pitch.degrees();
        host.speed = a.speed;
        host.radius = a.radius;
        host.height = a.height;
        host.angle = a.angle.degrees();
        host.velx = a.vel.x();
        host.vely = a.vel.y();
        host.velz = a.vel.z();
        host.alpha = a.alpha;
        host.x = a.pos.x();
        host.y = a.pos.y();
        host.z = a.pos.z();
        host.scalex = a.scale_x;
        host.scaley = a.scale_y;
        host.waterlevel = crate::physics::actor_waterlevel(world, a);
        host.floorz = crate::physics::check_position(world, a, a.pos, Some(self_id)).floor_z;
    }
    host.crouch = i32::from(
        world.thinking_up_move < 0 || world.players.iter().flatten().any(|p| p.cmd.up_move < 0),
    );
    for _ in 0..16 {
        host.rng.push(world.p_random("decorate"));
    }
    host
}

/// Link an integer expression program once (builtins + `deco.*` natives).
fn link_i(prog: CompiledProgram) -> Zvm {
    let mut vm = Zvm::new(prog);
    let id_i = vm.natives.register("deco.ident", native_ident);
    let id_c = vm.natives.register("deco.call", native_call);
    let id_x = vm.natives.register("deco.index", native_index);
    if let Some(f) = std::sync::Arc::make_mut(&mut vm.program)
        .functions
        .first_mut()
    {
        patch(&mut f.code, N_IDENT, id_i);
        patch(&mut f.code, N_CALL, id_c);
        patch(&mut f.code, N_INDEX, id_x);
    }
    vm
}

/// Reset per-call VM state so a cached VM behaves like a fresh one.
fn reset_vm(vm: &mut Zvm) {
    vm.heap = room_zvm::ZvmHeap::new();
    *vm.play = Default::default();
}

fn run_i(world: &mut World, self_id: ActorId, vm: &mut Zvm) -> Option<i64> {
    HOST.with(|c| *c.borrow_mut() = fill_host(world, self_id));
    reset_vm(vm);
    vm.call("__deco").ok()
}

fn run_f(world: &mut World, self_id: ActorId, vm: &mut Zvm) -> Option<f64> {
    HOST.with(|c| *c.borrow_mut() = fill_host(world, self_id));
    reset_vm(vm);
    vm.call("__deco_f").ok()?;
    Some(HOST.with(|c| c.borrow().last_f))
}

/// Link a float expression program once.
fn link_f(prog: CompiledProgram) -> Zvm {
    let mut vm = Zvm::new(prog);
    let id_i = vm.natives.register("deco.ident", native_ident);
    let id_if = vm.natives.register("deco.ident_f", native_ident_f);
    let id_x = vm.natives.register("deco.index", native_index);
    let id_fc = vm.natives.register("deco.fcall", native_fcall);
    let id_fr = vm.natives.register("deco.fret", native_fret);
    let id_fm = vm.natives.register("deco.fmod", native_fmod);
    if let Some(f) = std::sync::Arc::make_mut(&mut vm.program)
        .functions
        .first_mut()
    {
        patch(&mut f.code, N_IDENT, id_i);
        patch(&mut f.code, N_IDENT_F, id_if);
        patch(&mut f.code, N_INDEX, id_x);
        patch(&mut f.code, N_FCALL, id_fc);
        patch(&mut f.code, N_FRET, id_fr);
        patch(&mut f.code, N_FMOD, id_fm);
    }
    vm
}

fn patch(code: &mut [u32], from: u32, to: u32) {
    let mut i = 0;
    while i < code.len() {
        let (op, _, _, _) = room_zvm::unpack(code[i]);
        let has_ext = matches!(
            room_zvm::Op::from_u8(op),
            Some(
                room_zvm::Op::LoadConstI
                    | room_zvm::Op::LoadConstF
                    | room_zvm::Op::LoadConstS
                    | room_zvm::Op::Jmp
                    | room_zvm::Op::JmpIf
                    | room_zvm::Op::JmpIfNot
                    | room_zvm::Op::CallStatic
                    | room_zvm::Op::CallNative
                    | room_zvm::Op::NewObj
                    | room_zvm::Op::LoadFieldI
                    | room_zvm::Op::StoreFieldI
                    | room_zvm::Op::LoadFieldA
                    | room_zvm::Op::StoreFieldA
                    | room_zvm::Op::LoadFieldS
                    | room_zvm::Op::StoreFieldS
                    | room_zvm::Op::LoadFieldF
                    | room_zvm::Op::StoreFieldF
                    | room_zvm::Op::Trap
            )
        );
        if op == room_zvm::Op::CallNative as u8 && i + 1 < code.len() && code[i + 1] == from {
            code[i + 1] = to;
        }
        i += if has_ext { 2 } else { 1 };
    }
}

fn native_ident(
    ctx: &mut room_zvm::ZvmCtx<'_>,
    regs: &mut room_zvm::NativeRegs<'_>,
) -> Result<(), room_zvm::Trap> {
    let name = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
    let lower = name.to_ascii_lowercase();
    let v = HOST.with(|c| {
        let h = c.borrow();
        match lower.as_str() {
            "health" => i64::from(h.health),
            "mass" => i64::from(h.mass),
            "tid" => i64::from(h.tid),
            "args" => i64::from(h.args[0]),
            "playercrouch" | "crouch" => i64::from(h.crouch),
            "waterlevel" => i64::from(h.waterlevel),
            _ if lower.starts_with("user_") => i64::from(
                h.user
                    .get(name)
                    .or_else(|| h.user.get(&lower))
                    .copied()
                    .unwrap_or(0),
            ),
            _ => 0,
        }
    });
    if let Some(s) = regs.i.first_mut() {
        *s = v;
    }
    Ok(())
}

fn native_ident_f(
    ctx: &mut room_zvm::ZvmCtx<'_>,
    regs: &mut room_zvm::NativeRegs<'_>,
) -> Result<(), room_zvm::Trap> {
    let name = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
    let lower = name.to_ascii_lowercase();
    let v = HOST.with(|c| {
        let h = c.borrow();
        match lower.as_str() {
            "pitch" => h.pitch,
            "speed" => h.speed,
            "radius" => h.radius,
            "height" => h.height,
            "angle" => h.angle,
            "velx" | "momx" => h.velx,
            "vely" | "momy" => h.vely,
            "velz" | "momz" => h.velz,
            "alpha" => h.alpha,
            "x" => h.x,
            "y" => h.y,
            "z" => h.z,
            "floorz" => h.floorz,
            "scalex" => h.scalex,
            "scaley" => h.scaley,
            _ => 0.0,
        }
    });
    if let Some(s) = regs.f.first_mut() {
        *s = v;
    }
    Ok(())
}

fn native_call(
    ctx: &mut room_zvm::ZvmCtx<'_>,
    regs: &mut room_zvm::NativeRegs<'_>,
) -> Result<(), room_zvm::Trap> {
    let name = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
    let lo = regs.i.get(1).copied().unwrap_or(0) as i32;
    let hi = regs.i.get(2).copied().unwrap_or(i64::from(lo)) as i32;
    let v = HOST.with(|c| {
        let mut h = c.borrow_mut();
        if !name.eq_ignore_ascii_case("random") {
            return 0;
        }
        if hi <= lo {
            return i64::from(lo);
        }
        let span = (hi - lo + 1) as u32;
        let r = u32::from(h.rng.get(h.rng_i).copied().unwrap_or(0));
        h.rng_i += 1;
        i64::from(lo.wrapping_add((r % span) as i32))
    });
    if let Some(s) = regs.i.first_mut() {
        *s = v;
    }
    Ok(())
}

fn native_fcall(
    ctx: &mut room_zvm::ZvmCtx<'_>,
    regs: &mut room_zvm::NativeRegs<'_>,
) -> Result<(), room_zvm::Trap> {
    let name = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
    let lo = name.to_ascii_lowercase();
    let a0 = regs.f.first().copied().unwrap_or(0.0);
    let a1 = regs.f.get(1).copied().unwrap_or(a0);
    let v = HOST.with(|c| {
        let mut h = c.borrow_mut();
        match lo.as_str() {
            "frandom" => {
                let r = f64::from(h.rng.get(h.rng_i).copied().unwrap_or(0)) / 255.0;
                h.rng_i += 1;
                a0 + (a1 - a0) * r
            }
            "random" => {
                let lo_i = a0 as i32;
                let hi_i = a1 as i32;
                if hi_i <= lo_i {
                    return f64::from(lo_i);
                }
                let span = (hi_i - lo_i + 1) as u32;
                let r = u32::from(h.rng.get(h.rng_i).copied().unwrap_or(0));
                h.rng_i += 1;
                f64::from(lo_i.wrapping_add((r % span) as i32))
            }
            "sin" => room_core::dmath::sin(a0.to_radians()),
            "cos" => room_core::dmath::cos(a0.to_radians()),
            "abs" => a0.abs(),
            "sqrt" => {
                if a0 <= 0.0 {
                    0.0
                } else {
                    room_core::dmath::sqrt(a0)
                }
            }
            _ => 0.0,
        }
    });
    if let Some(s) = regs.f.first_mut() {
        *s = v;
    }
    Ok(())
}

fn native_fret(
    _ctx: &mut room_zvm::ZvmCtx<'_>,
    regs: &mut room_zvm::NativeRegs<'_>,
) -> Result<(), room_zvm::Trap> {
    let v = regs.f.first().copied().unwrap_or(0.0);
    HOST.with(|c| c.borrow_mut().last_f = v);
    Ok(())
}

fn native_fmod(
    _ctx: &mut room_zvm::ZvmCtx<'_>,
    regs: &mut room_zvm::NativeRegs<'_>,
) -> Result<(), room_zvm::Trap> {
    let a = regs.f.first().copied().unwrap_or(0.0);
    let b = regs.f.get(1).copied().unwrap_or(1.0);
    let v = if b == 0.0 { 0.0 } else { a % b };
    if let Some(s) = regs.f.first_mut() {
        *s = v;
    }
    Ok(())
}

fn native_index(
    ctx: &mut room_zvm::ZvmCtx<'_>,
    regs: &mut room_zvm::NativeRegs<'_>,
) -> Result<(), room_zvm::Trap> {
    let name = ctx.heap.str(regs.s.first().copied().unwrap_or_default());
    let idx = regs.i.get(1).copied().unwrap_or(0) as i32;
    let lower = name.to_ascii_lowercase();
    let v = HOST.with(|c| {
        let h = c.borrow();
        if lower == "args" {
            i64::from(h.args.get(idx as usize).copied().unwrap_or(0))
        } else if lower.starts_with("user_") {
            i64::from(
                h.user_arrays
                    .get(name)
                    .or_else(|| h.user_arrays.get(&lower))
                    .and_then(|m| m.get(&idx).copied())
                    .unwrap_or(0),
            )
        } else {
            0
        }
    });
    if let Some(s) = regs.i.first_mut() {
        *s = v;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use room_decorate::parse_deco_expr;

    #[test]
    fn compiles_integer_arith() {
        let expr = parse_deco_expr("1 + 2 * 3").unwrap();
        assert!(zvm_suitable(&expr));
        let f = compile_expr(&expr);
        assert!(!f.code.is_empty());
        let mut p = CompiledProgram {
            functions: vec![f],
            ..Default::default()
        };
        p.finalize();
        let _ = p;
    }

    #[test]
    fn float_path_accepts_frandom_and_literals() {
        assert!(zvm_suitable_f(&parse_deco_expr("1.5 + 2").unwrap()));
        assert!(zvm_suitable_f(&parse_deco_expr("frandom(0,1)").unwrap()));
        assert!(zvm_suitable_f(
            &parse_deco_expr("0.86 - 0.075 * pitch").unwrap()
        ));
        assert!(!zvm_suitable(&parse_deco_expr("1.5 + 2").unwrap()));
        assert!(!zvm_suitable_f(&parse_deco_expr("See").unwrap()));
    }
}

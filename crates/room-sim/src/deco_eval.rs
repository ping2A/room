//! Evaluate DECORATE [`DecoExpr`] arguments at action call time.
//!
//! Integer and float expressions are compiled to `room-zvm` bytecode (ZS5) and
//! cached on [`World::deco_expr_cache`]; string labels / exotic calls keep the
//! tree-walker.

use crate::actor::ActorId;
use crate::deco_zvm;
use crate::world::World;
use room_decorate::{BinOp, DecoExpr, is_trivial_arg, parse_deco_expr};

/// Evaluate a raw action-argument string for `self_id`.
///
/// Trivial literals/idents are returned unchanged; expressions are folded to a
/// decimal / float / string token suitable for existing action parsers.
#[must_use]
pub fn eval_action_arg(world: &mut World, self_id: ActorId, raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() || is_trivial_arg(raw) {
        return raw.to_owned();
    }
    match parse_deco_expr(raw) {
        Ok(expr) => {
            if deco_zvm::zvm_suitable(&expr) {
                let mut cache = std::mem::take(&mut world.deco_expr_cache);
                let zvm = cache.eval_i64(world, self_id, raw, &expr);
                world.deco_expr_cache = cache;
                if let Some(n) = zvm {
                    return n.to_string();
                }
            } else if deco_zvm::zvm_suitable_f(&expr) {
                let mut cache = std::mem::take(&mut world.deco_expr_cache);
                let zvm = cache.eval_f64(world, self_id, raw, &expr);
                world.deco_expr_cache = cache;
                if let Some(f) = zvm {
                    return if f.fract() == 0.0 && f.abs() < 1e15 {
                        (f as i64).to_string()
                    } else {
                        f.to_string()
                    };
                }
            }
            match eval_expr(world, self_id, &expr) {
                EvalValue::Int(n) => n.to_string(),
                EvalValue::Float(f) => {
                    if f.fract() == 0.0 && f.abs() < 1e15 {
                        (f as i64).to_string()
                    } else {
                        f.to_string()
                    }
                }
                EvalValue::Str(s) => s,
            }
        }
        Err(_) => raw.to_owned(),
    }
}

/// Evaluate all action args in order.
#[must_use]
pub fn eval_action_args(world: &mut World, self_id: ActorId, args: &[String]) -> Vec<String> {
    args.iter()
        .map(|a| eval_action_arg(world, self_id, a))
        .collect()
}

#[derive(Clone, Debug)]
enum EvalValue {
    Int(i32),
    Float(f64),
    Str(String),
}

impl EvalValue {
    fn as_f64(&self) -> f64 {
        match self {
            EvalValue::Int(n) => f64::from(*n),
            EvalValue::Float(f) => *f,
            EvalValue::Str(s) => s.parse().unwrap_or(0.0),
        }
    }

    fn as_i32(&self) -> i32 {
        match self {
            EvalValue::Int(n) => *n,
            EvalValue::Float(f) => *f as i32,
            EvalValue::Str(s) => s.parse().unwrap_or(0),
        }
    }
}

fn eval_expr(world: &mut World, self_id: ActorId, expr: &DecoExpr) -> EvalValue {
    match expr {
        DecoExpr::Int(n) => EvalValue::Int(*n),
        DecoExpr::Float(f) => EvalValue::Float(*f),
        DecoExpr::String(s) => EvalValue::Str(s.clone()),
        DecoExpr::Ident(name) => eval_ident(world, self_id, name),
        DecoExpr::Neg(inner) => {
            let v = eval_expr(world, self_id, inner);
            match v {
                EvalValue::Int(n) => EvalValue::Int(n.wrapping_neg()),
                other => EvalValue::Float(-other.as_f64()),
            }
        }
        DecoExpr::Not(inner) => {
            let v = eval_expr(world, self_id, inner);
            EvalValue::Int(i32::from(v.as_i32() == 0))
        }
        DecoExpr::Bin { op, lhs, rhs } => {
            let l = eval_expr(world, self_id, lhs);
            let r = eval_expr(world, self_id, rhs);
            bin_op(*op, l, r)
        }
        DecoExpr::Call { name, args } => eval_call(world, self_id, name, args),
        DecoExpr::Index { base, index } => {
            let idx = eval_expr(world, self_id, index).as_i32();
            match base.as_ref() {
                DecoExpr::Ident(n) if n.eq_ignore_ascii_case("args") => {
                    let v = world
                        .actors
                        .get(self_id)
                        .and_then(|a| a.args.get(idx as usize).copied())
                        .unwrap_or(0);
                    EvalValue::Int(v)
                }
                DecoExpr::Ident(n) if n.to_ascii_lowercase().starts_with("user_") => {
                    let v = world
                        .actors
                        .get(self_id)
                        .and_then(|a| a.user_arrays.get(n)?.get(&idx).copied())
                        .unwrap_or(0);
                    EvalValue::Int(v)
                }
                _ => EvalValue::Int(0),
            }
        }
    }
}

fn eval_ident(world: &World, self_id: ActorId, name: &str) -> EvalValue {
    let lower = name.to_ascii_lowercase();
    let Some(a) = world.actors.get(self_id) else {
        return EvalValue::Str(name.to_owned());
    };
    match lower.as_str() {
        "health" => EvalValue::Int(a.health),
        "mass" => EvalValue::Int(a.mass),
        "speed" => EvalValue::Float(a.speed),
        "radius" => EvalValue::Float(a.radius),
        "height" => EvalValue::Float(a.height),
        "angle" => EvalValue::Float(a.angle.degrees()),
        // GZDoom sign (negative looks up); Room's actor pitch is positive up.
        "pitch" => EvalValue::Float(-a.pitch.degrees()),
        "velx" | "momx" => EvalValue::Float(a.vel.x()),
        "vely" | "momy" => EvalValue::Float(a.vel.y()),
        "velz" | "momz" => EvalValue::Float(a.vel.z()),
        "tid" => EvalValue::Int(a.tid),
        // Holding move-down is Duke's crouch (pipebomb roll).
        "playercrouch" | "crouch" => {
            let crouched = world.thinking_up_move < 0
                || world.players.iter().flatten().any(|p| p.cmd.up_move < 0);
            EvalValue::Int(i32::from(crouched))
        }
        "alpha" => EvalValue::Float(a.alpha),
        "args" => EvalValue::Int(a.args[0]),
        // BD3 expr idents (docs/33 §3.7).
        "waterlevel" => EvalValue::Int(crate::physics::actor_waterlevel(world, a)),
        "x" => EvalValue::Float(a.pos.x()),
        "y" => EvalValue::Float(a.pos.y()),
        "z" => EvalValue::Float(a.pos.z()),
        "floorz" => {
            let z = crate::physics::check_position(world, a, a.pos, Some(self_id)).floor_z;
            EvalValue::Float(z)
        }
        "scalex" => EvalValue::Float(a.scale_x),
        "scaley" => EvalValue::Float(a.scale_y),
        "true" => EvalValue::Int(1),
        "false" => EvalValue::Int(0),
        _ if lower.starts_with("user_") => a
            .user_vars
            .get(name)
            .or_else(|| a.user_vars.get(&lower))
            .copied()
            .map(EvalValue::Int)
            .unwrap_or(EvalValue::Int(0)),
        _ if lower.starts_with("aaptr_") => EvalValue::Str(name.to_owned()),
        // UZDoom flag constants inside an expression
        // (`SXF_NOCHECKPOSITION|SXF_SETMASTER`, `CMF_AIMDIRECTION|CMF_TRACKOWNER`):
        // as a string they read as 0, so every OR-ed flag set used to vanish.
        _ if name.contains('_') => match crate::uz_constants::constant(name) {
            Some(v) => EvalValue::Int(v as i32),
            None => EvalValue::Str(name.to_owned()),
        },
        _ => EvalValue::Str(name.to_owned()),
    }
}

fn eval_call(world: &mut World, self_id: ActorId, name: &str, args: &[DecoExpr]) -> EvalValue {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "random" => {
            let lo = args
                .first()
                .map(|e| eval_expr(world, self_id, e).as_i32())
                .unwrap_or(0);
            let hi = args
                .get(1)
                .map(|e| eval_expr(world, self_id, e).as_i32())
                .unwrap_or(lo);
            if hi <= lo {
                return EvalValue::Int(lo);
            }
            let span = (hi - lo + 1) as u32;
            let r = u32::from(world.p_random("decorate"));
            EvalValue::Int(lo.wrapping_add((r % span) as i32))
        }
        "frandom" => {
            let lo = args
                .first()
                .map(|e| eval_expr(world, self_id, e).as_f64())
                .unwrap_or(0.0);
            let hi = args
                .get(1)
                .map(|e| eval_expr(world, self_id, e).as_f64())
                .unwrap_or(lo);
            let r = f64::from(world.p_random("decorate")) / 255.0;
            EvalValue::Float(lo + (hi - lo) * r)
        }
        "countinv" => {
            let name = match args.first() {
                Some(DecoExpr::String(s) | DecoExpr::Ident(s)) => s.clone(),
                _ => return EvalValue::Int(0),
            };
            let Some(cid) = world.classes.find_name(&name) else {
                return EvalValue::Int(0);
            };
            EvalValue::Int(crate::inventory::count_inventory(world, self_id, cid))
        }
        "checkclass" => {
            let class_name = match args.first() {
                Some(DecoExpr::String(s) | DecoExpr::Ident(s)) => s.clone(),
                _ => return EvalValue::Int(0),
            };
            let ptr = match args.get(1) {
                Some(DecoExpr::String(s) | DecoExpr::Ident(s)) => s.as_str(),
                _ => "AAPTR_DEFAULT",
            };
            let match_super = args
                .get(2)
                .map(|e| eval_expr(world, self_id, e).as_i32() != 0)
                .unwrap_or(false);
            let Some(tid) = resolve_aaptr(world, self_id, ptr) else {
                return EvalValue::Int(0);
            };
            let Some(cid) = world.actors.get(tid).map(|a| a.class) else {
                return EvalValue::Int(0);
            };
            EvalValue::Int(i32::from(class_is_a(world, cid, &class_name, match_super)))
        }
        "callacs" | "acs_namedexecutewithresult" => {
            let script = match args.first() {
                Some(DecoExpr::String(s) | DecoExpr::Ident(s)) => s.clone(),
                _ => return EvalValue::Int(0),
            };
            let a1 = args
                .get(1)
                .map(|e| eval_expr(world, self_id, e).as_i32())
                .unwrap_or(0);
            let a2 = args
                .get(2)
                .map(|e| eval_expr(world, self_id, e).as_i32())
                .unwrap_or(0);
            let a3 = args
                .get(3)
                .map(|e| eval_expr(world, self_id, e).as_i32())
                .unwrap_or(0);
            let r = crate::acs::acs_named_execute_by_name(
                world,
                &script,
                0,
                [a1, a2, a3],
                true,
                Some(self_id),
            );
            EvalValue::Int(r)
        }
        "sin" => {
            let deg = args
                .first()
                .map(|e| eval_expr(world, self_id, e).as_f64())
                .unwrap_or(0.0);
            EvalValue::Float(room_core::dmath::sin(deg.to_radians()))
        }
        "cos" => {
            let deg = args
                .first()
                .map(|e| eval_expr(world, self_id, e).as_f64())
                .unwrap_or(0.0);
            EvalValue::Float(room_core::dmath::cos(deg.to_radians()))
        }
        "abs" => {
            let v = args
                .first()
                .map(|e| eval_expr(world, self_id, e))
                .unwrap_or(EvalValue::Int(0));
            match v {
                EvalValue::Int(n) => EvalValue::Int(n.wrapping_abs()),
                other => EvalValue::Float(other.as_f64().abs()),
            }
        }
        "sqrt" => {
            let v = args
                .first()
                .map(|e| eval_expr(world, self_id, e).as_f64())
                .unwrap_or(0.0);
            EvalValue::Float(if v <= 0.0 {
                0.0
            } else {
                room_core::dmath::sqrt(v)
            })
        }
        "random2" => {
            // GZDoom: random2() → −255…255; random2(mask) → (−mask)…mask.
            let mask = args
                .first()
                .map(|e| eval_expr(world, self_id, e).as_i32())
                .unwrap_or(255)
                .unsigned_abs();
            let r = i32::from(world.p_random("decorate"));
            let span = (mask as i32).saturating_mul(2).saturating_add(1).max(1);
            EvalValue::Int(r % span - mask as i32)
        }
        "findinventory" => {
            let name = match args.first() {
                Some(DecoExpr::String(s) | DecoExpr::Ident(s)) => s.clone(),
                _ => return EvalValue::Int(0),
            };
            let Some(cid) = world.classes.find_name(&name) else {
                return EvalValue::Int(0);
            };
            EvalValue::Int(i32::from(
                crate::inventory::count_inventory(world, self_id, cid) > 0,
            ))
        }
        // A weapon's own ZScript function (`PressingFire()`).
        _ => {
            let texts: Vec<String> = args
                .iter()
                .map(|e| match eval_expr(world, self_id, e) {
                    EvalValue::Int(n) => n.to_string(),
                    EvalValue::Float(f) => f.to_string(),
                    EvalValue::Str(s) => format!("\"{s}\""),
                })
                .collect();
            crate::action::zscript_weapon_function_value(world, self_id, name, &texts)
                .map_or(EvalValue::Int(0), |v| {
                    EvalValue::Int(i32::try_from(v).unwrap_or(i32::from(v != 0)))
                })
        }
    }
}

fn resolve_aaptr(world: &World, self_id: ActorId, ptr: &str) -> Option<ActorId> {
    let key = ptr.trim_matches('"').to_ascii_lowercase();
    match key.as_str() {
        "aaptr_null" | "null" | "none" => None,
        "" | "aaptr_default" | "default" | "self" => Some(self_id),
        "aaptr_target" | "target" => world.actors.get(self_id).and_then(|a| a.target),
        "aaptr_master" | "master" => world.actors.get(self_id).and_then(|a| a.master),
        "aaptr_tracer" | "tracer" => world.actors.get(self_id).and_then(|a| a.tracer),
        "aaptr_player1" | "player1" => world.players[0].as_ref().and_then(|p| p.mo),
        _ => Some(self_id),
    }
}

pub(crate) fn class_is_a(
    world: &World,
    mut id: room_core::ClassId,
    want: &str,
    match_super: bool,
) -> bool {
    loop {
        let Some(def) = world.classes.get(id) else {
            return false;
        };
        if def.name.eq_ignore_ascii_case(want) {
            return true;
        }
        if !match_super {
            return false;
        }
        let Some(parent) = def.parent else {
            return false;
        };
        id = parent;
    }
}

fn bin_op(op: BinOp, lhs: EvalValue, rhs: EvalValue) -> EvalValue {
    match op {
        BinOp::Eq => EvalValue::Int(i32::from(lhs.as_f64() == rhs.as_f64())),
        BinOp::Ne => EvalValue::Int(i32::from(lhs.as_f64() != rhs.as_f64())),
        BinOp::Lt => EvalValue::Int(i32::from(lhs.as_f64() < rhs.as_f64())),
        BinOp::Le => EvalValue::Int(i32::from(lhs.as_f64() <= rhs.as_f64())),
        BinOp::Gt => EvalValue::Int(i32::from(lhs.as_f64() > rhs.as_f64())),
        BinOp::Ge => EvalValue::Int(i32::from(lhs.as_f64() >= rhs.as_f64())),
        BinOp::And => EvalValue::Int(i32::from(lhs.as_i32() != 0 && rhs.as_i32() != 0)),
        BinOp::Or => EvalValue::Int(i32::from(lhs.as_i32() != 0 || rhs.as_i32() != 0)),
        BinOp::BitOr => EvalValue::Int(lhs.as_i32() | rhs.as_i32()),
        BinOp::BitAnd => EvalValue::Int(lhs.as_i32() & rhs.as_i32()),
        _ => match (&lhs, &rhs) {
            (EvalValue::Int(a), EvalValue::Int(b)) => {
                let v = match op {
                    BinOp::Add => a.wrapping_add(*b),
                    BinOp::Sub => a.wrapping_sub(*b),
                    BinOp::Mul => a.wrapping_mul(*b),
                    BinOp::Div => {
                        if *b == 0 {
                            0
                        } else {
                            a / b
                        }
                    }
                    BinOp::Mod => {
                        if *b == 0 {
                            0
                        } else {
                            a % b
                        }
                    }
                    _ => 0,
                };
                EvalValue::Int(v)
            }
            _ => {
                let a = lhs.as_f64();
                let b = rhs.as_f64();
                let v = match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => {
                        if b == 0.0 {
                            0.0
                        } else {
                            a / b
                        }
                    }
                    BinOp::Mod => {
                        if b == 0.0 {
                            0.0
                        } else {
                            a % b
                        }
                    }
                    _ => 0.0,
                };
                EvalValue::Float(v)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::class::ClassRegistry;
    use crate::state::StateTable;
    use room_core::{Angle, DVec3};
    use room_decorate::parse_decorate;
    use room_nodebuild::apply_nodes;
    use room_testkit::{MapBuilder, rect};
    use std::sync::Arc;

    #[test]
    fn evals_health_args_user_and_arith() {
        let ir = parse_decorate(
            r#"
actor Probe : Actor 9001
{
  Health 50
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, classes, states).unwrap();
        let probe = w.classes.find_name("Probe").unwrap();
        let id = w
            .spawn_at(probe, DVec3::new(64.0, 64.0, 0.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.args[0] = 7;
            a.user_vars.insert("user_count".into(), 3);
        }
        assert_eq!(eval_action_arg(&mut w, id, "health + 1"), "51");
        assert_eq!(eval_action_arg(&mut w, id, "args[0] * 2"), "14");
        assert_eq!(eval_action_arg(&mut w, id, "user_count + 2"), "5");
        assert_eq!(eval_action_arg(&mut w, id, "See"), "See");
        assert_eq!(eval_action_arg(&mut w, id, "10"), "10");
        for _ in 0..16 {
            let v: f64 = eval_action_arg(&mut w, id, "frandom(-2, 3) * 2")
                .parse()
                .unwrap();
            assert!((-4.0..=6.0).contains(&v), "{v}");
        }
        assert_eq!(eval_action_arg(&mut w, id, "CountInv(\"Probe\") + 1"), "1");
        if let Some(p) = w.players[0].as_mut() {
            p.mo = Some(id);
            p.on_ground = true;
            p.cmd.up_move = -64;
        }
        assert_eq!(eval_action_arg(&mut w, id, "0 + playercrouch"), "1");
        if let Some(p) = w.players[0].as_mut() {
            p.cmd.up_move = 0;
        }
        assert_eq!(eval_action_arg(&mut w, id, "0 + playercrouch"), "0");
        if let Some(p) = w.players[0].as_mut() {
            p.cmd.up_move = -64;
        }
        let z = eval_action_arg(
            &mut w,
            id,
            "(0.86 - 0.075 * pitch) * (1 - playercrouch) + (-0.075 * pitch) * playercrouch",
        );
        assert_eq!(z, "0");
    }

    #[test]
    fn evals_bd3_idents_and_calls() {
        let ir = parse_decorate(
            r#"
actor Probe : Actor 9002
{
  Health 50
  States { Spawn: POSS A -1 Stop }
}
actor Imp : Actor 9003
{
  Health 60
  States { Spawn: POSS A -1 Stop }
}
"#,
        )
        .unwrap();
        let classes = Arc::new(ClassRegistry::from_decorate(&ir).unwrap());
        let states = Arc::new(StateTable::from_decorate(&ir, &classes));
        let mut map = MapBuilder::new()
            .room("a", rect(0.0, 0.0, 128.0, 128.0), 0.0, 128.0)
            .player_start(32.0, 32.0, 0)
            .build_udmf("MAP01")
            .unwrap();
        apply_nodes(&mut map);
        let mut w = World::from_map_with_classes(map, Arc::clone(&classes), states).unwrap();
        let probe = classes.find_name("Probe").unwrap();
        let imp = classes.find_name("Imp").unwrap();
        let id = w
            .spawn_at(probe, DVec3::new(40.0, 50.0, 8.0), Angle::ZERO)
            .unwrap();
        if let Some(a) = w.actors.get_mut(id) {
            a.scale_x = 1.5;
            a.scale_y = 2.0;
            a.class = imp; // CheckClass against Imp
        }
        // Bare idents are trivial and skipped by `eval_action_arg`; force expr form.
        assert_eq!(eval_action_arg(&mut w, id, "0 + x"), "40");
        assert_eq!(eval_action_arg(&mut w, id, "0 + y"), "50");
        assert_eq!(eval_action_arg(&mut w, id, "0 + z"), "8");
        assert_eq!(eval_action_arg(&mut w, id, "0 + scalex"), "1.5");
        assert_eq!(eval_action_arg(&mut w, id, "0 + waterlevel"), "0");
        assert_eq!(eval_action_arg(&mut w, id, "0 + true"), "1");
        assert_eq!(eval_action_arg(&mut w, id, "CheckClass(\"Imp\")"), "1");
        assert_eq!(
            eval_action_arg(&mut w, id, "CheckClass(\"Probe\", AAPTR_DEFAULT, 1)"),
            "0"
        );
        let s = eval_action_arg(&mut w, id, "sin(90)");
        let s: f64 = s.parse().unwrap();
        assert!((s - 1.0).abs() < 1e-6, "{s}");
        assert_eq!(eval_action_arg(&mut w, id, "abs(-7)"), "7");
    }
}

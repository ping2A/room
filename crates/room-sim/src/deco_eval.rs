//! Evaluate DECORATE [`DecoExpr`] arguments at action call time.

use crate::actor::ActorId;
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
        Ok(expr) => match eval_expr(world, self_id, &expr) {
            EvalValue::Int(n) => n.to_string(),
            EvalValue::Float(f) => {
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    (f as i64).to_string()
                } else {
                    f.to_string()
                }
            }
            EvalValue::Str(s) => s,
        },
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
        "pitch" => EvalValue::Float(a.pitch.degrees()),
        "velx" | "momx" => EvalValue::Float(a.vel.x()),
        "vely" | "momy" => EvalValue::Float(a.vel.y()),
        "velz" | "momz" => EvalValue::Float(a.vel.z()),
        "tid" => EvalValue::Int(a.tid),
        "alpha" => EvalValue::Float(a.alpha),
        "args" => EvalValue::Int(a.args[0]),
        _ if lower.starts_with("user_") => a
            .user_vars
            .get(name)
            .or_else(|| a.user_vars.get(&lower))
            .copied()
            .map(EvalValue::Int)
            .unwrap_or(EvalValue::Int(0)),
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
        _ => EvalValue::Int(0),
    }
}

fn bin_op(op: BinOp, lhs: EvalValue, rhs: EvalValue) -> EvalValue {
    match (&lhs, &rhs) {
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
            };
            EvalValue::Float(v)
        }
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
    }
}

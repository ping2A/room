//! Apply the play-scope ZScript natives the VM queued (UZ3 / UZ7, docs/36).
//!
//! `room-zvm` records natives it cannot answer without the world as
//! [`room_zvm::ZsNativeOp`]s. The receiver is resolved to an actor through
//! the mirrored script objects; `PlayerInfo` natives fall back to the actor
//! whose script ran (the usual `player.X()` caller is the player's pawn).

use crate::actor::{ActorId, BonePart};
use crate::world::{HudMessage, MessageTo, World};

/// One `PPShader` call, for the render shell (`room-app`): render-only, not
/// in the checksum.
#[derive(Clone, Debug, PartialEq)]
pub enum PostShaderOp {
    /// `PPShader.SetEnabled(name, on)`.
    Enable {
        /// GLDEFS post shader `Name`.
        shader: String,
        /// Enabled.
        on: bool,
    },
    /// `PPShader.SetUniform1f/2f/3f/1i(name, uniform, value…)`.
    Uniform {
        /// GLDEFS post shader `Name`.
        shader: String,
        /// Uniform name.
        uniform: String,
        /// Components (ints as floats).
        values: Vec<f32>,
    },
}

fn actor_id_of(world: &World, index: u32) -> Option<ActorId> {
    world
        .thinkers
        .default
        .iter()
        .copied()
        .find(|id| id.index() == index && world.actors.get(*id).is_some())
}

fn args_text(op: &room_zvm::ZsNativeOp) -> Vec<String> {
    let mut out: Vec<String> = op.s.clone();
    out.extend(op.f.iter().map(|v| v.to_string()));
    out.extend(op.i.iter().map(|v| v.to_string()));
    out
}

fn f(op: &room_zvm::ZsNativeOp, k: usize, d: f64) -> f64 {
    op.f.get(k).copied().unwrap_or(d)
}

fn i(op: &room_zvm::ZsNativeOp, k: usize, d: i64) -> i64 {
    op.i.get(k).copied().unwrap_or(d)
}

fn bone(world: &mut World, id: ActorId, op: &room_zvm::ZsNativeOp, part: BonePart, named: bool) {
    let dim = if part == BonePart::Rotation { 4 } else { 3 };
    let mut value = [0.0; 4];
    for (k, v) in value.iter_mut().enumerate().take(dim) {
        *v = f(op, k, 0.0);
    }
    // Ints: [bone index,] mode (default SB_ADD = 1); floats past the value:
    // interpolation tics (default 1).
    let (index, name, mode_k) = if named {
        (None, op.s.first().cloned().unwrap_or_default(), 0)
    } else {
        (Some(i(op, 0, -1) as i32), String::new(), 1)
    };
    let mode = op.i.get(mode_k).copied().unwrap_or(1) as i32;
    let duration = f(op, dim, 1.0);
    let now = f64::from(world.level.maptime);
    if let Some(a) = world.actors.get_mut(id) {
        a.ext.model.set_bone(index, &name, part, value, now, duration, mode);
    }
}

/// `LevelLocals.ReplaceTextures(from, to, flags)`: `NOT_BOTTOM` 1,
/// `NOT_MIDDLE` 2, `NOT_TOP` 4, `NOT_FLOOR` 8, `NOT_CEILING` 16.
pub fn replace_textures(world: &mut World, from: &str, to: &str, flags: i64) {
    if from.is_empty() {
        return;
    }
    let map = std::sync::Arc::make_mut(&mut world.map);
    for side in &mut map.sides {
        if flags & 4 == 0 && side.upper.eq_ignore_ascii_case(from) {
            side.upper = to.to_owned();
        }
        if flags & 2 == 0 && side.middle.eq_ignore_ascii_case(from) {
            side.middle = to.to_owned();
        }
        if flags & 1 == 0 && side.lower.eq_ignore_ascii_case(from) {
            side.lower = to.to_owned();
        }
    }
    for (k, sec) in map.sectors.iter_mut().enumerate() {
        let k = k as u32;
        if flags & 8 == 0 {
            let cur = world.sector_floor_pics.get(&k).cloned().unwrap_or_else(|| sec.floor_pic.clone());
            if cur.eq_ignore_ascii_case(from) {
                sec.floor_pic = to.to_owned();
                world.sector_floor_pics.insert(k, to.to_owned());
            }
        }
        if flags & 16 == 0 {
            let cur = world
                .sector_ceiling_pics
                .get(&k)
                .cloned()
                .unwrap_or_else(|| sec.ceiling_pic.clone());
            if cur.eq_ignore_ascii_case(from) {
                sec.ceiling_pic = to.to_owned();
                world.sector_ceiling_pics.insert(k, to.to_owned());
            }
        }
    }
}

impl World {
    /// Apply queued play-scope natives. `caller` is the actor whose script
    /// ran (the receiver fallback for `PlayerInfo` natives).
    pub fn apply_zscript_native_ops(&mut self, caller: Option<ActorId>) {
        let ops = self.zscript.take_native_ops();
        for (recv, op) in ops {
            let target = recv.and_then(|ix| actor_id_of(self, ix)).or(caller);
            self.apply_native_op(target, caller, &op);
        }
    }

    fn apply_native_op(
        &mut self,
        target: Option<ActorId>,
        caller: Option<ActorId>,
        op: &room_zvm::ZsNativeOp,
    ) {
        let name = op.name.as_str();
        match name {
            "ppshader.setenabled" => {
                self.post_shader_ops.push(PostShaderOp::Enable {
                    shader: op.s.first().cloned().unwrap_or_default(),
                    on: i(op, 0, 0) != 0,
                });
                return;
            }
            "ppshader.setuniform1f" | "ppshader.setuniform2f" | "ppshader.setuniform3f"
            | "ppshader.setuniform1i" => {
                let values = if name.ends_with("1i") {
                    vec![i(op, 0, 0) as f32]
                } else {
                    op.f.iter().map(|v| *v as f32).collect()
                };
                self.post_shader_ops.push(PostShaderOp::Uniform {
                    shader: op.s.first().cloned().unwrap_or_default(),
                    uniform: op.s.get(1).cloned().unwrap_or_default(),
                    values,
                });
                return;
            }
            "levellocals.replacetextures" => {
                let from = op.s.first().cloned().unwrap_or_default();
                let to = op.s.get(1).cloned().unwrap_or_default();
                replace_textures(self, &from, &to, i(op, 0, 0));
                return;
            }
            _ => {}
        }
        let Some(id) = target else {
            return;
        };
        match name {
            "actor.activate" | "actor.deactivate" => {
                if let Some(a) = self.actors.get_mut(id) {
                    if name == "actor.activate" {
                        a.flags &= !crate::class::FLAG_DORMANT;
                    } else {
                        a.flags |= crate::class::FLAG_DORMANT;
                    }
                }
            }
            "actor.destroyallinventory" => self.inventory.clear(id),
            "actor.startsoundsequence" => {
                if let Some(a) = self.actors.get(id) {
                    let (x, y, z) = (a.pos.x(), a.pos.y(), a.pos.z());
                    let seq = op.s.first().cloned().unwrap_or_default();
                    let _ = crate::sndseq::start_sequence(self, &seq, x, y, z);
                }
            }
            "actor.displaynametag" => {
                let tag = self
                    .actors
                    .get(id)
                    .and_then(|a| self.classes.get(a.class))
                    .map(|c| if c.tag.is_empty() { c.name.clone() } else { c.tag.clone() })
                    .unwrap_or_default();
                if !tag.is_empty() {
                    self.messages.push(HudMessage {
                        to: MessageTo::Unscoped,
                        text: tag,
                    });
                }
            }
            "actor.usepuzzleitem" => {
                let _ = crate::action::call_action_args(self, id, "A_UsePuzzleItem", &args_text(op));
            }
            "actor.setanimation" => {
                let _ = crate::action::call_action_args(self, id, "A_SetAnimation", &args_text(op));
            }
            "actor.setanimationframerate" => {
                let _ = crate::action::call_action_args(
                    self,
                    id,
                    "A_SetAnimationFrameRate",
                    &args_text(op),
                );
            }
            "actor.setmodelflag" => {
                let _ = crate::action::call_action_args(self, id, "A_SetModelFlag", &args_text(op));
            }
            "actor.clearmodelflag" => {
                let _ = crate::action::call_action_args(self, id, "A_ClearModelFlag", &args_text(op));
            }
            "actor.resetmodelflags" => {
                let _ = crate::action::call_action_args(self, id, "A_ResetModelFlags", &[]);
            }
            "actor.setbonerotation" => bone(self, id, op, BonePart::Rotation, false),
            "actor.setnamedbonerotation" => bone(self, id, op, BonePart::Rotation, true),
            "actor.setbonetranslation" => bone(self, id, op, BonePart::Translation, false),
            "actor.setnamedbonetranslation" => bone(self, id, op, BonePart::Translation, true),
            "actor.setbonescaling" => bone(self, id, op, BonePart::Scaling, false),
            "actor.setnamedbonescaling" => bone(self, id, op, BonePart::Scaling, true),
            "actor.clearboneoffsets" => {
                if let Some(a) = self.actors.get_mut(id) {
                    a.ext.model.bones.clear();
                }
            }
            "playerinfo.poisonplayer" => {
                let pawn = caller.unwrap_or(id);
                let poison = i(op, 0, 0) as i32;
                if let Some(p) = self.players.iter_mut().flatten().find(|p| p.mo == Some(pawn)) {
                    p.poison_count = (p.poison_count + poison).min(100);
                    p.poisoner = None;
                }
            }
            "playerinfo.setsubtitlenumber" | "playerinfo.setlognumber" => {
                self.subtitle_request = Some((i(op, 0, 0) as i32, op.s.first().cloned().unwrap_or_default()));
            }
            // Not applied yet (docs/36 UZ3): `SetFullbrightMode`,
            // `SetSafeFlash`, `SpotState.Add/RemoveSpot`, `DynamicLight`
            // toggles, `Sector.SetColor/SetFade/RemoveForceField`.
            _ => {}
        }
    }
}
